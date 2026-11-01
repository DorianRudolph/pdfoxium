// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxge/dib/fx_dib.{h,cpp}` for the AGG configuration.

pub type FxArgb = u32;
pub type FxCmyk = u32;
pub type FxColorRef = u32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum FxdibFormat {
    Invalid = 0,
    OneBppRgb = 0x001,
    EightBppRgb = 0x008,
    Bgr = 0x018,
    Bgrx = 0x020,
    OneBppMask = 0x101,
    EightBppMask = 0x108,
    Bgra = 0x220,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum BlendMode {
    Normal = 0,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub const LAST: Self = Self::Luminosity;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FxdibResampleOptions {
    pub interpolate_bilinear: bool,
    pub halftone: bool,
    pub no_smoothing: bool,
    pub lossy: bool,
}

impl FxdibResampleOptions {
    pub fn has_any_options(&self) -> bool {
        self.interpolate_bilinear || self.halftone || self.no_smoothing || self.lossy
    }
}

macro_rules! pixel_struct {
    ($name:ident { $($field:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq)]
        #[repr(C)]
        pub struct $name<T> { $(pub $field: T),+ }
    };
}

pixel_struct!(FxRgbStruct { red, green, blue });
pixel_struct!(FxBgrStruct { blue, green, red });
pixel_struct!(FxArgbStruct {
    alpha,
    red,
    green,
    blue
});
pixel_struct!(FxAbgrStruct {
    alpha,
    blue,
    green,
    red
});
pixel_struct!(FxRgbaStruct {
    red,
    green,
    blue,
    alpha
});
pixel_struct!(FxBgraStruct {
    blue,
    green,
    red,
    alpha
});
pixel_struct!(FxCmykStruct {
    cyan,
    magenta,
    yellow,
    key
});
pixel_struct!(FxLabStruct {
    lightness_star,
    a_star,
    b_star
});

pub const fn fxsys_bgr(blue: u8, green: u8, red: u8) -> u32 {
    ((blue as u32) << 16) | ((green as u32) << 8) | red as u32
}

pub const fn fxsys_get_r_value(bgr: u32) -> u8 {
    bgr as u8
}

pub const fn fxsys_get_g_value(bgr: u32) -> u8 {
    (bgr >> 8) as u8
}

pub const fn fxsys_get_b_value(bgr: u32) -> u8 {
    (bgr >> 16) as u8
}

pub fn fxsys_get_unsigned_alpha(alpha: f32) -> u32 {
    (alpha * 255.0 + 0.5) as u32
}

pub const fn get_bpp_from_format(format: FxdibFormat) -> i32 {
    (format as u16 & 0xff) as i32
}

pub const fn get_comps_from_format(format: FxdibFormat) -> i32 {
    get_bpp_from_format(format) / 8
}

pub const fn get_is_mask_from_format(format: FxdibFormat) -> bool {
    (format as u16 & 0x100) != 0
}

pub const fn get_is_alpha_from_format(format: FxdibFormat) -> bool {
    (format as u16 & 0x200) != 0
}

pub const fn argb_a(argb: FxArgb) -> u8 {
    (argb >> 24) as u8
}

pub const fn argb_r(argb: FxArgb) -> u8 {
    (argb >> 16) as u8
}

pub const fn argb_g(argb: FxArgb) -> u8 {
    (argb >> 8) as u8
}

pub const fn argb_b(argb: FxArgb) -> u8 {
    argb as u8
}

pub const fn argb_encode(alpha: u32, red: u32, green: u32, blue: u32) -> FxArgb {
    (alpha << 24) | (red << 16) | (green << 8) | blue
}

pub const fn cmyk_encode(cyan: u32, magenta: u32, yellow: u32, key: u32) -> FxCmyk {
    (cyan << 24) | (magenta << 16) | (yellow << 8) | key
}

pub const fn argb_mul_alpha(argb: FxArgb, alpha: u32) -> FxArgb {
    (((argb >> 24).wrapping_mul(alpha) / 255) << 24) | (argb & 0x00ff_ffff)
}

pub const fn rgb_to_gray(red: i32, green: i32, blue: i32) -> i32 {
    (blue * 11 + green * 59 + red * 30) / 100
}

pub const fn alpha_merge(backdrop: i32, source: i32, source_alpha: i32) -> i32 {
    (backdrop * (255 - source_alpha) + source * source_alpha) / 255
}

pub const fn cmyk_to_dib(cmyk: FxCmyk) -> u32 {
    ((cmyk >> 24) as u8 as u32)
        | (((cmyk >> 16) as u8 as u32) << 8)
        | (((cmyk >> 8) as u8 as u32) << 16)
        | ((cmyk as u8 as u32) << 24)
}

pub const fn argb_to_bgr_order_dib(argb: FxArgb) -> u32 {
    ((argb >> 16) as u8 as u32)
        | (((argb >> 8) as u8 as u32) << 8)
        | ((argb as u8 as u32) << 16)
        | (((argb >> 24) as u8 as u32) << 24)
}

pub const fn argb_get_dib(pixel: &[u8; 4]) -> FxArgb {
    argb_encode(
        pixel[3] as u32,
        pixel[2] as u32,
        pixel[1] as u32,
        pixel[0] as u32,
    )
}

pub fn argb_set_dib(pixel: &mut [u8; 4], argb: FxArgb) {
    *pixel = [argb_b(argb), argb_g(argb), argb_r(argb), argb_a(argb)];
}

pub fn reverse_copy_3_bytes(dest: &mut [u8; 3], src: &[u8; 3]) {
    *dest = [src[2], src[1], src[0]];
}

pub const fn argb_to_bgra_struct(argb: FxArgb) -> FxBgraStruct<u8> {
    FxBgraStruct {
        blue: argb_b(argb),
        green: argb_g(argb),
        red: argb_r(argb),
        alpha: argb_a(argb),
    }
}

pub const fn argb_to_bgr_struct(argb: FxArgb) -> FxBgrStruct<u8> {
    FxBgrStruct {
        blue: argb_b(argb),
        green: argb_g(argb),
        red: argb_r(argb),
    }
}

pub const fn argb_to_color_ref(argb: FxArgb) -> FxColorRef {
    fxsys_bgr(argb_b(argb), argb_g(argb), argb_r(argb))
}

pub const fn argb_to_alpha_and_color_ref(argb: FxArgb) -> (u8, FxColorRef) {
    (argb_a(argb), argb_to_color_ref(argb))
}

pub const fn alpha_and_color_ref_to_argb(alpha: i32, color_ref: FxColorRef) -> FxArgb {
    argb_encode(
        alpha as u32,
        fxsys_get_r_value(color_ref) as u32,
        fxsys_get_g_value(color_ref) as u32,
        fxsys_get_b_value(color_ref) as u32,
    )
}

pub trait PixelChannel: Copy + Default + PartialEq {
    fn to_float(self) -> f32;
    fn from_float(value: f32) -> Self;
    fn opaque() -> Self;
}

impl PixelChannel for u8 {
    fn to_float(self) -> f32 {
        self as f32
    }
    fn from_float(value: f32) -> Self {
        value as u8
    }
    fn opaque() -> Self {
        255
    }
}

impl PixelChannel for f32 {
    fn to_float(self) -> f32 {
        self
    }
    fn from_float(value: f32) -> Self {
        value
    }
    fn opaque() -> Self {
        255.0
    }
}

pub trait AlphaPixel: Copy {
    type Channel: PixelChannel;
    fn channels(self) -> (Self::Channel, Self::Channel, Self::Channel, Self::Channel);
    fn from_channels(
        alpha: Self::Channel,
        red: Self::Channel,
        green: Self::Channel,
        blue: Self::Channel,
    ) -> Self;
}

macro_rules! alpha_pixel {
    ($name:ident) => {
        impl<T: PixelChannel> AlphaPixel for $name<T> {
            type Channel = T;
            fn channels(self) -> (T, T, T, T) {
                (self.alpha, self.red, self.green, self.blue)
            }
            fn from_channels(alpha: T, red: T, green: T, blue: T) -> Self {
                Self {
                    alpha,
                    red,
                    green,
                    blue,
                }
            }
        }
    };
}

alpha_pixel!(FxArgbStruct);
alpha_pixel!(FxAbgrStruct);
alpha_pixel!(FxRgbaStruct);
alpha_pixel!(FxBgraStruct);

pub fn pre_multiply_color<P: AlphaPixel>(input: P) -> P {
    let (alpha, red, green, blue) = input.channels();
    if alpha == P::Channel::opaque() {
        return input;
    }
    let alpha_f = alpha.to_float();
    P::from_channels(
        alpha,
        P::Channel::from_float(red.to_float() * alpha_f / 255.0),
        P::Channel::from_float(green.to_float() * alpha_f / 255.0),
        P::Channel::from_float(blue.to_float() * alpha_f / 255.0),
    )
}

pub fn un_pre_multiply_color<P: AlphaPixel>(input: P) -> P {
    let (alpha, red, green, blue) = input.channels();
    if alpha == P::Channel::opaque() {
        return input;
    }
    if alpha.to_float() == 0.0 {
        return P::from_channels(
            alpha,
            P::Channel::default(),
            P::Channel::default(),
            P::Channel::default(),
        );
    }
    let alpha_f = alpha.to_float();
    P::from_channels(
        alpha,
        P::Channel::from_float(red.to_float() * 255.0 / alpha_f),
        P::Channel::from_float(green.to_float() * 255.0 / alpha_f),
        P::Channel::from_float(blue.to_float() * 255.0 / alpha_f),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_flags_and_argb_color_ref_order() {
        assert_eq!(get_bpp_from_format(FxdibFormat::Bgra), 32);
        assert!(get_is_alpha_from_format(FxdibFormat::Bgra));
        assert!(get_is_mask_from_format(FxdibFormat::EightBppMask));
        let argb = 0x8040_2010;
        assert_eq!(
            argb_to_bgra_struct(argb),
            FxBgraStruct {
                blue: 0x10,
                green: 0x20,
                red: 0x40,
                alpha: 0x80
            }
        );
        assert_eq!(argb_to_color_ref(argb), 0x102040);
        assert_eq!(alpha_and_color_ref_to_argb(0x80, 0x102040), argb);
        assert_eq!(argb_get_dib(&[0x10, 0x20, 0x40, 0x80]), argb);
    }

    #[test]
    fn premultiply_and_unpremultiply_cover_u8_and_float() {
        let pixel = FxAbgrStruct {
            alpha: 128u8,
            blue: 100,
            green: 50,
            red: 20,
        };
        assert_eq!(
            pre_multiply_color(pixel),
            FxAbgrStruct {
                alpha: 128,
                blue: 50,
                green: 25,
                red: 10
            }
        );
        let clear = FxRgbaStruct {
            red: 10.0f32,
            green: 20.0,
            blue: 30.0,
            alpha: 0.0,
        };
        assert_eq!(
            un_pre_multiply_color(clear),
            FxRgbaStruct {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
                alpha: 0.0
            }
        );
        let opaque = FxRgbaStruct {
            red: 10.0,
            green: 20.0,
            blue: 30.0,
            alpha: 255.0,
        };
        assert_eq!(pre_multiply_color(opaque), opaque);
    }
}

#[cfg(test)]
#[path = "fx_dib_unittest.rs"]
mod upstream_tests;
