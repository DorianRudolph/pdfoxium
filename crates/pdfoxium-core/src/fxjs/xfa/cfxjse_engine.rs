// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Node-context portions of cfxjse_engine/context/class and their Boa adapter.
//! Named global lookup, persistent variables realms and scoped event/service
//! contexts share Boa and CfxJsEngine's lease. FF services remain host supplied.
use super::{
    cfxjse_formcalc_context::{register_formcalc_for_document, translate},
    cfxjse_resolveprocessor::{
        CfxJseResolveProcessor, PseudoModel, ResolveError, ResolveResult, ResolveResultType,
        ResolvedObject, SomScriptHost, SomScriptLanguage, XfaResolveFlags,
    },
};
use crate::{
    fxcrt::fx_string::utf16_encode,
    fxjs::{cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16},
    xfa::fxfa::{
        cxfa_eventparam::CxfaEventParam,
        fxfa_basic::{XfaAttributeValue, XfaElement},
        parser::{
            cxfa_document::{CxfaDocument, CxfaNodeStore},
            cxfa_model_properties::CxfaScriptType,
            cxfa_node::CxfaNode,
            xfa_basic_data::{
                find_script_attribute_descriptor, get_element_by_name, ScriptAttributeDescriptor,
            },
        },
    },
};
use boa_engine::{
    js_string,
    object::{builtins::JsProxy, JsData, ObjectInitializer},
    Context, JsError, JsNativeError, JsObject, JsResult, JsString, JsValue, Source,
};
use boa_gc::{Finalize, Trace};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::{Rc, Weak},
};

#[derive(Clone, Trace, Finalize)]
struct RuntimeLink(#[unsafe_ignore_trace] Weak<CfxJseEngine>);
impl JsData for RuntimeLink {}
#[derive(Clone, Trace, Finalize)]
pub(crate) struct NodeBinding {
    #[unsafe_ignore_trace]
    runtime: Weak<CfxJseEngine>,
    #[unsafe_ignore_trace]
    pub(crate) node: CxfaNode,
    #[unsafe_ignore_trace]
    _store: Rc<CxfaNodeStore>,
    // Only UTF-16 names are stored here, never hidden Boa GC references.
    // Generated host accessors represent native named properties, whereas
    // explicit JavaScript own properties must keep ordinary precedence.
    #[unsafe_ignore_trace]
    generated_accessors: Option<Rc<RefCell<HashSet<Vec<u16>>>>>,
}
impl JsData for NodeBinding {}
impl NodeBinding {
    fn mark_generated_accessor(&self, name: &JsString) {
        if let Some(names) = &self.generated_accessors {
            names.borrow_mut().insert(name.to_vec());
        }
    }
    pub(crate) fn runtime(&self) -> JsResult<Rc<CfxJseEngine>> {
        self.runtime.upgrade().ok_or_else(|| {
            JsNativeError::reference()
                .with_message("XFA script context has expired")
                .into()
        })
    }
}
pub(crate) fn node_data(this: &JsValue, context: &Context) -> JsResult<NodeBinding> {
    if let Some(data) = this
        .as_object()
        .and_then(|o| o.downcast_ref::<NodeBinding>().map(|d| d.clone()))
    {
        data.runtime()?.validate_node(&data.node)?;
        return Ok(data);
    }
    let runtime = CfxJseEngine::from_context(context)
        .ok_or_else(|| JsNativeError::reference().with_message("XFA script context has expired"))?;
    let node = runtime
        .node_from_value(this)
        .ok_or_else(|| JsNativeError::typ().with_message("Invalid XFA node receiver"))?;
    runtime.binding_data(&node)
}
struct ScriptFrame {
    node: Option<CxfaNode>,
    object: Option<ResolvedObject>,
    kind: CxfaScriptType,
}
pub struct CfxJseEngine {
    host: RefCell<Option<Rc<dyn super::cjx_hostpseudomodel::XfaHost>>>,
    network_host: RefCell<Option<Rc<dyn super::formcalc_network::FormcalcNetworkHost>>>,
    layout_host: RefCell<Option<Rc<dyn super::cjx_layoutpseudomodel::XfaLayoutHost>>>,
    document: Rc<CxfaDocument>,
    javascript: Rc<CfxJsEngine>,
    weak_self: Weak<Self>,
    resolver: CfxJseResolveProcessor,
    bindings: RefCell<HashMap<CxfaNode, JsObject>>,
    reverse_bindings: RefCell<HashMap<JsObject, CxfaNode>>,
    pseudo_bindings: RefCell<HashMap<PseudoModel, objects::PseudoBinding>>,
    main_global: JsObject,
    main_realm: boa_engine::realm::Realm,
    reflection: reflection::Operations,
    variable_contexts: RefCell<HashMap<CxfaNode, contexts::VariablesContext>>,
    event_frames: RefCell<Vec<(Option<CxfaNode>, Rc<RefCell<CxfaEventParam>>)>>,
    event_host: RefCell<Option<Rc<dyn super::cjx_eventpseudomodel::XfaEventHost>>>,
    instance_manager_host:
        RefCell<Option<Rc<dyn super::cjx_instancemanager::XfaInstanceManagerHost>>>,
    widget_script_host: RefCell<Option<Rc<dyn super::cjx_subform::XfaWidgetScriptHost>>>,
    data_merge_layout_host: RefCell<
        Option<Rc<dyn crate::xfa::fxfa::parser::cxfa_document_datamerge::DataMergeLayoutHost>>,
    >,
    run_at: Cell<XfaAttributeValue>,
    up_objects: RefCell<Vec<CxfaNode>>,
    frames: RefCell<Vec<ScriptFrame>>,
    resolving: Cell<usize>,
    accessed_nodes: RefCell<Vec<CxfaNode>>,
}
struct FrameScope<'a>(&'a CfxJseEngine);
impl Drop for FrameScope<'_> {
    fn drop(&mut self) {
        self.0.frames.borrow_mut().pop();
    }
}
struct ResolveScope<'a>(&'a Cell<usize>);
impl Drop for ResolveScope<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
impl CfxJseEngine {
    pub fn new(document: Rc<CxfaDocument>) -> JsResult<Rc<Self>> {
        Self::with_context(document, crate::fxjs::boa_adapters::new_context()?)
    }
    pub fn with_context(document: Rc<CxfaDocument>, context: Context) -> JsResult<Rc<Self>> {
        Self::create_runtime(document, CfxJsEngine::new(context), false)
    }
    /// Native XFA contexts share the SDK isolate but have their own global
    /// realm. Boa supplies that same separation without another VM or clock.
    pub fn with_runtime(
        document: Rc<CxfaDocument>,
        javascript: Rc<CfxJsEngine>,
    ) -> JsResult<Rc<Self>> {
        Self::create_runtime(document, javascript, true)
    }
    fn create_runtime(
        document: Rc<CxfaDocument>,
        javascript: Rc<CfxJsEngine>,
        new_realm: bool,
    ) -> JsResult<Rc<Self>> {
        javascript.with_context(|context| {
            let main_realm = if new_realm {
                context.create_realm()?
            } else {
                context.realm().clone()
            };
            let mut scope = contexts::RealmScope::new(context, main_realm.clone());
            let context = scope.context();
            register_formcalc_for_document(context)?;
            let main_global = context.global_object();
            let reflection = reflection::Operations::new(context)?;
            let runtime = Rc::new_cyclic(|weak| Self {
                document: document.clone(),
                javascript: javascript.clone(),
                weak_self: weak.clone(),
                resolver: CfxJseResolveProcessor::with_host(
                    document,
                    Rc::new(EngineSomHost(weak.clone())),
                ),
                bindings: RefCell::new(HashMap::new()),
                reverse_bindings: RefCell::new(HashMap::new()),
                pseudo_bindings: RefCell::new(HashMap::new()),
                main_global,
                main_realm,
                reflection,
                variable_contexts: RefCell::new(HashMap::new()),
                event_frames: RefCell::new(Vec::new()),
                event_host: RefCell::new(None),
                instance_manager_host: RefCell::new(None),
                widget_script_host: RefCell::new(None),
                data_merge_layout_host: RefCell::new(None),
                host: RefCell::new(None),
                network_host: RefCell::new(None),
                layout_host: RefCell::new(None),
                run_at: Cell::new(XfaAttributeValue::Client),
                up_objects: RefCell::new(Vec::new()),
                frames: RefCell::new(Vec::new()),
                resolving: Cell::new(0),
                accessed_nodes: RefCell::new(Vec::new()),
            });
            context
                .realm()
                .host_defined_mut()
                .insert(RuntimeLink(runtime.weak_self.clone()));
            if let Some(root) = runtime.document.get_root() {
                let binding = runtime.get_or_create_binding(&root, context)?;
                context.register_global_property(
                    js_string!("xfa"),
                    binding,
                    boa_engine::property::Attribute::all(),
                )?;
                runtime
                    .reverse_bindings
                    .borrow_mut()
                    .insert(context.global_object(), root);
            }
            runtime.install_global_lookup(context, None, None)?;
            Ok(runtime)
        })
    }
    /// Enter the XFA main realm on the already-owned VM and restore whichever
    /// SDK/XFA realm was active, including on an error or Rust unwinding.
    pub fn with_script_context<R>(
        &self,
        operation: impl FnOnce(&mut Context) -> JsResult<R>,
    ) -> JsResult<R> {
        self.javascript.with_context(|context| {
            let mut scope = contexts::RealmScope::new(context, self.main_realm.clone());
            operation(scope.context())
        })
    }
    pub fn from_context(context: &Context) -> Option<Rc<Self>> {
        context
            .realm()
            .host_defined()
            .get::<RuntimeLink>()?
            .0
            .upgrade()
    }
    pub fn document(&self) -> &Rc<CxfaDocument> {
        &self.document
    }
    pub fn javascript(&self) -> &Rc<CfxJsEngine> {
        &self.javascript
    }
    pub fn resolver(&self) -> &CfxJseResolveProcessor {
        &self.resolver
    }
    pub fn current_this_node(&self) -> Option<CxfaNode> {
        self.frames.borrow().last().and_then(|f| f.node.clone())
    }
    pub fn current_script_type(&self) -> CxfaScriptType {
        self.frames
            .borrow()
            .last()
            .map_or(CxfaScriptType::Javascript, |f| f.kind)
    }
    fn validate_node(&self, node: &CxfaNode) -> JsResult<()> {
        if !node.is_alive()
            || node
                .document()
                .is_none_or(|doc| !Rc::ptr_eq(&doc, &self.document))
        {
            return Err(JsNativeError::reference()
                .with_message("XFA node belongs to an expired or different document")
                .into());
        }
        Ok(())
    }
    fn binding_data(&self, node: &CxfaNode) -> JsResult<NodeBinding> {
        self.validate_node(node)?;
        Ok(NodeBinding {
            runtime: self.weak_self.clone(),
            node: node.clone(),
            _store: node.retain_store().ok_or_else(|| {
                JsNativeError::reference().with_message("XFA node store has expired")
            })?,
            generated_accessors: None,
        })
    }
    pub fn node_from_value(&self, value: &JsValue) -> Option<CxfaNode> {
        let object = value.as_object()?;
        if let Some(data) = object.downcast_ref::<NodeBinding>() {
            return Some(data.node.clone());
        }
        self.reverse_bindings.borrow().get(&object).cloned()
    }
    pub fn get_or_create_binding(
        &self,
        node: &CxfaNode,
        context: &mut Context,
    ) -> JsResult<JsObject> {
        if !Self::from_context(context).is_some_and(|runtime| std::ptr::eq(runtime.as_ref(), self))
        {
            return Err(JsNativeError::typ()
                .with_message("XFA binding requires its document's JavaScript context")
                .into());
        }
        self.validate_node(node)?;
        if node.get_element_type() == XfaElement::Script {
            self.run_variables_script_in_context(node, context)?;
        }
        if let Some(object) = self.bindings.borrow().get(node).cloned() {
            return Ok(object);
        }
        let mut realm = contexts::RealmScope::new(context, self.main_realm.clone());
        let context = realm.context();
        let mut data = self.binding_data(node)?;
        data.generated_accessors = Some(Rc::new(RefCell::new(HashSet::new())));
        let target = ObjectInitializer::with_native_data(data, context).build();
        super::cjx_bindings::define_methods(node, &target, context)?;
        crate::fxjs::boa_adapters::finish_class(&target, context)?;
        for key in target.own_property_keys(context)? {
            let boa_engine::property::PropertyKey::String(name) = &key else {
                continue;
            };
            // Inspect the actual plain target without invoking JavaScript
            // getters while the binding is still being initialized.
            let accessor = target
                .borrow()
                .properties()
                .get(&key)
                .is_some_and(|descriptor| descriptor.is_accessor_descriptor());
            if accessor {
                target
                    .downcast_ref::<NodeBinding>()
                    .expect("XFA binding target")
                    .mark_generated_accessor(name);
            }
        }
        let proxy: JsObject = JsProxy::builder(target)
            .get_own_property_descriptor(reflection::descriptor)
            .define_property(reflection::define)
            .delete_property(reflection::delete)
            .prevent_extensions(reflection::prevent_extensions)
            .get(normal_get)
            .set(normal_set)
            .has(normal_has)
            .build(context)?
            .into();
        // Initialization executes no user script. Publish both identity views
        // together only after successful construction; callbacks never borrow them.
        self.reverse_bindings
            .borrow_mut()
            .insert(proxy.clone(), node.clone());
        self.bindings
            .borrow_mut()
            .insert(node.clone(), proxy.clone());
        Ok(proxy)
    }
    pub fn eval(&self, source: &str) -> JsResult<JsValue> {
        self.run_script(
            CxfaScriptType::Javascript,
            &source.chars().map(u32::from).collect::<Vec<_>>(),
            None,
        )
    }
    pub fn run_script(
        &self,
        kind: CxfaScriptType,
        source: &[u32],
        this_node: Option<&CxfaNode>,
    ) -> JsResult<JsValue> {
        let object = this_node.cloned().map(ResolvedObject::Node);
        self.run_script_object(kind, source, object.as_ref())
    }
    pub fn resolve_objects(
        &self,
        reference: Option<&CxfaNode>,
        expression: &[u32],
        flags: XfaResolveFlags,
    ) -> Result<Option<ResolveResult>, ResolveError> {
        self.resolving.set(self.resolving.get() + 1);
        let _scope = ResolveScope(&self.resolving);
        self.resolver.resolve_objects(reference, expression, flags)
    }
    pub fn read_script_attribute(
        &self,
        node: &CxfaNode,
        descriptor: ScriptAttributeDescriptor,
        context: &mut Context,
    ) -> JsResult<JsValue> {
        self.validate_node(node)?;
        super::cjx_object::read_attribute(self, node, descriptor, context)
    }
    pub fn write_script_attribute(
        &self,
        node: &CxfaNode,
        descriptor: ScriptAttributeDescriptor,
        value: &JsValue,
        context: &mut Context,
    ) -> JsResult<()> {
        self.validate_node(node)?;
        super::cjx_object::write_attribute(self, node, descriptor, value, context)
    }
    pub(crate) fn record_access(&self, node: &CxfaNode) {
        if self.frames.borrow().is_empty() {
            return;
        }
        let mut nodes = self.accessed_nodes.borrow_mut();
        if !nodes.contains(node) {
            nodes.push(node.clone());
        }
    }
    pub fn take_accessed_nodes(&self) -> Vec<CxfaNode> {
        self.accessed_nodes.take()
    }
}
pub(crate) fn resolve_error(error: ResolveError) -> JsError {
    match error {
        ResolveError::Script(error) => error,
        other => JsNativeError::error()
            .with_message(other.to_string())
            .into(),
    }
}
struct EngineSomHost(Weak<CfxJseEngine>);
impl EngineSomHost {
    fn runtime(&self) -> Result<Rc<CfxJseEngine>, ResolveError> {
        self.0
            .upgrade()
            .ok_or(ResolveError::Unsupported("expired script context"))
    }
}
impl SomScriptHost for EngineSomHost {
    fn this_object(&self) -> Result<Option<ResolvedObject>, ResolveError> {
        Ok(self.runtime()?.current_this_object())
    }
    fn attribute_object(
        &self,
        object: &ResolvedObject,
        attribute: ScriptAttributeDescriptor,
    ) -> Result<Option<ResolvedObject>, ResolveError> {
        let runtime = self.runtime()?;
        runtime
            .with_script_context(|context| {
                let value = runtime.read_resolved_attribute(object, attribute, context)?;
                Ok(runtime.resolved_object_from_value(&value))
            })
            .map_err(ResolveError::Script)
    }
    fn predicate_object(
        &self,
        language: SomScriptLanguage,
        script: &[u32],
        object: &ResolvedObject,
    ) -> Result<bool, ResolveError> {
        let kind = match language {
            SomScriptLanguage::Formcalc => CxfaScriptType::Formcalc,
            SomScriptLanguage::Javascript => CxfaScriptType::Javascript,
        };
        let runtime = self.runtime()?;
        runtime.resolving.set(runtime.resolving.get() + 1);
        let _scope = ResolveScope(&runtime.resolving);
        Ok(runtime
            .run_script_object(kind, script, Some(object))
            .map(|value| value.to_boolean())
            .unwrap_or(false))
    }
    fn this_node(&self) -> Result<Option<CxfaNode>, ResolveError> {
        Ok(self.runtime()?.current_this_node())
    }
    fn attribute_node(
        &self,
        node: &CxfaNode,
        attribute: ScriptAttributeDescriptor,
    ) -> Result<Option<CxfaNode>, ResolveError> {
        let runtime = self.runtime()?;
        runtime
            .javascript
            .with_context(|context| {
                let value = runtime.read_script_attribute(node, attribute, context)?;
                Ok(runtime.node_from_value(&value))
            })
            .map_err(ResolveError::Script)
    }
    fn predicate(
        &self,
        language: SomScriptLanguage,
        script: &[u32],
        node: &CxfaNode,
    ) -> Result<bool, ResolveError> {
        let kind = match language {
            SomScriptLanguage::Formcalc => CxfaScriptType::Formcalc,
            SomScriptLanguage::Javascript => CxfaScriptType::Javascript,
        };
        // Native predicate script failures remove that candidate. Actual getter
        // exceptions at the attribute boundary retain their opaque JsError.
        let runtime = self.runtime()?;
        // Direct processor consumers (including data merge) share this guard.
        runtime.resolving.set(runtime.resolving.get() + 1);
        let _scope = ResolveScope(&runtime.resolving);
        Ok(runtime
            .run_script(kind, script, Some(node))
            .map(|value| value.to_boolean())
            .unwrap_or(false))
    }
}
fn property_key(
    value: &JsValue,
    context: &mut Context,
) -> JsResult<boa_engine::property::PropertyKey> {
    value.to_property_key(context)
}
fn normal_get(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = args
        .first()
        .and_then(JsValue::as_object)
        .ok_or_else(|| JsNativeError::typ().with_message("Invalid XFA target"))?;
    let key = property_key(args.get(1).unwrap_or(&JsValue::undefined()), context)?;
    if target.has_property(key.clone(), context)? {
        if let boa_engine::property::PropertyKey::String(name) = &key {
            let generated = target.downcast_ref::<NodeBinding>().is_some_and(|data| {
                data.generated_accessors
                    .as_ref()
                    .is_some_and(|names| names.borrow().contains(&name.to_vec()))
            });
            if generated {
                let data = node_data(&target.clone().into(), context)?;
                let runtime = data.runtime()?;
                // Native NormalPropertyGetter resolves named children before
                // script attributes. Synthetic host accessors such as `model`
                // must not hide a valid child with that name. Attribute reads
                // below still use their original callbacks, avoiding recursive
                // binding lookups for CJX_Tree properties.
                let result = CfxJsEngine::with_host_callback(context, || {
                    runtime.resolve_object_values(
                        Some(&data.node),
                        &wide_from_utf16(&name.to_vec()),
                        XfaResolveFlags::CHILDREN | XfaResolveFlags::PROPERTIES,
                    )
                })
                .map_err(resolve_error)?;
                if let Some(result) = result {
                    return runtime.result_object_value(result, context);
                }
            }
        }
        return target.get(key, context);
    }
    let Some(name) = args.get(1).and_then(JsValue::as_string) else {
        return Ok(JsValue::undefined());
    };
    let data = node_data(&target.clone().into(), context)?;
    let runtime = data.runtime()?;
    let name = wide_from_utf16(&name.to_vec());
    if name.iter().copied().eq("xfa".chars().map(u32::from)) {
        return runtime.document.get_root().map_or_else(
            || Ok(JsValue::undefined()),
            |root| {
                runtime
                    .get_or_create_binding(&root, context)
                    .map(Into::into)
            },
        );
    }
    let flags =
        XfaResolveFlags::CHILDREN | XfaResolveFlags::PROPERTIES | XfaResolveFlags::ATTRIBUTES;
    let result = CfxJsEngine::with_host_callback(context, || {
        runtime.resolve_object_values(Some(&data.node), &name, flags)
    })
    .map_err(resolve_error)?;
    if let Some(result) = result {
        return runtime.result_object_value(result, context);
    }
    if runtime.current_this_node().as_ref() == Some(&data.node)
        || (runtime.current_script_type() == CxfaScriptType::Javascript
            && !runtime.document.is_strict_scoping())
    {
        let result = CfxJsEngine::with_host_callback(context, || {
            runtime.resolve_object_values(
                Some(&data.node),
                &name,
                XfaResolveFlags::PARENT | XfaResolveFlags::SIBLINGS,
            )
        })
        .map_err(resolve_error)?;
        if let Some(result) = result {
            return runtime.result_object_value(result, context);
        }
    }
    // Native NormalPropertyGetter checks the descriptor again after SOM.
    // In particular, `this` is a reserved resolver token and can yield no
    // object, while CJX_Xfa::thisValue must still return null outside a frame.
    if let Some(descriptor) = find_script_attribute_descriptor(data.node.get_element_type(), &name)
    {
        return runtime.read_script_attribute(&data.node, descriptor, context);
    }
    if data.node.get_element_type() == XfaElement::Script {
        if let Some(value) = runtime.query_variable_value_in_context(&data.node, &name, context)? {
            return Ok(value);
        }
    }
    runtime
        .query_host_global(&name, context)
        .map(|value| value.unwrap_or_default())
}
fn normal_set(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = args
        .first()
        .and_then(JsValue::as_object)
        .ok_or_else(|| JsNativeError::typ().with_message("Invalid XFA target"))?;
    let key = property_key(args.get(1).unwrap_or(&JsValue::undefined()), context)?;
    let value = args.get(2).cloned().unwrap_or_default();
    let data = node_data(&target.clone().into(), context)?;
    let runtime = data.runtime()?;
    if runtime.resolving.get() > 0 {
        return Ok(true.into());
    }
    if target.has_property(key.clone(), context)? {
        return target.set(key, value, true, context).map(Into::into);
    }
    let Some(name) = args.get(1).and_then(JsValue::as_string) else {
        return Ok(false.into());
    };
    let name = wide_from_utf16(&name.to_vec());
    if let Some(descriptor) = find_script_attribute_descriptor(data.node.get_element_type(), &name)
    {
        runtime.write_script_attribute(&data.node, descriptor, &value, context)?;
        return Ok(true.into());
    }
    let name = name.strip_prefix(&[35]).unwrap_or(&name);
    let element = get_element_by_name(name);
    let child = CfxJsEngine::with_host_callback(context, || {
        if element != XfaElement::Unknown {
            data.node.get_or_create_property(0, element)
        } else {
            data.node.get_first_child_by_name(name)
        }
    });
    if let Some(child) = child {
        if let Some(descriptor) = find_script_attribute_descriptor(
            child.get_element_type(),
            &"{default}".chars().map(u32::from).collect::<Vec<_>>(),
        ) {
            runtime.write_script_attribute(&child, descriptor, &value, context)?;
            return Ok(true.into());
        }
    }
    if data.node.get_element_type() == XfaElement::Script {
        runtime.update_variable_value_in_context(&data.node, &name, value, context)?;
    }
    // Native unknown-property writes do not invent custom JS expandos.
    Ok(true.into())
}

#[path = "cfxjse_engine_contexts.rs"]
mod contexts;

#[cfg(test)]
#[path = "cfxjse_engine_context_tests.rs"]
mod context_tests;
fn normal_has(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = args
        .first()
        .and_then(JsValue::as_object)
        .ok_or_else(|| JsNativeError::typ().with_message("Invalid XFA target"))?;
    let key = property_key(args.get(1).unwrap_or(&JsValue::undefined()), context)?;
    if target.has_property(key, context)? {
        return Ok(true.into());
    }
    let Some(name) = args.get(1).and_then(JsValue::as_string) else {
        return Ok(false.into());
    };
    let data = node_data(&target.clone().into(), context)?;
    let name = wide_from_utf16(&name.to_vec());
    Ok(
        find_script_attribute_descriptor(data.node.get_element_type(), &name)
            .is_some()
            .into(),
    )
}

#[cfg(test)]
#[path = "cfxjse_engine_shared_tests.rs"]
mod shared_runtime_tests;

#[path = "cfxjse_engine_reflection.rs"]
mod reflection;

#[path = "cfxjse_engine_objects.rs"]
mod objects;

#[cfg(test)]
#[path = "cfxjse_engine_objects_tests.rs"]
mod object_tests;
