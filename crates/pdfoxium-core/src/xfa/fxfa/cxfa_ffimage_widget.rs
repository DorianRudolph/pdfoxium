// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Concrete `cxfa_ffimage.{h,cpp}` on the shared actual FF widget base.
use super::{
    cxfa_ffimage::{draw_layout_image, ImageResource},
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget, FfWidgetError, HighlightOption},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    xfa::{fgas::graphics::cfgas_gegraphics::CfgasGeGraphics, fwl::cfwl_widget::WidgetAdapter},
};
use std::rc::Rc;
pub struct CxfaFfImage {
    pub base: CxfaFfWidget,
}
impl CxfaFfImage {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new(Self {
            base: CxfaFfWidget::new_base(node),
        })
    }
    pub fn resource(&self) -> Option<ImageResource> {
        self.base.node().get_layout_image_resource()
    }
    pub fn pre_finalize(&self) {
        self.base.node().set_layout_image(None);
    }
}
impl Drop for CxfaFfImage {
    fn drop(&mut self) {
        self.pre_finalize();
    }
}
impl LayoutItemWidget for CxfaFfImage {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfImage {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.base.get_rotate_matrix()
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        self.base.display_caret(visible, anchor);
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.base.get_border_color_and_thickness()
    }
}
impl FfWidget for CxfaFfImage {
    fn widget(&self) -> &CxfaFfWidget {
        &self.base
    }
    fn pre_finalize(&self) {
        self.pre_finalize();
    }
    fn is_loaded(&self) -> bool {
        self.base.node().get_layout_image().is_some()
    }
    fn load_widget(&self) -> FfResult<bool> {
        if self.base.node().get_layout_image().is_some() {
            return Ok(true);
        }
        let host = self.base.require_doc_view()?;
        let owner = host
            .image_owner()
            .ok_or(FfWidgetError::DetachedDocumentView)?;
        if !self.base.node().load_layout_image(owner.as_ref()) {
            return Ok(false);
        }
        self.base.perform_layout();
        Ok(true)
    }
    fn render_widget(
        &self,
        g: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        highlight: HighlightOption,
    ) -> FfResult<bool> {
        if !self.base.has_visible_status() {
            return Ok(true);
        }
        let mut rotate = self.base.get_rotate_matrix();
        rotate.concat(matrix);
        let success = self.base.render_widget(g, rotate, highlight)?;
        if let Some(resource) = self.resource() {
            draw_layout_image(
                g,
                self.base.node(),
                &resource,
                self.base.get_rect_without_rotate(),
                rotate,
            );
        }
        Ok(success)
    }
}
