// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Shared `CPDF_Object` behavior from `core/fpdfapi/parser/cpdf_object.{h,cpp}`.
//! Container, stream, and dictionary dispatch are added with those types.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_encryptor::CpdfEncryptor;
use super::cpdf_indirect_object_holder::HolderRef;
use super::cpdf_name::CpdfName;
use super::cpdf_reference::CpdfReference;
use super::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use super::cpdf_stream::CpdfStream;
use super::cpdf_string::CpdfString;
use crate::fxcrt::fx_stream::WriteStream;

pub const INVALID_OBJ_NUM: u32 = u32::MAX;

pub type ObjectRef = Rc<RefCell<dyn PdfObject>>;

pub fn retain_object<T: PdfObject + 'static>(object: T) -> ObjectRef {
    Rc::new(RefCell::new(object))
}

pub fn get_direct_object(object: &ObjectRef) -> Option<ObjectRef> {
    // Snapshot the weak holder/number before lazy parsing. An embedder's input
    // callback may re-enter and borrow the same reference object.
    let reference = {
        let borrowed = object.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfReference>()
            .map(|r| (r.indirect_object_holder(), r.ref_obj_num()))
    };
    match reference {
        Some((Some(holder), number)) => super::cpdf_indirect_object_holder::CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, number),
        Some((None, _)) => None,
        None => Some(Rc::clone(object)),
    }
}

pub fn get_dict_object(object: &ObjectRef) -> Option<ObjectRef> {
    let direct = get_direct_object(object)?;
    let borrowed = direct.borrow();
    if borrowed.object_type() == ObjectType::Dictionary {
        drop(borrowed);
        return Some(direct);
    }
    borrowed
        .as_any()
        .downcast_ref::<CpdfStream>()
        .map(CpdfStream::get_dict)
}

pub fn make_reference(object: &ObjectRef, holder: &HolderRef) -> ObjectRef {
    let borrowed = object.borrow();
    if let Some(reference) = borrowed.as_any().downcast_ref::<CpdfReference>() {
        return retain_object(reference.make_reference(holder));
    }
    assert!(
        !borrowed.is_inline(),
        "only indirect objects can be referenced"
    );
    retain_object(CpdfReference::new(holder, borrowed.obj_num()))
}

pub fn clone_direct_object(object: &ObjectRef) -> Option<ObjectRef> {
    clone_non_cyclic(object, true, &HashSet::new())
}

/// The virtual methods of `CPDF_Object`. Defaults match its base methods.
pub trait PdfObject: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// PDFium's AsDictionary(), without a second virtual Any::type_id call.
    fn as_dictionary(&self) -> Option<&CpdfDictionary> {
        None
    }
    fn object_type(&self) -> ObjectType;
    fn obj_num(&self) -> u32;
    fn set_obj_num(&mut self, value: u32);
    fn gen_num(&self) -> u32;
    fn set_gen_num(&mut self, value: u32);
    fn clone_object(&self) -> ObjectRef;
    fn clone_non_cyclic(&self, _direct: bool, _visited: &HashSet<usize>) -> Option<ObjectRef> {
        Some(self.clone_object())
    }
    fn write_to(
        &self,
        archive: &mut dyn WriteStream,
        encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool;

    fn is_inline(&self) -> bool {
        self.obj_num() == 0
    }
    fn get_string(&self) -> Vec<u8> {
        Vec::new()
    }
    /// Borrow a direct string/name's bytes; other GetString implementations
    /// can synthesize or resolve their result and retain the owned fallback.
    fn string_bytes(&self) -> Option<&[u8]> {
        None
    }
    fn get_unicode_text(&self) -> Vec<u32> {
        Vec::new()
    }
    fn get_number(&self) -> f32 {
        0.0
    }
    fn get_integer(&self) -> i32 {
        0
    }
    fn set_string(&mut self, _value: &[u8]) {
        panic!("SetString on unsupported PDF object type")
    }
}

/// Clone a retained object while preserving PDFium's per-branch visited set.
pub fn clone_non_cyclic(
    object: &ObjectRef,
    direct: bool,
    visited: &HashSet<usize>,
) -> Option<ObjectRef> {
    // clone_object() seeds visited with the concrete object's address. Use
    // that same address here, rather than the surrounding Rc<RefCell<_>>.
    let identity = {
        let borrowed = object.borrow();
        borrowed.as_any() as *const dyn Any as *const () as usize
    };
    if visited.contains(&identity) {
        return None;
    }
    let mut visited = visited.clone();
    visited.insert(identity);
    object.borrow().clone_non_cyclic(direct, &visited)
}

/// PDFium uses a pointer for inline objects and object/generation numbers for
/// indirect objects. The `ObjectRef` address is stable while retained.
pub fn key_for_cache(object: &ObjectRef) -> u64 {
    let borrowed = object.borrow();
    if borrowed.is_inline() {
        (1u64 << 63) | (Rc::as_ptr(object) as *const () as usize as u64)
    } else {
        (u64::from(borrowed.obj_num()) << 32) | u64::from(borrowed.gen_num())
    }
}

impl PdfObject for CpdfScalarObject {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn object_type(&self) -> ObjectType {
        CpdfScalarObject::object_type(self)
    }
    fn obj_num(&self) -> u32 {
        CpdfScalarObject::obj_num(self)
    }
    fn set_obj_num(&mut self, value: u32) {
        CpdfScalarObject::set_obj_num(self, value);
    }
    fn gen_num(&self) -> u32 {
        CpdfScalarObject::gen_num(self)
    }
    fn set_gen_num(&mut self, value: u32) {
        CpdfScalarObject::set_gen_num(self, value);
    }
    fn clone_object(&self) -> ObjectRef {
        retain_object(CpdfScalarObject::clone_object(self))
    }
    fn write_to(
        &self,
        archive: &mut dyn WriteStream,
        _encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        CpdfScalarObject::write_to(self, archive)
    }
    fn get_string(&self) -> Vec<u8> {
        CpdfScalarObject::get_string(self)
    }
    fn get_unicode_text(&self) -> Vec<u32> {
        CpdfScalarObject::get_unicode_text(self)
    }
    fn get_number(&self) -> f32 {
        CpdfScalarObject::get_number(self)
    }
    fn get_integer(&self) -> i32 {
        CpdfScalarObject::get_integer(self)
    }
    fn set_string(&mut self, value: &[u8]) {
        CpdfScalarObject::set_string(self, value);
    }
}

impl PdfObject for CpdfName {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn object_type(&self) -> ObjectType {
        CpdfName::object_type(self)
    }
    fn obj_num(&self) -> u32 {
        CpdfName::obj_num(self)
    }
    fn set_obj_num(&mut self, value: u32) {
        CpdfName::set_obj_num(self, value);
    }
    fn gen_num(&self) -> u32 {
        CpdfName::gen_num(self)
    }
    fn set_gen_num(&mut self, value: u32) {
        CpdfName::set_gen_num(self, value);
    }
    fn clone_object(&self) -> ObjectRef {
        retain_object(CpdfName::clone_object(self))
    }
    fn write_to(
        &self,
        archive: &mut dyn WriteStream,
        encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        CpdfName::write_to(self, archive, encryptor)
    }
    fn get_string(&self) -> Vec<u8> {
        CpdfName::get_string(self).to_vec()
    }
    fn string_bytes(&self) -> Option<&[u8]> {
        Some(CpdfName::get_string(self))
    }
    fn get_unicode_text(&self) -> Vec<u32> {
        CpdfName::get_unicode_text(self)
    }
    fn set_string(&mut self, value: &[u8]) {
        CpdfName::set_string(self, value);
    }
}

impl PdfObject for CpdfString {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn object_type(&self) -> ObjectType {
        CpdfString::object_type(self)
    }
    fn obj_num(&self) -> u32 {
        CpdfString::obj_num(self)
    }
    fn set_obj_num(&mut self, value: u32) {
        CpdfString::set_obj_num(self, value);
    }
    fn gen_num(&self) -> u32 {
        CpdfString::gen_num(self)
    }
    fn set_gen_num(&mut self, value: u32) {
        CpdfString::set_gen_num(self, value);
    }
    fn clone_object(&self) -> ObjectRef {
        retain_object(CpdfString::clone_object(self))
    }
    fn write_to(
        &self,
        archive: &mut dyn WriteStream,
        encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        CpdfString::write_to(self, archive, encryptor)
    }
    fn get_string(&self) -> Vec<u8> {
        CpdfString::get_string(self).to_vec()
    }
    fn string_bytes(&self) -> Option<&[u8]> {
        Some(CpdfString::get_string(self))
    }
    fn get_unicode_text(&self) -> Vec<u32> {
        CpdfString::get_unicode_text(self)
    }
    fn set_string(&mut self, value: &[u8]) {
        CpdfString::set_string(self, value);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn direct_reference_releases_borrow_before_lazy_parser_callback() {
        use super::*;
        use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
        use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
        let holder = CpdfIndirectObjectHolder::new();
        let reference = retain_object(CpdfReference::new(&holder, 7));
        let weak = Rc::downgrade(&reference);
        holder.borrow_mut().set_parser(Some(Rc::new(move |number| {
            assert_eq!(number, 7);
            let reference = weak.upgrade().unwrap();
            let _reentrant = reference.borrow_mut();
            Some(retain_object(CpdfScalarObject::number_from_signed(19)))
        })));
        assert_eq!(
            get_direct_object(&reference)
                .unwrap()
                .borrow()
                .get_integer(),
            19
        );
        drop(holder);
        assert!(get_direct_object(&reference).is_none());
    }

    use super::{key_for_cache, retain_object};
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn cache_keys_use_identity_or_indirect_numbers() {
        let first = retain_object(CpdfScalarObject::number_from_signed(3));
        let second = retain_object(CpdfScalarObject::number_from_signed(3));
        assert_ne!(key_for_cache(&first), key_for_cache(&second));
        first.borrow_mut().set_obj_num(12);
        first.borrow_mut().set_gen_num(2);
        second.borrow_mut().set_obj_num(12);
        second.borrow_mut().set_gen_num(2);
        assert_eq!(key_for_cache(&first), key_for_cache(&second));
        assert_eq!(key_for_cache(&first), (12u64 << 32) | 2);
    }
}

#[cfg(test)]
#[path = "cpdf_object_unittest.rs"]
mod upstream_tests;
