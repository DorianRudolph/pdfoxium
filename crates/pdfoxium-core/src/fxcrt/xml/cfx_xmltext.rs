// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxcrt/xml/cfx_xmltext.{h,cpp}`. Text and CDATA share the text interface.
use super::cfx_xmlnode::{XmlNodeData, XmlNodeRef, XmlNodeType};
use super::xml_string::XmlString;
use crate::fxcrt::fx_stream::WriteStream;
use crate::fxcrt::fx_string::{encode_entities, utf8_encode};

impl XmlNodeRef {
    pub fn is_text(&self) -> bool {
        matches!(self.node_type(), XmlNodeType::Text | XmlNodeType::CharData)
    }
    pub(crate) fn with_text<R>(&self, f: impl FnOnce(&[u32]) -> R) -> R {
        self.with(|data| match data {
            XmlNodeData::Text(text) | XmlNodeData::CharData(text) => f(text),
            _ => panic!("node is not XML text"),
        })
    }
    pub fn text(&self) -> XmlString {
        self.with(|data| match data {
            XmlNodeData::Text(text) | XmlNodeData::CharData(text) => text.clone(),
            _ => panic!("node is not XML text"),
        })
    }
    pub fn set_text(&self, value: &[u32]) {
        self.with_mut(|data| match data {
            XmlNodeData::Text(text) | XmlNodeData::CharData(text) => {
                *text = XmlString::from(value);
            }
            _ => panic!("node is not XML text"),
        });
    }
    pub(crate) fn save_text(&self, stream: &mut dyn WriteStream) {
        let encoded = self.with_text(|text| utf8_encode(&encode_entities(text)));
        stream.write_string(&encoded);
    }
}
#[cfg(test)]
#[path = "cfx_xmltext_unittest.rs"]
mod upstream_tests;
