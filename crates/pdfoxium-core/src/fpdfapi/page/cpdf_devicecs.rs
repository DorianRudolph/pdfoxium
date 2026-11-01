// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_devicecs.{h,cpp}`.

use super::cpdf_colorspace::{
    components_for_family, ColorSpaceFamily, CpdfColorSpace, CpdfColorSpaceData,
};
use crate::fxge::dib::cfx_cmyk_to_srgb::{
    adobe_cmyk_to_standard_rgb, adobe_cmyk_to_standard_rgb_f,
};
use crate::fxge::dib::fx_dib::FxRgbStruct;

pub struct CpdfDeviceCs {
    data: CpdfColorSpaceData,
}

impl CpdfDeviceCs {
    pub fn new(family: ColorSpaceFamily) -> Self {
        Self {
            data: CpdfColorSpaceData::new(family, components_for_family(family)),
        }
    }

    /// The in-place alias case of `fxcodec::ReverseRGB()` used by DeviceRGB.
    pub fn reverse_rgb_in_place(bytes: &mut [u8], pixels: usize) {
        crate::fxcodec::fx_codec::reverse_rgb_in_place(bytes, pixels);
    }
}

impl CpdfColorSpace for CpdfDeviceCs {
    fn data(&self) -> &CpdfColorSpaceData {
        &self.data
    }

    fn get_rgb(&self, values: &[f32]) -> Option<FxRgbStruct<f32>> {
        assert!(values.len() >= self.component_count() as usize);
        let rgb = match self.family() {
            ColorSpaceFamily::DeviceGray => {
                let gray = values[0].clamp(0.0, 1.0);
                FxRgbStruct {
                    red: gray,
                    green: gray,
                    blue: gray,
                }
            }
            ColorSpaceFamily::DeviceRgb => FxRgbStruct {
                red: values[0].clamp(0.0, 1.0),
                green: values[1].clamp(0.0, 1.0),
                blue: values[2].clamp(0.0, 1.0),
            },
            ColorSpaceFamily::DeviceCmyk => {
                if self.data.is_std_conversion_enabled() {
                    FxRgbStruct {
                        red: 1.0 - (values[0] + values[3]).min(1.0),
                        green: 1.0 - (values[1] + values[3]).min(1.0),
                        blue: 1.0 - (values[2] + values[3]).min(1.0),
                    }
                } else {
                    adobe_cmyk_to_standard_rgb_f(
                        values[0].clamp(0.0, 1.0),
                        values[1].clamp(0.0, 1.0),
                        values[2].clamp(0.0, 1.0),
                        values[3].clamp(0.0, 1.0),
                    )
                }
            }
            _ => unreachable!("device color space has a device family"),
        };
        Some(rgb)
    }

    fn translate_image_line(
        &self,
        dest: &mut [u8],
        src: &[u8],
        pixels: usize,
        _image_width: i32,
        _image_height: i32,
        trans_mask: bool,
    ) {
        let components = self.component_count() as usize;
        assert!(src.len() >= pixels * components && dest.len() >= pixels * 3);
        match self.family() {
            ColorSpaceFamily::DeviceGray => {
                assert!(!trans_mask);
                for (gray, pixel) in src.iter().zip(dest.chunks_exact_mut(3)).take(pixels) {
                    pixel.copy_from_slice(&[*gray, *gray, *gray]);
                }
            }
            ColorSpaceFamily::DeviceRgb => {
                assert!(!trans_mask);
                crate::fxcodec::fx_codec::reverse_rgb(dest, src, pixels);
            }
            ColorSpaceFamily::DeviceCmyk => {
                let std_conversion = self.data.is_std_conversion_enabled();
                for (cmyk, pixel) in src
                    .chunks_exact(4)
                    .zip(dest.chunks_exact_mut(3))
                    .take(pixels)
                {
                    let [c, m, y, k] = [cmyk[0], cmyk[1], cmyk[2], cmyk[3]];
                    if trans_mask {
                        let key = 255 - i32::from(k);
                        pixel[0] = (i32::from(255 - c) * key / 255) as u8;
                        pixel[1] = (i32::from(255 - m) * key / 255) as u8;
                        pixel[2] = (i32::from(255 - y) * key / 255) as u8;
                    } else if std_conversion {
                        pixel[0] = 255 - (u16::from(y) + u16::from(k)).min(255) as u8;
                        pixel[1] = 255 - (u16::from(m) + u16::from(k)).min(255) as u8;
                        pixel[2] = 255 - (u16::from(c) + u16::from(k)).min(255) as u8;
                    } else {
                        let rgb = adobe_cmyk_to_standard_rgb(c, m, y, k);
                        pixel.copy_from_slice(&[rgb.blue, rgb.green, rgb.red]);
                    }
                }
            }
            _ => unreachable!("device color space has a device family"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmyk_standard_conversion_is_reference_counted() {
        let cs = CpdfDeviceCs::new(ColorSpaceFamily::DeviceCmyk);
        let values = [0.25, 0.5, 0.75, 0.125];
        let adobe = cs.get_rgb(&values);
        cs.enable_std_conversion(true);
        cs.enable_std_conversion(true);
        assert_eq!(
            cs.get_rgb(&values),
            Some(FxRgbStruct {
                red: 0.625,
                green: 0.375,
                blue: 0.125
            })
        );
        cs.enable_std_conversion(false);
        assert_eq!(
            cs.get_rgb(&values),
            Some(FxRgbStruct {
                red: 0.625,
                green: 0.375,
                blue: 0.125
            })
        );
        cs.enable_std_conversion(false);
        assert_eq!(cs.get_rgb(&values), adobe);
    }

    #[test]
    fn image_lines_are_bgr_except_for_cmyk_transparency_masks() {
        let rgb = CpdfDeviceCs::new(ColorSpaceFamily::DeviceRgb);
        let mut out = [0; 6];
        rgb.translate_image_line(&mut out, &[10, 20, 30, 40, 50, 60], 2, 2, 1, false);
        assert_eq!(out, [30, 20, 10, 60, 50, 40]);
        CpdfDeviceCs::reverse_rgb_in_place(&mut out, 2);
        assert_eq!(out, [10, 20, 30, 40, 50, 60]);

        let cmyk = CpdfDeviceCs::new(ColorSpaceFamily::DeviceCmyk);
        cmyk.translate_image_line(&mut out, &[10, 20, 30, 40, 0, 0, 0, 255], 2, 2, 1, true);
        assert_eq!(out, [206, 198, 189, 0, 0, 0]);

        cmyk.enable_std_conversion(true);
        cmyk.translate_image_line(&mut out, &[255, 0, 0, 0, 0, 0, 255, 0], 2, 2, 1, false);
        assert_eq!(out, [255, 255, 0, 0, 255, 255]);
    }
}

#[cfg(test)]
#[path = "cpdf_devicecs_unittest.rs"]
mod upstream_tests;
