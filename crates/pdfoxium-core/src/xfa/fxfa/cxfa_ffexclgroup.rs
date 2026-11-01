// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `cxfa_ffexclgroup.{h,cpp}` on the actual FF layout/model renderer.
use super::{
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget, HighlightOption},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    xfa::{fgas::graphics::cfgas_gegraphics::CfgasGeGraphics, fwl::cfwl_widget::WidgetAdapter},
};
use std::{ops::Deref, rc::Rc};
pub struct CxfaFfExclGroup {
    base: CxfaFfWidget,
}
impl CxfaFfExclGroup {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new(Self {
            base: CxfaFfWidget::new_base(node),
        })
    }
}
impl Deref for CxfaFfExclGroup {
    type Target = CxfaFfWidget;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
impl LayoutItemWidget for CxfaFfExclGroup {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfExclGroup {
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
impl FfWidget for CxfaFfExclGroup {
    fn widget(&self) -> &CxfaFfWidget {
        &self.base
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
        self.base.render_widget(g, rotate, highlight)
    }
}
