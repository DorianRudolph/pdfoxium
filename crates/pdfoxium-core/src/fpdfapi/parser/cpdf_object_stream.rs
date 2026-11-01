// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/parser/cpdf_object_stream.{h,cpp}`. The decoded bytes are
//! owned by this object; each syntax parser borrows them through an owned
//! read-only stream, avoiding a self-referential stream accessor.

use std::rc::Rc;

use super::cpdf_cross_ref_table::MAX_OBJECT_NUMBER;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_indirect_object_holder::HolderRef;
use super::cpdf_object::ObjectRef;
use super::cpdf_scalar_object::CpdfScalarObject;
use super::cpdf_stream::CpdfStream;
use super::cpdf_stream_acc::CpdfStreamAcc;
use super::cpdf_syntax_parser::CpdfSyntaxParser;
use super::fpdf_parser_utility::validate_dict_type;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectInfo {
    pub obj_num: u32,
    pub obj_offset: u32,
}

pub struct CpdfObjectStream {
    data: Rc<CfxReadOnlyContainerStream<Vec<u8>>>,
    first_object_offset: i32,
    object_info: Vec<ObjectInfo>,
}

impl CpdfObjectStream {
    pub fn create(stream: &CpdfStream) -> Option<Self> {
        let dict_ref = stream.get_dict();
        let dict_borrow = dict_ref.borrow();
        let dict = dict_borrow.as_any().downcast_ref::<CpdfDictionary>()?;
        if !validate_dict_type(Some(dict), b"ObjStm") {
            return None;
        }
        let object_count = Self::integer_for(dict, b"N")?;
        if object_count < 0 || object_count as u32 > MAX_OBJECT_NUMBER {
            return None;
        }
        let first_object_offset = Self::integer_for(dict, b"First")?;
        if first_object_offset < 0 {
            return None;
        }
        drop(dict_borrow);

        let mut stream_acc = CpdfStreamAcc::new(stream);
        stream_acc.load_all_data_filtered();
        let data = Rc::new(CfxReadOnlyContainerStream::new(stream_acc.detach_data()));
        let mut syntax = CpdfSyntaxParser::from_file(data.clone());
        let mut object_info = Vec::new();
        for _ in 0..object_count {
            if syntax.get_pos() >= data.span().len() as i64 {
                break;
            }
            let obj_num = syntax.get_direct_num();
            let obj_offset = syntax.get_direct_num();
            if obj_num != 0 {
                object_info.push(ObjectInfo {
                    obj_num,
                    obj_offset,
                });
            }
        }
        Some(Self {
            data,
            first_object_offset,
            object_info,
        })
    }

    fn integer_for(dict: &CpdfDictionary, key: &[u8]) -> Option<i32> {
        let number = dict.get_number_for(key)?;
        let borrowed = number.borrow();
        let number = borrowed.as_any().downcast_ref::<CpdfScalarObject>()?;
        number.is_integer().then(|| number.get_integer())
    }

    pub fn object_info(&self) -> &[ObjectInfo] {
        &self.object_info
    }

    pub fn parse_object(
        &self,
        holder: Option<&HolderRef>,
        obj_number: u32,
        archive_obj_index: u32,
    ) -> Option<ObjectRef> {
        let info = self
            .object_info
            .get(usize::try_from(archive_obj_index).ok()?)?;
        if info.obj_num != obj_number {
            return None;
        }
        let offset = i64::from(self.first_object_offset).checked_add(i64::from(info.obj_offset))?;
        if offset >= self.data.span().len() as i64 {
            return None;
        }
        let mut syntax = CpdfSyntaxParser::from_file(self.data.clone());
        syntax.set_pos(offset);
        let object = syntax.get_object_body(holder)?;
        object.borrow_mut().set_obj_num(obj_number);
        Some(object)
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfObjectStream;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    fn make_stream(first: i32, n: i32, body: &[u8]) -> CpdfStream {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Type", b"ObjStm");
        dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(n));
        dict.set_new_for(b"First", CpdfScalarObject::number_from_signed(first));
        CpdfStream::from_data_and_dict(body.to_vec(), retain_object(dict))
    }

    #[test]
    fn reads_index_and_rejects_mismatched_object_number() {
        let stream = make_stream(8, 2, b"7 0 8 4 17  (ok)");
        let objects = CpdfObjectStream::create(&stream).unwrap();
        assert_eq!(objects.object_info().len(), 2);
        assert!(objects.parse_object(None, 7, 1).is_none());
        let first = objects.parse_object(None, 7, 0).unwrap();
        assert_eq!(first.borrow().get_integer(), 17);
        assert_eq!(first.borrow().obj_num(), 7);
        let second = objects.parse_object(None, 8, 1).unwrap();
        assert_eq!(second.borrow().get_string(), b"ok");
    }

    #[test]
    fn validates_stream_dictionary_and_offset() {
        let stream = make_stream(100, 1, b"7 0 17");
        let objects = CpdfObjectStream::create(&stream).unwrap();
        assert!(objects.parse_object(None, 7, 0).is_none());
        assert!(CpdfObjectStream::create(&make_stream(0, -1, b"")).is_none());
    }
}

#[cfg(test)]
#[path = "cpdf_object_stream_unittest.rs"]
mod upstream_tests;
