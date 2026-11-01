// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `cxfa_ffsignature.{cpp,h}`. The native signature field draws its
//! borders/caption/highlight and declines every mouse/key action.
use super::{
    cxfa_fffield::{CxfaFfField, ShapeOption},
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget, HighlightOption},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fpdfdoc::cpdf_formfield::FormFieldType,
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, CfxVector},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_message::MouseCommand, cfwl_widget::WidgetAdapter, fwl_widgethit::FwlWidgetHit,
        },
    },
};
use std::{ops::Deref, rc::Rc};

pub struct CxfaFfSignature {
    field: CxfaFfField,
}
impl CxfaFfSignature {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new(Self {
            field: CxfaFfField::new_base(node),
        })
    }
}
impl Deref for CxfaFfSignature {
    type Target = CxfaFfField;
    fn deref(&self) -> &Self::Target {
        &self.field
    }
}
impl LayoutItemWidget for CxfaFfSignature {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfSignature {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.field.base.get_rotate_matrix()
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.field.base.get_border_color_and_thickness()
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(host) = self.field.base.doc_view() {
            host.display_caret(self, visible, anchor);
        }
    }
}
impl FfWidget for CxfaFfSignature {
    fn widget(&self) -> &CxfaFfWidget {
        &self.field.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(&self.field)
    }
    fn load_widget(&self) -> FfResult<bool> {
        debug_assert!(!self.field.is_loaded());
        self.field.load_widget()
    }
    fn render_widget(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        highlight: HighlightOption,
    ) -> FfResult<bool> {
        if !self.field.base.has_visible_status() {
            return Ok(true);
        }
        let mut rotate = self.get_rotate_matrix();
        rotate.concat(matrix);
        let mut success = self.field.base.render_widget(graphics, rotate, highlight)?;
        success &= self.field.base.draw_border(
            graphics,
            self.field.base.node().get_ui_border().as_ref(),
            self.field.ui_rect(),
            rotate,
            false,
        );
        success &= self.field.render_caption(graphics, rotate)?;
        self.field
            .draw_highlight(graphics, rotate, highlight, ShapeOption::Square);
        Ok(success)
    }
    fn accepts_focus_on_button_down(&self, _: u32, _: CfxPointF, _: MouseCommand) -> bool {
        false
    }
    fn on_mouse_enter(&self) -> bool {
        false
    }
    fn on_mouse_exit(&self) -> bool {
        false
    }
    fn on_l_button_down(&self, _: u32, _: CfxPointF) -> bool {
        false
    }
    fn on_l_button_up(&self, _: u32, _: CfxPointF) -> bool {
        false
    }
    fn on_l_button_dbl_clk(&self, _: u32, _: CfxPointF) -> bool {
        false
    }
    fn on_mouse_move(&self, _: u32, _: CfxPointF) -> bool {
        false
    }
    fn on_mouse_wheel(&self, _: u32, _: CfxPointF, _: CfxVector) -> bool {
        false
    }
    fn on_r_button_down(&self, _: u32, _: CfxPointF) -> bool {
        false
    }
    fn on_r_button_up(&self, _: u32, _: CfxPointF) -> bool {
        false
    }
    fn on_r_button_dbl_clk(&self, _: u32, _: CfxPointF) -> bool {
        false
    }
    fn on_key_down(&self, _: u32, _: u32) -> bool {
        false
    }
    fn on_char(&self, _: u32, _: u32) -> bool {
        false
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        if self.field.normal_widget().is_some_and(|widget| {
            widget.hit_test(self.field.fwl_to_client(point)) != FwlWidgetHit::Unknown
        }) {
            return FwlWidgetHit::Client;
        }
        if !self
            .field
            .base
            .get_rect_without_rotate()
            .contains_point(point)
        {
            return FwlWidgetHit::Unknown;
        }
        if self.field.caption_rect().contains_point(point) {
            FwlWidgetHit::Titlebar
        } else {
            FwlWidgetHit::Client
        }
    }
    fn get_form_field_type(&self) -> FormFieldType {
        FormFieldType::XfaSignature
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::fx_coordinates::CfxSizeF,
        fxge::{
            agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
            dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
        },
        xfa::fxfa::{
            cxfa_ffwidget::FfWidgetError,
            fxfa::XfaWidgetStatus,
            fxfa_basic::{XfaAttribute, XfaElement, XfaPacketType, XfaUnit},
            layout::cxfa_layoutitem::CxfaLayoutStore,
            parser::{cxfa_document::CxfaDocument, cxfa_measurement::CxfaMeasurement},
        },
    };
    fn field(document: &Rc<CxfaDocument>) -> Rc<CxfaFfSignature> {
        let node = document
            .create_node(XfaPacketType::Form, XfaElement::Field)
            .unwrap();
        let ui = node.get_or_create_property(0, XfaElement::Ui).unwrap();
        ui.get_or_create_property(0, XfaElement::Signature).unwrap();
        CxfaFfSignature::new(&node)
    }
    #[test]
    fn signature_declines_all_native_input_actions_and_preserves_hit_regions() {
        let document = CxfaDocument::new();
        let signature = field(&document);
        let layout = CxfaLayoutStore::new();
        let item = layout.create_content(signature.base.node(), Some(signature.clone()));
        item.set_position(CfxPointF::new(10., 10.));
        item.set_size(CfxSizeF::new(40., 20.));
        signature.set_caption_rect(CfxRectF::new(10., 10., 40., 5.));
        let widget: &dyn FfWidget = signature.as_ref();
        let point = CfxPointF::new(15., 20.);
        assert_eq!(widget.get_form_field_type(), FormFieldType::XfaSignature);
        assert!(!widget.on_mouse_enter());
        assert!(!widget.on_mouse_exit());
        for flags in [0, u32::MAX] {
            assert!(!widget.accepts_focus_on_button_down(
                flags,
                point,
                MouseCommand::LeftButtonDown
            ));
            assert!(!widget.on_l_button_down(flags, point));
            assert!(!widget.on_l_button_up(flags, point));
            assert!(!widget.on_l_button_dbl_clk(flags, point));
            assert!(!widget.on_mouse_move(flags, point));
            assert!(!widget.on_mouse_wheel(flags, point, CfxVector::new(1, -1)));
            assert!(!widget.on_r_button_down(flags, point));
            assert!(!widget.on_r_button_up(flags, point));
            assert!(!widget.on_r_button_dbl_clk(flags, point));
            assert!(!widget.on_key_down(13, flags));
            assert!(!widget.on_char(65, flags));
        }
        assert_eq!(
            widget.hit_test(CfxPointF::new(9., 10.)),
            FwlWidgetHit::Unknown
        );
        assert_eq!(
            widget.hit_test(CfxPointF::new(11., 11.)),
            FwlWidgetHit::Titlebar
        );
        assert_eq!(widget.hit_test(point), FwlWidgetHit::Client);
        assert_eq!(
            widget.load_widget(),
            Err(FfWidgetError::DetachedDocumentView)
        );
    }
    #[test]
    fn visible_signature_border_uses_real_agg_graphics_and_hidden_skips_drawing() {
        let document = CxfaDocument::new();
        let signature = field(&document);
        let layout = CxfaLayoutStore::new();
        let item = layout.create_content(signature.base.node(), Some(signature.clone()));
        item.set_position(CfxPointF::new(8., 8.));
        item.set_size(CfxSizeF::new(40., 24.));
        signature.set_ui_rect(CfxRectF::new(8., 8., 40., 24.));
        let border = signature
            .base
            .node()
            .get_or_create_property(0, XfaElement::Border)
            .unwrap();
        let edge = border.get_or_create_property(0, XfaElement::Edge).unwrap();
        edge.set_measure(
            XfaAttribute::Thickness,
            CxfaMeasurement::new(2., XfaUnit::Pt),
            false,
        );
        edge.stroke_set_color(0xffc03020);
        let mut bitmap = CfxDibitmap::create(64, 64, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0xffffffff);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut graphics = CfgasGeGraphics::new(&mut device);
            assert!(FfWidget::render_widget(
                signature.as_ref(),
                &mut graphics,
                CfxMatrix::default(),
                HighlightOption::NoHighlight
            )
            .unwrap());
        }
        assert!(bitmap.buffer().iter().all(|&byte| byte == 255));
        item.set_status_bits(XfaWidgetStatus::kVisible);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut graphics = CfgasGeGraphics::new(&mut device);
            assert!(FfWidget::render_widget(
                signature.as_ref(),
                &mut graphics,
                CfxMatrix::default(),
                HighlightOption::NoHighlight
            )
            .unwrap());
        }
        assert!(bitmap
            .buffer()
            .chunks_exact(4)
            .any(|pixel| pixel != [255, 255, 255, 255]));
    }
}
