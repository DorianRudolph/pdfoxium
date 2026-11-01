// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/font/cpdf_simplefont_unittest.cpp`.
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

use crate::fpdfapi::font::cpdf_facebasedsimplefont::CpdfLoadedSimpleFont;
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxge::cfx_standardfont::{get_font_data, StandardFontIndex};

#[test]
fn BaseFontNameWithSubsetting() {
    let _fixture = TestWithPageModule::set_up();
    let mut mapper = CfxFontMapper::new();
    let doc = CpdfDocument::new();
    let font_file_stream = retain_object(CpdfStream::from_data_and_dict(
        get_font_data(StandardFontIndex::Courier).to_vec(),
        retain_object(CpdfDictionary::new()),
    ));
    doc.holder()
        .borrow_mut()
        .add_indirect_object(font_file_stream.clone());
    // A retained Rust value guarantees the native non-null assertion.
    let stream_object_number = font_file_stream.borrow().obj_num();
    assert!(stream_object_number > 0);
    let mut font_descriptor_dict = CpdfDictionary::new();
    font_descriptor_dict.set_for(
        b"FontFile",
        Some(retain_object(CpdfReference::new(
            &doc.holder(),
            stream_object_number,
        ))),
    );
    let mut font_dict = CpdfDictionary::new();
    font_dict.set_new_name_for(b"BaseFont", b"CHEESE+Swiss");
    font_dict.set_for(b"FontDescriptor", Some(retain_object(font_descriptor_dict)));
    // Rust's concrete loader contains the native common load operation; the
    // native test subclass otherwise supplies an empty LoadGlyphMap override.
    let font = CpdfLoadedSimpleFont::load(retain_object(font_dict), doc.page_data(), &mut mapper);
    assert!(font.is_some());
    assert_eq!(font.unwrap().base_font_name(), b"Swiss");
}
