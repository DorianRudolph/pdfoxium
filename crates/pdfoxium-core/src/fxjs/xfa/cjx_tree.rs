// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Executable CJX_Tree methods and properties backed by the real SOM resolver.
use super::{
    cfxjse_engine::{node_data, CfxJseEngine},
    cfxjse_resolveprocessor::{ResolveError, ResolveResultType, XfaResolveFlags},
    cjx_list::new_list_binding,
};
use crate::{
    fxcrt::fx_string::utf16_encode,
    fxjs::{
        cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16, cjs_field::binding_error,
        js_resources::JsMessage,
    },
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaElement, XfaScriptType},
        parser::{
            cxfa_arraynodelist::CxfaArrayNodeList, cxfa_attachnodelist::CxfaAttachNodeList,
            cxfa_list::CxfaList, cxfa_node::CxfaNode,
        },
    },
};
use boa_engine::{
    native_function::NativeFunctionPointer, object::FunctionObjectBuilder,
    property::PropertyDescriptor, Context, JsError, JsObject, JsResult, JsString, JsValue,
    NativeFunction,
};
use std::rc::Rc;

pub fn define_tree_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    for (name, callback) in [
        ("resolveNode", resolve_node as NativeFunctionPointer),
        ("resolveNodes", resolve_nodes),
    ] {
        let function = FunctionObjectBuilder::new(
            context.realm(),
            super::cjx_bindings::native_method(callback, super::cjx_bindings::MethodClass::Tree),
        )
        .name(JsString::from(name))
        .length(1)
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
    for (name, get) in [
        ("all", get_all as NativeFunctionPointer),
        ("classAll", get_class_all),
        ("nodes", get_nodes),
        ("parent", get_parent),
        ("index", get_index),
        ("classIndex", get_class_index),
        ("somExpression", get_som_expression),
    ] {
        let getter = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(get))
            .name(JsString::from(format!("get {name}")))
            .length(0)
            .constructor(false)
            .build();
        let setter_callback = if name == "nodes" {
            set_nodes
        } else {
            set_read_only
        };
        let setter = FunctionObjectBuilder::new(
            context.realm(),
            NativeFunction::from_fn_ptr(setter_callback),
        )
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
fn flags() -> XfaResolveFlags {
    XfaResolveFlags::CHILDREN
        | XfaResolveFlags::ATTRIBUTES
        | XfaResolveFlags::PROPERTIES
        | XfaResolveFlags::PARENT
        | XfaResolveFlags::SIBLINGS
}
fn error(error: ResolveError) -> JsError {
    match error {
        ResolveError::Script(error) => error,
        other => JsError::from_opaque(JsString::from(other.to_string()).into()),
    }
}
fn reference(runtime: &CfxJseEngine, node: &CxfaNode) -> Option<CxfaNode> {
    if node.get_element_type() == XfaElement::Xfa {
        runtime.current_this_node()
    } else {
        Some(node.clone())
    }
}
pub fn resolve_node(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(binding_error("Tree", "resolveNode", JsMessage::ParamError));
    }
    let expression = wide_from_utf16(&args[0].to_string(context)?.to_vec());
    let data = node_data(this, context)?;
    let runtime = data.runtime()?;
    let reference = reference(&runtime, &data.node);
    let result = CfxJsEngine::with_host_callback(context, || {
        runtime.resolve_object_values(reference.as_ref(), &expression, flags())
    })
    .map_err(error)?;
    let Some(result) = result else {
        return Ok(JsValue::null());
    };
    let Some(node) = result.objects.first() else {
        return Ok(JsValue::null());
    };
    if result.result_type == ResolveResultType::Nodes {
        return runtime
            .get_or_create_object_binding(node, context)
            .map(JsValue::from);
    }
    if let Some(attribute) = result
        .script_attribute
        .filter(|a| a.script_type == XfaScriptType::Object)
    {
        runtime.read_resolved_attribute(node, attribute, context)
    } else {
        Ok(JsValue::null())
    }
}
pub fn resolve_nodes(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(binding_error("Tree", "resolveNodes", JsMessage::ParamError));
    }
    let data = node_data(this, context)?;
    let runtime = data.runtime()?;
    // Source selects the xfa root's effective this-object before string coercion.
    let reference = reference(&runtime, &data.node);
    let expression = wide_from_utf16(&args[0].to_string(context)?.to_vec());
    resolve_node_list(
        &runtime,
        reference.as_ref().or(Some(&data.node)),
        &expression,
        flags(),
        context,
    )
}
fn resolve_node_list(
    runtime: &Rc<CfxJseEngine>,
    reference: Option<&CxfaNode>,
    expression: &[u32],
    flags: XfaResolveFlags,
    context: &mut Context,
) -> JsResult<JsValue> {
    let list = CxfaArrayNodeList::new(runtime.document());
    runtime
        .document()
        .get_node_owner()
        .persist_list(list.clone());
    let result = CfxJsEngine::with_host_callback(context, || {
        runtime.resolve_object_values(reference, expression, flags)
    })
    .map_err(error)?;
    if let Some(result) = result {
        if result.result_type == ResolveResultType::Nodes {
            // CJX_Tree's native NodeList accepts only CXFA_Node results.
            // resolveNode can return a pseudo-model; resolveNodes never puts a
            // pseudo-model into the parser-owned list.
            for object in result.objects {
                if let Some(node) = object.as_node() {
                    list.append(node);
                }
            }
        } else if let Some(attribute) = result
            .script_attribute
            .filter(|a| a.script_type == XfaScriptType::Object)
        {
            for object in result.objects {
                let value = runtime.read_resolved_attribute(&object, attribute, context)?;
                if let Some(node) = runtime.node_from_value(&value) {
                    if node.is_node() {
                        list.append(&node);
                    }
                }
            }
        }
    }
    new_list_binding(runtime, list, context).map(JsValue::from)
}
pub fn get_all(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = node_data(this, context)?;
    let runtime = data.runtime()?;
    let mut expression = data.node.get_cdata(XfaAttribute::Name);
    expression.extend("[*]".chars().map(u32::from));
    resolve_node_list(
        &runtime,
        Some(&data.node),
        &expression,
        XfaResolveFlags::SIBLINGS | XfaResolveFlags::ALL,
        context,
    )
}
pub fn get_class_all(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = node_data(this, context)?;
    let runtime = data.runtime()?;
    let expression = format!("#{}[*]", data.node.get_class_name())
        .chars()
        .map(u32::from)
        .collect::<Vec<_>>();
    resolve_node_list(
        &runtime,
        Some(&data.node),
        &expression,
        XfaResolveFlags::SIBLINGS | XfaResolveFlags::ALL,
        context,
    )
}
pub fn get_nodes(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = node_data(this, context)?;
    let runtime = data.runtime()?;
    let list = CxfaAttachNodeList::new(runtime.document(), data.node.clone())
        .ok_or_else(|| binding_error("Tree", "nodes", JsMessage::BadObjectError))?;
    runtime
        .document()
        .get_node_owner()
        .persist_list(list.clone());
    new_list_binding(&runtime, list, context).map(JsValue::from)
}
pub fn get_parent(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = node_data(this, context)?;
    let runtime = data.runtime()?;
    match data.node.parent() {
        Some(node) => runtime
            .get_or_create_binding(&node, context)
            .map(JsValue::from),
        None => Ok(JsValue::null()),
    }
}
pub fn get_index(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = node_data(this, context)?;
    let _runtime = data.runtime()?;
    Ok(JsValue::from(data.node.get_index_by_name() as f64))
}
pub fn get_class_index(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = node_data(this, context)?;
    let _runtime = data.runtime()?;
    Ok(JsValue::from(data.node.get_index_by_class_name() as f64))
}
pub fn get_som_expression(
    this: &JsValue,
    _: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let data = node_data(this, context)?;
    let _runtime = data.runtime()?;
    Ok(JsString::from(utf16_encode(&data.node.get_name_expression()).as_slice()).into())
}
pub fn set_read_only(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(binding_error("Tree", "", JsMessage::ReadOnlyError))
}
fn set_nodes(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(JsError::from_opaque(
        JsString::from("Unable to set ").into(),
    ))
}
