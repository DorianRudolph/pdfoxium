// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/font/cpdf_stockfontarray.{h,cpp}`.
//! Stock fonts clear their retained PDF dictionaries before cache teardown.

use std::rc::Rc;

use crate::fxge::cfx_standardfont::StandardFontIndex;

pub trait StockFont: std::any::Any {
    fn clear_font_dict(&self);
}

pub struct CpdfStockFontArray {
    stock_fonts: [Option<Rc<dyn StockFont>>; 14],
}

impl Default for CpdfStockFontArray {
    fn default() -> Self {
        Self {
            stock_fonts: std::array::from_fn(|_| None),
        }
    }
}

impl CpdfStockFontArray {
    pub fn get_font(&self, index: StandardFontIndex) -> Option<Rc<dyn StockFont>> {
        self.stock_fonts[index as usize].clone()
    }

    pub fn set_font(&mut self, index: StandardFontIndex, font: Option<Rc<dyn StockFont>>) {
        self.stock_fonts[index as usize] = font;
    }
}

impl Drop for CpdfStockFontArray {
    fn drop(&mut self) {
        for font in self.stock_fonts.iter().flatten() {
            // The dictionary reference must be removed while this array still
            // owns a font reference, before a possible last font release.
            font.clear_font_dict();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Font {
        cleared: Cell<bool>,
    }

    impl StockFont for Font {
        fn clear_font_dict(&self) {
            self.cleared.set(true);
        }
    }

    #[test]
    fn slots_retain_fonts_and_clear_dictionaries_on_drop() {
        let font = Rc::new(Font {
            cleared: Cell::new(false),
        });
        let mut fonts = CpdfStockFontArray::default();
        fonts.set_font(StandardFontIndex::Helvetica, Some(font.clone()));
        assert!(fonts.get_font(StandardFontIndex::Courier).is_none());
        assert!(fonts.get_font(StandardFontIndex::Helvetica).is_some());
        assert!(!font.cleared.get());
        drop(fonts);
        assert!(font.cleared.get());
    }
}
