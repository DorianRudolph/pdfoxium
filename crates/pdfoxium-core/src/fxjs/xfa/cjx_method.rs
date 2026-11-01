// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `jse_define.h` JSE_METHOD's declaring-class DynamicTypeIs check. Methods
//! retain ordinary Boa callable identity and do not wrap their receiver object.
use super::{is_container_class, is_model_class, is_node_class};
use crate::{
    fxjs::{cjs_field::binding_error, js_resources::JsMessage, xfa::cfxjse_engine::node_data},
    xfa::fxfa::fxfa_basic::XfaElement,
};
use boa_engine::{
    native_function::NativeFunctionPointer, Context, JsResult, JsValue, NativeFunction,
};
#[derive(Clone, Copy)]
pub(crate) enum Class {
    Tree,
    Node,
    Container,
    Model,
    Element(XfaElement),
    Pseudo(crate::fxjs::xfa::cjx_pseudomodel::Kind),
}
impl Class {
    fn accepts(self, actual: XfaElement) -> bool {
        match self {
            Self::Tree | Self::Node => is_node_class(actual),
            Self::Container => is_container_class(actual),
            Self::Model => is_model_class(actual),
            Self::Element(element) => actual == element,
            Self::Pseudo(_) => false,
        }
    }
}
pub(crate) fn validate(class: Class, this: &JsValue, context: &Context) -> JsResult<()> {
    if let Class::Pseudo(kind) = class {
        return crate::fxjs::xfa::cjx_pseudomodel::validate(this, kind, false).map(|_| ());
    }
    let data = node_data(this, context)?;
    if !class.accepts(data.node.get_element_type()) {
        return Err(binding_error("XFA", "method", JsMessage::BadObjectError));
    }
    Ok(())
}
pub(crate) fn native_method(callback: NativeFunctionPointer, class: Class) -> NativeFunction {
    NativeFunction::from_copy_closure(move |this, args, context| {
        validate(class, this, context)?;
        callback(this, args, context)
    })
}
