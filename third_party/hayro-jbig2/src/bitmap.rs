//! Bitmap representation for JBIG2 decoding.
//!
//! "The variable whose value is the result of this decoding procedure is shown
//! in Table 3." (6.2.3)
//!
//! "GBREG - The decoded region bitmap." (Table 3)

// PDFium external-canvas extensions are translated from
// core/fxcodec/jbig2/jbig2_image.cpp.
// Copyright 2014 The PDFium Authors. Original code copyright 2014 Foxit
// Software Inc. http://www.foxitsoftware.com
// These extensions use the BSD-style license in ../LICENSE-PDFIUM.

use crate::decode::CombinationOperator;
use crate::error::{OverflowError, Result, bail};
use alloc::vec;
use alloc::vec::Vec;

// To guard better against malicious files, we limit bitmap dimensions
// to `u16::MAX`, which should be more than enough.
pub(crate) const MAX_DIMENSION: u32 = u16::MAX as u32;

/// The underlying word type for packed pixel storage.
/// Change this to `u32` to switch back to 32-bit words.
pub(crate) type Word = u32;

pub(crate) const WORD_BITS: u32 = Word::BITS;
pub(crate) const WORD_BYTES: u32 = WORD_BITS / u8::BITS;
pub(crate) const WORD_SHIFT: u32 = WORD_BITS - 1;

/// A decoded bitmap with position information.
///
/// "Pixels decoded by the MMR decoder having the value 'black' shall be treated
/// as having the value 1. Pixels decoded by the MMR decoder having the value
/// 'white' shall be treated as having the value 0." (6.2.6)
#[derive(Debug, Clone, Default)]
pub(crate) struct Bitmap {
    /// Native null symbol slot, distinct from an owned empty image.
    pub(crate) is_null: bool,
    pub(crate) pdfium: bool,
    pub(crate) expand_background: Option<bool>,
    /// Width in pixels.
    pub(crate) width: u32,
    /// Height in pixels.
    pub(crate) height: u32,
    /// Number of words per row.
    pub(crate) stride: u32,
    /// Packed pixel data, one bit per pixel, row-major order.
    /// Each row is padded to a word boundary.
    pub(crate) data: Vec<Word>,
    /// "This four-byte field gives the horizontal offset in pixels of the bitmap
    /// encoded in this segment relative to the page bitmap." (7.4.1.3)
    pub(crate) x_location: u32,
    /// "This four-byte field gives the vertical offset in pixels of the bitmap
    /// encoded in this segment relative to the page bitmap." (7.4.1.4)
    pub(crate) y_location: u32,
}

impl Bitmap {
    /// CJBig2_Image::IsValidImageSize, used by non-generic region headers.
    pub(crate) fn is_valid_image_size(width: u32, height: u32) -> bool {
        width > 0 && width <= MAX_DIMENSION && height > 0 && height <= MAX_DIMENSION
    }
    /// CJBig2_Image's owned allocation predicate, independent of the ordinary
    /// 65535 region limit. Invalid construction retains a zero-sized image.
    pub(crate) fn is_valid_owned_size(width: u32, height: u32) -> bool {
        const MAX_IMAGE_PIXELS: u32 = i32::MAX as u32 - 31;
        if width == 0 || height == 0 || width > MAX_IMAGE_PIXELS {
            return false;
        }
        height <= MAX_IMAGE_PIXELS / (width.div_ceil(WORD_BITS) * WORD_BITS)
    }

    pub(crate) fn new_null() -> Self {
        Self {
            is_null: true,
            pdfium: true,
            ..Self::default()
        }
    }

    pub(crate) fn new_pdfium(width: u32, height: u32) -> Self {
        let mut image = Self::default();
        let _ = image.reinitialize_canvas(width, height, false);
        image.pdfium = true;
        image
    }

    /// Region procedures require has_data() after constructing their output.
    pub(crate) fn new_region(
        width: u32,
        height: u32,
        x: u32,
        y: u32,
        background: bool,
        pdfium: bool,
    ) -> Result<Self> {
        if !pdfium {
            return Self::new_with(width, height, x, y, background);
        }
        let mut image = Self::new_pdfium(width, height);
        if image.data.is_empty() {
            bail!(OverflowError::BitmapDimension);
        }
        image.x_location = x;
        image.y_location = y;
        if background {
            image.data.fill(!0);
        }
        Ok(image)
    }

    /// GRRD returns a constructed image without decoding when its ordinary
    /// dimension predicate fails. Valid dimensions still require pixel storage.
    pub(crate) fn new_refinement(width: u32, height: u32, pdfium: bool) -> Result<Self> {
        if !pdfium {
            return Self::new(width, height);
        }
        let image = Self::new_pdfium(width, height);
        if Self::is_valid_image_size(width, height) && image.data.is_empty() {
            bail!(OverflowError::BitmapDimension);
        }
        Ok(image)
    }

    /// Create a new bitmap filled with white pixels (false).
    ///
    /// The bitmap is positioned at (0, 0).
    pub(crate) fn new(width: u32, height: u32) -> Result<Self> {
        Self::new_with(width, height, 0, 0, false)
    }

    /// Create a new bitmap with full configuration.
    pub(crate) fn new_with(
        width: u32,
        height: u32,
        x_location: u32,
        y_location: u32,
        default_pixel: bool,
    ) -> Result<Self> {
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            bail!(OverflowError::BitmapDimension);
        }

        let stride = width.div_ceil(WORD_BITS);
        let default_word: Word = if default_pixel { !0 } else { 0 };
        let data = vec![default_word; stride as usize * height as usize];
        Ok(Self {
            is_null: false,
            pdfium: false,
            expand_background: None,
            width,
            height,
            stride,
            data,
            x_location,
            y_location,
        })
    }

    pub(crate) fn reinitialize(
        &mut self,
        width: u32,
        height: u32,
        default_pixel: bool,
    ) -> Result<()> {
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(OverflowError::BitmapDimension.into());
        }

        self.reinitialize_storage(width, height, default_pixel);
        Ok(())
    }

    /// CJBig2_Image's external-buffer dimensions are bounded by signed stride
    /// arithmetic and total pixels, rather than the region dimension limit.
    pub(crate) fn reinitialize_canvas(
        &mut self,
        width: u32,
        height: u32,
        default_pixel: bool,
    ) -> Result<()> {
        if !Self::is_valid_owned_size(width, height) {
            return Err(OverflowError::BitmapDimension.into());
        }
        self.reinitialize_storage(width, height, default_pixel);
        self.pdfium = true;
        Ok(())
    }

    fn reinitialize_storage(&mut self, width: u32, height: u32, default_pixel: bool) {
        self.is_null = false;
        self.pdfium = false;
        self.expand_background = None;
        let stride = width.div_ceil(WORD_BITS);
        let default_word: Word = if default_pixel { !0 } else { 0 };
        self.width = width;
        self.height = height;
        self.stride = stride;
        self.x_location = 0;
        self.y_location = 0;
        self.data.clear();
        self.data
            .resize(stride as usize * height as usize, default_word);
    }

    /// CJBig2_Image::SubImage copies source storage, including row padding.
    /// An origin outside the source produces a white image, without clipping
    /// the requested origin into the page. This is an explicit native copy.
    pub(crate) fn pdfium_subimage(&self, x: i32, y: i32, width: u32, height: u32) -> Result<Self> {
        let mut image = Self::new_pdfium(width, height);
        if image.data.is_empty() || self.data.is_empty() {
            return Ok(image);
        }
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return Ok(image);
        }
        let x = x as u32;
        let y = y as u32;
        let first_word = x / WORD_BITS;
        let shift = x % WORD_BITS;
        let words = image.stride.min(self.stride - first_word);
        let lines = height.min(self.height - y);
        for row in 0..lines {
            for word in 0..words {
                let saved = self.get_word(y + row, first_word + word);
                let value = if shift == 0 {
                    saved
                } else {
                    (saved << shift)
                        | (self.get_word(y + row, first_word + word + 1) >> (WORD_BITS - shift))
                };
                image.data[(row * image.stride + word) as usize] = value;
            }
        }
        Ok(image)
    }

    /// Get a pixel value at (x, y).
    #[inline]
    pub(crate) fn get_pixel(&self, x: u32, y: u32) -> u8 {
        if x >= self.width || y >= self.height {
            return 0;
        }

        let word_idx = (y * self.stride + x / WORD_BITS) as usize;
        let bit_pos = WORD_SHIFT - (x % WORD_BITS);
        ((self.data[word_idx] >> bit_pos) & 1) as u8
    }

    /// Set a pixel value at (x, y).
    #[inline]
    pub(crate) fn set_pixel(&mut self, x: u32, y: u32, value: u8) {
        if x >= self.width || y >= self.height {
            return;
        }

        let word_idx = (y * self.stride + x / WORD_BITS) as usize;
        let bit_pos = WORD_SHIFT - (x % WORD_BITS);
        if self.pdfium {
            let mask = 1 << bit_pos;
            if value != 0 {
                self.data[word_idx] |= mask;
            } else {
                self.data[word_idx] &= !mask;
            }
        } else {
            self.data[word_idx] |= (value as Word) << bit_pos;
        }
    }

    /// Combine another bitmap into this one at a specific location.
    ///
    /// "These operators describe how the segment's bitmap is to be combined with
    /// the page bitmap." (7.4.1.5)
    ///
    /// Pixels outside the destination bitmap are ignored.
    pub(crate) fn combine(&mut self, other: &Self, x: i32, y: i32, operator: CombinationOperator) {
        self.combine_completed_rows(other, x, y, operator, other.height);
    }

    /// Static GRD's replace rectangle includes only fully decoded rows.
    pub(crate) fn combine_completed_rows(
        &mut self,
        other: &Self,
        x: i32,
        y: i32,
        operator: CombinationOperator,
        rows: u32,
    ) {
        self.combine_row_range(other, x, y, operator, 0, rows);
    }

    /// Compose only this continuation's replace rectangle. In particular XOR
    /// and XNOR must never be applied twice to previously completed rows.
    pub(crate) fn combine_row_range(
        &mut self,
        other: &Self,
        x: i32,
        y: i32,
        operator: CombinationOperator,
        first_row: u32,
        rows: u32,
    ) {
        // Only PDFium's internally owned striped global page may expand.
        // The supplied PDF image canvas keeps expand_background=None.
        if let Some(background) = self.expand_background {
            if let Some(bottom) = y.checked_add(other.height as i32) {
                let max_pixels = i32::MAX as u32 - 31;
                if bottom > self.height as i32
                    && self.stride > 0
                    && bottom as u32 <= max_pixels / (self.stride * WORD_BITS)
                {
                    self.data.resize(
                        bottom as usize * self.stride as usize,
                        if background { !0 } else { 0 },
                    );
                    self.height = bottom as u32;
                }
            }
        }
        // CJBig2_Image::ComposeToInternal rejects distant coordinates before
        // clipping. Do not narrow or add an arbitrarily large region width.
        if self.pdfium && (!(-1048576..=1048576).contains(&x) || !(-1048576..=1048576).contains(&y))
        {
            return;
        }
        let dest_x_start = x.max(0);
        let dest_x_end = (i64::from(x) + i64::from(other.width)).min(i64::from(self.width));
        if i64::from(dest_x_start) >= dest_x_end {
            return;
        }
        let src_x_start = (i64::from(dest_x_start) - i64::from(x)) as u32;
        let dest_x_start = dest_x_start as u32;
        let dest_x_end = dest_x_end as u32;
        let src_y_start = ((-i64::from(y)).max(0) as u32).max(first_row);
        let src_y_end =
            (i64::from(self.height) - i64::from(y)).min(i64::from(rows.min(other.height)));
        if i64::from(src_y_start) >= src_y_end {
            return;
        }
        // Native clips both axes before walking rows. Keep work proportional
        // to visible rows even for large MMR regions outside the canvas.
        for src_y in src_y_start..src_y_end as u32 {
            let dest_y = (i64::from(y) + i64::from(src_y)) as u32;
            let first_word = dest_x_start / WORD_BITS;
            let last_word = (dest_x_end - 1) / WORD_BITS;

            for word_idx in first_word..=last_word {
                let word_start_x = word_idx * WORD_BITS;
                let word_end_x = word_start_x + WORD_BITS;

                let px_start = dest_x_start.max(word_start_x);
                let px_end = dest_x_end.min(word_end_x);

                let bit_start = px_start - word_start_x;
                let bit_end = px_end - word_start_x;

                let mask: Word = if bit_end == WORD_BITS {
                    !0 >> bit_start
                } else {
                    (!0 >> bit_start) & !(!0 >> bit_end)
                };

                let src_x_for_range = src_x_start + (px_start - dest_x_start);
                let src_word_idx = src_x_for_range / WORD_BITS;
                let src_bit_offset = src_x_for_range % WORD_BITS;

                let src_word1 = other.get_word(src_y, src_word_idx);
                let src_word2 = other.get_word(src_y, src_word_idx + 1);

                let src_raw = if src_bit_offset == 0 {
                    src_word1
                } else {
                    (src_word1 << src_bit_offset) | (src_word2 >> (WORD_BITS - src_bit_offset))
                };
                let src_aligned = src_raw >> bit_start;

                let dest_idx = (dest_y * self.stride + word_idx) as usize;
                let dest_word = self.data[dest_idx];

                let result = match operator {
                    CombinationOperator::Or => dest_word | src_aligned,
                    CombinationOperator::And => dest_word & src_aligned,
                    CombinationOperator::Xor => dest_word ^ src_aligned,
                    CombinationOperator::Xnor => !(dest_word ^ src_aligned),
                    CombinationOperator::Replace => src_aligned,
                };

                self.data[dest_idx] = (dest_word & !mask) | (result & mask);
            }
        }
    }

    #[inline]
    pub(crate) fn get_word(&self, row: u32, word_idx: u32) -> Word {
        if row >= self.height || word_idx >= self.stride {
            return 0;
        }

        self.data[(row * self.stride + word_idx) as usize]
    }
}

#[cfg(test)]
#[path = "jbig2_image_unittest.rs"]
mod upstream_tests;
