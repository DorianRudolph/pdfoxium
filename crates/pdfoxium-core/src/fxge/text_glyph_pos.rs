// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxge/text_glyph_pos.{h,cpp}`.

use crate::fxcrt::fx_coordinates::{CfxPoint, CfxPointF};

use super::cfx_glyphbitmap::CfxGlyphBitmap;

#[derive(Clone, Copy)]
pub struct TextGlyphPos<'a> {
    pub glyph: Option<&'a CfxGlyphBitmap>,
    pub origin: CfxPoint,
    pub device_origin: CfxPointF,
}

impl Default for TextGlyphPos<'_> {
    fn default() -> Self {
        Self {
            glyph: None,
            origin: CfxPoint::new(0, 0),
            device_origin: CfxPointF::new(0.0, 0.0),
        }
    }
}

impl TextGlyphPos<'_> {
    pub fn get_origin(&self, offset: CfxPoint) -> Option<CfxPoint> {
        let glyph_origin = self.glyph?.top_left_origin();
        let left = i64::from(self.origin.x) + i64::from(glyph_origin.x) - i64::from(offset.x);
        let top = i64::from(self.origin.y) - i64::from(glyph_origin.y) - i64::from(offset.y);
        Some(CfxPoint::new(
            i32::try_from(left).ok()?,
            i32::try_from(top).ok()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    #[test]
    fn glyph_origin_uses_top_left_and_checked_coordinates() {
        let bitmap = CfxDibitmap::create(1, 1, FxdibFormat::EightBppMask, 0).unwrap();
        let glyph = CfxGlyphBitmap::new(CfxPoint::new(2, 3), Rc::new(RefCell::new(bitmap)));
        let pos = TextGlyphPos {
            glyph: Some(&glyph),
            origin: CfxPoint::new(10, 20),
            device_origin: CfxPointF::new(0.0, 0.0),
        };
        assert_eq!(
            pos.get_origin(CfxPoint::new(4, 5)),
            Some(CfxPoint::new(8, 12))
        );
        assert_eq!(
            TextGlyphPos {
                origin: CfxPoint::new(i32::MAX, 20),
                ..pos
            }
            .get_origin(CfxPoint::new(0, 0)),
            None
        );
    }
}
