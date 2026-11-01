// Adapted from PDFium's Anti-Grain Geometry 2.3 for pdfoxium.
// Distributed under the original permissive license notice below.
//
// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! `third_party/agg23/agg_pixfmt_gray.h`'s gray8 pixel format.

use super::agg_color_gray::Gray8;
use super::agg_rendering_buffer::{RenderingBuffer, RowData, SpanData};

pub struct BlenderGray8;

impl BlenderGray8 {
    pub fn blend_pix(pixel: &mut u8, color: u32, alpha: u32, _cover: u32) {
        let old = u32::from(*pixel);
        let result = color
            .wrapping_sub(old)
            .wrapping_mul(alpha)
            .wrapping_add(old << Gray8::BASE_SHIFT)
            >> Gray8::BASE_SHIFT;
        *pixel = result as u8;
    }
}

pub struct PixelFormatsGray<'r, 'b, const STEP: usize = 1, const OFFSET: usize = 0> {
    buffer: &'r mut RenderingBuffer<'b>,
}

pub type PixfmtGray8<'r, 'b> = PixelFormatsGray<'r, 'b, 1, 0>;

impl<'r, 'b, const STEP: usize, const OFFSET: usize> PixelFormatsGray<'r, 'b, STEP, OFFSET> {
    pub fn new(buffer: &'r mut RenderingBuffer<'b>) -> Self {
        assert!(STEP > 0 && OFFSET < STEP);
        Self { buffer }
    }

    pub fn width(&self) -> u32 {
        self.buffer.width()
    }

    pub fn height(&self) -> u32 {
        self.buffer.height()
    }

    fn offset(x: i32) -> Option<usize> {
        usize::try_from(x)
            .ok()?
            .checked_mul(STEP)?
            .checked_add(OFFSET)
    }

    pub fn pixel(&self, x: i32, y: i32) -> Option<Gray8> {
        let row = self.buffer.row(u32::try_from(y).ok()?)?;
        Some(Gray8::opaque(*row.get(Self::offset(x)?)? as u32))
    }

    pub fn row(&self, x: i32, y: i32) -> RowData<'_> {
        let ptr = u32::try_from(y)
            .ok()
            .and_then(|y| self.buffer.row(y))
            .and_then(|row| row.get(Self::offset(x)?..));
        RowData::new(x, self.width().wrapping_sub(1) as i32, ptr)
    }

    pub fn span(&mut self, x: i32, y: i32, len: u32) -> SpanData<'_> {
        let ptr = u32::try_from(y)
            .ok()
            .and_then(|y| self.buffer.row_mut(y))
            .and_then(|row| {
                let start = Self::offset(x)?;
                let end = if len == 0 {
                    start
                } else {
                    start
                        .checked_add(((len - 1) as usize).checked_mul(STEP)?)?
                        .checked_add(1)?
                };
                row.get_mut(start..end)
            });
        SpanData::new(x, len, ptr)
    }

    fn copy_or_blend(pixel: &mut u8, color: Gray8, cover: Option<u8>) {
        if color.a == 0 {
            return;
        }
        let alpha = cover.map_or(u32::from(color.a), |cover| {
            (u32::from(color.a) * (u32::from(cover) + 1)) >> Gray8::BASE_SHIFT
        });
        if alpha == Gray8::BASE_MASK {
            *pixel = color.v;
        } else {
            BlenderGray8::blend_pix(pixel, u32::from(color.v), alpha, cover.unwrap_or(0).into());
        }
    }

    pub fn copy_pixel(&mut self, x: i32, y: i32, color: Gray8) {
        let row = self.buffer.row_mut(y as u32).expect("row out of range");
        row[Self::offset(x).expect("pixel out of range")] = color.v;
    }

    pub fn blend_pixel(&mut self, x: i32, y: i32, color: Gray8, cover: u8) {
        let row = self.buffer.row_mut(y as u32).expect("row out of range");
        let pixel = &mut row[Self::offset(x).expect("pixel out of range")];
        Self::copy_or_blend(pixel, color, Some(cover));
    }

    pub fn copy_hline(&mut self, x: i32, y: i32, len: u32, color: Gray8) {
        let row = self.buffer.row_mut(y as u32).expect("row out of range");
        let start = Self::offset(x).expect("pixel out of range");
        for index in 0..len as usize {
            row[start + index * STEP] = color.v;
        }
    }

    pub fn blend_hline(&mut self, x: i32, y: i32, len: u32, color: Gray8, cover: u8) {
        if color.a == 0 {
            return;
        }
        let row = self.buffer.row_mut(y as u32).expect("row out of range");
        let start = Self::offset(x).expect("pixel out of range");
        let alpha = (u32::from(color.a) * (u32::from(cover) + 1)) >> Gray8::BASE_SHIFT;
        for index in 0..len as usize {
            let pixel = &mut row[start + index * STEP];
            if alpha == Gray8::BASE_MASK {
                *pixel = color.v;
            } else {
                BlenderGray8::blend_pix(pixel, u32::from(color.v), alpha, cover.into());
            }
        }
    }

    pub fn blend_solid_hspan(&mut self, x: i32, y: i32, color: Gray8, covers: &[u8]) {
        if color.a == 0 {
            return;
        }
        let row = self.buffer.row_mut(y as u32).expect("row out of range");
        let start = Self::offset(x).expect("pixel out of range");
        if STEP == 1 && !covers.is_empty() {
            // Prove the contiguous span once so LLVM can vectorize the blend
            // without a destination bounds check inside each pixel update.
            let pixels = &mut row[start..][..covers.len()];
            // Only bits 8..15 survive the original u32 blend's shift and
            // byte conversion. Wrapping u16 arithmetic preserves those bits
            // while allowing twice as many SIMD lanes. Alpha products fit u16.
            let blend = |old: u8, alpha: u16| -> u8 {
                let old = u16::from(old);
                (u16::from(color.v)
                    .wrapping_sub(old)
                    .wrapping_mul(alpha)
                    .wrapping_add(old << Gray8::BASE_SHIFT)
                    >> Gray8::BASE_SHIFT) as u8
            };
            if color.a == 255 {
                for (pixel, &cover) in pixels.iter_mut().zip(covers) {
                    *pixel = if cover == 255 {
                        color.v
                    } else {
                        blend(*pixel, u16::from(cover))
                    };
                }
            } else {
                // A nonopaque color never reaches the direct-copy alpha=255
                // case. Transparent colors already returned above.
                for (pixel, &cover) in pixels.iter_mut().zip(covers) {
                    let alpha = (u16::from(color.a) * (u16::from(cover) + 1)) >> 8;
                    *pixel = blend(*pixel, alpha);
                }
            }
            return;
        }
        for (index, &cover) in covers.iter().enumerate() {
            Self::copy_or_blend(&mut row[start + index * STEP], color, Some(cover));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_gray_spans_preserve_all_alpha_cover_pairs_and_guards() {
        let covers: Vec<u8> = (0..=255).collect();
        for alpha in 0..=255 {
            for value in [0u32, 1, 127, 254, 255] {
                for length in [1, 7, 15, 16, 17, 31, 32, 33, 63, 64, 65, 255, 256] {
                    let covers = &covers[..length];
                    let mut data: Vec<u8> = (0..264).map(|x| (x * 67 + 29) as u8).collect();
                    let mut expected = data.clone();
                    for (pixel, &cover) in expected[3..3 + length].iter_mut().zip(covers) {
                        let effective_alpha = alpha * (u32::from(cover) + 1) >> 8;
                        if alpha != 0 {
                            *pixel = if effective_alpha == 255 {
                                value as u8
                            } else {
                                let old = u32::from(*pixel);
                                (value
                                    .wrapping_sub(old)
                                    .wrapping_mul(effective_alpha)
                                    .wrapping_add(old << 8)
                                    >> 8) as u8
                            };
                        }
                    }
                    let mut buffer = RenderingBuffer::from_buffer(&mut data, 264, 1, 264).unwrap();
                    let mut pixels = PixfmtGray8::new(&mut buffer);
                    pixels.blend_solid_hspan(3, 0, Gray8::new(value, alpha), covers);
                    assert_eq!(
                        data, expected,
                        "alpha={alpha}, value={value}, length={length}"
                    );
                }
            }
        }
        let mut data = [91u8; 8];
        let mut buffer = RenderingBuffer::from_buffer(&mut data, 8, 1, 8).unwrap();
        let mut pixels = PixfmtGray8::new(&mut buffer);
        // Empty spans never inspect the destination start, as in the old loop.
        pixels.blend_solid_hspan(i32::MAX, 0, Gray8::opaque(255), &[]);
        assert_eq!(data, [91; 8]);
    }

    #[test]
    fn gray_pixels_copy_and_blend_with_agg_cover_rules() {
        let mut data = [100u8; 8];
        let mut buffer = RenderingBuffer::from_buffer(&mut data, 4, 2, 4).unwrap();
        let mut pixels = PixfmtGray8::new(&mut buffer);
        pixels.copy_pixel(0, 0, Gray8::opaque(200));
        assert_eq!(pixels.pixel(0, 0), Some(Gray8::opaque(200)));
        pixels.blend_pixel(1, 0, Gray8::new(200, 128), 255);
        assert_eq!(pixels.pixel(1, 0).unwrap().v, 150);
        pixels.copy_hline(0, 1, 4, Gray8::opaque(20));
        pixels.blend_hline(0, 1, 1, Gray8::opaque(220), 0);
        assert_eq!(pixels.pixel(0, 1).unwrap().v, 20);
        pixels.blend_solid_hspan(1, 1, Gray8::opaque(220), &[255, 128, 0]);
        assert_eq!(pixels.pixel(1, 1).unwrap().v, 220);
        assert_eq!(pixels.pixel(2, 1).unwrap().v, 120);
        assert_eq!(pixels.row(1, 1).ptr.unwrap()[0], 220);
        assert_eq!(pixels.span(1, 1, 2).ptr.unwrap().len(), 2);
    }
}
