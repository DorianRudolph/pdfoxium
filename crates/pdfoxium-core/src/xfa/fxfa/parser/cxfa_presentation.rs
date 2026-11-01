// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Model accessors for CXFA_Color, Font, Fill, Stroke, Box, Line, Margin,
//! Caption, Para, Calculate and Validate. Drawing is in `cxfa_geometry`.
use super::{cxfa_node::CxfaNode, xfa_basic_data::get_attribute_value_by_name};
use crate::{
    fxcrt::fx_system::wtoi,
    xfa::fxfa::fxfa_basic::{XfaAttribute, XfaAttributeValue, XfaElement, XfaUnit},
};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
pub fn color_to_string(color: u32) -> Vec<u32> {
    wide(&format!(
        "{},{},{}",
        (color >> 16) & 255,
        (color >> 8) & 255,
        color & 255
    ))
}
/// CXFA_Color::StringToFXARGB: comma-separated decimal channels. Byte
/// accumulation makes the native uint8_t narrowing explicit and overflow safe.
pub fn string_to_argb(text: &[u32]) -> u32 {
    let mut cursor = 0;
    let mut channels = [0u8; 3];
    for channel in &mut channels {
        while text.get(cursor).is_some_and(|c| matches!(*c, 9..=13 | 32)) {
            cursor += 1;
        }
        while let Some(digit) = text
            .get(cursor)
            .and_then(|c| c.checked_sub(48))
            .filter(|d| *d <= 9)
        {
            *channel = channel.wrapping_mul(10).wrapping_add(digit as u8);
            cursor += 1;
        }
        if text.get(cursor) != Some(&44) {
            break;
        }
        cursor += 1;
    }
    0xff000000
        | (u32::from(channels[0]) << 16)
        | (u32::from(channels[1]) << 8)
        | u32::from(channels[2])
}
impl CxfaNode {
    pub fn button_get_highlight(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Highlight)
    }
    pub fn check_button_is_round(&self) -> bool {
        self.get_enum(XfaAttribute::Shape) == XfaAttributeValue::Round
    }
    pub fn check_button_get_mark(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Mark)
    }
    pub fn check_button_is_allow_neutral(&self) -> bool {
        self.get_boolean(XfaAttribute::AllowNeutral)
    }
    pub fn password_edit_get_password_char(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::PasswordChar)
    }
    pub fn image_get_aspect(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Aspect)
    }
    pub fn image_get_content_type(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::ContentType)
    }
    pub fn image_get_href(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::Href)
    }
    pub fn image_get_transfer_encoding(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::TransferEncoding)
    }
    pub fn image_get_content(&self) -> Vec<u32> {
        self.try_content().unwrap_or_default()
    }
    pub fn image_set_content_type(&self, content_type: &[u32]) {
        self.set_cdata(XfaAttribute::ContentType, content_type);
    }
    pub fn image_set_href(&self, href: &[u32]) {
        self.set_cdata(XfaAttribute::Href, href);
    }
    pub fn image_set_transfer_encoding(&self, encoding: XfaAttributeValue) {
        self.set_enum(XfaAttribute::TransferEncoding, encoding, false);
    }
    pub fn exdata_set_content_type(&self, content_type: &[u32]) {
        self.set_cdata(XfaAttribute::ContentType, content_type);
    }
    pub fn text_get_content(&self) -> Vec<u32> {
        self.get_content()
    }
    pub fn linear_get_type(&self) -> XfaAttributeValue {
        self.try_enum(XfaAttribute::Type, true)
            .unwrap_or(XfaAttributeValue::ToRight)
    }
    pub fn radial_is_to_edge(&self) -> bool {
        self.try_enum(XfaAttribute::Type, true)
            .is_none_or(|v| v == XfaAttributeValue::ToEdge)
    }
    pub fn pattern_get_type(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Type)
    }
    /// Shared GetColorIfExists from the four concrete fill-style classes.
    pub fn fill_style_get_color_if_exists(&self) -> Option<Self> {
        self.get_child(0, XfaElement::Color, false)
    }
    pub fn stipple_get_rate(&self) -> i32 {
        self.try_integer(XfaAttribute::Rate, true).unwrap_or(50)
    }
    pub fn fill_is_visible(&self) -> bool {
        self.try_enum(XfaAttribute::Presence, true)
            .unwrap_or(XfaAttributeValue::Visible)
            == XfaAttributeValue::Visible
    }
    pub fn stroke_is_corner(&self) -> bool {
        self.get_element_type() == XfaElement::Corner
    }
    pub fn stroke_is_visible(&self) -> bool {
        self.fill_is_visible()
    }
    pub fn stroke_is_inverted(&self) -> bool {
        self.get_boolean(XfaAttribute::Inverted)
    }
    pub fn stroke_get_cap_type(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Cap)
    }
    pub fn stroke_get_stroke_type(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Stroke)
    }
    pub fn stroke_get_join_type(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Join)
    }
    pub fn stroke_get_thickness(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::Thickness, XfaUnit::Pt)
    }
    pub fn stroke_get_ms_thickness(&self) -> super::cxfa_measurement::CxfaMeasurement {
        self.get_measure(XfaAttribute::Thickness)
    }
    pub fn stroke_set_ms_thickness(&self, thickness: super::cxfa_measurement::CxfaMeasurement) {
        self.set_measure(XfaAttribute::Thickness, thickness, false);
    }
    pub fn stroke_get_radius(&self) -> f32 {
        self.try_measure(XfaAttribute::Radius, true)
            .map_or(0.0, |radius| radius.to_unit(XfaUnit::Pt))
    }
    /// CXFA_Stroke::SameStyles. Cap/join/inversion are deliberately not among
    /// the native comparison keys; the rectangle path handles corners itself.
    pub fn stroke_same_styles(&self, other: &Self, no_presence: bool, corner: bool) -> bool {
        self == other
            || ((self.stroke_get_thickness() - other.stroke_get_thickness()).abs() < 0.01
                && (no_presence || self.stroke_is_visible() == other.stroke_is_visible())
                && self.stroke_get_stroke_type() == other.stroke_get_stroke_type()
                && self.stroke_get_color() == other.stroke_get_color()
                && (!corner || (self.stroke_get_radius() - other.stroke_get_radius()).abs() < 0.01))
    }
    pub fn box_get_hand(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Hand)
    }
    pub fn box_get_presence(&self) -> XfaAttributeValue {
        self.try_enum(XfaAttribute::Presence, true)
            .unwrap_or(XfaAttributeValue::Visible)
    }
    pub fn box_count_edges(&self) -> usize {
        self.count_children(XfaElement::Edge, false)
    }
    pub fn box_get_or_create_fill(&self) -> Option<Self> {
        self.get_or_create_property(0, XfaElement::Fill)
    }
    pub fn line_get_hand(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Hand)
    }
    pub fn line_get_slope(&self) -> bool {
        self.get_enum(XfaAttribute::Slope) == XfaAttributeValue::Slash
    }
    pub fn line_get_edge_if_exists(&self) -> Option<Self> {
        self.get_child(0, XfaElement::Edge, false)
    }
    pub fn margin_try_left_inset(&self) -> Option<f32> {
        self.try_measure_as_float(XfaAttribute::LeftInset)
    }
    pub fn margin_try_top_inset(&self) -> Option<f32> {
        self.try_measure_as_float(XfaAttribute::TopInset)
    }
    pub fn margin_try_right_inset(&self) -> Option<f32> {
        self.try_measure_as_float(XfaAttribute::RightInset)
    }
    pub fn margin_try_bottom_inset(&self) -> Option<f32> {
        self.try_measure_as_float(XfaAttribute::BottomInset)
    }
    pub fn margin_get_left_inset(&self) -> f32 {
        self.margin_try_left_inset().unwrap_or(0.0)
    }
    pub fn margin_get_top_inset(&self) -> f32 {
        self.margin_try_top_inset().unwrap_or(0.0)
    }
    pub fn margin_get_right_inset(&self) -> f32 {
        self.margin_try_right_inset().unwrap_or(0.0)
    }
    pub fn margin_get_bottom_inset(&self) -> f32 {
        self.margin_try_bottom_inset().unwrap_or(0.0)
    }
    pub fn caption_is_visible(&self) -> bool {
        self.fill_is_visible()
    }
    pub fn caption_is_hidden(&self) -> bool {
        self.try_enum(XfaAttribute::Presence, true)
            .is_none_or(|p| p == XfaAttributeValue::Hidden)
    }
    pub fn caption_get_placement_type(&self) -> XfaAttributeValue {
        self.try_enum(XfaAttribute::Placement, true)
            .unwrap_or(XfaAttributeValue::Left)
    }
    pub fn caption_get_reserve(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::Reserve, XfaUnit::Pt)
    }
    pub fn caption_get_margin_if_exists(&self) -> Option<Self> {
        self.get_child(0, XfaElement::Margin, false)
    }
    pub fn caption_get_font_if_exists(&self) -> Option<Self> {
        self.get_child(0, XfaElement::Font, false)
    }
    pub fn caption_get_value_if_exists(&self) -> Option<Self> {
        self.get_child(0, XfaElement::Value, false)
    }
    pub fn para_get_horizontal_align(&self) -> XfaAttributeValue {
        self.try_enum(XfaAttribute::HAlign, true)
            .unwrap_or(XfaAttributeValue::Left)
    }
    pub fn para_get_vertical_align(&self) -> XfaAttributeValue {
        self.try_enum(XfaAttribute::VAlign, true)
            .unwrap_or(XfaAttributeValue::Top)
    }
    pub fn para_get_line_height(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::LineHeight, XfaUnit::Pt)
    }
    pub fn para_get_margin_left(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::MarginLeft, XfaUnit::Pt)
    }
    pub fn para_get_margin_right(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::MarginRight, XfaUnit::Pt)
    }
    pub fn para_get_space_above(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::SpaceAbove, XfaUnit::Pt)
    }
    pub fn para_get_space_below(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::SpaceBelow, XfaUnit::Pt)
    }
    pub fn para_get_text_indent(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::TextIndent, XfaUnit::Pt)
    }
    pub fn calculate_get_override(&self) -> XfaAttributeValue {
        self.try_enum(XfaAttribute::Override, false)
            .unwrap_or(XfaAttributeValue::Error)
    }
    pub fn calculate_get_script_if_exists(&self) -> Option<Self> {
        self.get_child(0, XfaElement::Script, false)
    }
    pub fn calculate_get_message_text(&self) -> Vec<u32> {
        self.get_child(0, XfaElement::Message, false)
            .and_then(|message| message.get_child(0, XfaElement::Text, false))
            .map_or_else(Vec::new, |text| text.get_content())
    }
    pub fn color_get_value_or_default(&self, default: u32) -> u32 {
        self.try_cdata(XfaAttribute::Value, false)
            .map_or(default, |value| string_to_argb(&value))
    }
    pub fn color_set_value(&self, color: u32) {
        self.set_cdata(XfaAttribute::Value, &color_to_string(color));
    }
    pub fn fill_set_color(&self, color: u32) {
        if let Some(node) = self.get_or_create_property(0, XfaElement::Color) {
            node.color_set_value(color);
        }
    }
    pub fn fill_get_color(&self, text: bool) -> u32 {
        let default = if text { 0xff000000 } else { 0xffffffff };
        self.get_property(0, XfaElement::Color)
            .0
            .map_or(default, |color| color.color_get_value_or_default(default))
    }
    pub fn fill_get_type(&self) -> XfaElement {
        let mut child = self.first_child();
        while let Some(node) = child {
            if !matches!(
                node.get_element_type(),
                XfaElement::Color | XfaElement::Extras
            ) {
                return node.get_element_type();
            }
            child = node.next_sibling();
        }
        XfaElement::Solid
    }
    pub fn font_get_color(&self) -> u32 {
        self.get_property(0, XfaElement::Fill)
            .0
            .map_or(0xff000000, |fill| fill.fill_get_color(true))
    }
    pub fn font_set_color(&self, color: u32) {
        if let Some(fill) = self.get_or_create_property(0, XfaElement::Fill) {
            fill.fill_set_color(color);
        }
    }
    pub fn font_get_baseline_shift(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::BaselineShift, XfaUnit::Pt)
    }
    pub fn font_get_horizontal_scale(&self) -> f32 {
        let scale = wtoi(&self.get_cdata(XfaAttribute::FontHorizontalScale));
        if scale > 0 {
            scale as f32
        } else {
            100.0
        }
    }
    pub fn font_get_vertical_scale(&self) -> f32 {
        let scale = wtoi(&self.get_cdata(XfaAttribute::FontVerticalScale));
        if scale > 0 {
            scale as f32
        } else {
            100.0
        }
    }
    pub fn font_get_letter_spacing(&self) -> f32 {
        let spacing = super::cxfa_measurement::CxfaMeasurement::from_string(
            &self.get_cdata(XfaAttribute::LetterSpacing),
        );
        if spacing.get_unit() == XfaUnit::Em {
            spacing.get_value() * self.font_get_size()
        } else {
            spacing.to_unit(XfaUnit::Pt)
        }
    }
    pub fn font_get_size(&self) -> f32 {
        self.get_measure_in_unit(XfaAttribute::Size, XfaUnit::Pt)
    }
    pub fn font_is_bold(&self) -> bool {
        self.get_enum(XfaAttribute::Weight) == XfaAttributeValue::Bold
    }
    pub fn font_is_italic(&self) -> bool {
        self.get_enum(XfaAttribute::Posture) == XfaAttributeValue::Italic
    }
    pub fn font_get_line_through(&self) -> i32 {
        self.get_integer(XfaAttribute::LineThrough)
    }
    pub fn font_get_underline(&self) -> i32 {
        self.get_integer(XfaAttribute::Underline)
    }
    pub fn font_get_underline_period(&self) -> XfaAttributeValue {
        self.try_enum(XfaAttribute::UnderlinePeriod, true)
            .unwrap_or(XfaAttributeValue::All)
    }
    pub fn font_get_typeface(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::Typeface)
    }
    pub fn box_get_edge(&self, index: i32) -> Option<Self> {
        if index == 0 {
            self.get_or_create_property(index, XfaElement::Edge)
        } else {
            self.get_property(index, XfaElement::Edge).0
        }
    }
    pub fn stroke_get_color(&self) -> u32 {
        self.get_property(0, XfaElement::Color)
            .0
            .map_or(0xff000000, |color| {
                string_to_argb(&color.get_cdata(XfaAttribute::Value))
            })
    }
    pub fn stroke_set_color(&self, color: u32) {
        if let Some(color_node) = self.get_or_create_property(0, XfaElement::Color) {
            color_node.color_set_value(color);
        }
    }
    pub fn validate_get_format_test(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::FormatTest)
    }
    pub fn validate_get_null_test(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::NullTest)
    }
    pub fn validate_get_script_test(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::ScriptTest)
    }
    pub fn validate_set_null_test(&self, value: &[u32]) {
        self.set_enum(
            XfaAttribute::NullTest,
            get_attribute_value_by_name(value).unwrap_or(XfaAttributeValue::Disabled),
            false,
        );
    }
    pub fn validate_get_message_text(&self, kind: &[u32]) -> Vec<u32> {
        let Some(message) = self.get_property(0, XfaElement::Message).0 else {
            return Vec::new();
        };
        let mut child = message.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if node.get_element_type() == XfaElement::Text {
                let name = node.get_cdata(XfaAttribute::Name);
                if name.is_empty() || name == kind {
                    return node.get_content();
                }
            }
        }
        Vec::new()
    }
    pub fn validate_set_message_text(&self, kind: &[u32], value: &[u32]) -> bool {
        let _document = self.document();
        let Some(message) = self.get_or_create_property(0, XfaElement::Message) else {
            return false;
        };
        let mut child = message.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if node.get_element_type() == XfaElement::Text {
                let name = node.get_cdata(XfaAttribute::Name);
                if name.is_empty() || name == kind {
                    return node.set_content_with_sync(value, value, false, true);
                }
            }
        }
        let Some(text) = message.create_same_packet_node(XfaElement::Text) else {
            return false;
        };
        if !message.insert_child_before_and_notify(&text, None) {
            return false;
        }
        // A notifier can detach the newly created child. Do not write through
        // a relationship superseded while insertion was being observed.
        if text.parent().as_ref() != Some(&message) {
            return false;
        }
        text.set_cdata(XfaAttribute::Name, kind);
        text.set_content_with_sync(value, value, false, true)
    }
    pub fn validate_get_picture(&self) -> Vec<u32> {
        self.get_child(0, XfaElement::Picture, false)
            .map_or_else(Vec::new, |picture| picture.get_content())
    }
    pub fn validate_get_script_if_exists(&self) -> Option<Self> {
        self.get_child(0, XfaElement::Script, false)
    }
}
