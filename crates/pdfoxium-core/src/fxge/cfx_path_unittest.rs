// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/cfx_path_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::*;

#[test]
fn BasicTest() {
    let mut path = CfxPath::new();
    path.append_rect(1.0, 2.0, 3.0, 5.0);
    assert_eq!(5usize, path.points().len());
    assert!(path.is_rect());
    let mut rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0), path.bounding_box());

    let kScaleMatrix = CfxMatrix::new(1.0, 0.0, 0.0, 2.0, 60.0, 70.0);
    rect = path.rect(Some(&kScaleMatrix));
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(61.0, 74.0, 63.0, 80.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0), path.bounding_box());

    path.clear();
    assert_eq!(0usize, path.points().len());
    assert!(!path.is_rect());
    assert_eq!(CfxFloatRect::default(), path.bounding_box());

    // 4 points without a closed path makes a rect.
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(1.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(1.0, 0.0), PointType::Line);
    assert_eq!(4usize, path.points().len());
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0), path.bounding_box());

    // 4 points with a closed path also makes a rect.
    path.close_path();
    assert_eq!(4usize, path.points().len());
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0), path.bounding_box());

    path.transform(kScaleMatrix);
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(60.0, 70.0, 61.0, 72.0), rect.unwrap());
    assert_eq!(
        CfxFloatRect::new(60.0, 70.0, 61.0, 72.0),
        path.bounding_box()
    );

    path.clear();
    path.append_float_rect(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0));
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0), path.bounding_box());
}

#[test]
fn ShearTransform() {
    let mut path = CfxPath::new();
    path.append_rect(1.0, 2.0, 3.0, 5.0);

    let kShearMatrix = CfxMatrix::new(1.0, 2.0, 0.0, 1.0, 0.0, 0.0);
    assert!(path.is_rect());
    let mut rect = path.rect(Some(&kShearMatrix));
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0), path.bounding_box());

    path.transform(kShearMatrix);
    assert!(!path.is_rect());
    rect = path.rect(None);
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(1.0, 4.0, 3.0, 11.0), path.bounding_box());

    let shear_inverse_matrix = kShearMatrix.get_inverse();
    rect = path.rect(Some(&shear_inverse_matrix));
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(1.0, 4.0, 3.0, 11.0), path.bounding_box());

    path.transform(shear_inverse_matrix);
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(1.0, 2.0, 3.0, 5.0), path.bounding_box());
}

#[test]
fn Hexagon() {
    let mut path = CfxPath::new();
    path.append_point(CfxPointF::new(1.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(3.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 2.0), PointType::Line);
    path.append_point(CfxPointF::new(1.0, 2.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    assert_eq!(6usize, path.points().len());
    assert_eq!(PointType::Line, path.get_type(5));
    assert!(!path.is_closing_figure(5));
    assert!(!path.is_rect());
    assert!(!path.rect(None).is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 3.0, 2.0), path.bounding_box());

    path.close_path();
    assert_eq!(6usize, path.points().len());
    assert_eq!(PointType::Line, path.get_type(5));
    assert!(path.is_closing_figure(5));
    assert!(!path.is_rect());
    assert!(!path.rect(None).is_some());

    // Calling ClosePath() repeatedly makes no difference.
    path.close_path();
    assert_eq!(6usize, path.points().len());
    assert_eq!(PointType::Line, path.get_type(5));
    assert!(path.is_closing_figure(5));
    assert!(!path.is_rect());
    assert!(!path.rect(None).is_some());

    // A hexagon with the same start/end point is still not a rectangle.
    path.clear();
    path.append_point(CfxPointF::new(1.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(3.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 2.0), PointType::Line);
    path.append_point(CfxPointF::new(1.0, 2.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(1.0, 0.0), PointType::Line);
    assert!(!path.is_rect());
    assert!(!path.rect(None).is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 3.0, 2.0), path.bounding_box());
}

#[test]
fn ClosePath() {
    let mut path = CfxPath::new();
    path.append_line(CfxPointF::new(0.0, 0.0), CfxPointF::new(0.0, 1.0));
    path.append_line(CfxPointF::new(0.0, 1.0), CfxPointF::new(1.0, 1.0));
    path.append_line(CfxPointF::new(1.0, 1.0), CfxPointF::new(1.0, 0.0));
    assert_eq!(4usize, path.points().len());
    assert_eq!(PointType::Line, path.get_type(3));
    assert!(!path.is_closing_figure(3));
    assert!(path.is_rect());
    let mut rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0), rect.unwrap());

    let kIdentityMatrix = CfxMatrix::default();
    assert!(kIdentityMatrix.is_identity());
    rect = path.rect(Some(&kIdentityMatrix));
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0), rect.unwrap());

    path.close_path();
    assert_eq!(4usize, path.points().len());
    assert_eq!(PointType::Line, path.get_type(3));
    assert!(path.is_closing_figure(3));
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0), rect.unwrap());

    // Calling ClosePath() repeatedly makes no difference.
    path.close_path();
    assert_eq!(4usize, path.points().len());
    assert_eq!(PointType::Line, path.get_type(3));
    assert!(path.is_closing_figure(3));
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0), rect.unwrap());

    path.append_point_and_close(CfxPointF::new(0.0, 0.0), PointType::Line);
    assert_eq!(5usize, path.points().len());
    assert_eq!(PointType::Line, path.get_type(3));
    assert!(path.is_closing_figure(3));
    assert_eq!(PointType::Line, path.get_type(4));
    assert!(path.is_closing_figure(4));
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0), rect.unwrap());
}

#[test]
fn FivePointRect() {
    let mut path = CfxPath::new();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    assert_eq!(5usize, path.points().len());
    assert_eq!(PointType::Line, path.get_type(4));
    assert!(!path.is_closing_figure(4));
    assert!(path.is_rect());
    let mut rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), rect.unwrap());

    path.close_path();
    assert_eq!(5usize, path.points().len());
    assert_eq!(PointType::Line, path.get_type(4));
    assert!(path.is_closing_figure(4));
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), rect.unwrap());
}

#[test]
fn SixPlusPointRect() {
    let mut path = CfxPath::new();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    assert!(path.is_rect());
    let mut rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), path.bounding_box());

    path.clear();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    assert!(path.is_rect());
    rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), path.bounding_box());
}

#[test]
fn GetRectOppositeCorner() {
    let mut path = CfxPath::new();
    path.append_rect(1.0, 2.0, 3.0, 5.0);
    assert_eq!(Some(CfxPointF::new(3.0, 5.0)), path.rect_opposite_corner());

    // A duplicate point makes GetPoints()[2] a point on the first edge.
    path.clear();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    assert_eq!(CfxPointF::new(2.0, 0.0), path.points()[2].point);
    assert_eq!(Some(CfxPointF::new(2.0, 1.0)), path.rect_opposite_corner());

    path.clear();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(3.0, 1.0), PointType::Line);
    path.append_point_and_close(CfxPointF::new(0.0, 1.0), PointType::Line);
    assert!(!path.rect_opposite_corner().is_some());
}

#[test]
fn NotRect() {
    let mut path = CfxPath::new();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.1), PointType::Line);
    assert!(!path.is_rect());
    let mut rect = path.rect(None);
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), path.bounding_box());

    path.close_path();
    assert!(!path.is_rect());
    rect = path.rect(None);
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), path.bounding_box());

    path.clear();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(3.0, 1.0), PointType::Line);
    path.append_point_and_close(CfxPointF::new(0.0, 1.0), PointType::Line);
    assert!(!path.is_rect());
    rect = path.rect(None);
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 3.0, 1.0), path.bounding_box());

    path.clear();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
    path.append_point_and_close(CfxPointF::new(0.0, 1.0), PointType::Move);
    assert!(!path.is_rect());
    rect = path.rect(None);
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), path.bounding_box());

    path.clear();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(3.0, 0.0), PointType::Line);
    path.append_point_and_close(CfxPointF::new(0.0, 1.0), PointType::Line);
    assert!(!path.is_rect());
    rect = path.rect(None);
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 3.0, 1.0), path.bounding_box());

    path.clear();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    assert!(!path.is_rect());
    rect = path.rect(None);
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), path.bounding_box());

    path.clear();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    assert!(!path.is_rect());
    rect = path.rect(None);
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 1.0), path.bounding_box());

    path.clear();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(2.0, 2.0), PointType::Line);
    assert!(!path.is_rect());
    rect = path.rect(None);
    assert!(!rect.is_some());
    let kScaleMatrix = CfxMatrix::new(1.0, 0.0, 0.0, 2.0, 60.0, 70.0);
    rect = path.rect(Some(&kScaleMatrix));
    assert!(!rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 2.0, 2.0), path.bounding_box());
}

#[test]
fn EmptyRect() {
    // Document existing behavior where an empty rect is still considered a rect.
    let mut path = CfxPath::new();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    assert!(path.is_rect());
    let rect = path.rect(None);
    assert!(rect.is_some());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 0.0, 1.0), rect.unwrap());
    assert_eq!(CfxFloatRect::new(0.0, 0.0, 0.0, 1.0), path.bounding_box());
}

#[test]
fn Append() {
    let mut path = CfxPath::new();
    path.append_point(CfxPointF::new(5.0, 6.0), PointType::Move);
    assert_eq!(1usize, path.points().len());
    assert_eq!(CfxPointF::new(5.0, 6.0), path.get_point(0));

    let empty_path = CfxPath::new();
    path.append(&empty_path, None);
    assert_eq!(1usize, path.points().len());
    assert_eq!(CfxPointF::new(5.0, 6.0), path.get_point(0));

    path.append_self(None);
    assert_eq!(2usize, path.points().len());
    assert_eq!(CfxPointF::new(5.0, 6.0), path.get_point(0));
    assert_eq!(CfxPointF::new(5.0, 6.0), path.get_point(1));

    let kScaleMatrix = CfxMatrix::new(1.0, 0.0, 0.0, 2.0, 60.0, 70.0);
    path.append_self(Some(&kScaleMatrix));
    assert_eq!(4usize, path.points().len());
    assert_eq!(CfxPointF::new(5.0, 6.0), path.get_point(0));
    assert_eq!(CfxPointF::new(5.0, 6.0), path.get_point(1));
    assert_eq!(CfxPointF::new(65.0, 82.0), path.get_point(2));
    assert_eq!(CfxPointF::new(65.0, 82.0), path.get_point(3));
}

#[test]
fn GetBoundingBoxForStrokePath() {
    const kLineWidth: f32 = 1.0;
    const kMiterLimit: f32 = 1.0;

    {
        // Test the case that the first/last point is "move" and it closes the
        // paths.
        let mut path = CfxPath::new();
        path.append_point(CfxPointF::new(2.0, 0.0), PointType::Move);
        path.close_path();
        assert_eq!(
            CfxFloatRect::new(2.0, 0.0, 2.0, 0.0),
            path.bounding_box_for_stroke_path(kLineWidth, kMiterLimit)
        );
    }

    {
        // Test on a regular rect path.
        let mut path = CfxPath::new();
        path.append_point(CfxPointF::new(2.0, 0.0), PointType::Move);
        path.append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
        path.append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
        path.append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
        path.close_path();
        assert_eq!(
            CfxFloatRect::new(-1.0, -1.0, 3.0, 2.0),
            path.bounding_box_for_stroke_path(kLineWidth, kMiterLimit)
        );

        // If the final point is "move" and the path remains open, it should not
        // affect the bounding rect.
        path.append_point(CfxPointF::new(20.0, 20.0), PointType::Move);
        assert_eq!(
            CfxFloatRect::new(-1.0, -1.0, 3.0, 2.0),
            path.bounding_box_for_stroke_path(kLineWidth, kMiterLimit)
        );
    }
}
