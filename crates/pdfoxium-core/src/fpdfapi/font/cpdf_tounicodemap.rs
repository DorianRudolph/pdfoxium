// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/font/cpdf_tounicodemap.{h,cpp}`.

use std::collections::HashMap;

use crate::fxcrt::fx_string::string_to_int;

use super::cpdf_cid2unicodemap::CpdfCid2UnicodeMap;
use super::cpdf_cmapparser::CidSet;
use crate::fpdfapi::parser::cpdf_simple_parser::CpdfSimpleParser;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
use crate::fpdfapi::parser::fpdf_parser_utility::pdf_char_is_whitespace;

const OUT_OF_SPEC_BF_LIMIT: i32 = 160_000;

enum ParsedRange<'a> {
    CodeWords { low: u32, words: Vec<&'a [u8]> },
    Single { low: u32, high: u32, start: u32 },
    Multi { low: u32, values: Vec<Vec<u32>> },
}

pub struct CpdfToUnicodeMap {
    map: HashMap<u32, u32>,
    reverse_map: HashMap<u32, u32>,
    base_map: Option<CpdfCid2UnicodeMap>,
    multi_char_vec: Vec<Vec<u32>>,
}

impl CpdfToUnicodeMap {
    pub fn from_stream(stream: &CpdfStream) -> Self {
        let mut acc = CpdfStreamAcc::new(stream);
        acc.load_all_data_filtered();
        Self::from_data(acc.get_span())
    }

    pub fn from_data(data: &[u8]) -> Self {
        let mut this = Self {
            map: HashMap::new(),
            reverse_map: HashMap::new(),
            base_map: None,
            multi_char_vec: Vec::new(),
        };
        let mut cid_set = CidSet::Unknown;
        let mut parser = CpdfSimpleParser::new(data);
        let mut previous_word: &[u8] = &[];
        loop {
            let mut word = parser.get_word();
            if word.is_empty() {
                break;
            }
            match word {
                b"beginbfchar" => word = this.handle_begin_bfchar(&mut parser, previous_word),
                b"beginbfrange" => word = this.handle_begin_bfrange(&mut parser, previous_word),
                b"/Adobe-Korea1-UCS2" => cid_set = CidSet::Korea1,
                b"/Adobe-Japan1-UCS2" => cid_set = CidSet::Japan1,
                b"/Adobe-CNS1-UCS2" => cid_set = CidSet::CNS1,
                b"/Adobe-GB1-UCS2" => cid_set = CidSet::GB1,
                _ => {}
            }
            previous_word = word;
        }
        if cid_set != CidSet::Unknown {
            this.base_map = Some(CpdfCid2UnicodeMap::new(cid_set));
        }
        this
    }

    pub fn lookup(&self, charcode: u32) -> Vec<u32> {
        let Some(&value) = self.map.get(&charcode) else {
            return self.base_map.as_ref().map_or_else(Vec::new, |base| {
                vec![base.unicode_from_cid(charcode as u16)]
            });
        };
        let unicode = value & 0xffff;
        if unicode != 0xffff {
            return vec![unicode];
        }
        self.multi_char_vec
            .get((value >> 16) as usize)
            .cloned()
            .unwrap_or_default()
    }

    pub fn reverse_lookup(&self, unicode: u32) -> u32 {
        if (0x10000..=0x10ffff).contains(&unicode) {
            // A ToUnicode destination stores UTF-16 units. A supplementary
            // scalar passed by SetText must reverse-map the complete pair,
            // rather than the internal multi-character table indicator.
            let supplementary = unicode - 0x10000;
            let high = 0xd800 + (supplementary >> 10);
            let low = 0xdc00 + (supplementary & 0x3ff);
            if let Some(code) = self
                .map
                .iter()
                .filter_map(|(&code, &value)| {
                    if value & 0xffff != 0xffff {
                        return None;
                    }
                    self.multi_char_vec
                        .get((value >> 16) as usize)
                        .filter(|units| units.as_slice() == [high, low])
                        .map(|_| code)
                })
                .min()
            {
                return code;
            }
        }
        self.reverse_map.get(&unicode).copied().unwrap_or(0)
    }

    pub fn unicode_count_by_charcode(&self, charcode: u32) -> usize {
        self.reverse_map
            .values()
            .filter(|&&code| code == charcode)
            .count()
    }

    pub fn string_to_code(word: &[u8]) -> Option<u32> {
        let middle = word.strip_prefix(b"<")?.strip_suffix(b">")?;
        if middle.is_empty() {
            return None;
        }
        let mut code = 0u32;
        for &byte in middle {
            if pdf_char_is_whitespace(byte) {
                continue;
            }
            let digit = hex_digit(byte)?;
            code = code.checked_mul(16)?.checked_add(digit)?;
        }
        Some(code)
    }

    pub fn string_to_wide_string(word: &[u8]) -> Vec<u32> {
        let Some(middle) = word.strip_prefix(b"<").and_then(|w| w.strip_suffix(b">")) else {
            return Vec::new();
        };
        let mut result = Vec::new();
        let mut byte_pos = 0;
        let mut ch = 0u32;
        for &byte in middle {
            if pdf_char_is_whitespace(byte) {
                continue;
            }
            let Some(digit) = hex_digit(byte) else {
                break;
            };
            ch = ch * 16 + digit;
            byte_pos += 1;
            if byte_pos == 4 {
                result.push(ch);
                byte_pos = 0;
                ch = 0;
            }
        }
        result
    }

    fn handle_begin_bfchar<'a>(
        &mut self,
        parser: &mut CpdfSimpleParser<'a>,
        previous_word: &[u8],
    ) -> &'a [u8] {
        let raw_count = string_to_int(previous_word);
        let mut valid = (0..=OUT_OF_SPEC_BF_LIMIT).contains(&raw_count);
        let expected = if valid { raw_count as usize } else { 0 };
        let mut entries = Vec::with_capacity(expected);
        let word = loop {
            let word = parser.get_word();
            if word.is_empty() || word == b"endbfchar" {
                break word;
            }
            if !valid {
                continue;
            }
            // ToUnicode maps source character codes, not descendant-font
            // CIDs. Type0 source codes can contain up to four bytes (PDF
            // 32000-1 sections9.7.6.2 and9.10.3); the native16-bit CID guard
            // incorrectly discards valid three/four-byte mappings.
            let Some(code) = Self::string_to_code(word) else {
                valid = false;
                continue;
            };
            let destination = parser.get_word();
            entries.push((code, destination));
            if entries.len() > expected {
                valid = false;
            }
        };
        if valid && entries.len() == expected {
            for (code, destination) in entries {
                self.set_code(code, Self::string_to_wide_string(destination));
            }
        }
        word
    }

    fn handle_begin_bfrange<'a>(
        &mut self,
        parser: &mut CpdfSimpleParser<'a>,
        previous_word: &[u8],
    ) -> &'a [u8] {
        let raw_count = string_to_int(previous_word);
        let mut valid = (0..=OUT_OF_SPEC_BF_LIMIT).contains(&raw_count);
        let expected = if valid { raw_count as usize } else { 0 };
        let mut ranges = Vec::with_capacity(expected);
        let word = loop {
            let word = parser.get_word();
            if word.is_empty() || word == b"endbfrange" {
                break word;
            }
            if !valid {
                continue;
            }
            let Some(low) = Self::string_to_code(word) else {
                valid = false;
                continue;
            };
            let Some(high_word) = Self::string_to_code(parser.get_word()) else {
                valid = false;
                continue;
            };
            let high = (low & 0xffff_ff00) | (high_word & 0xff);
            // Preserve the source's final-byte range bound: at most256
            // mappings are expanded, independent of the u32 source value.
            if low > high {
                valid = false;
                continue;
            }
            let start = parser.get_word();
            if start == b"[" {
                let mut words = Vec::with_capacity((high - low + 1) as usize);
                for _ in low..=high {
                    words.push(parser.get_word());
                }
                ranges.push(ParsedRange::CodeWords { low, words });
                if ranges.len() > expected {
                    valid = false;
                    continue;
                }
                if parser.get_word() != b"]" {
                    valid = false;
                }
                continue;
            }
            let destination = Self::string_to_wide_string(start);
            if destination.len() == 1 {
                ranges.push(ParsedRange::Single {
                    low,
                    high,
                    start: destination[0],
                });
            } else {
                let mut values = Vec::with_capacity((high - low + 1) as usize);
                values.push(destination);
                for _ in 0..high - low {
                    values.push(string_data_add(values.last().unwrap()));
                }
                ranges.push(ParsedRange::Multi { low, values });
            }
            if ranges.len() > expected {
                valid = false;
            }
        };
        if valid && ranges.len() == expected {
            for range in ranges {
                match range {
                    ParsedRange::CodeWords { low, words } => {
                        for (offset, destination) in words.into_iter().enumerate() {
                            self.set_code(
                                low + offset as u32,
                                Self::string_to_wide_string(destination),
                            );
                        }
                    }
                    ParsedRange::Single { low, high, start } => {
                        for code in low..=high {
                            self.insert_into_maps(code, start + (code - low));
                        }
                    }
                    ParsedRange::Multi { low, values } => {
                        for (offset, destination) in values.into_iter().enumerate() {
                            self.insert_into_maps(low + offset as u32, self.multi_char_indicator());
                            self.multi_char_vec.push(destination);
                        }
                    }
                }
            }
        }
        word
    }

    fn multi_char_indicator(&self) -> u32 {
        (self.multi_char_vec.len() as u32)
            .checked_mul(0x1_0000)
            .and_then(|value| value.checked_add(0xffff))
            .unwrap_or(0)
    }

    fn set_code(&mut self, code: u32, destination: Vec<u32>) {
        match destination.len() {
            0 => {}
            1 => self.insert_into_maps(code, destination[0]),
            _ => {
                self.insert_into_maps(code, self.multi_char_indicator());
                self.multi_char_vec.push(destination);
            }
        }
    }

    fn insert_into_maps(&mut self, code: u32, destination: u32) {
        // Later CMap entries supersede earlier entries in the forward map.
        self.map.insert(code, destination);
        self.reverse_map.insert(destination, code);
    }
}

fn string_data_add(value: &[u32]) -> Vec<u32> {
    let mut result = Vec::with_capacity(value.len() + 1);
    let mut carry = 1u32;
    for &ch in value.iter().rev() {
        let next = ch.wrapping_add(carry);
        if next < ch {
            result.push(0);
        } else {
            result.push(next);
            carry = 0;
        }
    }
    if carry != 0 {
        result.push(carry);
    }
    result.reverse();
    result
}

fn hex_digit(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'a'..=b'f' => Some(u32::from(byte - b'a' + 10)),
        b'A'..=b'F' => Some(u32::from(byte - b'A' + 10)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_and_wide_string_helpers_follow_pdfium() {
        assert_eq!(CpdfToUnicodeMap::string_to_code(b"<00\n0\r1>"), Some(1));
        assert_eq!(CpdfToUnicodeMap::string_to_code(b"<c 2>"), Some(194));
        assert_eq!(CpdfToUnicodeMap::string_to_code(b"<100000000>"), None);
        assert_eq!(CpdfToUnicodeMap::string_to_code(b"<00NN>"), None);
        assert_eq!(
            CpdfToUnicodeMap::string_to_wide_string(b"<c2ab FaAb12>"),
            vec![0xc2ab, 0xfaab]
        );
        assert_eq!(
            CpdfToUnicodeMap::string_to_wide_string(b"<c2abab>"),
            vec![0xc2ab]
        );
    }

    #[test]
    fn bfchar_and_bfrange_override_and_reverse_lookup() {
        let map = CpdfToUnicodeMap::from_data(
            b"1 beginbfrange <00> <ff> <0000> endbfrange\n\
              3 beginbfchar <41> <00f3> <42> <00ba> <43> <20ac> endbfchar\n\
              1 beginbfrange <70> <71> [<0041> <0042>] endbfrange",
        );
        assert_eq!(map.lookup(0x41), vec![0xf3]);
        assert_eq!(map.lookup(0x42), vec![0xba]);
        assert_eq!(map.lookup(0x43), vec![0x20ac]);
        assert_eq!(map.lookup(0x44), vec![0x44]);
        assert_eq!(map.lookup(0x70), vec![0x41]);
        assert_eq!(map.reverse_lookup(0x20ac), 0x43);
    }

    #[test]
    fn invalid_sections_are_discarded_and_multi_unit_values_preserved() {
        let invalid = CpdfToUnicodeMap::from_data(b"1 beginbfchar <1><0041><2><0042> endbfchar");
        assert!(invalid.lookup(1).is_empty());
        let map = CpdfToUnicodeMap::from_data(
            b"1 beginbfchar <01> <d841de76> endbfchar\n\
              1 beginbfrange <10> <11> <00410042> endbfrange",
        );
        assert_eq!(map.lookup(1), vec![0xd841, 0xde76]);
        assert_eq!(map.lookup(0x10), vec![0x41, 0x42]);
        assert_eq!(map.lookup(0x11), vec![0x41, 0x43]);
        assert_eq!(map.reverse_lookup(0x20676), 1);
        assert_eq!(map.reverse_lookup(0x20677), 0);
    }

    #[test]
    fn bfrange_rejects_bad_counts_and_keeps_pdfium_ffff_sentinel() {
        for bytes in [
            &b"1 beginbfrange <1><2><0040><4><5><0050> endbfrange"[..],
            &b"3 beginbfrange <1><2><0040><4><5><0050> endbfrange"[..],
            &b"1 beginbfrange <3><3>[<0041>} endbfrange"[..],
        ] {
            let map = CpdfToUnicodeMap::from_data(bytes);
            assert!(map.lookup(1).is_empty());
            assert_eq!(map.reverse_lookup(0x40), 0);
        }
        let map = CpdfToUnicodeMap::from_data(b"1 beginbfrange <0010><00ff><fff0> endbfrange");
        assert_eq!(map.lookup(0x10), vec![0xfff0]);
        assert_eq!(map.lookup(0x11), vec![0xfff1]);
        assert!(map.lookup(0x1f).is_empty());
        assert_eq!(map.reverse_lookup(0xffff), 0x1f);
        assert_eq!(map.reverse_lookup(0x10000), 0x20);
    }

    #[test]
    fn usecmap_base_falls_back_to_cid_unicode_table() {
        let map = CpdfToUnicodeMap::from_data(b"/Adobe-Japan1-UCS2 usecmap");
        assert_eq!(map.lookup(1), vec![0x20]);
    }
}

#[cfg(test)]
#[path = "cpdf_tounicodemap_unittest.rs"]
mod upstream_tests;
