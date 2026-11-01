// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Scaling, quiet areas and drawing from `fxbarcode/BC_TwoDimWriter.{h,cpp}`.
use super::{
    bc_writer::{CbcWriter, BACKGROUND_COLOR, BAR_COLOR},
    common::bc_common_bit_matrix::CbcCommonBitMatrix,
};
use crate::{
    fxcrt::fx_coordinates::CfxMatrix,
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver, cfx_fillrenderoptions::CfxFillRenderOptions,
        cfx_graphstatedata::CfxGraphStateData, cfx_path::CfxPath, cfx_renderdevice::draw_path_agg,
    },
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EncodeResult {
    pub code: Vec<u8>,
    pub width: i32,
    pub height: i32,
}
impl EncodeResult {
    pub fn new(code: Vec<u8>, width: i32, height: i32) -> Self {
        Self {
            code,
            width,
            height,
        }
    }
}

pub struct CbcTwoDimWriter {
    pub writer: CbcWriter,
    output: Option<CbcCommonBitMatrix>,
    multi_x: i32,
    multi_y: i32,
    left_padding: i32,
    top_padding: i32,
    input_width: i32,
    input_height: i32,
    output_width: i32,
    output_height: i32,
    correction_level: i32,
    fixed_size: bool,
}
impl CbcTwoDimWriter {
    pub fn new(fixed_size: bool) -> Self {
        Self {
            writer: CbcWriter::default(),
            output: None,
            multi_x: 0,
            multi_y: 0,
            left_padding: 0,
            top_padding: 0,
            input_width: 0,
            input_height: 0,
            output_width: 0,
            output_height: 0,
            correction_level: 1,
            fixed_size,
        }
    }
    pub fn error_correction_level(&self) -> i32 {
        self.correction_level
    }
    pub fn set_correction_level(&mut self, level: i32) {
        self.correction_level = level;
    }
    pub fn clear_result(&mut self) {
        self.output = None;
    }
    pub fn render_result(&mut self, result: &EncodeResult) -> bool {
        self.output = None;
        if result.width <= 0
            || result.height <= 0
            || self.writer.width <= 0
            || self.writer.height <= 0
        {
            return false;
        }
        let (input_width, input_height) = (result.width as usize, result.height as usize);
        if input_width.checked_mul(input_height) != Some(result.code.len()) {
            return false;
        }
        let Some(temp_width) = result.width.checked_add(2) else {
            return false;
        };
        let Some(temp_height) = result.height.checked_add(2) else {
            return false;
        };
        let module_size = self
            .writer
            .module_width
            .min(self.writer.module_height)
            .clamp(1, 8);
        let Some(mut output_width) = temp_width.checked_mul(module_size) else {
            return false;
        };
        let Some(mut output_height) = temp_height.checked_mul(module_size) else {
            return false;
        };
        if self.fixed_size {
            if self.writer.width < output_width || self.writer.height < output_height {
                return false;
            }
        } else if self.writer.width > output_width || self.writer.height > output_height {
            let width_factor =
                ((self.writer.width as f32 / output_width as f32).floor() as i32).max(1);
            let height_factor =
                ((self.writer.height as f32 / output_height as f32).floor() as i32).max(1);
            let Some(width) = output_width.checked_mul(width_factor) else {
                return false;
            };
            let Some(height) = output_height.checked_mul(height_factor) else {
                return false;
            };
            output_width = width;
            output_height = height;
        }
        let mut multi_x = (output_width as f32 / temp_width as f32).ceil() as i32;
        let mut multi_y = (output_height as f32 / temp_height as f32).ceil() as i32;
        if self.fixed_size {
            multi_x = multi_x.min(multi_y);
            multi_y = multi_x;
        }
        let Some(mut output) = CbcCommonBitMatrix::new(input_width, input_height) else {
            return false;
        };
        for y in 0..input_height {
            for x in 0..input_width {
                if result.code[x + y * input_width] == 1 {
                    output.set(x, y);
                }
            }
        }
        self.input_width = result.width;
        self.input_height = result.height;
        self.output_width = output_width;
        self.output_height = output_height;
        self.multi_x = multi_x;
        self.multi_y = multi_y;
        self.left_padding = ((self.writer.width - output_width) / 2).max(0);
        self.top_padding = ((self.writer.height - output_height) / 2).max(0);
        self.output = Some(output);
        true
    }
    pub fn render_device_result(
        &self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
    ) -> bool {
        let Some(output) = self.output.as_ref() else {
            return false;
        };
        let state = CfxGraphStateData::default();
        let mut path = CfxPath::new();
        path.append_rect(0., 0., self.writer.width as f32, self.writer.height as f32);
        if !draw_path_agg(
            device,
            &path,
            Some(matrix),
            Some(&state),
            BACKGROUND_COLOR,
            BACKGROUND_COLOR,
            CfxFillRenderOptions::even_odd_options(),
        ) {
            return false;
        }
        let mut matri = matrix;
        if self.writer.width < self.output_width && self.writer.height < self.output_height {
            matri = CfxMatrix::new(
                self.writer.width as f32 / self.output_width as f32,
                0.,
                0.,
                self.writer.height as f32 / self.output_height as f32,
                0.,
                0.,
            );
            matri.concat(matrix);
        }
        for x in 0..self.input_width {
            for y in 0..self.input_height {
                if output.get(x as usize, y as usize) {
                    // Source adds one module on each side for the quiet area.
                    let mut rect = CfxPath::new();
                    rect.append_rect(
                        (self.left_padding + (x + 1) * self.multi_x) as f32,
                        (self.top_padding + (y + 1) * self.multi_y) as f32,
                        (self.left_padding + (x + 2) * self.multi_x) as f32,
                        (self.top_padding + (y + 2) * self.multi_y) as f32,
                    );
                    if !draw_path_agg(
                        device,
                        &rect,
                        Some(matri),
                        Some(&state),
                        BAR_COLOR,
                        0,
                        CfxFillRenderOptions::winding_options(),
                    ) {
                        return false;
                    }
                }
            }
        }
        true
    }
}
