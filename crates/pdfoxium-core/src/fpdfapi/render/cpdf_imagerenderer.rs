// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Image color modes and geometry from `core/fpdfapi/render/cpdf_imagerenderer.{h,cpp}`.
//! The static render-status path drives image loading and patterned stencils.

use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxSize, FxRect};
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions;
use crate::fxge::cfx_path::CfxPath;
use crate::fxge::cfx_renderdevice::stretch_bit_mask_agg;
use crate::fxge::dib::cfx_dibbase::transform_to;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::cstretchengine::StretchSource;
use crate::fxge::dib::fx_dib::argb_encode;
use crate::fxge::dib::fx_dib::{get_bpp_from_format, get_is_mask_from_format, FxdibFormat};
use std::rc::Rc;

/// StartRenderDIBBase's overprint choice, after separate mask/pattern dispatch.
pub(crate) fn image_blend_type(
    image: &crate::fpdfapi::page::cpdf_imageobject::CpdfImageObject,
    page_data: &crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData,
    page_resources: Option<&crate::fpdfapi::parser::cpdf_object::ObjectRef>,
) -> crate::fxge::dib::fx_dib::BlendMode {
    use crate::fpdfapi::page::cpdf_colorspace::ColorSpaceFamily;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fxge::dib::fx_dib::BlendMode;
    let state = image.base().general_state();
    if state.get_fill_alpha() != 1.0
        || !state.has_ref()
        || !state.get_fill_op()
        || state.get_op_mode() != 0
        || state.get_blend_type() != BlendMode::Normal
        || state.get_stroke_alpha() != 1.0
    {
        return BlendMode::Normal;
    }
    let object = image.image().and_then(|image| {
        image
            .dict()
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_direct_object_for(b"ColorSpace")
    });
    let family = object
        .and_then(|object| page_data.get_color_space(&object, page_resources))
        .map(|space| space.family());
    if matches!(
        family,
        Some(
            ColorSpaceFamily::DeviceCmyk | ColorSpaceFamily::Separation | ColorSpaceFamily::DeviceN
        )
    ) {
        BlendMode::Darken
    } else {
        BlendMode::Normal
    }
}

/// Source-axis sampling demand, independent of the output bitmap or clip.
/// Image matrices map the unit square; its transformed edges are the required
/// pixel widths/heights even when rotated or sheared. Evaluate in f64 so ceil
/// does not lose a fraction just above an integer decode-scale boundary.
pub(crate) fn image_decode_size(matrix: CfxMatrix) -> CfxSize {
    let [a, b, c, d] = [matrix.a, matrix.b, matrix.c, matrix.d].map(f64::from);
    if ![a, b, c, d].into_iter().all(f64::is_finite) || a * d - b * c == 0.0 {
        return CfxSize::new(0, 0);
    }
    let width = a.hypot(b).ceil();
    let height = c.hypot(d).ceil();
    if !(1.0..=f64::from(i32::MAX)).contains(&width)
        || !(1.0..=f64::from(i32::MAX)).contains(&height)
    {
        return CfxSize::new(0, 0);
    }
    CfxSize::new(width as i32, height as i32)
}

pub(crate) fn get_unit_rect(matrix: CfxMatrix) -> Option<FxRect> {
    let rect = matrix.get_unit_rect().get_outer_rect();
    rect.valid().then_some(rect)
}

pub(crate) fn get_dimensions_from_unit_rect(
    matrix: CfxMatrix,
    rect: FxRect,
) -> Option<(i32, i32, i32, i32)> {
    const LIMIT: i32 = 256 * 1024 * 1024;
    let too_big = |value: i32| value.checked_abs().is_none_or(|value| value >= LIMIT);
    let mut width = rect.right.checked_sub(rect.left)?;
    let mut height = rect.bottom.checked_sub(rect.top)?;
    if too_big(width) || too_big(height) {
        return None;
    }
    if matrix.a < 0.0 {
        width = -width;
    }
    if matrix.d > 0.0 {
        height = -height;
    }
    let left = if width > 0 { rect.left } else { rect.right };
    let top = if height > 0 { rect.top } else { rect.bottom };
    if too_big(left) || too_big(top) {
        return None;
    }
    Some((left, top, width, height))
}

/// Returns the source's pending-work flag, not rendering success. Its alpha
/// path has no continuation mode to run when affine mask creation fails.
pub(crate) fn start_bitmap_alpha<S: StretchSource>(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    source: Rc<S>,
    alpha: f32,
    matrix: CfxMatrix,
) -> bool {
    let bitmap_alpha = (alpha * 255.0).round() as u32;
    // Alpha soft-mask forms and uncolored pattern cells render into an
    // alpha-only bitmap. That destination consumes the color's alpha, not
    // its RGB intensity. Native's opaque gray loses constant image opacity
    // here and makes overlapping images incorrectly opaque.
    let color = if driver.bitmap().format() == FxdibFormat::EightBppMask {
        argb_encode(bitmap_alpha, 255, 255, 255)
    } else {
        argb_encode(255, bitmap_alpha, bitmap_alpha, bitmap_alpha)
    };
    if !get_is_mask_from_format(source.format()) && source.format() != FxdibFormat::Bgra {
        let mut path = CfxPath::new();
        path.append_rect(0.0, 0.0, 1.0, 1.0);
        path.transform(matrix);
        crate::fxge::cfx_renderdevice::draw_path_agg(
            driver,
            &path,
            None,
            None,
            color,
            0,
            CfxFillRenderOptions::winding_options(),
        );
        return false;
    }
    if get_is_mask_from_format(source.format()) {
        return start_alpha_mask(driver, source, color, matrix);
    }
    let Some(mask) = crate::fxge::dib::cfx_dibbase::clone_alpha_mask(source.as_ref()) else {
        return false;
    };
    start_alpha_mask(driver, Rc::new(mask), color, matrix)
}

fn start_alpha_mask<S: StretchSource>(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    mask: Rc<S>,
    color: u32,
    matrix: CfxMatrix,
) -> bool {
    if matrix.b.abs() >= 0.5 || matrix.c.abs() >= 0.5 {
        let (mask, left, top) = transform_to(mask, matrix);
        let Some(mask) = mask else { return true };
        let mask = mask.borrow();
        driver.set_dibits_normal(
            &mask,
            color,
            FxRect::new(0, 0, mask.width(), mask.height()),
            left,
            top,
        );
        return false;
    }
    let Some(rect) = get_unit_rect(matrix) else {
        return false;
    };
    let Some((left, top, width, height)) = get_dimensions_from_unit_rect(matrix, rect) else {
        return false;
    };
    stretch_bit_mask_agg(
        driver,
        mask,
        left,
        top,
        width,
        height,
        color,
        Default::default(),
    );
    false
}

/// StartDIBBase's size policy and AGG continuation for a static bitmap render.
pub(crate) fn draw_dib_base<S: StretchSource>(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    source: Rc<S>,
    alpha: f32,
    mask_color: u32,
    matrix: CfxMatrix,
    options: crate::fxge::dib::fx_dib::FxdibResampleOptions,
) {
    draw_dib_base_with_blend(
        driver,
        source,
        alpha,
        mask_color,
        matrix,
        options,
        crate::fxge::dib::fx_dib::BlendMode::Normal,
    );
}

pub(crate) fn draw_dib_base_with_blend<S: StretchSource>(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    source: Rc<S>,
    alpha: f32,
    mask_color: u32,
    matrix: CfxMatrix,
    mut options: crate::fxge::dib::fx_dib::FxdibResampleOptions,
    blend_type: crate::fxge::dib::fx_dib::BlendMode,
) {
    let bpp = get_bpp_from_format(source.format());
    if bpp > 1 {
        let Some(size) = (bpp as usize / 8)
            .checked_mul(source.width() as usize)
            .and_then(|size| size.checked_mul(source.height() as usize))
        else {
            return;
        };
        if size > 60_000_000 && !options.halftone {
            options.interpolate_bilinear = true;
        }
    }
    let mut renderer =
        driver.start_dibits_with_blend(source, alpha, mask_color, matrix, options, blend_type);
    while renderer.continue_render(None) {}
}

/// CalculateDrawImage(): resample the mask in its own gray surface, then
/// remove the matte from already resampled color pixels.
pub(crate) fn calculate_draw_image<S: StretchSource>(
    pixels: &mut CfxDibitmap,
    source: Rc<S>,
    matrix: CfxMatrix,
    options: crate::fxge::dib::fx_dib::FxdibResampleOptions,
    matte_color: u32,
) -> Option<CfxDibitmap> {
    use crate::fxge::dib::fx_dib::{argb_b, argb_g, argb_r, FxdibFormat};
    let (width, height) = (pixels.width(), pixels.height());
    let mut mask = CfxDibitmap::create(width, height, FxdibFormat::EightBppRgb, 0)?;
    {
        let mut driver = CfxAggDeviceDriver::new(&mut mask, false, None);
        draw_dib_base(&mut driver, source, 1.0, 0xffffffff, matrix, options);
    }
    if matte_color != u32::MAX {
        let matte = [
            argb_b(matte_color),
            argb_g(matte_color),
            argb_r(matte_color),
        ];
        for row in 0..height {
            let coverage = mask.scanline(row)?;
            let pixels = pixels.scanline_mut(row)?;
            for (&alpha, pixel) in coverage[..width as usize]
                .iter()
                .zip(pixels.chunks_exact_mut(4))
            {
                if alpha == 0 {
                    continue;
                }
                for (value, &matte) in pixel[..3].iter_mut().zip(&matte) {
                    let original = (i32::from(*value) - i32::from(matte)) * 255 / i32::from(alpha)
                        + i32::from(matte);
                    *value = original.clamp(0, 255) as u8;
                }
            }
        }
    }
    assert!(mask.palette().is_empty());
    mask.convert_to_8bpp_mask();
    Some(mask)
}

/// DrawMaskedImage(), using separate image/mask surfaces and source matte order.
pub(crate) fn draw_masked_image<S: StretchSource, M: StretchSource>(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    source: Rc<S>,
    mask: Rc<M>,
    matrix: CfxMatrix,
    options: crate::fxge::dib::fx_dib::FxdibResampleOptions,
    matte_color: u32,
    alpha: f32,
) {
    use crate::fxge::dib::fx_dib::FxdibFormat;
    let Some(mut rect) = get_unit_rect(matrix) else {
        return;
    };
    rect.intersect(driver.clip_box());
    if rect.is_empty() {
        return;
    }
    let Some(mut pixels) = CfxDibitmap::create(rect.width(), rect.height(), FxdibFormat::Bgrx, 0)
    else {
        return;
    };
    let mut local_matrix = matrix;
    local_matrix.translate(-(rect.left as f32), -(rect.top as f32));
    {
        let mut target = CfxAggDeviceDriver::new(&mut pixels, false, None);
        target.clear(0xffffffff);
        draw_dib_base(&mut target, source, 1.0, 0, local_matrix, options);
    }
    let Some(mask) = calculate_draw_image(&mut pixels, mask, local_matrix, options, matte_color)
    else {
        return;
    };
    pixels.multiply_alpha_mask(&mask);
    pixels.multiply_alpha(alpha);
    driver.set_dibits_normal(
        &pixels,
        0,
        FxRect::new(0, 0, pixels.width(), pixels.height()),
        rect.left,
        rect.top,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    #[test]
    fn alpha_only_destinations_preserve_image_opacity_and_union_overlaps() {
        for format in [FxdibFormat::Bgr, FxdibFormat::Bgra, FxdibFormat::OneBppMask] {
            let mut source = CfxDibitmap::create(1, 1, format, 0).unwrap();
            source.clear(0xffff0000);
            let source = Rc::new(source);
            for opacity in [0.0, 0.5, 1.0] {
                let mut dest = CfxDibitmap::create(3, 2, FxdibFormat::EightBppMask, 0).unwrap();
                let mut driver = CfxAggDeviceDriver::new(&mut dest, false, None);
                for x in [0.0, 1.0] {
                    assert!(!start_bitmap_alpha(
                        &mut driver,
                        source.clone(),
                        opacity,
                        CfxMatrix::new(2.0, 0.0, 0.0, -2.0, x, 2.0),
                    ));
                }
                drop(driver);
                let alpha = (opacity * 255.0).round() as u8;
                let union = i32::from(alpha) * 2 - i32::from(alpha).pow(2) / 255;
                for y in 0..2 {
                    let row = dest.scanline(y).unwrap();
                    assert_eq!(row[0], alpha, "source={format:?}");
                    // Path spans and bitmap masks quantize intermediate
                    // coverage in different orders; allow one 8-bit step.
                    assert!((i32::from(row[1]) - union).abs() <= 1, "source={format:?}");
                    assert_eq!(row[2], alpha, "source={format:?}");
                }
            }
        }
    }

    #[test]
    fn alpha_mode_draws_gray_intensity_and_preserves_mask_coverage() {
        for opaque in [true, false] {
            let format = if opaque {
                FxdibFormat::Bgr
            } else {
                FxdibFormat::Bgra
            };
            let mut source = CfxDibitmap::create(1, 1, format, 0).unwrap();
            source.clear(0x80ff0000);
            let mut dest = CfxDibitmap::create(2, 2, FxdibFormat::Bgra, 0).unwrap();
            let mut driver = CfxAggDeviceDriver::new(&mut dest, false, None);
            assert!(!start_bitmap_alpha(
                &mut driver,
                Rc::new(source),
                0.5,
                CfxMatrix::new(2.0, 0.0, 0.0, -2.0, 0.0, 2.0)
            ));
            drop(driver);
            assert_eq!(
                &dest.buffer()[..4],
                &[128, 128, 128, if opaque { 255 } else { 128 }]
            );
        }
    }

    #[test]
    fn alpha_dimensions_preserve_flips_and_reject_source_limit() {
        let matrix = CfxMatrix::new(-4.0, 0.0, 0.0, 3.0, 7.0, 8.0);
        let rect = get_unit_rect(matrix).unwrap();
        assert_eq!(
            get_dimensions_from_unit_rect(matrix, rect),
            Some((7, 11, -4, -3))
        );
        assert!(get_dimensions_from_unit_rect(
            CfxMatrix::default(),
            FxRect::new(0, 0, 256 * 1024 * 1024, 1)
        )
        .is_none());
    }
    #[test]
    fn differently_sized_masks_apply_matte_after_resampling_and_then_object_alpha() {
        use crate::fxge::dib::fx_dib::FxdibResampleOptions;
        let mut source = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
        source
            .buffer_mut()
            .copy_from_slice(&[127, 127, 255, 255, 255, 127, 127, 255]);
        let mut mask = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        mask.clear(0xff808080);
        let mut dest = CfxDibitmap::create(8, 4, FxdibFormat::Bgra, 0).unwrap();
        {
            let mut driver = CfxAggDeviceDriver::new(&mut dest, false, None);
            draw_masked_image(
                &mut driver,
                Rc::new(source),
                Rc::new(mask),
                CfxMatrix::new(8.0, 0.0, 0.0, -4.0, 0.0, 4.0),
                FxdibResampleOptions {
                    no_smoothing: true,
                    ..Default::default()
                },
                0x00ffffff,
                0.5,
            );
        }
        let row = dest.scanline(2).unwrap();
        assert_eq!(&row[4..8], &[0, 0, 255, 63]);
        assert_eq!(&row[24..28], &[255, 0, 0, 63]);
    }
}

#[cfg(test)]
#[path = "cpdf_imagerenderer_decode_size_tests.rs"]
mod decode_size_tests;
