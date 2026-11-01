// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_dictionary.{h,cpp}`. Stream typed access awaits
//! the stream object's translation.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::rc::{Rc, Weak};

use super::cpdf_array::CpdfArray;
use super::cpdf_encryptor::CpdfEncryptor;
use super::cpdf_indirect_object_holder::HolderRef;
use super::cpdf_name::CpdfName;
use super::cpdf_object::{
    clone_non_cyclic, get_dict_object, get_direct_object, make_reference, retain_object, ObjectRef,
    PdfObject,
};
use super::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use super::cpdf_string::CpdfString;
use super::fpdf_parser_utility::pdf_name_encode;
use crate::fxcrt::bytestring_pool::ByteStringPool;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxcrt::fx_stream::WriteStream;

pub struct CpdfDictionary {
    obj_num: u32,
    gen_num: u32,
    lock_count: Cell<u32>,
    pool: Weak<RefCell<ByteStringPool>>,
    map: BTreeMap<Rc<[u8]>, ObjectRef>,
}

impl CpdfDictionary {
    pub fn new() -> Self {
        Self::with_pool(Weak::new())
    }

    pub fn with_pool(pool: Weak<RefCell<ByteStringPool>>) -> Self {
        Self {
            obj_num: 0,
            gen_num: 0,
            lock_count: Cell::new(0),
            pool,
            map: BTreeMap::new(),
        }
    }

    pub fn byte_string_pool(&self) -> Weak<RefCell<ByteStringPool>> {
        self.pool.clone()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    pub fn size(&self) -> usize {
        self.map.len()
    }
    pub fn is_locked(&self) -> bool {
        self.lock_count.get() != 0
    }

    pub fn get_object_for(&self, key: &[u8]) -> Option<ObjectRef> {
        self.map.get(key).cloned()
    }

    pub fn get_direct_object_for(&self, key: &[u8]) -> Option<ObjectRef> {
        get_direct_object(self.map.get(key)?)
    }

    pub fn get_typed_direct_for(&self, key: &[u8], object_type: ObjectType) -> Option<ObjectRef> {
        let object = self.get_direct_object_for(key)?;
        let matches = object.borrow().object_type() == object_type;
        matches.then_some(object)
    }

    pub fn get_typed_raw_for(&self, key: &[u8], object_type: ObjectType) -> Option<ObjectRef> {
        let object = self.get_object_for(key)?;
        let matches = object.borrow().object_type() == object_type;
        matches.then_some(object)
    }

    pub fn get_byte_string_for(&self, key: &[u8], default: &[u8]) -> Vec<u8> {
        self.map
            .get(key)
            .map_or_else(|| default.to_vec(), |o| o.borrow().get_string())
    }

    /// Compare GetByteStringFor() with a native C-string literal without
    /// copying direct name/string values. Other objects retain GetString's
    /// scalar/reference behavior, including its distinction from a missing key.
    pub(crate) fn byte_string_eq_cstr_for(&self, key: &[u8], value: &[u8], default: &[u8]) -> bool {
        use crate::fxcrt::bytestring::eq_cstr;
        let Some(object) = self.map.get(key) else {
            return eq_cstr(default, value);
        };
        let borrowed = object.borrow();
        if let Some(bytes) = borrowed.string_bytes() {
            return eq_cstr(bytes, value);
        }
        eq_cstr(&borrowed.get_string(), value)
    }

    pub fn get_unicode_text_for(&self, key: &[u8]) -> Vec<u32> {
        self.get_direct_object_for(key)
            .map_or_else(Vec::new, |o| o.borrow().get_unicode_text())
    }

    pub fn get_name_for(&self, key: &[u8]) -> Vec<u8> {
        self.get_typed_raw_for(key, ObjectType::Name)
            .map_or_else(Vec::new, |o| o.borrow().get_string())
    }

    pub fn get_boolean_for(&self, key: &[u8], default: bool) -> bool {
        self.get_typed_raw_for(key, ObjectType::Boolean)
            .map_or(default, |o| o.borrow().get_integer() != 0)
    }

    pub fn get_integer_for(&self, key: &[u8], default: i32) -> i32 {
        self.map
            .get(key)
            .map_or(default, |o| o.borrow().get_integer())
    }

    pub fn get_direct_integer_for(&self, key: &[u8]) -> i32 {
        self.get_typed_raw_for(key, ObjectType::Number)
            .map_or(0, |o| o.borrow().get_integer())
    }

    pub fn get_float_for(&self, key: &[u8]) -> f32 {
        self.map.get(key).map_or(0.0, |o| o.borrow().get_number())
    }

    pub fn get_array_for(&self, key: &[u8]) -> Option<ObjectRef> {
        self.get_typed_direct_for(key, ObjectType::Array)
    }

    pub fn get_dict_for(&self, key: &[u8]) -> Option<ObjectRef> {
        get_dict_object(self.map.get(key)?)
    }

    pub fn get_stream_for(&self, key: &[u8]) -> Option<ObjectRef> {
        self.get_typed_direct_for(key, ObjectType::Stream)
    }

    pub fn get_number_for(&self, key: &[u8]) -> Option<ObjectRef> {
        self.get_typed_raw_for(key, ObjectType::Number)
    }

    pub fn get_string_for(&self, key: &[u8]) -> Option<ObjectRef> {
        self.get_typed_raw_for(key, ObjectType::String)
    }

    pub fn get_or_create_array_for(&mut self, key: &[u8]) -> ObjectRef {
        if let Some(object) = self.get_array_for(key) {
            return object;
        }
        let object = retain_object(CpdfArray::with_pool(self.pool.clone()));
        self.set_for(key, Some(Rc::clone(&object)));
        object
    }

    pub fn get_or_create_dict_for(&mut self, key: &[u8]) -> ObjectRef {
        if let Some(object) = self.get_dict_for(key) {
            return object;
        }
        let object = retain_object(Self::with_pool(self.pool.clone()));
        self.set_for(key, Some(Rc::clone(&object)));
        object
    }

    pub fn get_rect_for(&self, key: &[u8]) -> CfxFloatRect {
        let Some(array) = self.get_array_for(key) else {
            return CfxFloatRect::default();
        };
        let borrowed = array.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .get_rect()
    }

    pub fn get_matrix_for(&self, key: &[u8]) -> CfxMatrix {
        let Some(array) = self.get_array_for(key) else {
            return CfxMatrix::default();
        };
        let borrowed = array.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .get_matrix()
    }

    pub fn key_exist(&self, key: &[u8]) -> bool {
        self.map.contains_key(key)
    }

    pub fn get_keys(&self) -> Vec<Vec<u8>> {
        self.map.keys().map(|key| key.to_vec()).collect()
    }

    fn maybe_intern(&self, key: &[u8]) -> Rc<[u8]> {
        if let Some(pool) = self.pool.upgrade() {
            pool.borrow_mut().intern(key)
        } else {
            Rc::from(key)
        }
    }

    pub fn set_for(&mut self, key: &[u8], object: Option<ObjectRef>) {
        assert!(!self.is_locked(), "mutating locked PDF dictionary");
        let Some(object) = object else {
            self.map.remove(key);
            return;
        };
        let borrowed = object.borrow();
        assert!(
            borrowed.is_inline(),
            "PDF dictionary entries must be inline"
        );
        assert_ne!(
            borrowed.object_type(),
            ObjectType::Stream,
            "PDF streams must be indirect"
        );
        drop(borrowed);
        self.map.insert(self.maybe_intern(key), object);
    }

    pub fn set_new_for<T: PdfObject + 'static>(&mut self, key: &[u8], object: T) -> ObjectRef {
        let object = retain_object(object);
        self.set_for(key, Some(Rc::clone(&object)));
        object
    }

    pub fn set_new_name_for(&mut self, key: &[u8], name: &[u8]) -> ObjectRef {
        let value = if let Some(pool) = self.pool.upgrade() {
            CpdfName::new(Some(&mut pool.borrow_mut()), name)
        } else {
            CpdfName::new(None, name)
        };
        self.set_new_for(key, value)
    }

    pub fn set_new_string_for(&mut self, key: &[u8], data: &[u8]) -> ObjectRef {
        let value = if let Some(pool) = self.pool.upgrade() {
            CpdfString::from_bytes(Some(&mut pool.borrow_mut()), data)
        } else {
            CpdfString::from_bytes(None, data)
        };
        self.set_new_for(key, value)
    }

    pub fn set_new_array_for(&mut self, key: &[u8]) -> ObjectRef {
        self.set_new_for(key, CpdfArray::with_pool(self.pool.clone()))
    }

    pub fn set_new_dictionary_for(&mut self, key: &[u8]) -> ObjectRef {
        self.set_new_for(key, Self::with_pool(self.pool.clone()))
    }

    pub fn convert_to_indirect_object_for(&mut self, key: &[u8], holder: &HolderRef) {
        assert!(!self.is_locked(), "mutating locked PDF dictionary");
        let Some(object) = self.get_object_for(key) else {
            return;
        };
        if object.borrow().object_type() == ObjectType::Reference {
            return;
        }
        holder.borrow_mut().add_indirect_object(Rc::clone(&object));
        *self.map.get_mut(key).unwrap() = make_reference(&object, holder);
    }

    pub fn remove_for(&mut self, key: &[u8]) -> Option<ObjectRef> {
        assert!(!self.is_locked(), "mutating locked PDF dictionary");
        self.map.remove(key)
    }

    pub fn replace_key(&mut self, old_key: &[u8], new_key: &[u8]) {
        assert!(!self.is_locked(), "mutating locked PDF dictionary");
        if old_key == new_key {
            return;
        }
        if let Some(object) = self.map.remove(old_key) {
            self.map.insert(self.maybe_intern(new_key), object);
        }
    }

    pub fn set_rect_for(&mut self, key: &[u8], rect: CfxFloatRect) {
        self.set_float_array_for(key, &[rect.left, rect.bottom, rect.right, rect.top]);
    }

    pub fn set_matrix_for(&mut self, key: &[u8], matrix: CfxMatrix) {
        self.set_float_array_for(
            key,
            &[matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f],
        );
    }

    fn set_float_array_for(&mut self, key: &[u8], values: &[f32]) {
        let mut array = CpdfArray::with_pool(self.pool.clone());
        for &value in values {
            array.append(retain_object(CpdfScalarObject::number_from_float(value)));
        }
        self.set_for(key, Some(retain_object(array)));
    }

    pub fn with_locked_entries<R>(&self, f: impl FnOnce(&BTreeMap<Rc<[u8]>, ObjectRef>) -> R) -> R {
        self.lock_count.set(self.lock_count.get() + 1);
        struct Unlock<'a>(&'a Cell<u32>);
        impl Drop for Unlock<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() - 1);
            }
        }
        let _unlock = Unlock(&self.lock_count);
        f(&self.map)
    }

    pub(crate) fn lock_for_walker(&self) {
        self.lock_count.set(self.lock_count.get() + 1);
    }

    pub(crate) fn unlock_for_walker(&self) {
        self.lock_count.set(self.lock_count.get() - 1);
    }

    fn clone_with_visited(&self, direct: bool, visited: &HashSet<usize>) -> ObjectRef {
        let mut copy = Self::with_pool(self.pool.clone());
        for (key, object) in &self.map {
            if let Some(clone) = clone_non_cyclic(object, direct, visited) {
                copy.map.insert(Rc::clone(key), clone);
            }
        }
        retain_object(copy)
    }

    fn is_signature_dictionary(&self) -> bool {
        let type_object = self
            .get_direct_object_for(b"Type")
            .or_else(|| self.get_direct_object_for(b"FT"));
        type_object.is_some_and(|object| {
            crate::fxcrt::bytestring::eq_cstr(&object.borrow().get_string(), b"Sig")
        })
    }
}

impl Default for CpdfDictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfObject for CpdfDictionary {
    fn as_dictionary(&self) -> Option<&CpdfDictionary> {
        Some(self)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn object_type(&self) -> ObjectType {
        ObjectType::Dictionary
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
        if !archive.write_string(b"<<") {
            return false;
        }
        let is_signature = self.is_signature_dictionary();
        self.with_locked_entries(|entries| {
            for (key, object) in entries {
                if !archive.write_string(b"/") || !archive.write_string(&pdf_name_encode(key)) {
                    return false;
                }
                let field_encryptor =
                    if !is_signature || !crate::fxcrt::bytestring::eq_cstr(key, b"Contents") {
                        encryptor
                    } else {
                        None
                    };
                if !object.borrow().write_to(archive, field_encryptor) {
                    return false;
                }
            }
            archive.write_string(b">>")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_encryptor::{CpdfEncryptor, CryptoHandler};
    use crate::fpdfapi::parser::cpdf_name::CpdfName;
    use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef, PdfObject};
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    use crate::fxcrt::fx_stream::WriteStream;

    struct Buffer(Vec<u8>);
    impl WriteStream for Buffer {
        fn write_block(&mut self, bytes: &[u8]) -> bool {
            self.0.extend_from_slice(bytes);
            true
        }
    }
    struct Handler;
    impl CryptoHandler for Handler {
        fn encrypt_content(&self, _obj_num: u32, _gen_num: u32, src: &[u8]) -> Vec<u8> {
            src.iter().map(|byte| byte ^ 0x20).collect()
        }
    }

    #[test]
    fn borrowed_literal_comparison_keeps_defaults_nuls_and_reference_values() {
        use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
        use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
        use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
        let holder = CpdfIndirectObjectHolder::new();
        let id = holder
            .borrow_mut()
            .add_indirect_object(retain_object(CpdfName::new(None, b"OCG\0tail")));
        let mut dict = CpdfDictionary::new();
        assert!(dict.byte_string_eq_cstr_for(b"Type", b"OCG", b"OCG"));
        assert!(!dict.byte_string_eq_cstr_for(b"Type", b"OCG", b""));
        for (object, expected) in [
            (
                retain_object(CpdfName::new(None, b"OCG\0tail")),
                b"OCG".as_slice(),
            ),
            (
                retain_object(CpdfString::from_bytes(None, b"OCG\0tail")),
                b"OCG",
            ),
            (retain_object(CpdfName::new(None, b"OCMD")), b"OCMD"),
            (
                retain_object(CpdfScalarObject::number_from_signed(17)),
                b"17",
            ),
            (retain_object(CpdfScalarObject::boolean(true)), b"true"),
            (retain_object(CpdfDictionary::new()), b""),
            (retain_object(CpdfReference::new(&holder, id)), b"OCG"),
            (retain_object(CpdfReference::new(&holder, id + 1)), b""),
        ] {
            dict.set_for(b"Type", Some(object));
            assert!(dict.byte_string_eq_cstr_for(b"Type", expected, b"missing"));
            for value in [
                b"OCG".as_slice(),
                b"OC",
                b"OCGx",
                b"OCMD",
                b"",
                b"17",
                b"true",
                b"missing",
            ] {
                assert_eq!(
                    dict.byte_string_eq_cstr_for(b"Type", value, b"missing"),
                    crate::fxcrt::bytestring::eq_cstr(
                        &dict.get_byte_string_for(b"Type", b"missing"),
                        value
                    )
                );
            }
        }
    }

    #[test]
    fn signature_contents_remains_plain_while_other_strings_encrypt() {
        let mut dict = CpdfDictionary::new();
        dict.set_for(b"Type", Some(retain_object(CpdfName::new(None, b"Sig"))));
        dict.set_for(
            b"Contents",
            Some(retain_object(CpdfString::from_bytes(None, b"ab"))),
        );
        dict.set_for(
            b"Reason",
            Some(retain_object(CpdfString::from_bytes(None, b"ab"))),
        );
        let handler = Handler;
        let encryptor = CpdfEncryptor::new(&handler, 3);
        let mut buffer = Buffer(Vec::new());
        assert!(dict.write_to(&mut buffer, Some(&encryptor)));
        assert_eq!(buffer.0, b"<</Contents(ab)/Reason(AB)/Type/Sig>>");
    }

    #[test]
    fn signature_c_string_comparisons_preserve_embedded_nul_behavior() {
        for field in [b"Type".as_slice(), b"FT".as_slice()] {
            for contents_key in [b"Contents".as_slice(), b"Contents\0suffix".as_slice()] {
                let mut dict = CpdfDictionary::new();
                dict.set_for(
                    field,
                    Some(retain_object(CpdfName::new(None, b"Sig\0suffix"))),
                );
                dict.set_for(
                    contents_key,
                    Some(retain_object(CpdfString::from_bytes(None, b"ab"))),
                );
                dict.set_for(
                    b"Reason",
                    Some(retain_object(CpdfString::from_bytes(None, b"ab"))),
                );
                let handler = Handler;
                let encryptor = CpdfEncryptor::new(&handler, 3);
                let mut buffer = Buffer(Vec::new());
                assert!(dict.write_to(&mut buffer, Some(&encryptor)));
                let mut expected = b"<</".to_vec();
                expected.extend_from_slice(&super::pdf_name_encode(contents_key));
                expected.extend_from_slice(b"(ab)/");
                if field == b"FT" {
                    expected.extend_from_slice(b"FT/Sig#00suffix/Reason(AB)>>");
                } else {
                    expected.extend_from_slice(b"Reason(AB)/Type/Sig#00suffix>>");
                }
                assert_eq!(buffer.0, expected);
            }
        }
    }

    #[test]
    fn archive_callbacks_observe_dictionary_locker_and_failure_releases_it() {
        struct Observer {
            dictionary: ObjectRef,
            locked: Vec<bool>,
            fail_at: Option<usize>,
        }
        impl WriteStream for Observer {
            fn write_block(&mut self, _bytes: &[u8]) -> bool {
                let borrowed = self.dictionary.borrow();
                let dictionary = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
                self.locked.push(dictionary.is_locked());
                self.fail_at != Some(self.locked.len() - 1)
            }
        }
        let mut dictionary = CpdfDictionary::new();
        dictionary.set_for(
            b"A",
            Some(retain_object(CpdfString::from_bytes(None, b"ab"))),
        );
        let dictionary = retain_object(dictionary);
        for fail_at in [None, Some(0), Some(1), Some(4)] {
            let mut observer = Observer {
                dictionary: dictionary.clone(),
                locked: Vec::new(),
                fail_at,
            };
            assert_eq!(
                dictionary.borrow().write_to(&mut observer, None),
                fail_at.is_none()
            );
            let expected = [false, true, true, true, true];
            assert_eq!(observer.locked, expected[..observer.locked.len()]);
            assert_eq!(observer.locked.len(), fail_at.map_or(5, |index| index + 1));
            assert!(!dictionary
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .is_locked());
        }
    }
}

#[cfg(test)]
#[path = "cpdf_dictionary_unittest.rs"]
mod upstream_tests;
