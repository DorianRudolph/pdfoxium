// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_datawindow.{h,cpp}`: these four methods and properties are deliberately
//! empty in the pinned source, including argument validation and conversion.
use super::{
    cfxjse_engine::CfxJseEngine,
    cjx_pseudomodel::{self, Kind},
    cjx_subform::define,
};
use boa_engine::{Context, JsObject, JsResult, JsValue};
use std::rc::Rc;
pub(crate) fn datawindow_object(
    runtime: &Rc<CfxJseEngine>,
    context: &mut Context,
) -> JsResult<JsObject> {
    let object = cjx_pseudomodel::object(runtime, Kind::DataWindow, context)?;
    define(
        &object,
        context,
        &[
            ("gotoRecord", 0, noop),
            ("isRecordGroup", 0, noop),
            ("moveCurrentRecord", 0, noop),
            ("record", 0, noop),
        ],
    )?;
    for name in [
        "currentRecordNumber",
        "isDefined",
        "recordsAfter",
        "recordsBefore",
    ] {
        cjx_pseudomodel::empty_property(&object, Kind::DataWindow, name, context)?;
    }
    crate::fxjs::boa_adapters::finish_class(&object, context)?;
    Ok(object)
}
fn noop(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    cjx_pseudomodel::validate(this, Kind::DataWindow, false)?;
    Ok(JsValue::undefined())
}
