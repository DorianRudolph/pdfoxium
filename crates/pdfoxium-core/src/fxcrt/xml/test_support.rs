// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use crate::fxcrt::fx_stream::WriteStream;

pub fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}

#[derive(Default)]
pub struct StringWriteStream(pub Vec<u8>);
impl StringWriteStream {
    pub fn text(&self) -> String {
        String::from_utf8(self.0.clone()).unwrap()
    }
}
impl WriteStream for StringWriteStream {
    fn write_block(&mut self, data: &[u8]) -> bool {
        self.0.extend_from_slice(data);
        true
    }
}

pub fn assert_panics(action: impl FnOnce()) {
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(action)).is_err());
}
