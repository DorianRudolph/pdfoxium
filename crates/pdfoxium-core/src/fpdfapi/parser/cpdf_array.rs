// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_array.{h,cpp}`. Dictionary and stream typed
//! accessors follow with those object types.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::{Rc, Weak};

use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_encryptor::CpdfEncryptor;
use super::cpdf_indirect_object_holder::HolderRef;
use super::cpdf_name::CpdfName;
use super::cpdf_object::{
    clone_non_cyclic, get_dict_object, get_direct_object, make_reference, retain_object, ObjectRef,
    PdfObject,
};
use super::cpdf_scalar_object::ObjectType;
use super::cpdf_string::CpdfString;
use crate::fxcrt::bytestring_pool::ByteStringPool;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxcrt::fx_stream::WriteStream;

pub struct CpdfArray {
    obj_num: u32,
    gen_num: u32,
    objects: Vec<ObjectRef>,
    pool: Weak<RefCell<ByteStringPool>>,
    lock_count: Cell<u32>,
}

impl CpdfArray {
    pub fn new() -> Self {
        Self::with_pool(Weak::new())
    }

    pub fn with_pool(pool: Weak<RefCell<ByteStringPool>>) -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            objects: Vec::new(),
            pool,
            lock_count: Cell::new(0),
        }
    }

    pub fn size(&self) -> usize {
        self.objects.len()
    }
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
    pub fn is_locked(&self) -> bool {
        self.lock_count.get() != 0
    }

    pub fn byte_string_pool(&self) -> Weak<RefCell<ByteStringPool>> {
        self.pool.clone()
    }

    pub fn get_object_at(&self, index: usize) -> Option<ObjectRef> {
        self.objects.get(index).cloned()
    }

    pub fn get_direct_object_at(&self, index: usize) -> Option<ObjectRef> {
        get_direct_object(self.objects.get(index)?)
    }

    pub fn get_typed_direct_at(&self, index: usize, object_type: ObjectType) -> Option<ObjectRef> {
        let object = self.get_direct_object_at(index)?;
        let matches = object.borrow().object_type() == object_type;
        matches.then_some(object)
    }

    pub fn get_typed_raw_at(&self, index: usize, object_type: ObjectType) -> Option<ObjectRef> {
        let object = self.get_object_at(index)?;
        let matches = object.borrow().object_type() == object_type;
        matches.then_some(object)
    }

    pub fn get_dict_at(&self, index: usize) -> Option<ObjectRef> {
        get_dict_object(self.objects.get(index)?)
    }

    pub fn get_array_at(&self, index: usize) -> Option<ObjectRef> {
        self.get_typed_direct_at(index, ObjectType::Array)
    }

    pub fn get_stream_at(&self, index: usize) -> Option<ObjectRef> {
        self.get_typed_direct_at(index, ObjectType::Stream)
    }

    pub fn get_number_at(&self, index: usize) -> Option<ObjectRef> {
        self.get_typed_raw_at(index, ObjectType::Number)
    }

    pub fn get_string_at(&self, index: usize) -> Option<ObjectRef> {
        self.get_typed_raw_at(index, ObjectType::String)
    }

    pub fn get_byte_string_at(&self, index: usize) -> Vec<u8> {
        self.objects
            .get(index)
            .map_or_else(Vec::new, |o| o.borrow().get_string())
    }

    pub fn get_unicode_text_at(&self, index: usize) -> Vec<u32> {
        self.objects
            .get(index)
            .map_or_else(Vec::new, |o| o.borrow().get_unicode_text())
    }

    pub fn get_boolean_at(&self, index: usize, default: bool) -> bool {
        self.objects.get(index).map_or(default, |o| {
            let object = o.borrow();
            if object.object_type() == ObjectType::Boolean {
                object.get_integer() != 0
            } else {
                default
            }
        })
    }

    pub fn get_integer_at(&self, index: usize) -> i32 {
        self.objects
            .get(index)
            .map_or(0, |o| o.borrow().get_integer())
    }

    pub fn get_float_at(&self, index: usize) -> f32 {
        self.objects
            .get(index)
            .map_or(0.0, |o| o.borrow().get_number())
    }

    pub fn get_rect(&self) -> CfxFloatRect {
        if self.size() != 4 {
            return CfxFloatRect::default();
        }
        CfxFloatRect::new(
            self.get_float_at(0),
            self.get_float_at(1),
            self.get_float_at(2),
            self.get_float_at(3),
        )
    }

    pub fn get_matrix(&self) -> CfxMatrix {
        if self.size() != 6 {
            return CfxMatrix::default();
        }
        CfxMatrix::new(
            self.get_float_at(0),
            self.get_float_at(1),
            self.get_float_at(2),
            self.get_float_at(3),
            self.get_float_at(4),
            self.get_float_at(5),
        )
    }

    pub fn find<'a>(&self, that: impl Into<Option<&'a ObjectRef>>) -> Option<usize> {
        let that = that.into();
        (0..self.size()).find(|&index| match (self.get_direct_object_at(index), that) {
            (Some(object), Some(that)) => Rc::ptr_eq(&object, that),
            (None, None) => true,
            _ => false,
        })
    }

    pub fn contains(&self, that: &ObjectRef) -> bool {
        self.find(that).is_some()
    }

    fn check_new_object(&self, object: &ObjectRef) {
        assert!(!self.is_locked(), "mutating locked PDF array");
        let borrowed = object.borrow();
        assert!(borrowed.is_inline(), "PDF array entries must be inline");
        assert_ne!(
            borrowed.object_type(),
            ObjectType::Stream,
            "PDF streams must be indirect"
        );
    }

    pub fn append(&mut self, object: ObjectRef) {
        self.check_new_object(&object);
        self.objects.push(object);
    }

    pub fn append_new<T: PdfObject + 'static>(&mut self, object: T) -> ObjectRef {
        let object = retain_object(object);
        self.append(Rc::clone(&object));
        object
    }

    pub fn set_at(&mut self, index: usize, object: ObjectRef) -> Option<ObjectRef> {
        self.check_new_object(&object);
        if index >= self.size() {
            return None;
        }
        self.objects[index] = Rc::clone(&object);
        Some(object)
    }

    pub fn set_new_at<T: PdfObject + 'static>(
        &mut self,
        index: usize,
        object: T,
    ) -> Option<ObjectRef> {
        self.set_at(index, retain_object(object))
    }

    pub fn insert_at(&mut self, index: usize, object: ObjectRef) -> Option<ObjectRef> {
        self.check_new_object(&object);
        if index > self.size() {
            return None;
        }
        self.objects.insert(index, Rc::clone(&object));
        Some(object)
    }

    pub fn insert_new_at<T: PdfObject + 'static>(
        &mut self,
        index: usize,
        object: T,
    ) -> Option<ObjectRef> {
        self.insert_at(index, retain_object(object))
    }

    fn new_name(&self, name: &[u8]) -> CpdfName {
        if let Some(pool) = self.pool.upgrade() {
            CpdfName::new(Some(&mut pool.borrow_mut()), name)
        } else {
            CpdfName::new(None, name)
        }
    }

    fn new_string(&self, data: &[u8]) -> CpdfString {
        if let Some(pool) = self.pool.upgrade() {
            CpdfString::from_bytes(Some(&mut pool.borrow_mut()), data)
        } else {
            CpdfString::from_bytes(None, data)
        }
    }

    pub fn append_new_name(&mut self, name: &[u8]) -> ObjectRef {
        self.append_new(self.new_name(name))
    }

    pub fn append_new_string(&mut self, data: &[u8]) -> ObjectRef {
        self.append_new(self.new_string(data))
    }

    pub fn append_new_array(&mut self) -> ObjectRef {
        self.append_new(Self::with_pool(self.pool.clone()))
    }

    pub fn append_new_dictionary(&mut self) -> ObjectRef {
        self.append_new(CpdfDictionary::with_pool(self.pool.clone()))
    }

    pub fn set_new_name_at(&mut self, index: usize, name: &[u8]) -> Option<ObjectRef> {
        self.set_new_at(index, self.new_name(name))
    }

    pub fn set_new_string_at(&mut self, index: usize, data: &[u8]) -> Option<ObjectRef> {
        self.set_new_at(index, self.new_string(data))
    }

    pub fn set_new_array_at(&mut self, index: usize) -> Option<ObjectRef> {
        self.set_new_at(index, Self::with_pool(self.pool.clone()))
    }

    pub fn set_new_dictionary_at(&mut self, index: usize) -> Option<ObjectRef> {
        self.set_new_at(index, CpdfDictionary::with_pool(self.pool.clone()))
    }

    pub fn insert_new_name_at(&mut self, index: usize, name: &[u8]) -> Option<ObjectRef> {
        self.insert_new_at(index, self.new_name(name))
    }

    pub fn insert_new_string_at(&mut self, index: usize, data: &[u8]) -> Option<ObjectRef> {
        self.insert_new_at(index, self.new_string(data))
    }

    pub fn insert_new_array_at(&mut self, index: usize) -> Option<ObjectRef> {
        self.insert_new_at(index, Self::with_pool(self.pool.clone()))
    }

    pub fn insert_new_dictionary_at(&mut self, index: usize) -> Option<ObjectRef> {
        self.insert_new_at(index, CpdfDictionary::with_pool(self.pool.clone()))
    }

    pub fn clear(&mut self) {
        assert!(!self.is_locked(), "mutating locked PDF array");
        self.objects.clear();
    }

    pub fn remove_at(&mut self, index: usize) {
        assert!(!self.is_locked(), "mutating locked PDF array");
        if index < self.size() {
            self.objects.remove(index);
        }
    }

    pub fn convert_to_indirect_object_at(&mut self, index: usize, holder: &HolderRef) {
        assert!(!self.is_locked(), "mutating locked PDF array");
        let Some(object) = self.get_object_at(index) else {
            return;
        };
        if object.borrow().object_type() == ObjectType::Reference {
            return;
        }
        holder.borrow_mut().add_indirect_object(Rc::clone(&object));
        self.objects[index] = make_reference(&object, holder);
    }

    pub fn with_locked_objects<R>(&self, f: impl FnOnce(&[ObjectRef]) -> R) -> R {
        self.lock_count.set(self.lock_count.get() + 1);
        struct Unlock<'a>(&'a Cell<u32>);
        impl Drop for Unlock<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() - 1);
            }
        }
        let _unlock = Unlock(&self.lock_count);
        f(&self.objects)
    }

    pub(crate) fn lock_for_walker(&self) {
        self.lock_count.set(self.lock_count.get() + 1);
    }

    pub(crate) fn unlock_for_walker(&self) {
        self.lock_count.set(self.lock_count.get() - 1);
    }

    fn clone_with_visited(&self, direct: bool, visited: &HashSet<usize>) -> ObjectRef {
        let mut copy = Self::new();
        for object in &self.objects {
            if let Some(clone) = clone_non_cyclic(object, direct, visited) {
                copy.objects.push(clone);
            }
        }
        retain_object(copy)
    }
}

impl Default for CpdfArray {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfObject for CpdfArray {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn object_type(&self) -> ObjectType {
        ObjectType::Array
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
        let mut visited = HashSet::new();
        visited.insert(self as *const Self as usize);
        self.clone_with_visited(false, &visited)
    }
    fn clone_non_cyclic(&self, direct: bool, visited: &HashSet<usize>) -> Option<ObjectRef> {
        Some(self.clone_with_visited(direct, visited))
    }
    fn write_to(
        &self,
        archive: &mut dyn WriteStream,
        encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        if !archive.write_string(b"[") {
            return false;
        }
        for object in &self.objects {
            if !object.borrow().write_to(archive, encryptor) {
                return false;
            }
        }
        archive.write_string(b"]")
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfArray;
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
    fn array_geometry_mutation_and_archive_order() {
        let mut array = CpdfArray::new();
        for value in [1, 2, 3, 4] {
            array.append(retain_object(CpdfScalarObject::number_from_signed(value)));
        }
        assert_eq!(array.get_rect().left, 1.0);
        assert_eq!(array.get_rect().top, 4.0);
        let mut buffer = Buffer(Vec::new());
        assert!(array.write_to(&mut buffer, None));
        assert_eq!(buffer.0, b"[ 1 2 3 4]");
        let object = array.get_object_at(1).unwrap();
        assert_eq!(array.find(&object), Some(1));
        array.remove_at(1);
        assert!(!array.contains(&object));
        assert_eq!(array.get_rect(), Default::default());
    }

    #[test]
    fn indirect_conversion_preserves_direct_lookup() {
        let holder = CpdfIndirectObjectHolder::new();
        let mut array = CpdfArray::new();
        let number = retain_object(CpdfScalarObject::number_from_signed(8));
        array.append(number.clone());
        array.convert_to_indirect_object_at(0, &holder);
        assert_eq!(number.borrow().obj_num(), 1);
        assert_eq!(array.find(&number), Some(0));
        assert_eq!(
            array
                .get_direct_object_at(0)
                .unwrap()
                .borrow()
                .get_integer(),
            8
        );
        assert_eq!(array.get_integer_at(0), 8);
        let mut buffer = Buffer(Vec::new());
        assert!(array.write_to(&mut buffer, None));
        assert_eq!(buffer.0, b"[ 1 0 R ]");
    }

    #[test]
    fn null_query_matches_an_unresolved_reference_but_not_a_pdf_null() {
        let holder = CpdfIndirectObjectHolder::new();
        let mut array = CpdfArray::new();
        array.append_new(CpdfScalarObject::null());
        assert_eq!(array.find(None), None);
        array.append_new(crate::fpdfapi::parser::cpdf_reference::CpdfReference::new(
            &holder, 1000,
        ));
        assert_eq!(array.find(None), Some(1));
    }
}

#[cfg(test)]
#[path = "cpdf_array_unittest.rs"]
mod upstream_tests;
