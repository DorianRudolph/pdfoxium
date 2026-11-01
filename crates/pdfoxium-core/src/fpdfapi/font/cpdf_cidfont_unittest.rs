// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/font/cpdf_cidfont_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use crate::fpdfapi::page::cpdf_pagemodule::{destroy_page_module, initialize_page_module};
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fxge::cfx_fontmapper::CfxFontMapper;

// TestWithPageModule's setup/teardown, with cleanup during unwinding.
struct TestWithPageModule;
impl TestWithPageModule {
    fn set_up() -> Self {
        initialize_page_module();
        Self
    }
}
impl Drop for TestWithPageModule {
    fn drop(&mut self) {
        destroy_page_module();
    }
}

use crate::fpdfapi::parser::cpdf_array::CpdfArray;

#[test]
fn Bug920636() {
    let _fixture = TestWithPageModule::set_up();
    let mut mapper = CfxFontMapper::new();
    let doc = CpdfDocument::new();
    let mut font_dict = CpdfDictionary::new();
    // This is U+2212 MINUS SIGN, exactly as the source regression fixture.
    font_dict.set_new_name_for(b"Encoding", "Identity−H".as_bytes());
    {
        let mut descendant_fonts = CpdfArray::new();
        {
            let mut descendant_font = CpdfDictionary::new();
            descendant_font.set_new_name_for(b"BaseFont", b"CourierStd");
            descendant_fonts.append(retain_object(descendant_font));
        }
        font_dict.set_for(b"DescendantFonts", Some(retain_object(descendant_fonts)));
    }
    let font = doc
        .page_data()
        .get_cid_font(&retain_object(font_dict), &mut mapper);
    assert!(font.is_some());
    let font = font.unwrap();
    for (charcode, glyph) in [(0, 31), (256, 287), (34661, 34692)] {
        let (actual, _vertical) = font.metrics.borrow_mut().glyph_from_char_code(
            charcode,
            font.font().face(),
            font.to_unicode.as_ref(),
        );
        assert_eq!(actual, glyph);
    }
}
