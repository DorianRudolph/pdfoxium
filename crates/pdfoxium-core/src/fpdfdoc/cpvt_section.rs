// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Layout and word traversal from `core/fpdfdoc/cpvt_section.{h,cpp}`.
//! Indexed
//! storage and explicit Layout borrows replace the native backpointer.

use super::cpvt_floatrect::CpvtFloatRect;
use super::cpvt_lineinfo::CpvtLineInfo;
use super::cpvt_variabletext::{max, min, Alignment, Layout};
use super::cpvt_wordinfo::CpvtWordInfo;
use super::cpvt_wordplace::CpvtWordPlace;
use super::cpvt_wordrange::CpvtWordRange;
use crate::fxcrt::cfx_bidi_resolver::{CfxBidiResolver, ResolvedRun};

const SPECIAL_CHARS: [u8; 128] = [
    0x00, 0x0c, 0x08, 0x0c, 0x08, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x08, 0x08, 0x00, 0x10, 0x00, 0x00, 0x28, 0x0c, 0x08, 0x00, 0x00, 0x28, 0x28, 0x28, 0x28,
    0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x08, 0x08, 0x00, 0x00, 0x00, 0x08,
    0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x0c, 0x00, 0x08, 0x00, 0x00,
    0x00, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x0c, 0x00, 0x08, 0x00, 0x00,
];
fn is_latin(word: u16) -> bool {
    if word <= 0x7f {
        return SPECIAL_CHARS[word as usize] & 1 != 0;
    }
    matches!(word, 0x00c0..=0x00ff | 0x0100..=0x024f | 0x1e00..=0x1eff | 0x2c60..=0x2c7f | 0xa720..=0xa7ff | 0xff21..=0xff3a | 0xff41..=0xff5a)
}
fn is_digit(word: u32) -> bool {
    (0x30..=0x39).contains(&word)
}
fn is_cjk(word: u32) -> bool {
    if matches!(word, 0x1100..=0x11ff | 0x2e80..=0x2fff | 0x3040..=0x9fbf | 0xac00..=0xd7af | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0x20000..=0x2a6df | 0x2f800..=0x2fa1f)
    {
        return true;
    }
    if (0x3000..=0x303f).contains(&word) {
        return matches!(word, 0x3005 | 0x3006 | 0x3021..=0x3029 | 0x3031..=0x3035);
    }
    (0xff66..=0xff9d).contains(&word)
}
fn is_punctuation(word: u32) -> bool {
    if word <= 0x7f {
        return SPECIAL_CHARS[word as usize] & 8 != 0;
    }
    if (0x80..=0xff).contains(&word) {
        return matches!(
            word,
            0x82 | 0x84 | 0x85 | 0x91 | 0x92 | 0x93 | 0x94 | 0x96 | 0xb4 | 0xb8
        );
    }
    if (0x2000..=0x206f).contains(&word) {
        return matches!(word, 0x2010..=0x2013 | 0x2018..=0x201f | 0x2032..=0x2037 | 0x203c..=0x203e | 0x2044);
    }
    if (0x3000..=0x303f).contains(&word) {
        return matches!(word, 0x3001..=0x3003 | 0x3005 | 0x3009..=0x3011 | 0x3014..=0x301b | 0x301d..=0x301f);
    }
    if (0xfe50..=0xfe6f).contains(&word) {
        return (0xfe50..=0xfe5e).contains(&word) || word == 0xfe63;
    }
    if (0xff00..=0xffef).contains(&word) {
        return matches!(word, 0xff01 | 0xff02 | 0xff07..=0xff09 | 0xff0c | 0xff0e | 0xff0f | 0xff1a | 0xff1b | 0xff1f | 0xff3b | 0xff3d | 0xff40 | 0xff5b..=0xff5d | 0xff61..=0xff65 | 0xff9e | 0xff9f);
    }
    false
}
fn is_connective_symbol(word: u32) -> bool {
    word <= 0x7f && SPECIAL_CHARS[word as usize] & 0x20 != 0
}
fn is_open_style_punctuation(word: u32) -> bool {
    if word <= 0x7f {
        return SPECIAL_CHARS[word as usize] & 4 != 0;
    }
    matches!(
        word,
        0x300a
            | 0x300c
            | 0x300e
            | 0x3010
            | 0x3014
            | 0x3016
            | 0x3018
            | 0x301a
            | 0xff08
            | 0xff3b
            | 0xff5b
            | 0xff62
    )
}
fn is_currency_symbol(word: u16) -> bool {
    matches!(word, 0x24 | 0x80 | 0xa2..=0xa5 | 0x20a0..=0x20cf | 0xfe69 | 0xff04 | 0xffe0 | 0xffe1 | 0xffe5 | 0xffe6)
}
fn is_prefix_symbol(word: u16) -> bool {
    is_currency_symbol(word) || word == 0x2116
}
fn is_space(word: u16) -> bool {
    word == 0x20 || word == 0x3000
}
fn need_division(previous: u16, current: u16) -> bool {
    if (is_latin(previous) || is_digit(previous.into()))
        && (is_latin(current) || is_digit(current.into()))
    {
        return false;
    }
    if is_space(current) || is_punctuation(current.into()) {
        return false;
    }
    if is_connective_symbol(previous.into()) || is_connective_symbol(current.into()) {
        return false;
    }
    if is_space(previous) || is_punctuation(previous.into()) {
        return true;
    }
    if is_prefix_symbol(previous) {
        return false;
    }
    if is_prefix_symbol(current) || is_cjk(current.into()) {
        return true;
    }
    is_cjk(previous.into())
}

pub(super) struct Line {
    pub place: CpvtWordPlace,
    pub info: CpvtLineInfo,
}
impl Line {
    pub fn begin_word_place(&self) -> CpvtWordPlace {
        CpvtWordPlace::new(self.place.sec_index, self.place.line_index, -1)
    }
    pub fn end_word_place(&self) -> CpvtWordPlace {
        CpvtWordPlace::new(
            self.place.sec_index,
            self.place.line_index,
            self.info.end_word_index,
        )
    }
    fn next_word_place(&self, place: CpvtWordPlace) -> CpvtWordPlace {
        CpvtWordPlace::new(
            place.sec_index,
            place.line_index,
            if place.word_index < self.info.begin_word_index {
                self.info.begin_word_index
            } else {
                place.word_index + 1
            },
        )
    }
}

#[derive(Default)]
pub(super) struct CpvtSection {
    pub sec_place: CpvtWordPlace,
    pub rect: CpvtFloatRect,
    pub line_array: Vec<Line>,
    pub word_array: Vec<CpvtWordInfo>,
}
impl CpvtSection {
    fn erase_word_range(&mut self, begin: i32, end: i32) {
        // Native GetWordRangeIteratorPair clamps the end before the beginning.
        let end = end.clamp(0, self.word_array.len() as i32);
        let begin = begin.clamp(0, end);
        self.word_array.drain(begin as usize..end as usize);
    }
    pub fn clear_words(&mut self, range: CpvtWordRange) {
        if range.begin >= self.begin_word_place() {
            if range.end <= self.end_word_place() {
                self.erase_word_range(
                    range.begin.word_index.saturating_add(1),
                    range.end.word_index.saturating_add(1),
                );
            } else {
                self.erase_word_range(
                    range.begin.word_index.saturating_add(1),
                    self.word_array.len() as i32,
                );
            }
        } else if range.end <= self.end_word_place() {
            self.erase_word_range(0, range.end.word_index.saturating_add(1));
        } else {
            self.word_array.clear();
        }
    }
    pub fn reset_line_place(&mut self) {
        for (i, line) in self.line_array.iter_mut().enumerate() {
            line.place = CpvtWordPlace::new(self.sec_place.sec_index, i as i32, -1);
        }
    }
    pub fn add_word(&mut self, place: CpvtWordPlace, word: CpvtWordInfo) -> CpvtWordPlace {
        let index = place.word_index.clamp(0, self.word_array.len() as i32);
        self.word_array.insert(index as usize, word);
        place
    }
    pub fn add_line(&mut self, info: CpvtLineInfo) -> CpvtWordPlace {
        self.line_array.push(Line {
            place: CpvtWordPlace::default(),
            info,
        });
        CpvtWordPlace::new(
            self.sec_place.sec_index,
            self.line_array.len() as i32 - 1,
            -1,
        )
    }
    pub fn begin_word_place(&self) -> CpvtWordPlace {
        self.line_array
            .first()
            .map_or(self.sec_place, Line::begin_word_place)
    }
    pub fn end_word_place(&self) -> CpvtWordPlace {
        self.line_array
            .last()
            .map_or(self.sec_place, Line::end_word_place)
    }
    pub fn next_word_place(&self, place: CpvtWordPlace) -> CpvtWordPlace {
        if place.line_index < 0 {
            return self.begin_word_place();
        }
        let Some(line) = self.line_array.get(place.line_index as usize) else {
            return self.end_word_place();
        };
        if place.word_index < line.info.end_word_index {
            return line.next_word_place(place);
        }
        self.line_array
            .get(place.line_index as usize + 1)
            .map_or(place, Line::begin_word_place)
    }
    pub fn update_word_place(&self, place: &mut CpvtWordPlace) {
        let (mut left, mut right) = (0, self.line_array.len() as i32 - 1);
        let mut middle = (left + right) / 2;
        while left <= right {
            let line = &self.line_array[middle as usize];
            if place.word_index < line.info.begin_word_index {
                right = middle - 1;
                middle = (left + right) / 2;
            } else if place.word_index > line.info.end_word_index {
                left = middle + 1;
                middle = (left + right) / 2;
            } else {
                place.line_index = middle;
                return;
            }
        }
    }
    pub fn rearrange(&mut self, layout: &Layout<'_>) -> CpvtFloatRect {
        if layout.settings.char_array > 0 {
            return self.rearrange_char_array(layout);
        }
        self.line_array.clear();
        let rect = self.split_lines(layout, true, 0.0);
        self.output_lines(layout, rect)
    }
    fn rearrange_char_array(&mut self, layout: &Layout<'_>) -> CpvtFloatRect {
        let Some(line) = self.line_array.first_mut() else {
            return CpvtFloatRect::default();
        };
        let chars = layout.settings.char_array;
        let node_width = layout.plate_width() / (if chars <= 0 { 1 } else { chars }) as f32;
        let mut ascent = layout.font_ascent(layout.default_font_index(), layout.settings.font_size);
        let mut descent =
            layout.font_descent(layout.default_font_index(), layout.settings.font_size);
        let mut x = 0.0;
        let y = layout.settings.line_leading + ascent;
        let start = match layout.settings.alignment {
            Alignment::Left => {
                line.info.line_x = node_width * 0.5;
                0
            }
            Alignment::Center => {
                let start = (chars - self.word_array.len() as i32) / 2;
                line.info.line_x = node_width * start as f32 - node_width * 0.5;
                start
            }
            Alignment::Right => {
                let start = chars - self.word_array.len() as i32;
                line.info.line_x = node_width * start as f32 - node_width * 0.5;
                start
            }
        };
        for i in 0..self.word_array.len() {
            if i as i32 >= chars {
                break;
            }
            let mut next_width = 0.0;
            if let Some(next) = self.word_array.get_mut(i + 1) {
                next.word_tail = 0.0;
                next_width = layout.word_width(next);
            }
            let word_count = self.word_array.len();
            let word = &mut self.word_array[i];
            word.word_tail = 0.0;
            let width = layout.word_width(word);
            let word_ascent = layout.word_ascent(word);
            let word_descent = layout.word_descent(word);
            // The native 0.5 literal promotes this expression to double.
            x = (f64::from(node_width) * (f64::from(i as i32 + start) + 0.5)
                - f64::from(width * 0.5)) as f32;
            word.word_x = x;
            word.word_y = y;
            if i == 0 {
                line.info.line_x = x;
            }
            word.word_tail = if i != word_count - 1 {
                let tail = node_width - (width + next_width) * 0.5;
                if tail > 0.0 {
                    tail
                } else {
                    0.0
                }
            } else {
                0.0
            };
            x += width;
            ascent = max(ascent, word_ascent);
            descent = min(descent, word_descent);
        }
        line.info.begin_word_index = 0;
        line.info.end_word_index = self.word_array.len() as i32 - 1;
        line.info.line_y = y;
        line.info.line_width = x - line.info.line_x;
        line.info.line_ascent = ascent;
        line.info.line_descent = descent;
        CpvtFloatRect::new(0.0, 0.0, x, y - descent)
    }
    pub fn split_lines(
        &mut self,
        layout: &Layout<'_>,
        typeset: bool,
        font_size: f32,
    ) -> CpvtFloatRect {
        let mut line = CpvtLineInfo::default();
        if self.word_array.is_empty() {
            let (ascent, descent) = if typeset {
                let ascent =
                    layout.font_ascent(layout.default_font_index(), layout.settings.font_size);
                let descent =
                    layout.font_descent(layout.default_font_index(), layout.settings.font_size);
                line.line_ascent = ascent;
                line.line_descent = descent;
                self.add_line(line);
                (ascent, descent)
            } else {
                (
                    layout.font_ascent(layout.default_font_index(), font_size),
                    layout.font_descent(layout.default_font_index(), font_size),
                )
            };
            return CpvtFloatRect::new(
                0.0,
                0.0,
                0.0,
                layout.settings.line_leading + ascent - descent,
            );
        }
        let mut line_head = 0;
        let (mut max_x, mut max_y) = (0.0, 0.0);
        let (mut line_width, mut backup_width) = (0.0, 0.0);
        let (mut ascent, mut backup_ascent) = (0.0, 0.0);
        let (mut descent, mut backup_descent) = (0.0, 0.0);
        let mut word_start = 0;
        let mut full_word = false;
        let mut full_word_index = 0;
        let mut char_index = 0;
        let typeset_width = max(layout.plate_width() - layout.line_indent(), 0.0);
        let mut opened = false;
        let mut i = 0;
        while i < self.word_array.len() {
            let word = &self.word_array[i];
            let old_word = &self.word_array[i.saturating_sub(1)];
            let size = if typeset {
                layout.settings.font_size
            } else {
                font_size
            };
            ascent = max(ascent, layout.word_ascent_at(word, size));
            descent = min(descent, layout.word_descent_at(word, size));
            let word_width = layout.word_width_at(word, size);
            if !opened {
                if is_open_style_punctuation(word.word.into()) {
                    opened = true;
                    full_word = true;
                } else if need_division(old_word.word, word.word) {
                    full_word = true;
                }
            } else if !is_space(word.word) && !is_open_style_punctuation(word.word.into()) {
                opened = false;
            }
            if full_word {
                full_word = false;
                if char_index > 0 {
                    full_word_index += 1;
                }
                word_start = i;
                backup_width = line_width;
                backup_ascent = ascent;
                backup_descent = descent;
            }
            char_index += 1;
            if layout.settings.limit_width
                && typeset_width > 0.0
                && line_width + word_width > typeset_width
            {
                if full_word_index > 0 {
                    i = word_start;
                    line_width = backup_width;
                    ascent = backup_ascent;
                    descent = backup_descent;
                }
                if char_index == 1 {
                    line_width = word_width;
                    i += 1;
                }
                if typeset {
                    line.begin_word_index = line_head as i32;
                    line.end_word_index = i as i32 - 1;
                    line.line_width = line_width;
                    line.line_ascent = ascent;
                    line.line_descent = descent;
                    self.add_line(line);
                }
                max_y += ascent + layout.settings.line_leading;
                max_y -= descent;
                max_x = max(line_width, max_x);
                line_head = i;
                line_width = 0.0;
                ascent = 0.0;
                descent = 0.0;
                char_index = 0;
                full_word_index = 0;
                full_word = false;
            } else {
                line_width += word_width;
                i += 1;
            }
        }
        if line_head < self.word_array.len() {
            if typeset {
                line.begin_word_index = line_head as i32;
                line.end_word_index = self.word_array.len() as i32 - 1;
                line.line_width = line_width;
                line.line_ascent = ascent;
                line.line_descent = descent;
                self.add_line(line);
            }
            max_y += ascent + layout.settings.line_leading;
            max_y -= descent;
            max_x = max(line_width, max_x);
        }
        CpvtFloatRect::new(0.0, 0.0, max_x, max_y)
    }
    fn output_lines(&mut self, layout: &Layout<'_>, rect: CpvtFloatRect) -> CpvtFloatRect {
        let indent = layout.line_indent();
        let typeset_width = max(layout.plate_width() - indent, 0.0);
        let alignment_offset = |width| match layout.settings.alignment {
            Alignment::Left => 0.0,
            Alignment::Center => (typeset_width - width) * 0.5,
            Alignment::Right => typeset_width - width,
        };
        let min_x = alignment_offset(rect.width());
        let max_x = min_x + rect.width();
        let min_y = 0.0;
        let max_y = rect.height();
        let paragraph_text: Vec<u16> = self.word_array.iter().map(|word| word.word).collect();
        let bidi = CfxBidiResolver::create(&paragraph_text, layout.settings.text_direction);
        let mut pos_y = 0.0;
        for line in &mut self.line_array {
            let mut pos_x = alignment_offset(line.info.line_width) + indent;
            pos_y += layout.settings.line_leading;
            pos_y += line.info.line_ascent;
            line.info.line_x = pos_x - min_x;
            line.info.line_y = pos_y - min_y;
            let start = line.info.begin_word_index;
            if start < 0 {
                pos_y -= line.info.line_descent;
                continue;
            }
            let length = line.info.end_word_index - start + 1;
            let mut visual_runs = bidi
                .as_ref()
                .expect("nonempty paragraph resolver")
                .get_visual_runs_for_line(start, length);
            if visual_runs.is_empty() {
                visual_runs.push(ResolvedRun {
                    start,
                    length,
                    is_rtl: false,
                });
            }
            for run in visual_runs {
                // GetWordRangeIteratorPair clamps the end first, then begin.
                let end = (run.start + run.length).clamp(0, self.word_array.len() as i32) as usize;
                let begin = run.start.clamp(0, end as i32) as usize;
                let mut position = |word: &mut CpvtWordInfo| {
                    word.is_rtl = run.is_rtl;
                    word.word_x = pos_x - min_x;
                    word.word_y = pos_y - min_y;
                    pos_x += layout.word_width(word);
                };
                if run.is_rtl {
                    for word in self.word_array[begin..end].iter_mut().rev() {
                        position(word);
                    }
                } else {
                    for word in &mut self.word_array[begin..end] {
                        position(word);
                    }
                }
            }
            pos_y -= line.info.line_descent;
        }
        CpvtFloatRect::new(min_x, min_y, max_x, max_y)
    }
}
