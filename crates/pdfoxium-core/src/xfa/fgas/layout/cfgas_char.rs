// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/layout/cfgas_char.{h,cpp}`. The source's weak/neutral state
//! machine and reorder passes operate on one contiguous character slice.
use super::cfgas_textuserdata::CfgasTextUserData;
use super::fgas_linebreak::LineBreakType;
use crate::fxcrt::fx_unicode::{self as unicode, BidiClass, CharType};
use std::rc::Rc;
#[path = "cfgas_char_data.rs"]
mod data;
use data::{ADD_LEVELS, NEUTRAL_ACTIONS, NEUTRAL_STATES, WEAK_ACTIONS, WEAK_STATES};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum BreakType {
    #[default]
    None = 0,
    Piece,
    Line,
    Paragraph,
    Page,
}

#[derive(Clone)]
pub struct CfgasChar {
    pub status: BreakType,
    pub bidi_class: BidiClass,
    pub line_break_type: LineBreakType,
    pub char_styles: u32,
    pub char_width: i32,
    pub bidi_level: u16,
    pub bidi_pos: u16,
    pub bidi_order: u16,
    pub font_size: i32,
    pub identity: u32,
    pub user_data: Option<Rc<CfgasTextUserData>>,
    pub char_code: u16,
    pub horizontal_scale: i32,
    pub vertical_scale: i32,
}
impl std::fmt::Debug for CfgasChar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CfgasChar")
            .field("char_code", &self.char_code)
            .field("status", &self.status)
            .field("bidi_class", &self.bidi_class)
            .field("bidi_level", &self.bidi_level)
            .field("bidi_pos", &self.bidi_pos)
            .field("bidi_order", &self.bidi_order)
            .field("char_width", &self.char_width)
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}
impl CfgasChar {
    pub fn new(char_code: u16) -> Self {
        Self::with_scales(char_code, 100, 100)
    }
    pub fn with_scales(char_code: u16, horizontal_scale: i32, vertical_scale: i32) -> Self {
        Self {
            status: BreakType::None,
            bidi_class: BidiClass::ON,
            line_break_type: LineBreakType::Unknown,
            char_styles: 0,
            char_width: 0,
            bidi_level: 0,
            bidi_pos: 0,
            bidi_order: 0,
            font_size: 0,
            identity: 0,
            user_data: None,
            char_code,
            horizontal_scale,
            vertical_scale,
        }
    }
    pub fn with_scale(char_code: u16, horizontal_scale: i32, vertical_scale: i32) -> Self {
        Self::with_scales(char_code, horizontal_scale, vertical_scale)
    }
    pub fn char_code(&self) -> u16 {
        self.char_code
    }
    pub fn horizontal_scale(&self) -> i32 {
        i32::from(self.horizontal_scale as i16)
    }
    pub fn horizonal_scale(&self) -> i32 {
        self.horizontal_scale()
    }
    pub fn vertical_scale(&self) -> i32 {
        i32::from(self.vertical_scale as i16)
    }
    pub fn get_char_type(&self) -> CharType {
        unicode::get_char_type(u32::from(self.char_code))
    }
    pub fn bidi_line(chars: &mut [Self], count: usize) {
        assert!(count <= chars.len(), "bidi prefix exceeds character buffer");
        if count < 2 {
            return;
        }
        let chars = &mut chars[..count];
        classify_with_transform(chars);
        resolve_explicit(chars);
        resolve_weak(chars);
        resolve_neutrals(chars);
        resolve_implicit(chars);
        classify(chars);
        resolve_whitespace(chars);
        reorder(chars);
        position(chars);
    }
}

const N_TYPES: [BidiClass; 20] = [
    BidiClass::ON,
    BidiClass::L,
    BidiClass::R,
    BidiClass::AN,
    BidiClass::EN,
    BidiClass::AL,
    BidiClass::NSM,
    BidiClass::CS,
    BidiClass::ES,
    BidiClass::ET,
    BidiClass::BN,
    BidiClass::BN,
    BidiClass::ON,
    BidiClass::B,
    BidiClass::RLO,
    BidiClass::RLE,
    BidiClass::LRO,
    BidiClass::LRE,
    BidiClass::PDF,
    BidiClass::ON,
];
fn direction(level: u16) -> BidiClass {
    if level & 1 != 0 {
        BidiClass::R
    } else {
        BidiClass::L
    }
}
// Only the low bidi classes occur in action nibbles. 0xF means no action;
// unlike the C++ casts it is represented separately from BidiClass::RLE.
fn action_class(nibble: u16) -> Option<BidiClass> {
    match nibble {
        0 => Some(BidiClass::ON),
        1 => Some(BidiClass::L),
        2 => Some(BidiClass::R),
        3 => Some(BidiClass::AN),
        4 => Some(BidiClass::EN),
        0xf => None,
        _ => unreachable!("invalid pinned bidi action"),
    }
}
fn deferred_type(action: u16) -> Option<BidiClass> {
    action_class((action >> 4) & 0xf)
}
fn resolved_type(action: u16) -> Option<BidiClass> {
    action_class(action & 0xf)
}
fn deferred_neutrals(action: u16, level: u16) -> BidiClass {
    match deferred_type(action).expect("neutral action has a class") {
        BidiClass::AN => direction(level),
        class => class,
    }
}
fn set_deferred_run_class(chars: &mut [CfgasChar], start: usize, count: usize, class: BidiClass) {
    for ch in &mut chars[start - count..start] {
        ch.bidi_class = class;
    }
}
fn set_deferred_run_level(chars: &mut [CfgasChar], start: usize, count: usize, level: u16) {
    for ch in &mut chars[start - count..start] {
        ch.bidi_level = level;
    }
}
fn classify(chars: &mut [CfgasChar]) {
    for ch in chars {
        ch.bidi_class = unicode::get_bidi_class(u32::from(ch.char_code));
    }
}
fn classify_with_transform(chars: &mut [CfgasChar]) {
    for ch in chars {
        ch.bidi_class = N_TYPES[unicode::get_bidi_class(u32::from(ch.char_code)) as usize];
    }
}
fn resolve_explicit(chars: &mut [CfgasChar]) {
    for ch in chars {
        ch.bidi_level = 0;
    }
}
fn resolve_weak(chars: &mut [CfgasChar]) {
    if chars.len() <= 1 {
        return;
    }
    let mut current_level = 0;
    let mut run_count = 0;
    let mut state = 2; // FX_BWSxl
    for index in 0..chars.len() {
        let mut class = chars[index].bidi_class;
        if class == BidiClass::BN {
            chars[index].bidi_level = current_level;
            if index + 1 == chars.len() && current_level != 0 {
                class = direction(current_level);
                chars[index].bidi_class = class;
            } else if index + 1 < chars.len() {
                let next_class = chars[index + 1].bidi_class;
                let next_level = chars[index + 1].bidi_level;
                if next_class != BidiClass::BN && current_level != next_level {
                    let level = next_level.max(current_level);
                    chars[index].bidi_level = level;
                    class = direction(level);
                    chars[index].bidi_class = class;
                    current_level = next_level;
                } else {
                    if run_count > 0 {
                        run_count += 1;
                    }
                    continue;
                }
            } else {
                if run_count > 0 {
                    run_count += 1;
                }
                continue;
            }
        }
        if class > BidiClass::BN {
            continue;
        }
        let action = WEAK_ACTIONS[state][class as usize];
        if let Some(run_class) = deferred_type(action) {
            if run_count > 0 {
                set_deferred_run_class(chars, index, run_count, run_class);
                run_count = 0;
            }
        }
        if let Some(new_class) = resolved_type(action) {
            chars[index].bidi_class = new_class;
        }
        if action & 0x100 != 0 {
            run_count += 1;
        }
        state = WEAK_STATES[state][class as usize];
    }
    if run_count != 0 {
        if let Some(class) = deferred_type(WEAK_ACTIONS[state][direction(0) as usize]) {
            set_deferred_run_class(chars, chars.len(), run_count, class);
        }
    }
}
fn resolve_neutrals(chars: &mut [CfgasChar]) {
    if chars.len() <= 1 {
        return;
    }
    let mut level = 0;
    let mut run_count = 0;
    let mut state = 1; // FX_BNSl
    for index in 0..chars.len() {
        let class = chars[index].bidi_class;
        if class == BidiClass::BN {
            if run_count != 0 {
                run_count += 1;
            }
            continue;
        }
        if class >= BidiClass::AL {
            continue;
        }
        let action = NEUTRAL_ACTIONS[state][class as usize];
        let run_class = deferred_neutrals(action, level);
        if run_class != BidiClass::ON && run_count > 0 {
            set_deferred_run_class(chars, index, run_count, run_class);
            run_count = 0;
        }
        let new_class = resolved_type(action).expect("neutral action has a class");
        if new_class != BidiClass::ON {
            chars[index].bidi_class = new_class;
        }
        if action & 0x100 != 0 {
            run_count += 1;
        }
        state = NEUTRAL_STATES[state][class as usize];
        level = chars[index].bidi_level;
    }
    if run_count != 0 {
        let class = deferred_neutrals(NEUTRAL_ACTIONS[state][direction(level) as usize], level);
        if class != BidiClass::ON {
            set_deferred_run_class(chars, chars.len(), run_count, class);
        }
    }
}
fn resolve_implicit(chars: &mut [CfgasChar]) {
    for ch in chars {
        let class = ch.bidi_class;
        if class == BidiClass::BN || class <= BidiClass::ON || class >= BidiClass::AL {
            continue;
        }
        ch.bidi_level += ADD_LEVELS[usize::from(ch.bidi_level & 1)][class as usize - 1];
    }
}
fn resolve_whitespace(chars: &mut [CfgasChar]) {
    if chars.len() <= 1 {
        return;
    }
    let mut level = 0;
    let mut run_count = 0;
    for index in 0..chars.len() {
        match chars[index].bidi_class {
            BidiClass::WS => run_count += 1,
            BidiClass::RLE
            | BidiClass::LRE
            | BidiClass::LRO
            | BidiClass::RLO
            | BidiClass::PDF
            | BidiClass::BN => {
                chars[index].bidi_level = level;
                run_count += 1;
            }
            BidiClass::S | BidiClass::B => {
                if run_count > 0 {
                    set_deferred_run_level(chars, index, run_count, 0);
                }
                chars[index].bidi_level = 0;
                run_count = 0;
            }
            _ => run_count = 0,
        }
        level = chars[index].bidi_level;
    }
    if run_count != 0 {
        set_deferred_run_level(chars, chars.len(), run_count, 0);
    }
}
fn reorder_level(chars: &mut [CfgasChar], base_level: u16, start: usize, reverse: bool) -> usize {
    let reverse = reverse || base_level & 1 != 0;
    let mut index = start;
    while index < chars.len() {
        let level = chars[index].bidi_level;
        if level == base_level {
            index += 1;
            continue;
        }
        if level < base_level {
            break;
        }
        index += reorder_level(chars, base_level + 1, index, reverse);
    }
    if reverse && index - start > 1 {
        chars[start..index].reverse();
    }
    index - start
}
fn reorder(chars: &mut [CfgasChar]) {
    let mut index = 0;
    while index < chars.len() {
        index += reorder_level(chars, 0, index, false);
    }
}
fn position(chars: &mut [CfgasChar]) {
    for index in 0..chars.len() {
        let original = usize::from(chars[index].bidi_pos);
        // Source used > count and could write one past the line's prefix.
        if original >= chars.len() {
            continue;
        }
        chars[original].bidi_order = index as u16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chars(text: &str) -> Vec<CfgasChar> {
        text.encode_utf16()
            .enumerate()
            .map(|(index, ch)| {
                let mut ch = CfgasChar::new(ch);
                ch.bidi_pos = index as u16;
                ch.identity = index as u32;
                ch
            })
            .collect()
    }
    fn text(chars: &[CfgasChar]) -> String {
        String::from_utf16_lossy(&chars.iter().map(CfgasChar::char_code).collect::<Vec<_>>())
    }
    #[test]
    fn bidi_latin_hebrew_arabic_numbers_and_identity() {
        for (logical, visual) in [
            ("abc", "abc"),
            ("אב", "בא"),
            ("a אב 12 b", "a 12 בא b"),
            ("اب", "با"),
        ] {
            let mut line = chars(logical);
            let count = line.len();
            CfgasChar::bidi_line(&mut line, count);
            assert_eq!(text(&line), visual, "{logical}");
            let mut ids = line.iter().map(|ch| ch.identity).collect::<Vec<_>>();
            ids.sort_unstable();
            assert_eq!(ids, (0..count as u32).collect::<Vec<_>>());
            for index in 0..count {
                let visual = line
                    .iter()
                    .position(|ch| usize::from(ch.bidi_pos) == index)
                    .unwrap();
                assert_eq!(usize::from(line[index].bidi_order), visual);
            }
        }
    }
    #[test]
    fn bidi_whitespace_controls_prefix_bounds_and_empty_lines() {
        let mut line = chars("אב \t");
        let count = line.len();
        CfgasChar::bidi_line(&mut line, count);
        assert_eq!(text(&line), "בא \t");
        assert!(line[2..].iter().all(|ch| ch.bidi_level == 0));
        let mut line = chars("אבz");
        line[0].bidi_pos = 2;
        line[2].bidi_order = 42;
        CfgasChar::bidi_line(&mut line, 2);
        assert_eq!(line[2].char_code(), u16::from(b'z'));
        assert_eq!(line[2].bidi_order, 42);
        CfgasChar::bidi_line(&mut [], 0);
        let mut one = chars("א");
        CfgasChar::bidi_line(&mut one, 1);
        assert_eq!(text(&one), "א");
        let mut controls = chars("a\u{202e}אב\u{202c}\u{feff}");
        let count = controls.len();
        CfgasChar::bidi_line(&mut controls, count);
        assert!(controls.iter().all(|ch| ch.bidi_level <= 2));
    }
    #[test]
    fn character_values_and_source_scale_narrowing() {
        let ch = CfgasChar::new(0x0628);
        assert_eq!(ch.get_char_type(), CharType::ArabicDistortion);
        assert_eq!(ch.horizontal_scale(), 100);
        assert_eq!(ch.vertical_scale(), 100);
        let mut ch = CfgasChar::with_scales(u16::from(b'a'), 40_000, -40_000);
        assert_eq!(ch.horizontal_scale(), -25_536);
        assert_eq!(ch.vertical_scale(), 25_536);
        ch.identity = 47;
        ch.char_width = 123;
        ch.font_size = 240;
        let other = ch.clone();
        assert_eq!(other.identity, 47);
        assert_eq!(other.char_width, 123);
        assert_eq!(other.font_size, 240);
    }
}
