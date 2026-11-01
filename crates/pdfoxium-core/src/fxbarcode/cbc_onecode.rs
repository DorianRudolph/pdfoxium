// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original ZXing code copyright 2011, licensed under the Apache License 2.0.
// https://www.apache.org/licenses/LICENSE-2.0
//! `cbc_onecode`, `cbc_codabar`, `cbc_code128`, `cbc_code39`, `cbc_eancode`,
//! `cbc_ean8`, `cbc_ean13`, and `cbc_upca` (headers and implementations).
//! Concrete C++ wrappers are represented by the closed `OneDimWriter` enum;
//! encoding, preprocessing and render contents retain their source ownership.
use crate::fxbarcode::{
    bc_library::{BcTextLoc, BcType},
    bc_writer::{BarcodeFont, CbcWriter},
    oned::{
        bc_one_dim_writer::{wide_to_utf8, CbcOneDimWriter},
        bc_oned_coda_bar_writer::CbcOnedCodaBarWriter,
        bc_oned_code128_writer::CbcOnedCode128Writer,
        bc_oned_code39_writer::CbcOnedCode39Writer,
        bc_oned_ean13_writer::CbcOnedEan13Writer,
        bc_oned_ean8_writer::CbcOnedEan8Writer,
        bc_oned_ean_checksum::ean_calc_checksum,
        bc_oned_upca_writer::CbcOnedUpcaWriter,
    },
};
use crate::{
    fxcrt::fx_coordinates::CfxMatrix, fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
};
use std::rc::Rc;

enum OneDimWriter {
    Codabar(CbcOnedCodaBarWriter),
    Code128(CbcOnedCode128Writer),
    Code39(CbcOnedCode39Writer),
    Ean8(CbcOnedEan8Writer),
    Ean13(CbcOnedEan13Writer),
    Upca(CbcOnedUpcaWriter),
}
impl OneDimWriter {
    fn base(&self) -> &CbcOneDimWriter {
        match self {
            Self::Codabar(w) => &w.base,
            Self::Code128(w) => &w.base,
            Self::Code39(w) => &w.base,
            Self::Ean8(w) => &w.base,
            Self::Ean13(w) => &w.base,
            Self::Upca(w) => &w.base,
        }
    }
    fn base_mut(&mut self) -> &mut CbcOneDimWriter {
        match self {
            Self::Codabar(w) => &mut w.base,
            Self::Code128(w) => &mut w.base,
            Self::Code39(w) => &mut w.base,
            Self::Ean8(w) => &mut w.base,
            Self::Ean13(w) => &mut w.base,
            Self::Upca(w) => &mut w.base,
        }
    }
}
pub struct CbcOneCode {
    writer: OneDimWriter,
    render_contents: Vec<u32>,
}
impl CbcOneCode {
    pub fn new(kind: BcType) -> Option<Self> {
        let writer = match kind {
            BcType::Codabar => OneDimWriter::Codabar(CbcOnedCodaBarWriter::new()),
            BcType::Code39 => OneDimWriter::Code39(CbcOnedCode39Writer::new()),
            BcType::Code128 | BcType::Code128B => {
                OneDimWriter::Code128(CbcOnedCode128Writer::new(BcType::Code128B))
            }
            BcType::Code128C => OneDimWriter::Code128(CbcOnedCode128Writer::new(BcType::Code128C)),
            BcType::Ean8 => OneDimWriter::Ean8(CbcOnedEan8Writer::new()),
            BcType::Ean13 => OneDimWriter::Ean13(CbcOnedEan13Writer::new()),
            BcType::Upca => OneDimWriter::Upca(CbcOnedUpcaWriter::new()),
            _ => return None,
        };
        Some(Self {
            writer,
            render_contents: Vec::new(),
        })
    }
    pub fn writer(&self) -> &CbcWriter {
        &self.writer.base().writer
    }
    pub fn writer_mut(&mut self) -> &mut CbcWriter {
        &mut self.writer.base_mut().writer
    }
    pub fn get_type(&self) -> BcType {
        match self.writer {
            OneDimWriter::Codabar(_) => BcType::Codabar,
            OneDimWriter::Code128(_) => BcType::Code128,
            OneDimWriter::Code39(_) => BcType::Code39,
            OneDimWriter::Ean8(_) => BcType::Ean8,
            OneDimWriter::Ean13(_) => BcType::Ean13,
            OneDimWriter::Upca(_) => BcType::Upca,
        }
    }
    pub fn set_print_checksum(&mut self, state: bool) {
        self.writer.base_mut().print_checksum = state;
    }
    pub fn set_cal_checksum(&mut self, state: bool) {
        self.writer.base_mut().calc_checksum = state;
    }
    pub fn set_calc_checksum(&mut self, state: bool) {
        self.set_cal_checksum(state);
    }
    pub fn set_data_length(&mut self, length: i32) {
        match &mut self.writer {
            OneDimWriter::Codabar(w) => w.set_data_length(length),
            OneDimWriter::Ean8(w) => w.set_data_length(length),
            w => w.base_mut().data_length = length,
        }
    }
    pub fn set_font(&mut self, font: Rc<dyn BarcodeFont>) -> bool {
        self.writer.base_mut().set_font(font)
    }
    pub fn set_font_size(&mut self, size: f32) {
        self.writer.base_mut().font_size = size;
    }
    pub fn set_font_style(&mut self, style: i32) {
        self.writer.base_mut().font_style = style;
    }
    pub fn set_font_color(&mut self, color: u32) {
        self.writer.base_mut().font_color = color;
    }
    pub fn set_text_location(&mut self, location: BcTextLoc) {
        // An explicit no-text request must suppress the human-readable label
        // even for EAN/UPC writers, whose native placement setters otherwise
        // only accept BelowEmbed. Do not rely on the native painter's unrelated
        // no-space shortcut to hide a label that remains enabled internally.
        if location == BcTextLoc::None {
            self.writer.base_mut().text_location = location;
            return;
        }
        match &mut self.writer {
            OneDimWriter::Ean8(w) => w.set_text_location(location),
            OneDimWriter::Ean13(w) => w.set_text_location(location),
            OneDimWriter::Upca(w) => w.set_text_location(location),
            w => w.base_mut().text_location = location,
        }
    }
    pub fn set_wide_narrow_ratio(&mut self, ratio: i8) -> bool {
        match &mut self.writer {
            OneDimWriter::Codabar(w) => w.set_wide_narrow_ratio(ratio),
            OneDimWriter::Code39(w) => w.set_wide_narrow_ratio(ratio),
            _ => false,
        }
    }
    pub fn set_start_char(&mut self, start: u8) -> bool {
        match &mut self.writer {
            OneDimWriter::Codabar(w) => w.set_start_char(start),
            _ => false,
        }
    }
    pub fn set_end_char(&mut self, end: u8) -> bool {
        match &mut self.writer {
            OneDimWriter::Codabar(w) => w.set_end_char(end),
            _ => false,
        }
    }
    fn preprocess_ean(contents: &[u32], maximum: usize) -> Vec<u32> {
        let mut output: Vec<u32> = contents
            .iter()
            .copied()
            .filter(|&ch| (48..=57).contains(&ch))
            .collect();
        if output.len() <= maximum {
            let missing = maximum - output.len();
            if missing > 0 {
                let mut padded = vec![u32::from(b'0'); missing];
                padded.append(&mut output);
                output = padded;
            }
            let checksum = ean_calc_checksum(&wide_to_utf8(&output));
            output.push(u32::from(b'0') + checksum as u32);
        } else {
            output.truncate(maximum + 1);
        }
        output
    }
    pub fn encode(&mut self, contents: &[u32]) -> bool {
        // A failed new encoding cannot leave the previous barcode available to draw.
        self.writer.base_mut().output.clear();
        self.render_contents.clear();
        let code = match &mut self.writer {
            OneDimWriter::Codabar(w) => {
                if !w.check_content_validity(contents) {
                    return false;
                }
                self.render_contents = w.filter_contents(contents);
                let code = w.encode(&wide_to_utf8(&self.render_contents));
                code
            }
            OneDimWriter::Code39(w) => {
                if !w.check_content_validity(contents) {
                    return false;
                }
                let filtered = w.filter_contents(contents);
                self.render_contents = w.render_text_contents(contents);
                if w.encoded_contents(&self.render_contents).is_none() {
                    return false;
                }
                w.encode(&wide_to_utf8(&filtered))
            }
            OneDimWriter::Code128(w) => {
                if !w.check_content_validity(contents) {
                    return false;
                }
                if contents.len() % 2 != 0 && w.code_format == BcType::Code128C {
                    let mut padded = contents.to_vec();
                    padded.push(u32::from(b'0'));
                    self.render_contents = w.filter_contents(&padded);
                } else {
                    self.render_contents = w.filter_contents(contents);
                }
                w.encode(&wide_to_utf8(&self.render_contents))
            }
            OneDimWriter::Ean8(w) => {
                if !w.check_content_validity(contents) {
                    return false;
                }
                self.render_contents = Self::preprocess_ean(contents, 7);
                w.encode(&wide_to_utf8(&self.render_contents))
            }
            OneDimWriter::Ean13(w) => {
                if !w.check_content_validity(contents) {
                    return false;
                }
                self.render_contents = Self::preprocess_ean(contents, 12);
                w.encode(&wide_to_utf8(&self.render_contents))
            }
            OneDimWriter::Upca(w) => {
                if !w.check_content_validity(contents) {
                    return false;
                }
                self.render_contents = Self::preprocess_ean(contents, 11);
                w.encode(&wide_to_utf8(&self.render_contents))
            }
        };
        self.writer.base_mut().render_result(&code)
    }
    pub fn render_device(
        &self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
    ) -> bool {
        match &self.writer {
            OneDimWriter::Codabar(w) => w.base.render_device_result(
                device,
                matrix,
                &w.encoded_contents(&self.render_contents),
            ),
            OneDimWriter::Code39(w) => w
                .encoded_contents(&self.render_contents)
                .is_some_and(|contents| w.base.render_device_result(device, matrix, &contents)),
            OneDimWriter::Code128(w) => {
                w.base
                    .render_device_result(device, matrix, &self.render_contents)
            }
            OneDimWriter::Ean8(w) => w.render_device_result(device, matrix, &self.render_contents),
            OneDimWriter::Ean13(w) => w.render_device_result(device, matrix, &self.render_contents),
            OneDimWriter::Upca(w) => w.render_device_result(device, matrix, &self.render_contents),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::{
        cfx_font::CfxFont,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    };
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn bitmap(code: &CbcOneCode) -> Vec<u8> {
        let mut target = CfxDibitmap::create(220, 80, FxdibFormat::Bgrx, 0).unwrap();
        assert!(code.render_device(
            &mut CfxAggDeviceDriver::new(&mut target, false, None),
            CfxMatrix::default()
        ));
        target.buffer().to_vec()
    }
    #[test]
    fn numeric_text_uses_selected_font_for_every_linear_format() {
        let font: Rc<dyn BarcodeFont> = Rc::new(
            CfxFont::from_span(
                include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"),
                false,
                0,
            )
            .unwrap(),
        );
        for kind in [
            BcType::Code39,
            BcType::Codabar,
            BcType::Code128B,
            BcType::Code128C,
            BcType::Ean8,
            BcType::Ean13,
            BcType::Upca,
        ] {
            let mut code = CbcOneCode::new(kind).unwrap();
            code.writer_mut().set_width(220);
            code.writer_mut().set_height(80);
            code.set_font_size(12.);
            assert!(code.encode(&wide("123456")));
            let bars = bitmap(&code);
            assert!(code.set_font(font.clone()));
            let text = bitmap(&code);
            // These contents have no spaces: human-readable text used to be
            // skipped by the inverted upstream guard.
            assert_ne!(bars, text, "{kind:?}");
            assert_eq!(
                &bars[..220 * 4 * 60],
                &text[..220 * 4 * 60],
                "bars above text, {kind:?}"
            );
        }
    }
    #[test]
    fn preprocess_checksums_padding_truncation_and_failed_encode() {
        let mut code = CbcOneCode::new(BcType::Ean8).unwrap();
        assert!(code.encode(&wide("123")));
        assert_eq!(code.render_contents, wide("00001236"));
        assert!(code.encode(&wide("1234567890")));
        assert_eq!(code.render_contents, wide("12345678"));
        assert!(!code.encode(&wide("invalid")));
        assert!(code.writer.base().output.is_empty());
        let mut code = CbcOneCode::new(BcType::Code128C).unwrap();
        assert!(code.encode(&wide("123")));
        assert_eq!(code.render_contents, wide("1230"));
        let mut code = CbcOneCode::new(BcType::Code39).unwrap();
        code.set_cal_checksum(true);
        assert!(code.encode(&wide("123")));
        if let OneDimWriter::Code39(writer) = &code.writer {
            assert_eq!(
                writer.encoded_contents(&code.render_contents),
                Some(wide("1236"))
            );
        }
    }
}
