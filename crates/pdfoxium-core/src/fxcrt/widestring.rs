// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Selected `core/fxcrt/widestring.{h,cpp}` over owned Vec<u32> and slices.
//! Rust ownership/slicing/format! replace COW storage and C++ varargs. These
//! helpers preserve the source conversion, C-string, case and hash contracts.
use super::{
    fx_codepage::{self, FxCodePage},
    fx_extension as case, fx_string,
};
pub fn c_str(text: &[u32]) -> &[u32] {
    &text[..text.iter().position(|&ch| ch == 0).unwrap_or(text.len())]
}
pub fn eq_cstr(left: &[u32], right: &[u32]) -> bool {
    c_str(left) == c_str(right)
}
pub fn compare(left: &[u32], right: &[u32]) -> std::cmp::Ordering {
    left.cmp(right)
}
pub fn compare_cstr(left: &[u32], right: &[u32]) -> std::cmp::Ordering {
    c_str(left).cmp(c_str(right))
}
pub fn compare_no_case_cstr(left: &[u32], right: &[u32]) -> std::cmp::Ordering {
    c_str(left)
        .iter()
        .map(|&c| case::to_upper_wide(c))
        .cmp(c_str(right).iter().map(|&c| case::to_upper_wide(c)))
}
pub fn make_lower(text: &mut [u32]) {
    for ch in text.iter_mut().take_while(|c| **c != 0) {
        *ch = case::to_lower_wide(*ch);
    }
}
pub fn make_upper(text: &mut [u32]) {
    for ch in text.iter_mut().take_while(|c| **c != 0) {
        *ch = case::to_upper_wide(*ch);
    }
}
pub fn trim_whitespace_front(text: &[u32]) -> &[u32] {
    &text[text
        .iter()
        .position(|&c| !matches!(c, 9..=13 | 32))
        .unwrap_or(text.len())..]
}
pub fn trim_whitespace_back(text: &[u32]) -> &[u32] {
    &text[..text
        .iter()
        .rposition(|&c| !matches!(c, 9..=13 | 32))
        .map_or(0, |i| i + 1)]
}
pub fn trim_whitespace(text: &[u32]) -> &[u32] {
    trim_whitespace_front(trim_whitespace_back(text))
}
pub fn from_ascii(text: &[u8]) -> Vec<u32> {
    text.iter().map(|&c| u32::from(c & 0x7f)).collect()
}
pub fn from_latin1(text: &[u8]) -> Vec<u32> {
    text.iter().map(|&c| u32::from(c)).collect()
}
pub fn to_ascii(text: &[u32]) -> Vec<u8> {
    text.iter().map(|&c| (c & 0x7f) as u8).collect()
}
pub fn to_latin1(text: &[u32]) -> Vec<u8> {
    text.iter().map(|&c| (c & 0xff) as u8).collect()
}
pub fn from_def_ansi(text: &[u8]) -> Vec<u32> {
    let mut result =
        vec![0; fx_codepage::multi_byte_to_wide_char(FxCodePage::DefAnsi, text, &mut [])];
    let length = fx_codepage::multi_byte_to_wide_char(FxCodePage::DefAnsi, text, &mut result);
    result.truncate(length);
    result
}
pub fn to_def_ansi(text: &[u32]) -> Vec<u8> {
    let mut result =
        vec![0; fx_codepage::wide_char_to_multi_byte(FxCodePage::DefAnsi, text, &mut [])];
    let length = fx_codepage::wide_char_to_multi_byte(FxCodePage::DefAnsi, text, &mut result);
    result.truncate(length);
    result
}
pub fn to_utf16le(text: &[u32]) -> Vec<u8> {
    fx_string::utf16_encode(text)
        .into_iter()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}
pub fn to_ucs2le(text: &[u32]) -> Vec<u8> {
    text.iter()
        .copied()
        .filter(|c| !(0x10000..=0x10ffff).contains(c))
        .map(|c| c as u16)
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}
pub fn hash_code(text: &[u32]) -> u32 {
    text.iter().fold(0, |hash, &ch| {
        u32::wrapping_add(u32::wrapping_mul(hash, 1313), ch)
    })
}
pub fn hash_code_lowered(text: &[u32]) -> u32 {
    text.iter().fold(0, |hash, &ch| {
        u32::wrapping_add(u32::wrapping_mul(hash, 1313), case::to_lower_wide(ch))
    })
}
pub fn format_integer(value: i32) -> Vec<u32> {
    value.to_string().bytes().map(u32::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    #[test]
    fn unicode_case_cstring_boundaries_and_hashes_preserve_source_contract() {
        let mut text = wide("ß\u{1f80}\0abc");
        make_upper(&mut text);
        assert_eq!(text, wide("ß\u{1f88}\0abc"));
        make_lower(&mut text);
        assert_eq!(text, wide("ß\u{1f80}\0abc"));
        assert!(eq_cstr(&wide("abc\0x"), &wide("abc")));
        assert_ne!(
            compare(&wide("abc\0x"), &wide("abc")),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            compare_no_case_cstr(&wide("αβ\0x"), &wide("ΑΒ")),
            std::cmp::Ordering::Equal
        );
        assert_eq!(trim_whitespace(&wide("\t abc \n")), wide("abc"));
        assert_eq!(trim_whitespace(&[0xa0]), [0xa0]);
        assert_eq!(hash_code_lowered(&wide("Ä\0X")), hash_code(&wide("ä\0x")));
        for value in [i32::MIN, -123456, 0, 123456, i32::MAX] {
            assert_eq!(format_integer(value), wide(&value.to_string()));
        }
    }
    #[test]
    fn bounded_ascii_latin1_ansi_and_utf_outputs_preserve_embedded_zeros() {
        assert_eq!(from_ascii(&[0, 0x80, 0xff]), [0, 0, 127]);
        assert_eq!(from_latin1(&[0, 0x80, 0xff]), [0, 128, 255]);
        assert_eq!(to_ascii(&[0, 128, 255, 0x141]), [0, 0, 127, 65]);
        assert_eq!(to_latin1(&[0, 0x100, 0x141]), [0, 0, 65]);
        assert_eq!(from_def_ansi(&[0, 0xff]), [0, 255]);
        assert_eq!(to_def_ansi(&[0, 0x141, 0xff]), [0, 255]);
        assert_eq!(
            to_utf16le(&[65, 0, 0x1f3a8]),
            [65, 0, 0, 0, 0x3c, 0xd8, 0xa8, 0xdf, 0, 0]
        );
        assert_eq!(to_ucs2le(&[65, 0, 0x1f3a8]), [65, 0, 0, 0, 0, 0]);
    }
}
