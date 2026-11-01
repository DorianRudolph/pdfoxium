// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! AGG bitmap device buffer from `core/fpdfapi/render/cpdf_devicebuffer.{h,cpp}`.
//! The device is borrowed only for output; Windows DPI/printer paths are excluded.

use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, FxRect};
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::FxdibFormat;

pub struct CpdfDeviceBuffer {
    rect: FxRect,
    matrix: CfxMatrix,
    bitmap: Option<CfxDibitmap>,
}

impl CpdfDeviceBuffer {
    pub fn calculate_matrix(rect: FxRect, _max_dpi: i32) -> CfxMatrix {
        let mut matrix = CfxMatrix::default();
        matrix.translate(-(rect.left as f32), -(rect.top as f32));
        matrix
    }
    pub fn new(rect: FxRect, max_dpi: i32) -> Self {
        Self {
            rect,
            matrix: Self::calculate_matrix(rect, max_dpi),
            bitmap: None,
        }
    }
    pub fn matrix(&self) -> CfxMatrix {
        self.matrix
    }
    pub fn initialize(&mut self) -> Option<&mut CfxDibitmap> {
        let rect = self
            .matrix
            .transform_float_rect(CfxFloatRect::from_fx_rect(self.rect))
            .get_outer_rect();
        self.bitmap = CfxDibitmap::create(rect.width(), rect.height(), FxdibFormat::Bgra, 0);
        self.bitmap.as_mut()
    }
    pub fn output_to_device(&self, driver: &mut CfxAggDeviceDriver<'_, '_>) {
        // These are invariant for the selected AGG path. Other output devices
        // and DPI-scaled matrices belong to the excluded Windows branch.
        debug_assert!(driver.render_cap_get_bits());
        debug_assert!(self.matrix.a == 1.0 && self.matrix.d == 1.0);
        if let Some(bitmap) = &self.bitmap {
            driver.set_dibits_normal(
                bitmap,
                0,
                FxRect::new(0, 0, bitmap.width(), bitmap.height()),
                self.rect.left,
                self.rect.top,
            );
        }
    }
}
