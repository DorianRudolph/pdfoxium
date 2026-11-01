// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpvt_line.h`.

use super::cpvt_wordplace::CpvtWordPlace;
use crate::fxcrt::fx_coordinates::CfxPointF;

#[derive(Clone, Copy, Debug, Default)]
pub struct CpvtLine {
    pub line_place: CpvtWordPlace,
    pub line_end: CpvtWordPlace,
    pub point: CfxPointF,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
}
