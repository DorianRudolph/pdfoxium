// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fde/cfde_wordbreak_data.{h,cpp}`: the complete pinned word-boundary
//! classification and state-change tables. Even code points use the high
//! nibble and odd code points use the low nibble, as in the source lookup.

/// Numeric values index the word-boundary masks and packed property table.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum WordBreakProperty {
    #[default]
    None = 0,
    CR,
    LF,
    NewLine,
    Extend,
    Format,
    KataKana,
    ALetter,
    MidLetter,
    MidNum,
    MidNumLet,
    Numeric,
    ExtendNumLet,
}

impl WordBreakProperty {
    fn from_nibble(value: u8) -> Self {
        const PROPERTIES: [WordBreakProperty; 13] = [
            WordBreakProperty::None,
            WordBreakProperty::CR,
            WordBreakProperty::LF,
            WordBreakProperty::NewLine,
            WordBreakProperty::Extend,
            WordBreakProperty::Format,
            WordBreakProperty::KataKana,
            WordBreakProperty::ALetter,
            WordBreakProperty::MidLetter,
            WordBreakProperty::MidNum,
            WordBreakProperty::MidNumLet,
            WordBreakProperty::Numeric,
            WordBreakProperty::ExtendNumLet,
        ];
        PROPERTIES
            .get(usize::from(value))
            .copied()
            .unwrap_or_default()
    }
}

const fn mask(property: WordBreakProperty) -> u16 {
    1 << property as u8
}

use WordBreakProperty as W;
const WORD_BREAK_TABLE: [u16; 13] = [
    0xffff,
    !(mask(W::LF) | mask(W::CR)),
    !mask(W::LF),
    !mask(W::LF),
    !mask(W::LF),
    !mask(W::LF),
    !(mask(W::LF) | mask(W::KataKana) | mask(W::ExtendNumLet)),
    !(mask(W::LF) | mask(W::ALetter) | mask(W::Numeric) | mask(W::ExtendNumLet)),
    !mask(W::LF),
    !mask(W::LF),
    !mask(W::LF),
    !(mask(W::LF) | mask(W::ALetter) | mask(W::Numeric) | mask(W::ExtendNumLet)),
    !(mask(W::LF)
        | mask(W::KataKana)
        | mask(W::ALetter)
        | mask(W::Numeric)
        | mask(W::ExtendNumLet)),
];

// Two 4-bit property values per byte, covering every possible uint16_t input.
const CODE_POINT_PROPERTIES: &[u8; 32768] = include_bytes!("cfde_wordbreak_data.bin");

pub fn check_state_change_for_word_break(from: WordBreakProperty, to: WordBreakProperty) -> bool {
    WORD_BREAK_TABLE[from as usize] & mask(to) != 0
}

pub fn get_word_break_property(code_point: u32) -> WordBreakProperty {
    let Some(&packed) = CODE_POINT_PROPERTIES.get((code_point / 2) as usize) else {
        return WordBreakProperty::None;
    };
    WordBreakProperty::from_nibble(if code_point & 1 == 0 {
        packed >> 4
    } else {
        packed & 0xf
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/xfa/fde/cfde_wordbreak_data.cpp"
    ));

    #[test]
    fn complete_pinned_word_properties_and_boundaries() {
        let body = SOURCE
            .split_once("kCodePointProperties = {{")
            .unwrap()
            .1
            .split_once("}};")
            .unwrap()
            .0;
        let source_bytes: Vec<u8> = body
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| u8::from_str_radix(value.strip_prefix("0x").unwrap(), 16).unwrap())
            .collect();
        assert_eq!(source_bytes.len(), 32768);
        assert_eq!(CODE_POINT_PROPERTIES.as_slice(), source_bytes);
        for code_point in 0..=u32::from(u16::MAX) {
            let packed = source_bytes[(code_point / 2) as usize];
            let expected = if code_point & 1 == 0 {
                packed >> 4
            } else {
                packed & 0xf
            };
            assert!(expected < 13);
            assert_eq!(get_word_break_property(code_point) as u8, expected);
        }
        for code_point in [0x10000, 0x10ffff, u32::MAX] {
            assert_eq!(get_word_break_property(code_point), W::None);
        }
        for (ch, property) in [
            ('\r', W::CR),
            ('\n', W::LF),
            ('\u{b}', W::NewLine),
            ('\u{301}', W::Extend),
            ('\u{ad}', W::Format),
            ('\u{30a2}', W::KataKana),
            ('a', W::ALetter),
            (':', W::MidLetter),
            (',', W::MidNum),
            ('\'', W::MidNumLet),
            ('9', W::Numeric),
            ('_', W::ExtendNumLet),
            (' ', W::None),
        ] {
            assert_eq!(get_word_break_property(u32::from(ch)), property);
        }
    }

    #[test]
    fn complete_pinned_word_break_transitions() {
        const NAMES: [&str; 13] = [
            "None",
            "CR",
            "LF",
            "NewLine",
            "Extend",
            "Format",
            "KataKana",
            "ALetter",
            "MidLetter",
            "MidNum",
            "MidNumLet",
            "Numeric",
            "ExtendNumLet",
        ];
        let body = SOURCE
            .split_once("kWordBreakTableData[] = {")
            .unwrap()
            .1
            .split_once("};")
            .unwrap()
            .0;
        let without_comments = body
            .lines()
            .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
            .collect::<String>();
        let entries: Vec<_> = without_comments
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect();
        assert_eq!(entries.len(), 13);
        for (from, entry) in entries.iter().enumerate() {
            let mut blocked = 0u16;
            for word in entry.split(|ch: char| !ch.is_ascii_alphanumeric()) {
                let Some(name) = word.strip_prefix("kWordBreakMask") else {
                    continue;
                };
                blocked |= 1
                    << NAMES
                        .iter()
                        .position(|&candidate| candidate == name)
                        .unwrap();
            }
            let expected = if entry == &"0xFFFF" { 0xffff } else { !blocked };
            assert_eq!(WORD_BREAK_TABLE[from], expected);
            for to in 0..13 {
                assert_eq!(
                    check_state_change_for_word_break(
                        W::from_nibble(from as u8),
                        W::from_nibble(to as u8)
                    ),
                    expected & (1 << to) != 0,
                    "transition {} -> {}",
                    NAMES[from],
                    NAMES[to]
                );
            }
        }
    }
}
