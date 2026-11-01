// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/theme/cfwl_carettp.{h,cpp}` drawer.
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::xfa::fgas::graphics::{
    cfgas_gecolor::CfgasGeColor, cfgas_gegraphics::CfgasGeGraphics, cfgas_gepath::CfgasGePath,
};
use crate::xfa::fwl::{
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
};

pub fn draw_background(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    if params.part == Part::Background && params.states.contains(PartState::HIGHLIGHT) {
        let rect = params.part_rect;
        let mut path = CfgasGePath::new();
        path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
        graphics.set_fill_color(&CfgasGeColor::Solid(0xff000000));
        graphics.fill_path(&path, FillType::Winding, params.matrix);
    }
}
