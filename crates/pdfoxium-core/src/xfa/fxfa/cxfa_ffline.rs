// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `cxfa_ffline.{h,cpp}` on the actual FF layout/model renderer.
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
pub struct CxfaFfLine {
    base: CxfaFfWidget,
}
impl CxfaFfLine {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new(Self {
            base: CxfaFfWidget::new_base(node),
        })
    }
}
impl Deref for CxfaFfLine {
    type Target = CxfaFfWidget;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
impl LayoutItemWidget for CxfaFfLine {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfLine {
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
impl FfWidget for CxfaFfLine {
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
        self.render_line(g, rotate)
    }
}

use super::fxfa_basic::XfaAttributeValue as V;
use crate::{
    fxge::cfx_graphstatedata::LineCap,
    xfa::fgas::graphics::{cfgas_gecolor::CfgasGeColor, cfgas_gepath::CfgasGePath},
};
/// Native GetRectFromHand, with the vertical right-hand offset corrected to
/// the opposite side of the line instead of duplicating the left-hand case.
pub(crate) fn rect_from_hand(rect: &mut CfxRectF, hand: V, width: f32) {
    let half = width / 2.;
    if rect.height < 1. {
        match hand {
            V::Left => rect.top -= half,
            V::Right => rect.top += half,
            _ => {}
        }
    } else if rect.width < 1. {
        match hand {
            V::Left => rect.left += half,
            V::Right => rect.left -= half,
            _ => {}
        }
    } else {
        match hand {
            V::Left => rect.inflate_xy(half, half),
            V::Right => rect.deflate_xy(half, half),
            _ => {}
        }
    }
}
impl CxfaFfLine {
    fn render_line(
        &self,
        g: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
    ) -> FfResult<bool> {
        let Some(value) = self.base.node().get_form_value_if_exists() else {
            return Ok(true);
        };
        let line = value.value_get_line_if_exists();
        let edge = line.as_ref().and_then(CxfaNode::line_get_edge_if_exists);
        if edge.as_ref().is_some_and(|e| !e.stroke_is_visible()) {
            return Ok(true);
        }
        let (color, width, stroke, cap) =
            edge.as_ref()
                .map_or((0xff000000, 1., V::Unknown, V::Unknown), |edge| {
                    (
                        edge.stroke_get_color(),
                        edge.stroke_get_thickness(),
                        edge.stroke_get_stroke_type(),
                        edge.stroke_get_cap_type(),
                    )
                });
        if !width.is_finite() || width < 0. {
            return Ok(false);
        }
        let mut rect = self.base.get_rect_without_rotate();
        rect_without_margin(&mut rect, self.base.node().get_margin_if_exists().as_ref());
        rect_from_hand(
            &mut rect,
            line.as_ref().map_or(V::Left, CxfaNode::line_get_hand),
            width,
        );
        let mut path = CfgasGePath::new();
        if line.as_ref().is_some_and(CxfaNode::line_get_slope)
            && rect.right() > 0.
            && rect.bottom() > 0.
        {
            path.add_line(rect.top_right(), rect.bottom_left());
        } else {
            path.add_line(rect.top_left(), rect.bottom_right());
        }
        let mut state = g.state_restorer();
        state.set_line_width(width);
        state.enable_act_on_dash();
        super::parser::cxfa_geometry::stroke_type_set_line_dash(&mut state, stroke, cap);
        state.set_stroke_color(&CfgasGeColor::Solid(color));
        state.set_line_cap(match cap {
            V::Round => LineCap::ROUND,
            V::Butt => LineCap::BUTT,
            _ => LineCap::SQUARE,
        });
        Ok(state.stroke_path(&path, matrix))
    }
}
