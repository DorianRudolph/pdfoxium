// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Source-derived regressions for editing branches in
//! `core/fpdfapi/page/cpdf_pageobjectholder.cpp`.
use super::*;
use crate::fpdfapi::page::cpdf_pageobject::PageObjectType;
use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use std::cell::Cell;
fn holder() -> CpdfPageObjectHolder {
    CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None)
}
fn path(stream: i32) -> Box<dyn PageObject> {
    Box::new(CpdfPathObject::new(stream))
}

#[test]
fn insertion_adopts_next_stream_and_preserves_existing_streams() {
    let mut h = holder();
    h.append_page_object(path(7));
    h.append_page_object(path(9));
    assert!(h.insert_page_object_at_index(1, path(-1)));
    assert_eq!(
        h.page_object_by_index(1).unwrap().base().content_stream(),
        9
    );
    assert_eq!(h.take_dirty_streams(), BTreeSet::from([9]));
    assert!(h.insert_page_object_at_index(1, path(4)));
    assert_eq!(
        h.page_object_by_index(1).unwrap().base().content_stream(),
        4
    );
    assert!(h.take_dirty_streams().is_empty());
    assert!(h.insert_page_object_at_index(4, path(-1)));
    assert_eq!(
        h.page_object_by_index(4).unwrap().base().content_stream(),
        -1
    );
    assert!(h.take_dirty_streams().is_empty());
    assert!(h.insert_page_object_at_index(4, path(-1)));
    assert!(h.take_dirty_streams().is_empty());
}
#[test]
fn removal_tracks_allocation_identity_across_index_shifts_and_transfers_ownership() {
    let mut h = holder();
    h.append_page_object(path(7));
    h.append_page_object(path(-1));
    let first = h.page_object_id_by_index(0).unwrap();
    let second = h.page_object_id_by_index(1).unwrap();
    assert!(h.insert_page_object_at_index(0, path(3)));
    assert_eq!(h.page_object_id_by_index(1), Some(first));
    let removed = h.remove_page_object(first).unwrap();
    assert_eq!(removed.base().content_stream(), 7);
    assert!(h.remove_page_object(first).is_none()); // Keep removed allocation alive.
    assert_eq!(h.take_dirty_streams(), BTreeSet::from([7]));
    assert!(h.remove_page_object(PageObjectId(0)).is_none());
    let streamless = h.remove_page_object(second).unwrap();
    assert_eq!(streamless.base().content_stream(), -1);
    assert!(h.take_dirty_streams().is_empty());
    assert_eq!(h.page_object_count(), 1);
}
#[test]
fn erase_does_not_mark_stream_dirty_and_checks_bounds() {
    let mut h = holder();
    h.append_page_object(path(4));
    assert!(!h.erase_page_object_at_index(1));
    assert!(!h.erase_page_object_at_index(usize::MAX));
    assert_eq!(h.page_object_count(), 1);
    assert!(h.erase_page_object_at_index(0));
    assert!(!h.erase_page_object_at_index(0));
    assert!(h.take_dirty_streams().is_empty());
}
struct Tracked {
    base: CpdfPageObject,
    drops: Rc<Cell<usize>>,
}
impl Drop for Tracked {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
impl PageObject for Tracked {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn object_type(&self) -> PageObjectType {
        PageObjectType::Path
    }
    fn transform(&mut self, _: CfxMatrix) {}
    fn base(&self) -> &CpdfPageObject {
        &self.base
    }
    fn base_mut(&mut self) -> &mut CpdfPageObject {
        &mut self.base
    }
}
#[test]
fn invalid_insert_consumes_object_and_remove_transfers_without_destruction() {
    let drops = Rc::new(Cell::new(0));
    let mut h = holder();
    let make = || {
        Box::new(Tracked {
            base: CpdfPageObject::new(2),
            drops: Rc::clone(&drops),
        }) as Box<dyn PageObject>
    };
    assert!(!h.insert_page_object_at_index(1, make()));
    assert_eq!(drops.get(), 1);
    assert_eq!(h.page_object_count(), 0);
    h.append_page_object(make());
    let id = h.page_object_id_by_index(0).unwrap();
    let owned = h.remove_page_object(id).unwrap();
    assert_eq!(drops.get(), 1);
    drop(owned);
    assert_eq!(drops.get(), 2);
    h.append_page_object(make());
    assert!(h.erase_page_object_at_index(0));
    assert_eq!(drops.get(), 3);
}
