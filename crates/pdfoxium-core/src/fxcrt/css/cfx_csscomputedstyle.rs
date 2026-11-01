// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Computed style storage from `cfx_csscomputedstyle.{h,cpp}`. Inherited
//! immutable CSS lists share their values; custom properties own their text.
use super::cfx_css::{
    CfxCssDisplay, CfxCssFontStyle, CfxCssFontVariant, CfxCssLength, CfxCssLengthUnit, CfxCssRect,
    CfxCssTextAlign, CfxCssVerticalAlign,
};
use super::cfx_csscustomproperty::CfxCssCustomProperty;
use super::cfx_cssvalue::CfxCssValue;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct InheritedData {
    pub letter_spacing: CfxCssLength,
    pub word_spacing: CfxCssLength,
    pub text_indent: CfxCssLength,
    pub font_family: Option<Rc<CfxCssValue>>,
    pub font_size: f32,
    pub line_height: f32,
    pub font_color: u32,
    pub font_weight: u16,
    pub font_variant: CfxCssFontVariant,
    pub font_style: CfxCssFontStyle,
    pub text_align: CfxCssTextAlign,
}
impl Default for InheritedData {
    fn default() -> Self {
        Self {
            letter_spacing: CfxCssLength::new(CfxCssLengthUnit::Normal, 0.0),
            word_spacing: CfxCssLength::new(CfxCssLengthUnit::Normal, 0.0),
            text_indent: CfxCssLength::new(CfxCssLengthUnit::Point, 0.0),
            font_family: None,
            font_size: 12.0,
            line_height: 14.0,
            font_color: 0xff000000,
            font_weight: 400,
            font_variant: CfxCssFontVariant::Normal,
            font_style: CfxCssFontStyle::Normal,
            text_align: CfxCssTextAlign::Left,
        }
    }
}

#[derive(Clone, Debug)]
pub struct NonInheritedData {
    pub margin_width: CfxCssRect,
    pub border_width: CfxCssRect,
    pub padding_width: CfxCssRect,
    pub top: CfxCssLength,
    pub bottom: CfxCssLength,
    pub left: CfxCssLength,
    pub right: CfxCssLength,
    pub vertical_align: f32,
    pub display: CfxCssDisplay,
    pub vertical_align_type: CfxCssVerticalAlign,
    pub text_decoration: u8,
    pub has_margin: bool,
    pub has_border: bool,
    pub has_padding: bool,
}
impl Default for NonInheritedData {
    fn default() -> Self {
        Self {
            margin_width: CfxCssRect::new(CfxCssLengthUnit::Point, 0.0),
            border_width: CfxCssRect::new(CfxCssLengthUnit::Point, 0.0),
            padding_width: CfxCssRect::new(CfxCssLengthUnit::Point, 0.0),
            // The source leaves its unadorned length members uninitialized.
            // CSS offsets have a defined initial value of auto.
            top: CfxCssLength::new(CfxCssLengthUnit::Auto, 0.0),
            bottom: CfxCssLength::new(CfxCssLengthUnit::Auto, 0.0),
            left: CfxCssLength::new(CfxCssLengthUnit::Auto, 0.0),
            right: CfxCssLength::new(CfxCssLengthUnit::Auto, 0.0),
            vertical_align: 0.0,
            display: CfxCssDisplay::Inline,
            vertical_align_type: CfxCssVerticalAlign::Baseline,
            text_decoration: 0,
            has_margin: false,
            has_border: false,
            has_padding: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CfxCssComputedStyle {
    pub inherited_data: InheritedData,
    pub non_inherited_data: NonInheritedData,
    custom_properties: Vec<CfxCssCustomProperty>,
}
impl CfxCssComputedStyle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_custom_style(&self, name: &[u32]) -> Option<&[u32]> {
        self.custom_properties
            .iter()
            .rev()
            .find(|p| p.name == name)
            .map(|p| p.value.as_slice())
    }
    pub fn get_last_font_family(&self) -> Option<Vec<u32>> {
        let CfxCssValue::List(values) = self.inherited_data.font_family.as_deref()? else {
            return None;
        };
        match values.last()?.as_ref() {
            CfxCssValue::String(value) => Some(value.clone()),
            _ => None,
        }
    }
    pub fn get_font_weight(&self) -> u16 {
        self.inherited_data.font_weight
    }
    pub fn get_font_variant(&self) -> CfxCssFontVariant {
        self.inherited_data.font_variant
    }
    pub fn get_font_style(&self) -> CfxCssFontStyle {
        self.inherited_data.font_style
    }
    pub fn get_font_size(&self) -> f32 {
        self.inherited_data.font_size
    }
    pub fn get_color(&self) -> u32 {
        self.inherited_data.font_color
    }
    pub fn set_font_weight(&mut self, value: u16) {
        self.inherited_data.font_weight = value;
    }
    pub fn set_font_variant(&mut self, value: CfxCssFontVariant) {
        self.inherited_data.font_variant = value;
    }
    pub fn set_font_style(&mut self, value: CfxCssFontStyle) {
        self.inherited_data.font_style = value;
    }
    pub fn set_font_size(&mut self, value: f32) {
        self.inherited_data.font_size = value;
    }
    pub fn set_color(&mut self, value: u32) {
        self.inherited_data.font_color = value;
    }
    pub fn get_border_width(&self) -> Option<&CfxCssRect> {
        self.non_inherited_data
            .has_border
            .then_some(&self.non_inherited_data.border_width)
    }
    pub fn get_margin_width(&self) -> Option<&CfxCssRect> {
        self.non_inherited_data
            .has_margin
            .then_some(&self.non_inherited_data.margin_width)
    }
    pub fn get_padding_width(&self) -> Option<&CfxCssRect> {
        self.non_inherited_data
            .has_padding
            .then_some(&self.non_inherited_data.padding_width)
    }
    pub fn set_margin_width(&mut self, value: &CfxCssRect) {
        self.non_inherited_data.margin_width = *value;
        self.non_inherited_data.has_margin = true;
    }
    pub fn set_padding_width(&mut self, value: &CfxCssRect) {
        self.non_inherited_data.padding_width = *value;
        self.non_inherited_data.has_padding = true;
    }
    pub fn get_display(&self) -> CfxCssDisplay {
        self.non_inherited_data.display
    }
    pub fn get_line_height(&self) -> f32 {
        self.inherited_data.line_height
    }
    pub fn get_text_indent(&self) -> &CfxCssLength {
        &self.inherited_data.text_indent
    }
    pub fn get_text_align(&self) -> CfxCssTextAlign {
        self.inherited_data.text_align
    }
    pub fn get_vertical_align(&self) -> CfxCssVerticalAlign {
        self.non_inherited_data.vertical_align_type
    }
    pub fn get_number_vertical_align(&self) -> f32 {
        self.non_inherited_data.vertical_align
    }
    pub fn get_text_decoration(&self) -> u8 {
        self.non_inherited_data.text_decoration
    }
    pub fn get_letter_spacing(&self) -> &CfxCssLength {
        &self.inherited_data.letter_spacing
    }
    pub fn set_line_height(&mut self, value: f32) {
        self.inherited_data.line_height = value;
    }
    pub fn set_text_indent(&mut self, value: &CfxCssLength) {
        self.inherited_data.text_indent = *value;
    }
    pub fn set_text_align(&mut self, value: CfxCssTextAlign) {
        self.inherited_data.text_align = value;
    }
    pub fn set_number_vertical_align(&mut self, value: f32) {
        self.non_inherited_data.vertical_align_type = CfxCssVerticalAlign::Number;
        self.non_inherited_data.vertical_align = value;
    }
    pub fn set_text_decoration(&mut self, value: u8) {
        self.non_inherited_data.text_decoration = value;
    }
    pub fn set_letter_spacing(&mut self, value: &CfxCssLength) {
        self.inherited_data.letter_spacing = *value;
    }
    pub fn add_custom_style(&mut self, property: &CfxCssCustomProperty) {
        self.custom_properties.push(property.clone());
    }
}
