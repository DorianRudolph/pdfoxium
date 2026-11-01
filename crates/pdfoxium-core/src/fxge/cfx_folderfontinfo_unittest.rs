// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/cfx_folderfontinfo_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::fxge::fx_font::{FONT_PITCH_FAMILY_FIXED, FONT_PITCH_FAMILY_ROMAN};

const kArial: &[u8] = b"Arial";
const kCourierNew: &[u8] = b"Courier New";
const kTimesNewRoman: &[u8] = b"TimesNewRoman";
const kSymbol: &[u8] = b"Symbol";
const kBookshelfSymbol7: &[u8] = b"Bookshelf Symbol 7";
const kCalibri: &[u8] = b"Calibri";
const kBookshelf: &[u8] = b"Bookshelf";
const kBook: &[u8] = b"Book";
const kTofuBold: &[u8] = b"Tofu, Bold Italic";
const kTofu: &[u8] = b"Tofu";
const kLatoUltraBold: &[u8] = b"Lato Ultra-Bold";
const kLato: &[u8] = b"Lato";
const kOxygenSansSansBold: &[u8] = b"Oxygen-Sans Sans-Bold";
const kOxygenSans: &[u8] = b"Oxygen-Sans";
const kOxygen: &[u8] = b"Oxygen";
const kComicSansMS: &[u8] = b"Comic Sans MS";

struct CFXFolderFontInfoTest {
    font_info: CfxFolderFontInfo,
}
impl CFXFolderFontInfoTest {
    fn new() -> Self {
        let mut fixture = Self {
            font_info: CfxFolderFontInfo::new(),
        };
        fixture.add_dummy_font(kArial, charset_flag(FxCharset::Ansi));
        fixture.add_dummy_font(kCourierNew, charset_flag(FxCharset::Ansi));
        fixture.add_dummy_font(kTimesNewRoman, 0);
        fixture.add_dummy_font(kBookshelfSymbol7, charset_flag(FxCharset::Symbol));
        fixture.add_dummy_font(kSymbol, charset_flag(FxCharset::Symbol));
        fixture.add_dummy_font(kTofuBold, charset_flag(FxCharset::Symbol));
        fixture.add_dummy_font(kLatoUltraBold, charset_flag(FxCharset::Ansi));
        fixture.add_dummy_font(kOxygenSansSansBold, charset_flag(FxCharset::Ansi));
        fixture.add_dummy_font(kComicSansMS, charset_flag(FxCharset::Ansi));
        fixture
    }
    fn add_dummy_font(&mut self, name: &[u8], charset: u32) {
        let mut info = FontFaceInfo::new(PathBuf::new(), name.to_vec(), Vec::new(), 0, 0);
        info.charsets = charset;
        self.font_info.font_list.insert(name.to_vec(), info);
    }
    fn find_font(
        &self,
        weight: i32,
        italic: bool,
        charset: FxCharset,
        pitch_family: u32,
        family: &[u8],
        match_name: bool,
    ) -> Option<&FontFaceInfo> {
        self.font_info
            .find_font(weight, italic, charset, pitch_family, family, match_name)
    }
}

#[test]
fn TestFindFont() {
    let fixture = CFXFolderFontInfoTest::new();
    // Find "Symbol" font
    let mut font = fixture.find_font(
        0,
        false,
        FxCharset::Symbol,
        FONT_PITCH_FAMILY_ROMAN,
        kSymbol,
        true,
    );
    assert!(font.is_some());
    assert_eq!(&font.unwrap().face_name, kSymbol);

    // Find "Calibri" font that is not present in the installed fonts
    assert!((fixture.find_font(
        0,
        false,
        FxCharset::Symbol,
        FONT_PITCH_FAMILY_ROMAN,
        kCalibri,
        true
    ))
    .is_none());

    // Find the closest matching font to "Bookshelf" font that is present in the
    // installed fonts
    font = fixture.find_font(
        0,
        false,
        FxCharset::Symbol,
        FONT_PITCH_FAMILY_ROMAN,
        kBookshelf,
        true,
    );
    assert!(font.is_some());
    assert_eq!(&font.unwrap().face_name, kBookshelfSymbol7);

    // Find "Book" font is expected to fail, because none of the installed fonts
    // is in the same font family.
    assert!((fixture.find_font(
        0,
        false,
        FxCharset::Symbol,
        FONT_PITCH_FAMILY_ROMAN,
        kBook,
        true
    ))
    .is_none());

    // Find the closest matching font for "Tofu" in the installed fonts, which
    // has "," following the string "Tofu".
    font = fixture.find_font(
        0,
        false,
        FxCharset::Symbol,
        FONT_PITCH_FAMILY_ROMAN,
        kTofu,
        true,
    );
    assert!(font.is_some());
    assert_eq!(&font.unwrap().face_name, kTofuBold);

    // Find the closest matching font for "Lato" in the installed fonts, which
    // has a space character following the string "Lato".
    font = fixture.find_font(
        0,
        false,
        FxCharset::Ansi,
        FONT_PITCH_FAMILY_ROMAN,
        kLato,
        true,
    );
    assert!(font.is_some());
    assert_eq!(&font.unwrap().face_name, kLatoUltraBold);

    // Find the closest matching font for "Oxygen" in the installed fonts,
    // which has "-" following the string "Oxygen".
    font = fixture.find_font(
        0,
        false,
        FxCharset::Ansi,
        FONT_PITCH_FAMILY_ROMAN,
        kOxygen,
        true,
    );
    assert!(font.is_some());
    assert_eq!(&font.unwrap().face_name, kOxygenSansSansBold);

    // Find the closest matching font for "Oxygen-Sans" in the installed fonts,
    // to test matching a family name with "-".
    font = fixture.find_font(
        0,
        false,
        FxCharset::Ansi,
        FONT_PITCH_FAMILY_ROMAN,
        kOxygenSans,
        true,
    );
    assert!(font.is_some());
    assert_eq!(&font.unwrap().face_name, kOxygenSansSansBold);

    // Find "Symbol" font when name matching is false
    font = fixture.find_font(
        0,
        false,
        FxCharset::Symbol,
        FONT_PITCH_FAMILY_ROMAN,
        kSymbol,
        false,
    );
    assert!(font.is_some());
    assert_eq!(&font.unwrap().face_name, kBookshelfSymbol7);

    font = fixture.find_font(
        700,
        false,
        FxCharset::Ansi,
        FONT_PITCH_FAMILY_FIXED,
        kComicSansMS,
        true,
    );
    assert!(font.is_some());
    assert_eq!(&font.unwrap().face_name, kComicSansMS);
}
