// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Signature inspection from `fpdfsdk/fpdf_signature.cpp`. This reads embedded
//! signature metadata; cryptographic signature verification is a separate API.
use crate::fpdfapi::parser::{
    cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument,
    cpdf_object::ObjectRef, cpdf_scalar_object::ObjectType,
};
use crate::fpdfdoc::cpdf_formfield::field_attr;
use std::{collections::HashSet, rc::Rc};

fn dict_entry(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_direct_object_for(key)
}
fn dictionary(object: Option<ObjectRef>) -> Option<ObjectRef> {
    object.filter(|object| object.borrow().object_type() == ObjectType::Dictionary)
}
fn value(signature: &ObjectRef) -> Option<ObjectRef> {
    dictionary(field_attr(signature, b"V"))
}

/// Enumerate terminal signature fields in field-tree order. Unlike the pinned
/// native inspection wrapper's flat scan, valid nested AcroForm fields remain
/// discoverable. Follow the interactive form's field/widget distinction and
/// inherited attributes, without counting a field's widget children twice.
pub fn collect_signatures(document: &CpdfDocument) -> Vec<ObjectRef> {
    let Some(form) = dictionary(
        document
            .root()
            .and_then(|root| dict_entry(&root, b"AcroForm")),
    ) else {
        return Vec::new();
    };
    let Some(fields) = dict_entry(&form, b"Fields") else {
        return Vec::new();
    };
    let children = |array: &ObjectRef| {
        let borrowed = array.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfArray>()
            .map(|array| {
                (0..array.size())
                    .filter_map(|i| array.get_dict_at(i))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let mut pending = children(&fields);
    pending.reverse();
    let mut visited = HashSet::new();
    let mut signatures = Vec::new();
    while let Some(field) = pending.pop() {
        if !visited.insert(Rc::as_ptr(&field) as *const () as usize) {
            continue;
        }
        let kids = dict_entry(&field, b"Kids")
            .map(|kids| children(&kids))
            .unwrap_or_default();
        let has_child_fields = kids.iter().any(|kid| {
            let borrowed = kid.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            dict.key_exist(b"T")
                || dict.key_exist(b"Kids")
                || (dict.get_name_for(b"Subtype") != b"Widget" && dict.key_exist(b"FT"))
        });
        if has_child_fields {
            pending.extend(kids.into_iter().rev());
        } else if field_attr(&field, b"FT").is_some_and(|kind| kind.borrow().get_string() == b"Sig")
        {
            signatures.push(field);
        }
    }
    signatures
}
pub fn contents(signature: &ObjectRef) -> Option<Vec<u8>> {
    let value = value(signature)?;
    let borrowed = value.borrow();
    Some(
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_byte_string_for(b"Contents", b""),
    )
}
pub fn byte_range(signature: &ObjectRef) -> Option<Vec<i32>> {
    let range = dict_entry(&value(signature)?, b"ByteRange")?;
    let borrowed = range.borrow();
    let range = borrowed.as_any().downcast_ref::<CpdfArray>()?;
    Some(
        (0..range.size())
            .map(|index| range.get_integer_at(index))
            .collect(),
    )
}
pub fn sub_filter(signature: &ObjectRef) -> Option<Vec<u8>> {
    let value = value(signature)?;
    let borrowed = value.borrow();
    let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
    dict.key_exist(b"SubFilter")
        .then(|| dict.get_name_for(b"SubFilter"))
}
pub fn reason(signature: &ObjectRef) -> Option<Vec<u32>> {
    let reason = dict_entry(&value(signature)?, b"Reason")?;
    let borrowed = reason.borrow();
    (borrowed.object_type() == ObjectType::String).then(|| borrowed.get_unicode_text())
}
pub fn time(signature: &ObjectRef) -> Option<Vec<u8>> {
    let time = dict_entry(&value(signature)?, b"M")?;
    let borrowed = time.borrow();
    (borrowed.object_type() == ObjectType::String).then(|| borrowed.get_string())
}
pub fn doc_mdp_permission(signature: &ObjectRef) -> u32 {
    let Some(references) = value(signature).and_then(|value| dict_entry(&value, b"Reference"))
    else {
        return 0;
    };
    let references = {
        let borrowed = references.borrow();
        let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
            return 0;
        };
        (0..array.size())
            .filter_map(|index| array.get_dict_at(index))
            .collect::<Vec<_>>()
    };
    for reference in references {
        let method = reference
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_name_for(b"TransformMethod");
        if method != b"DocMDP" {
            continue;
        }
        let Some(params) = dictionary(dict_entry(&reference, b"TransformParams")) else {
            continue;
        };
        let permission = params
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_integer_for(b"P", 2);
        return if (1..=3).contains(&permission) {
            permission as u32
        } else {
            0
        };
    }
    0
}
