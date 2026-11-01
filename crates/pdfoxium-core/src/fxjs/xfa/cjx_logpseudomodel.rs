// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_logpseudomodel.{h,cpp}`. The pinned implementation intentionally emits
//! nothing and neither validates nor converts any supplied argument.
use super::{
    cfxjse_engine::CfxJseEngine,
    cjx_pseudomodel::{self, Kind},
    cjx_subform::define,
};
use boa_engine::{Context, JsObject, JsResult, JsValue};
use std::rc::Rc;
pub(crate) fn log_object(runtime: &Rc<CfxJseEngine>, context: &mut Context) -> JsResult<JsObject> {
    let object = cjx_pseudomodel::object(runtime, Kind::Log, context)?;
    define(
        &object,
        context,
        &[
            ("message", 0, noop),
            ("traceEnabled", 0, noop),
            ("traceActivate", 0, noop),
            ("traceDeactivate", 0, noop),
            ("trace", 0, noop),
        ],
    )?;
    crate::fxjs::boa_adapters::finish_class(&object, context)?;
    Ok(object)
}
fn noop(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    cjx_pseudomodel::validate(this, Kind::Log, false)?;
    Ok(JsValue::undefined())
}
