// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_result.{h,cpp}`. A successful empty return differs from a successful
//! return of JavaScript undefined. Errors keep PDFium's UTF-16 host messages.
use super::js_resources::{js_get_string_from_id, JsMessage};
use boa_engine::JsValue;

#[derive(Clone, Debug, Default)]
#[must_use]
pub struct CjsResult {
    error: Option<Vec<u16>>,
    value: Option<JsValue>,
}
impl CjsResult {
    pub fn success() -> Self {
        Self::default()
    }
    pub fn success_value(value: JsValue) -> Self {
        Self {
            error: None,
            value: Some(value),
        }
    }
    pub fn failure(error: Vec<u16>) -> Self {
        Self {
            error: Some(error),
            value: None,
        }
    }
    pub fn failure_message(message: JsMessage) -> Self {
        Self::failure(js_get_string_from_id(message))
    }
    pub fn has_error(&self) -> bool {
        self.error.is_some()
    }
    pub fn error(&self) -> Option<&[u16]> {
        self.error.as_deref()
    }
    pub fn has_return(&self) -> bool {
        self.value.is_some()
    }
    pub fn return_value(&self) -> Option<&JsValue> {
        self.value.as_ref()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_undefined_value_and_message_remain_distinct() {
        let empty = CjsResult::success();
        assert!(!empty.has_return() && !empty.has_error());
        let value = CjsResult::success_value(JsValue::undefined());
        assert!(value.has_return() && !value.has_error());
        assert!(value.return_value().unwrap().is_undefined());
        let failure = CjsResult::failure_message(JsMessage::ValueError);
        assert!(!failure.has_return() && failure.has_error());
        assert_eq!(
            failure.error().unwrap(),
            js_get_string_from_id(JsMessage::ValueError)
        );
        let custom = CjsResult::failure(vec![0xd800, 0]);
        assert_eq!(custom.clone().error(), Some([0xd800, 0].as_slice()));
    }
}
