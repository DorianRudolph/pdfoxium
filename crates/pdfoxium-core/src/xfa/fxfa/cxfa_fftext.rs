// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `cxfa_fftext.{h,cpp}` on the actual FF layout/model renderer.
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
pub struct CxfaFfText {
    base: CxfaFfWidget,
}
impl CxfaFfText {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new(Self {
            base: CxfaFfWidget::new_base(node),
        })
    }
}
impl Deref for CxfaFfText {
    type Target = CxfaFfWidget;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
impl LayoutItemWidget for CxfaFfText {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfText {
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
impl FfWidget for CxfaFfText {
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
        let border = self.base.render_widget(g, rotate, highlight)?;
        let host = self.base.require_doc_view()?;
        let Some(layout) = host.text_layout(self.base.node()) else {
            return Ok(border);
        };
        let mut rect = self.base.get_rect_without_rotate();
        if let Some(margin) = self.base.node().get_margin_if_exists() {
            let fragment = self.base.layout_item();
            let previous = fragment.as_ref().and_then(|i| i.previous());
            let next = fragment.as_ref().and_then(|i| i.next());
            if previous.is_none() && next.is_none() {
                rect_without_margin(&mut rect, Some(&margin));
            } else {
                let top = if previous.is_none() {
                    margin.margin_get_top_inset()
                } else {
                    0.
                };
                let bottom = if previous.is_some() && next.is_none() {
                    margin.margin_get_bottom_inset()
                } else {
                    0.
                };
                rect.deflate(
                    margin.margin_get_left_inset(),
                    top,
                    margin.margin_get_right_inset(),
                    bottom,
                );
            }
        }
        let mut translated = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
        translated.concat(rotate);
        let clip = rotate.transform_rect(rect);
        let index = self.base.layout_item().map_or(0, |item| item.get_index());
        let text = layout.with_layout(|layout| {
            layout.draw_block(g.get_render_device(), translated, clip, index)
        })?;
        Ok(border && text)
    }
    fn is_loaded(&self) -> bool {
        self.base
            .doc_view()
            .and_then(|host| host.text_layout(self.base.node()))
            .and_then(|layout| layout.with_layout(|layout| Ok(!layout.has_block())).ok())
            .unwrap_or(false)
    }
    fn perform_layout(&self) -> FfResult<()> {
        self.base.perform_layout();
        let host = self.base.require_doc_view()?;
        let Some(layout) = host.text_layout(self.base.node()) else {
            return Ok(());
        };
        layout.with_layout(|layout| {
            if !layout.has_block() {
                return Ok(());
            }
            layout.clear_blocks();
            let Some(fragment) = self.base.layout_item() else {
                return Ok(());
            };
            if fragment.previous().is_none() && fragment.next().is_none() {
                return Ok(());
            }
            let mut item = Some(fragment.get_first());
            while let Some(current) = item {
                let mut rect = current.get_absolute_rect();
                if let Some(margin) = self.base.node().get_margin_if_exists() {
                    if current.previous().is_none() {
                        rect.height -= margin.margin_get_top_inset();
                    } else if current.next().is_none() {
                        rect.height -= margin.margin_get_bottom_inset();
                    }
                }
                layout.item_blocks(rect, current.get_index())?;
                item = current.next();
            }
            layout.reset_has_block();
            Ok(())
        })
    }
    fn accepts_focus_on_button_down(
        &self,
        _flags: u32,
        point: CfxPointF,
        command: MouseCommand,
    ) -> bool {
        command == MouseCommand::LeftButtonDown
            && self.base.get_rect_without_rotate().contains_point(point)
            && self
                .get_link_url_at_point(point)
                .is_some_and(|url| !url.is_empty())
    }
    fn on_l_button_down(&self, _flags: u32, _point: CfxPointF) -> bool {
        self.base.set_button_down(true);
        true
    }
    fn on_l_button_up(&self, _flags: u32, point: CfxPointF) -> bool {
        if !self.base.is_button_down() {
            return false;
        }
        self.base.set_button_down(false);
        let Some(url) = self
            .get_link_url_at_point(point)
            .filter(|url| !url.is_empty())
        else {
            return false;
        };
        let Some(host) = self.base.doc_view() else {
            return false;
        };
        host.goto_url(&url);
        true
    }
    fn on_mouse_move(&self, _flags: u32, point: CfxPointF) -> bool {
        self.base.get_rect_without_rotate().contains_point(point)
            && self
                .get_link_url_at_point(point)
                .is_some_and(|url| !url.is_empty())
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        if self.on_mouse_move(0, point) {
            FwlWidgetHit::HyperLink
        } else {
            FwlWidgetHit::Unknown
        }
    }
}
use crate::fxcrt::fx_coordinates::CfxPointF;
use crate::xfa::fwl::{cfwl_message::MouseCommand, fwl_widgethit::FwlWidgetHit};
impl CxfaFfText {
    pub fn get_link_url_at_point(&self, point: CfxPointF) -> Option<Vec<u32>> {
        let host = self.base.doc_view()?;
        let layout = host.text_layout(self.base.node())?;
        let rect = self.base.get_rect_without_rotate();
        layout
            .with_layout(|layout| Ok(layout.get_link_url_at_point(point - rect.top_left())))
            .ok()
    }
}
