// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_reference.{h,cpp}`. The holder is weak because
//! PDFium's source reference keeps an unowned pointer to its document.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::{Rc, Weak};

use super::cpdf_encryptor::CpdfEncryptor;
use super::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use super::cpdf_object::{clone_non_cyclic, get_dict_object, retain_object, ObjectRef, PdfObject};
use super::cpdf_scalar_object::ObjectType;
use crate::fxcrt::fx_stream::WriteStream;

pub struct CpdfReference {
    obj_num: u32,
    gen_num: u32,
    holder: Weak<RefCell<CpdfIndirectObjectHolder>>,
    ref_obj_num: u32,
}

impl CpdfReference {
    pub fn new(holder: &HolderRef, ref_obj_num: u32) -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            holder: Rc::downgrade(holder),
            ref_obj_num,
        }
    }

    pub fn without_holder(ref_obj_num: u32) -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            holder: Weak::new(),
            ref_obj_num,
        }
    }

    pub fn ref_obj_num(&self) -> u32 {
        self.ref_obj_num
    }
    pub(crate) fn indirect_object_holder(&self) -> Option<HolderRef> {
        self.holder.upgrade()
    }

    pub fn has_indirect_object_holder(&self) -> bool {
        self.holder.upgrade().is_some()
    }

    pub fn set_ref(&mut self, holder: &HolderRef, ref_obj_num: u32) {
        self.holder = Rc::downgrade(holder);
        self.ref_obj_num = ref_obj_num;
    }

    pub fn get_direct(&self) -> Option<ObjectRef> {
        CpdfIndirectObjectHolder::get_or_parse_indirect_object(
            &self.holder.upgrade()?,
            self.ref_obj_num,
        )
    }

    pub fn get_dict(&self) -> Option<ObjectRef> {
        get_dict_object(&self.fast_get_direct()?)
    }

    fn fast_get_direct(&self) -> Option<ObjectRef> {
        let object = self.get_direct()?;
        let is_reference = object.borrow().object_type() == ObjectType::Reference;
        (!is_reference).then_some(object)
    }

    pub fn make_reference(&self, holder: &HolderRef) -> Self {
        assert!(Weak::ptr_eq(&self.holder, &Rc::downgrade(holder)));
        Self::new(holder, self.ref_obj_num)
    }
}

impl PdfObject for CpdfReference {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn object_type(&self) -> ObjectType {
        ObjectType::Reference
    }
    fn obj_num(&self) -> u32 {
        self.obj_num
    }
    fn set_obj_num(&mut self, value: u32) {
        self.obj_num = value;
    }
    fn gen_num(&self) -> u32 {
        self.gen_num
    }
    fn set_gen_num(&mut self, value: u32) {
        self.gen_num = value;
    }
    fn clone_object(&self) -> ObjectRef {
        retain_object(Self {
            obj_num: 0,
            gen_num: 0,
            holder: self.holder.clone(),
            ref_obj_num: self.ref_obj_num,
        })
    }
    fn clone_non_cyclic(&self, direct: bool, visited: &HashSet<usize>) -> Option<ObjectRef> {
        if !direct {
            return Some(self.clone_object());
        }
        clone_non_cyclic(&self.get_direct()?, true, visited)
    }
    fn get_string(&self) -> Vec<u8> {
        self.fast_get_direct()
            .map_or_else(Vec::new, |o| o.borrow().get_string())
    }
    fn get_number(&self) -> f32 {
        self.fast_get_direct()
            .map_or(0.0, |o| o.borrow().get_number())
    }
    fn get_integer(&self) -> i32 {
        self.fast_get_direct()
            .map_or(0, |o| o.borrow().get_integer())
    }
    fn write_to(
        &self,
        archive: &mut dyn WriteStream,
        _encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        archive.write_string(b" ")
            && archive.write_dword(self.ref_obj_num)
            && archive.write_string(b" 0 R ")
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfReference;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::{retain_object, PdfObject};
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fxcrt::fx_stream::WriteStream;

    struct Buffer(Vec<u8>);
    impl WriteStream for Buffer {
        fn write_block(&mut self, bytes: &[u8]) -> bool {
            self.0.extend_from_slice(bytes);
            true
        }
    }

    #[test]
    fn reference_forwards_scalar_values_and_writes_its_number() {
        let holder = CpdfIndirectObjectHolder::new();
        let number = retain_object(CpdfScalarObject::number_from_signed(27));
        let number_id = holder.borrow_mut().add_indirect_object(number);
        let reference = CpdfReference::new(&holder, number_id);
        assert_eq!(reference.get_integer(), 27);
        assert_eq!(reference.get_string(), b"27");
        let mut buffer = Buffer(Vec::new());
        assert!(reference.write_to(&mut buffer, None));
        assert_eq!(buffer.0, b" 1 0 R ");
    }
}
