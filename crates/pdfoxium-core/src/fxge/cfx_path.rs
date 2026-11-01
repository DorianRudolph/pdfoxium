// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxge/cfx_path.{h,cpp}`. `Rc<CfxPath>` stands for the source's
//! retainable path; cloning `CfxPath` makes its private copy.

use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointType {
    Line,
    Bezier,
    Move,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathPoint {
    pub point: CfxPointF,
    pub point_type: PointType,
    pub close_figure: bool,
}

impl Default for PathPoint {
    fn default() -> Self {
        Self {
            point: CfxPointF::default(),
            point_type: PointType::Line,
            close_figure: false,
        }
    }
}

impl PathPoint {
    pub fn new(point: CfxPointF, point_type: PointType, close_figure: bool) -> Self {
        Self {
            point,
            point_type,
            close_figure,
        }
    }
    fn is_type_and_open(self, point_type: PointType) -> bool {
        self.point_type == point_type && !self.close_figure
    }
}

fn is_rect_pre_transform(points: &[PathPoint]) -> bool {
    if points.len() != 4 && points.len() != 5 {
        return false;
    }
    if points.len() == 5 && points[0].point != points[4].point {
        return false;
    }
    if points[0].point == points[2].point || points[1].point == points[3].point {
        return false;
    }
    points[1..]
        .iter()
        .all(|point| point.point_type == PointType::Line)
}

fn xy_both_not_equal(first: CfxPointF, second: CfxPointF) -> bool {
    first.x != second.x && first.y != second.y
}

fn is_rect_impl(points: &[PathPoint]) -> bool {
    is_rect_pre_transform(points)
        && (1..4).all(|i| !xy_both_not_equal(points[i].point, points[i - 1].point))
        && !xy_both_not_equal(points[0].point, points[3].point)
}

fn normalized_points(points: &[PathPoint]) -> Vec<PathPoint> {
    if points[0].point != points.last().unwrap().point {
        return Vec::new();
    }
    let mut normalized = Vec::with_capacity(6);
    normalized.push(points[0]);
    for (index, point) in points.iter().enumerate().skip(1) {
        if normalized.len() + points.len() - index == 5 {
            normalized.extend_from_slice(&points[index..]);
            break;
        }
        if point.point_type == PointType::Line
            && !point.close_figure
            && !normalized.last().unwrap().close_figure
            && point.point == normalized.last().unwrap().point
        {
            continue;
        }
        normalized.push(*point);
        if normalized.len() > 5 {
            return Vec::new();
        }
    }
    debug_assert_eq!(normalized.len(), 5);
    normalized
}

fn update_line_end_points(rect: &mut CfxFloatRect, start: CfxPointF, end: CfxPointF, hw: f32) {
    if start.x == end.x {
        if start.y == end.y {
            rect.update_rect(end + CfxPointF::new(hw, hw));
            rect.update_rect(end - CfxPointF::new(hw, hw));
            return;
        }
        let point_y = if end.y < start.y {
            end.y - hw
        } else {
            end.y + hw
        };
        rect.update_rect(CfxPointF::new(end.x + hw, point_y));
        rect.update_rect(CfxPointF::new(end.x - hw, point_y));
        return;
    }
    if start.y == end.y {
        let point_x = if end.x < start.x {
            end.x - hw
        } else {
            end.x + hw
        };
        rect.update_rect(CfxPointF::new(point_x, end.y + hw));
        rect.update_rect(CfxPointF::new(point_x, end.y - hw));
        return;
    }
    let diff = end - start;
    let length = diff.x.hypot(diff.y);
    let mx = end.x + hw * diff.x / length;
    let my = end.y + hw * diff.y / length;
    let dx = hw * diff.y / length;
    let dy = hw * diff.x / length;
    rect.update_rect(CfxPointF::new(mx - dx, my + dy));
    rect.update_rect(CfxPointF::new(mx + dx, my - dy));
}

fn update_line_join_points(
    rect: &mut CfxFloatRect,
    start: CfxPointF,
    mid: CfxPointF,
    end: CfxPointF,
    half_width: f32,
    _miter_limit: f32,
) {
    let mut start_k = 0.0;
    let mut start_c = 0.0;
    let mut end_k = 0.0;
    let mut end_c = 0.0;
    let mut start_dc = 0.0;
    let mut end_dc = 0.0;
    let one_twentieth = 1.0 / 20.0;
    let start_vert = (start.x - mid.x).abs() < one_twentieth;
    let end_vert = (mid.x - end.x).abs() < one_twentieth;
    if start_vert && end_vert {
        let direction = if mid.y > start.y { 1.0 } else { -1.0 };
        let y = mid.y + half_width * direction;
        rect.update_rect(CfxPointF::new(mid.x + half_width, y));
        rect.update_rect(CfxPointF::new(mid.x - half_width, y));
        return;
    }
    if !start_vert {
        let diff = start - mid;
        start_k = (mid.y - start.y) / (mid.x - start.x);
        start_c = mid.y - start_k * mid.x;
        start_dc = (half_width * diff.x.hypot(diff.y) / diff.x).abs();
    }
    if !end_vert {
        let diff = end - mid;
        end_k = diff.y / diff.x;
        end_c = mid.y - end_k * mid.x;
        end_dc = (half_width * diff.x.hypot(diff.y) / diff.x).abs();
    }
    if start_vert {
        let x = start.x
            + if end.x < start.x {
                half_width
            } else {
                -half_width
            };
        let y = if start.y < end_k * start.x + end_c {
            end_k * x + end_c + end_dc
        } else {
            end_k * x + end_c - end_dc
        };
        rect.update_rect(CfxPointF::new(x, y));
        return;
    }
    if end_vert {
        let x = end.x
            + if start.x < end.x {
                half_width
            } else {
                -half_width
            };
        let y = if end.y < start_k * end.x + start_c {
            start_k * x + start_c + start_dc
        } else {
            start_k * x + start_c - start_dc
        };
        rect.update_rect(CfxPointF::new(x, y));
        return;
    }
    if (start_k - end_k).abs() < one_twentieth {
        let start_dir = if mid.x > start.x { 1 } else { -1 };
        let end_dir = if end.x > mid.x { 1 } else { -1 };
        if start_dir == end_dir {
            update_line_end_points(rect, mid, end, half_width);
        } else {
            update_line_end_points(rect, start, mid, half_width);
        }
        return;
    }
    let start_outside_c = start_c
        + if end.y < start_k * end.x + start_c {
            start_dc
        } else {
            -start_dc
        };
    let end_outside_c = end_c
        + if start.y < end_k * start.x + end_c {
            end_dc
        } else {
            -end_dc
        };
    let x = (end_outside_c - start_outside_c) / (start_k - end_k);
    rect.update_rect(CfxPointF::new(x, start_k * x + start_outside_c));
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CfxPath {
    points: Vec<PathPoint>,
}

impl CfxPath {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.points.clear();
    }
    pub fn points(&self) -> &[PathPoint] {
        &self.points
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.points.capacity() * std::mem::size_of::<PathPoint>()
    }
    pub fn points_mut(&mut self) -> &mut Vec<PathPoint> {
        &mut self.points
    }
    pub fn get_type(&self, index: usize) -> PointType {
        self.points[index].point_type
    }
    pub fn is_closing_figure(&self, index: usize) -> bool {
        self.points[index].close_figure
    }
    pub fn get_point(&self, index: usize) -> CfxPointF {
        self.points[index].point
    }
    pub fn close_path(&mut self) {
        if let Some(point) = self.points.last_mut() {
            point.close_figure = true;
        }
    }
    pub fn append(&mut self, source: &Self, matrix: Option<&CfxMatrix>) {
        if source.points.is_empty() {
            return;
        }
        let start = self.points.len();
        self.points.extend_from_slice(&source.points);
        if let Some(matrix) = matrix {
            for point in &mut self.points[start..] {
                point.point = matrix.transform(point.point);
            }
        }
    }
    /// Same operation as `Append(*this, matrix)` in the C++ interface.
    pub fn append_self(&mut self, matrix: Option<&CfxMatrix>) {
        let start = self.points.len();
        self.points.extend_from_within(..start);
        if let Some(matrix) = matrix {
            for point in &mut self.points[start..] {
                point.point = matrix.transform(point.point);
            }
        }
    }
    pub fn append_point(&mut self, point: CfxPointF, point_type: PointType) {
        self.points.push(PathPoint::new(point, point_type, false));
    }
    pub fn append_point_and_close(&mut self, point: CfxPointF, point_type: PointType) {
        self.points.push(PathPoint::new(point, point_type, true));
    }
    pub fn append_line(&mut self, first: CfxPointF, second: CfxPointF) {
        if self.points.last().is_none_or(|point| {
            (point.point.x - first.x).abs() > 0.001 || (point.point.y - first.y).abs() > 0.001
        }) {
            self.append_point(first, PointType::Move);
        }
        self.append_point(second, PointType::Line);
    }
    pub fn append_float_rect(&mut self, rect: CfxFloatRect) {
        self.append_rect(rect.left, rect.bottom, rect.right, rect.top);
    }
    pub fn append_rect(&mut self, left: f32, bottom: f32, right: f32, top: f32) {
        let lb = CfxPointF::new(left, bottom);
        let lt = CfxPointF::new(left, top);
        let rt = CfxPointF::new(right, top);
        let rb = CfxPointF::new(right, bottom);
        self.append_line(lb, lt);
        self.append_line(lt, rt);
        self.append_line(rt, rb);
        self.append_line(rb, lb);
        self.close_path();
    }
    pub fn bounding_box(&self) -> CfxFloatRect {
        let Some(first) = self.points.first() else {
            return CfxFloatRect::default();
        };
        let mut rect = CfxFloatRect::from_point(first.point);
        for point in &self.points[1..] {
            rect.update_rect(point.point);
        }
        rect
    }
    pub fn bounding_box_for_stroke_path(&self, line_width: f32, miter_limit: f32) -> CfxFloatRect {
        let mut rect = CfxFloatRect::new(100000.0, 100000.0, -100000.0, -100000.0);
        let mut index = 0;
        while index < self.points.len() {
            let (start, end, middle) = if self.points[index].point_type == PointType::Move {
                if index + 1 == self.points.len() {
                    if self.points[index].close_figure {
                        rect.update_rect(self.points[index].point);
                    }
                    break;
                }
                (index + 1, index, None)
            } else {
                if self.points[index].is_type_and_open(PointType::Bezier) {
                    assert!(index + 2 < self.points.len());
                    debug_assert_eq!(self.points[index + 1].point_type, PointType::Bezier);
                    debug_assert_eq!(self.points[index + 2].point_type, PointType::Bezier);
                    rect.update_rect(self.points[index].point);
                    rect.update_rect(self.points[index + 1].point);
                    index += 2;
                }
                assert!(index > 0);
                if index + 1 == self.points.len()
                    || self.points[index + 1].point_type == PointType::Move
                {
                    (index - 1, index, None)
                } else {
                    (index - 1, index + 1, Some(index))
                }
            };
            if let Some(middle) = middle {
                update_line_join_points(
                    &mut rect,
                    self.points[start].point,
                    self.points[middle].point,
                    self.points[end].point,
                    line_width,
                    miter_limit,
                );
            } else {
                update_line_end_points(
                    &mut rect,
                    self.points[start].point,
                    self.points[end].point,
                    line_width,
                );
            }
            index += 1;
        }
        rect
    }
    pub fn transform(&mut self, matrix: CfxMatrix) {
        for point in &mut self.points {
            point.point = matrix.transform(point.point);
        }
    }
    pub fn is_rect(&self) -> bool {
        if self.points.len() > 5 {
            is_rect_impl(&normalized_points(&self.points))
        } else {
            is_rect_impl(&self.points)
        }
    }
    pub fn rect_opposite_corner(&self) -> Option<CfxPointF> {
        let normalized;
        let points = if self.points.len() > 5 {
            normalized = normalized_points(&self.points);
            &normalized[..]
        } else {
            &self.points[..]
        };
        is_rect_impl(points).then(|| points[2].point)
    }
    pub fn rect(&self, matrix: Option<&CfxMatrix>) -> Option<CfxFloatRect> {
        let normalized;
        let points = if self.points.len() > 5 {
            normalized = normalized_points(&self.points);
            &normalized[..]
        } else {
            &self.points[..]
        };
        let Some(matrix) = matrix else {
            if !is_rect_impl(points) {
                return None;
            }
            let mut rect = CfxFloatRect::new(
                points[0].point.x,
                points[0].point.y,
                points[2].point.x,
                points[2].point.y,
            );
            rect.normalize();
            return Some(rect);
        };
        if !is_rect_pre_transform(points) {
            return None;
        }
        let mut transformed = [CfxPointF::default(); 5];
        for (i, point) in points.iter().enumerate() {
            transformed[i] = matrix.transform(point.point);
            if i > 0 && xy_both_not_equal(transformed[i], transformed[i - 1]) {
                return None;
            }
        }
        if xy_both_not_equal(transformed[0], transformed[3]) {
            return None;
        }
        let mut rect = CfxFloatRect::new(
            transformed[0].x,
            transformed[0].y,
            transformed[2].x,
            transformed[2].y,
        );
        rect.normalize();
        Some(rect)
    }
}

#[cfg(test)]
mod tests {
    use super::{CfxPath, PointType};
    use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};

    #[test]
    fn rectangle_and_transformed_rectangle_match_pdfium_cases() {
        let mut path = CfxPath::new();
        path.append_rect(1.0, 2.0, 3.0, 5.0);
        assert_eq!(path.points().len(), 5);
        assert!(path.is_rect());
        assert_eq!(path.rect(None), Some(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0)));
        assert_eq!(path.rect_opposite_corner(), Some(CfxPointF::new(3.0, 5.0)));
        let matrix = CfxMatrix::new(1.0, 0.0, 0.0, 2.0, 60.0, 70.0);
        assert_eq!(
            path.rect(Some(&matrix)),
            Some(CfxFloatRect::new(61.0, 74.0, 63.0, 80.0))
        );
        path.transform(matrix);
        assert_eq!(
            path.rect(None),
            Some(CfxFloatRect::new(61.0, 74.0, 63.0, 80.0))
        );
    }

    #[test]
    fn stroke_bounds_and_duplicate_point_normalization() {
        let mut path = CfxPath::new();
        for (x, y, kind) in [
            (0.0, 0.0, PointType::Move),
            (0.0, 0.0, PointType::Line),
            (2.0, 0.0, PointType::Line),
            (2.0, 1.0, PointType::Line),
            (0.0, 1.0, PointType::Line),
            (0.0, 0.0, PointType::Line),
        ] {
            path.append_point(CfxPointF::new(x, y), kind);
        }
        assert!(path.is_rect());
        assert_eq!(path.rect_opposite_corner(), Some(CfxPointF::new(2.0, 1.0)));
        path.clear();
        path.append_point(CfxPointF::new(2.0, 0.0), PointType::Move);
        path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
        path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
        path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
        path.close_path();
        assert_eq!(
            path.bounding_box_for_stroke_path(1.0, 1.0),
            CfxFloatRect::new(-1.0, -1.0, 3.0, 2.0)
        );
    }

    #[test]
    fn appending_path_to_itself_keeps_original_points() {
        let mut path = CfxPath::new();
        path.append_line(CfxPointF::new(1.0, 2.0), CfxPointF::new(3.0, 4.0));
        let matrix = CfxMatrix::new(1.0, 0.0, 0.0, 2.0, 60.0, 70.0);
        path.append_self(Some(&matrix));
        assert_eq!(path.points().len(), 4);
        assert_eq!(path.get_point(0), CfxPointF::new(1.0, 2.0));
        assert_eq!(path.get_point(2), CfxPointF::new(61.0, 74.0));
    }
}

#[cfg(test)]
#[path = "cfx_path_unittest.rs"]
mod upstream_tests;
