// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `CJX_Node` methods/properties over actual Boa node bindings.
//! Native PDFium TODO methods preserve their observable no-op/arity behavior;
//! XML replacement transfers actual per-node calculation dependencies.
use super::cfxjse_engine::{node_data, NodeBinding};
use crate::{
    fxcrt::{
        cfx_memorystream::CfxMemoryStream,
        cfx_read_only_span_stream::CfxReadOnlySpanStream,
        fx_stream::WriteStream,
        fx_string::{utf16_encode, utf8_decode, utf8_encode},
        xml::{
            cfx_xmldocument::CfxXmlDocument, cfx_xmlnode::XmlNodeType, cfx_xmlparser::CfxXmlParser,
        },
    },
    fxjs::{
        cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16, cjs_field::binding_error,
        js_resources::JsMessage,
    },
    xfa::fxfa::{
        fxfa::XfaEventError,
        fxfa_basic::{XfaAttribute, XfaElement, XfaPacketType},
        parser::{
            cxfa_document_builder::{recognize_rich_text, CxfaDocumentBuilder},
            cxfa_node::{CxfaNode, XfaNodeFlag, XFA_NODE_FILTER_ONE_OF_PROPERTY},
            xfa_basic_data::{get_attribute_by_name, get_element_by_name},
            xfa_utils::{deal_with_data_group_node, regenerate_form_file},
        },
    },
};
use boa_engine::{
    native_function::NativeFunctionPointer, object::FunctionObjectBuilder,
    property::PropertyDescriptor, Context, JsNativeError, JsObject, JsResult, JsString, JsValue,
    NativeFunction,
};
use std::{cell::RefCell, rc::Rc};
fn argument_error(method: &str) -> boa_engine::JsError {
    binding_error("Node", method, JsMessage::ParamError)
}
fn model_error(message: &str) -> boa_engine::JsError {
    JsNativeError::error()
        .with_message(message.to_owned())
        .into()
}
fn to_wide(value: &JsValue, context: &mut Context) -> JsResult<Vec<u32>> {
    Ok(wide_from_utf16(&value.to_string(context)?.to_vec()))
}
fn from_wide(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}
fn validate(binding: &NodeBinding) -> JsResult<()> {
    binding.runtime()?;
    if !binding.node.is_alive() {
        return Err(model_error("The XFA node is no longer available."));
    }
    Ok(())
}
fn box_node(
    binding: &NodeBinding,
    node: Option<CxfaNode>,
    context: &mut Context,
) -> JsResult<JsValue> {
    let runtime = binding.runtime()?;
    match node {
        Some(node) => Ok(runtime.get_or_create_binding(&node, context)?.into()),
        None => Ok(JsValue::null()),
    }
}

pub(crate) fn define_node_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    let methods: &[(&str, usize, NativeFunctionPointer)] = &[
        ("applyXSL", 1, apply_xsl),
        ("assignNode", 1, assign_node),
        ("clone", 1, clone_node),
        ("getAttribute", 1, get_attribute),
        ("getElement", 1, get_element),
        ("isPropertySpecified", 1, is_property_specified),
        ("loadXML", 1, load_xml),
        ("saveFilteredXML", 0, save_filtered_xml),
        ("saveXML", 0, save_xml),
        ("setAttribute", 2, set_attribute),
        ("setElement", 1, set_element),
    ];
    for &(name, length, callback) in methods {
        let function = FunctionObjectBuilder::new(
            context.realm(),
            super::cjx_bindings::native_method(callback, super::cjx_bindings::MethodClass::Node),
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
    let properties: &[(&str, NativeFunctionPointer)] = &[
        ("ns", get_ns),
        ("model", get_model),
        ("isContainer", get_is_container),
        ("isNull", get_is_null),
        ("oneOfChild", get_one_of_child),
    ];
    for &(name, callback) in properties {
        let getter =
            FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(callback))
                .name(JsString::from(format!("get {name}")))
                .length(0)
                .constructor(false)
                .build();
        let setter =
            FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set_readonly))
                .name(JsString::from(format!("set {name}")))
                .length(1)
                .constructor(false)
                .build();
        object.define_property_or_throw(
            JsString::from(name),
            PropertyDescriptor::builder()
                .get(getter)
                .set(setter)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    Ok(())
}
fn apply_xsl(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let _ = node_data(this, context)?;
    if args.len() != 1 {
        return Err(argument_error("applyXSL"));
    }
    // The pinned native body is deliberately empty (pdfium:501).
    // Do not coerce arguments or claim to perform a transformation.
    Ok(JsValue::undefined())
}
fn assign_node(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let _ = node_data(this, context)?;
    if args.is_empty() || args.len() > 3 {
        return Err(argument_error("assignNode"));
    }
    // The pinned native body is deliberately empty (pdfium:501).
    // Do not coerce arguments or claim to perform a transformation.
    Ok(JsValue::undefined())
}
fn set_element(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let _ = node_data(this, context)?;
    if args.len() != 1 && args.len() != 2 {
        return Err(argument_error("setElement"));
    }
    // The pinned native body is deliberately empty (pdfium:501).
    // Do not coerce arguments or claim to perform a transformation.
    Ok(JsValue::undefined())
}
fn save_filtered_xml(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let _ = node_data(this, context)?;
    // The pinned native body is deliberately empty (pdfium:501).
    // Do not coerce arguments or claim to perform a transformation.
    Ok(JsValue::undefined())
}
fn clone_node(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 {
        return Err(argument_error("clone"));
    }
    let recursive = args[0].to_boolean();
    validate(&binding)?;
    let cloned = CfxJsEngine::with_host_callback(context, || binding.node.clone_node(recursive));
    box_node(&binding, cloned, context)
}
fn get_attribute(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 {
        return Err(argument_error("getAttribute"));
    }
    let name = to_wide(&args[0], context)?;
    validate(&binding)?;
    Ok(from_wide(&binding.node.get_attribute_by_string(&name)))
}
fn set_attribute(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 2 {
        return Err(argument_error("setAttribute"));
    }
    // XFA deliberately specifies value before name.
    let value = to_wide(&args[0], context)?;
    let name = to_wide(&args[1], context)?;
    validate(&binding)?;
    CfxJsEngine::with_host_callback(context, || {
        binding.node.set_attribute_by_string(&name, &value)
    });
    Ok(JsValue::undefined())
}
fn get_element(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.is_empty() || args.len() > 2 {
        return Err(argument_error("getElement"));
    }
    let name = to_wide(&args[0], context)?;
    let index = if args.len() == 2 {
        args[1].to_i32(context)?
    } else {
        0
    };
    validate(&binding)?;
    let element = get_element_by_name(&name);
    if element == XfaElement::Unknown {
        return Ok(JsValue::null());
    }
    let property = CfxJsEngine::with_host_callback(context, || {
        binding.node.get_or_create_property(index, element)
    });
    box_node(&binding, property, context)
}
fn is_property_specified(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.is_empty() || args.len() > 3 {
        return Err(argument_error("isPropertySpecified"));
    }
    let name = to_wide(&args[0], context)?;
    validate(&binding)?;
    let attribute = get_attribute_by_name(&name);
    if attribute.is_some_and(|attribute| binding.node.has_local_attribute(attribute.attribute)) {
        return Ok(true.into());
    }
    let element = get_element_by_name(&name);
    if element == XfaElement::Unknown {
        return Ok(false.into());
    }
    let parent = args.len() < 2 || args[1].to_boolean();
    let index = if args.len() == 3 {
        args[2].to_i32(context)?
    } else {
        0
    };
    validate(&binding)?;
    let specified = CfxJsEngine::with_host_callback(context, || {
        if binding
            .node
            .get_or_create_property(index, element)
            .is_some()
        {
            return true;
        }
        if !parent {
            return false;
        }
        binding.node.parent().is_some_and(|node| {
            // The source dereferences a missing attribute descriptor for an
            // element-only name. Check the optional descriptor before use.
            attribute.is_some_and(|attribute| node.has_local_attribute(attribute.attribute))
                || node.get_or_create_property(index, element).is_some()
        })
    });
    Ok(specified.into())
}
fn save_xml(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() > 1 {
        return Err(argument_error("saveXML"));
    }
    if let Some(option) = args.first() {
        if to_wide(option, context)? != [112, 114, 101, 116, 116, 121] {
            return Err(binding_error("Node", "saveXML", JsMessage::ValueError));
        }
    }
    validate(&binding)?;
    let bytes = CfxJsEngine::with_host_callback(context, || save_xml_model(&binding.node))?;
    Ok(from_wide(&utf8_decode(&bytes)))
}
fn save_xml_model(node: &CxfaNode) -> JsResult<Vec<u8>> {
    if !matches!(
        node.get_packet_type(),
        XfaPacketType::Form | XfaPacketType::Datasets
    ) {
        return Ok(Vec::new());
    }
    let header = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
    let mut output = CfxMemoryStream::new();
    if !output.write_string(header) {
        return Err(model_error("Cannot write the XFA XML header."));
    }
    if node.get_packet_type() == XfaPacketType::Form {
        if !regenerate_form_file(node, &mut output, true) {
            return Err(model_error("Cannot serialize the XFA form node."));
        }
    } else if let Some(xml) = node
        .get_xml_mapping_node()
        .filter(|xml| xml.node_type() == XmlNodeType::Element)
    {
        if !deal_with_data_group_node(Some(node)) {
            return Err(model_error("Cannot serialize an unmapped XFA data group."));
        }
        xml.save(&mut output);
    }
    Ok(output.get_span().to_vec())
}
fn load_xml(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.is_empty() || args.len() > 3 {
        return Err(argument_error("loadXML"));
    }
    let expression = utf8_encode(&to_wide(&args[0], context)?);
    if expression.is_empty() {
        return Ok(JsValue::undefined());
    }
    let ignore_root = args.len() < 2 || args[1].to_boolean();
    let overwrite = args.len() >= 3 && args[2].to_boolean();
    validate(&binding)?;
    CfxJsEngine::with_host_callback(context, || {
        load_xml_model(&binding.node, &expression, ignore_root, overwrite)
    })?;
    Ok(JsValue::undefined())
}
// Parsed content is not initialized by NormalLoader. Initialize every installed
// descendant so subsequent content edits reach OnChanged, including edits to
// the underlying Sharptext node. Keep all arena borrows outside notifications.
fn initialize_loaded_subtree(root: &CxfaNode, parent: &CxfaNode) -> JsResult<()> {
    let mut pending = vec![(root.clone(), false)];
    while let Some((node, visited)) = pending.pop() {
        if !root.is_alive() || root.parent().as_ref() != Some(parent) {
            return Err(model_error(
                "Loaded XFA content was detached during initialization.",
            ));
        }
        if !node.is_alive() || (node != *root && !root.is_ancestor_of(&node)) {
            continue;
        }
        if visited {
            node.set_initialized_flag_and_notify();
        } else {
            pending.push((node.clone(), true));
            let mut child = node.last_child();
            while let Some(current) = child {
                child = current.prev_sibling();
                pending.push((current, false));
            }
        }
    }
    if root.parent().as_ref() != Some(parent) {
        return Err(model_error(
            "Loaded XFA content was detached during initialization.",
        ));
    }
    Ok(())
}
fn load_xml_model(
    node: &CxfaNode,
    expression: &[u8],
    mut ignore_root: bool,
    overwrite: bool,
) -> JsResult<()> {
    let document = node
        .document()
        .ok_or_else(|| model_error("The XFA document is no longer available."))?;
    let _store = node.retain_store();
    let stream = Rc::new(CfxReadOnlySpanStream::new(expression));
    let Some(parsed) = CfxXmlParser::new(stream).parse() else {
        return Ok(());
    };
    let incoming = Rc::new(RefCell::new(parsed));
    let top = document
        .xml_document()
        .unwrap_or_else(|| Rc::new(RefCell::new(CfxXmlDocument::new())));
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    let root = builder.build(incoming.clone());
    // Build installs its XML owner; loadXML keeps the original document owner.
    document.set_xml_document(top.clone());
    let Some(root) = root else {
        return Ok(());
    };
    top.borrow_mut()
        .append_nodes_from(&mut incoming.borrow_mut());
    if ignore_root && (root.node_type() != XmlNodeType::Element || recognize_rich_text(Some(&root)))
    {
        ignore_root = false;
    }
    let fake = node
        .clone_node(false)
        .ok_or_else(|| model_error("Cannot clone the XFA node for loadXML."))?;
    let content_type = node.get_cdata(XfaAttribute::ContentType);
    if !content_type.is_empty() {
        fake.set_cdata(XfaAttribute::ContentType, &content_type);
    }
    let fake_xml = if let Some(xml) = fake.get_xml_mapping_node() {
        xml
    } else if let Some(xml) = node.get_xml_mapping_node() {
        xml.clone_into(&mut top.borrow_mut())
    } else {
        top.borrow_mut().create_element(
            &node
                .get_class_name()
                .bytes()
                .map(u32::from)
                .collect::<Vec<_>>(),
        )
    };
    // Clone(false) still deep-clones its XML mapping. Retaining those old XML
    // children would duplicate them on append and preserve them on overwrite.
    let mut child = fake_xml.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        current.remove_self_if_parented();
    }
    if ignore_root {
        let mut child = root.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            current.remove_self_if_parented();
            fake_xml.append_last_child(&current);
        }
    } else {
        root.remove_self_if_parented();
        fake_xml.append_last_child(&root);
    }
    builder.construct_xfa_node(&fake, &fake_xml);
    let Some(fake) = builder.get_root_node() else {
        return Ok(());
    };
    // Construction can allocate more XML nodes through the builder's owner.
    top.borrow_mut()
        .append_nodes_from(&mut incoming.borrow_mut());
    let mut retained_fake_xml = Some(fake_xml.clone());
    if overwrite {
        let old = node.first_child();
        let mut child = fake.first_child();
        let mut index = 0;
        while let Some(current) = child {
            child = current.next_sibling();
            if !fake.remove_child_and_notify(&current, true)
                || !node.insert_child_and_notify(index, &current)
            {
                return Err(model_error("Cannot install the loaded XFA XML child."));
            }
            index += 1;
            initialize_loaded_subtree(&current, node)?;
        }
        let mut child = old;
        while let Some(current) = child {
            child = current.next_sibling();
            if !node.remove_child_and_notify(&current, true)
                || !fake.insert_child_and_notify(-1, &current)
            {
                return Err(model_error("Cannot detach the replaced XFA XML child."));
            }
        }
        if node.get_packet_type() == XfaPacketType::Form
            && node.get_element_type() == XfaElement::ExData
        {
            let previous = node.get_xml_mapping_node();
            node.set_xml_mapping_node(fake_xml.clone());
            retained_fake_xml = previous.filter(|xml| xml.parent().is_none());
            fake.clear_xml_mapping_node();
        }
        if !fake.sync_buffer_map_data(node) {
            return Err(model_error("Cannot synchronize loaded XFA content."));
        }
    } else {
        let mut child = fake.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            if !fake.remove_child_and_notify(&current, true)
                || !node.insert_child_and_notify(-1, &current)
            {
                return Err(model_error("Cannot append the loaded XFA XML child."));
            }
            initialize_loaded_subtree(&current, node)?;
        }
    }
    if let Some(xml) = retained_fake_xml {
        fake.set_xml_mapping_node(xml);
    }
    fake.set_flag(XfaNodeFlag::HasRemovedChildren);
    Ok(())
}
fn get_ns(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    Ok(from_wide(&binding.node.try_namespace().unwrap_or_default()))
}
fn get_model(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    let model = binding.node.get_model_node();
    box_node(&binding, model, context)
}
fn get_is_container(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    Ok(binding.node.is_container_node().into())
}
fn get_is_null(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if binding.node.get_element_type() == XfaElement::Subform {
        return Ok(false.into());
    }
    let empty = CfxJsEngine::with_host_callback(context, || binding.node.get_content().is_empty());
    Ok(empty.into())
}
fn get_one_of_child(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    let child = CfxJsEngine::with_host_callback(context, || {
        binding
            .node
            .get_node_list_with_filter(XFA_NODE_FILTER_ONE_OF_PROPERTY)
            .into_iter()
            .next()
    });
    match child {
        Some(node) => box_node(&binding, Some(node), context),
        None => Ok(JsValue::undefined()),
    }
}
fn set_readonly(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let _ = node_data(this, context)?;
    Err(model_error("Invalid property set operation."))
}

/// Native event applicability and routing through an optional real FF host.
pub(crate) fn exec_single_event_by_name(
    runtime: &super::cfxjse_engine::CfxJseEngine,
    node: &CxfaNode,
    name: &[u32],
    element: XfaElement,
    context: &mut Context,
) -> JsResult<XfaEventError> {
    use crate::xfa::fxfa::cxfa_eventparam::XfaEventType as Event;
    let Some(host) = runtime.widget_script_host() else {
        return Ok(XfaEventError::kNotExist);
    };
    let matches = |candidate: &str| name.iter().copied().eq(candidate.chars().map(u32::from));
    let (event, recursive) = if matches("initialize") {
        (Event::Initialize, true)
    } else if matches("validate") {
        (Event::Validate, true)
    } else if matches("calculate") {
        (Event::Calculate, true)
    } else if matches("enter") {
        (Event::Enter, false)
    } else if matches("exit") {
        (Event::Exit, false)
    } else if matches("indexChange") && element == XfaElement::Subform {
        (Event::IndexChange, false)
    } else if ["change", "full", "click"].iter().any(|name| matches(name))
        && [XfaElement::Field, XfaElement::ExclGroup].contains(&element)
    {
        (
            if matches("change") {
                Event::Change
            } else if matches("full") {
                Event::Full
            } else {
                Event::Click
            },
            false,
        )
    } else if ["mouseEnter", "mouseExit", "mouseDown", "mouseUp"]
        .iter()
        .any(|name| matches(name))
        && element == XfaElement::Field
    {
        (
            if matches("mouseEnter") {
                Event::MouseEnter
            } else if matches("mouseExit") {
                Event::MouseExit
            } else if matches("mouseDown") {
                Event::MouseDown
            } else {
                Event::MouseUp
            },
            false,
        )
    } else if ["preSign", "postSign", "preOpen", "postOpen"]
        .iter()
        .any(|name| matches(name))
    {
        if !node.is_widget_ready() {
            return Ok(XfaEventError::kNotExist);
        }
        let expected = if matches("preSign") || matches("postSign") {
            XfaElement::Signature
        } else {
            XfaElement::ChoiceList
        };
        if !CfxJsEngine::with_host_callback(context, || node.get_ui_child_node())
            .is_some_and(|ui| ui.get_element_type() == expected)
        {
            return Ok(XfaEventError::kNotExist);
        }
        (
            if matches("preSign") {
                Event::PreSign
            } else if matches("postSign") {
                Event::PostSign
            } else if matches("preOpen") {
                Event::PreOpen
            } else {
                Event::PostOpen
            },
            false,
        )
    } else {
        return Ok(XfaEventError::kNotExist);
    };
    // Dispatch a field's event once, including fields inside exclusion groups.
    // The native duplicate call there has no distinct target or purpose.
    CfxJsEngine::with_host_callback(context, || {
        host.exec_deep_first(node, event, false, recursive)
    })
}

#[cfg(test)]
#[path = "cjx_node_tests.rs"]
mod tests;
