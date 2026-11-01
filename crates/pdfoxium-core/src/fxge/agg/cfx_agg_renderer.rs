// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `CFX_AggRenderer` from `core/fxge/agg/cfx_agg_devicedriver.cpp`.

use super::cfx_agg_cliprgn::CfxAggClipRgn;
use crate::agg::agg_render_scanlines::ScanlineRenderer;
use crate::agg::agg_scanline_u::ScanlineU8;
use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxge::dib::cfx_dibitmap::{CfxDibitmap, CfxDibitmapData};
use crate::fxge::dib::fx_dib::{
    alpha_merge, argb_a, argb_b, argb_g, argb_r, argb_to_bgr_order_dib, get_bpp_from_format,
    get_is_alpha_from_format, get_is_mask_from_format, rgb_to_gray, FxdibFormat,
};

#[derive(Clone, Copy)]
enum ColorData {
    Gray(i32),
    Bgr([i32; 3]),
}

pub struct CfxAggRenderer<'a, 'storage> {
    device: &'a mut CfxDibitmapData<'storage>,
    backdrop: Option<&'a CfxDibitmap>,
    clip_region: Option<&'a CfxAggClipRgn>,
    alpha: i32,
    color: u32,
    color_data: ColorData,
    full_cover: bool,
    rgb_byte_order: bool,
    clip_box: FxRect,
}

impl<'a, 'storage> CfxAggRenderer<'a, 'storage> {
    pub fn new(
        device: &'a mut CfxDibitmapData<'storage>,
        backdrop: Option<&'a CfxDibitmap>,
        clip_region: Option<&'a CfxAggClipRgn>,
        color: u32,
        full_cover: bool,
        rgb_byte_order: bool,
    ) -> Self {
        assert_ne!(get_bpp_from_format(device.format()), 1);
        let clip_box = clip_region.map_or_else(
            || FxRect::new(0, 0, device.width(), device.height()),
            CfxAggClipRgn::box_rect,
        );
        let color_data = if get_bpp_from_format(device.format()) == 8 {
            assert!(!rgb_byte_order);
            if get_is_mask_from_format(device.format()) {
                ColorData::Gray(255)
            } else {
                ColorData::Gray(rgb_to_gray(
                    i32::from(argb_r(color)),
                    i32::from(argb_g(color)),
                    i32::from(argb_b(color)),
                ))
            }
        } else {
            ColorData::Bgr([
                i32::from(argb_b(color)),
                i32::from(argb_g(color)),
                i32::from(argb_r(color)),
            ])
        };
        Self {
            device,
            backdrop,
            clip_region,
            alpha: i32::from(argb_a(color)),
            color: if rgb_byte_order {
                argb_to_bgr_order_dib(color)
            } else {
                color
            },
            color_data,
            full_cover,
            rgb_byte_order,
            clip_box,
        }
    }

    fn source_alpha(alpha: i32, cover: u8, clip: u8) -> i32 {
        alpha * i32::from(cover) * i32::from(clip) / 255 / 255
    }

    fn src_alpha(alpha: i32, clip: u8) -> i32 {
        alpha * i32::from(clip) / 255
    }

    fn merge(dest: u8, source: i32, alpha: i32) -> u8 {
        alpha_merge(i32::from(dest), source, alpha) as u8
    }

    // CompositeSpanARGB without a backdrop or clip mask. As in upstream,
    // GetSourceAlpha omits the second /255 when there is no clip. Fix the
    // destination format and span bounds outside the pixel loop so opaque
    // pixels use a four-byte store rather than a runtime-sized memcpy.
    fn composite_span_argb_no_clip<const RGB: bool, const FULL: bool>(
        dest: &mut [u8],
        covers: &[u8],
        alpha: i32,
        color: u32,
        bgr: [i32; 3],
    ) {
        #[allow(unused_mut)]
        let mut processed = 0;
        #[cfg(all(feature = "simd", target_endian = "little"))]
        if !FULL && covers.len().min(dest.len() / 4) >= if alpha == 255 { 128 } else { 16 } {
            // Opaque interiors need only a scalar color store. For short
            // spans, SIMD setup and blending cost more than those stores;
            // translucent spans still benefit from eight-lane blending.
            processed = covers.len().min(dest.len() / 4) / 8 * 8;
            super::span_simd::composite(
                &mut dest[..processed * 4],
                &covers[..processed],
                alpha as u8,
                color,
            );
        }
        for (pixel, &cover) in dest[processed * 4..]
            .chunks_exact_mut(4)
            .zip(&covers[processed..])
        {
            let pixel: &mut [u8; 4] = pixel.try_into().unwrap();
            let src_alpha = if FULL {
                alpha
            } else {
                alpha * i32::from(cover) / 255
            };
            if src_alpha == 0 {
                continue;
            }
            if src_alpha == 255 {
                *pixel = color.to_le_bytes();
                continue;
            }
            if !RGB && pixel[3] == 0 {
                *pixel = [bgr[0] as u8, bgr[1] as u8, bgr[2] as u8, src_alpha as u8];
                continue;
            }
            let ratio = if pixel[3] == 255 {
                // AlphaUnion(255, src_alpha) is exactly 255, so the
                // source ratio is src_alpha without a variable division.
                src_alpha
            } else {
                let back_alpha = i32::from(pixel[3]);
                let out_alpha = back_alpha + src_alpha - back_alpha * src_alpha / 255;
                pixel[3] = out_alpha as u8;
                src_alpha * 255 / out_alpha
            };
            for channel in 0..3 {
                let source = if RGB { bgr[2 - channel] } else { bgr[channel] };
                pixel[channel] = Self::merge(pixel[channel], source, ratio);
            }
        }
    }

    #[inline(always)]
    fn composite_span_argb<const RGB: bool, const FULL: bool>(
        dest: &mut [u8],
        covers: &[u8],
        solid: bool,
        clip: Option<&[u8]>,
        alpha: i32,
        color: u32,
        bgr: [i32; 3],
    ) {
        if let Some(clip) = clip {
            Self::composite_span_argb_clipped::<RGB, FULL>(
                dest, covers, solid, clip, alpha, color, bgr,
            );
        } else if solid {
            Self::composite_span_argb_solid::<RGB, FULL>(dest, covers[0], alpha, color, bgr);
        } else {
            Self::composite_span_argb_no_clip::<RGB, FULL>(dest, covers, alpha, color, bgr);
        }
    }

    // As in CompositeSpanARGB, choose format/byte order and validate the span
    // once, rather than repeating the general renderer's checked indexing and
    // format dispatch for every clipped pixel. Keep both /255 operations in
    // their original order; quantizing cover*clip first can change the result.
    fn composite_span_argb_clipped<const RGB: bool, const FULL: bool>(
        dest: &mut [u8],
        covers: &[u8],
        solid: bool,
        clip: &[u8],
        alpha: i32,
        color: u32,
        bgr: [i32; 3],
    ) {
        let blend = |pixel: &mut [u8; 4], cover: u8, clip: u8| {
            let src_alpha = if FULL {
                Self::src_alpha(alpha, clip)
            } else {
                Self::source_alpha(alpha, cover, clip)
            };
            if src_alpha == 0 {
                return;
            }
            if src_alpha == 255 {
                *pixel = color.to_le_bytes();
                return;
            }
            if !RGB && pixel[3] == 0 {
                *pixel = [bgr[0] as u8, bgr[1] as u8, bgr[2] as u8, src_alpha as u8];
                return;
            }
            let ratio = if pixel[3] == 255 {
                src_alpha
            } else {
                let back_alpha = i32::from(pixel[3]);
                let out_alpha = back_alpha + src_alpha - back_alpha * src_alpha / 255;
                pixel[3] = out_alpha as u8;
                src_alpha * 255 / out_alpha
            };
            for channel in 0..3 {
                let source = if RGB { bgr[2 - channel] } else { bgr[channel] };
                pixel[channel] = Self::merge(pixel[channel], source, ratio);
            }
        };
        let pixels = dest.as_chunks_mut::<4>().0;
        if solid || FULL {
            let cover = if FULL { 255 } else { covers[0] };
            for (pixel, &clip) in pixels.iter_mut().zip(clip) {
                blend(pixel, cover, clip);
            }
        } else {
            for ((pixel, &cover), &clip) in pixels.iter_mut().zip(covers).zip(clip) {
                blend(pixel, cover, clip);
            }
        }
    }

    fn composite_span_argb_solid<const RGB: bool, const FULL: bool>(
        dest: &mut [u8],
        cover: u8,
        alpha: i32,
        color: u32,
        bgr: [i32; 3],
    ) {
        let src_alpha = if FULL {
            alpha
        } else {
            alpha * i32::from(cover) / 255
        };
        if src_alpha == 0 {
            return;
        }
        if src_alpha == 255 {
            for pixel in dest.as_chunks_mut::<4>().0 {
                *pixel = color.to_le_bytes();
            }
            return;
        }
        #[allow(unused_mut)]
        let mut processed = 0;
        #[cfg(all(feature = "simd", target_endian = "little"))]
        if dest.len() >= 16 * 4 {
            processed = dest.len() / 32 * 32;
            super::span_simd::composite_solid(&mut dest[..processed], src_alpha as u8, color);
        }
        for pixel in dest[processed..].as_chunks_mut::<4>().0 {
            let back = i32::from(pixel[3]);
            let output = back + src_alpha - back * src_alpha / 255;
            let ratio = if back == 255 {
                src_alpha
            } else {
                src_alpha * 255 / output
            };
            for channel in 0..3 {
                let source = if RGB { bgr[2 - channel] } else { bgr[channel] };
                pixel[channel] = Self::merge(pixel[channel], source, ratio);
            }
            pixel[3] = output as u8;
        }
    }

    fn composite_pixel(
        dest: &mut [u8],
        backdrop: Option<&[u8]>,
        cover: u8,
        clip: u8,
        alpha: i32,
        color: u32,
        color_data: ColorData,
        full_cover: bool,
        rgb_byte_order: bool,
        dest_alpha: bool,
        format: FxdibFormat,
    ) {
        let bpp = dest.len();
        let gray = match color_data {
            ColorData::Gray(gray) => gray,
            ColorData::Bgr(_) => 0,
        };
        let bgr = match color_data {
            ColorData::Bgr(bgr) => bgr,
            ColorData::Gray(_) => [0, 0, 0],
        };

        if let Some(backdrop) = backdrop {
            let src_alpha = Self::src_alpha(alpha, clip);
            if rgb_byte_order {
                if bpp == 4 && dest_alpha {
                    let out_alpha = i32::from(backdrop[3]) + src_alpha
                        - i32::from(backdrop[3]) * src_alpha / 255;
                    dest[3] = out_alpha as u8;
                    if out_alpha == 0 {
                        return;
                    }
                    let ratio = src_alpha * 255 / out_alpha;
                    for channel in 0..3 {
                        let merged = Self::merge(backdrop[channel], bgr[2 - channel], ratio);
                        dest[channel] = if full_cover {
                            merged
                        } else {
                            Self::merge(dest[channel], i32::from(merged), i32::from(cover))
                        };
                    }
                    return;
                }
                if bpp == 3 || bpp == 4 {
                    for channel in 0..3 {
                        let merged = Self::merge(backdrop[channel], bgr[2 - channel], src_alpha);
                        dest[channel] =
                            Self::merge(dest[channel], i32::from(merged), i32::from(cover));
                    }
                }
                return;
            }
            if bpp == 4 && dest_alpha {
                let covered = src_alpha * i32::from(cover) / 255;
                if covered == 0 {
                    return;
                }
                if cover == 255 || dest[3] == 0 {
                    dest[3] = covered as u8;
                    dest[..3].copy_from_slice(&bgr.map(|value| value as u8));
                    return;
                }
                dest[3] = Self::merge(dest[3], src_alpha, i32::from(cover));
                for channel in 0..3 {
                    dest[channel] = Self::merge(dest[channel], bgr[channel], i32::from(cover));
                }
                return;
            }
            if bpp == 3 || bpp == 4 {
                for channel in 0..3 {
                    let merged = Self::merge(backdrop[channel], bgr[channel], src_alpha);
                    dest[channel] = if full_cover {
                        merged
                    } else {
                        Self::merge(dest[channel], i32::from(merged), i32::from(cover))
                    };
                }
                return;
            }
            let merged = Self::merge(backdrop[0], gray, src_alpha);
            dest[0] = if full_cover {
                merged
            } else {
                Self::merge(dest[0], i32::from(merged), i32::from(cover))
            };
            return;
        }

        match format {
            FxdibFormat::EightBppMask | FxdibFormat::EightBppRgb => {
                let src_alpha = Self::source_alpha(alpha, cover, clip);
                if src_alpha == 255 {
                    dest[0] = gray as u8;
                } else if src_alpha != 0 {
                    dest[0] = Self::merge(dest[0], gray, src_alpha);
                }
            }
            FxdibFormat::Bgra => {
                let src_alpha = if full_cover {
                    Self::src_alpha(alpha, clip)
                } else {
                    Self::source_alpha(alpha, cover, clip)
                };
                if src_alpha == 0 {
                    return;
                }
                if src_alpha == 255 {
                    dest.copy_from_slice(&color.to_le_bytes());
                    return;
                }
                if !rgb_byte_order && dest[3] == 0 {
                    dest[3] = src_alpha as u8;
                    dest[..3].copy_from_slice(&bgr.map(|value| value as u8));
                    return;
                }
                let ratio = if dest[3] == 255 {
                    // AlphaUnion(255, src_alpha) is 255. Masked spans use
                    // this general path; avoid a variable divide for the
                    // usual opaque page, as the unclipped span kernel does.
                    src_alpha
                } else {
                    let out_alpha =
                        i32::from(dest[3]) + src_alpha - i32::from(dest[3]) * src_alpha / 255;
                    dest[3] = out_alpha as u8;
                    src_alpha * 255 / out_alpha
                };
                for channel in 0..3 {
                    let source = if rgb_byte_order {
                        bgr[2 - channel]
                    } else {
                        bgr[channel]
                    };
                    dest[channel] = Self::merge(dest[channel], source, ratio);
                }
            }
            FxdibFormat::Bgr | FxdibFormat::Bgrx => {
                let src_alpha = if rgb_byte_order || !full_cover {
                    Self::source_alpha(alpha, cover, clip)
                } else {
                    Self::src_alpha(alpha, clip)
                };
                if src_alpha == 0 {
                    return;
                }
                if src_alpha == 255 && bpp == 4 {
                    dest.copy_from_slice(&color.to_le_bytes());
                    return;
                }
                for channel in 0..3 {
                    let source = if rgb_byte_order {
                        bgr[2 - channel]
                    } else {
                        bgr[channel]
                    };
                    dest[channel] = if src_alpha == 255 {
                        source as u8
                    } else {
                        Self::merge(dest[channel], source, src_alpha)
                    };
                }
            }
            _ => {}
        }
    }
}

impl ScanlineRenderer for CfxAggRenderer<'_, '_> {
    fn supports_solid_spans(&self) -> bool {
        // PDFium's full-cover gray backdrop branch repeatedly writes the
        // first pixel of each span. Splitting its spans would change pixels.
        !(self.full_cover
            && self.backdrop.is_some()
            && get_bpp_from_format(self.device.format()) == 8)
    }
    fn prepare(&mut self, _max_span_len: u32) {}

    fn render(&mut self, scanline: &ScanlineU8) {
        let y = scanline.y();
        if y < self.clip_box.top || y >= self.clip_box.bottom {
            return;
        }
        let format = self.device.format();
        let bpp = (get_bpp_from_format(format) / 8) as usize;
        if bpp == 0 {
            return;
        }
        let dest_alpha = get_is_alpha_from_format(format) || get_is_mask_from_format(format);
        let Some(dest_row) = self.device.scanline_mut(y) else {
            return;
        };
        let backdrop_row = self.backdrop.and_then(|bitmap| bitmap.scanline(y));
        if self.backdrop.is_some() && backdrop_row.is_none() {
            return;
        }
        let clip_mask = self.clip_region.and_then(CfxAggClipRgn::mask);
        let clip_row = clip_mask.and_then(|mask| mask.scanline(y - self.clip_box.top));
        if clip_mask.is_some() && clip_row.is_none() {
            return;
        }
        for span in scanline.spans() {
            if span.len <= 0 {
                break;
            }
            let span_x = i32::from(span.x);
            let start = (self.clip_box.left - span_x).max(0);
            let end = (self.clip_box.right - span_x).min(i32::from(span.len));
            if format == FxdibFormat::Bgra && backdrop_row.is_none() && start < end {
                let offsets = usize::try_from(span_x + start)
                    .ok()
                    .and_then(|x| x.checked_mul(4))
                    .zip(
                        usize::try_from(span_x + end)
                            .ok()
                            .and_then(|x| x.checked_mul(4)),
                    );
                if let (Some((left, right)), Some(covers), ColorData::Bgr(bgr)) = (
                    offsets,
                    if span.solid {
                        Some(span.covers)
                    } else {
                        span.covers.get(start as usize..end as usize)
                    },
                    self.color_data,
                ) {
                    let clip = clip_row.and_then(|row| {
                        let left = usize::try_from(span_x + start - self.clip_box.left).ok()?;
                        let right = usize::try_from(span_x + end - self.clip_box.left).ok()?;
                        row.get(left..right)
                    });
                    if let Some(dest) = dest_row
                        .get_mut(left..right)
                        .filter(|_| clip_row.is_none() || clip.is_some())
                    {
                        match (self.rgb_byte_order, self.full_cover) {
                            (false, false) => Self::composite_span_argb::<false, false>(
                                dest, covers, span.solid, clip, self.alpha, self.color, bgr,
                            ),
                            (false, true) => Self::composite_span_argb::<false, true>(
                                dest, covers, span.solid, clip, self.alpha, self.color, bgr,
                            ),
                            (true, false) => Self::composite_span_argb::<true, false>(
                                dest, covers, span.solid, clip, self.alpha, self.color, bgr,
                            ),
                            (true, true) => Self::composite_span_argb::<true, true>(
                                dest, covers, span.solid, clip, self.alpha, self.color, bgr,
                            ),
                        }
                        continue;
                    }
                }
                // Preserve per-pixel handling for unusual out-of-row spans.
            }
            for col in start..end {
                let x = span_x + col;
                let dest_x = if backdrop_row.is_some() && bpp == 1 && self.full_cover {
                    // PDFium's gray backdrop branch omits the destination
                    // pointer increment in its full-cover loop.
                    span_x + start
                } else {
                    x
                };
                let Some(pixel_offset) = usize::try_from(dest_x)
                    .ok()
                    .and_then(|x| x.checked_mul(bpp))
                else {
                    continue;
                };
                let Some(end_offset) = pixel_offset.checked_add(bpp) else {
                    continue;
                };
                let Some(dest) = dest_row.get_mut(pixel_offset..end_offset) else {
                    continue;
                };
                let backdrop = usize::try_from(x)
                    .ok()
                    .and_then(|x| x.checked_mul(bpp))
                    .and_then(|offset| offset.checked_add(bpp).map(|end| (offset, end)))
                    .and_then(|(offset, end)| backdrop_row.and_then(|row| row.get(offset..end)));
                if backdrop_row.is_some() && backdrop.is_none() {
                    continue;
                }
                let cover = span.cover_at(col as usize);
                let clip = if let Some(row) = clip_row {
                    let Some(&cover) = row.get((x - self.clip_box.left) as usize) else {
                        continue;
                    };
                    cover
                } else {
                    255
                };
                Self::composite_pixel(
                    dest,
                    backdrop,
                    cover,
                    clip,
                    self.alpha,
                    self.color,
                    self.color_data,
                    self.full_cover,
                    self.rgb_byte_order,
                    dest_alpha,
                    format,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agg::agg_path_storage::PathStorage;
    use crate::agg::agg_rasterizer_scanline_aa::RasterizerScanlineAa;
    use crate::agg::agg_render_scanlines::render_scanlines;
    use std::rc::Rc;

    fn rectangle() -> PathStorage {
        let mut path = PathStorage::new();
        path.move_to(0.0, 0.0);
        path.line_to(2.0, 0.0);
        path.line_to(2.0, 2.0);
        path.line_to(0.0, 2.0);
        path.end_poly();
        path
    }

    fn check_argb_span<const RGB: bool, const FULL: bool>() {
        let covers: Vec<u8> = (0..=255).collect();
        for back_alpha in [0, 1, 127, 128, 254, 255] {
            for alpha in 0..=255 {
                let color = ((alpha as u32) << 24) | 0x00cb5b1f;
                let color = if RGB {
                    argb_to_bgr_order_dib(color)
                } else {
                    color
                };
                let initial = [29, 113, 227, back_alpha].repeat(covers.len());
                let mut expected = initial.clone();
                for (pixel, &cover) in expected.chunks_exact_mut(4).zip(&covers) {
                    CfxAggRenderer::composite_pixel(
                        pixel,
                        None,
                        cover,
                        255,
                        alpha,
                        color,
                        ColorData::Bgr([31, 91, 203]),
                        FULL,
                        RGB,
                        true,
                        FxdibFormat::Bgra,
                    );
                }
                let mut actual = initial;
                CfxAggRenderer::composite_span_argb_no_clip::<RGB, FULL>(
                    &mut actual,
                    &covers,
                    alpha,
                    color,
                    [31, 91, 203],
                );
                assert_eq!(
                    actual, expected,
                    "RGB={RGB} FULL={FULL} alpha={alpha} back={back_alpha}"
                );
            }
        }
    }

    #[test]
    fn argb_span_preserves_all_alpha_and_coverage_rounding() {
        check_argb_span::<false, false>();
        check_argb_span::<false, true>();
        check_argb_span::<true, false>();
        check_argb_span::<true, true>();
    }

    #[test]
    fn argb_span_mixed_alpha_unaligned_rows_and_tails_match_scalar() {
        for rgb in [false, true] {
            for length in [7, 15, 16, 17, 31, 32, 63, 64, 127, 128, 129, 263] {
                for alpha in 0..=255u32 {
                    let mut color = (alpha << 24) | 0x00cb5b1f;
                    if rgb {
                        color = argb_to_bgr_order_dib(color);
                    }
                    let covers = vec![255; length];
                    let mut actual = vec![0xa7; length * 4 + 8];
                    for (index, pixel) in actual[1..1 + length * 4].chunks_exact_mut(4).enumerate()
                    {
                        pixel.copy_from_slice(&[
                            index as u8,
                            (index * 31) as u8,
                            (index * 17) as u8,
                            index as u8,
                        ]);
                    }
                    let mut expected = actual.clone();
                    for (pixel, &cover) in
                        expected[1..1 + length * 4].chunks_exact_mut(4).zip(&covers)
                    {
                        CfxAggRenderer::composite_pixel(
                            pixel,
                            None,
                            cover,
                            255,
                            alpha as i32,
                            color,
                            ColorData::Bgr([31, 91, 203]),
                            false,
                            rgb,
                            true,
                            FxdibFormat::Bgra,
                        );
                    }
                    if rgb {
                        CfxAggRenderer::composite_span_argb_no_clip::<true, false>(
                            &mut actual[1..1 + length * 4],
                            &covers,
                            alpha as i32,
                            color,
                            [31, 91, 203],
                        );
                    } else {
                        CfxAggRenderer::composite_span_argb_no_clip::<false, false>(
                            &mut actual[1..1 + length * 4],
                            &covers,
                            alpha as i32,
                            color,
                            [31, 91, 203],
                        );
                    }
                    assert_eq!(actual, expected, "alpha={alpha} rgb={rgb} length={length}");
                }
            }
        }
    }

    #[test]
    fn solid_argb_spans_preserve_all_alpha_and_coverage_pairs() {
        fn check<const RGB: bool, const FULL: bool>() {
            for alpha in 0..=255u32 {
                let mut color = alpha << 24 | 0x00cb5b1f;
                if RGB {
                    color = argb_to_bgr_order_dib(color);
                }
                for cover in 0..=255u8 {
                    let mut actual = [0xa7; 17 * 4 + 2];
                    for (index, pixel) in actual[1..1 + 17 * 4].chunks_exact_mut(4).enumerate() {
                        pixel.copy_from_slice(&[
                            29,
                            113,
                            227,
                            [0, 1, 127, 128, 254, 255][index % 6],
                        ]);
                    }
                    let mut expected = actual;
                    for pixel in expected[1..1 + 17 * 4].chunks_exact_mut(4) {
                        CfxAggRenderer::composite_pixel(
                            pixel,
                            None,
                            cover,
                            255,
                            alpha as i32,
                            color,
                            ColorData::Bgr([31, 91, 203]),
                            FULL,
                            RGB,
                            true,
                            FxdibFormat::Bgra,
                        );
                    }
                    CfxAggRenderer::composite_span_argb_solid::<RGB, FULL>(
                        &mut actual[1..1 + 17 * 4],
                        cover,
                        alpha as i32,
                        color,
                        [31, 91, 203],
                    );
                    assert_eq!(
                        actual, expected,
                        "alpha={alpha} cover={cover} rgb={RGB} full={FULL}"
                    );
                }
            }
        }
        check::<false, false>();
        check::<true, false>();
        check::<false, true>();
        check::<true, true>();
    }

    #[test]
    fn clipped_argb_spans_match_scalar_for_mixed_destination_alpha() {
        fn check<const RGB: bool, const FULL: bool>() {
            let covers: Vec<u8> = (0..=255).collect();
            for alpha in [0, 1, 127, 128, 254, 255] {
                let mut color = (alpha as u32) << 24 | 0x00cb5b1f;
                if RGB {
                    color = argb_to_bgr_order_dib(color);
                }
                for shift in 0..=255usize {
                    let clip: Vec<u8> = (0..256).map(|i| (i + shift) as u8).collect();
                    for solid in [false, true] {
                        let mut actual = vec![0xa7; 256 * 4 + 2];
                        for (index, pixel) in actual[1..1025]
                            .as_chunks_mut::<4>()
                            .0
                            .iter_mut()
                            .enumerate()
                        {
                            *pixel = [
                                29,
                                113,
                                227,
                                [0, 1, 127, 128, 254, 255][(index + shift) % 6],
                            ];
                        }
                        let mut expected = actual.clone();
                        for (index, pixel) in expected[1..1025].chunks_exact_mut(4).enumerate() {
                            CfxAggRenderer::composite_pixel(
                                pixel,
                                None,
                                if solid { shift as u8 } else { covers[index] },
                                clip[index],
                                alpha,
                                color,
                                ColorData::Bgr([31, 91, 203]),
                                FULL,
                                RGB,
                                true,
                                FxdibFormat::Bgra,
                            );
                        }
                        let solid_cover = [shift as u8];
                        CfxAggRenderer::composite_span_argb::<RGB, FULL>(
                            &mut actual[1..1025],
                            if solid { &solid_cover } else { &covers },
                            solid,
                            Some(&clip),
                            alpha,
                            color,
                            [31, 91, 203],
                        );
                        assert_eq!(
                            actual, expected,
                            "RGB={RGB} FULL={FULL} alpha={alpha} shift={shift} solid={solid}"
                        );
                    }
                }
            }
        }
        check::<false, false>();
        check::<false, true>();
        check::<true, false>();
        check::<true, true>();
    }

    #[test]
    fn masked_argb_spans_crop_coverage_at_the_clip_origin() {
        for solid in [false, true] {
            let mut device = CfxDibitmap::create(6, 3, FxdibFormat::Bgra, 0).unwrap();
            let mut mask = CfxDibitmap::create(3, 1, FxdibFormat::EightBppMask, 0).unwrap();
            mask.scanline_mut(0).unwrap()[..3].copy_from_slice(&[0, 128, 255]);
            let mut clip = CfxAggClipRgn::new(6, 3);
            assert!(clip.intersect_mask(2, 1, Some(Rc::new(mask))));
            let mut row = ScanlineU8::new();
            if solid {
                assert!(row.reset_with_solid_spans(-1, 7));
                row.add_span(-1, 9, 128);
            } else {
                assert!(row.reset(-1, 7));
                row.add_cells(-1, &[0, 1, 17, 64, 128, 254, 255, 32, 12]);
            }
            row.finalize(1);
            CfxAggRenderer::new(&mut device, None, Some(&clip), 0xffff0000, false, false)
                .render(&row);
            let mut expected = [0; 24];
            expected[12..16].copy_from_slice(&[0, 0, 255, 64]);
            expected[16..20].copy_from_slice(&[0, 0, 255, if solid { 128 } else { 254 }]);
            assert_eq!(device.scanline(1).unwrap(), expected);
            assert!(device.scanline(0).unwrap().iter().all(|&byte| byte == 0));
            assert!(device.scanline(2).unwrap().iter().all(|&byte| byte == 0));
        }
    }

    #[test]
    fn clipped_argb_over_opaque_pixels_matches_direct_coverage_blending() {
        let all_covers: Vec<u8> = (0..=255).collect();
        for rgb in [false, true] {
            for full in [false, true] {
                for alpha in 0..=255u32 {
                    let mut color = alpha << 24 | 0x00cb5b1f;
                    if rgb {
                        color = argb_to_bgr_order_dib(color);
                    }
                    // Full-cover ignores path coverage; test both extremes.
                    let covers: &[u8] = if full { &[0, 255] } else { &all_covers };
                    for &cover in covers {
                        for clip in 0..=255u8 {
                            let initial = [clip, 255 - clip, clip ^ 0xa5, 255];
                            let effective = if full {
                                alpha * u32::from(clip) / 255
                            } else {
                                alpha * u32::from(cover) * u32::from(clip) / 65025
                            };
                            let mut expected = initial;
                            let source = if rgb { [203, 91, 31] } else { [31, 91, 203] };
                            for channel in 0..3 {
                                expected[channel] =
                                    ((u32::from(initial[channel]) * (255 - effective)
                                        + source[channel] * effective)
                                        / 255) as u8;
                            }
                            let mut actual = initial;
                            CfxAggRenderer::composite_pixel(
                                &mut actual,
                                None,
                                cover,
                                clip,
                                alpha as i32,
                                color,
                                ColorData::Bgr([31, 91, 203]),
                                full,
                                rgb,
                                true,
                                FxdibFormat::Bgra,
                            );
                            assert_eq!(
                                actual, expected,
                                "rgb={rgb} full={full} alpha={alpha} cover={cover} clip={clip}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn argb_span_clips_negative_origins_and_keeps_untouched_pixels() {
        let mut device = CfxDibitmap::create(4, 1, FxdibFormat::Bgra, 0).unwrap();
        let mut clip = CfxAggClipRgn::new(4, 1);
        assert!(clip.intersect_rect(FxRect::new(1, 0, 3, 1)));
        let mut scanline = ScanlineU8::new();
        assert!(scanline.reset(-2, 5));
        scanline.add_cells(-2, &[17, 18, 19, 64, 128, 255, 200, 190]);
        scanline.finalize(0);
        CfxAggRenderer::new(&mut device, None, Some(&clip), 0xffff0000, false, false)
            .render(&scanline);
        assert_eq!(
            device.scanline(0).unwrap(),
            &[0, 0, 0, 0, 0, 0, 255, 64, 0, 0, 255, 128, 0, 0, 0, 0,]
        );
    }

    #[test]
    fn rgba_path_composites_source_alpha_and_clip_mask() {
        let mut device = CfxDibitmap::create(2, 2, FxdibFormat::Bgra, 0).unwrap();
        let mut mask = CfxDibitmap::create(2, 2, FxdibFormat::EightBppMask, 0).unwrap();
        for row in 0..2 {
            mask.scanline_mut(row).unwrap()[..2].fill(128);
        }
        let mut clip = CfxAggClipRgn::new(2, 2);
        assert!(clip.intersect_mask(0, 0, Some(Rc::new(mask))));
        let mut path = rectangle();
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.add_path(&mut path, 0);
        let mut scanline = ScanlineU8::new();
        let mut renderer =
            CfxAggRenderer::new(&mut device, None, Some(&clip), 0xffff0000, false, false);
        assert!(render_scanlines(
            &mut rasterizer,
            &mut scanline,
            &mut renderer,
            false
        ));
        assert_eq!(&device.scanline(0).unwrap()[..4], &[0, 0, 255, 128]);
    }

    #[test]
    fn full_cover_gray_backdrop_retains_legacy_span_grouping() {
        let mut device = CfxDibitmap::create(68, 1, FxdibFormat::EightBppRgb, 0).unwrap();
        let mut backdrop = CfxDibitmap::create(68, 1, FxdibFormat::EightBppRgb, 0).unwrap();
        for (index, value) in backdrop.scanline_mut(0).unwrap().iter_mut().enumerate() {
            *value = index as u8;
        }
        let mut path = PathStorage::new();
        path.move_to(0.25, 0.0);
        path.line_to(65.75, 0.0);
        path.line_to(65.75, 1.0);
        path.line_to(0.25, 1.0);
        path.end_poly();
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.add_path(&mut path, 0);
        let mut row = ScanlineU8::new();
        let mut renderer =
            CfxAggRenderer::new(&mut device, Some(&backdrop), None, 0x80ffffff, true, false);
        assert!(!renderer.supports_solid_spans());
        assert!(render_scanlines(
            &mut rasterizer,
            &mut row,
            &mut renderer,
            false
        ));
        let result = device.scanline(0).unwrap();
        assert_eq!(result[0], 160);
        assert!(result[1..].iter().all(|&value| value == 0));
    }

    #[test]
    fn rgb_order_and_backdrop_follow_their_channel_paths() {
        let mut rgb_device = CfxDibitmap::create(1, 1, FxdibFormat::Bgrx, 0).unwrap();
        let mut backdrop = CfxDibitmap::create(1, 1, FxdibFormat::Bgrx, 0).unwrap();
        backdrop.scanline_mut(0).unwrap()[..4].copy_from_slice(&[100, 100, 100, 0]);
        CfxAggRenderer::composite_pixel(
            &mut rgb_device.scanline_mut(0).unwrap()[..4],
            Some(&backdrop.scanline(0).unwrap()[..4]),
            255,
            255,
            128,
            0,
            ColorData::Bgr([0, 0, 255]),
            false,
            true,
            false,
            FxdibFormat::Bgrx,
        );
        assert_eq!(&rgb_device.scanline(0).unwrap()[..4], &[177, 49, 49, 0]);

        CfxAggRenderer::composite_pixel(
            &mut rgb_device.scanline_mut(0).unwrap()[..4],
            None,
            255,
            255,
            255,
            argb_to_bgr_order_dib(0xffff0000),
            ColorData::Bgr([0, 0, 255]),
            false,
            true,
            false,
            FxdibFormat::Bgrx,
        );
        assert_eq!(&rgb_device.scanline(0).unwrap()[..4], &[255, 0, 0, 255]);

        let mut alpha_device = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        alpha_device.scanline_mut(0).unwrap()[..4].copy_from_slice(&[0, 0, 255, 128]);
        CfxAggRenderer::composite_pixel(
            &mut alpha_device.scanline_mut(0).unwrap()[..4],
            None,
            255,
            255,
            128,
            0x800000ff,
            ColorData::Bgr([255, 0, 0]),
            false,
            false,
            true,
            FxdibFormat::Bgra,
        );
        assert_eq!(&alpha_device.scanline(0).unwrap()[..4], &[170, 0, 85, 192]);
    }
}
