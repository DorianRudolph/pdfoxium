// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_template.{h,cpp}`. Native readiness calls use the node's actual flag;
//! formNodes/recalculate return true after arity checks without reading inputs.
use super::{
    cfxjse_engine::node_data,
    cjx_form::remerge,
    cjx_subform::{define, parameter_error},
};
use boa_engine::{Context, JsObject, JsResult, JsValue};
pub(crate) fn define_template_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    define(
        object,
        context,
        &[
            ("execCalculate", 0, exec_calculate),
            ("execInitialize", 0, exec_initialize),
            ("execValidate", 0, exec_validate),
            ("formNodes", 1, form_nodes),
            ("recalculate", 1, recalculate),
            ("remerge", 0, remerge),
        ],
    )
}
fn ready(this: &JsValue, args: &[JsValue], context: &mut Context, name: &str) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if !args.is_empty() {
        return Err(parameter_error("Template", name));
    }
    Ok(binding.node.is_widget_ready().into())
}
fn exec_calculate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    ready(this, args, context, "execCalculate")
}
fn exec_initialize(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    ready(this, args, context, "execInitialize")
}
fn exec_validate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    ready(this, args, context, "execValidate")
}
fn one_argument(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    name: &str,
) -> JsResult<JsValue> {
    node_data(this, context)?;
    if args.len() != 1 {
        return Err(parameter_error("Template", name));
    }
    Ok(true.into())
}
fn form_nodes(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    one_argument(this, args, context, "formNodes")
}
fn recalculate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    one_argument(this, args, context, "recalculate")
}
