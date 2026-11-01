// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Engine adaptations of the selected ordinary-page import/viewer-preference
//! cases in `fpdfsdk/fpdf_ppo_embeddertest.cpp`. New document creation here uses
//! the production CPDF_Document factory; SDK clock metadata is outside these cases.
#![allow(non_snake_case)]
use super::*;
use crate::fpdfapi::edit::cpdf_creator::CpdfCreator;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_parser::ParserError;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
use crate::fxcrt::fx_stream::WriteStream;

fn open(file: &str) -> Rc<RefCell<CpdfDocument>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../upstream/pdfium/testing/resources")
        .join(file);
    let mut doc = CpdfDocument::new();
    assert_eq!(
        doc.load_doc(
            Rc::new(CfxReadOnlyContainerStream::new(
                std::fs::read(path).unwrap()
            )),
            b""
        ),
        ParserError::Success
    );
    Rc::new(RefCell::new(doc))
}
fn new_document() -> Rc<RefCell<CpdfDocument>> {
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    Rc::new(RefCell::new(doc))
}
fn load_page(doc: &Rc<RefCell<CpdfDocument>>, index: usize) -> CpdfPage {
    let mut page = CpdfPage::from_document(&mut doc.borrow_mut(), index).unwrap();
    page.parse_path_content();
    page
}
fn dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
    let borrowed = object.borrow();
    f(borrowed.as_any().downcast_ref().unwrap())
}
#[derive(Default)]
struct Sink(Vec<u8>);
impl WriteStream for Sink {
    fn write_block(&mut self, b: &[u8]) -> bool {
        self.0.extend_from_slice(b);
        true
    }
}
fn save(doc: &Rc<RefCell<CpdfDocument>>) -> Vec<u8> {
    let mut sink = Sink::default();
    {
        assert!(CpdfCreator::new(Rc::clone(doc), &mut sink).create(0, 0));
    }
    sink.0
}

#[test]
fn NoViewerPreferences() {
    let src = open("hello_world.pdf");
    let dest = new_document();
    assert!(!copy_viewer_preferences(&dest, &src));
}
#[test]
fn ViewerPreferences() {
    let src = open("viewer_ref.pdf");
    let dest = new_document();
    assert!(copy_viewer_preferences(&dest, &src));
}
#[test]
fn ImportPagesByIndex() {
    let src = open("viewer_ref.pdf");
    let _page = load_page(&src, 0);
    let dest = new_document();
    assert!(copy_viewer_preferences(&dest, &src));
    assert!(import_pages_by_index(
        Rc::clone(&dest),
        Rc::clone(&src),
        Some(&[1]),
        0
    ));
    assert_eq!(dest.borrow().page_count(), 1);
}
#[test]
fn ImportPages() {
    let src = open("viewer_ref.pdf");
    let _page = load_page(&src, 0);
    let dest = new_document();
    assert!(copy_viewer_preferences(&dest, &src));
    assert!(import_pages(Rc::clone(&dest), Rc::clone(&src), b"1", 0));
    assert_eq!(dest.borrow().page_count(), 1);
}
#[test]
fn BadRepeatViewerPref() {
    let src = open("repeat_viewer_ref.pdf");
    let dest = new_document();
    assert!(copy_viewer_preferences(&dest, &src));
    assert!(!save(&dest).is_empty());
}
#[test]
fn BadCircularViewerPref() {
    let src = open("circular_viewer_ref.pdf");
    let dest = new_document();
    assert!(copy_viewer_preferences(&dest, &src));
    assert!(!save(&dest).is_empty());
}
#[test]
fn CopyViewerPrefTypes() {
    let src = open("viewer_pref_types.pdf");
    let dest = new_document();
    assert!(copy_viewer_preferences(&dest, &src));
    let root = dest.borrow().root().unwrap();
    let prefs = dict(&root, |d| d.get_dict_for(b"ViewerPreferences")).unwrap();
    dict(&prefs, |d| {
        assert_eq!(d.size(), 6);
        assert_eq!(
            d.get_object_for(b"Bool").unwrap().borrow().object_type(),
            ObjectType::Boolean
        );
        let number = d.get_object_for(b"Num").unwrap();
        let borrowed = number.borrow();
        let number = borrowed
            .as_any()
            .downcast_ref::<CpdfScalarObject>()
            .unwrap();
        assert!(number.is_integer());
        assert_eq!(number.get_integer(), 1);
        let string = d.get_object_for(b"Str").unwrap();
        assert_eq!(string.borrow().object_type(), ObjectType::String);
        assert_eq!(string.borrow().get_string(), b"str");
        assert_eq!(d.get_name_for(b"Name"), b"name");
        for (key, expected) in [(b"EmptyArray".as_slice(), 0), (b"GoodArray", 4)] {
            let a = d.get_array_for(key).unwrap();
            assert_eq!(
                a.borrow()
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .unwrap()
                    .size(),
                expected
            );
        }
    });
}
#[test]
fn BadIndices() {
    let src = open("hello_world.pdf");
    let _page = load_page(&src, 0);
    let dest = new_document();
    for indices in [&[-1][..], &[1], &[-1, 0, 1], &[42]] {
        assert!(!import_pages_by_index(
            Rc::clone(&dest),
            Rc::clone(&src),
            Some(indices),
            0
        ));
    }
}
#[test]
fn GoodIndices() {
    let src = open("viewer_ref.pdf");
    let _page = load_page(&src, 0);
    let dest = new_document();
    for (indices, count) in [
        (&[0, 0, 0, 0][..], 4),
        (&[0][..], 5),
        (&[4][..], 6),
        (&[1, 2, 3][..], 9),
    ] {
        assert!(import_pages_by_index(
            Rc::clone(&dest),
            Rc::clone(&src),
            Some(indices),
            0
        ));
        assert_eq!(dest.borrow().page_count(), count);
    }
    assert!(import_pages_by_index(
        Rc::clone(&dest),
        Rc::clone(&src),
        None,
        0
    ));
    assert_eq!(dest.borrow().page_count(), 14);
}
#[test]
fn BadRanges() {
    let src = open("hello_world.pdf");
    let _page = load_page(&src, 0);
    let dest = new_document();
    for range in [
        b"clams".as_slice(),
        b"0",
        b"42",
        b"1,2",
        b"1-2",
        b",1",
        b"1,",
        b"1-",
        b"-1",
        b"-,0,,,1-",
    ] {
        assert!(!import_pages(Rc::clone(&dest), Rc::clone(&src), range, 0));
    }
}
#[test]
fn GoodRanges() {
    let src = open("viewer_ref.pdf");
    let _page = load_page(&src, 0);
    let dest = new_document();
    assert!(copy_viewer_preferences(&dest, &src));
    for (range, count) in [
        (b"1,1,1,1".as_slice(), 4),
        (b"1-1", 5),
        (b"5-5", 6),
        (b"2-4", 9),
    ] {
        assert!(import_pages(Rc::clone(&dest), Rc::clone(&src), range, 0));
        assert_eq!(dest.borrow().page_count(), count);
    }
}
#[test]
fn Bug664284() {
    let src = open("bug_664284.pdf");
    let _page = load_page(&src, 0);
    let dest = new_document();
    assert!(import_pages_by_index(dest, Rc::clone(&src), Some(&[0]), 0));
}
#[test]
fn ImportWithSelfReferentialPageParent() {
    let src = open("bug_517126568.pdf");
    assert_eq!(src.borrow().page_count(), 1);
    let _page = load_page(&src, 0);
    let dest = new_document();
    assert!(import_pages_by_index(
        Rc::clone(&dest),
        Rc::clone(&src),
        Some(&[0]),
        0
    ));
    assert_eq!(dest.borrow().page_count(), 1);
    assert!(import_pages(Rc::clone(&dest), Rc::clone(&src), b"1", 1));
    assert_eq!(dest.borrow().page_count(), 2);
}
#[test]
fn ImportIntoDestDocWithoutInfo() {
    let dest = open("hello_world.pdf");
    assert_eq!(dest.borrow().page_count(), 1);
    let src = open("rectangles.pdf");
    assert!(import_pages_by_index(
        Rc::clone(&dest),
        Rc::clone(&src),
        Some(&[0]),
        0
    ));
    assert_eq!(dest.borrow().page_count(), 2);
    assert!(import_pages(Rc::clone(&dest), Rc::clone(&src), b"1", 0));
    assert_eq!(dest.borrow().page_count(), 3);
}
