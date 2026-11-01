// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct text breaker from `cfgas_txtbreak.{h,cpp}`. Buffer indices replace
//! unowned pointers, and split_off moves trailing characters without cloning.
use super::{
    cfgas_break::{scaled, CfgasBreak, LayoutStyle, Tpo, CONVERSION_FACTOR},
    cfgas_breakpiece::CfgasBreakPiece,
    cfgas_char::{BreakType, CfgasChar},
    fgas_arabic::{
        get_arabic_form_char, get_arabic_form_char_from_chars, get_arabic_from_shadda_table,
    },
    fgas_linebreak::{get_line_break_type_from_pair, LineBreakType},
};
use crate::{
    fxcrt::{
        fx_coordinates::{CfxPointF, CfxRectF},
        fx_unicode::{
            get_bidi_class, get_break_property, get_char_type, get_mirror_char, BidiClass,
            BreakProperty, CharType,
        },
    },
    fxge::text_char_pos::TextCharPos,
    xfa::fgas::font::cfgas_gefont::CfgasGeFont,
};
use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
};
pub const FX_TXTCHARSTYLE_ARABIC_SHADDA: u32 = 0x20;
pub const FX_TXTCHARSTYLE_ODD_BIDI_LEVEL: u32 = 0x40;
pub const ALIGNMENT_LEFT: i32 = 0;
pub const ALIGNMENT_CENTER: i32 = 1;
pub const ALIGNMENT_RIGHT: i32 = 2;
pub const ALIGNMENT_JUSTIFIED: i32 = 4;
pub fn break_type_none_or_piece(kind: BreakType) -> bool {
    matches!(kind, BreakType::None | BreakType::Piece)
}
pub trait TxtBreakEngine {
    fn get_char(&self, index: usize) -> u32;
    fn get_width_of_char(&mut self, index: usize) -> i32;
}
pub struct TxtBreakRun<'a> {
    pub engine: Option<&'a mut dyn TxtBreakEngine>,
    pub text: &'a [u32],
    pub widths: &'a [i32],
    pub start: i32,
    pub length: i32,
    pub font: Option<Rc<CfgasGeFont>>,
    pub font_size: f32,
    pub styles: LayoutStyle,
    pub horizontal_scale: i32,
    pub vertical_scale: i32,
    pub char_styles: u32,
    pub rect: CfxRectF,
    pub skip_space: bool,
}
impl Default for TxtBreakRun<'_> {
    fn default() -> Self {
        Self {
            engine: None,
            text: &[],
            widths: &[],
            start: 0,
            length: 0,
            font: None,
            font_size: 12.,
            styles: LayoutStyle::NONE,
            horizontal_scale: 100,
            vertical_scale: 100,
            char_styles: 0,
            rect: CfxRectF::default(),
            skip_space: true,
        }
    }
}
impl TxtBreakRun<'_> {
    fn char_at(&self, index: usize) -> Option<u32> {
        if let Some(engine) = self.engine.as_deref() {
            Some(engine.get_char(index.checked_add(self.start.max(0) as usize)?))
        } else {
            self.text.get(index).copied()
        }
    }
    fn width_at(&mut self, index: usize) -> Option<i32> {
        if let Some(engine) = self.engine.as_deref_mut() {
            Some(engine.get_width_of_char(index.checked_add(self.start.max(0) as usize)?))
        } else {
            self.widths.get(index).copied()
        }
    }
    fn count(&self) -> usize {
        if self.engine.is_some() {
            self.length.max(0) as usize
        } else {
            (self.length.max(0) as usize)
                .min(self.text.len())
                .min(self.widths.len())
        }
    }
}
pub struct CfgasTxtBreak {
    pub base: CfgasBreak,
    pub alignment: i32,
    pub comb_width: i32,
}
impl Default for CfgasTxtBreak {
    fn default() -> Self {
        Self::new()
    }
}
impl Deref for CfgasTxtBreak {
    type Target = CfgasBreak;
    fn deref(&self) -> &CfgasBreak {
        &self.base
    }
}
impl DerefMut for CfgasTxtBreak {
    fn deref_mut(&mut self) -> &mut CfgasBreak {
        &mut self.base
    }
}
fn checked_width(width: i32, size: i32, scale: i32) -> i32 {
    width
        .checked_mul(size)
        .and_then(|v| v.checked_mul(scale))
        .map(|v| v / 100)
        .unwrap_or(0)
}
impl CfgasTxtBreak {
    pub fn new() -> Self {
        Self {
            base: CfgasBreak::default(),
            alignment: ALIGNMENT_LEFT,
            comb_width: 360_000,
        }
    }
    pub fn set_line_width(&mut self, width: f32) {
        self.line_width = scaled(width * CONVERSION_FACTOR);
    }
    pub fn set_alignment(&mut self, alignment: i32) {
        self.alignment = alignment;
    }
    pub fn set_comb_width(&mut self, width: f32) {
        self.comb_width = scaled(width * CONVERSION_FACTOR);
    }
    fn character_width(&self, ch: u32) -> i32 {
        self.font
            .as_ref()
            .and_then(|font| font.get_char_width(ch))
            .map_or(0, i32::from)
    }
    fn append_char_combination(&mut self, index: usize) {
        let mut ch = u32::from(self.cur_line().line_chars[index].char_code());
        let width = if self.comb_text {
            self.comb_width
        } else {
            // The source's index zero reads the just-appended mark itself. Use the
            // preceding mark so shadda combinations actually coalesce.
            if let Some(last) = self.get_last_char_index(1, false, false) {
                let previous = &self.cur_line().line_chars[last];
                if previous.char_styles & FX_TXTCHARSTYLE_ARABIC_SHADDA == 0 {
                    let last_code = u32::from(previous.char_code());
                    let form = if ch == 0x651 {
                        get_arabic_from_shadda_table(last_code)
                    } else if last_code == 0x651 {
                        get_arabic_from_shadda_table(ch)
                    } else {
                        None
                    };
                    if let Some(form) = form {
                        ch = form;
                        let chars = &mut self.cur_line_mut().line_chars;
                        chars[index].char_styles |= FX_TXTCHARSTYLE_ARABIC_SHADDA;
                        chars[last].char_styles |= FX_TXTCHARSTYLE_ARABIC_SHADDA;
                        chars[last].char_width = 0;
                    }
                }
            }
            checked_width(
                self.character_width(ch),
                self.font_size,
                self.horizontal_scale,
            )
        };
        self.cur_line_mut().line_chars[index].char_width = width.checked_neg().unwrap_or(0);
    }
    fn append_char_control(&mut self, index: usize) -> BreakType {
        self.char_type = CharType::Control;
        if self.single_line {
            return BreakType::None;
        }
        let ch = u32::from(self.cur_line().line_chars[index].char_code());
        let kind = match ch {
            11 | 0x2028 => BreakType::Line,
            12 => BreakType::Page,
            0x2029 => BreakType::Paragraph,
            _ if ch == self.w_paragraph_break_char => BreakType::Paragraph,
            _ => BreakType::None,
        };
        if kind == BreakType::None {
            kind
        } else {
            self.end_break(kind)
        }
    }
    fn append_char_arabic(&mut self, index: usize) -> BreakType {
        let kind = self.cur_line().line_chars[index].get_char_type();
        let mut last = None;
        let mut alef = false;
        if !self.comb_text
            && self.char_type >= CharType::ArabicAlef
            && self.char_type <= CharType::ArabicDistortion
        {
            last = self.get_last_char_index(1, true, false);
            if let Some(last_index) = last {
                let previous = self.get_last_char_index(2, true, false);
                let chars = &self.cur_line().line_chars;
                let form = get_arabic_form_char_from_chars(
                    Some(&chars[last_index]),
                    previous.map(|i| &chars[i]),
                    Some(&chars[index]),
                );
                alef = form == 0xfeff && chars[last_index].get_char_type() == CharType::ArabicAlef;
                let old = chars[last_index].char_width;
                let width = if form == 0xfeff {
                    0
                } else {
                    checked_width(
                        self.character_width(form),
                        self.font_size,
                        self.horizontal_scale,
                    )
                };
                let line = self.cur_line_mut();
                if old > 0 {
                    line.width = line.width.saturating_sub(old);
                }
                line.line_chars[last_index].char_width = width;
                line.width = line.width.saturating_add(width);
            }
        }
        self.char_type = kind;
        let chars = &self.cur_line().line_chars;
        let form = get_arabic_form_char_from_chars(
            Some(&chars[index]),
            if alef { None } else { last.map(|i| &chars[i]) },
            None,
        );
        let width = if self.comb_text {
            self.comb_width
        } else if form == 0xfeff {
            0
        } else {
            checked_width(
                self.character_width(form),
                self.font_size,
                self.horizontal_scale,
            )
        };
        let line = self.cur_line_mut();
        line.line_chars[index].char_width = width;
        line.width = line.width.saturating_add(width);
        line.increment_arabic_char_count();
        if !self.single_line && self.is_greater_than_line_width(self.cur_line().width) {
            self.end_break(BreakType::Line)
        } else {
            BreakType::None
        }
    }
    fn append_char_others(&mut self, index: usize) -> BreakType {
        let kind = self.cur_line().line_chars[index].get_char_type();
        let ch = u32::from(self.cur_line().line_chars[index].char_code());
        self.char_type = kind;
        let width = if self.comb_text {
            self.comb_width
        } else {
            checked_width(
                self.character_width(ch),
                self.font_size,
                self.horizontal_scale,
            )
        }
        .checked_add(self.char_space)
        .unwrap_or(0);
        let line = self.cur_line_mut();
        line.line_chars[index].char_width = width;
        line.width = line.width.saturating_add(width);
        if !self.single_line
            && kind != CharType::Space
            && self.is_greater_than_line_width(self.cur_line().width)
        {
            self.end_break(BreakType::Line)
        } else {
            BreakType::None
        }
    }
    pub fn append_char(&mut self, ch: u32) -> BreakType {
        let kind = get_char_type(ch);
        let mut current =
            CfgasChar::with_scales(ch as u16, self.horizontal_scale, self.vertical_scale);
        current.char_styles = self.alignment as u32 | (1 << 8);
        self.cur_line_mut().line_chars.push(current);
        let mut first = BreakType::None;
        if kind != CharType::Combination
            && self.get_unified_char_type(self.char_type) != self.get_unified_char_type(kind)
            && self.char_type != CharType::Unknown
            && !self.single_line
            && self.is_greater_than_line_width(self.cur_line().width)
            && (self.char_type != CharType::Space || kind != CharType::Control)
        {
            first = self.end_break(BreakType::Line);
        }
        let Some(index) = self.cur_line().line_chars.len().checked_sub(1) else {
            return first;
        };
        let second = if ch == self.w_paragraph_break_char {
            self.end_break(BreakType::Paragraph);
            BreakType::Paragraph
        } else {
            match kind {
                CharType::Tab => {
                    self.char_type = CharType::Tab;
                    BreakType::None
                }
                CharType::Control => self.append_char_control(index),
                CharType::Combination => {
                    self.append_char_combination(index);
                    BreakType::None
                }
                CharType::ArabicAlef
                | CharType::ArabicSpecial
                | CharType::ArabicDistortion
                | CharType::ArabicNormal
                | CharType::ArabicForm
                | CharType::Arabic => self.append_char_arabic(index),
                _ => self.append_char_others(index),
            }
        };
        first.max(second)
    }
    fn end_break_split_line(&mut self, all: bool) {
        let mut done = false;
        if !self.single_line && self.is_greater_than_line_width(self.cur_line().width) {
            if self.cur_line().last_char().is_some_and(|ch| {
                !matches!(
                    ch.get_char_type(),
                    CharType::Tab | CharType::Control | CharType::Space
                )
            }) {
                self.split_text_line(all);
                done = true;
            }
        }
        if all && !done {
            let mut end = self.cur_line().width;
            let single = self.single_line;
            let width = self.line_width;
            Self::get_break_pos_impl(
                &mut self.cur_line_mut().line_chars,
                all,
                true,
                &mut end,
                single,
                width,
            );
        }
    }
    fn end_break_bidi_line(&mut self, status: BreakType) -> Vec<Tpo> {
        let mut positions = Vec::new();
        let bidi = self.cur_line().has_arabic_char()
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
            });
        if !bidi {
            let line = self.cur_line_mut();
            let first = &line.line_chars[0];
            line.line_pieces.push(CfgasBreakPiece {
                status,
                start_pos: line.start,
                width: line.width,
                char_count: line.line_chars.len() as i32,
                char_styles: first.char_styles,
                horizontal_scale: first.horizontal_scale(),
                vertical_scale: first.vertical_scale(),
                ..Default::default()
            });
            positions.push(Tpo { index: 0, pos: 0 });
            return positions;
        }
        let line = self.cur_line_mut();
        let mut count = 0;
        for (i, ch) in line.line_chars.iter_mut().enumerate() {
            ch.bidi_pos = i as u16;
            if ch.get_char_type() != CharType::Control {
                count = i;
            }
            if i == 0 {
                ch.bidi_level = 1;
            }
        }
        CfgasChar::bidi_line(&mut line.line_chars, count + 1);
        // BidiLine returns visual order, while the edit engine reads logical
        // text and matching widths. Keep the visual indices on each character
        // and restore the original order before grouping directional pieces.
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
        let mut piece = CfgasBreakPiece {
            start_pos: line.start,
            ..Default::default()
        };
        let mut level = -1;
        let mut i = 0;
        while i < line.line_chars.len() {
            let ch = &line.line_chars[i];
            if level < 0 {
                level = i32::from(ch.bidi_level);
                piece.width = 0;
                piece.bidi_level = level;
                piece.bidi_pos = i32::from(ch.bidi_order);
                piece.char_styles = ch.char_styles;
                piece.horizontal_scale = ch.horizontal_scale();
                piece.vertical_scale = ch.vertical_scale();
                piece.status = BreakType::Piece;
            }
            if level != i32::from(ch.bidi_level) || ch.status != BreakType::None {
                if level == i32::from(ch.bidi_level) {
                    piece.status = ch.status;
                    if ch.char_width > 0 {
                        piece.width = piece.width.saturating_add(ch.char_width);
                    }
                    i += 1;
                }
                piece.char_count = i as i32 - piece.start_char;
                let index = line.line_pieces.len();
                line.line_pieces.push(piece.clone());
                positions.push(Tpo {
                    index,
                    pos: piece.bidi_pos,
                });
                piece.start_pos = piece.start_pos.saturating_add(piece.width);
                piece.start_char = i as i32;
                level = -1;
            } else {
                if ch.char_width > 0 {
                    piece.width = piece.width.saturating_add(ch.char_width);
                }
                i += 1;
            }
        }
        if i as i32 > piece.start_char {
            piece.status = status;
            piece.char_count = i as i32 - piece.start_char;
            positions.push(Tpo {
                index: line.line_pieces.len(),
                pos: piece.bidi_pos,
            });
            line.line_pieces.push(piece);
        }
        if positions.len() > 1 {
            positions.sort_by_key(|p| p.pos);
            let mut start = 0i32;
            for pos in &positions {
                let piece = &mut line.line_pieces[pos.index];
                piece.start_pos = start;
                start = start.saturating_add(piece.width);
            }
        }
        if let Some(last) = line.line_pieces.last_mut() {
            last.status = status;
        }
        positions
    }
    fn end_break_alignment(&mut self, positions: &[Tpo], all: bool, status: BreakType) {
        let line = self.cur_line();
        let mut net = line.width;
        let mut gaps = 0;
        let mut found = false;
        for pos in positions.iter().rev() {
            let piece = &line.line_pieces[pos.index];
            if !found {
                net = piece.get_end_pos();
            }
            let rtl = piece.bidi_level & 1 != 0;
            let mut j = if rtl { 0 } else { piece.char_count - 1 };
            while j >= 0 && j < piece.char_count {
                let Some(ch) = piece.get_char(&line.line_chars, j as usize) else {
                    break;
                };
                if ch.line_break_type == LineBreakType::DirectBreak && ch.char_width >= 0 {
                    gaps += 1;
                }
                if !found || !all {
                    if matches!(ch.get_char_type(), CharType::Space | CharType::Control) {
                        if !found && all && ch.char_width > 0 {
                            net = net.saturating_sub(ch.char_width);
                        }
                    } else {
                        found = true;
                        if !all {
                            break;
                        }
                    }
                }
                j += if rtl { 1 } else { -1 };
            }
            if !all && found {
                break;
            }
        }
        let mut offset = self.line_width.saturating_sub(net);
        let alignment = self.alignment;
        if gaps > 0 && alignment & ALIGNMENT_JUSTIFIED != 0 && status != BreakType::Paragraph {
            let line = self.cur_line_mut();
            let mut start = None;
            for pos in positions {
                let piece = &mut line.line_pieces[pos.index];
                let start_pos = *start.get_or_insert(piece.start_pos);
                piece.start_pos = start_pos;
                for j in 0..piece.char_count.max(0) as usize {
                    if gaps == 0 {
                        break;
                    }
                    let Some(ch) = line
                        .line_chars
                        .get_mut(piece.start_char.max(0) as usize + j)
                    else {
                        break;
                    };
                    if ch.line_break_type != LineBreakType::DirectBreak || ch.char_width < 0 {
                        continue;
                    }
                    let increment = offset / gaps;
                    ch.char_width = ch.char_width.saturating_add(increment);
                    piece.width = piece.width.saturating_add(increment);
                    offset -= increment;
                    gaps -= 1;
                }
                start = Some(start_pos.saturating_add(piece.width));
            }
        } else if alignment & (ALIGNMENT_CENTER | ALIGNMENT_RIGHT) != 0 {
            if alignment & ALIGNMENT_CENTER != 0 && alignment & ALIGNMENT_RIGHT == 0 {
                offset /= 2;
            }
            if offset > 0 {
                for piece in &mut self.cur_line_mut().line_pieces {
                    piece.start_pos = piece.start_pos.saturating_add(offset);
                }
            }
        }
    }
    pub fn end_break(&mut self, status: BreakType) -> BreakType {
        if status == BreakType::None {
            return BreakType::None;
        }
        if let Some(piece) = self.cur_line_mut().line_pieces.last_mut() {
            if status != BreakType::Piece {
                piece.status = status;
            }
            return piece.status;
        }
        if self.has_line() {
            let ready = self.ready_line_index as usize;
            let line = &mut self.lines[ready];
            if let Some(piece) = line.line_pieces.last_mut() {
                if status != BreakType::Piece {
                    piece.status = status;
                }
                return piece.status;
            }
            return BreakType::None;
        }
        if self.cur_line().line_chars.is_empty() {
            return BreakType::None;
        }
        self.cur_line_mut().line_chars.last_mut().unwrap().status = status;
        if status == BreakType::Piece {
            return status;
        }
        self.ready_line_index = self.current_line as i8;
        let next = 1 - self.current_line;
        let all = self.alignment > ALIGNMENT_RIGHT;
        self.end_break_split_line(all);
        let positions = self.end_break_bidi_line(status);
        if self.alignment > ALIGNMENT_LEFT {
            self.end_break_alignment(&positions, all, status);
        }
        self.current_line = next;
        self.char_type = self
            .get_last_char(0, false, false)
            .map_or(CharType::Unknown, CfgasChar::get_char_type);
        status
    }
    fn get_break_pos_impl(
        chars: &mut [CfgasChar],
        all: bool,
        only: bool,
        end: &mut i32,
        single: bool,
        line_width: i32,
    ) -> i32 {
        let mut length = chars.len() as i32 - 1;
        if length < 1 {
            return length;
        }
        let (mut direct, mut direct_pos, mut indirect, mut indirect_pos, mut last, mut last_pos) =
            (-1, -1, -1, -1, -1, -1);
        if single || *end <= line_width {
            if !all {
                return length;
            }
            direct = length;
            direct_pos = *end;
        }
        let current = &mut chars[length as usize];
        if all {
            current.line_break_type = LineBreakType::Unknown;
        }
        let mut next = get_break_property(u32::from(current.char_code()));
        if current.char_width > 0 {
            *end = end.saturating_sub(current.char_width);
        }
        length -= 1;
        while length >= 0 {
            let current = &mut chars[length as usize];
            let property = get_break_property(u32::from(current.char_code()));
            let kind = if next == BreakProperty::SP {
                LineBreakType::ProhibitedBreak
            } else {
                get_line_break_type_from_pair(property, next)
            };
            if all {
                current.line_break_type = kind;
            }
            if !only {
                if single || *end <= line_width || property == BreakProperty::SP {
                    if kind == LineBreakType::DirectBreak && direct < 0 {
                        direct = length;
                        direct_pos = *end;
                        if !all {
                            return length;
                        }
                    } else if kind == LineBreakType::IndirectBreak && indirect < 0 {
                        indirect = length;
                        indirect_pos = *end;
                    }
                    if last < 0 {
                        last = length;
                        last_pos = *end;
                    }
                }
                if current.char_width > 0 {
                    *end = end.saturating_sub(current.char_width);
                }
            }
            next = property;
            length -= 1;
        }
        if only {
            return 0;
        }
        if direct >= 0 {
            *end = direct_pos;
            direct
        } else if indirect >= 0 {
            *end = indirect_pos;
            indirect
        } else if last >= 0 {
            *end = last_pos;
            last
        } else {
            0
        }
    }
    fn split_text_line(&mut self, all: bool) {
        if self.cur_line().line_chars.len() < 2 {
            return;
        }
        let mut end = self.cur_line().width;
        let single = self.single_line;
        let width = self.line_width;
        let position = Self::get_break_pos_impl(
            &mut self.cur_line_mut().line_chars,
            all,
            false,
            &mut end,
            single,
            width,
        )
        .max(0) as usize
            + 1;
        let (current, next) = self.line_pair_mut();
        if position >= current.line_chars.len() {
            next.clear();
            current.line_chars[position - 1].line_break_type = LineBreakType::Unknown;
            return;
        }
        next.clear();
        next.line_chars = current.line_chars.split_off(position);
        current.width = end;
        current.line_chars[position - 1].line_break_type = LineBreakType::Unknown;
        for ch in &mut next.line_chars {
            if ch.get_char_type() >= CharType::ArabicAlef {
                current.decrement_arabic_char_count();
                next.arabic_chars += 1;
            }
            next.width = next.width.saturating_add(ch.char_width.max(0));
            ch.status = BreakType::None;
        }
    }
    /// An empty destination counts required glyph positions, just as the source
    /// span API. A short destination receives its safe prefix and the full count.
    pub fn get_display_pos(
        &self,
        run: &mut TxtBreakRun<'_>,
        positions: &mut [TextCharPos],
    ) -> usize {
        let length = run.count();
        if length == 0 {
            return 0;
        }
        let Some(font) = run.font.clone() else {
            return 0;
        };
        let font_size = run.font_size;
        let integer_size = scaled(font_size * 20.);
        if integer_size <= 0 || !font_size.is_finite() {
            return 0;
        }
        let ascent = font.get_ascent();
        let maximum = ascent.saturating_sub(font.get_descent());
        let ascent_offset = if maximum != 0 {
            font_size * ascent as f32 / maximum as f32
        } else {
            0.
        };
        let rtl = run.char_styles & FX_TXTCHARSTYLE_ODD_BIDI_LEVEL != 0;
        let y_base = run.rect.top + (run.rect.height - font_size) / 2.;
        let mut x = if rtl { run.rect.right() } else { run.rect.left };
        let y = y_base + ascent_offset;
        let mut count = 0usize;
        let mut written = 0usize;
        let mut previous = 0xfeff;
        let mut last = 0xfeff;
        let mut shadda = false;
        let mut lam = false;
        for i in 0..length {
            let Some(mut ch) = run.char_at(i) else {
                break;
            };
            let Some(width) = run.width_at(i) else {
                break;
            };
            let mut kind = get_char_type(ch);
            if kind == CharType::ArabicAlef && width == 0 {
                previous = 0xfeff;
                last = ch;
                continue;
            }
            let mut next = 0xfeff;
            let form = if kind >= CharType::ArabicAlef {
                for index in i + 1..length {
                    let Some(candidate) = run.char_at(index) else {
                        break;
                    };
                    if get_char_type(candidate) != CharType::Combination {
                        next = candidate;
                        break;
                    }
                }
                lam = previous == 0x644 && ch == 0x644 && next == 0x647;
                get_arabic_form_char(ch, previous, next)
            } else if kind == CharType::Combination {
                let mut form = ch;
                if (0x64c..=0x651).contains(&ch) {
                    if shadda {
                        form = 0xfeff;
                        shadda = false;
                    } else {
                        next = run.char_at(i + 1).unwrap_or(0xfeff);
                        let combined = if ch == 0x651 {
                            get_arabic_from_shadda_table(next)
                        } else if next == 0x651 {
                            get_arabic_from_shadda_table(ch)
                        } else {
                            None
                        };
                        if let Some(combined) = combined {
                            form = combined;
                            shadda = true;
                        }
                    }
                } else {
                    shadda = false;
                }
                form
            } else if kind == CharType::Numeric || ch == u32::from(b'.') || ch == u32::from(b',') {
                ch
            } else if rtl {
                get_mirror_char(ch)
            } else {
                ch
            };
            if kind != CharType::Combination {
                shadda = false;
            }
            if kind < CharType::ArabicAlef {
                lam = false;
            }
            let empty = (kind >= CharType::Tab && kind <= CharType::Control) || form == 0xfeff;
            let forms = if lam { 3 } else { 1 };
            let emit = !empty || !run.skip_space;
            if emit {
                count = count.saturating_add(forms);
            }
            if positions.is_empty() {
                if width > 0 {
                    previous = ch;
                }
                last = ch;
                continue;
            }
            let normalized = width.checked_abs().unwrap_or(0) / integer_size;
            let mut form_chars = [(ch, form, normalized), (0x651, 0x651, 0), (0x670, 0x670, 0)];
            if lam {
                form_chars[1].2 = i32::from(font.get_char_width(0x651).unwrap_or(0));
                form_chars[2].2 = i32::from(font.get_char_width(0x670).unwrap_or(0));
            }
            for j in 0..forms {
                let (_, form, char_width) = form_chars[j];
                if j > 0 {
                    kind = CharType::Combination;
                    ch = form;
                    last = form_chars[j - 1].1;
                }
                let advance = font_size * char_width as f32 / 1000.;
                if rtl && kind != CharType::Combination {
                    x -= advance;
                }
                if emit {
                    if let Some(position) = positions.get_mut(written) {
                        position.glyph_index = font.get_glyph_index(form) as u32;
                        position.font_char_width = char_width;
                        position.origin = CfxPointF::new(x, y);
                        if run.styles.contains(LayoutStyle::COMB_TEXT) {
                            let form_width = font
                                .get_char_width(form)
                                .map(i32::from)
                                .unwrap_or(char_width);
                            position.origin.x +=
                                font_size * (char_width - form_width) as f32 / 2000.;
                        }
                        if kind == CharType::Combination && maximum != 0 {
                            if let Some(bbox) = font.get_char_bbox(form) {
                                position.origin.y = y_base + font_size
                                    - font_size * bbox.height() as f32 / maximum as f32;
                            }
                            if form == ch
                                && last != 0xfeff
                                && get_char_type(last) == CharType::Combination
                            {
                                if let Some(bbox) = font.get_char_bbox(last) {
                                    position.origin.y -=
                                        font_size * bbox.height() as f32 / maximum as f32;
                                }
                            }
                        }
                        position.glyph_adjust = true;
                        position.adjust_matrix = [
                            -(run.horizontal_scale as f32) / 100.,
                            0.,
                            0.,
                            run.vertical_scale as f32 / 100.,
                        ];
                    }
                    written += 1;
                }
                if !rtl && kind != CharType::Combination {
                    x += advance;
                }
            }
            if width > 0 {
                previous = form_chars[0].0;
            }
            last = ch;
        }
        count
    }
    pub fn get_char_rects(&self, run: &mut TxtBreakRun<'_>) -> Vec<CfxRectF> {
        let length = run.count();
        let mut rect = run.rect;
        let rtl = run.char_styles & FX_TXTCHARSTYLE_ODD_BIDI_LEVEL != 0;
        let single = run.styles.contains(LayoutStyle::SINGLE_LINE);
        let mut start = if rtl { rect.right() } else { rect.left };
        let mut result = Vec::with_capacity(length);
        for i in 0..length {
            let Some(ch) = run.char_at(i) else {
                break;
            };
            let Some(width) = run.width_at(i) else {
                break;
            };
            let mut size = width as f32 / CONVERSION_FACTOR;
            let control = matches!(get_char_type(ch), CharType::Tab | CharType::Control);
            if !single && control && matches!(ch, 11 | 12 | 0x2028 | 0x2029 | 10) {
                size = run.font_size / 2.;
            }
            rect.left = start;
            if rtl {
                rect.left -= size;
                start -= size;
            } else {
                start += size;
            }
            rect.width = size;
            result.push(rect);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn font() -> Rc<CfgasGeFont> {
        // Native fixtures assume host-installed Arimo Bold. A bundled metric
        // font keeps these text/break assertions independent of host fonts.
        CfgasGeFont::from_font(
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
        .unwrap()
    }

    // cfgas_txtbreak_unittest.cpp: complete BidiLine case, including embedded NUL.
    #[test]
    fn hebrew_breaks_keep_logical_widths_and_directional_visual_order() {
        for (logical, visual) in [
            ("בני", "ינב"),
            ("a בנ 12 b", "a 12 נב b"),
            ("\u{202a}AB\u{202c}", "\u{202a}AB\u{202c}"),
        ] {
            let mut breaker = CfgasTxtBreak::new();
            breaker.set_font(font());
            breaker.set_font_size(12.);
            breaker.set_line_width(200.);
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
    fn bidi_line() {
        let mut breaker = CfgasTxtBreak::new();
        breaker.set_font(font());
        breaker.set_line_break_tolerance(1.);
        breaker.set_font_size(12.);
        for ch in [10, 0, 10, 10] {
            breaker.append_char(ch);
        }
        let mut chars = breaker.get_current_line_for_testing().line_chars.clone();
        let count = chars.len();
        CfgasChar::bidi_line(&mut chars, count);
        assert_eq!(chars.len(), 3);
    }
    #[test]
    fn widths_split_ready_lifetimes_and_alignment() {
        let mut breaker = CfgasTxtBreak::new();
        breaker.set_font(font());
        breaker.set_font_size(12.);
        breaker.set_line_width(30.);
        let mut text = Vec::new();
        for ch in "alpha beta gamma".chars().map(u32::from) {
            if breaker.append_char(ch) >= BreakType::Line {
                for i in 0..breaker.count_break_pieces() {
                    let (piece, chars) = breaker.get_break_piece(i).unwrap();
                    text.extend(piece.get_string(chars));
                    assert!(piece.width >= 0);
                }
                breaker.clear_break_pieces();
            }
        }
        breaker.end_break(BreakType::Paragraph);
        for i in 0..breaker.count_break_pieces() {
            let (piece, chars) = breaker.get_break_piece(i).unwrap();
            text.extend(piece.get_string(chars));
        }
        assert_eq!(
            text,
            "alpha beta gamma"
                .chars()
                .map(u32::from)
                .collect::<Vec<_>>()
        );
        breaker.clear_break_pieces();
        assert_eq!(breaker.count_break_pieces(), 0);
        breaker.reset();
        breaker.set_line_width(100.);
        breaker.set_alignment(ALIGNMENT_RIGHT);
        breaker.append_char(u32::from(b'A'));
        breaker.end_break(BreakType::Paragraph);
        let (piece, _) = breaker.get_break_piece(0).unwrap();
        assert_eq!(piece.get_end_pos(), 2_000_000);
    }
    #[test]
    fn display_positions_count_rectangles_and_engine_adapter() {
        let font = font();
        let text = "A B".chars().map(u32::from).collect::<Vec<_>>();
        let widths = text
            .iter()
            .map(|&ch| i32::from(font.get_char_width(ch).unwrap()) * 240)
            .collect::<Vec<_>>();
        let breaker = CfgasTxtBreak::new();
        let mut run = TxtBreakRun {
            text: &text,
            widths: &widths,
            length: 3,
            font: Some(font),
            rect: CfxRectF::new(10., 20., 100., 12.),
            ..Default::default()
        };
        assert_eq!(breaker.get_display_pos(&mut run, &mut []), 2);
        let mut positions = [TextCharPos::default(); 2];
        assert_eq!(breaker.get_display_pos(&mut run, &mut positions), 2);
        assert!(positions[1].origin.x > positions[0].origin.x);
        assert_eq!(positions[0].adjust_matrix, [-1., 0., 0., 1.]);
        let rectangles = breaker.get_char_rects(&mut run);
        assert_eq!(rectangles.len(), 3);
        assert_eq!(rectangles[0].left, 10.);
        assert_eq!(rectangles[1].left, rectangles[0].right());
        struct Engine {
            chars: Vec<u32>,
            widths: Vec<i32>,
        }
        impl TxtBreakEngine for Engine {
            fn get_char(&self, i: usize) -> u32 {
                self.chars[i]
            }
            fn get_width_of_char(&mut self, i: usize) -> i32 {
                self.widths[i]
            }
        }
        let mut engine = Engine {
            chars: text.clone(),
            widths: widths.clone(),
        };
        let expected = rectangles;
        let mut engine_run = TxtBreakRun {
            engine: Some(&mut engine),
            length: 3,
            rect: run.rect,
            ..Default::default()
        };
        assert_eq!(breaker.get_char_rects(&mut engine_run), expected);
    }
    #[test]
    fn reset_discards_ready_output_and_accepts_new_text() {
        let mut breaker = CfgasTxtBreak::new();
        breaker.set_font(font());
        breaker.append_char(u32::from(b'A'));
        breaker.end_break(BreakType::Paragraph);
        assert_eq!(breaker.count_break_pieces(), 1);
        breaker.reset();
        assert_eq!(breaker.count_break_pieces(), 0);
        breaker.append_char(u32::from(b'B'));
        breaker.end_break(BreakType::Paragraph);
        let (piece, chars) = breaker.get_break_piece(0).unwrap();
        assert_eq!(piece.get_string(chars), vec![u32::from(b'B')]);
    }
    #[test]
    fn comb_scaling_and_checked_widths() {
        let mut breaker = CfgasTxtBreak::new();
        breaker.set_font(font());
        breaker.set_layout_styles(LayoutStyle::COMB_TEXT);
        breaker.set_comb_width(10.);
        breaker.append_char(u32::from(b'A'));
        breaker.append_char(u32::from(b'B'));
        assert_eq!(breaker.cur_line().width, 400_000);
        assert_eq!(checked_width(i32::MAX, 240, 100), 0);
        breaker.set_tab_width(0.);
        assert_eq!(breaker.get_tab_width(), 160_000);
        breaker.set_horizontal_scale(-1);
        assert_eq!(breaker.horizontal_scale, 0);
    }
}
