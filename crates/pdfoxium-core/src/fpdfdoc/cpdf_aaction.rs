// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpdf_aaction.{h,cpp}` additional-action event mapping.
use super::cpdf_action::CpdfAction;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AActionType {
    CursorEnter = 0,
    CursorExit,
    ButtonDown,
    ButtonUp,
    GetFocus,
    LoseFocus,
    PageOpen,
    PageClose,
    PageVisible,
    PageInvisible,
    OpenPage,
    ClosePage,
    KeyStroke,
    Format,
    Validate,
    Calculate,
    CloseDocument,
    SaveDocument,
    DocumentSaved,
    PrintDocument,
    DocumentPrinted,
    DocumentOpen,
}
const KEYS: [&[u8]; 21] = [
    b"E", b"X", b"D", b"U", b"Fo", b"Bl", b"PO", b"PC", b"PV", b"PI", b"O", b"C", b"K", b"F", b"V",
    b"C", b"WC", b"WS", b"DS", b"WP", b"DP",
];
#[derive(Clone, Default)]
pub struct CpdfAAction {
    dict: Option<ObjectRef>,
}
impl CpdfAAction {
    pub fn new(dict: Option<ObjectRef>) -> Self {
        Self {
            dict: dict.filter(|object| object.borrow().as_any().is::<CpdfDictionary>()),
        }
    }
    pub fn has_dict(&self) -> bool {
        self.dict.is_some()
    }
    pub fn action_exist(&self, kind: AActionType) -> bool {
        let Some(key) = KEYS.get(kind as usize) else {
            return false;
        };
        self.dict.as_ref().is_some_and(|d| {
            d.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .is_some_and(|d| d.key_exist(key))
        })
    }
    pub fn action(&self, kind: AActionType) -> CpdfAction {
        let value = (|| {
            let key = KEYS.get(kind as usize)?;
            let value = self
                .dict
                .as_ref()?
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_object_for(key)?;
            get_direct_object(&value)
        })();
        CpdfAction::new(value)
    }
    pub fn is_user_input(kind: AActionType) -> bool {
        matches!(
            kind,
            AActionType::ButtonUp | AActionType::ButtonDown | AActionType::KeyStroke
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    #[test]
    fn additional_action_event_keys_and_artificial_document_open() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"K", CpdfDictionary::new());
        let action = CpdfAAction::new(Some(retain_object(dict)));
        assert!(action.action_exist(AActionType::KeyStroke));
        assert!(action.action(AActionType::KeyStroke).has_dict());
        assert!(!action.action_exist(AActionType::DocumentOpen));
        assert!(!action.action(AActionType::DocumentOpen).has_dict());
        assert!(CpdfAAction::is_user_input(AActionType::ButtonDown));
        assert!(CpdfAAction::is_user_input(AActionType::ButtonUp));
        assert!(CpdfAAction::is_user_input(AActionType::KeyStroke));
        assert!(!CpdfAAction::is_user_input(AActionType::Validate));
    }
}
