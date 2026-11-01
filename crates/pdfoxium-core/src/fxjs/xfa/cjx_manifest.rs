// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_manifest.{h,cpp}` uses actual node widget readiness, never inferred
//! from the existence of a UI/value model or a script binding.
use super::{
    cfxjse_engine::node_data,
    cjx_subform::{define, parameter_error},
};
use boa_engine::{Context, JsObject, JsResult, JsValue};
pub(crate) fn define_manifest_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    define(object, context, &[("evaluate", 0, evaluate)])
}
fn evaluate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if !args.is_empty() {
        return Err(parameter_error("Manifest", "evaluate"));
    }
    Ok(binding.node.is_widget_ready().into())
}
