// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Widget controls from `core/fpdfdoc/cpdf_formcontrol.{h,cpp}`.
use super::cpdf_apsettings::{CpdfApSettings, TextPosition};
use super::cpdf_defaultappearance::CpdfDefaultAppearance;
use super::cpdf_formfield::{
    array_at, dict_bytes, dict_get, dict_has, dict_int, dict_mut, field_attr, typed, CpdfFormField,
    FieldType,
};
use super::cpdf_iconfit::CpdfIconFit;
use super::cpdf_interactiveform::CpdfInteractiveForm;
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::fpdf_parser_decode::pdf_decode_text;
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use crate::fxge::cfx_color::{CfxColor, TypeAndArgb};
use std::rc::{Rc, Weak};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum HighlightingMode {
    None = 0,
    Invert = 1,
    Outline = 2,
    Push = 3,
    Toggle = 4,
}
pub struct CpdfFormControl {
    field: Weak<CpdfFormField>,
    widget_dict: ObjectRef,
    form: Weak<CpdfInteractiveForm>,
}
impl CpdfFormControl {
    pub(crate) fn new(
        field: &Rc<CpdfFormField>,
        widget_dict: ObjectRef,
        form: Weak<CpdfInteractiveForm>,
    ) -> Rc<Self> {
        Rc::new(Self {
            field: Rc::downgrade(field),
            widget_dict,
            form,
        })
    }
    pub fn field(&self) -> Option<Rc<CpdfFormField>> {
        self.field.upgrade()
    }
    pub fn get_type(&self) -> FieldType {
        self.field().map_or(FieldType::Unknown, |f| f.get_type())
    }
    pub fn widget_dict(&self) -> ObjectRef {
        Rc::clone(&self.widget_dict)
    }
    pub fn rect(&self) -> CfxFloatRect {
        self.widget_dict
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_rect_for(b"Rect")
    }
    pub fn on_state_name(&self) -> Vec<u8> {
        let Some(ap) = typed(dict_get(&self.widget_dict, b"AP"), ObjectType::Dictionary) else {
            return Vec::new();
        };
        let Some(normal) = typed(dict_get(&ap, b"N"), ObjectType::Dictionary) else {
            return Vec::new();
        };
        let borrowed = normal.borrow();
        let d = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        d.with_locked_entries(|entries| {
            entries
                .keys()
                .find(|k| k.as_ref() != b"Off")
                .map_or_else(Vec::new, |k| k.to_vec())
        })
    }
    pub fn checked_ap_state(&self) -> Vec<u8> {
        let mut state = self.on_state_name();
        if let Some(field) = self.field() {
            if typed(field.get_field_attr(b"Opt"), ObjectType::Array).is_some() {
                state = field.control_index(self).to_string().into_bytes();
            }
        }
        if state.is_empty() {
            b"Yes".to_vec()
        } else {
            state
        }
    }
    pub fn export_value(&self) -> Vec<u32> {
        let mut state = self.on_state_name();
        if let Some(field) = self.field() {
            if let Some(options) = typed(field.get_field_attr(b"Opt"), ObjectType::Array) {
                let index = field.control_index(self);
                state = if index < 0 {
                    Vec::new()
                } else {
                    array_at(&options, index as usize)
                        .map_or_else(Vec::new, |o| o.borrow().get_string())
                };
            }
        }
        if state.is_empty() {
            state = b"Yes".to_vec();
        }
        pdf_decode_text(&state)
    }
    pub fn is_checked(&self) -> bool {
        dict_bytes(&self.widget_dict, b"AS") == self.on_state_name()
    }
    pub fn is_default_checked(&self) -> bool {
        self.field()
            .and_then(|f| f.get_field_attr(b"DV"))
            .is_some_and(|o| o.borrow().get_string() == self.on_state_name())
    }
    pub fn check_control(&self, checked: bool) {
        let old = if dict_has(&self.widget_dict, b"AS") {
            dict_bytes(&self.widget_dict, b"AS")
        } else {
            b"Off".to_vec()
        };
        let state = if checked {
            self.on_state_name()
        } else {
            b"Off".to_vec()
        };
        if old != state {
            dict_mut(&self.widget_dict, |d| {
                d.set_new_name_for(b"AS", &state);
            });
        }
    }
    pub fn highlighting_mode(&self) -> HighlightingMode {
        match dict_bytes(&self.widget_dict, b"H").as_slice() {
            b"N" => HighlightingMode::None,
            b"O" => HighlightingMode::Outline,
            b"P" => HighlightingMode::Push,
            b"T" => HighlightingMode::Toggle,
            _ => HighlightingMode::Invert,
        }
    }
    fn mk(&self) -> CpdfApSettings {
        CpdfApSettings::new(typed(
            dict_get(&self.widget_dict, b"MK"),
            ObjectType::Dictionary,
        ))
    }
    pub fn has_mk_entry(&self, key: &[u8]) -> bool {
        self.mk().has_mk_entry(key)
    }
    pub fn rotation(&self) -> i32 {
        self.mk().rotation()
    }
    pub fn color_argb(&self, key: &[u8]) -> TypeAndArgb {
        self.mk().color_argb(key)
    }
    pub fn original_color_component(&self, index: i32, key: &[u8]) -> f32 {
        self.mk().original_color_component(index, key)
    }
    pub fn original_border_color(&self) -> CfxColor {
        self.mk().original_color(b"BC")
    }
    pub fn original_background_color(&self) -> CfxColor {
        self.mk().original_color(b"BG")
    }
    pub fn normal_caption(&self) -> Vec<u32> {
        self.mk().caption(b"CA")
    }
    pub fn rollover_caption(&self) -> Vec<u32> {
        self.mk().caption(b"RC")
    }
    pub fn down_caption(&self) -> Vec<u32> {
        self.mk().caption(b"AC")
    }
    pub fn normal_icon(&self) -> Option<ObjectRef> {
        self.mk().icon(b"I")
    }
    pub fn rollover_icon(&self) -> Option<ObjectRef> {
        self.mk().icon(b"RI")
    }
    pub fn down_icon(&self) -> Option<ObjectRef> {
        self.mk().icon(b"IX")
    }
    pub fn icon_fit(&self) -> CpdfIconFit {
        self.mk().icon_fit()
    }
    pub fn text_position(&self) -> TextPosition {
        self.mk().text_position()
    }
    pub fn default_appearance(&self) -> CpdfDefaultAppearance {
        if dict_has(&self.widget_dict, b"DA") {
            return CpdfDefaultAppearance::new(dict_bytes(&self.widget_dict, b"DA"));
        }
        if let Some(value) = self.field().and_then(|f| f.get_field_attr(b"DA")) {
            return CpdfDefaultAppearance::new(value.borrow().get_string());
        }
        self.form.upgrade().map_or_else(
            || CpdfDefaultAppearance::new(Vec::new()),
            |f| f.default_appearance(),
        )
    }
    pub fn default_control_font(&self) -> Option<CpdfFont> {
        let font_name = self.default_appearance().font()?.name;
        if font_name.is_empty() {
            return None;
        }
        let form = self.form.upgrade()?;
        let from_resources = |resources: Option<ObjectRef>| -> Option<CpdfFont> {
            let resources = resources?;
            let fonts = typed(dict_get(&resources, b"Font"), ObjectType::Dictionary)?;
            let valid = {
                let b = fonts.borrow();
                crate::fpdfapi::parser::fpdf_parser_utility::validate_font_resource_dict(
                    b.as_any().downcast_ref::<CpdfDictionary>(),
                )
            };
            if !valid {
                return None;
            }
            form.font_for_element(typed(dict_get(&fonts, &font_name), ObjectType::Dictionary)?)
        };
        if let Some(font) = from_resources(typed(
            field_attr(&self.widget_dict, b"DR"),
            ObjectType::Dictionary,
        )) {
            return Some(font);
        }
        if let Some(font) = form.form_font(&font_name) {
            return Some(font);
        }
        let page = typed(dict_get(&self.widget_dict, b"P"), ObjectType::Dictionary)?;
        from_resources(typed(
            field_attr(&page, b"Resources"),
            ObjectType::Dictionary,
        ))
    }
    pub fn default_control_font_name(&self) -> Option<Vec<u32>> {
        Some(
            self.default_control_font()?
                .base_font_name()
                .iter()
                .map(|&b| u32::from(b))
                .collect(),
        )
    }
    pub fn control_alignment(&self) -> i32 {
        if dict_has(&self.widget_dict, b"Q") {
            return dict_int(&self.widget_dict, b"Q", 0);
        }
        if let Some(value) = self.field().and_then(|f| f.get_field_attr(b"Q")) {
            return value.borrow().get_integer();
        }
        self.form.upgrade().map_or(0, |f| f.form_alignment())
    }
}
