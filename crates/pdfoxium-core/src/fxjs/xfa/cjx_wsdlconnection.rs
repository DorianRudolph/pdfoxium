// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_wsdlconnection.{h,cpp}`. Native execute has no network implementation:
//! it accepts zero/one argument without conversion and returns false.
use super::{
    cfxjse_engine::node_data,
    cjx_subform::{define, parameter_error},
};
use boa_engine::{Context, JsObject, JsResult, JsValue};
pub(crate) fn define_wsdlconnection_methods(
    object: &JsObject,
    context: &mut Context,
) -> JsResult<()> {
    define(object, context, &[("execute", 0, execute)])
}
fn execute(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    node_data(this, context)?;
    if args.len() > 1 {
        return Err(parameter_error("WsdlConnection", "execute"));
    }
    Ok(false.into())
}
