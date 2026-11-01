// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Plain floating-point geometry in PDF page coordinates. Coordinates normally
//! use 1/72-inch units, but `/UserUnit` remains unapplied, matching the engine.
//! Page coordinates point up; rectangle edges are not render-pixel coordinates.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl Rect {
    pub fn width(self) -> f32 {
        self.right - self.left
    }
    pub fn height(self) -> f32 {
        self.top - self.bottom
    }
}

/// Affine transform: x' = a*x + c*y + e; y' = b*x + d*y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}
impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Matrix {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }
    pub fn scale(x: f32, y: f32) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::IDENTITY
        }
    }
    pub fn rotate_degrees(degrees: f32) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }
    /// Apply this transform, then `next`.
    pub fn then(self, next: Self) -> Self {
        Self {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }
    pub fn transform(self, point: Point) -> Point {
        Point {
            x: self.a * point.x + self.c * point.y + self.e,
            y: self.b * point.x + self.d * point.y + self.f,
        }
    }
    pub(crate) fn is_finite(self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f]
            .into_iter()
            .all(f32::is_finite)
    }
    pub(crate) fn core(self) -> pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix {
        pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix::new(
            self.a, self.b, self.c, self.d, self.e, self.f,
        )
    }
}
