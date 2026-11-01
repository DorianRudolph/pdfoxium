// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_annot.{h,cpp}`. Dynamic annotation objects observe SDK annotations
//! weakly and reject logical invalidation even if another Rust owner retains
//! the annotation. Coercion ends before observing the target again.
use super::cjs_event_context::wide_from_utf16;
use super::cjs_field::binding_error;
use super::js_resources::JsMessage;
use crate::fpdfsdk::cpdfsdk_annot::CpdfSdkAnnot;
use crate::fxcrt::fx_string::utf16_encode;
use boa_engine::{
    native_function::NativeFunctionPointer,
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::Attribute,
    Context, JsData, JsObject, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, Trace};
use std::rc::{Rc, Weak};

#[derive(Trace, Finalize, JsData)]
struct AnnotData {
    #[unsafe_ignore_trace]
    annotation: Weak<CpdfSdkAnnot>,
}
// JSPropGetter/Setter inspect the native property holder, including inherited
// accessors. Captures are Copy function pointers and contain no hidden GC roots.
fn property_callback(callback: NativeFunctionPointer) -> NativeFunction {
    NativeFunction::from_copy_closure(move |this, args, context| {
        let mut object = this.as_object();
        while let Some(candidate) = object {
            if candidate.is::<AnnotData>() {
                return callback(&candidate.into(), args, context);
            }
            object = candidate.prototype();
        }
        Ok(JsValue::undefined())
    })
}

pub fn annot_object(context: &mut Context, annotation: &Rc<CpdfSdkAnnot>) -> JsObject {
    let properties: [(&str, NativeFunctionPointer, NativeFunctionPointer); 3] = [
        ("hidden", get_hidden, set_hidden),
        ("name", get_name, set_name),
        ("type", get_type, set_type),
    ];
    let callbacks: Vec<_> = properties
        .into_iter()
        .map(|(name, get, set)| {
            (
                name,
                FunctionObjectBuilder::new(context.realm(), property_callback(get))
                    .name(JsString::from(format!("get {name}")))
                    .length(0)
                    .constructor(false)
                    .build(),
                FunctionObjectBuilder::new(context.realm(), property_callback(set))
                    .name(JsString::from(format!("set {name}")))
                    .length(1)
                    .constructor(false)
                    .build(),
            )
        })
        .collect();
    let mut object = ObjectInitializer::with_native_data(
        AnnotData {
            annotation: Rc::downgrade(annotation),
        },
        context,
    );
    for (name, get, set) in callbacks {
        object.accessor(JsString::from(name), Some(get), Some(set), Attribute::all());
    }
    object.build()
}
fn observe(this: &JsValue, property: &str) -> JsResult<Weak<CpdfSdkAnnot>> {
    let object = this
        .as_object()
        .ok_or_else(|| binding_error("Annot", property, JsMessage::BadObjectError))?;
    let data = object
        .downcast_ref::<AnnotData>()
        .ok_or_else(|| binding_error("Annot", property, JsMessage::BadObjectError))?;
    Ok(data.annotation.clone())
}
fn live(annotation: &Weak<CpdfSdkAnnot>, property: &str) -> JsResult<Rc<CpdfSdkAnnot>> {
    annotation
        .upgrade()
        .filter(|annotation| annotation.is_alive())
        .ok_or_else(|| binding_error("Annot", property, JsMessage::BadObjectError))
}
fn get_hidden(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok((live(&observe(this, "hidden")?, "hidden")?
        .as_ba_annot()
        .flags()
        & 2
        != 0)
        .into())
}
fn set_hidden(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let annotation = observe(this, "hidden")?;
    let hidden = args.first().is_some_and(JsValue::to_boolean);
    let annotation = live(&annotation, "hidden")?;
    let mut flags = annotation.as_ba_annot().flags();
    if hidden {
        flags |= 1 | 2 | 32;
        flags &= !4;
    } else {
        flags &= !(1 | 2 | 32);
        flags |= 4;
    }
    annotation.as_ba_annot().set_flags(flags);
    Ok(JsValue::undefined())
}
fn get_name(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let name = live(&observe(this, "name")?, "name")?
        .as_ba_annot()
        .annot_name();
    Ok(JsString::from(utf16_encode(&name).as_slice()).into())
}
fn set_name(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let annotation = observe(this, "name")?;
    let name = args
        .first()
        .cloned()
        .unwrap_or_else(JsValue::undefined)
        .to_string(context)?
        .to_vec();
    live(&annotation, "name")?
        .as_ba_annot()
        .set_annot_name(&wide_from_utf16(&name));
    Ok(JsValue::undefined())
}
fn get_type(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let subtype = live(&observe(this, "type")?, "type")?.subtype();
    Ok(JsString::from(
        subtype
            .as_pdf()
            .iter()
            .map(|&byte| u16::from(byte))
            .collect::<Vec<_>>()
            .as_slice(),
    )
    .into())
}
fn set_type(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Err(binding_error("Annot", "type", JsMessage::ReadOnlyError))
}
