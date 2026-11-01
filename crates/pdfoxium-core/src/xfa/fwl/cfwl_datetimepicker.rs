// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `cfwl_datetimepicker.{h,cpp}`. Children retain distinct widget IDs;
//! their outer references are weak app-owned identities, without owner cycles.
use super::{
    cfwl_app::CfwlApp,
    cfwl_datetimeedit::CfwlDateTimeEdit,
    cfwl_event::{EventData, FwlEvent},
    cfwl_message::{FwlMessage, MessageData, MouseCommand},
    cfwl_monthcalendar::CfwlMonthCalendar,
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
    cfwl_widget::{
        dispatch_message_to, CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef, WidgetState,
        WidgetStyle,
    },
    fwl_widgethit::FwlWidgetHit,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    fxjs::fx_date_helpers,
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{
    cell::{Cell, OnceCell},
    rc::Rc,
};

pub const FWL_STYLEEXT_DTP_SHORT_DATE_FORMAT: u32 = 1 << 1;
pub const FWL_STYLEEXT_DTP_EDIT_H_ALIGN_MASK: u32 = 3 << 4;
pub const FWL_STYLEEXT_DTP_EDIT_H_NEAR: u32 = 0;
pub const FWL_STYLEEXT_DTP_EDIT_H_CENTER: u32 = 1 << 4;
pub const FWL_STYLEEXT_DTP_EDIT_H_FAR: u32 = 2 << 4;
pub const FWL_STYLEEXT_DTP_EDIT_V_ALIGN_MASK: u32 = 3 << 6;
pub const FWL_STYLEEXT_DTP_EDIT_V_NEAR: u32 = 0;
pub const FWL_STYLEEXT_DTP_EDIT_V_CENTER: u32 = 1 << 6;
pub const FWL_STYLEEXT_DTP_EDIT_V_FAR: u32 = 2 << 6;
pub const FWL_STYLEEXT_DTP_EDIT_JUSTIFIED: u32 = 1 << 8;
const DATE_TIME_PICKER_HEIGHT: f32 = 20.;
const EDIT_READ_ONLY: u32 = 1;
const EDIT_H_ALIGN_MASK: u32 = 3 << 18;
const EDIT_V_ALIGN_MASK: u32 = 3 << 20;
const EDIT_H_ALIGN_MODE_MASK: u32 = 3 << 22;

pub struct CfwlDateTimePicker {
    base: CfwlWidget,
    edit: OnceCell<Rc<CfwlDateTimeEdit>>,
    month_cal: OnceCell<Rc<CfwlMonthCalendar>>,
    left_button_down: Cell<bool>,
    button_state: Cell<PartState>,
    year: Cell<i32>,
    month: Cell<i32>,
    day: Cell<i32>,
    button_width: Cell<f32>,
    button_rect: Cell<CfxRectF>,
    client_rect: Cell<CfxRectF>,
    popup_generation: Cell<u64>,
    showing: Cell<bool>,
    selection_generation: Cell<u64>,
}
struct Showing<'a>(&'a Cell<bool>);
impl Drop for Showing<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl CfwlDateTimePicker {
    pub fn new(app: &Rc<CfwlApp>) -> Rc<Self> {
        let picker = Rc::new(Self {
            base: CfwlWidget::new(
                app,
                Properties {
                    styles: WidgetStyle::OVER_LAPPER,
                    style_exts: FWL_STYLEEXT_DTP_SHORT_DATE_FORMAT,
                    states: WidgetState::NONE,
                },
                None,
            ),
            edit: OnceCell::new(),
            month_cal: OnceCell::new(),
            left_button_down: Cell::new(false),
            button_state: Cell::new(PartState::CHECKED),
            year: Cell::new(-1),
            month: Cell::new(-1),
            day: Cell::new(-1),
            button_width: Cell::new(0.),
            button_rect: Cell::new(CfxRectF::default()),
            client_rect: Cell::new(CfxRectF::default()),
            popup_generation: Cell::new(0),
            showing: Cell::new(false),
            selection_generation: Cell::new(0),
        });
        app.register_widget(picker.clone());
        let owner: WidgetRef = picker.clone();
        let edit = CfwlDateTimeEdit::new(app, Properties::default(), Some(owner.clone()));
        assert!(picker.edit.set(edit).is_ok());
        let calendar = CfwlMonthCalendar::new(
            app,
            Properties {
                styles: WidgetStyle::POPUP | WidgetStyle::BORDER,
                style_exts: 0,
                states: WidgetState::INVISIBLE,
            },
            Some(owner.clone()),
        );
        assert!(picker.month_cal.set(calendar).is_ok());
        // Both children are initialized before measurement can call host code.
        let size = picker.month_calendar().get_autosized_widget_rect().size();
        if !picker.base.is_destroyed() {
            picker
                .month_calendar()
                .base()
                .set_widget_rect(CfxRectF::from_point_size(CfxPointF::default(), size));
            let edit: WidgetRef = picker.edit().clone();
            let calendar: WidgetRef = picker.month_calendar().clone();
            app.note_driver()
                .register_event_target(&owner, Some(&calendar));
            app.note_driver().register_event_target(&owner, Some(&edit));
        }
        picker
    }
    pub fn edit(&self) -> &Rc<CfwlDateTimeEdit> {
        self.edit.get().expect("initialized date edit")
    }
    pub fn month_calendar(&self) -> &Rc<CfwlMonthCalendar> {
        self.month_cal.get().expect("initialized calendar")
    }
    pub fn button_rect(&self) -> CfxRectF {
        self.button_rect.get()
    }
    pub fn get_cur_sel(&self) -> (i32, i32, i32) {
        (self.year.get(), self.month.get(), self.day.get())
    }
    pub fn set_cur_sel(&self, year: i32, month: i32, day: i32) {
        if !valid_date(year, month, day) || self.base.is_destroyed() {
            return;
        }
        self.selection_generation
            .set(self.selection_generation.get().wrapping_add(1));
        self.year.set(year);
        self.month.set(month);
        self.day.set(day);
        self.month_calendar().set_select(year, month, day);
    }
    pub fn set_edit_text(&self, text: &[u32]) {
        if self.base.is_destroyed() {
            return;
        }
        self.edit().set_text(text);
        if self.base.is_destroyed() {
            return;
        }
        self.base.repaint_rect(self.client_rect.get());
        // Source has no source target on this programmatic EditChanged event.
        let mut event = FwlEvent::new(None, EventData::EditChanged);
        self.base.dispatch_event(&mut event);
    }
    pub fn get_edit_text(&self) -> Vec<u32> {
        self.edit().get_text()
    }
    pub fn get_edit_text_length(&self) -> usize {
        self.edit().get_text_length()
    }
    pub fn clear_text(&self) {
        self.edit().clear_text();
    }
    pub fn select_all(&self) {
        self.edit().select_all();
    }
    pub fn clear_selection(&self) {
        self.edit().clear_selection();
    }
    pub fn has_selection(&self) -> bool {
        self.edit().has_selection()
    }
    pub fn get_selection(&self) -> (usize, usize) {
        self.edit().get_selection()
    }
    pub fn copy(&self) -> Option<Vec<u32>> {
        self.edit().copy()
    }
    pub fn cut(&self) -> Option<Vec<u32>> {
        self.edit().cut()
    }
    pub fn paste(&self, text: &[u32]) -> bool {
        self.edit().paste(text)
    }
    pub fn undo(&self) -> bool {
        self.edit().undo()
    }
    pub fn redo(&self) -> bool {
        self.edit().redo()
    }
    pub fn can_undo(&self) -> bool {
        self.edit().can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.edit().can_redo()
    }
    pub fn set_edit_limit(&self, limit: i32) {
        self.edit().set_limit(limit);
    }
    pub fn modify_edit_style_exts(&self, added: u32, removed: u32) {
        self.edit().modify_style_exts(added, removed);
    }
    pub fn get_bbox(&self) -> CfxRectF {
        let mut rect = self.base.get_widget_rect();
        if self.needs_to_show_button() {
            rect.width += self.button_width.get();
        }
        if self.is_month_calendar_visible() {
            let mut calendar = self.month_calendar().get_widget_rect();
            let own = self.base.get_widget_rect();
            calendar.offset(own.left, own.top);
            rect.union(calendar);
        }
        rect
    }
    pub fn format_date_string(&self, year: i32, month: i32, day: i32) -> Vec<u32> {
        let text = if self.base.get_style_exts() & FWL_STYLEEXT_DTP_SHORT_DATE_FORMAT != 0 {
            format!("{year}-{month}-{day}")
        } else {
            format!("{year} Year {month} Month {day} Day")
        };
        text.chars().map(u32::from).collect()
    }
    pub fn is_month_calendar_visible(&self) -> bool {
        !self.base.is_destroyed()
            && self
                .month_cal
                .get()
                .is_some_and(|calendar| calendar.base().is_visible())
    }
    fn popup_live(&self, generation: u64) -> bool {
        self.popup_generation.get() == generation
            && !self.base.is_destroyed()
            && self.base.is_enabled()
            && self.base.is_visible()
    }
    pub fn show_month_calendar(&self) {
        if self.is_month_calendar_visible()
            || !self.base.is_enabled()
            || !self.base.is_visible()
            || self.base.is_destroyed()
            || self.showing.replace(true)
        {
            return;
        }
        let _showing = Showing(&self.showing);
        let generation = self.popup_generation.get().wrapping_add(1);
        self.popup_generation.set(generation);
        let mut popup = self.month_calendar().get_autosized_widget_rect();
        if !self.popup_live(generation) {
            return;
        }
        let height = popup.height;
        let mut anchor = self.base.get_widget_rect();
        anchor.width = popup.width;
        popup.left = self.client_rect.get().left;
        popup.top = anchor.height;
        let (Some(app), Some(owner)) = (self.base.app(), self.base.self_ref()) else {
            return;
        };
        if !app
            .widget_mgr()
            .get_adapter_popup_pos(&owner, height, height, anchor, &mut popup)
            || !self.popup_live(generation)
        {
            return;
        }
        self.month_calendar().base().set_widget_rect(popup);
        let (year, month, day) = self.get_cur_sel();
        if year > 0 && month > 0 && day > 0 {
            self.month_calendar().set_select(year, month, day);
        }
        if !self.popup_live(generation) {
            return;
        }
        self.month_calendar().update();
        if !self.popup_live(generation) {
            return;
        }
        self.month_calendar().clear_states(WidgetState::INVISIBLE);
        let mut message = FwlMessage::set_focus(Some(self.month_calendar().clone()));
        let edit: WidgetRef = self.edit().clone();
        dispatch_message_to(&edit, &mut message);
        if self.popup_live(generation) {
            self.repaint_inflated_month_cal_rect();
        }
    }
    pub fn hide_month_calendar(&self) {
        self.popup_generation
            .set(self.popup_generation.get().wrapping_add(1));
        if !self.is_month_calendar_visible() {
            return;
        }
        self.month_calendar().set_states(WidgetState::INVISIBLE);
        self.repaint_inflated_month_cal_rect();
    }
    fn repaint_inflated_month_cal_rect(&self) {
        let own = self.base.get_widget_rect();
        let mut invalid = CfxRectF::new(0., 0., own.width, own.height);
        invalid.union(self.month_calendar().get_widget_rect());
        invalid.inflate_xy(2., 2.);
        self.base.repaint_rect(invalid);
    }
    fn reset_edit_alignment(&self) {
        let style = self.base.get_style_exts();
        let horizontal = match style & FWL_STYLEEXT_DTP_EDIT_H_ALIGN_MASK {
            FWL_STYLEEXT_DTP_EDIT_H_CENTER => 1 << 18,
            FWL_STYLEEXT_DTP_EDIT_H_FAR => 2 << 18,
            _ => 0,
        };
        let vertical = match style & FWL_STYLEEXT_DTP_EDIT_V_ALIGN_MASK {
            FWL_STYLEEXT_DTP_EDIT_V_CENTER => 1 << 20,
            FWL_STYLEEXT_DTP_EDIT_V_FAR => 2 << 20,
            _ => 0,
        };
        let justified = if style & FWL_STYLEEXT_DTP_EDIT_JUSTIFIED != 0 {
            1 << 22
        } else {
            0
        };
        self.edit().modify_style_exts(
            horizontal | vertical | justified,
            EDIT_H_ALIGN_MASK | EDIT_H_ALIGN_MODE_MASK | EDIT_V_ALIGN_MASK,
        );
    }
    pub fn process_sel_changed(&self, year: i32, month: i32, day: i32) {
        if !valid_date(year, month, day) || self.base.is_destroyed() {
            return;
        }
        let generation = self.selection_generation.get().wrapping_add(1);
        self.selection_generation.set(generation);
        self.year.set(year);
        self.month.set(month);
        self.day.set(day);
        self.edit()
            .set_text(&self.format_date_string(year, month, day));
        if self.base.is_destroyed() || generation != self.selection_generation.get() {
            return;
        }
        self.edit().update();
        if self.base.is_destroyed() || generation != self.selection_generation.get() {
            return;
        }
        self.base.repaint_rect(self.client_rect.get());
        if self.base.is_destroyed() || generation != self.selection_generation.get() {
            return;
        }
        let mut event = FwlEvent::date_selected(self.base.self_ref(), year, month, day);
        self.base.dispatch_event(&mut event);
    }
    fn needs_to_show_button(&self) -> bool {
        self.base.get_states().contains(WidgetState::FOCUSED)
            || self
                .month_cal
                .get()
                .is_some_and(|w| w.base().get_states().contains(WidgetState::FOCUSED))
            || self
                .edit
                .get()
                .is_some_and(|w| w.base().get_states().contains(WidgetState::FOCUSED))
    }
    fn on_focus_gained(&self, message: &mut FwlMessage) {
        self.base.set_states(WidgetState::FOCUSED);
        if self.edit().base().get_style_exts() & EDIT_READ_ONLY == 0 {
            let own = self.base.get_widget_rect();
            self.button_rect.set(CfxRectF::new(
                own.width,
                0.,
                self.button_width.get(),
                own.height - 1.,
            ));
        }
        let mut invalid = self.button_rect.get();
        let edit: WidgetRef = self.edit().clone();
        message.destination = Some(edit.clone());
        dispatch_message_to(&edit, message);
        invalid.inflate_xy(2., 2.);
        self.base.repaint_rect(invalid);
    }
    fn on_focus_lost(&self, message: &mut FwlMessage) {
        let mut invalid = self.button_rect.get();
        self.base.clear_states(WidgetState::FOCUSED);
        self.button_rect.set(CfxRectF::default());
        self.hide_month_calendar();
        if self
            .edit()
            .base()
            .get_states()
            .contains(WidgetState::FOCUSED)
        {
            let edit: WidgetRef = self.edit().clone();
            dispatch_message_to(&edit, message);
        }
        invalid.inflate_xy(2., 2.);
        self.base.repaint_rect(invalid);
    }
}
fn valid_date(year: i32, month: i32, day: i32) -> bool {
    if !(1..3000).contains(&year) || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    let time = fx_date_helpers::make_day(year, month - 1, day) * 86_400_000.;
    fx_date_helpers::get_year_from_time(time) == year
        && fx_date_helpers::get_month_from_time(time) == month - 1
        && fx_date_helpers::get_day_from_time(time) == day
}
impl FwlWidget for CfwlDateTimePicker {
    fn as_edit(&self) -> Option<&super::cfwl_edit::CfwlEdit> {
        Some(self.edit().as_ref())
    }

    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::DateTimePicker
    }
    fn as_datetime_picker(&self) -> Option<&CfwlDateTimePicker> {
        Some(self)
    }
    fn draws_child(&self, id: super::cfwl_widget::WidgetId) -> bool {
        self.edit.get().is_some_and(|edit| edit.base().id() == id)
            || self
                .month_cal
                .get()
                .is_some_and(|calendar| calendar.base().id() == id)
    }
    fn pre_finalize(&self) {
        self.popup_generation
            .set(self.popup_generation.get().wrapping_add(1));
        self.base.unregister_event_target();
        self.base.pre_finalize();
    }
    fn update(&self) {
        if self.base.is_locked() || self.base.is_destroyed() {
            return;
        }
        self.client_rect.set(self.get_client_rect());
        self.edit().base().set_widget_rect(self.client_rect.get());
        self.reset_edit_alignment();
        self.edit().update();
        if self.base.is_destroyed() {
            return;
        }
        let Some(app) = self.base.app() else {
            return;
        };
        self.button_width
            .set(app.theme_provider().get_scroll_bar_width());
        if self.base.is_destroyed() {
            return;
        }
        let calendar = self.month_calendar().get_autosized_widget_rect();
        if self.base.is_destroyed() {
            return;
        }
        self.month_calendar().base().set_widget_rect(CfxRectF::new(
            calendar.left,
            calendar.top + DATE_TIME_PICKER_HEIGHT,
            calendar.width,
            calendar.height,
        ));
        self.month_calendar().update();
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        let own = self.base.get_widget_rect();
        let mut rect = CfxRectF::new(0., 0., own.width, own.height);
        if rect.contains_point(point) {
            return FwlWidgetHit::Edit;
        }
        if self.needs_to_show_button() {
            rect.width += self.button_width.get();
        }
        if rect.contains_point(point)
            || (self.is_month_calendar_visible()
                && self
                    .month_calendar()
                    .get_widget_rect()
                    .contains_point(point))
        {
            FwlWidgetHit::Client
        } else {
            FwlWidgetHit::Unknown
        }
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics, matrix: CfxMatrix) {
        if self.base.is_destroyed() {
            return;
        }
        if self.base.has_border() {
            self.base.draw_border(graphics, Part::Border, matrix);
        }
        if !self.button_rect.get().is_empty() {
            if let Some(app) = self.base.app() {
                let mut params = CfwlThemeBackground::new(Part::DropDownButton, self);
                params.states = self.button_state.get();
                params.part_rect = self.button_rect.get();
                params.matrix = matrix;
                app.theme_provider().draw_background(&params, graphics);
            }
        }
        if self.base.is_destroyed() {
            return;
        }
        let edit_rect = self.edit().get_widget_rect();
        let mut edit_matrix = CfxMatrix::new(1., 0., 0., 1., edit_rect.left, edit_rect.top);
        edit_matrix.concat(matrix);
        self.edit().draw_widget(graphics, edit_matrix);
        if !self.base.is_destroyed() && self.is_month_calendar_visible() {
            let rect = self.month_calendar().get_widget_rect();
            let mut cal_matrix = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
            cal_matrix.concat(matrix);
            self.month_calendar().draw_widget(graphics, cal_matrix);
        }
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        if self.base.is_destroyed() {
            return;
        }
        match message.data.clone() {
            MessageData::SetFocus => self.on_focus_gained(message),
            MessageData::KillFocus { .. } => self.on_focus_lost(message),
            MessageData::Mouse { command, pos, .. } => {
                if !self.base.is_enabled() {
                    return;
                }
                match command {
                    MouseCommand::LeftButtonDown => {
                        if self.button_rect.get().contains_point(pos) {
                            if self.is_month_calendar_visible() {
                                self.hide_month_calendar();
                            } else {
                                self.show_month_calendar();
                                self.left_button_down.set(true);
                                self.base.repaint_rect(self.client_rect.get());
                            }
                        }
                    }
                    MouseCommand::LeftButtonUp => {
                        self.left_button_down.set(false);
                        self.button_state
                            .set(if self.button_rect.get().contains_point(pos) {
                                PartState::HOVERED
                            } else {
                                PartState::NORMAL
                            });
                        self.base.repaint_rect(self.button_rect.get());
                    }
                    MouseCommand::Move => {
                        if !self.button_rect.get().contains_point(pos) {
                            self.button_state.set(PartState::NORMAL);
                        }
                        self.base.repaint_rect(self.button_rect.get());
                    }
                    MouseCommand::Leave => {
                        self.button_state.set(PartState::NORMAL);
                        self.base.repaint_rect(self.button_rect.get());
                    }
                    _ => {}
                }
            }
            MessageData::Key { .. } => {
                if self
                    .edit()
                    .base()
                    .get_states()
                    .contains(WidgetState::FOCUSED)
                {
                    let edit: WidgetRef = self.edit().clone();
                    dispatch_message_to(&edit, message);
                    return;
                }
            }
            _ => {}
        }
        if !self.base.is_destroyed() && message.destination.is_some() {
            self.base.on_process_message(message);
        }
    }
    fn set_states(&self, states: WidgetState) {
        self.base.set_states(states);
        let child_states = states & (WidgetState::INVISIBLE | WidgetState::DISABLED);
        if !child_states.is_empty() {
            self.hide_month_calendar();
            self.edit().set_states(child_states);
        }
    }
    fn clear_states(&self, states: WidgetState) {
        self.base.clear_states(states);
        self.edit()
            .clear_states(states & (WidgetState::INVISIBLE | WidgetState::DISABLED));
    }
}

#[cfg(test)]
#[path = "cfwl_datetimepicker_tests.rs"]
mod tests;
