// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Scalar portions of `CPDF_Object` and translations of `CPDF_Boolean`,
//! `CPDF_Null`, and `CPDF_Number` from `core/fpdfapi/parser/`.

use crate::fxcrt::fx_number::FxNumber;
use crate::fxcrt::fx_stream::WriteStream;
use crate::fxcrt::stream_write_utils::float_to_decimal;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ObjectType {
    Boolean = 1,
    Number,
    String,
    Name,
    Array,
    Dictionary,
    Stream,
    Null,
    Reference,
}

#[derive(Clone, Copy, Debug)]
enum ScalarKind {
    Boolean(bool),
    Null,
    Number(FxNumber),
}

#[derive(Clone, Copy, Debug)]
pub struct CpdfScalarObject {
    obj_num: u32,
    gen_num: u32,
    kind: ScalarKind,
}

impl CpdfScalarObject {
    pub fn boolean_default() -> Self {
        Self::boolean(false)
    }

    pub fn boolean(value: bool) -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            kind: ScalarKind::Boolean(value),
        }
    }

    pub fn null() -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            kind: ScalarKind::Null,
        }
    }

    pub fn number_from_signed(value: i32) -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            kind: ScalarKind::Number(FxNumber::from_signed(value)),
        }
    }

    pub fn number_default() -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            kind: ScalarKind::Number(FxNumber::default()),
        }
    }

    pub fn number_from_float(value: f32) -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            kind: ScalarKind::Number(FxNumber::from_float(value)),
        }
    }

    pub fn number_from_string(value: &[u8]) -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            kind: ScalarKind::Number(FxNumber::from_string(value)),
        }
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

    pub fn object_type(&self) -> ObjectType {
        match self.kind {
            ScalarKind::Boolean(_) => ObjectType::Boolean,
            ScalarKind::Null => ObjectType::Null,
            ScalarKind::Number(_) => ObjectType::Number,
        }
    }

    pub fn clone_object(&self) -> Self {
        // CPDF_Number::Clone() passes GetSigned() through its integer
        // constructor, even when the original FX_Number was unsigned.
        match self.kind {
            ScalarKind::Boolean(value) => Self::boolean(value),
            ScalarKind::Null => Self::null(),
            ScalarKind::Number(number) if number.is_integer() => {
                Self::number_from_signed(number.get_signed())
            }
            ScalarKind::Number(number) => Self::number_from_float(number.get_float()),
        }
    }

    pub fn get_string(&self) -> Vec<u8> {
        match self.kind {
            ScalarKind::Boolean(true) => b"true".to_vec(),
            ScalarKind::Boolean(false) => b"false".to_vec(),
            ScalarKind::Null => Vec::new(),
            ScalarKind::Number(number) if number.is_integer() => {
                number.get_signed().to_string().into_bytes()
            }
            ScalarKind::Number(number) => float_to_decimal(number.get_float()).into_bytes(),
        }
    }

    pub fn get_unicode_text(&self) -> Vec<u32> {
        Vec::new()
    }

    pub fn get_number(&self) -> f32 {
        match self.kind {
            ScalarKind::Number(number) => number.get_float(),
            _ => 0.0,
        }
    }

    pub fn get_integer(&self) -> i32 {
        match self.kind {
            ScalarKind::Boolean(value) => i32::from(value),
            ScalarKind::Number(number) => number.get_signed(),
            ScalarKind::Null => 0,
        }
    }

    pub fn set_string(&mut self, text: &[u8]) {
        match &mut self.kind {
            ScalarKind::Boolean(value) => *value = text == b"true",
            ScalarKind::Number(number) => *number = FxNumber::from_string(text),
            ScalarKind::Null => unreachable!("CPDF_Null does not override SetString"),
        }
    }

    pub fn as_boolean(&self) -> Option<bool> {
        if let ScalarKind::Boolean(value) = self.kind {
            Some(value)
        } else {
            None
        }
    }

    pub fn as_number(&self) -> Option<FxNumber> {
        if let ScalarKind::Number(value) = self.kind {
            Some(value)
        } else {
            None
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self.kind, ScalarKind::Null)
    }
    pub fn is_integer(&self) -> bool {
        self.as_number().is_some_and(FxNumber::is_integer)
    }

    pub fn write_to(&self, archive: &mut dyn WriteStream) -> bool {
        match self.kind {
            ScalarKind::Null => archive.write_string(b" null"),
            ScalarKind::Boolean(_) | ScalarKind::Number(_) => {
                archive.write_string(b" ") && archive.write_string(&self.get_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CpdfScalarObject, ObjectType};
    use crate::fxcrt::fx_stream::WriteStream;

    #[derive(Default)]
    struct Buffer(Vec<u8>);
    impl WriteStream for Buffer {
        fn write_block(&mut self, bytes: &[u8]) -> bool {
            self.0.extend_from_slice(bytes);
            true
        }
    }

    #[test]
    fn scalar_getters_clone_and_archive_bytes() {
        assert_eq!(
            CpdfScalarObject::boolean_default().as_boolean(),
            Some(false)
        );
        assert_eq!(CpdfScalarObject::number_default().get_string(), b"0");
        let mut boolean = CpdfScalarObject::boolean(false);
        assert_eq!(boolean.object_type(), ObjectType::Boolean);
        assert_eq!(boolean.get_integer(), 0);
        boolean.set_string(b"true");
        assert_eq!(boolean.as_boolean(), Some(true));
        let mut archive = Buffer::default();
        assert!(boolean.write_to(&mut archive));
        assert_eq!(archive.0, b" true");

        let mut number = CpdfScalarObject::number_from_string(b"4294967295");
        number.set_obj_num(12);
        number.set_gen_num(2);
        assert_eq!(number.get_string(), b"-1");
        let clone = number.clone_object();
        assert_eq!(clone.as_number().unwrap().get_signed(), -1);
        assert!(clone.is_inline());
        assert_eq!(clone.gen_num(), 0);
        number.set_string(b".5");
        assert_eq!(number.get_string(), b".5");

        let mut archive = Buffer::default();
        assert!(CpdfScalarObject::null().write_to(&mut archive));
        assert_eq!(archive.0, b" null");
        assert_eq!(CpdfScalarObject::null().get_number(), 0.0);
        assert!(CpdfScalarObject::null().get_unicode_text().is_empty());
    }
}

#[cfg(test)]
#[path = "cpdf_number_unittest.rs"]
mod upstream_tests;
