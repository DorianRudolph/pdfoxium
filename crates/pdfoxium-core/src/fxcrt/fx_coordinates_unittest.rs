// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fx_coordinates_unittest.cpp` and its
//! `fx_coordinates_test_support.{h,cpp}` rectangle stream helpers.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::test_support::assert_float_eq;
use std::fmt::Write;

const kMinFloat: f32 = f32::MIN_POSITIVE;
const kMaxInt: i32 = i32::MAX;
const kMinInt: i32 = i32::MIN;
const kMinIntAsFloat: f32 = kMinInt as f32;
const kMaxIntAsFloat: f32 = kMaxInt as f32;

fn assert_near(expected: f32, actual: f32, tolerance: f32) {
    assert!(
        (expected - actual).abs() <= tolerance,
        "expected {expected}, actual {actual}, tolerance {tolerance}"
    );
}
fn VerifyFloatRectExact(left: f32, bottom: f32, right: f32, top: f32, rect: CfxFloatRect) {
    assert_float_eq(left, rect.left);
    assert_float_eq(bottom, rect.bottom);
    assert_float_eq(top, rect.top);
    assert_float_eq(right, rect.right);
}
fn VerifyFloatRectNear(left: f32, bottom: f32, right: f32, top: f32, rect: CfxFloatRect) {
    assert_near(left, rect.left, 1e-5);
    assert_near(bottom, rect.bottom, 1e-5);
    assert_near(top, rect.top, 1e-5);
    assert_near(right, rect.right, 1e-5);
}
fn VerifyMatrixExact(expected: CfxMatrix, actual: CfxMatrix) {
    assert_float_eq(expected.a, actual.a);
    assert_float_eq(expected.b, actual.b);
    assert_float_eq(expected.c, actual.c);
    assert_float_eq(expected.d, actual.d);
    assert_float_eq(expected.e, actual.e);
    assert_float_eq(expected.f, actual.f);
}
fn VerifyMatrixNear(expected: CfxMatrix, actual: CfxMatrix) {
    assert_near(expected.a, actual.a, 1e-5);
    assert_near(expected.b, actual.b, 1e-5);
    assert_near(expected.c, actual.c, 1e-5);
    assert_near(expected.d, actual.d, 1e-5);
    assert_near(expected.e, actual.e, 1e-5);
    assert_near(expected.f, actual.f, 1e-5);
}
fn VerifyPointCoordinates(x: f32, y: f32, point: CfxPointF) {
    assert_float_eq(x, point.x);
    assert_float_eq(y, point.y);
}

// Native stream operators are test support, so keep their Rust equivalents
// in this test module. These fixtures use the shared decimal display values.
struct DefaultFloat(f32);
impl std::fmt::Display for DefaultFloat {
    fn fmt(&self, os: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        crate::fxcrt::fx_string_wrappers::write_default_float(os, self.0)
    }
}
impl std::fmt::Display for CfxFloatRect {
    fn fmt(&self, os: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            os,
            "rect[w {} x h {} (left {}, bot {})]",
            DefaultFloat(self.width()),
            DefaultFloat(self.height()),
            DefaultFloat(self.left),
            DefaultFloat(self.bottom)
        )
    }
}
impl std::fmt::Display for CfxRectF {
    fn fmt(&self, os: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            os,
            "rect[w {} x h {} (left {}, top {})]",
            DefaultFloat(self.width),
            DefaultFloat(self.height),
            DefaultFloat(self.left),
            DefaultFloat(self.top)
        )
    }
}
impl std::fmt::Display for FxRect {
    fn fmt(&self, os: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            os,
            "rect[w {} x h {} (left {}, top {})]",
            self.width(),
            self.height(),
            self.left,
            self.top
        )
    }
}

#[test]
fn FromFXRect() {
    let downwards = FxRect::new(10, 20, 30, 40);
    let rect = CfxFloatRect::from_fx_rect(downwards);
    VerifyFloatRectExact(10.0f32, 20.0f32, 30.0f32, 40.0f32, rect);
}

#[test]
fn GetBBox() {
    let mut rect = CfxFloatRect::get_bbox(&[]);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);

    let mut data = Vec::new();
    data.push(CfxPointF::new(0.0f32, 0.0f32));
    rect = CfxFloatRect::get_bbox(&data[..0]);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
    rect = CfxFloatRect::get_bbox(&data);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);

    data.push(CfxPointF::new(2.5f32, 6.2f32));
    data.push(CfxPointF::new(1.5f32, 6.2f32));
    rect = CfxFloatRect::get_bbox(&data[..2]);
    VerifyFloatRectExact(0.0f32, 0.0f32, 2.5f32, 6.2f32, rect);

    rect = CfxFloatRect::get_bbox(&data);
    VerifyFloatRectExact(0.0f32, 0.0f32, 2.5f32, 6.2f32, rect);

    data.push(CfxPointF::new(2.5f32, 6.3f32));
    rect = CfxFloatRect::get_bbox(&data);
    VerifyFloatRectExact(0.0f32, 0.0f32, 2.5f32, 6.3f32, rect);

    data.push(CfxPointF::new(-3.0f32, 6.3f32));
    rect = CfxFloatRect::get_bbox(&data);
    VerifyFloatRectExact(-3.0f32, 0.0f32, 2.5f32, 6.3f32, rect);

    data.push(CfxPointF::new(4.0f32, -8.0f32));
    rect = CfxFloatRect::get_bbox(&data);
    VerifyFloatRectExact(-3.0f32, -8.0f32, 4.0f32, 6.3f32, rect);
}

#[test]
fn GetInnerRect() {
    let mut inner_rect;
    let mut rect = CfxFloatRect::default();

    inner_rect = rect.get_inner_rect();
    assert_eq!(FxRect::new(0, 0, 0, 0), inner_rect);

    // Function converts from float to int using floor() for top and right, and
    // ceil() for left and bottom.
    rect = CfxFloatRect::new(-1.1f32, 3.6f32, 4.4f32, -5.7f32);
    inner_rect = rect.get_inner_rect();
    assert_eq!(FxRect::new(-1, -6, 4, 4), inner_rect);

    rect = CfxFloatRect::new(kMinFloat, kMinFloat, kMinFloat, kMinFloat);
    inner_rect = rect.get_inner_rect();
    assert_eq!(FxRect::new(0, 0, 1, 1), inner_rect);

    rect = CfxFloatRect::new(-kMinFloat, -kMinFloat, -kMinFloat, -kMinFloat);
    inner_rect = rect.get_inner_rect();
    assert_eq!(FxRect::new(-1, -1, 0, 0), inner_rect);

    // Check at limits of integer range. When saturated would expect to get values
    // that are clamped to the limits of integers, but instead it is returning all
    // negative values that represent a rectangle as a dot in a far reach of the
    // negative coordinate space.  Related to crbug.com/1019026
    rect = CfxFloatRect::new(
        kMinIntAsFloat,
        kMinIntAsFloat,
        kMaxIntAsFloat,
        kMaxIntAsFloat,
    );
    inner_rect = rect.get_inner_rect();
    assert_eq!(FxRect::new(kMinInt, kMinInt, kMaxInt, kMaxInt), inner_rect);

    rect = CfxFloatRect::new(
        kMinIntAsFloat - 1.0f32,
        kMinIntAsFloat - 1.0f32,
        kMaxIntAsFloat + 1.0f32,
        kMaxIntAsFloat + 1.0f32,
    );
    inner_rect = rect.get_inner_rect();
    assert_eq!(FxRect::new(kMinInt, kMinInt, kMaxInt, kMaxInt), inner_rect);
}

#[test]
fn GetOuterRect() {
    let mut outer_rect;
    let mut rect = CfxFloatRect::default();

    outer_rect = rect.get_outer_rect();
    assert_eq!(FxRect::new(0, 0, 0, 0), outer_rect);

    // Function converts from float to int using floor() for left and bottom, and
    // ceil() for right and top.
    rect = CfxFloatRect::new(-1.1f32, 3.6f32, 4.4f32, -5.7f32);
    outer_rect = rect.get_outer_rect();
    assert_eq!(FxRect::new(-2, -5, 5, 3), outer_rect);

    rect = CfxFloatRect::new(kMinFloat, kMinFloat, kMinFloat, kMinFloat);
    outer_rect = rect.get_outer_rect();
    assert_eq!(FxRect::new(0, 0, 1, 1), outer_rect);

    rect = CfxFloatRect::new(-kMinFloat, -kMinFloat, -kMinFloat, -kMinFloat);
    outer_rect = rect.get_outer_rect();
    assert_eq!(FxRect::new(-1, -1, 0, 0), outer_rect);

    // Check at limits of integer range. When saturated would expect to get values
    // that are clamped to the limits of integers.
    rect = CfxFloatRect::new(
        kMinIntAsFloat,
        kMinIntAsFloat,
        kMaxIntAsFloat,
        kMaxIntAsFloat,
    );
    outer_rect = rect.get_outer_rect();
    assert_eq!(FxRect::new(kMinInt, kMinInt, kMaxInt, kMaxInt), outer_rect);

    rect = CfxFloatRect::new(
        kMinIntAsFloat - 1.0f32,
        kMinIntAsFloat - 1.0f32,
        kMaxIntAsFloat + 1.0f32,
        kMaxIntAsFloat + 1.0f32,
    );
    outer_rect = rect.get_outer_rect();
    assert_eq!(FxRect::new(kMinInt, kMinInt, kMaxInt, kMaxInt), outer_rect);
}

#[test]
fn Normalize() {
    let mut rect = CfxFloatRect::default();
    rect.normalize();
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);

    rect = CfxFloatRect::new(-1.0f32, -3.0f32, 4.5f32, 3.2f32);
    rect.normalize();
    VerifyFloatRectExact(-1.0f32, -3.0f32, 4.5f32, 3.2f32, rect);
    rect.scale(-1.0f32);
    rect.normalize();
    VerifyFloatRectExact(-4.5f32, -3.2f32, 1.0f32, 3.0f32, rect);
}

#[test]
fn Scale() {
    let mut rect = CfxFloatRect::new(-1.0f32, -3.0f32, 4.5f32, 3.2f32);
    rect.scale(1.0f32);
    VerifyFloatRectExact(-1.0f32, -3.0f32, 4.5f32, 3.2f32, rect);
    rect.scale(0.5f32);
    VerifyFloatRectExact(-0.5, -1.5, 2.25f32, 1.6f32, rect);
    rect.scale(2.0f32);
    VerifyFloatRectExact(-1.0f32, -3.0f32, 4.5f32, 3.2f32, rect);
    rect.scale(-1.0f32);
    VerifyFloatRectExact(1.0f32, 3.0f32, -4.5f32, -3.2f32, rect);
    rect.scale(0.0f32);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
}

#[test]
fn ScaleEmpty() {
    let mut rect = CfxFloatRect::default();
    rect.scale(1.0f32);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
    rect.scale(0.5f32);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
    rect.scale(2.0f32);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
    rect.scale(0.0f32);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
}

#[test]
fn ScaleFromCenterPoint() {
    let mut rect = CfxFloatRect::new(-1.0f32, -3.0f32, 4.5f32, 3.2f32);
    rect.scale_from_center_point(1.0f32);
    VerifyFloatRectExact(-1.0f32, -3.0f32, 4.5f32, 3.2f32, rect);
    rect.scale_from_center_point(0.5f32);
    VerifyFloatRectExact(0.375f32, -1.45f32, 3.125f32, 1.65f32, rect);
    rect.scale_from_center_point(2.0f32);
    VerifyFloatRectExact(-1.0f32, -3.0f32, 4.5f32, 3.2f32, rect);
    rect.scale_from_center_point(-1.0f32);
    VerifyFloatRectExact(4.5f32, 3.2f32, -1.0f32, -3.0f32, rect);
    rect.scale_from_center_point(0.0f32);
    assert_float_eq(1.75f32, rect.left);
    assert_near(0.1f32, rect.bottom, 0.001f32);
    assert_float_eq(1.75f32, rect.right);
    assert_near(0.1f32, rect.top, 0.001f32);
}

#[test]
fn ScaleFromCenterPointEmpty() {
    let mut rect = CfxFloatRect::default();
    rect.scale_from_center_point(1.0f32);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
    rect.scale_from_center_point(0.5f32);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
    rect.scale_from_center_point(2.0f32);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
    rect.scale_from_center_point(0.0f32);
    VerifyFloatRectExact(0.0f32, 0.0f32, 0.0f32, 0.0f32, rect);
}

#[test]
fn CFXFloatRectTest_Print() {
    let mut os = String::new();
    let mut rect = CfxFloatRect::default();
    write!(&mut os, "{rect}").unwrap();
    assert_eq!("rect[w 0 x h 0 (left 0, bot 0)]", os);

    os.clear();
    rect = CfxFloatRect::new(10.0, 20.0, 14.0, 23.0);
    write!(&mut os, "{rect}").unwrap();
    assert_eq!("rect[w 4 x h 3 (left 10, bot 20)]", os);

    os.clear();
    rect = CfxFloatRect::new(10.5, 20.5, 14.75, 23.75);
    write!(&mut os, "{rect}").unwrap();
    assert_eq!("rect[w 4.25 x h 3.25 (left 10.5, bot 20.5)]", os);
}

#[test]
fn CFXRectFTest_Print() {
    let mut os = String::new();
    let mut rect = CfxRectF::default();
    write!(&mut os, "{rect}").unwrap();
    assert_eq!("rect[w 0 x h 0 (left 0, top 0)]", os);

    os.clear();
    rect = CfxRectF::new(10.0, 20.0, 4.0, 3.0);
    write!(&mut os, "{rect}").unwrap();
    assert_eq!("rect[w 4 x h 3 (left 10, top 20)]", os);

    os.clear();
    rect = CfxRectF::new(10.5, 20.5, 4.25, 3.25);
    write!(&mut os, "{rect}").unwrap();
    assert_eq!("rect[w 4.25 x h 3.25 (left 10.5, top 20.5)]", os);
}

#[test]
fn FXRectTest_Print() {
    let mut os = String::new();
    let mut rect = FxRect::default();
    write!(&mut os, "{rect}").unwrap();
    assert_eq!("rect[w 0 x h 0 (left 0, top 0)]", os);

    os.clear();
    rect = FxRect::new(10, 20, 14, 23);
    write!(&mut os, "{rect}").unwrap();
    assert_eq!("rect[w 4 x h 3 (left 10, top 20)]", os);
}

#[test]
fn ReverseIdentity() {
    let rev = CfxMatrix::default().get_inverse();
    VerifyMatrixExact(
        CfxMatrix::new(1.0f32, 0.0f32, 0.0f32, 1.0f32, 0.0f32, 0.0f32),
        rev,
    );

    let expected = CfxPointF::new(2.0, 3.0);
    let result = rev.transform(CfxMatrix::default().transform(CfxPointF::new(2.0, 3.0)));
    assert_float_eq(expected.x, result.x);
    assert_float_eq(expected.y, result.y);
}

#[test]
fn SetIdentity() {
    let mut m = CfxMatrix::default();
    VerifyMatrixExact(
        CfxMatrix::new(1.0f32, 0.0f32, 0.0f32, 1.0f32, 0.0f32, 0.0f32),
        m,
    );
    assert!(m.is_identity());

    m.a = -1.0;
    assert!(!m.is_identity());

    m = CfxMatrix::default();
    VerifyMatrixExact(
        CfxMatrix::new(1.0f32, 0.0f32, 0.0f32, 1.0f32, 0.0f32, 0.0f32),
        m,
    );
    assert!(m.is_identity());
}

#[test]
fn GetInverse() {
    let m = CfxMatrix::new(3.0, 0.0, 2.0, 3.0, 1.0, 4.0);
    let rev = m.get_inverse();

    VerifyMatrixExact(
        CfxMatrix::new(
            0.33333334f32,
            0.0f32,
            -0.22222222f32,
            0.33333334f32,
            0.55555556f32,
            -1.3333334f32,
        ),
        rev,
    );

    let expected = CfxPointF::new(2.0, 3.0);
    let result = rev.transform(m.transform(CfxPointF::new(2.0, 3.0)));
    assert_float_eq(expected.x, result.x);
    assert_float_eq(expected.y, result.y);
}

// Note, I think these are a bug and the matrix should be the identity.
#[test]
fn GetInverseCR702041() {
    // The determinate is < std::numeric_limits<float>::epsilon()
    let m = CfxMatrix::new(
        0.947368443f32,
        -0.108947366f32,
        -0.923076928f32,
        0.106153846f32,
        18.0f32,
        787.929993f32,
    );
    let rev = m.get_inverse();

    VerifyMatrixExact(
        CfxMatrix::new(
            14247728.0f32,
            14622668.0f32,
            1.2389329e+08f32,
            1.2715364e+08f32,
            -9.7875698e+10f32,
            -1.0045138e+11f32,
        ),
        rev,
    );

    // Should be 2, 3
    let expected = CfxPointF::new(0.0, 0.0);
    let result = rev.transform(m.transform(CfxPointF::new(2.0, 3.0)));
    assert_float_eq(expected.x, result.x);
    assert_float_eq(expected.y, result.y);
}

#[test]
fn GetInverseCR714187() {
    // The determinate is < std::numeric_limits<float>::epsilon()
    let m = CfxMatrix::new(
        0.000037f32,
        0.0f32,
        0.0f32,
        -0.000037f32,
        182.413101f32,
        136.977646f32,
    );
    let rev = m.get_inverse();

    VerifyMatrixExact(
        CfxMatrix::new(
            27027.025f32,
            0.0f32,
            0.0f32,
            -27027.025f32,
            -4930083.5f32,
            3702098.2f32,
        ),
        rev,
    );

    // Should be 3 ....
    let expected = CfxPointF::new(2.0, 2.75);
    let result = rev.transform(m.transform(CfxPointF::new(2.0, 3.0)));
    assert_float_eq(expected.x, result.x);
    assert_float_eq(expected.y, result.y);
}

#[test]
fn ComposeTransformations() {
    // sin(FXSYS_PI/2) and cos(FXSYS_PI/2) have a tiny error and are not
    // exactly 1.0f32 and 0.0f32. The rotation matrix is thus not perfect.

    let mut rotate_90 = CfxMatrix::default();
    rotate_90.rotate(std::f32::consts::PI / 2.0);
    VerifyMatrixNear(
        CfxMatrix::new(0.0f32, 1.0f32, -1.0f32, 0.0f32, 0.0f32, 0.0f32),
        rotate_90,
    );

    let mut translate_23_11 = CfxMatrix::default();
    translate_23_11.translate(23.0, 11.0);
    VerifyMatrixExact(
        CfxMatrix::new(1.0f32, 0.0f32, 0.0f32, 1.0f32, 23.0f32, 11.0f32),
        translate_23_11,
    );

    let mut scale_5_13 = CfxMatrix::default();
    scale_5_13.scale(5.0, 13.0);
    VerifyMatrixExact(
        CfxMatrix::new(5.0f32, 0.0f32, 0.0f32, 13.0f32, 0.0, 0.0),
        scale_5_13,
    );

    // Apply the transforms to points step by step.
    let mut origin_transformed = CfxPointF::new(0.0, 0.0);
    let mut p_10_20_transformed = CfxPointF::new(10.0, 20.0);

    origin_transformed = rotate_90.transform(origin_transformed);
    VerifyPointCoordinates(0.0f32, 0.0f32, origin_transformed);
    p_10_20_transformed = rotate_90.transform(p_10_20_transformed);
    VerifyPointCoordinates(-20.0f32, 10.0f32, p_10_20_transformed);

    origin_transformed = translate_23_11.transform(origin_transformed);
    VerifyPointCoordinates(23.0f32, 11.0f32, origin_transformed);
    p_10_20_transformed = translate_23_11.transform(p_10_20_transformed);
    VerifyPointCoordinates(3.0f32, 21.0f32, p_10_20_transformed);

    origin_transformed = scale_5_13.transform(origin_transformed);
    VerifyPointCoordinates(115.0f32, 143.0f32, origin_transformed);
    p_10_20_transformed = scale_5_13.transform(p_10_20_transformed);
    VerifyPointCoordinates(15.0f32, 273.0f32, p_10_20_transformed);

    // Apply the transforms to points in the reverse order.
    origin_transformed = CfxPointF::new(0.0, 0.0);
    p_10_20_transformed = CfxPointF::new(10.0, 20.0);

    origin_transformed = scale_5_13.transform(origin_transformed);
    VerifyPointCoordinates(0.0f32, 0.0f32, origin_transformed);
    p_10_20_transformed = scale_5_13.transform(p_10_20_transformed);
    VerifyPointCoordinates(50.0f32, 260.0f32, p_10_20_transformed);

    origin_transformed = translate_23_11.transform(origin_transformed);
    VerifyPointCoordinates(23.0f32, 11.0f32, origin_transformed);
    p_10_20_transformed = translate_23_11.transform(p_10_20_transformed);
    VerifyPointCoordinates(73.0f32, 271.0f32, p_10_20_transformed);

    origin_transformed = rotate_90.transform(origin_transformed);
    VerifyPointCoordinates(-11.0f32, 23.0f32, origin_transformed);
    p_10_20_transformed = rotate_90.transform(p_10_20_transformed);
    VerifyPointCoordinates(-271.0f32, 73.0f32, p_10_20_transformed);

    // Compose all transforms.
    let mut m = CfxMatrix::default();
    m.concat(rotate_90);
    m.concat(translate_23_11);
    m.concat(scale_5_13);
    VerifyMatrixNear(
        CfxMatrix::new(0.0f32, 13.0f32, -5.0f32, 0.0f32, 115.0f32, 143.0f32),
        m,
    );

    // Note how the results using the combined matrix are equal to the results
    // when applying the three original matrices step-by-step.
    origin_transformed = m.transform(CfxPointF::new(0.0, 0.0));
    VerifyPointCoordinates(115.0f32, 143.0f32, origin_transformed);

    p_10_20_transformed = m.transform(CfxPointF::new(10.0, 20.0));
    VerifyPointCoordinates(15.0f32, 273.0f32, p_10_20_transformed);

    // Now compose all transforms prepending.
    m = CfxMatrix::default();
    m = rotate_90 * m;
    m = translate_23_11 * m;
    m = scale_5_13 * m;
    VerifyMatrixNear(
        CfxMatrix::new(0.0f32, 5.0f32, -13.0f32, 0.0f32, -11.0f32, 23.0f32),
        m,
    );

    // Note how the results using the combined matrix are equal to the results
    // when applying the three original matrices step-by-step in the reverse
    // order.
    origin_transformed = m.transform(CfxPointF::new(0.0, 0.0));
    VerifyPointCoordinates(-11.0f32, 23.0f32, origin_transformed);

    p_10_20_transformed = m.transform(CfxPointF::new(10.0, 20.0));
    VerifyPointCoordinates(-271.0f32, 73.0f32, p_10_20_transformed);
}

#[test]
fn TransformRectForRectF() {
    let mut rotate_90 = CfxMatrix::default();
    rotate_90.rotate(std::f32::consts::PI / 2.0);

    let mut scale_5_13 = CfxMatrix::default();
    scale_5_13.scale(5.0, 13.0);

    let mut rect = CfxRectF::new(10.5f32, 20.5f32, 4.25f32, 3.25f32);
    rect = rotate_90.transform_rect(rect);
    assert_float_eq(-23.75f32, rect.left);
    assert_float_eq(10.5f32, rect.top);
    assert_float_eq(3.25f32, rect.width);
    assert_float_eq(4.25f32, rect.height);

    rect = scale_5_13.transform_rect(rect);
    assert_float_eq(-118.75f32, rect.left);
    assert_float_eq(136.5f32, rect.top);
    assert_float_eq(16.25f32, rect.width);
    assert_float_eq(55.25f32, rect.height);
}

#[test]
fn TransformRectForFloatRect() {
    let mut rotate_90 = CfxMatrix::default();
    rotate_90.rotate(std::f32::consts::PI / 2.0);

    let mut scale_5_13 = CfxMatrix::default();
    scale_5_13.scale(5.0, 13.0);

    let mut rect = CfxFloatRect::new(5.5f32, 0.0f32, 12.25f32, 2.7f32);
    rect = rotate_90.transform_float_rect(rect);
    VerifyFloatRectNear(-2.7f32, 5.5f32, 0.0f32, 12.25f32, rect);

    rect = scale_5_13.transform_float_rect(rect);
    VerifyFloatRectNear(-13.5f32, 71.5f32, 0.0f32, 159.25f32, rect);
}
