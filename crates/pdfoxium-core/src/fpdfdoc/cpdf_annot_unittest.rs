// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfdoc/cpdf_annot_unittest.cpp`.
#![allow(non_snake_case)]
use super::*;
use crate::fpdfapi::parser::cpdf_object::retain_object;
fn CreateQuadPointArrayFromVector(points: &[i32]) -> CpdfArray {
    let mut array = CpdfArray::new();
    for &point in points {
        array.append_new(CpdfScalarObject::number_from_float(point as f32));
    }
    array
}
#[test]
fn RectFromQuadPointsArray() {
    let array = CreateQuadPointArrayFromVector(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 7, 6, 5, 4, 3, 2, 1]);
    let mut rect = rect_from_quad_points_array(&array, 0);
    assert_eq!(4.0f32, rect.left);
    assert_eq!(5.0f32, rect.bottom);
    assert_eq!(2.0f32, rect.right);
    assert_eq!(3.0f32, rect.top);

    rect = rect_from_quad_points_array(&array, 1);
    assert_eq!(4.0f32, rect.left);
    assert_eq!(3.0f32, rect.bottom);
    assert_eq!(6.0f32, rect.right);
    assert_eq!(5.0f32, rect.top);
}

#[test]
fn BoundingRectFromQuadPoints() {
    let mut dict = CpdfDictionary::new();
    let mut rect = bounding_rect_from_quad_points(&dict);
    assert_eq!(0.0f32, rect.left);
    assert_eq!(0.0f32, rect.bottom);
    assert_eq!(0.0f32, rect.right);
    assert_eq!(0.0f32, rect.top);

    dict.set_for(
        b"QuadPoints",
        Some(retain_object(CreateQuadPointArrayFromVector(&[0, 1, 2]))),
    );
    rect = bounding_rect_from_quad_points(&dict);
    assert_eq!(0.0f32, rect.left);
    assert_eq!(0.0f32, rect.bottom);
    assert_eq!(0.0f32, rect.right);
    assert_eq!(0.0f32, rect.top);

    dict.set_for(
        b"QuadPoints",
        Some(retain_object(CreateQuadPointArrayFromVector(&[
            0, 1, 2, 3, 4, 5, 6, 7,
        ]))),
    );
    rect = bounding_rect_from_quad_points(&dict);
    assert_eq!(4.0f32, rect.left);
    assert_eq!(5.0f32, rect.bottom);
    assert_eq!(2.0f32, rect.right);
    assert_eq!(3.0f32, rect.top);

    dict.set_for(
        b"QuadPoints",
        Some(retain_object(CreateQuadPointArrayFromVector(&[
            0, 1, 2, 3, 4, 5, 6, 7, 8, 7, 6, 5, 4, 3, 2, 1, 9, 2, 5, 7, 3, 6, 4, 1,
        ]))),
    );
    rect = bounding_rect_from_quad_points(&dict);
    assert_eq!(2.0f32, rect.left);
    assert_eq!(3.0f32, rect.bottom);
    assert_eq!(6.0f32, rect.right);
    assert_eq!(7.0f32, rect.top);
}

#[test]
fn RectFromQuadPoints() {
    let mut dict = CpdfDictionary::new();
    let mut rect = rect_from_quad_points(&dict, 0);
    assert_eq!(0.0f32, rect.left);
    assert_eq!(0.0f32, rect.bottom);
    assert_eq!(0.0f32, rect.right);
    assert_eq!(0.0f32, rect.top);
    rect = rect_from_quad_points(&dict, 5);
    assert_eq!(0.0f32, rect.left);
    assert_eq!(0.0f32, rect.bottom);
    assert_eq!(0.0f32, rect.right);
    assert_eq!(0.0f32, rect.top);

    dict.set_for(
        b"QuadPoints",
        Some(retain_object(CreateQuadPointArrayFromVector(&[
            0, 1, 2, 3, 4, 5, 6, 7,
        ]))),
    );
    rect = rect_from_quad_points(&dict, 0);
    assert_eq!(4.0f32, rect.left);
    assert_eq!(5.0f32, rect.bottom);
    assert_eq!(2.0f32, rect.right);
    assert_eq!(3.0f32, rect.top);
    rect = rect_from_quad_points(&dict, 5);
    assert_eq!(0.0f32, rect.left);
    assert_eq!(0.0f32, rect.bottom);
    assert_eq!(0.0f32, rect.right);
    assert_eq!(0.0f32, rect.top);

    dict.set_for(
        b"QuadPoints",
        Some(retain_object(CreateQuadPointArrayFromVector(&[
            0, 1, 2, 3, 4, 5, 6, 7, 8, 7, 6, 5, 4, 3, 2, 1, 9, 2, 5, 7, 3, 6, 4, 1,
        ]))),
    );
    rect = rect_from_quad_points(&dict, 0);
    assert_eq!(4.0f32, rect.left);
    assert_eq!(5.0f32, rect.bottom);
    assert_eq!(2.0f32, rect.right);
    assert_eq!(3.0f32, rect.top);
    rect = rect_from_quad_points(&dict, 1);
    assert_eq!(4.0f32, rect.left);
    assert_eq!(3.0f32, rect.bottom);
    assert_eq!(6.0f32, rect.right);
    assert_eq!(5.0f32, rect.top);
    rect = rect_from_quad_points(&dict, 2);
    assert_eq!(3.0f32, rect.left);
    assert_eq!(6.0f32, rect.bottom);
    assert_eq!(5.0f32, rect.right);
    assert_eq!(7.0f32, rect.top);
}

#[test]
fn QuadPointCount() {
    let mut array = CreateQuadPointArrayFromVector(&[]);
    assert_eq!(0usize, quad_point_count(&array));

    for _i in 0..7 {
        array.append_new(CpdfScalarObject::number_from_signed(0));
        assert_eq!(0usize, quad_point_count(&array));
    }
    for _i in 0..8 {
        array.append_new(CpdfScalarObject::number_from_signed(0));
        assert_eq!(1usize, quad_point_count(&array));
    }
    for _i in 0..50 {
        array.append_new(CpdfScalarObject::number_from_signed(0));
    }
    assert_eq!(8usize, quad_point_count(&array));
}
