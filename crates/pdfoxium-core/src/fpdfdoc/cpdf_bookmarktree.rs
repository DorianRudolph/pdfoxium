// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Outline traversal from `core/fpdfdoc/cpdf_bookmarktree.{h,cpp}`.
use super::cpdf_bookmark::CpdfBookmark;
use crate::fpdfapi::parser::{
    cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument, cpdf_object::ObjectRef,
};
use std::rc::Rc;
pub struct CpdfBookmarkTree<'a> {
    document: &'a CpdfDocument,
}
impl<'a> CpdfBookmarkTree<'a> {
    pub fn new(document: &'a CpdfDocument) -> Self {
        Self { document }
    }
    pub fn first_child(&self, parent: &CpdfBookmark) -> CpdfBookmark {
        if let Some(parent) = parent.dict() {
            return CpdfBookmark::new(entry(&parent, b"First"));
        }
        CpdfBookmark::new(
            self.document
                .root()
                .and_then(|root| entry(&root, b"Outlines"))
                .and_then(|outlines| entry(&outlines, b"First")),
        )
    }
    pub fn next_sibling(&self, bookmark: &CpdfBookmark) -> CpdfBookmark {
        let Some(dict) = bookmark.dict() else {
            return CpdfBookmark::default();
        };
        CpdfBookmark::new(entry(&dict, b"Next").filter(|next| !Rc::ptr_eq(next, &dict)))
    }
}
fn entry(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_dict_for(key)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{cpdf_object::retain_object, cpdf_reference::CpdfReference};
    #[test]
    fn tree_uses_catalog_outlines_and_suppresses_self_sibling() {
        let mut doc = CpdfDocument::new();
        doc.create_new_doc();
        let root = doc.root().unwrap();
        let outlines = retain_object(CpdfDictionary::new());
        let first = retain_object(CpdfDictionary::new());
        let holder = doc.holder();
        let number = holder.borrow_mut().add_indirect_object(first.clone());
        root.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"Outlines", Some(outlines.clone()));
        outlines
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"First", CpdfReference::new(&holder, number));
        let tree = CpdfBookmarkTree::new(&doc);
        let bookmark = tree.first_child(&CpdfBookmark::default());
        assert!(Rc::ptr_eq(&bookmark.dict().unwrap(), &first));
        first
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Next", CpdfReference::new(&holder, number));
        assert!(tree.next_sibling(&bookmark).dict().is_none());
        // The reference is weak; cleanup also restores the ordinary outline graph.
        first
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .remove_for(b"Next");
    }
}
