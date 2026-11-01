// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
// Copyright (C) 2005 Tony Juricic (tonygeek@yahoo.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_curves.{h,cpp}`.

use std::ops::{Index, IndexMut};

use super::agg_basics::{
    PointType, PATH_CMD_LINE_TO, PATH_CMD_MOVE_TO, PATH_CMD_STOP, PATH_FLAGS_JR,
};
use super::agg_path_storage::AggVertexSource;

const CURVE_COLLINEARITY_EPSILON: f32 = 1e-30;
const CURVE_RECURSION_LIMIT: u32 = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Curve4Points {
    pub cp: [f32; 8],
}

impl Curve4Points {
    pub const fn new(cp: [f32; 8]) -> Self {
        Self { cp }
    }

    pub fn init(&mut self, cp: [f32; 8]) {
        self.cp = cp;
    }
}

impl Index<usize> for Curve4Points {
    type Output = f32;

    fn index(&self, index: usize) -> &Self::Output {
        &self.cp[index]
    }
}

impl IndexMut<usize> for Curve4Points {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.cp[index]
    }
}

#[derive(Default)]
pub struct Curve4Div {
    distance_tolerance_square: f32,
    distance_tolerance_manhattan: f32,
    count: usize,
    points: Vec<PointType>,
}

impl Curve4Div {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_points(points: Curve4Points) -> Self {
        let mut curve = Self::new();
        curve.init(points);
        curve
    }

    pub fn reset(&mut self) {
        self.points.clear();
        self.count = 0;
    }

    pub fn init(&mut self, control: Curve4Points) {
        self.points.clear();
        self.distance_tolerance_square = 1.0 / 4.0;
        self.distance_tolerance_manhattan = 1.0 * 4.0;
        self.bezier(control.cp);
        self.count = 0;
    }

    fn bezier(&mut self, control: [f32; 8]) {
        self.points.push(PointType::new(control[0], control[1], 0));
        self.recursive_bezier(control, 0);
        self.points.push(PointType::new(control[6], control[7], 0));
    }

    fn recursive_bezier(&mut self, c: [f32; 8], level: u32) {
        if level > CURVE_RECURSION_LIMIT {
            return;
        }
        let [x1, y1, x2, y2, x3, y3, x4, y4] = c;
        let x12 = (x1 + x2) / 2.0;
        let y12 = (y1 + y2) / 2.0;
        let x23 = (x2 + x3) / 2.0;
        let y23 = (y2 + y3) / 2.0;
        let x34 = (x3 + x4) / 2.0;
        let y34 = (y3 + y4) / 2.0;
        let x123 = (x12 + x23) / 2.0;
        let y123 = (y12 + y23) / 2.0;
        let x234 = (x23 + x34) / 2.0;
        let y234 = (y23 + y34) / 2.0;
        let x1234 = (x123 + x234) / 2.0;
        let y1234 = (y123 + y234) / 2.0;
        let dx = x4 - x1;
        let dy = y4 - y1;
        let d2 = ((x2 - x4) * dy - (y2 - y4) * dx).abs();
        let d3 = ((x3 - x4) * dy - (y3 - y4) * dx).abs();
        let case = (u8::from(d2 > CURVE_COLLINEARITY_EPSILON) << 1)
            + u8::from(d3 > CURVE_COLLINEARITY_EPSILON);
        let flat = match case {
            0 => {
                (x1 + x3 - x2 - x2).abs()
                    + (y1 + y3 - y2 - y2).abs()
                    + (x2 + x4 - x3 - x3).abs()
                    + (y2 + y4 - y3 - y3).abs()
                    <= self.distance_tolerance_manhattan
            }
            1 => d3 * d3 <= self.distance_tolerance_square * (dx * dx + dy * dy),
            2 => d2 * d2 <= self.distance_tolerance_square * (dx * dx + dy * dy),
            _ => (d2 + d3) * (d2 + d3) <= self.distance_tolerance_square * (dx * dx + dy * dy),
        };
        if flat {
            let (x, y) = if case == 0 {
                (x1234, y1234)
            } else {
                (x23, y23)
            };
            self.points.push(PointType::new(x, y, PATH_FLAGS_JR));
            return;
        }
        self.recursive_bezier([x1, y1, x12, y12, x123, y123, x1234, y1234], level + 1);
        self.recursive_bezier([x1234, y1234, x234, y234, x34, y34, x4, y4], level + 1);
    }

    pub fn point_count(&self) -> usize {
        self.points.len()
    }

    pub fn point(&self, index: usize) -> Option<PointType> {
        self.points.get(index).copied()
    }
}

impl AggVertexSource for Curve4Div {
    fn rewind(&mut self, _path_id: u32) {
        self.count = 0;
    }

    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        let Some(point) = self.points.get(self.count) else {
            return PATH_CMD_STOP;
        };
        self.count += 1;
        *x = point.x;
        *y = point.y;
        if self.count == 1 {
            PATH_CMD_MOVE_TO
        } else {
            PATH_CMD_LINE_TO
        }
    }
}

#[derive(Default)]
pub struct Curve4 {
    curve_div: Curve4Div,
}

impl Curve4 {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.curve_div.points.capacity() * std::mem::size_of::<PointType>()
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_points(points: Curve4Points) -> Self {
        Self {
            curve_div: Curve4Div::from_points(points),
        }
    }

    pub fn reset(&mut self) {
        self.curve_div.reset();
    }

    pub fn init(&mut self, points: Curve4Points) {
        self.curve_div.init(points);
    }
}

impl AggVertexSource for Curve4 {
    fn rewind(&mut self, path_id: u32) {
        self.curve_div.rewind(path_id);
    }

    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        self.curve_div.vertex(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_curve_emits_start_midpoint_and_end() {
        let mut curve =
            Curve4::from_points(Curve4Points::new([0.0, 0.0, 1.0, 0.0, 2.0, 0.0, 3.0, 0.0]));
        let (mut x, mut y) = (0.0, 0.0);
        assert_eq!(curve.vertex(&mut x, &mut y), PATH_CMD_MOVE_TO);
        assert_eq!((x, y), (0.0, 0.0));
        assert_eq!(curve.vertex(&mut x, &mut y), PATH_CMD_LINE_TO);
        assert_eq!((x, y), (1.5, 0.0));
        assert_eq!(curve.vertex(&mut x, &mut y), PATH_CMD_LINE_TO);
        assert_eq!((x, y), (3.0, 0.0));
        assert_eq!(curve.vertex(&mut x, &mut y), PATH_CMD_STOP);
        curve.rewind(0);
        assert_eq!(curve.vertex(&mut x, &mut y), PATH_CMD_MOVE_TO);
    }

    #[test]
    fn bent_curve_subdivides_with_joint_replacement_points() {
        let curve = Curve4Div::from_points(Curve4Points::new([
            0.0, 0.0, 0.0, 20.0, 20.0, 20.0, 20.0, 0.0,
        ]));
        assert!(curve.point_count() > 3);
        assert_eq!(curve.point(1).unwrap().flag, PATH_FLAGS_JR);
    }
}
