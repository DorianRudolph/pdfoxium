// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_instancemanager.{h,cpp}` using real model instances and optional FF
//! initialization/index/layout services. No default host claims Full-XFA.
use super::cfxjse_engine::{node_data, CfxJseEngine};
use crate::{
    fxjs::{cfxjs_engine::CfxJsEngine, cjs_field::binding_error, js_resources::JsMessage},
    xfa::fxfa::{fxfa_basic::XfaElement, parser::cxfa_node::CxfaNode},
};
use boa_engine::{
    native_function::NativeFunctionPointer, object::FunctionObjectBuilder,
    property::PropertyDescriptor, Context, JsNativeError, JsObject, JsResult, JsString, JsValue,
};
use std::rc::Rc;

/// Real document-host capabilities required by the native mutation methods.
/// The caller supplies each operation explicitly; failures propagate unchanged.
pub trait XfaInstanceManagerHost {
    fn is_full_xfa(&self) -> bool;
    fn run_node_initialize(&self, node: &CxfaNode) -> JsResult<()>;
    fn run_subform_index_change(&self, node: &CxfaNode) -> JsResult<()>;
    fn mark_layout_changed(&self) -> JsResult<()>;
}
fn error(method: &str, message: JsMessage) -> boa_engine::JsError {
    binding_error("InstanceManager", method, message)
}
fn model_error(message: &str) -> boa_engine::JsError {
    JsNativeError::error()
        .with_message(message.to_owned())
        .into()
}
fn merge_error(
    error: crate::xfa::fxfa::parser::cxfa_document_datamerge::DataMergeError,
) -> boa_engine::JsError {
    match error {
        crate::xfa::fxfa::parser::cxfa_document_datamerge::DataMergeError::Resolve(error) => {
            super::cfxjse_engine::resolve_error(error)
        }
        other => model_error(&other.to_string()),
    }
}
fn checked_receiver(this: &JsValue, context: &Context) -> JsResult<(Rc<CfxJseEngine>, CxfaNode)> {
    let binding = node_data(this, context)?;
    if binding.node.get_element_type() != XfaElement::InstanceManager {
        return Err(JsNativeError::typ()
            .with_message("Invalid XFA instance manager receiver")
            .into());
    }
    Ok((binding.runtime()?, binding.node.clone()))
}
fn host(
    runtime: &CfxJseEngine,
    full: bool,
    context: &mut Context,
) -> JsResult<Rc<dyn XfaInstanceManagerHost>> {
    let host = runtime
        .instance_manager_host()
        .ok_or_else(|| error("mutation", JsMessage::NotSupportedError))?;
    if full && !CfxJsEngine::with_host_callback(context, || host.is_full_xfa()) {
        return Err(error("mutation", JsMessage::NotSupportedError));
    }
    Ok(host)
}
fn occur_limits(node: &CxfaNode) -> (i32, i32) {
    node.get_occur_if_exists().map_or((1, 1), |occur| {
        (occur.occur_get_min(), occur.occur_get_max())
    })
}
fn changed(host: &Rc<dyn XfaInstanceManagerHost>, context: &mut Context) -> JsResult<()> {
    CfxJsEngine::with_host_callback(context, || host.mark_layout_changed())
}
fn initialized(
    host: &Rc<dyn XfaInstanceManagerHost>,
    node: &CxfaNode,
    context: &mut Context,
) -> JsResult<()> {
    CfxJsEngine::with_host_callback(context, || host.run_node_initialize(node))
}

pub fn define_instance_manager_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    for (name, length, callback) in [
        ("addInstance", 0, add_instance as NativeFunctionPointer),
        ("insertInstance", 1, insert_instance),
        ("moveInstance", 2, move_instance_method),
        ("removeInstance", 1, remove_instance),
        ("setInstances", 1, set_instances_method),
    ] {
        let function = FunctionObjectBuilder::new(
            context.realm(),
            super::cjx_bindings::native_method(
                callback,
                super::cjx_bindings::MethodClass::Element(
                    crate::xfa::fxfa::fxfa_basic::XfaElement::InstanceManager,
                ),
            ),
        )
        .name(JsString::from(name))
        .length(length)
        .constructor(false)
        .build();
        object.define_property_or_throw(
            JsString::from(name),
            PropertyDescriptor::builder()
                .value(function)
                .writable(false)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    Ok(())
}
pub(crate) fn read_attribute(node: &CxfaNode, callback_name: &str) -> JsResult<JsValue> {
    match callback_name {
        "CJX_InstanceManager::count" => Ok(node.get_count().into()),
        "CJX_InstanceManager::min" => Ok(occur_limits(node).0.into()),
        "CJX_InstanceManager::max" => Ok(occur_limits(node).1.into()),
        _ => Err(model_error("Unknown InstanceManager property")),
    }
}
pub(crate) fn write_attribute(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    callback_name: &str,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<()> {
    if callback_name != "CJX_InstanceManager::count" {
        return Err(JsNativeError::typ()
            .with_message("Invalid property set operation")
            .into());
    }
    let host = host(runtime, true, context)?;
    let desired = value.to_i32(context)?;
    set_instances(runtime, node, desired, &host, context)
}
fn set_instances(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    desired: i32,
    host: &Rc<dyn XfaInstanceManagerHost>,
    context: &mut Context,
) -> JsResult<()> {
    let (min, max) = occur_limits(node);
    if desired < min || max >= 0 && desired > max {
        return Err(error("count", JsMessage::TooManyOccurrences));
    }
    let mut count = node.get_count();
    if count == desired {
        return Ok(());
    }
    while count > desired {
        // Look up the next actual managed instance, skipping unrelated siblings.
        // Native SetInstances repeatedly inspects one sibling and can loop forever.
        let remove = node
            .get_item_if_exists(desired)
            .ok_or_else(|| error("count", JsMessage::InvalidInputError))?;
        if !CfxJsEngine::with_host_callback(context, || node.remove_item(&remove, true)) {
            return Err(model_error("Cannot remove XFA instance"));
        }
        let updated = node.get_count();
        if updated >= count {
            return Err(model_error(
                "Instance removal did not reduce the managed count",
            ));
        }
        count = updated;
    }
    while count < desired {
        let instance = CfxJsEngine::with_host_callback(context, || {
            node.create_instance_if_possible_with_resolver(true, runtime.resolver())
        })
        .map_err(merge_error)?
        .ok_or_else(|| model_error("Cannot create XFA instance"))?;
        if !CfxJsEngine::with_host_callback(context, || {
            node.insert_item(&instance, count, count, false)
        }) {
            return Err(model_error("Cannot insert XFA instance"));
        }
        initialized(host, &instance, context)?;
        let updated = node.get_count();
        if updated <= count {
            return Err(model_error(
                "Instance initialization did not increase the managed count",
            ));
        }
        count = updated;
    }
    changed(host, context)
}
pub(crate) fn move_instance(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    to: i32,
    from: i32,
    context: &mut Context,
) -> JsResult<()> {
    let host = host(runtime, false, context)?;
    let count = node.get_count();
    if from > count || to > count - 1 {
        return Err(error("moveInstance", JsMessage::InvalidInputError));
    }
    if from < 0 || to < 0 || from == to {
        return Ok(());
    }
    let instance = node
        .get_item_if_exists(from)
        .ok_or_else(|| error("moveInstance", JsMessage::InvalidInputError))?;
    let moved = CfxJsEngine::with_host_callback(context, || {
        node.remove_item(&instance, false) && node.insert_item(&instance, to, count - 1, true)
    });
    if !moved {
        return Err(model_error("Cannot move XFA instance"));
    }
    changed(&host, context)
}
pub(crate) fn notify_index_changes(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    to: i32,
    from: i32,
    context: &mut Context,
) -> JsResult<()> {
    let host = host(runtime, false, context)?;
    for index in [to, from] {
        if let Some(instance) = node
            .get_item_if_exists(index)
            .filter(|instance| instance.get_element_type() == XfaElement::Subform)
        {
            CfxJsEngine::with_host_callback(context, || host.run_subform_index_change(&instance))?;
        }
    }
    Ok(())
}
fn move_instance_method(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (runtime, node) = checked_receiver(this, context)?;
    let _host = host(&runtime, true, context)?;
    if args.len() != 2 {
        return Err(error("moveInstance", JsMessage::ParamError));
    }
    let from = args[0].to_i32(context)?;
    let to = args[1].to_i32(context)?;
    move_instance(&runtime, &node, to, from, context)?;
    notify_index_changes(&runtime, &node, to, from, context)?;
    Ok(JsValue::undefined())
}
fn remove_instance(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (runtime, node) = checked_receiver(this, context)?;
    let host = host(&runtime, true, context)?;
    if args.len() != 1 {
        return Err(error("removeInstance", JsMessage::ParamError));
    }
    let index = args[0].to_i32(context)?;
    let count = node.get_count();
    if index < 0 || index >= count {
        return Err(error("removeInstance", JsMessage::InvalidInputError));
    }
    if count - 1 < occur_limits(&node).0 {
        return Err(error("removeInstance", JsMessage::TooManyOccurrences));
    }
    let instance = node
        .get_item_if_exists(index)
        .ok_or_else(|| error("removeInstance", JsMessage::ParamError))?;
    if !CfxJsEngine::with_host_callback(context, || node.remove_item(&instance, true)) {
        return Err(model_error("Cannot remove XFA instance"));
    }
    for index in index..count - 1 {
        if let Some(instance) = node
            .get_item_if_exists(index)
            .filter(|node| node.get_element_type() == XfaElement::Subform)
        {
            CfxJsEngine::with_host_callback(context, || host.run_subform_index_change(&instance))?;
        }
    }
    changed(&host, context)?;
    Ok(JsValue::undefined())
}
fn set_instances_method(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (runtime, node) = checked_receiver(this, context)?;
    let host = host(&runtime, true, context)?;
    if args.len() != 1 {
        return Err(error("setInstances", JsMessage::ParamError));
    }
    let desired = args[0].to_i32(context)?;
    set_instances(&runtime, &node, desired, &host, context)?;
    Ok(JsValue::undefined())
}
fn add_instance(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (runtime, node) = checked_receiver(this, context)?;
    let host = host(&runtime, true, context)?;
    if args.len() > 1 {
        return Err(error("addInstance", JsMessage::ParamError));
    }
    let bind = args.first().is_none_or(JsValue::to_boolean);
    let count = node.get_count();
    let max = occur_limits(&node).1;
    if max >= 0 && count >= max {
        return Err(error("addInstance", JsMessage::TooManyOccurrences));
    }
    create_and_insert(&runtime, &node, count, count, bind, false, &host, context)
}
fn insert_instance(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (runtime, node) = checked_receiver(this, context)?;
    let host = host(&runtime, true, context)?;
    if !(1..=2).contains(&args.len()) {
        return Err(error("insertInstance", JsMessage::ParamError));
    }
    let index = args[0].to_i32(context)?;
    let bind = args.get(1).is_some_and(JsValue::to_boolean);
    let count = node.get_count();
    let max = occur_limits(&node).1;
    if index < 0 || index > count || max >= 0 && count >= max {
        return Err(error("insertInstance", JsMessage::InvalidInputError));
    }
    create_and_insert(&runtime, &node, index, count, bind, true, &host, context)
}
#[allow(clippy::too_many_arguments)]
fn create_and_insert(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    index: i32,
    count: i32,
    bind: bool,
    move_data: bool,
    host: &Rc<dyn XfaInstanceManagerHost>,
    context: &mut Context,
) -> JsResult<JsValue> {
    let instance = CfxJsEngine::with_host_callback(context, || {
        node.create_instance_if_possible_with_resolver(bind, runtime.resolver())
    })
    .map_err(merge_error)?;
    let Some(instance) = instance else {
        return Ok(JsValue::null());
    };
    if !CfxJsEngine::with_host_callback(context, || {
        node.insert_item(&instance, index, count, move_data)
    }) {
        return Err(model_error("Cannot insert XFA instance"));
    }
    initialized(host, &instance, context)?;
    changed(host, context)?;
    Ok(runtime.get_or_create_binding(&instance, context)?.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
        },
        xfa::fxfa::{
            fxfa_basic::{XfaHashCode, XfaPacketType},
            parser::{
                cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder,
                cxfa_model_properties::CxfaScriptType,
            },
        },
    };
    use std::{
        cell::{Cell, RefCell},
        rc::Weak,
    };
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn fixture() -> (Rc<CfxJseEngine>, CxfaNode) {
        let input="<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><template xmlns='http://www.xfa.org/schema/xfa-template/3.3/'><subform name='record'><subform name='row'><occur min='0' max='4' initial='0'/><field name='value'><ui><textEdit/></ui></field></subform></subform></template><xfa:datasets xmlns:xfa='http://www.xfa.org/schema/xfa-data/1.0/'><xfa:data><record><row><value>one</value></row><row><value>two</value></row></record></xfa:data></xfa:datasets></xdp:xdp>";
        let mut bytes = input.as_bytes().to_vec();
        bytes.push(0);
        let xml = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
            .parse()
            .unwrap();
        let doc = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(doc.clone());
        assert!(builder.build_document(Rc::new(RefCell::new(xml)), XfaPacketType::Xdp));
        assert!(doc.set_root(builder.get_root_node()));
        let runtime = CfxJseEngine::new(doc.clone()).unwrap();
        doc.do_data_merge_with_resolver(runtime.resolver()).unwrap();
        let manager = doc
            .get_xfa_node(XfaHashCode::Form as u32)
            .unwrap()
            .get_first_child_by_class(XfaElement::Subform)
            .unwrap()
            .get_first_child_by_name(&wide("_row"))
            .unwrap();
        assert_eq!(
            runtime
                .eval("var manager = xfa.form.resolveNode('record._row'); manager.count === 2")
                .unwrap()
                .as_boolean(),
            Some(true)
        );
        (runtime, manager)
    }
    struct Host {
        runtime: Weak<CfxJseEngine>,
        full: bool,
        events: RefCell<Vec<String>>,
        fail_init: Cell<bool>,
    }
    impl XfaInstanceManagerHost for Host {
        fn is_full_xfa(&self) -> bool {
            self.full
        }
        fn run_node_initialize(&self, node: &CxfaNode) -> JsResult<()> {
            self.events.borrow_mut().push("initialize".into());
            if self.fail_init.replace(false) {
                return Err(boa_engine::JsError::from_opaque(
                    JsString::from("initialization failure").into(),
                ));
            }
            let runtime = self.runtime.upgrade().unwrap();
            assert_eq!(
                runtime
                    .run_script(
                        CxfaScriptType::Javascript,
                        &wide("this.resolveNode('value').rawValue = 'initialized'; true"),
                        Some(node)
                    )?
                    .as_boolean(),
                Some(true)
            );
            Ok(())
        }
        fn run_subform_index_change(&self, node: &CxfaNode) -> JsResult<()> {
            self.events
                .borrow_mut()
                .push(format!("index:{}", node.get_instance_index()));
            let runtime = self.runtime.upgrade().unwrap();
            assert!(runtime.eval("manager.count >= 0")?.to_boolean());
            Ok(())
        }
        fn mark_layout_changed(&self) -> JsResult<()> {
            self.events.borrow_mut().push("layout".into());
            Ok(())
        }
    }
    fn host(runtime: &Rc<CfxJseEngine>, full: bool) -> Rc<Host> {
        Rc::new(Host {
            runtime: Rc::downgrade(runtime),
            full,
            events: RefCell::new(Vec::new()),
            fail_init: Cell::new(false),
        })
    }
    fn check(runtime: &CfxJseEngine, script: &str) {
        assert_eq!(
            runtime.eval(script).unwrap().as_boolean(),
            Some(true),
            "{script}"
        );
    }
    #[test]
    fn absent_or_non_full_host_rejects_before_coercion_and_readonly_limits_do_not_coerce() {
        let (runtime, manager) = fixture();
        check(&runtime,"var touched=false; var unsupported=false; try {manager.setInstances({valueOf(){touched=true;return 0;}});}catch(e){unsupported=true;} unsupported && !touched && manager.count===2 && manager.min===0 && manager.max===4");
        check(&runtime,"var readonly=false; try{manager.max={valueOf(){touched=true;return 9;}};}catch(e){readonly=true;} readonly&&!touched&&manager.max===4");
        let host = host(&runtime, false);
        runtime.set_instance_manager_host(Some(host.clone()));
        check(&runtime,"var unsupported=false;try{manager.removeInstance(0);}catch(e){unsupported=true;}unsupported&&manager.count===2");
        assert!(host.events.borrow().is_empty());
        assert_eq!(manager.get_count(), 2);
    }
    #[test]
    fn add_insert_move_remove_and_count_follow_real_model_identity_bindings_and_host_events() {
        let (runtime, manager) = fixture();
        let host = host(&runtime, true);
        runtime.set_instance_manager_host(Some(host.clone()));
        check(&runtime,"var first=manager.parent.resolveNodes('row[*]').item(0);var added=manager.addInstance(false); manager.count===3&&added.resolveNode('value').rawValue==='initialized'&&manager.parent.resolveNodes('row[*]').item(2)===added");
        assert_eq!(&*host.events.borrow(), &["initialize", "layout"]);
        check(&runtime,"var inserted=manager.insertInstance(0,true);manager.count===4&&manager.parent.resolveNodes('row[*]').item(0)===inserted&&manager.parent.resolveNodes('row[*]').item(1)===first");
        check(&runtime,"var maxError=false;try{manager.addInstance();}catch(e){maxError=true;}var insertError=false;try{manager.insertInstance(0);}catch(e){insertError=true;} maxError&&insertError&&manager.count===4");
        host.events.borrow_mut().clear();
        check(&runtime,"manager.moveInstance(3,1);manager.parent.resolveNodes('row[*]').item(1)===added&&manager.count===4");
        assert_eq!(&*host.events.borrow(), &["layout", "index:1", "index:3"]);
        let moved = manager.get_item_if_exists(1).unwrap();
        let moved_data = moved.get_bind_data().unwrap();
        assert_eq!(moved_data.get_bind_items_copy(), [moved.clone()]);
        check(&runtime,"manager.removeInstance(0);manager.count===3&&inserted.parent===null&&manager.parent.resolveNodes('row[*]').item(0)===added");
        check(&runtime,"manager.setInstances(1);manager.count===1&&manager.parent.resolveNodes('row[*]').item(0)===added");
        check(&runtime, "manager.count=0; manager.count===0");
        assert_eq!(manager.get_count(), 0);
        check(&runtime,"var fromZero=manager.addInstance();manager.count===1&&fromZero.parent===manager.parent&&fromZero.resolveNode('value').rawValue==='initialized'");
    }
    #[test]
    fn lowering_count_skips_unrelated_siblings_and_enforces_occurrence_minimum() {
        let (runtime, manager) = fixture();
        let host = host(&runtime, true);
        runtime.set_instance_manager_host(Some(host));
        let gap = runtime
            .document()
            .create_node(XfaPacketType::Form, XfaElement::Draw)
            .unwrap();
        let first = manager.get_item_if_exists(0).unwrap();
        assert!(manager
            .parent()
            .unwrap()
            .insert_child_before_and_notify(&gap, Some(&first)));
        check(&runtime, "manager.count=0;manager.count===0");
        assert!(gap.parent().is_some());
        check(&runtime, "manager.setInstances(2);manager.count===2");
        manager.get_occur_if_exists().unwrap().occur_set_min(2);
        check(&runtime,"var removeError=false;try{manager.removeInstance(0);}catch(e){removeError=true;}var minError=false;try{manager.count=1;}catch(e){minError=true;}removeError&&minError&&manager.count===2");
    }
    #[test]
    fn host_initialization_exceptions_propagate_without_faking_rollback() {
        let (runtime, manager) = fixture();
        let host = host(&runtime, true);
        host.fail_init.set(true);
        runtime.set_instance_manager_host(Some(host.clone()));
        check(&runtime,"var original=false;try{manager.addInstance();}catch(e){original=e==='initialization failure';}original&&manager.count===3");
        assert_eq!(manager.get_count(), 3);
        assert_eq!(&*host.events.borrow(), &["initialize"]);
    }
    #[test]
    fn instance_index_setter_uses_shared_move_and_index_callbacks() {
        let (runtime, manager) = fixture();
        let host = host(&runtime, true);
        runtime.set_instance_manager_host(Some(host.clone()));
        let first = manager.get_item_if_exists(0).unwrap();
        check(&runtime,"var first=manager.parent.resolveNodes('row[*]').item(0);first.instanceIndex=1;first.instanceIndex===1&&manager.parent.resolveNodes('row[*]').item(1)===first");
        assert_eq!(manager.get_item_if_exists(1), Some(first));
        assert_eq!(&*host.events.borrow(), &["layout", "index:1", "index:0"]);
    }
}
