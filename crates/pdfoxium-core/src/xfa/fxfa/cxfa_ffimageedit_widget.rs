// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Concrete `cxfa_ffimageedit.{h,cpp}` using actual FFField/PictureBox owners.
use super::{
    cxfa_fffield::CxfaFfField,
    cxfa_ffimage::ImageResource,
    cxfa_ffimageedit::{draw_edit_image, image_edit_rect},
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget, FfWidgetError, HighlightOption},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fpdfdoc::cpdf_formfield::FormFieldType,
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_event::FwlEvent,
            cfwl_message::{FwlMessage, MouseCommand},
            cfwl_picturebox::CfwlPictureBox,
            cfwl_widget::{WidgetAdapter, WidgetRef},
            ifwl_widgetdelegate::WidgetDelegate,
        },
    },
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
pub struct CxfaFfImageEdit {
    pub field: CxfaFfField,
    weak_self: Weak<Self>,
    old_delegate: RefCell<Option<Rc<dyn WidgetDelegate>>>,
}
impl CxfaFfImageEdit {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new_cyclic(|weak| Self {
            field: CxfaFfField::new_base(node),
            weak_self: weak.clone(),
            old_delegate: RefCell::new(None),
        })
    }
    pub fn resource(&self) -> Option<ImageResource> {
        self.widget().node().get_edit_image_resource()
    }
    pub fn pre_finalize(&self) {
        self.widget().node().set_edit_image(None);
        if let Some(widget) = self.field.normal_widget() {
            if let Some(app) = widget.base().app() {
                app.note_driver().unregister_event_target(&widget);
            }
        }
    }
}
impl Drop for CxfaFfImageEdit {
    fn drop(&mut self) {
        self.pre_finalize();
    }
}
impl LayoutItemWidget for CxfaFfImageEdit {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfImageEdit {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.field.base.get_rotate_matrix()
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(host) = self.field.base.doc_view() {
            host.display_caret(self, visible, anchor);
        }
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.field.base.get_border_color_and_thickness()
    }
}
impl FfWidget for CxfaFfImageEdit {
    fn widget(&self) -> &CxfaFfWidget {
        &self.field.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(&self.field)
    }
    fn pre_finalize(&self) {
        self.pre_finalize();
    }
    fn load_widget(&self) -> FfResult<bool> {
        let host = self.field.base.require_doc_view()?;
        let owner: Rc<dyn FfWidget> = self
            .weak_self
            .upgrade()
            .ok_or(FfWidgetError::DetachedDocumentView)?;
        let picture: WidgetRef = CfwlPictureBox::new(&host.fwl_app());
        host.fwl_app()
            .note_driver()
            .register_event_target(&picture, Some(&picture));
        *self.old_delegate.borrow_mut() = picture.base().get_delegate();
        self.field.attach_normal_widget(&owner, picture);
        self.field.load_widget_with_rect(|| self.set_fwl_rect())?;
        if self.widget().node().get_edit_image().is_none() {
            self.update_fwl_data();
        }
        Ok(true)
    }
    fn set_fwl_rect(&self) {
        if let Some(widget) = self.field.normal_widget() {
            widget.base().set_widget_rect(image_edit_rect(
                self.field.ui_rect(),
                self.widget().node().get_ui_margin(),
            ));
        }
    }
    fn update_fwl_data(&self) {
        self.widget().node().set_edit_image(None);
        let Some(owner) = self.widget().doc_view().and_then(|host| host.image_owner()) else {
            return;
        };
        self.widget().node().load_edit_image(owner.as_ref());
    }
    fn commit_data(&self) -> bool {
        true
    } // Native image selection commits through its host.
    fn get_form_field_type(&self) -> FormFieldType {
        FormFieldType::XfaImageField
    }
    fn accepts_focus_on_button_down(
        &self,
        flags: u32,
        point: CfxPointF,
        command: MouseCommand,
    ) -> bool {
        if command != MouseCommand::LeftButtonDown {
            return self
                .field
                .accepts_focus_on_button_down(flags, point, command);
        }
        self.widget().node().is_open_access() && self.field.pt_in_active_rect(point)
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.widget().set_button_down(true);
        let mut message = FwlMessage::mouse(
            self.field.normal_widget(),
            MouseCommand::LeftButtonDown,
            flags,
            self.field.fwl_to_client(point),
        );
        self.field.send_message_to_fwl_widget(&mut message);
        true
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_process_message(message);
        } else if let Some(widget) = self.field.normal_widget() {
            widget.on_process_message(message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        self.field.on_process_event(event);
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_process_event(event);
        } else if let Some(widget) = self.field.normal_widget() {
            widget.on_process_event(event);
        }
    }
    fn on_draw_widget(&self, g: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_draw_widget(g, matrix);
        } else if let Some(widget) = self.field.normal_widget() {
            widget.draw_widget(g, matrix);
        }
    }
    fn render_widget(
        &self,
        g: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        highlight: HighlightOption,
    ) -> FfResult<bool> {
        if !self.widget().has_visible_status() {
            return Ok(true);
        }
        let mut rotate = self.widget().get_rotate_matrix();
        rotate.concat(matrix);
        let mut success = self.widget().render_widget(g, rotate, highlight)?;
        success &= self.widget().draw_border(
            g,
            self.widget().node().get_ui_border().as_ref(),
            self.field.ui_rect(),
            rotate,
            false,
        );
        success &= self.field.render_caption(g, rotate)?;
        if let (Some(resource), Some(control)) = (self.resource(), self.field.normal_widget()) {
            draw_edit_image(
                g,
                self.widget().node(),
                &resource,
                control.get_widget_rect(),
                rotate,
            );
        }
        Ok(success)
    }
}
