// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_indirect_object_holder.{h,cpp}`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use super::cpdf_array::CpdfArray;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_name::CpdfName;
use super::cpdf_object::{retain_object, ObjectRef, INVALID_OBJ_NUM};
use super::cpdf_stream::CpdfStream;
use super::cpdf_string::CpdfString;
use crate::fxcrt::bytestring_pool::ByteStringPool;

pub type HolderRef = Rc<RefCell<CpdfIndirectObjectHolder>>;
type ParseIndirectObject = dyn Fn(u32) -> Option<ObjectRef>;

pub struct CpdfIndirectObjectHolder {
    indirect_objects: BTreeMap<u32, Option<ObjectRef>>,
    last_obj_num: u32,
    byte_string_pool: Rc<RefCell<ByteStringPool>>,
    parser: Option<Rc<ParseIndirectObject>>,
}

impl CpdfIndirectObjectHolder {
    pub fn new() -> HolderRef {
        Rc::new(RefCell::new(Self {
            indirect_objects: BTreeMap::new(),
            last_obj_num: 0,
            byte_string_pool: Rc::new(RefCell::new(ByteStringPool::new())),
            parser: None,
        }))
    }

    pub fn byte_string_pool(&self) -> Weak<RefCell<ByteStringPool>> {
        Rc::downgrade(&self.byte_string_pool)
    }

    pub fn last_obj_num(&self) -> u32 {
        self.last_obj_num
    }
    pub fn set_last_obj_num(&mut self, value: u32) {
        self.last_obj_num = value;
    }
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &Option<ObjectRef>)> {
        self.indirect_objects.iter()
    }

    pub fn new_name(&self, name: &[u8]) -> ObjectRef {
        retain_object(CpdfName::new(
            Some(&mut self.byte_string_pool.borrow_mut()),
            name,
        ))
    }

    pub fn new_array(&self) -> ObjectRef {
        retain_object(CpdfArray::with_pool(self.byte_string_pool()))
    }

    pub fn new_dictionary(&self) -> ObjectRef {
        retain_object(CpdfDictionary::with_pool(self.byte_string_pool()))
    }

    pub fn new_stream_from_bytes(&self, data: &[u8]) -> ObjectRef {
        retain_object(CpdfStream::from_bytes(data))
    }

    pub fn new_indirect_stream_from_bytes(&mut self, data: &[u8]) -> ObjectRef {
        let object = self.new_stream_from_bytes(data);
        self.add_indirect_object(Rc::clone(&object));
        object
    }

    pub fn new_string(&self, data: &[u8]) -> ObjectRef {
        retain_object(CpdfString::from_bytes(
            Some(&mut self.byte_string_pool.borrow_mut()),
            data,
        ))
    }

    pub fn new_indirect_name(&mut self, name: &[u8]) -> ObjectRef {
        let object = self.new_name(name);
        self.add_indirect_object(Rc::clone(&object));
        object
    }

    pub fn new_indirect_string(&mut self, data: &[u8]) -> ObjectRef {
        let object = self.new_string(data);
        self.add_indirect_object(Rc::clone(&object));
        object
    }

    pub fn new_indirect_array(&mut self) -> ObjectRef {
        let object = self.new_array();
        self.add_indirect_object(Rc::clone(&object));
        object
    }

    pub fn new_indirect_dictionary(&mut self) -> ObjectRef {
        let object = self.new_dictionary();
        self.add_indirect_object(Rc::clone(&object));
        object
    }

    pub fn set_parser(&mut self, parser: Option<Rc<ParseIndirectObject>>) {
        self.parser = parser;
    }

    pub fn get_indirect_object(&self, obj_num: u32) -> Option<ObjectRef> {
        self.indirect_objects
            .get(&obj_num)?
            .as_ref()
            .and_then(|object| {
                (object.borrow().obj_num() != INVALID_OBJ_NUM).then(|| Rc::clone(object))
            })
    }

    /// The pending `None` entry is installed before invoking the parser.
    /// The `RefCell` borrow ends first, so a parser can re-enter this holder.
    pub fn get_or_parse_indirect_object(holder: &HolderRef, obj_num: u32) -> Option<ObjectRef> {
        if obj_num == 0 || obj_num == INVALID_OBJ_NUM {
            return None;
        }
        let parser = {
            let mut borrowed = holder.borrow_mut();
            if let Some(object) = borrowed.indirect_objects.get(&obj_num) {
                return object.as_ref().and_then(|object| {
                    (object.borrow().obj_num() != INVALID_OBJ_NUM).then(|| Rc::clone(object))
                });
            }
            borrowed.indirect_objects.insert(obj_num, None);
            borrowed.parser.clone()
        };
        let parsed = parser.and_then(|parse| parse(obj_num));
        let mut borrowed = holder.borrow_mut();
        let Some(object) = parsed else {
            borrowed.indirect_objects.remove(&obj_num);
            return None;
        };
        object.borrow_mut().set_obj_num(obj_num);
        borrowed.last_obj_num = borrowed.last_obj_num.max(obj_num);
        borrowed
            .indirect_objects
            .insert(obj_num, Some(Rc::clone(&object)));
        Some(object)
    }

    pub fn add_indirect_object(&mut self, object: ObjectRef) -> u32 {
        assert!(
            object.borrow().is_inline(),
            "indirect object must start inline"
        );
        self.last_obj_num = self.last_obj_num.wrapping_add(1);
        object.borrow_mut().set_obj_num(self.last_obj_num);
        self.indirect_objects
            .insert(self.last_obj_num, Some(object));
        self.last_obj_num
    }

    pub fn replace_indirect_object_if_higher_generation(
        &mut self,
        obj_num: u32,
        object: Option<ObjectRef>,
    ) -> bool {
        assert_ne!(obj_num, 0);
        let Some(object) = object else {
            return false;
        };
        if obj_num == INVALID_OBJ_NUM {
            return false;
        }
        if let Some(old) = self.get_indirect_object(obj_num) {
            if object.borrow().gen_num() <= old.borrow().gen_num() {
                return false;
            }
        }
        object.borrow_mut().set_obj_num(obj_num);
        self.indirect_objects.insert(obj_num, Some(object));
        self.last_obj_num = self.last_obj_num.max(obj_num);
        true
    }

    pub fn delete_indirect_object(&mut self, obj_num: u32) {
        if self.get_indirect_object(obj_num).is_some() {
            self.indirect_objects.remove(&obj_num);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use std::rc::Rc;

    #[test]
    fn recursive_parse_is_blocked_and_generations_only_increase() {
        let holder = CpdfIndirectObjectHolder::new();
        let weak = Rc::downgrade(&holder);
        holder.borrow_mut().set_parser(Some(Rc::new(move |obj_num| {
            let holder = weak.upgrade().unwrap();
            assert!(
                CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, obj_num).is_none()
            );
            Some(retain_object(CpdfScalarObject::number_from_signed(9)))
        })));
        let first = CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, 4).unwrap();
        assert_eq!(first.borrow().obj_num(), 4);
        let same = CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, 4).unwrap();
        assert!(Rc::ptr_eq(&first, &same));
        let replacement = retain_object(CpdfScalarObject::number_from_signed(10));
        assert!(!holder
            .borrow_mut()
            .replace_indirect_object_if_higher_generation(4, Some(Rc::clone(&replacement))));
        replacement.borrow_mut().set_gen_num(1);
        assert!(holder
            .borrow_mut()
            .replace_indirect_object_if_higher_generation(4, Some(Rc::clone(&replacement))));
        assert!(Rc::ptr_eq(
            &replacement,
            &holder.borrow().get_indirect_object(4).unwrap()
        ));
    }
}

#[cfg(test)]
#[path = "cpdf_indirect_object_holder_unittest.rs"]
mod upstream_tests;
