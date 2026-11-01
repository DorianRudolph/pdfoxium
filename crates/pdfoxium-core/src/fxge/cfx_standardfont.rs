// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fxge/cfx_standardfont.{h,cpp}` and bundled Foxit font data.

use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StandardFontIndex {
    Courier,
    CourierBold,
    CourierBoldOblique,
    CourierOblique,
    Helvetica,
    HelveticaBold,
    HelveticaBoldOblique,
    HelveticaOblique,
    Times,
    TimesBold,
    TimesBoldOblique,
    TimesOblique,
    Symbol,
    Dingbats,
}

pub const NUM_STANDARD_FONTS: usize = 14;

const CANONICAL_NAMES: [&str; NUM_STANDARD_FONTS] = [
    "Courier",
    "Courier-Bold",
    "Courier-BoldOblique",
    "Courier-Oblique",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-BoldOblique",
    "Helvetica-Oblique",
    "Times-Roman",
    "Times-Bold",
    "Times-BoldItalic",
    "Times-Italic",
    "Symbol",
    "ZapfDingbats",
];

const ALT_NAMES: &[(&str, StandardFontIndex)] = &[
    ("Arial", StandardFontIndex::Helvetica),
    ("Arial,Bold", StandardFontIndex::HelveticaBold),
    ("Arial,BoldItalic", StandardFontIndex::HelveticaBoldOblique),
    ("Arial,Italic", StandardFontIndex::HelveticaOblique),
    ("Arial-Bold", StandardFontIndex::HelveticaBold),
    ("Arial-BoldItalic", StandardFontIndex::HelveticaBoldOblique),
    (
        "Arial-BoldItalicMT",
        StandardFontIndex::HelveticaBoldOblique,
    ),
    ("Arial-BoldMT", StandardFontIndex::HelveticaBold),
    ("Arial-Italic", StandardFontIndex::HelveticaOblique),
    ("Arial-ItalicMT", StandardFontIndex::HelveticaOblique),
    ("ArialBold", StandardFontIndex::HelveticaBold),
    ("ArialBoldItalic", StandardFontIndex::HelveticaBoldOblique),
    ("ArialItalic", StandardFontIndex::HelveticaOblique),
    ("ArialMT", StandardFontIndex::Helvetica),
    ("ArialMT,Bold", StandardFontIndex::HelveticaBold),
    (
        "ArialMT,BoldItalic",
        StandardFontIndex::HelveticaBoldOblique,
    ),
    ("ArialMT,Italic", StandardFontIndex::HelveticaOblique),
    ("ArialRoundedMTBold", StandardFontIndex::HelveticaBold),
    ("Courier", StandardFontIndex::Courier),
    ("Courier,Bold", StandardFontIndex::CourierBold),
    ("Courier,BoldItalic", StandardFontIndex::CourierBoldOblique),
    ("Courier,Italic", StandardFontIndex::CourierOblique),
    ("Courier-Bold", StandardFontIndex::CourierBold),
    ("Courier-BoldOblique", StandardFontIndex::CourierBoldOblique),
    ("Courier-Oblique", StandardFontIndex::CourierOblique),
    ("CourierBold", StandardFontIndex::CourierBold),
    ("CourierBoldItalic", StandardFontIndex::CourierBoldOblique),
    ("CourierItalic", StandardFontIndex::CourierOblique),
    ("CourierNew", StandardFontIndex::Courier),
    ("CourierNew,Bold", StandardFontIndex::CourierBold),
    (
        "CourierNew,BoldItalic",
        StandardFontIndex::CourierBoldOblique,
    ),
    ("CourierNew,Italic", StandardFontIndex::CourierOblique),
    ("CourierNew-Bold", StandardFontIndex::CourierBold),
    (
        "CourierNew-BoldItalic",
        StandardFontIndex::CourierBoldOblique,
    ),
    ("CourierNew-Italic", StandardFontIndex::CourierOblique),
    ("CourierNewBold", StandardFontIndex::CourierBold),
    (
        "CourierNewBoldItalic",
        StandardFontIndex::CourierBoldOblique,
    ),
    ("CourierNewItalic", StandardFontIndex::CourierOblique),
    (
        "CourierNewPS-BoldItalicMT",
        StandardFontIndex::CourierBoldOblique,
    ),
    ("CourierNewPS-BoldMT", StandardFontIndex::CourierBold),
    ("CourierNewPS-ItalicMT", StandardFontIndex::CourierOblique),
    ("CourierNewPSMT", StandardFontIndex::Courier),
    ("CourierStd", StandardFontIndex::Courier),
    ("CourierStd-Bold", StandardFontIndex::CourierBold),
    (
        "CourierStd-BoldOblique",
        StandardFontIndex::CourierBoldOblique,
    ),
    ("CourierStd-Oblique", StandardFontIndex::CourierOblique),
    ("Helvetica", StandardFontIndex::Helvetica),
    ("Helvetica,Bold", StandardFontIndex::HelveticaBold),
    (
        "Helvetica,BoldItalic",
        StandardFontIndex::HelveticaBoldOblique,
    ),
    ("Helvetica,Italic", StandardFontIndex::HelveticaOblique),
    ("Helvetica-Bold", StandardFontIndex::HelveticaBold),
    (
        "Helvetica-BoldItalic",
        StandardFontIndex::HelveticaBoldOblique,
    ),
    (
        "Helvetica-BoldOblique",
        StandardFontIndex::HelveticaBoldOblique,
    ),
    ("Helvetica-Italic", StandardFontIndex::HelveticaOblique),
    ("Helvetica-Oblique", StandardFontIndex::HelveticaOblique),
    ("HelveticaBold", StandardFontIndex::HelveticaBold),
    (
        "HelveticaBoldItalic",
        StandardFontIndex::HelveticaBoldOblique,
    ),
    ("HelveticaItalic", StandardFontIndex::HelveticaOblique),
    ("Symbol", StandardFontIndex::Symbol),
    ("SymbolMT", StandardFontIndex::Symbol),
    ("Times-Bold", StandardFontIndex::TimesBold),
    ("Times-BoldItalic", StandardFontIndex::TimesBoldOblique),
    ("Times-Italic", StandardFontIndex::TimesOblique),
    ("Times-Roman", StandardFontIndex::Times),
    ("TimesBold", StandardFontIndex::TimesBold),
    ("TimesBoldItalic", StandardFontIndex::TimesBoldOblique),
    ("TimesItalic", StandardFontIndex::TimesOblique),
    ("TimesNewRoman", StandardFontIndex::Times),
    ("TimesNewRoman,Bold", StandardFontIndex::TimesBold),
    (
        "TimesNewRoman,BoldItalic",
        StandardFontIndex::TimesBoldOblique,
    ),
    ("TimesNewRoman,Italic", StandardFontIndex::TimesOblique),
    ("TimesNewRoman-Bold", StandardFontIndex::TimesBold),
    (
        "TimesNewRoman-BoldItalic",
        StandardFontIndex::TimesBoldOblique,
    ),
    ("TimesNewRoman-Italic", StandardFontIndex::TimesOblique),
    ("TimesNewRomanBold", StandardFontIndex::TimesBold),
    (
        "TimesNewRomanBoldItalic",
        StandardFontIndex::TimesBoldOblique,
    ),
    ("TimesNewRomanItalic", StandardFontIndex::TimesOblique),
    ("TimesNewRomanPS", StandardFontIndex::Times),
    ("TimesNewRomanPS-Bold", StandardFontIndex::TimesBold),
    (
        "TimesNewRomanPS-BoldItalic",
        StandardFontIndex::TimesBoldOblique,
    ),
    (
        "TimesNewRomanPS-BoldItalicMT",
        StandardFontIndex::TimesBoldOblique,
    ),
    ("TimesNewRomanPS-BoldMT", StandardFontIndex::TimesBold),
    ("TimesNewRomanPS-Italic", StandardFontIndex::TimesOblique),
    ("TimesNewRomanPS-ItalicMT", StandardFontIndex::TimesOblique),
    ("TimesNewRomanPSMT", StandardFontIndex::Times),
    ("TimesNewRomanPSMT,Bold", StandardFontIndex::TimesBold),
    (
        "TimesNewRomanPSMT,BoldItalic",
        StandardFontIndex::TimesBoldOblique,
    ),
    ("TimesNewRomanPSMT,Italic", StandardFontIndex::TimesOblique),
    ("ZapfDingbats", StandardFontIndex::Dingbats),
];

fn compare_ignore_ascii_case(a: &[u8], b: &[u8]) -> Ordering {
    crate::fxcrt::fx_system::stricmp(a, b).cmp(&0)
}

pub fn get_standard_font_index(name: &[u8]) -> Option<StandardFontIndex> {
    // PDFium passes ByteString::c_str() to its case-insensitive search.
    let name = name.split(|&ch| ch == 0).next().unwrap_or_default();
    ALT_NAMES
        .binary_search_by(|(candidate, _)| compare_ignore_ascii_case(candidate.as_bytes(), name))
        .ok()
        .map(|index| ALT_NAMES[index].1)
}

pub fn get_canonical_font_name(index: StandardFontIndex) -> &'static str {
    CANONICAL_NAMES[index as usize]
}

pub fn is_standard_font_name(name: &[u8]) -> bool {
    CANONICAL_NAMES
        .iter()
        .any(|canonical| canonical.as_bytes() == name)
}

pub fn is_symbolic_font(index: StandardFontIndex) -> bool {
    matches!(
        index,
        StandardFontIndex::Symbol | StandardFontIndex::Dingbats
    )
}

pub fn is_fixed_font(index: StandardFontIndex) -> bool {
    matches!(
        index,
        StandardFontIndex::Courier
            | StandardFontIndex::CourierBold
            | StandardFontIndex::CourierBoldOblique
            | StandardFontIndex::CourierOblique
    )
}

pub fn get_font_data(index: StandardFontIndex) -> &'static [u8] {
    match index {
        StandardFontIndex::Courier => include_bytes!("../../assets/chromefontdata/FoxitFixed.bin"),
        StandardFontIndex::CourierBold => {
            include_bytes!("../../assets/chromefontdata/FoxitFixedBold.bin")
        }
        StandardFontIndex::CourierBoldOblique => {
            include_bytes!("../../assets/chromefontdata/FoxitFixedBoldItalic.bin")
        }
        StandardFontIndex::CourierOblique => {
            include_bytes!("../../assets/chromefontdata/FoxitFixedItalic.bin")
        }
        StandardFontIndex::Helvetica => include_bytes!("../../assets/chromefontdata/FoxitSans.bin"),
        StandardFontIndex::HelveticaBold => {
            include_bytes!("../../assets/chromefontdata/FoxitSansBold.bin")
        }
        StandardFontIndex::HelveticaBoldOblique => {
            include_bytes!("../../assets/chromefontdata/FoxitSansBoldItalic.bin")
        }
        StandardFontIndex::HelveticaOblique => {
            include_bytes!("../../assets/chromefontdata/FoxitSansItalic.bin")
        }
        StandardFontIndex::Times => include_bytes!("../../assets/chromefontdata/FoxitSerif.bin"),
        StandardFontIndex::TimesBold => {
            include_bytes!("../../assets/chromefontdata/FoxitSerifBold.bin")
        }
        StandardFontIndex::TimesBoldOblique => {
            include_bytes!("../../assets/chromefontdata/FoxitSerifBoldItalic.bin")
        }
        StandardFontIndex::TimesOblique => {
            include_bytes!("../../assets/chromefontdata/FoxitSerifItalic.bin")
        }
        StandardFontIndex::Symbol => include_bytes!("../../assets/chromefontdata/FoxitSymbol.bin"),
        StandardFontIndex::Dingbats => {
            include_bytes!("../../assets/chromefontdata/FoxitDingbats.bin")
        }
    }
}

pub fn get_generic_sans_font_data() -> &'static [u8] {
    include_bytes!("../../assets/chromefontdata/FoxitSansMM.bin")
}

pub fn get_generic_serif_font_data() -> &'static [u8] {
    include_bytes!("../../assets/chromefontdata/FoxitSerifMM.bin")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_names_and_embedded_data_match_pdfium() {
        assert_eq!(
            get_standard_font_index(b"arial,bold"),
            Some(StandardFontIndex::HelveticaBold)
        );
        assert_eq!(
            get_standard_font_index(b"TimesNewRomanPSMT"),
            Some(StandardFontIndex::Times)
        );
        assert_eq!(
            get_standard_font_index(b"ZapfDingbats"),
            Some(StandardFontIndex::Dingbats)
        );
        assert_eq!(get_standard_font_index(b"Unknown"), None);
        assert_eq!(
            get_standard_font_index(b"Arial\0Ignored"),
            Some(StandardFontIndex::Helvetica)
        );
        assert_eq!(
            get_canonical_font_name(StandardFontIndex::Times),
            "Times-Roman"
        );
        assert!(is_standard_font_name(b"Times-Roman"));
        assert!(!is_standard_font_name(b"times-roman"));
        assert!(is_symbolic_font(StandardFontIndex::Symbol));
        assert!(is_fixed_font(StandardFontIndex::CourierBold));
        assert!(get_font_data(StandardFontIndex::Helvetica).len() > 10_000);
        assert!(get_generic_sans_font_data().len() > 60_000);
        assert!(get_generic_serif_font_data().len() > 100_000);
    }
}

#[cfg(test)]
#[path = "cfx_standardfont_unittest.rs"]
mod upstream_tests;
