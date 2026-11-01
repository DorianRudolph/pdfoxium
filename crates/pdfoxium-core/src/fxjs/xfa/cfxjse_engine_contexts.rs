// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Context/variables/global and EventParamScope portions of cfxjse_engine.cpp.
//! Boa's real realm/global prototype APIs provide named lookup; lexical/own
//! bindings remain language-owned. No source rewriting or with(this) is used.
use super::*;
use crate::xfa::fxfa::fxfa_basic::{XfaAttribute, XfaHashCode};
use boa_engine::{
    builtins::function::BoundFunction,
    property::{PropertyDescriptor, PropertyKey},
    realm::Realm,
};

#[derive(Clone)]
pub(super) struct VariablesContext {
    realm: Realm,
    global: JsObject,
}

pub(super) struct RealmScope<'a> {
    context: &'a mut Context,
    previous: Option<Realm>,
}
impl<'a> RealmScope<'a> {
    pub(super) fn new(context: &'a mut Context, realm: Realm) -> Self {
        let previous = context.enter_realm(realm);
        Self {
            context,
            previous: Some(previous),
        }
    }
    pub(super) fn context(&mut self) -> &mut Context {
        self.context
    }
}
impl Drop for RealmScope<'_> {
    fn drop(&mut self) {
        self.context
            .enter_realm(self.previous.take().expect("realm scope restores once"));
    }
}
struct EventScope<'a>(&'a CfxJseEngine);
impl Drop for EventScope<'_> {
    fn drop(&mut self) {
        self.0.event_frames.borrow_mut().pop();
    }
}

#[derive(Clone, Trace, Finalize)]
struct GlobalBinding {
    #[unsafe_ignore_trace]
    runtime: Weak<CfxJseEngine>,
    #[unsafe_ignore_trace]
    variables_script: Option<CxfaNode>,
    #[unsafe_ignore_trace]
    this_node: Option<CxfaNode>,
}
impl JsData for GlobalBinding {}

impl CfxJseEngine {
    pub fn host(&self) -> Option<Rc<dyn super::super::cjx_hostpseudomodel::XfaHost>> {
        self.host.borrow().clone()
    }
    pub fn set_host(&self, host: Option<Rc<dyn super::super::cjx_hostpseudomodel::XfaHost>>) {
        let old = self.host.replace(host);
        drop(old);
    }

    pub fn network_host(
        &self,
    ) -> Option<Rc<dyn super::super::formcalc_network::FormcalcNetworkHost>> {
        self.network_host.borrow().clone()
    }
    pub fn set_network_host(
        &self,
        host: Option<Rc<dyn super::super::formcalc_network::FormcalcNetworkHost>>,
    ) {
        let old = self.network_host.replace(host);
        drop(old);
    }

    pub fn layout_host(
        &self,
    ) -> Option<Rc<dyn super::super::cjx_layoutpseudomodel::XfaLayoutHost>> {
        self.layout_host.borrow().clone()
    }
    pub fn set_layout_host(
        &self,
        host: Option<Rc<dyn super::super::cjx_layoutpseudomodel::XfaLayoutHost>>,
    ) {
        let old = self.layout_host.replace(host);
        drop(old);
    }

    pub fn data_merge_layout_host(
        &self,
    ) -> Option<Rc<dyn crate::xfa::fxfa::parser::cxfa_document_datamerge::DataMergeLayoutHost>>
    {
        self.data_merge_layout_host.borrow().clone()
    }
    pub fn set_data_merge_layout_host(
        &self,
        host: Option<
            Rc<dyn crate::xfa::fxfa::parser::cxfa_document_datamerge::DataMergeLayoutHost>,
        >,
    ) {
        let old = self.data_merge_layout_host.replace(host);
        drop(old);
    }
    pub fn current_event(&self) -> Option<Rc<RefCell<CxfaEventParam>>> {
        self.event_frames
            .borrow()
            .last()
            .map(|(_, event)| event.clone())
    }
    pub fn event_target(&self) -> Option<CxfaNode> {
        self.event_frames
            .borrow()
            .last()
            .and_then(|(target, _)| target.clone())
    }
    pub fn with_event<R>(
        &self,
        target: Option<&CxfaNode>,
        event: Rc<RefCell<CxfaEventParam>>,
        operation: impl FnOnce() -> R,
    ) -> R {
        self.event_frames
            .borrow_mut()
            .push((target.cloned(), event));
        let _scope = EventScope(self);
        operation()
    }
    pub fn event_host(&self) -> Option<Rc<dyn super::super::cjx_eventpseudomodel::XfaEventHost>> {
        self.event_host.borrow().clone()
    }
    pub fn set_event_host(
        &self,
        host: Option<Rc<dyn super::super::cjx_eventpseudomodel::XfaEventHost>>,
    ) {
        let old = self.event_host.replace(host);
        drop(old);
    }
    pub fn instance_manager_host(
        &self,
    ) -> Option<Rc<dyn super::super::cjx_instancemanager::XfaInstanceManagerHost>> {
        self.instance_manager_host.borrow().clone()
    }
    pub fn set_instance_manager_host(
        &self,
        host: Option<Rc<dyn super::super::cjx_instancemanager::XfaInstanceManagerHost>>,
    ) {
        let old = self.instance_manager_host.replace(host);
        drop(old);
    }
    pub fn widget_script_host(
        &self,
    ) -> Option<Rc<dyn super::super::cjx_subform::XfaWidgetScriptHost>> {
        self.widget_script_host.borrow().clone()
    }
    pub fn set_widget_script_host(
        &self,
        host: Option<Rc<dyn super::super::cjx_subform::XfaWidgetScriptHost>>,
    ) {
        let old = self.widget_script_host.replace(host);
        drop(old);
    }
    pub fn set_run_at_type(&self, value: XfaAttributeValue) {
        self.run_at.set(value);
    }
    pub fn is_run_at_client(&self) -> bool {
        self.run_at.get() != XfaAttributeValue::Server
    }
    pub fn add_object_to_up_array(&self, node: &CxfaNode) {
        self.up_objects.borrow_mut().push(node.clone());
    }
    pub fn last_object_from_up_array(&self) -> Option<CxfaNode> {
        self.up_objects.borrow().last().cloned()
    }

    pub(super) fn install_global_lookup(
        &self,
        context: &mut Context,
        this_node: Option<&CxfaNode>,
        variables_script: Option<&CxfaNode>,
    ) -> JsResult<()> {
        // Runtime ownership belongs to this realm. Sharing a VM must not
        // overwrite SDK context data or another XFA/variables realm's owner.
        context
            .realm()
            .host_defined_mut()
            .insert(RuntimeLink(self.weak_self.clone()));
        crate::fxjs::boa_adapters::enable_compatible_mode(context)?;
        crate::fxjs::boa_adapters::set_global_class(
            context,
            if variables_script.is_some() {
                "XFAScriptObject"
            } else {
                "Root"
            },
        )?;
        let global = context.global_object();
        let target = JsObject::from_proto_and_data(
            global.prototype(),
            GlobalBinding {
                runtime: self.weak_self.clone(),
                variables_script: variables_script.cloned(),
                this_node: this_node.cloned(),
            },
        );
        let proxy: JsObject = JsProxy::builder(target)
            .get(global_get)
            .set(global_set)
            .has(global_has)
            .build(context)?
            .into();
        if !global.set_prototype(Some(proxy)) {
            return Err(JsNativeError::typ()
                .with_message("XFA global object cannot install named lookup")
                .into());
        }
        Ok(())
    }
    pub(super) fn query_host_global(
        &self,
        name: &[u32],
        context: &mut Context,
    ) -> JsResult<Option<JsValue>> {
        let key = JsString::from(utf16_encode(name).as_slice());
        if let Some(runtime) = crate::fxjs::cjs_runtime::CjsRuntime::from_context(context) {
            return runtime.get_value_by_name_in_context(key, context).map(Some);
        }
        if self.main_global.has_own_property(key.clone(), context)? {
            self.main_global.get(key, context).map(Some)
        } else {
            Ok(None)
        }
    }
    fn query_node_by_flags(
        &self,
        node: Option<&CxfaNode>,
        name: &[u32],
        flags: XfaResolveFlags,
        context: &mut Context,
    ) -> JsResult<Option<JsValue>> {
        let Some(node) = node else {
            return Ok(None);
        };
        let result = CfxJsEngine::with_host_callback(context, || {
            self.resolve_object_values(Some(node), name, flags)
        })
        .map_err(resolve_error)?;
        result
            .map(|result| self.result_object_value(result, context))
            .transpose()
    }
    fn update_node_by_flags(
        &self,
        node: Option<&CxfaNode>,
        name: &[u32],
        value: &JsValue,
        context: &mut Context,
    ) -> JsResult<bool> {
        let Some(node) = node else {
            return Ok(false);
        };
        let flags = XfaResolveFlags::PARENT
            | XfaResolveFlags::SIBLINGS
            | XfaResolveFlags::CHILDREN
            | XfaResolveFlags::PROPERTIES
            | XfaResolveFlags::ATTRIBUTES;
        let Some(result) = CfxJsEngine::with_host_callback(context, || {
            self.resolve_object_values(Some(node), name, flags)
        })
        .map_err(resolve_error)?
        else {
            return Ok(false);
        };
        if result.result_type == ResolveResultType::Attribute {
            if let (Some(node), Some(descriptor)) =
                (result.objects.first(), result.script_attribute)
            {
                self.write_resolved_attribute(node, descriptor, value, context)?;
            }
        }
        Ok(true)
    }

    pub fn run_variables_script(&self, script: &CxfaNode) -> JsResult<bool> {
        self.javascript
            .with_context(|context| self.run_variables_script_in_context(script, context))
    }
    pub(super) fn run_variables_script_in_context(
        &self,
        script: &CxfaNode,
        context: &mut Context,
    ) -> JsResult<bool> {
        self.validate_node(script)?;
        if script.get_element_type() != XfaElement::Script {
            return Ok(false);
        }
        let Some(variables) = script
            .parent()
            .filter(|node| node.get_element_type() == XfaElement::Variables)
        else {
            return Ok(false);
        };
        if self.variable_contexts.borrow().contains_key(script) {
            return Ok(true);
        }
        let Some(source) = script
            .first_child()
            .and_then(|text| text.try_cdata(XfaAttribute::Value, true))
        else {
            return Ok(false);
        };
        let Some(this_node) = variables.parent() else {
            return Ok(false);
        };
        let realm = context.create_realm()?;
        let mut scope = RealmScope::new(context, realm.clone());
        let context = scope.context();
        let global = context.global_object();
        self.install_global_lookup(context, Some(&this_node), Some(script))?;
        self.reverse_bindings
            .borrow_mut()
            .insert(global.clone(), this_node.clone());
        // Publish before executing: recursive access observes this same realm,
        // including declarations initialized before a subsequently thrown value.
        self.variable_contexts
            .borrow_mut()
            .insert(script.clone(), VariablesContext { realm, global });
        self.frames.borrow_mut().push(ScriptFrame {
            object: Some(ResolvedObject::Node(this_node.clone())),
            node: Some(this_node),
            kind: CxfaScriptType::Javascript,
        });
        let _frame = FrameScope(self);
        let source = utf16_encode(&source);
        context.eval(Source::from_utf16(&source))?;
        Ok(true)
    }
    pub fn query_variable_value(
        &self,
        script: &CxfaNode,
        name: &[u32],
    ) -> JsResult<Option<JsValue>> {
        self.javascript
            .with_context(|context| self.query_variable_value_in_context(script, name, context))
    }
    pub(super) fn query_variable_value_in_context(
        &self,
        script: &CxfaNode,
        name: &[u32],
        context: &mut Context,
    ) -> JsResult<Option<JsValue>> {
        self.validate_node(script)?;
        let Some(variables) = self.variable_contexts.borrow().get(script).cloned() else {
            return Ok(None);
        };
        let mut scope = RealmScope::new(context, variables.realm);
        let context = scope.context();
        let key = JsString::from(utf16_encode(name).as_slice());
        if !variables.global.has_own_property(key.clone(), context)? {
            return Ok(None);
        }
        let value = variables.global.get(key, context)?;
        if let Some(function) = value.as_callable() {
            return BoundFunction::create(function, variables.global.into(), Vec::new(), context)
                .map(|value| Some(value.into()));
        }
        Ok(Some(value))
    }
    pub fn update_variable_value(
        &self,
        script: &CxfaNode,
        name: &[u32],
        value: JsValue,
    ) -> JsResult<bool> {
        self.javascript.with_context(|context| {
            self.update_variable_value_in_context(script, name, value, context)
        })
    }
    pub(super) fn update_variable_value_in_context(
        &self,
        script: &CxfaNode,
        name: &[u32],
        value: JsValue,
        context: &mut Context,
    ) -> JsResult<bool> {
        self.validate_node(script)?;
        let Some(variables) = self.variable_contexts.borrow().get(script).cloned() else {
            return Ok(false);
        };
        let mut scope = RealmScope::new(context, variables.realm);
        let key = JsString::from(utf16_encode(name).as_slice());
        // Boa's top-level `var` bindings are nonconfigurable. Updating their
        // value must retain the declaration's attributes, rather than trying
        // to make the binding configurable again. New exported values receive
        // the native helper's ordinary writable/enumerable/configurable bits.
        let descriptor = PropertyDescriptor::builder().value(value);
        let descriptor = if variables
            .global
            .has_own_property(key.clone(), scope.context())?
        {
            descriptor
        } else {
            descriptor
                .writable(true)
                .enumerable(true)
                .configurable(true)
        };
        variables
            .global
            .define_property_or_throw(key, descriptor, scope.context())?;
        Ok(true)
    }
}

fn global_data(args: &[JsValue]) -> JsResult<(JsObject, GlobalBinding)> {
    let target = args
        .first()
        .and_then(JsValue::as_object)
        .ok_or_else(|| JsNativeError::typ().with_message("Invalid XFA global target"))?;
    let data = target
        .downcast_ref::<GlobalBinding>()
        .map(|data| data.clone())
        .ok_or_else(|| JsNativeError::typ().with_message("Invalid XFA global target"))?;
    Ok((target, data))
}
fn global_runtime(data: &GlobalBinding) -> JsResult<Rc<CfxJseEngine>> {
    data.runtime.upgrade().ok_or_else(|| {
        JsNativeError::reference()
            .with_message("XFA script context has expired")
            .into()
    })
}
fn reference(runtime: &CfxJseEngine, data: &GlobalBinding) -> Option<CxfaNode> {
    data.this_node
        .clone()
        .or_else(|| runtime.current_this_node())
}
fn method_value(
    runtime: &CfxJseEngine,
    data: &GlobalBinding,
    name: &JsString,
    context: &mut Context,
) -> JsResult<Option<JsValue>> {
    let node = data
        .this_node
        .clone()
        .or_else(|| runtime.document.get_root());
    let Some(node) = node else {
        return Ok(None);
    };
    let object = runtime.get_or_create_binding(&node, context)?;
    // Native GlobalPropTypeGetter gives only CJX methods precedence over
    // current-this SOM lookup. Reflected attributes on the root/variables
    // object must not capture bare names such as `index` in a predicate.
    // Inspect the descriptor value without executing an attribute getter.
    let descriptor = runtime.reflection.get_descriptor(&object, name, context)?;
    let Some(descriptor) = descriptor.as_object() else {
        return Ok(None);
    };
    let value_key = js_string!("value");
    if !descriptor.has_own_property(value_key.clone(), context)? {
        return Ok(None);
    }
    let value = descriptor.get(value_key, context)?;
    Ok(value.as_callable().is_some().then_some(value))
}
fn global_get(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (target, data) = global_data(args)?;
    let runtime = global_runtime(&data)?;
    let key = property_key(args.get(1).unwrap_or(&JsValue::undefined()), context)?;
    if target.has_property(key.clone(), context)? {
        return target.get(key, context);
    }
    let Some(name) = args.get(1).and_then(JsValue::as_string) else {
        return Ok(JsValue::undefined());
    };
    if let Some(method) = method_value(&runtime, &data, &name, context)? {
        return Ok(method);
    }
    let wide = wide_from_utf16(&name.to_vec());
    if runtime.current_script_type() == CxfaScriptType::Formcalc {
        let hash = crate::xfa::fxfa::parser::cxfa_node::hash_name(&wide);
        if hash != XfaHashCode::Layout as u32 {
            if let Some(kind) = PseudoModel::from_hash(hash) {
                return runtime
                    .get_or_create_pseudo_binding(kind, context)
                    .map(Into::into);
            }
            if let Some(node) = runtime.document.get_xfa_node(hash) {
                return runtime
                    .get_or_create_binding(&node, context)
                    .map(Into::into);
            }
        }
    }
    let node = reference(&runtime, &data);
    for flags in [
        XfaResolveFlags::CHILDREN | XfaResolveFlags::PROPERTIES | XfaResolveFlags::ATTRIBUTES,
        XfaResolveFlags::PARENT | XfaResolveFlags::SIBLINGS,
    ] {
        if let Some(value) = runtime.query_node_by_flags(node.as_ref(), &wide, flags, context)? {
            return Ok(value);
        }
    }
    if let Some(script) = data.variables_script.as_ref() {
        if let Some(value) = runtime.query_variable_value_in_context(script, &wide, context)? {
            return Ok(value);
        }
    }
    runtime
        .query_host_global(&wide, context)
        .map(|value| value.unwrap_or_default())
}
fn global_has(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (target, data) = global_data(args)?;
    let runtime = global_runtime(&data)?;
    let key = property_key(args.get(1).unwrap_or(&JsValue::undefined()), context)?;
    if target.has_property(key, context)? {
        return Ok(true.into());
    }
    let Some(name) = args.get(1).and_then(JsValue::as_string) else {
        return Ok(false.into());
    };
    if method_value(&runtime, &data, &name, context)?.is_some() {
        return Ok(true.into());
    }
    let wide = wide_from_utf16(&name.to_vec());
    let node = reference(&runtime, &data);
    if runtime.current_script_type() == CxfaScriptType::Formcalc {
        let hash = crate::xfa::fxfa::parser::cxfa_node::hash_name(&wide);
        if hash != XfaHashCode::Layout as u32
            && (PseudoModel::from_hash(hash).is_some()
                || runtime.document.get_xfa_node(hash).is_some())
        {
            return Ok(true.into());
        }
    }
    if let Some(node) = node {
        let flags = XfaResolveFlags::CHILDREN
            | XfaResolveFlags::PROPERTIES
            | XfaResolveFlags::ATTRIBUTES
            | XfaResolveFlags::PARENT
            | XfaResolveFlags::SIBLINGS;
        if CfxJsEngine::with_host_callback(context, || {
            runtime.resolve_object_values(Some(&node), &wide, flags)
        })
        .map_err(resolve_error)?
        .is_some()
        {
            return Ok(true.into());
        }
    }
    if let Some(sdk) = crate::fxjs::cjs_runtime::CjsRuntime::from_context(context) {
        return sdk
            .has_value_by_name_in_context(name, context)
            .map(Into::into);
    }
    runtime
        .main_global
        .has_own_property(name, context)
        .map(Into::into)
}
fn global_set(_this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (_target, data) = global_data(args)?;
    let runtime = global_runtime(&data)?;
    let key = property_key(args.get(1).unwrap_or(&JsValue::undefined()), context)?;
    let value = args.get(2).cloned().unwrap_or_default();
    if let PropertyKey::String(name) = &key {
        let wide = wide_from_utf16(&name.to_vec());
        let node = reference(&runtime, &data);
        if runtime.update_node_by_flags(node.as_ref(), &wide, &value, context)? {
            return Ok(true.into());
        }
        if data.variables_script.is_some() && value.is_undefined() {
            if let Some(receiver) = args.get(3).and_then(JsValue::as_object) {
                return receiver
                    .delete_property_or_throw(key, context)
                    .map(Into::into);
            }
        }
    }
    if let PropertyKey::String(name) = &key {
        if let Some(sdk) = crate::fxjs::cjs_runtime::CjsRuntime::from_context(context) {
            return sdk
                .set_value_by_name_in_context(name.clone(), value, context)
                .map(Into::into);
        }
    }
    // Standalone XFA realms keep their real JavaScript global store. No SDK
    // runtime is constructed just to service a missing embedding capability.
    runtime
        .main_global
        .create_data_property(key, value, context)
        .map(Into::into)
}
