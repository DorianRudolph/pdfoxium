// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxge/cfx_fillrenderoptions.h`.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum FillType {
    #[default]
    NoFill = 0,
    EvenOdd = 1,
    Winding = 2,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CfxFillRenderOptions {
    pub fill_type: FillType,
    pub adjust_stroke: bool,
    pub aliased_path: bool,
    pub full_cover: bool,
    pub rect_aa: bool,
    pub stroke: bool,
    pub stroke_text_mode: bool,
    pub text_mode: bool,
    pub zero_area: bool,
}

impl CfxFillRenderOptions {
    pub const fn new(fill_type: FillType) -> Self {
        Self {
            fill_type,
            adjust_stroke: false,
            aliased_path: false,
            full_cover: false,
            rect_aa: false,
            stroke: false,
            stroke_text_mode: false,
            text_mode: false,
            zero_area: false,
        }
    }

    pub const fn even_odd_options() -> Self {
        Self::new(FillType::EvenOdd)
    }

    pub const fn winding_options() -> Self {
        Self::new(FillType::Winding)
    }
}
