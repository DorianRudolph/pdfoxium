// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/theme/cfwl_checkboxtp.{h,cpp}` drawer.
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::xfa::fgas::graphics::{
    cfgas_gecolor::CfgasGeColor, cfgas_gegraphics::CfgasGeGraphics, cfgas_gepath::CfgasGePath,
};
use crate::xfa::fwl::{
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
};

use super::cfwl_utils::{FWL_THEME_TEXT_COLOR, FWL_THEME_TEXT_DISABLED_COLOR};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    xfa::fwl::cfwl_themetext::CfwlThemeText,
};

pub fn text_color(params: &CfwlThemeText<'_>) -> u32 {
    if params.states.contains(PartState::DISABLED) {
        FWL_THEME_TEXT_DISABLED_COLOR
    } else {
        FWL_THEME_TEXT_COLOR
    }
}

pub fn draw_background(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    if params.part != Part::CheckBox
        || !params
            .states
            .intersects(PartState::CHECKED | PartState::NEUTRAL)
    {
        return;
    }
    let mut sign = params.part_rect;
    sign.deflate_xy(sign.width / 4., sign.height / 4.);
    let color = if params.states.contains(PartState::NEUTRAL) {
        0xffa9a9a9
    } else {
        0xff000000
    };
    draw_check_sign(
        graphics,
        sign,
        color,
        (params.widget.base().get_style_exts() >> 10) & 7,
        params.matrix,
    );
}

pub fn draw_check_sign(
    graphics: &mut CfgasGeGraphics,
    rect: CfxRectF,
    color: u32,
    shape: u32,
    matrix: CfxMatrix,
) {
    let mut path = CfgasGePath::new();
    match shape {
        0 => {
            path = check_path(rect.width);
            let mut translated = CfxMatrix::default();
            translated.translate(rect.left, rect.top);
            translated.concat(matrix);
            let mut state = graphics.state_restorer();
            state.set_fill_color(&CfgasGeColor::Solid(color));
            state.fill_path(&path, FillType::Winding, translated);
            return;
        }
        1 => path.add_ellipse(rect),
        2 => {
            path.add_line(rect.top_left(), CfxPointF::new(rect.right(), rect.bottom()));
            path.add_line(
                CfxPointF::new(rect.left, rect.bottom()),
                CfxPointF::new(rect.right(), rect.top),
            );
            let mut state = graphics.state_restorer();
            state.set_stroke_color(&CfgasGeColor::Solid(color));
            state.set_line_width(1.);
            state.stroke_path(&path, matrix);
            return;
        }
        3 => {
            path.move_to(CfxPointF::new(rect.left + rect.width / 2., rect.top));
            path.line_to(CfxPointF::new(rect.left, rect.top + rect.height / 2.));
            path.line_to(CfxPointF::new(rect.left + rect.width / 2., rect.bottom()));
            path.line_to(CfxPointF::new(rect.right(), rect.top + rect.height / 2.));
            path.line_to(CfxPointF::new(rect.left + rect.width / 2., rect.top));
        }
        4 => path.add_rectangle(rect.left, rect.top, rect.width, rect.height),
        5 => {
            let radius = (rect.top - rect.bottom()) / (1. + (std::f32::consts::PI / 5.).cos());
            let center = CfxPointF::new(
                (rect.left + rect.right()) / 2.,
                (rect.top + rect.bottom()) / 2.,
            );
            let mut angle = std::f32::consts::PI / 10.;
            let points: [CfxPointF; 5] = std::array::from_fn(|_| {
                let point = center + CfxPointF::new(radius * angle.cos(), radius * angle.sin());
                angle += std::f32::consts::PI * 2. / 5.;
                point
            });
            path.move_to(points[0]);
            let mut next = 0;
            for _ in 0..5 {
                next = (next + 2) % 5;
                path.line_to(points[next]);
            }
        }
        _ => return,
    }
    let mut state = graphics.state_restorer();
    state.set_fill_color(&CfgasGeColor::Solid(color));
    state.fill_path(&path, FillType::Winding, matrix);
}

/// Source checkmark cubic geometry. Recompute its inexpensive current-size
/// path instead of retaining the source's incorrectly first-size-only cache.
fn check_path(length: f32) -> CfgasGePath {
    let w = 100.;
    let h = -100.;
    let bottom = 100.;
    let p1 = CfxPointF::new(w / 15., bottom + h * 2. / 5.);
    let p2 = CfxPointF::new(w / 4.5, bottom + h / 16.);
    let p3 = CfxPointF::new(w / 3., bottom);
    let p4 = CfxPointF::new(w * 14. / 15., bottom + h * 15. / 16.);
    let p5 = CfxPointF::new(w / 3.6, bottom + h / 3.5);
    let controls = [
        (
            CfxPointF::new(w / 7., bottom + h * 2. / 7.),
            CfxPointF::new(w / 5., bottom + h / 5.),
        ),
        (
            CfxPointF::new(w / 4.4, bottom + h * 0. / 16.),
            CfxPointF::new(w / 4., bottom),
        ),
        (
            CfxPointF::new(w * (1. / 7. + 7. / 15.), bottom + h * 4. / 5.),
            CfxPointF::new(w * (1. / 7. + 7. / 15.), bottom + h * 4. / 5.),
        ),
        (
            CfxPointF::new(w * 7. / 15., bottom + h * 8. / 7.),
            CfxPointF::new(w / 3.4, bottom + h / 3.5),
        ),
        (
            CfxPointF::new(w / 3.6, bottom + h / 4.),
            CfxPointF::new(w / 3.5, bottom + h * 3.5 / 5.),
        ),
    ];
    let points = [p1, p2, p3, p4, p5, p1];
    let mut path = CfgasGePath::new();
    path.move_to(p1);
    for i in 0..5 {
        let scale = |point: CfxPointF| {
            CfxPointF::new(point.x * 0.5522847498308_f32, point.y * 0.5522847498308_f32)
        };
        path.bezier_to(
            points[i] + scale(controls[i].0 - points[i]),
            points[i + 1] + scale(controls[i].1 - points[i + 1]),
            points[i + 1],
        );
    }
    let scale = length / 100.;
    let mut matrix = CfxMatrix::default();
    matrix.scale(scale, scale);
    path.transform_by(matrix);
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    };

    fn sign(shape: u32, size: f32) -> CfxDibitmap {
        let mut bitmap = CfxDibitmap::create(64, 64, FxdibFormat::Bgra, 0).unwrap();
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut graphics = CfgasGeGraphics::new(&mut device);
            let saved = graphics.get_clip_rect();
            graphics.set_fill_color(&CfgasGeColor::Solid(0xff00ff00));
            draw_check_sign(
                &mut graphics,
                CfxRectF::new(8., 8., size, size),
                0xff000000,
                shape,
                CfxMatrix::default(),
            );
            assert_eq!(graphics.get_clip_rect(), saved);
            assert_eq!(graphics.get_matrix(), CfxMatrix::default());
            let mut path = CfgasGePath::new();
            path.add_rectangle(55., 55., 4., 4.);
            graphics.fill_path(&path, FillType::Winding, CfxMatrix::default());
        }
        assert_eq!(
            &bitmap.scanline(56).unwrap()[56 * 4..57 * 4],
            &[0, 255, 0, 255]
        );
        bitmap
    }

    #[test]
    fn six_check_signs_draw_distinct_geometry_and_restore_state() {
        let mut results = Vec::new();
        for shape in 0..6 {
            let bitmap = sign(shape, 32.);
            let ink = bitmap
                .buffer()
                .chunks_exact(4)
                .filter(|pixel| pixel[3] != 0 && pixel[1] == 0)
                .count();
            assert!(ink > 30, "shape {shape}: {ink}");
            assert!(bitmap.scanline(0).unwrap().iter().all(|&byte| byte == 0));
            assert!(
                results
                    .iter()
                    .all(|previous: &Vec<u8>| previous.as_slice() != bitmap.buffer()),
                "shape {shape} must have its own outline"
            );
            results.push(bitmap.buffer().to_vec());
        }
    }

    #[test]
    fn checkmark_resizes_after_a_different_size_was_drawn() {
        let small = sign(0, 16.);
        let large = sign(0, 32.);
        let ink = |bitmap: &CfxDibitmap| {
            bitmap
                .buffer()
                .chunks_exact(4)
                .filter(|pixel| pixel[3] != 0 && pixel[1] == 0)
                .count()
        };
        assert!(ink(&large) > ink(&small) * 2);
        let rightmost = |bitmap: &CfxDibitmap| {
            (0..64)
                .filter(|&x| {
                    (0..50).any(|y| {
                        let pixel = &bitmap.scanline(y).unwrap()[x * 4..x * 4 + 4];
                        pixel[3] != 0 && pixel[1] == 0
                    })
                })
                .max()
                .unwrap()
        };
        assert!(rightmost(&large) > rightmost(&small) + 8);
        let small_path = check_path(16.);
        let large_path = check_path(32.);
        assert_eq!(small_path.get_path().points().len(), 16);
        for (small, large) in small_path
            .get_path()
            .points()
            .iter()
            .zip(large_path.get_path().points())
        {
            assert_eq!(small.point_type, large.point_type);
            assert_eq!(small.point.x * 2., large.point.x);
            assert_eq!(small.point.y * 2., large.point.y);
        }
    }
}
