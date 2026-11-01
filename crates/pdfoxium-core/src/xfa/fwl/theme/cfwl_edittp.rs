// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/theme/cfwl_edittp.{h,cpp}` drawer.
use super::cfwl_utils::{
    FWL_THEME_COLOR_BACKGROUND, FWL_THEME_COLOR_BACKGROUND_SELECTED, FWL_THEME_COLOR_EDGE_RB1,
};
use super::cfwl_widgettp::CfwlWidgetTp;
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::xfa::fgas::graphics::{
    cfgas_gecolor::CfgasGeColor, cfgas_gegraphics::CfgasGeGraphics, cfgas_gepath::CfgasGePath,
};
use crate::xfa::fwl::{
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
};

pub fn draw_background(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    match params.part {
        Part::Border => CfwlWidgetTp::draw_border(graphics, params.part_rect, params.matrix),
        Part::Background => {
            let mut state = graphics.state_restorer();
            if let Some(path) = params.path {
                state.set_fill_color(&CfgasGeColor::Solid(FWL_THEME_COLOR_BACKGROUND_SELECTED));
                state.fill_path(path, FillType::Winding, params.matrix);
            } else {
                let rect = params.part_rect;
                let mut path = CfgasGePath::new();
                path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
                let color = if params.static_background {
                    FWL_THEME_COLOR_BACKGROUND
                } else if params.states.contains(PartState::DISABLED) {
                    FWL_THEME_COLOR_EDGE_RB1
                } else if params.states.contains(PartState::READ_ONLY) {
                    0xffece9d8
                } else {
                    0xffffffff
                };
                state.set_fill_color(&CfgasGeColor::Solid(color));
                state.fill_path(&path, FillType::Winding, params.matrix);
            }
        }
        Part::CombTextLine => {
            let Some((color, width)) = params.widget.base().border_color_and_thickness() else {
                return;
            };
            graphics.set_stroke_color(&CfgasGeColor::Solid(color));
            graphics.set_line_width(width);
            if let Some(path) = params.path {
                graphics.stroke_path(path, params.matrix);
            }
        }
        _ => {}
    }
}
