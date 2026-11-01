// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Shared TestHolder from cpdf_object_avail_unittest.cpp and
//! cpdf_page_object_avail_unittest.cpp (identical native fixtures).
#![allow(non_snake_case)]
use super::cpdf_array::CpdfArray;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use super::cpdf_object::ObjectRef;
use super::cpdf_read_validator::CpdfReadValidator;
use super::cpdf_reference::CpdfReference;
use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
#[derive(Clone, Copy, PartialEq)]
pub enum ObjectState {
    Unavailable,
    Available,
}
struct ObjectData {
    object: ObjectRef,
    state: ObjectState,
}
// testing/invalid_seekable_read_stream.{h,cpp}.
struct InvalidSeekableReadStream(i64);
impl StreamWithSize for InvalidSeekableReadStream {
    fn get_size(&self) -> i64 {
        self.0
    }
}
impl SeekableReadStream for InvalidSeekableReadStream {
    fn read_block_at_offset(&self, _: &mut [u8], _: i64) -> bool {
        false
    }
}
pub struct TestHolder {
    // Native derived members are destroyed before the base holder cache.
    validator: Rc<CpdfReadValidator>,
    objects_data: Rc<RefCell<BTreeMap<u32, ObjectData>>>,
    pub holder: HolderRef,
}
impl TestHolder {
    pub fn new() -> Self {
        let validator = Rc::new(CpdfReadValidator::new(
            Rc::new(InvalidSeekableReadStream(100)),
            None,
        ));
        let objects_data = Rc::new(RefCell::new(BTreeMap::<u32, ObjectData>::new()));
        let holder = CpdfIndirectObjectHolder::new();
        let weak_data = Rc::downgrade(&objects_data);
        let weak_validator = Rc::downgrade(&validator);
        holder.borrow_mut().set_parser(Some(Rc::new(move |objnum| {
            let data = weak_data.upgrade().unwrap();
            let objects = data.borrow();
            let object = objects.get(&objnum)?;
            if object.state == ObjectState::Unavailable {
                drop(objects);
                // Simulate an error via a read from an InvalidSeekableReadStream.
                weak_validator
                    .upgrade()
                    .unwrap()
                    .read_block_at_offset(&mut [], 0);
                return None;
            }
            Some(object.object.clone())
        })));
        Self {
            validator,
            objects_data,
            holder,
        }
    }
    pub fn GetValidator(&self) -> Rc<CpdfReadValidator> {
        self.validator.clone()
    }
    pub fn AddObject(&self, objnum: u32, object: ObjectRef, state: ObjectState) {
        let mut objects = self.objects_data.borrow_mut();
        assert!(!objects.contains_key(&objnum));
        objects.insert(objnum, ObjectData { object, state });
    }
    pub fn SetObjectState(&self, objnum: u32, state: ObjectState) {
        self.objects_data
            .borrow_mut()
            .get_mut(&objnum)
            .unwrap()
            .state = state;
    }
    pub fn GetTestObject(&self, objnum: u32) -> Option<ObjectRef> {
        self.objects_data
            .borrow()
            .get(&objnum)
            .map(|data| data.object.clone())
    }
    // Scoped casts replace native GetMutableDict()/AsMutableArray() pointers.
    pub fn WithDict<R>(&self, objnum: u32, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
        let object = self.GetTestObject(objnum).unwrap();
        let mut object = object.borrow_mut();
        f(object
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap())
    }
    pub fn WithArray<R>(&self, objnum: u32, f: impl FnOnce(&mut CpdfArray) -> R) -> R {
        let object = self.GetTestObject(objnum).unwrap();
        let mut object = object.borrow_mut();
        f(object.as_any_mut().downcast_mut::<CpdfArray>().unwrap())
    }
    pub fn SetReference(&self, objnum: u32, key: &[u8], target: u32) {
        self.WithDict(objnum, |dict| {
            dict.set_new_for(key, CpdfReference::new(&self.holder, target));
        });
    }
}
