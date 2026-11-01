// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxge/cfx_color.h` and `.cpp`.

use std::ops::{Div, Sub};

use super::dib::fx_dib::argb_encode;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(i32)]
pub enum ColorType {
    Transparent,
    Gray,
    Rgb,
    Cmyk,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeAndArgb {
    pub color_type: ColorType,
    pub argb: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct CfxColor {
    pub color_type: ColorType,
    pub color1: f32,
    pub color2: f32,
    pub color3: f32,
    pub color4: f32,
}

impl Default for CfxColor {
    fn default() -> Self {
        Self::make_transparent()
    }
}

impl CfxColor {
    pub const fn make_transparent() -> Self {
        Self::make_cmyk_color(ColorType::Transparent, 0.0, 0.0, 0.0, 0.0)
    }

    pub const fn make_gray(gray: f32) -> Self {
        Self::make_cmyk_color(ColorType::Gray, gray, 0.0, 0.0, 0.0)
    }

    pub const fn make_rgb(red: f32, green: f32, blue: f32) -> Self {
        Self::make_cmyk_color(ColorType::Rgb, red, green, blue, 0.0)
    }

    pub const fn make_cmyk(cyan: f32, magenta: f32, yellow: f32, key: f32) -> Self {
        Self::make_cmyk_color(ColorType::Cmyk, cyan, magenta, yellow, key)
    }

    pub const fn make_rgb_bytes(red: i32, green: i32, blue: i32) -> Self {
        Self::make_rgb(
            red as f32 / 255.0,
            green as f32 / 255.0,
            blue as f32 / 255.0,
        )
    }

    pub const fn from_color_ref(reference: u32) -> Self {
        Self::make_rgb_bytes(
            ((reference >> 16) as u8) as i32,
            ((reference >> 8) as u8) as i32,
            (reference as u8) as i32,
        )
    }

    const fn make_cmyk_color(
        color_type: ColorType,
        color1: f32,
        color2: f32,
        color3: f32,
        color4: f32,
    ) -> Self {
        Self {
            color_type,
            color1,
            color2,
            color3,
            color4,
        }
    }

    pub fn convert_color_type(self, target: ColorType) -> Self {
        if self.color_type == target {
            return self;
        }
        match (self.color_type, target) {
            (ColorType::Transparent, _) | (_, ColorType::Transparent) => Self::make_transparent(),
            (ColorType::Gray, ColorType::Rgb) => gray_to_rgb(self.color1),
            (ColorType::Gray, ColorType::Cmyk) => gray_to_cmyk(self.color1),
            (ColorType::Rgb, ColorType::Gray) => rgb_to_gray(self.color1, self.color2, self.color3),
            (ColorType::Rgb, ColorType::Cmyk) => rgb_to_cmyk(self.color1, self.color2, self.color3),
            (ColorType::Cmyk, ColorType::Gray) => {
                cmyk_to_gray(self.color1, self.color2, self.color3, self.color4)
            }
            (ColorType::Cmyk, ColorType::Rgb) => {
                cmyk_to_rgb(self.color1, self.color2, self.color3, self.color4)
            }
            _ => unreachable!("equal color types returned above"),
        }
    }

    pub fn to_fx_color(self, transparency: i32) -> u32 {
        let mut color = match self.color_type {
            ColorType::Transparent => Self::make_transparent(),
            ColorType::Gray => gray_to_rgb(self.color1),
            ColorType::Rgb => Self::make_rgb(self.color1, self.color2, self.color3),
            ColorType::Cmyk => cmyk_to_rgb(self.color1, self.color2, self.color3, self.color4),
        };
        if self.color_type != ColorType::Transparent {
            color.color4 = transparency as f32;
        }
        // PDFium casts each integer argument to uint32_t inside ArgbEncode.
        argb_encode(
            color.color4 as i32 as u32,
            (color.color1 * 255.0) as i32 as u32,
            (color.color2 * 255.0) as i32 as u32,
            (color.color3 * 255.0) as i32 as u32,
        )
    }
}

impl PartialEq for CfxColor {
    fn eq(&self, other: &Self) -> bool {
        if self.color_type != other.color_type {
            return false;
        }
        fn near(a: f32, b: f32) -> bool {
            a - b < 0.0001 && a - b > -0.0001
        }
        match self.color_type {
            ColorType::Transparent => true,
            ColorType::Gray => near(self.color1, other.color1),
            ColorType::Rgb => {
                near(self.color1, other.color1)
                    && near(self.color2, other.color2)
                    && near(self.color3, other.color3)
            }
            ColorType::Cmyk => {
                near(self.color1, other.color1)
                    && near(self.color2, other.color2)
                    && near(self.color3, other.color3)
                    && near(self.color4, other.color4)
            }
        }
    }
}

impl Sub<f32> for CfxColor {
    type Output = Self;

    fn sub(self, amount: f32) -> Self {
        let mut result = Self::make_transparent();
        result.color_type = self.color_type;
        match self.color_type {
            ColorType::Transparent => {
                result = Self::make_rgb(
                    cpp_max(1.0 - amount, 0.0),
                    cpp_max(1.0 - amount, 0.0),
                    cpp_max(1.0 - amount, 0.0),
                );
            }
            ColorType::Gray => result.color1 = cpp_max(self.color1 - amount, 0.0),
            ColorType::Rgb => {
                result.color1 = cpp_max(self.color1 - amount, 0.0);
                result.color2 = cpp_max(self.color2 - amount, 0.0);
                result.color3 = cpp_max(self.color3 - amount, 0.0);
            }
            ColorType::Cmyk => {
                result.color1 = cpp_max(self.color1 - amount, 0.0);
                result.color2 = cpp_max(self.color2 - amount, 0.0);
                result.color3 = cpp_max(self.color3 - amount, 0.0);
                result.color4 = cpp_max(self.color4 - amount, 0.0);
            }
        }
        result
    }
}

impl Div<f32> for CfxColor {
    type Output = Self;

    fn div(self, divisor: f32) -> Self {
        let mut result = Self::make_transparent();
        result.color_type = self.color_type;
        match self.color_type {
            ColorType::Transparent => {
                result = Self::make_rgb(1.0 / divisor, 1.0 / divisor, 1.0 / divisor);
            }
            ColorType::Gray => result.color1 = self.color1 / divisor,
            ColorType::Rgb => {
                result.color1 = self.color1 / divisor;
                result.color2 = self.color2 / divisor;
                result.color3 = self.color3 / divisor;
            }
            ColorType::Cmyk => {
                result.color1 = self.color1 / divisor;
                result.color2 = self.color2 / divisor;
                result.color3 = self.color3 / divisor;
                result.color4 = self.color4 / divisor;
            }
        }
        result
    }
}

fn in_range(component: f32) -> bool {
    (0.0..=1.0).contains(&component)
}

// Keep std::min/max's first-operand choice for unordered floats.
fn cpp_min(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

fn cpp_max(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

fn cmyk_to_gray(c: f32, m: f32, y: f32, k: f32) -> CfxColor {
    if ![c, m, y, k].into_iter().all(in_range) {
        return CfxColor::make_gray(0.0);
    }
    CfxColor::make_gray(1.0 - cpp_min(1.0, 0.3 * c + 0.59 * m + 0.11 * y + k))
}

fn gray_to_cmyk(gray: f32) -> CfxColor {
    if !in_range(gray) {
        return CfxColor::make_cmyk(0.0, 0.0, 0.0, 0.0);
    }
    CfxColor::make_cmyk(0.0, 0.0, 0.0, 1.0 - gray)
}

fn gray_to_rgb(gray: f32) -> CfxColor {
    if !in_range(gray) {
        return CfxColor::make_rgb(0.0, 0.0, 0.0);
    }
    CfxColor::make_rgb(gray, gray, gray)
}

fn rgb_to_gray(r: f32, g: f32, b: f32) -> CfxColor {
    if ![r, g, b].into_iter().all(in_range) {
        return CfxColor::make_gray(0.0);
    }
    CfxColor::make_gray(0.3 * r + 0.59 * g + 0.11 * b)
}

fn cmyk_to_rgb(c: f32, m: f32, y: f32, k: f32) -> CfxColor {
    if ![c, m, y, k].into_iter().all(in_range) {
        return CfxColor::make_rgb(0.0, 0.0, 0.0);
    }
    CfxColor::make_rgb(
        1.0 - cpp_min(1.0, c + k),
        1.0 - cpp_min(1.0, m + k),
        1.0 - cpp_min(1.0, y + k),
    )
}

fn rgb_to_cmyk(r: f32, g: f32, b: f32) -> CfxColor {
    if ![r, g, b].into_iter().all(in_range) {
        return CfxColor::make_cmyk(0.0, 0.0, 0.0, 0.0);
    }
    let c = 1.0 - r;
    let m = 1.0 - g;
    let y = 1.0 - b;
    CfxColor::make_cmyk(c, m, y, cpp_min(c, cpp_min(m, y)))
}

#[cfg(test)]
mod tests {
    use super::{CfxColor, ColorType};

    #[test]
    fn inactive_components_stay_zero() {
        let gray = CfxColor::make_gray(0.2);
        let subtracted = gray - (-0.5);
        assert_eq!(subtracted, CfxColor::make_gray(0.7));
        assert_eq!(
            (subtracted.color2, subtracted.color3, subtracted.color4),
            (0.0, 0.0, 0.0)
        );
        let divided = CfxColor::make_gray(0.8) / 2.0;
        assert_eq!(divided, CfxColor::make_gray(0.4));
        assert_eq!(
            (divided.color2, divided.color3, divided.color4),
            (0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn conversion_and_packing() {
        assert_eq!(
            CfxColor::make_transparent().convert_color_type(ColorType::Rgb),
            CfxColor::make_transparent()
        );
        assert_eq!(
            CfxColor::make_rgb(1.0, 0.0, 0.0).to_fx_color(128),
            0x80ff0000
        );
        assert_eq!(
            CfxColor::make_gray(2.0).convert_color_type(ColorType::Rgb),
            CfxColor::make_rgb(0.0, 0.0, 0.0)
        );
        assert_eq!(
            CfxColor::make_rgb(1.0, 0.0, 0.0)
                .convert_color_type(ColorType::Cmyk)
                .color4,
            0.0
        );
    }
}

#[cfg(test)]
#[path = "cfx_color_unittest.rs"]
mod upstream_tests;
