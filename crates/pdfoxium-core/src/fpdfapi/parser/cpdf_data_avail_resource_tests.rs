// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Availability resource inheritance, shared scans and resumable callbacks.
use super::super::cpdf_indirect_object_holder::HolderRef;
use super::super::cpdf_object::retain_object;
use super::super::cpdf_scalar_object::CpdfScalarObject;
use super::*;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
use std::cell::Cell;

fn dictionary() -> ObjectRef {
    retain_object(CpdfDictionary::new())
}
fn set(dict: &ObjectRef, key: &[u8], value: ObjectRef) {
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(key, Some(value));
}
fn link(holder: &HolderRef, number: u32) -> ObjectRef {
    retain_object(CpdfReference::new(holder, number))
}
fn reader() -> CpdfDataAvail {
    CpdfDataAvail::new(
        None,
        Rc::new(CfxReadOnlyContainerStream::new(vec![0; 1024])),
    )
}
fn document() -> (Rc<RefCell<CpdfDocument>>, HolderRef) {
    let doc = Rc::new(RefCell::new(CpdfDocument::new()));
    let holder = doc.borrow().holder();
    (doc, holder)
}

#[test]
fn indirect_page_counts_follow_multiple_references_without_losing_the_sign() {
    for value in [3, 0, -7] {
        let holder = CpdfIndirectObjectHolder::new();
        let number = holder
            .borrow_mut()
            .add_indirect_object(retain_object(CpdfScalarObject::number_from_signed(value)));
        let indirect = holder
            .borrow_mut()
            .add_indirect_object(link(&holder, number));
        let pages = dictionary();
        set(&pages, b"Count", link(&holder, indirect));
        assert_eq!(integer(&pages, b"Count"), value);
    }
}
#[test]
fn cyclic_and_expired_count_references_have_a_bounded_zero_fallback() {
    for two_nodes in [false, true] {
        let holder = CpdfIndirectObjectHolder::new();
        assert_eq!(
            holder
                .borrow_mut()
                .add_indirect_object(link(&holder, if two_nodes { 2 } else { 1 })),
            1
        );
        if two_nodes {
            assert_eq!(holder.borrow_mut().add_indirect_object(link(&holder, 1)), 2);
        }
        let pages = dictionary();
        set(&pages, b"Count", link(&holder, 1));
        assert_eq!(integer(&pages, b"Count"), 0);
    }
    let pages = dictionary();
    let dangling = {
        let holder = CpdfIndirectObjectHolder::new();
        link(&holder, 99)
    };
    set(&pages, b"Count", dangling);
    assert_eq!(integer(&pages, b"Count"), 0);
}
#[test]
fn inherited_shared_resource_graph_uses_one_retained_availability_scan() {
    let (doc, holder) = document();
    let resources = holder.borrow_mut().new_indirect_dictionary();
    let number = resources.borrow().obj_num();
    // An unused indirect back-reference must not create a traversal loop.
    set(&resources, b"Cycle", link(&holder, number));
    let resource_ref = link(&holder, number);
    let parent = holder.borrow_mut().new_indirect_dictionary();
    set(&parent, b"Resources", resource_ref.clone());
    let parent_number = parent.borrow().obj_num();
    let first = dictionary();
    let second = dictionary();
    set(&first, b"Parent", link(&holder, parent_number));
    set(&second, b"Parent", link(&holder, parent_number));
    let mut avail = reader();
    assert_eq!(avail.check_resources(first.clone(), &doc), DataAvailable);
    assert_eq!(avail.pages_resources_avail.len(), 1);
    assert_eq!(avail.check_resources(second, &doc), DataAvailable);
    assert_eq!(avail.check_resources(first, &doc), DataAvailable);
    assert_eq!(avail.pages_resources_avail.len(), 1);
    let retained = &avail.pages_resources_avail.values().next().unwrap().0;
    assert!(Rc::ptr_eq(retained, &resource_ref));
}
#[test]
fn a_parent_cycle_without_resources_finishes_without_allocating_a_resource_scan() {
    let (doc, holder) = document();
    let first = holder.borrow_mut().new_indirect_dictionary();
    let second = holder.borrow_mut().new_indirect_dictionary();
    let a = first.borrow().obj_num();
    let b = second.borrow().obj_num();
    set(&first, b"Parent", link(&holder, b));
    set(&second, b"Parent", link(&holder, a));
    let mut avail = reader();
    assert_eq!(avail.check_resources(first.clone(), &doc), DataAvailable);
    assert_eq!(avail.check_resources(second, &doc), DataAvailable);
    assert!(avail.pages_resources_avail.is_empty());
    assert_eq!(integer(&first, b"Count"), 0);
}

struct Gate(Cell<bool>);
impl FileAvail for Gate {
    fn is_data_avail(&self, _: i64, _: usize) -> bool {
        self.0.get()
    }
}
struct Input {
    fail: Cell<bool>,
    reads: Cell<usize>,
}
impl StreamWithSize for Input {
    fn get_size(&self) -> i64 {
        1024
    }
}
impl SeekableReadStream for Input {
    fn read_block_at_offset(&self, output: &mut [u8], offset: i64) -> bool {
        self.reads.set(self.reads.get() + 1);
        if self.fail.replace(false) {
            return false;
        }
        if offset < 0 || offset as usize > 1024 || output.len() > 1024 - offset as usize {
            return false;
        }
        output.fill(0);
        true
    }
}
fn retry_resource_scan(failed_read: bool) {
    let gate = Rc::new(Gate(Cell::new(failed_read)));
    let availability: Rc<dyn FileAvail> = gate.clone();
    let input = Rc::new(Input {
        fail: Cell::new(failed_read),
        reads: Cell::new(0),
    });
    let mut avail = CpdfDataAvail::new(Some(&availability), input.clone());
    let (doc, holder) = document();
    let resources = holder.borrow_mut().new_indirect_dictionary();
    set(&resources, b"Pending", link(&holder, 99));
    let number = resources.borrow().obj_num();
    let page = dictionary();
    set(&page, b"Resources", link(&holder, number));
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let validator = avail.validator();
    holder.borrow_mut().set_parser(Some(Rc::new(move |number| {
        assert_eq!(number, 99);
        count.set(count.get() + 1);
        if !validator.check_data_range_and_request_if_unavailable(128, 8) {
            return None;
        }
        let mut data = [0; 8];
        if !validator.read_block_at_offset(&mut data, 128) {
            return None;
        }
        Some(dictionary())
    })));
    assert_eq!(avail.check_resources(page.clone(), &doc), DataNotAvailable);
    assert_eq!(calls.get(), 1);
    assert_eq!(avail.pages_resources_avail.len(), 1);
    let retained = avail
        .pages_resources_avail
        .values()
        .next()
        .unwrap()
        .0
        .clone();
    gate.0.set(true);
    assert_eq!(avail.check_resources(page.clone(), &doc), DataAvailable);
    assert_eq!(calls.get(), 2);
    assert_eq!(avail.check_resources(page, &doc), DataAvailable);
    assert_eq!(
        calls.get(),
        2,
        "Completed shared scans must not parse dependencies again"
    );
    assert!(input.reads.get() > 0);
    assert!(Rc::ptr_eq(
        &retained,
        &avail.pages_resources_avail.values().next().unwrap().0
    ));
}
#[test]
fn a_shared_resource_scan_resumes_after_dependency_bytes_become_available() {
    retry_resource_scan(false);
}
#[test]
fn a_shared_resource_scan_retries_a_temporary_read_error_without_replacing_its_owner() {
    retry_resource_scan(true);
}

fn ordinary_document() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Count 2 /Kids [3 0 R 4 0 R] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << >> >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 96 96] /Resources << >> >>",
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, body).as_bytes());
    }
    let xref = out.len();
    out.extend(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in &offsets[1..] {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    out
}
#[test]
fn a_missing_page_number_cache_uses_the_existing_tree_then_retains_dictionary_identity() {
    let bytes = ordinary_document();
    let mut avail = CpdfDataAvail::new(None, Rc::new(CfxReadOnlyContainerStream::new(bytes)));
    // A fully downloaded file can be opened before polling IsDocAvail.
    // Its document parser is ready while this availability scanner is still lazy.
    let doc = avail.parse_document(b"").unwrap();
    assert_eq!(doc.borrow().page_count(), 2);
    doc.borrow_mut().set_page_obj_num(1, 0);
    assert_eq!(avail.page_nodes[0].number, 0);
    let page = avail
        .get_page_dictionary(1)
        .expect("The lazy page-tree path must populate the missing slot");
    assert_eq!(page.borrow().obj_num(), 4);
    assert_eq!(avail.page_nodes[0].number, 2);
    assert!(Rc::ptr_eq(&page, &avail.get_page_dictionary(1).unwrap()));
    assert!(avail.get_page_dictionary(2).is_none());
    assert!(Rc::ptr_eq(&page, &avail.get_page_dictionary(1).unwrap()));
}
