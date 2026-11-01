// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Tag matching, inheritance and cascade from `cfx_cssstyleselector.{h,cpp}`.
//! This deliberately retains PDFium's single-tag selector scope. Parsed
//! descendants are represented by the stylesheet but do not match here.
use super::cfx_css::{
    CfxCssDisplay, CfxCssFontStyle, CfxCssFontVariant, CfxCssLength, CfxCssLengthUnit,
    CfxCssProperty, CfxCssPropertyValue, CfxCssTextAlign, CfxCssTextDecoration,
    CfxCssVerticalAlign,
};
use super::cfx_csscomputedstyle::CfxCssComputedStyle;
use super::cfx_csscustomproperty::CfxCssCustomProperty;
use super::cfx_cssdata::{hash_code_lowered_wide, CfxCssData};
use super::cfx_cssdeclaration::CfxCssDeclaration;
use super::cfx_cssnumbervalue::CfxCssNumberUnit;
use super::cfx_csspropertyholder::CfxCssPropertyHolder;
use super::cfx_cssrulecollection::CfxCssRuleCollection;
use super::cfx_cssselector::CfxCssSelector;
use super::cfx_cssstylesheet::CfxCssStyleSheet;
use super::cfx_csssyntaxparser::{CfxCssSyntaxParser, Status};
use super::cfx_cssvalue::CfxCssValue;
use std::rc::Rc;

#[derive(Debug)]
pub struct CfxCssStyleSelector {
    default_font_size: f32,
    ua_styles: Option<CfxCssStyleSheet>,
    ua_rules: CfxCssRuleCollection,
}
impl Default for CfxCssStyleSelector {
    fn default() -> Self {
        Self {
            default_font_size: 12.0,
            ua_styles: None,
            ua_rules: CfxCssRuleCollection::new(),
        }
    }
}
impl CfxCssStyleSelector {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_default_font_size(&mut self, value: f32) {
        assert!(
            value.is_finite() && value > 0.0,
            "default CSS font size must be finite and positive"
        );
        self.default_font_size = value;
    }
    pub fn create_computed_style(
        &self,
        parent: Option<&CfxCssComputedStyle>,
    ) -> CfxCssComputedStyle {
        let mut style = CfxCssComputedStyle::new();
        if let Some(parent) = parent {
            style.inherited_data = parent.inherited_data.clone();
        }
        style
    }
    pub fn set_ua_style_sheet(&mut self, sheet: CfxCssStyleSheet) {
        self.ua_styles = Some(sheet);
    }
    pub fn update_style_index(&mut self) {
        self.ua_rules.set_rules_from_sheet(self.ua_styles.as_ref());
    }
    pub fn match_declarations(&self, tag_name: &[u32]) -> Vec<Rc<CfxCssDeclaration>> {
        if tag_name.is_empty() {
            return Vec::new();
        }
        self.ua_rules
            .get_tag_rule_data(tag_name)
            .into_iter()
            .flatten()
            .filter(|data| Self::match_selector(tag_name, &data.selector))
            .map(|data| Rc::clone(&data.declaration))
            .collect()
    }
    fn match_selector(tag_name: &[u32], selector: &CfxCssSelector) -> bool {
        selector.next_selector().is_none()
            && !selector.is_descendant()
            && selector.name_hash() == hash_code_lowered_wide(tag_name)
    }
    /// Mutates the caller's style because XFA text parsing supplies selected
    /// non-inherited values from the parent before it applies declarations.
    pub fn compute_style(
        &self,
        declarations: &[Rc<CfxCssDeclaration>],
        inline: &[u32],
        align: &[u32],
        dest: &mut CfxCssComputedStyle,
    ) {
        let extra = if !inline.is_empty() || !align.is_empty() {
            let mut declaration = CfxCssDeclaration::new();
            if !inline.is_empty() {
                Self::append_inline_style(&mut declaration, inline);
            }
            if !align.is_empty() {
                declaration.add_property(
                    CfxCssData::get_property_by_enum(CfxCssProperty::TextAlign),
                    align,
                );
            }
            Some(declaration)
        } else {
            None
        };
        self.apply_declarations(declarations, extra.as_ref(), dest);
    }
    fn apply_declarations(
        &self,
        declarations: &[Rc<CfxCssDeclaration>],
        extra: Option<&CfxCssDeclaration>,
        dest: &mut CfxCssComputedStyle,
    ) {
        let mut importants = Vec::new();
        let mut normals = Vec::new();
        let mut customs = Vec::new();
        for declaration in declarations {
            Self::extract_values(declaration, &mut importants, &mut normals, &mut customs);
        }
        if let Some(extra) = extra {
            Self::extract_values(extra, &mut importants, &mut normals, &mut customs);
        }
        for property in normals {
            self.apply_property(property.property, &property.value, dest);
        }
        for property in customs {
            dest.add_custom_style(property);
        }
        for property in importants {
            self.apply_property(property.property, &property.value, dest);
        }
    }
    fn extract_values<'a>(
        declaration: &'a CfxCssDeclaration,
        importants: &mut Vec<&'a CfxCssPropertyHolder>,
        normals: &mut Vec<&'a CfxCssPropertyHolder>,
        customs: &mut Vec<&'a CfxCssCustomProperty>,
    ) {
        for property in declaration.properties() {
            if property.important {
                importants.push(property);
            } else {
                normals.push(property);
            }
        }
        customs.extend(declaration.custom_properties());
    }
    fn append_inline_style(declaration: &mut CfxCssDeclaration, inline: &[u32]) {
        let mut parser = CfxCssSyntaxParser::new(inline);
        parser.set_parse_only_declarations();
        let mut property = None;
        let mut name = Vec::new();
        loop {
            match parser.do_syntax_parse() {
                Status::PropertyName => {
                    let value = parser.get_current_string();
                    property = CfxCssData::get_property_by_name(value);
                    name.clear();
                    if property.is_none() {
                        name.extend_from_slice(value);
                    }
                }
                Status::PropertyValue => {
                    let value = parser.get_current_string();
                    if !value.is_empty() {
                        if let Some(property) = property {
                            declaration.add_property(property, value);
                        } else if !name.is_empty() {
                            // Source saves this name but gates it on an integer
                            // that is never updated, dropping XFA custom styles.
                            declaration.add_custom_property(&name, value);
                        }
                    }
                }
                _ => break,
            }
        }
    }
    fn apply_property(
        &self,
        property: CfxCssProperty,
        value: &Rc<CfxCssValue>,
        style: &mut CfxCssComputedStyle,
    ) {
        use CfxCssProperty as P;
        use CfxCssValue as V;
        if let V::List(values) = value.as_ref() {
            if !values.is_empty() {
                match property {
                    P::FontFamily => style.inherited_data.font_family = Some(Rc::clone(value)),
                    P::TextDecoration => {
                        style.non_inherited_data.text_decoration = Self::to_text_decoration(values)
                    }
                    _ => {}
                }
            }
            return;
        }
        let font_size = style.inherited_data.font_size;
        match property {
            P::Display => {
                if let V::Enum(value) = value.as_ref() {
                    style.non_inherited_data.display = Self::to_display(*value);
                }
            }
            P::FontSize => match value.as_ref() {
                V::Number(value) => style.inherited_data.font_size = value.apply(font_size),
                V::Enum(value) => {
                    style.inherited_data.font_size = self.to_font_size(*value, font_size)
                }
                _ => {}
            },
            P::LineHeight => {
                if let V::Number(value) = value.as_ref() {
                    style.inherited_data.line_height = if value.unit == CfxCssNumberUnit::Number {
                        value.value * font_size
                    } else {
                        value.apply(font_size)
                    };
                }
            }
            P::TextAlign => {
                if let V::Enum(value) = value.as_ref() {
                    style.inherited_data.text_align = Self::to_text_align(*value);
                }
            }
            P::TextIndent => {
                Self::set_length_with_percent(
                    &mut style.inherited_data.text_indent,
                    value,
                    font_size,
                );
            }
            P::FontWeight => match value.as_ref() {
                V::Enum(value) => style.inherited_data.font_weight = Self::to_font_weight(*value),
                V::Number(value) => {
                    // Rust's narrowing is defined for NaN and out-of-range
                    // floats. Only source-supported 100..900 weights apply.
                    let weight = (value.value as i32) / 100;
                    if (1..=9).contains(&weight) {
                        style.inherited_data.font_weight = (weight * 100) as u16;
                    }
                }
                _ => {}
            },
            P::FontStyle => {
                if let V::Enum(value) = value.as_ref() {
                    style.inherited_data.font_style = Self::to_font_style(*value);
                }
            }
            P::Color => {
                if let V::Color(value) = value.as_ref() {
                    style.inherited_data.font_color = *value;
                }
            }
            P::MarginLeft => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.margin_width.left,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_margin = true;
                }
            }
            P::MarginTop => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.margin_width.top,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_margin = true;
                }
            }
            P::MarginRight => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.margin_width.right,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_margin = true;
                }
            }
            P::MarginBottom => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.margin_width.bottom,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_margin = true;
                }
            }
            P::PaddingLeft => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.padding_width.left,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_padding = true;
                }
            }
            P::PaddingTop => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.padding_width.top,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_padding = true;
                }
            }
            P::PaddingRight => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.padding_width.right,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_padding = true;
                }
            }
            P::PaddingBottom => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.padding_width.bottom,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_padding = true;
                }
            }
            P::BorderLeftWidth => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.border_width.left,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_border = true;
                }
            }
            P::BorderTopWidth => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.border_width.top,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_border = true;
                }
            }
            P::BorderRightWidth => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.border_width.right,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_border = true;
                }
            }
            P::BorderBottomWidth => {
                if Self::set_length_with_percent(
                    &mut style.non_inherited_data.border_width.bottom,
                    value,
                    font_size,
                ) {
                    style.non_inherited_data.has_border = true;
                }
            }
            P::VerticalAlign => match value.as_ref() {
                V::Enum(value) => {
                    style.non_inherited_data.vertical_align_type = Self::to_vertical_align(*value)
                }
                V::Number(value) => style.set_number_vertical_align(value.apply(font_size)),
                _ => {}
            },
            P::FontVariant => {
                if let V::Enum(value) = value.as_ref() {
                    style.inherited_data.font_variant = Self::to_font_variant(*value);
                }
            }
            P::LetterSpacing => match value.as_ref() {
                V::Enum(_) => {
                    style
                        .inherited_data
                        .letter_spacing
                        .set_unit(CfxCssLengthUnit::Normal);
                }
                V::Number(number) if number.unit != CfxCssNumberUnit::Percent => {
                    Self::set_length_with_percent(
                        &mut style.inherited_data.letter_spacing,
                        value,
                        font_size,
                    );
                }
                _ => {}
            },
            P::WordSpacing => match value.as_ref() {
                V::Enum(_) => {
                    style
                        .inherited_data
                        .word_spacing
                        .set_unit(CfxCssLengthUnit::Normal);
                }
                V::Number(number) if number.unit != CfxCssNumberUnit::Percent => {
                    Self::set_length_with_percent(
                        &mut style.inherited_data.word_spacing,
                        value,
                        font_size,
                    );
                }
                _ => {}
            },
            P::Top => {
                Self::set_length_with_percent(&mut style.non_inherited_data.top, value, font_size);
            }
            P::Bottom => {
                Self::set_length_with_percent(
                    &mut style.non_inherited_data.bottom,
                    value,
                    font_size,
                );
            }
            P::Left => {
                Self::set_length_with_percent(&mut style.non_inherited_data.left, value, font_size);
            }
            P::Right => {
                Self::set_length_with_percent(
                    &mut style.non_inherited_data.right,
                    value,
                    font_size,
                );
            }
            _ => {}
        }
    }
    fn set_length_with_percent(
        length: &mut CfxCssLength,
        value: &CfxCssValue,
        font_size: f32,
    ) -> bool {
        match value {
            CfxCssValue::Number(number) => {
                if number.unit == CfxCssNumberUnit::Percent {
                    length.set(CfxCssLengthUnit::Percent, number.value / 100.0);
                } else {
                    length.set(CfxCssLengthUnit::Point, number.apply(font_size));
                }
                length.non_zero()
            }
            CfxCssValue::Enum(value) => match value {
                CfxCssPropertyValue::Auto => {
                    length.set_unit(CfxCssLengthUnit::Auto);
                    true
                }
                CfxCssPropertyValue::None => {
                    length.set_unit(CfxCssLengthUnit::None);
                    true
                }
                CfxCssPropertyValue::Thin => {
                    length.set(CfxCssLengthUnit::Point, 2.0);
                    true
                }
                CfxCssPropertyValue::Medium => {
                    length.set(CfxCssLengthUnit::Point, 3.0);
                    true
                }
                CfxCssPropertyValue::Thick => {
                    length.set(CfxCssLengthUnit::Point, 4.0);
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }
    fn to_font_size(&self, value: CfxCssPropertyValue, current: f32) -> f32 {
        use CfxCssPropertyValue as V;
        match value {
            V::XxSmall => self.default_font_size / 1.2 / 1.2 / 1.2,
            V::XSmall => self.default_font_size / 1.2 / 1.2,
            V::Small => self.default_font_size / 1.2,
            V::Medium => self.default_font_size,
            V::Large => self.default_font_size * 1.2,
            V::XLarge => self.default_font_size * 1.2 * 1.2,
            V::XxLarge => self.default_font_size * 1.2 * 1.2 * 1.2,
            V::Larger => current * 1.2,
            V::Smaller => current / 1.2,
            _ => current,
        }
    }
    fn to_display(value: CfxCssPropertyValue) -> CfxCssDisplay {
        use CfxCssPropertyValue as V;
        match value {
            V::Block => CfxCssDisplay::Block,
            V::None => CfxCssDisplay::None,
            V::ListItem => CfxCssDisplay::ListItem,
            V::InlineTable => CfxCssDisplay::InlineTable,
            V::InlineBlock => CfxCssDisplay::InlineBlock,
            _ => CfxCssDisplay::Inline,
        }
    }
    fn to_text_align(value: CfxCssPropertyValue) -> CfxCssTextAlign {
        match value {
            CfxCssPropertyValue::Center => CfxCssTextAlign::Center,
            CfxCssPropertyValue::Right => CfxCssTextAlign::Right,
            CfxCssPropertyValue::Justify => CfxCssTextAlign::Justify,
            _ => CfxCssTextAlign::Left,
        }
    }
    fn to_font_weight(value: CfxCssPropertyValue) -> u16 {
        match value {
            CfxCssPropertyValue::Bold => 700,
            CfxCssPropertyValue::Bolder => 900,
            CfxCssPropertyValue::Lighter => 200,
            _ => 400,
        }
    }
    fn to_font_style(value: CfxCssPropertyValue) -> CfxCssFontStyle {
        match value {
            CfxCssPropertyValue::Italic | CfxCssPropertyValue::Oblique => CfxCssFontStyle::Italic,
            _ => CfxCssFontStyle::Normal,
        }
    }
    fn to_vertical_align(value: CfxCssPropertyValue) -> CfxCssVerticalAlign {
        use CfxCssPropertyValue as V;
        match value {
            V::Middle => CfxCssVerticalAlign::Middle,
            V::Bottom => CfxCssVerticalAlign::Bottom,
            V::Super => CfxCssVerticalAlign::Super,
            V::Sub => CfxCssVerticalAlign::Sub,
            V::Top => CfxCssVerticalAlign::Top,
            V::TextTop => CfxCssVerticalAlign::TextTop,
            V::TextBottom => CfxCssVerticalAlign::TextBottom,
            _ => CfxCssVerticalAlign::Baseline,
        }
    }
    fn to_text_decoration(values: &[Rc<CfxCssValue>]) -> u8 {
        use CfxCssPropertyValue as V;
        let mut result = 0;
        for value in values.iter().rev() {
            if let CfxCssValue::Enum(value) = value.as_ref() {
                result |= match value {
                    V::Underline => CfxCssTextDecoration::Underline as u8,
                    V::LineThrough => CfxCssTextDecoration::LineThrough as u8,
                    V::Overline => CfxCssTextDecoration::Overline as u8,
                    V::Blink => CfxCssTextDecoration::Blink as u8,
                    V::Double => CfxCssTextDecoration::Double as u8,
                    _ => 0,
                };
            }
        }
        result
    }
    fn to_font_variant(value: CfxCssPropertyValue) -> CfxCssFontVariant {
        if value == CfxCssPropertyValue::SmallCaps {
            CfxCssFontVariant::SmallCaps
        } else {
            CfxCssFontVariant::Normal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn sheet(source: &str) -> CfxCssStyleSheet {
        let mut sheet = CfxCssStyleSheet::new();
        assert!(sheet.load_buffer(&wide(source)), "{source}");
        sheet
    }
    fn make_selector(source: &str) -> CfxCssStyleSelector {
        let mut selector = CfxCssStyleSelector::new();
        selector.set_ua_style_sheet(sheet(source));
        selector.update_style_index();
        selector
    }
    fn compute(
        selector: &CfxCssStyleSelector,
        tag: &str,
        inline: &str,
        parent: Option<&CfxCssComputedStyle>,
    ) -> CfxCssComputedStyle {
        let mut style = selector.create_computed_style(parent);
        selector.compute_style(
            &selector.match_declarations(&wide(tag)),
            &wide(inline),
            &[],
            &mut style,
        );
        style
    }
    fn near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.00001,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn actual_xfa_default_sheet_computes_tag_styles_and_limits_selector_scope() {
        // CXFA_TextParser::LoadDefaultSheetStyle, complete production literal.
        let selector = make_selector(concat!(
            "html,body,ol,p,ul{display:block}",
            "li{display:list-item}",
            "ol,ul{padding-left:33px;margin:1.12em 0}",
            "ol{list-style-type:decimal}",
            "a{color:#0000ff;text-decoration:underline}",
            "b{font-weight:bolder}",
            "i{font-style:italic}",
            "sup{vertical-align:+15em;font-size:.66em}",
            "sub{vertical-align:-15em;font-size:.66em}"
        ));
        for tag in ["html", "body", "ol", "p", "ul", "P"] {
            assert_eq!(
                compute(&selector, tag, "", None).get_display(),
                CfxCssDisplay::Block
            );
        }
        let list = compute(&selector, "ul", "", None);
        near(list.get_padding_width().unwrap().left.value, 24.75);
        near(list.get_margin_width().unwrap().top.value, 13.44);
        assert_eq!(
            compute(&selector, "li", "", None).get_display(),
            CfxCssDisplay::ListItem
        );
        assert_eq!(compute(&selector, "b", "", None).get_font_weight(), 900);
        assert_eq!(
            compute(&selector, "i", "", None).get_font_style(),
            CfxCssFontStyle::Italic
        );
        let anchor = compute(&selector, "a", "", None);
        assert_eq!(anchor.get_color(), 0xff0000ff);
        assert_eq!(
            anchor.get_text_decoration(),
            CfxCssTextDecoration::Underline as u8
        );
        let sup = compute(&selector, "sup", "", None);
        near(sup.get_number_vertical_align(), 180.0);
        near(sup.get_font_size(), 7.92);
        let sub = compute(&selector, "sub", "", None);
        near(sub.get_number_vertical_align(), -180.0);
        assert_eq!(sub.get_vertical_align(), CfxCssVerticalAlign::Number);
        assert!(selector.match_declarations(&[]).is_empty());
        let restricted = make_selector("body p{color:red}*{color:blue}p{font-weight:bold}");
        let plain = compute(&restricted, "p", "", None);
        assert_eq!(plain.get_color(), 0xff000000);
        assert_eq!(plain.get_font_weight(), 700);
    }

    #[test]
    fn source_cascade_priority_relative_units_and_parent_inheritance_are_connected() {
        let selector = make_selector(concat!(
            "p{font-size:20pt;color:red !important;margin-left:1pt;",
            "font-family:'Alpha',serif;line-height:2}",
            "p{color:blue;font-size:150%;font-style:italic}"
        ));
        let declarations = selector.match_declarations(&wide("p"));
        let mut parent = selector.create_computed_style(None);
        selector.compute_style(
            &declarations,
            &wide("color:green;font-size:50%;font-weight:650;text-align:right"),
            &wide("center"),
            &mut parent,
        );
        // Source applies normal values in source order before important values;
        // relative values therefore use the font size at each application.
        near(parent.get_font_size(), 15.0);
        near(parent.get_line_height(), 40.0);
        assert_eq!(parent.get_font_weight(), 600);
        assert_eq!(parent.get_color(), 0xffff0000);
        assert_eq!(parent.get_text_align(), CfxCssTextAlign::Center);
        assert_eq!(parent.get_font_style(), CfxCssFontStyle::Italic);
        assert_eq!(parent.get_last_font_family(), Some(wide("serif")));
        assert_eq!(parent.get_margin_width().unwrap().left.value, 1.0);
        let child = compute(
            &selector,
            "span",
            "font-size:2em;line-height:125%;padding-left:.5pt",
            Some(&parent),
        );
        near(child.get_font_size(), 30.0);
        near(child.get_line_height(), 37.5);
        assert_eq!(child.get_color(), parent.get_color());
        assert_eq!(child.get_font_weight(), parent.get_font_weight());
        assert!(child.get_margin_width().is_none());
        assert_eq!(child.get_padding_width().unwrap().left.value, 0.5);
        assert!(Rc::ptr_eq(
            parent.inherited_data.font_family.as_ref().unwrap(),
            child.inherited_data.font_family.as_ref().unwrap()
        ));
    }

    #[test]
    fn rule_index_replacement_and_saved_declarations_have_safe_shared_lifetimes() {
        let mut selector = make_selector("p{color:red}");
        let old = selector.match_declarations(&wide("p"));
        selector.set_ua_style_sheet(sheet("p{color:blue}"));
        // Until the explicit index update, the source index refers to its old
        // sheet. Rc owners keep that snapshot valid instead of dangling.
        assert_eq!(compute(&selector, "p", "", None).get_color(), 0xffff0000);
        selector.update_style_index();
        assert_eq!(compute(&selector, "p", "", None).get_color(), 0xff0000ff);
        drop(selector);
        let fresh = CfxCssStyleSelector::new();
        let mut style = fresh.create_computed_style(None);
        fresh.compute_style(&old, &[], &[], &mut style);
        assert_eq!(style.get_color(), 0xffff0000);
        let mut collection = CfxCssRuleCollection::new();
        let sheet = sheet("a,b{color:red}");
        collection.set_rules_from_sheet(Some(&sheet));
        assert_eq!(collection.get_tag_rule_data(&wide("A")).unwrap().len(), 1);
        collection.set_rules_from_sheet(None);
        assert!(collection.get_tag_rule_data(&wide("a")).is_none());
    }

    #[test]
    fn xfa_custom_styles_preserve_last_definition_and_own_text() {
        let selector = make_selector("p{tab-interval:12pt;foo:sheet1}p{foo:sheet2}");
        let parent = compute(
            &selector,
            "p",
            "foo:inline;xfa-spacerun:yes;tab-stops:20pt left",
            None,
        );
        assert_eq!(
            parent.get_custom_style(&wide("foo")),
            Some(wide("inline").as_slice())
        );
        assert_eq!(
            parent.get_custom_style(&wide("tab-interval")),
            Some(wide("12pt").as_slice())
        );
        assert_eq!(
            parent.get_custom_style(&wide("xfa-spacerun")),
            Some(wide("yes").as_slice())
        );
        assert_eq!(
            parent.get_custom_style(&wide("tab-stops")),
            Some(wide("20pt left").as_slice())
        );
        assert!(parent.get_custom_style(&wide("missing")).is_none());
        let child = selector.create_computed_style(Some(&parent));
        assert!(child.get_custom_style(&wide("foo")).is_none());
        drop(selector);
        assert_eq!(
            parent.get_custom_style(&wide("foo")),
            Some(wide("inline").as_slice())
        );
    }

    #[test]
    fn fractional_box_lengths_unit_conversions_and_non_inherited_fields_work() {
        let selector=make_selector("p{margin:1pc 25% .5pt 2em;padding:.25pt;border:thin solid red;letter-spacing:.5em;word-spacing:3pt;text-decoration:underline line-through;font-variant:small-caps;top:25%;left:2em;bottom:none;right:auto}");
        let style = compute(&selector, "p", "word-spacing:20%", None);
        let margin = style.get_margin_width().unwrap();
        near(margin.top.value, 12.0);
        near(margin.right.value, 0.25);
        assert_eq!(margin.right.unit, CfxCssLengthUnit::Percent);
        near(margin.bottom.value, 0.5);
        near(margin.left.value, 24.0);
        near(style.get_padding_width().unwrap().top.value, 0.25);
        near(style.get_border_width().unwrap().right.value, 2.0);
        near(style.get_letter_spacing().value, 6.0);
        near(style.inherited_data.word_spacing.value, 3.0);
        assert_eq!(style.get_font_variant(), CfxCssFontVariant::SmallCaps);
        assert_eq!(
            style.get_text_decoration(),
            CfxCssTextDecoration::Underline as u8 | CfxCssTextDecoration::LineThrough as u8
        );
        assert_eq!(style.non_inherited_data.top.unit, CfxCssLengthUnit::Percent);
        near(style.non_inherited_data.top.value, 0.25);
        near(style.non_inherited_data.left.value, 24.0);
        assert_eq!(style.non_inherited_data.bottom.unit, CfxCssLengthUnit::None);
        assert_eq!(style.non_inherited_data.right.unit, CfxCssLengthUnit::Auto);
        let fractional = compute(
            &selector,
            "span",
            "margin-left:25%;padding-left:.5pt;border-left-width:.25pt",
            None,
        );
        near(fractional.get_margin_width().unwrap().left.value, 0.25);
        near(fractional.get_padding_width().unwrap().left.value, 0.5);
        near(fractional.get_border_width().unwrap().left.value, 0.25);
        let child = selector.create_computed_style(Some(&style));
        assert_eq!(child.get_text_decoration(), 0);
        assert_eq!(child.get_vertical_align(), CfxCssVerticalAlign::Baseline);
        assert_eq!(child.non_inherited_data.top.unit, CfxCssLengthUnit::Auto);
    }
}
