// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Source CFGAS_Break. Two owned buffers and integer indices replace its
//! self-referential current/ready line pointers.
use super::{
    cfgas_breakline::CfgasBreakLine,
    cfgas_breakpiece::CfgasBreakPiece,
    cfgas_char::{BreakType, CfgasChar},
};
use crate::{fxcrt::fx_unicode::CharType, xfa::fgas::font::cfgas_gefont::CfgasGeFont};
use std::{
    ops::{BitAnd, BitOr, BitOrAssign},
    rc::Rc,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LayoutStyle(pub u8);
impl LayoutStyle {
    pub const NONE: Self = Self(0);
    pub const PAGINATION: Self = Self(1);
    pub const EXPAND_TAB: Self = Self(2);
    pub const SINGLE_LINE: Self = Self(4);
    pub const COMB_TEXT: Self = Self(8);
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}
impl BitOr for LayoutStyle {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
impl BitAnd for LayoutStyle {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}
impl BitOrAssign for LayoutStyle {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Tpo {
    pub index: usize,
    pub pos: i32,
}
pub const CONVERSION_FACTOR: f32 = 20_000.;
pub const MINIMUM_TAB_WIDTH: i32 = 160_000;
pub fn scaled(value: f32) -> i32 {
    value.round() as i32
}
pub struct CfgasBreak {
    pub char_type: CharType,
    pub single_line: bool,
    pub comb_text: bool,
    pub layout_styles: LayoutStyle,
    pub identity: u32,
    pub line_start: i32,
    pub line_width: i32,
    pub w_paragraph_break_char: u32,
    pub font_size: i32,
    pub tab_width: i32,
    pub horizontal_scale: i32,
    pub vertical_scale: i32,
    pub tolerance: i32,
    pub char_space: i32,
    pub font: Option<Rc<CfgasGeFont>>,
    pub ready_line_index: i8,
    pub current_line: usize,
    pub lines: [CfgasBreakLine; 2],
}
impl Default for CfgasBreak {
    fn default() -> Self {
        Self::new(LayoutStyle::NONE)
    }
}
impl CfgasBreak {
    pub fn new(layout_styles: LayoutStyle) -> Self {
        Self {
            char_type: CharType::Unknown,
            single_line: false,
            comb_text: false,
            layout_styles,
            identity: 0,
            line_start: 0,
            line_width: 2_000_000,
            w_paragraph_break_char: 10,
            font_size: 240,
            tab_width: 720_000,
            horizontal_scale: 100,
            vertical_scale: 100,
            tolerance: 0,
            char_space: 0,
            font: None,
            ready_line_index: -1,
            current_line: 0,
            lines: std::array::from_fn(|_| CfgasBreakLine::default()),
        }
    }
    pub fn cur_line(&self) -> &CfgasBreakLine {
        &self.lines[self.current_line]
    }
    pub fn cur_line_mut(&mut self) -> &mut CfgasBreakLine {
        &mut self.lines[self.current_line]
    }
    pub fn line_pair_mut(&mut self) -> (&mut CfgasBreakLine, &mut CfgasBreakLine) {
        let (a, b) = self.lines.split_at_mut(1);
        if self.current_line == 0 {
            (&mut a[0], &mut b[0])
        } else {
            (&mut b[0], &mut a[0])
        }
    }
    pub fn reset(&mut self) {
        self.char_type = CharType::Unknown;
        for line in &mut self.lines {
            line.clear();
        }
        // Cleared ready buffers are no longer pending output. The source left
        // this index live, causing every later EndBreak to return no pieces.
        self.ready_line_index = -1;
    }
    pub fn set_layout_styles(&mut self, styles: LayoutStyle) {
        self.layout_styles = styles;
        self.single_line = styles.contains(LayoutStyle::SINGLE_LINE);
        self.comb_text = styles.contains(LayoutStyle::COMB_TEXT);
    }
    pub fn get_layout_styles(&self) -> LayoutStyle {
        self.layout_styles
    }
    pub fn set_horizontal_scale(&mut self, scale: i32) {
        let scale = scale.max(0);
        if self.horizontal_scale != scale {
            self.set_break_status();
            self.horizontal_scale = scale;
        }
    }
    pub fn set_vertical_scale(&mut self, scale: i32) {
        let scale = scale.max(0);
        if self.vertical_scale != scale {
            self.set_break_status();
            self.vertical_scale = scale;
        }
    }
    pub fn set_font(&mut self, font: Rc<CfgasGeFont>) {
        if self.font.as_ref().is_some_and(|old| Rc::ptr_eq(old, &font)) {
            return;
        }
        self.set_break_status();
        self.font = Some(font);
    }
    pub fn set_font_size(&mut self, size: f32) {
        let size = scaled(size * 20.);
        if self.font_size != size {
            self.set_break_status();
            self.font_size = size;
        }
    }
    pub fn set_break_status(&mut self) {
        self.identity = self.identity.wrapping_add(1);
        if let Some(ch) = self.cur_line_mut().last_char_mut() {
            if ch.status == BreakType::None {
                ch.status = BreakType::Piece;
            }
        }
    }
    pub fn has_line(&self) -> bool {
        self.ready_line_index >= 0
    }
    pub fn is_greater_than_line_width(&self, width: i32) -> bool {
        self.line_width
            .checked_add(self.tolerance)
            .is_some_and(|limit| width > limit)
    }
    pub fn get_unified_char_type(&self, kind: CharType) -> CharType {
        if kind >= CharType::ArabicAlef {
            CharType::Arabic
        } else {
            kind
        }
    }
    pub fn set_tab_width(&mut self, width: f32) {
        self.tab_width = scaled(width * CONVERSION_FACTOR).max(MINIMUM_TAB_WIDTH);
    }
    pub fn get_tab_width(&self) -> i32 {
        self.tab_width
    }
    pub fn set_paragraph_break_char(&mut self, ch: u32) {
        if ch == 10 || ch == 13 {
            self.w_paragraph_break_char = ch;
        }
    }
    pub fn set_line_break_tolerance(&mut self, tolerance: f32) {
        self.tolerance = scaled(tolerance * CONVERSION_FACTOR);
    }
    pub fn set_char_space(&mut self, space: f32) {
        self.char_space = scaled(space * CONVERSION_FACTOR);
    }
    pub fn set_line_boundary(&mut self, start: f32, end: f32) {
        if start > end {
            return;
        }
        self.line_start = scaled(start * CONVERSION_FACTOR);
        self.line_width = scaled(end * CONVERSION_FACTOR);
        let low = self.line_start;
        let high = self.line_width;
        self.cur_line_mut().start = self.cur_line().start.min(high).max(low);
    }
    pub fn get_last_char_index(&self, mut index: i32, omit: bool, rich: bool) -> Option<usize> {
        let chars = &self.cur_line().line_chars;
        if index < 0 || index as usize >= chars.len() {
            return None;
        }
        for (i, ch) in chars.iter().enumerate().rev() {
            if ((rich && ch.char_width < 0) || omit) && ch.get_char_type() == CharType::Combination
            {
                continue;
            }
            index -= 1;
            if index < 0 {
                return Some(i);
            }
        }
        None
    }
    pub fn get_last_char(&self, index: i32, omit: bool, rich: bool) -> Option<&CfgasChar> {
        self.get_last_char_index(index, omit, rich)
            .map(|i| &self.cur_line().line_chars[i])
    }
    pub fn count_break_pieces(&self) -> i32 {
        if self.has_line() {
            self.lines[self.ready_line_index as usize]
                .line_pieces
                .len()
                .min(i32::MAX as usize) as i32
        } else {
            0
        }
    }
    pub fn get_break_piece(&self, index: i32) -> Option<(&CfgasBreakPiece, &[CfgasChar])> {
        if !self.has_line() || index < 0 {
            return None;
        }
        let line = &self.lines[self.ready_line_index as usize];
        Some((line.line_pieces.get(index as usize)?, &line.line_chars))
    }
    pub fn get_break_piece_unstable(&self, index: i32) -> Option<&CfgasBreakPiece> {
        self.get_break_piece(index).map(|(piece, _)| piece)
    }
    pub fn clear_break_pieces(&mut self) {
        if self.has_line() {
            self.lines[self.ready_line_index as usize].clear();
        }
        self.ready_line_index = -1;
    }
    pub fn get_current_line_for_testing(&self) -> &CfgasBreakLine {
        self.cur_line()
    }
}
