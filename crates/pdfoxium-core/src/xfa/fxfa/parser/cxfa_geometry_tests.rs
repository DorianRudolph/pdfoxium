// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::{
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::fxfa::{
        fxfa_basic::{XfaPacketType, XfaUnit},
        parser::{cxfa_document::CxfaDocument, cxfa_measurement::CxfaMeasurement},
    },
};
fn node(doc: &Rc<CxfaDocument>, element: E) -> CxfaNode {
    doc.create_node(XfaPacketType::Form, element).unwrap()
}
fn measure(node: &CxfaNode, attribute: A, value: f32) {
    node.set_measure(attribute, CxfaMeasurement::new(value, XfaUnit::Pt), false);
}
fn strokes(node: &CxfaNode) -> [CxfaNode; 8] {
    node.box_get_strokes().map(Option::unwrap)
}
fn bitmap() -> CfxDibitmap {
    let mut b = CfxDibitmap::create(64, 64, FxdibFormat::Bgra, 0).unwrap();
    b.clear(0xffffffff);
    b
}
fn pixel(b: &CfxDibitmap, x: usize, y: usize) -> u32 {
    u32::from_le_bytes(
        b.scanline(y as i32).unwrap()[x * 4..x * 4 + 4]
            .try_into()
            .unwrap(),
    )
}
fn draw_box(node: &CxfaNode, b: &mut CfxDibitmap, round: bool) {
    let mut device = CfxAggDeviceDriver::new(b, false, None);
    let mut g = CfgasGeGraphics::new(&mut device);
    assert!(node.box_draw(
        &mut g,
        CfxRectF::new(8., 8., 40., 32.),
        CfxMatrix::default(),
        round
    ));
}
#[test]
fn missing_strokes_repeat_native_corner_and_edge_identities() {
    let doc = CxfaDocument::new();
    let rect = node(&doc, E::Rectangle);
    assert_eq!(rect.box_count_edges(), 0);
    let s = strokes(&rect);
    assert_eq!(rect.box_count_edges(), 1);
    for i in 1..4 {
        assert_eq!(s[i * 2], s[0]);
        assert_eq!(s[i * 2 + 1], s[1]);
    }
    // Two explicitly supplied strokes produce the source's alternating fallback:
    // third uses first; fourth uses second.
    let second_corner = rect.get_or_create_property(1, E::Corner).unwrap();
    let second_edge = rect.get_or_create_property(1, E::Edge).unwrap();
    let s = strokes(&rect);
    assert_eq!(s[2], second_corner);
    assert_eq!(s[3], second_edge);
    assert_eq!(s[4], s[0]);
    assert_eq!(s[5], s[1]);
    assert_eq!(s[6], s[2]);
    assert_eq!(s[7], s[3]);
    assert_eq!(rect.box_get_3d_style(), (V::Unknown, false, 0.));
    second_edge.set_enum(A::Stroke, V::Raised, false);
    // Only the first subsequent explicit stroke participates in Style3D.
    assert_eq!(rect.box_get_3d_style(), (V::Unknown, false, 0.));
    s[1].set_enum(A::Stroke, V::Raised, false);
    assert_eq!(rect.box_get_3d_style(), (V::Raised, true, 0.5));
}
#[test]
fn style_tolerance_presence_corner_flags_and_default_leaf_accessors() {
    let doc = CxfaDocument::new();
    let a = node(&doc, E::Corner);
    let b = node(&doc, E::Corner);
    assert!(a.stroke_same_styles(&b, false, false));
    measure(&b, A::Thickness, 0.509);
    assert!(a.stroke_same_styles(&b, false, false));
    measure(&b, A::Thickness, 0.511);
    assert!(!a.stroke_same_styles(&b, false, false));
    measure(&b, A::Thickness, 0.5);
    b.set_enum(A::Presence, V::Hidden, false);
    assert!(!a.stroke_same_styles(&b, false, false));
    assert!(a.stroke_same_styles(&b, true, false));
    measure(&b, A::Radius, 4.);
    assert!(!a.stroke_same_styles(&b, true, true));
    assert!(a.stroke_same_styles(&b, true, false));
    assert_eq!(node(&doc, E::Line).line_get_hand(), V::Even);
    assert!(!node(&doc, E::Line).line_get_slope());
    let line = node(&doc, E::Line);
    assert!(line.line_get_edge_if_exists().is_none());
    assert_eq!(line.count_children(E::Edge, false), 0);
    let caption = node(&doc, E::Caption);
    assert!(caption.caption_is_visible());
    assert!(!caption.caption_is_hidden());
    assert_eq!(caption.caption_get_placement_type(), V::Left);
    // The native -1un sentinel converts to zero when requested in points.
    assert_eq!(caption.caption_get_reserve(), 0.);
    assert!(caption.caption_get_font_if_exists().is_none());
    let para = node(&doc, E::Para);
    measure(&para, A::LineHeight, 12.);
    measure(&para, A::TextIndent, -3.);
    assert_eq!(para.para_get_horizontal_align(), V::Left);
    assert_eq!(para.para_get_vertical_align(), V::Top);
    assert_eq!(para.para_get_line_height(), 12.);
    assert_eq!(para.para_get_text_indent(), -3.);
    let margin = node(&doc, E::Margin);
    assert_eq!(margin.margin_try_left_inset(), None);
    measure(&margin, A::LeftInset, 5.);
    assert_eq!(margin.margin_try_left_inset(), Some(5.));
    assert_eq!(margin.margin_get_left_inset(), 5.);
    assert_eq!(margin.margin_get_top_inset(), 0.);
    let check = node(&doc, E::CheckButton);
    assert!(!check.check_button_is_round());
    assert!(!check.check_button_is_allow_neutral());
    let password = node(&doc, E::PasswordEdit);
    assert_eq!(password.password_edit_get_password_char(), [42]);
    let image = node(&doc, E::Image);
    assert_eq!(image.image_get_transfer_encoding(), V::Base64);
    image.image_set_href(&[97, 98]);
    assert_eq!(image.image_get_href(), [97, 98]);
    image.image_set_content_type(&[112, 110, 103]);
    assert_eq!(image.image_get_content_type(), [112, 110, 103]);
}
#[test]
fn rectangle_corner_paths_keep_inversion_and_per_edge_offsets() {
    let doc = CxfaDocument::new();
    let rect = node(&doc, E::Rectangle);
    let s = strokes(&rect);
    let bounds = CfxRectF::new(8., 10., 40., 30.);
    let plain = rectangle_fill_path(&s, bounds);
    assert_eq!(plain.get_path().points().len(), 5);
    measure(&s[0], A::Radius, 4.);
    s[0].set_enum(A::Join, V::Round, false);
    let round = rectangle_fill_path(&s, bounds);
    let points = round.get_path().points();
    assert_eq!(points[0].point, CfxPointF::new(8., 14.));
    assert_eq!(points[3].point, CfxPointF::new(12., 10.));
    s[0].set_boolean(A::Inverted, true, false);
    let inverted = rectangle_fill_path(&s, bounds);
    assert_ne!(inverted.get_path().points()[1].point, points[1].point);
    // A square top edge extends through adjacent half widths to prevent gaps.
    s[0].set_boolean(A::Inverted, false, false);
    s[0].set_enum(A::Join, V::Square, false);
    let mut edge = CfgasGePath::new();
    rectangle_get_path(&s, bounds, &mut edge, 1, true, true);
    assert_eq!(
        edge.get_path().points()[0].point,
        CfxPointF::new(11.75, 10.)
    );
    assert_eq!(
        edge.get_path().points()[1].point,
        CfxPointF::new(44.25, 10.)
    );
}
#[test]
fn arc_angles_circular_center_and_extreme_angles_remain_finite() {
    let doc = CxfaDocument::new();
    let arc = node(&doc, E::Arc);
    let bounds = CfxRectF::new(2., 4., 40., 20.);
    let ellipse = arc.box_get_path_arc_or_rounded(bounds, false);
    assert_eq!(ellipse.get_path().points().len(), 13);
    assert_eq!(
        ellipse.get_path().points()[0].point,
        CfxPointF::new(42., 14.)
    );
    let circle = arc.box_get_path_arc_or_rounded(bounds, true);
    assert_eq!(
        circle.get_path().points()[0].point,
        CfxPointF::new(32., 14.)
    );
    arc.set_integer(A::StartAngle, 90, false);
    arc.set_integer(A::SweepAngle, 90, false);
    let quarter = arc.box_get_path_arc_or_rounded(bounds, false);
    assert_eq!(quarter.get_path().points().len(), 4);
    let start = quarter.get_path().points()[0].point;
    assert!((start.x - 22.).abs() < 0.0001);
    assert!((start.y - 4.).abs() < 0.0001);
    arc.set_integer(A::StartAngle, i32::MIN, false);
    let extreme = arc.box_get_path_arc_or_rounded(bounds, false);
    assert!(extreme
        .get_path()
        .points()
        .iter()
        .all(|p| p.point.x.is_finite() && p.point.y.is_finite()));
}
#[test]
fn solid_rect_border_hand_presence_and_circle_render_to_agg() {
    let doc = CxfaDocument::new();
    let rect = node(&doc, E::Rectangle);
    let fill = rect.box_get_or_create_fill().unwrap();
    fill.fill_set_color(0xffff0000);
    let edge = rect.box_get_edge(0).unwrap();
    edge.stroke_set_color(0xff0000ff);
    measure(&edge, A::Thickness, 4.);
    let mut b = bitmap();
    draw_box(&rect, &mut b, false);
    assert_eq!(pixel(&b, 20, 20), 0xffff0000);
    assert_eq!(pixel(&b, 8, 20), 0xff0000ff);
    assert_eq!(pixel(&b, 5, 20), 0xffffffff);
    rect.set_enum(A::Hand, V::Left, false);
    let mut b = bitmap();
    draw_box(&rect, &mut b, false);
    assert_eq!(pixel(&b, 5, 20), 0xff0000ff);
    rect.set_enum(A::Presence, V::Hidden, false);
    let mut b = bitmap();
    draw_box(&rect, &mut b, false);
    assert!(b
        .buffer()
        .chunks_exact(4)
        .all(|p| p == [255, 255, 255, 255]));
    rect.set_enum(A::Presence, V::Visible, false);
    edge.set_enum(A::Presence, V::Hidden, false);
    let mut b = bitmap();
    draw_box(&rect, &mut b, true);
    assert_eq!(pixel(&b, 28, 24), 0xffff0000);
    assert_eq!(pixel(&b, 8, 8), 0xffffffff);
}
#[test]
fn native_fill_modes_produce_shading_hatch_and_stipple_pixels() {
    let doc = CxfaDocument::new();
    for kind in [E::Linear, E::Radial, E::Pattern, E::Stipple] {
        let fill = node(&doc, E::Fill);
        fill.fill_set_color(0xffff0000);
        let style = fill.get_or_create_property(0, kind).unwrap();
        style
            .get_or_create_property(0, E::Color)
            .unwrap()
            .color_set_value(0xff0000ff);
        if kind == E::Stipple {
            style.set_integer(A::Rate, 50, false);
        }
        let mut b = bitmap();
        let mut path = CfgasGePath::new();
        path.add_rectangle(8., 8., 40., 32.);
        {
            let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
            let mut g = CfgasGeGraphics::new(&mut device);
            assert!(fill.fill_draw(
                &mut g,
                &path,
                CfxRectF::new(8., 8., 40., 32.),
                CfxMatrix::default()
            ));
        }
        assert_eq!(pixel(&b, 2, 2), 0xffffffff);
        match kind {
            E::Linear => {
                assert_eq!(pixel(&b, 8, 20), 0xffff0000);
                assert_ne!(pixel(&b, 40, 20), 0xffff0000);
            }
            E::Radial => {
                assert_eq!(pixel(&b, 28, 24), 0xffff0000);
                assert_ne!(pixel(&b, 10, 10), 0xffff0000);
            }
            E::Pattern => {
                let pixels = (8..40)
                    .flat_map(|y| (8..48).map(move |x| (x, y)))
                    .map(|(x, y)| pixel(&b, x, y))
                    .collect::<Vec<_>>();
                assert!(pixels.contains(&0xffff0000));
                assert!(pixels.contains(&0xff0000ff));
            }
            E::Stipple => {
                let p = pixel(&b, 20, 20);
                assert_eq!(p & 255, 255);
                assert!((p >> 16) & 255 >= 127);
                assert!((p >> 16) & 255 <= 129);
            }
            _ => unreachable!(),
        }
    }
}
#[test]
fn all_3d_rect_styles_and_rounded_border_shading_are_connected() {
    let doc = CxfaDocument::new();
    for style in [V::Lowered, V::Raised, V::Etched, V::Embossed] {
        let border = node(&doc, E::Border);
        let edge = border.box_get_edge(0).unwrap();
        edge.set_enum(A::Stroke, style, false);
        measure(&edge, A::Thickness, 4.);
        assert_eq!(border.box_get_3d_style(), (style, true, 4.));
        let mut b = bitmap();
        draw_box(&border, &mut b, false);
        assert_eq!(
            pixel(&b, 9, 9),
            if matches!(style, V::Lowered | V::Raised) {
                0xff000000
            } else {
                0xff808080
            }
        );
        let mut round = bitmap();
        draw_box(&border, &mut round, true);
        assert_ne!(pixel(&round, 28, 8), 0xffffffff);
        assert_eq!(pixel(&round, 8, 8), 0xffffffff);
    }
}

#[test]
fn dashed_stroke_uses_width_scaled_lengths_and_restores_graphics_state() {
    let doc = CxfaDocument::new();
    let edge = node(&doc, E::Edge);
    edge.set_enum(A::Stroke, V::Dashed, false);
    measure(&edge, A::Thickness, 2.);
    let mut b = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
        let mut g = CfgasGeGraphics::new(&mut device);
        g.set_stroke_color(&CfgasGeColor::Solid(0xffff0000));
        g.set_line_width(4.);
        let mut path = CfgasGePath::new();
        path.add_line(CfxPointF::new(4., 20.), CfxPointF::new(60., 20.));
        assert!(edge.stroke_draw(&mut g, &path, CfxMatrix::default()));
        let mut after = CfgasGePath::new();
        after.add_line(CfxPointF::new(4., 30.), CfxPointF::new(60., 30.));
        assert!(g.stroke_path(&after, CfxMatrix::default()));
    }
    assert_eq!(pixel(&b, 8, 20), 0xff000000);
    assert_eq!(pixel(&b, 15, 20), 0xffffffff);
    assert_eq!(pixel(&b, 20, 20), 0xff000000);
    assert_eq!(pixel(&b, 15, 30), 0xffff0000);
    assert_eq!(pixel(&b, 15, 31), 0xffff0000);
}
