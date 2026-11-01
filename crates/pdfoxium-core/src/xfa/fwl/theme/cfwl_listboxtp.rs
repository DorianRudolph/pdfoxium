// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/theme/cfwl_listboxtp.{h,cpp}` drawer.
use super::{
    cfwl_utils::{FWL_THEME_COLOR_BACKGROUND, FWL_THEME_COLOR_BACKGROUND_SELECTED},
    cfwl_widgettp::CfwlWidgetTp,
};
use crate::xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics;
use crate::xfa::fwl::{
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
};

pub fn draw_background(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    match params.part {
        Part::Border => CfwlWidgetTp::draw_border(graphics, params.part_rect, params.matrix),
        Part::Background => {
            CfwlWidgetTp::fill_solid_rect(graphics, 0xffffffff, params.part_rect, params.matrix);
            if let Some(rect) = params.data_rect {
                CfwlWidgetTp::fill_solid_rect(
                    graphics,
                    FWL_THEME_COLOR_BACKGROUND,
                    rect,
                    params.matrix,
                );
            }
        }
        Part::ListItem => {
            if params.states.contains(PartState::SELECTED) {
                let mut rect = params.part_rect;
                if cfg!(target_vendor = "apple") {
                    rect.width -= 1.;
                    rect.height -= 1.;
                }
                CfwlWidgetTp::fill_solid_rect(
                    graphics,
                    FWL_THEME_COLOR_BACKGROUND_SELECTED,
                    rect,
                    params.matrix,
                );
            }
            if params.states.contains(PartState::FOCUSED) {
                if let Some(rect) = params.data_rect {
                    CfwlWidgetTp::draw_focus(graphics, rect, params.matrix);
                }
            }
        }
        Part::Check => {
            let color = if params.states == PartState::CHECKED {
                0xffff0000
            } else if params.states == PartState::NORMAL {
                0xff0000ff
            } else {
                0xff000000
            };
            CfwlWidgetTp::fill_solid_rect(graphics, color, params.part_rect, params.matrix);
        }
        _ => {}
    }
}
