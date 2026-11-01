// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Outline item data from `core/fpdfdoc/cpdf_bookmark.{h,cpp}`.
use super::{cpdf_action::CpdfAction, cpdf_dest::CpdfDest};
use crate::fpdfapi::parser::{
    cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument,
    cpdf_object::ObjectRef, cpdf_string::CpdfString,
};
#[derive(Clone, Default)]
pub struct CpdfBookmark {
    dict: Option<ObjectRef>,
}
impl CpdfBookmark {
    pub fn new(dict: Option<ObjectRef>) -> Self {
        Self { dict }
    }
    pub fn dict(&self) -> Option<ObjectRef> {
        self.dict.clone()
    }
    pub fn title(&self) -> Vec<u32> {
        let Some(title) = self.dict.as_ref().and_then(|object| {
            object
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_direct_object_for(b"Title")
        }) else {
            return Vec::new();
        };
        let borrowed = title.borrow();
        if borrowed.as_any().is::<CpdfString>() {
            borrowed
                .get_unicode_text()
                .into_iter()
                .map(|ch| ch.max(0x20))
                .collect()
        } else {
            Vec::new()
        }
    }
    pub fn dest(&self, document: &CpdfDocument) -> CpdfDest {
        CpdfDest::create(
            document,
            self.dict.as_ref().and_then(|object| {
                object
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()?
                    .get_direct_object_for(b"Dest")
            }),
        )
    }
    pub fn action(&self) -> CpdfAction {
        CpdfAction::new(self.dict.as_ref().and_then(|object| {
            object
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_dict_for(b"A")
        }))
    }
    pub fn color(&self) -> Option<[f32; 3]> {
        let rgb = self.dict.as_ref().and_then(|object| {
            object
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_array_for(b"C")
        })?;
        let borrowed = rgb.borrow();
        let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
        if array.size() != 3 {
            return None;
        }
        // Native code dereferences missing numeric components. Treat malformed
        // arrays as absent color rather than reproducing that crash.
        let number = |index| array.get_number_at(index).map(|v| v.borrow().get_number());
        Some([number(0)?, number(1)?, number(2)?])
    }
    pub fn style(&self) -> i32 {
        self.integer(b"F")
    }
    pub fn count(&self) -> i32 {
        self.integer(b"Count")
    }
    fn integer(&self, key: &[u8]) -> i32 {
        self.dict.as_ref().map_or(0, |object| {
            object
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .map_or(0, |dict| dict.get_integer_for(key, 0))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{
        cpdf_object::retain_object, cpdf_scalar_object::CpdfScalarObject,
    };
    #[test]
    fn bookmark_title_requires_string_and_replaces_control_characters() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_string_for(b"Title", b"\nA\tB\r");
        dict.set_new_for(b"F", CpdfScalarObject::number_from_signed(3));
        dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(-5));
        let dict = retain_object(dict);
        let bookmark = CpdfBookmark::new(Some(dict.clone()));
        assert_eq!(
            bookmark.title(),
            " A B ".chars().map(u32::from).collect::<Vec<_>>()
        );
        assert_eq!(bookmark.style(), 3);
        assert_eq!(bookmark.count(), -5);
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_name_for(b"Title", b"ignored");
        assert!(bookmark.title().is_empty());
    }
    #[test]
    fn bookmark_color_requires_three_numeric_components() {
        let dict = retain_object(CpdfDictionary::new());
        let bookmark = CpdfBookmark::new(Some(dict.clone()));
        assert_eq!(bookmark.color(), None);
        let mut rgb = CpdfArray::new();
        for v in [0.25, 0.5, 0.75] {
            rgb.append_new(CpdfScalarObject::number_from_float(v));
        }
        let rgb = retain_object(rgb);
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"C", Some(rgb.clone()));
        assert_eq!(bookmark.color(), Some([0.25, 0.5, 0.75]));
        rgb.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .set_new_at(1, CpdfScalarObject::null());
        assert_eq!(bookmark.color(), None);
    }
}
