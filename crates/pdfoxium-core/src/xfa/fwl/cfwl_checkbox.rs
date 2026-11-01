// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/cfwl_checkbox.{h,cpp}` port. Mutable control state uses
//! small Cells; no state borrow survives dispatch to a host delegate.
use super::{
    cfwl_app::CfwlApp,
    cfwl_event::{EventData, FwlEvent},
    cfwl_message::{FwlMessage, KeyCommand, MessageData, MouseCommand},
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
    cfwl_themetext::CfwlThemeText,
    cfwl_widget::{CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef, WidgetState},
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::{
        fde::cfde_data::{FdeTextAlignment, FdeTextStyle},
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
    },
};
use std::{cell::Cell, rc::Rc};

pub const STYLEEXT_CKB_3_STATE: u32 = 1 << 6;
pub const STYLEEXT_CKB_RADIO_BUTTON: u32 = 1 << 7;
pub const STYLEEXT_CKB_SIGN_SHAPE_CHECK: u32 = 0;
pub const STYLEEXT_CKB_SIGN_SHAPE_CIRCLE: u32 = 1 << 10;
pub const STYLEEXT_CKB_SIGN_SHAPE_CROSS: u32 = 2 << 10;
pub const STYLEEXT_CKB_SIGN_SHAPE_DIAMOND: u32 = 3 << 10;
pub const STYLEEXT_CKB_SIGN_SHAPE_SQUARE: u32 = 4 << 10;
pub const STYLEEXT_CKB_SIGN_SHAPE_STAR: u32 = 5 << 10;
pub const STYLEEXT_CKB_SIGN_SHAPE_MASK: u32 = 7 << 10;
const CAPTION_MARGIN: f32 = 5.;

pub struct CfwlCheckBox {
    base: CfwlWidget,
    client_rect: Cell<CfxRectF>,
    box_rect: Cell<CfxRectF>,
    caption_rect: Cell<CfxRectF>,
    focus_rect: Cell<CfxRectF>,
    tto_styles: Cell<FdeTextStyle>,
    tto_align: Cell<FdeTextAlignment>,
    button_down: Cell<bool>,
    box_height: Cell<f32>,
}
impl CfwlCheckBox {
    pub fn new(app: &Rc<CfwlApp>) -> Rc<Self> {
        Self::with_properties(app, Properties::default(), None)
    }
    pub fn with_properties(
        app: &Rc<CfwlApp>,
        properties: Properties,
        outer: Option<WidgetRef>,
    ) -> Rc<Self> {
        let control = Rc::new(Self {
            base: CfwlWidget::new(app, properties, outer),
            client_rect: Cell::new(CfxRectF::default()),
            box_rect: Cell::new(CfxRectF::default()),
            caption_rect: Cell::new(CfxRectF::default()),
            focus_rect: Cell::new(CfxRectF::default()),
            tto_styles: Cell::new(FdeTextStyle {
                single_line: true,
                ..Default::default()
            }),
            tto_align: Cell::new(FdeTextAlignment::Center),
            button_down: Cell::new(false),
            box_height: Cell::new(16.),
        });
        app.register_widget(control.clone() as WidgetRef);
        control
    }
    pub fn set_box_size(&self, height: f32) {
        if height.is_finite() && height >= 0. {
            self.box_height.set(height);
        }
    }
    pub fn box_rect(&self) -> CfxRectF {
        self.box_rect.get()
    }
    pub fn caption_rect(&self) -> CfxRectF {
        self.caption_rect.get()
    }
    pub fn focus_rect(&self) -> CfxRectF {
        self.focus_rect.get()
    }
    pub fn set_check_state(&self, check: i32) {
        self.base.clear_states(WidgetState::CHECKBOX_CHECK_MASK);
        match check {
            1 => self.base.set_states(WidgetState::CHECKBOX_CHECKED),
            2 if self.base.get_style_exts() & STYLEEXT_CKB_3_STATE != 0 => {
                self.base.set_states(WidgetState::CHECKBOX_NEUTRAL)
            }
            _ => {}
        }
        self.base.repaint_rect(self.client_rect.get());
    }
    pub fn get_check_state(&self) -> i32 {
        match (self.base.get_states() & WidgetState::CHECKBOX_CHECK_MASK).0 {
            value if value == WidgetState::CHECKBOX_CHECKED.0 => 1,
            value if value == WidgetState::CHECKBOX_NEUTRAL.0 => 2,
            _ => 0,
        }
    }
    fn layout(&self) {
        let mut widget = self.base.get_widget_rect();
        widget.width = widget.width.round();
        widget.height = widget.height.round();
        self.base.set_widget_rect(widget);
        let client = self.get_client_rect();
        self.client_rect.set(client);
        let left = client.left + self.box_height.get();
        self.box_rect.set(CfxRectF::new(
            client.left,
            client.top,
            self.box_height.get(),
            self.box_height.get(),
        ));
        let mut caption = CfxRectF::new(left, client.top, client.right() - left, client.height);
        caption.inflate_xy(-CAPTION_MARGIN, -CAPTION_MARGIN);
        self.caption_rect.set(caption);
        let mut text_rect = caption;
        self.base.calc_text_rect(
            &caption_text(),
            self.tto_styles.get(),
            self.tto_align.get(),
            &mut text_rect,
        );
        let mut focus = CfxRectF::new(
            caption.left,
            caption.top,
            caption.width.max(text_rect.width),
            caption.height.min(text_rect.height),
        );
        focus.inflate_xy(1., 1.);
        self.focus_rect.set(focus);
    }
    pub fn get_part_states(&self) -> PartState {
        let states = self.base.get_states();
        let mut part = match self.get_check_state() {
            2 => PartState::NEUTRAL,
            1 => PartState::CHECKED,
            _ => PartState::NORMAL,
        };
        if (states & WidgetState::DISABLED).0 != 0 {
            part |= PartState::DISABLED;
        } else if (states & WidgetState::HOVERED).0 != 0 {
            part |= PartState::HOVERED;
        } else if (states & WidgetState::PRESSED).0 != 0 {
            part |= PartState::PRESSED;
        }
        if (states & WidgetState::FOCUSED).0 != 0 {
            part |= PartState::FOCUSED;
        }
        part
    }
    fn next_states(&self) {
        let previous = self.base.get_states();
        if self.base.get_style_exts() & STYLEEXT_CKB_RADIO_BUTTON != 0 {
            if (previous & WidgetState::CHECKBOX_CHECK_MASK).0 == 0 {
                self.base.set_states(WidgetState::CHECKBOX_CHECKED);
            }
        } else {
            let three = self.base.get_style_exts() & STYLEEXT_CKB_3_STATE != 0;
            let next = match self.get_check_state() {
                2 => {
                    if three {
                        1
                    } else {
                        0
                    }
                }
                1 => 0,
                _ => {
                    if three {
                        2
                    } else {
                        1
                    }
                }
            };
            self.base.clear_states(WidgetState::CHECKBOX_CHECK_MASK);
            match next {
                1 => self.base.set_states(WidgetState::CHECKBOX_CHECKED),
                2 => self.base.set_states(WidgetState::CHECKBOX_NEUTRAL),
                _ => {}
            }
        }
        self.base.repaint_rect(self.client_rect.get());
        if previous != self.base.get_states() {
            let mut event = FwlEvent::new(self.base.self_ref(), EventData::CheckStateChanged);
            self.base.dispatch_event(&mut event);
        }
    }
    fn on_focus(&self, gained: bool) {
        if gained {
            self.base.set_states(WidgetState::FOCUSED);
        } else {
            self.base.clear_states(WidgetState::FOCUSED);
        }
        self.base.repaint_rect(self.client_rect.get());
    }
    fn on_left_button_down(&self) {
        if !self.base.is_enabled() {
            return;
        }
        self.button_down.set(true);
        self.base.clear_states(WidgetState::HOVERED);
        self.base.set_states(WidgetState::PRESSED);
        self.base.repaint_rect(self.client_rect.get());
    }
    fn on_left_button_up(&self, pos: CfxPointF) {
        if !self.button_down.replace(false) {
            return;
        }
        // A release outside the client must not leave the control pressed.
        self.base.clear_states(WidgetState::PRESSED);
        if !self.client_rect.get().contains_point(pos) {
            self.base.clear_states(WidgetState::HOVERED);
            self.base.repaint_rect(self.client_rect.get());
            return;
        }
        self.base.set_states(WidgetState::HOVERED);
        self.next_states();
    }
    fn on_mouse_move(&self, pos: CfxPointF) {
        if !self.base.is_enabled() {
            return;
        }
        let before = self.base.get_states();
        let inside = self.client_rect.get().contains_point(pos);
        if self.button_down.get() {
            if inside {
                self.base.set_states(WidgetState::PRESSED);
                self.base.clear_states(WidgetState::HOVERED);
            } else {
                self.base.clear_states(WidgetState::PRESSED);
                self.base.set_states(WidgetState::HOVERED);
            }
        } else if inside {
            self.base.set_states(WidgetState::HOVERED);
        }
        if before != self.base.get_states() {
            self.base.repaint_rect(self.box_rect.get());
        }
    }
    fn on_mouse_leave(&self) {
        if self.button_down.get() {
            self.base.set_states(WidgetState::HOVERED);
        } else {
            self.base.clear_states(WidgetState::HOVERED);
        }
        self.base.repaint_rect(self.box_rect.get());
    }
}
fn caption_text() -> Vec<u32> {
    "Check box".chars().map(u32::from).collect()
}
impl FwlWidget for CfwlCheckBox {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::CheckBox
    }
    fn update(&self) {
        if self.base.is_locked() {
            return;
        }
        self.tto_align.set(FdeTextAlignment::TopLeft);
        self.tto_styles.set(FdeTextStyle {
            single_line: true,
            ..Default::default()
        });
        self.layout();
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if self.base.has_border() {
            self.base.draw_border(graphics, Part::Border, matrix);
        }
        let Some(app) = self.base.app() else {
            return;
        };
        let provider = app.theme_provider();
        let states = self.get_part_states();
        for (part, rect) in [
            (Part::Background, self.client_rect.get()),
            (Part::CheckBox, self.box_rect.get()),
        ] {
            let mut param = CfwlThemeBackground::new(part, self);
            param.states = states;
            param.matrix = matrix;
            param.part_rect = rect;
            if (self.base.get_states() & WidgetState::FOCUSED).0 != 0 {
                param.data_rect = Some(self.focus_rect.get());
            }
            provider.draw_background(&param, graphics);
        }
        let mut param = CfwlThemeText::new(Part::Caption, self);
        param.states = states;
        param.matrix = matrix;
        param.part_rect = self.caption_rect.get();
        param.text = caption_text();
        param.tto_styles = self.tto_styles.get();
        param.tto_align = self.tto_align.get();
        provider.draw_text(&param, graphics);
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        match &message.data {
            MessageData::SetFocus => self.on_focus(true),
            MessageData::KillFocus { .. } => self.on_focus(false),
            MessageData::Mouse { command, pos, .. } => match command {
                MouseCommand::LeftButtonDown => self.on_left_button_down(),
                MouseCommand::LeftButtonUp => self.on_left_button_up(*pos),
                MouseCommand::Move => self.on_mouse_move(*pos),
                MouseCommand::Leave => self.on_mouse_leave(),
                _ => {}
            },
            MessageData::Key {
                command: KeyCommand::KeyDown,
                key_code_or_char,
                ..
            } => {
                // Keyboard activation obeys the same disabled state as mouse input.
                if self.base.is_enabled() && matches!(*key_code_or_char, 0x0d | 0x20) {
                    self.next_states();
                }
            }
            _ => {}
        }
        if self.base.self_ref().is_some() && message.destination.is_some() {
            self.base.on_process_message(message);
        }
    }
}

#[cfg(test)]
#[path = "cfwl_controls_tests.rs"]
mod tests;
