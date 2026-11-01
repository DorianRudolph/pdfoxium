// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Bitmap allocation, transfer, conversion and composition from
//! `core/fxge/dib/cfx_dibitmap.{h,cpp}`. Owned and borrowed pixel storage
//! share the same safe bitmap operations.

use super::cfx_dibbase;
use super::cfx_scanlinecompositor::CfxScanlineCompositor;
use super::cstretchengine::StretchSource;
use super::fx_dib::{
    alpha_merge, argb_a, argb_b, argb_g, argb_r, get_bpp_from_format, rgb_to_gray, BlendMode,
    FxdibFormat,
};
use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxge::calculate_pitch::{calculate_pitch32, calculate_pitch8};

pub type CfxDibitmap = CfxDibitmapData<'static>;

enum PixelBuffer<'storage> {
    Owned(Vec<u8>),
    Borrowed(&'storage mut [u8]),
}

impl std::ops::Deref for PixelBuffer<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Owned(buffer) => buffer,
            Self::Borrowed(buffer) => buffer,
        }
    }
}
impl std::ops::DerefMut for PixelBuffer<'_> {
    fn deref_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Owned(buffer) => buffer,
            Self::Borrowed(buffer) => buffer,
        }
    }
}

pub struct CfxDibitmapData<'storage> {
    width: i32,
    height: i32,
    format: FxdibFormat,
    pitch: u32,
    buffer: PixelBuffer<'storage>,
    palette: Vec<u32>,
}

impl Default for CfxDibitmapData<'_> {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            format: FxdibFormat::Invalid,
            pitch: 0,
            buffer: PixelBuffer::Owned(Vec::new()),
            palette: Vec::new(),
        }
    }
}

impl<'storage> CfxDibitmapData<'storage> {
    fn find_palette(&self, color: u32) -> i32 {
        if self.palette.is_empty() {
            return if get_bpp_from_format(self.format) == 1 {
                i32::from((color as u8) == 255)
            } else {
                (color as u8) as i32
            };
        }
        self.palette
            .iter()
            .position(|&entry| entry == color)
            .map_or(-1, |index| index as i32)
    }

    pub fn clear(&mut self, color: u32) {
        let bytes = color.to_le_bytes();
        match self.format {
            FxdibFormat::Invalid => {}
            FxdibFormat::OneBppMask => {
                self.buffer_mut()
                    .fill(if color & 0xff00_0000 != 0 { 255 } else { 0 });
            }
            FxdibFormat::OneBppRgb => {
                let fill = if self.find_palette(color) != 0 {
                    255
                } else {
                    0
                };
                self.buffer_mut().fill(fill);
            }
            FxdibFormat::EightBppMask => self.buffer_mut().fill(argb_a(color)),
            FxdibFormat::EightBppRgb => {
                let index = self.find_palette(color) as u8;
                self.buffer_mut().fill(index);
            }
            FxdibFormat::Bgr => {
                if bytes[0] == bytes[1] && bytes[1] == bytes[2] {
                    self.buffer_mut().fill(bytes[0]);
                } else {
                    let width = self.width as usize;
                    for y in 0..self.height {
                        let row = self.scanline_mut(y).expect("bitmap row");
                        for pixel in row[..width * 3].chunks_exact_mut(3) {
                            pixel.copy_from_slice(&bytes[..3]);
                        }
                    }
                }
            }
            FxdibFormat::Bgrx | FxdibFormat::Bgra => {
                let width = self.width as usize;
                for y in 0..self.height {
                    let row = self.scanline_mut(y).expect("bitmap row");
                    for pixel in row[..width * 4].chunks_exact_mut(4) {
                        pixel.copy_from_slice(&bytes);
                    }
                }
            }
        }
    }

    pub fn composite_rect(
        &mut self,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
        color: u32,
    ) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        let src_alpha = i32::from(argb_a(color));
        if src_alpha == 0 {
            return true;
        }
        let Some(right) = left.checked_add(width) else {
            return false;
        };
        let Some(bottom) = top.checked_add(height) else {
            return false;
        };
        let mut rect = FxRect::new(left, top, right, bottom);
        rect.intersect(FxRect::new(0, 0, self.width, self.height));
        if rect.is_empty() {
            return true;
        }
        let bytes = color.to_le_bytes();
        let pixel_count = rect.width() as usize;
        match self.format {
            FxdibFormat::EightBppMask | FxdibFormat::EightBppRgb => {
                let gray = if self.format == FxdibFormat::EightBppMask {
                    255
                } else {
                    rgb_to_gray(
                        i32::from(argb_r(color)),
                        i32::from(argb_g(color)),
                        i32::from(argb_b(color)),
                    ) as u8
                };
                for y in rect.top..rect.bottom {
                    let row = self.scanline_mut(y).expect("bitmap row");
                    let target = &mut row[rect.left as usize..rect.right as usize];
                    if src_alpha == 255 {
                        target.fill(gray);
                    } else {
                        for pixel in target {
                            *pixel =
                                alpha_merge(i32::from(*pixel), i32::from(gray), src_alpha) as u8;
                        }
                    }
                }
                true
            }
            FxdibFormat::OneBppMask | FxdibFormat::OneBppRgb => {
                let mut index = 0;
                if self.palette.is_empty() {
                    index = usize::from(color as u8 == 255);
                } else {
                    for (i, &entry) in self.palette.iter().take(2).enumerate() {
                        if entry == color {
                            index = i;
                        }
                    }
                }
                let left_byte = rect.left as usize / 8;
                let right_byte = rect.right as usize / 8;
                let left_shift = rect.left % 8;
                let right_shift = rect.right % 8;
                let new_width = right_byte - left_byte;
                // Create() reserves four cleared bytes beyond the exposed span.
                // Keep the pinned right-edge and read/modify/write order even
                // when that byte lies in the next row or the final guard.
                let last_edge = (rect.bottom as usize - 1) * self.pitch as usize + right_byte;
                if last_edge >= self.buffer.len() {
                    return false;
                }
                for row in rect.top..rect.bottom {
                    let offset = row as usize * self.pitch as usize;
                    let left = offset + left_byte;
                    let right = offset + right_byte;
                    let left_flag = self.buffer[left] & (255_i32 << (8 - left_shift)) as u8;
                    let right_flag = self.buffer[right] & (255_i32 >> right_shift) as u8;
                    if new_width != 0 {
                        self.buffer[left + 1..right].fill(if index != 0 { 255 } else { 0 });
                        if index == 0 {
                            self.buffer[left] &= left_flag;
                            self.buffer[right] &= right_flag;
                        } else {
                            self.buffer[left] |= !left_flag;
                            self.buffer[right] |= !right_flag;
                        }
                    } else if index == 0 {
                        self.buffer[left] &= left_flag | right_flag;
                    } else {
                        self.buffer[left] |= !(left_flag | right_flag);
                    }
                }
                true
            }
            FxdibFormat::Bgr | FxdibFormat::Bgrx | FxdibFormat::Bgra => {
                let bpp = (get_bpp_from_format(self.format) / 8) as usize;
                let format = self.format;
                if src_alpha == 255 {
                    let uniform = bytes[..bpp].iter().all(|&byte| byte == bytes[0]);
                    // Only collapse rows when the rectangle includes every byte
                    // of each row; otherwise preserve stride padding and edges.
                    if uniform && rect.left == 0 && rect.right as usize * bpp == self.pitch as usize
                    {
                        let start = rect.top as usize * self.pitch as usize;
                        let end = rect.bottom as usize * self.pitch as usize;
                        self.buffer_mut()[start..end].fill(bytes[0]);
                        return true;
                    }
                    for y in rect.top..rect.bottom {
                        let row = self.scanline_mut(y).expect("bitmap row");
                        let target = &mut row[rect.left as usize * bpp..rect.right as usize * bpp];
                        if uniform {
                            target.fill(bytes[0]);
                        } else if bpp == 4 {
                            for pixel in target.chunks_exact_mut(4) {
                                pixel.copy_from_slice(&bytes);
                            }
                        } else {
                            for pixel in target.chunks_exact_mut(3) {
                                pixel.copy_from_slice(&bytes[..3]);
                            }
                        }
                    }
                    return true;
                }
                for y in rect.top..rect.bottom {
                    let row = self.scanline_mut(y).expect("bitmap row");
                    let target = &mut row[rect.left as usize * bpp..rect.right as usize * bpp];
                    for pixel in target.chunks_exact_mut(bpp).take(pixel_count) {
                        if format == FxdibFormat::Bgra {
                            let back_alpha = i32::from(pixel[3]);
                            if back_alpha == 0 {
                                pixel[..3].copy_from_slice(&bytes[..3]);
                                pixel[3] = src_alpha as u8;
                            } else {
                                let out_alpha =
                                    back_alpha + src_alpha - back_alpha * src_alpha / 255;
                                let ratio = src_alpha * 255 / out_alpha;
                                for channel in 0..3 {
                                    pixel[channel] = alpha_merge(
                                        i32::from(pixel[channel]),
                                        i32::from(bytes[channel]),
                                        ratio,
                                    ) as u8;
                                }
                                pixel[3] = out_alpha as u8;
                            }
                        } else {
                            for channel in 0..3 {
                                pixel[channel] = alpha_merge(
                                    i32::from(pixel[channel]),
                                    i32::from(bytes[channel]),
                                    src_alpha,
                                ) as u8;
                            }
                            if bpp == 4 {
                                pixel[3] = 255;
                            }
                        }
                    }
                }
                true
            }
            FxdibFormat::Invalid => false,
        }
    }

    pub fn set_red_from_alpha(&mut self) {
        assert_eq!(self.format, FxdibFormat::Bgra);
        assert!(!self.buffer.is_empty());
        let width = self.width as usize;
        for y in 0..self.height {
            let row = self.scanline_mut(y).expect("bitmap row");
            for pixel in row[..width * 4].chunks_exact_mut(4) {
                pixel[2] = pixel[3];
            }
        }
    }

    pub fn set_uniform_opaque_alpha(&mut self) {
        assert_eq!(self.format, FxdibFormat::Bgra);
        assert!(!self.buffer.is_empty());
        let width = self.width as usize;
        for y in 0..self.height {
            let row = self.scanline_mut(y).expect("bitmap row");
            for pixel in row[..width * 4].chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }
    }

    pub fn convert_to_bgra(&mut self) -> bool {
        self.convert_format(FxdibFormat::Bgra)
    }

    pub fn convert_to_8bpp_mask(&mut self) -> bool {
        self.convert_format(FxdibFormat::EightBppMask)
    }

    pub fn convert_to_bgr(&mut self) -> bool {
        self.convert_format(FxdibFormat::Bgr)
    }

    /// CFX_DIBitmap::ConvertFormat(), using the shared base conversions.
    pub fn convert_format(&mut self, format: FxdibFormat) -> bool {
        assert!(matches!(
            format,
            FxdibFormat::Bgra | FxdibFormat::Bgr | FxdibFormat::EightBppMask
        ));
        if self.format == format {
            return true;
        }
        if self.format == FxdibFormat::Invalid {
            return false;
        }
        if format == FxdibFormat::EightBppMask
            && self.format == FxdibFormat::EightBppRgb
            && self.palette.is_empty()
        {
            self.format = format;
            return true;
        }
        if format == FxdibFormat::Bgra && self.format == FxdibFormat::Bgrx {
            self.format = format;
            self.set_uniform_opaque_alpha();
            return true;
        }
        // Preserve the explicit safe failures for the converter's NOTREACHED
        // one-bit alpha and CHECKed one-bit palette-to-gray combinations.
        if get_bpp_from_format(self.format) == 1
            && (format == FxdibFormat::Bgra
                || (format == FxdibFormat::EightBppMask && !self.palette.is_empty()))
        {
            return false;
        }
        let Some(mut result) = Self::create(self.width, self.height, format, 0) else {
            return false;
        };
        if format == FxdibFormat::Bgra {
            result.buffer.fill(255);
        }
        let Some(palette) = cfx_dibbase::convert_buffer(
            format,
            &mut result.buffer,
            result.pitch as usize,
            self.width,
            self.height,
            self,
            0,
            0,
        ) else {
            return false;
        };
        result.palette = palette;
        *self = result;
        true
    }

    pub fn convert_to(&self, format: FxdibFormat) -> Option<CfxDibitmap> {
        cfx_dibbase::convert_to(self, format)
    }

    pub fn clone_alpha_mask(&self) -> Option<CfxDibitmap> {
        cfx_dibbase::clone_alpha_mask(self)
    }

    pub fn realize(&self) -> Option<CfxDibitmap> {
        cfx_dibbase::realize(self)
    }

    pub fn estimated_image_memory_burden(&self) -> usize {
        cfx_dibbase::estimated_image_memory_burden(self.format) + self.buffer().len()
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        let pixels = match &self.buffer {
            PixelBuffer::Owned(buffer) => buffer.capacity(),
            PixelBuffer::Borrowed(buffer) => buffer.len(),
        };
        pixels + self.palette.capacity() * std::mem::size_of::<u32>()
    }

    pub fn multiply_alpha_mask(&mut self, mask: &CfxDibitmapData<'_>) -> bool {
        assert_eq!(self.width, mask.width);
        assert_eq!(self.height, mask.height);
        assert_eq!(mask.format, FxdibFormat::EightBppMask);
        if !matches!(self.format, FxdibFormat::Bgra | FxdibFormat::Bgrx) {
            return false;
        }
        let started_bgrx = self.format == FxdibFormat::Bgrx;
        if !self.convert_to_bgra() {
            return false;
        }
        let width = self.width as usize;
        for y in 0..self.height {
            let mask_row = mask.scanline(y).expect("mask row");
            let row = self.scanline_mut(y).expect("bitmap row");
            for x in 0..width {
                let alpha = &mut row[x * 4 + 3];
                *alpha = if started_bgrx {
                    mask_row[x]
                } else {
                    (u32::from(*alpha) * u32::from(mask_row[x]) / 255) as u8
                };
            }
        }
        true
    }

    pub fn multiply_alpha(&mut self, alpha: f32) -> bool {
        assert!((0.0..=1.0).contains(&alpha));
        assert!(!matches!(
            self.format,
            FxdibFormat::OneBppMask | FxdibFormat::EightBppMask
        ));
        if alpha == 1.0 {
            return true;
        }
        if !self.convert_to_bgra() {
            return false;
        }
        let bitmap_alpha = (alpha * 255.0) as i32;
        let width = self.width as usize;
        for y in 0..self.height {
            let row = self.scanline_mut(y).expect("bitmap row");
            for pixel in row[..width * 4].chunks_exact_mut(4) {
                pixel[3] = (i32::from(pixel[3]) * bitmap_alpha / 255) as u8;
            }
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn composite_mask_normal(
        &mut self,
        dest_left: i32,
        dest_top: i32,
        width: i32,
        height: i32,
        mask: &CfxDibitmapData<'_>,
        color: u32,
        src_left: i32,
        src_top: i32,
        rgb_byte_order: bool,
    ) -> bool {
        self.composite_mask_normal_clipped(
            dest_left,
            dest_top,
            width,
            height,
            mask,
            color,
            src_left,
            src_top,
            rgb_byte_order,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn composite_mask_normal_clipped(
        &mut self,
        dest_left: i32,
        dest_top: i32,
        width: i32,
        height: i32,
        mask: &CfxDibitmapData<'_>,
        color: u32,
        src_left: i32,
        src_top: i32,
        rgb_byte_order: bool,
        clip: Option<(&CfxDibitmap, FxRect)>,
    ) -> bool {
        self.composite_mask_clipped(
            dest_left,
            dest_top,
            width,
            height,
            mask,
            color,
            src_left,
            src_top,
            BlendMode::Normal,
            rgb_byte_order,
            clip,
        )
    }

    pub fn composite_mask_clipped<S: StretchSource>(
        &mut self,
        mut dest_left: i32,
        mut dest_top: i32,
        mut width: i32,
        mut height: i32,
        mask: &S,
        color: u32,
        mut src_left: i32,
        mut src_top: i32,
        blend: BlendMode,
        rgb_byte_order: bool,
        clip: Option<(&CfxDibitmap, FxRect)>,
    ) -> bool {
        let source_format = mask.format();
        if !super::fx_dib::get_is_mask_from_format(source_format)
            || self.buffer.is_empty()
            || self.bpp() < 8
        {
            return false;
        }
        if width <= 0 || height <= 0 {
            return true;
        }
        if !cfx_dibbase::get_overlap_rect(
            self.width,
            self.height,
            &mut dest_left,
            &mut dest_top,
            &mut width,
            &mut height,
            mask.width(),
            mask.height(),
            &mut src_left,
            &mut src_top,
            clip.map(|(_, rect)| rect),
        ) {
            return true;
        }
        if argb_a(color) == 0 {
            return true;
        }
        let Some(compositor) = CfxScanlineCompositor::new(
            self.format,
            source_format,
            &[],
            color,
            blend,
            rgb_byte_order,
        ) else {
            return false;
        };
        let bytes = self.bpp() as usize / 8;
        for row in 0..height {
            let src = mask.scanline(src_top + row);
            let clip_row = clip.map(|(mask, rect)| {
                &mask.scanline(dest_top + row - rect.top).expect("clip row")
                    [(dest_left - rect.left) as usize..]
            });
            let dest = self.scanline_mut(dest_top + row).expect("destination row");
            let dest = &mut dest[dest_left as usize * bytes..];
            if get_bpp_from_format(source_format) == 1 {
                compositor.composite_bit_mask_line(
                    dest,
                    &src,
                    src_left as usize,
                    width as usize,
                    clip_row,
                );
            } else {
                compositor.composite_byte_mask_line(
                    dest,
                    &src[src_left as usize..],
                    width as usize,
                    clip_row,
                );
            }
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn composite_one_bpp_mask(
        &mut self,
        dest_left: i32,
        dest_top: i32,
        width: i32,
        height: i32,
        source: &CfxDibitmapData<'_>,
        src_left: i32,
        src_top: i32,
    ) {
        if get_bpp_from_format(self.format) != 1
            || get_bpp_from_format(source.format) != 1
            || width <= 0
            || height <= 0
        {
            return;
        }
        let (mut dest_left, mut dest_top, mut width, mut height, mut src_left, mut src_top) =
            (dest_left, dest_top, width, height, src_left, src_top);
        if !cfx_dibbase::get_overlap_rect(
            self.width,
            self.height,
            &mut dest_left,
            &mut dest_top,
            &mut width,
            &mut height,
            source.width,
            source.height,
            &mut src_left,
            &mut src_top,
            None,
        ) {
            return;
        }
        for row in 0..height {
            let src = source.scanline(src_top + row).expect("source row");
            let dest = self.scanline_mut(dest_top + row).expect("destination row");
            for col in 0..width {
                let src_col = (src_left + col) as usize;
                let dest_col = (dest_left + col) as usize;
                if src[src_col / 8] & (0x80 >> (src_col % 8)) != 0 {
                    dest[dest_col / 8] |= 0x80 >> (dest_col % 8);
                }
            }
        }
    }

    /// Expands a mono glyph span into the byte mask used by PDFium's glyph path.
    pub fn populate_8bpp_mask_from_1bpp_span(&mut self, source: &[u8], src_pitch: u32) -> bool {
        if self.format != FxdibFormat::EightBppMask {
            return false;
        }
        let width = self.width as usize;
        let src_pitch = src_pitch as usize;
        let Some(required) = src_pitch.checked_mul(self.height as usize) else {
            return false;
        };
        if src_pitch < width.div_ceil(8) || source.len() < required {
            return false;
        }
        for y in 0..self.height as usize {
            let src_row = &source[y * src_pitch..(y + 1) * src_pitch];
            let dest_row = self.scanline_mut(y as i32).expect("bitmap row");
            for x in 0..width {
                dest_row[x] = if src_row[x / 8] & (0x80 >> (x % 8)) != 0 {
                    255
                } else {
                    0
                };
            }
        }
        true
    }

    /// Copies a glyph span, zeroing destination bytes beyond the source pitch.
    pub fn populate_from_span(&mut self, source: &[u8], src_pitch: u32) -> bool {
        let src_pitch = src_pitch as usize;
        let Some(required) = src_pitch.checked_mul(self.height as usize) else {
            return false;
        };
        if source.len() < required {
            return false;
        }
        let rowbytes = src_pitch.min(self.pitch as usize);
        self.buffer_mut().fill(0);
        for y in 0..self.height as usize {
            let dest_row = self.scanline_mut(y as i32).expect("bitmap row");
            dest_row[..rowbytes].copy_from_slice(&source[y * src_pitch..y * src_pitch + rowbytes]);
        }
        true
    }

    /// The equal-format branches of `CFX_DIBitmap::TransferBitmap()`.
    pub fn transfer_bitmap<S: StretchSource>(
        &mut self,
        mut width: i32,
        mut height: i32,
        source: &S,
        mut src_left: i32,
        mut src_top: i32,
    ) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        if width <= 0 || height <= 0 {
            return true;
        }
        let (mut left, mut top) = (0, 0);
        if !cfx_dibbase::get_overlap_rect(
            self.width,
            self.height,
            &mut left,
            &mut top,
            &mut width,
            &mut height,
            source.width(),
            source.height(),
            &mut src_left,
            &mut src_top,
            None,
        ) {
            return true;
        }
        let format = self.format;
        let bpp = self.bpp() as usize;
        if format != source.format() {
            if !self.palette.is_empty() || bpp == 1 {
                return false;
            }
            let dest_format = if bpp == 8 {
                FxdibFormat::EightBppMask
            } else {
                format
            };
            // Preserve the safe failure for the source converter's CHECKed
            // one-bit palette-to-gray combination.
            if dest_format == FxdibFormat::EightBppMask
                && get_bpp_from_format(source.format()) == 1
                && !source.palette().is_empty()
            {
                return false;
            }
            let Some(offset) = (left as u32).checked_mul(bpp as u32).map(|value| value / 8) else {
                return false;
            };
            let pitch = self.pitch as usize;
            let start = top as usize * pitch + offset as usize;
            let Some(palette) = cfx_dibbase::convert_buffer(
                dest_format,
                &mut self.buffer_mut()[start..],
                pitch,
                width,
                height,
                source,
                src_left,
                src_top,
            ) else {
                return false;
            };
            assert!(palette.is_empty());
            return true;
        }
        if bpp != 1 {
            let bytes = bpp / 8;
            let count = width as usize * bytes;
            let src_offset = src_left as usize * bytes;
            let dest_offset = left as usize * bytes;
            for row in 0..height {
                let src = source.scanline(src_top + row);
                let dest = self.scanline_mut(top + row).expect("destination row");
                dest[dest_offset..dest_offset + count]
                    .copy_from_slice(&src[src_offset..src_offset + count]);
            }
        } else {
            for row in 0..height {
                let src = source.scanline(src_top + row);
                let dest = self.scanline_mut(top + row).expect("destination row");
                for col in 0..width {
                    let src_col = (src_left + col) as usize;
                    let dest_col = (left + col) as usize;
                    let bit = 0x80 >> (dest_col % 8);
                    if src[src_col / 8] & (0x80 >> (src_col % 8)) != 0 {
                        dest[dest_col / 8] |= bit;
                    } else {
                        dest[dest_col / 8] &= !bit;
                    }
                }
            }
        }
        true
    }

    pub fn transfer_bitmap_same_format(
        &mut self,
        width: i32,
        height: i32,
        source: &CfxDibitmapData<'_>,
        src_left: i32,
        src_top: i32,
    ) -> bool {
        if self.format != source.format {
            return false;
        }
        self.transfer_bitmap(width, height, source, src_left, src_top)
    }

    pub fn transfer_bitmap_direct_color(
        &mut self,
        width: i32,
        height: i32,
        source: &CfxDibitmapData<'_>,
        src_left: i32,
        src_top: i32,
    ) -> bool {
        self.transfer_bitmap(width, height, source, src_left, src_top)
    }

    pub fn clip_to(&self, rect: FxRect) -> Option<CfxDibitmap> {
        cfx_dibbase::clip_to(self, rect)
    }

    pub fn swap_xy(&self, flip_x: bool, flip_y: bool) -> Option<CfxDibitmap> {
        cfx_dibbase::swap_xy(self, flip_x, flip_y)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn composite_bitmap<S: StretchSource>(
        &mut self,
        dest_left: i32,
        dest_top: i32,
        width: i32,
        height: i32,
        source: &S,
        src_left: i32,
        src_top: i32,
        blend_mode: BlendMode,
    ) -> bool {
        self.composite_bitmap_clipped(
            dest_left, dest_top, width, height, source, src_left, src_top, blend_mode, None, false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn composite_bitmap_normal(
        &mut self,
        dest_left: i32,
        dest_top: i32,
        width: i32,
        height: i32,
        source: &CfxDibitmapData<'_>,
        src_left: i32,
        src_top: i32,
    ) -> bool {
        self.composite_bitmap_normal_clipped(
            dest_left, dest_top, width, height, source, src_left, src_top, None, false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn composite_bitmap_normal_clipped(
        &mut self,
        dest_left: i32,
        dest_top: i32,
        width: i32,
        height: i32,
        source: &CfxDibitmapData<'_>,
        src_left: i32,
        src_top: i32,
        clip: Option<(&CfxDibitmap, FxRect)>,
        rgb_byte_order: bool,
    ) -> bool {
        self.composite_bitmap_clipped(
            dest_left,
            dest_top,
            width,
            height,
            source,
            src_left,
            src_top,
            BlendMode::Normal,
            clip,
            rgb_byte_order,
        )
    }

    pub fn composite_bitmap_clipped<S: StretchSource>(
        &mut self,
        mut dest_left: i32,
        mut dest_top: i32,
        mut width: i32,
        mut height: i32,
        source: &S,
        mut src_left: i32,
        mut src_top: i32,
        blend: BlendMode,
        clip: Option<(&CfxDibitmap, FxRect)>,
        rgb_byte_order: bool,
    ) -> bool {
        let source_format = source.format();
        if super::fx_dib::get_is_mask_from_format(source_format)
            || self.buffer.is_empty()
            || self.bpp() < 8
        {
            return false;
        }
        if width <= 0 || height <= 0 {
            return true;
        }
        if !cfx_dibbase::get_overlap_rect(
            self.width,
            self.height,
            &mut dest_left,
            &mut dest_top,
            &mut width,
            &mut height,
            source.width(),
            source.height(),
            &mut src_left,
            &mut src_top,
            clip.map(|(_, rect)| rect),
        ) {
            return true;
        }
        let Some(compositor) = CfxScanlineCompositor::new(
            self.format,
            source_format,
            &source.palette(),
            0,
            blend,
            rgb_byte_order,
        ) else {
            return false;
        };
        let dest_bytes = self.bpp() as usize / 8;
        let source_bytes = get_bpp_from_format(source_format) as usize / 8;
        let rgb = source_bytes > 1;
        if !rgb && source.palette().is_empty() {
            return false;
        }
        for row in 0..height {
            let src = source.scanline(src_top + row);
            let src = &src[src_left as usize * source_bytes..];
            let clip_row = clip.map(|(mask, rect)| {
                &mask.scanline(dest_top + row - rect.top).expect("clip row")
                    [(dest_left - rect.left) as usize..]
            });
            let dest = self.scanline_mut(dest_top + row).expect("destination row");
            let dest = &mut dest[dest_left as usize * dest_bytes..];
            if rgb {
                compositor.composite_rgb_bitmap_line(dest, src, width as usize, clip_row);
            } else {
                compositor.composite_pal_bitmap_line(
                    dest,
                    &src,
                    src_left as usize,
                    width as usize,
                    clip_row,
                );
            }
        }
        true
    }

    pub fn calculate_pitch_and_size(
        width: i32,
        height: i32,
        format: FxdibFormat,
        pitch: u32,
    ) -> Option<(u32, u32)> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let bpp = get_bpp_from_format(format);
        if bpp == 0 {
            return None;
        }
        let pitch = if pitch == 0 {
            calculate_pitch32(bpp, width)?
        } else {
            if pitch < calculate_pitch8(bpp as u32, 1, width)? {
                return None;
            }
            pitch
        };
        let size = pitch.checked_mul(height as u32)?;
        Some((pitch, size))
    }

    pub fn create(width: i32, height: i32, format: FxdibFormat, pitch: u32) -> Option<Self> {
        let mut bitmap = Self::default();
        bitmap
            .create_in_place(width, height, format, pitch)
            .then_some(bitmap)
    }

    pub fn create_in_place(
        &mut self,
        width: i32,
        height: i32,
        format: FxdibFormat,
        pitch: u32,
    ) -> bool {
        self.buffer = PixelBuffer::Owned(Vec::new());
        self.format = format;
        self.width = 0;
        self.height = 0;
        self.pitch = 0;
        let Some((pitch, size)) = Self::calculate_pitch_and_size(width, height, format, pitch)
        else {
            return false;
        };
        let Some(alloc_size) = (size as usize).checked_add(4) else {
            return false;
        };
        let Some(buffer) = crate::fxcrt::fx_memory::try_alloc_zeroed(alloc_size) else {
            return false;
        };
        self.buffer = PixelBuffer::Owned(buffer);
        self.width = width;
        self.height = height;
        self.pitch = pitch;
        true
    }

    /// Borrow the caller's initialized pixel span. No pixel allocation or copy
    /// occurs; the slice must cover the calculated pitch and height.
    pub fn from_buffer(
        width: i32,
        height: i32,
        format: FxdibFormat,
        pitch: u32,
        buffer: &'storage mut [u8],
    ) -> Option<Self> {
        let (pitch, size) = Self::calculate_pitch_and_size(width, height, format, pitch)?;
        if buffer.len() < size as usize {
            return None;
        }
        Some(Self {
            width,
            height,
            format,
            pitch,
            buffer: PixelBuffer::Borrowed(buffer),
            palette: Vec::new(),
        })
    }

    pub fn is_borrowed(&self) -> bool {
        matches!(self.buffer, PixelBuffer::Borrowed(_))
    }

    /// ConvertFormat/Create may replace borrowed storage with a new owned
    /// allocation. Return that allocation without copying it to the old buffer.
    pub fn into_owned_if_allocated(self) -> Option<CfxDibitmap> {
        let Self {
            width,
            height,
            format,
            pitch,
            buffer,
            palette,
        } = self;
        match buffer {
            PixelBuffer::Owned(buffer) => Some(CfxDibitmapData {
                width,
                height,
                format,
                pitch,
                buffer: PixelBuffer::Owned(buffer),
                palette,
            }),
            PixelBuffer::Borrowed(_) => None,
        }
    }

    pub fn copy_from<S: StretchSource>(&mut self, source: &S) -> bool {
        if !self.buffer.is_empty() {
            return false;
        }
        if !self.create_in_place(source.width(), source.height(), source.format(), 0) {
            return false;
        }
        self.set_palette(&source.palette());
        for row in 0..source.height() {
            let src = source.scanline(row);
            let dest = self.scanline_mut(row).expect("copied bitmap row");
            dest[..src.len()].copy_from_slice(&src);
        }
        true
    }

    pub fn convert_color_scale(&mut self, white_on_black: bool) {
        if self.buffer.is_empty() || self.is_mask_format() {
            return;
        }
        if self.bpp() <= 8 {
            if !white_on_black && self.palette.is_empty() {
                return;
            }
            self.build_palette();
            for color in &mut self.palette {
                let mut gray = rgb_to_gray(
                    i32::from(argb_r(*color)),
                    i32::from(argb_g(*color)),
                    i32::from(argb_b(*color)),
                ) as u32;
                if !white_on_black {
                    gray = 255 - gray;
                }
                *color = 0xff000000 | gray << 16 | gray << 8 | gray;
            }
            return;
        }
        let bytes = self.bpp() as usize / 8;
        let width = self.width as usize;
        for row in 0..self.height {
            let dest = self.scanline_mut(row).expect("bitmap row");
            for pixel in dest[..width * bytes].chunks_exact_mut(bytes) {
                let gray = rgb_to_gray(
                    i32::from(pixel[2]),
                    i32::from(pixel[1]),
                    i32::from(pixel[0]),
                ) as u8;
                // As in the source, polarity inversion applies to palettes;
                // direct-color pixels always become their grayscale value.
                pixel[..3].fill(gray);
            }
        }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn bpp(&self) -> i32 {
        get_bpp_from_format(self.format)
    }
    pub fn is_mask_format(&self) -> bool {
        super::fx_dib::get_is_mask_from_format(self.format)
    }
    pub fn is_alpha_format(&self) -> bool {
        super::fx_dib::get_is_alpha_from_format(self.format)
    }
    pub fn is_opaque_image(&self) -> bool {
        !self.is_mask_format() && !self.is_alpha_format()
    }
    pub fn is_premultiplied(&self) -> bool {
        false
    } // Non-Skia selected configuration.

    pub fn format(&self) -> FxdibFormat {
        self.format
    }

    pub fn pitch(&self) -> u32 {
        self.pitch
    }

    pub fn buffer(&self) -> &[u8] {
        &self.buffer[..self.pitch as usize * self.height as usize]
    }

    pub fn buffer_mut(&mut self) -> &mut [u8] {
        let size = self.pitch as usize * self.height as usize;
        &mut self.buffer[..size]
    }

    pub fn palette(&self) -> &[u32] {
        &self.palette
    }

    pub fn required_palette_size(&self) -> usize {
        cfx_dibbase::required_palette_size(self.format)
    }

    pub fn palette_argb(&self, index: usize) -> u32 {
        debug_assert!(matches!(
            self.format,
            FxdibFormat::OneBppRgb | FxdibFormat::EightBppRgb
        ));
        if !self.palette.is_empty() {
            return self.palette[index];
        }
        let value = if get_bpp_from_format(self.format) == 1 {
            u32::from(index != 0) * 255
        } else {
            index as u32
        };
        0xff000000 | (value << 16) | (value << 8) | value
    }

    pub fn set_palette_argb(&mut self, index: usize, color: u32) {
        debug_assert!(matches!(
            self.format,
            FxdibFormat::OneBppRgb | FxdibFormat::EightBppRgb
        ));
        self.build_palette();
        self.palette[index] = color;
    }

    fn build_palette(&mut self) {
        if !self.palette.is_empty() {
            return;
        }
        match get_bpp_from_format(self.format) {
            1 => self.palette = vec![0xff000000, 0xffffffff],
            8 => {
                self.palette = (0..256)
                    .map(|value| 0xff000000 | value << 16 | value << 8 | value)
                    .collect()
            }
            _ => {}
        }
    }

    pub fn set_palette(&mut self, palette: &[u32]) {
        self.take_palette(palette.to_vec());
    }

    /// `CFX_DIBBase::TakePalette()` retains palettes only for indexed formats.
    pub fn take_palette(&mut self, palette: Vec<u32>) {
        let bpp = get_bpp_from_format(self.format);
        if palette.is_empty() || bpp > 8 {
            self.palette.clear();
            return;
        }
        self.palette = palette;
        self.palette.resize(1usize << bpp, 0);
    }

    pub fn scanline(&self, line: i32) -> Option<&[u8]> {
        let start = usize::try_from(line)
            .ok()?
            .checked_mul(self.pitch as usize)?;
        self.buffer()
            .get(start..start.checked_add(self.pitch as usize)?)
    }

    pub fn scanline_mut(&mut self, line: i32) -> Option<&mut [u8]> {
        let start = usize::try_from(line)
            .ok()?
            .checked_mul(self.pitch as usize)?;
        let end = start.checked_add(self.pitch as usize)?;
        self.buffer_mut().get_mut(start..end)
    }

    /// GetScanlineAs(): width pixels, excluding any trailing pitch padding.
    /// Borrow byte groups instead of reinterpreting storage as C++ structs.
    pub fn scanline_pixels<const BYTES: usize>(
        &self,
        line: i32,
    ) -> Option<std::slice::ChunksExact<'_, u8>> {
        let size = usize::try_from(self.width).ok()?.checked_mul(BYTES)?;
        Some(self.scanline(line)?.get(..size)?.chunks_exact(BYTES))
    }

    /// GetWritableScanlineAs(), with the same width/padding boundary.
    pub fn scanline_pixels_mut<const BYTES: usize>(
        &mut self,
        line: i32,
    ) -> Option<std::slice::ChunksExactMut<'_, u8>> {
        let size = usize::try_from(self.width).ok()?.checked_mul(BYTES)?;
        Some(
            self.scanline_mut(line)?
                .get_mut(..size)?
                .chunks_exact_mut(BYTES),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_fill_paths_preserve_clipping_padding_guards_and_alpha() {
        for (format, bpp) in [
            (FxdibFormat::Bgr, 3),
            (FxdibFormat::Bgrx, 4),
            (FxdibFormat::Bgra, 4),
        ] {
            for width in [1usize, 3, 4, 7] {
                for padding in [0, 7] {
                    let pitch = width * bpp + padding;
                    for initial in [0u8, 165, 255] {
                        for color in [
                            0xffff_ffffu32,
                            0xff7f_7f7f,
                            0xff12_3456,
                            0x0012_3456,
                            0x7f12_3456,
                        ] {
                            for (left, top, w, h) in [
                                (0, 0, width as i32, 4),
                                (1, 1, width as i32 - 1, 2),
                                (-2, -1, 4, 3),
                                (width as i32 - 1, 3, 4, 4),
                                (0, 0, 0, 4),
                                (width as i32 + 1, 0, 2, 2),
                            ] {
                                let mut actual = vec![initial; 1 + pitch * 4 + 7];
                                let mut expected = actual.clone();
                                let a = u64::from(color >> 24);
                                let bytes = color.to_le_bytes();
                                for y in 0..4 {
                                    for x in 0..width {
                                        if a == 0
                                            || (x as i32) < left
                                            || x as i32 >= left + w
                                            || y < top
                                            || y >= top + h
                                        {
                                            continue;
                                        }
                                        let offset = 1 + y as usize * pitch + x * bpp;
                                        let pixel = &mut expected[offset..offset + bpp];
                                        let back = u64::from(initial);
                                        let union = back + a - back * a / 255;
                                        for c in 0..3 {
                                            pixel[c] = if a == 255
                                                || (format == FxdibFormat::Bgra && back == 0)
                                            {
                                                bytes[c]
                                            } else {
                                                let ratio = if format == FxdibFormat::Bgra {
                                                    a * 255 / union
                                                } else {
                                                    a
                                                };
                                                ((back * (255 - ratio)
                                                    + u64::from(bytes[c]) * ratio)
                                                    / 255)
                                                    as u8
                                            };
                                        }
                                        if bpp == 4 {
                                            pixel[3] = if format == FxdibFormat::Bgra {
                                                union as u8
                                            } else {
                                                255
                                            };
                                        }
                                    }
                                }
                                let mut view = CfxDibitmapData::from_buffer(
                                    width as i32,
                                    4,
                                    format,
                                    pitch as u32,
                                    &mut actual[1..1 + pitch * 4],
                                )
                                .unwrap();
                                assert!(view.composite_rect(left, top, w, h, color));
                                assert_eq!(
                                    actual, expected,
                                    "format={format:?} width={width} pitch={pitch} color={color:x}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn owned_bitmap_rejects_native_malloc_limit_before_allocating() {
        let pitch = (crate::fxcrt::fx_memory::MALLOC_SIZE_LIMIT - 4) as u32;
        // Pitch/height arithmetic is valid; only the owned allocation, including
        // the native four-byte tail, reaches the general allocator's limit.
        assert!(CfxDibitmap::calculate_pitch_and_size(1, 1, FxdibFormat::Bgra, pitch).is_some());
        assert!(CfxDibitmap::create(1, 1, FxdibFormat::Bgra, pitch).is_none());
    }

    #[test]
    fn bitmap_pitch_and_bounds_follow_pdfium() {
        let mut bitmap = CfxDibitmap::create(3, 2, FxdibFormat::Bgr, 0).unwrap();
        assert_eq!(bitmap.pitch(), 12);
        assert_eq!(bitmap.buffer().len(), 24);
        bitmap.scanline_mut(1).unwrap()[0] = 42;
        assert_eq!(bitmap.scanline(1).unwrap()[0], 42);
        assert!(bitmap.scanline(2).is_none());
        assert!(CfxDibitmap::create(0, 1, FxdibFormat::Bgra, 0).is_none());
        assert!(CfxDibitmap::create(3, 1, FxdibFormat::Bgr, 8).is_none());
        assert!(CfxDibitmap::create(i32::MAX, i32::MAX, FxdibFormat::Bgra, 0).is_none());
    }

    #[test]
    fn clear_and_rectangle_composition_handle_alpha_and_clipping() {
        let mut bitmap = CfxDibitmap::create(3, 2, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0x800000ff);
        assert_eq!(&bitmap.scanline(0).unwrap()[..4], &[255, 0, 0, 128]);
        assert!(bitmap.composite_rect(1, 0, 5, 1, 0x80ff0000));
        assert_eq!(&bitmap.scanline(0).unwrap()[4..8], &[85, 0, 170, 192]);
        assert_eq!(&bitmap.scanline(0).unwrap()[0..4], &[255, 0, 0, 128]);
        assert_eq!(&bitmap.scanline(1).unwrap()[4..8], &[255, 0, 0, 128]);
    }

    #[test]
    fn alpha_multiplier_converts_rgb_and_combines_bgra_mask() {
        let mut rgb = CfxDibitmap::create(2, 1, FxdibFormat::Bgr, 0).unwrap();
        rgb.scanline_mut(0).unwrap()[..6].copy_from_slice(&[10, 20, 30, 40, 50, 60]);
        assert!(rgb.multiply_alpha(0.5));
        assert_eq!(rgb.format(), FxdibFormat::Bgra);
        assert_eq!(
            &rgb.scanline(0).unwrap()[..8],
            &[10, 20, 30, 127, 40, 50, 60, 127]
        );

        let mut mask = CfxDibitmap::create(2, 1, FxdibFormat::EightBppMask, 0).unwrap();
        mask.scanline_mut(0).unwrap()[..2].copy_from_slice(&[255, 128]);
        assert!(rgb.multiply_alpha_mask(&mask));
        assert_eq!(
            &rgb.scanline(0).unwrap()[..8],
            &[10, 20, 30, 127, 40, 50, 60, 63]
        );

        let mut bgrx = CfxDibitmap::create(2, 1, FxdibFormat::Bgrx, 0).unwrap();
        bgrx.scanline_mut(0).unwrap()[..8].copy_from_slice(&[1, 2, 3, 0, 4, 5, 6, 0]);
        assert!(bgrx.multiply_alpha_mask(&mask));
        assert_eq!(bgrx.format(), FxdibFormat::Bgra);
        assert_eq!(
            &bgrx.scanline(0).unwrap()[..8],
            &[1, 2, 3, 255, 4, 5, 6, 128]
        );
    }

    #[test]
    fn normal_mask_composition_crops_source_and_destination() {
        let mut dest = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        let mut mask = CfxDibitmap::create(3, 1, FxdibFormat::EightBppMask, 0).unwrap();
        mask.scanline_mut(0).unwrap()[..3].copy_from_slice(&[0, 128, 255]);
        assert!(dest.composite_mask_normal(-1, 0, 3, 1, &mask, 0xffff0000, 0, 0, false));
        assert_eq!(
            &dest.scanline(0).unwrap()[..12],
            &[0, 0, 255, 128, 0, 0, 255, 255, 0, 0, 0, 0]
        );

        let mut bit_mask = CfxDibitmap::create(3, 1, FxdibFormat::OneBppMask, 0).unwrap();
        bit_mask.scanline_mut(0).unwrap()[0] = 0b1010_0000;
        dest.clear(0);
        assert!(dest.composite_mask_normal(0, 0, 3, 1, &bit_mask, 0xff00ff00, 0, 0, false));
        assert_eq!(
            &dest.scanline(0).unwrap()[..12],
            &[0, 255, 0, 255, 0, 0, 0, 0, 0, 255, 0, 255]
        );
    }

    #[test]
    fn glyph_mask_copy_expands_bits_and_unions_with_clipping() {
        let mut expanded = CfxDibitmap::create(10, 2, FxdibFormat::EightBppMask, 0).unwrap();
        assert!(expanded.populate_8bpp_mask_from_1bpp_span(
            &[0b1010_0001, 0b1100_0000, 0b0100_0000, 0b1000_0000],
            2,
        ));
        assert_eq!(
            &expanded.scanline(0).unwrap()[..10],
            &[255, 0, 255, 0, 0, 0, 0, 255, 255, 255]
        );
        assert_eq!(
            &expanded.scanline(1).unwrap()[..10],
            &[0, 255, 0, 0, 0, 0, 0, 0, 255, 0]
        );

        let mut glyph = CfxDibitmap::create(9, 1, FxdibFormat::OneBppMask, 0).unwrap();
        assert!(glyph.populate_from_span(&[0b1010_0000, 0b1000_0000], 2));
        let mut target = CfxDibitmap::create(10, 1, FxdibFormat::OneBppMask, 0).unwrap();
        target.composite_one_bpp_mask(2, 0, 9, 1, &glyph, 0, 0);
        assert_eq!(&target.scanline(0).unwrap()[..2], &[0b0010_1000, 0]);
        target.composite_one_bpp_mask(-1, 0, 9, 1, &glyph, 0, 0);
        assert_eq!(&target.scanline(0).unwrap()[..2], &[0b0110_1001, 0]);
    }

    #[test]
    fn same_format_transfer_overwrites_bits_and_crops_source() {
        let mut bits = CfxDibitmap::create(8, 1, FxdibFormat::OneBppMask, 0).unwrap();
        bits.scanline_mut(0).unwrap()[0] = 0xff;
        let mut source = CfxDibitmap::create(8, 1, FxdibFormat::OneBppMask, 0).unwrap();
        source.scanline_mut(0).unwrap()[0] = 0b1010_0000;
        assert!(bits.transfer_bitmap_same_format(8, 1, &source, 2, 0));
        assert_eq!(bits.scanline(0).unwrap()[0], 0b1000_0011);

        let mut rgb = CfxDibitmap::create(3, 1, FxdibFormat::Bgr, 0).unwrap();
        let mut source_rgb = CfxDibitmap::create(3, 1, FxdibFormat::Bgr, 0).unwrap();
        source_rgb.scanline_mut(0).unwrap()[..9].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert!(rgb.transfer_bitmap_same_format(3, 1, &source_rgb, -1, 0));
        assert_eq!(&rgb.scanline(0).unwrap()[..9], &[0, 0, 0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn direct_color_transfer_preserves_destination_alpha() {
        let mut source = CfxDibitmap::create(2, 1, FxdibFormat::Bgr, 0).unwrap();
        source.scanline_mut(0).unwrap()[..6].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        let mut destination = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
        destination.scanline_mut(0).unwrap()[..8].copy_from_slice(&[0, 0, 0, 80, 0, 0, 0, 90]);
        assert!(destination.transfer_bitmap_direct_color(2, 1, &source, 0, 0));
        assert_eq!(
            &destination.scanline(0).unwrap()[..8],
            &[1, 2, 3, 80, 4, 5, 6, 90]
        );

        let mut bgrx = CfxDibitmap::create(2, 1, FxdibFormat::Bgrx, 0).unwrap();
        bgrx.scanline_mut(0).unwrap()[..8].copy_from_slice(&[7, 8, 9, 17, 10, 11, 12, 18]);
        assert!(source.transfer_bitmap_direct_color(2, 1, &bgrx, 0, 0));
        assert_eq!(&source.scanline(0).unwrap()[..6], &[7, 8, 9, 10, 11, 12]);
    }

    #[test]
    fn clip_to_shifts_one_bit_pixels_and_retains_palette() {
        let mut source = CfxDibitmap::create(10, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        source.take_palette(vec![0xff112233, 0xff445566]);
        source.scanline_mut(0).unwrap()[..2].copy_from_slice(&[0b1010_1000, 0b1100_0000]);
        let clipped = source.clip_to(FxRect::new(3, 0, 9, 1)).unwrap();
        assert_eq!((clipped.width(), clipped.height()), (6, 1));
        assert_eq!(clipped.scanline(0).unwrap()[0] & 0xfc, 0b0100_0100);
        assert_eq!(clipped.palette(), source.palette());
        assert!(source.clip_to(FxRect::new(20, 0, 21, 1)).is_none());
    }

    #[test]
    fn normal_bitmap_composition_crops_and_merges_alpha() {
        let mut destination = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        destination.clear(0x800a141e);
        let mut source = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        source.scanline_mut(0).unwrap()[..12]
            .copy_from_slice(&[100, 120, 140, 128, 40, 50, 60, 255, 1, 2, 3, 0]);
        assert!(destination.composite_bitmap_normal(-1, 0, 3, 1, &source, 0, 0));
        assert_eq!(&destination.scanline(0).unwrap()[..4], &[40, 50, 60, 255]);
        assert_eq!(&destination.scanline(0).unwrap()[4..8], &[30, 20, 10, 128]);
        assert_eq!(&destination.scanline(0).unwrap()[8..12], &[30, 20, 10, 128]);
    }

    #[test]
    fn indexed_bitmap_composition_requires_an_explicit_palette() {
        let mut destination = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        let mut source = CfxDibitmap::create(4, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        source.take_palette(vec![0xff0000ff, 0xff00ff00]);
        source.scanline_mut(0).unwrap()[0] = 0b1010_0000;
        assert!(destination.composite_bitmap_normal(-1, 0, 4, 1, &source, 0, 0));
        assert_eq!(
            &destination.scanline(0).unwrap()[..12],
            &[255, 0, 0, 255, 0, 255, 0, 255, 255, 0, 0, 255]
        );

        let mut gray = CfxDibitmap::create(2, 1, FxdibFormat::EightBppRgb, 0).unwrap();
        gray.scanline_mut(0).unwrap()[..2].copy_from_slice(&[17, 200]);
        assert!(!destination.composite_bitmap_normal(0, 0, 2, 1, &gray, 0, 0));
        gray.build_palette();
        assert!(destination.composite_bitmap_normal(0, 0, 2, 1, &gray, 0, 0));
        assert_eq!(
            &destination.scanline(0).unwrap()[..8],
            &[17, 17, 17, 255, 200, 200, 200, 255]
        );
    }

    #[test]
    fn mask_conversion_expands_bits_and_grays_palette_colors() {
        let mut bits = CfxDibitmap::create(3, 1, FxdibFormat::OneBppMask, 0).unwrap();
        bits.scanline_mut(0).unwrap()[0] = 0b1010_0000;
        assert!(bits.convert_to_8bpp_mask());
        assert_eq!(bits.format(), FxdibFormat::EightBppMask);
        assert_eq!(&bits.scanline(0).unwrap()[..3], &[255, 0, 255]);

        let mut indexed = CfxDibitmap::create(2, 1, FxdibFormat::EightBppRgb, 0).unwrap();
        indexed.take_palette(vec![0xff000000, 0xff00ff00]);
        indexed.scanline_mut(0).unwrap()[..2].copy_from_slice(&[1, 0]);
        assert!(indexed.convert_to_8bpp_mask());
        assert_eq!(
            indexed.scanline(0).unwrap()[0],
            rgb_to_gray(0, 255, 0) as u8
        );
        assert_eq!(indexed.scanline(0).unwrap()[1], 0);
        assert!(indexed.palette().is_empty());
    }

    #[test]
    fn bgr_conversion_uses_palette_and_discards_source_alpha() {
        let mut bits = CfxDibitmap::create(2, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        bits.take_palette(vec![0xff112233, 0xff445566]);
        bits.scanline_mut(0).unwrap()[0] = 0b0100_0000;
        assert!(bits.convert_to_bgr());
        assert_eq!(
            &bits.scanline(0).unwrap()[..6],
            &[0x33, 0x22, 0x11, 0x66, 0x55, 0x44]
        );
        assert!(bits.palette().is_empty());

        let mut bgra = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        bgra.scanline_mut(0).unwrap()[..4].copy_from_slice(&[7, 8, 9, 0]);
        assert!(bgra.convert_to_bgr());
        assert_eq!(&bgra.scanline(0).unwrap()[..3], &[7, 8, 9]);

        let mut mask = CfxDibitmap::create(1, 1, FxdibFormat::EightBppMask, 0).unwrap();
        mask.scanline_mut(0).unwrap()[0] = 42;
        assert!(mask.convert_to_bgra());
        assert_eq!(&mask.scanline(0).unwrap()[..4], &[42, 42, 42, 255]);
    }
    #[test]
    fn empty_copy_and_failed_recreation_preserve_bitmap_state_rules() {
        let mut source = CfxDibitmap::create(1, 1, FxdibFormat::Bgr, 3).unwrap();
        source.buffer_mut().copy_from_slice(&[11, 22, 33]);
        let mut copy = CfxDibitmap::default();
        assert!(copy.copy_from(&source));
        assert_eq!(copy.buffer(), &[11, 22, 33, 0]);
        assert!(!copy.copy_from(&source));
        assert_eq!(copy.buffer().len(), 4);
        assert_eq!(copy.buffer.len(), 8);
        assert!(copy.scanline(1).is_none());
        let mut indexed = CfxDibitmap::create(1, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        indexed.take_palette(vec![0xff112233, 0xff445566]);
        assert!(!indexed.create_in_place(0, 1, FxdibFormat::Bgra, 0));
        assert_eq!(
            (indexed.width(), indexed.height(), indexed.pitch()),
            (0, 0, 0)
        );
        assert_eq!(indexed.format(), FxdibFormat::Bgra);
        assert_eq!(indexed.palette(), &[0xff112233, 0xff445566]);
        assert!(indexed.buffer().is_empty());
        assert!(!indexed.composite_rect(0, 0, 1, 1, 0));
    }

    #[test]
    fn one_bit_rectangles_keep_pinned_edge_flags_inside_the_allocation_guard() {
        let mut bitmap = CfxDibitmap::create(32, 2, FxdibFormat::OneBppMask, 0).unwrap();
        assert!(bitmap.composite_rect(0, 0, 32, 1, 0xffffffff));
        assert_eq!(bitmap.buffer(), &[255, 255, 255, 255, 255, 0, 0, 0]);
        bitmap.clear(0);
        assert!(bitmap.composite_rect(0, 1, 32, 1, 0xffffffff));
        assert_eq!(bitmap.buffer()[4..], [255; 4]);
        assert_eq!(bitmap.buffer[8], 255);
        assert_eq!(bitmap.buffer().len(), 8);
        bitmap.clear(0);
        assert_eq!(bitmap.buffer[8], 255); // Clear exposes only the logical span.
        let mut palette = CfxDibitmap::create(8, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        palette.take_palette(vec![0xffff0000, 0xff0000ff]);
        palette.buffer_mut()[0] = 255;
        assert!(palette.composite_rect(2, 0, 3, 1, 0xffff0000));
        assert_eq!(palette.buffer()[0], 0xc7); // Palette index zero clears the selected bits.
    }

    #[test]
    fn unequal_transfer_converts_gray_with_clipping_and_checks_destination_palette() {
        let mut source = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        source
            .buffer_mut()
            .copy_from_slice(&[0, 0, 255, 7, 0, 255, 0, 8, 255, 0, 0, 9]);
        let mut dest = CfxDibitmap::create(5, 1, FxdibFormat::EightBppRgb, 0).unwrap();
        dest.buffer_mut().fill(170);
        assert!(dest.transfer_bitmap(3, 1, &source, -1, 0));
        assert_eq!(&dest.buffer()[..5], &[170, 76, 150, 170, 170]);
        dest.build_palette();
        assert!(!dest.transfer_bitmap(3, 1, &source, 0, 0));
        assert_eq!(&dest.buffer()[..5], &[170, 76, 150, 170, 170]);
    }

    #[test]
    fn color_scale_keeps_source_palette_polarity_and_direct_alpha_rules() {
        let mut indexed = CfxDibitmap::create(1, 1, FxdibFormat::EightBppRgb, 0).unwrap();
        indexed.convert_color_scale(false);
        assert!(indexed.palette().is_empty());
        indexed.set_palette_argb(0, 0x00112233);
        indexed.convert_color_scale(false);
        assert_eq!(indexed.palette_argb(0), 0xffe1e1e1);
        let mut direct = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        direct
            .buffer_mut()
            .copy_from_slice(&[0x33, 0x22, 0x11, 128]);
        direct.convert_color_scale(false);
        assert_eq!(direct.buffer(), &[30, 30, 30, 128]);
    }
    #[test]
    fn borrowed_bitmap_writes_original_pixels_and_retains_padding() {
        let mut pixels = [0xaa; 12];
        let original = pixels.as_mut_ptr();
        let mut view =
            CfxDibitmapData::from_buffer(1, 2, FxdibFormat::Bgr, 6, &mut pixels).unwrap();
        assert!(view.is_borrowed());
        assert_eq!(view.buffer_mut().as_mut_ptr(), original);
        assert!(view.composite_rect(0, 0, 1, 2, 0xffff0000));
        assert_eq!(
            view.buffer(),
            &[0, 0, 255, 0xaa, 0xaa, 0xaa, 0, 0, 255, 0xaa, 0xaa, 0xaa]
        );
        assert!(view.into_owned_if_allocated().is_none());
        assert_eq!(
            pixels,
            [0, 0, 255, 0xaa, 0xaa, 0xaa, 0, 0, 255, 0xaa, 0xaa, 0xaa]
        );
        assert!(CfxDibitmapData::from_buffer(2, 2, FxdibFormat::Bgra, 0, &mut pixels).is_none());
    }

    #[test]
    fn borrowed_bitmap_conversion_moves_to_an_owned_allocation() {
        let mut pixels = [11, 22, 33];
        let original = pixels.as_mut_ptr();
        let mut view =
            CfxDibitmapData::from_buffer(1, 1, FxdibFormat::Bgr, 3, &mut pixels).unwrap();
        assert!(view.convert_to_bgra());
        assert!(!view.is_borrowed());
        assert_ne!(view.buffer_mut().as_mut_ptr(), original);
        let owned = view.into_owned_if_allocated().unwrap();
        assert_eq!(pixels, [11, 22, 33]);
        assert_eq!(owned.buffer(), &[11, 22, 33, 255]);
    }

    #[test]
    fn borrowed_scanline_sources_stretch_without_copying_source_storage() {
        use std::rc::Rc;
        let mut pixels = [255];
        {
            let view =
                CfxDibitmapData::from_buffer(1, 1, FxdibFormat::EightBppMask, 1, &mut pixels)
                    .unwrap();
            let source = Rc::new(view);
            let result = cfx_dibbase::stretch_to(source, 2, 1, Default::default(), None).unwrap();
            assert_eq!(&result.borrow().buffer()[..2], &[255, 255]);
        }
        assert_eq!(pixels, [255]);
    }

    #[test]
    fn borrowed_one_bit_right_edges_require_an_accessible_guard_byte() {
        let mut unguarded = [0];
        let mut view =
            CfxDibitmapData::from_buffer(8, 1, FxdibFormat::OneBppMask, 1, &mut unguarded).unwrap();
        assert!(!view.composite_rect(0, 0, 8, 1, 0xffffffff));
        assert_eq!(view.buffer(), &[0]);
        let mut guarded = [0; 2];
        let mut view =
            CfxDibitmapData::from_buffer(8, 1, FxdibFormat::OneBppMask, 1, &mut guarded).unwrap();
        assert!(view.composite_rect(0, 0, 8, 1, 0xffffffff));
        assert_eq!(view.buffer(), &[255]);
        drop(view);
        assert_eq!(guarded, [255, 255]);
    }
}

#[cfg(test)]
#[path = "cfx_dibitmap_unittest.rs"]
mod upstream_tests;
