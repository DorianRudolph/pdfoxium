// Copyright 2025 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fpdfapi/page/jpx_decode_conversion.{h,cpp}`.

use super::cpdf_colorspace::{get_stock_cs_for_name, ColorSpaceFamily, ColorSpaceRef};
use crate::fxcodec::jpx::{JpxColorSpace, JpxImageInfo};
use crate::fxge::dib::fx_dib::FxdibFormat;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum JpxDecodeAction {
    DoNothing,
    UseGray,
    UseIndexed,
    UseRgb,
    UseCmyk,
    ConvertArgbToRgb,
}

fn matches_or_unknown(actual: JpxColorSpace, expected: JpxColorSpace) -> bool {
    actual == expected || matches!(actual, JpxColorSpace::Unknown | JpxColorSpace::Unspecified)
}

fn action_from_color_spaces(info: JpxImageInfo, space: &ColorSpaceRef) -> Option<JpxDecodeAction> {
    for (name, expected, action) in [
        (
            b"DeviceGray".as_slice(),
            JpxColorSpace::Gray,
            JpxDecodeAction::UseGray,
        ),
        (b"DeviceRGB", JpxColorSpace::SRgb, JpxDecodeAction::UseRgb),
        (b"DeviceCMYK", JpxColorSpace::Cmyk, JpxDecodeAction::UseCmyk),
    ] {
        if Rc::ptr_eq(space, &get_stock_cs_for_name(name).unwrap()) {
            if !matches_or_unknown(info.colorspace, expected) {
                return None;
            }
            return Some(
                if matches!(action, JpxDecodeAction::UseRgb) && info.channels > 3 {
                    JpxDecodeAction::ConvertArgbToRgb
                } else {
                    action
                },
            );
        }
    }
    if space.component_count() == 3 && info.channels == 4 && info.colorspace == JpxColorSpace::SRgb
    {
        return Some(JpxDecodeAction::ConvertArgbToRgb);
    }
    if space.family() == ColorSpaceFamily::Indexed && space.component_count() == 1 {
        return Some(JpxDecodeAction::UseIndexed);
    }
    Some(JpxDecodeAction::DoNothing)
}

fn action_from_image(info: JpxImageInfo) -> JpxDecodeAction {
    match info.colorspace {
        JpxColorSpace::Unknown | JpxColorSpace::Unspecified => {
            if info.channels == 3 {
                JpxDecodeAction::UseRgb
            } else {
                JpxDecodeAction::DoNothing
            }
        }
        JpxColorSpace::Sycc | JpxColorSpace::Eycc => JpxDecodeAction::DoNothing,
        JpxColorSpace::SRgb => {
            if info.channels > 3 {
                JpxDecodeAction::ConvertArgbToRgb
            } else {
                JpxDecodeAction::UseRgb
            }
        }
        JpxColorSpace::Gray => JpxDecodeAction::UseGray,
        JpxColorSpace::Cmyk => JpxDecodeAction::UseCmyk,
    }
}

fn component_count(info: JpxImageInfo) -> u32 {
    match info.colorspace {
        JpxColorSpace::Unknown | JpxColorSpace::Unspecified => info.channels,
        JpxColorSpace::Gray => 1,
        JpxColorSpace::SRgb | JpxColorSpace::Sycc | JpxColorSpace::Eycc => 3,
        JpxColorSpace::Cmyk => 4,
    }
}

pub struct JpxDecodeConversion {
    swap_rgb: bool,
    convert_argb_to_rgb: bool,
    format: FxdibFormat,
    width: u32,
    // None means no change; Some(None) clears the PDF color space.
    override_colorspace: Option<Option<ColorSpaceRef>>,
    jpx_components_count: Option<u32>,
}

impl JpxDecodeConversion {
    pub fn create(info: JpxImageInfo, pdf_colorspace: Option<&ColorSpaceRef>) -> Option<Self> {
        let action = match pdf_colorspace {
            Some(space) => action_from_color_spaces(info, space)?,
            None => action_from_image(info),
        };
        let mut conversion = Self {
            swap_rgb: false,
            convert_argb_to_rgb: false,
            format: FxdibFormat::Invalid,
            width: 0,
            override_colorspace: None,
            jpx_components_count: None,
        };
        match action {
            JpxDecodeAction::DoNothing | JpxDecodeAction::UseIndexed => {}
            JpxDecodeAction::UseGray => {
                conversion.override_colorspace = Some(get_stock_cs_for_name(b"DeviceGray"))
            }
            JpxDecodeAction::UseCmyk => {
                conversion.override_colorspace = Some(get_stock_cs_for_name(b"DeviceCMYK"))
            }
            JpxDecodeAction::UseRgb => {
                if info.channels < 3 {
                    return None;
                }
                conversion.swap_rgb = true;
                conversion.override_colorspace = Some(None);
            }
            JpxDecodeAction::ConvertArgbToRgb => {
                if info.channels < 4 {
                    return None;
                }
                conversion.swap_rgb = true;
                conversion.convert_argb_to_rgb = true;
                conversion.override_colorspace = Some(None);
            }
        }
        if pdf_colorspace.is_none() {
            conversion.jpx_components_count = Some(component_count(info));
        }
        let format = match action {
            JpxDecodeAction::UseGray | JpxDecodeAction::UseIndexed => {
                Some(FxdibFormat::EightBppRgb)
            }
            JpxDecodeAction::UseRgb if info.channels == 3 => Some(FxdibFormat::Bgr),
            JpxDecodeAction::UseRgb if info.channels == 4 => Some(FxdibFormat::Bgrx),
            JpxDecodeAction::ConvertArgbToRgb => Some(FxdibFormat::Bgrx),
            _ => None,
        };
        if let Some(format) = format {
            conversion.format = format;
            conversion.width = info.width;
        } else {
            conversion.format = FxdibFormat::Bgr;
            conversion.width = info.width.checked_mul(info.channels)?.div_ceil(3);
        }
        Some(conversion)
    }
    pub fn override_colorspace(&self) -> Option<&Option<ColorSpaceRef>> {
        self.override_colorspace.as_ref()
    }
    pub fn jpx_components_count(&self) -> Option<u32> {
        self.jpx_components_count
    }
    pub fn swap_rgb(&self) -> bool {
        self.swap_rgb
    }
    pub fn convert_argb_to_rgb(&self) -> bool {
        self.convert_argb_to_rgb
    }
    pub fn format(&self) -> FxdibFormat {
        self.format
    }
    pub fn width(&self) -> u32 {
        self.width
    }
}
