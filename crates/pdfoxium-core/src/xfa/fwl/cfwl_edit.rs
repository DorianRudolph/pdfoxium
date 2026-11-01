// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct FWL edit control. Cancellable host events run before borrowing the
//! FDE engine; committed UI refreshes run after releasing that borrow. The
//! backend retains its gap buffer/history and has no host-calling delegate.
use super::{
    cfwl_app::CfwlApp,
    cfwl_caret::CfwlCaret,
    cfwl_event::{EventData, FwlEvent, ScrollCode},
    cfwl_message::{FwlMessage, KeyCommand, MessageData, MouseCommand},
    cfwl_scrollbar::{CfwlScrollBar, FWL_STYLEEXT_SCB_VERT},
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{CfwlThemePart, Part},
    cfwl_widget::{
        CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef, WidgetState, WidgetStyle,
    },
    fwl_widgetdef::*,
    fwl_widgethit::FwlWidgetHit,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::{
        fde::{
            cfde_texteditengine::{CfdeTextEditEngine, RecordOperation},
            cfde_textout::CfdeTextOut,
        },
        fgas::{
            graphics::{cfgas_gegraphics::CfgasGeGraphics, cfgas_gepath::CfgasGePath},
            layout::cfgas_txtbreak::{ALIGNMENT_CENTER, ALIGNMENT_JUSTIFIED, ALIGNMENT_RIGHT},
        },
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub const FWL_STYLEEXT_EDT_READ_ONLY: u32 = 1;
pub const FWL_STYLEEXT_EDT_MULTI_LINE: u32 = 1 << 1;
pub const FWL_STYLEEXT_EDT_WANT_RETURN: u32 = 1 << 2;
pub const FWL_STYLEEXT_EDT_AUTO_HSCROLL: u32 = 1 << 4;
pub const FWL_STYLEEXT_EDT_AUTO_VSCROLL: u32 = 1 << 5;
pub const FWL_STYLEEXT_EDT_VALIDATE: u32 = 1 << 7;
pub const FWL_STYLEEXT_EDT_PASSWORD: u32 = 1 << 8;
pub const FWL_STYLEEXT_EDT_NUMBER: u32 = 1 << 9;
pub const FWL_STYLEEXT_EDT_COMB_TEXT: u32 = 1 << 17;
pub const FWL_STYLEEXT_EDT_HNEAR: u32 = 0;
pub const FWL_STYLEEXT_EDT_HCENTER: u32 = 1 << 18;
pub const FWL_STYLEEXT_EDT_HFAR: u32 = 2 << 18;
pub const FWL_STYLEEXT_EDT_VNEAR: u32 = 0;
pub const FWL_STYLEEXT_EDT_VCENTER: u32 = 1 << 20;
pub const FWL_STYLEEXT_EDT_VFAR: u32 = 2 << 20;
pub const FWL_STYLEEXT_EDT_JUSTIFIED: u32 = 1 << 22;
pub const FWL_STYLEEXT_EDT_HALIGN_MASK: u32 = 3 << 18;
pub const FWL_STYLEEXT_EDT_VALIGN_MASK: u32 = 3 << 20;
pub const FWL_STYLEEXT_EDT_HALIGN_MODE_MASK: u32 = 3 << 22;
pub const FWL_STYLEEXT_EDT_SHOW_SCROLLBAR_FOCUS: u32 = 1 << 25;
pub const FWL_STYLEEXT_EDT_OUTER_SCROLLBAR: u32 = 1 << 26;
const EDIT_MARGIN: f32 = 3.;
pub struct CfwlEdit {
    base: CfwlWidget,
    engine: RefCell<CfdeTextEditEngine>,
    client_rect: Cell<CfxRectF>,
    engine_rect: Cell<CfxRectF>,
    caret_rect: Cell<CfxRectF>,
    lbutton_down: Cell<bool>,
    mouse_anchor: Cell<usize>,
    limit: Cell<i32>,
    valign_offset: Cell<f32>,
    scroll_x: Cell<f32>,
    scroll_y: Cell<f32>,
    font_size: Cell<f32>,
    cursor: Cell<usize>,
    vert_scroll_bar: RefCell<Option<Rc<CfwlScrollBar>>>,
    caret: RefCell<Option<Rc<CfwlCaret>>>,
    generation: Cell<u64>,
    finalizing: Cell<bool>,
}
impl CfwlEdit {
    pub fn new(app: &Rc<CfwlApp>, properties: Properties, outer: Option<WidgetRef>) -> Rc<Self> {
        let edit = Rc::new(Self::new_unregistered(app, properties, outer));
        app.register_widget(edit.clone());
        edit
    }
    pub(crate) fn new_unregistered(
        app: &Rc<CfwlApp>,
        properties: Properties,
        outer: Option<WidgetRef>,
    ) -> Self {
        Self {
            base: CfwlWidget::new(app, properties, outer),
            engine: RefCell::new(CfdeTextEditEngine::new()),
            client_rect: Cell::new(CfxRectF::default()),
            engine_rect: Cell::new(CfxRectF::default()),
            caret_rect: Cell::new(CfxRectF::default()),
            lbutton_down: Cell::new(false),
            mouse_anchor: Cell::new(0),
            limit: Cell::new(-1),
            valign_offset: Cell::new(0.),
            scroll_x: Cell::new(0.),
            scroll_y: Cell::new(0.),
            font_size: Cell::new(0.),
            cursor: Cell::new(0),
            vert_scroll_bar: RefCell::new(None),
            caret: RefCell::new(None),
            generation: Cell::new(0),
            finalizing: Cell::new(false),
        }
    }
    fn changed(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.cursor
            .set(self.cursor.get().min(self.get_text_length()));
        self.on_text_changed();
    }
    pub fn text_revision(&self) -> u64 {
        self.generation.get()
    }
    pub fn client_rect(&self) -> CfxRectF {
        self.client_rect.get()
    }
    pub fn engine_rect(&self) -> CfxRectF {
        self.engine_rect.get()
    }
    pub fn cursor_position(&self) -> usize {
        self.cursor.get()
    }
    pub fn vertical_scroll_bar(&self) -> Option<Rc<CfwlScrollBar>> {
        self.vert_scroll_bar.borrow().clone()
    }
    pub fn caret(&self) -> Option<Rc<CfwlCaret>> {
        self.caret.borrow().clone()
    }
    pub fn set_text(&self, text: &[u32]) {
        self.engine.borrow_mut().clear();
        self.generation.set(self.generation.get().wrapping_add(1));
        self.insert_at(0, text, RecordOperation::InsertRecord);
    }
    pub fn set_text_skip_notify(&self, text: &[u32]) {
        let mut engine = self.engine.borrow_mut();
        engine.clear();
        engine.insert(0, text, RecordOperation::SkipNotify);
        self.generation.set(self.generation.get().wrapping_add(1));
    }
    pub fn get_text_length(&self) -> usize {
        self.engine.borrow().get_length()
    }
    pub fn get_text(&self) -> Vec<u32> {
        self.engine.borrow().get_text()
    }
    pub fn clear_text(&self) {
        self.engine.borrow_mut().clear();
        self.changed();
    }
    pub fn select_all(&self) {
        self.engine.borrow_mut().select_all();
    }
    pub fn clear_selection(&self) {
        self.engine.borrow_mut().clear_selection();
    }
    pub fn set_selection(&self, start: usize, count: usize) {
        self.engine.borrow_mut().set_selection(start, count);
    }
    pub fn has_selection(&self) -> bool {
        self.engine.borrow().has_selection()
    }
    pub fn get_selection(&self) -> (usize, usize) {
        self.engine.borrow().get_selection()
    }
    pub fn get_limit(&self) -> i32 {
        self.limit.get()
    }
    pub fn set_limit(&self, limit: i32) {
        self.limit.set(limit);
        let mut e = self.engine.borrow_mut();
        e.set_has_character_limit(limit > 0);
        if limit > 0 {
            e.set_character_limit(limit as usize);
        }
    }
    pub fn set_alias_char(&self, alias: u32) {
        self.engine.borrow_mut().set_alias_char(alias);
    }
    pub fn copy(&self) -> Option<Vec<u32>> {
        self.has_selection()
            .then(|| self.engine.borrow().get_selected_text())
    }
    pub fn cut(&self) -> Option<Vec<u32>> {
        if !self.has_selection() {
            return None;
        }
        let (s, n) = self.get_selection();
        let removed = self.delete_range(s, n);
        self.update_caret();
        Some(removed)
    }
    pub fn paste(&self, text: &[u32]) -> bool {
        if self.has_selection() {
            let (s, n) = self.get_selection();
            self.replace_range(s, n, text);
        } else {
            self.insert_at(self.cursor.get(), text, RecordOperation::InsertRecord);
        }
        true
    }
    pub fn can_undo(&self) -> bool {
        self.engine.borrow().can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.engine.borrow().can_redo()
    }
    pub fn undo(&self) -> bool {
        self.history(false)
    }
    pub fn redo(&self) -> bool {
        self.history(true)
    }
    fn history(&self, redo: bool) -> bool {
        let candidate = if self.base.get_style_exts() & FWL_STYLEEXT_EDT_VALIDATE != 0 {
            self.engine.borrow().history_validation_text(redo)
        } else {
            None
        };
        let epoch = self.generation.get();
        if let Some(candidate) = candidate {
            if !self.validate(&candidate) || epoch != self.generation.get() {
                return false;
            }
        }
        let (result, full) = {
            let mut e = self.engine.borrow_mut();
            let result = if redo { e.redo() } else { e.undo() };
            (result, e.last_insert_was_full())
        };
        if full {
            self.notify_text_full();
        }
        if result && !self.base.is_destroyed() {
            self.changed();
            self.update_cursor_rect();
            self.on_caret_changed();
        }
        result
    }
    fn prepare(&self, start: usize, end: usize, text: &[u32]) -> Option<(usize, usize, Vec<u32>)> {
        let mut event = FwlEvent::text_will_change(
            self.base.self_ref(),
            text.to_vec(),
            self.get_text(),
            start,
            end,
        );
        self.base.dispatch_event(&mut event);
        if self.base.is_destroyed() {
            return None;
        }
        let EventData::TextWillChange {
            change_text,
            selection_start,
            selection_end,
            cancelled,
            ..
        } = event.data
        else {
            return None;
        };
        if cancelled {
            return None;
        }
        let len = self.get_text_length();
        let s = selection_start.min(len);
        Some((s, selection_end.min(len).max(s), change_text))
    }
    fn validate(&self, text: &[u32]) -> bool {
        if self.base.get_style_exts() & FWL_STYLEEXT_EDT_VALIDATE == 0 {
            return true;
        }
        let mut event = FwlEvent::validate(self.base.self_ref(), text.to_vec());
        self.base.dispatch_event(&mut event);
        matches!(event.data, EventData::Validate { validate: true, .. })
            && !self.base.is_destroyed()
    }
    fn validate_change(&self, start: usize, end: usize, text: &[u32]) -> bool {
        if self.base.get_style_exts() & FWL_STYLEEXT_EDT_VALIDATE == 0 {
            return true;
        }
        let mut candidate = self.get_text();
        let start = start.min(candidate.len());
        let end = end.min(candidate.len()).max(start);
        let allowed = if self.limit.get() > 0 {
            (self.limit.get() as usize).saturating_sub(candidate.len() - (end - start))
        } else {
            text.len()
        };
        candidate.splice(start..end, text[..text.len().min(allowed)].iter().copied());
        self.validate(&candidate)
    }
    fn insert_at(&self, start: usize, text: &[u32], record: RecordOperation) {
        if text.is_empty() {
            return;
        }
        let Some((start, end, text)) = (if record == RecordOperation::InsertRecord {
            self.prepare(start, start, text)
        } else {
            Some((start, start, text.to_vec()))
        }) else {
            return;
        };
        if !self.validate_change(start, end, &text) {
            return;
        }
        let (changed, full) = {
            let mut e = self.engine.borrow_mut();
            let start = start.min(e.get_length());
            let end = end.min(e.get_length()).max(start);
            let changed = if end > start {
                e.set_selection(start, end - start);
                e.replace_selected_text(&text);
                true
            } else {
                let before = e.get_length();
                e.insert(start, &text, record);
                e.get_length() != before
            };
            (changed, e.last_insert_was_full())
        };
        if full {
            self.notify_text_full();
        }
        if changed && !self.base.is_destroyed() {
            self.changed();
        }
    }
    fn replace_range(&self, start: usize, count: usize, text: &[u32]) {
        let Some((start, end, text)) = self.prepare(start, start.saturating_add(count), text)
        else {
            return;
        };
        if !self.validate_change(start, end, &text) {
            return;
        }
        let full = {
            let mut e = self.engine.borrow_mut();
            let s = start.min(e.get_length());
            let end = end.min(e.get_length()).max(s);
            e.set_selection(s, end - s);
            e.replace_selected_text(&text);
            e.last_insert_was_full()
        };
        if full {
            self.notify_text_full();
        }
        if !self.base.is_destroyed() {
            self.changed();
        }
    }
    fn delete_range(&self, start: usize, count: usize) -> Vec<u32> {
        let Some((start, end, text)) = self.prepare(start, start.saturating_add(count), &[]) else {
            return Vec::new();
        };
        if !text.is_empty() {
            if !self.validate_change(start, end, &text) {
                return Vec::new();
            }
        }
        let removed = {
            let mut e = self.engine.borrow_mut();
            let start = start.min(e.get_length());
            let end = end.min(e.get_length()).max(start);
            if text.is_empty() {
                e.delete(start, end - start, RecordOperation::InsertRecord)
            } else {
                e.set_selection(start, end - start);
                let removed = e.get_selected_text();
                e.replace_selected_text(&text);
                removed
            }
        };
        if !self.base.is_destroyed() {
            self.changed();
        }
        removed
    }
    pub fn notify_text_full(&self) {
        let mut event = FwlEvent::new(self.base.self_ref(), EventData::TextFull);
        self.base.dispatch_event(&mut event);
    }
    pub fn on_text_changed(&self) {
        if self.base.get_style_exts() & FWL_STYLEEXT_EDT_VALIGN_MASK != 0 {
            self.update_valignment();
        }
        self.layout_scroll_bar();
        self.base.repaint_rect(self.get_client_rect());
    }
    pub fn on_sel_changed(&self) {
        self.base.repaint_rect(self.get_client_rect());
    }
    pub fn set_scroll_offset(&self, offset: f32) {
        if offset.is_finite() {
            self.scroll_y.set(offset);
        }
    }
    fn draw_content(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        let mut state = graphics.state_restorer();
        let engine_rect = self.engine_rect.get();
        let clip = matrix.transform_rect(engine_rect);
        let x = engine_rect.left - self.scroll_x.get();
        let y = engine_rect.top - self.scroll_y.get() + self.valign_offset.get();
        let mut text_matrix = CfxMatrix::new(1., 0., 0., 1., x, y);
        text_matrix.concat(matrix);
        if self.base.get_states().contains(WidgetState::FOCUSED) && self.has_selection() {
            let rects = {
                let mut e = self.engine.borrow_mut();
                let (s, n) = e.get_selection();
                e.get_character_rects_in_range(
                    s.min(i32::MAX as usize) as i32,
                    n.min(i32::MAX as usize) as i32,
                )
            };
            let mut path = CfgasGePath::new();
            for mut rect in rects {
                rect.offset(x, y);
                path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
            }
            state.set_clip_rect(clip);
            if let Some(app) = self.base.app() {
                let mut params = CfwlThemeBackground::new(Part::Background, self);
                params.matrix = matrix;
                params.set_path(Some(&path));
                app.theme_provider().draw_background(&params, &mut state);
            }
        }
        if self.base.is_destroyed() {
            return;
        }
        state.set_clip_rect(clip);
        // Glyph drawing itself has no host callbacks, so the engine can lend
        // one small piece at a time instead of cloning the layout/text arrays.
        {
            let mut e = self.engine.borrow_mut();
            let Some(font) = e.get_font() else {
                return;
            };
            let color = e.get_font_color();
            let size = e.get_font_size();
            let doc_clip = text_matrix.get_inverse().transform_rect(clip);
            let count = e.get_text_pieces().len();
            for i in 0..count {
                let info = e.get_text_pieces()[i].clone();
                if !doc_clip.intersects(info.rect) {
                    continue;
                }
                let mut positions = e.get_display_pos(&info);
                CfdeTextOut::draw_string(
                    state.get_render_device(),
                    color,
                    &font,
                    &mut positions,
                    size,
                    text_matrix,
                );
            }
        }
        if self.base.get_style_exts() & FWL_STYLEEXT_EDT_COMB_TEXT != 0 {
            let limit = self.limit.get().max(1);
            let step = engine_rect.width / limit as f32;
            let mut left = engine_rect.left + 1.;
            let mut path = CfgasGePath::new();
            for _ in 1..limit {
                left += step;
                path.add_line(
                    CfxPointF::new(left, self.client_rect.get().top),
                    CfxPointF::new(left, self.client_rect.get().bottom()),
                );
            }
            if let Some(app) = self.base.app() {
                let mut params = CfwlThemeBackground::new(Part::CombTextLine, self);
                params.matrix = matrix;
                params.set_path(Some(&path));
                app.theme_provider().draw_background(&params, &mut state);
            }
        }
    }
    fn update_edit_engine(&self) {
        let Some(app) = self.base.app() else {
            return;
        };
        let theme = app.theme_provider();
        let part = CfwlThemePart::new(Part::None, self);
        // Query host properties before the engine borrow; host font/metric
        // access may execute arbitrary widget callbacks.
        let font = theme.get_font(&part);
        let font_size = theme.get_font_size(&part);
        let color = theme.get_text_color(&part);
        let line = theme.get_line_height(&part);
        if self.base.is_destroyed() {
            return;
        }
        self.font_size.set(font_size);
        let styles = self.base.get_style_exts();
        let auto_h = styles & FWL_STYLEEXT_EDT_AUTO_HSCROLL != 0;
        let mut e = self.engine.borrow_mut();
        e.set_available_width(self.engine_rect.get().width.max(0.) as usize);
        e.set_comb_text(styles & FWL_STYLEEXT_EDT_COMB_TEXT != 0);
        // Synchronous validation belongs to the outer preflight adapter, never
        // to a callback while this engine borrow is held.
        e.enable_validation(false);
        e.enable_password_mode(styles & FWL_STYLEEXT_EDT_PASSWORD != 0);
        let mut alignment = match styles & FWL_STYLEEXT_EDT_HALIGN_MASK {
            FWL_STYLEEXT_EDT_HCENTER => ALIGNMENT_CENTER,
            FWL_STYLEEXT_EDT_HFAR => ALIGNMENT_RIGHT,
            _ => 0,
        };
        if styles & FWL_STYLEEXT_EDT_HALIGN_MODE_MASK == FWL_STYLEEXT_EDT_JUSTIFIED {
            alignment |= ALIGNMENT_JUSTIFIED;
        }
        e.set_alignment(alignment);
        if styles & FWL_STYLEEXT_EDT_MULTI_LINE != 0 {
            e.enable_multi_line(true);
            e.enable_line_wrap(!auto_h);
            e.limit_vertical_scroll(
                !self.base.get_styles().contains(WidgetStyle::VSCROLL)
                    && styles & FWL_STYLEEXT_EDT_AUTO_VSCROLL == 0,
            );
        } else {
            e.enable_multi_line(false);
            e.enable_line_wrap(false);
            e.limit_vertical_scroll(false);
        }
        e.limit_horizontal_scroll(!auto_h);
        e.set_font(Some(font));
        e.set_font_color(color);
        e.set_font_size(font_size);
        e.set_line_space(line);
        e.set_tab_width(font_size);
        e.set_visible_line_count(if line > 0. {
            (self.engine_rect.get().height / line).max(1.) as usize
        } else {
            1
        });
        e.layout();
    }
    fn update_offset(&self) -> bool {
        let bounds = self.engine_rect.get();
        let x = bounds.left - self.scroll_x.get();
        let y = bounds.top - self.scroll_y.get() + self.valign_offset.get();
        let mut caret = self.caret_rect.get();
        caret.offset(x, y);
        if bounds.contains_rect(caret) {
            let mut content = self.engine.borrow_mut().get_contents_bounding_box();
            content.offset(x, y);
            if content.right() < bounds.right() && self.scroll_x.get() > 0. {
                self.scroll_x
                    .set((self.scroll_x.get() + content.right() - bounds.right()).max(0.));
            }
            if content.bottom() < bounds.bottom() && self.scroll_y.get() > 0. {
                self.scroll_y
                    .set((self.scroll_y.get() + content.bottom() - bounds.bottom()).max(0.));
            }
            return false;
        }
        let dx = if caret.left < bounds.left {
            caret.left - bounds.left
        } else if caret.right() > bounds.right() {
            caret.right() - bounds.right()
        } else {
            0.
        };
        let dy = if caret.top < bounds.top {
            caret.top - bounds.top
        } else if caret.bottom() > bounds.bottom() {
            caret.bottom() - bounds.bottom()
        } else {
            0.
        };
        self.scroll_x.set(self.scroll_x.get() + dx);
        self.scroll_y.set(if self.font_size.get() > bounds.height {
            0.
        } else {
            self.scroll_y.get() + dy
        });
        true
    }
    fn update_valignment(&self) {
        let Some(app) = self.base.app() else {
            return;
        };
        let part = CfwlThemePart::new(Part::None, self);
        let space = app.theme_provider().get_space_above_below(&part);
        let above = if space.width >= 0.1 { space.width } else { 0. };
        let below = if space.height >= 0.1 {
            space.height
        } else {
            0.
        };
        let content = self.engine.borrow_mut().get_contents_bounding_box();
        let styles = self.base.get_style_exts();
        let mut offset = 0.;
        if styles & FWL_STYLEEXT_EDT_VCENTER != 0 {
            offset = (self.engine_rect.get().height - content.height) / 2.;
            if offset < (above + below) / 2. && above < below {
                return;
            }
            offset += (above - below) / 2.;
        } else if styles & FWL_STYLEEXT_EDT_VFAR != 0 {
            offset = self.engine_rect.get().height - content.height - below;
        } else {
            offset += above;
        }
        self.valign_offset.set(offset.max(0.));
    }
    fn update_cursor_rect(&self) {
        let info = {
            let mut e = self.engine.borrow_mut();
            if e.can_generate_character_info() {
                e.get_character_info(self.cursor.get().min(i32::MAX as usize) as i32)
            } else {
                None
            }
        };
        let mut rect = info.map_or(CfxRectF::default(), |(_, rect)| rect);
        rect.width = 1.;
        if rect.height == 0. {
            rect.height = 8.;
        }
        self.caret_rect.set(rect);
    }
    pub fn set_cursor_position(&self, position: usize) {
        let position = position.min(self.get_text_length());
        if self.cursor.replace(position) == position {
            return;
        }
        self.update_cursor_rect();
        self.on_caret_changed();
    }
    pub fn on_caret_changed(&self) {
        if self.engine_rect.get().is_empty()
            || !self.base.get_states().contains(WidgetState::FOCUSED)
        {
            return;
        }
        let repaint = self.update_offset();
        self.update_caret();
        let scroll = if self.base.get_style_exts() & FWL_STYLEEXT_EDT_MULTI_LINE != 0 {
            self.update_scroll()
        } else {
            None
        };
        if repaint || scroll.is_some() {
            let mut invalid = scroll.map_or(CfxRectF::default(), |s| s.get_widget_rect());
            if repaint {
                invalid.union(self.engine_rect.get());
            }
            self.base.repaint_rect(invalid);
        }
    }
    fn update_caret(&self) {
        let mut caret = self.caret_rect.get();
        let engine = self.engine_rect.get();
        caret.offset(
            engine.left - self.scroll_x.get(),
            engine.top - self.scroll_y.get() + self.valign_offset.get(),
        );
        let client = self.get_client_rect();
        caret.intersect(client);
        if caret.left > client.right() {
            let right = caret.right();
            caret.left = client.right() - 1.;
            caret.width = right - caret.left;
        }
        if self.base.get_states().contains(WidgetState::FOCUSED) && !caret.is_empty() {
            self.show_caret(caret);
        } else {
            self.hide_caret(Some(caret));
        }
    }
    pub fn show_caret(&self, rect: CfxRectF) {
        let caret = self.caret.borrow().clone();
        if let Some(caret) = caret {
            caret.show_caret();
            if !rect.is_empty() {
                caret.base().set_widget_rect(rect);
            }
            self.base.repaint_rect(self.engine_rect.get());
            return;
        }
        let Some(outer) = self.base.get_outmost() else {
            return;
        };
        let Some(adapter) = outer.base().adapter_iface() else {
            return;
        };
        let mut rect = rect;
        let own = self.base.get_widget_rect();
        rect.offset(own.left, own.top);
        let mut current = self.base.outer();
        while let Some(widget) = current {
            let r = widget.get_widget_rect();
            rect.offset(r.left, r.top);
            current = widget.base().outer();
        }
        let rect = adapter.get_rotate_matrix().transform_rect(rect);
        adapter.display_caret(true, Some(rect));
    }
    pub fn hide_caret(&self, rect: Option<CfxRectF>) {
        let caret = self.caret.borrow().clone();
        if let Some(caret) = caret {
            caret.hide_caret();
            self.base.repaint_rect(self.engine_rect.get());
            return;
        }
        if let Some(outer) = self.base.get_outmost() {
            if let Some(adapter) = outer.base().adapter_iface() {
                adapter.display_caret(false, rect);
            }
        }
    }
    fn init_caret(&self) {
        if self.caret.borrow().is_some() {
            return;
        }
        let (Some(app), Some(owner)) = (self.base.app(), self.base.self_ref()) else {
            return;
        };
        let caret = CfwlCaret::new(&app, Properties::default(), Some(owner));
        caret.set_states(self.base.get_states() | WidgetState::INVISIBLE);
        *self.caret.borrow_mut() = Some(caret);
        self.update_cursor_rect();
    }
    fn init_vertical_scrollbar(&self) {
        if self.vert_scroll_bar.borrow().is_some() {
            return;
        }
        let (Some(app), Some(owner)) = (self.base.app(), self.base.self_ref()) else {
            return;
        };
        let scroll = CfwlScrollBar::new(
            &app,
            Properties {
                styles: WidgetStyle::NONE,
                style_exts: FWL_STYLEEXT_SCB_VERT,
                states: WidgetState::DISABLED | WidgetState::INVISIBLE,
            },
            Some(owner),
        );
        *self.vert_scroll_bar.borrow_mut() = Some(scroll);
    }
    fn content_height_overflow(&self) -> bool {
        self.engine.borrow_mut().get_contents_bounding_box().height
            > self.engine_rect.get().height + 1.
    }
    fn show_vertical_scrollbar(&self) -> bool {
        let styles = self.base.get_style_exts();
        (styles & FWL_STYLEEXT_EDT_SHOW_SCROLLBAR_FOCUS == 0
            || self.base.get_states().contains(WidgetState::FOCUSED))
            && self.base.get_styles().contains(WidgetStyle::VSCROLL)
            && styles & FWL_STYLEEXT_EDT_MULTI_LINE != 0
            && self.content_height_overflow()
    }
    fn layout(&self) {
        let client = self.get_client_rect();
        self.client_rect.set(client);
        let mut engine = client;
        let Some(app) = self.base.app() else {
            return;
        };
        let theme = app.theme_provider();
        let width = theme.get_scroll_bar_width();
        let outer = self.base.outer();
        if outer.is_none()
            || outer
                .as_ref()
                .is_some_and(|w| w.get_class_id() == FwlType::DateTimePicker)
        {
            let widget = outer
                .as_ref()
                .map_or(self as &dyn FwlWidget, |w| w.as_ref());
            let part = CfwlThemePart::new(Part::None, widget);
            let margin = theme.get_ui_margin(&part);
            engine.deflate(margin.left, margin.top, margin.width, margin.height);
        }
        self.engine_rect.set(engine);
        if self.show_vertical_scrollbar() {
            self.init_vertical_scrollbar();
            let scroll = self.vert_scroll_bar.borrow().clone();
            if let Some(scroll) = scroll {
                let rect = if self.base.get_style_exts() & FWL_STYLEEXT_EDT_OUTER_SCROLLBAR != 0 {
                    CfxRectF::new(
                        client.right() + EDIT_MARGIN,
                        client.top,
                        width,
                        client.height,
                    )
                } else {
                    engine.width -= width;
                    self.engine_rect.set(engine);
                    CfxRectF::new(client.right() - width, client.top, width, client.height)
                };
                scroll.base().set_widget_rect(rect);
                scroll.clear_states(WidgetState::INVISIBLE);
                scroll.update();
            }
        } else {
            let scroll = self.vert_scroll_bar.borrow().clone();
            if let Some(scroll) = scroll {
                scroll.set_states(WidgetState::INVISIBLE);
            }
        }
    }
    fn layout_scroll_bar(&self) {
        if self.base.get_style_exts() & FWL_STYLEEXT_EDT_SHOW_SCROLLBAR_FOCUS == 0 {
            return;
        }
        let show = self.show_vertical_scrollbar();
        if show && self.vert_scroll_bar.borrow().is_none() {
            self.init_vertical_scrollbar();
            let width = self
                .base
                .app()
                .map_or(0., |a| a.theme_provider().get_scroll_bar_width());
            let client = self.client_rect.get();
            let rect = if self.base.get_style_exts() & FWL_STYLEEXT_EDT_OUTER_SCROLLBAR != 0 {
                CfxRectF::new(
                    client.right() + EDIT_MARGIN,
                    client.top,
                    width,
                    client.height,
                )
            } else {
                CfxRectF::new(client.right() - width, client.top, width, client.height)
            };
            let scroll = self.vert_scroll_bar.borrow().clone();
            if let Some(scroll) = scroll {
                scroll.base().set_widget_rect(rect);
                scroll.update();
            }
        }
        let scroll = self.vert_scroll_bar.borrow().clone();
        if let Some(scroll) = scroll {
            if show {
                scroll.clear_states(WidgetState::INVISIBLE);
            } else {
                scroll.set_states(WidgetState::INVISIBLE);
            }
        }
        if show {
            self.update_scroll();
        }
    }
    fn update_scroll(&self) -> Option<Rc<CfwlScrollBar>> {
        let scroll = self.vert_scroll_bar.borrow().clone()?;
        if !scroll.base().is_visible() {
            return None;
        }
        let (bounds, step) = {
            let mut e = self.engine.borrow_mut();
            (e.get_contents_bounding_box(), e.get_line_space())
        };
        let rect = scroll.get_widget_rect();
        if rect.height < bounds.height {
            let range = (bounds.height - self.engine_rect.get().height).max(step);
            scroll.set_range(0., range);
            let pos = self.scroll_y.get().clamp(0., range.max(0.));
            scroll.set_pos(pos);
            scroll.set_track_pos(pos);
            scroll.set_page_size(rect.height);
            scroll.set_step_size(step);
            scroll.clear_states(WidgetState::DISABLED);
            scroll.update();
            Some(scroll)
        } else if !scroll.base().get_states().contains(WidgetState::DISABLED) {
            scroll.set_range(0., -1.);
            scroll.set_states(WidgetState::DISABLED);
            scroll.update();
            Some(scroll)
        } else {
            None
        }
    }
    fn device_to_engine(&self, point: CfxPointF) -> CfxPointF {
        let engine = self.engine_rect.get();
        CfxPointF::new(
            point.x + self.scroll_x.get() - engine.left,
            point.y + self.scroll_y.get() - engine.top - self.valign_offset.get(),
        )
    }
    fn focus_gained(&self) {
        self.base.set_states(WidgetState::FOCUSED);
        self.update_valignment();
        self.update_offset();
        self.update_caret();
        self.layout_scroll_bar();
    }
    fn focus_lost(&self) {
        let selected = self.has_selection();
        if self.base.get_states().contains(WidgetState::FOCUSED) {
            self.base.clear_states(WidgetState::FOCUSED);
            self.hide_caret(None);
            if selected {
                self.clear_selection();
            }
            self.update_offset();
        }
        self.layout_scroll_bar();
        if selected {
            self.base.repaint_rect(self.base.get_relative_rect());
        }
    }
    fn left_down(&self, point: CfxPointF, flags: u32) {
        if !self.base.is_enabled() {
            return;
        }
        self.lbutton_down.set(true);
        self.base.set_grab(true);
        let selected = self.has_selection();
        self.clear_selection();
        let index = self
            .engine
            .borrow_mut()
            .get_index_for_point(self.device_to_engine(point));
        self.mouse_anchor.set(if flags & KEY_FLAG_SHIFT != 0 {
            self.cursor.get()
        } else {
            index
        });
        if flags & KEY_FLAG_SHIFT != 0 && index != self.cursor.get() {
            self.set_selection(
                index.min(self.cursor.get()),
                index.abs_diff(self.cursor.get()),
            );
            self.base.repaint_rect(self.engine_rect.get());
        } else {
            self.set_cursor_position(index);
            if selected {
                self.base.repaint_rect(self.engine_rect.get());
            }
        }
    }
    fn double_click(&self, point: CfxPointF) {
        let index = self
            .engine
            .borrow_mut()
            .get_index_for_point(self.device_to_engine(point));
        let (start, count) = self.engine.borrow().bounds_for_word_at(index);
        self.set_selection(start, count);
        self.cursor.set(start + count);
        self.base.repaint_rect(self.engine_rect.get());
    }
    fn mouse_move(&self, point: CfxPointF, _flags: u32) {
        // Native incorrectly requires Shift for ordinary mouse selection.
        if !self.lbutton_down.get() {
            return;
        }
        let old = self.cursor.get();
        let index = self
            .engine
            .borrow_mut()
            .get_index_for_point(self.device_to_engine(point));
        self.set_cursor_position(index);
        if old == self.cursor.get() {
            return;
        }
        let start = self.mouse_anchor.get();
        self.set_selection(
            start.min(self.cursor.get()),
            start.abs_diff(self.cursor.get()),
        );
    }
    fn key_down(&self, key: u32, flags: u32) {
        let start = if self.has_selection() {
            let (start, count) = self.get_selection();
            if self.cursor.get() == start {
                start + count
            } else {
                start
            }
        } else {
            self.cursor.get()
        };
        let position = {
            let e = self.engine.borrow();
            match key {
                VKEY_LEFT => Some(e.get_index_left(self.cursor.get())),
                VKEY_RIGHT => Some(e.get_index_right(self.cursor.get())),
                VKEY_UP => Some(e.get_index_up(self.cursor.get())),
                VKEY_DOWN => Some(e.get_index_down(self.cursor.get())),
                VKEY_HOME => Some(if flags & KEY_FLAG_CTRL != 0 {
                    0
                } else {
                    e.get_index_at_start_of_line(self.cursor.get())
                }),
                VKEY_END => Some(if flags & KEY_FLAG_CTRL != 0 {
                    e.get_length()
                } else {
                    e.get_index_at_end_of_line(self.cursor.get())
                }),
                _ => None,
            }
        };
        if let Some(position) = position {
            self.set_cursor_position(position);
            if flags & KEY_FLAG_SHIFT == 0 && self.has_selection() {
                self.clear_selection();
                self.base.repaint_rect(self.engine_rect.get());
            }
        }
        if key == VKEY_DELETE
            && self.base.is_enabled()
            && self.base.get_style_exts() & FWL_STYLEEXT_EDT_READ_ONLY == 0
        {
            if self.has_selection() {
                let (s, n) = self.get_selection();
                if !self.delete_range(s, n).is_empty() && !self.base.is_destroyed() {
                    // Deleting a selected range collapses its caret to the
                    // start, independently of the selection's direction.
                    self.set_cursor_position(s);
                }
            } else {
                self.delete_range(self.cursor.get(), 1);
            }
            self.update_cursor_rect();
            self.update_caret();
        }
        if position.is_some() && flags & KEY_FLAG_SHIFT != 0 {
            if start != self.cursor.get() {
                self.set_selection(
                    start.min(self.cursor.get()),
                    start.abs_diff(self.cursor.get()),
                );
            } else {
                self.clear_selection();
            }
            self.base.repaint_rect(self.engine_rect.get());
        }
    }
    fn on_char(&self, ch: u32, flags: u32) {
        if !self.base.is_enabled() || self.base.get_style_exts() & FWL_STYLEEXT_EDT_READ_ONLY != 0 {
            return;
        }
        match ch {
            8 => {
                if self.has_selection() {
                    let (s, n) = self.get_selection();
                    self.delete_range(s, n);
                    self.set_cursor_position(s);
                } else if self.cursor.get() > 0 {
                    self.set_cursor_position(self.cursor.get() - 1);
                    self.delete_range(self.cursor.get(), 1);
                }
                self.update_cursor_rect();
                self.update_caret();
            }
            127 => {}
            9 => {
                self.insert_at(self.cursor.get(), &[9], RecordOperation::InsertRecord);
                self.set_cursor_position(self.cursor.get() + 1);
            }
            13 => {
                if self.base.get_style_exts() & FWL_STYLEEXT_EDT_WANT_RETURN != 0 {
                    self.insert_at(self.cursor.get(), &[10], RecordOperation::InsertRecord);
                    self.set_cursor_position(self.cursor.get() + 1);
                }
            }
            _ => {
                if ch < 32 || is_platform_shortcut_key(flags) {
                    return;
                }
                if self.has_selection() {
                    let (s, n) = self.get_selection();
                    self.replace_range(s, n, &[ch]);
                    self.set_cursor_position(s + 1);
                } else {
                    self.insert_at(self.cursor.get(), &[ch], RecordOperation::InsertRecord);
                    self.set_cursor_position(self.cursor.get() + 1);
                }
            }
        }
    }
    fn on_scroll(&self, scroll: &CfwlScrollBar, code: ScrollCode, mut pos: f32) -> bool {
        let (min, max) = scroll.get_range();
        let old = scroll.get_pos();
        let step = scroll.get_step_size();
        match code {
            ScrollCode::Min => pos = min,
            ScrollCode::Max => pos = max,
            ScrollCode::StepBackward => {
                pos -= step;
                if pos < min + step / 2. {
                    pos = min;
                }
            }
            ScrollCode::StepForward => {
                pos += step;
                if pos > max - step / 2. {
                    pos = max;
                }
            }
            ScrollCode::PageBackward => pos = (pos - scroll.get_page_size()).max(min),
            ScrollCode::PageForward => pos = (pos + scroll.get_page_size()).min(max),
            ScrollCode::EndScroll => return false,
            _ => {}
        }
        if old == pos {
            return true;
        }
        scroll.set_pos(pos);
        scroll.set_track_pos(pos);
        self.scroll_y.set(self.scroll_y.get() + pos - old);
        self.update_caret();
        let r = self.get_widget_rect();
        self.base
            .repaint_rect(CfxRectF::new(0., 0., r.width + 2., r.height + 2.));
        true
    }
}

#[cfg(test)]
#[path = "cfwl_edit_tests.rs"]
mod tests;
impl FwlWidget for CfwlEdit {
    fn as_edit(&self) -> Option<&super::cfwl_edit::CfwlEdit> {
        Some(self)
    }

    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::Edit
    }
    fn pre_finalize(&self) {
        if self.finalizing.replace(true) {
            return;
        }
        self.engine.borrow_mut().set_delegate(None);
        if self.base.get_states().contains(WidgetState::FOCUSED) {
            self.hide_caret(None);
        }
        self.base.pre_finalize();
    }
    fn get_widget_rect(&self) -> CfxRectF {
        let mut rect = self.base.get_widget_rect();
        if self.base.get_style_exts() & FWL_STYLEEXT_EDT_OUTER_SCROLLBAR != 0
            && self.show_vertical_scrollbar()
        {
            if let Some(app) = self.base.app() {
                rect.width += app.theme_provider().get_scroll_bar_width() + EDIT_MARGIN;
            }
        }
        rect
    }
    fn get_autosized_widget_rect(&self) -> CfxRectF {
        let text = self.get_text();
        let mut rect = if text.is_empty() {
            CfxRectF::default()
        } else {
            let size = self.base.calc_text_size(
                &text,
                self.base.get_style_exts() & FWL_STYLEEXT_EDT_MULTI_LINE != 0,
            );
            CfxRectF::new(0., 0., size.width, size.height)
        };
        self.base.inflate_widget_rect(&mut rect);
        rect
    }
    fn set_states(&self, states: WidgetState) {
        if states.intersects(WidgetState::DISABLED | WidgetState::INVISIBLE) {
            self.hide_caret(None);
        }
        self.base.set_states(states);
    }
    fn update(&self) {
        if self.base.is_locked() || self.base.is_destroyed() {
            return;
        }
        self.layout();
        if self.client_rect.get().is_empty() {
            return;
        }
        self.update_edit_engine();
        self.update_valignment();
        self.update_scroll();
        self.init_caret();
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        if self.base.get_style_exts() & FWL_STYLEEXT_EDT_OUTER_SCROLLBAR != 0
            && self.show_vertical_scrollbar()
        {
            if let Some(scroll) = self.vert_scroll_bar.borrow().as_ref() {
                if scroll.get_widget_rect().contains_point(point) {
                    return FwlWidgetHit::VScrollBar;
                }
            }
        }
        if self.client_rect.get().contains_point(point) {
            FwlWidgetHit::Edit
        } else {
            FwlWidgetHit::Unknown
        }
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if self.client_rect.get().is_empty() {
            return;
        }
        self.draw_content(graphics, matrix);
        if self.base.has_border() {
            self.base.draw_border(graphics, Part::Border, matrix);
        }
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        match message.data.clone() {
            MessageData::SetFocus => self.focus_gained(),
            MessageData::KillFocus { .. } => self.focus_lost(),
            MessageData::Mouse {
                command,
                pos,
                flags,
            } => match command {
                MouseCommand::LeftButtonDown => self.left_down(pos, flags),
                MouseCommand::LeftButtonUp => {
                    self.lbutton_down.set(false);
                    self.base.set_grab(false);
                }
                MouseCommand::LeftButtonDblClk => self.double_click(pos),
                MouseCommand::Move => self.mouse_move(pos, flags),
                MouseCommand::RightButtonDown => {
                    let index = self
                        .engine
                        .borrow_mut()
                        .get_index_for_point(self.device_to_engine(pos));
                    self.set_cursor_position(index);
                }
                _ => {}
            },
            MessageData::Key {
                command,
                key_code_or_char,
                flags,
            } => match command {
                KeyCommand::KeyDown => self.key_down(key_code_or_char, flags),
                KeyCommand::Char => self.on_char(key_code_or_char, flags),
            },
            _ => {}
        }
        if !self.base.is_destroyed() && message.destination.is_some() {
            self.base.on_process_message(message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        if let EventData::Scroll { code, pos } = event.data {
            let scroll = self.vert_scroll_bar.borrow().clone();
            if let (Some(scroll), Some(source)) = (scroll, event.source.as_ref()) {
                if scroll.base().id() == source.base().id() {
                    self.on_scroll(&scroll, code, pos);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "cfwl_edit_scroll_tests.rs"]
mod scroll_tests;

#[cfg(test)]
#[path = "cfwl_nested_widget_tests.rs"]
mod nested_widget_tests;
