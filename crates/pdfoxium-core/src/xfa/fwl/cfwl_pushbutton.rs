// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/cfwl_pushbutton.{h,cpp}` port.
use super::{
    cfwl_app::CfwlApp,
    cfwl_event::{EventData, FwlEvent},
    cfwl_message::{FwlMessage, KeyCommand, MessageData, MouseCommand},
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
    cfwl_widget::{CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef, WidgetState},
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{cell::Cell, rc::Rc};

pub struct CfwlPushButton {
    base: CfwlWidget,
    button_down: Cell<bool>,
    client_rect: Cell<CfxRectF>,
    caption_rect: Cell<CfxRectF>,
}
impl CfwlPushButton {
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
            button_down: Cell::new(false),
            client_rect: Cell::new(CfxRectF::default()),
            caption_rect: Cell::new(CfxRectF::default()),
        });
        app.register_widget(control.clone() as WidgetRef);
        control
    }
    pub fn get_part_states(&self) -> PartState {
        let states = self.base.get_states();
        let mut part = PartState::NORMAL;
        if (states & WidgetState::FOCUSED).0 != 0 {
            part |= PartState::FOCUSED;
        }
        if (states & WidgetState::DISABLED).0 != 0 {
            part = PartState::DISABLED;
        } else if (states & WidgetState::PRESSED).0 != 0 {
            part |= PartState::PRESSED;
        } else if (states & WidgetState::HOVERED).0 != 0 {
            part |= PartState::HOVERED;
        }
        part
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
        self.button_down.set(true);
        self.base
            .set_states(WidgetState::HOVERED | WidgetState::PRESSED);
        self.base.repaint_rect(self.client_rect.get());
    }
    fn on_left_button_up(&self, pos: CfxPointF) {
        self.button_down.set(false);
        self.base.clear_states(WidgetState::PRESSED);
        if self.client_rect.get().contains_point(pos) {
            self.base.set_states(WidgetState::HOVERED);
            let mut event = FwlEvent::new(self.base.self_ref(), EventData::Click);
            self.base.dispatch_event(&mut event);
        } else {
            self.base.clear_states(WidgetState::HOVERED);
        }
        self.base.repaint_rect(self.client_rect.get());
    }
    fn on_mouse_move(&self, pos: CfxPointF) {
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
            self.base.repaint_rect(self.client_rect.get());
        }
    }
    fn on_mouse_leave(&self) {
        self.button_down.set(false);
        self.base
            .clear_states(WidgetState::HOVERED | WidgetState::PRESSED);
        self.base.repaint_rect(self.client_rect.get());
    }
    fn on_key_down(&self, key: u32) {
        if key != 0x0d {
            return;
        }
        let mut mouse = FwlEvent::new(
            self.base.self_ref(),
            EventData::Mouse(MouseCommand::LeftButtonUp),
        );
        self.base.dispatch_event(&mut mouse);
        // A callback can remove the widget or invalidate the event source.
        if mouse.source.is_none() || self.base.self_ref().is_none() {
            return;
        }
        let mut click = FwlEvent::new(self.base.self_ref(), EventData::Click);
        self.base.dispatch_event(&mut click);
    }
}
impl FwlWidget for CfwlPushButton {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::PushButton
    }
    fn set_states(&self, states: WidgetState) {
        if (states & WidgetState::DISABLED).0 != 0 {
            self.button_down.set(false);
            self.base.replace_states(WidgetState::DISABLED);
        } else {
            self.base.set_states(states);
        }
    }
    fn update(&self) {
        if self.base.is_locked() {
            return;
        }
        self.client_rect.set(self.get_client_rect());
        self.caption_rect.set(self.client_rect.get());
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if self.base.has_border() {
            self.base.draw_border(graphics, Part::Border, matrix);
        }
        let Some(app) = self.base.app() else {
            return;
        };
        let mut param = CfwlThemeBackground::new(Part::Background, self);
        param.states = self.get_part_states();
        param.matrix = matrix;
        param.part_rect = self.client_rect.get();
        if (self.base.get_states() & WidgetState::FOCUSED).0 != 0 {
            param.data_rect = Some(self.caption_rect.get());
        }
        app.theme_provider().draw_background(&param, graphics);
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        if !self.base.is_enabled() {
            return;
        }
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
            } => self.on_key_down(*key_code_or_char),
            _ => {}
        }
        if self.base.self_ref().is_some() && message.destination.is_some() {
            self.base.on_process_message(message);
        }
    }
}
