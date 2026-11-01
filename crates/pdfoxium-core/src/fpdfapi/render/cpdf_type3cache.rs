// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/render/cpdf_type3cache.{h,cpp}`: image glyph rasterization and
//! size caches. Returned Rc glyphs keep their pixels alive outside map borrows.

use super::cpdf_type3glyphmap::CpdfType3GlyphMap;
use crate::fpdfapi::font::cpdf_type3font::CpdfType3Font;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPoint};
use crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap;
use crate::fxge::dib::cfx_dibbase::{stretch_to, transform_to};
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::{get_bpp_from_format, FxdibResampleOptions};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

type SizeKey = [i32; 4];

pub struct CpdfType3Cache {
    font: Rc<CpdfType3Font>,
    size_map: RefCell<BTreeMap<SizeKey, Rc<RefCell<CpdfType3GlyphMap>>>>,
}

impl CpdfType3Cache {
    pub fn new(font: Rc<CpdfType3Font>) -> Self {
        Self {
            font,
            size_map: RefCell::new(BTreeMap::new()),
        }
    }

    pub fn load_glyph_bitmap(&self, code: u32, matrix: CfxMatrix) -> Option<Rc<CfxGlyphBitmap>> {
        let key =
            [matrix.a, matrix.b, matrix.c, matrix.d].map(|value| (value * 10000.0).round() as i32);
        let size = Rc::clone(
            self.size_map
                .borrow_mut()
                .entry(key)
                .or_insert_with(|| Rc::new(RefCell::new(CpdfType3GlyphMap::new()))),
        );
        if let Some(bitmap) = size.borrow().get_bitmap(code) {
            return Some(bitmap);
        }
        // Character loading can re-enter font and page caches. Hold no map
        // borrow while loading it or transforming its bitmap.
        let bitmap = self.render_glyph(&size, code, matrix);
        let mut size = size.borrow_mut();
        size.set_bitmap(code, bitmap);
        size.get_bitmap(code)
    }

    fn render_glyph(
        &self,
        size: &RefCell<CpdfType3GlyphMap>,
        code: u32,
        matrix: CfxMatrix,
    ) -> Option<CfxGlyphBitmap> {
        let glyph = self.font.load_char(code)?;
        let (bitmap, glyph_matrix) = {
            let glyph = glyph.borrow();
            (glyph.get_bitmap()?, glyph.matrix())
        };
        let text_matrix = CfxMatrix::new(matrix.a, matrix.b, matrix.c, matrix.d, 0.0, 0.0);
        let image_matrix = glyph_matrix * text_matrix;
        let mut result_bitmap = None;
        let (mut left, mut top) = (0, 0);
        if image_matrix.b.abs() < image_matrix.a.abs() / 100.0
            && image_matrix.c.abs() < image_matrix.d.abs() / 100.0
        {
            let (first, last, height) = {
                let source = bitmap.borrow();
                (
                    detect_first_scan(&source),
                    detect_last_scan(&source),
                    source.height(),
                )
            };
            if first == 0 && last == height - 1 {
                let mut top_y = image_matrix.d + image_matrix.f;
                let mut bottom_y = image_matrix.f;
                let flipped = top_y > bottom_y;
                if flipped {
                    std::mem::swap(&mut top_y, &mut bottom_y);
                }
                let (top_line, bottom_line) = size.borrow_mut().adjust_blue(top_y, bottom_y);
                let height = if flipped {
                    top_line.checked_sub(bottom_line)?
                } else {
                    bottom_line.checked_sub(top_line)?
                };
                result_bitmap = stretch_to(
                    Rc::clone(&bitmap),
                    image_matrix.a as i32,
                    height,
                    FxdibResampleOptions::default(),
                    None,
                );
                top = top_line;
                left = if image_matrix.a < 0.0 {
                    image_matrix.e + image_matrix.a
                } else {
                    image_matrix.e
                }
                .round() as i32;
            }
        }
        if result_bitmap.is_none() {
            (result_bitmap, left, top) = transform_to(bitmap, image_matrix);
        }
        Some(CfxGlyphBitmap::new(
            CfxPoint::new(left, top.checked_neg()?),
            result_bitmap?,
        ))
    }
}

fn scanline_nonempty(line: &[u8], width: usize, bpp: i32) -> bool {
    if bpp == 1 {
        if line[..width / 8].iter().any(|&byte| byte != 0) {
            return true;
        }
        return width % 8 != 0 && line[width / 8] & (255u8 << (8 - width % 8)) != 0;
    }
    let bytes = if bpp > 8 {
        width * (bpp / 8) as usize
    } else {
        width
    };
    line[..bytes].iter().any(|&byte| byte > 0x40)
}

fn detect_first_scan(bitmap: &CfxDibitmap) -> i32 {
    let bpp = get_bpp_from_format(bitmap.format());
    (0..bitmap.height())
        .find(|&row| {
            scanline_nonempty(
                bitmap.scanline(row).expect("source row"),
                bitmap.width() as usize,
                bpp,
            )
        })
        .unwrap_or(-1)
}

fn detect_last_scan(bitmap: &CfxDibitmap) -> i32 {
    let bpp = get_bpp_from_format(bitmap.format());
    (0..bitmap.height())
        .rev()
        .find(|&row| {
            scanline_nonempty(
                bitmap.scanline(row).expect("source row"),
                bitmap.width() as usize,
                bpp,
            )
        })
        .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::super::cpdf_docrenderdata::CpdfDocRenderData;
    use super::*;
    use crate::fpdfapi::font::cpdf_type3char::{BitmapRef, CpdfType3Char, Type3Form};
    use crate::fpdfapi::font::cpdf_type3font::Type3FormFactory;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::{make_reference, ObjectRef};
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    struct Form {
        bitmap: BitmapRef,
        shifted: bool,
    }
    impl Type3Form for Form {
        fn parse_content_for_type3_char(&mut self, glyph: &mut CpdfType3Char) {
            glyph.initialize_from_stream_data(false, [0.5, 0.0, 0.0, 0.0, 1.0, 1.0]);
        }
        fn has_page_objects(&self) -> bool {
            true
        }
        fn calc_bounding_box(&self) -> CfxFloatRect {
            CfxFloatRect::new(0.0, 0.0, 1.0, 1.0)
        }
        fn get_bitmap_and_matrix_from_sole_image_of_form(&self) -> Option<(BitmapRef, CfxMatrix)> {
            Some((
                Rc::clone(&self.bitmap),
                CfxMatrix::new(
                    1.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    if self.shifted { 0.05 } else { 0.0 },
                ),
            ))
        }
    }
    struct Factory(BitmapRef);
    impl Type3FormFactory for Factory {
        fn create_form(
            &self,
            _resources: Option<ObjectRef>,
            stream: ObjectRef,
        ) -> Option<Box<dyn Type3Form>> {
            Some(Box::new(Form {
                bitmap: Rc::clone(&self.0),
                shifted: stream.borrow().obj_num() == 2,
            }))
        }
    }

    #[test]
    fn image_glyph_cache_retries_missing_bitmaps_and_aligns_nearby_edges() {
        let mut bitmap = CfxDibitmap::create(2, 2, FxdibFormat::EightBppMask, 0).unwrap();
        bitmap.buffer_mut().fill(255);
        let mut font = CpdfType3Font::new(Rc::new(Factory(Rc::new(RefCell::new(bitmap)))));
        let objects = CpdfIndirectObjectHolder::new();
        let first = objects.borrow_mut().new_indirect_stream_from_bytes(b"A");
        let second = objects.borrow_mut().new_indirect_stream_from_bytes(b"B");
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
        let procs = dict.set_new_dictionary_for(b"CharProcs");
        {
            let mut borrowed = procs.borrow_mut();
            let procs = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            procs.set_for(b"A", Some(make_reference(&first, &objects)));
            procs.set_for(b"B", Some(make_reference(&second, &objects)));
        }
        font.load(&dict, b"T3");
        let font = Rc::new(font);
        let render_data = CpdfDocRenderData::new();
        let cache = render_data.get_cached_type3(&font);
        assert!(Rc::ptr_eq(&cache, &render_data.get_cached_type3(&font)));
        let matrix = CfxMatrix::new(8.0, 0.0, 0.0, -8.0, 100.0, 200.0);
        assert!(cache.load_glyph_bitmap(65, matrix).is_none());
        assert!(font
            .load_char(65)
            .unwrap()
            .borrow_mut()
            .load_bitmap_from_sole_image_of_form());
        let first = cache.load_glyph_bitmap(65, matrix).unwrap();
        assert_eq!(first.top_left_origin(), CfxPoint::new(0, 8));
        assert_eq!((first.bitmap().width(), first.bitmap().height()), (8, 8));
        let moved = cache
            .load_glyph_bitmap(
                65,
                CfxMatrix {
                    e: 300.0,
                    f: 400.0,
                    ..matrix
                },
            )
            .unwrap();
        assert!(Rc::ptr_eq(&first, &moved));
        assert!(font
            .load_char(66)
            .unwrap()
            .borrow_mut()
            .load_bitmap_from_sole_image_of_form());
        let shifted = cache.load_glyph_bitmap(66, matrix).unwrap();
        assert_eq!(shifted.top_left_origin(), CfxPoint::new(0, 8));
        assert_eq!(shifted.bitmap().height(), 8);
        let weak = Rc::downgrade(&cache);
        drop(cache);
        assert!(weak.upgrade().is_none());
        assert!(first.bitmap().buffer().iter().any(|&byte| byte != 0));
        let cache = render_data.get_cached_type3(&font);
        assert!(cache.load_glyph_bitmap(65, matrix).is_some());
    }

    #[test]
    fn scan_detection_ignores_one_bit_padding_and_uses_strict_threshold() {
        assert!(!scanline_nonempty(&[0, 0x7f], 9, 1));
        assert!(scanline_nonempty(&[0, 0x80], 9, 1));
        assert!(!scanline_nonempty(&[0x40, 0x40, 255, 255], 2, 8));
        assert!(scanline_nonempty(&[0, 0x41, 0, 0], 2, 8));
        assert!(scanline_nonempty(&[0, 0, 0, 255], 1, 32));
    }
}
