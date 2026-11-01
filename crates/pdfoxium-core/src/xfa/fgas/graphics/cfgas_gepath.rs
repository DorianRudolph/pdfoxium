// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/graphics/cfgas_gepath.{h,cpp}`. Elliptical arcs retain the
//! source quarter-arc cubic construction and its 0.01-radian split tolerance.

use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, CfxSizeF},
    fxge::cfx_path::{CfxPath, PointType},
};
use std::f32::consts::{FRAC_PI_2, TAU};

#[derive(Clone, Debug, Default)]
pub struct CfgasGePath {
    path: CfxPath,
}

impl CfgasGePath {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_path(&self) -> &CfxPath {
        &self.path
    }
    pub fn clear(&mut self) {
        self.path.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.path.points().is_empty()
    }
    pub fn transform_by(&mut self, matrix: CfxMatrix) {
        self.path.transform(matrix);
    }
    pub fn close(&mut self) {
        self.path.close_path();
    }
    pub fn move_to(&mut self, point: CfxPointF) {
        self.path.append_point(point, PointType::Move);
    }
    pub fn line_to(&mut self, point: CfxPointF) {
        self.path.append_point(point, PointType::Line);
    }
    pub fn bezier_to(&mut self, c1: CfxPointF, c2: CfxPointF, to: CfxPointF) {
        self.path.append_point(c1, PointType::Bezier);
        self.path.append_point(c2, PointType::Bezier);
        self.path.append_point(to, PointType::Bezier);
    }
    pub fn arc_to(&mut self, pos: CfxPointF, size: CfxSizeF, start_angle: f32, sweep_angle: f32) {
        if !valid_arc(pos, size, start_angle, sweep_angle) {
            return;
        }
        let size = size / 2.0;
        self.arc_to_internal(
            CfxPointF::new(pos.x + size.width, pos.y + size.height),
            size,
            start_angle,
            sweep_angle,
        );
    }
    fn arc_to_internal(
        &mut self,
        pos: CfxPointF,
        size: CfxSizeF,
        start_angle: f32,
        sweep_angle: f32,
    ) {
        let x0 = (sweep_angle / 2.0).cos();
        let y0 = (sweep_angle / 2.0).sin();
        if y0 == 0.0 {
            return;
        }
        let tx = ((1.0 - x0) * 4.0) / 3.0;
        let ty = y0 - ((tx * x0) / y0);
        let points = [CfxPointF::new(x0 + tx, -ty), CfxPointF::new(x0 + tx, ty)];
        let sn = (start_angle + sweep_angle / 2.0).sin();
        let cs = (start_angle + sweep_angle / 2.0).cos();
        for point in points {
            self.path.append_point(
                CfxPointF::new(
                    pos.x + (size.width * ((point.x * cs) - (point.y * sn))),
                    pos.y + (size.height * ((point.x * sn) + (point.y * cs))),
                ),
                PointType::Bezier,
            );
        }
        self.path.append_point(
            CfxPointF::new(
                pos.x + (size.width * (start_angle + sweep_angle).cos()),
                pos.y + (size.height * (start_angle + sweep_angle).sin()),
            ),
            PointType::Bezier,
        );
    }
    pub fn add_line(&mut self, p1: CfxPointF, p2: CfxPointF) {
        self.path.append_point(p1, PointType::Move);
        self.path.append_point(p2, PointType::Line);
    }
    pub fn add_rectangle(&mut self, left: f32, top: f32, width: f32, height: f32) {
        self.path.append_rect(left, top, left + width, top + height);
    }
    pub fn add_ellipse(&mut self, rect: CfxRectF) {
        self.add_arc(rect.top_left(), rect.size(), 0.0, TAU);
    }
    pub fn add_arc(
        &mut self,
        original_pos: CfxPointF,
        original_size: CfxSizeF,
        mut start_angle: f32,
        mut sweep_angle: f32,
    ) {
        if !valid_arc(original_pos, original_size, start_angle, sweep_angle) {
            return;
        }
        // Bounded normalization avoids the source's unbounded repeated
        // subtraction for very large finite angles. Ordinary angles are kept.
        if !(0.0..=TAU).contains(&start_angle) {
            start_angle = start_angle.rem_euclid(TAU);
        }
        sweep_angle = sweep_angle.clamp(-TAU, TAU);
        let size = original_size / 2.0;
        let pos = CfxPointF::new(original_pos.x + size.width, original_pos.y + size.height);
        self.path.append_point(
            pos + CfxPointF::new(
                size.width * start_angle.cos(),
                size.height * start_angle.sin(),
            ),
            PointType::Move,
        );
        let mut total_sweep = 0.0;
        loop {
            let prev_sweep = total_sweep;
            let mut local_sweep;
            let done;
            if sweep_angle < 0.0 {
                local_sweep = -FRAC_PI_2;
                total_sweep -= FRAC_PI_2;
                done = total_sweep <= sweep_angle + 0.01;
            } else {
                local_sweep = FRAC_PI_2;
                total_sweep += FRAC_PI_2;
                done = total_sweep >= sweep_angle - 0.01;
            }
            if done {
                local_sweep = sweep_angle - prev_sweep;
            }
            self.arc_to_internal(pos, size, start_angle, local_sweep);
            start_angle += local_sweep;
            if done {
                break;
            }
        }
    }
    pub fn add_subpath(&mut self, path: &Self) {
        self.path.append(&path.path, None);
    }
}

fn valid_arc(pos: CfxPointF, size: CfxSizeF, start: f32, sweep: f32) -> bool {
    // Zero sweep would divide by zero in the source cubic formula. Invalid
    // coordinates/angles must not enter an unbounded normalization loop.
    sweep != 0.0
        && [pos.x, pos.y, size.width, size.height, start, sweep]
            .into_iter()
            .all(f32::is_finite)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.00001,
            "{actual} != {expected}"
        );
    }
    #[test]
    fn quarter_arcs_have_expected_cubic_controls_in_both_directions() {
        for sign in [-1.0, 1.0] {
            let mut path = CfgasGePath::new();
            path.add_arc(
                CfxPointF::new(10.0, 20.0),
                CfxSizeF::new(8.0, 4.0),
                0.0,
                sign * FRAC_PI_2,
            );
            let points = path.get_path().points();
            assert_eq!(points.len(), 4);
            assert_eq!(points[0].point_type, PointType::Move);
            assert!(points[1..]
                .iter()
                .all(|p| p.point_type == PointType::Bezier));
            near(points[0].point.x, 18.0);
            near(points[0].point.y, 22.0);
            near(points[1].point.x, 18.0);
            near(points[1].point.y, 22.0 + sign * 1.1045695);
            near(points[2].point.x, 16.209139);
            near(points[2].point.y, 22.0 + sign * 2.0);
            near(points[3].point.x, 14.0);
            near(points[3].point.y, 22.0 + sign * 2.0);
            let mut continuation = CfgasGePath::new();
            continuation.move_to(points[0].point);
            continuation.arc_to(
                CfxPointF::new(10.0, 20.0),
                CfxSizeF::new(8.0, 4.0),
                0.0,
                sign * FRAC_PI_2,
            );
            assert_eq!(continuation.get_path().points(), points);
        }
    }
    #[test]
    fn ellipse_clamping_splitting_and_invalid_angles() {
        let mut ellipse = CfgasGePath::new();
        ellipse.add_ellipse(CfxRectF::new(2.0, 3.0, 6.0, 4.0));
        assert_eq!(ellipse.get_path().points().len(), 13);
        let first = ellipse.get_path().points()[0].point;
        let last = ellipse.get_path().points()[12].point;
        near(first.x, last.x);
        near(first.y, last.y);
        for sweep in [100.0, -100.0] {
            let mut path = CfgasGePath::new();
            path.add_arc(CfxPointF::default(), CfxSizeF::new(2.0, 2.0), 0.0, sweep);
            assert_eq!(path.get_path().points().len(), 13);
        }
        for sweep in [0.0, f32::NAN, f32::INFINITY] {
            let mut path = CfgasGePath::new();
            path.add_arc(CfxPointF::default(), CfxSizeF::new(2.0, 2.0), 0.0, sweep);
            path.arc_to(CfxPointF::default(), CfxSizeF::new(2.0, 2.0), 0.0, sweep);
            assert!(path.is_empty());
        }
        let mut large = CfgasGePath::new();
        large.add_arc(
            CfxPointF::default(),
            CfxSizeF::new(2.0, 2.0),
            f32::MAX,
            FRAC_PI_2,
        );
        assert_eq!(large.get_path().points().len(), 4);
        assert!(large
            .get_path()
            .points()
            .iter()
            .all(|p| p.point.x.is_finite() && p.point.y.is_finite()));
    }
    #[test]
    fn geometry_mutation_preserves_subpath_and_close_flags() {
        let mut path = CfgasGePath::new();
        path.add_line(CfxPointF::new(1.0, 2.0), CfxPointF::new(3.0, 4.0));
        path.close();
        let mut other = CfgasGePath::new();
        other.add_rectangle(5.0, 6.0, 7.0, 8.0);
        path.add_subpath(&other);
        assert_eq!(path.get_path().points().len(), 7);
        assert!(path.get_path().points()[1].close_figure);
        assert!(path.get_path().points()[6].close_figure);
        let mut matrix = CfxMatrix::default();
        matrix.translate(10.0, -5.0);
        path.transform_by(matrix);
        assert_eq!(
            path.get_path().points()[0].point,
            CfxPointF::new(11.0, -3.0)
        );
        path.clear();
        assert!(path.is_empty());
        path.move_to(CfxPointF::new(0.0, 0.0));
        path.line_to(CfxPointF::new(1.0, 1.0));
        path.bezier_to(
            CfxPointF::new(2.0, 2.0),
            CfxPointF::new(3.0, 3.0),
            CfxPointF::new(4.0, 4.0),
        );
        assert_eq!(path.get_path().points().len(), 5);
    }
}
