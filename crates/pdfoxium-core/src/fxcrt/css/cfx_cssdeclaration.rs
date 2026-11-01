// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Declaration parsing from `core/fxcrt/css/cfx_cssdeclaration.{h,cpp}`.
//! Immutable values share `Rc` ownership instead of polymorphic retain pointers.
//! Malformed hexadecimal colours are rejected, RGB channels clamp before
//! narrowing, and whitespace before `!important` is trimmed before conversion.

use super::cfx_css::{
    CfxCssProperty, CfxCssPropertyValue, VALUE_TYPE_LIST, VALUE_TYPE_MAYBE_COLOR,
    VALUE_TYPE_MAYBE_ENUM, VALUE_TYPE_MAYBE_NUMBER, VALUE_TYPE_MAYBE_STRING, VALUE_TYPE_PRIMITIVE,
    VALUE_TYPE_SHORTHAND,
};
use super::cfx_csscustomproperty::CfxCssCustomProperty;
use super::cfx_cssdata::{CfxCssData, Property};
use super::cfx_cssnumbervalue::{CfxCssNumber, CfxCssNumberUnit};
use super::cfx_csspropertyholder::CfxCssPropertyHolder;
use super::cfx_cssvalue::{CfxCssPrimitiveType, CfxCssValue};
use super::cfx_cssvaluelistparser::CfxCssValueListParser;
use crate::fxcrt::fx_extension::wcstof;
use std::rc::Rc;

fn equals_ascii_no_case(value: &[u32], ascii: &[u8]) -> bool {
    value.len() == ascii.len()
        && value
            .iter()
            .zip(ascii)
            .all(|(&left, &right)| left <= 0x7f && (left as u8).eq_ignore_ascii_case(&right))
}
fn parse_css_number(view: &[u32]) -> Option<CfxCssNumber> {
    if view.is_empty() {
        return None;
    }
    let (value, used) = wcstof(view);
    if used == 0 || !value.is_finite() {
        return None;
    }
    let suffix = &view[used..];
    let unit = if suffix.first() == Some(&u32::from(b'%')) {
        CfxCssNumberUnit::Percent
    } else if suffix.len() == 2 {
        CfxCssData::get_length_unit_by_name(suffix)
            .map_or(CfxCssNumberUnit::Number, |unit| unit.unit)
    } else {
        CfxCssNumberUnit::Number
    };
    Some(CfxCssNumber { unit, value })
}
fn number_value(number: CfxCssNumber) -> Rc<CfxCssValue> {
    Rc::new(CfxCssValue::Number(CfxCssNumber::new(
        number.unit,
        number.value,
    )))
}
fn enum_value(value: CfxCssPropertyValue) -> Rc<CfxCssValue> {
    Rc::new(CfxCssValue::Enum(value))
}
fn hex_digit(value: u32) -> Option<u32> {
    match value {
        48..=57 => Some(value - 48),
        65..=70 => Some(value - 65 + 10),
        97..=102 => Some(value - 97 + 10),
        _ => None,
    }
}

#[derive(Default, Clone, Debug)]
pub struct CfxCssDeclaration {
    properties: Vec<CfxCssPropertyHolder>,
    custom_properties: Vec<CfxCssCustomProperty>,
}
impl CfxCssDeclaration {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn properties(&self) -> &[CfxCssPropertyHolder] {
        &self.properties
    }
    pub fn custom_properties(&self) -> &[CfxCssCustomProperty] {
        &self.custom_properties
    }
    pub fn empty(&self) -> bool {
        self.properties.is_empty()
    }
    pub fn get_property(&self, property: CfxCssProperty) -> Option<(Rc<CfxCssValue>, bool)> {
        self.properties
            .iter()
            .find(|holder| holder.property == property)
            .map(|holder| (Rc::clone(&holder.value), holder.important))
    }
    pub fn property_count_for_testing(&self) -> usize {
        self.properties.len()
    }
    pub fn parse_css_string(mut value: &[u32]) -> Option<&[u32]> {
        if value.len() >= 2
            && ((value[0] == u32::from(b'"') && value.last() == Some(&u32::from(b'"')))
                || (value[0] == u32::from(b'\'') && value.last() == Some(&u32::from(b'\''))))
        {
            value = &value[1..value.len() - 1];
        }
        (!value.is_empty()).then_some(value)
    }
    pub fn parse_css_color(value: &[u32]) -> Option<u32> {
        if value.first() == Some(&u32::from(b'#')) {
            return match value.len() {
                4 => {
                    let red = hex_digit(value[1])? * 17;
                    let green = hex_digit(value[2])? * 17;
                    let blue = hex_digit(value[3])? * 17;
                    Some(0xff00_0000 | red << 16 | green << 8 | blue)
                }
                7 => {
                    let red = hex_digit(value[1])? * 16 + hex_digit(value[2])?;
                    let green = hex_digit(value[3])? * 16 + hex_digit(value[4])?;
                    let blue = hex_digit(value[5])? * 16 + hex_digit(value[6])?;
                    Some(0xff00_0000 | red << 16 | green << 8 | blue)
                }
                _ => None,
            };
        }
        if value.len() >= 10 {
            if !equals_ascii_no_case(&value[..4], b"rgb(") || value.last() != Some(&u32::from(b')'))
            {
                return None;
            }
            let mut rgb = [0u8; 3];
            let mut list = CfxCssValueListParser::new(&value[4..value.len() - 1], u32::from(b','));
            for component in &mut rgb {
                let next = list.next_value()?;
                if next.value_type != CfxCssPrimitiveType::Number {
                    return None;
                }
                let number = parse_css_number(next.string_view)?;
                let channel = if number.unit == CfxCssNumberUnit::Percent {
                    number.value * 2.55f32
                } else {
                    number.value
                };
                *component = channel.round().clamp(0.0, 255.0) as u8;
            }
            if list.next_value().is_some() {
                return None;
            }
            return Some(
                0xff00_0000 | u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2]),
            );
        }
        CfxCssData::get_color_by_name(value).map(|color| color.value)
    }
    pub fn parse_color_for_test(&self, value: &[u32]) -> Option<u32> {
        Self::parse_css_color(value)
    }
    fn add_property_holder(
        &mut self,
        property: CfxCssProperty,
        value: Rc<CfxCssValue>,
        important: bool,
    ) {
        self.properties.push(CfxCssPropertyHolder {
            property,
            value,
            important,
        });
    }
    pub fn add_property(&mut self, property: &Property, mut value: &[u32]) {
        if value.is_empty() {
            return;
        }
        let mut important = false;
        if value.len() >= 10 && equals_ascii_no_case(&value[value.len() - 10..], b"!important") {
            value = &value[..value.len() - 10];
            // The source leaves this separator in the value, making ordinary
            // `color: red !important` fail its colour/keyword conversion.
            while value
                .last()
                .is_some_and(|&character| character <= u32::from(b' '))
            {
                value = &value[..value.len() - 1];
            }
            if value.is_empty() {
                return;
            }
            important = true;
        }
        let types = property.types;
        match types & 0x0f {
            VALUE_TYPE_PRIMITIVE => {
                for guess in [
                    VALUE_TYPE_MAYBE_NUMBER,
                    VALUE_TYPE_MAYBE_ENUM,
                    VALUE_TYPE_MAYBE_COLOR,
                    VALUE_TYPE_MAYBE_STRING,
                ] {
                    if types & guess == 0 {
                        continue;
                    }
                    let parsed = match guess {
                        VALUE_TYPE_MAYBE_NUMBER => self.parse_number(value),
                        VALUE_TYPE_MAYBE_ENUM => self.parse_enum(value),
                        VALUE_TYPE_MAYBE_COLOR => self.parse_color(value),
                        VALUE_TYPE_MAYBE_STRING => self.parse_string(value),
                        _ => None,
                    };
                    if let Some(parsed) = parsed {
                        self.add_property_holder(property.name, parsed, important);
                        return;
                    }
                    if types & !guess == VALUE_TYPE_PRIMITIVE {
                        return;
                    }
                }
            }
            VALUE_TYPE_SHORTHAND => {
                use CfxCssProperty::*;
                let target = match property.name {
                    Font => {
                        self.parse_font_property(value, important);
                        return;
                    }
                    Border => {
                        let width = self.parse_border_property(value);
                        for side in [
                            BorderLeftWidth,
                            BorderTopWidth,
                            BorderRightWidth,
                            BorderBottomWidth,
                        ] {
                            self.add_property_holder(side, Rc::clone(&width), important);
                        }
                        return;
                    }
                    BorderLeft => BorderLeftWidth,
                    BorderTop => BorderTopWidth,
                    BorderRight => BorderRightWidth,
                    BorderBottom => BorderBottomWidth,
                    _ => return,
                };
                let width = self.parse_border_property(value);
                self.add_property_holder(target, width, important);
            }
            VALUE_TYPE_LIST => self.parse_value_list_property(property, value, important),
            _ => {}
        }
    }
    pub fn add_custom_property(&mut self, name: &[u32], value: &[u32]) {
        self.custom_properties.push(CfxCssCustomProperty {
            name: name.to_vec(),
            value: value.to_vec(),
        });
    }
    fn parse_number(&self, value: &[u32]) -> Option<Rc<CfxCssValue>> {
        parse_css_number(value).map(number_value)
    }
    fn parse_enum(&self, value: &[u32]) -> Option<Rc<CfxCssValue>> {
        CfxCssData::get_property_value_by_name(value).map(|entry| enum_value(entry.name))
    }
    fn parse_color(&self, value: &[u32]) -> Option<Rc<CfxCssValue>> {
        Self::parse_css_color(value).map(|color| Rc::new(CfxCssValue::Color(color)))
    }
    fn parse_string(&self, value: &[u32]) -> Option<Rc<CfxCssValue>> {
        Self::parse_css_string(value).map(|text| Rc::new(CfxCssValue::String(text.to_vec())))
    }
    fn parse_value_list_property(&mut self, property: &Property, value: &[u32], important: bool) {
        let separator = if property.name == CfxCssProperty::FontFamily {
            b','
        } else {
            b' '
        };
        let mut parser = CfxCssValueListParser::new(value, u32::from(separator));
        let types = property.types;
        let mut list = Vec::new();
        while let Some(next) = parser.next_value() {
            match next.value_type {
                CfxCssPrimitiveType::Number => {
                    if types & VALUE_TYPE_MAYBE_NUMBER != 0 {
                        if let Some(number) = parse_css_number(next.string_view) {
                            list.push(number_value(number));
                        }
                    }
                }
                CfxCssPrimitiveType::String => {
                    if types & VALUE_TYPE_MAYBE_COLOR != 0 {
                        if let Some(color) = Self::parse_css_color(next.string_view) {
                            list.push(Rc::new(CfxCssValue::Color(color)));
                            continue;
                        }
                    }
                    if types & VALUE_TYPE_MAYBE_ENUM != 0 {
                        if let Some(entry) =
                            CfxCssData::get_property_value_by_name(next.string_view)
                        {
                            list.push(enum_value(entry.name));
                            continue;
                        }
                    }
                    if types & VALUE_TYPE_MAYBE_STRING != 0 {
                        list.push(Rc::new(CfxCssValue::String(next.string_view.to_vec())));
                    }
                }
                CfxCssPrimitiveType::RGB => {
                    if types & VALUE_TYPE_MAYBE_COLOR != 0 {
                        if let Some(color) = Self::parse_css_color(next.string_view) {
                            list.push(Rc::new(CfxCssValue::Color(color)));
                        }
                    }
                }
                _ => {}
            }
        }
        if list.is_empty() {
            return;
        }
        use CfxCssProperty::*;
        let sides = match property.name {
            BorderWidth => [
                BorderLeftWidth,
                BorderTopWidth,
                BorderRightWidth,
                BorderBottomWidth,
            ],
            Margin => [MarginLeft, MarginTop, MarginRight, MarginBottom],
            Padding => [PaddingLeft, PaddingTop, PaddingRight, PaddingBottom],
            _ => {
                self.add_property_holder(
                    property.name,
                    Rc::new(CfxCssValue::List(list)),
                    important,
                );
                return;
            }
        };
        self.add_4_values_property(&list, important, sides);
    }
    fn add_4_values_property(
        &mut self,
        list: &[Rc<CfxCssValue>],
        important: bool,
        sides: [CfxCssProperty; 4],
    ) {
        let indices = match list.len() {
            1 => [0, 0, 0, 0],
            2 => [1, 0, 1, 0],
            3 => [1, 0, 1, 2],
            4 => [3, 0, 1, 2],
            _ => return,
        };
        for (property, index) in sides.into_iter().zip(indices) {
            self.add_property_holder(property, Rc::clone(&list[index]), important);
        }
    }
    fn parse_border_property(&self, value: &[u32]) -> Rc<CfxCssValue> {
        let mut width = None;
        let mut parser = CfxCssValueListParser::new(value, u32::from(b' '));
        while let Some(next) = parser.next_value() {
            match next.value_type {
                CfxCssPrimitiveType::Number if width.is_none() => {
                    width = parse_css_number(next.string_view).map(number_value);
                }
                CfxCssPrimitiveType::String => {
                    if CfxCssData::get_color_by_name(next.string_view).is_some() {
                        continue;
                    }
                    if let Some(entry) = CfxCssData::get_property_value_by_name(next.string_view) {
                        if width.is_none()
                            && matches!(
                                entry.name,
                                CfxCssPropertyValue::Thin
                                    | CfxCssPropertyValue::Thick
                                    | CfxCssPropertyValue::Medium
                            )
                        {
                            width = Some(enum_value(entry.name));
                        }
                    }
                }
                _ => {}
            }
        }
        width.unwrap_or_else(|| {
            number_value(CfxCssNumber {
                unit: CfxCssNumberUnit::Number,
                value: 0.0,
            })
        })
    }
    fn parse_font_property(&mut self, value: &[u32], important: bool) {
        let mut style = None;
        let mut variant = None;
        let mut weight = None;
        let mut size = None;
        let mut line_height = None;
        let mut family_list = Vec::new();
        let mut parser = CfxCssValueListParser::new(value, u32::from(b'/'));
        while let Some(next) = parser.next_value() {
            match next.value_type {
                CfxCssPrimitiveType::String => {
                    if let Some(entry) = CfxCssData::get_property_value_by_name(next.string_view) {
                        use CfxCssPropertyValue as V;
                        let slot = match entry.name {
                            V::XxSmall
                            | V::XSmall
                            | V::Small
                            | V::Medium
                            | V::Large
                            | V::XLarge
                            | V::XxLarge
                            | V::Smaller
                            | V::Larger => Some(&mut size),
                            V::Bold | V::Bolder | V::Lighter => Some(&mut weight),
                            V::Italic | V::Oblique => Some(&mut style),
                            V::SmallCaps => Some(&mut variant),
                            V::Normal => Some(if style.is_none() {
                                &mut style
                            } else if variant.is_none() {
                                &mut variant
                            } else if weight.is_none() {
                                &mut weight
                            } else if size.is_none() {
                                &mut size
                            } else {
                                &mut line_height
                            }),
                            _ => None,
                        };
                        if let Some(slot) = slot {
                            if slot.is_none() {
                                *slot = Some(enum_value(entry.name));
                            }
                            continue;
                        }
                    }
                    if size.is_some() {
                        family_list.push(Rc::new(CfxCssValue::String(next.string_view.to_vec())));
                    }
                    parser.use_comma_separator();
                }
                CfxCssPrimitiveType::Number => {
                    let Some(number) = parse_css_number(next.string_view) else {
                        continue;
                    };
                    if number.unit == CfxCssNumberUnit::Number
                        && matches!(
                            number.value as i32,
                            100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
                        )
                    {
                        if weight.is_none() {
                            weight = Some(number_value(number));
                        }
                        continue;
                    }
                    if size.is_none() {
                        size = Some(number_value(number));
                    } else if line_height.is_none() {
                        line_height = Some(number_value(number));
                    }
                }
                _ => {}
            }
        }
        let normal = CfxCssPropertyValue::Normal;
        for (property, value) in [
            (
                CfxCssProperty::FontStyle,
                style.unwrap_or_else(|| enum_value(normal)),
            ),
            (
                CfxCssProperty::FontVariant,
                variant.unwrap_or_else(|| enum_value(normal)),
            ),
            (
                CfxCssProperty::FontWeight,
                weight.unwrap_or_else(|| enum_value(normal)),
            ),
            (
                CfxCssProperty::FontSize,
                size.unwrap_or_else(|| enum_value(CfxCssPropertyValue::Medium)),
            ),
            (
                CfxCssProperty::LineHeight,
                line_height.unwrap_or_else(|| enum_value(normal)),
            ),
        ] {
            self.add_property_holder(property, value, important);
        }
        if !family_list.is_empty() {
            self.add_property_holder(
                CfxCssProperty::FontFamily,
                Rc::new(CfxCssValue::List(family_list)),
                important,
            );
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn add(declaration: &mut CfxCssDeclaration, property: CfxCssProperty, value: &str) {
        declaration.add_property(CfxCssData::get_property_by_enum(property), &wide(value));
    }
    fn get(declaration: &CfxCssDeclaration, property: CfxCssProperty) -> Rc<CfxCssValue> {
        declaration
            .get_property(property)
            .expect("stored property")
            .0
    }
    fn assert_number(value: &CfxCssValue, unit: CfxCssNumberUnit, expected: f32) {
        let CfxCssValue::Number(number) = value else {
            panic!("not a CSS number: {value:?}");
        };
        assert_eq!(number.unit, unit);
        assert_eq!(number.value, expected);
    }
    // cfx_cssdeclaration_unittest.cpp: complete HexEncodingParsing vectors,
    // correcting the source's invalid-digit fallback from black to rejection.
    #[test]
    fn HexEncodingParsing() {
        for value in ["#00", "#0000", "#0000000", "#zxytlm"] {
            assert!(
                CfxCssDeclaration::parse_css_color(&wide(value)).is_none(),
                "{value}"
            );
        }
        for (value, expected) in [
            ("#000", 0xff00_0000),
            ("#FFF", 0xffff_ffff),
            ("#F0F0F0", 0xfff0_f0f0),
            ("#1b2F3c", 0xff1b_2f3c),
        ] {
            assert_eq!(
                CfxCssDeclaration::parse_css_color(&wide(value)),
                Some(expected),
                "{value}"
            );
        }
    }
    // cfx_cssdeclaration_unittest.cpp: complete RGBEncodingParsing vectors.
    #[test]
    fn RGBEncodingParsing() {
        assert!(CfxCssDeclaration::parse_css_color(&wide("blahblahblah")).is_none());
        assert_eq!(
            CfxCssDeclaration::parse_css_color(&wide("rgb(0, 0, 0)")),
            Some(0xff00_0000)
        );
        assert_eq!(
            CfxCssDeclaration::parse_css_color(&wide("rgb(128,255,48)")),
            Some(0xff80_ff30)
        );
    }
    #[test]
    fn malformed_colours_and_rgb_channel_limits_are_defined() {
        for (input, expected) in [
            ("rgb(-5,300,1)", 0xff00_ff01),
            ("RGB(100%,50%,0%)", 0xffff_8000),
            ("rgb(500%,-5%,0)", 0xffff_0000),
            ("Grey", 0xff80_8080),
        ] {
            assert_eq!(
                CfxCssDeclaration::parse_css_color(&wide(input)),
                Some(expected),
                "{input}"
            );
        }
        for input in [
            "",
            "#GGG",
            "#Ā00",
            "rgb(1,2)",
            "rgb(1,2,3,4)",
            "rgb(1e999,2,3)",
            "rgb(abc,2,3)",
            "rgb(1,2,3",
        ] {
            assert!(
                CfxCssDeclaration::parse_css_color(&wide(input)).is_none(),
                "{input}"
            );
        }
    }
    #[test]
    fn primitive_conversion_important_first_lookup_and_owned_custom_values() {
        let mut declaration = CfxCssDeclaration::new();
        add(&mut declaration, CfxCssProperty::Color, "red !IMPORTANT");
        let (value, important) = declaration.get_property(CfxCssProperty::Color).unwrap();
        assert!(important);
        assert!(matches!(&*value, CfxCssValue::Color(0xffff0000)));
        add(&mut declaration, CfxCssProperty::Color, "blue");
        assert_eq!(declaration.property_count_for_testing(), 2);
        let (first, _) = declaration.get_property(CfxCssProperty::Color).unwrap();
        assert!(Rc::ptr_eq(&first, &value));
        add(&mut declaration, CfxCssProperty::FontSize, "1.25em");
        assert_number(
            &get(&declaration, CfxCssProperty::FontSize),
            CfxCssNumberUnit::EMS,
            1.25,
        );
        add(&mut declaration, CfxCssProperty::TextIndent, ".0009");
        assert_number(
            &get(&declaration, CfxCssProperty::TextIndent),
            CfxCssNumberUnit::Number,
            0.0,
        );
        let before = declaration.property_count_for_testing();
        add(&mut declaration, CfxCssProperty::TextAlign, "!important");
        add(&mut declaration, CfxCssProperty::FontSize, "1e999");
        add(&mut declaration, CfxCssProperty::FontSize, "bogus");
        assert_eq!(declaration.property_count_for_testing(), before);
        for (input, output) in [
            ("'Times New Roman'", "Times New Roman"),
            ("\"serif\"", "serif"),
            ("serif", "serif"),
        ] {
            assert_eq!(
                CfxCssDeclaration::parse_css_string(&wide(input)),
                Some(wide(output).as_slice())
            );
        }
        for input in ["", "''", "\"\""] {
            assert!(CfxCssDeclaration::parse_css_string(&wide(input)).is_none());
        }
        declaration.add_custom_property(&wide("xfa-tab-stops"), &wide("center 24pt"));
        assert_eq!(
            declaration.custom_properties()[0].name,
            wide("xfa-tab-stops")
        );
        assert_eq!(
            declaration.custom_properties()[0].value,
            wide("center 24pt")
        );
    }
    #[test]
    fn four_side_shorthands_border_widths_and_immutable_value_sharing() {
        use CfxCssProperty::*;
        for (input, expected) in [
            ("1pt", [1., 1., 1., 1.]),
            ("1pt 2pt", [2., 1., 2., 1.]),
            ("1pt 2pt 3pt", [2., 1., 2., 3.]),
            ("1pt 2pt 3pt 4pt", [4., 1., 2., 3.]),
        ] {
            for (property, sides) in [
                (Margin, [MarginLeft, MarginTop, MarginRight, MarginBottom]),
                (
                    Padding,
                    [PaddingLeft, PaddingTop, PaddingRight, PaddingBottom],
                ),
                (
                    BorderWidth,
                    [
                        BorderLeftWidth,
                        BorderTopWidth,
                        BorderRightWidth,
                        BorderBottomWidth,
                    ],
                ),
            ] {
                let mut declaration = CfxCssDeclaration::new();
                add(&mut declaration, property, input);
                assert_eq!(declaration.property_count_for_testing(), 4);
                for (side, expected) in sides.into_iter().zip(expected) {
                    assert_number(&get(&declaration, side), CfxCssNumberUnit::Points, expected);
                }
            }
        }
        let mut declaration = CfxCssDeclaration::new();
        add(&mut declaration, Margin, "1pt 2pt 3pt 4pt 5pt");
        assert!(declaration.empty());
        add(&mut declaration, Border, "solid thin red !important");
        let (left, important) = declaration.get_property(BorderLeftWidth).unwrap();
        assert!(important);
        assert!(matches!(
            &*left,
            CfxCssValue::Enum(CfxCssPropertyValue::Thin)
        ));
        for side in [BorderTopWidth, BorderRightWidth, BorderBottomWidth] {
            let (value, important) = declaration.get_property(side).unwrap();
            assert!(important);
            assert!(Rc::ptr_eq(&left, &value));
        }
        let mut declaration = CfxCssDeclaration::new();
        add(&mut declaration, BorderRight, "solid red");
        assert_number(
            &get(&declaration, BorderRightWidth),
            CfxCssNumberUnit::Number,
            0.0,
        );
        add(&mut declaration, BorderLeft, "solid 3pt 7pt black");
        assert_number(
            &get(&declaration, BorderLeftWidth),
            CfxCssNumberUnit::Points,
            3.0,
        );
    }
    #[test]
    fn font_shorthand_numeric_weight_line_height_quoted_families_and_defaults() {
        use CfxCssProperty::*;
        let mut declaration = CfxCssDeclaration::new();
        add(
            &mut declaration,
            Font,
            "italic small-caps 700 12pt/18pt 'Times New Roman', serif !important",
        );
        assert_eq!(declaration.property_count_for_testing(), 6);
        assert!(declaration
            .properties()
            .iter()
            .all(|holder| holder.important));
        assert!(matches!(
            &*get(&declaration, FontStyle),
            CfxCssValue::Enum(CfxCssPropertyValue::Italic)
        ));
        assert!(matches!(
            &*get(&declaration, FontVariant),
            CfxCssValue::Enum(CfxCssPropertyValue::SmallCaps)
        ));
        assert_number(
            &get(&declaration, FontWeight),
            CfxCssNumberUnit::Number,
            700.0,
        );
        assert_number(&get(&declaration, FontSize), CfxCssNumberUnit::Points, 12.0);
        assert_number(
            &get(&declaration, LineHeight),
            CfxCssNumberUnit::Points,
            18.0,
        );
        let families = get(&declaration, FontFamily);
        let CfxCssValue::List(families) = &*families else {
            panic!("font families must be a list");
        };
        assert_eq!(families.len(), 2);
        for (family, expected) in families.iter().zip(["Times New Roman", "serif"]) {
            assert!(matches!(&**family, CfxCssValue::String(name) if name == &wide(expected)));
        }
        let mut declaration = CfxCssDeclaration::new();
        add(&mut declaration, Font, "normal");
        assert_eq!(declaration.property_count_for_testing(), 5);
        for property in [FontStyle, FontVariant, FontWeight, LineHeight] {
            assert!(matches!(
                &*get(&declaration, property),
                CfxCssValue::Enum(CfxCssPropertyValue::Normal)
            ));
        }
        assert!(matches!(
            &*get(&declaration, FontSize),
            CfxCssValue::Enum(CfxCssPropertyValue::Medium)
        ));
    }
}
