// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/fx_coordinates.{h,cpp}`.

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct CfxPointTemplate<T> {
    pub x: T,
    pub y: T,
}

impl<T> CfxPointTemplate<T> {
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl<T: Add<Output = T>> Add for CfxPointTemplate<T> {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
}
impl<T: Sub<Output = T>> Sub for CfxPointTemplate<T> {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }
}
impl<T: AddAssign> AddAssign for CfxPointTemplate<T> {
    fn add_assign(&mut self, other: Self) {
        self.x += other.x;
        self.y += other.y;
    }
}
impl<T: SubAssign> SubAssign for CfxPointTemplate<T> {
    fn sub_assign(&mut self, other: Self) {
        self.x -= other.x;
        self.y -= other.y;
    }
}

pub type CfxPoint16 = CfxPointTemplate<i16>;
pub type CfxPoint = CfxPointTemplate<i32>;
pub type CfxPointF = CfxPointTemplate<f32>;

impl Mul<CfxPointF> for f32 {
    type Output = CfxPointF;
    fn mul(self, point: CfxPointF) -> CfxPointF {
        CfxPointF::new(self * point.x, self * point.y)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct CfxSizeTemplate<T> {
    pub width: T,
    pub height: T,
}

impl<T> CfxSizeTemplate<T> {
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

pub trait CoordinateCastFrom<T> {
    fn from_coordinate(value: T) -> Self;
}

macro_rules! coordinate_casts {
    ($source:ty => $($target:ty),+ $(,)?) => {
        $(impl CoordinateCastFrom<$source> for $target {
            fn from_coordinate(value: $source) -> Self { value as Self }
        })+
    };
}
coordinate_casts!(i16 => i16, i32, f32);
coordinate_casts!(i32 => i16, i32, f32);
coordinate_casts!(f32 => i16, i32, f32);

impl<T: Copy> CfxSizeTemplate<T> {
    pub fn as_type<U: CoordinateCastFrom<T>>(self) -> CfxSizeTemplate<U> {
        CfxSizeTemplate::new(
            U::from_coordinate(self.width),
            U::from_coordinate(self.height),
        )
    }
}
impl<T: Default> CfxSizeTemplate<T> {
    pub fn clear(&mut self) {
        self.width = T::default();
        self.height = T::default();
    }
}
impl<T: Add<Output = T>> Add for CfxSizeTemplate<T> {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.width + other.width, self.height + other.height)
    }
}
impl<T: Sub<Output = T>> Sub for CfxSizeTemplate<T> {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.width - other.width, self.height - other.height)
    }
}
impl<T: Copy + Mul<Output = T>> Mul<T> for CfxSizeTemplate<T> {
    type Output = Self;
    fn mul(self, factor: T) -> Self {
        Self::new(self.width * factor, self.height * factor)
    }
}
impl<T: Copy + Div<Output = T>> Div<T> for CfxSizeTemplate<T> {
    type Output = Self;
    fn div(self, divisor: T) -> Self {
        Self::new(self.width / divisor, self.height / divisor)
    }
}
impl<T: AddAssign> AddAssign for CfxSizeTemplate<T> {
    fn add_assign(&mut self, other: Self) {
        self.width += other.width;
        self.height += other.height;
    }
}
impl<T: SubAssign> SubAssign for CfxSizeTemplate<T> {
    fn sub_assign(&mut self, other: Self) {
        self.width -= other.width;
        self.height -= other.height;
    }
}
impl<T: Copy + MulAssign> MulAssign<T> for CfxSizeTemplate<T> {
    fn mul_assign(&mut self, factor: T) {
        self.width *= factor;
        self.height *= factor;
    }
}
impl<T: Copy + DivAssign> DivAssign<T> for CfxSizeTemplate<T> {
    fn div_assign(&mut self, divisor: T) {
        self.width /= divisor;
        self.height /= divisor;
    }
}

pub type CfxSize = CfxSizeTemplate<i32>;
pub type CfxSizeF = CfxSizeTemplate<f32>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct CfxVectorTemplate<T> {
    pub x: T,
    pub y: T,
}

impl<T> CfxVectorTemplate<T> {
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl<T: Sub<Output = T> + Copy> CfxVectorTemplate<T> {
    pub fn between(first: CfxPointTemplate<T>, second: CfxPointTemplate<T>) -> Self {
        Self::new(second.x - first.x, second.y - first.y)
    }
}
impl CfxVectorTemplate<f32> {
    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }
    pub fn normalize(&mut self) {
        let length = self.length();
        if length < 0.0001 {
            return;
        }
        self.x /= length;
        self.y /= length;
    }
}
pub type CfxVector = CfxVectorTemplate<i32>;
pub type CfxVectorF = CfxVectorTemplate<f32>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct FxRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl FxRect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
    pub fn is_empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
    pub fn valid(self) -> bool {
        self.right.checked_sub(self.left).is_some() && self.bottom.checked_sub(self.top).is_some()
    }
    pub fn normalize(&mut self) {
        if self.left > self.right {
            std::mem::swap(&mut self.left, &mut self.right);
        }
        if self.top > self.bottom {
            std::mem::swap(&mut self.top, &mut self.bottom);
        }
    }
    pub fn intersect(&mut self, source: Self) {
        let mut source = source;
        source.normalize();
        self.normalize();
        self.left = self.left.max(source.left);
        self.top = self.top.max(source.top);
        self.right = self.right.min(source.right);
        self.bottom = self.bottom.min(source.bottom);
        if self.left > self.right || self.top > self.bottom {
            *self = Self::default();
        }
    }
    pub fn swapped_clip_box(self, width: i32, height: i32, flip_x: bool, flip_y: bool) -> Self {
        let mut result = Self::new(
            if flip_y { height - self.top } else { self.top },
            if flip_x { width - self.left } else { self.left },
            if flip_y {
                height - self.bottom
            } else {
                self.bottom
            },
            if flip_x {
                width - self.right
            } else {
                self.right
            },
        );
        result.normalize();
        result
    }
    pub fn offset(&mut self, dx: i32, dy: i32) {
        self.left += dx;
        self.right += dx;
        self.top += dy;
        self.bottom += dy;
    }
    pub fn near(self, other: Self, tolerance: i32) -> bool {
        (i64::from(self.left) - i64::from(other.left)).abs() <= i64::from(tolerance)
            && (i64::from(self.top) - i64::from(other.top)).abs() <= i64::from(tolerance)
            && (i64::from(self.right) - i64::from(other.right)).abs() <= i64::from(tolerance)
            && (i64::from(self.bottom) - i64::from(other.bottom)).abs() <= i64::from(tolerance)
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

fn cpp_min(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}
fn cpp_max(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

fn match_float_range(first: f32, second: f32) -> (i32, i32) {
    let length = (second - first).ceil();
    let floor = first.floor();
    let ceil = first.ceil();
    let error1 = first - floor + (second - floor - length).abs();
    let error2 = ceil - first + (second - ceil - length).abs();
    let start = if error1 > error2 { ceil } else { floor };
    let end = start + length;
    let valid =
        |v: f32| v.is_finite() && (v as f64) >= i32::MIN as f64 && (v as f64) < 2147483648.0;
    if valid(start) && valid(end) {
        (start as i32, end as i32)
    } else {
        (0, 0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct CfxFloatRect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl CfxFloatRect {
    pub const fn new(left: f32, bottom: f32, right: f32, top: f32) -> Self {
        Self {
            left,
            bottom,
            right,
            top,
        }
    }
    pub fn from_fx_rect(rect: FxRect) -> Self {
        Self::new(
            rect.left as f32,
            rect.top as f32,
            rect.right as f32,
            rect.bottom as f32,
        )
    }
    pub fn from_point(point: CfxPointF) -> Self {
        Self::new(point.x, point.y, point.x, point.y)
    }
    pub fn get_bbox(points: &[CfxPointF]) -> Self {
        let Some(first) = points.first() else {
            return Self::default();
        };
        let mut result = Self::from_point(*first);
        for &point in &points[1..] {
            result.update_rect(point);
        }
        result
    }
    pub fn normalize(&mut self) {
        if self.left > self.right {
            std::mem::swap(&mut self.left, &mut self.right);
        }
        if self.bottom > self.top {
            std::mem::swap(&mut self.bottom, &mut self.top);
        }
    }
    pub fn is_empty(self) -> bool {
        self.left >= self.right || self.bottom >= self.top
    }
    pub fn contains_point(self, point: CfxPointF) -> bool {
        let mut rect = self;
        rect.normalize();
        point.x <= rect.right
            && point.x >= rect.left
            && point.y <= rect.top
            && point.y >= rect.bottom
    }
    pub fn contains_rect(self, other: Self) -> bool {
        let mut first = self;
        first.normalize();
        let mut second = other;
        second.normalize();
        second.left >= first.left
            && second.right <= first.right
            && second.bottom >= first.bottom
            && second.top <= first.top
    }
    pub fn intersect(&mut self, other: Self) {
        self.normalize();
        let mut other = other;
        other.normalize();
        self.left = cpp_max(self.left, other.left);
        self.bottom = cpp_max(self.bottom, other.bottom);
        self.right = cpp_min(self.right, other.right);
        self.top = cpp_min(self.top, other.top);
        if self.left > self.right || self.bottom > self.top {
            *self = Self::default();
        }
    }
    pub fn union(&mut self, other: Self) {
        self.normalize();
        let mut other = other;
        other.normalize();
        self.left = cpp_min(self.left, other.left);
        self.bottom = cpp_min(self.bottom, other.bottom);
        self.right = cpp_max(self.right, other.right);
        self.top = cpp_max(self.top, other.top);
    }
    pub fn get_inner_rect(self) -> FxRect {
        let mut result = FxRect::new(
            self.left.ceil() as i32,
            self.bottom.ceil() as i32,
            self.right.floor() as i32,
            self.top.floor() as i32,
        );
        result.normalize();
        result
    }
    pub fn get_outer_rect(self) -> FxRect {
        let mut result = FxRect::new(
            self.left.floor() as i32,
            self.bottom.floor() as i32,
            self.right.ceil() as i32,
            self.top.ceil() as i32,
        );
        result.normalize();
        result
    }
    pub fn get_closest_rect(self) -> FxRect {
        let (left, right) = match_float_range(self.left, self.right);
        let (top, bottom) = match_float_range(self.bottom, self.top);
        let mut result = FxRect::new(left, top, right, bottom);
        result.normalize();
        result
    }
    pub fn get_center_square(self) -> Self {
        let half = if self.width() > self.height() {
            self.height() / 2.0
        } else {
            self.width() / 2.0
        };
        let center_x = (self.left + self.right) / 2.0;
        let center_y = (self.top + self.bottom) / 2.0;
        Self::new(
            center_x - half,
            center_y - half,
            center_x + half,
            center_y + half,
        )
    }
    pub fn update_rect(&mut self, point: CfxPointF) {
        self.left = cpp_min(self.left, point.x);
        self.bottom = cpp_min(self.bottom, point.y);
        self.right = cpp_max(self.right, point.x);
        self.top = cpp_max(self.top, point.y);
    }
    pub fn width(self) -> f32 {
        self.right - self.left
    }
    pub fn height(self) -> f32 {
        self.top - self.bottom
    }
    pub fn inflate(&mut self, left: f32, bottom: f32, right: f32, top: f32) {
        self.normalize();
        self.left -= left;
        self.bottom -= bottom;
        self.right += right;
        self.top += top;
    }
    pub fn inflate_xy(&mut self, x: f32, y: f32) {
        self.inflate(x, y, x, y);
    }
    pub fn inflate_rect(&mut self, other: Self) {
        self.inflate(other.left, other.bottom, other.right, other.top);
    }
    pub fn deflate(&mut self, left: f32, bottom: f32, right: f32, top: f32) {
        self.inflate(-left, -bottom, -right, -top);
    }
    pub fn deflate_xy(&mut self, x: f32, y: f32) {
        self.deflate(x, y, x, y);
    }
    pub fn deflate_rect(&mut self, other: Self) {
        self.deflate(other.left, other.bottom, other.right, other.top);
    }
    pub fn get_deflated(self, x: f32, y: f32) -> Self {
        if self.is_empty() {
            return Self::default();
        }
        let mut result = self;
        result.deflate_xy(x, y);
        result.normalize();
        result
    }
    pub fn translate(&mut self, x: f32, y: f32) {
        self.left += x;
        self.right += x;
        self.top += y;
        self.bottom += y;
    }
    pub fn scale(&mut self, factor: f32) {
        self.left *= factor;
        self.bottom *= factor;
        self.right *= factor;
        self.top *= factor;
    }
    pub fn scale_from_center_point(&mut self, factor: f32) {
        let half_width = (self.right - self.left) / 2.0;
        let half_height = (self.top - self.bottom) / 2.0;
        let center_x = (self.left + self.right) / 2.0;
        let center_y = (self.top + self.bottom) / 2.0;
        self.left = center_x - half_width * factor;
        self.bottom = center_y - half_height * factor;
        self.right = center_x + half_width * factor;
        self.top = center_y + half_height * factor;
    }
    pub fn to_fx_rect(self) -> FxRect {
        FxRect::new(
            self.left as i32,
            self.top as i32,
            self.right as i32,
            self.bottom as i32,
        )
    }
    pub fn to_rounded_fx_rect(self) -> FxRect {
        let round = |v: f32| if v.is_nan() { 0 } else { v.round() as i32 };
        FxRect::new(
            round(self.left),
            round(self.top),
            round(self.right),
            round(self.bottom),
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct CfxRectF {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl CfxRectF {
    pub const fn new(left: f32, top: f32, width: f32, height: f32) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }
    pub fn from_point_size(point: CfxPointF, size: CfxSizeF) -> Self {
        Self::new(point.x, point.y, size.width, size.height)
    }
    pub fn from_fx_rect(rect: FxRect) -> Self {
        Self::new(
            rect.left as f32,
            rect.top as f32,
            rect.width() as f32,
            rect.height() as f32,
        )
    }
    pub fn right(self) -> f32 {
        self.left + self.width
    }
    pub fn bottom(self) -> f32 {
        self.top + self.height
    }
    pub fn normalize(&mut self) {
        if self.width < 0.0 {
            self.left += self.width;
            self.width = -self.width;
        }
        if self.height < 0.0 {
            self.top += self.height;
            self.height = -self.height;
        }
    }
    pub fn offset(&mut self, dx: f32, dy: f32) {
        self.left += dx;
        self.top += dy;
    }
    pub fn inflate(&mut self, left: f32, top: f32, right: f32, bottom: f32) {
        self.left -= left;
        self.top -= top;
        self.width += left + right;
        self.height += top + bottom;
    }
    pub fn inflate_xy(&mut self, x: f32, y: f32) {
        self.inflate(x, y, x, y);
    }
    pub fn inflate_rect(&mut self, rect: Self) {
        self.inflate(
            rect.left,
            rect.top,
            rect.left + rect.width,
            rect.top + rect.height,
        );
    }
    pub fn deflate(&mut self, left: f32, top: f32, right: f32, bottom: f32) {
        self.left += left;
        self.top += top;
        self.width -= left + right;
        self.height -= top + bottom;
    }
    pub fn deflate_xy(&mut self, x: f32, y: f32) {
        self.deflate(x, y, x, y);
    }
    pub fn deflate_rect(&mut self, rect: Self) {
        // Preserve the source's `rt.top + rt.width` third argument.
        self.deflate(
            rect.left,
            rect.top,
            rect.top + rect.width,
            rect.top + rect.height,
        );
    }
    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
    pub fn is_empty_with_epsilon(self, epsilon: f32) -> bool {
        self.width <= epsilon || self.height <= epsilon
    }
    pub fn empty(&mut self) {
        self.width = 0.0;
        self.height = 0.0;
    }
    pub fn contains_point(self, point: CfxPointF) -> bool {
        point.x >= self.left
            && point.x < self.right()
            && point.y >= self.top
            && point.y < self.bottom()
    }
    pub fn contains_rect(self, rect: Self) -> bool {
        rect.left >= self.left
            && rect.right() <= self.right()
            && rect.top >= self.top
            && rect.bottom() <= self.bottom()
    }
    pub fn size(self) -> CfxSizeF {
        CfxSizeF::new(self.width, self.height)
    }
    pub fn top_left(self) -> CfxPointF {
        CfxPointF::new(self.left, self.top)
    }
    pub fn top_right(self) -> CfxPointF {
        CfxPointF::new(self.right(), self.top)
    }
    pub fn bottom_left(self) -> CfxPointF {
        CfxPointF::new(self.left, self.bottom())
    }
    pub fn bottom_right(self) -> CfxPointF {
        CfxPointF::new(self.right(), self.bottom())
    }
    pub fn center(self) -> CfxPointF {
        CfxPointF::new(self.left + self.width / 2.0, self.top + self.height / 2.0)
    }
    pub fn union_point(&mut self, x: f32, y: f32) {
        let mut right = self.right();
        let mut bottom = self.bottom();
        self.left = cpp_min(self.left, x);
        self.top = cpp_min(self.top, y);
        right = cpp_max(right, x);
        bottom = cpp_max(bottom, y);
        self.width = right - self.left;
        self.height = bottom - self.top;
    }
    pub fn union(&mut self, other: Self) {
        let mut right = self.right();
        let mut bottom = self.bottom();
        self.left = cpp_min(self.left, other.left);
        self.top = cpp_min(self.top, other.top);
        right = cpp_max(right, other.right());
        bottom = cpp_max(bottom, other.bottom());
        self.width = right - self.left;
        self.height = bottom - self.top;
    }
    pub fn intersect(&mut self, other: Self) {
        let mut right = self.right();
        let mut bottom = self.bottom();
        self.left = cpp_max(self.left, other.left);
        self.top = cpp_max(self.top, other.top);
        right = cpp_min(right, other.right());
        bottom = cpp_min(bottom, other.bottom());
        self.width = right - self.left;
        self.height = bottom - self.top;
    }
    pub fn intersects(self, other: Self) -> bool {
        let mut rect = other;
        rect.intersect(self);
        !rect.is_empty()
    }
    pub fn intersects_with_epsilon(self, other: Self, epsilon: f32) -> bool {
        let mut rect = other;
        rect.intersect(self);
        !rect.is_empty_with_epsilon(epsilon)
    }
    pub fn to_float_rect(self) -> CfxFloatRect {
        CfxFloatRect::new(self.left, self.top, self.right(), self.bottom())
    }
    pub fn get_outer_rect(self) -> FxRect {
        FxRect::new(
            self.left.floor() as i32,
            self.top.floor() as i32,
            self.right().ceil() as i32,
            self.bottom().ceil() as i32,
        )
    }
}

impl AddAssign<CfxPointF> for CfxRectF {
    fn add_assign(&mut self, point: CfxPointF) {
        self.left += point.x;
        self.top += point.y;
    }
}
impl SubAssign<CfxPointF> for CfxRectF {
    fn sub_assign(&mut self, point: CfxPointF) {
        self.left -= point.x;
        self.top -= point.y;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct CfxMatrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Default for CfxMatrix {
    fn default() -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
    }
}

impl CfxMatrix {
    pub const fn new(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        Self { a, b, c, d, e, f }
    }
    pub fn is_identity(self) -> bool {
        self == Self::default()
    }
    pub fn get_inverse(self) -> Self {
        let determinant = self.a * self.d - self.b * self.c;
        if determinant.abs() == 0.0 {
            return Self::default();
        }
        let negative = -determinant;
        Self::new(
            self.d / determinant,
            self.b / negative,
            self.c / negative,
            self.a / determinant,
            (self.c * self.f - self.d * self.e) / determinant,
            (self.a * self.f - self.b * self.e) / negative,
        )
    }
    pub fn is_90_rotated(self) -> bool {
        (self.a * 1000.0).abs() < self.b.abs() && (self.d * 1000.0).abs() < self.c.abs()
    }
    pub fn is_scaled(self) -> bool {
        (self.b * 1000.0).abs() < self.a.abs() && (self.c * 1000.0).abs() < self.d.abs()
    }
    pub fn will_scale(self) -> bool {
        self.a != 1.0 || self.b != 0.0 || self.c != 0.0 || self.d != 1.0
    }
    pub fn concat(&mut self, right: Self) {
        *self *= right;
    }
    pub fn translate(&mut self, x: f32, y: f32) {
        self.e += x;
        self.f += y;
    }
    pub fn translate_prepend(&mut self, x: f32, y: f32) {
        self.e += x * self.a + y * self.c;
        self.f += y * self.d + x * self.b;
    }
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.a *= sx;
        self.b *= sy;
        self.c *= sx;
        self.d *= sy;
        self.e *= sx;
        self.f *= sy;
    }
    pub fn rotate(&mut self, radians: f32) {
        let cosine = radians.cos();
        let sine = radians.sin();
        self.concat(Self::new(cosine, sine, -sine, cosine, 0.0, 0.0));
    }
    pub fn match_rect(&mut self, dest: CfxFloatRect, src: CfxFloatRect) {
        let mut difference = src.left - src.right;
        self.a = if difference.abs() < 0.001 {
            1.0
        } else {
            (dest.left - dest.right) / difference
        };
        difference = src.bottom - src.top;
        self.d = if difference.abs() < 0.001 {
            1.0
        } else {
            (dest.bottom - dest.top) / difference
        };
        self.e = dest.left - src.left * self.a;
        self.f = dest.bottom - src.bottom * self.d;
        self.b = 0.0;
        self.c = 0.0;
    }
    pub fn get_x_unit(self) -> f32 {
        if self.b == 0.0 {
            return if self.a > 0.0 { self.a } else { -self.a };
        }
        if self.a == 0.0 {
            return if self.b > 0.0 { self.b } else { -self.b };
        }
        self.a.hypot(self.b)
    }
    pub fn get_y_unit(self) -> f32 {
        if self.c == 0.0 {
            return if self.d > 0.0 { self.d } else { -self.d };
        }
        if self.d == 0.0 {
            return if self.c > 0.0 { self.c } else { -self.c };
        }
        self.c.hypot(self.d)
    }
    pub fn get_unit_rect(self) -> CfxFloatRect {
        self.transform_float_rect(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0))
    }
    pub fn transform_x_distance(self, dx: f32) -> f32 {
        (self.a * dx).hypot(self.b * dx)
    }
    pub fn transform_distance(self, distance: f32) -> f32 {
        distance * (self.get_x_unit() + self.get_y_unit()) / 2.0
    }
    pub fn transform(self, point: CfxPointF) -> CfxPointF {
        CfxPointF::new(
            self.a * point.x + self.c * point.y + self.e,
            self.b * point.x + self.d * point.y + self.f,
        )
    }
    pub fn transform_rect(self, rect: CfxRectF) -> CfxRectF {
        let result = self.transform_float_rect(rect.to_float_rect());
        CfxRectF::new(result.left, result.bottom, result.width(), result.height())
    }
    pub fn transform_float_rect(self, rect: CfxFloatRect) -> CfxFloatRect {
        let points = [
            self.transform(CfxPointF::new(rect.left, rect.top)),
            self.transform(CfxPointF::new(rect.left, rect.bottom)),
            self.transform(CfxPointF::new(rect.right, rect.top)),
            self.transform(CfxPointF::new(rect.right, rect.bottom)),
        ];
        let mut result = CfxFloatRect::from_point(points[0]);
        for &point in &points[1..] {
            result.update_rect(point);
        }
        result
    }
}

impl Mul for CfxMatrix {
    type Output = Self;
    fn mul(self, right: Self) -> Self {
        Self::new(
            self.a * right.a + self.b * right.c,
            self.a * right.b + self.b * right.d,
            self.c * right.a + self.d * right.c,
            self.c * right.b + self.d * right.d,
            self.e * right.a + self.f * right.c + right.e,
            self.e * right.b + self.f * right.d + right.f,
        )
    }
}
impl MulAssign for CfxMatrix {
    fn mul_assign(&mut self, right: Self) {
        *self = *self * right;
    }
}

#[cfg(test)]
mod tests {
    use super::{CfxFloatRect, CfxMatrix, CfxPointF, CfxRectF, FxRect};

    #[test]
    fn rectangles_round_and_normalize_like_pdfium() {
        let rect = CfxFloatRect::new(-1.1, 3.6, 4.4, -5.7);
        assert_eq!(rect.get_inner_rect(), FxRect::new(-1, -6, 4, 4));
        assert_eq!(rect.get_outer_rect(), FxRect::new(-2, -5, 5, 3));
        assert_eq!(CfxFloatRect::get_bbox(&[]), CfxFloatRect::default());
        let points = [CfxPointF::new(1.0, 2.0), CfxPointF::new(-3.0, 4.0)];
        assert_eq!(
            CfxFloatRect::get_bbox(&points),
            CfxFloatRect::new(-3.0, 2.0, 1.0, 4.0)
        );
        let mut rect_f = CfxRectF::new(10.0, 20.0, -5.0, -8.0);
        rect_f.normalize();
        assert_eq!(rect_f, CfxRectF::new(5.0, 12.0, 5.0, 8.0));
        let mut clipped = FxRect::new(10, 20, 0, 0);
        clipped.intersect(FxRect::new(5, 5, 15, 25));
        assert_eq!(clipped, FxRect::new(5, 5, 10, 20));
        let mut source_quirk = CfxRectF::new(20.0, 30.0, 40.0, 50.0);
        source_quirk.deflate_rect(CfxRectF::new(1.0, 2.0, 3.0, 4.0));
        assert_eq!(source_quirk, CfxRectF::new(21.0, 32.0, 34.0, 42.0));
    }

    #[test]
    fn matrix_concat_inverse_and_transform_order() {
        let matrix = CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 4.0, 5.0);
        assert_eq!(
            matrix.transform(CfxPointF::new(1.0, 2.0)),
            CfxPointF::new(6.0, 11.0)
        );
        assert_eq!(
            matrix.get_inverse().transform(CfxPointF::new(6.0, 11.0)),
            CfxPointF::new(1.0, 2.0)
        );
        let mut transformed = CfxMatrix::default();
        transformed.concat(CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 0.0, 0.0));
        transformed.translate(4.0, 5.0);
        assert_eq!(transformed, matrix);
        assert_eq!(
            CfxMatrix::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0).get_inverse(),
            CfxMatrix::default()
        );
        let source_matrix = CfxMatrix::new(3.0, 0.0, 2.0, 3.0, 1.0, 4.0);
        assert_eq!(
            source_matrix.get_inverse(),
            CfxMatrix::new(
                0.33333334,
                0.0,
                -0.22222222,
                0.33333334,
                0.5555556,
                -1.3333334,
            )
        );
    }
}

#[cfg(test)]
#[path = "fx_coordinates_unittest.rs"]
mod upstream_tests;
