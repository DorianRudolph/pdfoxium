// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Layout, selection, editing, navigation, scrolling and undo from
//! `fpdfsdk/pwl/cpwl_edit_impl.{h,cpp}`. Native edit backpointers become borrowed
//! providers and weak notifications. Device drawing and appearance streams
//! consume the same layout iterator.

use crate::fpdfapi::font::cpdf_font::{CpdfFont, INVALID_CHAR_CODE};
use crate::fpdfdoc::cpvt_line::CpvtLine;
use crate::fpdfdoc::cpvt_variabletext::{
    Alignment, CpvtVariableText, Iterator as VtIterator, Provider,
};
use crate::fpdfdoc::cpvt_word::CpvtWord;
use crate::fpdfdoc::cpvt_wordplace::CpvtWordPlace;
use crate::fpdfdoc::cpvt_wordrange::CpvtWordRange;
use crate::fpdfdoc::ipvt_fontmap::IpvtFontMap;
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::fx_codepage::FxCharset;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

fn bigger(a: f32, b: f32) -> bool {
    a > b && (a - b).abs() >= 0.0001
}
fn equal(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.0001
}
fn unicode_compatible(font: &CpdfFont) -> bool {
    match font {
        CpdfFont::Simple(font) => font.base().simple.is_unicode_compatible(),
        CpdfFont::Cid(font) => font.metrics.borrow().is_unicode_compatible(),
        CpdfFont::Type3(_) => false,
    }
}

pub struct EditFontMapProvider<'a> {
    pub font_map: &'a dyn IpvtFontMap,
}
impl Provider for EditFontMapProvider<'_> {
    fn font_map(&self) -> Option<&dyn IpvtFontMap> {
        Some(self.font_map)
    }
    fn char_width(&self, index: i32, word: u16) -> i32 {
        let Some(font) = self.font_map.get_pdf_font(index) else {
            return 0;
        };
        let code = if unicode_compatible(&font) {
            font.char_code_from_unicode(u32::from(word))
        } else {
            self.font_map.char_code_from_unicode(index, word) as u32
        };
        if code == INVALID_CHAR_CODE {
            0
        } else {
            font.get_char_width(code)
        }
    }
    fn type_ascent(&self, index: i32) -> i32 {
        self.font_map
            .get_pdf_font(index)
            .map_or(0, |f| f.type_ascent())
    }
    fn type_descent(&self, index: i32) -> i32 {
        self.font_map
            .get_pdf_font(index)
            .map_or(0, |f| f.type_descent())
    }
    fn word_font_index(&self, word: u16, charset: FxCharset, index: i32) -> i32 {
        self.font_map.get_word_font_index(word, charset, index)
    }
    fn default_font_index(&self) -> i32 {
        0
    }
}

pub struct OwnedEditFontMapProvider {
    pub font_map: Rc<dyn IpvtFontMap>,
}
impl Provider for OwnedEditFontMapProvider {
    fn font_map(&self) -> Option<&dyn IpvtFontMap> {
        Some(self.font_map.as_ref())
    }
    fn char_width(&self, index: i32, word: u16) -> i32 {
        EditFontMapProvider {
            font_map: self.font_map.as_ref(),
        }
        .char_width(index, word)
    }
    fn type_ascent(&self, index: i32) -> i32 {
        self.font_map
            .get_pdf_font(index)
            .map_or(0, |font| font.type_ascent())
    }
    fn type_descent(&self, index: i32) -> i32 {
        self.font_map
            .get_pdf_font(index)
            .map_or(0, |font| font.type_descent())
    }
    fn word_font_index(&self, word: u16, charset: FxCharset, index: i32) -> i32 {
        self.font_map.get_word_font_index(word, charset, index)
    }
    fn default_font_index(&self) -> i32 {
        0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScrollInfo {
    pub plate_width: f32,
    pub content_min: f32,
    pub content_max: f32,
    pub small_step: f32,
    pub big_step: f32,
}
/// Notifications are weakly retained, like the observed CPWL_Edit. Returning
/// false from invalidation signals that the receiving window was destroyed.
pub trait EditNotify {
    fn set_scroll_info(&self, info: ScrollInfo);
    fn set_scroll_position(&self, position: f32);
    fn set_caret(&self, visible: bool, head: CfxPointF, foot: CfxPointF);
    fn invalidate_rect(&self, rect: CfxFloatRect) -> bool;
}

#[derive(Default)]
struct RefreshState {
    new_lines: Vec<CfxFloatRect>,
    old_lines: Vec<CfxFloatRect>,
}
impl RefreshState {
    fn begin(&mut self) {
        self.old_lines = std::mem::take(&mut self.new_lines);
    }
    fn rects(&self) -> Vec<CfxFloatRect> {
        let mut rects: Vec<CfxFloatRect> = Vec::new();
        for &rect in self.old_lines.iter().chain(&self.new_lines) {
            if !rects.iter().any(|old| {
                old.left <= rect.left
                    && old.right >= rect.right
                    && old.bottom <= rect.bottom
                    && old.top >= rect.top
            }) {
                rects.push(rect);
            }
        }
        rects
    }
}

#[derive(Default)]
struct SelectState {
    begin: CpvtWordPlace,
    end: CpvtWordPlace,
}
impl SelectState {
    fn is_empty(&self) -> bool {
        self.begin == self.end
    }
    fn range(&self) -> CpvtWordRange {
        CpvtWordRange::new(self.begin, self.end)
    }
    fn set(&mut self, begin: CpvtWordPlace, end: CpvtWordPlace) {
        self.begin = begin;
        self.end = end;
    }
}

enum UndoItem {
    Sentinel,
    InsertWord {
        old: CpvtWordPlace,
        new: CpvtWordPlace,
        word: u16,
        charset: FxCharset,
    },
    InsertReturn {
        old: CpvtWordPlace,
        new: CpvtWordPlace,
    },
    Backspace {
        old: CpvtWordPlace,
        new: CpvtWordPlace,
        word: u16,
        charset: FxCharset,
    },
    Delete {
        old: CpvtWordPlace,
        new: CpvtWordPlace,
        word: u16,
        charset: FxCharset,
        section_end: bool,
    },
    Clear {
        range: CpvtWordRange,
        text: Vec<u32>,
    },
    InsertText {
        old: CpvtWordPlace,
        new: CpvtWordPlace,
        text: Vec<u32>,
        charset: FxCharset,
    },
}
impl UndoItem {
    fn is_sentinel(&self) -> bool {
        matches!(self, Self::Sentinel)
    }
}
struct UndoStack {
    items: VecDeque<UndoItem>,
    position: usize,
    maximum: usize,
    working: bool,
}
impl Default for UndoStack {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
            position: 0,
            maximum: 10000,
            working: false,
        }
    }
}
impl UndoStack {
    fn add(&mut self, item: UndoItem) {
        if self.working {
            return;
        }
        self.items.truncate(self.position);
        if self.items.len() >= self.maximum {
            let sentinel = self
                .items
                .pop_front()
                .is_some_and(|item| item.is_sentinel());
            if sentinel {
                while let Some(item) = self.items.pop_front() {
                    if item.is_sentinel() {
                        break;
                    }
                }
            }
        }
        self.items.push_back(item);
        self.position = self.items.len();
    }
}

pub struct CpwlEditImpl<'a> {
    vt: CpvtVariableText<'a>,
    enable_scroll: bool,
    enable_overflow: bool,
    enable_refresh: bool,
    enable_undo: bool,
    alignment: i32,
    notify: Option<Weak<dyn EditNotify>>,
    notify_flag: bool,
    caret: CpvtWordPlace,
    old_caret: CpvtWordPlace,
    selection: SelectState,
    scroll_pos: CfxPointF,
    caret_point: CfxPointF,
    refresh: RefreshState,
    undo: UndoStack,
}
impl<'a> CpwlEditImpl<'a> {
    pub fn new_owned(provider: Option<Rc<dyn Provider>>) -> Self {
        let mut edit = Self::new(None);
        edit.vt = CpvtVariableText::new_owned(provider);
        edit
    }
    pub fn new(provider: Option<&'a dyn Provider>) -> Self {
        Self {
            vt: CpvtVariableText::new(provider),
            enable_scroll: false,
            enable_overflow: false,
            enable_refresh: true,
            enable_undo: true,
            alignment: 0,
            notify: None,
            notify_flag: false,
            caret: CpvtWordPlace::default(),
            old_caret: CpvtWordPlace::default(),
            selection: SelectState::default(),
            scroll_pos: CfxPointF::default(),
            caret_point: CfxPointF::default(),
            refresh: RefreshState::default(),
            undo: UndoStack::default(),
        }
    }
    pub fn set_notify(&mut self, notify: Option<&Rc<dyn EditNotify>>) {
        self.notify = notify.map(Rc::downgrade);
    }
    pub fn initialize(&mut self) {
        self.vt.initialize();
        self.set_caret(self.vt.begin_word_place());
        self.set_caret_origin();
    }
    pub fn set_plate_rect(&mut self, rect: CfxFloatRect) {
        self.vt.set_plate_rect(rect);
        self.scroll_pos = CfxPointF::new(rect.left, rect.top);
    }
    pub fn set_alignment_h(&mut self, value: i32) {
        self.vt.set_alignment(Alignment::from_pdf(value));
    }
    pub fn set_alignment_v(&mut self, value: i32) {
        self.alignment = value;
    }
    pub fn set_text_direction(&mut self, direction: ParagraphDirection) {
        if self.vt.text_direction() != direction {
            self.vt.set_text_direction(direction);
            self.paint();
        }
    }
    pub fn text_direction(&self) -> ParagraphDirection {
        self.vt.text_direction()
    }
    pub fn set_password_char(&mut self, value: u16) {
        self.vt.set_password_char(value);
    }
    pub fn set_limit_char(&mut self, value: i32) {
        self.vt.set_limit_char(value);
    }
    pub fn set_char_array(&mut self, value: i32) {
        self.vt.set_char_array(value);
    }
    pub fn set_multi_line(&mut self, value: bool) {
        self.vt.set_multi_line(value);
    }
    pub fn set_auto_return(&mut self, value: bool) {
        self.vt.set_auto_return(value);
    }
    pub fn set_auto_font_size(&mut self, value: bool) {
        self.vt.set_auto_font_size(value);
    }
    pub fn set_font_size(&mut self, value: f32) {
        self.vt.set_font_size(value);
    }
    pub fn set_auto_scroll(&mut self, value: bool) {
        self.enable_scroll = value;
    }
    pub fn set_text_overflow(&mut self, value: bool) {
        self.enable_overflow = value;
    }
    pub fn enable_refresh(&mut self, value: bool) {
        self.enable_refresh = value;
    }
    pub fn enable_undo(&mut self, value: bool) {
        self.enable_undo = value;
    }
    pub fn font_size(&self) -> f32 {
        self.vt.font_size()
    }
    pub fn password_char(&self) -> u16 {
        self.vt.password_char()
    }
    pub fn char_array(&self) -> i32 {
        self.vt.char_array()
    }
    pub fn scroll_pos(&self) -> CfxPointF {
        self.scroll_pos
    }
    pub fn font_map(&self) -> Option<&dyn IpvtFontMap> {
        self.vt.provider().and_then(Provider::font_map)
    }
    pub fn content_rect(&self) -> CfxFloatRect {
        self.vt_to_edit_rect(self.vt.content_rect())
    }
    pub fn caret(&self) -> i32 {
        if self.vt.is_valid() {
            self.vt.word_place_to_word_index(self.caret)
        } else {
            -1
        }
    }
    pub fn caret_word_place(&self) -> CpvtWordPlace {
        self.caret
    }
    pub fn word_index_to_word_place(&self, index: i32) -> CpvtWordPlace {
        if self.vt.is_valid() {
            self.vt.word_index_to_word_place(index)
        } else {
            CpvtWordPlace::default()
        }
    }
    fn set_caret(&mut self, place: CpvtWordPlace) {
        self.old_caret = self.caret;
        self.caret = place;
    }
    pub fn whole_word_range(&self) -> CpvtWordRange {
        if self.vt.is_valid() {
            CpvtWordRange::new(self.vt.begin_word_place(), self.vt.end_word_place())
        } else {
            CpvtWordRange::default()
        }
    }
    pub fn select_word_range(&self) -> CpvtWordRange {
        self.selection.range()
    }
    pub fn is_selected(&self) -> bool {
        !self.selection.is_empty()
    }
    pub fn selection(&self) -> (i32, i32) {
        if !self.vt.is_valid() {
            return (-1, -1);
        }
        if self.selection.is_empty() {
            return (self.caret(), self.caret());
        }
        let range = self.selection.range();
        (
            self.vt.word_place_to_word_index(range.begin),
            self.vt.word_place_to_word_index(range.end),
        )
    }
    pub fn set_selection(&mut self, start: i32, end: i32) {
        if !self.vt.is_valid() {
            return;
        }
        if start == 0 && end < 0 {
            self.select_all();
        } else if start < 0 {
            self.select_none();
        } else {
            self.set_selection_places(
                self.vt.word_index_to_word_place(start.min(end)),
                self.vt.word_index_to_word_place(start.max(end)),
            );
        }
    }
    fn set_selection_places(&mut self, begin: CpvtWordPlace, end: CpvtWordPlace) {
        if !self.vt.is_valid() {
            return;
        }
        self.select_none();
        self.selection.set(begin, end);
        self.set_caret(end);
        self.scroll_to_caret();
        if !self.selection.is_empty() {
            self.refresh();
        }
        self.set_caret_info();
    }
    pub fn select_all(&mut self) {
        if !self.vt.is_valid() {
            return;
        }
        let range = self.whole_word_range();
        self.selection.set(range.begin, range.end);
        self.set_caret(range.end);
        self.scroll_to_caret();
        self.refresh();
        self.set_caret_info();
    }
    pub fn select_none(&mut self) {
        if !self.vt.is_valid() || self.selection.is_empty() {
            return;
        }
        self.selection = SelectState::default();
        self.refresh();
    }
    pub fn text(&self) -> Vec<u32> {
        self.range_text(self.whole_word_range())
    }
    pub fn selected_text(&self) -> Vec<u32> {
        self.range_text(self.selection.range())
    }
    pub fn range_text(&self, mut range: CpvtWordRange) -> Vec<u32> {
        let mut result = Vec::new();
        if !self.vt.is_valid() {
            return result;
        }
        self.vt.update_word_place(&mut range.begin);
        self.vt.update_word_place(&mut range.end);
        let mut iter = self.vt.iterator();
        iter.set_at(range.begin);
        let mut old = range.begin;
        while iter.next_word() {
            let place = iter.word_place();
            if place > range.end {
                break;
            }
            if let Some(word) = iter.word() {
                result.push(u32::from(word.word));
            }
            if old.sec_index != place.sec_index {
                result.extend([13, 10]);
            }
            old = place;
        }
        result
    }
    pub fn set_text(&mut self, text: &[u32]) {
        if !self.vt.is_valid() {
            return;
        }
        self.vt.delete_words(self.whole_word_range());
        self.set_caret(self.vt.begin_word_place());
        self.do_insert_text(CpvtWordPlace::new(0, 0, -1), text, FxCharset::Default);
    }
    fn do_insert_text(
        &mut self,
        mut place: CpvtWordPlace,
        text: &[u32],
        charset: FxCharset,
    ) -> CpvtWordPlace {
        if !self.vt.is_valid() {
            return place;
        }
        let mut index = 0;
        while index < text.len() {
            let word = text[index] as u16;
            match word {
                13 => {
                    place = self.vt.insert_section(place);
                    if text.get(index + 1) == Some(&10) {
                        index += 1;
                    }
                }
                10 => place = self.vt.insert_section(place),
                _ => {
                    let word = if word == 9 { 32 } else { word };
                    let charset = self.charset_from_unicode(word, charset);
                    place = self.vt.insert_word(place, word, charset);
                }
            }
            index += 1;
        }
        place
    }
    fn charset_from_unicode(&self, word: u16, charset: FxCharset) -> FxCharset {
        self.font_map()
            .map_or(charset, |map| map.charset_from_unicode(word, charset))
    }
    pub fn get_pdf_word_string(&self, index: i32, mut word: u16, sub_word: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        let Some(map) = self.font_map() else {
            return bytes;
        };
        let Some(font) = map.get_pdf_font(index) else {
            return bytes;
        };
        if sub_word > 0 {
            word = sub_word;
        } else {
            let code = if unicode_compatible(&font) {
                font.char_code_from_unicode(u32::from(word))
            } else {
                map.char_code_from_unicode(index, word) as u32
            };
            if code > 0 && code != INVALID_CHAR_CODE {
                font.append_char(&mut bytes, code);
                return bytes;
            }
        }
        font.append_char(&mut bytes, u32::from(word));
        bytes
    }
    pub fn iterator(&self) -> EditIterator<'_, 'a> {
        EditIterator {
            inner: self.vt.iterator(),
            edit: self,
        }
    }
    pub fn paint(&mut self) {
        if !self.vt.is_valid() {
            return;
        }
        self.rearrange_all();
        self.scroll_to_caret();
        self.refresh();
        self.set_caret_origin();
        self.set_caret_info();
    }
    fn rearrange_all(&mut self) {
        self.vt.update_word_place(&mut self.caret);
        self.vt.rearrange_all();
        self.vt.update_word_place(&mut self.caret);
        self.set_scroll_info();
    }
    fn rearrange_part(&mut self, range: CpvtWordRange) {
        self.vt.update_word_place(&mut self.caret);
        self.vt.rearrange_part(range);
        self.vt.update_word_place(&mut self.caret);
        self.set_scroll_info();
    }
    fn paint_insert_text(&mut self, old: CpvtWordPlace, new: CpvtWordPlace) {
        self.rearrange_part(CpvtWordRange::new(old, new));
        self.scroll_to_caret();
        self.refresh();
        self.set_caret_origin();
        self.set_caret_info();
    }
    fn padding(&self) -> f32 {
        let diff = self.vt.plate_rect().height() - self.vt.content_rect().height();
        match self.alignment {
            1 => diff * 0.5,
            2 => diff,
            _ => 0.0,
        }
    }
    fn vt_to_edit(&self, point: CfxPointF) -> CfxPointF {
        let plate = self.vt.plate_rect();
        CfxPointF::new(
            point.x - (self.scroll_pos.x - plate.left),
            point.y - (self.scroll_pos.y + self.padding() - plate.top),
        )
    }
    fn edit_to_vt(&self, point: CfxPointF) -> CfxPointF {
        let plate = self.vt.plate_rect();
        CfxPointF::new(
            point.x + (self.scroll_pos.x - plate.left),
            point.y + (self.scroll_pos.y + self.padding() - plate.top),
        )
    }
    fn vt_to_edit_rect(&self, rect: CfxFloatRect) -> CfxFloatRect {
        let lb = self.vt_to_edit(CfxPointF::new(rect.left, rect.bottom));
        let rt = self.vt_to_edit(CfxPointF::new(rect.right, rect.top));
        CfxFloatRect::new(lb.x, lb.y, rt.x, rt.y)
    }
    pub fn search_word_place(&self, point: CfxPointF) -> CpvtWordPlace {
        if self.vt.is_valid() {
            self.vt.search_word_place(self.edit_to_vt(point))
        } else {
            CpvtWordPlace::default()
        }
    }
    pub fn visible_word_range(&self) -> CpvtWordRange {
        if self.enable_overflow {
            return self.whole_word_range();
        }
        if !self.vt.is_valid() {
            return CpvtWordRange::default();
        }
        let plate = self.vt.plate_rect();
        CpvtWordRange::new(
            self.search_word_place(CfxPointF::new(plate.left, plate.top)),
            self.search_word_place(CfxPointF::new(plate.right, plate.bottom)),
        )
    }
    fn notification(&self) -> Option<Rc<dyn EditNotify>> {
        if self.notify_flag {
            None
        } else {
            self.notify.as_ref().and_then(Weak::upgrade)
        }
    }
    fn set_scroll_info(&mut self) {
        let Some(notify) = self.notification() else {
            return;
        };
        let plate = self.vt.plate_rect();
        let content = self.vt.content_rect();
        self.notify_flag = true;
        notify.set_scroll_info(ScrollInfo {
            plate_width: plate.height(),
            content_min: content.bottom,
            content_max: content.top,
            small_step: plate.height() / 3.0,
            big_step: plate.height(),
        });
        self.notify_flag = false;
    }
    fn set_scroll_pos_x(&mut self, x: f32) {
        if self.enable_scroll && self.vt.is_valid() && !equal(self.scroll_pos.x, x) {
            self.scroll_pos.x = x;
            self.refresh();
        }
    }
    fn set_scroll_pos_y(&mut self, y: f32) {
        if self.enable_scroll && self.vt.is_valid() && !equal(self.scroll_pos.y, y) {
            self.scroll_pos.y = y;
            self.refresh();
            if let Some(notify) = self.notification() {
                self.notify_flag = true;
                notify.set_scroll_position(y);
                self.notify_flag = false;
            }
        }
    }
    pub fn set_scroll_pos(&mut self, point: CfxPointF) {
        self.set_scroll_pos_x(point.x);
        self.set_scroll_pos_y(point.y);
        self.set_scroll_limit();
        self.set_caret_info();
    }
    fn set_scroll_limit(&mut self) {
        if !self.vt.is_valid() {
            return;
        }
        let content = self.vt.content_rect();
        let plate = self.vt.plate_rect();
        if plate.width() > content.width() {
            self.set_scroll_pos_x(plate.left);
        } else if bigger(content.left, self.scroll_pos.x) {
            self.set_scroll_pos_x(content.left);
        } else if bigger(self.scroll_pos.x, content.right - plate.width()) {
            self.set_scroll_pos_x(content.right - plate.width());
        }
        if plate.height() > content.height() {
            self.set_scroll_pos_y(plate.top);
        } else if bigger(content.bottom + plate.height(), self.scroll_pos.y) {
            self.set_scroll_pos_y(content.bottom + plate.height());
        } else if bigger(self.scroll_pos.y, content.top) {
            self.set_scroll_pos_y(content.top);
        }
    }
    fn caret_points(&self) -> (CfxPointF, CfxPointF) {
        let mut iter = self.vt.iterator();
        iter.set_at(self.caret);
        if let Some(word) = iter.word() {
            return (
                CfxPointF::new(word.caret_x(), word.ascent_y()),
                CfxPointF::new(word.caret_x(), word.descent_y()),
            );
        }
        if let Some(line) = iter.line() {
            let x = iter.line_caret_x(&line);
            return (
                CfxPointF::new(x, line.point.y + line.ascent),
                CfxPointF::new(x, line.point.y + line.descent),
            );
        }
        (CfxPointF::default(), CfxPointF::default())
    }
    fn scroll_to_caret(&mut self) {
        self.set_scroll_limit();
        if !self.vt.is_valid() {
            return;
        }
        let (head, foot) = self.caret_points();
        let head_edit = self.vt_to_edit(head);
        let foot_edit = self.vt_to_edit(foot);
        let plate = self.vt.plate_rect();
        if !equal(plate.left, plate.right) {
            if bigger(plate.left, head_edit.x) || equal(head_edit.x, plate.left) {
                self.set_scroll_pos_x(head.x);
            } else if bigger(head_edit.x, plate.right) {
                self.set_scroll_pos_x(head.x - plate.width());
            }
        }
        if !equal(plate.top, plate.bottom) {
            if bigger(plate.bottom, foot_edit.y) || equal(foot_edit.y, plate.bottom) {
                if bigger(plate.top, head_edit.y) {
                    self.set_scroll_pos_y(foot.y + plate.height());
                }
            } else if bigger(head_edit.y, plate.top) && bigger(foot_edit.y, plate.bottom) {
                self.set_scroll_pos_y(head.y);
            }
        }
    }
    fn set_caret_origin(&mut self) {
        if !self.vt.is_valid() {
            return;
        }
        let mut iter = self.vt.iterator();
        iter.set_at(self.caret);
        if let Some(word) = iter.word() {
            self.caret_point = CfxPointF::new(word.caret_x(), word.location.y);
        } else if let Some(line) = iter.line() {
            self.caret_point = line.point;
        }
    }
    fn set_caret_info(&mut self) {
        let Some(notify) = self.notification() else {
            return;
        };
        let (head, foot) = self.caret_points();
        self.notify_flag = true;
        notify.set_caret(
            self.selection.is_empty(),
            self.vt_to_edit(head),
            self.vt_to_edit(foot),
        );
        self.notify_flag = false;
    }
    fn invalidate(&mut self, rect: CfxFloatRect) {
        if let Some(notify) = self.notification() {
            self.notify_flag = true;
            if !notify.invalidate_rect(rect) {
                self.notify = None;
            }
            self.notify_flag = false;
        }
    }
    fn refresh(&mut self) {
        if !self.enable_refresh || !self.vt.is_valid() {
            return;
        }
        self.refresh.begin();
        let mut range = self.visible_word_range();
        self.vt.update_word_place(&mut range.begin);
        self.vt.update_word_place(&mut range.end);
        let mut iter = self.vt.iterator();
        iter.set_at(range.begin);
        loop {
            let Some(line) = iter.line() else {
                break;
            };
            if line.line_place.line_cmp(range.end) > 0 {
                break;
            }
            let rect = CfxFloatRect::new(
                line.point.x,
                line.point.y + line.descent,
                line.point.x + line.width,
                line.point.y + line.ascent,
            );
            self.refresh.new_lines.push(self.vt_to_edit_rect(rect));
            if !iter.next_line() {
                break;
            }
        }
        for rect in self.refresh.rects() {
            self.invalidate(rect);
        }
    }
    pub fn refresh_word_range(&mut self, mut range: CpvtWordRange) {
        self.vt.update_word_place(&mut range.begin);
        self.vt.update_word_place(&mut range.end);
        // Collect scalar rectangles before notifying, so no iterator borrow is
        // held while an observer can destroy its receiving window.
        let mut rects = Vec::new();
        let mut iter = self.vt.iterator();
        iter.set_at(range.begin);
        while iter.next_word() {
            let place = iter.word_place();
            if place > range.end {
                break;
            }
            let (Some(word), Some(line)) = (iter.word(), iter.line()) else {
                continue;
            };
            let rect = if place.line_cmp(range.begin) == 0 || place.line_cmp(range.end) == 0 {
                CfxFloatRect::new(
                    word.location.x,
                    line.point.y + line.descent,
                    word.location.x + word.width,
                    line.point.y + line.ascent,
                )
            } else {
                CfxFloatRect::new(
                    line.point.x,
                    line.point.y + line.descent,
                    line.point.x + line.width,
                    line.point.y + line.ascent,
                )
            };
            rects.push(self.vt_to_edit_rect(rect));
            if place.line_cmp(range.begin) != 0 && place.line_cmp(range.end) != 0 {
                iter.next_line();
            }
        }
        for rect in rects {
            self.invalidate(rect);
        }
    }
    pub fn on_mouse_down(&mut self, point: CfxPointF, _shift: bool, _ctrl: bool) {
        if !self.vt.is_valid() {
            return;
        }
        self.select_none();
        self.set_caret(self.search_word_place(point));
        self.selection.set(self.caret, self.caret);
        self.scroll_to_caret();
        self.set_caret_origin();
        self.set_caret_info();
    }
    pub fn on_mouse_move(&mut self, point: CfxPointF, _shift: bool, _ctrl: bool) {
        if !self.vt.is_valid() {
            return;
        }
        self.set_caret(self.search_word_place(point));
        if self.caret == self.old_caret {
            return;
        }
        self.selection.end = self.caret;
        self.scroll_to_caret();
        self.refresh();
        self.set_caret_origin();
        self.set_caret_info();
    }
    fn move_caret(&mut self, place: CpvtWordPlace, shift: bool, origin: bool) {
        self.set_caret(place);
        if shift {
            if self.selection.is_empty() {
                self.selection.set(self.old_caret, self.caret);
            } else {
                self.selection.end = self.caret;
            }
        } else {
            self.select_none();
        }
        self.scroll_to_caret();
        if shift {
            self.refresh();
        }
        if origin {
            self.set_caret_origin();
        }
        self.set_caret_info();
    }
    pub fn on_vk_up(&mut self, shift: bool) {
        if self.vt.is_valid() {
            self.move_caret(
                self.vt.up_word_place(self.caret, self.caret_point),
                shift,
                false,
            );
        }
    }
    pub fn on_vk_down(&mut self, shift: bool) {
        if self.vt.is_valid() {
            self.move_caret(
                self.vt.down_word_place(self.caret, self.caret_point),
                shift,
                false,
            );
        }
    }
    pub fn on_vk_left(&mut self, shift: bool) {
        if !self.vt.is_valid() {
            return;
        }
        if !shift && self.is_selected() {
            self.move_caret(self.selection.range().begin, false, false);
            return;
        }
        let mut place = self.caret;
        if place == self.vt.line_begin_place(place) && place != self.vt.section_begin_place(place) {
            place = self.vt.prev_word_place(place);
        }
        place = self.vt.prev_word_place(place);
        self.move_caret(place, shift, !shift);
    }
    pub fn on_vk_right(&mut self, shift: bool) {
        if !self.vt.is_valid() {
            return;
        }
        if !shift && self.is_selected() {
            self.move_caret(self.selection.range().end, false, false);
            return;
        }
        let mut place = self.vt.next_word_place(self.caret);
        if place == self.vt.line_end_place(place) && place != self.vt.section_end_place(place) {
            place = self.vt.next_word_place(place);
        }
        self.move_caret(place, shift, !shift);
    }
    pub fn on_vk_home(&mut self, shift: bool, ctrl: bool) {
        if !self.vt.is_valid() {
            return;
        }
        if !shift && self.is_selected() {
            self.move_caret(self.selection.range().begin, false, false);
            return;
        }
        let place = if ctrl {
            self.vt.begin_word_place()
        } else {
            self.vt.line_begin_place(self.caret)
        };
        self.move_caret(place, shift, !shift);
    }
    pub fn on_vk_end(&mut self, shift: bool, ctrl: bool) {
        if !self.vt.is_valid() {
            return;
        }
        if !shift && self.is_selected() {
            self.move_caret(self.selection.range().end, false, false);
            return;
        }
        let place = if ctrl {
            self.vt.end_word_place()
        } else {
            self.vt.line_end_place(self.caret)
        };
        self.move_caret(place, shift, !shift);
    }
    pub fn is_text_overflow(&self) -> bool {
        if self.enable_scroll || self.enable_overflow {
            return false;
        }
        let content = self.vt.content_rect();
        let plate = self.vt.plate_rect();
        (self.vt.is_multi_line()
            && self.total_lines() > 1
            && bigger(content.height(), plate.height()))
            || bigger(content.width(), plate.width())
    }
    pub fn is_text_full(&self) -> bool {
        self.is_text_overflow()
            || (self.vt.limit_char() > 0 && self.vt.total_words() >= self.vt.limit_char())
            || (self.vt.char_array() > 0 && self.vt.total_words() >= self.vt.char_array())
    }
    fn total_lines(&self) -> i32 {
        let mut lines = 1;
        let mut iter = self.vt.iterator();
        iter.set_at_index(0);
        while iter.next_line() {
            lines += 1;
        }
        lines
    }
    pub fn insert_word(&mut self, word: u16, charset: FxCharset) -> bool {
        self.insert_word_impl(word, charset, true)
    }
    fn insert_word_impl(&mut self, word: u16, charset: FxCharset, add_undo: bool) -> bool {
        if self.is_text_overflow() || !self.vt.is_valid() {
            return false;
        }
        self.vt.update_word_place(&mut self.caret);
        let charset_actual = self.charset_from_unicode(word, charset);
        let new = self.vt.insert_word(self.caret, word, charset_actual);
        self.set_caret(new);
        self.selection.set(self.caret, self.caret);
        if self.caret == self.old_caret {
            return false;
        }
        if add_undo && self.enable_undo {
            self.undo.add(UndoItem::InsertWord {
                old: self.old_caret,
                new: self.caret,
                word,
                charset,
            });
        }
        self.paint_insert_text(self.old_caret, self.caret);
        true
    }
    pub fn insert_return(&mut self) {
        self.insert_return_impl(true);
    }
    fn insert_return_impl(&mut self, add_undo: bool) {
        if self.is_text_overflow() || !self.vt.is_valid() {
            return;
        }
        self.vt.update_word_place(&mut self.caret);
        let new = self.vt.insert_section(self.caret);
        self.set_caret(new);
        self.selection.set(self.caret, self.caret);
        if self.caret == self.old_caret {
            return;
        }
        if add_undo && self.enable_undo {
            self.undo.add(UndoItem::InsertReturn {
                old: self.old_caret,
                new: self.caret,
            });
        }
        self.paint_insert_text(self.old_caret, self.caret);
    }
    pub fn backspace(&mut self) {
        self.backspace_impl(true);
    }
    fn backspace_impl(&mut self, add_undo: bool) {
        if !self.vt.is_valid() || self.caret == self.vt.begin_word_place() {
            return;
        }
        let mut iter = self.vt.iterator();
        iter.set_at(self.caret);
        let word = iter.word().unwrap_or_default();
        self.vt.update_word_place(&mut self.caret);
        let new = self.vt.backspace_word(self.caret);
        self.set_caret(new);
        self.selection.set(self.caret, self.caret);
        if self.caret == self.old_caret {
            return;
        }
        if add_undo && self.enable_undo {
            self.undo.add(UndoItem::Backspace {
                old: self.old_caret,
                new: self.caret,
                word: word.word,
                charset: word.charset,
            });
        }
        self.paint_insert_text(self.old_caret, self.caret);
    }
    pub fn delete(&mut self) -> bool {
        self.delete_impl(true)
    }
    fn delete_impl(&mut self, add_undo: bool) -> bool {
        if !self.vt.is_valid() || self.caret == self.vt.end_word_place() {
            return false;
        }
        let mut iter = self.vt.iterator();
        iter.set_at(self.vt.next_word_place(self.caret));
        let word = iter.word().unwrap_or_default();
        self.vt.update_word_place(&mut self.caret);
        let section_end = self.caret == self.vt.section_end_place(self.caret);
        let new = self.vt.delete_word(self.caret);
        self.set_caret(new);
        self.selection.set(self.caret, self.caret);
        if add_undo && self.enable_undo {
            self.undo.add(UndoItem::Delete {
                old: self.old_caret,
                new: self.caret,
                word: word.word,
                charset: word.charset,
                section_end,
            });
        }
        self.paint_insert_text(self.old_caret, self.caret);
        true
    }
    pub fn clear_selection(&mut self) -> bool {
        self.clear_selection_impl(true)
    }
    fn clear_selection_impl(&mut self, add_undo: bool) -> bool {
        if !self.vt.is_valid() || self.selection.is_empty() {
            return false;
        }
        let range = self.selection.range();
        if add_undo && self.enable_undo {
            let text = self.selected_text();
            self.undo.add(UndoItem::Clear { range, text });
        }
        self.select_none();
        let new = self.vt.delete_words(range);
        self.set_caret(new);
        self.selection.set(self.caret, self.caret);
        self.paint_insert_text(range.begin, range.end);
        true
    }
    pub fn insert_text(&mut self, text: &[u32], charset: FxCharset) {
        self.insert_text_impl(text, charset, true);
    }
    fn insert_text_impl(&mut self, text: &[u32], charset: FxCharset, add_undo: bool) {
        if self.is_text_overflow() {
            return;
        }
        self.vt.update_word_place(&mut self.caret);
        let new = self.do_insert_text(self.caret, text, charset);
        self.set_caret(new);
        self.selection.set(self.caret, self.caret);
        if self.caret == self.old_caret {
            return;
        }
        if add_undo && self.enable_undo {
            self.undo.add(UndoItem::InsertText {
                old: self.old_caret,
                new: self.caret,
                text: text.to_vec(),
                charset,
            });
        }
        self.paint_insert_text(self.old_caret, self.caret);
    }
    pub fn replace_selection(&mut self, text: &[u32]) {
        self.replace_selection_impl(text, false);
    }
    pub fn replace_and_keep_selection(&mut self, text: &[u32]) {
        self.replace_selection_impl(text, true);
    }
    fn replace_selection_impl(&mut self, text: &[u32], keep: bool) {
        if self.enable_undo {
            self.undo.add(UndoItem::Sentinel);
        }
        self.clear_selection();
        let before = self.caret;
        self.insert_text(text, FxCharset::Default);
        if keep {
            self.selection.set(before, self.caret);
        }
        if self.enable_undo {
            self.undo.add(UndoItem::Sentinel);
        }
    }
    pub fn type_char(&mut self, word: u16, charset: FxCharset) {
        let selected = self.is_selected();
        if word == 8 {
            if selected {
                self.clear_selection();
            } else {
                self.backspace();
            }
            return;
        }
        if selected {
            if self.enable_undo {
                self.undo.add(UndoItem::Sentinel);
            }
            self.clear_selection();
        }
        if word == 13 {
            self.insert_return();
        } else {
            self.insert_word(word, charset);
        }
        if selected && self.enable_undo {
            self.undo.add(UndoItem::Sentinel);
        }
    }
    pub fn can_undo(&self) -> bool {
        self.enable_undo && self.undo.position > 0
    }
    pub fn can_redo(&self) -> bool {
        self.enable_undo && self.undo.position < self.undo.items.len()
    }
    pub fn set_max_undo_items_for_test(&mut self, items: usize) {
        assert!(items >= 4);
        self.undo.maximum = items;
    }
    pub fn undo(&mut self) -> bool {
        self.run_undo(false)
    }
    pub fn redo(&mut self) -> bool {
        self.run_undo(true)
    }
    fn run_undo(&mut self, redo: bool) -> bool {
        if !self.enable_undo
            || (if redo {
                !self.can_redo()
            } else {
                !self.can_undo()
            })
        {
            return false;
        }
        // Detach the command queue while executing its commands: no stored
        // backpointers, RefCell borrows, or copying document text are necessary.
        let mut stack = std::mem::take(&mut self.undo);
        stack.working = true;
        self.undo.working = true;
        let mut first = true;
        loop {
            let available = if redo {
                stack.position < stack.items.len()
            } else {
                stack.position > 0
            };
            if !available {
                break;
            }
            if !redo {
                stack.position -= 1;
            }
            let item = &stack.items[stack.position];
            if redo {
                stack.position += 1;
            }
            self.apply_undo(item, redo);
            if first {
                first = false;
                if !item.is_sentinel() {
                    break;
                }
            } else if item.is_sentinel() {
                break;
            }
        }
        stack.working = false;
        self.undo = stack;
        true
    }
    fn apply_undo(&mut self, item: &UndoItem, redo: bool) {
        match item {
            UndoItem::Sentinel => {}
            UndoItem::InsertWord {
                old,
                new,
                word,
                charset,
            } => {
                self.select_none();
                self.set_caret(if redo { *old } else { *new });
                if redo {
                    self.insert_word_impl(*word, *charset, false);
                } else {
                    self.backspace_impl(false);
                }
            }
            UndoItem::InsertReturn { old, new } => {
                self.select_none();
                self.set_caret(if redo { *old } else { *new });
                if redo {
                    self.insert_return_impl(false);
                } else {
                    self.backspace_impl(false);
                }
            }
            UndoItem::Backspace {
                old,
                new,
                word,
                charset,
            } => {
                self.select_none();
                self.set_caret(if redo { *old } else { *new });
                if redo {
                    self.backspace_impl(false);
                } else if old.sec_index != new.sec_index {
                    self.insert_return_impl(false);
                } else {
                    self.insert_word_impl(*word, *charset, false);
                }
            }
            UndoItem::Delete {
                old,
                new,
                word,
                charset,
                section_end,
            } => {
                self.select_none();
                self.set_caret(if redo { *old } else { *new });
                if redo {
                    self.delete_impl(false);
                } else if *section_end {
                    self.insert_return_impl(false);
                } else {
                    self.insert_word_impl(*word, *charset, false);
                }
            }
            UndoItem::Clear { range, text } => {
                self.select_none();
                if redo {
                    self.set_selection_places(range.begin, range.end);
                    self.clear_selection_impl(false);
                } else {
                    self.set_caret(range.begin);
                    self.insert_text_impl(text, FxCharset::Default, false);
                    self.set_selection_places(range.begin, range.end);
                }
            }
            UndoItem::InsertText {
                old,
                new,
                text,
                charset,
            } => {
                self.select_none();
                if redo {
                    self.set_caret(*old);
                    self.insert_text_impl(text, *charset, false);
                } else {
                    self.set_selection_places(*old, *new);
                    self.clear_selection_impl(false);
                }
            }
        }
    }
}

pub struct EditIterator<'e, 'p> {
    inner: VtIterator<'e, 'p>,
    edit: &'e CpwlEditImpl<'p>,
}
impl EditIterator<'_, '_> {
    pub fn set_at_index(&mut self, index: i32) {
        self.inner.set_at_index(index);
    }
    pub fn set_at(&mut self, place: CpvtWordPlace) {
        self.inner.set_at(place);
    }
    pub fn word_place(&self) -> CpvtWordPlace {
        self.inner.word_place()
    }
    pub fn next_word(&mut self) -> bool {
        self.inner.next_word()
    }
    pub fn next_line(&mut self) -> bool {
        self.inner.next_line()
    }
    pub fn word(&self) -> Option<CpvtWord> {
        self.inner.word().map(|mut word| {
            word.location = self.edit.vt_to_edit(word.location);
            word
        })
    }
    pub fn line(&self) -> Option<CpvtLine> {
        self.inner.line().map(|mut line| {
            line.point = self.edit.vt_to_edit(line.point);
            line
        })
    }
    pub fn line_caret_x(&self, line: &CpvtLine) -> f32 {
        // The native iterator forwards to VT; replace only the x-coordinate
        // component of the caller's line with its original layout coordinate.
        let mut raw = *line;
        raw.point = self.edit.edit_to_vt(raw.point);
        self.inner.line_caret_x(&raw) - (self.edit.scroll_pos.x - self.edit.vt.plate_rect().left)
    }
}

/// Owned rendering commands permit releasing an enclosing editor RefCell before
/// host selection callbacks. Fonts and existing resources remain shared.
pub struct EditDrawPlan {
    clip: CfxFloatRect,
    matrix: crate::fxcrt::fx_coordinates::CfxMatrix,
    operations: Vec<EditDrawOp>,
}
enum EditDrawOp {
    Selection(CfxFloatRect),
    Background(CfxFloatRect),
    Text {
        point: CfxPointF,
        font: CpdfFont,
        size: f32,
        bytes: Vec<u8>,
        fill: u32,
    },
}
impl EditDrawPlan {
    pub fn draw(
        &self,
        draw: &mut dyn super::pwl_draw::PwlDraw,
        filler: Option<&dyn super::ipwl_fillernotify::FillerNotify>,
        attached: Option<&dyn super::ipwl_fillernotify::PerWindowData>,
    ) {
        self.draw_observed(draw, filler, attached, &|| true);
    }
    pub fn draw_observed(
        &self,
        draw: &mut dyn super::pwl_draw::PwlDraw,
        filler: Option<&dyn super::ipwl_fillernotify::FillerNotify>,
        attached: Option<&dyn super::ipwl_fillernotify::PerWindowData>,
        is_alive: &dyn Fn() -> bool,
    ) {
        draw.save_state();
        if !self.clip.is_empty() {
            draw.set_clip_rect(self.clip, self.matrix);
        }
        for operation in &self.operations {
            if !is_alive() {
                break;
            }
            match operation {
                EditDrawOp::Selection(rect) => {
                    if let Some(filler) = filler {
                        filler.output_selected_rect(attached, *rect);
                    }
                }
                EditDrawOp::Background(rect) => {
                    draw.fill_rect(self.matrix, *rect, 0xff003371);
                }
                EditDrawOp::Text {
                    point,
                    font,
                    size,
                    bytes,
                    fill,
                } => {
                    draw.draw_text(*point, font, *size, self.matrix, bytes, *fill);
                }
            }
        }
        draw.restore_state();
    }
}
impl CpwlEditImpl<'_> {
    /// `CPWL_EditImpl::DrawEdit`: continuous runs group by line, font and color;
    /// comb and RTL words are emitted separately with their layout positions.
    pub fn draw_edit_plan(
        &self,
        matrix: crate::fxcrt::fx_coordinates::CfxMatrix,
        text_fill: u32,
        clip: CfxFloatRect,
        offset: CfxPointF,
        range: Option<&CpvtWordRange>,
        host_selection: bool,
    ) -> EditDrawPlan {
        let mut plan = EditDrawPlan {
            clip,
            matrix,
            operations: Vec::new(),
        };
        let Some(map) = self.font_map() else {
            return plan;
        };
        let continuous = self.char_array() == 0;
        let select = self.select_word_range();
        let mut current_fill = text_fill;
        let mut old_fill = current_fill;
        let mut font_index = -1;
        let mut origin = CfxPointF::default();
        let mut buffer = Vec::new();
        let mut old_place = CpvtWordPlace::default();
        let mut iterator = self.iterator();
        if let Some(range) = range {
            iterator.set_at(range.begin)
        } else {
            iterator.set_at_index(0)
        }
        let flush = |plan: &mut EditDrawPlan,
                     buffer: &mut Vec<u8>,
                     index: i32,
                     origin: CfxPointF,
                     fill: u32| {
            if buffer.is_empty() {
                return;
            }
            let bytes = std::mem::take(buffer);
            if let Some(font) = map.get_pdf_font(index) {
                plan.operations.push(EditDrawOp::Text {
                    point: CfxPointF::new(origin.x + offset.x, origin.y + offset.y),
                    font,
                    size: self.font_size(),
                    bytes,
                    fill,
                });
            }
        };
        while iterator.next_word() {
            let place = iterator.word_place();
            if range.is_some_and(|range| place > range.end) {
                break;
            }
            let selected = !select.is_empty() && place > select.begin && place <= select.end;
            if !select.is_empty() {
                current_fill = if selected { 0xffffffff } else { text_fill };
            }
            if host_selection {
                current_fill = text_fill;
                old_fill = current_fill;
            }
            let Some(word) = iterator.word() else {
                continue;
            };
            if selected {
                if let Some(line) = iterator.line() {
                    let mut rect = CfxFloatRect::new(
                        word.location.x,
                        line.point.y + line.descent,
                        word.location.x + word.width,
                        line.point.y + line.ascent,
                    );
                    if host_selection {
                        rect.intersect(clip);
                        plan.operations.push(EditDrawOp::Selection(rect));
                    } else {
                        plan.operations.push(EditDrawOp::Background(rect));
                    }
                }
            }
            let index = word.font_index;
            if continuous && !word.is_rtl {
                if place.line_cmp(old_place) != 0
                    || index != font_index
                    || old_fill != current_fill
                    || buffer.is_empty()
                {
                    flush(&mut plan, &mut buffer, font_index, origin, old_fill);
                    font_index = index;
                    origin = word.location;
                    old_fill = current_fill;
                }
                buffer.extend(self.get_pdf_word_string(index, word.word, self.password_char()));
            } else {
                flush(&mut plan, &mut buffer, font_index, origin, old_fill);
                if let Some(font) = map.get_pdf_font(index) {
                    plan.operations.push(EditDrawOp::Text {
                        point: CfxPointF::new(
                            word.location.x + offset.x,
                            word.location.y + offset.y,
                        ),
                        font,
                        size: self.font_size(),
                        bytes: self.get_pdf_word_string(index, word.word, self.password_char()),
                        fill: current_fill,
                    });
                }
            }
            old_place = place;
        }
        flush(&mut plan, &mut buffer, font_index, origin, old_fill);
        plan
    }
    pub fn draw_edit(
        &self,
        draw: &mut dyn super::pwl_draw::PwlDraw,
        matrix: crate::fxcrt::fx_coordinates::CfxMatrix,
        text_fill: u32,
        clip: CfxFloatRect,
        offset: CfxPointF,
        range: Option<&CpvtWordRange>,
        filler: Option<&dyn super::ipwl_fillernotify::FillerNotify>,
        attached: Option<&dyn super::ipwl_fillernotify::PerWindowData>,
    ) {
        self.draw_edit_plan(
            matrix,
            text_fill,
            clip,
            offset,
            range,
            filler.is_some_and(|f| f.is_selection_implemented()),
        )
        .draw(draw, filler, attached)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct StubProvider;
    impl Provider for StubProvider {
        fn char_width(&self, _: i32, _: u16) -> i32 {
            500
        }
        fn type_ascent(&self, _: i32) -> i32 {
            800
        }
        fn type_descent(&self, _: i32) -> i32 {
            -200
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
    fn edit() -> CpwlEditImpl<'static> {
        static PROVIDER: StubProvider = StubProvider;
        let mut edit = CpwlEditImpl::new(Some(&PROVIDER));
        edit.set_plate_rect(CfxFloatRect::new(0.0, 0.0, 200.0, 100.0));
        edit.set_font_size(10.0);
        edit.initialize();
        edit
    }
    fn text(edit: &CpwlEditImpl<'_>) -> String {
        edit.text().into_iter().filter_map(char::from_u32).collect()
    }
    fn selected(edit: &CpwlEditImpl<'_>) -> String {
        edit.selected_text()
            .into_iter()
            .filter_map(char::from_u32)
            .collect()
    }
    fn type_text(edit: &mut CpwlEditImpl<'_>, text: &str) {
        for ch in text.chars() {
            edit.type_char(ch as u16, FxCharset::Default);
        }
    }

    // These exercise the same engine operations as CPWLEditEmbedderTest's
    // ReplaceAndKeepSelection, ReplaceSelectionUndoRedo and queue-limit cases;
    // FORM host callbacks are intentionally outside this engine-level fixture.
    #[test]
    fn replace_and_keep_selection() {
        let mut edit = edit();
        type_text(&mut edit, "ABCDEFGHIJ");
        edit.set_selection(1, 3);
        edit.replace_and_keep_selection(&wide("xyz"));
        assert_eq!(text(&edit), "AxyzDEFGHIJ");
        assert_eq!(selected(&edit), "xyz");
        assert_eq!(edit.selection(), (1, 4));
        edit.set_selection(4, 1);
        edit.replace_and_keep_selection(&wide("12"));
        assert_eq!(text(&edit), "A12DEFGHIJ");
        assert_eq!(selected(&edit), "12");
        assert_eq!(edit.selection(), (1, 3));
    }
    #[test]
    fn replace_selection_undo_redo() {
        let mut edit = edit();
        type_text(&mut edit, "ABCDEFGHIJKLM");
        edit.set_selection(0, -1);
        edit.replace_selection(&wide("123"));
        assert_eq!(text(&edit), "123");
        assert!(edit.undo());
        assert_eq!(selected(&edit), "ABCDEFGHIJKLM");
        assert!(edit.redo());
        assert_eq!(text(&edit), "123");
        assert!(!edit.can_redo());
    }
    #[test]
    fn replace_selection_undo_queue_limit() {
        let mut edit = edit();
        edit.set_max_undo_items_for_test(4);
        type_text(&mut edit, "ABCDEFGHIJKLM");
        edit.set_selection(0, -1);
        edit.replace_selection(&wide("123"));
        type_text(&mut edit, "AB");
        assert_eq!(text(&edit), "123AB");
        assert!(edit.undo());
        assert_eq!(text(&edit), "123A");
        assert!(edit.undo());
        assert_eq!(text(&edit), "123");
        assert!(!edit.can_undo());
    }
    #[test]
    fn replace_selection_redo_queue_limit() {
        let mut edit = edit();
        edit.set_max_undo_items_for_test(4);
        type_text(&mut edit, "ABCDEFGHIJKLM");
        edit.set_selection(0, -1);
        edit.replace_selection(&wide("123"));
        assert!(edit.undo());
        assert_eq!(selected(&edit), "ABCDEFGHIJKLM");
        edit.set_selection(-1, 0);
        assert!(edit.can_redo());
        type_text(&mut edit, "A");
        assert_eq!(text(&edit), "ABCDEFGHIJKLMA");
        assert!(!edit.can_redo());
        assert!(edit.undo());
        assert_eq!(text(&edit), "ABCDEFGHIJKLM");
        assert!(!edit.undo());
    }
    #[test]
    fn backspace_and_delete_undo_redo() {
        let mut edit = edit();
        type_text(&mut edit, "ABC");
        edit.type_char(8, FxCharset::Default);
        assert_eq!(text(&edit), "AB");
        assert!(edit.undo());
        assert_eq!(text(&edit), "ABC");
        assert!(edit.redo());
        assert_eq!(text(&edit), "AB");
        edit.on_vk_home(false, false);
        assert!(edit.delete());
        assert_eq!(text(&edit), "B");
        assert!(edit.undo());
        assert_eq!(text(&edit), "AB");
        assert!(edit.redo());
        assert_eq!(text(&edit), "B");
    }
    #[test]
    fn typing_and_backspace_over_selection_undo_redo() {
        let mut edit = edit();
        type_text(&mut edit, "ABC");
        edit.select_all();
        edit.type_char(b'Z' as u16, FxCharset::Default);
        assert_eq!(text(&edit), "Z");
        assert!(!edit.is_selected());
        assert!(edit.undo());
        assert_eq!(selected(&edit), "ABC");
        assert!(edit.redo());
        assert_eq!(text(&edit), "Z");
        edit.select_all();
        edit.type_char(8, FxCharset::Default);
        assert_eq!(text(&edit), "");
        assert!(edit.undo());
        assert_eq!(selected(&edit), "Z");
        assert!(edit.redo());
        assert_eq!(text(&edit), "");
    }
    #[test]
    fn multiline_return_and_join_undo_redo() {
        let mut edit = edit();
        edit.set_multi_line(true);
        type_text(&mut edit, "ABC");
        edit.insert_return();
        assert_eq!(text(&edit), "ABC\r\n");
        assert!(edit.undo());
        assert_eq!(text(&edit), "ABC");
        assert!(edit.redo());
        assert_eq!(text(&edit), "ABC\r\n");
        type_text(&mut edit, "DEF");
        edit.set_selection(4, 4);
        edit.backspace();
        assert_eq!(text(&edit), "ABCDEF");
        assert!(edit.undo());
        assert_eq!(text(&edit), "ABC\r\nDEF");
        assert!(edit.redo());
        assert_eq!(text(&edit), "ABCDEF");
    }
    #[test]
    fn multiline_range_deletion_preserves_suffix_and_undo() {
        let mut edit = edit();
        edit.set_multi_line(true);
        edit.set_text(&wide("abc\r\ndef\r\nghi"));
        edit.paint();
        edit.set_selection(1, 9);
        assert_eq!(selected(&edit), "bc\r\ndef\r\ng");
        edit.clear_selection();
        assert_eq!(text(&edit), "ahi");
        assert!(edit.undo());
        assert_eq!(text(&edit), "abc\r\ndef\r\nghi");
        assert_eq!(selected(&edit), "bc\r\ndef\r\ng");
    }
    #[test]
    fn character_limit_and_single_line_linebreaks() {
        for input in [
            "Foo\r",
            "Foo\n",
            "Foo\r\n",
            "Foo\n\r",
            "Foo\rBar",
            "Foo\nBar",
            "Foo\r\nBar",
            "Foo\n\rBar",
        ] {
            let mut edit = edit();
            edit.set_text(&wide(input));
            edit.paint();
            assert_eq!(
                text(&edit),
                if input.ends_with("Bar") {
                    "FooBar"
                } else {
                    "Foo"
                }
            );
        }
        let mut edit = edit();
        edit.set_limit_char(5);
        edit.insert_text(&wide("Hippopotamus"), FxCharset::Default);
        assert_eq!(text(&edit), "Hippo");
        assert!(edit.is_text_full());
        edit.set_selection(1, 3);
        edit.replace_selection(&wide("XYZ"));
        assert_eq!(text(&edit), "HXYpo");
    }
    #[test]
    fn mouse_selection_navigation_and_vertical_alignment() {
        let mut edit = edit();
        edit.set_text(&wide("abcd"));
        edit.paint();
        edit.on_mouse_down(CfxPointF::new(0.0, 95.0), false, false);
        edit.on_mouse_move(CfxPointF::new(10.1, 95.0), false, false);
        assert_eq!(selected(&edit), "ab");
        edit.on_vk_right(false);
        assert_eq!(edit.caret(), 2);
        edit.on_vk_right(true);
        assert_eq!(selected(&edit), "c");
        edit.set_alignment_v(1);
        edit.paint();
        let mut iter = edit.iterator();
        iter.set_at_index(1);
        let word = iter.word().unwrap();
        assert!((word.location.y - 47.0).abs() < 0.001);
    }
    #[test]
    fn wrapped_line_and_section_navigation() {
        let mut edit = edit();
        edit.set_multi_line(true);
        edit.set_auto_return(true);
        edit.set_plate_rect(CfxFloatRect::new(0.0, 0.0, 15.0, 100.0));
        edit.set_text(&wide("abcdef\r\ngh"));
        edit.paint();
        edit.on_vk_right(false);
        edit.on_vk_right(false);
        edit.on_vk_down(false);
        assert_eq!(edit.caret(), 5);
        edit.on_vk_down(false);
        assert_eq!(edit.caret(), 9);
        edit.on_vk_up(false);
        assert_eq!(edit.caret(), 5);
        edit.on_vk_home(false, false);
        assert_eq!(edit.caret(), 3);
        edit.on_vk_left(false);
        assert_eq!(edit.caret(), 2);
        edit.on_vk_end(false, true);
        assert_eq!(edit.caret(), 9);
    }
}
