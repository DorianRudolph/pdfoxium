// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpdf_color_utils.{h,cpp}`.

use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fxge::cfx_color::CfxColor;

pub fn cfx_color_from_array(array: &CpdfArray) -> CfxColor {
    match array.size() {
        1 => CfxColor::make_gray(array.get_float_at(0)),
        3 => CfxColor::make_rgb(
            array.get_float_at(0),
            array.get_float_at(1),
            array.get_float_at(2),
        ),
        4 => CfxColor::make_cmyk(
            array.get_float_at(0),
            array.get_float_at(1),
            array.get_float_at(2),
            array.get_float_at(3),
        ),
        _ => CfxColor::default(),
    }
}
