// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Connected geometry and drawing from CXFA_Box, Rectangle, Stroke, Fill,
//! Linear, Radial, Pattern and Stipple. Node schema supplies their constructors.
use super::cxfa_node::CxfaNode;
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    fxge::{cfx_fillrenderoptions::FillType, cfx_graphstatedata::LineCap},
    xfa::{
        fgas::graphics::{
            cfgas_gecolor::CfgasGeColor,
            cfgas_gegraphics::CfgasGeGraphics,
            cfgas_gepath::CfgasGePath,
            cfgas_gepattern::{CfgasGePattern, HatchStyle},
            cfgas_geshading::CfgasGeShading,
        },
        fxfa::fxfa_basic::{XfaAttribute as A, XfaAttributeValue as V, XfaElement as E},
    },
};
use std::{
    f32::consts::{FRAC_PI_2, PI},
    rc::Rc,
};

type Graphics<'a, 'b, 'c> = CfgasGeGraphics<'a, 'b, 'c>;

/// XFA_StrokeTypeSetLineDash. Dash lengths are scaled by the graphics state
/// when act-on-dash is enabled, as in the native stroke path.
pub fn stroke_type_set_line_dash(graphics: &mut Graphics<'_, '_, '_>, stroke: V, cap: V) {
    let gap = if cap == V::Butt { 1.0 } else { 2.0 };
    let dash = match stroke {
        V::DashDot => vec![4., gap, 2., gap],
        V::DashDotDot => vec![4., gap, 2., gap, 2., gap],
        V::Dashed => vec![5., gap],
        V::Dotted => vec![2., gap],
        _ => {
            graphics.set_solid_line_dash();
            return;
        }
    };
    graphics.set_line_dash(dash);
}

impl CxfaNode {
    pub fn stroke_draw(
        &self,
        graphics: &mut Graphics<'_, '_, '_>,
        path: &CfgasGePath,
        matrix: CfxMatrix,
    ) -> bool {
        if !self.stroke_is_visible() {
            return true;
        }
        let mut thickness = self.stroke_get_thickness();
        if thickness < 0.001 {
            return true;
        }
        if self.stroke_is_corner() && thickness > 2. * self.stroke_get_radius() {
            thickness = 2. * self.stroke_get_radius();
        }
        // A negative corner radius must not leave a previous state's width in
        // use when the graphics adapter rejects a negative line width.
        if !thickness.is_finite() || thickness < 0. {
            return false;
        }
        let mut g = graphics.state_restorer();
        g.set_line_width(thickness);
        g.enable_act_on_dash();
        g.set_line_cap(LineCap::BUTT);
        stroke_type_set_line_dash(&mut g, self.stroke_get_stroke_type(), V::Butt);
        g.set_stroke_color(&CfgasGeColor::Solid(self.stroke_get_color()));
        g.stroke_path(path, matrix)
    }

    pub fn fill_draw(
        &self,
        graphics: &mut Graphics<'_, '_, '_>,
        path: &CfgasGePath,
        rect: CfxRectF,
        matrix: CfxMatrix,
    ) -> bool {
        let mut g = graphics.state_restorer();
        let kind = self.fill_get_type();
        let color = match kind {
            E::Radial | E::Linear | E::Pattern | E::Stipple => {
                let Some(child) = self.get_or_create_property(0, kind) else {
                    return false;
                };
                child.fill_style_color(self.fill_get_color(false), rect)
            }
            _ => CfgasGeColor::Solid(self.fill_get_color(false)),
        };
        g.set_fill_color(&color);
        g.fill_path(path, FillType::Winding, matrix)
    }

    /// Fill-style nodes own only the small shading/pattern resource. No node
    /// collection borrow or document borrow is retained while drawing.
    fn fill_style_color(&self, mut start: u32, rect: CfxRectF) -> CfgasGeColor {
        let mut end = self
            .fill_style_get_color_if_exists()
            .map_or(0xff000000, |color| {
                color.color_get_value_or_default(0xff000000)
            });
        match self.get_element_type() {
            E::Linear => {
                let (begin, end_point) = match self.linear_get_type() {
                    V::ToRight => (rect.top_left(), rect.top_right()),
                    V::ToBottom => (rect.top_left(), rect.bottom_left()),
                    V::ToLeft => (rect.top_right(), rect.top_left()),
                    V::ToTop => (rect.bottom_left(), rect.top_left()),
                    _ => (CfxPointF::default(), CfxPointF::default()),
                };
                CfgasGeColor::Shading(Rc::new(CfgasGeShading::axial(
                    begin, end_point, false, false, start, end,
                )))
            }
            E::Radial => {
                if !self.radial_is_to_edge() {
                    std::mem::swap(&mut start, &mut end);
                }
                CfgasGeColor::Shading(Rc::new(CfgasGeShading::radial(
                    rect.center(),
                    rect.center(),
                    0.,
                    rect.width.hypot(rect.height) / 2.,
                    true,
                    true,
                    start,
                    end,
                )))
            }
            E::Pattern => {
                let hatch = match self.pattern_get_type() {
                    V::CrossDiagonal => HatchStyle::DiagonalCross,
                    V::DiagonalLeft => HatchStyle::ForwardDiagonal,
                    V::DiagonalRight => HatchStyle::BackwardDiagonal,
                    V::Horizontal => HatchStyle::Horizontal,
                    V::Vertical => HatchStyle::Vertical,
                    _ => HatchStyle::Cross,
                };
                CfgasGeColor::Pattern {
                    pattern: Rc::new(CfgasGePattern::new(hatch, end, start)),
                    argb: 0,
                }
            }
            E::Stipple => {
                let rate = self.stipple_get_rate();
                let rate = if rate == 0 { 100 } else { rate };
                // Clamp malformed opacity instead of overflowing signed C++
                // multiplication or wrapping it into unrelated ARGB bits.
                let alpha = (i64::from(rate) * i64::from(end >> 24) / 100).clamp(0, 255) as u32;
                CfgasGeColor::Solid((end & 0x00ffffff) | (alpha << 24))
            }
            _ => CfgasGeColor::Solid(start),
        }
    }

    fn box_strokes_internal(&self, preserve_absent: bool) -> [Option<Self>; 8] {
        let mut strokes: [Option<Self>; 8] = std::array::from_fn(|_| None);
        for i in 0..4 {
            for (offset, kind) in [(0, E::Corner), (1, E::Edge)] {
                let j = i * 2 + offset;
                let child = if i == 0 {
                    self.get_or_create_property(0, kind)
                } else {
                    self.get_property(i as i32, kind).0
                };
                strokes[j] = if child.is_some() || i == 0 || preserve_absent {
                    child
                } else {
                    strokes[if i == 3 { 2 + offset } else { offset }].clone()
                };
            }
        }
        strokes
    }
    pub fn box_get_strokes(&self) -> [Option<Self>; 8] {
        self.box_strokes_internal(false)
    }
    pub fn box_get_3d_style(&self) -> (V, bool, f32) {
        if self.get_element_type() == E::Arc {
            return (V::Unknown, false, 0.);
        }
        let strokes = self.box_strokes_internal(true);
        let mut stroke = strokes[0].as_ref();
        // Native Style3D chooses the first subsequent explicit stroke, using
        // its style when it differs from the first corner's style.
        for found in strokes[1..].iter().flatten() {
            if stroke.is_none_or(|first| {
                first.stroke_get_stroke_type() != found.stroke_get_stroke_type()
            }) {
                stroke = Some(found);
            }
            break;
        }
        let Some(stroke) = stroke else {
            return (V::Unknown, false, 0.);
        };
        let kind = stroke.stroke_get_stroke_type();
        if matches!(kind, V::Lowered | V::Raised | V::Etched | V::Embossed) {
            (
                kind,
                stroke.stroke_is_visible(),
                stroke.stroke_get_thickness(),
            )
        } else {
            (V::Unknown, false, 0.)
        }
    }

    pub fn box_get_path_arc_or_rounded(
        &self,
        mut rect: CfxRectF,
        force_round: bool,
    ) -> CfgasGePath {
        let mut a = rect.width / 2.;
        let mut b = rect.height / 2.;
        if self.get_boolean(A::Circular) || force_round {
            a = a.min(b);
            b = a;
        }
        let center = rect.center();
        rect = CfxRectF::new(center.x - a, center.y - b, a + a, b + b);
        let start = self.try_integer(A::StartAngle, false);
        let sweep = self.try_integer(A::SweepAngle, false);
        let mut path = CfgasGePath::new();
        if start.is_none() && sweep.is_none() {
            path.add_ellipse(rect);
        } else {
            path.add_arc(
                rect.top_left(),
                rect.size(),
                -(start.unwrap_or(0) as f32) * PI / 180.,
                -(sweep.unwrap_or(360) as f32) * PI / 180.,
            );
        }
        path
    }

    pub fn box_draw(
        &self,
        graphics: &mut Graphics<'_, '_, '_>,
        rect: CfxRectF,
        matrix: CfxMatrix,
        force_round: bool,
    ) -> bool {
        if self.box_get_presence() != V::Visible {
            return true;
        }
        let kind = self.get_element_type();
        if !matches!(kind, E::Arc | E::Border | E::Rectangle) {
            return false;
        }
        // Keep the arena alive across property creation callbacks; handles
        // remain weak and do not create a node/document ownership cycle.
        let _store = self.retain_store();
        let strokes = if !force_round && kind != E::Arc {
            let strokes = self.box_get_strokes();
            if strokes.iter().any(Option::is_none) {
                return false;
            }
            Some(strokes.map(Option::unwrap))
        } else {
            None
        };
        if let Some(fill) = self
            .get_property(0, E::Fill)
            .0
            .filter(Self::fill_is_visible)
        {
            let mut fill_rect = rect;
            let mut path = if let Some(strokes) = &strokes {
                rectangle_fill_path(strokes, rect)
            } else {
                let thickness = self
                    .box_get_edge(0)
                    .map_or(0., |e| e.stroke_get_thickness())
                    .max(0.);
                adjust_hand(&mut fill_rect, self.box_get_hand(), thickness / 2.);
                self.box_get_path_arc_or_rounded(fill_rect, force_round)
            };
            path.close();
            if !fill.fill_draw(graphics, &path, fill_rect, matrix) {
                return false;
            }
        }
        if let Some(strokes) = &strokes {
            self.rectangle_draw(strokes, graphics, rect, matrix)
        } else {
            self.box_stroke_arc_or_rounded(graphics, rect, matrix, force_round)
        }
    }

    fn box_stroke_arc_or_rounded(
        &self,
        graphics: &mut Graphics<'_, '_, '_>,
        mut rect: CfxRectF,
        matrix: CfxMatrix,
        force_round: bool,
    ) -> bool {
        let Some(edge) = self.box_get_edge(0).filter(Self::stroke_is_visible) else {
            return true;
        };
        let (style, visible, thickness) = self.box_get_3d_style();
        let lowered = style != V::Unknown && visible && thickness >= 0.001;
        let half = (edge.stroke_get_thickness() / 2.).max(0.);
        adjust_hand(&mut rect, self.box_get_hand(), half);
        if !force_round || !lowered {
            return half < 0.001
                || edge.stroke_draw(
                    graphics,
                    &self.box_get_path_arc_or_rounded(rect, force_round),
                    matrix,
                );
        }
        let mut g = graphics.state_restorer();
        g.set_line_width(half);
        let a = (rect.width / 2.).min(rect.height / 2.);
        let center = rect.center();
        rect = CfxRectF::new(center.x - a, center.y - a, a + a, a + a);
        for (i, (start, color)) in [
            (3. * PI / 4., 0xff808080),
            (-PI / 4., 0xffffffff),
            (3. * PI / 4., 0xff404040),
            (-PI / 4., 0xffc0c0c0),
        ]
        .into_iter()
        .enumerate()
        {
            if i == 2 {
                rect.deflate_xy(half, half);
            }
            let mut path = CfgasGePath::new();
            path.add_arc(rect.top_left(), rect.size(), start, PI);
            g.set_stroke_color(&CfgasGeColor::Solid(color));
            if !g.stroke_path(&path, matrix) {
                return false;
            }
        }
        true
    }

    fn rectangle_draw(
        &self,
        strokes: &[Self; 8],
        graphics: &mut Graphics<'_, '_, '_>,
        mut rect: CfxRectF,
        matrix: CfxMatrix,
    ) -> bool {
        if !(0..4).any(|i| strokes[i * 2 + 1].stroke_is_visible()) {
            return true;
        }
        let sign = match self.box_get_hand() {
            V::Left => 1.,
            V::Right => -1.,
            _ => 0.,
        };
        for i in (1..8).step_by(2) {
            let delta = strokes[i].stroke_get_thickness().max(0.) / 2. * sign;
            match i {
                1 => {
                    rect.top -= delta;
                    rect.height += delta;
                }
                3 => rect.width += delta,
                5 => rect.height += delta,
                7 => {
                    rect.left -= delta;
                    rect.width += delta;
                }
                _ => unreachable!(),
            }
        }
        let (style, visible, thickness) = self.box_get_3d_style();
        if style != V::Unknown {
            return !visible
                || thickness < 0.001
                || rectangle_stroke_3d(graphics, rect, thickness, matrix, style);
        }
        let close = all_stroke_styles(strokes);
        let same = close && same_square_corners(strokes);
        let mut start = true;
        let mut path = CfgasGePath::new();
        for i in 0..8 {
            let stroke = &strokes[i];
            if stroke.stroke_get_radius() < 0. {
                if !path.is_empty() {
                    if !stroke.stroke_draw(graphics, &path, matrix) {
                        return false;
                    }
                    path.clear();
                }
                start = true;
                continue;
            }
            rectangle_get_path(strokes, rect, &mut path, i, start, !same);
            start = !stroke.stroke_same_styles(&strokes[(i + 1) % 8], false, false);
            if start {
                if !stroke.stroke_draw(graphics, &path, matrix) {
                    return false;
                }
                path.clear();
            }
        }
        if !path.is_empty() {
            if close {
                path.close();
            }
            if !strokes[7].stroke_draw(graphics, &path, matrix) {
                return false;
            }
        }
        true
    }
}

fn adjust_hand(rect: &mut CfxRectF, hand: V, half: f32) {
    match hand {
        V::Left => rect.inflate_xy(half, half),
        V::Right => rect.deflate_xy(half, half),
        _ => {}
    }
}
fn all_stroke_styles(strokes: &[CxfaNode; 8]) -> bool {
    strokes
        .windows(2)
        .all(|p| p[0].stroke_same_styles(&p[1], false, false))
}
fn same_square_corners(strokes: &[CxfaNode; 8]) -> bool {
    (2..8)
        .step_by(2)
        .all(|i| strokes[i - 2].stroke_same_styles(&strokes[i], true, true))
        && !strokes[0].stroke_is_inverted()
        && strokes[0].stroke_get_join_type() == V::Square
}

/// Each clockwise corner's native coordinate basis, shared by the fill and
/// outline algorithms; angle directions are in the graphics' downward y axis.
struct CornerBasis {
    p1: CfxPointF,
    p2: CfxPointF,
    vx: f32,
    vy: f32,
    nx: f32,
    ny: f32,
    sx: f32,
    sy: f32,
}
fn corner_basis(rect: CfxRectF, index: usize, round: bool, inverted: bool) -> CornerBasis {
    let (p1, p2, vx, vy, nx, ny, square_x, square_y, angle) = match index / 2 {
        0 => (
            rect.top_left(),
            rect.top_right(),
            1.,
            1.,
            -1.,
            0.,
            1.,
            0.,
            if inverted { FRAC_PI_2 } else { PI },
        ),
        1 => (
            rect.top_right(),
            rect.bottom_right(),
            -1.,
            1.,
            0.,
            -1.,
            0.,
            1.,
            if inverted { PI } else { 3. * FRAC_PI_2 },
        ),
        2 => (
            rect.bottom_right(),
            rect.bottom_left(),
            -1.,
            -1.,
            1.,
            0.,
            -1.,
            0.,
            if inverted { 3. * FRAC_PI_2 } else { 0. },
        ),
        _ => (
            rect.bottom_left(),
            rect.top_left(),
            1.,
            -1.,
            0.,
            1.,
            0.,
            -1.,
            if inverted { 0. } else { FRAC_PI_2 },
        ),
    };
    CornerBasis {
        p1,
        p2,
        vx,
        vy,
        nx,
        ny,
        sx: if round { angle } else { square_x },
        sy: if round { FRAC_PI_2 } else { square_y },
    }
}
fn add_corner(
    path: &mut CfgasGePath,
    b: &CornerBasis,
    radius: f32,
    round: bool,
    inverted: bool,
    ox: f32,
    oy: f32,
) {
    if round {
        let angle = b.sx - if radius < 0. { PI } else { 0. };
        let sweep = if inverted { -b.sy } else { b.sy };
        let mut rect = CfxRectF::new(
            b.p1.x + ox * 2.,
            b.p1.y + oy * 2.,
            radius * 2. * b.vx - ox * 2.,
            radius * 2. * b.vy - oy * 2.,
        );
        rect.normalize();
        if inverted {
            rect.offset(-radius * b.vx, -radius * b.vy);
        }
        path.arc_to(rect.top_left(), rect.size(), angle, sweep);
    } else {
        let p = if inverted {
            CfxPointF::new(b.p1.x + radius * b.vx, b.p1.y + radius * b.vy)
        } else {
            b.p1
        };
        path.line_to(p);
        path.line_to(CfxPointF::new(
            b.p1.x + radius * b.sx + ox,
            b.p1.y + radius * b.sy + oy,
        ));
    }
}
fn rectangle_fill_path(strokes: &[CxfaNode; 8], rect: CfxRectF) -> CfgasGePath {
    let mut path = CfgasGePath::new();
    if all_stroke_styles(strokes) && same_square_corners(strokes) {
        path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
        return path;
    }
    for i in (0..8).step_by(2) {
        let corner = &strokes[i];
        let next = &strokes[(i + 2) % 8];
        let radius = corner.stroke_get_radius();
        let round = corner.stroke_get_join_type() == V::Round;
        let inverted = corner.stroke_is_inverted();
        let b = corner_basis(rect, i, round, inverted);
        if i == 0 {
            path.move_to(CfxPointF::new(b.p1.x, b.p1.y + radius));
        }
        add_corner(&mut path, &b, radius, round, inverted, 0., 0.);
        path.line_to(CfxPointF::new(
            b.p2.x + next.stroke_get_radius() * b.nx,
            b.p2.y + next.stroke_get_radius() * b.ny,
        ));
    }
    path
}
fn rectangle_get_path(
    strokes: &[CxfaNode; 8],
    rect: CfxRectF,
    path: &mut CfgasGePath,
    index: usize,
    start: bool,
    corners: bool,
) {
    let n = index & !1;
    let corner = &strokes[n];
    let next = &strokes[(n + 2) % 8];
    let stroke = &strokes[index];
    let r1 = if corners {
        corner.stroke_get_radius()
    } else {
        0.
    };
    let r2 = if corners {
        next.stroke_get_radius()
    } else {
        0.
    };
    let inverted = corner.stroke_is_inverted();
    let round = corner.stroke_get_join_type() == V::Round;
    let mut before = 0.;
    let mut after = 0.;
    if stroke.stroke_is_corner() {
        if stroke.stroke_is_inverted() {
            let previous = &strokes[(index + 7) % 8];
            let next = &strokes[index + 1];
            if !stroke.stroke_same_styles(previous, false, false) {
                before = previous.stroke_get_thickness() / 2.;
            }
            if !stroke.stroke_same_styles(next, false, false) {
                after = next.stroke_get_thickness() / 2.;
            }
        }
    } else if !round && !inverted {
        before = strokes[(index + 6) % 8].stroke_get_thickness() / 2.;
        after = strokes[(index + 2) % 8].stroke_get_thickness() / 2.;
    }
    let b = corner_basis(rect, n, round, inverted);
    let (p, ox, oy, ex, ey) = match index {
        0 => (
            CfxPointF::new(b.p1.x - before, b.p1.y + r1),
            0.,
            -after,
            0.,
            0.,
        ),
        1 => (
            CfxPointF::new(b.p1.x + r1 - before, b.p1.y),
            0.,
            0.,
            after,
            0.,
        ),
        2 => (
            CfxPointF::new(b.p1.x - r1, b.p1.y - before),
            after,
            0.,
            0.,
            0.,
        ),
        3 => (
            CfxPointF::new(b.p1.x, b.p1.y + r1 - before),
            0.,
            0.,
            0.,
            after,
        ),
        4 => (
            CfxPointF::new(b.p1.x + before, b.p1.y - r1),
            0.,
            after,
            0.,
            0.,
        ),
        5 => (
            CfxPointF::new(b.p1.x - r1 + before, b.p1.y),
            0.,
            0.,
            -after,
            0.,
        ),
        6 => (
            CfxPointF::new(b.p1.x + r1, b.p1.y + before),
            -after,
            0.,
            0.,
            0.,
        ),
        _ => (
            CfxPointF::new(b.p1.x, b.p1.y - r1 + before),
            0.,
            0.,
            0.,
            -after,
        ),
    };
    if start {
        path.move_to(p);
    }
    if index & 1 != 0 {
        path.line_to(CfxPointF::new(
            b.p2.x + r2 * b.nx + ex,
            b.p2.y + r2 * b.ny + ey,
        ));
    } else {
        add_corner(path, &b, r1, round, inverted, ox, oy);
    }
}

fn stroke_rect(
    g: &mut Graphics<'_, '_, '_>,
    r: CfxRectF,
    w: f32,
    matrix: CfxMatrix,
    lt: u32,
    rb: u32,
) -> bool {
    let right = r.right();
    let bottom = r.bottom();
    let polygons = [
        (
            lt,
            [
                (r.left, bottom),
                (r.left, r.top),
                (right, r.top),
                (right - w, r.top + w),
                (r.left + w, r.top + w),
                (r.left + w, bottom - w),
                (r.left, bottom),
            ],
        ),
        (
            rb,
            [
                (right, r.top),
                (right, bottom),
                (r.left, bottom),
                (r.left + w, bottom - w),
                (right - w, bottom - w),
                (right - w, r.top + w),
                (right, r.top),
            ],
        ),
    ];
    for (color, points) in polygons {
        let mut path = CfgasGePath::new();
        path.move_to(CfxPointF::new(points[0].0, points[0].1));
        for (x, y) in &points[1..] {
            path.line_to(CfxPointF::new(*x, *y));
        }
        g.set_fill_color(&CfgasGeColor::Solid(color));
        if !g.fill_path(&path, FillType::Winding, matrix) {
            return false;
        }
    }
    true
}
fn rectangle_stroke_3d(
    g: &mut Graphics<'_, '_, '_>,
    rect: CfxRectF,
    thickness: f32,
    matrix: CfxMatrix,
    style: V,
) -> bool {
    let half = thickness / 2.;
    let mut inner = rect;
    inner.deflate_xy(half, half);
    match style {
        V::Lowered | V::Raised => {
            let mut path = CfgasGePath::new();
            path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
            path.add_rectangle(inner.left, inner.top, inner.width, inner.height);
            g.set_fill_color(&CfgasGeColor::Solid(0xff000000));
            if !g.fill_path(&path, FillType::EvenOdd, matrix) {
                return false;
            }
            let (lt, rb) = if style == V::Lowered {
                (0xff808080, 0xffc0c0c0)
            } else {
                (0xffffffff, 0xff808080)
            };
            stroke_rect(g, inner, half, matrix, lt, rb)
        }
        V::Etched => {
            stroke_rect(g, rect, thickness, matrix, 0xff808080, 0xffffffff)
                && stroke_rect(g, inner, half, matrix, 0xffffffff, 0xff808080)
        }
        V::Embossed => {
            stroke_rect(g, rect, thickness, matrix, 0xff808080, 0xff000000)
                && stroke_rect(g, inner, half, matrix, 0xff000000, 0xff808080)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "cxfa_geometry_tests.rs"]
mod tests;
