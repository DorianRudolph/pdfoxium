// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Source-derived content-manager contracts. The removal/remap assertions
//! correspond to the manager portion of FPDFEditEmbedderTest.RemoveAllFromStream
//! and RemoveAllFromSingleStream; full GenerateContent/save/reload ports remain
//! separate work. No dedicated upstream PageContentManager unit suite exists.

use super::*;
use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use crate::fpdfapi::parser::cpdf_name::CpdfName;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

fn dictionary<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
    let object = object.borrow();
    f(object.as_any().downcast_ref::<CpdfDictionary>().unwrap())
}

fn dictionary_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut object = object.borrow_mut();
    f(object
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap())
}

fn document_with_pages(count: usize) -> (CpdfDocument, Vec<ObjectRef>) {
    let mut document = CpdfDocument::new();
    document.create_new_doc();
    let objects = document.holder();
    let pages_node = dictionary(&document.root().unwrap(), |dict| {
        dict.get_dict_for(b"Pages").unwrap()
    });
    let kids = dictionary(&pages_node, |dict| dict.get_array_for(b"Kids").unwrap());
    let mut pages = Vec::new();
    for _ in 0..count {
        let page = objects.borrow_mut().new_indirect_dictionary();
        dictionary_mut(&page, |dict| {
            dict.set_new_for(b"Type", CpdfName::new(None, b"Page"));
            dict.set_new_for(
                b"Parent",
                CpdfReference::new(&objects, pages_node.borrow().obj_num()),
            );
        });
        array_mut(&kids, |kids| {
            kids.append_new(CpdfReference::new(&objects, page.borrow().obj_num()));
        });
        pages.push(page);
    }
    dictionary_mut(&pages_node, |dict| {
        dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(count as i32));
    });
    (document, pages)
}

fn stream(document: &CpdfDocument, data: &[u8]) -> ObjectRef {
    document
        .holder()
        .borrow_mut()
        .new_indirect_stream_from_bytes(data)
}

fn contents_reference(document: &CpdfDocument, page: &ObjectRef, object: &ObjectRef) {
    dictionary_mut(page, |dict| {
        dict.set_new_for(
            b"Contents",
            CpdfReference::new(&document.holder(), object.borrow().obj_num()),
        );
    });
}

fn contents(page: &ObjectRef) -> Option<ObjectRef> {
    dictionary(page, |dict| dict.get_object_for(b"Contents"))
}

fn direct_contents(page: &ObjectRef) -> ObjectRef {
    dictionary(page, |dict| {
        dict.get_direct_object_for(b"Contents").unwrap()
    })
}

fn bytes(stream: &ObjectRef) -> Vec<u8> {
    stream
        .borrow()
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .get_in_memory_raw_data()
        .to_vec()
}

fn content_array(document: &CpdfDocument, streams: &[ObjectRef], indirect: bool) -> ObjectRef {
    let objects = document.holder();
    let array = if indirect {
        objects.borrow_mut().new_indirect_array()
    } else {
        retain_object(CpdfArray::new())
    };
    array_mut(&array, |array| {
        for stream in streams {
            array.append_new(CpdfReference::new(&objects, stream.borrow().obj_num()));
        }
    });
    array
}

fn page_holder(page: &ObjectRef) -> CpdfPageObjectHolder {
    CpdfPageObjectHolder::new(Rc::clone(page), None, None)
}

#[test]
fn appending_changes_absent_to_single_to_indirect_array_and_preserves_prefixes() {
    let (document, pages) = document_with_pages(1);
    let mut holder = page_holder(&pages[0]);
    let mut manager = CpdfPageContentManager::new(&mut holder, &document);
    assert!(!manager.has_stream_at_index(0));
    assert!(!manager.has_stream_at_index(usize::MAX));
    assert_eq!(manager.add_stream(b"first ignored", 5), 0);
    let first = direct_contents(&pages[0]);
    assert_eq!(bytes(&first), b"first");
    assert!(manager.has_stream_at_index(0));
    assert!(!manager.has_stream_at_index(1));
    assert_eq!(manager.add_stream(b"\0second", 7), 1);
    assert_eq!(manager.add_stream(b"not written", -1), 2);
    let array_object = direct_contents(&pages[0]);
    assert!(!array_object.borrow().is_inline());
    array(&array_object, |array| {
        assert_eq!(array.size(), 3);
        assert!(Rc::ptr_eq(&array.get_stream_at(0).unwrap(), &first));
        assert_eq!(bytes(&array.get_stream_at(1).unwrap()), b"\0second");
        assert!(bytes(&array.get_stream_at(2).unwrap()).is_empty());
    });
    assert!(manager.has_stream_at_index(2));
    assert!(!manager.has_stream_at_index(3));
}

#[test]
fn exclusive_stream_updates_in_place_and_removes_all_decode_parameters() {
    let (document, pages) = document_with_pages(1);
    let body = stream(&document, b"compressed");
    let stream_dict = body
        .borrow()
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .get_dict();
    dictionary_mut(&stream_dict, |dict| {
        dict.set_new_for(b"Filter", CpdfName::new(None, b"FlateDecode"));
        dict.set_new_for(b"DecodeParms", CpdfDictionary::new());
    });
    contents_reference(&document, &pages[0], &body);
    let original_reference = contents(&pages[0]).unwrap();
    let mut holder = page_holder(&pages[0]);
    {
        let mut manager = CpdfPageContentManager::new(&mut holder, &document);
        manager.update_stream(0, b"plain suffix", 5);
        assert!(Rc::ptr_eq(&manager.get_stream_by_index(0).unwrap(), &body));
    }
    assert!(Rc::ptr_eq(
        &contents(&pages[0]).unwrap(),
        &original_reference
    ));
    assert_eq!(bytes(&body), b"plain");
    dictionary(&stream_dict, |dict| {
        assert!(dict.get_object_for(b"Filter").is_none());
        assert!(dict.get_object_for(b"DecodeParms").is_none());
        assert_eq!(dict.get_integer_for(b"Length", 0), 5);
    });
}

#[test]
fn shared_single_update_keeps_cached_original_like_source_and_does_not_mutate_other_page() {
    let (document, pages) = document_with_pages(2);
    let original = stream(&document, b"shared");
    for page in &pages {
        contents_reference(&document, page, &original);
    }
    assert!(get_objects_with_multiple_references(&document).contains(&original.borrow().obj_num()));
    let mut holder = page_holder(&pages[0]);
    let mut manager = CpdfPageContentManager::new(&mut holder, &document);
    manager.update_stream(0, b"first edit", 10);
    let first_edit = direct_contents(&pages[0]);
    assert_eq!(bytes(&first_edit), b"first edit");
    assert!(!Rc::ptr_eq(&first_edit, &original));
    assert!(Rc::ptr_eq(
        &manager.get_stream_by_index(0).unwrap(),
        &original
    ));
    manager.update_stream(0, b"second edit", 11);
    let second_edit = direct_contents(&pages[0]);
    assert_eq!(bytes(&second_edit), b"second edit");
    assert!(!Rc::ptr_eq(&second_edit, &first_edit));
    assert_eq!(bytes(&original), b"shared");
    assert!(Rc::ptr_eq(&direct_contents(&pages[1]), &original));
    // AddStream still uses the cached original, not the newly written page ref.
    assert_eq!(manager.add_stream(b"tail", 4), 1);
    array(&direct_contents(&pages[0]), |array| {
        assert!(Rc::ptr_eq(&array.get_stream_at(0).unwrap(), &original));
        assert_eq!(bytes(&array.get_stream_at(1).unwrap()), b"tail");
    });
}

#[test]
fn shared_indirect_array_clones_reference_entries_before_stream_replacement() {
    let (document, pages) = document_with_pages(3);
    let original = stream(&document, b"shared body");
    let original_array = content_array(&document, &[Rc::clone(&original)], true);
    for page in &pages[..2] {
        contents_reference(&document, page, &original_array);
    }
    contents_reference(&document, &pages[2], &original);
    let original_entry = array(&original_array, |array| array.get_object_at(0).unwrap());
    let mut holder = page_holder(&pages[0]);
    {
        let mut manager = CpdfPageContentManager::new(&mut holder, &document);
        let copied_array = direct_contents(&pages[0]);
        assert!(!Rc::ptr_eq(&copied_array, &original_array));
        assert!(copied_array.borrow().is_inline());
        let copied_entry = array(&copied_array, |array| array.get_object_at(0).unwrap());
        assert!(!Rc::ptr_eq(&copied_entry, &original_entry));
        // The body is multiply referenced before the clone, so its reference
        // entry is replaced without modifying the original stream.
        manager.update_stream(0, b"edited", 6);
        assert_eq!(bytes(&manager.get_stream_by_index(0).unwrap()), b"edited");
        assert_eq!(bytes(&original), b"shared body");
        assert_eq!(manager.add_stream(b"new", 3), 1);
    }
    assert!(Rc::ptr_eq(&direct_contents(&pages[1]), &original_array));
    array(&original_array, |array| {
        assert_eq!(array.size(), 1);
        assert!(Rc::ptr_eq(&array.get_stream_at(0).unwrap(), &original));
    });
}

#[test]
fn shared_array_clone_preserves_child_streams_on_other_pages() {
    let (document, pages) = document_with_pages(2);
    let original = stream(&document, b"body");
    let original_array = content_array(&document, &[Rc::clone(&original)], true);
    for page in &pages {
        contents_reference(&document, page, &original_array);
    }
    let counts = get_objects_with_multiple_references(&document);
    assert!(counts.contains(&original_array.borrow().obj_num()));
    assert!(!counts.contains(&original.borrow().obj_num()));
    let mut holder = page_holder(&pages[0]);
    let mut manager = CpdfPageContentManager::new(&mut holder, &document);
    assert!(!Rc::ptr_eq(&direct_contents(&pages[0]), &original_array));
    manager.update_stream(0, b"changed", 7);
    assert_eq!(bytes(&manager.get_stream_by_index(0).unwrap()), b"changed");
    assert!(!Rc::ptr_eq(
        &manager.get_stream_by_index(0).unwrap(),
        &original
    ));
    assert_eq!(bytes(&original), b"body");
    array(&direct_contents(&pages[1]), |array| {
        assert!(Rc::ptr_eq(&array.get_stream_at(0).unwrap(), &original));
    });
}

#[test]
fn exclusive_indirect_array_is_reused_and_new_streams_append() {
    let (document, pages) = document_with_pages(1);
    let first = stream(&document, b"one");
    let array_object = content_array(&document, &[first], true);
    contents_reference(&document, &pages[0], &array_object);
    let mut holder = page_holder(&pages[0]);
    let mut manager = CpdfPageContentManager::new(&mut holder, &document);
    assert_eq!(manager.add_stream(b"two", 3), 1);
    assert!(Rc::ptr_eq(&direct_contents(&pages[0]), &array_object));
    assert_eq!(array(&array_object, CpdfArray::size), 2);
}

#[test]
fn deferred_single_removal_happens_on_drop_and_only_index_zero_removes_it() {
    let (document, pages) = document_with_pages(1);
    let body = stream(&document, b"sole");
    contents_reference(&document, &pages[0], &body);
    let mut holder = page_holder(&pages[0]);
    {
        let mut manager = CpdfPageContentManager::new(&mut holder, &document);
        manager.update_stream(7, &[], 0);
    }
    assert!(Rc::ptr_eq(&direct_contents(&pages[0]), &body));
    {
        let mut manager = CpdfPageContentManager::new(&mut holder, &document);
        manager.update_stream(0, b"nonempty but tellp failed", -1);
        assert!(manager.has_stream_at_index(0));
        assert!(contents(&pages[0]).is_some());
    }
    assert!(contents(&pages[0]).is_none());
    assert_eq!(bytes(&body), b"sole");
}

#[test]
fn array_removal_is_reversed_deduplicated_and_remaps_inactive_and_unmapped_objects() {
    let (document, pages) = document_with_pages(1);
    let streams: Vec<_> = [b"zero".as_slice(), b"one", b"two", b"three"]
        .into_iter()
        .map(|data| stream(&document, data))
        .collect();
    let array_object = content_array(&document, &streams, false);
    dictionary_mut(&pages[0], |dict| {
        dict.set_for(b"Contents", Some(Rc::clone(&array_object)))
    });
    let mut holder = page_holder(&pages[0]);
    for (index, content_stream) in [0, 2, 3, -1, 8].into_iter().enumerate() {
        let mut object = CpdfPathObject::new(content_stream);
        if index == 1 {
            object.base_mut().set_is_active(false);
        }
        holder.append_page_object(Box::new(object));
    }
    {
        let mut manager = CpdfPageContentManager::new(&mut holder, &document);
        manager.update_stream(1, &[], 0);
        manager.update_stream(3, &[], 0);
        manager.update_stream(1, &[], 0);
        assert_eq!(array(&array_object, CpdfArray::size), 4);
    }
    array(&array_object, |array| {
        assert_eq!(array.size(), 2);
        assert!(Rc::ptr_eq(&array.get_stream_at(0).unwrap(), &streams[0]));
        assert!(Rc::ptr_eq(&array.get_stream_at(1).unwrap(), &streams[2]));
    });
    assert_eq!(
        holder
            .iter()
            .map(|object| object.base().content_stream())
            .collect::<Vec<_>>(),
        [0, 1, 0, 0, 0]
    );
    assert!(!holder.page_object_by_index(1).unwrap().base().is_active());
}

#[test]
fn singleton_array_is_retained_for_later_append() {
    let (document, pages) = document_with_pages(1);
    let streams: Vec<_> = [b"zero".as_slice(), b"one", b"two"]
        .into_iter()
        .map(|data| stream(&document, data))
        .collect();
    let array_object = content_array(&document, &streams, false);
    dictionary_mut(&pages[0], |dict| {
        dict.set_for(b"Contents", Some(Rc::clone(&array_object)))
    });
    let mut holder = page_holder(&pages[0]);
    {
        let mut manager = CpdfPageContentManager::new(&mut holder, &document);
        manager.update_stream(2, &[], 0);
        manager.update_stream(1, &[], 0);
    }
    assert!(Rc::ptr_eq(&direct_contents(&pages[0]), &array_object));
    assert_eq!(array(&array_object, CpdfArray::size), 1);
    let mut manager = CpdfPageContentManager::new(&mut holder, &document);
    assert_eq!(manager.add_stream(b"later", 5), 1);
}

#[test]
fn typed_lookup_ignores_nonreference_array_entries_and_nonstream_targets() {
    let (document, pages) = document_with_pages(1);
    let body = stream(&document, b"body");
    let array_object = retain_object(CpdfArray::new());
    array_mut(&array_object, |array| {
        array.append_new(CpdfScalarObject::null());
        array.append_new(CpdfReference::new(
            &document.holder(),
            document.root().unwrap().borrow().obj_num(),
        ));
        array.append_new(CpdfReference::new(&document.holder(), 999));
        array.append_new(CpdfReference::new(
            &document.holder(),
            body.borrow().obj_num(),
        ));
    });
    dictionary_mut(&pages[0], |dict| {
        dict.set_for(b"Contents", Some(array_object))
    });
    let mut holder = page_holder(&pages[0]);
    let manager = CpdfPageContentManager::new(&mut holder, &document);
    assert!(!manager.has_stream_at_index(0));
    assert!(!manager.has_stream_at_index(1));
    assert!(!manager.has_stream_at_index(2));
    assert!(manager.has_stream_at_index(3));
    assert!(!manager.has_stream_at_index(4));
}

#[test]
fn noncontainer_and_dangling_contents_are_replaced_as_absent() {
    let (document, pages) = document_with_pages(2);
    dictionary_mut(&pages[0], |dict| {
        dict.set_new_for(b"Contents", CpdfScalarObject::number_from_signed(7))
    });
    dictionary_mut(&pages[1], |dict| {
        dict.set_new_for(b"Contents", CpdfReference::new(&document.holder(), 999))
    });
    for page in pages {
        let mut holder = page_holder(&page);
        let mut manager = CpdfPageContentManager::new(&mut holder, &document);
        assert!(!manager.has_stream_at_index(0));
        assert_eq!(manager.add_stream(b"replacement", 11), 0);
        assert_eq!(bytes(&direct_contents(&page)), b"replacement");
    }
}

#[test]
#[should_panic(expected = "UpdateStream requires an existing content stream")]
fn nonempty_update_requires_an_existing_stream() {
    let (document, pages) = document_with_pages(1);
    let mut holder = page_holder(&pages[0]);
    CpdfPageContentManager::new(&mut holder, &document).update_stream(0, b"invalid", 7);
}

#[test]
#[should_panic(expected = "assertion failed")]
fn direct_contents_array_must_be_inline() {
    let (document, pages) = document_with_pages(1);
    let array_object = retain_object(CpdfArray::new());
    dictionary_mut(&pages[0], |dict| {
        dict.set_for(b"Contents", Some(Rc::clone(&array_object)))
    });
    // Native objects can become indirect after insertion. The manager detects
    // a direct dictionary entry that now violates its required inline state.
    document
        .holder()
        .borrow_mut()
        .add_indirect_object(array_object);
    let mut holder = page_holder(&pages[0]);
    CpdfPageContentManager::new(&mut holder, &document);
}
