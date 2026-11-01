// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Source-derived organizer contracts; upstream has no dedicated unit suite.
use super::super::cpdf_pageexporter::CpdfPageExporter;
use super::*;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfRetainedStreamAcc;
use crate::fxcrt::fx_coordinates::CfxFloatRect;
fn new_doc() -> Rc<RefCell<CpdfDocument>> {
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    Rc::new(RefCell::new(doc))
}
fn add_page(doc: &Rc<RefCell<CpdfDocument>>, index: i32) -> ObjectRef {
    doc.borrow_mut().create_new_page(index).unwrap()
}
fn name(object: &ObjectRef, key: &[u8]) -> Vec<u8> {
    dict(object, |d| d.get_name_for(key))
}

#[test]
fn shared_resources_content_and_cycles_are_rebound_once_and_survive_source_drop() {
    let src = new_doc();
    let holder = src.borrow().holder();
    let page0 = add_page(&src, 0);
    let page1 = add_page(&src, 1);
    let root = src.borrow().root().unwrap();
    let parent = dict(&root, |d| d.get_dict_for(b"Pages")).unwrap();
    let resources = holder.borrow_mut().new_indirect_dictionary();
    let resource_num = resources.borrow().obj_num();
    dict_mut(&resources, |d| {
        d.set_new_name_for(b"Marker", b"Shared");
        d.set_new_for(b"Cycle", CpdfReference::new(&holder, resource_num));
    });
    let content = holder
        .borrow_mut()
        .new_indirect_stream_from_bytes(b"1 0 0 rg 10 10 20 20 re f");
    let content_num = content.borrow().obj_num();
    dict_mut(&parent, |d| {
        d.set_rect_for(b"MediaBox", CfxFloatRect::new(0., 0., 300., 500.));
        d.set_rect_for(b"CropBox", CfxFloatRect::new(10., 20., 250., 450.));
        d.set_new_for(b"Rotate", CpdfScalarObject::number_from_signed(90));
        d.set_new_for(b"Resources", CpdfReference::new(&holder, resource_num));
    });
    for page in [&page0, &page1] {
        dict_mut(page, |d| {
            d.set_new_for(b"Contents", CpdfReference::new(&holder, content_num));
        });
    }
    let dest = new_doc();
    assert!(CpdfPageExporter::new(Rc::clone(&dest), Rc::clone(&src)).export_pages(&[0, 1, 0], 0));
    let pages: Vec<_> = (0..3)
        .map(|i| dest.borrow_mut().page_dictionary(i).unwrap())
        .collect();
    let copied = dict(&pages[0], |d| d.get_dict_for(b"Resources")).unwrap();
    assert!(!Rc::ptr_eq(&resources, &copied));
    assert!(Rc::ptr_eq(
        &copied,
        &dict(&copied, |d| d.get_dict_for(b"Cycle")).unwrap()
    ));
    let copied_content = dict(&pages[0], |d| d.get_stream_for(b"Contents")).unwrap();
    for page in &pages {
        assert!(Rc::ptr_eq(
            &copied,
            &dict(page, |d| d.get_dict_for(b"Resources")).unwrap()
        ));
        assert!(Rc::ptr_eq(
            &copied_content,
            &dict(page, |d| d.get_stream_for(b"Contents")).unwrap()
        ));
        assert_eq!(
            dict(page, |d| d.get_rect_for(b"MediaBox")),
            CfxFloatRect::new(0., 0., 300., 500.)
        );
        assert_eq!(
            dict(page, |d| d.get_rect_for(b"CropBox")),
            CfxFloatRect::new(10., 20., 250., 450.)
        );
        assert_eq!(dict(page, |d| d.get_integer_for(b"Rotate", 0)), 90);
    }
    drop(holder);
    drop(src);
    assert_eq!(name(&copied, b"Marker"), b"Shared");
    assert_eq!(
        &*CpdfRetainedStreamAcc::load_filtered(copied_content)
            .unwrap()
            .get_span(),
        b"1 0 0 rg 10 10 20 20 re f"
    );
}

#[test]
fn defaults_and_failed_lookup_preserve_native_mutation_order() {
    let src = new_doc();
    let page = add_page(&src, 0);
    let dest = new_doc();
    assert!(!CpdfPageExporter::new(Rc::clone(&dest), Rc::clone(&src)).export_pages(&[0, 99], 0));
    // Export creates the failing destination page before looking up source 99.
    assert_eq!(dest.borrow().page_count(), 2);
    let copied = dest.borrow_mut().page_dictionary(0).unwrap();
    assert_eq!(
        dict(&copied, |d| d.get_rect_for(b"MediaBox")),
        CfxFloatRect::new(0., 0., 612., 792.)
    );
    assert_eq!(
        dict(&copied, |d| d.get_dict_for(b"Resources"))
            .unwrap()
            .borrow()
            .object_type(),
        ObjectType::Dictionary
    );
    dict_mut(&page, |d| {
        d.set_rect_for(b"CropBox", CfxFloatRect::new(1., 2., 30., 40.))
    });
    assert!(CpdfPageExporter::new(Rc::clone(&dest), src).export_pages(&[0], 2));
    let copied = dest.borrow_mut().page_dictionary(2).unwrap();
    assert_eq!(
        dict(&copied, |d| d.get_rect_for(b"MediaBox")),
        CfxFloatRect::new(1., 2., 30., 40.)
    );
}

#[test]
fn inheritance_requires_page_name_and_resolvable_parent_and_terminates_cycles() {
    let holder = CpdfIndirectObjectHolder::new();
    let page = holder.borrow_mut().new_indirect_dictionary();
    let parent = holder.borrow_mut().new_indirect_dictionary();
    dict_mut(&page, |d| {
        d.set_new_name_for(b"Type", b"Page");
        d.set_new_for(b"MediaBox", CpdfArray::new());
    });
    assert!(CpdfPageOrganizer::page_dict_get_inheritable_tag(&page, b"MediaBox").is_none());
    dict_mut(&page, |d| {
        d.set_new_for(
            b"Parent",
            CpdfReference::new(&holder, parent.borrow().obj_num()),
        );
    });
    assert!(CpdfPageOrganizer::page_dict_get_inheritable_tag(&page, b"MediaBox").is_some());
    let stream_parent = holder.borrow_mut().new_indirect_stream_from_bytes(b"");
    dict_mut(&page, |d| {
        d.set_new_for(
            b"Parent",
            CpdfReference::new(&holder, stream_parent.borrow().obj_num()),
        );
    });
    assert!(CpdfPageOrganizer::page_dict_get_inheritable_tag(&page, b"MediaBox").is_none());
    dict_mut(&page, |d| {
        d.set_new_for(
            b"Parent",
            CpdfReference::new(&holder, parent.borrow().obj_num()),
        );
    });
    dict_mut(&parent, |d| {
        d.set_new_for(
            b"Parent",
            CpdfReference::new(&holder, page.borrow().obj_num()),
        );
    });
    assert!(CpdfPageOrganizer::page_dict_get_inheritable_tag(&page, b"Missing").is_none());
    dict_mut(&page, |d| {
        d.set_new_for(
            b"Type",
            super::super::super::parser::cpdf_string::CpdfString::from_bytes(None, b"Page"),
        );
    });
    assert!(CpdfPageOrganizer::page_dict_get_inheritable_tag(&page, b"MediaBox").is_none());
}

#[test]
fn reference_failures_remove_dictionary_keys_stop_arrays_and_keep_allocated_mapping() {
    let src = new_doc();
    let dest = new_doc();
    let holder = src.borrow().holder();
    let source_array = holder.borrow().new_array();
    let mut array = source_array.borrow_mut();
    array
        .as_any_mut()
        .downcast_mut::<CpdfArray>()
        .unwrap()
        .append_new(CpdfReference::new(&holder, 999));
    drop(array);
    let num = holder.borrow_mut().add_indirect_object(source_array);
    let object = retain_object(CpdfDictionary::new());
    dict_mut(&object, |d| {
        for key in [b"A", b"B"] {
            d.set_new_for(key, CpdfReference::new(&holder, num));
        }
        for key in [b"Parent".as_slice(), b"Prev", b"First"] {
            d.set_new_for(key, CpdfReference::new(&holder, 998));
        }
    });
    let mut organizer = CpdfPageOrganizer::new(Rc::clone(&dest), src);
    assert!(organizer.update_reference(&object));
    assert!(!dict(&object, |d| d.key_exist(b"A")));
    assert!(dict(&object, |d| d.key_exist(b"B")));
    assert_eq!(
        dict(&object, |d| d.get_array_for(b"B"))
            .unwrap()
            .borrow()
            .object_type(),
        ObjectType::Array
    );
    // A non-page Parent is a source reference, not an already-rewritten page
    // tree link. Invalid annotation parents are discarded like other keys.
    assert!(!dict(&object, |d| d.key_exist(b"Parent")));
    for key in [b"Prev".as_slice(), b"First"] {
        let value = dict(&object, |d| d.get_object_for(key)).unwrap();
        assert_eq!(
            value
                .borrow()
                .as_any()
                .downcast_ref::<CpdfReference>()
                .unwrap()
                .ref_obj_num(),
            998
        );
    }
    organizer.clear_object_number_map();
    assert!(organizer.object_number_map.is_empty());
}

#[test]
fn unmapped_page_references_fail_and_pages_references_use_destination_tree() {
    let src = new_doc();
    let dest = new_doc();
    let holder = src.borrow().holder();
    let page = add_page(&src, 0);
    let num = page.borrow().obj_num();
    let parent = dict(&page, |d| d.get_object_for(b"Parent")).unwrap();
    let root = dest.borrow().root().unwrap();
    let dest_pages = dict(&root, |d| d.get_dict_for(b"Pages")).unwrap();
    let dest_pages_number = dest_pages.borrow().obj_num();
    assert_ne!(dest_pages_number, 4);
    let mut organizer = CpdfPageOrganizer::new(dest, src);
    let reference = retain_object(CpdfReference::new(&holder, num));
    assert!(!organizer.update_reference(&reference));
    let parent = parent.borrow().clone_object();
    assert!(organizer.update_reference(&parent));
    assert_eq!(
        parent
            .borrow()
            .as_any()
            .downcast_ref::<CpdfReference>()
            .unwrap()
            .ref_obj_num(),
        dest_pages_number
    );
    organizer.add_object_mapping(num, 18);
    assert!(organizer.update_reference(&reference));
    assert_eq!(
        reference
            .borrow()
            .as_any()
            .downcast_ref::<CpdfReference>()
            .unwrap()
            .ref_obj_num(),
        18
    );
}

#[test]
fn destination_initialization_repairs_missing_catalog_pages_kids_and_keeps_source_order() {
    let dest = new_doc();
    let root = dest.borrow().root().unwrap();
    dict_mut(&root, |d| {
        d.remove_for(b"Type");
        d.set_new_for(b"Pages", CpdfScalarObject::null());
    });
    let src = new_doc();
    let organizer = CpdfPageOrganizer::new(Rc::clone(&dest), src);
    assert!(organizer.init());
    assert_eq!(name(&root, b"Type"), b"Catalog");
    let pages = dict(&root, |d| d.get_dict_for(b"Pages")).unwrap();
    assert_eq!(name(&pages, b"Type"), b"Pages");
    assert_eq!(dict(&pages, |d| d.get_integer_for(b"Count", -1)), 0);
    let kids = dict(&pages, |d| d.get_array_for(b"Kids")).unwrap();
    assert!(!kids.borrow().is_inline());
    assert_eq!(
        kids.borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .size(),
        0
    );
    let info = dest.borrow_mut().info().unwrap();
    assert_eq!(
        dict(&info, |d| d.get_byte_string_for(b"Producer", b"")),
        b"PDFium"
    );
    let last = dest.borrow().holder().borrow().last_obj_num();
    assert!(organizer.init());
    assert_eq!(dest.borrow().holder().borrow().last_obj_num(), last);
    let holder = dest.borrow().holder();
    let stream = retain_object(CpdfStream::from_bytes(b""));
    let stream_number = holder.borrow_mut().add_indirect_object(stream);
    dict_mut(&root, |d| {
        d.set_new_for(b"Pages", CpdfReference::new(&holder, stream_number));
    });
    assert!(organizer.init());
    let replacement = dict(&root, |d| d.get_object_for(b"Pages")).unwrap();
    assert_eq!(replacement.borrow().object_type(), ObjectType::Reference);
    assert_eq!(
        get_direct_object(&replacement)
            .unwrap()
            .borrow()
            .object_type(),
        ObjectType::Dictionary
    );
    let invalid = Rc::new(RefCell::new(CpdfDocument::new()));
    assert!(!CpdfPageOrganizer::new(invalid, Rc::clone(&dest)).init());
}

#[test]
fn imported_popup_parent_cycle_rebinds_to_the_link_after_the_source_is_dropped() {
    let source = new_doc();
    let holder = source.borrow().holder();
    let weak_holder = Rc::downgrade(&holder);
    let page = add_page(&source, 0);
    let pages = dict(&page, |d| d.get_dict_for(b"Parent")).unwrap();
    let link = holder.borrow_mut().new_indirect_dictionary();
    let popup = holder.borrow_mut().new_indirect_dictionary();
    dict_mut(&link, |d| {
        d.set_new_name_for(b"Type", b"Annot");
        d.set_new_name_for(b"Subtype", b"Link");
        d.set_new_for(
            b"Popup",
            CpdfReference::new(&holder, popup.borrow().obj_num()),
        );
        d.set_new_for(b"P", CpdfReference::new(&holder, page.borrow().obj_num()));
        d.set_new_for(
            b"Tree",
            CpdfReference::new(&holder, pages.borrow().obj_num()),
        );
    });
    dict_mut(&popup, |d| {
        d.set_new_name_for(b"Type", b"Annot");
        d.set_new_name_for(b"Subtype", b"Popup");
        d.set_new_for(
            b"Parent",
            CpdfReference::new(&holder, link.borrow().obj_num()),
        );
    });
    let mut annotations = CpdfArray::new();
    annotations.append_new(CpdfReference::new(&holder, link.borrow().obj_num()));
    dict_mut(&page, |d| {
        d.set_new_for(b"Annots", annotations);
    });
    let target = new_doc();
    assert!(CpdfPageExporter::new(Rc::clone(&target), Rc::clone(&source)).export_pages(&[0], 0));
    drop(holder);
    drop(source);
    assert!(weak_holder.upgrade().is_none());
    let imported_page = target.borrow_mut().page_dictionary(0).unwrap();
    let annotations = dict(&imported_page, |d| d.get_array_for(b"Annots")).unwrap();
    let imported_link = annotations
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .unwrap()
        .get_dict_at(0)
        .unwrap();
    let imported_popup = dict(&imported_link, |d| d.get_dict_for(b"Popup")).unwrap();
    let back_reference = dict(&imported_popup, |d| d.get_dict_for(b"Parent")).unwrap();
    assert!(Rc::ptr_eq(&back_reference, &imported_link));
    assert!(Rc::ptr_eq(
        &dict(&imported_link, |d| d.get_dict_for(b"P")).unwrap(),
        &imported_page
    ));
    let destination_pages = dict(&imported_page, |d| d.get_dict_for(b"Parent")).unwrap();
    assert!(Rc::ptr_eq(
        &dict(&imported_link, |d| d.get_dict_for(b"Tree")).unwrap(),
        &destination_pages
    ));
    assert_eq!(name(&imported_popup, b"Subtype"), b"Popup");
}
