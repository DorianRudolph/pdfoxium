// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Native named-property queries/deletion on Boa's invariant-checked Proxy.
//! Reflected attributes are real nonconfigurable accessors into the model;
//! they never cache a fabricated/default value or retain the runtime strongly.
use super::*;
use crate::xfa::fxfa::{fxfa_basic::XfaAttribute, parser::xfa_basic_data::attribute_to_name};
use boa_engine::{
    object::FunctionObjectBuilder,
    property::{PropertyDescriptor, PropertyKey},
    NativeFunction,
};
#[derive(Clone)]
pub(super) struct Operations {
    descriptor: JsObject,
    define: JsObject,
    delete: JsObject,
    prevent_extensions: JsObject,
}
impl Operations {
    pub(super) fn get_descriptor(
        &self,
        object: &JsObject,
        name: &JsString,
        context: &mut Context,
    ) -> JsResult<JsValue> {
        self.descriptor.call(
            &JsValue::undefined(),
            &[object.clone().into(), name.clone().into()],
            context,
        )
    }
    pub(super) fn new(context: &mut Context) -> JsResult<Self> {
        let reflect = context.intrinsics().objects().reflect();
        let mut get = |name: &str| -> JsResult<JsObject> {
            reflect
                .get(JsString::from(name), context)?
                .as_callable()
                .ok_or_else(|| {
                    JsNativeError::typ()
                        .with_message("Missing Boa reflection intrinsic")
                        .into()
                })
        };
        Ok(Self {
            descriptor: get("getOwnPropertyDescriptor")?,
            define: get("defineProperty")?,
            delete: get("deleteProperty")?,
            prevent_extensions: get("preventExtensions")?,
        })
    }
}
#[derive(Clone, Trace, Finalize)]
struct AttributeAccess {
    target: JsObject,
    #[unsafe_ignore_trace]
    descriptor: ScriptAttributeDescriptor,
}
fn target_and_key(args: &[JsValue], context: &mut Context) -> JsResult<(JsObject, PropertyKey)> {
    let target = args
        .first()
        .and_then(JsValue::as_object)
        .ok_or_else(|| JsNativeError::typ().with_message("Invalid XFA reflection target"))?;
    let key = property_key(args.get(1).unwrap_or(&JsValue::undefined()), context)?;
    Ok((target, key))
}
fn materialize(target: &JsObject, key: &PropertyKey, context: &mut Context) -> JsResult<bool> {
    if target.has_own_property(key.clone(), context)? {
        return Ok(true);
    }
    let PropertyKey::String(name) = key else {
        return Ok(false);
    };
    let data = node_data(&target.clone().into(), context)?;
    let Some(descriptor) = find_script_attribute_descriptor(
        data.node.get_element_type(),
        &wide_from_utf16(&name.to_vec()),
    ) else {
        return Ok(false);
    };
    let capture = AttributeAccess {
        target: target.clone(),
        descriptor,
    };
    let getter = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |_, _, capture: &AttributeAccess, context| {
                let data = node_data(&capture.target.clone().into(), context)?;
                data.runtime()?
                    .read_script_attribute(&data.node, capture.descriptor, context)
            },
            capture.clone(),
        ),
    )
    .name(JsString::from(format!(
        "get {}",
        name.to_std_string_escaped()
    )))
    .constructor(false)
    .build();
    let setter = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure_with_captures(
            |_, args, capture: &AttributeAccess, context| {
                let data = node_data(&capture.target.clone().into(), context)?;
                data.runtime()?.write_script_attribute(
                    &data.node,
                    capture.descriptor,
                    args.first().unwrap_or(&JsValue::undefined()),
                    context,
                )?;
                Ok(JsValue::undefined())
            },
            capture,
        ),
    )
    .name(JsString::from(format!(
        "set {}",
        name.to_std_string_escaped()
    )))
    .length(1)
    .constructor(false)
    .build();
    target.define_property_or_throw(
        key.clone(),
        PropertyDescriptor::builder()
            .get(getter)
            .set(setter)
            .enumerable(true)
            .configurable(false),
        context,
    )?;
    node_data(&target.clone().into(), context)?.mark_generated_accessor(name);
    Ok(true)
}
pub(super) fn descriptor(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let (target, key) = target_and_key(args, context)?;
    materialize(&target, &key, context)?;
    let data = node_data(&target.clone().into(), context)?;
    let operation = data.runtime()?.reflection.descriptor.clone();
    let descriptor =
        operation.call(&JsValue::undefined(), &[target.into(), key.into()], context)?;
    if let Some(object) = descriptor.as_object() {
        // The Proxy converts this internal result back with ToPropertyDescriptor.
        // Inherited user fields must not turn a real accessor descriptor into a
        // mixed descriptor or invoke a polluted Object.prototype getter.
        if !object.set_prototype(None) {
            return Err(JsNativeError::typ()
                .with_message("Cannot isolate XFA property descriptor")
                .into());
        }
    }
    Ok(descriptor)
}
pub(super) fn delete(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (target, key) = target_and_key(args, context)?;
    materialize(&target, &key, context)?;
    let data = node_data(&target.clone().into(), context)?;
    let operation = data.runtime()?.reflection.delete.clone();
    operation.call(&JsValue::undefined(), &[target.into(), key.into()], context)
}
pub(super) fn define(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (target, key) = target_and_key(args, context)?;
    materialize(&target, &key, context)?;
    let data = node_data(&target.clone().into(), context)?;
    let operation = data.runtime()?.reflection.define.clone();
    operation.call(
        &JsValue::undefined(),
        &[
            target.into(),
            key.into(),
            args.get(2).cloned().unwrap_or_default(),
        ],
        context,
    )
}
pub(super) fn prevent_extensions(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let target = args
        .first()
        .and_then(JsValue::as_object)
        .ok_or_else(|| JsNativeError::typ().with_message("Invalid XFA reflection target"))?;
    let data = node_data(&target.clone().into(), context)?;
    // All metadata-declared attributes must exist before the target becomes
    // nonextensible; otherwise a later descriptor would violate Proxy invariants.
    for &attribute in XfaAttribute::ALL {
        let name = attribute_to_name(attribute);
        if !name.is_empty() {
            materialize(&target, &PropertyKey::from(JsString::from(name)), context)?;
        }
    }
    let operation = data.runtime()?.reflection.prevent_extensions.clone();
    operation.call(&JsValue::undefined(), &[target.into()], context)
}
#[cfg(test)]
#[path = "cfxjse_engine_reflection_tests.rs"]
mod tests;
