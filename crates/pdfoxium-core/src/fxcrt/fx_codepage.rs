// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Linux translation of `core/fxcrt/fx_codepage.{h,cpp}` and its forward header.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum FxCodePage {
    DefAnsi = 0,
    Symbol = 42,
    MsdosUs = 437,
    ArabicAsmo708 = 708,
    MsdosGreek1 = 737,
    MsdosBaltic = 775,
    MsdosWesternEuropean = 850,
    MsdosEasternEuropean = 852,
    MsdosCyrillic = 855,
    MsdosTurkish = 857,
    MsdosPortuguese = 860,
    MsdosIcelandic = 861,
    MsdosHebrew = 862,
    MsdosFrenchCanadian = 863,
    MsdosArabic = 864,
    MsdosNorwegian = 865,
    MsdosRussian = 866,
    MsdosGreek2 = 869,
    MsdosThai = 874,
    ShiftJis = 932,
    ChineseSimplified = 936,
    Hangul = 949,
    ChineseTraditional = 950,
    Utf16Le = 1200,
    Utf16Be = 1201,
    MswinEasternEuropean = 1250,
    MswinCyrillic = 1251,
    MswinWesternEuropean = 1252,
    MswinGreek = 1253,
    MswinTurkish = 1254,
    MswinHebrew = 1255,
    MswinArabic = 1256,
    MswinBaltic = 1257,
    MswinVietnamese = 1258,
    Johab = 1361,
    MacRoman = 10000,
    MacShiftJis = 10001,
    MacChineseTraditional = 10002,
    MacKorean = 10003,
    MacArabic = 10004,
    MacHebrew = 10005,
    MacGreek = 10006,
    MacCyrillic = 10007,
    MacChineseSimplified = 10008,
    MacThai = 10021,
    MacEasternEuropean = 10029,
    MacTurkish = 10081,
    Utf8 = 65001,
    Failure = 65535,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FxCharset {
    Ansi = 0,
    Default = 1,
    Symbol = 2,
    MacRoman = 77,
    MacShiftJis = 78,
    MacKorean = 79,
    MacChineseSimplified = 80,
    MacChineseTraditional = 81,
    MacHebrew = 83,
    MacArabic = 84,
    MacGreek = 85,
    MacTurkish = 86,
    MacThai = 87,
    MacEasternEuropean = 88,
    MacCyrillic = 89,
    ShiftJis = 128,
    Hangul = 129,
    Johab = 130,
    ChineseSimplified = 134,
    ChineseTraditional = 136,
    MswinGreek = 161,
    MswinTurkish = 162,
    MswinVietnamese = 163,
    MswinHebrew = 177,
    MswinArabic = 178,
    MswinBaltic = 186,
    MswinCyrillic = 204,
    Thai = 222,
    MswinEasternEuropean = 238,
    Us = 254,
    Oem = 255,
}

const CHARSET_CODEPAGES: &[(FxCharset, FxCodePage)] = &[
    (FxCharset::Ansi, FxCodePage::MswinWesternEuropean),
    (FxCharset::Default, FxCodePage::DefAnsi),
    (FxCharset::Symbol, FxCodePage::Symbol),
    (FxCharset::MacRoman, FxCodePage::MacRoman),
    (FxCharset::MacShiftJis, FxCodePage::MacShiftJis),
    (FxCharset::MacKorean, FxCodePage::MacKorean),
    (
        FxCharset::MacChineseSimplified,
        FxCodePage::MacChineseSimplified,
    ),
    (
        FxCharset::MacChineseTraditional,
        FxCodePage::MacChineseTraditional,
    ),
    (FxCharset::MacHebrew, FxCodePage::MacHebrew),
    (FxCharset::MacArabic, FxCodePage::MacArabic),
    (FxCharset::MacGreek, FxCodePage::MacGreek),
    (FxCharset::MacTurkish, FxCodePage::MacTurkish),
    (FxCharset::MacThai, FxCodePage::MacThai),
    (
        FxCharset::MacEasternEuropean,
        FxCodePage::MacEasternEuropean,
    ),
    (FxCharset::MacCyrillic, FxCodePage::MacCyrillic),
    (FxCharset::ShiftJis, FxCodePage::ShiftJis),
    (FxCharset::Hangul, FxCodePage::Hangul),
    (FxCharset::Johab, FxCodePage::Johab),
    (FxCharset::ChineseSimplified, FxCodePage::ChineseSimplified),
    (
        FxCharset::ChineseTraditional,
        FxCodePage::ChineseTraditional,
    ),
    (FxCharset::MswinGreek, FxCodePage::MswinGreek),
    (FxCharset::MswinTurkish, FxCodePage::MswinTurkish),
    (FxCharset::MswinVietnamese, FxCodePage::MswinVietnamese),
    (FxCharset::MswinHebrew, FxCodePage::MswinHebrew),
    (FxCharset::MswinArabic, FxCodePage::MswinArabic),
    (FxCharset::MswinBaltic, FxCodePage::MswinBaltic),
    (FxCharset::MswinCyrillic, FxCodePage::MswinCyrillic),
    (FxCharset::Thai, FxCodePage::MsdosThai),
    (
        FxCharset::MswinEasternEuropean,
        FxCodePage::MswinEasternEuropean,
    ),
    (FxCharset::Us, FxCodePage::MsdosUs),
    (FxCharset::Oem, FxCodePage::MsdosWesternEuropean),
];

pub fn get_acp() -> FxCodePage {
    FxCodePage::DefAnsi
}

pub fn codepage_from_charset(charset: FxCharset) -> FxCodePage {
    let index = CHARSET_CODEPAGES.partition_point(|entry| (entry.0 as u8) < charset as u8);
    CHARSET_CODEPAGES
        .get(index)
        .filter(|entry| entry.0 == charset)
        .map_or(FxCodePage::Failure, |entry| entry.1)
}

pub fn charset_from_codepage(codepage: FxCodePage) -> FxCharset {
    CHARSET_CODEPAGES
        .iter()
        .find(|entry| entry.1 == codepage)
        .map_or(FxCharset::Ansi, |entry| entry.0)
}

pub fn charset_from_int(value: i32) -> FxCharset {
    CHARSET_CODEPAGES
        .iter()
        .find(|entry| entry.0 as i32 == value)
        .map_or(FxCharset::Ansi, |entry| entry.0)
}

pub fn charset_is_cjk(charset: FxCharset) -> bool {
    matches!(
        charset,
        FxCharset::ChineseSimplified
            | FxCharset::ChineseTraditional
            | FxCharset::Hangul
            | FxCharset::ShiftJis
    )
}

pub fn charset_flag(charset: FxCharset) -> u32 {
    match charset {
        FxCharset::Ansi => 1 << 0,
        FxCharset::Symbol => 1 << 1,
        FxCharset::ShiftJis => 1 << 2,
        FxCharset::Hangul => 1 << 3,
        FxCharset::Johab => 1 << 4,
        FxCharset::ChineseSimplified => 1 << 5,
        FxCharset::ChineseTraditional => 1 << 6,
        FxCharset::MswinGreek => 1 << 7,
        FxCharset::MswinTurkish => 1 << 8,
        FxCharset::MswinVietnamese => 1 << 9,
        FxCharset::MswinHebrew => 1 << 10,
        FxCharset::MswinArabic => 1 << 11,
        FxCharset::MswinBaltic => 1 << 12,
        FxCharset::MswinCyrillic => 1 << 13,
        FxCharset::Thai => 1 << 14,
        FxCharset::MswinEasternEuropean => 1 << 15,
        FxCharset::Oem => 1 << 16,
        _ => 0,
    }
}

pub fn wide_char_to_multi_byte(_codepage: FxCodePage, input: &[u32], output: &mut [u8]) -> usize {
    let mut len = 0;
    for &ch in input {
        if ch < 0x100 {
            if let Some(slot) = output.get_mut(len) {
                *slot = ch as u8;
            }
            len += 1;
        }
    }
    len
}

pub fn multi_byte_to_wide_char(_codepage: FxCodePage, input: &[u8], output: &mut [u32]) -> usize {
    for (slot, &byte) in output.iter_mut().zip(input) {
        *slot = u32::from(byte);
    }
    input.len()
}

#[rustfmt::skip]
mod tables;
pub use tables::CHARSET_UNICODES;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charset_codepage_lookup_and_linux_conversion() {
        assert_eq!(get_acp(), FxCodePage::DefAnsi);
        assert_eq!(
            codepage_from_charset(FxCharset::Thai),
            FxCodePage::MsdosThai
        );
        assert_eq!(
            charset_from_codepage(FxCodePage::MsdosThai),
            FxCharset::Thai
        );
        assert_eq!(charset_from_int(204), FxCharset::MswinCyrillic);
        assert_eq!(charset_from_int(42), FxCharset::Ansi);
        assert!(charset_is_cjk(FxCharset::ShiftJis));
        assert!(!charset_is_cjk(FxCharset::MacShiftJis));
        assert_eq!(charset_flag(FxCharset::Thai), 1 << 14);
        let mut bytes = [0; 2];
        assert_eq!(
            wide_char_to_multi_byte(FxCodePage::Utf8, &[0x41, 0x100, 0xff], &mut bytes[..1]),
            2
        );
        assert_eq!(bytes[0], b'A');
        let mut wide = [0; 1];
        assert_eq!(
            multi_byte_to_wide_char(FxCodePage::Utf8, &[0x80, 0x41], &mut wide),
            2
        );
        assert_eq!(wide, [0x80]);
        assert_eq!(CHARSET_UNICODES.len(), 8);
        assert_eq!(CHARSET_UNICODES[0].0, FxCharset::Thai);
        assert_eq!(CHARSET_UNICODES[0].1[0], 0x20ac);
        assert_eq!(CHARSET_UNICODES[0].1[33], 0x0e01);
    }
}
