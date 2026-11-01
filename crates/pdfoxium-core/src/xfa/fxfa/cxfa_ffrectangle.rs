// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `cxfa_ffrectangle.{h,cpp}` on the actual FF layout/model renderer.
use super::{
    cxfa_ffwidget::{rect_without_margin, CxfaFfWidget, FfResult, FfWidget, HighlightOption},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    xfa::{fgas::graphics::cfgas_gegraphics::CfgasGeGraphics, fwl::cfwl_widget::WidgetAdapter},
};
use std::{ops::Deref, rc::Rc};
pub struct CxfaFfRectangle {
    base: CxfaFfWidget,
}
impl CxfaFfRectangle {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new(Self {
            base: CxfaFfWidget::new_base(node),
        })
    }
}
impl Deref for CxfaFfRectangle {
    type Target = CxfaFfWidget;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
impl LayoutItemWidget for CxfaFfRectangle {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfRectangle {
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
impl FfWidget for CxfaFfRectangle {
    fn widget(&self) -> &CxfaFfWidget {
        &self.base
    }
    fn render_widget(
        &self,
        g: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        _highlight: HighlightOption,
    ) -> FfResult<bool> {
        if !self.base.has_visible_status() {
            return Ok(true);
        }
        let mut rotate = self.base.get_rotate_matrix();
        rotate.concat(matrix);
        let Some(value) = self.base.node().get_form_value_if_exists() else {
            return Ok(true);
        };
        let mut rect = self.base.get_rect_without_rotate();
        rect_without_margin(&mut rect, self.base.node().get_margin_if_exists().as_ref());
        Ok(self.base.draw_border(
            g,
            value.value_get_rectangle_if_exists().as_ref(),
            rect,
            rotate,
            false,
        ))
    }
}
