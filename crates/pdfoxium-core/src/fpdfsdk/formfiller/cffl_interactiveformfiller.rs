// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Ordinary form dispatch from `cffl_interactiveformfiller.{h,cpp}`. Widget and
//! window observers use logical invalidation, and map borrows end before calls.
//! JavaScript/XFA execution remains an explicit runtime boundary.
use super::cffl_fieldaction::CfflFieldAction;
#[cfg(feature = "xfa")]
use crate::fpdfsdk::cpdfsdk_widget_xfa::XfaAActionType;
#[cfg(feature = "xfa")]
#[path = "cffl_interactiveformfiller_xfa.rs"]
mod xfa_hooks;
use super::cffl_formfield::FormField;
use super::cffl_perwindowdata::CfflPerWindowData;
use super::{
    cffl_checkbox::CfflCheckBox, cffl_combobox::CfflComboBox, cffl_listbox::CfflListBox,
    cffl_pushbutton::CfflPushButton, cffl_radiobutton::CfflRadioButton,
    cffl_textfield::CfflTextField,
};
use crate::fpdfdoc::cpdf_aaction::AActionType;
use crate::fpdfdoc::cpdf_annot::AppearanceMode;
use crate::fpdfdoc::cpdf_formfield::FormFieldType;
use crate::fpdfsdk::cpdfsdk_annot::{CpdfSdkAnnot, SdkInputEvent};
use crate::fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
use crate::fpdfsdk::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use crate::fpdfsdk::pwl::cpwl_wnd::{is_ctrl_key_down, is_shift_key_down};
use crate::fpdfsdk::pwl::ipwl_fillernotify::{
    BeforeKeystrokeResult, CursorStyle, FillerNotify, PerWindowData,
};
use crate::fpdfsdk::pwl::pwl_draw::PwlDraw;
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::cfx_timer::TimerHandler;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF, FxRect};
use crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions;
use crate::fxge::cfx_graphstatedata::CfxGraphStateData;
use crate::fxge::cfx_path::{CfxPath, PointType};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

struct NotifyGuard<'a>(&'a Cell<bool>, bool);
impl<'a> NotifyGuard<'a> {
    fn new(cell: &'a Cell<bool>) -> Self {
        Self(cell, cell.replace(true))
    }
}
impl Drop for NotifyGuard<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}
fn key(widget: &CpdfSdkWidget) -> usize {
    widget as *const CpdfSdkWidget as usize
}
pub struct CfflInteractiveFormFiller {
    env: Weak<CpdfSdkFormFillEnvironment>,
    this: Weak<Self>,
    fields: RefCell<HashMap<usize, Rc<dyn FormField>>>,
    notifying: Cell<bool>,
    alive: Cell<bool>,
}
impl CfflInteractiveFormFiller {
    pub fn new(env: Weak<CpdfSdkFormFillEnvironment>) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            env,
            this: this.clone(),
            fields: RefCell::new(HashMap::new()),
            notifying: Cell::new(false),
            alive: Cell::new(true),
        })
    }
    pub fn dispatch_annot_event(&self, annot: &Rc<CpdfSdkAnnot>, event: SdkInputEvent) -> bool {
        let (Some(widget), Some(view)) = (annot.widget(), annot.page_view()) else {
            return false;
        };
        if !self.alive.get() || !annot.is_alive() || widget.is_signature_widget() {
            return false;
        }
        match event {
            SdkInputEvent::MouseEnter(flags) => {
                self.on_mouse_enter(&view, &widget, flags);
                widget.is_alive()
            }
            SdkInputEvent::MouseExit(flags) => {
                self.on_mouse_exit(&view, &widget, flags);
                widget.is_alive()
            }
            SdkInputEvent::LButtonDown(flags, point) => {
                self.on_l_button_down(&view, &widget, flags, point)
            }
            SdkInputEvent::LButtonUp(flags, point) => {
                self.on_l_button_up(&view, &widget, flags, point)
            }
            SdkInputEvent::LButtonDblClk(flags, point) => {
                self.on_l_button_dbl_clk(&view, &widget, flags, point)
            }
            SdkInputEvent::MouseMove(flags, point) => {
                self.on_mouse_move(&view, &widget, flags, point)
            }
            SdkInputEvent::MouseWheel(flags, point, delta) => {
                self.on_mouse_wheel(&view, &widget, flags, point, delta)
            }
            SdkInputEvent::RButtonDown(flags, point) => {
                self.on_r_button_down(&view, &widget, flags, point)
            }
            SdkInputEvent::RButtonUp(flags, point) => {
                self.on_r_button_up(&view, &widget, flags, point)
            }
            SdkInputEvent::Char(character, flags) => self.on_char(&widget, character, flags),
            SdkInputEvent::KeyDown(key, flags) => self.on_key_down(&widget, key, flags),
            SdkInputEvent::SetFocus(flags) => self.on_set_focus(&widget, flags),
            SdkInputEvent::KillFocus(flags) => self.on_kill_focus(&widget, flags),
        }
    }
    pub fn environment(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
        self.env.upgrade().filter(|e| e.is_alive())
    }
    pub fn shutdown(&self) {
        if !self.alive.replace(false) {
            return;
        }
        loop {
            let item = {
                let mut fields = self.fields.borrow_mut();
                let id = fields.keys().next().copied();
                id.and_then(|id| fields.remove(&id))
            };
            let Some(field) = item else {
                break;
            };
            field.base().destroy();
        }
    }
    pub fn get_form_field(&self, widget: &CpdfSdkWidget) -> Option<Rc<dyn FormField>> {
        self.fields
            .borrow()
            .get(&key(widget))
            .cloned()
            .filter(|f| f.base().is_alive())
    }
    pub fn get_or_create_form_field(
        &self,
        widget: &Rc<CpdfSdkWidget>,
    ) -> Option<Rc<dyn FormField>> {
        if !self.alive.get() || !widget.is_alive() {
            return None;
        }
        if let Some(field) = self.get_form_field(widget) {
            return Some(field);
        }
        let this = self.this.upgrade()?;
        let field: Rc<dyn FormField> = match widget.field_type() {
            FormFieldType::PushButton => CfflPushButton::new(&this, widget),
            FormFieldType::CheckBox => CfflCheckBox::new(&this, widget),
            FormFieldType::RadioButton => CfflRadioButton::new(&this, widget),
            FormFieldType::TextField => CfflTextField::new(&this, widget),
            FormFieldType::ListBox => CfflListBox::new(&this, widget),
            FormFieldType::ComboBox => CfflComboBox::new(&this, widget),
            _ => return None,
        };
        if !widget.is_alive() || !self.alive.get() {
            field.base().destroy();
            return None;
        }
        let old = self.fields.borrow_mut().insert(key(widget), field.clone());
        drop(old);
        Some(field)
    }
    pub fn unregister_form_field(&self, widget: &CpdfSdkWidget) {
        let field = self.fields.borrow_mut().remove(&key(widget));
        if let Some(field) = field {
            field.base().destroy();
        }
    }
    pub fn on_delete(&self, widget: &CpdfSdkWidget) {
        self.unregister_form_field(widget);
    }
    pub fn get_page_view(&self, widget: &CpdfSdkWidget) -> Option<Rc<CpdfSdkPageView>> {
        widget.page_view()
    }
    pub fn get_or_create_page_view(&self, widget: &CpdfSdkWidget) -> Option<Rc<CpdfSdkPageView>> {
        widget.page_view()
    }
    pub fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        self.environment().map(|env| env as Rc<dyn TimerHandler>)
    }
    pub fn invalidate(&self, widget: &CpdfSdkWidget, rect: FxRect) {
        if let (Some(env), Some(page)) = (
            self.environment(),
            widget.page_view().and_then(|v| v.page()),
        ) {
            env.invalidate_page(&page, rect);
        }
    }
    pub fn on_change(&self) {
        if let Some(env) = self.environment() {
            env.set_change_mark();
            env.on_change();
        }
    }
    pub fn on_set_field_input_focus(&self, text: &[u32]) {
        if let Some(env) = self.environment() {
            env.on_set_field_input_focus(text);
        }
    }
    pub fn is_filling_allowed(&self, widget: &CpdfSdkWidget) -> bool {
        widget.field_type() != FormFieldType::PushButton
            && self
                .environment()
                .is_some_and(|e| e.has_permissions(0x100 | 0x20 | 0x8))
    }
    pub fn annot_hit_test(&self, widget: &CpdfSdkWidget, point: CfxPointF) -> bool {
        widget.rect().contains_point(point)
    }
    pub fn is_valid_annot(view: &CpdfSdkPageView, widget: &CpdfSdkWidget) -> bool {
        view.is_alive()
            && widget.is_alive()
            && widget
                .annotation()
                .is_some_and(|a| view.is_valid_sdk_annot(&a))
    }
    pub fn get_view_bbox(&self, view: &Rc<CpdfSdkPageView>, widget: &Rc<CpdfSdkWidget>) -> FxRect {
        if let Some(field) = self.get_form_field(widget) {
            return field.base().get_view_bbox(view);
        }
        let mut rect = widget.rect();
        if !rect.is_empty() {
            rect.inflate_xy(1.0, 1.0);
            rect.normalize();
        }
        rect.get_outer_rect()
    }
    fn focused(&self, widget: &CpdfSdkWidget) -> bool {
        self.environment()
            .and_then(|e| e.get_focus_annot())
            .and_then(|a| a.widget())
            .is_some_and(|w| std::ptr::eq(w.as_ref(), widget))
    }
    pub fn on_draw(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
    ) {
        if !widget.annot().is_visible() {
            return;
        }
        let field = self.get_form_field(widget);
        if let Some(field) = field.as_ref().filter(|f| f.base().is_valid()) {
            field.on_draw(view, widget, draw, matrix);
            if !widget.is_alive() || !self.focused(widget) {
                return;
            }
            let rect = field.base().get_focus_box(view);
            if rect.is_empty() {
                return;
            }
            let mut path = CfxPath::new();
            // CFX_DrawUtils::DrawFocusRect starts at the top-left and runs
            // downwards. The path direction determines the dotted border phase.
            for (index, point) in [
                CfxPointF::new(rect.left, rect.top),
                CfxPointF::new(rect.left, rect.bottom),
                CfxPointF::new(rect.right, rect.bottom),
                CfxPointF::new(rect.right, rect.top),
                CfxPointF::new(rect.left, rect.top),
            ]
            .into_iter()
            .enumerate()
            {
                path.append_point(
                    point,
                    if index == 0 {
                        PointType::Move
                    } else {
                        PointType::Line
                    },
                );
            }
            let mut graph = CfxGraphStateData::default();
            graph.set_dash_array(vec![1.0]);
            draw.draw_path(
                &path,
                matrix,
                Some(&graph),
                0,
                0xff000000,
                CfxFillRenderOptions::even_odd_options(),
            );
            return;
        }
        if let Some(field) = field {
            field.on_draw_deactive(view, widget, draw, matrix);
        } else {
            widget.draw_appearance(draw, matrix, AppearanceMode::Normal);
        }
        if widget.is_alive() && widget.field_flags() & 1 == 0 && self.is_filling_allowed(widget) {
            widget.draw_shadow(draw, view);
        }
    }
    fn flags(flags: u32) -> CfflFieldAction {
        CfflFieldAction {
            b_modifier: is_ctrl_key_down(flags),
            b_shift: is_shift_key_down(flags),
            ..Default::default()
        }
    }
    fn run_action(
        &self,
        widget: &Rc<CpdfSdkWidget>,
        view: &Rc<CpdfSdkPageView>,
        kind: AActionType,
        flags: u32,
        field_data: bool,
    ) -> bool {
        if self.notifying.get() || !widget.additional_action(kind).has_dict() {
            return widget.is_alive();
        }
        let _guard = NotifyGuard::new(&self.notifying);
        let mut data = Self::flags(flags);
        if field_data {
            if let Some(field) = self.get_or_create_form_field(widget) {
                field.get_action_data(view, kind, &mut data);
            } else {
                return false;
            }
        }
        widget.on_a_action(kind, &mut data, view);
        widget.is_alive()
            && self
                .environment()
                .is_some_and(|e| e.action_error().is_none())
    }
    fn cursor_action(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        kind: AActionType,
    ) -> bool {
        if self.notifying.get() || !widget.additional_action(kind).has_dict() {
            return widget.is_alive();
        }
        let age = widget.value_age();
        widget.set_appearance_modified(false);
        if !self.run_action(widget, view, kind, flags, false) {
            return false;
        }
        if widget.appearance_modified() {
            if let Some(field) = self.get_form_field(widget) {
                field
                    .base()
                    .reset_pwl_window_for_value_age(view, widget, age);
            }
        }
        widget.is_alive()
    }
    pub fn on_mouse_enter(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
    ) {
        if self.cursor_action(view, widget, flags, AActionType::CursorEnter) {
            if let Some(field) = self.get_or_create_form_field(widget) {
                field.on_mouse_enter(view);
            }
        }
    }
    pub fn on_mouse_exit(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
    ) {
        if self.cursor_action(view, widget, flags, AActionType::CursorExit) {
            if let Some(field) = self.get_form_field(widget) {
                field.on_mouse_exit(view);
            }
        }
    }
    pub fn on_l_button_down(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        if !self.notifying.get()
            && self.annot_hit_test(widget, point)
            && widget.additional_action(AActionType::ButtonDown).has_dict()
        {
            if !self.cursor_action(view, widget, flags, AActionType::ButtonDown)
                || !Self::is_valid_annot(view, widget)
            {
                return true;
            }
        }
        self.get_form_field(widget)
            .is_some_and(|f| f.on_l_button_down(view, widget, flags, point))
    }
    pub fn on_l_button_up(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        let set_focus = if matches!(
            widget.field_type(),
            FormFieldType::PushButton | FormFieldType::CheckBox | FormFieldType::RadioButton
        ) {
            self.get_view_bbox(view, widget)
                .contains(point.x as i32, point.y as i32)
        } else {
            true
        };
        if set_focus {
            if let (Some(env), Some(annot)) = (self.environment(), widget.annotation()) {
                env.set_focus_annot(&annot);
            }
        }
        if !widget.is_alive() {
            return true;
        }
        let result = self
            .get_form_field(widget)
            .is_some_and(|f| f.on_l_button_up(view, widget, flags, point));
        // Release capture/pressed state above even when the pointer left the
        // widget. A mouse-up action (PDF additional-action U) and activation
        // require release inside the annotation's active area. The pinned
        // native filler also dispatches them to a still-focused outside widget.
        if !widget.is_alive() || !self.focused(widget) || !self.annot_hit_test(widget, point) {
            return result;
        }
        if self.on_button_up(widget, view, flags) || !widget.is_alive() {
            return true;
        }
        #[cfg(feature = "xfa")]
        if self.on_xfa_action(widget, view, XfaAActionType::Click, flags) || !widget.is_alive() {
            return true;
        }
        result
    }
    pub fn on_button_up(
        &self,
        widget: &Rc<CpdfSdkWidget>,
        view: &Rc<CpdfSdkPageView>,
        flags: u32,
    ) -> bool {
        if self.notifying.get() || !widget.additional_action(AActionType::ButtonUp).has_dict() {
            return false;
        }
        let (age, value_age) = (widget.appearance_age(), widget.value_age());
        if !self.run_action(widget, view, AActionType::ButtonUp, flags, false)
            || !Self::is_valid_annot(view, widget)
        {
            return true;
        }
        if age == widget.appearance_age() {
            return false;
        }
        if let Some(field) = self.get_form_field(widget) {
            field
                .base()
                .reset_pwl_window_for_value_age(view, widget, value_age);
        }
        true
    }
    pub fn on_l_button_dbl_clk(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.on_l_button_dbl_clk(view, flags, point))
    }
    pub fn on_mouse_move(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.get_or_create_form_field(widget)
            .is_some_and(|f| f.on_mouse_move(view, flags, point))
    }
    pub fn on_mouse_wheel(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
        delta: CfxPointF,
    ) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.on_mouse_wheel(view, flags, point, delta))
    }
    pub fn on_r_button_down(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.on_r_button_down(view, flags, point))
    }
    pub fn on_r_button_up(
        &self,
        view: &Rc<CpdfSdkPageView>,
        widget: &Rc<CpdfSdkWidget>,
        flags: u32,
        point: CfxPointF,
    ) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.on_r_button_up(view, flags, point))
    }
    pub fn on_key_down(&self, widget: &Rc<CpdfSdkWidget>, key: i32, flags: u32) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.on_key_down(key, flags))
    }
    pub fn on_char(&self, widget: &Rc<CpdfSdkWidget>, character: u32, flags: u32) -> bool {
        character == 9
            || self
                .get_form_field(widget)
                .is_some_and(|f| f.on_char(widget, character, flags))
    }
    pub fn on_set_focus(&self, widget: &Rc<CpdfSdkWidget>, flags: u32) -> bool {
        let Some(view) = widget.page_view() else {
            return false;
        };
        if !self.notifying.get() && widget.additional_action(AActionType::GetFocus).has_dict() {
            let age = widget.value_age();
            widget.set_appearance_modified(false);
            if !self.run_action(widget, &view, AActionType::GetFocus, flags, true) {
                return false;
            }
            if widget.appearance_modified() {
                if let Some(field) = self.get_form_field(widget) {
                    field
                        .base()
                        .reset_pwl_window_for_value_age(&view, widget, age);
                }
            }
        }
        if let Some(field) = self.get_or_create_form_field(widget) {
            field.base().set_focus_for_annot(widget, flags);
        }
        widget.is_alive()
            && self
                .environment()
                .is_some_and(|e| e.action_error().is_none())
    }
    pub fn on_kill_focus(&self, widget: &Rc<CpdfSdkWidget>, flags: u32) -> bool {
        let Some(field) = self.get_form_field(widget) else {
            return widget.is_alive();
        };
        field.base().kill_focus_for_annot(flags);
        if !widget.is_alive() {
            return false;
        }
        let Some(view) = widget.page_view() else {
            return false;
        };
        widget.set_appearance_modified(false);
        self.run_action(widget, &view, AActionType::LoseFocus, flags, true)
    }
    fn commit_action(
        &self,
        widget: &Rc<CpdfSdkWidget>,
        view: &Rc<CpdfSdkPageView>,
        flags: u32,
        kind: AActionType,
    ) -> bool {
        if self.notifying.get() || !widget.additional_action(kind).has_dict() {
            return true;
        }
        widget.set_appearance_modified(false);
        let _guard = NotifyGuard::new(&self.notifying);
        let mut data = Self::flags(flags);
        data.b_key_down = true;
        data.b_will_commit = kind == AActionType::KeyStroke;
        let Some(field) = self.get_form_field(widget) else {
            return false;
        };
        field.get_action_data(view, kind, &mut data);
        field.save_pwl_window_state(view);
        widget.on_a_action(kind, &mut data, view);
        data.b_rc
            && self
                .environment()
                .is_some_and(|e| e.action_error().is_none())
    }
    pub fn on_key_stroke_commit(
        &self,
        widget: &Rc<CpdfSdkWidget>,
        view: &Rc<CpdfSdkPageView>,
        flags: u32,
    ) -> bool {
        self.commit_action(widget, view, flags, AActionType::KeyStroke)
    }
    pub fn on_validate(
        &self,
        widget: &Rc<CpdfSdkWidget>,
        view: &Rc<CpdfSdkPageView>,
        flags: u32,
    ) -> bool {
        self.commit_action(widget, view, flags, AActionType::Validate)
    }
    pub fn on_calculate(&self, widget: &Rc<CpdfSdkWidget>) {
        if !self.notifying.get() {
            if let (Some(env), Some(field)) = (self.environment(), widget.field()) {
                if let Err(error) = env.interactive_form().on_calculate(Some(&field)) {
                    env.record_action_error(error);
                }
            }
        }
    }
    pub fn on_format(&self, widget: &Rc<CpdfSdkWidget>) {
        if self.notifying.get() {
            return;
        }
        let (Some(env), Some(field)) = (self.environment(), widget.field()) else {
            return;
        };
        match env.interactive_form().on_format(&field) {
            Ok(Some(value)) if widget.is_alive() => {
                env.interactive_form()
                    .reset_field_appearance(&field, Some(&value));
                env.interactive_form().update_field(&field);
            }
            Err(error) => env.record_action_error(error),
            _ => {}
        }
    }
    pub fn get_text(&self, widget: &CpdfSdkWidget) -> Vec<u32> {
        self.get_form_field(widget)
            .map_or_else(Vec::new, |f| f.base().text())
    }
    pub fn get_selected_text(&self, widget: &CpdfSdkWidget) -> Vec<u32> {
        self.get_form_field(widget)
            .map_or_else(Vec::new, |f| f.base().selected_text())
    }
    pub fn replace_selection(&self, widget: &CpdfSdkWidget, text: &[u32]) {
        if let Some(f) = self.get_form_field(widget) {
            f.base().replace_selection(text);
        }
    }
    pub fn replace_and_keep_selection(&self, widget: &CpdfSdkWidget, text: &[u32]) {
        if let Some(f) = self.get_form_field(widget) {
            f.base().replace_and_keep_selection(text);
        }
    }
    pub fn select_all_text(&self, widget: &CpdfSdkWidget) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.base().select_all_text())
    }
    pub fn can_undo(&self, widget: &CpdfSdkWidget) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.base().can_undo())
    }
    pub fn can_redo(&self, widget: &CpdfSdkWidget) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.base().can_redo())
    }
    pub fn undo(&self, widget: &CpdfSdkWidget) -> bool {
        self.get_form_field(widget).is_some_and(|f| f.base().undo())
    }
    pub fn redo(&self, widget: &CpdfSdkWidget) -> bool {
        self.get_form_field(widget).is_some_and(|f| f.base().redo())
    }
    pub fn set_index_selected(&self, widget: &CpdfSdkWidget, index: i32, selected: bool) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.set_index_selected(index, selected))
    }
    pub fn is_index_selected(&self, widget: &CpdfSdkWidget, index: i32) -> bool {
        self.get_form_field(widget)
            .is_some_and(|f| f.is_index_selected(index))
    }
    pub fn update_form_field_text_direction(
        &self,
        widget: &CpdfSdkWidget,
        direction: ParagraphDirection,
    ) {
        widget.set_text_direction(direction);
        if let Some(field) = self.get_form_field(widget) {
            field.base().update_pwl_window_text_direction();
        }
        widget.update_field();
        if let Some(view) = widget.page_view() {
            view.update_rects(&[widget.rect()]);
        }
    }
}
fn attached(data: Option<&dyn PerWindowData>) -> Option<&CfflPerWindowData> {
    data?.as_any().downcast_ref()
}
impl FillerNotify for CfflInteractiveFormFiller {
    fn invalidate_rect(&self, data: Option<&dyn PerWindowData>, rect: CfxFloatRect) {
        if let (Some(env), Some(widget)) = (
            self.environment(),
            attached(data).and_then(CfflPerWindowData::widget),
        ) {
            env.invalidate_rect(&widget, rect);
        }
    }
    fn output_selected_rect(&self, data: Option<&dyn PerWindowData>, rect: CfxFloatRect) {
        let Some(field) = attached(data).and_then(CfflPerWindowData::form_field) else {
            return;
        };
        if let (Some(env), Some(widget)) = (self.environment(), field.base().widget()) {
            env.output_selected_rect(&widget, field.base().pwl_to_ffl_rect(rect));
        }
    }
    fn is_selection_implemented(&self) -> bool {
        self.environment()
            .is_some_and(|e| e.is_selection_implemented())
    }
    fn set_cursor(&self, cursor: CursorStyle) {
        if let Some(env) = self.environment() {
            env.set_cursor(cursor);
        }
    }
    fn query_where_popup(
        &self,
        data: Option<&dyn PerWindowData>,
        min: f32,
        max: f32,
    ) -> (bool, f32) {
        let Some(widget) = attached(data).and_then(CfflPerWindowData::widget) else {
            return (true, 0.0);
        };
        let Some(page) = widget.page_view().and_then(|v| v.page()) else {
            return (true, 0.0);
        };
        let rect = widget.rect();
        let (top, bottom) = match (widget.rotate() / 90).rem_euclid(4) {
            1 => (rect.left, page.width() - rect.right),
            2 => (rect.bottom, page.height() - rect.top),
            3 => (page.width() - rect.right, rect.left),
            _ => (page.height() - rect.top, rect.bottom),
        };
        let height = 140.0f32.max(min).min(max.max(min));
        if bottom > height {
            (true, height)
        } else if top > height {
            (false, height)
        } else if top > bottom {
            (false, top.max(0.0))
        } else {
            (true, bottom.max(0.0))
        }
    }
    fn on_before_key_stroke(
        &self,
        data: Option<&dyn PerWindowData>,
        change: &mut Vec<u32>,
        change_ex: &[u32],
        start: i32,
        end: i32,
        key_down: bool,
        flags: u32,
    ) -> BeforeKeystrokeResult {
        let (Some(mut widget), Some(mut view)) = (
            attached(data).and_then(CfflPerWindowData::widget),
            attached(data).and_then(CfflPerWindowData::page_view),
        ) else {
            return BeforeKeystrokeResult {
                rc: false,
                exit: true,
            };
        };
        #[cfg(feature = "xfa")]
        if self
            .get_form_field(&widget)
            .is_some_and(|field| field.is_field_full(&view))
        {
            if self.on_xfa_action(&widget, &view, XfaAActionType::Full, flags) || !widget.is_alive()
            {
                return BeforeKeystrokeResult {
                    rc: true,
                    exit: true,
                };
            }
        }
        if self.notifying.get() || !widget.additional_action(AActionType::KeyStroke).has_dict() {
            return BeforeKeystrokeResult {
                rc: true,
                exit: false,
            };
        }
        let Some(field) = self.get_form_field(&widget) else {
            return BeforeKeystrokeResult {
                rc: false,
                exit: true,
            };
        };
        let _guard = NotifyGuard::new(&self.notifying);
        let (age, value_age) = (widget.appearance_age(), widget.value_age());
        let mut action = Self::flags(flags);
        action.s_change = change.clone();
        action.s_change_ex = change_ex.to_vec();
        action.b_key_down = key_down;
        action.n_sel_start = start;
        action.n_sel_end = end;
        field.get_action_data(&view, AActionType::KeyStroke, &mut action);
        field.save_pwl_window_state(&view);
        let status = widget.on_a_action(AActionType::KeyStroke, &mut action, &view);
        if self
            .environment()
            .is_none_or(|e| e.action_error().is_some())
        {
            return BeforeKeystrokeResult {
                rc: false,
                exit: true,
            };
        }
        if !Self::is_valid_annot(&view, &widget) {
            return BeforeKeystrokeResult {
                rc: true,
                exit: true,
            };
        }
        if !status {
            return BeforeKeystrokeResult {
                rc: true,
                exit: false,
            };
        }
        let mut exit = false;
        if age != widget.appearance_age() {
            field
                .base()
                .reset_pwl_window_for_value_age(&view, &widget, value_age);
            let Some(fresh) = field.base().get_per_pwl_window_data(&view) else {
                return BeforeKeystrokeResult {
                    rc: true,
                    exit: true,
                };
            };
            let (Some(fresh_widget), Some(fresh_view)) = (fresh.widget(), fresh.page_view()) else {
                return BeforeKeystrokeResult {
                    rc: true,
                    exit: true,
                };
            };
            widget = fresh_widget;
            view = fresh_view;
            exit = true;
        }
        if action.b_rc {
            field.set_action_data(&view, AActionType::KeyStroke, &action);
        } else {
            field.recreate_pwl_window_from_saved_state(&view);
        }
        if self.focused(&widget) {
            BeforeKeystrokeResult { rc: false, exit }
        } else {
            field.base().commit_data(&view, flags);
            BeforeKeystrokeResult {
                rc: false,
                exit: true,
            }
        }
    }
    fn on_popup_pre_open(&self, data: Option<&dyn PerWindowData>, flags: u32) -> bool {
        let (Some(widget), Some(view)) = (
            attached(data).and_then(CfflPerWindowData::widget),
            attached(data).and_then(CfflPerWindowData::page_view),
        ) else {
            return false;
        };
        #[cfg(not(feature = "xfa"))]
        let _ = (&view, flags);
        #[cfg(feature = "xfa")]
        if self.on_xfa_action(&widget, &view, XfaAActionType::PreOpen, flags) {
            return true;
        }
        !widget.is_alive()
    }
    fn on_popup_post_open(&self, data: Option<&dyn PerWindowData>, flags: u32) -> bool {
        let (Some(widget), Some(view)) = (
            attached(data).and_then(CfflPerWindowData::widget),
            attached(data).and_then(CfflPerWindowData::page_view),
        ) else {
            return false;
        };
        #[cfg(not(feature = "xfa"))]
        let _ = (&view, flags);
        #[cfg(feature = "xfa")]
        if self.on_xfa_action(&widget, &view, XfaAActionType::PostOpen, flags) {
            return true;
        }
        !widget.is_alive()
    }
}

#[cfg(all(test, feature = "js"))]
#[path = "cffl_interactiveformfiller_recreation_tests.rs"]
mod recreation_tests;
