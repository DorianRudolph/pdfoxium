// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Embedded CMap storage and CID lookup from `cpdf_cmap.{h,cpp}`.
//! The bundled mappings come from the pinned PDFium CMap tables.

use super::cpdf_cmapparser::{CidSet, CmapParser, CodeRange};
use crate::fpdfapi::cmaps::{fpdf_cmaps, maps_for_charset};
use crate::fpdfapi::parser::cpdf_simple_parser::CpdfSimpleParser;

const DIRECT_MAP_TABLE_SIZE: usize = 65_536;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodingScheme {
    OneByte,
    TwoBytes,
    MixedTwoBytes,
    MixedFourBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CidCoding {
    Unknown,
    GB,
    Big5,
    Jis,
    Korea,
    Ucs2,
    Cid,
    Utf16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CidRange {
    pub start_code: u32,
    pub end_code: u32,
    pub start_cid: u16,
}

pub struct CpdfCmap {
    loaded: bool,
    vertical: bool,
    charset: CidSet,
    coding_scheme: CodingScheme,
    coding: CidCoding,
    mixed_two_byte_leading_bytes: [bool; 256],
    direct_charcode_to_cid: Vec<u16>,
    additional_mappings: Vec<CidRange>,
    mixed_four_byte_leading_ranges: Vec<CodeRange>,
    embed_map: Option<(&'static [fpdf_cmaps::CMap], usize)>,
}

impl CpdfCmap {
    pub fn from_embedded(data: &[u8]) -> Self {
        let mut cmap = Self {
            loaded: false,
            vertical: false,
            charset: CidSet::Unknown,
            coding_scheme: CodingScheme::TwoBytes,
            coding: CidCoding::Unknown,
            mixed_two_byte_leading_bytes: [false; 256],
            direct_charcode_to_cid: vec![0; DIRECT_MAP_TABLE_SIZE],
            additional_mappings: Vec::new(),
            mixed_four_byte_leading_ranges: Vec::new(),
            embed_map: None,
        };
        let mut parser = CmapParser::new(&mut cmap);
        let mut syntax = CpdfSimpleParser::new(data);
        loop {
            let word = syntax.get_word();
            if word.is_empty() {
                break;
            }
            parser.parse_word(word);
        }
        parser.finish();
        cmap
    }

    pub fn from_predefined(name: &[u8]) -> Self {
        let mut cmap = Self {
            loaded: false,
            vertical: name.last() == Some(&b'V'),
            charset: CidSet::Unknown,
            coding_scheme: CodingScheme::TwoBytes,
            coding: CidCoding::Unknown,
            mixed_two_byte_leading_bytes: [false; 256],
            direct_charcode_to_cid: Vec::new(),
            additional_mappings: Vec::new(),
            mixed_four_byte_leading_ranges: Vec::new(),
            embed_map: None,
        };
        if matches!(name, b"Identity-H" | b"Identity-V") {
            cmap.coding = CidCoding::Cid;
            cmap.loaded = true;
            return cmap;
        }
        let base = if name.len() > 2 {
            &name[..name.len() - 2]
        } else {
            name
        };
        let Some((charset, coding, scheme, leading)) = predefined_metadata(base) else {
            return cmap;
        };
        cmap.charset = charset;
        cmap.coding = coding;
        cmap.coding_scheme = scheme;
        if scheme == CodingScheme::MixedTwoBytes {
            for (first, last) in leading {
                if first == 0 && last == 0 {
                    break;
                }
                for byte in first..=last {
                    cmap.mixed_two_byte_leading_bytes[usize::from(byte)] = true;
                }
            }
        }
        let maps = maps_for_charset(charset);
        if let Some(index) = fpdf_cmaps::find_cmap(maps, name) {
            cmap.embed_map = Some((maps, index));
            cmap.loaded = true;
        }
        cmap
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn coding(&self) -> CidCoding {
        self.coding
    }

    pub fn embed_map(&self) -> Option<&'static fpdf_cmaps::CMap> {
        self.embed_map.and_then(|(maps, index)| maps.get(index))
    }

    pub fn embedded_char_code_from_cid(&self, cid: u16) -> u32 {
        self.embed_map.map_or(0, |(maps, index)| {
            fpdf_cmaps::char_code_from_cid(maps, index, cid)
        })
    }

    pub fn is_direct_charcode_to_cid_table_empty(&self) -> bool {
        self.direct_charcode_to_cid.is_empty()
    }

    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    pub fn charset(&self) -> CidSet {
        self.charset
    }

    pub fn coding_scheme(&self) -> CodingScheme {
        self.coding_scheme
    }

    pub fn mixed_four_byte_leading_ranges(&self) -> &[CodeRange] {
        &self.mixed_four_byte_leading_ranges
    }

    pub fn cid_from_char_code(&self, charcode: u32) -> u16 {
        if self.coding == CidCoding::Cid {
            return charcode as u16;
        }
        if let Some((maps, index)) = self.embed_map {
            return fpdf_cmaps::cid_from_char_code(maps, index, charcode);
        }
        if self.direct_charcode_to_cid.is_empty() {
            return charcode as u16;
        }
        if let Some(&cid) = self.direct_charcode_to_cid.get(charcode as usize) {
            return cid;
        }
        let index = self
            .additional_mappings
            .partition_point(|r| r.end_code < charcode);
        let Some(range) = self.additional_mappings.get(index) else {
            return 0;
        };
        if range.start_code > charcode {
            return 0;
        }
        range
            .start_cid
            .wrapping_add((charcode - range.start_code) as u16)
    }

    pub fn get_char_size(&self, charcode: u32) -> usize {
        match self.coding_scheme {
            CodingScheme::OneByte => 1,
            CodingScheme::TwoBytes => 2,
            CodingScheme::MixedTwoBytes => {
                if charcode < 0x100 {
                    1
                } else {
                    2
                }
            }
            CodingScheme::MixedFourBytes => {
                if charcode < 0x100 {
                    1
                } else if charcode < 0x1_0000 {
                    2
                } else if charcode < 0x100_0000 {
                    3
                } else {
                    4
                }
            }
        }
    }

    pub fn get_next_char(&self, bytes: &[u8], offset: &mut usize) -> u32 {
        let read = |offset: &mut usize| {
            if let Some(&byte) = bytes.get(*offset) {
                *offset += 1;
                byte
            } else {
                0
            }
        };
        match self.coding_scheme {
            CodingScheme::OneByte => u32::from(read(offset)),
            CodingScheme::TwoBytes => {
                let first = read(offset);
                u32::from(first) * 256 + u32::from(read(offset))
            }
            CodingScheme::MixedTwoBytes => {
                let first = read(offset);
                if !self.mixed_two_byte_leading_bytes[usize::from(first)] {
                    u32::from(first)
                } else {
                    u32::from(first) * 256 + u32::from(read(offset))
                }
            }
            CodingScheme::MixedFourBytes => {
                let mut codes = [0; 4];
                let mut char_size = 1;
                codes[0] = read(offset);
                loop {
                    match check_four_byte_code_range(
                        &codes[..char_size],
                        &self.mixed_four_byte_leading_ranges,
                    ) {
                        0 => return 0,
                        2 => {
                            return codes[..char_size]
                                .iter()
                                .fold(0u32, |code, &byte| (code << 8) + u32::from(byte));
                        }
                        _ => {
                            if char_size == 4 || *offset == bytes.len() {
                                return 0;
                            }
                            codes[char_size] = read(offset);
                            char_size += 1;
                        }
                    }
                }
            }
        }
    }

    pub fn count_char(&self, bytes: &[u8]) -> usize {
        match self.coding_scheme {
            CodingScheme::OneByte => bytes.len(),
            CodingScheme::TwoBytes => bytes.len().div_ceil(2),
            CodingScheme::MixedTwoBytes => {
                let mut offset = 0;
                let mut count = 0;
                while offset < bytes.len() {
                    let first = bytes[offset];
                    offset += if self.mixed_two_byte_leading_bytes[usize::from(first)] {
                        2
                    } else {
                        1
                    };
                    count += 1;
                }
                count
            }
            CodingScheme::MixedFourBytes => {
                let mut offset = 0;
                let mut count = 0;
                while offset < bytes.len() {
                    self.get_next_char(bytes, &mut offset);
                    count += 1;
                }
                count
            }
        }
    }

    pub fn append_char(&self, bytes: &mut Vec<u8>, charcode: u32) {
        match self.coding_scheme {
            CodingScheme::OneByte => bytes.push(charcode as u8),
            CodingScheme::TwoBytes => {
                bytes.extend_from_slice(&[(charcode >> 8) as u8, charcode as u8])
            }
            CodingScheme::MixedTwoBytes => {
                if charcode < 0x100 && !self.mixed_two_byte_leading_bytes[charcode as usize] {
                    bytes.push(charcode as u8);
                } else {
                    bytes.extend_from_slice(&[(charcode >> 8) as u8, charcode as u8]);
                }
            }
            CodingScheme::MixedFourBytes => {
                if charcode < 0x100 {
                    let size =
                        get_four_byte_char_size(charcode, &self.mixed_four_byte_leading_ranges);
                    bytes.resize(bytes.len() + size.saturating_sub(1), 0);
                    bytes.push(charcode as u8);
                } else if charcode < 0x1_0000 {
                    bytes.extend_from_slice(&[(charcode >> 8) as u8, charcode as u8]);
                } else if charcode < 0x100_0000 {
                    bytes.extend_from_slice(&[
                        (charcode >> 16) as u8,
                        (charcode >> 8) as u8,
                        charcode as u8,
                    ]);
                } else {
                    bytes.extend_from_slice(&charcode.to_be_bytes());
                }
            }
        }
    }

    pub(super) fn set_vertical(&mut self, vertical: bool) {
        self.vertical = vertical;
    }

    pub(super) fn set_charset(&mut self, charset: CidSet) {
        self.charset = charset;
    }

    pub(super) fn set_coding_scheme(&mut self, scheme: CodingScheme) {
        self.coding_scheme = scheme;
    }

    pub(super) fn set_direct_range(&mut self, start: u32, end: u32, cid: u16) {
        // PDFium assumes well-formed input here. Limit malformed ranges to the
        // allocated table while retaining the same mapping for valid input.
        let Some(last) = self
            .direct_charcode_to_cid
            .get_mut(start as usize..=end as usize)
        else {
            return;
        };
        for (offset, slot) in last.iter_mut().enumerate() {
            *slot = cid.wrapping_add(offset as u16);
        }
    }

    pub(super) fn set_additional_mappings(&mut self, mut mappings: Vec<CidRange>) {
        if self.coding_scheme != CodingScheme::MixedFourBytes || mappings.is_empty() {
            return;
        }
        mappings.sort_by_key(|range| range.end_code);
        self.additional_mappings = mappings;
    }

    pub(super) fn set_mixed_four_byte_leading_ranges(&mut self, ranges: Vec<CodeRange>) {
        self.mixed_four_byte_leading_ranges = ranges;
    }
}

// Return 0 for no range, 1 for a prefix, and 2 for a complete character.
fn check_four_byte_code_range(codes: &[u8], ranges: &[CodeRange]) -> u8 {
    for range in ranges.iter().rev() {
        if range.char_size < codes.len() {
            continue;
        }
        let mut matched = 0;
        while matched < codes.len()
            && codes[matched] >= range.lower[matched]
            && codes[matched] <= range.upper[matched]
        {
            matched += 1;
        }
        // Every supplied byte must match this code-space prefix. Accepting
        // only a shared first byte lets a longer range consume bytes from a
        // valid shorter code (for example 00 82 xx versus 00 00 90 xx).
        // The native helper accepts that partial mismatch; retain the valid
        // code-space distinction instead of reproducing its decoding error.
        if matched == codes.len() {
            return if matched == range.char_size { 2 } else { 1 };
        }
    }
    0
}

fn get_four_byte_char_size(charcode: u32, ranges: &[CodeRange]) -> usize {
    if ranges.is_empty() {
        return 1;
    }
    // Preserve PDFium's two high zero bytes, including for larger charcodes.
    let codes = [0, 0, (charcode >> 8) as u8, charcode as u8];
    for offset in 0..4 {
        let size = 4 - offset;
        for range in ranges.iter().rev() {
            if range.char_size < size {
                continue;
            }
            let mut matched = 0;
            while matched < size
                && codes[offset + matched] >= range.lower[matched]
                && codes[offset + matched] <= range.upper[matched]
            {
                matched += 1;
            }
            if matched == range.char_size {
                return size;
            }
        }
    }
    1
}

type PredefinedMetadata = (CidSet, CidCoding, CodingScheme, [(u8, u8); 2]);

fn predefined_metadata(base: &[u8]) -> Option<PredefinedMetadata> {
    use CidCoding::{Big5, Jis, Korea, Ucs2, Utf16, GB};
    use CodingScheme::{MixedTwoBytes, TwoBytes};
    let empty = [(0, 0); 2];
    let metadata = match base {
        b"GB-EUC" => (CidSet::GB1, GB, MixedTwoBytes, [(0xa1, 0xfe), (0, 0)]),
        b"GBpc-EUC" => (CidSet::GB1, GB, MixedTwoBytes, [(0xa1, 0xfc), (0, 0)]),
        b"GBK-EUC" | b"GBKp-EUC" | b"GBK2K-EUC" | b"GBK2K" => {
            (CidSet::GB1, GB, MixedTwoBytes, [(0x81, 0xfe), (0, 0)])
        }
        b"UniGB-UCS2" => (CidSet::GB1, Ucs2, TwoBytes, empty),
        b"UniGB-UTF16" => (CidSet::GB1, Utf16, TwoBytes, empty),
        b"B5pc" => (CidSet::CNS1, Big5, MixedTwoBytes, [(0xa1, 0xfc), (0, 0)]),
        b"HKscs-B5" => (CidSet::CNS1, Big5, MixedTwoBytes, [(0x88, 0xfe), (0, 0)]),
        b"ETen-B5" | b"ETenms-B5" => (CidSet::CNS1, Big5, MixedTwoBytes, [(0xa1, 0xfe), (0, 0)]),
        b"UniCNS-UCS2" => (CidSet::CNS1, Ucs2, TwoBytes, empty),
        b"UniCNS-UTF16" => (CidSet::CNS1, Utf16, TwoBytes, empty),
        b"83pv-RKSJ" | b"90ms-RKSJ" | b"90msp-RKSJ" | b"90pv-RKSJ" | b"Add-RKSJ" | b"Ext-RKSJ" => (
            CidSet::Japan1,
            Jis,
            MixedTwoBytes,
            [(0x81, 0x9f), (0xe0, 0xfc)],
        ),
        b"EUC" => (
            CidSet::Japan1,
            Jis,
            MixedTwoBytes,
            [(0x8e, 0x8e), (0xa1, 0xfe)],
        ),
        b"H" | b"V" => (CidSet::Japan1, Jis, TwoBytes, [(0x21, 0x7e), (0, 0)]),
        b"UniJIS-UCS2" | b"UniJIS-UCS2-HW" => (CidSet::Japan1, Ucs2, TwoBytes, empty),
        b"UniJIS-UTF16" => (CidSet::Japan1, Utf16, TwoBytes, empty),
        b"KSC-EUC" => (CidSet::Korea1, Korea, MixedTwoBytes, [(0xa1, 0xfe), (0, 0)]),
        b"KSCms-UHC" | b"KSCms-UHC-HW" => {
            (CidSet::Korea1, Korea, MixedTwoBytes, [(0x81, 0xfe), (0, 0)])
        }
        b"KSCpc-EUC" => (CidSet::Korea1, Korea, MixedTwoBytes, [(0xa1, 0xfd), (0, 0)]),
        b"UniKS-UCS2" => (CidSet::Korea1, Ucs2, TwoBytes, empty),
        b"UniKS-UTF16" => (CidSet::Korea1, Utf16, TwoBytes, empty),
        _ => return None,
    };
    Some(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_cmap_direct_and_four_byte_cids() {
        let cmap = CpdfCmap::from_embedded(
            b"/Ordering (Japan1) def /WMode 1 def\n\
              1 begincodespacerange <00> <7f> endcodespacerange\n\
              1 begincodespacerange <81000000> <81ffffff> endcodespacerange\n\
              2 begincidchar <0041> 9 <81000001> 42 endcidchar\n\
              1 begincidrange <0042> <0044> 100 endcidrange\n\
              1 begincidrange <81000010> <81000012> 200 endcidrange",
        );
        assert!(cmap.is_vertical());
        // PDFium's CMap_GetString() drops the first two bytes of `(Japan1)`,
        // so this conventional token does not match the ordering table.
        assert_eq!(cmap.charset(), CidSet::Unknown);
        assert_eq!(cmap.coding_scheme(), CodingScheme::MixedFourBytes);
        assert_eq!(cmap.mixed_four_byte_leading_ranges().len(), 2);
        assert_eq!(cmap.cid_from_char_code(0x41), 9);
        assert_eq!(cmap.cid_from_char_code(0x43), 101);
        assert_eq!(cmap.cid_from_char_code(0x8100_0001), 42);
        assert_eq!(cmap.cid_from_char_code(0x8100_0011), 201);
        assert_eq!(cmap.cid_from_char_code(0x8100_0020), 0);
    }

    #[test]
    fn cmap_char_iteration_and_append_follow_scheme() {
        let mut cmap = CpdfCmap::from_embedded(b"");
        assert_eq!(cmap.count_char(b"\x12\x34\x56"), 2);
        let mut offset = 0;
        assert_eq!(cmap.get_next_char(b"\x12\x34\x56", &mut offset), 0x1234);
        assert_eq!(cmap.get_next_char(b"\x12\x34\x56", &mut offset), 0x5600);
        assert_eq!(offset, 3);
        let mut encoded = Vec::new();
        cmap.append_char(&mut encoded, 0xabcd);
        assert_eq!(encoded, b"\xab\xcd");

        cmap.coding_scheme = CodingScheme::MixedTwoBytes;
        cmap.mixed_two_byte_leading_bytes[0x81] = true;
        let bytes = b"A\x81\x40\x81";
        assert_eq!(cmap.count_char(bytes), 3);
        let mut offset = 0;
        assert_eq!(cmap.get_next_char(bytes, &mut offset), 0x41);
        assert_eq!(cmap.get_next_char(bytes, &mut offset), 0x8140);
        assert_eq!(cmap.get_next_char(bytes, &mut offset), 0x8100);
        assert_eq!(cmap.get_char_size(0x8140), 2);
        encoded.clear();
        cmap.append_char(&mut encoded, 0x81);
        assert_eq!(encoded, b"\x00\x81");

        cmap.coding_scheme = CodingScheme::MixedFourBytes;
        cmap.mixed_four_byte_leading_ranges = vec![
            CodeRange {
                char_size: 1,
                lower: [0; 4],
                upper: [0x7f, 0, 0, 0],
            },
            CodeRange {
                char_size: 4,
                lower: [0x81, 0, 0, 0],
                upper: [0x81, 0xff, 0xff, 0xff],
            },
        ];
        let bytes = b"A\x81\x01\x02\x03B\x82";
        assert_eq!(cmap.count_char(bytes), 4);
        let mut offset = 0;
        assert_eq!(cmap.get_next_char(bytes, &mut offset), 0x41);
        assert_eq!(cmap.get_next_char(bytes, &mut offset), 0x8101_0203);
        assert_eq!(cmap.get_next_char(bytes, &mut offset), 0x42);
        assert_eq!(cmap.get_next_char(bytes, &mut offset), 0);
        encoded.clear();
        cmap.append_char(&mut encoded, 0x8101_0203);
        assert_eq!(encoded, b"\x81\x01\x02\x03");
    }

    #[test]
    fn predefined_cmaps_use_bundled_tables_and_lead_bytes() {
        let identity = CpdfCmap::from_predefined(b"Identity-V");
        assert!(identity.is_loaded());
        assert!(identity.is_vertical());
        assert_eq!(identity.coding(), CidCoding::Cid);
        assert_eq!(identity.cid_from_char_code(0x1_1234), 0x1234);

        let gb = CpdfCmap::from_predefined(b"GB-EUC-H");
        assert!(gb.is_loaded());
        assert_eq!(gb.coding(), CidCoding::GB);
        assert_eq!(gb.charset(), CidSet::GB1);
        assert_eq!(gb.cid_from_char_code(0x20), 0x1e24);
        assert_eq!(gb.get_char_size(0xa1a1), 2);
        let mut offset = 0;
        assert_eq!(gb.get_next_char(b"A\xa1\xa1", &mut offset), 0x41);
        assert_eq!(gb.get_next_char(b"A\xa1\xa1", &mut offset), 0xa1a1);

        let japanese = CpdfCmap::from_predefined(b"H");
        assert!(japanese.is_loaded());
        assert_eq!(japanese.cid_from_char_code(0x2121), 0x279);
        let unknown = CpdfCmap::from_predefined(b"Not-A-CMap");
        assert!(!unknown.is_loaded());
        assert_eq!(unknown.cid_from_char_code(0x41), 0x41);
    }

    #[test]
    fn mixed_code_spaces_distinguish_shared_leading_zero_prefixes() {
        let cmap = CpdfCmap::from_embedded(
            b"4 begincodespacerange <40> <7f> <8100> <81ff> \
              <008200> <0082ff> <00009000> <000090ff> endcodespacerange",
        );
        let bytes = b"\x41\x81\x40\x00\x82\x43\x00\x82\x44\x00\x00\x90\x45\x00\x00\x90\x46";
        assert_eq!(cmap.count_char(bytes), 6);
        let mut offset = 0;
        for expected in [0x41, 0x8140, 0x8243, 0x8244, 0x9045, 0x9046] {
            assert_eq!(cmap.get_next_char(bytes, &mut offset), expected);
        }
        assert_eq!(offset, bytes.len());
        assert_eq!(
            check_four_byte_code_range(b"\x00\x82", cmap.mixed_four_byte_leading_ranges()),
            1
        );
        assert_eq!(
            check_four_byte_code_range(b"\x00\x83", cmap.mixed_four_byte_leading_ranges()),
            0
        );
        assert_eq!(
            check_four_byte_code_range(b"\x00\x82\x43", cmap.mixed_four_byte_leading_ranges()),
            2
        );
        assert_eq!(
            check_four_byte_code_range(b"\x00\x00\x90\x45", cmap.mixed_four_byte_leading_ranges()),
            2
        );
    }
}
