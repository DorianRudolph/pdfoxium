// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_source.{h,cpp}`. All seventeen native database methods intentionally
//! do nothing after a zero-argument check; no database service is fabricated.
use super::cjx_subform::parameter_error;
use boa_engine::{
    object::FunctionObjectBuilder, property::PropertyDescriptor, Context, JsObject, JsResult,
    JsString, JsValue, NativeFunction,
};
pub(crate) fn define_source_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    define_source_method(object, context, "addNew")?;
    define_source_method(object, context, "cancel")?;
    define_source_method(object, context, "cancelBatch")?;
    define_source_method(object, context, "close")?;
    define_source_method(object, context, "delete")?;
    define_source_method(object, context, "first")?;
    define_source_method(object, context, "hasDataChanged")?;
    define_source_method(object, context, "isBOF")?;
    define_source_method(object, context, "isEOF")?;
    define_source_method(object, context, "last")?;
    define_source_method(object, context, "next")?;
    define_source_method(object, context, "open")?;
    define_source_method(object, context, "previous")?;
    define_source_method(object, context, "requery")?;
    define_source_method(object, context, "resync")?;
    define_source_method(object, context, "update")?;
    define_source_method(object, context, "updateBatch")?;
    Ok(())
}
fn define_source_method(
    object: &JsObject,
    context: &mut Context,
    name: &'static str,
) -> JsResult<()> {
    let callback = NativeFunction::from_copy_closure(move |this, args, context| {
        super::cjx_bindings::validate_method_receiver(
            super::cjx_bindings::MethodClass::Element(
                crate::xfa::fxfa::fxfa_basic::XfaElement::Source,
            ),
            this,
            context,
        )?;
        if !args.is_empty() {
            return Err(parameter_error("Source", name));
        }
        Ok(JsValue::undefined())
    });
    let function = FunctionObjectBuilder::new(context.realm(), callback)
        .name(JsString::from(name))
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
    Ok(())
}
