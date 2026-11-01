// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! RGB channel reversal and encoder registration from
//! `core/fxcodec/fx_codec.{h,cpp}`. Encoder registration is empty for the
//! supported non-Windows AGG build, as in PDFium.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u16)]
pub enum ResolutionUnit {
    #[default]
    None = 0,
    Inch = 1,
    Centimeter = 2,
    Meter = 3,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CfxDibAttribute {
    pub x_dpi: i32,
    pub y_dpi: i32,
    pub dpi_unit: ResolutionUnit,
}
impl Default for CfxDibAttribute {
    fn default() -> Self {
        Self {
            x_dpi: -1,
            y_dpi: -1,
            dpi_unit: ResolutionUnit::None,
        }
    }
}

pub fn reverse_rgb(dest: &mut [u8], src: &[u8], pixels: usize) {
    let count = pixels.checked_mul(3).expect("RGB row length overflow");
    assert!(dest.len() >= count && src.len() >= count);
    for (target, source) in dest[..count]
        .chunks_exact_mut(3)
        .zip(src[..count].chunks_exact(3))
    {
        target.copy_from_slice(&[source[2], source[1], source[0]]);
    }
}

pub fn reverse_rgb_in_place(pixels: &mut [u8], count: usize) {
    let byte_count = count.checked_mul(3).expect("RGB row length overflow");
    assert!(pixels.len() >= byte_count);
    for pixel in pixels[..byte_count].chunks_exact_mut(3) {
        pixel.swap(0, 2);
    }
}

pub fn register_encoders() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverses_only_requested_rgb_pixels() {
        let src = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut dest = [0; 9];
        reverse_rgb(&mut dest, &src, 2);
        assert_eq!(dest, [3, 2, 1, 6, 5, 4, 0, 0, 0]);
        let mut pixels = src;
        reverse_rgb_in_place(&mut pixels, 2);
        assert_eq!(pixels, [3, 2, 1, 6, 5, 4, 7, 8, 9]);
    }
}
