// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/page/cpdf_contentmarks.{h,cpp}`.

use std::cell::{Ref, RefCell};
use std::rc::Rc;

use super::cpdf_contentmarkitem::CpdfContentMarkItem;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};

pub type MarkItemRef = Rc<RefCell<CpdfContentMarkItem>>;

#[derive(Default)]
pub struct CpdfContentMarks {
    mark_data: Option<Rc<RefCell<Vec<MarkItemRef>>>>,
}

impl Clone for CpdfContentMarks {
    fn clone(&self) -> Self {
        self.clone_marks()
    }
}

impl CpdfContentMarks {
    pub fn new() -> Self {
        Self::default()
    }

    /// `Clone()` in PDFium copies the mark list but retains each item.
    pub fn clone_marks(&self) -> Self {
        Self {
            mark_data: self
                .mark_data
                .as_ref()
                .map(|data| Rc::new(RefCell::new(data.borrow().clone()))),
        }
    }

    pub fn count_items(&self) -> usize {
        self.mark_data
            .as_ref()
            .map_or(0, |data| data.borrow().len())
    }

    pub fn contains_item(&self, item: &MarkItemRef) -> bool {
        self.mark_data
            .as_ref()
            .is_some_and(|data| data.borrow().iter().any(|mark| Rc::ptr_eq(mark, item)))
    }

    pub fn get_item(&self, index: usize) -> MarkItemRef {
        self.mark_data
            .as_ref()
            .expect("marked content list is empty")
            .borrow()[index]
            .clone()
    }

    /// Borrow the list once for internal read-only traversal. Callers must not
    /// mutate the marks or invoke external callbacks while holding the borrow.
    pub(crate) fn items(&self) -> Option<Ref<'_, [MarkItemRef]>> {
        self.mark_data
            .as_ref()
            .map(|data| Ref::map(data.borrow(), Vec::as_slice))
    }

    pub fn marked_content_id(&self) -> i32 {
        let Some(data) = &self.mark_data else {
            return -1;
        };
        for mark in data.borrow().iter() {
            let Some(dict) = mark.borrow().param() else {
                continue;
            };
            let borrowed = dict.borrow();
            let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
                continue;
            };
            if dict.key_exist(b"MCID") {
                return dict.get_integer_for(b"MCID", 0);
            }
        }
        -1
    }

    fn data_mut(&mut self) -> Rc<RefCell<Vec<MarkItemRef>>> {
        self.mark_data
            .get_or_insert_with(|| Rc::new(RefCell::new(Vec::new())))
            .clone()
    }

    pub fn add_mark(&mut self, name: &[u8]) -> MarkItemRef {
        let item = Rc::new(RefCell::new(CpdfContentMarkItem::new(name)));
        self.add_existing_mark(item.clone());
        item
    }

    pub fn add_existing_mark(&mut self, mark: MarkItemRef) {
        self.data_mut().borrow_mut().push(mark);
    }

    pub fn add_mark_with_direct_dict(
        &mut self,
        name: &[u8],
        dict: &ObjectRef,
    ) -> Option<MarkItemRef> {
        let cloned = get_dict_object(&dict.borrow().clone_object())?;
        let item = Rc::new(RefCell::new(CpdfContentMarkItem::new(name)));
        item.borrow_mut().set_direct_dict(cloned);
        self.add_existing_mark(item.clone());
        Some(item)
    }

    pub fn add_mark_with_properties_holder(
        &mut self,
        name: &[u8],
        dict: ObjectRef,
        property_name: &[u8],
    ) -> MarkItemRef {
        let item = Rc::new(RefCell::new(CpdfContentMarkItem::new(name)));
        item.borrow_mut().set_properties_holder(dict, property_name);
        self.add_existing_mark(item.clone());
        item
    }

    pub fn remove_mark(&mut self, item: &MarkItemRef) -> bool {
        let Some(data) = &self.mark_data else {
            return false;
        };
        let mut data = data.borrow_mut();
        let Some(index) = data.iter().position(|mark| Rc::ptr_eq(mark, item)) else {
            return false;
        };
        data.remove(index);
        true
    }

    pub fn find_first_difference(&self, other: &Self) -> usize {
        if let (Some(left), Some(right)) = (&self.mark_data, &other.mark_data) {
            if Rc::ptr_eq(left, right) {
                return self.count_items();
            }
        }
        let min_len = self.count_items().min(other.count_items());
        for i in 0..min_len {
            if !Rc::ptr_eq(&self.get_item(i), &other.get_item(i)) {
                return i;
            }
        }
        min_len
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfContentMarks;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn explicit_clone_copies_list_but_retains_mark_identity() {
        let properties = retain_object(CpdfDictionary::new());
        {
            let mut borrowed = properties.borrow_mut();
            borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(b"MCID", CpdfScalarObject::number_from_signed(7));
        }
        let mut marks = CpdfContentMarks::new();
        let mark = marks
            .add_mark_with_direct_dict(b"Span", &properties)
            .unwrap();
        let mut copy = marks.clone_marks();
        assert_eq!(copy.marked_content_id(), 7);
        assert!(copy.contains_item(&mark));
        copy.add_mark(b"Artifact");
        assert_eq!(marks.count_items(), 1);
        assert_eq!(marks.find_first_difference(&copy), 1);
        assert!(copy.remove_mark(&mark));
        assert!(marks.contains_item(&mark));
    }

    #[test]
    fn ordinary_clone_is_the_parser_content_marks_snapshot() {
        let mut source = CpdfContentMarks::new();
        let mark = source.add_mark(b"Span");
        let mut page_object_marks = source.clone();
        assert!(page_object_marks.contains_item(&mark));
        page_object_marks.add_mark(b"Artifact");
        assert_eq!(source.count_items(), 1);
        assert_eq!(page_object_marks.count_items(), 2);
        assert!(page_object_marks.remove_mark(&mark));
        assert!(source.contains_item(&mark));
    }
}
