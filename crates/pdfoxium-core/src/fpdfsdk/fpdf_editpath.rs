// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Path construction, editing and inspection from `fpdfsdk/fpdf_editpath.cpp`.
//! Public handles and output-pointer validation belong to pdfoxium-capi.

use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use crate::fxcrt::fx_coordinates::CfxPointF;
use crate::fxge::cfx_path::{PathPoint, PointType};

pub fn create_new_path(x: f32, y: f32) -> CpdfPathObject {
    let mut object = CpdfPathObject::default();
    object
        .mutable_path()
        .append_point(CfxPointF::new(x, y), PointType::Move);
    object.base_mut().set_default_states();
    object
}

pub fn create_new_rect(x: f32, y: f32, width: f32, height: f32) -> CpdfPathObject {
    let mut object = CpdfPathObject::default();
    object
        .mutable_path()
        .append_rect(x, y, x + width, y + height);
    object.base_mut().set_default_states();
    object
}

pub fn segments(object: &CpdfPathObject) -> &[PathPoint] {
    object.path().get_object().map_or(&[], |path| path.points())
}

pub fn move_to(object: &mut CpdfPathObject, x: f32, y: f32) {
    object
        .mutable_path()
        .append_point(CfxPointF::new(x, y), PointType::Move);
    object.base_mut().set_dirty(true);
}

pub fn line_to(object: &mut CpdfPathObject, x: f32, y: f32) {
    object
        .mutable_path()
        .append_point(CfxPointF::new(x, y), PointType::Line);
    object.base_mut().set_dirty(true);
}

pub fn bezier_to(
    object: &mut CpdfPathObject,
    first: CfxPointF,
    second: CfxPointF,
    endpoint: CfxPointF,
) {
    let path = object.mutable_path();
    path.append_point(first, PointType::Bezier);
    path.append_point(second, PointType::Bezier);
    path.append_point(endpoint, PointType::Bezier);
    object.base_mut().set_dirty(true);
}

pub fn bezier_control_points(
    object: &CpdfPathObject,
    index: usize,
) -> Option<(CfxPointF, CfxPointF)> {
    let points = segments(object);
    if points.get(index)?.point_type != PointType::Bezier {
        return None;
    }
    // Consecutive cubic curves form a run of three Bezier points per curve,
    // with the endpoint last. Only endpoint indexes have control points.
    let mut run_start_index = index;
    while run_start_index > 0 && points[run_start_index - 1].point_type == PointType::Bezier {
        run_start_index -= 1;
    }
    if (index - run_start_index) % 3 != 2 {
        return None;
    }
    Some((points[index - 2].point, points[index - 1].point))
}

pub fn close(object: &mut CpdfPathObject) -> bool {
    if segments(object).is_empty() {
        return false;
    }
    object.mutable_path().close_path();
    object.base_mut().set_dirty(true);
    true
}

pub fn set_draw_mode(object: &mut CpdfPathObject, fill_mode: i32, stroke: bool) {
    object.set_stroke(stroke);
    match fill_mode {
        1 => object.set_alternate_filltype(),
        2 => object.set_winding_filltype(),
        _ => object.set_no_filltype(),
    }
    object.base_mut().set_dirty(true);
}

pub fn draw_mode(object: &CpdfPathObject) -> (i32, bool) {
    let fill_mode = if object.has_alternate_filltype() {
        1
    } else if object.has_winding_filltype() {
        2
    } else {
        0
    };
    (fill_mode, object.stroke())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_cubic_control_points_require_endpoint_indexes() {
        let mut path = create_new_path(0.0, 0.0);
        for y in [1.0, 2.0] {
            bezier_to(
                &mut path,
                CfxPointF::new(1.0, y),
                CfxPointF::new(2.0, y),
                CfxPointF::new(3.0, y),
            );
        }
        assert_eq!(segments(&path).len(), 7);
        for index in [0, 1, 2, 4, 5, 7, usize::MAX] {
            assert_eq!(bezier_control_points(&path, index), None);
        }
        assert_eq!(
            bezier_control_points(&path, 3),
            Some((CfxPointF::new(1.0, 1.0), CfxPointF::new(2.0, 1.0)))
        );
        assert_eq!(
            bezier_control_points(&path, 6),
            Some((CfxPointF::new(1.0, 2.0), CfxPointF::new(2.0, 2.0)))
        );
        line_to(&mut path, 10.0, 10.0);
        bezier_to(
            &mut path,
            CfxPointF::new(11.0, 11.0),
            CfxPointF::new(12.0, 12.0),
            CfxPointF::new(13.0, 13.0),
        );
        assert_eq!(
            bezier_control_points(&path, 10),
            Some((CfxPointF::new(11.0, 11.0), CfxPointF::new(12.0, 12.0)))
        );
    }

    #[test]
    fn draw_modes_and_closure_preserve_editing_state() {
        let mut path = CpdfPathObject::default();
        assert!(!close(&mut path));
        move_to(&mut path, 4.0, 5.0);
        line_to(&mut path, 6.0, 7.0);
        assert!(close(&mut path));
        assert!(segments(&path).last().unwrap().close_figure);
        assert!(path.base().is_dirty());
        for (input, expected) in [(1, 1), (2, 2), (0, 0), (-1, 0), (9, 0)] {
            set_draw_mode(&mut path, input, true);
            assert_eq!(draw_mode(&path), (expected, true));
        }
        set_draw_mode(&mut path, 2, false);
        assert_eq!(draw_mode(&path), (2, false));
    }

    #[test]
    fn rectangle_and_path_have_default_graphic_states() {
        let path = create_new_path(7.0, 8.0);
        assert_eq!(segments(&path)[0].point, CfxPointF::new(7.0, 8.0));
        assert_eq!(segments(&path)[0].point_type, PointType::Move);
        let rect = create_new_rect(2.0, 3.0, 4.0, 5.0);
        assert_eq!(segments(&rect).len(), 5);
        assert!(segments(&rect).last().unwrap().close_figure);
        assert_eq!(rect.base().graph_state().get_line_width(), 1.0);
        assert_eq!(draw_mode(&rect), (0, false));
    }
}
