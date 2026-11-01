// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Source-derived cases for CFX_MacFontInfo::MapFont and default path policy.
use super::*;
use crate::fxge::fx_font::{FONT_PITCH_FAMILY_FIXED, FONT_PITCH_FAMILY_ROMAN};
use crate::fxge::systemfontinfo_iface::{TABLE_NAME, TABLE_NONE};

fn select(
    names: &[&[u8]],
    face: &[u8],
    weight: i32,
    italic: bool,
    charset: FxCharset,
    pitch: u32,
) -> Option<Vec<u8>> {
    map_font(weight, italic, charset, pitch, face, |name| {
        names
            .iter()
            .find(|&&candidate| candidate == name)
            .map(|&name| name.to_vec())
    })
}

#[test]
fn all_base14_substitutions_precede_requested_style_and_exact_name() {
    for (base, substitute) in [
        (b"Courier".as_slice(), b"Courier New".as_slice()),
        (b"Courier-Bold", b"Courier New Bold"),
        (b"Courier-BoldOblique", b"Courier New Bold Italic"),
        (b"Courier-Oblique", b"Courier New Italic"),
        (b"Helvetica", b"Arial"),
        (b"Helvetica-Bold", b"Arial Bold"),
        (b"Helvetica-BoldOblique", b"Arial Bold Italic"),
        (b"Helvetica-Oblique", b"Arial Italic"),
        (b"Times-Roman", b"Times New Roman"),
        (b"Times-Bold", b"Times New Roman Bold"),
        (b"Times-BoldItalic", b"Times New Roman Bold Italic"),
        (b"Times-Italic", b"Times New Roman Italic"),
    ] {
        assert_eq!(
            select(&[base, substitute], base, 900, true, FxCharset::ShiftJis, 0),
            Some(substitute.to_vec())
        );
        // Missing canonical substitutes return immediately, even if an exact
        // font or the charset fallback happens to be installed.
        assert_eq!(
            select(
                &[base, b"Hiragino Mincho Pro W6"],
                base,
                400,
                false,
                FxCharset::ShiftJis,
                0
            ),
            None
        );
    }
}

#[test]
fn style_suffixes_precede_exact_name_and_preserve_case_sensitive_rules() {
    let names: &[&[u8]] = &[
        b"Family",
        b"Family Bold",
        b"Family Italic",
        b"Family Bold Italic",
        b"Family Bold Italic Bold",
        b"lowerbold Bold",
    ];
    for (weight, italic, expected) in [
        (400, false, b"Family".as_slice()),
        (401, false, b"Family Bold"),
        (400, true, b"Family Italic"),
        (700, true, b"Family Bold Italic"),
    ] {
        assert_eq!(
            select(names, b"Family", weight, italic, FxCharset::Ansi, 0),
            Some(expected.to_vec())
        );
    }
    for face in [
        b"Family Bold".as_slice(),
        b"Family Italic",
        b"Family Bold Italic",
    ] {
        assert_eq!(
            select(names, face, 900, true, FxCharset::Ansi, 0),
            Some(face.to_vec())
        );
    }
    assert_eq!(
        select(names, b"lowerbold", 700, false, FxCharset::Ansi, 0),
        Some(b"lowerbold Bold".to_vec())
    );
    assert_eq!(
        select(&[b"Family"], b"Family", 700, true, FxCharset::Ansi, 0),
        Some(b"Family".to_vec())
    );
    assert_eq!(
        select(
            &[b"Family Heavy"],
            b"Family",
            700,
            false,
            FxCharset::Ansi,
            0
        ),
        None
    );
}

#[test]
fn ansi_fixed_pitch_fallback_and_missing_symbol_return_without_similarity_search() {
    let names: &[&[u8]] = &[b"Courier New", b"Unknown Extra", b"Symbol Other"];
    assert_eq!(
        select(
            names,
            b"Unknown",
            400,
            false,
            FxCharset::Ansi,
            FONT_PITCH_FAMILY_FIXED
        ),
        Some(b"Courier New".to_vec())
    );
    assert_eq!(
        select(names, b"Unknown", 400, false, FxCharset::Ansi, 0),
        None
    );
    assert_eq!(
        select(
            names,
            b"Symbol",
            400,
            false,
            FxCharset::Symbol,
            FONT_PITCH_FAMILY_FIXED
        ),
        None
    );
    assert_eq!(
        select(
            &[],
            b"Unknown",
            400,
            false,
            FxCharset::Ansi,
            FONT_PITCH_FAMILY_FIXED
        ),
        None
    );
}

#[test]
fn cjk_names_and_japanese_weight_family_precedence_match_source() {
    let names: &[&[u8]] = &[
        b"Hiragino Kaku Gothic Pro W6",
        b"Hiragino Mincho Pro W6",
        b"STSong",
        b"AppleMyungjo",
        b"LiSong Pro Light",
    ];
    for (face, weight, pitch, expected) in [
        (
            b"Missing Gothic".as_slice(),
            400,
            FONT_PITCH_FAMILY_ROMAN,
            b"Hiragino Kaku Gothic Pro W6".as_slice(),
        ),
        (b"Missing", 400, 0, b"Hiragino Mincho Pro W6"),
        (b"Missing", 401, 0, b"Hiragino Kaku Gothic Pro W6"),
        (
            b"Missing",
            700,
            FONT_PITCH_FAMILY_ROMAN,
            b"Hiragino Mincho Pro W6",
        ),
        (b"missing gothic", 400, 0, b"Hiragino Mincho Pro W6"),
    ] {
        assert_eq!(
            select(names, face, weight, false, FxCharset::ShiftJis, pitch),
            Some(expected.to_vec())
        );
    }
    for (charset, expected) in [
        (FxCharset::ChineseSimplified, b"STSong".as_slice()),
        (FxCharset::Hangul, b"AppleMyungjo"),
        (FxCharset::ChineseTraditional, b"LiSong Pro Light"),
    ] {
        assert_eq!(
            select(names, b"Missing", 700, true, charset, 0),
            Some(expected.to_vec())
        );
        assert_eq!(select(&[], b"Missing", 700, true, charset, 0), None);
        assert_eq!(
            select(&[b"Exact"], b"Exact", 400, false, charset, 0),
            Some(b"Exact".to_vec())
        );
    }
    assert_eq!(
        select(names, b"Missing", 400, false, FxCharset::MswinGreek, 0),
        None
    );
}

#[test]
fn default_path_order_and_explicit_folder_handles_preserve_native_contract() {
    assert_eq!(
        DEFAULT_PATHS,
        ["~/Library/Fonts", "/Library/Fonts", "/System/Library/Fonts"]
    );
    let mut empty = CfxMacFontInfo::from_paths([]);
    assert!(empty.enum_font_list().is_empty());
    assert_eq!(
        empty.map_font(400, false, FxCharset::Ansi, 0, b"Ahem"),
        None
    );
    let mut info = CfxMacFontInfo::from_paths([PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/fonts/ahem"
    ))]);
    let fonts = info.enum_font_list();
    assert!(fonts
        .iter()
        .any(|(name, charset)| name == b"Ahem" && *charset == FxCharset::Ansi));
    let font = info
        .map_font(700, true, FxCharset::Ansi, 0, b"Ahem")
        .unwrap();
    assert_eq!(info.get_font(b"Ahem"), Some(font));
    assert_eq!(info.get_face_name(font).unwrap(), b"Ahem");
    // Ahem advertises the Symbol codepage; the folder provider reports that
    // before the unconditional ANSI enumeration entry.
    assert_eq!(info.get_font_charset(font), Some(FxCharset::Symbol));
    for table in [TABLE_NAME, TABLE_NONE] {
        let size = info.get_font_data(font, table, &mut []);
        assert!(size > 0);
        let mut bytes = vec![0; size];
        assert_eq!(info.get_font_data(font, table, &mut bytes), size);
        assert!(bytes.iter().any(|&byte| byte != 0));
    }
    info.delete_font(font);
    assert_eq!(info.get_font(b"Ahem"), Some(font));
}
