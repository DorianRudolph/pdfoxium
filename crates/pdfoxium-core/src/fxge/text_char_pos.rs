// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxge/text_char_pos.{h,cpp}`.

use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};

#[derive(Clone, Copy, Debug, Default)]
pub struct TextCharPos {
    pub origin: CfxPointF,
    pub unicode: u32,
    pub glyph_index: u32,
    pub font_char_width: i32,
    pub fallback_font_position: i32,
    pub glyph_adjust: bool,
    pub is_cid_font: bool,
    pub adjust_matrix: [f32; 4],
}

impl TextCharPos {
    pub fn get_effective_matrix(&self, matrix: CfxMatrix) -> CfxMatrix {
        let mut result = if self.glyph_adjust {
            CfxMatrix::new(
                self.adjust_matrix[0],
                self.adjust_matrix[1],
                self.adjust_matrix[2],
                self.adjust_matrix[3],
                0.0,
                0.0,
            )
        } else {
            CfxMatrix::default()
        };
        result.concat(matrix);
        result
    }
}
