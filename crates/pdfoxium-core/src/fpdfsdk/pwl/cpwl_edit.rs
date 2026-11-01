// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fpdfsdk/pwl/cpwl_edit.{h,cpp}`: editable window, event filtering and caret.
//! Scalar engine notifications are delivered after the editor borrow ends;
//! callbacks may mutate or logically destroy the window safely.
use super::cpwl_caret::CpwlCaret;
use super::cpwl_edit_impl::{CpwlEditImpl, EditNotify, OwnedEditFontMapProvider, ScrollInfo};
use super::cpwl_scroll_bar::{CpwlScrollBar, PwlScrollInfo};
use super::cpwl_wnd::*;
use super::ipwl_fillernotify::{CursorStyle, PerWindowData};
use super::pwl_draw::PwlDraw;
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfdoc::cpvt_variabletext::Provider;
use crate::fpdfdoc::cpvt_wordrange::CpvtWordRange;
use crate::fpdfsdk::cpdfsdk_baannot::BorderStyle;
use crate::fpdfsdk::fpdf_fwlevent::*;
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::fx_codepage::FxCharset;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions;
use crate::fxge::cfx_graphstatedata::CfxGraphStateData;
use crate::fxge::cfx_path::CfxPath;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

enum EditEvent {
    ScrollInfo(ScrollInfo),
    ScrollPosition(f32),
    Caret(bool, CfxPointF, CfxPointF),
    Invalidate(CfxFloatRect),
}
#[derive(Default)]
struct QueuedNotify {
    events: RefCell<VecDeque<EditEvent>>,
}
impl EditNotify for QueuedNotify {
    fn set_scroll_info(&self, info: ScrollInfo) {
        self.events
            .borrow_mut()
            .push_back(EditEvent::ScrollInfo(info));
    }
    fn set_scroll_position(&self, pos: f32) {
        self.events
            .borrow_mut()
            .push_back(EditEvent::ScrollPosition(pos));
    }
    fn set_caret(&self, visible: bool, head: CfxPointF, foot: CfxPointF) {
        self.events
            .borrow_mut()
            .push_back(EditEvent::Caret(visible, head, foot));
    }
    fn invalidate_rect(&self, rect: CfxFloatRect) -> bool {
        self.events
            .borrow_mut()
            .push_back(EditEvent::Invalidate(rect));
        true
    }
}
pub struct CpwlEdit {
    wnd: CpwlWnd,
    edit: RefCell<CpwlEditImpl<'static>>,
    queue: Rc<QueuedNotify>,
    draining: Cell<bool>,
    mouse_down: Cell<bool>,
    focus: Cell<bool>,
    char_set: Cell<FxCharset>,
    old_window_rect: Cell<CfxFloatRect>,
    caret: RefCell<Option<Weak<CpwlCaret>>>,
}
impl CpwlEdit {
    pub fn new(mut cp: CreateParams, attached: Option<Box<dyn PerWindowData>>) -> Rc<Self> {
        cp.cursor_type = CursorStyle::VBeam;
        let provider = cp
            .font_map
            .clone()
            .map(|font_map| Rc::new(OwnedEditFontMapProvider { font_map }) as Rc<dyn Provider>);
        let queue = Rc::new(QueuedNotify::default());
        let notify: Rc<dyn EditNotify> = queue.clone();
        let mut engine = CpwlEditImpl::new_owned(provider);
        engine.set_notify(Some(&notify));
        let edit = Rc::new(Self {
            wnd: CpwlWnd::new(cp, attached),
            edit: RefCell::new(engine),
            queue,
            draining: Cell::new(false),
            mouse_down: Cell::new(false),
            focus: Cell::new(false),
            char_set: Cell::new(FxCharset::Default),
            old_window_rect: Cell::new(CfxFloatRect::default()),
            caret: RefCell::new(None),
        });
        let owner: Rc<dyn Window> = edit.clone();
        edit.wnd.bind_owner(&owner);
        edit
    }
    fn with_edit<R>(&self, operation: impl FnOnce(&mut CpwlEditImpl<'static>) -> R) -> R {
        let result = operation(&mut self.edit.borrow_mut());
        self.flush_notifications();
        result
    }
    fn flush_notifications(&self) {
        if self.draining.replace(true) {
            return;
        }
        loop {
            if !self.wnd.is_valid() {
                self.queue.events.borrow_mut().clear();
                break;
            }
            let event = self.queue.events.borrow_mut().pop_front();
            let Some(event) = event else { break };
            match event {
                EditEvent::ScrollInfo(info) => self.set_scroll_info(&PwlScrollInfo {
                    plate_width: info.plate_width,
                    content_min: info.content_min,
                    content_max: info.content_max,
                    small_step: info.small_step,
                    big_step: info.big_step,
                }),
                EditEvent::ScrollPosition(pos) => self.set_scroll_position(pos),
                EditEvent::Caret(visible, head, foot) => {
                    self.set_caret(visible, head, foot);
                }
                EditEvent::Invalidate(rect) => {
                    self.invalidate_rect(Some(rect));
                }
            }
        }
        self.draining.set(false);
    }
    fn caret(&self) -> Option<Rc<CpwlCaret>> {
        self.caret.borrow().as_ref().and_then(Weak::upgrade)
    }
    pub fn set_text(&self, text: &[u32]) {
        self.with_edit(|edit| {
            edit.set_text(text);
            edit.paint();
        });
    }
    pub fn text(&self) -> Vec<u32> {
        self.edit.borrow().text()
    }
    pub fn selected_text(&self) -> Vec<u32> {
        self.edit.borrow().selected_text()
    }
    pub fn replace_and_keep_selection(&self, text: &[u32]) {
        self.with_edit(|edit| edit.replace_and_keep_selection(text));
    }
    pub fn replace_selection(&self, text: &[u32]) {
        self.with_edit(|edit| edit.replace_selection(text));
    }
    pub fn select_all_text(&self) -> bool {
        self.with_edit(CpwlEditImpl::select_all);
        true
    }
    pub fn set_selection(&self, start: i32, end: i32) {
        self.with_edit(|edit| edit.set_selection(start, end));
    }
    pub fn selection(&self) -> (i32, i32) {
        self.edit.borrow().selection()
    }
    pub fn clear_selection(&self) {
        if !self.wnd.is_read_only() {
            self.with_edit(|edit| {
                edit.clear_selection();
            });
        }
    }
    pub fn set_scroll_pos(&self, pos: CfxPointF) {
        self.with_edit(|edit| edit.set_scroll_pos(pos));
    }
    pub fn scroll_pos(&self) -> CfxPointF {
        self.edit.borrow().scroll_pos()
    }
    pub fn set_char_set(&self, charset: FxCharset) {
        self.char_set.set(charset);
    }
    pub fn char_set(&self) -> FxCharset {
        self.char_set.get()
    }
    pub fn set_ready_to_input(&self) {
        if self.mouse_down.replace(false) {
            self.wnd.release_capture();
        }
    }
    pub fn set_align_format_vertical_center(&self) {
        self.with_edit(|edit| {
            edit.set_alignment_v(1);
            edit.paint();
        });
    }
    pub fn can_select_all(&self) -> bool {
        self.select_word_range() != self.edit.borrow().whole_word_range()
    }
    fn select_word_range(&self) -> CpvtWordRange {
        let edit = self.edit.borrow();
        if !edit.is_selected() {
            return CpvtWordRange::default();
        }
        let (start, end) = edit.selection();
        CpvtWordRange::new(
            edit.word_index_to_word_place(start),
            edit.word_index_to_word_place(end),
        )
    }
    pub fn is_text_full(&self) -> bool {
        self.edit.borrow().is_text_full()
    }
    pub fn set_limit_char(&self, limit: i32) {
        self.with_edit(|edit| {
            edit.set_limit_char(limit);
            edit.paint();
        });
    }
    pub fn set_max_undo_items_for_test(&self, items: usize) {
        self.edit.borrow_mut().set_max_undo_items_for_test(items);
    }
    pub fn can_undo(&self) -> bool {
        !self.wnd.is_read_only() && self.edit.borrow().can_undo()
    }
    pub fn can_redo(&self) -> bool {
        !self.wnd.is_read_only() && self.edit.borrow().can_redo()
    }
    pub fn undo(&self) -> bool {
        self.can_undo() && self.with_edit(CpwlEditImpl::undo)
    }
    pub fn redo(&self) -> bool {
        self.can_redo() && self.with_edit(CpwlEditImpl::redo)
    }
    pub fn set_font_size(&self, size: f32) {
        self.with_edit(|edit| {
            edit.set_font_size(size);
            edit.paint();
        });
    }
    pub fn font_size(&self) -> f32 {
        self.edit.borrow().font_size()
    }
    pub fn set_text_direction(&self, direction: ParagraphDirection) {
        self.with_edit(|edit| edit.set_text_direction(direction));
    }
    pub fn text_direction(&self) -> ParagraphDirection {
        self.edit.borrow().text_direction()
    }
    pub fn set_char_array(&self, count: i32) {
        if !self.wnd.has_flag(EDIT_CHAR_ARRAY) || count <= 0 {
            return;
        }
        self.with_edit(|edit| {
            edit.set_char_array(count);
            edit.set_text_overflow(true);
            edit.paint();
        });
        if !self.wnd.has_flag(WINDOW_AUTO_FONT_SIZE) {
            return;
        }
        let Some(map) = self.wnd.font_map() else {
            return;
        };
        let size = Self::char_array_auto_font_size(
            map.get_pdf_font(0).as_ref(),
            self.client_rect(),
            count,
        );
        if size > 0. {
            self.with_edit(|edit| {
                edit.set_auto_font_size(false);
                edit.set_font_size(size);
                edit.paint();
            });
        }
    }
    pub fn char_array_auto_font_size(
        font: Option<&CpdfFont>,
        plate: CfxFloatRect,
        count: i32,
    ) -> f32 {
        let Some(font) = font else { return 0. };
        let bbox = match font {
            CpdfFont::Simple(font) => {
                if font.is_type1()
                    && crate::fxge::cfx_standardfont::get_standard_font_index(font.base_font_name())
                        .is_some()
                {
                    return 0.;
                }
                font.font_base.font_bbox
            }
            CpdfFont::Cid(font) => font.font_base.font_bbox,
            CpdfFont::Type3(font) => font.font_base.borrow().font_bbox,
        };
        if count <= 0 || bbox.width() == 0 || bbox.height() == 0 {
            return 0.;
        }
        let x = plate.width() / count as f32 * 1000. / bbox.width() as f32;
        let y = -plate.height() * 1000. / bbox.height() as f32;
        x.min(y)
    }
    fn set_param_by_flag(&self) {
        let flag = |flag| self.wnd.has_flag(flag);
        self.with_edit(|edit| {
            edit.set_alignment_h(if flag(EDIT_RIGHT) {
                2
            } else if flag(EDIT_MIDDLE) {
                1
            } else {
                0
            });
            edit.set_alignment_v(if flag(EDIT_CENTER) { 1 } else { 0 });
            if flag(EDIT_PASSWORD) {
                edit.set_password_char(b'*' as u16);
            }
            edit.set_multi_line(flag(EDIT_MULTILINE));
            edit.set_auto_return(flag(EDIT_AUTO_RETURN));
            edit.set_auto_font_size(flag(WINDOW_AUTO_FONT_SIZE));
            edit.set_auto_scroll(flag(EDIT_AUTO_SCROLL));
            edit.enable_undo(flag(EDIT_UNDO));
            if flag(EDIT_TEXT_OVERFLOW) {
                edit.set_text_overflow(true);
            }
        });
        if flag(EDIT_TEXT_OVERFLOW) {
            self.wnd.set_clip_rect(CfxFloatRect::default());
        } else if let Some(caret) = self.caret() {
            let mut rect = self.client_rect();
            if !rect.is_empty() {
                rect.inflate_xy(1., 1.);
                rect.normalize();
            }
            caret.wnd().set_clip_rect(rect);
        }
    }
    fn set_visible_edit_caret(&self) {
        let (head, foot) = {
            let edit = self.edit.borrow();
            let mut iterator = edit.iterator();
            iterator.set_at_index(edit.caret());
            if let Some(word) = iterator.word() {
                (
                    CfxPointF::new(word.caret_x(), word.ascent_y()),
                    CfxPointF::new(word.caret_x(), word.descent_y()),
                )
            } else if let Some(line) = iterator.line() {
                let x = iterator.line_caret_x(&line);
                (
                    CfxPointF::new(x, line.point.y + line.ascent),
                    CfxPointF::new(x, line.point.y + line.descent),
                )
            } else {
                (CfxPointF::default(), CfxPointF::default())
            }
        };
        self.set_caret(true, head, foot);
    }
    pub fn set_caret(&self, mut visible: bool, head: CfxPointF, foot: CfxPointF) -> bool {
        let Some(caret) = self.caret() else {
            return self.wnd.is_valid();
        };
        if !self.wnd.is_focused() || self.edit.borrow().is_selected() {
            visible = false;
        }
        caret.set_caret(visible, head, foot);
        self.wnd.is_valid()
    }
    fn on_key_down_internal(&self, mut key: i32, flags: u32) -> bool {
        if self.mouse_down.get() {
            return true;
        }
        self.wnd.base_on_key_down(key, flags);
        if !self.wnd.is_valid() {
            return false;
        }
        if key == FWL_VKEY_Delete && self.edit.borrow().is_selected() {
            key = FWL_VKEY_Unknown;
        }
        let shift = flags & FWL_EVENTFLAG_ShiftKey != 0;
        let ctrl = flags & FWL_EVENTFLAG_ControlKey != 0;
        let alt = flags & FWL_EVENTFLAG_AltKey != 0;
        let shortcut = if cfg!(target_vendor = "apple") {
            flags & FWL_EVENTFLAG_MetaKey != 0
        } else {
            ctrl
        };
        match key {
            FWL_VKEY_Delete => {
                if !self.wnd.is_read_only() {
                    self.with_edit(|edit| {
                        edit.delete();
                    });
                }
                true
            }
            FWL_VKEY_Up => {
                self.with_edit(|edit| edit.on_vk_up(shift));
                true
            }
            FWL_VKEY_Down => {
                self.with_edit(|edit| edit.on_vk_down(shift));
                true
            }
            FWL_VKEY_Left => {
                self.with_edit(|edit| edit.on_vk_left(shift));
                true
            }
            FWL_VKEY_Right => {
                self.with_edit(|edit| edit.on_vk_right(shift));
                true
            }
            FWL_VKEY_Home => {
                self.with_edit(|edit| edit.on_vk_home(shift, ctrl));
                true
            }
            FWL_VKEY_End => {
                self.with_edit(|edit| edit.on_vk_end(shift, ctrl));
                true
            }
            FWL_VKEY_A if shortcut && !shift && !alt => self.select_all_text(),
            FWL_VKEY_Y if !cfg!(target_vendor = "apple") && shortcut && !shift && !alt => {
                self.redo();
                true
            }
            FWL_VKEY_Z if shortcut && !alt => {
                if shift {
                    self.redo();
                } else {
                    self.undo();
                }
                true
            }
            FWL_VKEY_Unknown => {
                self.clear_selection();
                true
            }
            _ => false,
        }
    }
    fn on_char_internal(&self, character: u16, flags: u32) -> bool {
        if self.mouse_down.get() {
            return true;
        }
        self.wnd.base_on_char(character, flags);
        if !self.wnd.is_valid() {
            return false;
        }
        if [10, 27, 127].contains(&character) {
            return false;
        }
        let ctrl = flags & FWL_EVENTFLAG_ControlKey != 0;
        let meta = flags & FWL_EVENTFLAG_MetaKey != 0;
        let alt = flags & FWL_EVENTFLAG_AltKey != 0;
        let shortcut = if cfg!(target_vendor = "apple") {
            meta
        } else {
            ctrl
        };
        if shortcut && !alt || character < 32 && (ctrl || meta) {
            return false;
        }
        if self.wnd.is_read_only() {
            return true;
        }
        self.with_edit(|edit| edit.type_char(character, self.char_set.get()));
        true
    }
}
impl Window for CpwlEdit {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn wnd(&self) -> &CpwlWnd {
        &self.wnd
    }
    fn client_rect(&self) -> CfxFloatRect {
        let width = (self.wnd.border_width() + self.wnd.inner_border_width()) as f32;
        let mut rect = self.wnd.window_rect();
        rect.deflate_xy(width, width);
        if self
            .wnd
            .v_scroll_bar()
            .is_some_and(|bar| bar.wnd().is_visible())
        {
            rect.right -= CpwlScrollBar::WIDTH;
        }
        rect
    }
    fn focus_rect(&self) -> CfxFloatRect {
        CfxFloatRect::default()
    }
    fn on_created(&self) {
        self.set_font_size(self.wnd.creation_params().font_size);
        self.with_edit(CpwlEditImpl::initialize);
        if let Some(scroll) = self.wnd.v_scroll_bar() {
            scroll.wnd().remove_flag(WINDOW_AUTO_TRANSPARENT);
            scroll.wnd().set_transparency(255);
        }
        self.set_param_by_flag();
        self.old_window_rect.set(self.wnd.window_rect());
    }
    fn on_destroy(&self) {
        self.caret.borrow_mut().take();
        self.focus.set(false);
        self.queue.events.borrow_mut().clear();
    }
    fn create_child_wnd(&self, cp: &CreateParams) {
        if self.wnd.is_read_only() || self.caret().is_some() {
            return;
        }
        let mut cp = cp.clone();
        cp.flags = WINDOW_NO_REFRESH_CLIP;
        cp.border_width = 0;
        cp.border_style = BorderStyle::Solid;
        cp.rect = CfxFloatRect::default();
        let caret = CpwlCaret::new(cp, self.wnd.clone_attached_data());
        caret.set_invalid_rect(self.client_rect());
        *self.caret.borrow_mut() = Some(Rc::downgrade(&caret));
        self.wnd.add_child(caret.clone());
        caret.wnd().realize();
    }
    fn reposition_child_wnd(&self) -> bool {
        if let Some(scroll) = self.wnd.v_scroll_bar() {
            let rect = self.old_window_rect.get();
            scroll.wnd().move_window(
                CfxFloatRect::new(
                    rect.right,
                    rect.bottom,
                    rect.right + CpwlScrollBar::WIDTH,
                    rect.top,
                ),
                true,
                false,
            );
            if !self.wnd.is_alive() {
                return false;
            }
        }
        if !self.wnd.has_flag(EDIT_TEXT_OVERFLOW) {
            if let Some(caret) = self.caret() {
                let mut rect = self.client_rect();
                if !rect.is_empty() {
                    rect.inflate_xy(1., 1.);
                    rect.normalize();
                }
                caret.wnd().set_clip_rect(rect);
            }
        }
        self.with_edit(|edit| {
            edit.set_plate_rect(self.client_rect());
            edit.paint();
        });
        self.wnd.is_alive()
    }
    fn set_font_size(&self, size: f32) {
        CpwlEdit::set_font_size(self, size);
    }
    fn font_size(&self) -> f32 {
        CpwlEdit::font_size(self)
    }
    fn set_text_direction(&self, direction: ParagraphDirection) {
        CpwlEdit::set_text_direction(self, direction);
    }
    fn text(&self) -> Vec<u32> {
        CpwlEdit::text(self)
    }
    fn selected_text(&self) -> Vec<u32> {
        CpwlEdit::selected_text(self)
    }
    fn replace_and_keep_selection(&self, text: &[u32]) {
        CpwlEdit::replace_and_keep_selection(self, text)
    }
    fn replace_selection(&self, text: &[u32]) {
        CpwlEdit::replace_selection(self, text)
    }
    fn select_all_text(&self) -> bool {
        CpwlEdit::select_all_text(self)
    }
    fn can_undo(&self) -> bool {
        CpwlEdit::can_undo(self)
    }
    fn can_redo(&self) -> bool {
        CpwlEdit::can_redo(self)
    }
    fn undo(&self) -> bool {
        CpwlEdit::undo(self)
    }
    fn redo(&self) -> bool {
        CpwlEdit::redo(self)
    }
    fn set_scroll_info(&self, info: &PwlScrollInfo) {
        if let Some(scroll) = self.wnd.v_scroll_bar() {
            scroll.set_scroll_info(info);
        }
    }
    fn set_scroll_position(&self, pos: f32) {
        if let Some(scroll) = self.wnd.v_scroll_bar() {
            scroll.set_scroll_position(pos);
        }
    }
    fn scroll_window_vertically(&self, pos: f32) {
        let x = self.scroll_pos().x;
        self.set_scroll_pos(CfxPointF::new(x, pos));
    }
    fn on_set_focus(&self) {
        self.set_visible_edit_caret();
        if !self.wnd.is_valid() {
            return;
        }
        if !self.wnd.is_read_only() {
            if let Some(provider) = self.wnd.provider() {
                provider.on_set_focus_for_edit(self);
                if !self.wnd.is_valid() {
                    return;
                }
            }
        }
        self.focus.set(true);
    }
    fn on_kill_focus(&self) {
        if let Some(scroll) = self.wnd.v_scroll_bar() {
            if scroll.wnd().is_visible() {
                if !scroll.set_visible(false) || !self.wnd.is_valid() {
                    return;
                }
                if !self.wnd.move_window(self.old_window_rect.get(), true, true) {
                    return;
                }
            }
        }
        self.with_edit(CpwlEditImpl::select_none);
        if !self.wnd.is_valid() {
            return;
        }
        if !self.set_caret(false, CfxPointF::default(), CfxPointF::default()) {
            return;
        }
        self.char_set.set(FxCharset::Ansi);
        self.focus.set(false);
    }
    fn set_cursor(&self) {
        if !self.wnd.is_valid() {
            return;
        }
        let matrix = self.wnd.window_matrix();
        let horizontal = matrix.transform(CfxPointF::new(1., 1.)).y
            == matrix.transform(CfxPointF::new(0., 1.)).y;
        if let Some(filler) = self.wnd.filler_notify() {
            filler.set_cursor(if horizontal {
                CursorStyle::VBeam
            } else {
                CursorStyle::HBeam
            });
        }
    }
    fn on_key_down(&self, key: i32, flags: u32) -> bool {
        if self.mouse_down.get() {
            return true;
        }
        if key == FWL_VKEY_Delete {
            let (start, mut end) = self.selection();
            if start == end {
                end = start.saturating_add(1);
            }
            if let Some(filler) = self.wnd.filler_notify() {
                let mut change = Vec::new();
                let result = filler.on_before_key_stroke(
                    self.wnd.attached_data(),
                    &mut change,
                    &[],
                    start,
                    end,
                    true,
                    flags,
                );
                if !self.wnd.is_valid() || !result.rc || result.exit {
                    return false;
                }
            }
        }
        self.on_key_down_internal(key, flags)
    }
    fn on_char(&self, character: u16, flags: u32) -> bool {
        if self.mouse_down.get() {
            return true;
        }
        if flags & FWL_EVENTFLAG_ControlKey == 0 {
            let (mut start, end) = self.selection();
            let mut change = Vec::new();
            match character {
                8 => {
                    if start == end {
                        start = end.saturating_sub(1);
                    }
                }
                13 => {}
                _ => change.push(character as u32),
            }
            if let Some(filler) = self.wnd.filler_notify() {
                let result = filler.on_before_key_stroke(
                    self.wnd.attached_data(),
                    &mut change,
                    &[],
                    start,
                    end,
                    true,
                    flags,
                );
                if !self.wnd.is_valid() {
                    return false;
                }
                if !result.rc {
                    return true;
                }
                if result.exit {
                    return false;
                }
            }
        }
        if let Some(map) = self.wnd.font_map() {
            self.char_set
                .set(map.charset_from_unicode(character, FxCharset::Default));
        }
        self.on_char_internal(character, flags)
    }
    fn on_mouse_wheel(&self, _flags: u32, _point: CfxPointF, delta: CfxPointF) -> bool {
        if !self.wnd.has_flag(EDIT_MULTILINE) {
            return false;
        }
        let mut pos = self.scroll_pos();
        if delta.y > 0. {
            pos.y += self.font_size();
        } else {
            pos.y -= self.font_size();
        }
        self.set_scroll_pos(pos);
        true
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_down(flags, point);
        if !self.wnd.is_valid() {
            return true;
        }
        if self.wnd.has_flag(EDIT_TEXT_OVERFLOW) || self.wnd.client_hit_test(point) {
            if self.mouse_down.get() && !self.invalidate_rect(None) {
                return true;
            }
            self.mouse_down.set(true);
            self.wnd.set_capture();
            self.with_edit(|edit| {
                edit.on_mouse_down(
                    point,
                    flags & FWL_EVENTFLAG_ShiftKey != 0,
                    flags & FWL_EVENTFLAG_ControlKey != 0,
                )
            });
        }
        true
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_up(flags, point);
        if !self.wnd.is_valid() {
            return true;
        }
        if self.mouse_down.get() {
            if self.wnd.client_hit_test(point) && !self.wnd.is_focused() {
                self.wnd.set_focus();
                if !self.wnd.is_valid() {
                    return true;
                }
            }
            self.wnd.release_capture();
            self.mouse_down.set(false);
        }
        true
    }
    fn on_l_button_dbl_clk(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_l_button_dbl_clk(flags, point);
        if self.wnd.is_valid()
            && (self.wnd.has_flag(EDIT_TEXT_OVERFLOW) || self.wnd.client_hit_test(point))
        {
            self.with_edit(CpwlEditImpl::select_all);
        }
        true
    }
    fn on_r_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        if self.mouse_down.get() {
            return false;
        }
        let _ = (flags, point);
        if !self.wnd.is_valid() {
            return false;
        }
        if !self.wnd.has_flag(EDIT_TEXT_OVERFLOW) && !self.wnd.client_hit_test(point) {
            return true;
        }
        self.wnd.set_focus();
        false
    }
    fn on_mouse_move(&self, flags: u32, point: CfxPointF) -> bool {
        self.wnd.base_on_mouse_move(flags, point);
        if self.wnd.is_valid() && self.mouse_down.get() {
            self.with_edit(|edit| edit.on_mouse_move(point, false, false));
        }
        true
    }
    fn draw_this_appearance(&self, draw: &mut dyn PwlDraw, matrix: CfxMatrix) {
        self.wnd.base_draw_this_appearance(draw, matrix);
        let client = self.client_rect();
        let border = self.wnd.border_style();
        let count = self.edit.borrow().char_array();
        if count > 0
            && matches!(border, BorderStyle::Solid | BorderStyle::Dash)
            && count
                .checked_sub(1)
                .and_then(|n| n.checked_mul(2))
                .is_some()
        {
            let mut graph = CfxGraphStateData::default();
            graph.set_line_width(self.wnd.border_width() as f32);
            if border == BorderStyle::Dash {
                let dash = self.wnd.border_dash();
                graph.set_dash_array(vec![dash.dash as f32, dash.gap as f32]);
                graph.set_dash_phase(dash.phase as f32);
            }
            let width = client.width() / count as f32;
            let mut path = CfxPath::new();
            for index in 0..count - 1 {
                let x = client.left + width * (index + 1) as f32;
                path.append_line(
                    CfxPointF::new(x, client.bottom),
                    CfxPointF::new(x, client.top),
                );
            }
            if !path.points().is_empty() {
                draw.draw_path(
                    &path,
                    matrix,
                    Some(&graph),
                    0,
                    self.wnd.border_color().to_fx_color(255),
                    CfxFillRenderOptions::even_odd_options(),
                );
            }
        }
        let filler = self.wnd.filler_notify();
        let host = filler
            .as_ref()
            .is_some_and(|filler| filler.is_selection_implemented());
        let plan = {
            let edit = self.edit.borrow();
            let overflow = self.wnd.has_flag(EDIT_TEXT_OVERFLOW);
            let range = edit.visible_word_range();
            edit.draw_edit_plan(
                matrix,
                self.wnd.text_color().to_fx_color(self.wnd.transparency()),
                if overflow {
                    CfxFloatRect::default()
                } else {
                    client
                },
                CfxPointF::default(),
                if overflow { None } else { Some(&range) },
                host,
            )
        };
        plan.draw_observed(draw, filler.as_deref(), self.wnd.attached_data(), &|| {
            self.wnd.is_alive()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::ipwl_fillernotify::{BeforeKeystrokeResult, FillerNotify};
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfdoc::cpdf_bafontmap::CpdfBaFontMap;
    #[derive(Default)]
    struct Host {
        veto: Cell<bool>,
        exit: Cell<bool>,
        editor: RefCell<Option<Weak<CpwlEdit>>>,
        invalidate_action: Cell<u8>,
        invalidations: Cell<usize>,
    }
    impl FillerNotify for Host {
        fn invalidate_rect(&self, _: Option<&dyn PerWindowData>, _: CfxFloatRect) {
            self.invalidations.set(self.invalidations.get() + 1);
            let action = self.invalidate_action.replace(0);
            let editor = self.editor.borrow().as_ref().and_then(Weak::upgrade);
            if let Some(editor) = editor {
                if action == 1 {
                    editor.replace_selection(&wide("R"));
                } else if action == 2 {
                    editor.wnd().destroy();
                }
            }
        }
        fn output_selected_rect(&self, _: Option<&dyn PerWindowData>, _: CfxFloatRect) {}
        fn is_selection_implemented(&self) -> bool {
            false
        }
        fn set_cursor(&self, _: CursorStyle) {}
        fn query_where_popup(
            &self,
            _: Option<&dyn PerWindowData>,
            _: f32,
            max: f32,
        ) -> (bool, f32) {
            (true, max)
        }
        fn on_before_key_stroke(
            &self,
            _: Option<&dyn PerWindowData>,
            _: &mut Vec<u32>,
            _: &[u32],
            _: i32,
            _: i32,
            _: bool,
            _: u32,
        ) -> BeforeKeystrokeResult {
            BeforeKeystrokeResult {
                rc: !self.veto.get(),
                exit: self.exit.get(),
            }
        }
        fn on_popup_pre_open(&self, _: Option<&dyn PerWindowData>, _: u32) -> bool {
            true
        }
        fn on_popup_post_open(&self, _: Option<&dyn PerWindowData>, _: u32) -> bool {
            true
        }
    }
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn create(
        flags: u32,
        host: Option<&Rc<dyn FillerNotify>>,
    ) -> (Rc<RefCell<CpdfDocument>>, Rc<CpwlEdit>) {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let mut annot = CpdfDictionary::new();
        annot.set_new_name_for(b"Subtype", b"Widget");
        annot.set_new_string_for(b"DA", b"/F1 12 Tf 0 g");
        let font_map = Rc::new(CpdfBaFontMap::new(&document, retain_object(annot), b"N"));
        let mut cp = CreateParams::new(None, host, None);
        cp.flags = WINDOW_VISIBLE | EDIT_UNDO | EDIT_AUTO_SCROLL | flags;
        cp.rect = CfxFloatRect::new(0., 0., 200., 30.);
        cp.font_map = Some(font_map);
        cp.font_size = 12.;
        let editor = CpwlEdit::new(cp, None);
        editor.wnd().realize();
        assert!(editor.wnd().is_valid());
        (document, editor)
    }
    #[test]
    fn source_window_typing_selection_delete_and_undo_redo() {
        let (_document, edit) = create(0, None);
        for character in b"ABC" {
            assert!(edit.on_char(*character as u16, 0));
        }
        assert_eq!(edit.text(), wide("ABC"));
        edit.set_selection(0, -1);
        assert_eq!(edit.selected_text(), wide("ABC"));
        assert!(edit.on_key_down(FWL_VKEY_Delete, 0));
        assert!(edit.text().is_empty());
        assert!(!edit.on_char(127, 0));
        assert!(edit.undo());
        assert_eq!(edit.selected_text(), wide("ABC"));
        assert!(edit.redo());
        assert!(edit.text().is_empty());
        assert!(!edit.can_redo());
        edit.replace_selection(&wide("123"));
        edit.set_selection(0, -1);
        edit.replace_and_keep_selection(&wide("XY"));
        assert_eq!(edit.selected_text(), wide("XY"));
        edit.wnd().destroy();
    }
    #[test]
    fn shortcut_and_control_characters_preserve_current_selection() {
        let (_document, edit) = create(0, None);
        edit.set_text(&wide("ABC"));
        edit.set_selection(0, -1);
        for character in [3, 22, 24] {
            assert!(!edit.on_char(character, FWL_EVENTFLAG_ControlKey | FWL_EVENTFLAG_AltKey));
            assert_eq!(edit.selected_text(), wide("ABC"));
        }
        let shortcut = if cfg!(target_vendor = "apple") {
            FWL_EVENTFLAG_MetaKey
        } else {
            FWL_EVENTFLAG_ControlKey
        };
        assert!(!edit.on_char(b'a' as u16, shortcut));
        assert_eq!(edit.text(), wide("ABC"));
        assert!(edit.on_key_down(FWL_VKEY_Right, 0));
        assert!(edit.selected_text().is_empty());
        assert!(edit.on_key_down(FWL_VKEY_A, shortcut));
        assert_eq!(edit.selected_text(), wide("ABC"));
        assert!(!edit.on_char(10, 0));
        assert!(!edit.on_char(27, 0));
        edit.wnd().destroy();
    }
    #[test]
    fn read_only_and_keystroke_veto_exit_are_real_window_boundaries() {
        let (_document, edit) = create(WINDOW_READ_ONLY, None);
        edit.set_text(&wide("fixed"));
        edit.set_selection(0, -1);
        assert!(edit.on_char(b'X' as u16, 0));
        assert!(edit.on_key_down(FWL_VKEY_Delete, 0));
        assert_eq!(edit.text(), wide("fixed"));
        assert!(!edit.undo());
        edit.wnd().destroy();
        let host = Rc::new(Host::default());
        let notify: Rc<dyn FillerNotify> = host.clone();
        let (_document, edit) = create(0, Some(&notify));
        host.veto.set(true);
        assert!(edit.on_char(b'A' as u16, 0));
        assert!(edit.text().is_empty());
        assert!(!edit.on_key_down(FWL_VKEY_Delete, 0));
        host.veto.set(false);
        host.exit.set(true);
        assert!(!edit.on_char(b'A' as u16, 0));
        assert!(edit.text().is_empty());
        host.exit.set(false);
        assert!(edit.on_char(b'B' as u16, 0));
        assert_eq!(edit.text(), wide("B"));
        edit.wnd().destroy();
    }
    #[test]
    fn invalidation_callbacks_can_reenter_or_destroy_the_editor() {
        let host = Rc::new(Host::default());
        let notify: Rc<dyn FillerNotify> = host.clone();
        let (_document, edit) = create(0, Some(&notify));
        *host.editor.borrow_mut() = Some(Rc::downgrade(&edit));
        host.invalidate_action.set(1);
        assert!(edit.on_char(b'A' as u16, 0));
        assert_eq!(edit.text(), wide("AR"));
        assert!(host.invalidations.get() > 0);
        host.invalidate_action.set(2);
        edit.on_char(b'B' as u16, 0);
        assert!(!edit.wnd().is_alive());
    }
    #[test]
    fn multiline_window_and_character_limit_preserve_source_edit_behavior() {
        let (_document, edit) = create(EDIT_MULTILINE | EDIT_AUTO_RETURN, None);
        edit.set_limit_char(6);
        for character in [b'A' as u16, 13, b'B' as u16] {
            assert!(edit.on_char(character, 0));
        }
        assert_eq!(edit.text(), wide("A\r\nB"));
        assert!(edit.undo());
        assert_eq!(edit.text(), wide("A\r\n"));
        assert!(edit.redo());
        assert_eq!(edit.text(), wide("A\r\nB"));
        edit.wnd().set_focus();
        assert!(edit.wnd().is_focused());
        edit.wnd().kill_focus();
        assert!(!edit.wnd().is_focused());
        edit.wnd().destroy();
    }
}
