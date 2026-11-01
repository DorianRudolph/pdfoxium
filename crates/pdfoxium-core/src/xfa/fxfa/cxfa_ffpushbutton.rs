// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_ffpushbutton.{h,cpp}` with real FWL input and alternate text layouts.
use super::{
    cxfa_fffield::CxfaFfField,
    cxfa_ffwidget::{
        rect_without_margin, CaptionLayout, CxfaFfWidget, FfResult, FfWidget, HighlightOption,
    },
    cxfa_textprovider::TextProviderType,
    fxfa_basic::XfaAttributeValue as V,
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fpdfdoc::cpdf_formfield::FormFieldType,
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    fxge::cfx_fillrenderoptions::FillType,
    xfa::{
        fgas::graphics::{
            cfgas_gecolor::CfgasGeColor, cfgas_gegraphics::CfgasGeGraphics,
            cfgas_gepath::CfgasGePath,
        },
        fwl::{
            cfwl_event::FwlEvent,
            cfwl_message::FwlMessage,
            cfwl_pushbutton::CfwlPushButton,
            cfwl_widget::{FwlWidget, WidgetAdapter, WidgetRef, WidgetState},
            ifwl_widgetdelegate::WidgetDelegate,
        },
    },
};
use std::{
    cell::RefCell,
    ops::Deref,
    rc::{Rc, Weak},
};
pub const HIGHLIGHT_INVERTED: u32 = 1;
pub const HIGHLIGHT_PUSH: u32 = 2;
pub const HIGHLIGHT_OUTLINE: u32 = 4;
pub struct CxfaFfPushButton {
    field: CxfaFfField,
    button: CxfaNode,
    weak_self: Weak<Self>,
    control: RefCell<Option<Rc<CfwlPushButton>>>,
    old_delegate: RefCell<Option<Rc<dyn WidgetDelegate>>>,
    rollover: RefCell<Option<CaptionLayout>>,
    down: RefCell<Option<CaptionLayout>>,
}
impl CxfaFfPushButton {
    pub fn new(node: &CxfaNode, button: &CxfaNode) -> Rc<Self> {
        Rc::new_cyclic(|weak| Self {
            field: CxfaFfField::new_base(node),
            button: button.clone(),
            weak_self: weak.clone(),
            control: RefCell::new(None),
            old_delegate: RefCell::new(None),
            rollover: RefCell::new(None),
            down: RefCell::new(None),
        })
    }
    pub fn control(&self) -> Option<Rc<CfwlPushButton>> {
        self.control.borrow().clone()
    }
    fn line_width(&self) -> f32 {
        self.field
            .base
            .node()
            .get_border_if_exists()
            .filter(|b| b.box_get_presence() == V::Visible)
            .and_then(|b| b.box_get_edge(0))
            .map_or(0., |e| e.stroke_get_thickness())
    }
    fn load_highlight_caption(&self) -> FfResult<()> {
        let node = self.field.base.node();
        if !node
            .get_caption_if_exists()
            .is_some_and(|c| !c.caption_is_hidden())
        {
            return Ok(());
        }
        let host = self.field.base.require_doc_view()?;
        if node.has_button_rollover() {
            let layout = host.load_text_layout(node, TextProviderType::Rollover)?;
            self.rollover.replace(layout);
        }
        if node.has_button_down() {
            let layout = host.load_text_layout(node, TextProviderType::Down)?;
            self.down.replace(layout);
        }
        Ok(())
    }
    fn layout_highlight_caption(&self) -> FfResult<()> {
        let size = self.field.caption_rect().size();
        self.field.layout_caption()?;
        let rollover = self.rollover.borrow().clone();
        let down = self.down.borrow().clone();
        for layout in [rollover, down].into_iter().flatten() {
            layout.with_layout(|layout| layout.layout(size))?;
        }
        Ok(())
    }
    fn render_highlight_caption(
        &self,
        g: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
    ) -> FfResult<bool> {
        let node = self.field.base.node();
        if !node
            .get_caption_if_exists()
            .is_some_and(|c| c.caption_is_visible())
        {
            return Ok(true);
        }
        let Some(control) = self.control() else {
            return Ok(true);
        };
        let states = control.base().get_states();
        let rect = self.field.caption_rect();
        let mut clip = rect;
        clip.intersect(self.field.base.get_rect_without_rotate());
        clip = matrix.transform_rect(clip);
        let mut translated = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
        translated.concat(matrix);
        let down = self.down.borrow().clone();
        let selected =
            if down.is_some() && states.contains(WidgetState::PRESSED | WidgetState::HOVERED) {
                down
            } else if states.contains(WidgetState::HOVERED) {
                self.rollover.borrow().clone()
            } else {
                None
            };
        if let Some(layout) = selected {
            if layout
                .with_layout(|layout| layout.draw_string(g.get_render_device(), translated, clip))?
            {
                return Ok(true);
            }
        }
        let layout = self.field.base.require_doc_view()?.caption_layout(node);
        if let Some(layout) = layout {
            layout.with_layout(|layout| layout.draw_string(g.get_render_device(), translated, clip))
        } else {
            Ok(true)
        }
    }
}
impl Deref for CxfaFfPushButton {
    type Target = CxfaFfField;
    fn deref(&self) -> &Self::Target {
        &self.field
    }
}
impl LayoutItemWidget for CxfaFfPushButton {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfPushButton {
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
impl FfWidget for CxfaFfPushButton {
    fn widget(&self) -> &CxfaFfWidget {
        &self.field.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(&self.field)
    }
    fn load_widget(&self) -> FfResult<bool> {
        if self.control().is_some() {
            return Ok(false);
        }
        let host = self.field.base.require_doc_view()?;
        let app = host.fwl_app();
        let control = CfwlPushButton::new(&app);
        self.old_delegate.replace(control.base().get_delegate());
        let owner: Rc<dyn FfWidget> = self.weak_self.upgrade().expect("live FF button");
        let widget: WidgetRef = control.clone();
        self.field.attach_normal_widget(&owner, widget.clone());
        self.control.replace(Some(control.clone()));
        app.note_driver()
            .register_event_target(&widget, Some(&widget));
        {
            let _lock = control.base().scoped_update_lock();
            self.update_widget_property();
            self.load_highlight_caption()?;
        }
        host.load_caption(self.field.base.node())?;
        self.perform_layout()?;
        Ok(true)
    }
    fn update_widget_property(&self) {
        let Some(control) = self.control() else {
            return;
        };
        let style = match self.button.button_get_highlight() {
            V::Inverted => HIGHLIGHT_INVERTED,
            V::Outline => HIGHLIGHT_OUTLINE,
            V::Push => HIGHLIGHT_PUSH,
            _ => 0,
        };
        control.base().modify_style_exts(style, u32::MAX);
    }
    fn perform_layout(&self) -> FfResult<()> {
        self.field.base.perform_layout();
        let mut rect = self.field.base.get_rect_without_rotate();
        self.field.set_ui_rect(rect);
        rect_without_margin(
            &mut rect,
            self.field.base.node().get_margin_if_exists().as_ref(),
        );
        let caption_margin = self
            .field
            .base
            .node()
            .get_caption_if_exists()
            .and_then(|c| c.caption_get_margin_if_exists());
        rect_without_margin(&mut rect, caption_margin.as_ref());
        self.field.set_caption_rect(rect);
        self.layout_highlight_caption()?;
        self.field.set_fwl_rect();
        if let Some(control) = self.control() {
            control.update();
        }
        Ok(())
    }
    fn render_widget(
        &self,
        g: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        highlight: HighlightOption,
    ) -> FfResult<bool> {
        if !self.field.base.has_visible_status() {
            return Ok(true);
        }
        let mut rotate = self.get_rotate_matrix();
        rotate.concat(matrix);
        let border = self.field.base.render_widget(g, rotate, highlight)?;
        let caption = self.render_highlight_caption(g, rotate)?;
        if let Some(control) = self.control() {
            let rect = self.field.base.get_rect_without_rotate();
            let mut translated = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
            translated.concat(rotate);
            let widget: WidgetRef = control;
            self.field
                .base
                .require_doc_view()?
                .fwl_app()
                .widget_mgr()
                .on_draw_widget(&widget, g, translated);
        }
        Ok(border && caption)
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_process_message(message);
        } else if let Some(control) = self.control() {
            control.on_process_message(message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        let old = self.old_delegate.borrow().clone();
        if let Some(old) = old {
            old.on_process_event(event);
        } else if let Some(control) = self.control() {
            control.on_process_event(event);
        }
        self.field.on_process_event(event);
    }
    fn on_draw_widget(&self, g: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        let Some(control) = self.control() else {
            return;
        };
        let states = control.base().get_states();
        if !states.contains(WidgetState::PRESSED | WidgetState::HOVERED) {
            return;
        }
        let style = control.base().get_style_exts();
        if style & HIGHLIGHT_INVERTED != 0 {
            let size = control.get_widget_rect().size();
            let mut rect = CfxRectF::new(0., 0., size.width, size.height);
            let width = self.line_width();
            rect.deflate_xy(width, width);
            let mut path = CfgasGePath::new();
            path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
            g.set_fill_color(&CfgasGeColor::Solid(0x8080ffff));
            g.fill_path(&path, FillType::Winding, matrix);
        } else if style & HIGHLIGHT_OUTLINE != 0 {
            let rect = control.get_widget_rect();
            let mut path = CfgasGePath::new();
            path.add_rectangle(0., 0., rect.width, rect.height);
            g.set_stroke_color(&CfgasGeColor::Solid(0xff80ffff));
            g.set_line_width(self.line_width());
            g.stroke_path(&path, matrix);
        }
    }
    fn get_form_field_type(&self) -> FormFieldType {
        FormFieldType::XfaPushButton
    }
}
