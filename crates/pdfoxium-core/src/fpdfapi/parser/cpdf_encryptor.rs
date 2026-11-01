// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_encryptor.{h,cpp}`. The crypto handler itself is
//! supplied by the still-open `CPDF_CryptoHandler` translation.

/// The `EncryptContent()` operation used by `CPDF_Encryptor`.
pub trait CryptoHandler {
    fn encrypt_content(&self, obj_num: u32, gen_num: u32, src: &[u8]) -> Vec<u8>;
}

pub struct CpdfEncryptor<'a> {
    handler: &'a dyn CryptoHandler,
    obj_num: i32,
}

impl<'a> CpdfEncryptor<'a> {
    pub fn new(handler: &'a dyn CryptoHandler, obj_num: i32) -> Self {
        Self { handler, obj_num }
    }

    pub fn encrypt(&self, src: &[u8]) -> Vec<u8> {
        if src.is_empty() {
            return Vec::new();
        }
        self.handler.encrypt_content(self.obj_num as u32, 0, src)
    }
}
