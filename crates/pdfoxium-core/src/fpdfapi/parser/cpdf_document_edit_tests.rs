// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Source-derived page-tree mutation regressions for CPDF_Document.
use super::*;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;

fn fresh() -> CpdfDocument {
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    doc
}
fn count(node: &ObjectRef) -> i32 {
    with_dict(node, |dict| dict.get_integer_for(b"Count", -1)).unwrap()
}
fn kids(node: &ObjectRef) -> ObjectRef {
    with_dict(node, |dict| dict.get_array_for(b"Kids"))
        .flatten()
        .unwrap()
}
fn set_children(doc: &CpdfDocument, node: &ObjectRef, children: &[ObjectRef], count: i32) {
    let mut array = CpdfArray::new();
    for child in children {
        array.append_new(CpdfReference::new(&doc.holder(), child.borrow().obj_num()));
        with_dict_mut(child, |dict| {
            dict.set_new_for(
                b"Parent",
                CpdfReference::new(&doc.holder(), node.borrow().obj_num()),
            );
        });
    }
    with_dict_mut(node, |dict| {
        dict.set_new_for(b"Kids", array);
        dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(count));
    });
}
fn page_numbers(doc: &mut CpdfDocument) -> Vec<u32> {
    (0..doc.page_count())
        .map(|index| doc.page_dictionary(index).unwrap().borrow().obj_num())
        .collect()
}

#[test]
fn rejected_page_creation_discards_the_object_but_reserves_its_number() {
    let mut doc = CpdfDocument::new();
    assert!(doc.create_new_page(0).is_none());
    assert_eq!(doc.holder().borrow().last_obj_num(), 1);
    assert!(doc.holder().borrow().get_indirect_object(1).is_none());
    doc.create_new_doc();
    for index in [-1, 1, i32::MAX] {
        let last = doc.holder().borrow().last_obj_num();
        assert!(doc.create_new_page(index).is_none());
        assert_eq!(doc.holder().borrow().last_obj_num(), last + 1);
        assert!(doc
            .holder()
            .borrow()
            .get_indirect_object(last + 1)
            .is_none());
        assert_eq!(doc.page_count(), 0);
    }
    assert_eq!(doc.delete_page(-1), 0);
    assert_eq!(doc.delete_page(0), 0);
}

#[test]
fn flat_insertions_and_deletions_keep_page_cache_parent_and_counts_in_sync() {
    let mut doc = fresh();
    let a = doc.create_new_page(0).unwrap();
    let c = doc.create_new_page(1).unwrap();
    let b = doc.create_new_page(1).unwrap();
    let first = doc.create_new_page(0).unwrap();
    let pages = doc.pages_dict().unwrap();
    let expected = [first.clone(), a.clone(), b.clone(), c.clone()];
    assert_eq!(
        page_numbers(&mut doc),
        expected
            .iter()
            .map(|p| p.borrow().obj_num())
            .collect::<Vec<_>>()
    );
    assert_eq!(count(&pages), 4);
    assert_eq!(with_array(&kids(&pages), CpdfArray::size), Some(4));
    for page in &expected {
        let parent = with_dict(page, |dict| dict.get_dict_for(b"Parent"))
            .flatten()
            .unwrap();
        assert!(Rc::ptr_eq(&parent, &pages));
    }
    // Force a traversal before deleting, so cached traversal also needs reset.
    doc.set_page_obj_num(2, 0);
    assert!(Rc::ptr_eq(&doc.page_dictionary(2).unwrap(), &b));
    assert_eq!(doc.delete_page(1), a.borrow().obj_num());
    assert_eq!(count(&pages), 3);
    assert_eq!(
        page_numbers(&mut doc),
        [first, b, c]
            .iter()
            .map(|p| p.borrow().obj_num())
            .collect::<Vec<_>>()
    );
    assert!(doc
        .holder()
        .borrow()
        .get_indirect_object(a.borrow().obj_num())
        .is_some());
    assert_eq!(doc.delete_page(3), 0);
    assert_eq!(doc.page_count(), 3);
}

#[test]
fn nested_mutation_updates_each_ancestor_and_repairs_missing_node_types() {
    let mut doc = fresh();
    let a = doc.create_new_page(0).unwrap();
    let b = doc.create_new_page(1).unwrap();
    let c = doc.create_new_page(2).unwrap();
    let branch = doc.holder().borrow_mut().new_indirect_dictionary();
    set_children(&doc, &branch, &[a.clone(), b.clone()], 2);
    let pages = doc.pages_dict().unwrap();
    set_children(&doc, &pages, &[branch.clone(), c.clone()], 3);
    doc.load_pages();
    let inserted = doc.create_new_page(1).unwrap();
    assert_eq!(count(&pages), 4);
    assert_eq!(count(&branch), 3);
    assert_eq!(
        with_dict(&branch, |dict| dict.get_name_for(b"Type")),
        Some(b"Pages".to_vec())
    );
    let parent = with_dict(&inserted, |dict| dict.get_dict_for(b"Parent"))
        .flatten()
        .unwrap();
    assert!(Rc::ptr_eq(&parent, &branch));
    assert_eq!(
        page_numbers(&mut doc),
        [a.clone(), inserted.clone(), b.clone(), c.clone()]
            .iter()
            .map(|p| p.borrow().obj_num())
            .collect::<Vec<_>>()
    );
    assert_eq!(doc.delete_page(2), b.borrow().obj_num());
    assert_eq!(count(&branch), 2);
    assert_eq!(count(&pages), 3);
    assert_eq!(with_array(&kids(&branch), CpdfArray::size), Some(2));
    assert_eq!(
        page_numbers(&mut doc),
        [a, inserted, c]
            .iter()
            .map(|p| p.borrow().obj_num())
            .collect::<Vec<_>>()
    );
}

#[test]
fn insertion_cycle_failure_keeps_the_existing_tree_and_page_cache() {
    let mut doc = fresh();
    let old = doc.create_new_page(0).unwrap();
    let pages = doc.pages_dict().unwrap();
    let mut array = CpdfArray::new();
    array.append_new(CpdfReference::new(&doc.holder(), pages.borrow().obj_num()));
    with_dict_mut(&pages, |dict| {
        dict.set_new_for(b"Kids", array);
    });
    let last = doc.holder().borrow().last_obj_num();
    assert!(doc.create_new_page(0).is_none());
    assert_eq!(doc.page_count(), 1);
    assert_eq!(count(&pages), 1);
    assert_eq!(doc.page_list, [old.borrow().obj_num()]);
    assert!(doc
        .holder()
        .borrow()
        .get_indirect_object(last + 1)
        .is_none());
}

#[test]
fn null_replacement_waits_until_no_remaining_page_uses_the_object() {
    let mut doc = fresh();
    let a = doc.create_new_page(0).unwrap();
    let b = doc.create_new_page(1).unwrap();
    assert!(doc.insert_new_page(1, &a));
    let number = a.borrow().obj_num();
    assert_eq!(doc.delete_page(0), number);
    // The page still appears once in the tree. Fill all lazy cache entries.
    doc.set_page_obj_num(0, 0);
    doc.set_page_to_null_object(number);
    assert_eq!(
        doc.holder()
            .borrow()
            .get_indirect_object(number)
            .unwrap()
            .borrow()
            .object_type(),
        ObjectType::Dictionary
    );
    assert_eq!(doc.delete_page(0), number);
    doc.set_page_to_null_object(number);
    assert_eq!(
        doc.holder()
            .borrow()
            .get_indirect_object(number)
            .unwrap()
            .borrow()
            .object_type(),
        ObjectType::Null
    );
    // External retained dictionary handles survive replacing the holder entry.
    assert_eq!(a.borrow().object_type(), ObjectType::Dictionary);
    let last = b.borrow().obj_num();
    assert_eq!(doc.delete_page(0), last);
    doc.set_page_to_null_object(last);
    assert_eq!(
        doc.holder()
            .borrow()
            .get_indirect_object(last)
            .unwrap()
            .borrow()
            .object_type(),
        ObjectType::Dictionary
    );
    doc.set_page_to_null_object(0);
}

#[test]
fn malformed_tree_missing_kids_fails_but_empty_kids_retains_native_success() {
    let mut doc = fresh();
    doc.create_new_page(0).unwrap();
    let pages = doc.pages_dict().unwrap();
    with_dict_mut(&pages, |dict| {
        dict.remove_for(b"Kids");
    });
    assert!(doc.create_new_page(0).is_none());
    assert_eq!(doc.page_count(), 1);
    with_dict_mut(&pages, |dict| {
        dict.set_for(b"Kids", Some(retain_object(CpdfArray::new())));
    });
    let page = doc.create_new_page(0).unwrap();
    // InsertDeletePDFPage returns true when its loop selects no child.
    assert_eq!(doc.page_count(), 2);
    assert_eq!(count(&pages), 1);
    assert_eq!(with_array(&kids(&pages), CpdfArray::size), Some(0));
    assert!(!with_dict(&page, |dict| dict.key_exist(b"Parent")).unwrap());
}

#[test]
fn insertion_resets_loaded_document_traversal_without_discarding_existing_pages() {
    let input = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
    ));
    let mut doc = CpdfDocument::new();
    assert_eq!(
        doc.load_doc(
            Rc::new(crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream::new(input)),
            b""
        ),
        ParserError::Success
    );
    let old_first = doc.page_dictionary(0).unwrap().borrow().obj_num();
    let new = doc.create_new_page(0).unwrap().borrow().obj_num();
    let old_last = doc.page_dictionary(2).unwrap().borrow().obj_num();
    assert_eq!([old_first, old_last], [3, 4]);
    assert_eq!(page_numbers(&mut doc), [new, old_first, old_last]);
    assert_eq!(doc.delete_page(1), old_first);
    assert_eq!(page_numbers(&mut doc), [new, old_last]);
}

#[test]
fn appending_replaces_a_wrong_typed_self_referencing_kids_value() {
    let mut doc = fresh();
    let pages = doc.pages_dict().unwrap();
    let number = pages.borrow().obj_num();
    with_dict_mut(&pages, |dict| {
        dict.set_new_for(b"Kids", CpdfReference::new(&doc.holder(), number));
    });
    let page = doc.create_new_page(0).unwrap();
    let children = kids(&pages);
    assert_eq!(with_array(&children, CpdfArray::size), Some(1));
    assert!(Rc::ptr_eq(
        &with_array(&children, |array| array.get_dict_at(0))
            .flatten()
            .unwrap(),
        &page
    ));
    assert_eq!(count(&pages), 1);
    assert_eq!(doc.page_count(), 1);
}
