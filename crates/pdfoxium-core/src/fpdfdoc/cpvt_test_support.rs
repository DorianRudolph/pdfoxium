// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Identical document/font/map/stub fixtures from cpvt_section_unittest.cpp
//! and cpvt_variabletext_unittest.cpp; native teardown order is field order.
use super::*;
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfdoc::cpvt_fontmap::CpvtFontMap;
use crate::fxge::cfx_fontmapper::CfxFontMapper;
use std::rc::{Rc, Weak};
pub(super) fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
pub(super) struct StubProvider {
    _font_map: Weak<CpvtFontMap>,
}
impl Provider for StubProvider {
    fn char_width(&self, _: i32, _: u16) -> i32 {
        10
    }
    fn type_ascent(&self, _: i32) -> i32 {
        10
    }
    fn type_descent(&self, _: i32) -> i32 {
        -2
    }
    fn word_font_index(&self, _: u16, _: FxCharset, _: i32) -> i32 {
        0
    }
    fn default_font_index(&self) -> i32 {
        0
    }
}
pub(super) struct LayoutFixture {
    pub provider: StubProvider,
    _font_map: Rc<CpvtFontMap>,
    _font: CpdfFont,
    _test_doc: CpdfDocument,
    _mapper: CfxFontMapper,
}
impl LayoutFixture {
    pub fn new() -> Self {
        let test_doc = CpdfDocument::new();
        let mut mapper = CfxFontMapper::new();
        let font = CpdfFont::Simple(
            test_doc
                .page_data()
                .get_stock_font(b"Helvetica", &mut mapper)
                .unwrap(),
        );
        let font_map = Rc::new(CpvtFontMap::new(Some(font.clone()), b"Helvetica".to_vec()));
        let provider = StubProvider {
            _font_map: Rc::downgrade(&font_map),
        };
        Self {
            provider,
            _font_map: font_map,
            _font: font,
            _test_doc: test_doc,
            _mapper: mapper,
        }
    }
}
