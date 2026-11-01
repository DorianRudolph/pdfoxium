// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fpdfsdk/formfiller/cffl_formfield.{h,cpp}`. Concrete fillers compose this
//! base, binding their weak virtual receiver after Rc construction. Storage
//! borrows end before window, script or embedder callbacks.
use super::cffl_fieldaction::CfflFieldAction;
use super::cffl_interactiveformfiller::CfflInteractiveFormFiller;
use super::cffl_perwindowdata::CfflPerWindowData;
use crate::fpdfdoc::cpdf_aaction::AActionType;
use crate::fpdfdoc::cpdf_annot::AppearanceMode;
use crate::fpdfdoc::cpdf_formfield::FormFieldType;
use crate::fpdfsdk::cpdfsdk_baannot::BorderStyle;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::pwl::cpwl_wnd::*;
use crate::fpdfsdk::pwl::ipwl_fillernotify::{FillerNotify, PerWindowData};
use crate::fpdfsdk::pwl::pwl_draw::PwlDraw;
use crate::fxcrt::cfx_timer::{CfxTimer, TimerCallback};
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF, FxRect};
use crate::fxge::cfx_color::CfxColor;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

pub trait FormField: Any {
    fn as_any(&self) -> &dyn Any;
    fn base(&self) -> &CfflFormField;
    fn get_create_param(&self) -> CreateParams {
        self.base().get_create_param()
    }
    fn new_pwl_window(
        &self,
        cp: CreateParams,
        data: Box<CfflPerWindowData>,
    ) -> Option<Rc<dyn Window>>;
    fn on_draw(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
    ) {
        self.base().on_draw(view, widget, draw, matrix);
    }
    fn on_draw_deactive(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
    ) {
        self.base().on_draw_deactive(view, widget, draw, matrix);
    }
    fn on_mouse_enter(&self, _view: &Rc<CpdfSdkPageView>) {}
    fn on_mouse_exit(&self, _view: &Rc<CpdfSdkPageView>) {
        self.base().cancel_timer();
    }
    fn on_l_button_down(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.base().on_l_button_down(view, widget, flags, point)
    }
    fn on_l_button_up(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.base().on_l_button_up(view, widget, flags, point)
    }
    fn on_l_button_dbl_clk(
        &self,
        view: &Rc<CpdfSdkPageView>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.base().on_l_button_dbl_clk(view, flags, point)
    }
    fn on_mouse_move(&self, view: &Rc<CpdfSdkPageView>, flags: u32, point: CfxPointF) -> bool {
        self.base().on_mouse_move(view, flags, point)
    }
    fn on_mouse_wheel(
        &self,
        view: &Rc<CpdfSdkPageView>,
        flags: u32,
        point: CfxPointF,
        delta: CfxPointF,
    ) -> bool {
        self.base().on_mouse_wheel(view, flags, point, delta)
    }
    fn on_r_button_down(&self, view: &Rc<CpdfSdkPageView>, flags: u32, point: CfxPointF) -> bool {
        self.base().on_r_button_down(view, flags, point)
    }
    fn on_r_button_up(&self, view: &Rc<CpdfSdkPageView>, flags: u32, point: CfxPointF) -> bool {
        self.base().on_r_button_up(view, flags, point)
    }
    fn on_key_down(&self, key: i32, flags: u32) -> bool {
        self.base().on_key_down(key, flags)
    }
    fn on_char(&self, widget: &Rc<CpdfSdkWidget>, ch: u32, flags: u32) -> bool {
        self.base().on_char(widget, ch, flags)
    }
    fn set_index_selected(&self, _index: i32, _selected: bool) -> bool {
        false
    }
    fn is_index_selected(&self, _index: i32) -> bool {
        false
    }
    fn is_field_full(&self, _view: &Rc<CpdfSdkPageView>) -> bool {
        false
    }
    fn is_data_changed(&self, _view: &Rc<CpdfSdkPageView>) -> bool {
        false
    }
    fn save_data(&self, _view: &Rc<CpdfSdkPageView>) {}
    fn get_action_data(
        &self,
        _view: &Rc<CpdfSdkPageView>,
        _kind: AActionType,
        data: &mut CfflFieldAction,
    ) {
        if let Some(widget) = self.base().widget() {
            data.s_value = widget.value();
        }
    }
    fn set_action_data(
        &self,
        _view: &Rc<CpdfSdkPageView>,
        _kind: AActionType,
        _data: &CfflFieldAction,
    ) {
    }
    fn save_pwl_window_state(&self, _view: &Rc<CpdfSdkPageView>) {}
    fn recreate_pwl_window_from_saved_state(&self, _view: &Rc<CpdfSdkPageView>) {}
    fn reset_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.base().pwl_window(view)
    }
    fn restore_pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.base().pwl_window(view)
    }
    fn on_set_focus_for_edit(&self, _edit: &dyn Window) {}
    fn on_timer_fired(&self) {}
}

pub struct CfflFormField {
    filler: Weak<CfflInteractiveFormFiller>,
    widget: Weak<CpdfSdkWidget>,
    owner: RefCell<Option<Weak<dyn FormField>>>,
    windows: RefCell<BTreeMap<u64, Rc<dyn Window>>>,
    valid: Cell<bool>,
    alive: Cell<bool>,
    timer: RefCell<Option<Rc<CfxTimer>>>,
    provider: RefCell<Option<Rc<dyn ProviderIface>>>,
}
impl CfflFormField {
    pub fn new(filler: &Rc<CfflInteractiveFormFiller>, widget: &Rc<CpdfSdkWidget>) -> Self {
        Self {
            filler: Rc::downgrade(filler),
            widget: Rc::downgrade(widget),
            owner: RefCell::new(None),
            windows: RefCell::new(BTreeMap::new()),
            valid: Cell::new(false),
            alive: Cell::new(true),
            timer: RefCell::new(None),
            provider: RefCell::new(None),
        }
    }
    pub fn bind_owner(&self, owner: &Rc<dyn FormField>) {
        *self.owner.borrow_mut() = Some(Rc::downgrade(owner));
        *self.provider.borrow_mut() = Some(Rc::new(FieldProvider {
            field: Rc::downgrade(owner),
        }));
    }
    pub fn owner(&self) -> Option<Rc<dyn FormField>> {
        self.owner.borrow().as_ref().and_then(Weak::upgrade)
    }
    pub fn widget(&self) -> Option<Rc<CpdfSdkWidget>> {
        self.widget.upgrade().filter(|widget| widget.is_alive())
    }
    pub fn filler(&self) -> Option<Rc<CfflInteractiveFormFiller>> {
        self.filler.upgrade()
    }
    pub fn is_alive(&self) -> bool {
        self.alive.get() && self.filler.upgrade().is_some() && self.widget().is_some()
    }
    pub fn is_valid(&self) -> bool {
        self.valid.get() && self.is_alive()
    }
    pub fn set_valid(&self, valid: bool) {
        self.valid.set(valid);
    }
    pub fn destroy(&self) {
        if !self.alive.replace(false) {
            return;
        }
        self.valid.set(false);
        self.cancel_timer();
        self.destroy_windows();
        self.provider.borrow_mut().take();
    }
    pub fn cancel_timer(&self) {
        let timer = self.timer.borrow_mut().take();
        if let Some(timer) = timer {
            timer.cancel();
        }
    }
    pub fn set_timer(&self, timer: Option<Rc<CfxTimer>>) {
        self.cancel_timer();
        *self.timer.borrow_mut() = timer;
    }
    pub fn destroy_windows(&self) {
        loop {
            let window = self
                .windows
                .borrow_mut()
                .pop_first()
                .map(|(_, window)| window);
            let Some(window) = window else {
                break;
            };
            if let Some(provider) = self.provider.borrow().clone() {
                window.wnd().invalidate_provider(&provider);
            }
            window.wnd().destroy();
        }
    }
    pub fn current_page_view(&self) -> Option<Rc<CpdfSdkPageView>> {
        self.widget()?.page_view()
    }
    pub fn pwl_window(&self, view: &Rc<CpdfSdkPageView>) -> Option<Rc<dyn Window>> {
        self.windows
            .borrow()
            .get(&view.id())
            .cloned()
            .filter(|window| window.wnd().is_alive())
    }
    pub fn create_or_update_pwl_window(
        &self,
        view: &Rc<CpdfSdkPageView>,
    ) -> Option<Rc<dyn Window>> {
        if !self.is_alive() || !view.is_alive() {
            return None;
        }
        let widget = self.widget()?;
        let owner = self.owner()?;
        if let Some(window) = self.pwl_window(view) {
            let data = window
                .wnd()
                .attached_data()?
                .as_any()
                .downcast_ref::<CfflPerWindowData>()?;
            if data.appearance_age_equals(widget.appearance_age()) {
                return Some(window);
            }
            return self.reset_pwl_window_for_value_age_internal(view, &widget, data.value_age());
        }
        let cp = owner.get_create_param();
        let data = Box::new(CfflPerWindowData::new(
            &widget,
            view,
            widget.appearance_age(),
            0,
        ));
        let window = owner.new_pwl_window(cp, data)?;
        if !self.is_alive() || !view.is_alive() || !window.wnd().is_alive() {
            window.wnd().destroy();
            return None;
        }
        self.windows.borrow_mut().insert(view.id(), window.clone());
        Some(window)
    }
    pub fn destroy_pwl_window(&self, view: &Rc<CpdfSdkPageView>) {
        let window = self.windows.borrow_mut().remove(&view.id());
        if let Some(window) = window {
            window.wnd().destroy();
        }
    }
    pub fn reset_pwl_window_for_value_age(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        age: u32,
    ) {
        self.reset_pwl_window_for_value_age_internal(view, widget, age);
    }
    fn reset_pwl_window_for_value_age_internal(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        age: u32,
    ) -> Option<Rc<dyn Window>> {
        let owner = self.owner()?;
        if age == widget.value_age() {
            owner.restore_pwl_window(view)
        } else {
            owner.reset_pwl_window(view)
        }
    }
    pub fn get_per_pwl_window_data(
        &self,
        view: &Rc<CpdfSdkPageView>,
    ) -> Option<Box<CfflPerWindowData>> {
        let window = self.pwl_window(view)?;
        let data = window
            .wnd()
            .attached_data()?
            .as_any()
            .downcast_ref::<CfflPerWindowData>()?;
        Some(Box::new(data.copy()))
    }
    pub fn current_matrix(&self) -> CfxMatrix {
        self.widget().map_or(CfxMatrix::default(), |widget| {
            widget_matrix(widget.rect(), widget.rotate())
        })
    }
    pub fn pdf_annot_rect(&self) -> CfxFloatRect {
        self.widget().map_or(CfxFloatRect::default(), |widget| {
            let rect = widget.rect();
            let (mut width, mut height) = (rect.width(), rect.height());
            if (widget.rotate() / 90) & 1 != 0 {
                std::mem::swap(&mut width, &mut height);
            }
            CfxFloatRect::new(0., 0., width, height)
        })
    }
    pub fn ffl_to_pwl(&self, point: CfxPointF) -> CfxPointF {
        self.current_matrix().get_inverse().transform(point)
    }
    pub fn pwl_to_ffl(&self, point: CfxPointF) -> CfxPointF {
        self.current_matrix().transform(point)
    }
    pub fn ffl_to_pwl_rect(&self, rect: CfxFloatRect) -> CfxFloatRect {
        self.current_matrix()
            .get_inverse()
            .transform_float_rect(rect)
    }
    pub fn pwl_to_ffl_rect(&self, rect: CfxFloatRect) -> CfxFloatRect {
        self.current_matrix().transform_float_rect(rect)
    }
    pub fn get_focus_box(&self, view: &Rc<CpdfSdkPageView>) -> CfxFloatRect {
        let Some(window) = self.pwl_window(view) else {
            return CfxFloatRect::default();
        };
        let rect = self.pwl_to_ffl_rect(window.focus_rect());
        if view.bbox().contains_rect(rect) {
            rect
        } else {
            CfxFloatRect::default()
        }
    }
    pub fn get_view_bbox(&self, view: &Rc<CpdfSdkPageView>) -> FxRect {
        let mut rect = self
            .pwl_window(view)
            .map(|window| self.pwl_to_ffl_rect(window.wnd().window_rect()))
            .or_else(|| self.widget().map(|widget| widget.rect()))
            .unwrap_or_default();
        let focus = self.get_focus_box(view);
        if !focus.is_empty() {
            rect.union(focus);
        }
        if !rect.is_empty() {
            rect.inflate_xy(1., 1.);
            rect.normalize();
        }
        rect.get_outer_rect()
    }
    pub fn on_draw(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
    ) {
        if let Some(window) = self.pwl_window(view) {
            window.draw_appearance(draw, self.current_matrix() * matrix);
        } else if widget.annot().is_visible() {
            widget.draw_appearance(draw, matrix, AppearanceMode::Normal);
        }
    }
    pub fn on_draw_deactive(
        &self,
        _view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
    ) {
        widget.draw_appearance(draw, matrix, AppearanceMode::Normal);
    }
    pub fn on_l_button_down(
        &self,
        view: &Rc<CpdfSdkPageView>,
        _widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        let Some(window) = self.create_or_update_pwl_window(view) else {
            return false;
        };
        self.valid.set(true);
        let rect = self.get_view_bbox(view);
        self.invalidate_rect(rect);
        self.is_alive()
            && window.wnd().is_alive()
            && rect.contains(point.x as i32, point.y as i32)
            && window.on_l_button_down(flags, self.ffl_to_pwl(point))
    }
    pub fn on_l_button_up(
        &self,
        view: &Rc<CpdfSdkPageView>,
        _widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        let Some(window) = self.pwl_window(view) else {
            return false;
        };
        self.invalidate_rect(self.get_view_bbox(view));
        if !self.is_alive() || !window.wnd().is_alive() {
            return false;
        }
        window.on_l_button_up(flags, self.ffl_to_pwl(point));
        self.is_alive()
    }
    pub fn on_l_button_dbl_clk(
        &self,
        view: &Rc<CpdfSdkPageView>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        let Some(window) = self.pwl_window(view) else {
            return false;
        };
        window.on_l_button_dbl_clk(flags, self.ffl_to_pwl(point));
        self.is_alive()
    }
    pub fn on_mouse_move(&self, view: &Rc<CpdfSdkPageView>, flags: u32, point: CfxPointF) -> bool {
        let Some(window) = self.pwl_window(view) else {
            return false;
        };
        window.on_mouse_move(flags, self.ffl_to_pwl(point));
        self.is_alive()
    }
    pub fn on_mouse_wheel(
        &self,
        view: &Rc<CpdfSdkPageView>,
        flags: u32,
        point: CfxPointF,
        delta: CfxPointF,
    ) -> bool {
        if !self.is_valid() {
            return false;
        }
        self.create_or_update_pwl_window(view)
            .is_some_and(|window| window.on_mouse_wheel(flags, self.ffl_to_pwl(point), delta))
    }
    pub fn on_r_button_down(
        &self,
        view: &Rc<CpdfSdkPageView>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.create_or_update_pwl_window(view)
            .is_some_and(|window| window.on_r_button_down(flags, self.ffl_to_pwl(point)))
    }
    pub fn on_r_button_up(&self, view: &Rc<CpdfSdkPageView>, flags: u32, point: CfxPointF) -> bool {
        self.pwl_window(view)
            .is_some_and(|window| window.on_r_button_up(flags, self.ffl_to_pwl(point)))
    }
    pub fn on_key_down(&self, key: i32, flags: u32) -> bool {
        if !self.is_valid() {
            return false;
        }
        self.current_page_view()
            .and_then(|view| self.pwl_window(&view))
            .is_some_and(|window| window.on_key_down(key, flags))
    }
    pub fn on_char(&self, _widget: &Rc<CpdfSdkWidget>, ch: u32, flags: u32) -> bool {
        if !self.is_valid() {
            return false;
        }
        self.current_page_view()
            .and_then(|view| self.pwl_window(&view))
            .is_some_and(|window| window.on_char(ch as u16, flags))
    }
    pub fn text(&self) -> Vec<u32> {
        self.focused_window()
            .map_or_else(Vec::new, |window| window.text())
    }
    pub fn selected_text(&self) -> Vec<u32> {
        self.focused_window()
            .map_or_else(Vec::new, |window| window.selected_text())
    }
    pub fn replace_and_keep_selection(&self, text: &[u32]) {
        if let Some(window) = self.focused_window() {
            window.replace_and_keep_selection(text);
        }
    }
    pub fn replace_selection(&self, text: &[u32]) {
        if let Some(window) = self.focused_window() {
            window.replace_selection(text);
        }
    }
    pub fn select_all_text(&self) -> bool {
        self.focused_window()
            .is_some_and(|window| window.select_all_text())
    }
    pub fn can_undo(&self) -> bool {
        self.focused_window()
            .is_some_and(|window| window.can_undo())
    }
    pub fn can_redo(&self) -> bool {
        self.focused_window()
            .is_some_and(|window| window.can_redo())
    }
    pub fn undo(&self) -> bool {
        self.focused_window().is_some_and(|window| window.undo())
    }
    pub fn redo(&self) -> bool {
        self.focused_window().is_some_and(|window| window.redo())
    }
    fn focused_window(&self) -> Option<Rc<dyn Window>> {
        if !self.is_valid() {
            return None;
        }
        self.pwl_window(&self.current_page_view()?)
    }
    pub fn set_focus_for_annot(&self, widget: &Rc<CpdfSdkWidget>, _flags: u32) {
        let Some(view) = widget.page_view() else {
            return;
        };
        let window = self.create_or_update_pwl_window(&view);
        if !self.is_alive() {
            return;
        }
        if let Some(window) = window {
            window.set_focus();
        }
        if self.is_alive() {
            self.valid.set(true);
            self.invalidate_rect(self.get_view_bbox(&view));
        }
    }
    pub fn kill_focus_for_annot(&self, flags: u32) {
        if !self.is_valid() {
            return;
        }
        let Some(view) = self.current_page_view() else {
            return;
        };
        if !self.commit_data(&view, flags) || !self.is_alive() {
            return;
        }
        if let Some(window) = self.pwl_window(&view) {
            window.kill_focus();
        }
        if !self.is_alive() {
            return;
        }
        let destroy = self.widget().is_some_and(|widget| {
            matches!(
                widget.field_type(),
                FormFieldType::PushButton | FormFieldType::CheckBox | FormFieldType::RadioButton
            )
        });
        self.escape_filler(&view, destroy);
    }
    pub fn commit_data(&self, view: &Rc<CpdfSdkPageView>, flags: u32) -> bool {
        let Some(owner) = self.owner() else {
            return false;
        };
        if !owner.is_data_changed(view) {
            return self.is_alive();
        }
        let (Some(widget), Some(filler)) = (self.widget(), self.filler()) else {
            return false;
        };
        let committed = filler.on_key_stroke_commit(&widget, view, flags);
        if !self.is_alive() || !widget.is_alive() {
            return false;
        }
        if !committed {
            owner.reset_pwl_window(view);
            return self.is_alive();
        }
        let validated = filler.on_validate(&widget, view, flags);
        if !self.is_alive() || !widget.is_alive() {
            return false;
        }
        if !validated {
            owner.reset_pwl_window(view);
            return self.is_alive();
        }
        owner.save_data(view);
        if !self.is_alive() || !widget.is_alive() {
            return false;
        }
        filler.on_calculate(&widget);
        if !self.is_alive() || !widget.is_alive() {
            return false;
        }
        filler.on_format(&widget);
        self.is_alive() && widget.is_alive()
    }
    pub fn escape_filler(&self, view: &Rc<CpdfSdkPageView>, destroy: bool) {
        self.valid.set(false);
        self.invalidate_rect(self.get_view_bbox(view));
        if destroy {
            self.destroy_pwl_window(view);
        }
    }
    pub fn set_change_mark(&self) {
        if let Some(filler) = self.filler() {
            filler.on_change();
        }
    }
    pub fn invalidate_rect(&self, rect: FxRect) {
        if let (Some(filler), Some(widget)) = (self.filler(), self.widget()) {
            filler.invalidate(&widget, rect);
        }
    }
    pub fn update_pwl_window_text_direction(&self) {
        let Some(widget) = self.widget() else { return };
        let windows: Vec<_> = self.windows.borrow().values().cloned().collect();
        for window in windows {
            window.set_text_direction(widget.text_direction());
            if !self.is_alive() {
                break;
            }
        }
    }
    pub fn get_create_param(&self) -> CreateParams {
        let (Some(widget), Some(filler)) = (self.widget(), self.filler()) else {
            return CreateParams::default();
        };
        let timer = filler.timer_handler();
        let notify: Rc<dyn FillerNotify> = filler;
        let provider = self.provider.borrow().clone();
        let mut cp = CreateParams::new(timer.as_ref(), Some(&notify), provider.as_ref());
        cp.rect = self.pdf_annot_rect();
        cp.flags = WINDOW_BORDER | WINDOW_BACKGROUND | WINDOW_VISIBLE;
        if widget.field_flags() & 1 != 0 {
            cp.flags |= WINDOW_READ_ONLY;
        }
        cp.background_color = widget
            .fill_color()
            .map_or_else(CfxColor::default, color_ref);
        cp.border_color = widget
            .border_color()
            .map_or_else(CfxColor::default, color_ref);
        cp.text_color = widget
            .text_color()
            .map_or_else(|| CfxColor::make_gray(0.0), color_ref);
        cp.font_size = widget.font_size();
        cp.border_width = widget.border_width();
        cp.border_style = widget.border_style();
        match cp.border_style {
            BorderStyle::Dash => {
                cp.dash = Dash {
                    dash: 3,
                    gap: 3,
                    phase: 0,
                }
            }
            BorderStyle::Beveled | BorderStyle::Inset => {
                cp.border_width = cp.border_width.saturating_mul(2)
            }
            _ => {}
        }
        if cp.font_size <= 0. {
            cp.flags |= WINDOW_AUTO_FONT_SIZE;
        }
        cp
    }
}
impl Drop for CfflFormField {
    fn drop(&mut self) {
        self.alive.set(false);
        self.cancel_timer();
        self.destroy_windows();
    }
}

struct FieldProvider {
    field: Weak<dyn FormField>,
}
impl ProviderIface for FieldProvider {
    fn window_matrix(&self, data: Option<&dyn PerWindowData>) -> CfxMatrix {
        let Some(field) = self.field.upgrade().filter(|field| field.base().is_alive()) else {
            return CfxMatrix::default();
        };
        let Some(view) = data
            .and_then(|data| data.as_any().downcast_ref::<CfflPerWindowData>())
            .and_then(CfflPerWindowData::page_view)
        else {
            return CfxMatrix::default();
        };
        field.base().current_matrix() * view.current_matrix()
    }
    fn on_set_focus_for_edit(&self, edit: &dyn Window) {
        if let Some(field) = self.field.upgrade().filter(|field| field.base().is_alive()) {
            field.on_set_focus_for_edit(edit);
        }
    }
}
impl TimerCallback for CfflFormField {
    fn on_timer_fired(&self) {
        if let Some(owner) = self.owner().filter(|owner| owner.base().is_alive()) {
            owner.on_timer_fired();
        }
    }
}
fn widget_matrix(rect: CfxFloatRect, rotate: i32) -> CfxMatrix {
    let (w, h) = (rect.width(), rect.height());
    let mut matrix = match rotate {
        90 => CfxMatrix::new(0., 1., -1., 0., w, 0.),
        180 => CfxMatrix::new(-1., 0., 0., -1., w, h),
        270 => CfxMatrix::new(0., -1., 1., 0., 0., h),
        _ => CfxMatrix::default(),
    };
    matrix.e += rect.left;
    matrix.f += rect.bottom;
    matrix
}
fn color_ref(color: u32) -> CfxColor {
    CfxColor::make_rgb(
        (color & 255) as f32 / 255.0,
        ((color >> 8) & 255) as f32 / 255.0,
        ((color >> 16) & 255) as f32 / 255.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widget_rotation_maps_local_coordinates_into_annotation_bounds() {
        let rect = CfxFloatRect::new(20., 30., 120., 80.);
        for rotate in [0, 90, 180, 270] {
            let matrix = widget_matrix(rect, rotate);
            let local = if rotate % 180 == 0 {
                CfxFloatRect::new(0., 0., 100., 50.)
            } else {
                CfxFloatRect::new(0., 0., 50., 100.)
            };
            assert_eq!(matrix.transform_float_rect(local), rect);
            let point = CfxPointF::new(14., 23.);
            let restored = matrix.get_inverse().transform(matrix.transform(point));
            assert!((point.x - restored.x).abs() < 0.0001);
            assert!((point.y - restored.y).abs() < 0.0001);
        }
    }
    #[test]
    fn field_action_defaults_preserve_acceptance_and_empty_change() {
        let action = CfflFieldAction::default();
        assert!(action.b_rc);
        assert!(!action.b_will_commit);
        assert!(!action.b_key_down);
        assert_eq!((action.n_sel_start, action.n_sel_end), (0, 0));
        assert!(action.s_change.is_empty());
        assert!(action.s_value.is_empty());
    }
}
