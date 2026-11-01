// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Font caches from `core/fpdfapi/font/cpdf_fontglobals.{h,cpp}`.
//! Bundled map tables are static in Rust. The font cache is owned by its
//! engine context in place of PDFium's process singleton.

use std::collections::HashMap;
use std::rc::Rc;

use super::cpdf_cid2unicodemap::CpdfCid2UnicodeMap;
use super::cpdf_cmap::CpdfCmap;
use super::cpdf_cmapparser::CidSet;
use super::cpdf_stockfontarray::{CpdfStockFontArray, StockFont};
use crate::fpdfapi::cmaps::{fpdf_cmaps::CMap, maps_for_charset, unicode_map_for_charset};
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fxge::cfx_standardfont::StandardFontIndex;

pub struct CpdfFontGlobals {
    cmaps: HashMap<Vec<u8>, Rc<CpdfCmap>>,
    cid2unicode_maps: [Option<Rc<CpdfCid2UnicodeMap>>; 6],
    stock_map: HashMap<*const CpdfDocument, CpdfStockFontArray>,
}

impl Default for CpdfFontGlobals {
    fn default() -> Self {
        Self {
            cmaps: HashMap::new(),
            cid2unicode_maps: std::array::from_fn(|_| None),
            stock_map: HashMap::new(),
        }
    }
}

impl CpdfFontGlobals {
    pub fn embedded_charset(&self, charset: CidSet) -> &'static [CMap] {
        maps_for_charset(charset)
    }

    pub fn embedded_to_unicode(&self, charset: CidSet) -> &'static [u16] {
        unicode_map_for_charset(charset)
    }

    pub fn get_predefined_cmap(&mut self, name: &[u8]) -> Rc<CpdfCmap> {
        if let Some(map) = self.cmaps.get(name) {
            return Rc::clone(map);
        }
        let stripped = name.strip_prefix(b"/").unwrap_or(name);
        let map = Rc::new(CpdfCmap::from_predefined(stripped));
        if !name.is_empty() {
            self.cmaps.insert(name.to_vec(), Rc::clone(&map));
        }
        map
    }

    pub fn get_cid2unicode_map(&mut self, charset: CidSet) -> Rc<CpdfCid2UnicodeMap> {
        let index = charset as usize;
        if let Some(map) = &self.cid2unicode_maps[index] {
            return Rc::clone(map);
        }
        let map = Rc::new(CpdfCid2UnicodeMap::new(charset));
        self.cid2unicode_maps[index] = Some(Rc::clone(&map));
        map
    }

    pub fn find(
        &self,
        document: &CpdfDocument,
        index: StandardFontIndex,
    ) -> Option<Rc<dyn StockFont>> {
        self.stock_map.get(&(document as *const _))?.get_font(index)
    }

    pub fn set(
        &mut self,
        document: &CpdfDocument,
        index: StandardFontIndex,
        font: Option<Rc<dyn StockFont>>,
    ) {
        self.stock_map
            .entry(document as *const _)
            .or_default()
            .set_font(index, font);
    }

    pub fn clear(&mut self, document: &CpdfDocument) {
        self.stock_map.remove(&(document as *const _));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    struct Font(Cell<bool>);
    impl StockFont for Font {
        fn clear_font_dict(&self) {
            self.0.set(true);
        }
    }

    #[test]
    fn caches_predefined_maps_and_clears_per_document_stock_fonts() {
        let mut globals = CpdfFontGlobals::default();
        let first = globals.get_predefined_cmap(b"/GB-EUC-H");
        let second = globals.get_predefined_cmap(b"/GB-EUC-H");
        assert!(Rc::ptr_eq(&first, &second));
        assert!(first.is_loaded());
        assert_eq!(first.cid_from_char_code(0x20), 0x1e24);
        let unicode = globals.get_cid2unicode_map(CidSet::GB1);
        assert!(Rc::ptr_eq(
            &unicode,
            &globals.get_cid2unicode_map(CidSet::GB1)
        ));
        assert_eq!(globals.embedded_to_unicode(CidSet::GB1)[1], 0x20);

        let document = CpdfDocument::new();
        let other = CpdfDocument::new();
        let font = Rc::new(Font(Cell::new(false)));
        globals.set(&document, StandardFontIndex::Courier, Some(font.clone()));
        assert!(globals
            .find(&document, StandardFontIndex::Courier)
            .is_some());
        assert!(globals.find(&other, StandardFontIndex::Courier).is_none());
        globals.clear(&document);
        assert!(font.0.get());
        assert!(globals
            .find(&document, StandardFontIndex::Courier)
            .is_none());
    }
}
