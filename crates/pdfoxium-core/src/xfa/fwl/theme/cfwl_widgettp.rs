// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/theme/cfwl_widgettp.{h,cpp}`. Theme text outputs retain their
//! buffers between draws, but a local owner replaces the cached slot before
//! host callbacks, so reentry never observes an outstanding mutable borrow.
use super::cfwl_utils::*;
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    fxge::cfx_fillrenderoptions::FillType,
    xfa::{
        fde::cfde_textout::CfdeTextOut,
        fgas::graphics::{
            cfgas_gecolor::CfgasGeColor, cfgas_gegraphics::CfgasGeGraphics,
            cfgas_gepath::CfgasGePath,
        },
        fwl::{cfwl_themetext::CfwlThemeText, ifwl_themeprovider::ThemeProvider},
    },
};
use std::cell::RefCell;

// Every source color entry is retained, including the currently unused start
// colors, so state indexing and future source consumers remain traceable.
const ARROW_BORDER: [u32; 4] = [0xffcad8f9, 0xffabbee9, 0xff8793db, 0xffaca899];
const ARROW_START: [u32; 4] = [0xffe1eafe, 0xfffdffff, 0xff6e8ef1, 0xfffefefb];
const ARROW_END: [u32; 4] = [0xffafccfb, 0xffb9dafb, 0xffd2deeb, 0xfff3f1ec];
const ARROW_SIGN: [u32; 4] = [0xff4d6185, 0xff4d6185, 0xff4d6185, 0xff808080];

#[derive(Default)]
pub struct CfwlWidgetTp {
    text_out: RefCell<Option<CfdeTextOut>>,
    measure_out: RefCell<Option<CfdeTextOut>>,
}
impl CfwlWidgetTp {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn draw_text(
        &self,
        provider: &dyn ThemeProvider,
        params: &CfwlThemeText,
        graphics: &mut CfgasGeGraphics,
    ) {
        self.draw_text_with_color(provider, params, graphics, FWL_THEME_TEXT_COLOR);
    }
    pub fn draw_text_with_color(
        &self,
        provider: &dyn ThemeProvider,
        params: &CfwlThemeText,
        graphics: &mut CfgasGeGraphics,
        color: u32,
    ) {
        let cached = self.text_out.borrow_mut().take();
        let mut output = cached.unwrap_or_else(|| {
            let font = provider.get_fwl_font();
            let mut output = CfdeTextOut::new();
            output.set_font(font);
            output.set_font_size(FWL_THEME_FONT_SIZE);
            output.set_text_color(FWL_THEME_TEXT_COLOR);
            output
        });
        if !params.text.is_empty() {
            output.set_text_color(color);
            output.set_styles(params.tto_styles);
            output.set_alignment(params.tto_align);
            let mut matrix = params.matrix;
            matrix.concat(graphics.get_matrix());
            output.set_matrix(matrix);
            output.draw_logic_text(
                Some(graphics.get_render_device()),
                &params.text,
                params.part_rect,
            );
        }
        *self.text_out.borrow_mut() = Some(output);
    }
    /// Convenience for providers' required measurement method. Host metrics
    /// remain explicit; this helper supplies no substitute font or dimensions.
    pub fn calc_text_rect(
        &self,
        provider: &dyn ThemeProvider,
        params: &CfwlThemeText,
        rect: &mut CfxRectF,
    ) {
        let font = provider.get_font(&params.base);
        let size = provider.get_font_size(&params.base);
        let line_height = provider.get_line_height(&params.base);
        let cached = self.measure_out.borrow_mut().take();
        let mut output = cached.unwrap_or_default();
        output.set_font(font);
        output.set_font_size(size);
        output.set_line_space(line_height);
        output.set_styles(params.tto_styles);
        output.set_alignment(params.tto_align);
        output.calc_logic_rect(&params.text, rect);
        *self.measure_out.borrow_mut() = Some(output);
    }
    pub fn draw_border(graphics: &mut CfgasGeGraphics, rect: CfxRectF, matrix: CfxMatrix) {
        let mut path = CfgasGePath::new();
        path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
        path.add_rectangle(
            rect.left + 1.,
            rect.top + 1.,
            rect.width - 2.,
            rect.height - 2.,
        );
        let mut state = graphics.state_restorer();
        state.set_fill_color(&CfgasGeColor::Solid(0xff000000));
        state.fill_path(&path, FillType::EvenOdd, matrix);
    }
    pub fn fill_background(graphics: &mut CfgasGeGraphics, rect: CfxRectF, matrix: CfxMatrix) {
        Self::fill_solid_rect(graphics, FWL_THEME_COLOR_BACKGROUND, rect, matrix);
    }
    pub fn fill_solid_rect(
        graphics: &mut CfgasGeGraphics,
        color: u32,
        rect: CfxRectF,
        matrix: CfxMatrix,
    ) {
        let mut path = CfgasGePath::new();
        path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
        let mut state = graphics.state_restorer();
        state.set_fill_color(&CfgasGeColor::Solid(color));
        state.fill_path(&path, FillType::Winding, matrix);
    }
    pub fn draw_focus(graphics: &mut CfgasGeGraphics, rect: CfxRectF, matrix: CfxMatrix) {
        let mut path = CfgasGePath::new();
        path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
        let mut state = graphics.state_restorer();
        state.set_stroke_color(&CfgasGeColor::Solid(0xff000000));
        state.set_line_dash(vec![1., 1.]);
        state.stroke_path(&path, matrix);
    }
    pub fn draw_arrow(
        graphics: &mut CfgasGeGraphics,
        rect: CfxRectF,
        direction: ThemeDirection,
        color: u32,
        matrix: CfxMatrix,
    ) {
        let vertical = matches!(direction, ThemeDirection::Up | ThemeDirection::Down);
        let left = ((rect.width - if vertical { 9. } else { 6. }) / 2. + rect.left) + 0.5;
        let top = ((rect.height - if vertical { 6. } else { 9. }) / 2. + rect.top) + 0.5;
        let points = match direction {
            ThemeDirection::Down => [(0., 1.), (4., 5.), (8., 1.), (7., 0.), (4., 3.), (1., 0.)],
            ThemeDirection::Up => [(0., 4.), (4., 0.), (8., 4.), (7., 5.), (4., 2.), (1., 5.)],
            ThemeDirection::Right => [(1., 0.), (5., 4.), (1., 8.), (0., 7.), (3., 4.), (0., 1.)],
            ThemeDirection::Left => [(0., 4.), (4., 0.), (5., 1.), (2., 4.), (5., 7.), (4., 8.)],
        };
        let mut path = CfgasGePath::new();
        for (index, (x, y)) in points.into_iter().enumerate() {
            let point = CfxPointF::new(left + x, top + y);
            if index == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        graphics.set_fill_color(&CfgasGeColor::Solid(color));
        graphics.fill_path(&path, FillType::Winding, matrix);
    }
    pub fn draw_btn(
        graphics: &mut CfgasGeGraphics,
        rect: CfxRectF,
        state: ThemeState,
        matrix: CfxMatrix,
    ) {
        let index = state as usize - 1;
        // Source's start palette is preserved even though the selected AGG
        // drawing implementation fills each button with its end color.
        let _start = ARROW_START[index];
        Self::fill_solid_rect(graphics, ARROW_END[index], rect, matrix);
        let mut path = CfgasGePath::new();
        path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
        graphics.set_stroke_color(&CfgasGeColor::Solid(ARROW_BORDER[index]));
        graphics.stroke_path(&path, matrix);
    }
    pub fn draw_arrow_btn(
        graphics: &mut CfgasGeGraphics,
        rect: CfxRectF,
        direction: ThemeDirection,
        state: ThemeState,
        matrix: CfxMatrix,
    ) {
        Self::draw_btn(graphics, rect, state, matrix);
        Self::draw_arrow(
            graphics,
            rect,
            direction,
            ARROW_SIGN[state as usize - 1],
            matrix,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    };
    fn bitmap() -> CfxDibitmap {
        let mut b = CfxDibitmap::create(40, 40, FxdibFormat::Bgra, 0).unwrap();
        b.clear(0xffffffff);
        b
    }
    fn pixel(b: &CfxDibitmap, x: usize, y: i32) -> u32 {
        u32::from_le_bytes(b.scanline(y).unwrap()[x * 4..x * 4 + 4].try_into().unwrap())
    }
    #[test]
    fn border_background_and_focus_draw_actual_geometry() {
        let mut b = bitmap();
        {
            let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
            let mut g = CfgasGeGraphics::new(&mut device);
            let saved = g.get_clip_rect();
            CfwlWidgetTp::fill_background(
                &mut g,
                CfxRectF::new(2., 2., 20., 20.),
                CfxMatrix::default(),
            );
            CfwlWidgetTp::draw_border(
                &mut g,
                CfxRectF::new(2., 2., 20., 20.),
                CfxMatrix::default(),
            );
            CfwlWidgetTp::draw_focus(
                &mut g,
                CfxRectF::new(26., 4., 10., 20.),
                CfxMatrix::default(),
            );
            assert_eq!(g.get_clip_rect(), saved);
        }
        assert_eq!(pixel(&b, 1, 1), 0xffffffff);
        assert_eq!(pixel(&b, 2, 2), 0xff000000);
        assert_eq!(pixel(&b, 12, 12), FWL_THEME_COLOR_BACKGROUND);
        assert!((2..26).any(|y| pixel(&b, 26, y) != 0xffffffff));
    }
    #[test]
    fn arrow_buttons_retain_all_four_source_palettes_and_directions() {
        for state in [
            ThemeState::Normal,
            ThemeState::Hover,
            ThemeState::Pressed,
            ThemeState::Disable,
        ] {
            for direction in [
                ThemeDirection::Up,
                ThemeDirection::Down,
                ThemeDirection::Left,
                ThemeDirection::Right,
            ] {
                let mut b = bitmap();
                {
                    let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
                    let mut g = CfgasGeGraphics::new(&mut device);
                    CfwlWidgetTp::draw_arrow_btn(
                        &mut g,
                        CfxRectF::new(4., 4., 24., 24.),
                        direction,
                        state,
                        CfxMatrix::default(),
                    );
                }
                assert_eq!(pixel(&b, 7, 7), ARROW_END[state as usize - 1]);
                assert_eq!(pixel(&b, 0, 0), 0xffffffff);
                let sign = ARROW_SIGN[state as usize - 1];
                let background = ARROW_END[state as usize - 1];
                // These narrow diagonal polygons can cover every device pixel
                // only partially. Validate actual arrow ink and composited
                // source channels rather than requiring an opaque sign pixel.
                let mut ink = 0;
                for y in 8..25 {
                    for x in 8..25 {
                        let actual = pixel(&b, x, y);
                        ink += usize::from(actual != background);
                        for shift in [0, 8, 16, 24] {
                            let (a, b) = ((sign >> shift) & 255, (background >> shift) & 255);
                            let channel = (actual >> shift) & 255;
                            assert!((a.min(b)..=a.max(b)).contains(&channel));
                        }
                    }
                }
                assert!(ink > 0, "{direction:?} {state:?}");
            }
        }
    }
}
