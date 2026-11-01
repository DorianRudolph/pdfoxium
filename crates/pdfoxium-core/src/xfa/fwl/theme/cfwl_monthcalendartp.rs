// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/theme/cfwl_monthcalendartp.{h,cpp}` drawer.
use super::cfwl_widgettp::CfwlWidgetTp;
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::xfa::fgas::graphics::{
    cfgas_gecolor::CfgasGeColor, cfgas_gegraphics::CfgasGeGraphics, cfgas_gepath::CfgasGePath,
};
use crate::xfa::fwl::{
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
};

use super::cfwl_utils::ThemeDirection;
use crate::{fxcrt::fx_coordinates::CfxPointF, xfa::fwl::cfwl_themetext::CfwlThemeText};

pub const CAPTION_COLOR: u32 = 0xff0099ff;
pub const SEPARATOR_COLOR: u32 = 0xff8da1ef;
pub const DATES_HOVER_BACKGROUND_COLOR: u32 = 0xffc1d3fb;
pub const DATES_SELECTED_BACKGROUND_COLOR: u32 = 0xffadbcef;
pub const DATES_CIRCLE_COLOR: u32 = 0xff6790d1;
pub const BACKGROUND_COLOR: u32 = 0xffffffff;

pub fn text_color(params: &CfwlThemeText<'_>) -> u32 {
    if params.part == Part::DatesIn
        && !params.states.contains(PartState::FLAGGED)
        && params
            .states
            .intersects(PartState::HOVERED | PartState::SELECTED)
    {
        0xffffffff
    } else if params.part == Part::Caption {
        CAPTION_COLOR
    } else {
        0xff000000
    }
}

pub fn draw_background(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    match params.part {
        Part::Border => CfwlWidgetTp::draw_border(graphics, params.part_rect, params.matrix),
        Part::Background | Part::Header => CfwlWidgetTp::fill_solid_rect(
            graphics,
            BACKGROUND_COLOR,
            params.part_rect,
            params.matrix,
        ),
        Part::LBtn => CfwlWidgetTp::draw_arrow_btn(
            graphics,
            params.part_rect,
            ThemeDirection::Left,
            params.get_theme_state(),
            params.matrix,
        ),
        Part::RBtn => CfwlWidgetTp::draw_arrow_btn(
            graphics,
            params.part_rect,
            ThemeDirection::Right,
            params.get_theme_state(),
            params.matrix,
        ),
        Part::DatesIn => {
            if params.states.contains(PartState::SELECTED) {
                CfwlWidgetTp::fill_solid_rect(
                    graphics,
                    DATES_SELECTED_BACKGROUND_COLOR,
                    params.part_rect,
                    params.matrix,
                );
            } else if params.states.contains(PartState::HOVERED) {
                CfwlWidgetTp::fill_solid_rect(
                    graphics,
                    DATES_HOVER_BACKGROUND_COLOR,
                    params.part_rect,
                    params.matrix,
                );
            }
        }
        Part::TodayCircle | Part::DateInCircle => {
            let rect = params.part_rect;
            let mut path = CfgasGePath::new();
            path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
            let mut state = graphics.state_restorer();
            state.set_stroke_color(&CfgasGeColor::Solid(DATES_CIRCLE_COLOR));
            state.stroke_path(&path, params.matrix);
        }
        Part::HSeparator | Part::WeekNumSep => {
            let rect = params.part_rect;
            let mut path = CfgasGePath::new();
            if params.part == Part::HSeparator {
                path.move_to(CfxPointF::new(rect.left, rect.top + rect.height / 2.));
                path.line_to(CfxPointF::new(rect.right(), rect.top + rect.height / 2.));
            } else {
                path.move_to(rect.top_left());
                path.line_to(CfxPointF::new(rect.left, rect.bottom()));
            }
            let mut state = graphics.state_restorer();
            state.set_stroke_color(&CfgasGeColor::Solid(SEPARATOR_COLOR));
            state.stroke_path(&path, params.matrix);
        }
        _ => {}
    }
}

// These source helper variants are retained separately. The current native
// switch selects themed arrow buttons instead; either geometry remains usable.
pub fn draw_l_button(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    draw_legacy_button(params, graphics, false);
}
pub fn draw_r_button(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    draw_legacy_button(params, graphics, true);
}
fn draw_legacy_button(
    params: &CfwlThemeBackground<'_>,
    graphics: &mut CfgasGeGraphics,
    right: bool,
) {
    let rect = params.part_rect;
    let mut path = CfgasGePath::new();
    path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
    let mut state = graphics.state_restorer();
    state.set_stroke_color(&CfgasGeColor::Solid(0xffcddbf3));
    state.stroke_path(&path, params.matrix);
    state.set_fill_color(&CfgasGeColor::Solid(
        if params.states.contains(PartState::PRESSED) {
            0xffaec6f2
        } else {
            0xffe3ebf9
        },
    ));
    state.fill_path(&path, FillType::Winding, params.matrix);
    path.clear();
    let outer = if right {
        rect.left + rect.width / 3.
    } else {
        rect.left + rect.width / 3. * 2.
    };
    let inner = if right {
        rect.left + rect.width / 3. * 2.
    } else {
        rect.left + rect.width / 3.
    };
    path.move_to(CfxPointF::new(outer, rect.top + rect.height / 4.));
    path.line_to(CfxPointF::new(inner, rect.top + rect.height / 2.));
    path.line_to(CfxPointF::new(outer, rect.bottom() - rect.height / 4.));
    state.set_stroke_color(&CfgasGeColor::Solid(0xff3268cd));
    state.stroke_path(&path, params.matrix);
}

#[cfg(test)]
#[path = "theme_subclass_tests.rs"]
mod tests;
