// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! RGBA bitmap -> ordinary egui images. No custom GPU upload or texture atlas.
use eframe::egui::{Color32, ColorImage};
use pdfoxium::{Bitmap, DeviceRect, PixelFormat};

/// One alpha/alignment check per rendered RGBA region, then one Color32
/// allocation per output texture. Source rectangles are relative to the region.
/// Color32's alignment is 4, so ownership cannot safely transfer from Bitmap's
/// Vec<u8> (allocation alignment 1); copy once into its final allocation.
pub struct Converter<'a> {
    bitmap: &'a Bitmap,
    opaque: bool,
    aligned: Option<&'a [Color32]>,
}
impl<'a> Converter<'a> {
    pub fn new(bitmap: &'a Bitmap) -> Self {
        assert_eq!(bitmap.format(), PixelFormat::Rgba8);
        let opaque = bitmap
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[3] == 255);
        let aligned = if opaque && bitmap.stride().is_multiple_of(4) {
            bytemuck::try_cast_slice::<u8, Color32>(bitmap.pixels()).ok()
        } else {
            None
        };
        Self {
            bitmap,
            opaque,
            aligned,
        }
    }
    pub fn image(&self, rect: DeviceRect) -> ColorImage {
        let bitmap = self.bitmap;
        let size = [rect.width as usize, rect.height as usize];
        if rect == DeviceRect::new(0, 0, bitmap.width(), bitmap.height())
            && bitmap.stride() == bitmap.width() as usize * 4
        {
            return if self.opaque {
                opaque_image(size, bitmap.pixels())
            } else {
                ColorImage::from_rgba_unmultiplied(size, bitmap.pixels())
            };
        }
        let mut pixels = Vec::with_capacity(size[0] * size[1]);
        for y in rect.y..rect.y + rect.height {
            let start = y as usize * bitmap.stride() + rect.x as usize * 4;
            let end = start + rect.width as usize * 4;
            if let Some(colors) = self.aligned {
                pixels.extend_from_slice(&colors[start / 4..end / 4]);
            } else {
                pixels.extend(
                    bitmap.pixels()[start..end]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])),
                );
            }
        }
        ColorImage::new(size, pixels)
    }
}

fn opaque_image(size: [usize; 2], bytes: &[u8]) -> ColorImage {
    match bytemuck::try_cast_slice::<u8, Color32>(bytes) {
        Ok(colors) => ColorImage::new(size, colors.to_vec()),
        // Do not depend on an allocator returning an over-aligned byte buffer.
        Err(_) => ColorImage::from_rgba_premultiplied(size, bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn images(bitmap: Bitmap, sources: &[DeviceRect]) -> Vec<ColorImage> {
        let converter = Converter::new(&bitmap);
        sources.iter().map(|r| converter.image(*r)).collect()
    }
    fn bitmap(alpha: bool) -> Bitmap {
        let mut bitmap = Bitmap::new(259, 17, PixelFormat::Rgba8).unwrap();
        for (i, p) in bitmap
            .pixels_mut()
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .enumerate()
        {
            p.copy_from_slice(&[
                (i % 256) as u8,
                171,
                53,
                if alpha { (i % 256) as u8 } else { 255 },
            ]);
        }
        bitmap
    }
    #[test]
    fn whole_bitmap_matches_egui_for_every_alpha_value() {
        for alpha in [false, true] {
            let bitmap = bitmap(alpha);
            let expected = ColorImage::from_rgba_unmultiplied([259, 17], bitmap.pixels());
            let image = images(bitmap, &[DeviceRect::new(0, 0, 259, 17)])
                .pop()
                .unwrap();
            assert_eq!(image, expected);
        }
    }
    #[test]
    fn strided_subrects_match_egui_including_transparency_and_partial_edge_tiles() {
        for alpha in [false, true] {
            let bitmap = bitmap(alpha);
            let sources = [
                DeviceRect::new(13, 3, 80, 7),
                DeviceRect::new(256, 0, 3, 17),
            ];
            let expected: Vec<_> = sources
                .iter()
                .map(|r| {
                    let mut bytes = Vec::new();
                    for y in r.y..r.y + r.height {
                        let start = y as usize * bitmap.stride() + r.x as usize * 4;
                        bytes.extend_from_slice(
                            &bitmap.pixels()[start..start + r.width as usize * 4],
                        );
                    }
                    ColorImage::from_rgba_unmultiplied(
                        [r.width as usize, r.height as usize],
                        &bytes,
                    )
                })
                .collect();
            assert_eq!(images(bitmap, &sources), expected);
        }
    }
    #[test]
    fn unaligned_opaque_bytes_use_the_safe_copy_fallback() {
        let mut bytes = [0; 12];
        let start = (5 - bytes.as_ptr() as usize % 4) % 4;
        bytes[start..start + 8].copy_from_slice(&[255, 23, 67, 255, 200, 100, 50, 255]);
        let source = &bytes[start..start + 8];
        assert!(bytemuck::try_cast_slice::<u8, Color32>(source).is_err());
        assert_eq!(
            opaque_image([2, 1], source),
            ColorImage::from_rgba_unmultiplied([2, 1], source)
        );
    }
}
