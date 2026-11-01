// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete cases from `object_tree_traversal_util_embeddertest.cpp`.
//! The engine-level document substitutes for the C ABI handle conversion.
#![allow(non_snake_case)]

use std::collections::BTreeSet;
use std::rc::Rc;

use super::{get_objects_with_multiple_references, get_objects_with_references};
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_parser::ParserError;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

fn open_document(filename: &str) -> CpdfDocument {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../upstream/pdfium/testing/resources")
        .join(filename);
    let mut doc = CpdfDocument::new();
    assert_eq!(
        doc.load_doc(
            Rc::new(CfxReadOnlyContainerStream::new(
                std::fs::read(path).unwrap()
            )),
            b"",
        ),
        ParserError::Success,
    );
    doc
}

#[test]
fn GetObjectsWithReferencesBasic() {
    let doc = open_document("hello_world.pdf");
    assert_eq!(
        get_objects_with_references(&doc),
        BTreeSet::from([1, 2, 3, 4, 5, 6])
    );
}

#[test]
fn GetObjectsWithReferencesNewDoc() {
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    // Empty documents have a catalog and an empty pages object.
    assert_eq!(get_objects_with_references(&doc), BTreeSet::from([1, 2]));
}

#[test]
fn GetObjectsWithReferencesCircularRefs() {
    let doc = open_document("circular_viewer_ref.pdf");
    // The trailer points at a catalog which only references itself.
    assert_eq!(get_objects_with_references(&doc), BTreeSet::from([1]));
}

#[test]
fn GetObjectsWithReferencesCrossRefStream() {
    let doc = open_document("bug_1399.pdf");
    // The trailer is inside /XRef object 16 0 and uniquely references 3 0.
    assert_eq!(
        get_objects_with_references(&doc),
        BTreeSet::from([1, 2, 3, 4, 5, 12, 13, 14, 16]),
    );
}

#[test]
fn GetObjectsWithReferencesObjectZero() {
    let doc = open_document("rectangles_object_zero.pdf");
    assert_eq!(
        get_objects_with_references(&doc),
        BTreeSet::from([1, 2, 3, 4])
    );
}

#[test]
fn GetObjectsWithMultipleReferencesBasic() {
    let doc = open_document("hello_world.pdf");
    assert!(get_objects_with_multiple_references(&doc).is_empty());
}

#[test]
fn GetObjectsWithMultipleReferencesNewDoc() {
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    assert!(get_objects_with_multiple_references(&doc).is_empty());
}

#[test]
fn GetObjectsWithMultipleReferencesCircularRefs() {
    let doc = open_document("circular_viewer_ref.pdf");
    assert!(get_objects_with_multiple_references(&doc).is_empty());
}

#[test]
fn GetObjectsWithMultipleReferencesSharedObjects() {
    let doc = open_document("hello_world_2_pages.pdf");
    assert_eq!(
        get_objects_with_multiple_references(&doc),
        BTreeSet::from([5, 6, 7])
    );
}

#[test]
fn GetObjectsWithMultipleReferencesObjectZero() {
    let doc = open_document("rectangles_object_zero.pdf");
    assert!(get_objects_with_multiple_references(&doc).is_empty());
}
