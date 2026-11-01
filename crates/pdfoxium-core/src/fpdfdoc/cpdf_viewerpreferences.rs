// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `core/fpdfdoc/cpdf_viewerpreferences.{h,cpp}`.
use crate::fpdfapi::parser::{
    cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument, cpdf_object::ObjectRef,
    cpdf_scalar_object::ObjectType,
};
pub struct CpdfViewerPreferences<'a> {
    document: &'a CpdfDocument,
}
impl<'a> CpdfViewerPreferences<'a> {
    pub fn new(document: &'a CpdfDocument) -> Self {
        Self { document }
    }
    fn preferences(&self) -> Option<ObjectRef> {
        self.document
            .root()?
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_dict_for(b"ViewerPreferences")
    }
    fn bytes(&self, key: &[u8]) -> Option<Vec<u8>> {
        Some(
            self.preferences()?
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_byte_string_for(key, b""),
        )
    }
    pub fn is_direction_r2l(&self) -> bool {
        self.bytes(b"Direction").is_some_and(|name| name == b"R2L")
    }
    pub fn print_scaling(&self) -> bool {
        self.bytes(b"PrintScaling")
            .is_none_or(|name| name != b"None")
    }
    pub fn num_copies(&self) -> i32 {
        self.preferences().map_or(1, |dict| {
            dict.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_integer_for(b"NumCopies", 0)
        })
    }
    pub fn print_page_range(&self) -> Option<ObjectRef> {
        self.preferences()?
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_array_for(b"PrintPageRange")
    }
    pub fn duplex(&self) -> Vec<u8> {
        self.bytes(b"Duplex").unwrap_or_else(|| b"None".to_vec())
    }
    pub fn generic_name(&self, key: &[u8]) -> Option<Vec<u8>> {
        let object = self
            .preferences()?
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_object_for(key)?;
        let borrowed = object.borrow();
        (borrowed.object_type() == ObjectType::Name).then(|| borrowed.get_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{
        cpdf_object::retain_object, cpdf_scalar_object::CpdfScalarObject, cpdf_string::CpdfString,
    };
    #[test]
    fn absent_dictionary_defaults_differ_from_missing_entries() {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let prefs = CpdfViewerPreferences::new(&document);
        assert!(!prefs.is_direction_r2l());
        assert!(prefs.print_scaling());
        assert_eq!(prefs.num_copies(), 1);
        assert_eq!(prefs.duplex(), b"None");
        assert!(prefs.generic_name(b"X").is_none());
        assert!(prefs.print_page_range().is_none());
        let dictionary = retain_object(CpdfDictionary::new());
        document
            .root()
            .unwrap()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"ViewerPreferences", Some(dictionary.clone()));
        assert_eq!(prefs.num_copies(), 0);
        assert_eq!(prefs.duplex(), b"");
        let mut borrowed = dictionary.borrow_mut();
        let dict = borrowed
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap();
        dict.set_new_name_for(b"Direction", b"R2L");
        dict.set_new_name_for(b"PrintScaling", b"None");
        dict.set_new_for(b"NumCopies", CpdfScalarObject::number_from_signed(3));
        dict.set_new_name_for(b"Duplex", b"DuplexFlipLongEdge");
        dict.set_new_for(b"X", CpdfString::from_bytes(None, b"not a name"));
        drop(borrowed);
        assert!(prefs.is_direction_r2l());
        assert!(!prefs.print_scaling());
        assert_eq!(prefs.num_copies(), 3);
        assert_eq!(
            prefs.generic_name(b"Duplex"),
            Some(b"DuplexFlipLongEdge".to_vec())
        );
        assert!(prefs.generic_name(b"X").is_none());
    }
}
