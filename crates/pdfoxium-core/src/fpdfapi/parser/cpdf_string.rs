// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_string.{h,cpp}`.

use std::rc::Rc;

use super::cpdf_encryptor::CpdfEncryptor;
use super::cpdf_scalar_object::ObjectType;
use super::fpdf_parser_decode::{
    pdf_decode_text, pdf_encode_string, pdf_encode_text, pdf_hex_encode_string,
};
use crate::fxcrt::bytestring_pool::ByteStringPool;
use crate::fxcrt::fx_stream::WriteStream;

#[derive(Debug)]
pub struct CpdfString {
    obj_num: u32,
    gen_num: u32,
    data: Rc<[u8]>,
    output_is_hex: bool,
}

impl CpdfString {
    pub fn new() -> Self {
        Self::from_bytes(None, b"")
    }

    pub fn from_bytes(pool: Option<&mut ByteStringPool>, data: &[u8]) -> Self {
        let data = pool.map_or_else(|| Rc::from(data), |pool| pool.intern(data));
        Self {
            obj_num: 0,
            gen_num: 0,
            data,
            output_is_hex: false,
        }
    }

    pub fn from_hex_bytes(pool: Option<&mut ByteStringPool>, data: &[u8]) -> Self {
        let mut result = Self::from_bytes(pool, data);
        result.output_is_hex = true;
        result
    }

    pub fn from_unicode(pool: Option<&mut ByteStringPool>, text: &[u32]) -> Self {
        Self::from_bytes(pool, &pdf_encode_text(text))
    }

    pub fn object_type(&self) -> ObjectType {
        ObjectType::String
    }
    pub fn obj_num(&self) -> u32 {
        self.obj_num
    }
    pub fn set_obj_num(&mut self, value: u32) {
        self.obj_num = value;
    }
    pub fn gen_num(&self) -> u32 {
        self.gen_num
    }
    pub fn set_gen_num(&mut self, value: u32) {
        self.gen_num = value;
    }
    pub fn is_inline(&self) -> bool {
        self.obj_num == 0
    }

    pub fn clone_object(&self) -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            data: Rc::clone(&self.data),
            output_is_hex: self.output_is_hex,
        }
    }

    pub fn get_string(&self) -> &[u8] {
        &self.data
    }
    pub fn get_unicode_text(&self) -> Vec<u32> {
        pdf_decode_text(&self.data)
    }
    pub fn set_string(&mut self, data: &[u8]) {
        self.data = Rc::from(data);
    }
    pub fn is_hex(&self) -> bool {
        self.output_is_hex
    }

    pub fn encode_string(&self) -> Vec<u8> {
        if self.output_is_hex {
            pdf_hex_encode_string(&self.data)
        } else {
            pdf_encode_string(&self.data)
        }
    }

    pub fn write_to(
        &self,
        archive: &mut dyn WriteStream,
        encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        let encrypted;
        let data = if let Some(encryptor) = encryptor {
            encrypted = encryptor.encrypt(&self.data);
            encrypted.as_slice()
        } else {
            &self.data
        };
        let content = if self.output_is_hex {
            pdf_hex_encode_string(data)
        } else {
            pdf_encode_string(data)
        };
        archive.write_string(&content)
    }
}

impl Default for CpdfString {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfString;
    use crate::fpdfapi::parser::cpdf_encryptor::{CpdfEncryptor, CryptoHandler};
    use crate::fxcrt::fx_stream::WriteStream;
    use std::cell::Cell;

    struct Buffer(Vec<u8>);
    impl WriteStream for Buffer {
        fn write_block(&mut self, bytes: &[u8]) -> bool {
            self.0.extend_from_slice(bytes);
            true
        }
    }
    struct Handler(Cell<usize>);
    impl CryptoHandler for Handler {
        fn encrypt_content(&self, obj_num: u32, gen_num: u32, src: &[u8]) -> Vec<u8> {
            assert_eq!((obj_num, gen_num), (42, 0));
            self.0.set(self.0.get() + 1);
            src.iter().map(|b| b ^ 0x20).collect()
        }
    }

    #[test]
    fn encrypted_hex_output_and_empty_shortcut() {
        let handler = Handler(Cell::new(0));
        let encryptor = CpdfEncryptor::new(&handler, 42);
        let string = CpdfString::from_hex_bytes(None, b"ab");
        assert_eq!(string.encode_string(), b"<6162>");
        let mut buffer = Buffer(Vec::new());
        assert!(string.write_to(&mut buffer, Some(&encryptor)));
        assert_eq!(buffer.0, b"<4142>");
        assert_eq!(handler.0.get(), 1);
        let empty = CpdfString::new();
        assert!(empty.write_to(&mut buffer, Some(&encryptor)));
        assert_eq!(handler.0.get(), 1);
        assert_eq!(buffer.0, b"<4142>()");
    }
}
