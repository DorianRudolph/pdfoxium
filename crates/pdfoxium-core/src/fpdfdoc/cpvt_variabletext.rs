// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Text layout, editing, cursor hit testing and partial rearrangement from
//! `core/fpdfdoc/cpvt_variabletext.{h,cpp}`.
//! Sections receive borrowed settings/provider instead of a self backpointer.

use super::cpvt_floatrect::CpvtFloatRect;
use super::cpvt_line::CpvtLine;
use super::cpvt_lineinfo::CpvtLineInfo;
use super::cpvt_section::CpvtSection;
use super::cpvt_word::CpvtWord;
use super::cpvt_wordinfo::CpvtWordInfo;
use super::cpvt_wordplace::CpvtWordPlace;
use super::ipvt_fontmap::IpvtFontMap;
use crate::fpdfapi::font::cpdf_font::INVALID_CHAR_CODE;
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::fx_codepage::FxCharset;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF};
use std::rc::Rc;

const FONT_SCALE: f32 = 0.001;
const FONT_SIZE_STEPS: [u8; 25] = [
    4, 6, 8, 9, 10, 12, 14, 18, 20, 25, 30, 35, 40, 45, 50, 55, 60, 70, 80, 90, 100, 110, 120, 130,
    144,
];

pub trait Provider {
    fn font_map(&self) -> Option<&dyn IpvtFontMap> {
        None
    }
    fn char_width(&self, font_index: i32, word: u16) -> i32;
    fn type_ascent(&self, font_index: i32) -> i32;
    fn type_descent(&self, font_index: i32) -> i32;
    fn word_font_index(&self, word: u16, charset: FxCharset, font_index: i32) -> i32;
    fn default_font_index(&self) -> i32;
}

pub struct FontMapProvider<'a> {
    pub font_map: &'a dyn IpvtFontMap,
}
impl Provider for FontMapProvider<'_> {
    fn char_width(&self, font_index: i32, word: u16) -> i32 {
        let Some(font) = self.font_map.get_pdf_font(font_index) else {
            return 0;
        };
        let code = font.char_code_from_unicode(u32::from(word));
        if code == INVALID_CHAR_CODE {
            0
        } else {
            font.get_char_width(code)
        }
    }
    fn type_ascent(&self, font_index: i32) -> i32 {
        self.font_map
            .get_pdf_font(font_index)
            .map_or(0, |font| font.type_ascent())
    }
    fn type_descent(&self, font_index: i32) -> i32 {
        self.font_map
            .get_pdf_font(font_index)
            .map_or(0, |font| font.type_descent())
    }
    fn word_font_index(&self, word: u16, _: FxCharset, _: i32) -> i32 {
        for index in [0, 1] {
            if self.font_map.get_pdf_font(index).is_some_and(|font| {
                font.char_code_from_unicode(u32::from(word)) != INVALID_CHAR_CODE
            }) {
                return index;
            }
        }
        -1
    }
    fn default_font_index(&self) -> i32 {
        0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Alignment {
    #[default]
    Left = 0,
    Center = 1,
    Right = 2,
}
impl Alignment {
    pub fn from_pdf(value: i32) -> Self {
        match value {
            1 => Self::Center,
            2 => Self::Right,
            _ => Self::Left,
        }
    }
}

#[derive(Default)]
pub(super) struct TextSettings {
    pub plate_rect: CfxFloatRect,
    pub multi_line: bool,
    pub limit_width: bool,
    pub auto_font_size: bool,
    pub text_direction: ParagraphDirection,
    pub sub_word: u16,
    pub limit_char: i32,
    pub char_array: i32,
    pub alignment: Alignment,
    pub line_leading: f32,
    pub font_size: f32,
}

pub(super) struct Layout<'a> {
    pub settings: &'a TextSettings,
    pub provider: Option<&'a dyn Provider>,
}
impl Layout<'_> {
    pub fn default_font_index(&self) -> i32 {
        self.provider
            .map_or(-1, |provider| provider.default_font_index())
    }
    pub fn plate_width(&self) -> f32 {
        self.settings.plate_rect.right - self.settings.plate_rect.left
    }
    pub fn plate_height(&self) -> f32 {
        self.settings.plate_rect.top - self.settings.plate_rect.bottom
    }
    pub fn line_indent(&self) -> f32 {
        0.0
    }
    pub fn font_ascent(&self, index: i32, size: f32) -> f32 {
        let ascent = self
            .provider
            .map_or(0, |provider| provider.type_ascent(index)) as f32;
        ascent * size * FONT_SCALE
    }
    pub fn font_descent(&self, index: i32, size: f32) -> f32 {
        let descent = self
            .provider
            .map_or(0, |provider| provider.type_descent(index)) as f32;
        descent * size * FONT_SCALE
    }
    pub fn word_width_at(&self, word: &CpvtWordInfo, size: f32) -> f32 {
        let character = if self.settings.sub_word != 0 {
            self.settings.sub_word
        } else {
            word.word
        };
        let width = self.provider.map_or(0, |provider| {
            provider.char_width(word.font_index, character)
        });
        width as f32 * size * FONT_SCALE + word.word_tail
    }
    pub fn word_width(&self, word: &CpvtWordInfo) -> f32 {
        self.word_width_at(word, self.settings.font_size)
    }
    pub fn word_ascent_at(&self, word: &CpvtWordInfo, size: f32) -> f32 {
        self.font_ascent(word.font_index, size)
    }
    pub fn word_descent_at(&self, word: &CpvtWordInfo, size: f32) -> f32 {
        self.font_descent(word.font_index, size)
    }
    pub fn word_ascent(&self, word: &CpvtWordInfo) -> f32 {
        self.word_ascent_at(word, self.settings.font_size)
    }
    pub fn word_descent(&self, word: &CpvtWordInfo) -> f32 {
        self.word_descent_at(word, self.settings.font_size)
    }
}

// std::min/max retain their first argument when unordered, unlike f32::min/max.
pub(super) fn min(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}
pub(super) fn max(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}
fn is_float_bigger(a: f32, b: f32) -> bool {
    let difference = a - b;
    a > b && !(difference < 0.0001 && difference > -0.0001)
}

pub struct CpvtVariableText<'a> {
    initialized: bool,
    settings: TextSettings,
    section_array: Vec<CpvtSection>,
    provider: Option<&'a dyn Provider>,
    owned_provider: Option<Rc<dyn Provider>>,
    content_rect: CpvtFloatRect,
}
impl<'a> CpvtVariableText<'a> {
    pub fn new_owned(provider: Option<Rc<dyn Provider>>) -> Self {
        let mut text = Self::new(None);
        text.owned_provider = provider;
        text
    }
    pub(crate) fn provider(&self) -> Option<&dyn Provider> {
        self.provider.or(self.owned_provider.as_deref())
    }
    pub fn new(provider: Option<&'a dyn Provider>) -> Self {
        Self {
            initialized: false,
            settings: TextSettings::default(),
            section_array: Vec::new(),
            provider,
            owned_provider: None,
            content_rect: CpvtFloatRect::default(),
        }
    }
    pub fn set_plate_rect(&mut self, rect: CfxFloatRect) {
        self.settings.plate_rect = rect;
    }
    pub fn set_alignment(&mut self, alignment: Alignment) {
        self.settings.alignment = alignment;
    }
    pub fn set_font_size(&mut self, size: f32) {
        self.settings.font_size = size;
    }
    pub fn set_auto_font_size(&mut self, auto: bool) {
        self.settings.auto_font_size = auto;
    }
    pub fn set_multi_line(&mut self, value: bool) {
        self.settings.multi_line = value;
    }
    pub fn set_auto_return(&mut self, value: bool) {
        self.settings.limit_width = value;
    }
    pub fn set_password_char(&mut self, value: u16) {
        self.settings.sub_word = value;
    }
    pub fn set_char_array(&mut self, value: i32) {
        self.settings.char_array = value;
    }
    pub fn set_limit_char(&mut self, value: i32) {
        self.settings.limit_char = value;
    }
    pub fn font_size(&self) -> f32 {
        self.settings.font_size
    }
    pub fn initialize(&mut self) {
        if self.initialized {
            return;
        }
        let place = CpvtWordPlace::new(0, -1, -1);
        self.add_section(place);
        let layout = Layout {
            settings: &self.settings,
            provider: self.provider.or(self.owned_provider.as_deref()),
        };
        let line = CpvtLineInfo {
            line_ascent: layout.font_ascent(layout.default_font_index(), self.font_size()),
            line_descent: layout.font_descent(layout.default_font_index(), self.font_size()),
            ..CpvtLineInfo::default()
        };
        if let Some(section) = self.section_array.first_mut() {
            section.add_line(line);
            section.reset_line_place();
        }
        self.initialized = true;
    }
    fn add_section(&mut self, place: CpvtWordPlace) -> CpvtWordPlace {
        if self.initialized && !self.settings.multi_line {
            return place;
        }
        let index = place.sec_index.clamp(0, self.section_array.len() as i32);
        let mut section = CpvtSection::default();
        section.sec_place.sec_index = index;
        self.section_array.insert(index as usize, section);
        place
    }
    pub fn total_words(&self) -> i32 {
        let mut total: i32 = 0;
        for section in &self.section_array {
            total = total
                .checked_add(i32::try_from(section.word_array.len()).unwrap() + 1)
                .unwrap();
        }
        total - 1
    }
    pub fn insert_word(
        &mut self,
        place: CpvtWordPlace,
        word: u16,
        charset: FxCharset,
    ) -> CpvtWordPlace {
        let total = self.total_words();
        if self.settings.limit_char > 0 && total >= self.settings.limit_char {
            return place;
        }
        if self.settings.char_array > 0 && total >= self.settings.char_array {
            return place;
        }
        let mut next = place;
        next.word_index += 1;
        let layout = Layout {
            settings: &self.settings,
            provider: self.provider.or(self.owned_provider.as_deref()),
        };
        let default_index = layout.default_font_index();
        let font_index = if self.settings.sub_word > 0 {
            default_index
        } else {
            self.provider().map_or(-1, |provider| {
                provider.word_font_index(word, charset, default_index)
            })
        };
        if self.section_array.is_empty() {
            return next;
        }
        next.sec_index = next.sec_index.clamp(0, self.section_array.len() as i32 - 1);
        self.section_array[next.sec_index as usize]
            .add_word(next, CpvtWordInfo::new(word, charset, font_index))
    }
    pub fn set_text(&mut self, text: &[u32]) {
        // DeleteWords(GetBeginWordPlace(), GetEndWordPlace()) clears every
        // word, removes empty sections after the first, and retains its lines.
        self.section_array.truncate(1);
        if let Some(section) = self.section_array.first_mut() {
            section.word_array.clear();
            section.rect = CpvtFloatRect::default();
        }
        let mut place = CpvtWordPlace::new(0, 0, -1);
        let mut char_count: i32 = 0;
        let mut i = 0;
        while i < text.len() {
            if self.settings.limit_char > 0 && char_count >= self.settings.limit_char {
                break;
            }
            if self.settings.char_array > 0 && char_count >= self.settings.char_array {
                break;
            }
            // Native narrows each wchar_t to uint16_t, including on Linux.
            let word = text[i] as u16;
            match word {
                0x0d | 0x0a => {
                    if self.settings.multi_line {
                        let other = if word == 0x0d { 0x0a } else { 0x0d };
                        if text.get(i + 1) == Some(&other) {
                            i += 1;
                        }
                        place.advance_section();
                        self.add_section(place);
                    }
                }
                _ => {
                    place = self.insert_word(
                        place,
                        if word == 9 { 32 } else { word },
                        FxCharset::Default,
                    );
                }
            }
            char_count = char_count.checked_add(1).unwrap();
            i += 1;
        }
    }
    pub fn begin_word_place(&self) -> CpvtWordPlace {
        if self.initialized {
            CpvtWordPlace::new(0, 0, -1)
        } else {
            CpvtWordPlace::default()
        }
    }
    pub fn end_word_place(&self) -> CpvtWordPlace {
        self.section_array
            .last()
            .map_or_else(CpvtWordPlace::default, CpvtSection::end_word_place)
    }
    pub fn next_word_place(&self, place: CpvtWordPlace) -> CpvtWordPlace {
        if place.sec_index < 0 {
            return self.begin_word_place();
        }
        let Some(section) = self.section_array.get(place.sec_index as usize) else {
            return self.end_word_place();
        };
        if place < section.end_word_place() {
            return section.next_word_place(place);
        }
        self.section_array
            .get(place.sec_index as usize + 1)
            .map_or_else(|| self.end_word_place(), CpvtSection::begin_word_place)
    }
    pub fn word_index_to_word_place(&self, index: i32) -> CpvtWordPlace {
        let mut place = self.begin_word_place();
        let mut old_index = 0;
        let mut current_index = 0;
        for (i, section) in self.section_array.iter().enumerate() {
            current_index += section.word_array.len() as i32;
            if current_index == index {
                return section.end_word_place();
            }
            if current_index > index {
                place.sec_index = i as i32;
                place.word_index = index - old_index - 1;
                section.update_word_place(&mut place);
                return place;
            }
            if i != self.section_array.len() - 1 {
                current_index += 1;
            }
            old_index = current_index;
        }
        self.end_word_place()
    }
    fn is_bigger(&mut self, font_size: f32) -> bool {
        let mut width = 0.0;
        let mut height = 0.0;
        let layout = Layout {
            settings: &self.settings,
            provider: self.provider.or(self.owned_provider.as_deref()),
        };
        for section in &mut self.section_array {
            let size = section.split_lines(&layout, false, font_size);
            width = max(size.width(), width);
            height += size.height();
            if is_float_bigger(width, layout.plate_width())
                || is_float_bigger(height, layout.plate_height())
            {
                return true;
            }
        }
        false
    }
    fn auto_font_size(&mut self) -> f32 {
        let layout = Layout {
            settings: &self.settings,
            provider: self.provider.or(self.owned_provider.as_deref()),
        };
        if layout.plate_width() <= 0.0 {
            return 0.0;
        }
        let size = if self.settings.multi_line {
            FONT_SIZE_STEPS.len() / 4
        } else {
            FONT_SIZE_STEPS.len()
        };
        let steps = &FONT_SIZE_STEPS[..size];
        // std::lower_bound with !IsBigger as the comparator.
        let (mut first, mut count) = (0, steps.len());
        while count > 0 {
            let step = count / 2;
            let middle = first + step;
            if !self.is_bigger(f32::from(steps[middle])) {
                first = middle + 1;
                count -= step + 1;
            } else {
                count = step;
            }
        }
        f32::from(if first == steps.len() {
            steps[steps.len() - 1]
        } else if first == 0 {
            steps[0]
        } else {
            steps[first - 1]
        })
    }
    pub fn rearrange_all(&mut self) {
        let range = super::cpvt_wordrange::CpvtWordRange::new(
            self.begin_word_place(),
            self.end_word_place(),
        );
        self.rearrange_part(range);
    }
    pub fn rearrange_part(&mut self, range: super::cpvt_wordrange::CpvtWordRange) {
        let mut result = CpvtFloatRect::default();
        if self.initialized {
            if self.settings.auto_font_size {
                self.settings.font_size = self.auto_font_size();
            }
            let layout = Layout {
                settings: &self.settings,
                provider: self.provider.or(self.owned_provider.as_deref()),
            };
            let mut pos_y = 0.0;
            for (index, section) in self.section_array.iter_mut().enumerate() {
                section.sec_place = CpvtWordPlace::new(index as i32, -1, -1);
                let mut rect = section.rect;
                if self.settings.auto_font_size || index as i32 >= range.begin.sec_index {
                    if self.settings.auto_font_size || index as i32 <= range.end.sec_index {
                        rect = section.rearrange(&layout);
                        rect.top += pos_y;
                        rect.bottom += pos_y;
                    } else {
                        let height = rect.height();
                        rect.top = pos_y;
                        rect.bottom = pos_y + height;
                    }
                    section.rect = rect;
                    section.reset_line_place();
                }
                if index == 0 {
                    result = rect;
                } else {
                    result.left = min(rect.left, result.left);
                    result.top = min(rect.top, result.top);
                    result.right = max(rect.right, result.right);
                    result.bottom = max(rect.bottom, result.bottom);
                }
                pos_y += rect.height();
            }
        }
        self.content_rect = result;
    }
    fn in_to_out(&self, point: CfxPointF) -> CfxPointF {
        CfxPointF::new(
            point.x + self.settings.plate_rect.left,
            self.settings.plate_rect.top - point.y,
        )
    }
    pub fn content_rect(&self) -> CfxFloatRect {
        let top_left = self.in_to_out(CfxPointF::new(
            self.content_rect.left,
            self.content_rect.top,
        ));
        let bottom_right = self.in_to_out(CfxPointF::new(
            self.content_rect.right,
            self.content_rect.bottom,
        ));
        CfxFloatRect::new(top_left.x, bottom_right.y, bottom_right.x, top_left.y)
    }
    pub fn iterator(&self) -> Iterator<'_, 'a> {
        Iterator {
            cur_pos: CpvtWordPlace::default(),
            vt: self,
        }
    }
}

pub struct Iterator<'v, 'p> {
    cur_pos: CpvtWordPlace,
    vt: &'v CpvtVariableText<'p>,
}
impl Iterator<'_, '_> {
    pub fn set_at_index(&mut self, index: i32) {
        self.cur_pos = self.vt.word_index_to_word_place(index);
    }
    pub fn set_at(&mut self, place: CpvtWordPlace) {
        self.cur_pos = place;
    }
    pub fn word_place(&self) -> CpvtWordPlace {
        self.cur_pos
    }
    pub fn next_word(&mut self) -> bool {
        if self.cur_pos == self.vt.end_word_place() {
            return false;
        }
        self.cur_pos = self.vt.next_word_place(self.cur_pos);
        true
    }
    pub fn next_line(&mut self) -> bool {
        let Some(section) = self.vt.section_array.get(self.cur_pos.sec_index as usize) else {
            return false;
        };
        if self.cur_pos.line_index < section.line_array.len() as i32 - 1 {
            self.cur_pos =
                CpvtWordPlace::new(self.cur_pos.sec_index, self.cur_pos.line_index + 1, -1);
            return true;
        }
        if self.cur_pos.sec_index < self.vt.section_array.len() as i32 - 1 {
            self.cur_pos = CpvtWordPlace::new(self.cur_pos.sec_index + 1, 0, -1);
            return true;
        }
        false
    }
    pub fn word(&self) -> Option<CpvtWord> {
        let section = self.vt.section_array.get(self.cur_pos.sec_index as usize)?;
        section.line_array.get(self.cur_pos.line_index as usize)?;
        let info = section.word_array.get(self.cur_pos.word_index as usize)?;
        let layout = Layout {
            settings: &self.vt.settings,
            provider: self.vt.provider(),
        };
        Some(CpvtWord {
            word: info.word,
            charset: info.charset,
            location: self.vt.in_to_out(CfxPointF::new(
                info.word_x + section.rect.left,
                info.word_y + section.rect.top,
            )),
            ascent: layout.word_ascent(info),
            descent: layout.word_descent(info),
            width: layout.word_width(info),
            font_index: info.font_index,
            font_size: self.vt.font_size(),
            is_rtl: info.is_rtl,
        })
    }
    pub fn line(&self) -> Option<CpvtLine> {
        let section = self.vt.section_array.get(self.cur_pos.sec_index as usize)?;
        let line = section.line_array.get(self.cur_pos.line_index as usize)?;
        Some(CpvtLine {
            line_place: CpvtWordPlace::new(self.cur_pos.sec_index, self.cur_pos.line_index, -1),
            line_end: line.end_word_place(),
            point: self.vt.in_to_out(CfxPointF::new(
                line.info.line_x + section.rect.left,
                line.info.line_y + section.rect.top,
            )),
            width: line.info.line_width,
            ascent: line.info.line_ascent,
            descent: line.info.line_descent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The upstream CPVT_StubProvider is test-only (fpdfdoc/BUILD.gn).
    struct StubProvider;
    impl Provider for StubProvider {
        fn char_width(&self, _: i32, _: u16) -> i32 {
            10
        }
        fn type_ascent(&self, _: i32) -> i32 {
            10
        }
        fn type_descent(&self, _: i32) -> i32 {
            -2
        }
        fn word_font_index(&self, _: u16, _: FxCharset, _: i32) -> i32 {
            0
        }
        fn default_font_index(&self) -> i32 {
            0
        }
    }

    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.00001,
            "{actual} != {expected}"
        );
    }

    #[test]
    fn native_logical_iterator_preserves_bidi_visual_positions() {
        let provider = StubProvider;
        let mut vt = CpvtVariableText::new(Some(&provider));
        vt.set_plate_rect(CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
        vt.set_font_size(10.0);
        vt.initialize();
        vt.initialize();
        vt.set_text(&wide("hello"));
        vt.rearrange_all();
        let mut iterator = vt.iterator();
        iterator.set_at_index(0);
        for (i, character) in "hello".chars().enumerate() {
            assert!(iterator.next_word());
            let word = iterator.word().unwrap();
            assert_eq!(word.word, character as u16);
            near(word.caret_x(), (i + 1) as f32 * 0.1);
        }
        assert!(!iterator.next_word());

        vt.set_text(&wide("שלום"));
        vt.rearrange_all();
        let mut iterator = vt.iterator();
        iterator.set_at_index(1);
        for (i, character) in "שלום".chars().enumerate() {
            let word = iterator.word().unwrap();
            assert_eq!(word.word, character as u16);
            assert!(word.is_rtl);
            near(word.caret_x(), (3 - i) as f32 * 0.1);
            assert_eq!(iterator.next_word(), i != 3);
        }
    }

    #[test]
    fn single_line_control_characters_alignment_and_auto_font_size() {
        let provider = StubProvider;
        let mut vt = CpvtVariableText::new(Some(&provider));
        vt.set_plate_rect(CfxFloatRect::new(10.0, 20.0, 12.0, 21.0));
        vt.set_auto_font_size(true);
        vt.set_alignment(Alignment::Right);
        vt.initialize();
        vt.set_text(&wide("a\r\n\tb"));
        vt.rearrange_all();
        // Three 10-unit words: size 60 fits a two-unit plate; 70 does not.
        assert_eq!(vt.font_size(), 60.0);
        near(vt.content_rect().width(), 1.8);
        near(vt.content_rect().height(), 0.72);
        near(vt.content_rect().right, 12.0);
        let mut iterator = vt.iterator();
        iterator.set_at_index(0);
        let mut words = Vec::new();
        while iterator.next_word() {
            words.push(iterator.word().unwrap().word);
        }
        assert_eq!(words, [97, 32, 98]);
        // Linux wchar_t values are narrowed, rather than encoded as a pair.
        vt.set_text(&[0x1f600]);
        vt.rearrange_all();
        let mut iterator = vt.iterator();
        iterator.set_at_index(1);
        assert_eq!(iterator.word().unwrap().word, 0xf600);

        vt.set_text(&[]);
        vt.rearrange_all();
        let mut iterator = vt.iterator();
        iterator.set_at_index(0);
        assert!(!iterator.next_word());
        near(iterator.line().unwrap().point.x, 12.0);
    }
}

#[cfg(test)]
#[path = "cpvt_test_support.rs"]
mod upstream_test_support;

#[cfg(test)]
#[path = "cpvt_section_unittest.rs"]
mod section_upstream_tests;

#[cfg(test)]
#[path = "cpvt_variabletext_unittest.rs"]
mod upstream_tests;

// Interactive branches of the same upstream class. Section word arrays are
// moved when splitting/linking instead of copying the native unique_ptr data.
impl CpvtVariableText<'_> {
    pub fn is_valid(&self) -> bool {
        self.initialized
    }
    pub fn plate_rect(&self) -> CfxFloatRect {
        self.settings.plate_rect
    }
    pub fn password_char(&self) -> u16 {
        self.settings.sub_word
    }
    pub fn char_array(&self) -> i32 {
        self.settings.char_array
    }
    pub fn limit_char(&self) -> i32 {
        self.settings.limit_char
    }
    pub fn is_multi_line(&self) -> bool {
        self.settings.multi_line
    }
    pub fn text_direction(&self) -> ParagraphDirection {
        self.settings.text_direction
    }
    pub fn set_text_direction(&mut self, direction: ParagraphDirection) {
        self.settings.text_direction = direction;
    }
    fn prev_line_header_place(&self, place: CpvtWordPlace) -> CpvtWordPlace {
        if place.word_index < 0 && place.line_index > 0 {
            self.prev_word_place(place)
        } else {
            place
        }
    }
    pub fn update_word_place(&self, place: &mut CpvtWordPlace) {
        if place.sec_index < 0 {
            *place = self.begin_word_place();
        }
        if place.sec_index as usize >= self.section_array.len() {
            *place = self.end_word_place();
        }
        *place = self.prev_line_header_place(*place);
        if let Some(section) = self.section_array.get(place.sec_index as usize) {
            section.update_word_place(place);
        }
    }
    pub fn word_place_to_word_index(&self, mut place: CpvtWordPlace) -> i32 {
        self.update_word_place(&mut place);
        let mut index = 0;
        for (i, section) in self.section_array.iter().enumerate() {
            if i as i32 == place.sec_index {
                return index + place.word_index + 1;
            }
            index += section.word_array.len() as i32 + 1;
        }
        index
    }
    pub fn insert_section(&mut self, mut place: CpvtWordPlace) -> CpvtWordPlace {
        let total = self.total_words();
        if !self.settings.multi_line
            || (self.settings.limit_char > 0 && total >= self.settings.limit_char)
            || (self.settings.char_array > 0 && total >= self.settings.char_array)
        {
            return place;
        }
        self.update_word_place(&mut place);
        let Some(section) = self.section_array.get_mut(place.sec_index as usize) else {
            return place;
        };
        let split = (place.word_index + 1).clamp(0, section.word_array.len() as i32) as usize;
        let suffix = section.word_array.split_off(split);
        let result = CpvtWordPlace::new(place.sec_index + 1, 0, -1);
        self.add_section(result);
        self.section_array[result.sec_index as usize].word_array = suffix;
        result
    }
    fn link_latter_section(&mut self, place: CpvtWordPlace) {
        if place.sec_index < 0 || place.sec_index as usize + 1 >= self.section_array.len() {
            return;
        }
        let mut next = self.section_array.remove(place.sec_index as usize + 1);
        let place = self.prev_line_header_place(place);
        let section = &mut self.section_array[place.sec_index as usize];
        let index = (place.word_index + 1).clamp(0, section.word_array.len() as i32) as usize;
        section
            .word_array
            .splice(index..index, next.word_array.drain(..));
    }
    pub fn delete_words(&mut self, range: super::cpvt_wordrange::CpvtWordRange) -> CpvtWordPlace {
        let last_end = self
            .section_array
            .get(range.end.sec_index as usize)
            .is_some_and(|s| range.end == s.end_word_place());
        let begin = self.prev_line_header_place(range.begin);
        let end = self.prev_line_header_place(range.end);
        for index in (begin.sec_index..=end.sec_index).rev() {
            let Some(section) = self.section_array.get_mut(index as usize) else {
                continue;
            };
            section.clear_words(super::cpvt_wordrange::CpvtWordRange::new(begin, end));
        }
        if range.begin.sec_index != range.end.sec_index {
            for index in (range.begin.sec_index + 1..=range.end.sec_index).rev() {
                if self
                    .section_array
                    .get(index as usize)
                    .is_some_and(|s| s.word_array.is_empty())
                    && self.section_array.len() > 1
                {
                    self.section_array.remove(index as usize);
                }
            }
            if !last_end {
                self.link_latter_section(range.begin);
            }
        }
        range.begin
    }
    pub fn prev_word_place(&self, place: CpvtWordPlace) -> CpvtWordPlace {
        if place.sec_index < 0 {
            return self.begin_word_place();
        }
        let Some(section) = self.section_array.get(place.sec_index as usize) else {
            return self.end_word_place();
        };
        if place <= section.begin_word_place() {
            return place
                .sec_index
                .checked_sub(1)
                .and_then(|i| self.section_array.get(i as usize))
                .map_or(self.begin_word_place(), CpvtSection::end_word_place);
        }
        if place.line_index < 0 {
            return section.begin_word_place();
        }
        let Some(line) = section.line_array.get(place.line_index as usize) else {
            return section.end_word_place();
        };
        if place.word_index == line.info.begin_word_index {
            return CpvtWordPlace::new(place.sec_index, place.line_index, -1);
        }
        if place.word_index >= line.info.begin_word_index {
            return CpvtWordPlace::new(
                place.sec_index,
                place.line_index,
                (place.word_index - 1).min(line.info.end_word_index),
            );
        }
        section
            .line_array
            .get((place.line_index - 1) as usize)
            .map_or(place, |line| line.end_word_place())
    }
    pub fn delete_word(&mut self, place: CpvtWordPlace) -> CpvtWordPlace {
        let place = self.prev_line_header_place(place);
        let mut right = self.next_word_place(place);
        if right.word_index < 0 && right.line_index > 0 {
            right = self.next_word_place(right);
        }
        if right == place {
            return place;
        }
        if right.sec_index != place.sec_index {
            self.link_latter_section(place);
        } else if let Some(section) = self.section_array.get_mut(place.sec_index as usize) {
            if right.word_index >= 0 && (right.word_index as usize) < section.word_array.len() {
                section.word_array.remove(right.word_index as usize);
            }
        }
        place
    }
    pub fn backspace_word(&mut self, place: CpvtWordPlace) -> CpvtWordPlace {
        let place = self.prev_line_header_place(place);
        let left = self.prev_word_place(place);
        if left == place {
            return place;
        }
        if left.sec_index != place.sec_index {
            self.link_latter_section(left);
        } else if let Some(section) = self.section_array.get_mut(place.sec_index as usize) {
            if place.word_index >= 0 && (place.word_index as usize) < section.word_array.len() {
                section.word_array.remove(place.word_index as usize);
            }
        }
        left
    }
    pub fn line_begin_place(&self, place: CpvtWordPlace) -> CpvtWordPlace {
        CpvtWordPlace::new(place.sec_index, place.line_index, -1)
    }
    pub fn line_end_place(&self, place: CpvtWordPlace) -> CpvtWordPlace {
        self.section_array
            .get(place.sec_index as usize)
            .and_then(|s| s.line_array.get(place.line_index as usize))
            .map_or(place, |line| line.end_word_place())
    }
    pub fn section_begin_place(&self, place: CpvtWordPlace) -> CpvtWordPlace {
        CpvtWordPlace::new(place.sec_index, 0, -1)
    }
    pub fn section_end_place(&self, place: CpvtWordPlace) -> CpvtWordPlace {
        self.section_array
            .get(place.sec_index as usize)
            .map_or(place, CpvtSection::end_word_place)
    }
    fn out_to_in(&self, point: CfxPointF) -> CfxPointF {
        CfxPointF::new(
            point.x - self.settings.plate_rect.left,
            self.settings.plate_rect.top - point.y,
        )
    }
    // SearchWordPlaceImpl: the midpoint test is against each word's center.
    // Use the native binary-search bounds and strict comparison.
    fn search_line(&self, section_index: usize, line_index: usize, x: f32) -> CpvtWordPlace {
        let section = &self.section_array[section_index];
        let Some(line) = section.line_array.get(line_index) else {
            return section.begin_word_place();
        };
        let mut result = CpvtWordPlace::new(section_index as i32, line_index as i32, -1);
        let (mut left, mut right) = (line.info.begin_word_index, line.info.end_word_index + 1);
        let mut middle = (left + right) / 2;
        let layout = Layout {
            settings: &self.settings,
            provider: self.provider.or(self.owned_provider.as_deref()),
        };
        while left < right {
            if middle == left {
                break;
            }
            if middle == right {
                middle -= 1;
                break;
            }
            let Some(word) = section.word_array.get(middle as usize) else {
                break;
            };
            if x > word.word_x + layout.word_width(word) * 0.5 {
                left = middle;
            } else {
                right = middle;
            }
            middle = (left + right) / 2;
        }
        if let Some(word) = section.word_array.get(middle as usize) {
            if x > word.word_x + layout.word_width(word) * 0.5 {
                result.word_index = middle;
            }
        }
        result
    }
    pub fn search_word_place(&self, point: CfxPointF) -> CpvtWordPlace {
        let point = self.out_to_in(point);
        let (mut left, mut right) = (0_i32, self.section_array.len() as i32 - 1);
        let mut middle = self.section_array.len() as i32 / 2;
        let (mut up, mut down) = (true, true);
        while left <= right {
            let Some(section) = self.section_array.get(middle as usize) else {
                break;
            };
            if is_float_bigger(point.y, section.rect.top) {
                up = false;
            }
            if is_float_bigger(section.rect.bottom, point.y) {
                down = false;
            }
            if is_float_bigger(section.rect.top, point.y) {
                right = middle - 1;
                middle = (left + right) / 2;
                continue;
            }
            if is_float_bigger(point.y, section.rect.bottom) {
                left = middle + 1;
                middle = (left + right) / 2;
                continue;
            }
            let y = point.y - section.rect.top;
            let (mut lo, mut hi) = (0_i32, section.line_array.len() as i32 - 1);
            let mut mid = section.line_array.len() as i32 / 2;
            let (mut above, mut below) = (true, true);
            while lo <= hi {
                let line = &section.line_array[mid as usize];
                let top = line.info.line_y - line.info.line_ascent - self.settings.line_leading;
                let bottom = line.info.line_y - line.info.line_descent;
                if is_float_bigger(y, top) {
                    above = false;
                }
                if is_float_bigger(bottom, y) {
                    below = false;
                }
                if is_float_bigger(top, y) {
                    hi = mid - 1;
                    mid = (lo + hi) / 2;
                    continue;
                }
                if is_float_bigger(y, bottom) {
                    lo = mid + 1;
                    mid = (lo + hi) / 2;
                    continue;
                }
                return self.search_line(
                    middle as usize,
                    mid as usize,
                    point.x - section.rect.left,
                );
            }
            let mut result = section.begin_word_place();
            if above {
                result = section.begin_word_place();
            }
            if below {
                result = section.end_word_place();
            }
            return result;
        }
        let mut result = self.begin_word_place();
        if up {
            result = self.begin_word_place();
        }
        if down {
            result = self.end_word_place();
        }
        result
    }
    pub fn up_word_place(&self, place: CpvtWordPlace, point: CfxPointF) -> CpvtWordPlace {
        let Some(section) = self.section_array.get(place.sec_index as usize) else {
            return place;
        };
        let (index, line) = if place.line_index > 0 {
            (place.sec_index as usize, place.line_index as usize - 1)
        } else if place.sec_index > 0 {
            let index = place.sec_index as usize - 1;
            (
                index,
                self.section_array[index].line_array.len().saturating_sub(1),
            )
        } else {
            return place;
        };
        let _ = section;
        // Correct the native double subtraction of section.left at this host boundary.
        self.search_line(
            index,
            line,
            self.out_to_in(point).x - self.section_array[index].rect.left,
        )
    }
    pub fn down_word_place(&self, place: CpvtWordPlace, point: CfxPointF) -> CpvtWordPlace {
        let Some(section) = self.section_array.get(place.sec_index as usize) else {
            return place;
        };
        let (index, line) = if place.line_index + 1 < section.line_array.len() as i32 {
            (place.sec_index as usize, place.line_index as usize + 1)
        } else if place.sec_index as usize + 1 < self.section_array.len() {
            (place.sec_index as usize + 1, 0)
        } else {
            return place;
        };
        self.search_line(
            index,
            line,
            self.out_to_in(point).x - self.section_array[index].rect.left,
        )
    }
}

impl Iterator<'_, '_> {
    pub fn line_caret_x(&self, line: &CpvtLine) -> f32 {
        let mut iter = self.vt.iterator();
        iter.set_at(line.line_place);
        if iter.next_word()
            && iter.word_place().line_cmp(line.line_place) == 0
            && iter.word().is_some_and(|w| w.is_rtl)
        {
            line.point.x + line.width
        } else {
            line.point.x
        }
    }
}
