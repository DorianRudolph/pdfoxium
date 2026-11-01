// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxcrt/xml/cfx_xmlchardata.{h,cpp}`.
use super::cfx_xmlnode::XmlNodeRef;
use crate::fxcrt::fx_stream::WriteStream;
use crate::fxcrt::fx_string::utf8_encode;

impl XmlNodeRef {
    pub(crate) fn save_chardata(&self, stream: &mut dyn WriteStream) {
        stream.write_string(b"<![CDATA[");
        let encoded = self.with_text(utf8_encode);
        stream.write_string(&encoded);
        stream.write_string(b"]]>");
    }
}
#[cfg(test)]
#[path = "cfx_xmlchardata_unittest.rs"]
mod upstream_tests;
