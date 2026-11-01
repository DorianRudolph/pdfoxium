// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_delta.{h,cpp}`. CJX_Delta derives Object, not Tree/Node; its native
//! restore operation only checks arity and returns undefined.
use super::{
    cfxjse_engine::node_data,
    cjx_subform::{define, parameter_error},
};
use boa_engine::{Context, JsObject, JsResult, JsValue};
pub(crate) fn define_delta_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    define(object, context, &[("restore", 0, restore)])
}
fn restore(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    node_data(this, context)?;
    if !args.is_empty() {
        return Err(parameter_error("Delta", "restore"));
    }
    Ok(JsValue::undefined())
}
