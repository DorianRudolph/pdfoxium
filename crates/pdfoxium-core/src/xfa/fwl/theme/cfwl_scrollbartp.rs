// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/theme/cfwl_scrollbartp.{h,cpp}` drawer.
use super::cfwl_widgettp::CfwlWidgetTp;
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::xfa::fgas::graphics::{
    cfgas_gecolor::CfgasGeColor, cfgas_gegraphics::CfgasGeGraphics, cfgas_gepath::CfgasGePath,
};
use crate::xfa::fwl::{cfwl_themebackground::CfwlThemeBackground, cfwl_themepart::Part};

use super::cfwl_utils::{ThemeDirection, ThemeState};
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxRectF};

pub const TRACK_BACKGROUND_START: u32 = 0xfff3f1ec;
pub const TRACK_BACKGROUND_END: u32 = 0xfffefefb;
pub const BUTTON_BACKGROUND: [u32; 4] = [0xffb6cdfb, 0xffcce1ff, 0xff92b3f9, 0xff8d9d73];
pub const BUTTON_BORDER: [u32; 4] = [0xff94b0dd, 0xffdae6fe, 0xff7c9fd3, 0xffece9d8];

pub fn draw_background(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    let vertical = params.widget.base().get_style_exts() != 0;
    let state = params.get_theme_state();
    match params.part {
        Part::ForeArrow => draw_max_min_btn(
            graphics,
            params.part_rect,
            if vertical {
                ThemeDirection::Up
            } else {
                ThemeDirection::Left
            },
            state,
            params.matrix,
        ),
        Part::BackArrow => draw_max_min_btn(
            graphics,
            params.part_rect,
            if vertical {
                ThemeDirection::Down
            } else {
                ThemeDirection::Right
            },
            state,
            params.matrix,
        ),
        Part::Thumb => draw_thumb_btn(graphics, params.part_rect, vertical, state, params.matrix),
        Part::LowerTrack => draw_track(
            graphics,
            params.part_rect,
            vertical,
            state,
            true,
            params.matrix,
        ),
        Part::UpperTrack => draw_track(
            graphics,
            params.part_rect,
            vertical,
            state,
            false,
            params.matrix,
        ),
        _ => {}
    }
}

fn draw_thumb_btn(
    graphics: &mut CfgasGeGraphics,
    mut rect: CfxRectF,
    vertical: bool,
    state: ThemeState,
    matrix: CfxMatrix,
) {
    if vertical {
        rect.deflate_xy(1., 0.);
    } else {
        rect.deflate_xy(0., 1.);
    }
    if rect.width <= 0.1 || rect.height <= 0.1 {
        return;
    }
    let index = state as usize - 1;
    CfwlWidgetTp::fill_solid_rect(graphics, BUTTON_BACKGROUND[index], rect, matrix);
    let mut state = graphics.state_restorer();
    let mut path = CfgasGePath::new();
    path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
    state.set_stroke_color(&CfgasGeColor::Solid(BUTTON_BORDER[index]));
    state.stroke_path(&path, matrix);
}
fn draw_track(
    graphics: &mut CfgasGeGraphics,
    rect: CfxRectF,
    vertical: bool,
    _state: ThemeState,
    _lower: bool,
    matrix: CfxMatrix,
) {
    {
        let mut state = graphics.state_restorer();
        let mut path = CfgasGePath::new();
        if vertical {
            path.add_rectangle(rect.left, rect.top, 1., rect.height);
            path.add_rectangle(rect.right() - 1., rect.top, 1., rect.height);
        } else {
            path.add_rectangle(rect.left, rect.top, rect.width, 1.);
            path.add_rectangle(rect.left, rect.bottom() - 1., rect.width, 1.);
        }
        state.set_fill_color(&CfgasGeColor::Solid(0xffeeede5));
        state.fill_path(&path, FillType::Winding, matrix);
        path.clear();
        path.add_rectangle(rect.left + 1., rect.top, rect.width - 2., rect.height);
    }
    CfwlWidgetTp::fill_solid_rect(graphics, TRACK_BACKGROUND_END, rect, matrix);
}
fn draw_max_min_btn(
    graphics: &mut CfgasGeGraphics,
    mut rect: CfxRectF,
    direction: ThemeDirection,
    state: ThemeState,
    matrix: CfxMatrix,
) {
    draw_track(
        graphics,
        rect,
        matches!(direction, ThemeDirection::Up | ThemeDirection::Down),
        state,
        true,
        matrix,
    );
    rect.deflate(1., 1., 1., 1.);
    CfwlWidgetTp::draw_arrow_btn(graphics, rect, direction, state, matrix);
}
