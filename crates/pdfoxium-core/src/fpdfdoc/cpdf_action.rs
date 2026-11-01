// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpdf_action.{h,cpp}`: action dictionaries, destinations and data.
//! JavaScript is returned as PDF text; executing it belongs to the script host.
use super::cpdf_dest::CpdfDest;
use super::cpdf_filespec::{decode_ansi, CpdfFileSpec};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::fpdf_parser_utility::validate_dict_optional_type;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ActionType {
    Unknown = 0,
    GoTo,
    GoToR,
    GoToE,
    Launch,
    Thread,
    Uri,
    Sound,
    Movie,
    Hide,
    Named,
    SubmitForm,
    ResetForm,
    ImportData,
    JavaScript,
    SetOcgState,
    Rendition,
    Trans,
    GoTo3DView,
}
const TYPES: [(&[u8], ActionType); 18] = [
    (b"GoTo", ActionType::GoTo),
    (b"GoToR", ActionType::GoToR),
    (b"GoToE", ActionType::GoToE),
    (b"Launch", ActionType::Launch),
    (b"Thread", ActionType::Thread),
    (b"URI", ActionType::Uri),
    (b"Sound", ActionType::Sound),
    (b"Movie", ActionType::Movie),
    (b"Hide", ActionType::Hide),
    (b"Named", ActionType::Named),
    (b"SubmitForm", ActionType::SubmitForm),
    (b"ResetForm", ActionType::ResetForm),
    (b"ImportData", ActionType::ImportData),
    (b"JavaScript", ActionType::JavaScript),
    (b"SetOCGState", ActionType::SetOcgState),
    (b"Rendition", ActionType::Rendition),
    (b"Trans", ActionType::Trans),
    (b"GoTo3DView", ActionType::GoTo3DView),
];
fn entry(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    let value = object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)?;
    get_direct_object(&value)
}
fn dictionary(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    entry(object, key).filter(|v| v.borrow().object_type() == ObjectType::Dictionary)
}
#[derive(Clone, Default)]
pub struct CpdfAction {
    dict: Option<ObjectRef>,
}
impl CpdfAction {
    pub fn new(dict: Option<ObjectRef>) -> Self {
        Self {
            dict: dict.filter(|v| v.borrow().object_type() == ObjectType::Dictionary),
        }
    }
    pub fn has_dict(&self) -> bool {
        self.dict.is_some()
    }
    pub fn dict(&self) -> Option<ObjectRef> {
        self.dict.clone()
    }
    pub fn action_type(&self) -> ActionType {
        let Some(dict) = &self.dict else {
            return ActionType::Unknown;
        };
        let valid = {
            let borrowed = dict.borrow();
            validate_dict_optional_type(
                borrowed.as_any().downcast_ref::<CpdfDictionary>(),
                b"Action",
            )
        };
        if !valid {
            return ActionType::Unknown;
        }
        let Some(kind) = entry(dict, b"S").filter(|v| v.borrow().object_type() == ObjectType::Name)
        else {
            return ActionType::Unknown;
        };
        let name = kind.borrow().get_string();
        TYPES
            .iter()
            .find(|(key, _)| *key == name)
            .map_or(ActionType::Unknown, |(_, value)| *value)
    }
    pub fn dest(&self, document: &CpdfDocument) -> CpdfDest {
        if !matches!(
            self.action_type(),
            ActionType::GoTo | ActionType::GoToR | ActionType::GoToE
        ) {
            return CpdfDest::default();
        }
        CpdfDest::create(document, self.dict.as_ref().and_then(|d| entry(d, b"D")))
    }
    pub fn file_path(&self) -> Vec<u32> {
        if !matches!(
            self.action_type(),
            ActionType::GoToR
                | ActionType::GoToE
                | ActionType::Launch
                | ActionType::SubmitForm
                | ActionType::ImportData
        ) {
            return Vec::new();
        }
        let Some(dict) = &self.dict else {
            return Vec::new();
        };
        if let Some(file) = entry(dict, b"F") {
            return CpdfFileSpec::new(file).file_name();
        }
        if self.action_type() != ActionType::Launch {
            return Vec::new();
        }
        dictionary(dict, b"Win")
            .and_then(|win| entry(&win, b"F"))
            .map_or_else(Vec::new, |f| decode_ansi(&f.borrow().get_string()))
    }
    pub fn uri(&self, document: &CpdfDocument) -> Vec<u8> {
        if self.action_type() != ActionType::Uri {
            return Vec::new();
        }
        let mut uri = self
            .dict
            .as_ref()
            .and_then(|d| entry(d, b"URI"))
            .map_or_else(Vec::new, |v| v.borrow().get_string());
        let colon = uri.iter().position(|&c| c == b':');
        if colon.is_none() || colon == Some(0) {
            let base = document
                .root()
                .and_then(|r| dictionary(&r, b"URI"))
                .and_then(|uri| entry(&uri, b"Base"));
            if let Some(base) = base.filter(|v| {
                matches!(
                    v.borrow().object_type(),
                    ObjectType::String | ObjectType::Stream
                )
            }) {
                let mut prefix = base.borrow().get_string();
                prefix.extend_from_slice(&uri);
                uri = prefix;
            }
        }
        uri
    }
    pub fn hide_status(&self) -> bool {
        self.dict.as_ref().map_or(true, |dict| {
            dict.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_boolean_for(b"H", true)
        })
    }
    pub fn named_action(&self) -> Vec<u8> {
        self.dict
            .as_ref()
            .and_then(|d| entry(d, b"N"))
            .map_or_else(Vec::new, |v| v.borrow().get_string())
    }
    pub fn flags(&self) -> u32 {
        self.dict
            .as_ref()
            .and_then(|d| entry(d, b"Flags"))
            .map_or(0, |v| v.borrow().get_integer() as u32)
    }
    pub fn has_fields(&self) -> bool {
        self.dict.as_ref().is_some_and(|d| {
            d.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .key_exist(b"Fields")
        })
    }
    pub fn all_fields(&self) -> Vec<ObjectRef> {
        let Some(dict) = &self.dict else {
            return Vec::new();
        };
        let hide = entry(dict, b"S").is_some_and(|v| v.borrow().get_string() == b"Hide");
        let Some(fields) = entry(dict, if hide { b"T" } else { b"Fields" }) else {
            return Vec::new();
        };
        let kind = fields.borrow().object_type();
        if !hide && kind != ObjectType::Array {
            return Vec::new();
        }
        if matches!(kind, ObjectType::Dictionary | ObjectType::String) {
            return vec![fields];
        }
        if kind != ObjectType::Array {
            return Vec::new();
        }
        let size = fields
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .size();
        (0..size)
            .filter_map(|i| {
                let value = fields
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .unwrap()
                    .get_object_at(i)?;
                get_direct_object(&value)
            })
            .collect()
    }
    fn javascript_object(&self) -> Option<ObjectRef> {
        entry(self.dict.as_ref()?, b"JS").filter(|v| {
            matches!(
                v.borrow().object_type(),
                ObjectType::String | ObjectType::Stream
            )
        })
    }
    pub fn maybe_javascript(&self) -> Option<Vec<u32>> {
        Some(self.javascript_object()?.borrow().get_unicode_text())
    }
    pub fn javascript(&self) -> Vec<u32> {
        self.maybe_javascript().unwrap_or_default()
    }
    pub fn sub_actions_count(&self) -> usize {
        let Some(next) = self.dict.as_ref().and_then(|d| entry(d, b"Next")) else {
            return 0;
        };
        let borrowed = next.borrow();
        match borrowed.object_type() {
            ObjectType::Dictionary => 1,
            ObjectType::Array => borrowed
                .as_any()
                .downcast_ref::<CpdfArray>()
                .unwrap()
                .size(),
            _ => 0,
        }
    }
    pub fn sub_action(&self, index: usize) -> Self {
        let Some(next) = self.dict.as_ref().and_then(|d| entry(d, b"Next")) else {
            return Self::default();
        };
        let kind = next.borrow().object_type();
        if kind == ObjectType::Dictionary {
            return if index == 0 {
                Self::new(Some(next))
            } else {
                Self::default()
            };
        }
        let value = next
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .and_then(|a| a.get_object_at(index));
        Self::new(value.and_then(|v| get_direct_object(&v)))
    }
}
#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    #[test]
    fn GetType() {
        let cases: &[(&[u8], ActionType)] = &[
            (b"GoTo", ActionType::GoTo),
            (b"GoToR", ActionType::GoToR),
            (b"GoToE", ActionType::GoToE),
            (b"Launch", ActionType::Launch),
            (b"Thread", ActionType::Thread),
            (b"URI", ActionType::Uri),
            (b"Sound", ActionType::Sound),
            (b"Movie", ActionType::Movie),
            (b"Hide", ActionType::Hide),
            (b"Named", ActionType::Named),
            (b"SubmitForm", ActionType::SubmitForm),
            (b"ResetForm", ActionType::ResetForm),
            (b"ImportData", ActionType::ImportData),
            (b"JavaScript", ActionType::JavaScript),
            (b"SetOCGState", ActionType::SetOcgState),
            (b"Rendition", ActionType::Rendition),
            (b"Trans", ActionType::Trans),
            (b"GoTo3DView", ActionType::GoTo3DView),
        ];
        for &(name, expected) in cases {
            for type_mode in 0..3 {
                let mut dict = CpdfDictionary::new();
                if type_mode != 0 {
                    dict.set_new_name_for(
                        b"Type",
                        if type_mode == 1 { b"Action" } else { b"Lights" },
                    );
                }
                dict.set_new_name_for(b"S", name);
                assert_eq!(
                    CpdfAction::new(Some(retain_object(dict))).action_type(),
                    if type_mode == 2 {
                        ActionType::Unknown
                    } else {
                        expected
                    }
                );
            }
            for with_type in [true, false] {
                let mut dict = CpdfDictionary::new();
                if with_type {
                    dict.set_new_name_for(b"Type", b"Action");
                }
                dict.set_new_for(b"S", CpdfString::from_bytes(None, name));
                assert_eq!(
                    CpdfAction::new(Some(retain_object(dict))).action_type(),
                    ActionType::Unknown
                );
            }
        }
        for name in [b"Camera".as_slice(), b"Javascript", b"Unknown"] {
            let mut dict = CpdfDictionary::new();
            dict.set_new_name_for(b"Type", b"Action");
            dict.set_new_name_for(b"S", name);
            assert_eq!(
                CpdfAction::new(Some(retain_object(dict))).action_type(),
                ActionType::Unknown
            );
        }
    }
    #[test]
    fn javascript_empty_vs_absent_and_nested_actions() {
        let object = retain_object(CpdfDictionary::new());
        let action = CpdfAction::new(Some(object.clone()));
        assert_eq!(action.maybe_javascript(), None);
        object
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"JS", CpdfString::from_bytes(None, b""));
        assert_eq!(action.maybe_javascript(), Some(Vec::new()));
        object
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Next", CpdfDictionary::new());
        assert_eq!(action.sub_actions_count(), 1);
        assert!(action.sub_action(0).has_dict());
        assert!(!action.sub_action(1).has_dict());
    }
}
