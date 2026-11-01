// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Shared lifetime/receiver mechanics for native Object-derived pseudo-models.
//! No synthetic parser node or strong document/runtime cycle is allocated.
use super::cfxjse_engine::CfxJseEngine;
use boa_engine::{
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::PropertyDescriptor,
    Context, JsData, JsNativeError, JsObject, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, Trace};
use std::rc::{Rc, Weak};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    DataWindow,
    Layout,
    Log,
    Signature,
}
impl Kind {
    fn class_name(self) -> &'static str {
        match self {
            Self::DataWindow => "dataWindow",
            Self::Layout => "layoutPseudoModel",
            Self::Log => "logPseudoModel",
            Self::Signature => "signaturePseudoModel",
        }
    }
}
#[derive(Trace, Finalize, JsData)]
struct Binding {
    #[unsafe_ignore_trace]
    runtime: Weak<CfxJseEngine>,
    #[unsafe_ignore_trace]
    kind: Kind,
}
/// Inspect the actual native class while installing its methods. This does not
/// create a parser node or infer a pseudo-model from a JavaScript property.
pub(crate) fn binding_kind(object: &JsObject) -> Option<Kind> {
    object.downcast_ref::<Binding>().map(|data| data.kind)
}
pub(crate) fn validate(this: &JsValue, kind: Kind, inherited: bool) -> JsResult<Rc<CfxJseEngine>> {
    let mut object = this.as_object();
    while let Some(candidate) = object {
        // Clone only the weak owner link, releasing the native object borrow
        // before upgrading or invoking any other runtime operation.
        let link = candidate
            .downcast_ref::<Binding>()
            .and_then(|data| (data.kind == kind).then(|| data.runtime.clone()));
        if let Some(link) = link {
            return link.upgrade().ok_or_else(|| {
                JsNativeError::reference()
                    .with_message("XFA script context has expired")
                    .into()
            });
        }
        if !inherited {
            break;
        }
        object = candidate.prototype();
    }
    Err(JsNativeError::typ()
        .with_message("Invalid XFA pseudo-model receiver")
        .into())
}
pub(crate) fn object(
    runtime: &Rc<CfxJseEngine>,
    kind: Kind,
    context: &mut Context,
) -> JsResult<JsObject> {
    let object = ObjectInitializer::with_native_data(
        Binding {
            runtime: Rc::downgrade(runtime),
            kind,
        },
        context,
    )
    .build();
    let get = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure(move |this, _, _| {
            validate(this, kind, true)?;
            Ok(JsString::from(kind.class_name()).into())
        }),
    )
    .name(JsString::from("get className"))
    .constructor(false)
    .build();
    let set = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure(move |this, _, _| {
            validate(this, kind, true)?;
            Err(JsNativeError::typ()
                .with_message("Invalid property set operation.")
                .into())
        }),
    )
    .name(JsString::from("set className"))
    .length(1)
    .constructor(false)
    .build();
    object.define_property_or_throw(
        JsString::from("className"),
        PropertyDescriptor::builder()
            .get(get)
            .set(set)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    Ok(object)
}
/// Empty native property callbacks ignore setters without inspecting/coercing
/// their argument; an accessor is essential so assignment cannot replace them.
pub(crate) fn empty_property(
    object: &JsObject,
    kind: Kind,
    name: &str,
    context: &mut Context,
) -> JsResult<()> {
    let callback = NativeFunction::from_copy_closure(move |this, _, _| {
        validate(this, kind, true)?;
        Ok(JsValue::undefined())
    });
    let get = FunctionObjectBuilder::new(context.realm(), callback.clone())
        .name(JsString::from(format!("get {name}")))
        .constructor(false)
        .build();
    let set = FunctionObjectBuilder::new(context.realm(), callback)
        .name(JsString::from(format!("set {name}")))
        .length(1)
        .constructor(false)
        .build();
    object.define_property_or_throw(
        JsString::from(name),
        PropertyDescriptor::builder()
            .get(get)
            .set(set)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    Ok(())
}
