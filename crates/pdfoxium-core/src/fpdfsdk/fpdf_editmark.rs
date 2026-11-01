// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Content-mark editing from `fpdfsdk/fpdf_editpage.cpp`. The C ABI owns weak
//! handles; mark lists retain shared items exactly as CPDF_ContentMarks does.

use crate::fpdfapi::page::cpdf_contentmarkitem::CpdfContentMarkItem;
use crate::fpdfapi::page::cpdf_contentmarks::MarkItemRef;
use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fxcrt::fx_string::{utf16_encode, utf8_decode};

pub fn mark_at(object: &dyn PageObject, index: usize) -> Option<MarkItemRef> {
    let marks = object.base().content_marks();
    (index < marks.count_items()).then(|| marks.get_item(index))
}

pub fn add_mark(object: &mut dyn PageObject, name: &[u8]) -> MarkItemRef {
    let mark = object.base_mut().mutable_content_marks().add_mark(name);
    object.base_mut().set_dirty(true);
    mark
}

pub fn add_existing_mark(object: &mut dyn PageObject, mark: MarkItemRef) {
    object
        .base_mut()
        .mutable_content_marks()
        .add_existing_mark(mark);
    object.base_mut().set_dirty(true);
}

pub fn remove_mark(object: &mut dyn PageObject, mark: &MarkItemRef) -> bool {
    if !object.base_mut().mutable_content_marks().remove_mark(mark) {
        return false;
    }
    object.base_mut().set_dirty(true);
    true
}

pub fn mark_params(mark: &MarkItemRef) -> Option<ObjectRef> {
    CpdfContentMarkItem::param_from_ref(mark)
}

pub fn get_or_create_mark_params(holder: &HolderRef, mark: &MarkItemRef) -> ObjectRef {
    if let Some(params) = mark_params(mark) {
        return params;
    }
    let params = holder.borrow().new_dictionary();
    mark.borrow_mut().set_direct_dict(params.clone());
    params
}

pub fn param_count(mark: &MarkItemRef) -> usize {
    mark_params(mark).map_or(0, |params| {
        let borrowed = params.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .size()
    })
}

pub fn param_key(mark: &MarkItemRef, index: usize) -> Option<Vec<u8>> {
    let params = mark_params(mark)?;
    let borrowed = params.borrow();
    let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
    dict.with_locked_entries(|entries| entries.keys().nth(index).map(|key| key.to_vec()))
}

pub fn param_object(mark: &MarkItemRef, key: &[u8]) -> Option<ObjectRef> {
    let params = mark_params(mark)?;
    let borrowed = params.borrow();
    borrowed
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)
}

pub fn param_type(mark: &MarkItemRef, key: &[u8]) -> Option<ObjectType> {
    Some(param_object(mark, key)?.borrow().object_type())
}

pub fn param_int(mark: &MarkItemRef, key: &[u8]) -> Option<i32> {
    let object = param_object(mark, key)?;
    let borrowed = object.borrow();
    (borrowed.object_type() == ObjectType::Number).then(|| borrowed.get_integer())
}

pub fn param_float(mark: &MarkItemRef, key: &[u8]) -> Option<f32> {
    let object = param_object(mark, key)?;
    let borrowed = object.borrow();
    (borrowed.object_type() == ObjectType::Number).then(|| borrowed.get_number())
}

pub fn param_bytes(mark: &MarkItemRef, key: &[u8]) -> Option<Vec<u8>> {
    let object = param_object(mark, key)?;
    let borrowed = object.borrow();
    (borrowed.object_type() == ObjectType::String).then(|| borrowed.get_string())
}

/// WideString::FromUTF8(...).ToUTF16LE(), including the UTF-16 NUL terminator.
pub fn utf8_to_utf16le(bytes: &[u8]) -> Vec<u8> {
    utf16_encode(&utf8_decode(bytes))
        .into_iter()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

pub enum ParamValue<'a> {
    Int(i32),
    Float(f32),
    String(&'a [u8]),
    Blob(&'a [u8]),
}

/// The caller checks that its page object contains this mark, then sets that
/// object's dirty flag. Keeping these steps separate releases the object borrow
/// before a property dictionary is resolved through the document's input stream.
pub fn set_param(holder: &HolderRef, mark: &MarkItemRef, key: &[u8], value: ParamValue<'_>) {
    let params = get_or_create_mark_params(holder, mark);
    let mut borrowed = params.borrow_mut();
    let dict = borrowed
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap();
    match value {
        ParamValue::Int(value) => {
            dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        ParamValue::Float(value) => {
            dict.set_new_for(key, CpdfScalarObject::number_from_float(value));
        }
        ParamValue::String(value) => {
            dict.set_new_string_for(key, value);
        }
        ParamValue::Blob(value) => {
            dict.set_new_for(key, CpdfString::from_hex_bytes(None, value));
        }
    }
}

pub fn remove_param(mark: &MarkItemRef, key: &[u8]) -> bool {
    let Some(params) = mark_params(mark) else {
        return false;
    };
    let mut borrowed = params.borrow_mut();
    borrowed
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .remove_for(key)
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
    use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use std::rc::Rc;

    // Focused coverage of AddExistingMark/SetMarkParam/RemoveMarks from
    // fpdf_edit_embeddertest.cpp, with explicit shared mark identity.
    #[test]
    fn shared_marks_keep_typed_parameters_and_remove_independently() {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let mut first = CpdfPathObject::new(-1);
        let mut second = CpdfPathObject::new(-1);
        let mark = add_mark(&mut first, b"Span");
        add_existing_mark(&mut second, mark.clone());
        assert!(Rc::ptr_eq(&mark, &mark_at(&second, 0).unwrap()));
        set_param(&document.holder(), &mark, b"MCID", ParamValue::Int(7));
        set_param(
            &document.holder(),
            &mark,
            b"Factor",
            ParamValue::Float(3.25),
        );
        set_param(
            &document.holder(),
            &mark,
            b"Position",
            ParamValue::String(b"End"),
        );
        set_param(
            &document.holder(),
            &mark,
            b"Blob",
            ParamValue::Blob(&[0, 255, 0x42]),
        );
        assert_eq!(param_count(&mark), 4);
        assert_eq!(param_int(&mark, b"MCID"), Some(7));
        assert_eq!(param_float(&mark, b"Factor"), Some(3.25));
        assert_eq!(param_int(&mark, b"Factor"), Some(3));
        assert_eq!(param_type(&mark, b"Blob"), Some(ObjectType::String));
        assert_eq!(param_bytes(&mark, b"Blob"), Some(vec![0, 255, 0x42]));
        assert_eq!(param_int(&mark, b"Position"), None);
        assert_eq!(param_bytes(&mark, b"MCID"), None);
        assert_eq!(param_type(&mark, b"Missing"), None);
        assert_eq!(second.base().content_marks().marked_content_id(), 7);
        assert_eq!(param_key(&mark, 0), Some(b"Blob".to_vec()));
        assert_eq!(param_key(&mark, 4), None);
        assert!(remove_param(&mark, b"Factor"));
        assert!(!remove_param(&mark, b"Factor"));
        assert!(remove_mark(&mut first, &mark));
        assert_eq!(first.base().content_marks().count_items(), 0);
        assert_eq!(second.base().content_marks().count_items(), 1);
        assert_eq!(param_bytes(&mark, b"Position"), Some(b"End".to_vec()));
    }

    #[test]
    fn properties_dictionary_is_edited_in_place_and_utf8_uses_utf16le() {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let properties = retain_object(CpdfDictionary::new());
        let params = {
            let mut borrowed = properties.borrow_mut();
            borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_dictionary_for(b"MC0")
        };
        let mut object = CpdfPathObject::new(-1);
        let mark = object
            .base_mut()
            .mutable_content_marks()
            .add_mark_with_properties_holder(b"Span", properties, b"MC0");
        set_param(
            &document.holder(),
            &mark,
            b"Position",
            ParamValue::String("\u{1f3a8}".as_bytes()),
        );
        assert!(Rc::ptr_eq(&params, &mark_params(&mark).unwrap()));
        assert_eq!(
            utf8_to_utf16le(&param_bytes(&mark, b"Position").unwrap()),
            vec![0x3c, 0xd8, 0xa8, 0xdf, 0, 0]
        );
        assert_eq!(utf8_to_utf16le(b""), vec![0, 0]);
    }
}
