// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_name.{h,cpp}`.

use std::rc::Rc;

use super::cpdf_encryptor::CpdfEncryptor;
use super::cpdf_scalar_object::ObjectType;
use super::fpdf_parser_decode::pdf_decode_text;
use super::fpdf_parser_utility::pdf_name_encode;
use crate::fxcrt::bytestring_pool::ByteStringPool;
use crate::fxcrt::fx_stream::WriteStream;

#[derive(Debug)]
pub struct CpdfName {
    obj_num: u32,
    gen_num: u32,
    name: Rc<[u8]>,
}

impl CpdfName {
    pub fn new(pool: Option<&mut ByteStringPool>, name: &[u8]) -> Self {
        let name = pool.map_or_else(|| Rc::from(name), |pool| pool.intern(name));
        Self {
            obj_num: 0,
            gen_num: 0,
            name,
        }
    }

    pub fn object_type(&self) -> ObjectType {
        ObjectType::Name
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
            name: Rc::clone(&self.name),
        }
    }

    pub fn get_string(&self) -> &[u8] {
        &self.name
    }
    pub fn get_unicode_text(&self) -> Vec<u32> {
        pdf_decode_text(&self.name)
    }
    pub fn set_string(&mut self, name: &[u8]) {
        self.name = Rc::from(name);
    }

    pub fn write_to(
        &self,
        archive: &mut dyn WriteStream,
        _encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        if !archive.write_string(b"/") {
            return false;
        }
        let name = pdf_name_encode(&self.name);
        name.is_empty() || archive.write_string(&name)
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfName;
    use crate::fxcrt::bytestring_pool::ByteStringPool;
    use crate::fxcrt::fx_stream::WriteStream;
    use std::rc::Rc;

    struct Buffer(Vec<u8>);
    impl WriteStream for Buffer {
        fn write_block(&mut self, bytes: &[u8]) -> bool {
            self.0.extend_from_slice(bytes);
            true
        }
    }

    #[test]
    fn pooled_names_share_bytes_and_write_pdf_escapes() {
        let mut pool = ByteStringPool::new();
        let mut first = CpdfName::new(Some(&mut pool), b"A B#");
        let second = CpdfName::new(Some(&mut pool), b"A B#");
        assert!(Rc::ptr_eq(&first.name, &second.name));
        first.set_obj_num(7);
        let clone = first.clone_object();
        assert!(clone.is_inline());
        assert_eq!(clone.get_string(), b"A B#");
        let mut buffer = Buffer(Vec::new());
        assert!(first.write_to(&mut buffer, None));
        assert_eq!(buffer.0, b"/A#20B#23");
        first.set_string(b"Other");
        assert_eq!(second.get_string(), b"A B#");
    }
}
