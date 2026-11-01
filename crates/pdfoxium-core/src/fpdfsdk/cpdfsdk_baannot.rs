// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Dictionary, geometry, appearance-state and action portions of
//! `fpdfsdk/cpdfsdk_baannot.{h,cpp}`. Page-view drawing, focus, popup events and
//! host invalidation are composed by `CpdfSdkAnnot` in `cpdfsdk_annot.rs`.
//!
//! This wrapper shares the actual annotation dictionary; it has no separate
//! appearance cache or borrowed page-view pointer.

use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfdoc::cpdf_aaction::{AActionType, CpdfAAction};
use crate::fpdfdoc::cpdf_action::CpdfAction;
use crate::fpdfdoc::cpdf_annot::{annot_rect, Subtype};
use crate::fpdfdoc::cpdf_dest::CpdfDest;
use crate::fxcrt::fx_coordinates::CfxFloatRect;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorderStyle {
    Solid,
    Dash,
    Beveled,
    Inset,
    Underline,
}

pub struct CpdfSdkBaAnnot {
    dict: ObjectRef,
}

impl CpdfSdkBaAnnot {
    pub fn new(dict: ObjectRef) -> Self {
        assert!(
            dict.borrow().as_any().is::<CpdfDictionary>(),
            "annotation dictionary"
        );
        Self { dict }
    }

    pub fn dict(&self) -> ObjectRef {
        self.dict.clone()
    }

    fn with_dict<R>(&self, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
        let borrowed = self.dict.borrow();
        f(borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("annotation dictionary"))
    }

    fn with_dict_mut<R>(&self, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
        let mut borrowed = self.dict.borrow_mut();
        f(borrowed
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .expect("annotation dictionary"))
    }

    pub fn rect(&self) -> CfxFloatRect {
        self.with_dict(annot_rect)
    }

    pub fn subtype(&self) -> Subtype {
        self.with_dict(|dict| Subtype::from_pdf(&dict.get_byte_string_for(b"Subtype", b"")))
    }

    pub fn view_bbox(&self) -> CfxFloatRect {
        self.rect()
    }

    pub fn layout_order(&self) -> i32 {
        if self.subtype() == Subtype::Popup {
            1
        } else {
            5
        }
    }

    pub fn ap_dict(&self) -> ObjectRef {
        self.with_dict_mut(|dict| dict.get_or_create_dict_for(b"AP"))
    }

    pub fn is_appearance_valid(&self) -> bool {
        self.with_dict(|dict| dict.get_dict_for(b"AP").is_some())
    }

    pub fn set_annot_name(&self, name: &[u32]) {
        self.with_dict_mut(|dict| {
            if name.is_empty() {
                dict.remove_for(b"NM");
            } else {
                dict.set_new_for(b"NM", CpdfString::from_unicode(None, name));
            }
        });
    }

    pub fn annot_name(&self) -> Vec<u32> {
        self.with_dict(|dict| dict.get_unicode_text_for(b"NM"))
    }

    pub fn set_flags(&self, flags: u32) {
        self.with_dict_mut(|dict| {
            dict.set_new_for(b"F", CpdfScalarObject::number_from_signed(flags as i32));
        });
    }

    pub fn flags(&self) -> u32 {
        self.with_dict(|dict| dict.get_integer_for(b"F", 0) as u32)
    }

    pub fn set_app_state_off(&self) {
        self.with_dict_mut(|dict| {
            dict.set_new_string_for(b"AS", b"Off");
        });
    }

    pub fn app_state(&self) -> Vec<u8> {
        self.with_dict(|dict| dict.get_byte_string_for(b"AS", b""))
    }

    pub fn set_border_width(&self, width: i32) {
        let border = self.with_dict(|dict| dict.get_array_for(b"Border"));
        if let Some(border) = border {
            let mut borrowed = border.borrow_mut();
            let border = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfArray>()
                .expect("border array");
            border.set_new_at(2, CpdfScalarObject::number_from_signed(width));
            return;
        }
        let bs = self.with_dict_mut(|dict| dict.get_or_create_dict_for(b"BS"));
        bs.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .expect("border style dictionary")
            .set_new_for(b"W", CpdfScalarObject::number_from_signed(width));
    }

    pub fn border_width(&self) -> i32 {
        let (border, bs) =
            self.with_dict(|dict| (dict.get_array_for(b"Border"), dict.get_dict_for(b"BS")));
        if let Some(border) = border {
            return border
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .expect("border array")
                .get_integer_at(2);
        }
        bs.map_or(1, |bs| {
            let borrowed = bs.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .expect("border style dictionary")
                .get_integer_for(b"W", 1)
        })
    }

    pub fn set_border_style(&self, style: BorderStyle) {
        let name: &[u8] = match style {
            BorderStyle::Solid => b"S",
            BorderStyle::Dash => b"D",
            BorderStyle::Beveled => b"B",
            BorderStyle::Inset => b"I",
            BorderStyle::Underline => b"U",
        };
        let bs = self.with_dict_mut(|dict| dict.get_or_create_dict_for(b"BS"));
        bs.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .expect("border style dictionary")
            .set_new_name_for(b"S", name);
    }

    pub fn border_style(&self) -> BorderStyle {
        let (bs, border) =
            self.with_dict(|dict| (dict.get_dict_for(b"BS"), dict.get_array_for(b"Border")));
        if let Some(bs) = bs {
            let style = bs
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .expect("border style dictionary")
                .get_byte_string_for(b"S", b"S");
            match style.as_slice() {
                b"S" => return BorderStyle::Solid,
                b"D" => return BorderStyle::Dash,
                b"B" => return BorderStyle::Beveled,
                b"I" => return BorderStyle::Inset,
                b"U" => return BorderStyle::Underline,
                _ => {}
            }
        }
        if let Some(border) = border {
            let dash = border
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .expect("border array")
                .get_array_at(3);
            if dash.is_some_and(|dash| {
                let borrowed = dash.borrow();
                !borrowed
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .expect("dash array")
                    .is_empty()
            }) {
                return BorderStyle::Dash;
            }
        }
        BorderStyle::Solid
    }

    pub fn is_visible(&self) -> bool {
        // Invisible, Hidden and NoView.
        self.flags() & (1 | 2 | 32) == 0
    }

    pub fn action(&self) -> CpdfAction {
        CpdfAction::new(self.with_dict(|dict| dict.get_dict_for(b"A")))
    }

    pub fn additional_actions(&self) -> CpdfAAction {
        CpdfAAction::new(self.with_dict(|dict| dict.get_dict_for(b"AA")))
    }

    pub fn additional_action(&self, kind: AActionType) -> CpdfAction {
        let actions = self.additional_actions();
        if actions.action_exist(kind) {
            return actions.action(kind);
        }
        if matches!(kind, AActionType::ButtonUp | AActionType::KeyStroke) {
            return self.action();
        }
        CpdfAction::default()
    }

    pub fn destination(&self, document: &CpdfDocument) -> CpdfDest {
        if self.subtype() != Subtype::Link {
            return CpdfDest::default();
        }
        let dest = self.with_dict(|dict| dict.get_direct_object_for(b"Dest"));
        CpdfDest::create(document, dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfdoc::cpdf_action::ActionType;

    fn annot(dict: CpdfDictionary) -> CpdfSdkBaAnnot {
        CpdfSdkBaAnnot::new(retain_object(dict))
    }

    #[test]
    fn border_array_width_and_border_style_dictionary_have_different_precedence() {
        let mut border = CpdfArray::new();
        for n in [0, 0, 3] {
            border.append_new(CpdfScalarObject::number_from_signed(n));
        }
        let mut dash = CpdfArray::new();
        dash.append_new(CpdfScalarObject::number_from_signed(2));
        border.append_new(dash);
        let mut bs = CpdfDictionary::new();
        bs.set_new_for(b"W", CpdfScalarObject::number_from_signed(7));
        bs.set_new_name_for(b"S", b"I");
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Border", border);
        dict.set_new_for(b"BS", bs);
        let annot = annot(dict);
        assert_eq!(annot.border_width(), 3);
        assert_eq!(annot.border_style(), BorderStyle::Inset);
        annot.set_border_width(4);
        assert_eq!(annot.border_width(), 4);
        annot.set_border_style(BorderStyle::Underline);
        assert_eq!(annot.border_style(), BorderStyle::Underline);
        annot.with_dict_mut(|dict| {
            let bs = dict.get_dict_for(b"BS").unwrap();
            bs.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_name_for(b"S", b"Unknown");
        });
        assert_eq!(annot.border_style(), BorderStyle::Dash);
    }

    #[test]
    fn missing_border_width_defaults_and_visibility_flags() {
        let annot = annot(CpdfDictionary::new());
        assert_eq!(annot.border_width(), 1);
        assert!(annot.is_visible());
        for flag in [1, 2, 32, u32::MAX] {
            annot.set_flags(flag);
            assert_eq!(annot.flags(), flag);
            assert!(!annot.is_visible());
        }
        annot.set_flags(4 | 8 | 16);
        assert!(annot.is_visible());
        annot.set_border_width(5);
        assert_eq!(annot.border_width(), 5);
    }

    #[test]
    fn explicit_additional_action_suppresses_primary_fallback_even_when_malformed() {
        let mut primary = CpdfDictionary::new();
        primary.set_new_name_for(b"S", b"URI");
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"A", primary);
        let annot = annot(dict);
        assert_eq!(
            annot.additional_action(AActionType::ButtonUp).action_type(),
            ActionType::Uri
        );
        assert_eq!(
            annot
                .additional_action(AActionType::KeyStroke)
                .action_type(),
            ActionType::Uri
        );
        assert!(!annot.additional_action(AActionType::ButtonDown).has_dict());
        annot.with_dict_mut(|dict| {
            let mut aa = CpdfDictionary::new();
            aa.set_new_string_for(b"U", b"malformed");
            dict.set_new_for(b"AA", aa);
        });
        assert!(!annot.additional_action(AActionType::ButtonUp).has_dict());
        assert_eq!(
            annot
                .additional_action(AActionType::KeyStroke)
                .action_type(),
            ActionType::Uri
        );
    }

    #[test]
    fn name_geometry_state_and_empty_appearance_share_the_annotation_dictionary() {
        let mut dict = CpdfDictionary::new();
        dict.set_rect_for(b"Rect", CfxFloatRect::new(12.0, 10.0, 2.0, 3.0));
        dict.set_new_name_for(b"Subtype", b"Popup");
        let annot = annot(dict);
        assert_eq!(annot.rect(), CfxFloatRect::new(2.0, 3.0, 12.0, 10.0));
        assert_eq!(annot.layout_order(), 1);
        let name = [0x1f4da, u32::from(b'A')];
        annot.set_annot_name(&name);
        assert_eq!(annot.annot_name(), name);
        annot.set_annot_name(&[]);
        assert!(!annot.with_dict(|dict| dict.key_exist(b"NM")));
        annot.set_app_state_off();
        assert_eq!(annot.app_state(), b"Off");
        assert!(!annot.is_appearance_valid());
        let ap = annot.ap_dict();
        assert!(annot.is_appearance_valid());
        assert!(std::rc::Rc::ptr_eq(&ap, &annot.ap_dict()));
    }
}
