// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct port of `xfa/fgas/layout/cfgas_rtfbreak.{h,cpp}`. The two line buffers
//! own their characters; pieces refer to ranges in the corresponding buffer.
use super::cfgas_break::{CfgasBreak, LayoutStyle};
use super::cfgas_breakpiece::CfgasBreakPiece;
use super::cfgas_char::{BreakType, CfgasChar};
use super::cfgas_textpiece::CfgasTextPiece;
use super::cfgas_textuserdata::CfgasTextUserData;
use super::fgas_arabic::{get_arabic_form_char, get_arabic_form_char_from_chars};
use super::fgas_linebreak::{get_line_break_type_from_pair, LineBreakType};
use crate::fxcrt::fx_coordinates::CfxPointF;
use crate::fxcrt::fx_unicode::{
    get_bidi_class, get_break_property, get_char_type, get_mirror_char, BidiClass, BreakProperty,
    CharType,
};
use crate::fxge::text_char_pos::TextCharPos;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

const CONVERSION_FACTOR: f32 = 20_000.0;
const ZERO_WIDTH_NO_BREAK_SPACE: u32 = 0xfeff;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineAlignment {
    #[default]
    Left,
    Center,
    Right,
    Justified,
    Distributed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::fx_coordinates::CfxRectF;
    use crate::xfa::fgas::font::cfgas_gefont::CfgasGeFont;

    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn breaker(styles: LayoutStyle) -> CfgasRtfBreak {
        // Preserve the native text/control assertions with bundled metrics
        // rather than requiring Arimo Bold in the host's font installation.
        let font = CfgasGeFont::from_font(
            crate::fxge::cfx_font::CfxFont::from_span(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../upstream/pdfium/testing/resources/fonts/roboto.ttf"
                )),
                false,
                0,
            )
            .unwrap(),
        )
        .unwrap();
        let mut breaker = CfgasRtfBreak::new(styles);
        breaker.set_font(font);
        breaker
    }
    fn piece_string(breaker: &CfgasRtfBreak, index: i32) -> Vec<u32> {
        let (piece, chars) = breaker.get_break_piece(index).expect("ready break piece");
        piece.get_string(chars)
    }
    // cfgas_rtfbreak_unittest.cpp: AddChars. The same source strings, every
    // append status, piece count/string and reset line-end assertion.
    #[test]
    fn hebrew_rich_breaks_keep_logical_widths_and_directional_visual_order() {
        for (logical, visual) in [
            ("בני", "ינב"),
            ("a בנ 12 b", "a 12 נב b"),
            ("\u{202a}AB\u{202c}", "\u{202a}AB\u{202c}"),
        ] {
            let mut breaker = breaker(LayoutStyle::EXPAND_TAB);
            breaker.set_line_boundary(0., 200.);
            for ch in logical.chars() {
                breaker.append_char(u32::from(ch));
            }
            let widths: Vec<_> = breaker
                .cur_line()
                .line_chars
                .iter()
                .map(|ch| ch.char_width)
                .collect();
            breaker.end_break(BreakType::Paragraph);
            let (_, chars) = breaker.get_break_piece(0).unwrap();
            assert_eq!(
                chars.iter().map(CfgasChar::char_code).collect::<Vec<_>>(),
                logical.encode_utf16().collect::<Vec<_>>()
            );
            assert_eq!(
                chars.iter().map(|ch| ch.char_width).collect::<Vec<_>>(),
                widths
            );
            let mut visual_chars: Vec<_> = chars.iter().collect();
            visual_chars.sort_by_key(|ch| ch.bidi_order);
            assert_eq!(
                visual_chars
                    .iter()
                    .map(|ch| ch.char_code())
                    .collect::<Vec<_>>(),
                visual.encode_utf16().collect::<Vec<_>>()
            );
            for ch in chars
                .iter()
                .filter(|ch| get_bidi_class(u32::from(ch.char_code())) == BidiClass::R)
            {
                assert_eq!(ch.bidi_level & 1, 1);
            }
        }
    }
    #[test]
    fn add_chars() {
        let mut breaker = breaker(LayoutStyle::EXPAND_TAB);
        for ch in "Input String.".chars() {
            assert_eq!(breaker.append_char(u32::from(ch)), BreakType::None);
        }
        assert_eq!(breaker.append_char(10), BreakType::Paragraph);
        assert_eq!(breaker.count_break_pieces(), 1);
        assert_eq!(piece_string(&breaker, 0), wide("Input String.\n"));
        breaker.clear_break_pieces();
        breaker.reset();
        assert_eq!(breaker.get_current_line_for_testing().get_line_end(), 0);
        for ch in "Second str.".chars() {
            assert_eq!(breaker.append_char(u32::from(ch)), BreakType::None);
        }
        breaker.end_break(BreakType::Paragraph);
        assert_eq!(breaker.count_break_pieces(), 1);
        assert_eq!(piece_string(&breaker, 0), wide("Second str."));
    }
    // cfgas_rtfbreak_unittest.cpp: ControlCharacters, retaining its intentional
    // sequence with unconsumed ready pieces and pending next-line controls.
    #[test]
    fn control_characters() {
        let mut breaker = breaker(LayoutStyle::EXPAND_TAB);
        for (ch, expected) in [
            (11, BreakType::Line),
            (12, BreakType::Page),
            (0x2029, BreakType::Paragraph),
            (10, BreakType::Paragraph),
        ] {
            assert_eq!(breaker.append_char(ch), expected);
        }
        assert_eq!(breaker.count_break_pieces(), 1);
        assert_eq!(piece_string(&breaker, 0), wide("\u{b}"));
    }
    // cfgas_rtfbreak_unittest.cpp: BidiLine, including the embedded NUL.
    #[test]
    fn bidi_line() {
        let mut breaker = breaker(LayoutStyle::EXPAND_TAB);
        breaker.set_line_break_tolerance(1.0);
        breaker.set_font_size(12.0);
        for ch in [10, 0, 10, 10] {
            breaker.append_char(ch);
        }
        let mut chars = breaker.get_current_line_for_testing().line_chars.clone();
        let count = chars.len();
        CfgasChar::bidi_line(&mut chars, count);
        assert_eq!(chars.len(), 3);
    }
    #[test]
    fn real_metrics_positioned_tabs_alignment_and_pagination() {
        let mut breaker = breaker(LayoutStyle::EXPAND_TAB);
        breaker.set_line_boundary(10.0, 110.0);
        breaker.set_line_start_pos(10.0);
        breaker.add_positioned_tab(30.0);
        breaker.add_positioned_tab(30.0);
        assert_eq!(breaker.positioned_tabs.len(), 1);
        assert_eq!(breaker.append_char(u32::from(b'A')), BreakType::None);
        assert!(breaker.cur_line().width > 0);
        breaker.append_char(9);
        assert_eq!(breaker.cur_line().width, 600_000);
        breaker.append_char(u32::from(b'B'));
        breaker.set_alignment(LineAlignment::Right);
        breaker.end_break(BreakType::Paragraph);
        let (piece, _) = breaker.get_break_piece(0).unwrap();
        assert_eq!(piece.get_end_pos(), 2_200_000);

        let mut pagination = self::breaker(LayoutStyle::PAGINATION);
        pagination.append_char(u32::from(b'A'));
        pagination.set_font_size(14.0);
        pagination.append_char(u32::from(b'B'));
        pagination.end_break(BreakType::Paragraph);
        assert_eq!(pagination.count_break_pieces(), 2);
        let (first, _) = pagination.get_break_piece(0).unwrap();
        let (second, _) = pagination.get_break_piece(1).unwrap();
        // Correct source's initial start=-1 (default piece width was -1).
        assert_eq!(first.start_pos, 0);
        assert_eq!(first.font_size, 240);
        assert_eq!(second.font_size, 280);
        assert_eq!(second.start_pos, first.get_end_pos());
        assert_eq!(piece_string(&pagination, 0), wide("A"));
        assert_eq!(piece_string(&pagination, 1), wide("B"));
    }
    #[test]
    fn combining_marks_overlay_and_display_positions_are_bounded() {
        let mut breaker = breaker(LayoutStyle::NONE);
        breaker.append_char(u32::from(b'A'));
        let base_width = breaker.cur_line().width;
        breaker.append_char(0x0301);
        assert_eq!(breaker.cur_line().width, base_width);
        assert!(breaker.cur_line().line_chars[1].char_width <= 0);
        let font = breaker.font.clone().unwrap();
        let width = i32::from(font.get_char_width(u32::from(b'A')).unwrap()) * 240;
        let piece = CfgasTextPiece {
            text: wide("AA"),
            widths: vec![width, width],
            chars: 2,
            horizontal_scale: 100,
            vertical_scale: 100,
            bidi_level: 0,
            font_size: 12.0,
            rect: CfxRectF::new(3.0, 5.0, 100.0, 20.0),
            font: Some(font),
        };
        let mut positions = [TextCharPos::default(); 2];
        assert_eq!(breaker.get_display_pos(&piece, &mut positions), 2);
        assert_eq!(positions[0].origin.x, 3.0);
        assert!(positions[1].origin.x > positions[0].origin.x);
        assert_ne!(positions[0].glyph_index, 0xffff);
        assert_eq!(positions[0].adjust_matrix, [-1.0, 0.0, 0.0, 1.0]);
        assert_eq!(breaker.get_display_pos(&piece, &mut positions[..1]), 1);
        assert_eq!(breaker.get_display_pos(&piece, &mut []), 0);
        let mut rtl = piece.clone();
        rtl.bidi_level = 1;
        assert_eq!(breaker.get_display_pos(&rtl, &mut positions), 2);
        assert!(positions[0].origin.x > positions[1].origin.x);
    }
}

pub struct CfgasRtfBreak {
    pub base: CfgasBreak,
    pagination: bool,
    alignment: LineAlignment,
    positioned_tabs: Vec<i32>,
    user_data: Option<Rc<CfgasTextUserData>>,
}
impl Deref for CfgasRtfBreak {
    type Target = CfgasBreak;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
impl DerefMut for CfgasRtfBreak {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl CfgasRtfBreak {
    pub fn new(layout_styles: LayoutStyle) -> Self {
        let mut base = CfgasBreak::new(layout_styles);
        base.set_break_status();
        Self {
            base,
            pagination: layout_styles.contains(LayoutStyle::PAGINATION),
            alignment: LineAlignment::Left,
            positioned_tabs: Vec::new(),
            user_data: None,
        }
    }
    pub fn set_line_start_pos(&mut self, position: f32) {
        let position = ((position * CONVERSION_FACTOR).round() as i32)
            .min(self.line_width)
            .max(self.line_start);
        self.base.cur_line_mut().start = position;
    }
    pub fn set_alignment(&mut self, alignment: LineAlignment) {
        self.alignment = alignment;
    }
    pub fn add_positioned_tab(&mut self, position: f32) {
        let position = ((position * CONVERSION_FACTOR).round() as i32)
            .saturating_add(self.line_start)
            .min(self.line_width);
        if let Err(index) = self.positioned_tabs.binary_search(&position) {
            self.positioned_tabs.insert(index, position);
        }
    }
    pub fn set_user_data(&mut self, user_data: Option<Rc<CfgasTextUserData>>) {
        if match (&self.user_data, &user_data) {
            (None, None) => true,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        } {
            return;
        }
        self.base.set_break_status();
        self.user_data = user_data;
    }
    fn get_positioned_tab(&self, position: i32) -> Option<i32> {
        self.positioned_tabs
            .get(self.positioned_tabs.partition_point(|&tab| tab <= position))
            .copied()
    }
    fn current_char_index(&self) -> Option<usize> {
        self.cur_line().line_chars.len().checked_sub(1)
    }
    fn last_char_index(&self, index: i32) -> Option<usize> {
        self.base.get_last_char_index(index, false, true)
    }
    fn scaled_width(&self, code: u32) -> i32 {
        let width = self
            .font
            .as_ref()
            .and_then(|font| font.get_char_width(code))
            .unwrap_or(0);
        i32::from(width)
            .checked_mul(self.font_size)
            .and_then(|value| value.checked_mul(self.horizontal_scale))
            .map(|value| value / 100)
            .unwrap_or(0)
    }
    fn arabic_width(&self, form: u32, original: u32) -> i32 {
        let width = self
            .font
            .as_ref()
            .and_then(|font| {
                font.get_char_width(form)
                    .or_else(|| font.get_char_width(original))
            })
            .unwrap_or(0);
        i32::from(width)
            .checked_mul(self.font_size)
            .and_then(|value| value.checked_mul(self.horizontal_scale))
            .map(|value| value / 100)
            .unwrap_or(0)
    }
    fn add_current_width(&mut self, width: i32) -> bool {
        let Some(width) = self.cur_line().width.checked_add(width) else {
            return false;
        };
        self.base.cur_line_mut().width = width;
        true
    }
    pub fn append_char(&mut self, code: u32) -> BreakType {
        let char_type = get_char_type(code);
        let mut character =
            CfgasChar::with_scales(code as u16, self.horizontal_scale, self.vertical_scale);
        character.font_size = self.font_size;
        character.identity = self.identity;
        character.user_data = self.user_data.clone();
        self.base.cur_line_mut().line_chars.push(character);
        let mut first = BreakType::None;
        if char_type != CharType::Combination
            && self.get_unified_char_type(self.char_type) != self.get_unified_char_type(char_type)
            && self.char_type != CharType::Unknown
            && self.is_greater_than_line_width(self.cur_line().get_line_end())
            && (self.char_type != CharType::Space || char_type != CharType::Control)
        {
            first = self.end_break(BreakType::Line);
        }
        let Some(index) = self.current_char_index() else {
            self.base.char_type = char_type;
            return first;
        };
        let second = match char_type {
            CharType::Tab => {
                self.append_tab(index);
                BreakType::None
            }
            CharType::Control => self.append_control(index),
            CharType::Combination => {
                self.append_combination(index);
                BreakType::None
            }
            CharType::ArabicAlef
            | CharType::ArabicSpecial
            | CharType::ArabicDistortion
            | CharType::ArabicNormal
            | CharType::ArabicForm
            | CharType::Arabic => self.append_arabic(index),
            _ => self.append_others(index),
        };
        self.base.char_type = char_type;
        first.max(second)
    }
    fn append_combination(&mut self, index: usize) {
        let mut width = self.scaled_width(u32::from(self.cur_line().line_chars[index].char_code()));
        // The current mark is already in the line. Look behind it rather than
        // rediscovering the mark itself, so combining widths overlay the base.
        if self.last_char_index(1).is_some_and(|last| {
            self.cur_line().line_chars[last].get_char_type() > CharType::Combination
        }) {
            width = width.checked_neg().unwrap_or(0);
        } else {
            self.base.char_type = CharType::Combination;
        }
        self.base.cur_line_mut().line_chars[index].char_width = width;
        if width > 0 {
            self.add_current_width(width);
        }
    }
    fn append_tab(&mut self, index: usize) {
        if !self.layout_styles.contains(LayoutStyle::EXPAND_TAB) {
            return;
        }
        let line_width = self.cur_line().width;
        let absolute_position = self.cur_line().get_line_end();
        let tab = self
            .get_positioned_tab(absolute_position)
            .map(|tab| tab.saturating_sub(self.cur_line().start))
            .or_else(|| {
                line_width
                    .checked_div(self.tab_width)?
                    .checked_add(1)?
                    .checked_mul(self.tab_width)
            });
        let width = tab.and_then(|tab| tab.checked_sub(line_width)).unwrap_or(0);
        self.base.cur_line_mut().line_chars[index].char_width = width;
        self.add_current_width(width);
    }
    fn append_control(&mut self, index: usize) -> BreakType {
        let code = u32::from(self.cur_line().line_chars[index].char_code());
        let status = match code {
            0x0b | 0x2028 => BreakType::Line,
            0x0c => BreakType::Page,
            0x2029 => BreakType::Paragraph,
            _ if code == self.w_paragraph_break_char => BreakType::Paragraph,
            _ => BreakType::None,
        };
        if status == BreakType::None {
            status
        } else {
            self.end_break(status)
        }
    }
    fn append_arabic(&mut self, index: usize) -> BreakType {
        self.base.cur_line_mut().increment_arabic_char_count();
        let mut last = None;
        let mut alef = false;
        if (CharType::ArabicAlef..=CharType::ArabicDistortion).contains(&self.char_type) {
            last = self.last_char_index(1);
            if let Some(last_index) = last {
                let previous = self.last_char_index(2);
                let chars = &self.cur_line().line_chars;
                let form = get_arabic_form_char_from_chars(
                    Some(&chars[last_index]),
                    previous.map(|i| &chars[i]),
                    Some(&chars[index]),
                );
                alef = form == ZERO_WIDTH_NO_BREAK_SPACE
                    && chars[last_index].get_char_type() == CharType::ArabicAlef;
                let old_width = chars[last_index].char_width;
                let width = self.arabic_width(form, u32::from(chars[last_index].char_code()));
                self.base.cur_line_mut().width = self.cur_line().width.saturating_sub(old_width);
                self.base.cur_line_mut().line_chars[last_index].char_width = width;
                if !self.add_current_width(width) {
                    return BreakType::None;
                }
            }
        }
        let chars = &self.cur_line().line_chars;
        let form = get_arabic_form_char_from_chars(
            Some(&chars[index]),
            if alef { None } else { last.map(|i| &chars[i]) },
            None,
        );
        let width = self.arabic_width(form, u32::from(chars[index].char_code()));
        self.base.cur_line_mut().line_chars[index].char_width = width;
        if !self.add_current_width(width) {
            return BreakType::None;
        }
        if self.is_greater_than_line_width(self.cur_line().get_line_end()) {
            self.end_break(BreakType::Line)
        } else {
            BreakType::None
        }
    }
    fn append_others(&mut self, index: usize) -> BreakType {
        let character = &self.cur_line().line_chars[index];
        let char_type = character.get_char_type();
        let width = self
            .scaled_width(u32::from(character.char_code()))
            .checked_add(self.char_space)
            .unwrap_or(0);
        self.base.cur_line_mut().line_chars[index].char_width = width;
        if !self.add_current_width(width) {
            return BreakType::None;
        }
        if char_type != CharType::Space
            && self.is_greater_than_line_width(self.cur_line().get_line_end())
        {
            self.end_break(BreakType::Line)
        } else {
            BreakType::None
        }
    }
    pub fn end_break(&mut self, status: BreakType) -> BreakType {
        if status == BreakType::None {
            return BreakType::None;
        }
        self.base.identity = self.identity.wrapping_add(1);
        if let Some(piece) = self.base.cur_line_mut().line_pieces.last_mut() {
            if status != BreakType::Piece {
                piece.status = status;
            }
            return piece.status;
        }
        if self.ready_line_index >= 0 {
            let ready = self.ready_line_index as usize;
            if let Some(piece) = self.base.lines[ready].line_pieces.last_mut() {
                if status != BreakType::Piece {
                    piece.status = status;
                }
                return piece.status;
            }
            return BreakType::None;
        }
        let Some(character) = self.base.cur_line_mut().line_chars.last_mut() else {
            return BreakType::None;
        };
        character.status = status;
        if status == BreakType::Piece {
            return status;
        }
        self.base.ready_line_index = self.current_line as i8;
        let next_line = 1 - self.current_line;
        let all = matches!(
            self.alignment,
            LineAlignment::Justified | LineAlignment::Distributed
        );
        if !self.end_break_split_line(next_line, all, status) {
            let positions = self.end_break_bidi_line(status);
            if !self.pagination && self.alignment != LineAlignment::Left {
                self.end_break_alignment(&positions, all, status);
            }
        }
        self.base.current_line = next_line;
        self.base.cur_line_mut().start = self.line_start;
        self.base.char_type = self.last_char_index(0).map_or(CharType::Unknown, |index| {
            self.cur_line().line_chars[index].get_char_type()
        });
        status
    }
    fn end_break_split_line(&mut self, next: usize, all: bool, _status: BreakType) -> bool {
        if self.is_greater_than_line_width(self.cur_line().get_line_end()) {
            if self.cur_line().line_chars.last().is_some_and(|character| {
                !matches!(
                    character.get_char_type(),
                    CharType::Tab | CharType::Control | CharType::Space
                )
            }) {
                self.split_text_line(next, !self.pagination && all);
            } else if !self.pagination && all {
                let mut end = self.cur_line().get_line_end();
                let line_width = self.line_width;
                Self::get_break_pos(
                    &mut self.base.cur_line_mut().line_chars,
                    all,
                    true,
                    &mut end,
                    line_width,
                );
            }
        } else if !self.pagination && all {
            let mut end = self.cur_line().get_line_end();
            let line_width = self.line_width;
            Self::get_break_pos(
                &mut self.base.cur_line_mut().line_chars,
                all,
                true,
                &mut end,
                line_width,
            );
        }
        if !self.pagination {
            return false;
        }
        let line = self.base.cur_line_mut();
        let mut index = 0;
        let mut start = line.start;
        while index < line.line_chars.len() {
            let character = &line.line_chars[index];
            let identity = character.identity;
            let mut piece = CfgasBreakPiece::default();
            piece.start_pos = start;
            piece.width = 0;
            piece.start_char = index as i32;
            piece.font_size = character.font_size;
            piece.horizontal_scale = character.horizontal_scale();
            piece.vertical_scale = character.vertical_scale();
            piece.user_data = character.user_data.clone();
            loop {
                let character = &line.line_chars[index];
                if character.identity != identity {
                    break;
                }
                piece.status = character.status;
                piece.width = piece.width.saturating_add(character.char_width);
                index += 1;
                if index == line.line_chars.len() || character.status != BreakType::None {
                    break;
                }
            }
            piece.char_count = index as i32 - piece.start_char;
            start = start.saturating_add(piece.width);
            line.line_pieces.push(piece);
        }
        true
    }
    fn end_break_bidi_line(&mut self, status: BreakType) -> Vec<(usize, i32)> {
        let bidi = !self.pagination
            && (self.cur_line().has_arabic_char()
                || self.cur_line().line_chars.iter().any(|ch| {
                    matches!(
                        get_bidi_class(u32::from(ch.char_code())),
                        BidiClass::R
                            | BidiClass::AL
                            | BidiClass::RLE
                            | BidiClass::RLO
                            | BidiClass::LRE
                            | BidiClass::LRO
                            | BidiClass::PDF
                    )
                }));
        let line = self.base.cur_line_mut();
        if bidi {
            let mut count = 0;
            for (index, character) in line.line_chars.iter_mut().enumerate() {
                character.bidi_pos = index as u16;
                if character.get_char_type() != CharType::Control {
                    count = index;
                }
                if index == 0 {
                    character.bidi_level = 1;
                }
            }
            CfgasChar::bidi_line(&mut line.line_chars, count + 1);
            // Rich pieces also retain their logical text/style owners. Drawing
            // uses each piece's direction, so keep visual order as metadata.
            for (visual, ch) in line.line_chars.iter_mut().enumerate() {
                ch.bidi_order = visual as u16;
            }
            for index in 0..line.line_chars.len() {
                loop {
                    let target = usize::from(line.line_chars[index].bidi_pos);
                    if target == index
                        || target >= line.line_chars.len()
                        || line.line_chars[target].bidi_pos == line.line_chars[index].bidi_pos
                    {
                        break;
                    }
                    line.line_chars.swap(index, target);
                }
            }
        } else {
            for character in &mut line.line_chars {
                character.bidi_level = 0;
                character.bidi_pos = 0;
                character.bidi_order = 0;
            }
        }
        let mut positions = Vec::new();
        let mut start = line.start;
        let mut index = 0;
        while index < line.line_chars.len() {
            let character = &line.line_chars[index];
            let identity = character.identity;
            let level = character.bidi_level;
            let mut piece = CfgasBreakPiece::default();
            piece.start_pos = start;
            piece.start_char = index as i32;
            piece.width = 0;
            piece.bidi_level = i32::from(level);
            piece.bidi_pos = i32::from(character.bidi_order);
            piece.font_size = character.font_size;
            piece.horizontal_scale = character.horizontal_scale();
            piece.vertical_scale = character.vertical_scale();
            piece.user_data = character.user_data.clone();
            while index < line.line_chars.len() {
                let character = &line.line_chars[index];
                if character.identity != identity || character.bidi_level != level {
                    break;
                }
                if character.char_width > 0 {
                    piece.width = piece.width.saturating_add(character.char_width);
                }
                index += 1;
            }
            piece.char_count = index as i32 - piece.start_char;
            piece.status = if index == line.line_chars.len() {
                status
            } else {
                BreakType::Piece
            };
            start = start.saturating_add(piece.width);
            positions.push((line.line_pieces.len(), piece.bidi_pos));
            line.line_pieces.push(piece);
        }
        positions.sort_by_key(|&(_, position)| position);
        let mut start = line.start;
        for &(index, _) in &positions {
            let piece = &mut line.line_pieces[index];
            piece.start_pos = start;
            start = start.saturating_add(piece.width);
        }
        positions
    }
    fn end_break_alignment(&mut self, positions: &[(usize, i32)], all: bool, status: BreakType) {
        let mut net_width = self.cur_line().width;
        let mut gaps = 0;
        let mut found = false;
        for &(index, _) in positions.iter().rev() {
            let piece = &self.cur_line().line_pieces[index];
            if !found {
                net_width = piece.start_pos.saturating_add(piece.width);
            }
            let arabic = piece.bidi_level & 1 != 0;
            let mut offset = if arabic { 0 } else { piece.char_count - 1 };
            while offset >= 0 && offset < piece.char_count {
                let character = &self.cur_line().line_chars[(piece.start_char + offset) as usize];
                if character.line_break_type == LineBreakType::DirectBreak {
                    gaps += 1;
                }
                if !found || !all {
                    if matches!(
                        character.get_char_type(),
                        CharType::Space | CharType::Control
                    ) {
                        if !found && all && character.char_width > 0 {
                            net_width = net_width.saturating_sub(character.char_width);
                        }
                    } else {
                        found = true;
                        if !all {
                            break;
                        }
                    }
                }
                offset += if arabic { 1 } else { -1 };
            }
            if !all && found {
                break;
            }
        }
        let mut offset = self.line_width.saturating_sub(net_width);
        if gaps > 0
            && (self.alignment == LineAlignment::Distributed
                || (self.alignment == LineAlignment::Justified && status != BreakType::Paragraph))
        {
            let line = self.base.cur_line_mut();
            let mut start = None;
            for &(index, _) in positions {
                let piece = &mut line.line_pieces[index];
                if let Some(position) = start {
                    piece.start_pos = position;
                }
                for position in piece.start_char..piece.start_char + piece.char_count {
                    let character = &mut line.line_chars[position as usize];
                    if character.line_break_type != LineBreakType::DirectBreak
                        || character.char_width < 0
                    {
                        continue;
                    }
                    let addition = offset / gaps;
                    character.char_width = character.char_width.saturating_add(addition);
                    piece.width = piece.width.saturating_add(addition);
                    offset -= addition;
                    gaps -= 1;
                    if gaps < 1 {
                        break;
                    }
                }
                start = Some(piece.start_pos.saturating_add(piece.width));
            }
        } else if matches!(self.alignment, LineAlignment::Right | LineAlignment::Center) {
            if self.alignment == LineAlignment::Center {
                offset /= 2;
            }
            if offset > 0 {
                for piece in &mut self.base.cur_line_mut().line_pieces {
                    piece.start_pos = piece.start_pos.saturating_add(offset);
                }
            }
        }
    }
    fn get_break_pos(
        chars: &mut [CfgasChar],
        all: bool,
        only_break: bool,
        end: &mut i32,
        line_width: i32,
    ) -> i32 {
        let Some(mut length) = chars.len().checked_sub(1) else {
            return -1;
        };
        if length < 1 {
            return length as i32;
        }
        let (mut direct, mut indirect, mut last) = (None, None, None);
        if *end <= line_width {
            if !all {
                return length as i32;
            }
            direct = Some((length as i32, *end));
        }
        if all {
            chars[length].line_break_type = LineBreakType::Unknown;
        }
        let mut next = get_break_property(u32::from(chars[length].char_code()));
        if chars[length].char_width > 0 {
            *end = end.saturating_sub(chars[length].char_width);
        }
        while length > 0 {
            length -= 1;
            let character = &mut chars[length];
            let current = get_break_property(u32::from(character.char_code()));
            let need_break = matches!(current, BreakProperty::TB | BreakProperty::SP);
            let kind = if (current == BreakProperty::TB && next == BreakProperty::TB)
                || (current != BreakProperty::TB && next == BreakProperty::SP)
            {
                LineBreakType::ProhibitedBreak
            } else {
                get_line_break_type_from_pair(current, next)
            };
            if all {
                character.line_break_type = kind;
            }
            if !only_break {
                if *end <= line_width || need_break {
                    if kind == LineBreakType::DirectBreak && direct.is_none() {
                        direct = Some((length as i32, *end));
                        if !all {
                            return length as i32;
                        }
                    } else if kind == LineBreakType::IndirectBreak && indirect.is_none() {
                        indirect = Some((length as i32, *end));
                    }
                    if last.is_none() {
                        last = Some((length as i32, *end));
                    }
                }
                if character.char_width > 0 {
                    *end = end.saturating_sub(character.char_width);
                }
            }
            next = current;
        }
        if only_break {
            return 0;
        }
        if let Some((index, position)) = direct.or(indirect).or(last) {
            *end = position;
            index
        } else {
            0
        }
    }
    fn split_text_line(&mut self, next: usize, all: bool) {
        if self.cur_line().line_chars.len() < 2 {
            return;
        }
        let mut end = self.cur_line().get_line_end();
        let width = self.line_width;
        let position = Self::get_break_pos(
            &mut self.base.cur_line_mut().line_chars,
            all,
            false,
            &mut end,
            width,
        )
        .max(0) as usize
            + 1;
        let current = self.current_line;
        if position >= self.cur_line().line_chars.len() {
            self.base.lines[next].clear();
            self.base.lines[current].line_chars[position - 1].line_break_type =
                LineBreakType::Unknown;
            return;
        }
        let moved = self.base.lines[current].line_chars.split_off(position);
        let next_width = self.base.lines[current].get_line_end().saturating_sub(end);
        self.base.lines[next].line_chars = moved;
        self.base.lines[next].start = self.base.lines[current].start;
        self.base.lines[next].width = next_width;
        // GetBreakPos returns an absolute coordinate; line widths exclude start.
        self.base.lines[current].width = end.saturating_sub(self.base.lines[current].start);
        self.base.lines[current].line_chars[position - 1].line_break_type = LineBreakType::Unknown;
        for index in 0..self.base.lines[next].line_chars.len() {
            if self.base.lines[next].line_chars[index].get_char_type() >= CharType::ArabicAlef {
                self.base.lines[current].decrement_arabic_char_count();
                self.base.lines[next].increment_arabic_char_count();
            }
            self.base.lines[next].line_chars[index].status = BreakType::None;
        }
    }
    pub fn get_display_pos(&self, piece: &CfgasTextPiece, output: &mut [TextCharPos]) -> usize {
        let Some(font) = piece.font.as_ref() else {
            return 0;
        };
        let count = (piece.chars.max(0) as usize)
            .min(piece.text.len())
            .min(piece.widths.len());
        if count == 0 {
            return 0;
        }
        let font_size = (piece.font_size * 20.0).round() as i32;
        if font_size == 0 {
            return 0;
        }
        let ascent = font.get_ascent();
        let maximum_height = ascent.saturating_sub(font.get_descent());
        let ascent = if maximum_height != 0 {
            piece.font_size * ascent as f32 / maximum_height as f32
        } else {
            0.0
        };
        let rtl = piece.bidi_level & 1 != 0;
        let mut previous = ZERO_WIDTH_NO_BREAK_SPACE;
        let mut x = if rtl {
            piece.rect.right()
        } else {
            piece.rect.left
        };
        let y = piece.rect.top + ascent;
        let mut used = 0;
        for index in 0..count {
            let code = piece.text[index];
            let width = piece.widths[index];
            let char_type = get_char_type(code);
            if width == 0 {
                if char_type == CharType::ArabicAlef {
                    previous = ZERO_WIDTH_NO_BREAK_SPACE;
                }
                continue;
            }
            let empty = (CharType::Tab..=CharType::Control).contains(&char_type);
            if !empty && used >= output.len() {
                break;
            }
            let character_width = (i64::from(width).abs() / i64::from(font_size))
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX))
                as i32;
            let form = if char_type >= CharType::ArabicAlef {
                let mut next_index = index + 1;
                if next_index < count && piece.widths[next_index] < 0 && next_index + 1 < count {
                    next_index += 1;
                }
                get_arabic_form_char(
                    code,
                    previous,
                    piece
                        .text
                        .get(next_index)
                        .filter(|_| next_index < count)
                        .copied()
                        .unwrap_or(ZERO_WIDTH_NO_BREAK_SPACE),
                )
            } else if rtl {
                get_mirror_char(code)
            } else {
                code
            };
            let advance = piece.font_size * character_width as f32 / 1000.0;
            if rtl && char_type != CharType::Combination {
                x -= advance;
            }
            if !empty {
                let mut glyph = font.get_glyph_index(form);
                if glyph == 0xffff {
                    glyph = font.get_glyph_index(code);
                }
                let character = &mut output[used];
                character.glyph_index = glyph as u32;
                character.unicode = code;
                character.font_char_width = character_width;
                character.origin =
                    CfxPointF::new(x, y + ascent * piece.vertical_scale as f32 / 100.0 - ascent);
                character.glyph_adjust = true;
                character.adjust_matrix = [
                    -(piece.horizontal_scale as f32) / 100.0,
                    0.0,
                    0.0,
                    piece.vertical_scale as f32 / 100.0,
                ];
                used += 1;
            }
            if !rtl && char_type != CharType::Combination {
                x += advance;
            }
            if width > 0 {
                previous = code;
            }
        }
        used
    }
}
