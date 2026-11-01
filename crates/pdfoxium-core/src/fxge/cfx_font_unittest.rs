// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/cfx_font_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CfxFont;
use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxge::fx_font::CharCodeAndIndex;

#[test]
fn GetCharCodesAndIndices() {
    let bytes = include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
    let font = CfxFont::from_span(bytes, false, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    let results = font.get_char_codes_and_indices(0xffff);
    assert_eq!(results.len(), 278);
    assert_eq!(
        results[0],
        CharCodeAndIndex {
            char_code: 32,
            glyph_index: 3
        }
    );
    assert_eq!(
        results[1],
        CharCodeAndIndex {
            char_code: 33,
            glyph_index: 4
        }
    );
    assert_eq!(
        results[277],
        CharCodeAndIndex {
            char_code: 65279,
            glyph_index: 245
        }
    );
}

#[test]
fn GetCharBBox() {
    let bytes = include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
    let font = CfxFont::from_span(bytes, false, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    // Native's code=33 is used only by an unselected platform tricky-font path;
    // the selected API takes its glyph index directly.
    let bbox = font.get_char_bbox(4);
    assert!(bbox.is_some());
    assert_eq!(bbox.unwrap(), FxRect::new(0, 812, 1000, -199));
    let invalid_bbox = font.get_char_bbox(u32::MAX);
    assert!(invalid_bbox.is_some());
    assert_eq!(invalid_bbox.unwrap(), FxRect::new(0, 0, 0, 0));
}

#[test]
fn ParseGSUBTable() {
    let bytes =
        include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/bug_377948405.ttf");
    let font = CfxFont::from_span(bytes, false, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    // CFX_Font::ParseGSUBTable only forwards to its retained face.
    let gsub = font.face().and_then(|face| face.parse_gsub_table());
    assert!(gsub.is_some());
    let gsub = gsub.unwrap();
    assert_eq!(gsub.get_vertical_glyph(0), 0);
    assert_eq!(gsub.get_vertical_glyph(1), 0);
    let sc_bytes = include_bytes!(
        "../../../../upstream/pdfium/third_party/NotoSansCJK/NotoSansSC-Regular.subset.otf"
    );
    let sc_font = CfxFont::from_span(sc_bytes, false, 0);
    assert!(sc_font.is_ok());
    let sc_font = sc_font.unwrap();
    let sc_gsub = sc_font.face().and_then(|face| face.parse_gsub_table());
    assert!(sc_gsub.is_some());
    let sc_gsub = sc_gsub.unwrap();
    assert_eq!(sc_gsub.get_vertical_glyph(0), 0);
    assert_eq!(sc_gsub.get_vertical_glyph(1), 0);
    assert_eq!(sc_gsub.get_vertical_glyph(2), 10);
    assert_eq!(sc_gsub.get_vertical_glyph(3), 0);
}
