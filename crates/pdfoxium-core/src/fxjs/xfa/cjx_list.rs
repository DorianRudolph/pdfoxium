// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Boa list bindings from CJX_List/CJX_TreeList. Native lists retain their node
//! stores; JS wrappers observe their engine weakly and never retain a Context.
use super::cfxjse_engine::CfxJseEngine;
use crate::{
    fxjs::{
        cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16, cjs_field::binding_error,
        js_resources::JsMessage,
    },
    xfa::fxfa::parser::{
        cxfa_list::CxfaList, cxfa_treelist::CxfaTreeList, xfa_basic_data::element_to_name,
    },
};
use boa_engine::{
    native_function::NativeFunctionPointer,
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::{Attribute, PropertyDescriptor},
    Context, JsData, JsObject, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, Trace};
use std::rc::{Rc, Weak};

#[derive(Clone, Trace, Finalize, JsData)]
struct ListBinding {
    #[unsafe_ignore_trace]
    list: Rc<dyn CxfaList>,
    #[unsafe_ignore_trace]
    runtime: Weak<CfxJseEngine>,
}
impl ListBinding {
    fn runtime(&self, method: &str) -> JsResult<Rc<CfxJseEngine>> {
        self.runtime
            .upgrade()
            .ok_or_else(|| binding_error("TreeList", method, JsMessage::BadObjectError))
    }
}
fn data(this: &JsValue, method: &str) -> JsResult<ListBinding> {
    this.as_object()
        .and_then(|object| {
            object
                .downcast_ref::<ListBinding>()
                .map(|value| value.clone())
        })
        .ok_or_else(|| binding_error("TreeList", method, JsMessage::BadObjectError))
}
pub fn is_list(value: &JsValue) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.is::<ListBinding>())
}

fn property_callback(callback: NativeFunctionPointer) -> NativeFunction {
    NativeFunction::from_copy_closure(move |this, args, context| {
        // Native accessors use the holder, including Object.create(list).
        let mut object = this.as_object();
        while let Some(holder) = object {
            if holder.is::<ListBinding>() {
                return callback(&holder.into(), args, context);
            }
            object = holder.prototype();
        }
        Err(binding_error("TreeList", "", JsMessage::BadObjectError))
    })
}

pub fn new_list_binding(
    runtime: &Rc<CfxJseEngine>,
    list: Rc<dyn CxfaList>,
    context: &mut Context,
) -> JsResult<JsObject> {
    let get = FunctionObjectBuilder::new(context.realm(), property_callback(length))
        .name(JsString::from("get length"))
        .length(0)
        .constructor(false)
        .build();
    let set = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set_length))
        .name(JsString::from("set length"))
        .length(1)
        .constructor(false)
        .build();
    let object = ObjectInitializer::with_native_data(
        ListBinding {
            list,
            runtime: Rc::downgrade(runtime),
        },
        context,
    )
    .accessor(
        JsString::from("length"),
        Some(get),
        Some(set),
        Attribute::all(),
    )
    .build();
    let class_name_get = FunctionObjectBuilder::new(context.realm(), property_callback(class_name))
        .name(JsString::from("get className"))
        .length(0)
        .constructor(false)
        .build();
    let class_name_set =
        FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set_class_name))
            .name(JsString::from("set className"))
            .length(1)
            .constructor(false)
            .build();
    object.define_property_or_throw(
        JsString::from("className"),
        PropertyDescriptor::builder()
            .get(class_name_get)
            .set(class_name_set)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    for (name, count, callback) in [
        ("append", 1, append as NativeFunctionPointer),
        ("insert", 2, insert),
        ("remove", 1, remove),
        ("item", 1, item),
        ("namedItem", 1, named_item),
    ] {
        let function =
            FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(callback))
                .name(JsString::from(name))
                .length(count)
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
    crate::fxjs::boa_adapters::finish_class(&object, context)?;
    Ok(object)
}
fn parameter_count(args: &[JsValue], count: usize, method: &str) -> JsResult<()> {
    if args.len() == count {
        Ok(())
    } else {
        Err(binding_error("TreeList", method, JsMessage::ParamError))
    }
}
fn append(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    parameter_count(args, 1, "append")?;
    let data = data(this, "append")?;
    let runtime = data.runtime("append")?;
    let node = runtime
        .node_from_value(&args[0])
        .ok_or_else(|| binding_error("TreeList", "append", JsMessage::ValueError))?;
    if !CfxJsEngine::with_host_callback(context, || data.list.append(&node)) {
        return Err(binding_error(
            "TreeList",
            "append",
            JsMessage::WouldBeCyclic,
        ));
    }
    Ok(JsValue::undefined())
}
fn insert(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    parameter_count(args, 2, "insert")?;
    let data = data(this, "insert")?;
    let runtime = data.runtime("insert")?;
    let node = runtime
        .node_from_value(&args[0])
        .ok_or_else(|| binding_error("TreeList", "insert", JsMessage::ValueError))?;
    // Native ToNode accepts any non-node second value as a null append anchor.
    let before = runtime.node_from_value(&args[1]);
    if !CfxJsEngine::with_host_callback(context, || data.list.insert(&node, before.as_ref())) {
        return Err(binding_error("TreeList", "insert", JsMessage::ValueError));
    }
    Ok(JsValue::undefined())
}
fn remove(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    parameter_count(args, 1, "remove")?;
    let data = data(this, "remove")?;
    let runtime = data.runtime("remove")?;
    let node = runtime
        .node_from_value(&args[0])
        .ok_or_else(|| binding_error("TreeList", "remove", JsMessage::ValueError))?;
    CfxJsEngine::with_host_callback(context, || data.list.remove(&node));
    Ok(JsValue::undefined())
}
fn item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    parameter_count(args, 1, "item")?;
    let data = data(this, "item")?;
    let runtime = data.runtime("item")?;
    let index = args[0].to_i32(context)?;
    if index < 0 || index as usize >= data.list.get_length() {
        return Err(binding_error(
            "TreeList",
            "item",
            JsMessage::InvalidInputError,
        ));
    }
    match data.list.item(index as usize) {
        Some(node) => runtime
            .get_or_create_binding(&node, context)
            .map(JsValue::from),
        None => Ok(JsValue::null()),
    }
}
fn named_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    parameter_count(args, 1, "namedItem")?;
    let data = data(this, "namedItem")?;
    let runtime = data.runtime("namedItem")?;
    let name = wide_from_utf16(&args[0].to_string(context)?.to_vec());
    match data.list.named_item(&name) {
        Some(node) => runtime
            .get_or_create_binding(&node, context)
            .map(JsValue::from),
        None => Ok(JsValue::undefined()),
    }
}
fn length(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "length")?;
    let _runtime = data.runtime("length")?;
    Ok(JsValue::from(data.list.get_length() as f64))
}
fn set_length(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(binding_error(
        "TreeList",
        "length",
        JsMessage::ReadOnlyError,
    ))
}
fn class_name(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let data = data(this, "className")?;
    let _runtime = data.runtime("className")?;
    Ok(JsString::from(element_to_name(data.list.get_element_type())).into())
}
fn set_class_name(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(binding_error(
        "TreeList",
        "className",
        JsMessage::ReadOnlyError,
    ))
}
