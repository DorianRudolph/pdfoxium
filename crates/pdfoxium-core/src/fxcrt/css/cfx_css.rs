// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Shared values and property enums from `core/fxcrt/css/cfx_css.h`.
//! The two source `.inc` files retain their complete hash-table order.

pub const VALUE_TYPE_PRIMITIVE: u32 = 1 << 0;
pub const VALUE_TYPE_LIST: u32 = 1 << 1;
pub const VALUE_TYPE_SHORTHAND: u32 = 1 << 2;
pub const VALUE_TYPE_MAYBE_NUMBER: u32 = 1 << 4;
pub const VALUE_TYPE_MAYBE_ENUM: u32 = 1 << 5;
pub const VALUE_TYPE_MAYBE_STRING: u32 = 1 << 7;
pub const VALUE_TYPE_MAYBE_COLOR: u32 = 1 << 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum CfxCssProperty {
    BorderLeft,
    Top,
    Margin,
    TextIndent,
    Right,
    PaddingLeft,
    MarginLeft,
    Border,
    BorderTop,
    Bottom,
    PaddingRight,
    BorderBottom,
    FontFamily,
    FontWeight,
    Color,
    LetterSpacing,
    TextAlign,
    BorderRightWidth,
    VerticalAlign,
    PaddingTop,
    FontVariant,
    BorderWidth,
    BorderBottomWidth,
    BorderRight,
    FontSize,
    BorderSpacing,
    FontStyle,
    Font,
    LineHeight,
    MarginRight,
    BorderLeftWidth,
    Display,
    PaddingBottom,
    BorderTopWidth,
    WordSpacing,
    Left,
    TextDecoration,
    Padding,
    MarginBottom,
    MarginTop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum CfxCssPropertyValue {
    Bolder,
    None,
    Dot,
    Sub,
    Top,
    Right,
    Normal,
    Auto,
    Text,
    XSmall,
    Thin,
    Small,
    Bottom,
    Underline,
    Double,
    Lighter,
    Oblique,
    Super,
    Center,
    XxLarge,
    Smaller,
    Baseline,
    Thick,
    Justify,
    Middle,
    Medium,
    ListItem,
    XxSmall,
    Bold,
    SmallCaps,
    Inline,
    Overline,
    TextBottom,
    Larger,
    InlineTable,
    InlineBlock,
    Blink,
    Block,
    Italic,
    LineThrough,
    XLarge,
    Large,
    Left,
    TextTop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum CfxCssLengthUnit {
    Auto,
    None,
    Normal,
    Point,
    Percent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum CfxCssDisplay {
    None,
    ListItem,
    Block,
    Inline,
    InlineBlock,
    InlineTable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum CfxCssFontStyle {
    Normal,
    Italic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum CfxCssTextAlign {
    Left,
    Right,
    Center,
    Justify,
    JustifyAll,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum CfxCssVerticalAlign {
    Baseline,
    Sub,
    Super,
    Top,
    TextTop,
    Middle,
    Bottom,
    TextBottom,
    Number,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum CfxCssFontVariant {
    Normal,
    SmallCaps,
}

/// Source `CFX_CSSTEXTDECORATION` bits; combinations are stored as a u8 mask.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CfxCssTextDecoration {
    None = 0,
    Underline = 1 << 0,
    Overline = 1 << 1,
    LineThrough = 1 << 2,
    Blink = 1 << 3,
    Double = 1 << 4,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CfxCssLength {
    pub unit: CfxCssLengthUnit,
    pub value: f32,
}
impl Default for CfxCssLength {
    fn default() -> Self {
        // Native default construction leaves the fields uninitialized. An
        // explicit auto/zero length is the safe initial state for offsets.
        Self::new(CfxCssLengthUnit::Auto, 0.0)
    }
}
impl CfxCssLength {
    pub const fn new(unit: CfxCssLengthUnit, value: f32) -> Self {
        Self { unit, value }
    }
    pub fn set_unit(&mut self, unit: CfxCssLengthUnit) -> &mut Self {
        self.unit = unit;
        self
    }
    pub fn set(&mut self, unit: CfxCssLengthUnit, value: f32) -> &mut Self {
        self.unit = unit;
        self.value = value;
        self
    }
    pub fn get_unit(&self) -> CfxCssLengthUnit {
        self.unit
    }
    pub fn get_value(&self) -> f32 {
        self.value
    }
    pub fn non_zero(&self) -> bool {
        // Source int truncation incorrectly hides fractional box widths.
        self.value != 0.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CfxCssRect {
    pub left: CfxCssLength,
    pub top: CfxCssLength,
    pub right: CfxCssLength,
    pub bottom: CfxCssLength,
}
impl CfxCssRect {
    pub const fn new(unit: CfxCssLengthUnit, value: f32) -> Self {
        let length = CfxCssLength::new(unit, value);
        Self {
            left: length,
            top: length,
            right: length,
            bottom: length,
        }
    }
    pub fn set_unit(&mut self, unit: CfxCssLengthUnit) -> &mut Self {
        self.left.set_unit(unit);
        self.top.set_unit(unit);
        self.right.set_unit(unit);
        self.bottom.set_unit(unit);
        self
    }
    pub fn set(&mut self, unit: CfxCssLengthUnit, value: f32) -> &mut Self {
        self.left.set(unit, value);
        self.top.set(unit, value);
        self.right.set(unit, value);
        self.bottom.set(unit, value);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_lengths_and_rect_updates_keep_their_units_and_values() {
        for value in [0.25, -0.5, 1.0] {
            assert!(CfxCssLength::new(CfxCssLengthUnit::Point, value).non_zero());
        }
        for value in [0.0, -0.0] {
            assert!(!CfxCssLength::new(CfxCssLengthUnit::Percent, value).non_zero());
        }
        let mut rect = CfxCssRect::new(CfxCssLengthUnit::Point, 0.5);
        rect.set_unit(CfxCssLengthUnit::Percent);
        for length in [rect.left, rect.top, rect.right, rect.bottom] {
            assert_eq!(length, CfxCssLength::new(CfxCssLengthUnit::Percent, 0.5));
        }
        rect.set(CfxCssLengthUnit::Point, 2.0);
        assert_eq!(rect, CfxCssRect::new(CfxCssLengthUnit::Point, 2.0));
        assert_eq!(
            CfxCssLength::default(),
            CfxCssLength::new(CfxCssLengthUnit::Auto, 0.0)
        );
    }
}
