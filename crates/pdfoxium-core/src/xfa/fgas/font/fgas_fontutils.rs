// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/font/fgas_fontutils.{h,cpp}`. These tables select XFA font
//! candidates; actual outlines and metrics still use the engine's Fontations
//! boundary. Names preserve wchar32 units and PDFium's lowered wide hash.
use crate::fxcrt::css::cfx_cssdata::hash_code_lowered_wide;
use crate::fxcrt::fx_codepage::FxCodePage;

#[path = "fgas_fontutils_data.rs"]
mod data;
use data::{FONT_MAP, UNICODE_RANGES};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FgasFontUsb {
    pub start_unicode: u16,
    pub end_unicode: u16,
    pub bit_field: u16,
    pub code_page: FxCodePage,
}
impl FgasFontUsb {
    pub const NO_BIT_FIELD: u16 = 999;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FgasFontInfo {
    pub font_name_hash: u32,
    pub ps_name: &'static str,
    pub replacement_font: Option<&'static str>,
    pub styles: u16,
    pub code_page: FxCodePage,
}

pub fn get_unicode_bit_field(unicode: u32) -> Option<&'static FgasFontUsb> {
    // Source ranges include both endpoints. Native upper_bound's strict `>`
    // skips each range's final code point; lower_bound preserves that endpoint.
    // The source's A830 gap also ended at A8FF, overlapping the next two ranges
    // and violating binary-search ordering. Its end is corrected to A83F in
    // the table; the separate A8E0..A8FF gap already covers the following hole.
    let index = UNICODE_RANGES.partition_point(|entry| u32::from(entry.end_unicode) < unicode);
    UNICODE_RANGES
        .get(index)
        .filter(|entry| u32::from(entry.start_unicode) <= unicode)
}

fn font_info_by_hash(hash: u32) -> Option<&'static FgasFontInfo> {
    let index = FONT_MAP.partition_point(|entry| entry.font_name_hash < hash);
    FONT_MAP
        .get(index)
        .filter(|entry| entry.font_name_hash == hash)
}

pub fn font_name_to_english_name(name: &[u32]) -> Vec<u32> {
    font_info_by_hash(hash_code_lowered_wide(name)).map_or_else(
        || name.to_vec(),
        |entry| entry.ps_name.bytes().map(u32::from).collect(),
    )
}

pub fn font_info_by_font_name(name: &[u32]) -> Option<&'static FgasFontInfo> {
    // Only this source entry point removes ASCII spaces. Hash the filtered
    // name through the shared helper, retaining its exact simple-lower rules.
    let compact: Vec<u32> = name.iter().copied().filter(|&ch| ch != 0x20).collect();
    font_info_by_hash(hash_code_lowered_wide(&compact))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }

    // fgas_fontutils_unittest.cpp: GetUnicodeBitField, all native assertions.
    #[test]
    fn get_unicode_bit_field_native_vectors() {
        let result = get_unicode_bit_field(0).unwrap();
        assert_eq!(result.bit_field, 0);
        assert_eq!(result.code_page, FxCodePage::MswinWesternEuropean);
        assert!(get_unicode_bit_field(65535).is_none());
        let result = get_unicode_bit_field(1313).unwrap();
        assert_eq!(result.bit_field, 9);
        assert_eq!(result.code_page, FxCodePage::Failure);
        let result = get_unicode_bit_field(14321).unwrap();
        assert_eq!(result.bit_field, 59);
        assert_eq!(result.code_page, FxCodePage::Failure);
    }

    // fgas_fontutils_unittest.cpp: FontNameToEnglishName, all four vectors.
    #[test]
    fn font_name_to_english_name_native_vectors() {
        for (name, expected) in [
            ("Myriad Pro", "Myriad Pro"),
            ("mYriad pRo", "mYriad pRo"),
            ("MyriadPro", "MyriadPro"),
            ("mYriadpRo", "MyriadPro"),
        ] {
            assert_eq!(font_name_to_english_name(&wide(name)), wide(expected));
        }
    }

    // fgas_fontutils_unittest.cpp: FontInfoByFontName, all four vectors.
    #[test]
    fn font_info_by_font_name_native_vectors() {
        for name in ["Myriad Pro", "mYriad pRo", "MyriadPro", "mYriadpRo"] {
            assert!(font_info_by_font_name(&wide(name)).is_some(), "{name}");
        }
    }

    #[test]
    fn inclusive_range_edges_and_corrected_phags_pa_gap() {
        for pair in UNICODE_RANGES.windows(2) {
            assert!(pair[0].end_unicode < pair[1].start_unicode);
        }
        for range in UNICODE_RANGES {
            assert_eq!(
                get_unicode_bit_field(u32::from(range.start_unicode)),
                Some(range)
            );
            assert_eq!(
                get_unicode_bit_field(u32::from(range.end_unicode)),
                Some(range)
            );
        }
        assert_eq!(get_unicode_bit_field(0x7f).unwrap().bit_field, 0);
        assert_eq!(get_unicode_bit_field(0xff).unwrap().bit_field, 1);
        assert_eq!(
            get_unicode_bit_field(0xa83f).unwrap().bit_field,
            FgasFontUsb::NO_BIT_FIELD
        );
        assert_eq!(get_unicode_bit_field(0xa840).unwrap().bit_field, 53);
        assert_eq!(get_unicode_bit_field(0xa87f).unwrap().bit_field, 53);
        assert_eq!(get_unicode_bit_field(0xa880).unwrap().bit_field, 115);
        assert_eq!(get_unicode_bit_field(0xa8df).unwrap().bit_field, 115);
        assert_eq!(
            get_unicode_bit_field(0xa8e0).unwrap().bit_field,
            FgasFontUsb::NO_BIT_FIELD
        );
        for missing in [0xfff0, 0xffff, 0x10000, 0x10ffff, u32::MAX] {
            assert!(get_unicode_bit_field(missing).is_none(), "{missing:x}");
        }
    }

    #[test]
    fn font_aliases_preserve_hash_order_and_space_rules() {
        assert!(FONT_MAP
            .windows(2)
            .all(|pair| pair[0].font_name_hash < pair[1].font_name_hash));
        let canonical = font_info_by_font_name(&wide("MyriadPro")).unwrap();
        let spaced = font_info_by_font_name(&wide(" M y r i a d P r o ")).unwrap();
        assert!(std::ptr::eq(canonical, spaced));
        assert_eq!(canonical.ps_name, "MyriadPro");
        assert_eq!(canonical.code_page, FxCodePage::MswinWesternEuropean);
        assert!(canonical
            .replacement_font
            .is_some_and(|name| !name.is_empty()));
        for unknown in ["", "unknown-font-for-xfa", "Myriad\tPro"] {
            assert!(font_info_by_font_name(&wide(unknown)).is_none());
            assert_eq!(font_name_to_english_name(&wide(unknown)), wide(unknown));
        }
        let invalid_wide = [0xd800, 0x110000];
        assert_eq!(font_name_to_english_name(&invalid_wide), invalid_wide);
    }
}
