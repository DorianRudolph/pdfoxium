// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cfwl_comboedit.{h,cpp}`. Composition shares the edit's actual widget
//! identity; only this wrapper is registered in the app's widget tree.
use super::{
    cfwl_app::CfwlApp,
    cfwl_edit::CfwlEdit,
    cfwl_event::FwlEvent,
    cfwl_message::{FwlMessage, MessageData, MouseCommand},
    cfwl_widget::{CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef, WidgetState},
    fwl_widgethit::FwlWidgetHit,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{ops::Deref, rc::Rc};

pub struct CfwlComboEdit {
    edit: CfwlEdit,
}
impl CfwlComboEdit {
    pub fn new(app: &Rc<CfwlApp>, properties: Properties, outer: Option<WidgetRef>) -> Rc<Self> {
        let control = Rc::new(Self {
            edit: CfwlEdit::new_unregistered(app, properties, outer),
        });
        app.register_widget(control.clone());
        control
    }
    pub fn clear_selected(&self) {
        self.edit.clear_selection();
        self.base().repaint_rect(self.edit.client_rect());
    }
    pub fn set_selected(&self) {
        self.base().set_states(WidgetState::FOCUSED);
        self.edit.select_all();
    }
}
impl Deref for CfwlComboEdit {
    type Target = CfwlEdit;
    fn deref(&self) -> &Self::Target {
        &self.edit
    }
}
impl FwlWidget for CfwlComboEdit {
    fn base(&self) -> &CfwlWidget {
        self.edit.base()
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::Edit
    }
    fn update(&self) {
        self.edit.update();
    }
    fn draw_widget(&self, g: &mut CfgasGeGraphics, m: CfxMatrix) {
        self.edit.draw_widget(g, m);
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        match &message.data {
            MessageData::SetFocus => {
                self.base().set_states(WidgetState::FOCUSED);
                return;
            }
            MessageData::KillFocus { .. } => {
                // Forward blur before clearing the bit: the edit must hide its
                // caret, stop its timer and clear its selection as well.
                self.edit.on_process_message(message);
                return;
            }
            MessageData::Mouse {
                command: MouseCommand::LeftButtonDown,
                ..
            } => {
                if !self.base().get_states().contains(WidgetState::FOCUSED) {
                    self.set_selected();
                }
            }
            _ => {}
        }
        if !self.base().is_destroyed() {
            self.edit.on_process_message(message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        self.edit.on_process_event(event);
    }
    fn get_widget_rect(&self) -> CfxRectF {
        self.edit.get_widget_rect()
    }
    fn get_client_rect(&self) -> CfxRectF {
        self.edit.get_client_rect()
    }
    fn get_autosized_widget_rect(&self) -> CfxRectF {
        self.edit.get_autosized_widget_rect()
    }
    fn hit_test(&self, p: CfxPointF) -> FwlWidgetHit {
        self.edit.hit_test(p)
    }
    fn set_states(&self, s: WidgetState) {
        self.edit.set_states(s);
    }
    fn clear_states(&self, s: WidgetState) {
        self.edit.clear_states(s);
    }
    fn modify_style_exts(&self, a: u32, r: u32) {
        self.edit.modify_style_exts(a, r);
    }
    fn pre_finalize(&self) {
        self.edit.pre_finalize();
    }
}
