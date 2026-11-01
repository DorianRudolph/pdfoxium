// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/theme/cfwl_pictureboxtp.{h,cpp}` drawer.
use super::cfwl_widgettp::CfwlWidgetTp;
use crate::xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics;
use crate::xfa::fwl::{cfwl_themebackground::CfwlThemeBackground, cfwl_themepart::Part};

pub fn draw_background(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    if params.part == Part::Border {
        CfwlWidgetTp::draw_border(graphics, params.part_rect, params.matrix);
    }
}
