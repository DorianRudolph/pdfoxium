// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Ordinary-PDF import and viewer preferences from `fpdfsdk/fpdf_ppo.cpp`.
//! N-up/XObject APIs and XFA extension notifications remain pending.
use super::cpdfsdk_helpers::parse_page_range_string;
use crate::fpdfapi::edit::cpdf_pageexporter::CpdfPageExporter;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use std::cell::RefCell;
use std::rc::Rc;

pub fn import_pages_by_index(
    dest: Rc<RefCell<CpdfDocument>>,
    src: Rc<RefCell<CpdfDocument>>,
    indices: Option<&[i32]>,
    index: i32,
) -> bool {
    let mut exporter = CpdfPageExporter::new(Rc::clone(&dest), Rc::clone(&src));
    let indices = match indices {
        None => (0..src.borrow().page_count() as u32).collect(),
        Some(indices) => {
            if indices.is_empty() || indices.iter().any(|&page| page < 0) {
                return false;
            }
            indices.iter().map(|&page| page as u32).collect::<Vec<_>>()
        }
    };
    exporter.export_pages(&indices, index)
}

pub fn import_pages(
    dest: Rc<RefCell<CpdfDocument>>,
    src: Rc<RefCell<CpdfDocument>>,
    range: &[u8],
    index: i32,
) -> bool {
    let count = src.borrow().page_count() as u32;
    let indices: Vec<_> = if range.is_empty() {
        (0..count).collect()
    } else {
        parse_page_range_string(range, count)
    };
    if indices.is_empty() {
        return false;
    }
    CpdfPageExporter::new(dest, src).export_pages(&indices, index)
}

fn valid_viewer_preferences_object(object: &ObjectRef) -> bool {
    let borrowed = object.borrow();
    match borrowed.object_type() {
        ObjectType::Dictionary | ObjectType::Null | ObjectType::Reference | ObjectType::Stream => {
            false
        }
        ObjectType::Array => borrowed
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .with_locked_objects(|items| {
                items.iter().all(|item| {
                    !matches!(
                        item.borrow().object_type(),
                        ObjectType::Array
                            | ObjectType::Dictionary
                            | ObjectType::Reference
                            | ObjectType::Stream
                    )
                })
            }),
        _ => true,
    }
}

pub fn copy_viewer_preferences(
    dest: &Rc<RefCell<CpdfDocument>>,
    src: &Rc<RefCell<CpdfDocument>>,
) -> bool {
    let root = src.borrow().root().expect("source catalog");
    let preferences = root
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .get_dict_for(b"ViewerPreferences");
    let Some(preferences) = preferences else {
        return false;
    };
    let Some(dest_root) = dest.borrow().root() else {
        return false;
    };
    // New<Dictionary> uses the destination document's byte-string pool.
    let holder = dest.borrow().holder();
    let mut cloned = CpdfDictionary::with_pool(holder.borrow().byte_string_pool());
    {
        let borrowed = preferences.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .with_locked_entries(|entries| {
                for (key, value) in entries {
                    if valid_viewer_preferences_object(value) {
                        cloned.set_for(key, Some(value.borrow().clone_object()));
                    }
                }
            });
    }
    dest_root
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(b"ViewerPreferences", Some(retain_object(cloned)));
    true
}

#[cfg(test)]
#[path = "fpdf_ppo_embeddertest.rs"]
mod embedder_tests;

/// Remaining ordinary-PDF `fpdf_ppo.cpp` paths; document creation happens first
/// at the SDK/host boundary so CreationDate and sandbox policy stay consistent.
pub fn import_n_pages_to_one(
    dest: Rc<RefCell<CpdfDocument>>,
    src: Rc<RefCell<CpdfDocument>>,
    width: f32,
    height: f32,
    x: usize,
    y: usize,
) -> bool {
    let indices: Vec<_> = (0..src.borrow().page_count() as u32).collect();
    if indices.is_empty() {
        return false;
    }
    if x == 1 && y == 1 {
        return CpdfPageExporter::new(dest, src).export_pages(&indices, 0);
    }
    crate::fpdfapi::edit::cpdf_npagetooneexporter::CpdfNPageToOneExporter::new(dest, src)
        .export_n_pages_to_one(
            &indices,
            crate::fxcrt::fx_coordinates::CfxSizeF::new(width, height),
            x,
            y,
        )
}

pub fn new_form_object_from_xobject(
    context: &crate::fpdfapi::edit::cpdf_npagetooneexporter::XObjectContext,
) -> crate::fpdfapi::page::cpdf_formobject::CpdfFormObject {
    use crate::fpdfapi::page::{
        cpdf_form::CpdfForm, cpdf_formobject::CpdfFormObject, cpdf_pageobject::CpdfPageObject,
    };
    let document = context
        .dest_doc
        .upgrade()
        .expect("XObject must close before its destination");
    let page_data = document.borrow().page_data_ref();
    let mut form = CpdfForm::new(Rc::clone(&context.xobject), None, None).expect("XObject stream");
    form.parse_content(
        None,
        None,
        page_data,
        Rc::new(RefCell::new(Default::default())),
        0,
    );
    CpdfFormObject::new(CpdfPageObject::NO_CONTENT_STREAM, form, Default::default())
}

/// Import an owned PDF object graph, rebinding indirect references with the same
/// cycle-aware mapping used by page import. The source object is never mutated.
/// This is also used for isolated form streams exported by safe Rust adapters.
pub fn import_object(
    destination: Rc<RefCell<CpdfDocument>>,
    source: Rc<RefCell<CpdfDocument>>,
    object: &crate::fpdfapi::parser::cpdf_object::ObjectRef,
) -> Option<crate::fpdfapi::parser::cpdf_object::ObjectRef> {
    use crate::fpdfapi::{
        edit::cpdf_pageorganizer::CpdfPageOrganizer, parser::cpdf_object::get_direct_object,
    };
    let object = get_direct_object(object)?;
    let old_number = object.borrow().obj_num();
    let cloned = object.borrow().clone_object();
    let number = destination
        .borrow()
        .holder()
        .borrow_mut()
        .add_indirect_object(cloned.clone());
    let mut organizer = CpdfPageOrganizer::for_object_graph(destination, source);
    if old_number != 0 {
        organizer.add_object_mapping(old_number, number);
    }
    organizer.update_reference(&cloned).then_some(cloned)
}

#[cfg(test)]
mod import_object_tests {
    use super::*;
    use crate::fpdfapi::parser::{cpdf_dictionary::CpdfDictionary, cpdf_object::make_reference};

    #[test]
    fn imports_shared_and_cyclic_references_without_retaining_source() {
        let source = super::super::fpdf_editpage::create_new_document(false, || None);
        let destination = super::super::fpdf_editpage::create_new_document(false, || None);
        let holder = source.borrow().holder();
        let parent = holder.borrow_mut().new_indirect_dictionary();
        let child = holder.borrow_mut().new_indirect_dictionary();
        {
            let mut object = parent.borrow_mut();
            let dict = object
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_for(b"First", Some(make_reference(&child, &holder)));
            dict.set_for(b"Second", Some(make_reference(&child, &holder)));
        }
        {
            let mut object = child.borrow_mut();
            let dict = object
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_for(b"Parent", Some(make_reference(&parent, &holder)));
            dict.set_new_name_for(b"Name", b"retained graph");
        }
        let imported = import_object(destination.clone(), source.clone(), &parent).unwrap();
        let source_weak = Rc::downgrade(&holder);
        drop(parent);
        drop(child);
        drop(holder);
        drop(source);
        assert!(source_weak.upgrade().is_none());
        let dict = imported.borrow();
        let dict = dict.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        let first = dict.get_dict_for(b"First").unwrap();
        let second = dict.get_dict_for(b"Second").unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        let child = first.borrow();
        let child = child.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        assert_eq!(child.get_name_for(b"Name"), b"retained graph");
        assert!(Rc::ptr_eq(
            &child.get_dict_for(b"Parent").unwrap(),
            &imported
        ));
    }
}
