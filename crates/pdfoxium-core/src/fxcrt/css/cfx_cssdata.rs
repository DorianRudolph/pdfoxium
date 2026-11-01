// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Complete lookup tables from `cfx_cssdata.cpp`, `properties.inc` and
//! `property_values.inc`, retaining the source hash order for binary lookup.
use super::cfx_css::*;
use super::cfx_cssnumbervalue::CfxCssNumberUnit;

#[derive(Clone, Copy, Debug)]
pub struct Property {
    pub name: CfxCssProperty,
    pub hash: u32,
    pub types: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct PropertyValue {
    pub name: CfxCssPropertyValue,
    pub hash: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct LengthUnit {
    pub value: &'static str,
    pub unit: CfxCssNumberUnit,
}
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub name: &'static str,
    pub value: u32,
}

const PROPERTY_TABLE: &[Property] = &[
    Property {
        name: CfxCssProperty::BorderLeft,
        hash: 0x04080036,
        types: VALUE_TYPE_SHORTHAND,
    },
    Property {
        name: CfxCssProperty::Top,
        hash: 0x0BEDAF33,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::Margin,
        hash: 0x0CB016BE,
        types: VALUE_TYPE_LIST | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::TextIndent,
        hash: 0x169ADB74,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::Right,
        hash: 0x193ADE3E,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::PaddingLeft,
        hash: 0x228CF02F,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::MarginLeft,
        hash: 0x297C5656,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_NUMBER | VALUE_TYPE_MAYBE_ENUM,
    },
    Property {
        name: CfxCssProperty::Border,
        hash: 0x2A23349E,
        types: VALUE_TYPE_SHORTHAND,
    },
    Property {
        name: CfxCssProperty::BorderTop,
        hash: 0x2B866ADE,
        types: VALUE_TYPE_SHORTHAND,
    },
    Property {
        name: CfxCssProperty::Bottom,
        hash: 0x399F02B5,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::PaddingRight,
        hash: 0x3F616AC2,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::BorderBottom,
        hash: 0x452CE780,
        types: VALUE_TYPE_SHORTHAND,
    },
    Property {
        name: CfxCssProperty::FontFamily,
        hash: 0x574686E6,
        types: VALUE_TYPE_LIST | VALUE_TYPE_MAYBE_STRING,
    },
    Property {
        name: CfxCssProperty::FontWeight,
        hash: 0x6692F60C,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::Color,
        hash: 0x6E67921F,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_COLOR,
    },
    Property {
        name: CfxCssProperty::LetterSpacing,
        hash: 0x70536102,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::TextAlign,
        hash: 0x7553F1BD,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM,
    },
    Property {
        name: CfxCssProperty::BorderRightWidth,
        hash: 0x8F5A6036,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::VerticalAlign,
        hash: 0x934A87D2,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::PaddingTop,
        hash: 0x959D22B7,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::FontVariant,
        hash: 0x9C785779,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM,
    },
    Property {
        name: CfxCssProperty::BorderWidth,
        hash: 0xA8DE4FEB,
        types: VALUE_TYPE_LIST | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::BorderBottomWidth,
        hash: 0xAE41204D,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::BorderRight,
        hash: 0xB78E9EA9,
        types: VALUE_TYPE_SHORTHAND,
    },
    Property {
        name: CfxCssProperty::FontSize,
        hash: 0xB93956DF,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::BorderSpacing,
        hash: 0xC72030F0,
        types: VALUE_TYPE_LIST | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::FontStyle,
        hash: 0xCB1950F5,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM,
    },
    Property {
        name: CfxCssProperty::Font,
        hash: 0xCD308B77,
        types: VALUE_TYPE_SHORTHAND,
    },
    Property {
        name: CfxCssProperty::LineHeight,
        hash: 0xCFCACE2E,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::MarginRight,
        hash: 0xD13C58C9,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_NUMBER | VALUE_TYPE_MAYBE_ENUM,
    },
    Property {
        name: CfxCssProperty::BorderLeftWidth,
        hash: 0xD1E93D83,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::Display,
        hash: 0xD4224C36,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM,
    },
    Property {
        name: CfxCssProperty::PaddingBottom,
        hash: 0xE555B3B9,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::BorderTopWidth,
        hash: 0xED2CB62B,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::WordSpacing,
        hash: 0xEDA63BAE,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::Left,
        hash: 0xF5AD782B,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_ENUM | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::TextDecoration,
        hash: 0xF7C634BA,
        types: VALUE_TYPE_LIST | VALUE_TYPE_MAYBE_ENUM,
    },
    Property {
        name: CfxCssProperty::Padding,
        hash: 0xF8C373F7,
        types: VALUE_TYPE_LIST | VALUE_TYPE_MAYBE_NUMBER,
    },
    Property {
        name: CfxCssProperty::MarginBottom,
        hash: 0xF93485A0,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_NUMBER | VALUE_TYPE_MAYBE_ENUM,
    },
    Property {
        name: CfxCssProperty::MarginTop,
        hash: 0xFE51DCFE,
        types: VALUE_TYPE_PRIMITIVE | VALUE_TYPE_MAYBE_NUMBER | VALUE_TYPE_MAYBE_ENUM,
    },
];

const PROPERTY_VALUE_TABLE: &[PropertyValue] = &[
    PropertyValue {
        name: CfxCssPropertyValue::Bolder,
        hash: 0x009F1058,
    },
    PropertyValue {
        name: CfxCssPropertyValue::None,
        hash: 0x048B6670,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Dot,
        hash: 0x0A48CB27,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Sub,
        hash: 0x0BD37FAA,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Top,
        hash: 0x0BEDAF33,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Right,
        hash: 0x193ADE3E,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Normal,
        hash: 0x247CF3E9,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Auto,
        hash: 0x2B35B6D9,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Text,
        hash: 0x2D08AF85,
    },
    PropertyValue {
        name: CfxCssPropertyValue::XSmall,
        hash: 0x2D2FCAFE,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Thin,
        hash: 0x2D574D53,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Small,
        hash: 0x316A3739,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Bottom,
        hash: 0x399F02B5,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Underline,
        hash: 0x3A0273A6,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Double,
        hash: 0x3D98515B,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Lighter,
        hash: 0x45BEB7AF,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Oblique,
        hash: 0x53EBDDB1,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Super,
        hash: 0x6A4F842F,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Center,
        hash: 0x6C51AFC1,
    },
    PropertyValue {
        name: CfxCssPropertyValue::XxLarge,
        hash: 0x70BB1508,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Smaller,
        hash: 0x849769F0,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Baseline,
        hash: 0x87436BA3,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Thick,
        hash: 0x8CC35EB3,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Justify,
        hash: 0x8D269CAE,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Middle,
        hash: 0x947FA00F,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Medium,
        hash: 0xA084A381,
    },
    PropertyValue {
        name: CfxCssPropertyValue::ListItem,
        hash: 0xA32382B8,
    },
    PropertyValue {
        name: CfxCssPropertyValue::XxSmall,
        hash: 0xADE1FC76,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Bold,
        hash: 0xB18313A1,
    },
    PropertyValue {
        name: CfxCssPropertyValue::SmallCaps,
        hash: 0xB299428D,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Inline,
        hash: 0xC02D649F,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Overline,
        hash: 0xC0EC9FA4,
    },
    PropertyValue {
        name: CfxCssPropertyValue::TextBottom,
        hash: 0xC7D08D87,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Larger,
        hash: 0xCD3C409D,
    },
    PropertyValue {
        name: CfxCssPropertyValue::InlineTable,
        hash: 0xD131F494,
    },
    PropertyValue {
        name: CfxCssPropertyValue::InlineBlock,
        hash: 0xD26A8BD7,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Blink,
        hash: 0xDC36E390,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Block,
        hash: 0xDCD480AB,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Italic,
        hash: 0xE31D5396,
    },
    PropertyValue {
        name: CfxCssPropertyValue::LineThrough,
        hash: 0xE4C5A276,
    },
    PropertyValue {
        name: CfxCssPropertyValue::XLarge,
        hash: 0xF008E390,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Large,
        hash: 0xF4434FCB,
    },
    PropertyValue {
        name: CfxCssPropertyValue::Left,
        hash: 0xF5AD782B,
    },
    PropertyValue {
        name: CfxCssPropertyValue::TextTop,
        hash: 0xFCB58D45,
    },
];

const LENGTH_UNIT_TABLE: &[LengthUnit] = &[
    LengthUnit {
        value: "cm",
        unit: CfxCssNumberUnit::CentiMeters,
    },
    LengthUnit {
        value: "em",
        unit: CfxCssNumberUnit::EMS,
    },
    LengthUnit {
        value: "ex",
        unit: CfxCssNumberUnit::EXS,
    },
    LengthUnit {
        value: "in",
        unit: CfxCssNumberUnit::Inches,
    },
    LengthUnit {
        value: "mm",
        unit: CfxCssNumberUnit::MilliMeters,
    },
    LengthUnit {
        value: "pc",
        unit: CfxCssNumberUnit::Picas,
    },
    LengthUnit {
        value: "pt",
        unit: CfxCssNumberUnit::Points,
    },
    LengthUnit {
        value: "px",
        unit: CfxCssNumberUnit::Pixels,
    },
];

const COLOR_TABLE: &[Color] = &[
    Color {
        name: "aqua",
        value: 0xff00ffff,
    },
    Color {
        name: "black",
        value: 0xff000000,
    },
    Color {
        name: "blue",
        value: 0xff0000ff,
    },
    Color {
        name: "fuchsia",
        value: 0xffff00ff,
    },
    Color {
        name: "gray",
        value: 0xff808080,
    },
    Color {
        name: "green",
        value: 0xff008000,
    },
    Color {
        name: "grey",
        value: 0xff808080,
    },
    Color {
        name: "lime",
        value: 0xff00ff00,
    },
    Color {
        name: "maroon",
        value: 0xff800000,
    },
    Color {
        name: "navy",
        value: 0xff000080,
    },
    Color {
        name: "olive",
        value: 0xff808000,
    },
    Color {
        name: "orange",
        value: 0xffffa500,
    },
    Color {
        name: "purple",
        value: 0xff800080,
    },
    Color {
        name: "red",
        value: 0xffff0000,
    },
    Color {
        name: "silver",
        value: 0xffc0c0c0,
    },
    Color {
        name: "teal",
        value: 0xff008080,
    },
    Color {
        name: "white",
        value: 0xffffffff,
    },
    Color {
        name: "yellow",
        value: 0xffffff00,
    },
];

/// `FX_HashCode_GetLoweredW`: one lowered scalar per wide input unit.
/// Invalid wchar values remain unchanged. ICU4X supplies the source simple
/// lowercase operation (not full-string case folding).
pub fn hash_code_lowered_wide(text: &[u32]) -> u32 {
    crate::fxcrt::widestring::hash_code_lowered(text)
}

fn equals_ascii_no_case(text: &[u32], ascii: &str) -> bool {
    text.len() == ascii.len()
        && text
            .iter()
            .zip(ascii.bytes())
            .all(|(&point, byte)| point <= 0x7f && (point as u8).eq_ignore_ascii_case(&byte))
}
pub struct CfxCssData;
impl CfxCssData {
    pub fn hash_name(name: &[u32]) -> u32 {
        hash_code_lowered_wide(name)
    }
    pub fn get_property_by_name(name: &[u32]) -> Option<&'static Property> {
        if name.is_empty() {
            return None;
        }
        let hash = hash_code_lowered_wide(name);
        PROPERTY_TABLE
            .binary_search_by_key(&hash, |property| property.hash)
            .ok()
            .map(|index| &PROPERTY_TABLE[index])
    }
    pub fn get_property_by_enum(property: CfxCssProperty) -> &'static Property {
        &PROPERTY_TABLE[property as usize]
    }
    pub fn get_property_value_by_name(name: &[u32]) -> Option<&'static PropertyValue> {
        if name.is_empty() {
            return None;
        }
        let hash = hash_code_lowered_wide(name);
        PROPERTY_VALUE_TABLE
            .binary_search_by_key(&hash, |property| property.hash)
            .ok()
            .map(|index| &PROPERTY_VALUE_TABLE[index])
    }
    pub fn get_length_unit_by_name(name: &[u32]) -> Option<&'static LengthUnit> {
        if name.len() != 2 {
            return None;
        }
        LENGTH_UNIT_TABLE
            .iter()
            .find(|unit| equals_ascii_no_case(name, unit.value))
    }
    pub fn get_color_by_name(name: &[u32]) -> Option<&'static Color> {
        if name.is_empty() {
            return None;
        }
        COLOR_TABLE
            .iter()
            .find(|color| equals_ascii_no_case(name, color.name))
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    const PROPERTY_NAMES: &[&str] = &[
        "border-left",
        "top",
        "margin",
        "text-indent",
        "right",
        "padding-left",
        "margin-left",
        "border",
        "border-top",
        "bottom",
        "padding-right",
        "border-bottom",
        "font-family",
        "font-weight",
        "color",
        "letter-spacing",
        "text-align",
        "border-right-width",
        "vertical-align",
        "padding-top",
        "font-variant",
        "border-width",
        "border-bottom-width",
        "border-right",
        "font-size",
        "border-spacing",
        "font-style",
        "font",
        "line-height",
        "margin-right",
        "border-left-width",
        "display",
        "padding-bottom",
        "border-top-width",
        "word-spacing",
        "left",
        "text-decoration",
        "padding",
        "margin-bottom",
        "margin-top",
    ];
    const PROPERTY_VALUE_NAMES: &[&str] = &[
        "bolder",
        "none",
        "dot",
        "sub",
        "top",
        "right",
        "normal",
        "auto",
        "text",
        "x-small",
        "thin",
        "small",
        "bottom",
        "underline",
        "double",
        "lighter",
        "oblique",
        "super",
        "center",
        "xx-large",
        "smaller",
        "baseline",
        "thick",
        "justify",
        "middle",
        "medium",
        "list-item",
        "xx-small",
        "bold",
        "small-caps",
        "inline",
        "overline",
        "text-bottom",
        "larger",
        "inline-table",
        "inline-block",
        "blink",
        "block",
        "italic",
        "line-through",
        "x-large",
        "large",
        "left",
        "text-top",
    ];
    // Both complete macro-expanded cases from cfx_cssdata_unittest.cpp.
    #[test]
    fn PropertyHashes() {
        assert_eq!(PROPERTY_TABLE.len(), PROPERTY_NAMES.len());
        let mut max_hash = 0;
        for (property, name) in PROPERTY_TABLE.iter().zip(PROPERTY_NAMES) {
            let actual = wide(name).iter().fold(0u32, |hash, &point| {
                hash.wrapping_mul(1313).wrapping_add(point)
            });
            assert_eq!(actual, property.hash, "{name}");
            assert!(property.hash > max_hash, "{name}");
            max_hash = property.hash;
            assert_eq!(
                CfxCssData::get_property_by_name(&wide(name)).unwrap().name,
                property.name
            );
            assert_eq!(
                CfxCssData::get_property_by_enum(property.name).hash,
                property.hash
            );
        }
    }
    #[test]
    fn PropertyValueHashes() {
        assert_eq!(PROPERTY_VALUE_TABLE.len(), PROPERTY_VALUE_NAMES.len());
        let mut max_hash = 0;
        for (property, name) in PROPERTY_VALUE_TABLE.iter().zip(PROPERTY_VALUE_NAMES) {
            let actual = wide(name).iter().fold(0u32, |hash, &point| {
                hash.wrapping_mul(1313).wrapping_add(point)
            });
            assert_eq!(actual, property.hash, "{name}");
            assert!(property.hash > max_hash, "{name}");
            max_hash = property.hash;
            assert_eq!(
                CfxCssData::get_property_value_by_name(&wide(name))
                    .unwrap()
                    .name,
                property.name
            );
        }
    }
    #[test]
    fn lookup_rejects_unknown_values_and_preserves_case_units_and_argb_colors() {
        for name in ["", "invalid"] {
            assert!(CfxCssData::get_property_by_name(&wide(name)).is_none());
            assert!(CfxCssData::get_property_value_by_name(&wide(name)).is_none());
            assert!(CfxCssData::get_color_by_name(&wide(name)).is_none());
        }
        assert_eq!(
            CfxCssData::get_property_by_name(&wide("FONT-SIZE"))
                .unwrap()
                .name,
            CfxCssProperty::FontSize
        );
        for unit in LENGTH_UNIT_TABLE {
            let upper = unit.value.to_ascii_uppercase();
            assert_eq!(
                CfxCssData::get_length_unit_by_name(&wide(&upper))
                    .unwrap()
                    .unit,
                unit.unit
            );
        }
        for name in ["", "p", "ptx", "%", "zz"] {
            assert!(CfxCssData::get_length_unit_by_name(&wide(name)).is_none());
        }
        for color in COLOR_TABLE {
            let upper = color.name.to_ascii_uppercase();
            assert_eq!(
                CfxCssData::get_color_by_name(&wide(&upper)).unwrap().value,
                color.value
            );
        }
        assert_eq!(
            CfxCssData::get_color_by_name(&wide("red")).unwrap().value,
            0xffff0000
        );
        assert_eq!(
            hash_code_lowered_wide(&wide("Äİ")),
            hash_code_lowered_wide(&wide("äi"))
        );
        assert_eq!(hash_code_lowered_wide(&[0xd800]), 0xd800);
        assert_eq!(hash_code_lowered_wide(&[0x110000]), 0x110000);
    }
}
