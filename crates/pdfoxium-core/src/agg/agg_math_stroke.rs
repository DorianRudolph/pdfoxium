// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_math_stroke.h`.
//! The source's coordinate consumer is represented by a contiguous point vector.

use std::f32::consts::PI;

use super::agg_math::{calc_distance, calc_intersection, calc_point_location};
use super::agg_vertex_sequence::VertexDist;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineCap {
    Butt,
    Square,
    Round,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineJoin {
    Miter,
    MiterRevert,
    MiterRound,
    Round,
    Bevel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InnerJoin {
    Bevel,
    Miter,
    Jag,
    Round,
}

pub const STROKE_THETA: f32 = 1.0 / 1000.0;
pub type StrokePoints = Vec<(f32, f32)>;

#[allow(clippy::too_many_arguments)]
pub fn stroke_calc_arc(
    out: &mut StrokePoints,
    x: f32,
    y: f32,
    dx1: f32,
    dy1: f32,
    dx2: f32,
    dy2: f32,
    mut width: f32,
    approximation_scale: f32,
) {
    let mut a1 = dy1.atan2(dx1);
    let mut a2 = dy2.atan2(dx2);
    let mut da = a1 - a2;
    let ccw = da > 0.0 && da < PI;
    if width < 0.0 {
        width = -width;
    }
    da = (width / (width + ((1.0 / 8.0) / approximation_scale))).acos() * 2.0;
    out.push((x + dx1, y + dy1));
    if da > 0.0 {
        if !ccw {
            if a1 > a2 {
                a2 += 2.0 * PI;
            }
            a2 -= da / 4.0;
            a1 += da;
            while a1 < a2 {
                out.push((x + width * a1.cos(), y + width * a1.sin()));
                a1 += da;
            }
        } else {
            if a1 < a2 {
                a2 -= 2.0 * PI;
            }
            a2 += da / 4.0;
            a1 -= da;
            while a1 > a2 {
                out.push((x + width * a1.cos(), y + width * a1.sin()));
                a1 -= da;
            }
        }
    }
    out.push((x + dx2, y + dy2));
}

#[allow(clippy::too_many_arguments)]
pub fn stroke_calc_miter(
    out: &mut StrokePoints,
    v0: VertexDist,
    v1: VertexDist,
    v2: VertexDist,
    dx1: f32,
    dy1: f32,
    dx2: f32,
    dy2: f32,
    width: f32,
    line_join: LineJoin,
    miter_limit: f32,
    approximation_scale: f32,
) {
    let (mut xi, mut yi) = (v1.x, v1.y);
    let mut miter_limit_exceeded = true;
    if calc_intersection(
        v0.x + dx1,
        v0.y - dy1,
        v1.x + dx1,
        v1.y - dy1,
        v1.x + dx2,
        v1.y - dy2,
        v2.x + dx2,
        v2.y - dy2,
        &mut xi,
        &mut yi,
    ) {
        let d1 = calc_distance(v1.x, v1.y, xi, yi);
        let limit = width * miter_limit;
        if d1 <= limit {
            out.push((xi, yi));
            miter_limit_exceeded = false;
        }
    } else {
        let x2 = v1.x + dx1;
        let y2 = v1.y - dy1;
        if (((x2 - v0.x) * dy1 - (v0.y - y2) * dx1) < 0.0)
            != (((x2 - v2.x) * dy1 - (v2.y - y2) * dx1) < 0.0)
        {
            out.push((v1.x + dx1, v1.y - dy1));
            miter_limit_exceeded = false;
        }
    }
    if miter_limit_exceeded {
        match line_join {
            LineJoin::MiterRevert => {
                out.push((v1.x + dx1, v1.y - dy1));
                out.push((v1.x + dx2, v1.y - dy2));
            }
            LineJoin::MiterRound => stroke_calc_arc(
                out,
                v1.x,
                v1.y,
                dx1,
                -dy1,
                dx2,
                -dy2,
                width,
                approximation_scale,
            ),
            _ => {
                out.push((
                    v1.x + dx1 + dy1 * miter_limit,
                    v1.y - dy1 + dx1 * miter_limit,
                ));
                out.push((
                    v1.x + dx2 - dy2 * miter_limit,
                    v1.y - dy2 - dx2 * miter_limit,
                ));
            }
        }
    }
}

pub fn stroke_calc_cap(
    out: &mut StrokePoints,
    v0: VertexDist,
    v1: VertexDist,
    len: f32,
    line_cap: LineCap,
    width: f32,
    approximation_scale: f32,
) {
    out.clear();
    let dx1 = (v1.y - v0.y) / len * width;
    let dy1 = (v1.x - v0.x) / len * width;
    if line_cap != LineCap::Round {
        let (dx2, dy2) = if line_cap == LineCap::Square {
            (dy1, dx1)
        } else {
            (0.0, 0.0)
        };
        out.push((v0.x - dx1 - dx2, v0.y + dy1 - dy2));
        out.push((v0.x + dx1 - dx2, v0.y - dy1 - dy2));
    } else {
        let mut a1 = dy1.atan2(-dx1);
        let mut a2 = a1 + PI;
        let mut da = (width / (width + ((1.0 / 8.0) / approximation_scale))).acos() * 2.0;
        if da < STROKE_THETA {
            da = STROKE_THETA;
        }
        out.push((v0.x - dx1, v0.y + dy1));
        a1 += da;
        a2 -= da / 4.0;
        while a1 < a2 {
            out.push((v0.x + width * a1.cos(), v0.y + width * a1.sin()));
            a1 += da;
        }
        out.push((v0.x + dx1, v0.y - dy1));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn stroke_calc_join(
    out: &mut StrokePoints,
    v0: VertexDist,
    v1: VertexDist,
    v2: VertexDist,
    len1: f32,
    len2: f32,
    width: f32,
    line_join: LineJoin,
    inner_join: InnerJoin,
    miter_limit: f32,
    inner_miter_limit: f32,
    approximation_scale: f32,
) {
    let dx1 = width * (v1.y - v0.y) / len1;
    let dy1 = width * (v1.x - v0.x) / len1;
    let dx2 = width * (v2.y - v1.y) / len2;
    let dy2 = width * (v2.x - v1.x) / len2;
    out.clear();
    if calc_point_location(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y) > 0.0 {
        match inner_join {
            InnerJoin::Bevel => {
                out.push((v1.x + dx1, v1.y - dy1));
                out.push((v1.x + dx2, v1.y - dy2));
            }
            InnerJoin::Miter => stroke_calc_miter(
                out,
                v0,
                v1,
                v2,
                dx1,
                dy1,
                dx2,
                dy2,
                width,
                LineJoin::MiterRevert,
                inner_miter_limit,
                1.0,
            ),
            InnerJoin::Jag | InnerJoin::Round => {
                let d = (dx1 - dx2) * (dx1 - dx2) + (dy1 - dy2) * (dy1 - dy2);
                if d < len1 * len1 && d < len2 * len2 {
                    stroke_calc_miter(
                        out,
                        v0,
                        v1,
                        v2,
                        dx1,
                        dy1,
                        dx2,
                        dy2,
                        width,
                        LineJoin::MiterRevert,
                        inner_miter_limit,
                        1.0,
                    );
                } else if inner_join == InnerJoin::Jag {
                    out.push((v1.x + dx1, v1.y - dy1));
                    out.push((v1.x, v1.y));
                    out.push((v1.x + dx2, v1.y - dy2));
                } else {
                    out.push((v1.x + dx1, v1.y - dy1));
                    out.push((v1.x, v1.y));
                    stroke_calc_arc(
                        out,
                        v1.x,
                        v1.y,
                        dx2,
                        -dy2,
                        dx1,
                        -dy1,
                        width,
                        approximation_scale,
                    );
                    out.push((v1.x, v1.y));
                    out.push((v1.x + dx2, v1.y - dy2));
                }
            }
        }
    } else {
        match line_join {
            LineJoin::Miter | LineJoin::MiterRevert | LineJoin::MiterRound => stroke_calc_miter(
                out,
                v0,
                v1,
                v2,
                dx1,
                dy1,
                dx2,
                dy2,
                width,
                line_join,
                miter_limit,
                approximation_scale,
            ),
            LineJoin::Round => stroke_calc_arc(
                out,
                v1.x,
                v1.y,
                dx1,
                -dy1,
                dx2,
                -dy2,
                width,
                approximation_scale,
            ),
            LineJoin::Bevel => {
                out.push((v1.x + dx1, v1.y - dy1));
                out.push((v1.x + dx2, v1.y - dy2));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn join(inner: InnerJoin, outer: LineJoin, width: f32, limit: f32, turn: f32) -> StrokePoints {
        let mut points = vec![(999.0, 999.0)];
        stroke_calc_join(
            &mut points,
            VertexDist::new(-2.0, 0.0),
            VertexDist::new(0.0, 0.0),
            VertexDist::new(0.0, turn * 2.0),
            2.0,
            2.0,
            width,
            outer,
            inner,
            limit,
            limit,
            1.0,
        );
        points
    }

    fn assert_circle(points: &[(f32, f32)], center: (f32, f32), radius: f32) {
        assert!(points.len() > 2);
        for &(x, y) in points {
            assert!(x.is_finite() && y.is_finite());
            assert!(((x - center.0).hypot(y - center.1) - radius).abs() < 0.00001);
        }
    }

    #[test]
    fn inner_bevel_and_miter_preserve_the_two_offset_edges() {
        // Right turn: the inward offset lines meet at (-1,-1).
        assert_eq!(
            join(InnerJoin::Bevel, LineJoin::Miter, 1.0, 2.0, -1.0),
            vec![(0.0, -1.0), (-1.0, 0.0)]
        );
        for kind in [InnerJoin::Miter, InnerJoin::Jag, InnerJoin::Round] {
            assert_eq!(
                join(kind, LineJoin::Miter, 1.0, 2.0, -1.0),
                vec![(-1.0, -1.0)],
                "{kind:?}"
            );
        }
        assert_eq!(
            join(InnerJoin::Miter, LineJoin::Miter, 1.0, 0.5, -1.0),
            vec![(0.0, -1.0), (-1.0, 0.0)]
        );
    }

    #[test]
    fn wide_inner_jag_joins_through_the_corner_instead_of_crossing_edges() {
        assert_eq!(
            join(InnerJoin::Jag, LineJoin::Miter, 4.0, 2.0, -1.0),
            vec![(0.0, -4.0), (0.0, 0.0), (-4.0, 0.0)]
        );
    }

    #[test]
    fn wide_inner_round_contains_a_circular_arc_between_corner_fans() {
        let points = join(InnerJoin::Round, LineJoin::Miter, 4.0, 2.0, -1.0);
        assert_eq!(&points[..2], &[(0.0, -4.0), (0.0, 0.0)]);
        assert_eq!(&points[points.len() - 2..], &[(0.0, 0.0), (-4.0, 0.0)]);
        let arc = &points[2..points.len() - 2];
        assert_eq!(arc.first(), Some(&(-4.0, 0.0)));
        assert_eq!(arc.last(), Some(&(0.0, -4.0)));
        assert_circle(arc, (0.0, 0.0), 4.0);
        assert!(arc.iter().all(|&(x, y)| x <= 0.00001 && y <= 0.00001));
    }

    #[test]
    fn outer_miter_limits_select_clipped_bevel_or_round_geometry() {
        assert_eq!(
            join(InnerJoin::Miter, LineJoin::Miter, 1.0, 2.0, 1.0),
            vec![(1.0, -1.0)]
        );
        assert_eq!(
            join(InnerJoin::Miter, LineJoin::Miter, 1.0, 0.5, 1.0),
            vec![(0.5, -1.0), (1.0, -0.5)]
        );
        assert_eq!(
            join(InnerJoin::Miter, LineJoin::MiterRevert, 1.0, 0.5, 1.0),
            vec![(0.0, -1.0), (1.0, 0.0)]
        );
        let arc = join(InnerJoin::Miter, LineJoin::MiterRound, 1.0, 0.5, 1.0);
        assert_eq!(arc.first(), Some(&(0.0, -1.0)));
        assert_eq!(arc.last(), Some(&(1.0, 0.0)));
        assert_circle(&arc, (0.0, 0.0), 1.0);
        assert!(arc.iter().all(|&(x, y)| x >= -0.00001 && y <= 0.00001));
    }

    #[test]
    fn straight_and_reversing_segments_have_bounded_parallel_line_fallbacks() {
        for (end, expected) in [
            (2.0, vec![(0.0, -1.0)]),
            (-2.0, vec![(0.0, -1.0), (0.0, 1.0)]),
        ] {
            let mut points = Vec::new();
            stroke_calc_miter(
                &mut points,
                VertexDist::new(-2.0, 0.0),
                VertexDist::new(0.0, 0.0),
                VertexDist::new(end, 0.0),
                0.0,
                1.0,
                0.0,
                end.signum(),
                1.0,
                LineJoin::MiterRevert,
                4.0,
                1.0,
            );
            assert_eq!(points, expected);
        }
    }

    #[test]
    fn arcs_cross_the_angle_seam_and_ignore_the_sign_of_the_radius() {
        // 135 to -135 degrees takes the short arc through the negative x axis.
        let component = 2.0_f32.sqrt();
        let mut positive = Vec::new();
        stroke_calc_arc(
            &mut positive,
            7.0,
            9.0,
            -component,
            component,
            -component,
            -component,
            2.0,
            4.0,
        );
        let mut negative = Vec::new();
        stroke_calc_arc(
            &mut negative,
            7.0,
            9.0,
            -component,
            component,
            -component,
            -component,
            -2.0,
            4.0,
        );
        assert_eq!(positive, negative);
        assert_circle(&positive, (7.0, 9.0), 2.0);
        assert!(positive.iter().all(|&(x, _)| x < 7.0));
    }

    #[test]
    fn clockwise_arcs_keep_their_endpoints_and_radius() {
        let mut points = Vec::new();
        stroke_calc_arc(&mut points, 0.0, 0.0, 0.0, 2.0, 2.0, 0.0, 2.0, 4.0);
        assert_eq!(points.first(), Some(&(0.0, 2.0)));
        assert_eq!(points.last(), Some(&(2.0, 0.0)));
        assert_circle(&points, (0.0, 0.0), 2.0);
        assert!(points.iter().all(|&(x, y)| x >= -0.00001 && y >= -0.00001));
    }

    #[test]
    fn extremely_fine_round_caps_retain_a_finite_step_and_semicircle() {
        let mut points = Vec::new();
        stroke_calc_cap(
            &mut points,
            VertexDist::new(0.0, 0.0),
            VertexDist::new(10.0, 0.0),
            10.0,
            LineCap::Round,
            1.0,
            f32::MAX,
        );
        assert!((3000..3200).contains(&points.len()));
        assert_eq!(points.first(), Some(&(0.0, 1.0)));
        assert_eq!(points.last(), Some(&(0.0, -1.0)));
        assert_circle(&points, (0.0, 0.0), 1.0);
        assert!(points.iter().all(|&(x, _)| x <= 0.00001));
    }

    #[test]
    fn cap_geometry_tracks_butt_square_and_round_edges() {
        let v0 = VertexDist::new(0.0, 0.0);
        let v1 = VertexDist::new(10.0, 0.0);
        let mut out = Vec::new();
        stroke_calc_cap(&mut out, v0, v1, 10.0, LineCap::Butt, 1.0, 1.0);
        assert_eq!(out, vec![(0.0, 1.0), (0.0, -1.0)]);
        stroke_calc_cap(&mut out, v0, v1, 10.0, LineCap::Square, 1.0, 1.0);
        assert_eq!(out, vec![(-1.0, 1.0), (-1.0, -1.0)]);
        stroke_calc_cap(&mut out, v0, v1, 10.0, LineCap::Round, 1.0, 1.0);
        assert!(out.len() > 2);
        assert_eq!(out.first(), Some(&(0.0, 1.0)));
        assert_eq!(out.last(), Some(&(0.0, -1.0)));
    }
}
