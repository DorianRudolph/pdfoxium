// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Fixed-point resampling from `core/fxge/dib/cstretchengine.{h,cpp}`.
//! Horizontal and vertical passes retain source weights and fixed-point rounding.

#[cfg(test)]
use super::cfx_dibitmap::CfxDibitmap;
use super::cfx_dibitmap::CfxDibitmapData;
use super::fx_dib::FxdibResampleOptions;
use super::fx_dib::{get_bpp_from_format, FxdibFormat};
use super::scanlinecomposer_iface::ScanlineComposerIface;
use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxcrt::pauseindicator_iface::PauseIndicatorIface;
use crate::fxge::calculate_pitch::calculate_pitch32;
use std::cell::{Ref, RefCell};
use std::ops::Deref;
use std::rc::Rc;

/// Read side of CFX_DIBBase used by the stretch engine. Guarded scanlines let
/// a lazy source keep its row alive without copying its pixels or palette.
pub trait StretchSource {
    type Row<'a>: Deref<Target = [u8]>
    where
        Self: 'a;
    type Palette<'a>: Deref<Target = [u32]>
    where
        Self: 'a;
    fn width(&self) -> i32;
    fn height(&self) -> i32;
    fn format(&self) -> FxdibFormat;
    fn pitch(&self) -> u32;
    fn palette(&self) -> Self::Palette<'_>;
    fn scanline(&self, row: i32) -> Self::Row<'_>;
    fn estimated_image_memory_burden(&self) -> usize {
        super::cfx_dibbase::estimated_image_memory_burden(self.format())
    }
    /// CFX_DIBBase's default for an already available memory bitmap.
    fn skip_to_scanline(&self, _row: i32, _pause: Option<&mut dyn PauseIndicatorIface>) -> bool {
        false
    }
}

impl StretchSource for CfxDibitmapData<'_> {
    type Row<'a>
        = &'a [u8]
    where
        Self: 'a;
    type Palette<'a>
        = &'a [u32]
    where
        Self: 'a;
    fn width(&self) -> i32 {
        self.width()
    }
    fn height(&self) -> i32 {
        self.height()
    }
    fn format(&self) -> FxdibFormat {
        self.format()
    }
    fn pitch(&self) -> u32 {
        self.pitch()
    }
    fn estimated_image_memory_burden(&self) -> usize {
        self.estimated_image_memory_burden()
    }
    fn palette(&self) -> &[u32] {
        self.palette()
    }
    fn scanline(&self, row: i32) -> &[u8] {
        self.scanline(row).expect("source row")
    }
}

impl StretchSource for RefCell<CfxDibitmapData<'_>> {
    type Row<'a>
        = Ref<'a, [u8]>
    where
        Self: 'a;
    type Palette<'a>
        = Ref<'a, [u32]>
    where
        Self: 'a;
    fn width(&self) -> i32 {
        self.borrow().width()
    }
    fn height(&self) -> i32 {
        self.borrow().height()
    }
    fn format(&self) -> FxdibFormat {
        self.borrow().format()
    }
    fn pitch(&self) -> u32 {
        self.borrow().pitch()
    }
    fn estimated_image_memory_burden(&self) -> usize {
        self.borrow().estimated_image_memory_burden()
    }
    fn palette(&self) -> Ref<'_, [u32]> {
        Ref::map(self.borrow(), CfxDibitmapData::palette)
    }
    fn scanline(&self, row: i32) -> Ref<'_, [u8]> {
        Ref::map(self.borrow(), |bitmap| {
            bitmap.scanline(row).expect("source row")
        })
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum State {
    Initial,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TransformMethod {
    OneToEight,
    OneToMany,
    EightToEight,
    EightToMany,
    Many,
    Alpha,
}

pub struct CStretchEngine<S: StretchSource> {
    source: Rc<S>,
    dest_format: FxdibFormat,
    dest_bpp: usize,
    src_width: i32,
    src_height: i32,
    dest_width: i32,
    dest_height: i32,
    dest_clip: FxRect,
    src_clip: FxRect,
    dest_scanline: Vec<u8>,
    inter_buf: Vec<u8>,
    expanded_row: Vec<u8>,
    inter_pitch: usize,
    options: FxdibResampleOptions,
    method: TransformMethod,
    state: State,
    cur_row: i32,
    weights: WeightTable,
}

impl<S: StretchSource> CStretchEngine<S> {
    /// The composer is passed to Continue rather than stored as an unowned
    /// pointer, allowing a transformer to own both engine and destination.
    pub fn new(
        source: Rc<S>,
        dest_format: FxdibFormat,
        dest_width: i32,
        dest_height: i32,
        clip: FxRect,
        options: FxdibResampleOptions,
    ) -> Self {
        let src_width = source.width();
        let src_height = source.height();
        let dest_bpp = get_bpp_from_format(dest_format) as usize;
        let src_bpp = get_bpp_from_format(source.format());
        let has_alpha = source.format() == FxdibFormat::Bgra;
        if has_alpha {
            assert_eq!(dest_format, FxdibFormat::Bgra);
        }
        let mut engine = Self {
            source,
            dest_format,
            dest_bpp,
            src_width,
            src_height,
            dest_width,
            dest_height,
            dest_clip: clip,
            src_clip: FxRect::default(),
            dest_scanline: Vec::new(),
            inter_buf: Vec::new(),
            expanded_row: Vec::new(),
            inter_pitch: 0,
            options: FxdibResampleOptions::default(),
            method: TransformMethod::OneToEight,
            state: State::Initial,
            cur_row: 0,
            weights: WeightTable::new(),
        };
        let Some(width) = clip.right.checked_sub(clip.left) else {
            return engine;
        };
        let Some(pitch) = calculate_pitch32(dest_bpp as i32, width) else {
            return engine;
        };
        if !is_valid_destination_dimension(dest_width)
            || !is_valid_destination_dimension(dest_height)
        {
            return engine;
        }
        engine.dest_scanline.resize(
            pitch as usize,
            if dest_format == FxdibFormat::Bgrx {
                255
            } else {
                0
            },
        );
        engine.inter_pitch = pitch as usize;
        if options.no_smoothing {
            engine.options.no_smoothing = true;
        } else if use_interpolate_bilinear(options, dest_width, dest_height, src_width, src_height)
        {
            engine.options.interpolate_bilinear = true;
        } else {
            engine.options = options;
        }
        // The source divides in float precision, then promotes to double.
        let scale_x = (src_width as f32 / dest_width as f32) as f64;
        let scale_y = (src_height as f32 / dest_height as f32) as f64;
        let base_x = if dest_width > 0 {
            0.0
        } else {
            f64::from(dest_width)
        };
        let base_y = if dest_height > 0 {
            0.0
        } else {
            f64::from(dest_height)
        };
        let mut left = scale_x * (f64::from(clip.left) + base_x);
        let mut right = scale_x * (f64::from(clip.right) + base_x);
        let mut top = scale_y * (f64::from(clip.top) + base_y);
        let mut bottom = scale_y * (f64::from(clip.bottom) + base_y);
        if left > right {
            std::mem::swap(&mut left, &mut right);
        }
        if top > bottom {
            std::mem::swap(&mut top, &mut bottom);
        }
        engine.src_clip = FxRect::new(
            left.floor() as i32,
            top.floor() as i32,
            right.ceil() as i32,
            bottom.ceil() as i32,
        );
        if engine.options.interpolate_bilinear {
            // A clipped destination still samples neighbors on both sides of
            // each source pixel center. Clamping weights to only the inverse-
            // mapped rectangle changes edge pixels compared with a full draw.
            engine.src_clip.left = engine.src_clip.left.saturating_sub(1);
            engine.src_clip.top = engine.src_clip.top.saturating_sub(1);
            engine.src_clip.right = engine.src_clip.right.saturating_add(1);
            engine.src_clip.bottom = engine.src_clip.bottom.saturating_add(1);
        }
        engine
            .src_clip
            .intersect(FxRect::new(0, 0, src_width, src_height));
        engine.method = match src_bpp {
            1 => {
                if dest_bpp == 8 {
                    TransformMethod::OneToEight
                } else {
                    TransformMethod::OneToMany
                }
            }
            8 => {
                if dest_bpp == 8 {
                    TransformMethod::EightToEight
                } else {
                    TransformMethod::EightToMany
                }
            }
            _ => {
                if has_alpha {
                    TransformMethod::Alpha
                } else {
                    TransformMethod::Many
                }
            }
        };
        engine
    }

    pub fn resample_options(&self) -> FxdibResampleOptions {
        self.options
    }

    pub fn start_stretch_horz(&mut self) -> bool {
        if self.dest_width == 0 || self.inter_pitch == 0 || self.dest_scanline.is_empty() {
            return false;
        }
        let Some(size) = usize::try_from(self.src_clip.height())
            .ok()
            .and_then(|height| height.checked_mul(self.inter_pitch))
            .filter(|&size| size != 0)
        else {
            return false;
        };
        self.inter_buf.clear();
        if self.inter_buf.try_reserve_exact(size).is_err() {
            return false;
        }
        self.inter_buf.resize(size, 0);
        if !self.weights.calculate_weights(
            self.dest_width,
            self.dest_clip.left,
            self.dest_clip.right,
            self.src_width,
            self.src_clip.left,
            self.src_clip.right,
            self.options,
        ) {
            return false;
        }
        if self.src_width > 0
            && matches!(
                self.method,
                TransformMethod::OneToEight | TransformMethod::OneToMany
            )
        {
            let size = (self.src_width as usize).div_ceil(8) * 8;
            self.expanded_row.clear();
            if self.expanded_row.try_reserve_exact(size).is_err() {
                return false;
            }
            self.expanded_row.resize(size, 0);
        }
        self.cur_row = self.src_clip.top;
        self.state = State::Horizontal;
        true
    }

    pub fn continue_stretch(
        &mut self,
        dest: &mut dyn ScanlineComposerIface,
        mut pause: Option<&mut dyn PauseIndicatorIface>,
    ) -> bool {
        #[cfg(feature = "render-profile")]
        let _profile = crate::render_profile::Scope::new("image resampling");
        while self.state == State::Horizontal {
            let pending = match pause.as_mut() {
                Some(pause) => self.continue_stretch_horz(Some(&mut **pause)),
                None => self.continue_stretch_horz(None),
            };
            if pending {
                return true;
            }
            self.state = State::Vertical;
            self.stretch_vert(dest);
        }
        false
    }

    pub fn continue_stretch_horz(
        &mut self,
        mut pause: Option<&mut dyn PauseIndicatorIface>,
    ) -> bool {
        if self.dest_width == 0 {
            return false;
        }
        let skip = match pause.as_mut() {
            Some(pause) => self
                .source
                .skip_to_scanline(self.cur_row, Some(&mut **pause)),
            None => self.source.skip_to_scanline(self.cur_row, None),
        };
        if skip {
            return true;
        }
        let mut rows_to_go = 10;
        while self.cur_row < self.src_clip.bottom {
            if rows_to_go == 0 {
                if pause
                    .as_mut()
                    .is_some_and(|pause| pause.need_to_pause_now())
                {
                    return true;
                }
                rows_to_go = 10;
            }
            // Row and palette guards end before the next pause callback.
            let row = self.source.scanline(self.cur_row);
            let palette = self.source.palette();
            let source_row = if matches!(
                self.method,
                TransformMethod::OneToEight | TransformMethod::OneToMany
            ) {
                expand_1bpp_row(&row, &mut self.expanded_row);
                self.expanded_row.as_slice()
            } else {
                &row
            };
            let offset = (self.cur_row - self.src_clip.top) as usize * self.inter_pitch;
            let dest = &mut self.inter_buf[offset..offset + self.inter_pitch];
            let bytes_per_pixel = self.dest_bpp / 8;
            if matches!(
                self.method,
                TransformMethod::OneToEight | TransformMethod::EightToEight
            ) {
                // Gray/mask rows use the same contiguous weight walk as RGB.
                // Padding is left untouched after the last weight record.
                stretch_byte_row(&self.weights, source_row, dest);
                self.cur_row += 1;
                rows_to_go -= 1;
                continue;
            }
            if self.method == TransformMethod::Many {
                // Match PDFium's row dispatch: fix the pixel stride once, then
                // walk destination pixels and weight records together.
                if bytes_per_pixel == 3 {
                    stretch_rgb_row::<3>(&self.weights, source_row, dest);
                } else {
                    assert_eq!(bytes_per_pixel, 4);
                    stretch_rgb_row::<4>(&self.weights, source_row, dest);
                }
                self.cur_row += 1;
                rows_to_go -= 1;
                continue;
            }
            for (index, col) in (self.dest_clip.left..self.dest_clip.right).enumerate() {
                let weights = self.weights.get_pixel_weight(col);
                match self.method {
                    TransformMethod::OneToEight | TransformMethod::EightToEight => unreachable!(),
                    TransformMethod::OneToMany | TransformMethod::EightToMany => {
                        let source = &source_row
                            [weights.src_start..weights.src_start + weights.weights.len()];
                        let mut rgb = [0u32; 3];
                        for (&weight, &sample) in weights.weights.iter().zip(source) {
                            let argb = palette[usize::from(sample)];
                            let channels = if self.dest_format == FxdibFormat::Bgr {
                                [argb as u8, (argb >> 8) as u8, (argb >> 16) as u8]
                            } else {
                                [(argb >> 24) as u8, (argb >> 16) as u8, (argb >> 8) as u8]
                            };
                            for (value, channel) in rgb.iter_mut().zip(channels) {
                                *value =
                                    value.wrapping_add(weight.wrapping_mul(u32::from(channel)));
                            }
                        }
                        for channel in 0..3 {
                            dest[index * 3 + channel] = pixel_from_fixed(rgb[channel]);
                        }
                    }
                    TransformMethod::Many => unreachable!(),
                    TransformMethod::Alpha => {
                        assert_eq!(bytes_per_pixel, 4);
                        let rgba = weighted_alpha(&weights, source_row);
                        for channel in 0..3 {
                            dest[index * 4 + channel] = pixel_from_fixed(rgba[channel]);
                        }
                        dest[index * 4 + 3] = pixel_from_fixed(255u32.wrapping_mul(rgba[3]));
                    }
                }
            }
            self.cur_row += 1;
            rows_to_go -= 1;
        }
        false
    }

    pub fn stretch_vert(&mut self, dest: &mut dyn ScanlineComposerIface) {
        if self.dest_height == 0 {
            return;
        }
        let mut table = WeightTable::new();
        if !table.calculate_weights(
            self.dest_height,
            self.dest_clip.top,
            self.dest_clip.bottom,
            self.src_height,
            self.src_clip.top,
            self.src_clip.bottom,
            self.options,
        ) {
            return;
        }
        let cols = self.dest_clip.width();
        if cols <= 0 {
            return;
        }
        let bytes_per_pixel = self.dest_bpp / 8;
        let row_bytes = (cols as usize)
            .checked_mul(bytes_per_pixel)
            .expect("row size");
        let mut accum = vec![0u32; row_bytes];
        for row in self.dest_clip.top..self.dest_clip.bottom {
            let weights = table.get_pixel_weight(row);
            if !weights.weights.is_empty() {
                assert!(
                    weights.src_start >= self.src_clip.top as usize
                        && weights.src_end < self.src_clip.bottom
                );
            } else {
                accum.fill(0);
            }
            if bytes_per_pixel == 1
                && matches!(
                    self.method,
                    TransformMethod::OneToEight | TransformMethod::EightToEight
                )
                && matches!(weights.weights, [_] | [_, _])
            {
                let offset = (weights.src_start - self.src_clip.top as usize) * self.inter_pitch;
                let first = &self.inter_buf[offset..offset + row_bytes];
                let second = if weights.weights.len() == 2 {
                    &self.inter_buf
                        [offset + self.inter_pitch..offset + self.inter_pitch + row_bytes]
                } else {
                    &[]
                };
                // Keep short filters in registers instead of writing and then
                // rereading the u32 accumulator for every byte of the row.
                stretch_byte_vert_row(
                    weights.weights,
                    first,
                    second,
                    &mut self.dest_scanline[..row_bytes],
                );
                dest.compose_scanline(row - self.dest_clip.top, &self.dest_scanline);
                continue;
            }
            for (tap, &weight) in weights.weights.iter().enumerate() {
                let offset =
                    (weights.src_start - self.src_clip.top as usize + tap) * self.inter_pitch;
                let source = &self.inter_buf[offset..offset + row_bytes];
                if tap == 0 {
                    for (acc, &byte) in accum.iter_mut().zip(source) {
                        *acc = weight.wrapping_mul(u32::from(byte));
                    }
                } else {
                    for (acc, &byte) in accum.iter_mut().zip(source) {
                        *acc = acc.wrapping_add(weight.wrapping_mul(u32::from(byte)));
                    }
                }
            }
            let dest_row = &mut self.dest_scanline[..row_bytes];
            match self.method {
                TransformMethod::OneToEight
                | TransformMethod::OneToMany
                | TransformMethod::EightToEight => {
                    for (pixel, acc) in dest_row
                        .chunks_exact_mut(bytes_per_pixel)
                        .zip(accum.chunks_exact(bytes_per_pixel))
                    {
                        pixel[0] = pixel_from_fixed(acc[0]);
                    }
                }
                TransformMethod::EightToMany | TransformMethod::Many => {
                    if bytes_per_pixel == 3 {
                        for (byte, &value) in dest_row.iter_mut().zip(&accum) {
                            *byte = pixel_from_fixed(value);
                        }
                    } else {
                        assert_eq!(bytes_per_pixel, 4);
                        for (pixel, acc) in dest_row.chunks_exact_mut(4).zip(accum.chunks_exact(4))
                        {
                            for channel in 0..3 {
                                pixel[channel] = pixel_from_fixed(acc[channel]);
                            }
                        }
                    }
                }
                TransformMethod::Alpha => {
                    assert_eq!(bytes_per_pixel, 4);
                    for (pixel, acc) in dest_row.chunks_exact_mut(4).zip(accum.chunks_exact(4)) {
                        if acc[3] != 0 {
                            for channel in 0..3 {
                                pixel[channel] =
                                    (acc[channel].wrapping_mul(255) / acc[3]).min(255) as u8;
                            }
                        }
                        pixel[3] = pixel_from_fixed(acc[3]);
                    }
                }
            }
            dest.compose_scanline(row - self.dest_clip.top, &self.dest_scanline);
        }
    }
}

fn stretch_byte_row(table: &WeightTable, source: &[u8], dest: &mut [u8]) {
    if table.full_single_taps {
        for (pixel, entry) in dest
            .iter_mut()
            .zip(table.words.chunks_exact(table.item_words))
        {
            *pixel = source[entry[0] as usize];
        }
        return;
    }
    match table.item_words {
        4 => return stretch_byte_row_short::<4>(&table.words, source, dest),
        5 => return stretch_byte_row_short::<5>(&table.words, source, dest),
        _ => (),
    }
    for (pixel, entry) in dest
        .iter_mut()
        .zip(table.words.chunks_exact(table.item_words))
    {
        let weights = pixel_weight_from_entry(entry);
        let source = &source[weights.src_start..weights.src_start + weights.weights.len()];
        *pixel = pixel_from_fixed(weighted_byte(weights.weights, source));
    }
}

fn stretch_byte_row_short<const WORDS: usize>(words: &[u32], source: &[u8], dest: &mut [u8]) {
    for (pixel, entry) in dest.iter_mut().zip(words.as_chunks::<WORDS>().0) {
        // The table builder validates nonnegative starts. Decode the
        // common one/two-tap records directly, leaving general/empty records
        // to the existing checked view. WORDS fixes the record stride per row.
        let start = entry[0] as usize;
        let sum = match entry[1].wrapping_sub(entry[0]) {
            0 => entry[2].wrapping_mul(u32::from(source[start])),
            1 => {
                let bytes = &source[start..start + 2];
                entry[2]
                    .wrapping_mul(u32::from(bytes[0]))
                    .wrapping_add(entry[3].wrapping_mul(u32::from(bytes[1])))
            }
            _ => {
                let weights = pixel_weight_from_entry(entry);
                let bytes = &source[start..start + weights.weights.len()];
                weighted_byte(weights.weights, bytes)
            }
        };
        *pixel = pixel_from_fixed(sum);
    }
}

// Keep short taps inline without pulling the vectorized long-filter loop into
// every destination pixel (matching the RGB helper below).
#[inline(always)]
fn weighted_byte(weights: &[u32], source: &[u8]) -> u32 {
    match weights {
        [weight] => weight.wrapping_mul(u32::from(source[0])),
        [a, b] => a
            .wrapping_mul(u32::from(source[0]))
            .wrapping_add(b.wrapping_mul(u32::from(source[1]))),
        [a, b, c] => a
            .wrapping_mul(u32::from(source[0]))
            .wrapping_add(b.wrapping_mul(u32::from(source[1])))
            .wrapping_add(c.wrapping_mul(u32::from(source[2]))),
        _ => weighted_byte_many(weights, source),
    }
}

#[inline(never)]
fn weighted_byte_many(weights: &[u32], source: &[u8]) -> u32 {
    weights
        .iter()
        .zip(source)
        .fold(0u32, |acc, (&weight, &byte)| {
            acc.wrapping_add(weight.wrapping_mul(u32::from(byte)))
        })
}

fn stretch_byte_vert_row(weights: &[u32], first: &[u8], second: &[u8], dest: &mut [u8]) {
    match weights {
        [weight] => {
            for (out, &sample) in dest.iter_mut().zip(first) {
                *out = pixel_from_fixed(weight.wrapping_mul(u32::from(sample)));
            }
        }
        [a, b] => {
            for ((out, &first), &second) in dest.iter_mut().zip(first).zip(second) {
                *out = pixel_from_fixed(
                    a.wrapping_mul(u32::from(first))
                        .wrapping_add(b.wrapping_mul(u32::from(second))),
                );
            }
        }
        _ => unreachable!(),
    }
}

fn stretch_rgb_row<const STRIDE: usize>(table: &WeightTable, source: &[u8], dest: &mut [u8]) {
    #[cfg(feature = "simd")]
    // SIMD dot-product setup pays off on long downsampling filters. Keep
    // the compact scalar unroll for short filters and magnification.
    if table.item_words >= 10 {
        resample_simd::stretch_rgb_row::<STRIDE>(table, source, dest);
        return;
    }
    stretch_rgb_row_scalar::<STRIDE>(table, source, dest);
}

fn stretch_rgb_row_scalar<const STRIDE: usize>(
    table: &WeightTable,
    source: &[u8],
    dest: &mut [u8],
) {
    // Select the wider unroll once per row. Leave the common one-to-three-tap
    // loop compact, and retain the vectorized general loop for longer filters.
    if (6..=9).contains(&table.item_words) {
        stretch_rgb_row_impl::<STRIDE, true>(table, source, dest);
    } else {
        stretch_rgb_row_impl::<STRIDE, false>(table, source, dest);
    }
}

fn stretch_rgb_row_impl<const STRIDE: usize, const WIDE: bool>(
    table: &WeightTable,
    source: &[u8],
    dest: &mut [u8],
) {
    for (pixel, entry) in dest
        .chunks_exact_mut(STRIDE)
        .zip(table.words.chunks_exact(table.item_words))
    {
        let weights = pixel_weight_from_entry(entry);
        let rgb = weighted_rgb::<STRIDE, WIDE>(&weights, source);
        for channel in 0..3 {
            pixel[channel] = pixel_from_fixed(rgb[channel]);
        }
    }
}

// Keep common short filters at the call site. A separate long-filter body
// prevents its vectorized loop from making this per-pixel helper too large
// for LLVM's inlining heuristic.
#[inline(always)]
fn weighted_rgb<const STRIDE: usize, const WIDE: bool>(
    weights: &PixelWeight<'_>,
    source: &[u8],
) -> [u32; 3] {
    let source =
        &source[weights.src_start * STRIDE..(weights.src_start + weights.weights.len()) * STRIDE];
    let mut result = [0u32; 3];
    let mut add = |weight: u32, pixel: &[u8]| {
        for channel in 0..3 {
            result[channel] =
                result[channel].wrapping_add(weight.wrapping_mul(u32::from(pixel[channel])));
        }
    };
    match weights.weights {
        [weight] => add(*weight, source),
        [a, b] => {
            add(*a, source);
            add(*b, &source[STRIDE..]);
        }
        [a, b, c] => {
            add(*a, source);
            add(*b, &source[STRIDE..]);
            add(*c, &source[2 * STRIDE..]);
        }
        [a, b, c, d] if WIDE => {
            add(*a, source);
            add(*b, &source[STRIDE..]);
            add(*c, &source[2 * STRIDE..]);
            add(*d, &source[3 * STRIDE..]);
        }
        [a, b, c, d, e] if WIDE => {
            add(*a, source);
            add(*b, &source[STRIDE..]);
            add(*c, &source[2 * STRIDE..]);
            add(*d, &source[3 * STRIDE..]);
            add(*e, &source[4 * STRIDE..]);
        }
        [a, b, c, d, e, f] if WIDE => {
            add(*a, source);
            add(*b, &source[STRIDE..]);
            add(*c, &source[2 * STRIDE..]);
            add(*d, &source[3 * STRIDE..]);
            add(*e, &source[4 * STRIDE..]);
            add(*f, &source[5 * STRIDE..]);
        }
        [a, b, c, d, e, f, g] if WIDE => {
            add(*a, source);
            add(*b, &source[STRIDE..]);
            add(*c, &source[2 * STRIDE..]);
            add(*d, &source[3 * STRIDE..]);
            add(*e, &source[4 * STRIDE..]);
            add(*f, &source[5 * STRIDE..]);
            add(*g, &source[6 * STRIDE..]);
        }
        _ => return weighted_rgb_many::<STRIDE>(weights.weights, source),
    }
    result
}

#[inline(never)]
fn weighted_rgb_many<const STRIDE: usize>(weights: &[u32], source: &[u8]) -> [u32; 3] {
    let mut result = [0u32; 3];
    for (&weight, pixel) in weights.iter().zip(source.chunks_exact(STRIDE)) {
        for channel in 0..3 {
            result[channel] =
                result[channel].wrapping_add(weight.wrapping_mul(u32::from(pixel[channel])));
        }
    }
    result
}

fn weighted_alpha(weights: &PixelWeight<'_>, source: &[u8]) -> [u32; 4] {
    let source = &source[weights.src_start * 4..(weights.src_start + weights.weights.len()) * 4];
    let mut result = [0u32; 4];
    let mut add = |weight: u32, pixel: &[u8]| {
        let weight = weight.wrapping_mul(u32::from(pixel[3])) / 255;
        for channel in 0..3 {
            result[channel] =
                result[channel].wrapping_add(weight.wrapping_mul(u32::from(pixel[channel])));
        }
        result[3] = result[3].wrapping_add(weight);
    };
    match weights.weights {
        [weight] => add(*weight, source),
        [a, b] => {
            add(*a, source);
            add(*b, &source[4..]);
        }
        _ => {
            for (&weight, pixel) in weights.weights.iter().zip(source.chunks_exact(4)) {
                add(weight, pixel);
            }
        }
    }
    result
}

fn expand_1bpp_row(source: &[u8], expanded: &mut [u8]) {
    const TABLE: [[u8; 8]; 256] = {
        let mut table = [[0u8; 8]; 256];
        let mut value = 0;
        while value < 256 {
            let mut bit = 0;
            while bit < 8 {
                table[value][bit] = if value >> (7 - bit) & 1 != 0 { 255 } else { 0 };
                bit += 1;
            }
            value += 1;
        }
        table
    };
    let source = &source[..expanded.len() / 8];
    for (dest, &byte) in expanded.chunks_exact_mut(8).zip(source) {
        dest.copy_from_slice(&TABLE[usize::from(byte)]);
    }
}

pub const FIXED_POINT_BITS: u32 = 16;
pub const FIXED_POINT_ONE: u32 = 1 << FIXED_POINT_BITS;

pub fn fixed_from_double(value: f64) -> u32 {
    // FXSYS_round() returns a signed int before the unsigned conversion.
    (value * f64::from(FIXED_POINT_ONE)).round() as i32 as u32
}

pub fn pixel_from_fixed(value: u32) -> u8 {
    (value >> FIXED_POINT_BITS) as u8
}

pub fn is_valid_destination_dimension(value: i32) -> bool {
    (-1024 * 1024..=1024 * 1024).contains(&value)
}

pub fn use_interpolate_bilinear(
    options: FxdibResampleOptions,
    dest_width: i32,
    dest_height: i32,
    src_width: i32,
    src_height: i32,
) -> bool {
    let width = i64::from(dest_width).abs();
    !options.interpolate_bilinear
        && !options.no_smoothing
        && width != 0
        && i64::from(dest_height).abs() / 8 < i64::from(src_width) * i64::from(src_height) / width
}

pub struct PixelWeight<'a> {
    pub src_start: usize,
    pub src_end: i32,
    pub weights: &'a [u32],
}

/// Contiguous headers and weight words replace C++'s variable-tail structs.
/// Each entry has two signed range words followed by at least one weight.
#[derive(Default)]
pub struct WeightTable {
    dest_min: i32,
    item_words: usize,
    words: Vec<u32>,
    // Proven by the builder: every record has exactly one weight of 65536.
    // Nearest-neighbor byte rows can gather source samples without doing a
    // redundant multiply, shift and tap-count dispatch for every pixel.
    full_single_taps: bool,
}

impl WeightTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_pixel_weight(&self, pixel: i32) -> PixelWeight<'_> {
        assert!(pixel >= self.dest_min);
        let index = (i64::from(pixel) - i64::from(self.dest_min)) as usize * self.item_words;
        let entry = &self.words[index..index + self.item_words];
        pixel_weight_from_entry(entry)
    }

    fn entry_mut(&mut self, pixel: i32) -> &mut [u32] {
        assert!(pixel >= self.dest_min);
        let index = (i64::from(pixel) - i64::from(self.dest_min)) as usize * self.item_words;
        &mut self.words[index..index + self.item_words]
    }

    pub fn calculate_weights(
        &mut self,
        dest_len: i32,
        dest_min: i32,
        dest_max: i32,
        src_len: i32,
        src_min: i32,
        src_max: i32,
        options: FxdibResampleOptions,
    ) -> bool {
        const MAX_TABLE_BYTES: usize = 512 * 1024 * 1024;
        self.dest_min = 0;
        self.item_words = 0;
        self.words.clear();
        self.full_single_taps = false;
        if dest_len == 0 {
            return true;
        }
        if dest_min > dest_max {
            return false;
        }
        self.dest_min = dest_min;
        let scale = f64::from(src_len) / f64::from(dest_len);
        let base = if dest_len < 0 {
            f64::from(src_len)
        } else {
            0.0
        };
        let weight_count = scale.abs().ceil() as usize + 1;
        self.item_words = 2 + weight_count.max(1);
        let item_bytes = self.item_words * std::mem::size_of::<u32>();
        let dest_range = (i64::from(dest_max) - i64::from(dest_min)) as usize;
        if dest_range > MAX_TABLE_BYTES / item_bytes {
            return false;
        }
        let size = dest_range * self.item_words;
        if self.words.try_reserve_exact(size).is_err() {
            return false;
        }
        self.words.resize(size, 0);
        if options.no_smoothing || scale.abs() < 1.0 {
            self.full_single_taps = true;
            for dest_pixel in dest_min..dest_max {
                let entry = self.entry_mut(dest_pixel);
                let src_pos = f64::from(dest_pixel) * scale + scale / 2.0 + base;
                if options.interpolate_bilinear {
                    let start = ((src_pos - 0.5).floor() as i32).max(src_min);
                    let end = ((src_pos + 0.5).floor() as i32).min(src_max.saturating_sub(1));
                    set_range(entry, start, end);
                    if start >= end {
                        entry[2] = FIXED_POINT_ONE;
                    } else {
                        let second_weight = fixed_from_double(src_pos - f64::from(start) - 0.5);
                        set_weight(entry, start, FIXED_POINT_ONE.wrapping_sub(second_weight));
                        set_weight(entry, start + 1, second_weight);
                    }
                } else {
                    let position = src_pos.floor() as i32;
                    set_range(
                        entry,
                        position.max(src_min),
                        position.min(src_max.saturating_sub(1)),
                    );
                    entry[2] = FIXED_POINT_ONE;
                }
                let full_single_tap = entry[0] == entry[1] && entry[2] == FIXED_POINT_ONE;
                self.full_single_taps &= full_single_tap;
            }
            return true;
        }
        for dest_pixel in dest_min..dest_max {
            let entry = self.entry_mut(dest_pixel);
            let src_start = f64::from(dest_pixel) * scale + base;
            let src_end = src_start + scale;
            let mut start = (src_start.min(src_end).floor() as i32).max(src_min);
            let end = (src_start.max(src_end).floor() as i32).min(src_max.saturating_sub(1));
            if start > end {
                start = start.clamp(0, src_max.saturating_sub(1).max(0));
                set_range(entry, start, start);
                continue;
            }
            set_range(entry, start, end);
            let mut remaining = FIXED_POINT_ONE;
            let mut rounding_error = 0.0;
            for source in start..end {
                let mut dest_start = (f64::from(source) - base) / scale;
                let mut dest_end = (f64::from(source) + 1.0 - base) / scale;
                if dest_start > dest_end {
                    std::mem::swap(&mut dest_start, &mut dest_end);
                }
                let area_start = dest_start.max(f64::from(dest_pixel));
                let area_end = dest_end.min(f64::from(dest_pixel) + 1.0);
                let weight = (area_end - area_start).max(0.0);
                let fixed_weight = fixed_from_double(weight + rounding_error);
                set_weight(entry, source, fixed_weight);
                remaining = remaining.wrapping_sub(fixed_weight);
                rounding_error = weight - f64::from(fixed_weight) / f64::from(FIXED_POINT_ONE);
            }
            if remaining != 0 && remaining <= FIXED_POINT_ONE {
                set_weight(entry, end, remaining);
            } else {
                assert!(end > start);
                entry[1] = (end - 1) as u32;
                let index = 2 + (end - 1 - start) as usize;
                entry[index] = entry[index].wrapping_add(remaining);
            }
        }
        true
    }
}

fn pixel_weight_from_entry(entry: &[u32]) -> PixelWeight<'_> {
    let start = entry[0] as i32;
    let end = entry[1] as i32;
    assert!(start >= 0);
    let count = if end < start {
        0
    } else {
        (end - start + 1) as usize
    };
    PixelWeight {
        src_start: start as usize,
        src_end: end,
        weights: &entry[2..2 + count],
    }
}

fn set_range(entry: &mut [u32], start: i32, end: i32) {
    assert!(start >= 0);
    assert!(i64::from(end) - i64::from(start) < (entry.len() - 2) as i64);
    entry[0] = start as u32;
    entry[1] = end as u32;
}

fn set_weight(entry: &mut [u32], position: i32, weight: u32) {
    let start = entry[0] as i32;
    assert!(position >= start && position <= entry[1] as i32);
    entry[2 + (position - start) as usize] = weight;
}

#[cfg(test)]
mod tests {
    use super::super::cfx_bitmapstorer::CfxBitmapStorer;
    use super::*;

    #[test]
    fn bilinear_region_keeps_neighbor_samples_outside_the_destination_clip() {
        for format in [
            FxdibFormat::EightBppMask,
            FxdibFormat::Bgr,
            FxdibFormat::Bgra,
        ] {
            let mut source = CfxDibitmap::create(7, 5, format, 0).unwrap();
            let bpp = get_bpp_from_format(format) as usize / 8;
            for row in 0..5 {
                for (i, byte) in source.scanline_mut(row).unwrap()[..7 * bpp]
                    .iter_mut()
                    .enumerate()
                {
                    *byte = ((i * 73 + row as usize * 57) % 256) as u8;
                }
            }
            let source = Rc::new(source);
            for (width, height) in [(17i32, 13i32), (-17, 13), (17, -13), (-17, -13)] {
                let render = |clip: FxRect| {
                    let mut storer = CfxBitmapStorer::new();
                    assert!(storer.set_info(clip.width(), clip.height(), format, Vec::new()));
                    let mut engine = CStretchEngine::new(
                        Rc::clone(&source),
                        format,
                        width,
                        height,
                        clip,
                        FxdibResampleOptions {
                            interpolate_bilinear: true,
                            ..Default::default()
                        },
                    );
                    assert!(engine.start_stretch_horz());
                    assert!(!engine.continue_stretch(&mut storer, None));
                    storer.detach().unwrap()
                };
                let whole = render(FxRect::new(0, 0, width.abs(), height.abs()));
                let whole = whole.borrow();
                for left in (1..14).step_by(3) {
                    for top in (1..10).step_by(3) {
                        let clip = FxRect::new(left, top, left + 3, top + 3);
                        let part = render(clip);
                        let part = part.borrow();
                        for y in 0..clip.height() {
                            assert_eq!(
                                &whole.scanline(y + top).unwrap()
                                    [left as usize * bpp..(left + 3) as usize * bpp],
                                &part.scanline(y).unwrap()[..3 * bpp],
                                "format={format:?} size={width},{height} clip={clip:?} row={y}"
                            );
                        }
                    }
                }
            }
        }
    }

    fn stretch(
        source: CfxDibitmap,
        format: FxdibFormat,
        width: i32,
        height: i32,
    ) -> Rc<RefCell<CfxDibitmap>> {
        let mut storer = CfxBitmapStorer::new();
        assert!(storer.set_info(width.abs(), height.abs(), format, Vec::new()));
        let mut engine = CStretchEngine::new(
            Rc::new(source),
            format,
            width,
            height,
            FxRect::new(0, 0, width.abs(), height.abs()),
            FxdibResampleOptions {
                no_smoothing: true,
                ..Default::default()
            },
        );
        assert!(engine.start_stretch_horz());
        assert!(!engine.continue_stretch(&mut storer, None));
        storer.detach().unwrap()
    }

    #[test]
    fn byte_rows_preserve_clipping_mirroring_filters_and_padding() {
        let source: Vec<u8> = (0..23).map(|i| ((i * 73) ^ (i >> 1)) as u8).collect();
        for width in [1i32, 7, 13, 17, 23, 47, -7, -13, -17, -23, -47] {
            for left in [0, 1] {
                let right = width.abs();
                if left >= right {
                    continue;
                }
                for options in [
                    FxdibResampleOptions::default(),
                    FxdibResampleOptions {
                        no_smoothing: true,
                        ..Default::default()
                    },
                    FxdibResampleOptions {
                        interpolate_bilinear: true,
                        ..Default::default()
                    },
                ] {
                    let mut table = WeightTable::new();
                    assert!(table.calculate_weights(width, left, right, 23, 0, 23, options));
                    let mut expected = vec![0xa5; (right - left) as usize + 3];
                    for (index, col) in (left..right).enumerate() {
                        let weights = table.get_pixel_weight(col);
                        let sum = weights
                            .weights
                            .iter()
                            .enumerate()
                            .map(|(tap, &weight)| {
                                u128::from(weight) * u128::from(source[weights.src_start + tap])
                            })
                            .sum::<u128>() as u32;
                        expected[index] = pixel_from_fixed(sum);
                    }
                    let mut actual = vec![0xa5; expected.len()];
                    stretch_byte_row(&table, &source, &mut actual);
                    assert_eq!(actual, expected, "width={width} left={left}");
                }
            }
        }
    }

    #[test]
    fn byte_short_and_long_filters_keep_wrapping_fixed_point_sums() {
        for count in (0..=17).chain([31, 32, 63, 64, 65]) {
            for start in 0..=3 {
                let weights: Vec<u32> = (0..count)
                    .map(|i| [u32::MAX, 1, 0, 65536, 0x8000_0000, 7][i % 6])
                    .collect();
                let source: Vec<u8> = (0..start + count)
                    .map(|i| ((i * 73) ^ (i >> 1)) as u8)
                    .collect();
                let expected = (0..count)
                    .map(|i| u128::from(weights[i]) * u128::from(source[start + i]))
                    .sum::<u128>() as u32;
                assert_eq!(weighted_byte(&weights, &source[start..]), expected);
            }
        }
    }

    #[test]
    fn byte_short_records_keep_empty_ranges_wrapping_weights_and_row_padding() {
        let source: Vec<u8> = (0..80).map(|i| ((i * 79) ^ (i >> 1)) as u8).collect();
        for words in [4, 5] {
            for width in [0, 1, 7, 8, 9, 31, 32, 33, 64, 65] {
                let mut table = WeightTable {
                    dest_min: 17,
                    item_words: words,
                    words: vec![0xa5a5_a5a5; width * words],
                    full_single_taps: false,
                };
                let mut expected = vec![0xa5; width + 3];
                for (i, entry) in table.words.chunks_exact_mut(words).enumerate() {
                    let start = (13 * i) % 73;
                    let count = i % (words - 1);
                    entry[0] = start as u32;
                    entry[1] = (start as i32 + count as i32 - 1) as u32;
                    let mut sum = 0u128;
                    for tap in 0..count {
                        let weight = [u32::MAX, 0, 1, FIXED_POINT_ONE, 0x8000_0000][(i + tap) % 5];
                        entry[2 + tap] = weight;
                        sum += u128::from(weight) * u128::from(source[start + tap]);
                    }
                    // Unused weight slots retain arbitrary data and must not
                    // contribute to the result, including for empty records.
                    expected[i] = pixel_from_fixed(sum as u32);
                }
                let mut actual = vec![0xa5; expected.len()];
                stretch_byte_row(&table, &source, &mut actual);
                assert_eq!(actual, expected, "words={words} width={width}");
            }
        }
    }

    #[test]
    fn byte_vertical_short_filters_keep_rounding_tails_and_row_guards() {
        for width in [0, 1, 2, 7, 8, 15, 16, 31, 32, 63, 64, 65, 129] {
            let first: Vec<u8> = (0..width).map(|i| (i * 73) as u8).collect();
            let second: Vec<u8> = (0..width).map(|i| (255 - (i * 37) % 256) as u8).collect();
            for weights in [
                &[0][..],
                &[FIXED_POINT_ONE][..],
                &[u32::MAX][..],
                &[1, 65535][..],
                &[32769, 32767][..],
                &[u32::MAX, 0x8000_0000][..],
            ] {
                let mut output = vec![0xa5; width + 2];
                stretch_byte_vert_row(weights, &first, &second, &mut output[1..width + 1]);
                assert_eq!(output[0], 0xa5);
                assert_eq!(output[width + 1], 0xa5);
                for i in 0..width {
                    let sum = u128::from(weights[0]) * u128::from(first[i])
                        + weights
                            .get(1)
                            .map_or(0, |&b| u128::from(b) * u128::from(second[i]));
                    assert_eq!(output[i + 1], pixel_from_fixed(sum as u32));
                }
            }
        }
    }

    #[test]
    fn rgb_rows_preserve_clipping_mirroring_padding_and_unused_channels() {
        fn check<const STRIDE: usize>() {
            let source: Vec<u8> = (0..23 * STRIDE).map(|i| (i * 73) as u8).collect();
            for width in [1i32, 4, 5, 7, 8, 23, 47, -4, -5, -7, -8, -23, -47] {
                for left in [0, 1] {
                    let right = width.abs();
                    if left >= right {
                        continue;
                    }
                    for options in [
                        FxdibResampleOptions::default(),
                        FxdibResampleOptions {
                            no_smoothing: true,
                            ..Default::default()
                        },
                        FxdibResampleOptions {
                            interpolate_bilinear: true,
                            ..Default::default()
                        },
                    ] {
                        let mut table = WeightTable::new();
                        assert!(table.calculate_weights(width, left, right, 23, 0, 23, options));
                        let mut expected = vec![0xa5; (right - left) as usize * STRIDE + 3];
                        for (index, col) in (left..right).enumerate() {
                            let weights = table.get_pixel_weight(col);
                            for channel in 0..3 {
                                let value = weights.weights.iter().enumerate().fold(
                                    0u32,
                                    |sum, (tap, &weight)| {
                                        sum.wrapping_add(weight.wrapping_mul(u32::from(
                                            source[(weights.src_start + tap) * STRIDE + channel],
                                        )))
                                    },
                                );
                                expected[index * STRIDE + channel] = pixel_from_fixed(value);
                            }
                        }
                        let mut actual = vec![0xa5; expected.len()];
                        stretch_rgb_row::<STRIDE>(&table, &source, &mut actual);
                        assert_eq!(
                            actual, expected,
                            "stride={STRIDE} width={width} left={left}"
                        );
                    }
                }
            }
        }
        check::<3>();
        check::<4>();
    }

    #[test]
    fn rgb_short_and_long_filters_preserve_wrapping_dot_products_and_ignore_x_channel() {
        fn check<const STRIDE: usize>() {
            for count in (0..=17).chain([31, 32, 63, 64, 65]) {
                for start in 0..=3 {
                    let weights: Vec<u32> = (0..count)
                        .map(|i| [u32::MAX, 1, 0, 65536, 0x8000_0000, 7][i % 6])
                        .collect();
                    let source: Vec<u8> = (0..(start + count + 1) * STRIDE)
                        .map(|i| ((i * 73) ^ (i >> 1)) as u8)
                        .collect();
                    let input = PixelWeight {
                        src_start: start,
                        src_end: (start + count) as i32 - 1,
                        weights: &weights,
                    };
                    let actual = weighted_rgb::<STRIDE, false>(&input, &source);
                    let wide = weighted_rgb::<STRIDE, true>(&input, &source);
                    // Wide arithmetic is an independent reference for modulo-u32
                    // sums, including overflowing fixed-point products.
                    let expected = std::array::from_fn(|channel| {
                        (0..count)
                            .map(|i| {
                                u128::from(weights[i])
                                    * u128::from(source[(start + i) * STRIDE + channel])
                            })
                            .sum::<u128>() as u32
                    });
                    assert_eq!(
                        actual, expected,
                        "stride={STRIDE} count={count} start={start}"
                    );
                    assert_eq!(
                        wide, expected,
                        "wide stride={STRIDE} count={count} start={start}"
                    );
                }
            }
        }
        check::<3>();
        check::<4>();
    }

    #[test]
    fn stretch_passes_preserve_bit_expansion_mirroring_palette_and_alpha() {
        let mut mask = CfxDibitmap::create(9, 1, FxdibFormat::OneBppMask, 0).unwrap();
        mask.buffer_mut()[..2].copy_from_slice(&[0x80, 0x80]);
        let mask = stretch(mask, FxdibFormat::EightBppMask, 9, 1);
        assert_eq!(
            &mask.borrow().buffer()[..9],
            &[255, 0, 0, 0, 0, 0, 0, 0, 255]
        );
        let mut gray = CfxDibitmap::create(2, 2, FxdibFormat::EightBppMask, 0).unwrap();
        gray.scanline_mut(0).unwrap()[..2].copy_from_slice(&[0, 255]);
        gray.scanline_mut(1).unwrap()[..2].copy_from_slice(&[64, 192]);
        let gray = stretch(gray, FxdibFormat::EightBppMask, -2, 2);
        assert_eq!(&gray.borrow().scanline(0).unwrap()[..2], &[255, 0]);
        assert_eq!(&gray.borrow().scanline(1).unwrap()[..2], &[192, 64]);
        let mut palette = CfxDibitmap::create(2, 1, FxdibFormat::EightBppRgb, 0).unwrap();
        palette.take_palette(vec![0xffff0000, 0xff0000ff]);
        palette.buffer_mut()[..2].copy_from_slice(&[0, 1]);
        let palette = stretch(palette, FxdibFormat::Bgr, 2, 1);
        assert_eq!(&palette.borrow().buffer()[..6], &[0, 0, 255, 255, 0, 0]);
        let mut alpha = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
        alpha.buffer_mut()[..8].copy_from_slice(&[100, 50, 0, 255, 0, 0, 255, 0]);
        let mut storer = CfxBitmapStorer::new();
        assert!(storer.set_info(1, 1, FxdibFormat::Bgra, Vec::new()));
        let mut engine = CStretchEngine::new(
            Rc::new(alpha),
            FxdibFormat::Bgra,
            1,
            1,
            FxRect::new(0, 0, 1, 1),
            FxdibResampleOptions::default(),
        );
        assert!(engine.start_stretch_horz());
        assert!(!engine.continue_stretch(&mut storer, None));
        assert_eq!(
            &storer.detach().unwrap().borrow().buffer()[..4],
            &[100, 50, 0, 127]
        );
    }

    #[test]
    fn stretch_pause_releases_source_guards_and_resumes_the_same_row() {
        struct Pause {
            source: Rc<RefCell<CfxDibitmap>>,
            calls: usize,
        }
        impl PauseIndicatorIface for Pause {
            fn need_to_pause_now(&mut self) -> bool {
                assert!(self.source.try_borrow_mut().is_ok());
                self.calls += 1;
                self.calls == 1
            }
        }
        let mut source = CfxDibitmap::create(1, 25, FxdibFormat::EightBppMask, 0).unwrap();
        for row in 0..25 {
            source.scanline_mut(row).unwrap()[0] = row as u8 + 1;
        }
        let source = Rc::new(RefCell::new(source));
        let mut pause = Pause {
            source: Rc::clone(&source),
            calls: 0,
        };
        let mut engine = CStretchEngine::new(
            source,
            FxdibFormat::EightBppMask,
            1,
            25,
            FxRect::new(0, 0, 1, 25),
            FxdibResampleOptions::default(),
        );
        let mut storer = CfxBitmapStorer::new();
        assert!(storer.set_info(1, 25, FxdibFormat::EightBppMask, Vec::new()));
        assert!(engine.start_stretch_horz());
        assert!(engine.continue_stretch(&mut storer, Some(&mut pause)));
        assert_eq!(engine.cur_row, 10);
        assert!(!engine.continue_stretch(&mut storer, Some(&mut pause)));
        let bitmap = storer.detach().unwrap();
        let borrowed = bitmap.borrow();
        for row in 0..25 {
            assert_eq!(borrowed.scanline(row).unwrap()[0], row as u8 + 1);
        }
    }

    #[test]
    fn mirrored_and_clipped_weights_keep_fixed_point_rounding() {
        let mut table = WeightTable::new();
        for options in [
            FxdibResampleOptions::default(),
            FxdibResampleOptions {
                no_smoothing: true,
                ..Default::default()
            },
            FxdibResampleOptions {
                interpolate_bilinear: true,
                ..Default::default()
            },
            FxdibResampleOptions {
                no_smoothing: true,
                interpolate_bilinear: true,
                ..Default::default()
            },
        ] {
            for (src, dest) in [(1, 2), (2, 1), (187, 337), (809, 512), (256, 808)] {
                for sign in [1, -1] {
                    assert!(table.calculate_weights(sign * dest, 0, dest, src, 0, src, options));
                    for pixel in 0..dest {
                        assert_eq!(
                            table
                                .get_pixel_weight(pixel)
                                .weights
                                .iter()
                                .copied()
                                .fold(0u32, u32::wrapping_add),
                            FIXED_POINT_ONE
                        );
                    }
                }
            }
        }
        assert!(table.calculate_weights(
            -4,
            1,
            3,
            2,
            0,
            2,
            FxdibResampleOptions {
                no_smoothing: true,
                ..Default::default()
            }
        ));
        assert_eq!(table.get_pixel_weight(1).src_start, 1);
        assert_eq!(table.get_pixel_weight(2).src_start, 0);
        assert!(!table.calculate_weights(4, 3, 2, 2, 0, 2, Default::default()));
        assert!(!table.calculate_weights(
            32 * 1024 * 1024 + 1,
            0,
            32 * 1024 * 1024 + 1,
            20,
            0,
            20,
            Default::default()
        ));
        for options in [
            FxdibResampleOptions::default(),
            FxdibResampleOptions {
                no_smoothing: true,
                ..Default::default()
            },
        ] {
            assert!(table.calculate_weights(100, 0, 100, 0, 0, 0, options));
        }
    }
}

#[cfg(test)]
#[path = "cstretchengine_unittest.rs"]
mod upstream_tests;

#[cfg(feature = "simd")]
#[path = "resample_simd.rs"]
mod resample_simd;

#[cfg(test)]
#[path = "resample_row_tests.rs"]
mod resample_row_tests;
