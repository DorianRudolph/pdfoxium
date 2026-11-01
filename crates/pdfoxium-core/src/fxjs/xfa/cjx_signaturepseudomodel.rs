// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_signaturepseudomodel.{h,cpp}` preserves the native fixed responses;
//! these are source behavior, not a signature implementation or host fallback.
use super::{
    cfxjse_engine::CfxJseEngine,
    cjx_pseudomodel::{self, Kind},
    cjx_subform::{define, parameter_error},
};
use boa_engine::{Context, JsObject, JsResult, JsValue};
use std::rc::Rc;
pub(crate) fn signature_object(
    runtime: &Rc<CfxJseEngine>,
    context: &mut Context,
) -> JsResult<JsObject> {
    let object = cjx_pseudomodel::object(runtime, Kind::Signature, context)?;
    define(
        &object,
        context,
        &[
            ("verify", 1, verify),
            ("sign", 3, sign),
            ("enumerate", 0, enumerate),
            ("clear", 1, clear),
        ],
    )?;
    crate::fxjs::boa_adapters::finish_class(&object, context)?;
    Ok(object)
}
fn check(this: &JsValue, args: &[JsValue], method: &str, min: usize, max: usize) -> JsResult<()> {
    cjx_pseudomodel::validate(this, Kind::Signature, false)?;
    if !(min..=max).contains(&args.len()) {
        return Err(parameter_error("SignaturePseudoModel", method));
    }
    Ok(())
}
fn verify(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    check(this, args, "verify", 1, 4)?;
    Ok(0.into())
}
fn sign(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    check(this, args, "sign", 3, 7)?;
    Ok(false.into())
}
fn enumerate(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    check(this, args, "enumerate", 0, 0)?;
    Ok(JsValue::undefined())
}
fn clear(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    check(this, args, "clear", 1, 2)?;
    Ok(false.into())
}
