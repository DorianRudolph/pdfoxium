// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Image composition from `core/fxge/agg/cfx_agg_bitmapcomposer.{h,cpp}`.

use super::cfx_agg_cliprgn::CfxAggClipRgn;
use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxge::dib::cfx_dibitmap::{CfxDibitmap, CfxDibitmapData};
use crate::fxge::dib::cfx_scanlinecompositor::CfxScanlineCompositor;
use crate::fxge::dib::fx_dib::{
    get_bpp_from_format, get_is_mask_from_format, BlendMode, FxdibFormat,
};
use crate::fxge::dib::scanlinecomposer_iface::ScanlineComposerIface;

pub struct CfxAggBitmapComposer<'a, 'storage> {
    bitmap: &'a mut CfxDibitmapData<'storage>,
    clip: Option<&'a CfxAggClipRgn>,
    dest_rect: FxRect,
    alpha: f32,
    mask_color: u32,
    vertical: bool,
    flip_x: bool,
    flip_y: bool,
    rgb_byte_order: bool,
    blend_mode: BlendMode,
    compositor: Option<CfxScanlineCompositor>,
    scanline_v: Vec<u8>,
    clip_scan_v: Vec<u8>,
    add_clip_scan: Vec<u8>,
}

fn do_compose(
    dest: &mut [u8],
    source: &[u8],
    width: usize,
    clip: Option<&[u8]>,
    alpha: f32,
    add_clip: &mut [u8],
    compositor: &CfxScanlineCompositor,
) {
    let clip = if alpha != 1.0 {
        if let Some(clip) = clip {
            for (output, &input) in add_clip[..width].iter_mut().zip(clip) {
                *output = (f32::from(input) * alpha) as u8;
            }
        } else {
            add_clip[..width].fill((alpha * 255.0).round() as u8);
        }
        Some(&add_clip[..width])
    } else {
        clip
    };
    match compositor.source_format() {
        FxdibFormat::EightBppMask => compositor.composite_byte_mask_line(dest, source, width, clip),
        FxdibFormat::EightBppRgb => {
            compositor.composite_pal_bitmap_line(dest, source, 0, width, clip)
        }
        _ => compositor.composite_rgb_bitmap_line(dest, source, width, clip),
    }
}

fn replace_vertical_rgb<const STRIDE: usize, const RGB: bool>(
    bitmap: &mut [u8],
    source: &[u8],
    pitch: usize,
    offset: usize,
    height: usize,
    flip_y: bool,
) {
    if height == 0 {
        return;
    }
    let rows = bitmap[offset..].chunks_mut(pitch).take(height);
    let pixels = source[..height * STRIDE].chunks_exact(STRIDE);
    let copy = |row: &mut [u8], pixel: &[u8]| {
        row[..4].copy_from_slice(&if RGB {
            [pixel[2], pixel[1], pixel[0], 255]
        } else {
            [pixel[0], pixel[1], pixel[2], 255]
        });
    };
    if flip_y {
        for (row, pixel) in rows.rev().zip(pixels) {
            copy(row, pixel);
        }
    } else {
        for (row, pixel) in rows.zip(pixels) {
            copy(row, pixel);
        }
    }
}

fn replace_vertical_indexed<const RGB: bool>(
    bitmap: &mut [u8],
    source: &[u8],
    palette: &[u32],
    pitch: usize,
    offset: usize,
    height: usize,
    flip_y: bool,
) {
    if height == 0 {
        return;
    }
    let rows = bitmap[offset..].chunks_mut(pitch).take(height);
    let indices = &source[..height];
    let copy = |row: &mut [u8], &index: &u8| {
        // Indexed color composition ignores the palette's alpha, just like
        // CompositePalBitmapLine: without coverage, every pixel is opaque.
        let mut color = (palette[usize::from(index)] | 0xff00_0000).to_le_bytes();
        if RGB {
            color.swap(0, 2);
        }
        row[..4].copy_from_slice(&color);
    };
    if flip_y {
        for (row, index) in rows.rev().zip(indices) {
            copy(row, index);
        }
    } else {
        for (row, index) in rows.zip(indices) {
            copy(row, index);
        }
    }
}

impl<'a, 'storage> CfxAggBitmapComposer<'a, 'storage> {
    /// Device and clip borrows remain valid for the entire continuation; the
    /// stretcher receives this composer at each stage instead of retaining it.
    pub fn compose_normal(
        bitmap: &'a mut CfxDibitmapData<'storage>,
        clip: Option<&'a CfxAggClipRgn>,
        alpha: f32,
        mask_color: u32,
        dest_rect: FxRect,
        vertical: bool,
        flip_x: bool,
        flip_y: bool,
        rgb_byte_order: bool,
    ) -> Self {
        Self::compose(
            bitmap,
            clip,
            alpha,
            mask_color,
            dest_rect,
            vertical,
            flip_x,
            flip_y,
            rgb_byte_order,
            BlendMode::Normal,
        )
    }

    pub fn compose(
        bitmap: &'a mut CfxDibitmapData<'storage>,
        clip: Option<&'a CfxAggClipRgn>,
        alpha: f32,
        mask_color: u32,
        dest_rect: FxRect,
        vertical: bool,
        flip_x: bool,
        flip_y: bool,
        rgb_byte_order: bool,
        blend_mode: BlendMode,
    ) -> Self {
        Self {
            bitmap,
            clip,
            dest_rect,
            alpha,
            mask_color,
            vertical,
            flip_x,
            flip_y,
            rgb_byte_order,
            blend_mode,
            compositor: None,
            scanline_v: Vec::new(),
            clip_scan_v: Vec::new(),
            add_clip_scan: Vec::new(),
        }
    }

    pub fn composite_transformed(&mut self, source: &CfxDibitmap, color: u32, left: i32, top: i32) {
        let clip = self
            .clip
            .and_then(|clip| clip.mask().map(|mask| (mask, clip.box_rect())));
        // The rectangle also bounds the source for rectangular clips.
        let mut rect = FxRect::new(
            left,
            top,
            left.saturating_add(source.width()),
            top.saturating_add(source.height()),
        );
        if let Some(clip) = self.clip {
            rect.intersect(clip.box_rect());
        }
        rect.intersect(FxRect::new(0, 0, self.bitmap.width(), self.bitmap.height()));
        if rect.is_empty() {
            return;
        }
        let src_left = rect.left - left;
        let src_top = rect.top - top;
        if get_is_mask_from_format(source.format()) {
            self.bitmap.composite_mask_clipped(
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                source,
                color,
                src_left,
                src_top,
                self.blend_mode,
                self.rgb_byte_order,
                clip,
            );
        } else {
            self.bitmap.composite_bitmap_clipped(
                rect.left,
                rect.top,
                rect.width(),
                rect.height(),
                source,
                src_left,
                src_top,
                self.blend_mode,
                clip,
                self.rgb_byte_order,
            );
        }
    }

    fn compose_scanline_v<const BYTES: usize>(&mut self, line: i32, source: &[u8]) {
        let bytes = BYTES;
        let pitch = self.bitmap.pitch() as usize;
        let height = self.dest_rect.height() as usize;
        let dest_x = self.dest_rect.left
            + if self.flip_x {
                self.dest_rect.width() - line - 1
            } else {
                line
            };
        let dest_col = dest_x as usize * bytes;
        let start_y = self.dest_rect.top as usize;
        let source_format = self
            .compositor
            .as_ref()
            .expect("scanline compositor")
            .source_format();
        let replace = self.bitmap.format() == FxdibFormat::Bgra
            && matches!(
                source_format,
                FxdibFormat::Bgr | FxdibFormat::Bgrx | FxdibFormat::EightBppRgb
            )
            && self.alpha == 1.0
            && self.blend_mode == BlendMode::Normal
            && self.clip.is_none_or(|clip| clip.mask().is_none());
        let buffer = self.bitmap.buffer_mut();
        if replace {
            // Opaque Normal RGB replaces every BGRA destination byte. Write
            // the column directly instead of gathering the old background,
            // composing into a temporary row, and scattering it back.
            let offset = start_y * pitch + dest_col;
            match (source_format, self.rgb_byte_order) {
                (FxdibFormat::Bgr, false) => replace_vertical_rgb::<3, false>(
                    buffer,
                    source,
                    pitch,
                    offset,
                    height,
                    self.flip_y,
                ),
                (FxdibFormat::Bgr, true) => replace_vertical_rgb::<3, true>(
                    buffer,
                    source,
                    pitch,
                    offset,
                    height,
                    self.flip_y,
                ),
                (FxdibFormat::Bgrx, false) => replace_vertical_rgb::<4, false>(
                    buffer,
                    source,
                    pitch,
                    offset,
                    height,
                    self.flip_y,
                ),
                (FxdibFormat::Bgrx, true) => replace_vertical_rgb::<4, true>(
                    buffer,
                    source,
                    pitch,
                    offset,
                    height,
                    self.flip_y,
                ),
                (FxdibFormat::EightBppRgb, false) => replace_vertical_indexed::<false>(
                    buffer,
                    source,
                    self.compositor.as_ref().unwrap().source_palette_argb(),
                    pitch,
                    offset,
                    height,
                    self.flip_y,
                ),
                (FxdibFormat::EightBppRgb, true) => replace_vertical_indexed::<true>(
                    buffer,
                    source,
                    self.compositor.as_ref().unwrap().source_palette_argb(),
                    pitch,
                    offset,
                    height,
                    self.flip_y,
                ),
                _ => unreachable!(),
            }
            return;
        }
        for row in 0..height {
            let y = start_y + if self.flip_y { height - row - 1 } else { row };
            let offset = y * pitch + dest_col;
            self.scanline_v[row * bytes..(row + 1) * bytes]
                .copy_from_slice(&buffer[offset..offset + bytes]);
        }
        let clip_scan = if let Some(clip) = self.clip.filter(|clip| clip.mask().is_some()) {
            let mask = clip.mask().expect("clip mask");
            let col = (dest_x - clip.box_rect().left) as usize;
            for row in 0..height {
                let y = start_y + if self.flip_y { height - row - 1 } else { row };
                self.clip_scan_v[row] = mask
                    .scanline(y as i32 - clip.box_rect().top)
                    .expect("clip row")[col];
            }
            Some(&self.clip_scan_v[..height])
        } else {
            None
        };
        do_compose(
            &mut self.scanline_v,
            source,
            height,
            clip_scan,
            self.alpha,
            &mut self.add_clip_scan,
            self.compositor.as_ref().expect("scanline compositor"),
        );
        for row in 0..height {
            let y = start_y + if self.flip_y { height - row - 1 } else { row };
            let offset = y * pitch + dest_col;
            buffer[offset..offset + bytes]
                .copy_from_slice(&self.scanline_v[row * bytes..(row + 1) * bytes]);
        }
    }
}

impl ScanlineComposerIface for CfxAggBitmapComposer<'_, '_> {
    fn set_info(
        &mut self,
        width: i32,
        _height: i32,
        source_format: FxdibFormat,
        palette: Vec<u32>,
    ) -> bool {
        debug_assert!(!matches!(
            source_format,
            FxdibFormat::OneBppMask | FxdibFormat::OneBppRgb
        ));
        let format = self.bitmap.format();
        self.compositor = CfxScanlineCompositor::new(
            format,
            source_format,
            &palette,
            self.mask_color,
            self.blend_mode,
            self.rgb_byte_order,
        );
        if self.compositor.is_none() {
            return false;
        }
        if self.vertical {
            self.scanline_v.resize(
                get_bpp_from_format(format) as usize / 8 * width as usize + 4,
                0,
            );
            self.clip_scan_v.resize(self.bitmap.height() as usize, 0);
        }
        if self.alpha != 1.0 {
            self.add_clip_scan.resize(
                if self.vertical {
                    self.bitmap.height()
                } else {
                    self.bitmap.width()
                } as usize,
                0,
            );
        }
        true
    }

    fn compose_scanline(&mut self, line: i32, source: &[u8]) {
        if self.vertical {
            // Fix the pixel size outside the gather/scatter loops so each
            // tiny copy becomes an inline load/store rather than runtime memcpy.
            // SetInfo rejects one-bit and invalid destination formats.
            match get_bpp_from_format(self.bitmap.format()) {
                8 => self.compose_scanline_v::<1>(line, source),
                24 => self.compose_scanline_v::<3>(line, source),
                32 => self.compose_scanline_v::<4>(line, source),
                _ => unreachable!("unsupported compositor destination"),
            }
            return;
        }
        let clip_scan = self.clip.and_then(|clip| {
            clip.mask().map(|mask| {
                let row = mask
                    .scanline(self.dest_rect.top + line - clip.box_rect().top)
                    .expect("clip row");
                &row[(self.dest_rect.left - clip.box_rect().left) as usize..]
            })
        });
        let format = self.bitmap.format();
        let Some(offset) = (self.dest_rect.left as u32)
            .checked_mul(get_bpp_from_format(format) as u32)
            .map(|offset| offset / 8)
        else {
            return;
        };
        let row = self
            .bitmap
            .scanline_mut(self.dest_rect.top + line)
            .expect("destination row");
        do_compose(
            &mut row[offset as usize..],
            source,
            self.dest_rect.width() as usize,
            clip_scan,
            self.alpha,
            &mut self.add_clip_scan,
            self.compositor.as_ref().expect("scanline compositor"),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn vertical_composition_matches_transposed_horizontal_for_all_pixel_sizes() {
        for format in [
            FxdibFormat::EightBppMask,
            FxdibFormat::EightBppRgb,
            FxdibFormat::Bgr,
            FxdibFormat::Bgrx,
            FxdibFormat::Bgra,
        ] {
            let bytes = get_bpp_from_format(format) as usize / 8;
            for source_format in [
                FxdibFormat::EightBppMask,
                FxdibFormat::EightBppRgb,
                FxdibFormat::Bgr,
                FxdibFormat::Bgra,
            ] {
                let source_bytes = get_bpp_from_format(source_format) as usize / 8;
                for flip_x in [false, true] {
                    for flip_y in [false, true] {
                        for mode in [BlendMode::Normal, BlendMode::Multiply] {
                            let mut actual = CfxDibitmap::create(5, 6, format, 0).unwrap();
                            let mut expected = CfxDibitmap::create(6, 5, format, 0).unwrap();
                            actual.buffer_mut().fill(0x55);
                            for y in 0..6 {
                                for x in 0..5 {
                                    for c in 0..bytes {
                                        let value = (x * 37 + y * 71 + c * 97) as u8;
                                        actual.scanline_mut(y as i32).unwrap()[x * bytes + c] =
                                            value;
                                        expected.scanline_mut(x as i32).unwrap()[y * bytes + c] =
                                            value;
                                    }
                                }
                            }
                            {
                                // Fractional opacity forces the gather/blend/scatter
                                // path even for opaque RGB sources.
                                let mut vertical = CfxAggBitmapComposer::compose(
                                    &mut actual,
                                    None,
                                    0.5,
                                    0x8046_a3d1,
                                    FxRect::new(1, 2, 3, 5),
                                    true,
                                    flip_x,
                                    flip_y,
                                    false,
                                    mode,
                                );
                                let mut horizontal = CfxAggBitmapComposer::compose(
                                    &mut expected,
                                    None,
                                    0.5,
                                    0x8046_a3d1,
                                    FxRect::new(2, 1, 5, 3),
                                    false,
                                    false,
                                    false,
                                    false,
                                    mode,
                                );
                                assert!(vertical.set_info(3, 2, source_format, Vec::new()));
                                assert!(horizontal.set_info(3, 2, source_format, Vec::new()));
                                for line in 0..2 {
                                    let source: Vec<u8> = (0..3 * source_bytes)
                                        .map(|i| (i * 73 + line * 41 + 19) as u8)
                                        .collect();
                                    let row: Vec<u8> = if flip_y {
                                        source
                                            .chunks_exact(source_bytes)
                                            .rev()
                                            .flatten()
                                            .copied()
                                            .collect()
                                    } else {
                                        source.clone()
                                    };
                                    vertical.compose_scanline(line as i32, &source);
                                    horizontal.compose_scanline(
                                        if flip_x { 1 - line as i32 } else { line as i32 },
                                        &row,
                                    );
                                }
                            }
                            for y in 0..6 {
                                let row = actual.scanline(y as i32).unwrap();
                                for x in 0..5 {
                                    assert_eq!(
                                        &row[x * bytes..(x + 1) * bytes],
                                        &expected.scanline(x as i32).unwrap()[y * bytes..(y + 1) * bytes],
                                        "{format:?}/{source_format:?} flip={flip_x}/{flip_y} mode={mode:?} at {x}/{y}",
                                    );
                                }
                                assert!(row[5 * bytes..].iter().all(|&v| v == 0x55));
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn clip_alpha_truncation_matches_horizontal_and_vertical_composition() {
        for vertical in [false, true] {
            let mut dest = CfxDibitmap::create(4, 4, FxdibFormat::Bgra, 0).unwrap();
            let mut mask = CfxDibitmap::create(2, 2, FxdibFormat::EightBppMask, 0).unwrap();
            mask.scanline_mut(0).unwrap()[..2].copy_from_slice(&[255, 128]);
            mask.scanline_mut(1).unwrap()[..2].copy_from_slice(&[64, 0]);
            let mut clip = CfxAggClipRgn::new(4, 4);
            clip.intersect_mask(1, 1, Some(Rc::new(mask)));
            {
                let mut composer = CfxAggBitmapComposer::compose_normal(
                    &mut dest,
                    Some(&clip),
                    0.5,
                    0,
                    FxRect::new(1, 1, 3, 3),
                    vertical,
                    true,
                    true,
                    false,
                );
                assert!(composer.set_info(2, 2, FxdibFormat::Bgr, Vec::new()));
                composer.compose_scanline(0, &[0, 0, 255, 0, 0, 255]);
                composer.compose_scanline(1, &[0, 0, 255, 0, 0, 255]);
            }
            assert_eq!(dest.scanline(1).unwrap()[7], 127);
            assert_eq!(dest.scanline(1).unwrap()[11], 64);
            assert_eq!(dest.scanline(2).unwrap()[7], 32);
            assert_eq!(dest.scanline(2).unwrap()[11], 0);
        }
    }

    #[test]
    fn opaque_vertical_rgb_matches_the_masked_compositor_and_preserves_guards() {
        for format in [FxdibFormat::Bgr, FxdibFormat::Bgrx, FxdibFormat::Bgra] {
            let stride = if format == FxdibFormat::Bgr { 3 } else { 4 };
            let source: Vec<u8> = (0..6 * stride).map(|i| (i * 73 + 19) as u8).collect();
            for flip_x in [false, true] {
                for flip_y in [false, true] {
                    for rgb in [false, true] {
                        for alpha in [0.0, 1.0] {
                            for mode in [BlendMode::Normal, BlendMode::Multiply] {
                                let mut mask =
                                    CfxDibitmap::create(2, 3, FxdibFormat::EightBppMask, 0)
                                        .unwrap();
                                mask.clear(0xffff_ffff);
                                let mut clip = CfxAggClipRgn::new(5, 6);
                                clip.intersect_mask(1, 1, Some(Rc::new(mask)));
                                let mut actual =
                                    CfxDibitmap::create(5, 6, FxdibFormat::Bgra, 0).unwrap();
                                let mut expected =
                                    CfxDibitmap::create(5, 6, FxdibFormat::Bgra, 0).unwrap();
                                actual.clear(0x8011_2537);
                                expected.clear(0x8011_2537);
                                for (bitmap, clip) in
                                    [(&mut actual, None), (&mut expected, Some(&clip))]
                                {
                                    let mut composer = CfxAggBitmapComposer::compose(
                                        bitmap,
                                        clip,
                                        alpha,
                                        0,
                                        FxRect::new(1, 1, 3, 4),
                                        true,
                                        flip_x,
                                        flip_y,
                                        rgb,
                                        mode,
                                    );
                                    assert!(composer.set_info(3, 2, format, Vec::new()));
                                    composer.compose_scanline(0, &source[..3 * stride]);
                                    composer.compose_scanline(1, &source[3 * stride..]);
                                }
                                assert_eq!(actual.buffer(), expected.buffer(), "{format:?} flip={flip_x}/{flip_y} rgb={rgb} alpha={alpha} mode={mode:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn opaque_vertical_indices_match_masked_composition_for_all_palette_entries() {
        let source: Vec<u8> = (0..512).map(|i| i as u8).collect();
        let colors: Vec<u32> = (0..256u32)
            .map(|i| i << 24 | ((i * 73) & 255) << 16 | (255 - i) << 8 | i)
            .collect();
        for palette in [Vec::new(), colors] {
            for flip_x in [false, true] {
                for flip_y in [false, true] {
                    for rgb in [false, true] {
                        for alpha in [0.0, 1.0] {
                            let mut mask =
                                CfxDibitmap::create(2, 256, FxdibFormat::EightBppMask, 0).unwrap();
                            mask.clear(0xffff_ffff);
                            let mut clip = CfxAggClipRgn::new(5, 260);
                            clip.intersect_mask(1, 2, Some(Rc::new(mask)));
                            let mut actual =
                                CfxDibitmap::create(5, 260, FxdibFormat::Bgra, 0).unwrap();
                            let mut expected =
                                CfxDibitmap::create(5, 260, FxdibFormat::Bgra, 0).unwrap();
                            // Exercise transparent, translucent and opaque
                            // backdrops, including untouched border pixels.
                            for (i, pixel) in actual.buffer_mut().chunks_exact_mut(4).enumerate() {
                                pixel.copy_from_slice(&[17, 37, 55, (i * 73) as u8]);
                            }
                            expected.buffer_mut().copy_from_slice(actual.buffer());
                            for (bitmap, clip) in
                                [(&mut actual, None), (&mut expected, Some(&clip))]
                            {
                                let mut composer = CfxAggBitmapComposer::compose_normal(
                                    bitmap,
                                    clip,
                                    alpha,
                                    0,
                                    FxRect::new(1, 2, 3, 258),
                                    true,
                                    flip_x,
                                    flip_y,
                                    rgb,
                                );
                                assert!(composer.set_info(
                                    256,
                                    2,
                                    FxdibFormat::EightBppRgb,
                                    palette.clone()
                                ));
                                composer.compose_scanline(0, &source[..256]);
                                composer.compose_scanline(1, &source[256..]);
                            }
                            assert_eq!(
                                actual.buffer(),
                                expected.buffer(),
                                "palette={} flip={flip_x}/{flip_y} rgb={rgb} alpha={alpha}",
                                palette.len()
                            );
                        }
                    }
                }
            }
        }
    }
}

#[path = "cfx_agg_bitmapcomposer_state.rs"]
mod owned;
pub(crate) use owned::DetachedBitmapComposer;
