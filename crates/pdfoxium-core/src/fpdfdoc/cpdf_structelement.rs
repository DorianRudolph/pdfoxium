// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct structure element/child semantics from `cpdf_structelement.{cpp,h}`.
use super::cpdf_structtree::{
    array_direct, array_size, direct_for, integer_for, name_for, raw_for, reference_number,
    CpdfStructTree,
};
use crate::fpdfapi::parser::{cpdf_object::ObjectRef, cpdf_scalar_object::ObjectType};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

// Native metadata retained even where the current public queries only expose
// element identity and MCID, rather than stream/object/page numbers.
#[allow(dead_code)]
enum Kid {
    Invalid,
    Element {
        dictionary: ObjectRef,
        element: Option<Rc<CpdfStructElement>>,
    },
    PageContent {
        page: u32,
        content_id: u32,
    },
    StreamContent {
        page: u32,
        stream: u32,
        content_id: u32,
    },
    Object {
        page: u32,
        object: u32,
    },
}
pub struct CpdfStructElement {
    dictionary: ObjectRef,
    parent: RefCell<Weak<Self>>,
    kind: Vec<u8>,
    kids: RefCell<Vec<Kid>>,
}
impl CpdfStructElement {
    pub(crate) fn new(tree: &CpdfStructTree, dictionary: ObjectRef) -> Rc<Self> {
        let kind = tree.get_role_map_name_for(&name_for(&dictionary, b"S"));
        let page = raw_for(&dictionary, b"Pg")
            .and_then(|page| reference_number(&page))
            .unwrap_or(0);
        let mut kids = Vec::new();
        if let Some(object) = direct_for(&dictionary, b"K") {
            if object.borrow().object_type() == ObjectType::Array {
                kids.reserve(array_size(&object));
                for index in 0..array_size(&object) {
                    kids.push(Self::load_kid(
                        tree.get_page_obj_num(),
                        page,
                        array_direct(&object, index),
                    ));
                }
            } else {
                kids.push(Self::load_kid(tree.get_page_obj_num(), page, Some(object)));
            }
        }
        Rc::new(Self {
            dictionary,
            parent: RefCell::new(Weak::new()),
            kind,
            kids: RefCell::new(kids),
        })
    }
    pub fn get_type(&self) -> Vec<u8> {
        self.kind.clone()
    }
    pub fn get_raw_type(&self) -> Vec<u8> {
        name_for(&self.dictionary, b"S")
    }
    pub fn get_obj_type(&self) -> Vec<u8> {
        raw_for(&self.dictionary, b"Type")
            .map_or_else(Vec::new, |value| value.borrow().get_string())
    }
    fn text(&self, key: &[u8]) -> Vec<u32> {
        direct_for(&self.dictionary, key)
            .map_or_else(Vec::new, |value| value.borrow().get_unicode_text())
    }
    pub fn get_alt_text(&self) -> Vec<u32> {
        self.text(b"Alt")
    }
    pub fn get_actual_text(&self) -> Vec<u32> {
        self.text(b"ActualText")
    }
    pub fn get_expansion(&self) -> Vec<u32> {
        self.text(b"E")
    }
    pub fn get_title(&self) -> Vec<u32> {
        self.text(b"T")
    }
    fn optional_string(&self, key: &[u8]) -> Option<Vec<u32>> {
        let value = raw_for(&self.dictionary, key)?;
        let value = value.borrow();
        (value.object_type() == ObjectType::String).then(|| value.get_unicode_text())
    }
    pub fn get_id(&self) -> Option<Vec<u32>> {
        self.optional_string(b"ID")
    }
    pub fn get_lang(&self) -> Option<Vec<u32>> {
        self.optional_string(b"Lang")
    }
    pub fn get_a(&self) -> Option<ObjectRef> {
        raw_for(&self.dictionary, b"A")
    }
    pub fn get_k(&self) -> Option<ObjectRef> {
        raw_for(&self.dictionary, b"K")
    }
    pub fn count_kids(&self) -> usize {
        self.kids.borrow().len()
    }
    pub fn get_kid_if_element(&self, index: usize) -> Option<Rc<Self>> {
        match self.kids.borrow().get(index)? {
            Kid::Element { element, .. } => element.clone(),
            _ => None,
        }
    }
    pub fn get_kid_content_id(&self, index: usize) -> i32 {
        match self.kids.borrow().get(index) {
            Some(Kid::PageContent { content_id, .. } | Kid::StreamContent { content_id, .. }) => {
                *content_id as i32
            }
            _ => -1,
        }
    }
    pub fn get_parent(&self) -> Option<Rc<Self>> {
        self.parent.borrow().upgrade()
    }
    pub fn set_parent(&self, parent: Option<&Rc<Self>>) {
        *self.parent.borrow_mut() = parent.map_or_else(Weak::new, Rc::downgrade);
    }
    pub(crate) fn update_kid_if_element(&self, dictionary: &ObjectRef, element: &Rc<Self>) -> bool {
        if std::ptr::eq(self, element.as_ref()) {
            return false;
        }
        let mut saved = false;
        for kid in self.kids.borrow_mut().iter_mut() {
            if let Kid::Element {
                dictionary: expected,
                element: child,
            } = kid
            {
                if Rc::ptr_eq(expected, dictionary) {
                    *child = Some(element.clone());
                    saved = true;
                }
            }
        }
        saved
    }
    fn load_kid(tree_page: u32, mut page: u32, object: Option<ObjectRef>) -> Kid {
        let Some(object) = object else {
            return Kid::Invalid;
        };
        let kind = object.borrow().object_type();
        if kind == ObjectType::Number {
            return if tree_page == page {
                Kid::PageContent {
                    page,
                    content_id: object.borrow().get_integer() as u32,
                }
            } else {
                Kid::Invalid
            };
        }
        if kind != ObjectType::Dictionary {
            return Kid::Invalid;
        }
        if let Some(number) = raw_for(&object, b"Pg").and_then(|page| reference_number(&page)) {
            page = number;
        }
        let kind = name_for(&object, b"Type");
        if (kind == b"MCR" || kind == b"OBJR") && tree_page != page {
            return Kid::Invalid;
        }
        if kind == b"MCR" {
            let stream = raw_for(&object, b"Stm")
                .and_then(|stream| reference_number(&stream))
                .unwrap_or(0);
            Kid::StreamContent {
                page,
                stream,
                content_id: integer_for(&object, b"MCID", 0) as u32,
            }
        } else if kind == b"OBJR" {
            let target = raw_for(&object, b"Obj")
                .and_then(|target| reference_number(&target))
                .unwrap_or(0);
            Kid::Object {
                page,
                object: target,
            }
        } else {
            Kid::Element {
                dictionary: object,
                element: None,
            }
        }
    }
}
