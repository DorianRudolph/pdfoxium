// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `cfwl_datetimeedit.{h,cpp}`. Composition retains one widget identity;
//! the contained edit is not separately registered with the widget manager.
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

pub struct CfwlDateTimeEdit {
    edit: CfwlEdit,
}
impl CfwlDateTimeEdit {
    pub fn new(app: &Rc<CfwlApp>, properties: Properties, outer: Option<WidgetRef>) -> Rc<Self> {
        let edit = Rc::new(Self {
            edit: CfwlEdit::new_unregistered(app, properties, outer),
        });
        app.register_widget(edit.clone());
        edit
    }
}
impl Deref for CfwlDateTimeEdit {
    type Target = CfwlEdit;
    fn deref(&self) -> &Self::Target {
        &self.edit
    }
}
impl FwlWidget for CfwlDateTimeEdit {
    fn base(&self) -> &CfwlWidget {
        self.edit.base()
    }
    fn get_class_id(&self) -> FwlType {
        self.edit.get_class_id()
    }
    fn update(&self) {
        self.edit.update();
    }
    fn draw_widget(&self, g: &mut CfgasGeGraphics, m: CfxMatrix) {
        self.edit.draw_widget(g, m);
    }
    fn on_draw_widget(&self, g: &mut CfgasGeGraphics, m: CfxMatrix) {
        self.edit.on_draw_widget(g, m);
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
    fn on_process_event(&self, e: &mut FwlEvent) {
        self.edit.on_process_event(e);
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        if let MessageData::Mouse {
            command: MouseCommand::LeftButtonDown | MouseCommand::RightButtonDown,
            ..
        } = message.data
        {
            self.base().set_states(WidgetState::FOCUSED);
            if let Some(outer) = self.base().outer() {
                if let Some(picker) = outer.as_datetime_picker() {
                    if picker.is_month_calendar_visible() {
                        let mut invalid = outer.get_widget_rect();
                        picker.hide_month_calendar();
                        invalid.offset(-invalid.left, -invalid.top);
                        outer.base().repaint_rect(invalid);
                    }
                }
            }
        }
        if !self.base().is_destroyed() {
            self.edit.on_process_message(message);
        }
    }
}
