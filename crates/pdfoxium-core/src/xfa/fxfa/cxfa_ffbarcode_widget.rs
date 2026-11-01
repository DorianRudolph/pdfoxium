// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Complete `cxfa_ffbarcode.{h,cpp}` on the real TextEdit/FWL barcode engine.
use super::{
    cxfa_fffield::CxfaFfField,
    cxfa_fftextedit::CxfaFfTextEdit,
    cxfa_ffwidget::{CxfaFfWidget, FfResult, FfWidget, FfWidgetError, HighlightOption},
    fxfa_basic::XfaAttributeValue as V,
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::{cxfa_localevalue::XfaValuePicture, cxfa_node::CxfaNode},
};
use crate::{
    fxbarcode::bc_library::{BcTextLoc, BcType},
    fxcrt::{
        css::cfx_cssdata::hash_code_lowered_wide,
        fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    },
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_barcode::CfwlBarcode,
            cfwl_message::MouseCommand,
            cfwl_widget::{WidgetAdapter, WidgetRef},
        },
    },
};
use std::{cell::RefCell, rc::Rc};
pub struct CxfaFfBarcode {
    pub text_edit: CxfaFfTextEdit,
    barcode: CxfaNode,
    control: RefCell<Option<Rc<CfwlBarcode>>>,
}
impl CxfaFfBarcode {
    pub fn new(node: &CxfaNode, barcode: &CxfaNode) -> Rc<Self> {
        let value = Rc::new(Self {
            text_edit: CxfaFfTextEdit::new_base(node),
            barcode: barcode.clone(),
            control: RefCell::new(None),
        });
        let owner: Rc<dyn FfWidget> = value.clone();
        value.text_edit.bind_owner(&owner);
        value
    }
    pub fn control(&self) -> Option<Rc<CfwlBarcode>> {
        self.control.borrow().clone()
    }
    pub fn get_barcode_type_by_name(name: &[u32]) -> BcType {
        if name.is_empty() {
            return BcType::Unknown;
        }
        let hash = hash_code_lowered_wide(name);
        let index = BARCODES.partition_point(|entry| entry.0 < hash);
        BARCODES
            .get(index)
            .filter(|entry| name.iter().copied().eq(entry.1.bytes().map(u32::from)))
            .map_or(BcType::Unknown, |entry| entry.2)
    }
    pub fn update_widget_property(&self) {
        self.text_edit.update_widget_property();
        let kind = Self::get_barcode_type_by_name(&self.barcode.barcode_get_barcode_type());
        if kind == BcType::Unknown {
            return;
        }
        let Some(control) = self.control() else {
            return;
        };
        control.set_type(kind);
        if let Some(value) = self.barcode.barcode_get_checksum() {
            control.set_cal_checksum(value);
        }
        if let Some(value) = self.barcode.barcode_get_data_length() {
            control.set_data_length(value);
        }
        if let Some(value) = self.barcode.barcode_get_start_char() {
            control.set_start_char(value);
        }
        if let Some(value) = self.barcode.barcode_get_end_char() {
            control.set_end_char(value);
        }
        if let Some(value) = self.barcode.barcode_get_ec_level() {
            control.set_error_correction_level(value);
        }
        if let Some(value) = self.barcode.barcode_get_module_width() {
            control.set_module_width(value);
        }
        if let Some(value) = self.barcode.barcode_get_module_height() {
            control.set_module_height(value);
        }
        if let Some(value) = self.barcode.barcode_get_print_checksum() {
            control.set_print_checksum(value);
        }
        if let Some(value) = self.barcode.barcode_get_text_location() {
            let value = match value {
                V::None => Some(BcTextLoc::None),
                V::Above => Some(BcTextLoc::Above),
                V::Below => Some(BcTextLoc::Below),
                V::AboveEmbedded => Some(BcTextLoc::AboveEmbed),
                V::BelowEmbedded => Some(BcTextLoc::BelowEmbed),
                _ => None,
            };
            if let Some(value) = value {
                control.set_text_location(value);
            }
        }
        if let Some(value) = self.barcode.barcode_get_wide_narrow_ratio() {
            control.set_wide_narrow_ratio(value);
        }
        if matches!(
            kind,
            BcType::Code39 | BcType::Ean8 | BcType::Ean13 | BcType::Upca
        ) {
            control.set_print_checksum(true);
        }
    }
}
impl LayoutItemWidget for CxfaFfBarcode {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.text_edit.field.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfBarcode {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.widget().get_rotate_matrix()
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(host) = self.widget().doc_view() {
            host.display_caret(self, visible, anchor);
        }
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.widget().get_border_color_and_thickness()
    }
}
impl FfWidget for CxfaFfBarcode {
    fn widget(&self) -> &CxfaFfWidget {
        &self.text_edit.field.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(&self.text_edit.field)
    }
    fn as_text_edit(&self) -> Option<&CxfaFfTextEdit> {
        Some(&self.text_edit)
    }
    fn load_widget(&self) -> FfResult<bool> {
        let host = self.widget().require_doc_view()?;
        let owner = self
            .text_edit
            .owner()
            .ok_or(FfWidgetError::DetachedDocumentView)?;
        let control = CfwlBarcode::new(&host.fwl_app());
        let widget: WidgetRef = control.clone();
        self.text_edit.attach_edit_widget(&owner, widget.clone());
        *self.control.borrow_mut() = Some(control.clone());
        {
            let _lock = widget.base().scoped_update_lock();
            control.set_text(&self.widget().node().get_value(XfaValuePicture::Display));
            self.update_widget_property();
        }
        self.text_edit.field.load_widget()
    }
    fn update_widget_property(&self) {
        self.update_widget_property();
    }
    fn accepts_focus_on_button_down(
        &self,
        flags: u32,
        point: CfxPointF,
        command: MouseCommand,
    ) -> bool {
        let Some(control) = self.control() else {
            return false;
        };
        if control.is_protected_type() {
            return false;
        }
        if command == MouseCommand::LeftButtonDown && !self.widget().node().is_open_access() {
            return false;
        }
        self.text_edit
            .accepts_focus_on_button_down(flags, point, command)
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
            self.text_edit.field.ui_rect(),
            rotate,
            false,
        );
        success &= self.text_edit.field.render_caption(g, rotate)?;
        if let Some(widget) = self.text_edit.field.normal_widget() {
            let rect = widget.get_widget_rect();
            let mut translated = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
            translated.concat(rotate);
            widget.draw_widget(g, translated);
        }
        Ok(success)
    }
}

// The complete source name/hash/type table follows below.
const BARCODES: &[(u32, &str, BcType)] = &[
    (0x7fb4a18, "ean13", BcType::Ean13),
    (0x8d13a3d, "code11", BcType::Unknown),
    (0x8d149a8, "code49", BcType::Unknown),
    (0x8d16347, "code93", BcType::Unknown),
    (0x91a92e2, "upsMaxicode", BcType::Unknown),
    (0xa7d48dc, "fim", BcType::Unknown),
    (0xb359fe9, "msi", BcType::Unknown),
    (0x121f738c, "code2Of5Matrix", BcType::Unknown),
    (0x15358616, "ucc128", BcType::Unknown),
    (0x1f4bfa05, "rfid", BcType::Unknown),
    (0x1fda71bc, "rss14Stacked", BcType::Unknown),
    (0x22065087, "ean8add2", BcType::Unknown),
    (0x2206508a, "ean8add5", BcType::Unknown),
    (0x2278366c, "codabar", BcType::Codabar),
    (0x2a039a8d, "telepen", BcType::Unknown),
    (0x323ed337, "upcApwcd", BcType::Unknown),
    (0x347a1846, "postUSIMB", BcType::Unknown),
    (0x391bb836, "code128", BcType::Code128),
    (0x398eddaf, "dataMatrix", BcType::DataMatrix),
    (0x3cff60a8, "upcEadd2", BcType::Unknown),
    (0x3cff60ab, "upcEadd5", BcType::Unknown),
    (0x402cb188, "code2Of5Standard", BcType::Unknown),
    (0x411764f7, "aztec", BcType::Unknown),
    (0x44d4e84c, "ean8", BcType::Ean8),
    (0x48468902, "ucc128sscc", BcType::Unknown),
    (0x4880aea4, "upcAadd2", BcType::Unknown),
    (0x4880aea7, "upcAadd5", BcType::Unknown),
    (0x54f18256, "code2Of5Industrial", BcType::Unknown),
    (0x58e15f25, "rss14Limited", BcType::Unknown),
    (0x5c08d1b9, "postAUSReplyPaid", BcType::Unknown),
    (0x5fa700bd, "rss14", BcType::Unknown),
    (0x631a7e35, "logmars", BcType::Unknown),
    (0x6a236236, "pdf417", BcType::Pdf417),
    (0x6d098ece, "upcean2", BcType::Unknown),
    (0x6d098ed1, "upcean5", BcType::Unknown),
    (0x76b04eed, "code3Of9extended", BcType::Unknown),
    (0x7c7db84a, "maxicode", BcType::Unknown),
    (0x8266f7f7, "ucc128random", BcType::Unknown),
    (0x83eca147, "postUSDPBC", BcType::Unknown),
    (0x8dd71de0, "postAUSStandard", BcType::Unknown),
    (0x98adad85, "plessey", BcType::Unknown),
    (0x9f84cce6, "ean13pwcd", BcType::Unknown),
    (0xb514fbe9, "upcA", BcType::Upca),
    (0xb514fbed, "upcE", BcType::Unknown),
    (0xb5c6a853, "ean13add2", BcType::Unknown),
    (0xb5c6a856, "ean13add5", BcType::Unknown),
    (0xb81fc512, "postUKRM4SCC", BcType::Unknown),
    (0xbad34b22, "code128SSCC", BcType::Unknown),
    (0xbfbe0cf6, "postUS5Zip", BcType::Unknown),
    (0xc56618e8, "pdf417macro", BcType::Unknown),
    (0xca730f8a, "code2Of5Interleaved", BcType::Unknown),
    (0xd0097ac6, "rss14Expanded", BcType::Unknown),
    (0xd25a0240, "postAUSCust2", BcType::Unknown),
    (0xd25a0241, "postAUSCust3", BcType::Unknown),
    (0xd53ed3e7, "rss14Truncated", BcType::Unknown),
    (0xe72bcd57, "code128A", BcType::Unknown),
    (0xe72bcd58, "code128B", BcType::Code128B),
    (0xe72bcd59, "code128C", BcType::Code128C),
    (0xee83c50f, "rss14StackedOmni", BcType::Unknown),
    (0xf2a18f7e, "QRCode", BcType::QrCode),
    (0xfaeaf37f, "postUSStandard", BcType::Unknown),
    (0xfb48155c, "code3Of9", BcType::Code39),
];

#[cfg(test)]
#[path = "cxfa_ffimage_barcode_widget_tests.rs"]
mod tests;
