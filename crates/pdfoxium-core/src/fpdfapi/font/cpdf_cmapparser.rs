// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/font/cpdf_cmapparser.{h,cpp}`.

use super::cpdf_cmap::{CidRange, CodingScheme, CpdfCmap};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CidSet {
    Unknown = 0,
    GB1 = 1,
    CNS1 = 2,
    Japan1 = 3,
    Korea1 = 4,
    Unicode = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodeRange {
    pub char_size: usize,
    pub lower: [u8; 4],
    pub upper: [u8; 4],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum Status {
    #[default]
    Start,
    CidChar,
    CidRange,
    Registry,
    Ordering,
    Supplement,
    WMode,
    CodeSpaceRange,
}

pub struct CmapParser<'a> {
    cmap: &'a mut CpdfCmap,
    status: Status,
    code_seq: usize,
    ranges: Vec<CodeRange>,
    pending_ranges: Vec<CodeRange>,
    additional_mappings: Vec<CidRange>,
    last_word: Vec<u8>,
    code_points: [u32; 4],
}

impl<'a> CmapParser<'a> {
    pub fn new(cmap: &'a mut CpdfCmap) -> Self {
        Self {
            cmap,
            status: Status::Start,
            code_seq: 0,
            ranges: Vec::new(),
            pending_ranges: Vec::new(),
            additional_mappings: Vec::new(),
            last_word: Vec::new(),
            code_points: [0; 4],
        }
    }

    pub fn parse_word(&mut self, word: &[u8]) {
        if word.is_empty() {
            return;
        }
        match word {
            b"begincidchar" => {
                self.status = Status::CidChar;
                self.code_seq = 0;
            }
            b"begincidrange" => {
                self.status = Status::CidRange;
                self.code_seq = 0;
            }
            b"endcidrange" | b"endcidchar" => self.status = Status::Start,
            b"/WMode" => self.status = Status::WMode,
            b"/Registry" => self.status = Status::Registry,
            b"/Ordering" => self.status = Status::Ordering,
            b"/Supplement" => self.status = Status::Supplement,
            b"begincodespacerange" => {
                self.status = Status::CodeSpaceRange;
                self.code_seq = 0;
            }
            b"usecmap" => {}
            _ => match self.status {
                Status::CidChar | Status::CidRange => self.handle_cid(word),
                Status::Registry | Status::Supplement => self.status = Status::Start,
                Status::Ordering => {
                    self.cmap
                        .set_charset(charset_from_ordering(word.get(2..).unwrap_or_default()));
                    self.status = Status::Start;
                }
                Status::WMode => {
                    self.cmap.set_vertical(get_code(word) != 0);
                    self.status = Status::Start;
                }
                Status::CodeSpaceRange => self.handle_code_space_range(word),
                Status::Start => {}
            },
        }
        self.last_word.clear();
        self.last_word.extend_from_slice(word);
    }

    pub fn finish(mut self) {
        self.cmap
            .set_additional_mappings(std::mem::take(&mut self.additional_mappings));
        self.cmap
            .set_mixed_four_byte_leading_ranges(std::mem::take(&mut self.ranges));
    }

    fn handle_cid(&mut self, word: &[u8]) {
        let required = if self.status == Status::CidChar { 2 } else { 3 };
        self.code_points[self.code_seq] = get_code(word);
        self.code_seq += 1;
        if self.code_seq < required {
            return;
        }
        let start = self.code_points[0];
        let (end, cid) = if required == 2 {
            (start, self.code_points[1] as u16)
        } else {
            (self.code_points[1], self.code_points[2] as u16)
        };
        if end < 65_536 {
            self.cmap.set_direct_range(start, end, cid);
        } else {
            self.additional_mappings.push(CidRange {
                start_code: start,
                end_code: end,
                start_cid: cid,
            });
        }
        self.code_seq = 0;
    }

    fn handle_code_space_range(&mut self, word: &[u8]) {
        if word != b"endcodespacerange" {
            if word.first() != Some(&b'<') {
                return;
            }
            if self.code_seq % 2 == 1 {
                if let Some(range) = get_code_range(&self.last_word, word) {
                    self.pending_ranges.push(range);
                }
            }
            self.code_seq += 1;
            return;
        }
        let segments = self.ranges.len() + self.pending_ranges.len();
        if segments == 1 {
            let first = self
                .ranges
                .first()
                .or_else(|| self.pending_ranges.first())
                .unwrap();
            self.cmap.set_coding_scheme(if first.char_size == 2 {
                CodingScheme::TwoBytes
            } else {
                CodingScheme::OneByte
            });
        } else if segments > 1 {
            self.cmap.set_coding_scheme(CodingScheme::MixedFourBytes);
            self.ranges.append(&mut self.pending_ranges);
        }
        self.status = Status::Start;
    }
}

fn hex_char_to_int(ch: u8) -> u8 {
    match ch {
        b'0'..=b'9' => ch - b'0',
        b'a'..=b'f' => ch - b'a' + 10,
        b'A'..=b'F' => ch - b'A' + 10,
        _ => 0,
    }
}

pub fn get_code(word: &[u8]) -> u32 {
    let Some((&first, rest)) = word.split_first() else {
        return 0;
    };
    let (radix, digits) = if first == b'<' {
        (16u32, rest)
    } else {
        (10u32, word)
    };
    let mut value = 0u32;
    for &ch in digits {
        let digit = if radix == 16 {
            if !ch.is_ascii_hexdigit() {
                break;
            }
            u32::from(hex_char_to_int(ch))
        } else {
            if !ch.is_ascii_digit() {
                break;
            }
            u32::from(ch - b'0')
        };
        let Some(next) = value.checked_mul(radix).and_then(|v| v.checked_add(digit)) else {
            return 0;
        };
        value = next;
    }
    value
}

pub fn get_code_range(first: &[u8], second: &[u8]) -> Option<CodeRange> {
    if first.first() != Some(&b'<') {
        return None;
    }
    let closing = first
        .iter()
        .position(|&ch| ch == b'>')
        .unwrap_or(first.len());
    let char_size = (closing - 1) / 2;
    if char_size > 4 {
        return None;
    }
    let mut range = CodeRange {
        char_size,
        lower: [0; 4],
        upper: [0; 4],
    };
    for index in 0..char_size {
        let i1 = index * 2 + 1;
        let i2 = i1 + 1;
        let lower1 = *first.get(i1)?;
        let lower2 = *first.get(i2)?;
        range.lower[index] = hex_char_to_int(lower1) * 16 + hex_char_to_int(lower2);
        let upper1 = second.get(i1).copied().unwrap_or(b'0');
        let upper2 = second.get(i2).copied().unwrap_or(b'0');
        range.upper[index] = hex_char_to_int(upper1) * 16 + hex_char_to_int(upper2);
    }
    Some(range)
}

pub fn charset_from_ordering(ordering: &[u8]) -> CidSet {
    match ordering {
        b"GB1" => CidSet::GB1,
        b"CNS1" => CidSet::CNS1,
        b"Japan1" => CidSet::Japan1,
        b"Korea1" => CidSet::Korea1,
        b"UCS" => CidSet::Unicode,
        _ => CidSet::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmap_code_helpers_match_pdfium_cases() {
        assert_eq!(get_code(b""), 0);
        assert_eq!(get_code(b"<"), 0);
        assert_eq!(get_code(b"<c2"), 194);
        assert_eq!(get_code(b"<A2z"), 162);
        assert_eq!(get_code(b"12d"), 12);
        assert_eq!(get_code(b"<FFFFFFFF"), u32::MAX);
        assert_eq!(get_code(b"<100000000"), 0);
        assert_eq!(get_code_range(b"A", b""), None);
        assert_eq!(get_code_range(b"<aaaaaaaaaa>", b""), None);
        let range = get_code_range(b"<12345678>", b"<87654321>").unwrap();
        assert_eq!(range.char_size, 4);
        assert_eq!(range.lower, [18, 52, 86, 120]);
        assert_eq!(range.upper, [135, 101, 67, 33]);
        assert_eq!(get_code_range(b"<a1>", b"").unwrap().upper[0], 0);
        assert_eq!(charset_from_ordering(b"Japan1"), CidSet::Japan1);
    }
}

#[cfg(test)]
#[path = "cpdf_cmapparser_unittest.rs"]
mod upstream_tests;
