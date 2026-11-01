// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! `fpdfsdk/fpdf_save.cpp::SaveXFADocumentData`. PDF streams retain their
//! memory-stream owners directly, replacing the native temporary file list.
use super::fpdfxfa::cpdfxfa_context::CpdfXfaContext;
use crate::{
    fpdfapi::parser::{
        cpdf_array::CpdfArray,
        cpdf_dictionary::CpdfDictionary,
        cpdf_indirect_object_holder::HolderRef,
        cpdf_object::{retain_object, ObjectRef},
        cpdf_reference::CpdfReference,
        cpdf_scalar_object::ObjectType,
        cpdf_stream::CpdfStream,
    },
    fxcrt::cfx_memorystream::CfxMemoryStream,
};
use std::rc::Rc;

pub fn save_xfa_document_data(context: Option<&CpdfXfaContext>) -> bool {
    let Some(context) = context else {
        return false;
    };
    if !context.contains_extension_form() {
        return true;
    }
    let Some(document) = context.pdf_document() else {
        return false;
    };
    let (root, holder) = {
        let document = document.borrow();
        (document.root(), document.holder())
    };
    let Some(root) = root else {
        return false;
    };
    let Some(acro) = root
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .and_then(|root| root.get_dict_for(b"AcroForm"))
    else {
        return false;
    };
    let xfa = acro
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .and_then(|acro| acro.get_object_for(b"XFA"));
    let Some(xfa) = xfa else {
        return true;
    };
    if xfa.borrow().object_type() != ObjectType::Array {
        return false;
    }
    // No PDF/array borrow spans package serialization or the script-capable
    // document services it may invoke.
    let mut datasets = CfxMemoryStream::new();
    if context.save_datasets_package(&mut datasets) && !datasets.get_span().is_empty() {
        replace_package(&xfa, &holder, b"datasets", datasets);
    }
    let mut form = CfxMemoryStream::new();
    if context.save_form_package(&mut form) && !form.get_span().is_empty() {
        replace_package(&xfa, &holder, b"form", form);
    }
    true
}
fn replace_package(array: &ObjectRef, holder: &HolderRef, name: &[u8], data: CfxMemoryStream) {
    // Locate each package against the current array after prior insertion;
    // no saved numeric index is allowed to address a different packet.
    let (index, existing) = {
        let object = array.borrow();
        let Some(array) = object.as_any().downcast_ref::<CpdfArray>() else {
            return;
        };
        let index = (0..array.size().saturating_sub(1))
            .rev()
            .find(|&index| {
                array.get_object_at(index).is_some_and(|object| {
                    let object = object.borrow();
                    object.object_type() == ObjectType::String && object.get_string() == name
                })
            })
            .map(|index| index + 1);
        (index, index.and_then(|index| array.get_stream_at(index)))
    };
    let data = Rc::new(data);
    if index.is_some() {
        if let Some(existing) = existing {
            if let Some(stream) = existing
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfStream>()
            {
                stream.init_stream_from_file(data);
            }
        }
        // A present malformed packet is left intact, matching native behavior.
        return;
    }
    let stream = retain_object(CpdfStream::from_file(
        data,
        retain_object(CpdfDictionary::new()),
    ));
    let number = holder.borrow_mut().add_indirect_object(stream);
    let mut object = array.borrow_mut();
    let Some(array) = object.as_any_mut().downcast_mut::<CpdfArray>() else {
        return;
    };
    // A well-formed packet array ends with its postamble name/stream pair.
    // Saturation also safely handles an empty or truncated packet array.
    let insertion = array.size().saturating_sub(2);
    array.insert_new_string_at(insertion, name);
    array.insert_new_at(insertion + 1, CpdfReference::new(holder, number));
}
#[cfg(test)]
#[path = "fpdf_xfa_save_tests.rs"]
mod tests;
