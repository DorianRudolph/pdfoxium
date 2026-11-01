// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Lookup rules from `core/fpdfapi/cmaps/fpdf_cmaps.{h,cpp}`.
//! The four bundled character-set tables are generated separately.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SingleCmap {
    pub code: u16,
    pub cid: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RangeCmap {
    pub low: u16,
    pub high: u16,
    pub cid: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwordCidMap {
    pub hi_word: u16,
    pub lo_word_low: u16,
    pub lo_word_high: u16,
    pub cid: u16,
}

#[derive(Clone, Copy)]
pub enum WordMap {
    Single(&'static [SingleCmap]),
    Range(&'static [RangeCmap]),
}

#[derive(Clone, Copy)]
pub struct CMap {
    pub name: &'static str,
    pub word_map: WordMap,
    pub dword_map: &'static [DwordCidMap],
    pub use_offset: i8,
}

pub fn find_cmap(maps: &'static [CMap], name: &[u8]) -> Option<usize> {
    maps.iter().position(|map| map.name.as_bytes() == name)
}

fn next_map(maps: &[CMap], index: usize) -> Option<usize> {
    let offset = maps.get(index)?.use_offset;
    if offset == 0 {
        None
    } else {
        index
            .checked_add_signed(isize::from(offset))
            .filter(|&i| i < maps.len())
    }
}

pub fn cid_from_char_code(maps: &[CMap], mut index: usize, charcode: u32) -> u16 {
    if charcode >> 16 != 0 {
        let lo_word = charcode as u16;
        loop {
            let Some(map) = maps.get(index) else {
                return 0;
            };
            let found = map.dword_map.partition_point(|entry| {
                if entry.hi_word != (charcode >> 16) as u16 {
                    entry.hi_word < (charcode >> 16) as u16
                } else {
                    entry.lo_word_high < lo_word
                }
            });
            if let Some(entry) = map.dword_map.get(found) {
                if lo_word >= entry.lo_word_low && lo_word <= entry.lo_word_high {
                    return entry.cid.wrapping_add(lo_word - entry.lo_word_low);
                }
            }
            let Some(next) = next_map(maps, index) else {
                return 0;
            };
            index = next;
        }
    }

    let code = charcode as u16;
    loop {
        let Some(map) = maps.get(index) else {
            return 0;
        };
        match map.word_map {
            WordMap::Single(entries) => {
                let found = entries.partition_point(|entry| entry.code < code);
                if let Some(entry) = entries.get(found) {
                    if entry.code == code {
                        return entry.cid;
                    }
                }
            }
            WordMap::Range(entries) => {
                let found = entries.partition_point(|entry| entry.high < code);
                if let Some(entry) = entries.get(found) {
                    if code >= entry.low && code <= entry.high {
                        return entry.cid.wrapping_add(code - entry.low);
                    }
                }
            }
        }
        let Some(next) = next_map(maps, index) else {
            return 0;
        };
        index = next;
    }
}

pub fn char_code_from_cid(maps: &[CMap], mut index: usize, cid: u16) -> u32 {
    // PDFium searches only the word maps in the reverse direction.
    loop {
        let Some(map) = maps.get(index) else {
            return 0;
        };
        match map.word_map {
            WordMap::Single(entries) => {
                if let Some(entry) = entries.iter().find(|entry| entry.cid == cid) {
                    return u32::from(entry.code);
                }
            }
            WordMap::Range(entries) => {
                if let Some(entry) = entries.iter().find(|entry| {
                    u32::from(cid) >= u32::from(entry.cid)
                        && u32::from(cid)
                            <= u32::from(entry.cid) + u32::from(entry.high - entry.low)
                }) {
                    return u32::from(entry.low) + u32::from(cid - entry.cid);
                }
            }
        }
        let Some(next) = next_map(maps, index) else {
            return 0;
        };
        index = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &[RangeCmap] = &[
        RangeCmap {
            low: 0x20,
            high: 0x7e,
            cid: 100,
        },
        RangeCmap {
            low: 0xa1a1,
            high: 0xa1fe,
            cid: 200,
        },
    ];
    const OVERRIDE: &[SingleCmap] = &[
        SingleCmap {
            code: 0x40,
            cid: 900,
        },
        SingleCmap {
            code: 0xa1a2,
            cid: 901,
        },
    ];
    const DWORD: &[DwordCidMap] = &[DwordCidMap {
        hi_word: 0x8100,
        lo_word_low: 0x0010,
        lo_word_high: 0x0012,
        cid: 1000,
    }];
    const MAPS: &[CMap] = &[
        CMap {
            name: "base",
            word_map: WordMap::Range(BASE),
            dword_map: &[],
            use_offset: 0,
        },
        CMap {
            name: "derived",
            word_map: WordMap::Single(OVERRIDE),
            dword_map: DWORD,
            use_offset: -1,
        },
    ];

    #[test]
    fn single_range_dword_and_base_map_lookup() {
        assert_eq!(find_cmap(MAPS, b"derived"), Some(1));
        assert_eq!(cid_from_char_code(MAPS, 1, 0x40), 900);
        assert_eq!(cid_from_char_code(MAPS, 1, 0x41), 133);
        assert_eq!(cid_from_char_code(MAPS, 1, 0xa1a2), 901);
        assert_eq!(cid_from_char_code(MAPS, 1, 0x8100_0011), 1001);
        assert_eq!(cid_from_char_code(MAPS, 1, 0x8100_0013), 0);
        assert_eq!(char_code_from_cid(MAPS, 1, 900), 0x40);
        assert_eq!(char_code_from_cid(MAPS, 1, 133), 0x41);
        assert_eq!(char_code_from_cid(MAPS, 1, 1001), 0);
    }
}
