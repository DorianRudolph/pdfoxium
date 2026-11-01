// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Factory, encoding and configuration dispatch from `cfx_barcode.{h,cpp}`.
use super::{
    bc_library::{BcTextLoc, BcType},
    bc_writer::BarcodeFont,
    cbc_codebase::CbcCodeBase,
};
use crate::{
    fxcrt::fx_coordinates::CfxMatrix, fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
};
use std::rc::Rc;

pub struct CfxBarcode {
    engine: Option<CbcCodeBase>,
}
impl CfxBarcode {
    pub fn new(kind: BcType) -> Self {
        Self {
            engine: CbcCodeBase::new(kind),
        }
    }
    pub fn get_type(&self) -> BcType {
        // CBC_Code128 reports the Code128 family for both B and C encodings.
        self.engine
            .as_ref()
            .map_or(BcType::Unknown, CbcCodeBase::get_type)
    }
    pub fn encode(&mut self, contents: &[u32]) -> bool {
        self.engine
            .as_mut()
            .is_some_and(|code| code.encode(contents))
    }
    pub fn render_device(
        &mut self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
    ) -> bool {
        self.engine
            .as_mut()
            .is_some_and(|code| code.render_device(device, matrix))
    }
    pub fn set_module_height(&mut self, height: i32) -> bool {
        self.engine
            .as_mut()
            .is_some_and(|code| code.writer_mut().set_module_height(height))
    }
    pub fn set_module_width(&mut self, width: i32) -> bool {
        self.engine
            .as_mut()
            .is_some_and(|code| code.writer_mut().set_module_width(width))
    }
    pub fn set_height(&mut self, height: i32) {
        if let Some(code) = self.engine.as_mut() {
            code.writer_mut().set_height(height);
        }
    }
    pub fn set_width(&mut self, width: i32) {
        if let Some(code) = self.engine.as_mut() {
            code.writer_mut().set_width(width);
        }
    }
    pub fn set_print_checksum(&mut self, checksum: bool) -> bool {
        let Some(code) = self.engine.as_mut().and_then(CbcCodeBase::one_mut) else {
            return false;
        };
        code.set_print_checksum(checksum);
        true
    }
    pub fn set_data_length(&mut self, length: i32) -> bool {
        let Some(code) = self.engine.as_mut().and_then(CbcCodeBase::one_mut) else {
            return false;
        };
        code.set_data_length(length);
        true
    }
    pub fn set_cal_checksum(&mut self, state: bool) -> bool {
        let Some(code) = self.engine.as_mut().and_then(CbcCodeBase::one_mut) else {
            return false;
        };
        code.set_cal_checksum(state);
        true
    }
    pub fn set_font(&mut self, font: Rc<dyn BarcodeFont>) -> bool {
        self.engine
            .as_mut()
            .and_then(CbcCodeBase::one_mut)
            .is_some_and(|code| code.set_font(font))
    }
    pub fn set_font_size(&mut self, size: f32) -> bool {
        let Some(code) = self.engine.as_mut().and_then(CbcCodeBase::one_mut) else {
            return false;
        };
        code.set_font_size(size);
        true
    }
    pub fn set_font_color(&mut self, color: u32) -> bool {
        let Some(code) = self.engine.as_mut().and_then(CbcCodeBase::one_mut) else {
            return false;
        };
        code.set_font_color(color);
        true
    }
    pub fn set_text_location(&mut self, location: BcTextLoc) {
        if let Some(code) = self.engine.as_mut().and_then(CbcCodeBase::one_mut) {
            code.set_text_location(location);
        }
    }
    pub fn set_wide_narrow_ratio(&mut self, ratio: i8) -> bool {
        self.engine
            .as_mut()
            .and_then(CbcCodeBase::one_mut)
            .is_some_and(|code| code.set_wide_narrow_ratio(ratio))
    }
    pub fn set_start_char(&mut self, start: u8) -> bool {
        self.engine
            .as_mut()
            .and_then(CbcCodeBase::one_mut)
            .is_some_and(|code| code.set_start_char(start))
    }
    pub fn set_end_char(&mut self, end: u8) -> bool {
        self.engine
            .as_mut()
            .and_then(CbcCodeBase::one_mut)
            .is_some_and(|code| code.set_end_char(end))
    }
    pub fn set_error_correction_level(&mut self, level: i32) -> bool {
        match self.engine.as_mut() {
            Some(CbcCodeBase::Qr(code)) => code.writer.set_error_correction_level(level),
            Some(CbcCodeBase::Pdf417(code)) => code.writer.set_error_correction_level(level),
            // Linear writers and Data Matrix inherit the source unsupported setter.
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "cfx_barcode_tests.rs"]
mod tests;
