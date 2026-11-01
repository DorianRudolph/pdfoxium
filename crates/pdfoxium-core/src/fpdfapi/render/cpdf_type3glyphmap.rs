// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Type 3 glyph bitmap map and blue-zone alignment from
//! `core/fpdfapi/render/cpdf_type3glyphmap.{h,cpp}`.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap;

const TYPE3_MAX_BLUES: usize = 16;

#[derive(Default)]
pub struct CpdfType3GlyphMap {
    top_blue: Vec<i32>,
    bottom_blue: Vec<i32>,
    glyph_map: BTreeMap<u32, Option<Rc<CfxGlyphBitmap>>>,
}

impl CpdfType3GlyphMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn adjust_blue(&mut self, top: f32, bottom: f32) -> (i32, i32) {
        (
            adjust_blue_helper(top, &mut self.top_blue),
            adjust_blue_helper(bottom, &mut self.bottom_blue),
        )
    }

    pub fn get_bitmap(&self, charcode: u32) -> Option<Rc<CfxGlyphBitmap>> {
        self.glyph_map
            .get(&charcode)
            .and_then(|bitmap| bitmap.as_ref().map(Rc::clone))
    }

    pub fn set_bitmap(&mut self, charcode: u32, bitmap: Option<CfxGlyphBitmap>) {
        self.glyph_map.insert(charcode, bitmap.map(Rc::new));
    }
}

fn adjust_blue_helper(position: f32, blues: &mut Vec<i32>) -> i32 {
    let mut closest = None;
    let mut min_distance = 1_000_000.0f32;
    for &blue in blues.iter() {
        let distance = (position - blue as f32).abs();
        if distance < 0.8f32.min(min_distance) {
            min_distance = distance;
            closest = Some(blue);
        }
    }
    if let Some(blue) = closest {
        return blue;
    }
    let rounded = position.round() as i32;
    if blues.len() < TYPE3_MAX_BLUES {
        blues.push(rounded);
    }
    rounded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::fx_coordinates::CfxPoint;
    use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
    use crate::fxge::dib::fx_dib::FxdibFormat;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn aligns_nearby_edges_but_keeps_top_and_bottom_separate() {
        let mut map = CpdfType3GlyphMap::new();
        assert_eq!(map.adjust_blue(10.1, 20.1), (10, 20));
        assert_eq!(map.adjust_blue(10.7, 20.7), (10, 20));
        assert_eq!(map.adjust_blue(20.1, 10.1), (20, 10));
        assert_eq!(map.adjust_blue(11.1, 21.1), (11, 21));
    }

    #[test]
    fn caches_bitmap_by_character_code() {
        let mut map = CpdfType3GlyphMap::new();
        assert!(map.get_bitmap(65).is_none());
        let bitmap = Rc::new(RefCell::new(
            CfxDibitmap::create(1, 1, FxdibFormat::EightBppMask, 0).unwrap(),
        ));
        map.set_bitmap(65, Some(CfxGlyphBitmap::new(CfxPoint::new(3, 4), bitmap)));
        assert_eq!(
            map.get_bitmap(65).unwrap().top_left_origin(),
            CfxPoint::new(3, 4)
        );
        assert!(map.get_bitmap(66).is_none());
        map.set_bitmap(65, None);
        assert!(map.get_bitmap(65).is_none());
    }
}
