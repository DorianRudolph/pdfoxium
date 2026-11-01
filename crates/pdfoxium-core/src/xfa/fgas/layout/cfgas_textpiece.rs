// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Source CFGAS_TextPiece owns its text/widths and shares its selected font.
use crate::fxcrt::fx_coordinates::CfxRectF;
use crate::xfa::fgas::font::cfgas_gefont::CfgasGeFont;
use std::rc::Rc;
#[derive(Clone, Default)]
pub struct CfgasTextPiece {
    pub text: Vec<u32>,
    pub widths: Vec<i32>,
    pub chars: i32,
    pub horizontal_scale: i32,
    pub vertical_scale: i32,
    pub bidi_level: i32,
    pub font_size: f32,
    pub rect: CfxRectF,
    pub font: Option<Rc<CfgasGeFont>>,
}
