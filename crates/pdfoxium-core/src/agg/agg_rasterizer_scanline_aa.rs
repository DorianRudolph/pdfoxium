// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_rasterizer_scanline_aa.{h,cpp}`.
//! The underlying cell store is in `agg_outline_aa`.

use super::agg_basics::{is_close, is_move_to, is_stop, is_vertex, Rect};
use super::agg_clip_liang_barsky::{clip_liang_barsky, clipping_flags};
use super::agg_outline_aa::OutlineAa;
use super::agg_path_storage::AggVertexSource;
use super::agg_render_scanlines::{PathRasterizer, ScanlineRasterizer};
use super::agg_scanline_u::ScanlineU8;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};

const POLY_BASE_SHIFT: u32 = 8;
const POLY_BASE_SIZE: i32 = 1 << POLY_BASE_SHIFT;
const AA_NUM: u32 = 1 << 8;
const AA_MASK: u32 = AA_NUM - 1;
const AA_2NUM: u32 = AA_NUM * 2;
const AA_2MASK: u32 = AA_2NUM - 1;

// Flooring preserves the subpixel phase across integer translations through zero.
pub fn poly_coord(coordinate: f32) -> i32 {
    (coordinate * POLY_BASE_SIZE as f32).floor() as i32
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FillingRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Status {
    Initial,
    LineTo,
    Closed,
}

pub struct ScanlineHitTest {
    x: i32,
    hit: bool,
}

impl ScanlineHitTest {
    pub fn new(x: i32) -> Self {
        Self { x, hit: false }
    }

    pub fn reset_spans(&mut self) {}

    pub fn finalize(&mut self, _y: i32) {}

    pub fn add_cell(&mut self, x: i32, _cover: u32) {
        if self.x == x {
            self.hit = true;
        }
    }

    pub fn add_span(&mut self, x: i32, len: i32, _cover: u32) {
        if self.x >= x && self.x < x.wrapping_add(len) {
            self.hit = true;
        }
    }

    pub fn num_spans(&self) -> usize {
        1
    }

    pub fn hit(&self) -> bool {
        self.hit
    }
}

pub struct RasterizerScanlineAa {
    outline: OutlineAa,
    filling_rule: FillingRule,
    clipped_start_x: i32,
    clipped_start_y: i32,
    start_x: i32,
    start_y: i32,
    prev_x: i32,
    prev_y: i32,
    prev_flags: u32,
    status: Status,
    clip_box: Rect,
    clipping: bool,
    cur_y: i32,
}

impl Default for RasterizerScanlineAa {
    fn default() -> Self {
        Self {
            outline: OutlineAa::new(),
            filling_rule: FillingRule::NonZero,
            clipped_start_x: 0,
            clipped_start_y: 0,
            start_x: 0,
            start_y: 0,
            prev_x: 0,
            prev_y: 0,
            prev_flags: 0,
            status: Status::Initial,
            clip_box: Rect::default(),
            clipping: false,
            cur_y: 0,
        }
    }
}

impl RasterizerScanlineAa {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.outline.retained_bytes()
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_filling_rule(&mut self, filling_rule: FillingRule) {
        self.filling_rule = filling_rule;
    }

    pub fn min_x(&self) -> i32 {
        self.outline.min_x()
    }

    pub fn min_y(&self) -> i32 {
        self.outline.min_y()
    }

    pub fn max_x(&self) -> i32 {
        self.outline.max_x()
    }

    pub fn max_y(&self) -> i32 {
        self.outline.max_y()
    }

    pub fn reset(&mut self) {
        self.outline.reset();
        self.status = Status::Initial;
    }

    pub fn clip_box(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.clip_box = Rect::new(
            poly_coord(x1),
            poly_coord(y1),
            poly_coord(x2),
            poly_coord(y2),
        );
        self.clip_box.normalize();
        // Clip geometry before scan conversion, as in PDFium's AGG renderer.
        // Rounded intersections may change edge AA by a few channel levels.
        self.clipping = true;
    }

    pub fn add_vertex(&mut self, x: f32, y: f32, command: u32) {
        if is_close(command) {
            self.close_polygon();
        } else if is_move_to(command) {
            self.move_to(poly_coord(x), poly_coord(y));
        } else if is_vertex(command) {
            self.line_to(poly_coord(x), poly_coord(y));
        }
    }

    pub fn move_to(&mut self, x: i32, y: i32) {
        if self.clipping {
            if self.outline.sorted() {
                self.reset();
            }
            if self.status == Status::LineTo {
                self.close_polygon();
            }
            self.prev_x = x;
            self.start_x = x;
            self.prev_y = y;
            self.start_y = y;
            self.status = Status::Initial;
            self.prev_flags = clipping_flags(x, y, self.clip_box);
            if self.prev_flags == 0 {
                self.move_to_no_clip(x, y);
            }
        } else {
            self.move_to_no_clip(x, y);
        }
    }

    pub fn line_to(&mut self, x: i32, y: i32) {
        if self.clipping {
            self.clip_segment(x, y);
        } else {
            self.line_to_no_clip(x, y);
        }
    }

    pub fn close_polygon(&mut self) {
        if self.status != Status::LineTo {
            return;
        }
        if self.clipping {
            self.clip_segment(self.start_x, self.start_y);
        }
        self.close_polygon_no_clip();
    }

    pub fn calculate_alpha(&self, area: i32, no_smooth: bool) -> u32 {
        match (self.filling_rule, no_smooth) {
            (FillingRule::NonZero, false) => Self::calculate_alpha_mode::<false, false>(area),
            (FillingRule::NonZero, true) => Self::calculate_alpha_mode::<false, true>(area),
            (FillingRule::EvenOdd, false) => Self::calculate_alpha_mode::<true, false>(area),
            (FillingRule::EvenOdd, true) => Self::calculate_alpha_mode::<true, true>(area),
        }
    }

    #[inline(always)]
    fn calculate_alpha_mode<const EVEN_ODD: bool, const NO_SMOOTH: bool>(area: i32) -> u32 {
        let mut cover = (area >> (POLY_BASE_SHIFT * 2 + 1 - 8)).unsigned_abs();
        if EVEN_ODD {
            cover &= AA_2MASK;
            if cover > AA_NUM {
                cover = AA_2NUM - cover;
            }
        }
        if NO_SMOOTH {
            cover = if cover > AA_MASK / 2 { AA_MASK } else { 0 };
        }
        cover.min(AA_MASK)
    }

    pub fn sort(&mut self) -> bool {
        self.outline.sort_cells()
    }

    pub fn rewind_scanlines(&mut self) -> bool {
        self.close_polygon();
        if !self.outline.sort_cells() {
            return false;
        }
        self.cur_y = self.outline.min_y();
        true
    }

    pub fn navigate_scanline(&mut self, y: i32) -> bool {
        self.close_polygon();
        if !self.outline.sort_cells() || y < self.min_y() || y > self.max_y() {
            return false;
        }
        self.cur_y = y;
        true
    }

    pub fn sweep_scanline(&mut self, scanline: &mut ScanlineU8, no_smooth: bool) -> bool {
        // Select fill rule and smoothing once rather than folding/thresholding
        // every edge and interior coverage for modes that are not enabled.
        match (self.filling_rule, no_smooth) {
            (FillingRule::NonZero, false) => self.sweep_scanline_mode::<false, false>(scanline),
            (FillingRule::NonZero, true) => self.sweep_scanline_mode::<false, true>(scanline),
            (FillingRule::EvenOdd, false) => self.sweep_scanline_mode::<true, false>(scanline),
            (FillingRule::EvenOdd, true) => self.sweep_scanline_mode::<true, true>(scanline),
        }
    }

    fn sweep_scanline_mode<const EVEN_ODD: bool, const NO_SMOOTH: bool>(
        &mut self,
        scanline: &mut ScanlineU8,
    ) -> bool {
        loop {
            if self.cur_y > self.outline.max_y() {
                return false;
            }
            scanline.reset_spans();
            let mut cells = self.outline.scanline_cells(self.cur_y).peekable();
            let mut cover = 0i32;
            while let Some(first) = cells.next() {
                let mut x = first.x;
                let mut area = first.area;
                let Some(sum) = cover.checked_add(first.cover) else {
                    break;
                };
                cover = sum;
                let mut area_overflow = false;
                let mut cover_overflow = false;
                while let Some(next) = cells.peek() {
                    if next.x != x {
                        break;
                    }
                    let next = cells.next().expect("peeked cell");
                    if area_overflow {
                        continue;
                    }
                    if let Some(sum) = area.checked_add(next.area) {
                        area = sum;
                    } else {
                        area_overflow = true;
                        continue;
                    }
                    if let Some(sum) = cover.checked_add(next.cover) {
                        cover = sum;
                    } else {
                        cover_overflow = true;
                        break;
                    }
                }
                if area_overflow {
                    continue;
                }
                if cover_overflow {
                    break;
                }
                if area != 0 {
                    let alpha = Self::calculate_alpha_mode::<EVEN_ODD, NO_SMOOTH>(
                        Self::calculate_area(cover, POLY_BASE_SHIFT + 1).wrapping_sub(area),
                    );
                    if alpha != 0 {
                        scanline.add_cell(x, alpha);
                    }
                    x = x.wrapping_add(1);
                }
                if let Some(next) = cells.peek() {
                    if next.x > x {
                        let alpha = Self::calculate_alpha_mode::<EVEN_ODD, NO_SMOOTH>(
                            Self::calculate_area(cover, POLY_BASE_SHIFT + 1),
                        );
                        if alpha != 0 {
                            scanline.add_span(x, next.x.wrapping_sub(x) as u32, alpha);
                        }
                    }
                }
            }
            if scanline.num_spans() != 0 {
                break;
            }
            self.cur_y = self.cur_y.wrapping_add(1);
        }
        scanline.finalize(self.cur_y);
        self.cur_y = self.cur_y.wrapping_add(1);
        true
    }

    pub fn add_path<S: AggVertexSource>(&mut self, source: &mut S, path_id: u32) {
        source.rewind(path_id);
        let (mut x, mut y) = (0.0, 0.0);
        loop {
            let command = source.vertex(&mut x, &mut y);
            if is_stop(command) {
                break;
            }
            self.add_vertex(x, y, command);
        }
    }

    /// Rasterize a cached glyph in its fixed font-local coordinate system.
    /// These masks never receive page/tile translations, so retain the native
    /// truncation contract rather than changing their existing edge coverage.
    pub(crate) fn add_path_truncated<S: AggVertexSource>(&mut self, source: &mut S, path_id: u32) {
        source.rewind(path_id);
        let (mut x, mut y) = (0.0, 0.0);
        loop {
            let command = source.vertex(&mut x, &mut y);
            if is_stop(command) {
                break;
            }
            let coord = |value: f32| (value * POLY_BASE_SIZE as f32) as i32;
            if is_close(command) {
                self.close_polygon();
            } else if is_move_to(command) {
                self.move_to(coord(x), coord(y));
            } else if is_vertex(command) {
                self.line_to(coord(x), coord(y));
            }
        }
    }

    /// Apply an integer device offset after quantization, so translated curves
    /// retain exactly the same subpixel vertices even across the device origin.
    pub fn add_path_offset<S: AggVertexSource>(
        &mut self,
        source: &mut S,
        path_id: u32,
        offset_x: i32,
        offset_y: i32,
    ) {
        let coord = |value: f32, offset: i32| {
            let local = (value as f64 * POLY_BASE_SIZE as f64).floor() as i64;
            local
                .saturating_add(offset as i64 * POLY_BASE_SIZE as i64)
                .clamp(i32::MIN as i64, i32::MAX as i64) as i32
        };
        source.rewind(path_id);
        let (mut x, mut y) = (0.0, 0.0);
        loop {
            let command = source.vertex(&mut x, &mut y);
            if is_stop(command) {
                break;
            }
            if is_close(command) {
                self.close_polygon();
            } else if is_move_to(command) {
                self.move_to(coord(x, offset_x), coord(y, offset_y));
            } else if is_vertex(command) {
                self.line_to(coord(x, offset_x), coord(y, offset_y));
            }
        }
    }

    pub fn add_path_transformed<S: AggVertexSource>(
        &mut self,
        source: &mut S,
        matrix: Option<CfxMatrix>,
        path_id: u32,
    ) {
        source.rewind(path_id);
        let (mut x, mut y) = (0.0, 0.0);
        loop {
            let command = source.vertex(&mut x, &mut y);
            if is_stop(command) {
                break;
            }
            if let Some(matrix) = matrix {
                let point = matrix.transform(CfxPointF::new(x, y));
                x = point.x;
                y = point.y;
            }
            self.add_vertex(x, y, command);
        }
    }

    fn move_to_no_clip(&mut self, x: i32, y: i32) {
        if self.status == Status::LineTo {
            self.close_polygon_no_clip();
        }
        self.outline.move_to(x, y);
        self.clipped_start_x = x;
        self.clipped_start_y = y;
        self.status = Status::LineTo;
    }

    fn line_to_no_clip(&mut self, x: i32, y: i32) {
        if self.status != Status::Initial {
            self.outline.line_to(x, y);
            self.status = Status::LineTo;
        }
    }

    fn close_polygon_no_clip(&mut self) {
        if self.status == Status::LineTo {
            self.outline
                .line_to(self.clipped_start_x, self.clipped_start_y);
            self.status = Status::Closed;
        }
    }

    fn clip_segment(&mut self, x: i32, y: i32) {
        let flags = clipping_flags(x, y, self.clip_box);
        if self.prev_flags == flags {
            if flags == 0 {
                if self.status == Status::Initial {
                    self.move_to_no_clip(x, y);
                } else {
                    self.line_to_no_clip(x, y);
                }
            }
        } else {
            let mut cx = [0; 4];
            let mut cy = [0; 4];
            let count = clip_liang_barsky(
                self.prev_x,
                self.prev_y,
                x,
                y,
                self.clip_box,
                &mut cx,
                &mut cy,
            );
            for index in 0..count {
                if self.status == Status::Initial {
                    self.move_to_no_clip(cx[index], cy[index]);
                } else {
                    self.line_to_no_clip(cx[index], cy[index]);
                }
            }
        }
        self.prev_flags = flags;
        self.prev_x = x;
        self.prev_y = y;
    }

    fn calculate_area(cover: i32, shift: u32) -> i32 {
        (cover as u32).wrapping_shl(shift) as i32
    }
}

impl ScanlineRasterizer for RasterizerScanlineAa {
    fn rewind_scanlines(&mut self) -> bool {
        RasterizerScanlineAa::rewind_scanlines(self)
    }

    fn min_x(&self) -> i32 {
        RasterizerScanlineAa::min_x(self)
    }

    fn max_x(&self) -> i32 {
        RasterizerScanlineAa::max_x(self)
    }

    fn sweep_scanline(&mut self, scanline: &mut ScanlineU8, no_smooth: bool) -> bool {
        RasterizerScanlineAa::sweep_scanline(self, scanline, no_smooth)
    }
}

impl<S: AggVertexSource> PathRasterizer<S, u32> for RasterizerScanlineAa {
    fn reset(&mut self) {
        RasterizerScanlineAa::reset(self);
    }

    fn add_path(&mut self, source: &mut S, path_id: u32) {
        RasterizerScanlineAa::add_path(self, source, path_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agg::agg_path_storage::PathStorage;

    #[test]
    fn fixed_point_quantization_preserves_negative_translation_phase() {
        for step in -2048..2048 {
            let coordinate = step as f32 / 1024.0;
            for shift in [-128, -3, 0, 5, 127] {
                assert_eq!(
                    poly_coord(coordinate + shift as f32),
                    poly_coord(coordinate) + shift * 256
                );
            }
        }
    }

    #[test]
    fn coverage_modes_preserve_signed_extremes_and_fold_thresholds() {
        let mut rasterizer = RasterizerScanlineAa::new();
        for rule in [FillingRule::NonZero, FillingRule::EvenOdd] {
            rasterizer.set_filling_rule(rule);
            for no_smooth in [false, true] {
                for area in (i32::MIN..=i32::MAX).step_by(65537).chain([
                    -262145,
                    -262144,
                    -131073,
                    -131072,
                    -65537,
                    -65536,
                    -1,
                    0,
                    1,
                    65535,
                    65536,
                    131071,
                    131072,
                    262143,
                    262144,
                    i32::MAX,
                ]) {
                    let mut cover = (i64::from(area) >> 9).unsigned_abs() as u32;
                    if rule == FillingRule::EvenOdd {
                        cover %= 512;
                        cover = if cover <= 256 { cover } else { 512 - cover };
                    }
                    let expected = if no_smooth {
                        if cover >= 128 {
                            255
                        } else {
                            0
                        }
                    } else {
                        cover.min(255)
                    };
                    assert_eq!(
                        rasterizer.calculate_alpha(area, no_smooth),
                        expected,
                        "{area} {rule:?} no_smooth={no_smooth}"
                    );
                }
            }
        }
    }

    #[test]
    fn closed_rectangle_sweeps_full_coverage_rows() {
        let mut path = PathStorage::new();
        path.move_to(1.0, 1.0);
        path.line_to(4.0, 1.0);
        path.line_to(4.0, 3.0);
        path.line_to(1.0, 3.0);
        path.end_poly();
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.add_path(&mut path, 0);
        assert!(rasterizer.rewind_scanlines());
        let mut scanline = ScanlineU8::new();
        assert!(scanline.reset(rasterizer.min_x(), rasterizer.max_x()));
        let mut rows = Vec::new();
        while rasterizer.sweep_scanline(&mut scanline, false) {
            rows.push((
                scanline.y(),
                scanline
                    .spans()
                    .map(|span| (span.x, span.len, span.covers.to_vec()))
                    .collect::<Vec<_>>(),
            ));
        }
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, 1);
        assert_eq!(rows[1].0, 2);
        assert_eq!(rows[0].1, vec![(1, 3, vec![255, 255, 255])]);
        assert_eq!(rows[1].1, rows[0].1);
    }

    #[test]
    fn even_odd_rule_cancels_coincident_rectangles() {
        let mut path = PathStorage::new();
        for _ in 0..2 {
            path.move_to(0.0, 0.0);
            path.line_to(2.0, 0.0);
            path.line_to(2.0, 2.0);
            path.line_to(0.0, 2.0);
            path.end_poly();
        }
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.set_filling_rule(FillingRule::EvenOdd);
        rasterizer.add_path(&mut path, 0);
        assert!(rasterizer.rewind_scanlines());
        let mut scanline = ScanlineU8::new();
        assert!(scanline.reset(rasterizer.min_x(), rasterizer.max_x()));
        assert!(!rasterizer.sweep_scanline(&mut scanline, false));
    }

    #[test]
    fn geometric_clipping_bounds_extreme_edges_before_scan_conversion() {
        for (start, end) in [
            ((-8_000_000, 1), (8_000_000, 255)),
            ((-8_000_000, 255), (8_000_000, 1)),
            ((1, -8_000_000), (255, 8_000_000)),
            ((255, 8_000_000), (1, -8_000_000)),
            ((256, i32::MIN), (256, i32::MAX)),
            ((i32::MIN, 256), (i32::MAX, 256)),
        ] {
            let mut rasterizer = RasterizerScanlineAa::new();
            rasterizer.clip_box(0.0, 0.0, 7.0, 5.0);
            rasterizer.move_to(start.0, start.1);
            rasterizer.line_to(end.0, end.1);
            rasterizer.line_to(3 * POLY_BASE_SIZE, 3 * POLY_BASE_SIZE);
            rasterizer.close_polygon();
            rasterizer.rewind_scanlines();
            assert!(rasterizer.outline.total_cells() <= 100);
            assert!(rasterizer.min_x() >= 0 && rasterizer.max_x() <= 7);
            assert!(rasterizer.min_y() >= 0 && rasterizer.max_y() <= 5);
        }
    }

    #[test]
    fn clip_box_restricts_rasterized_rectangle() {
        let mut path = PathStorage::new();
        path.move_to(-1.0, -1.0);
        path.line_to(3.0, -1.0);
        path.line_to(3.0, 3.0);
        path.line_to(-1.0, 3.0);
        path.end_poly();
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.clip_box(0.0, 0.0, 2.0, 2.0);
        rasterizer.add_path(&mut path, 0);
        assert!(rasterizer.rewind_scanlines());
        let mut scanline = ScanlineU8::new();
        assert!(scanline.reset(rasterizer.min_x(), rasterizer.max_x()));
        let mut covered = Vec::new();
        while rasterizer.sweep_scanline(&mut scanline, false) {
            for span in scanline.spans() {
                covered.extend(span.covers.iter().enumerate().map(|(offset, &cover)| {
                    (i32::from(span.x) + offset as i32, scanline.y(), cover)
                }));
            }
        }
        assert_eq!(
            covered,
            vec![(0, 0, 255), (1, 0, 255), (0, 1, 255), (1, 1, 255)]
        );
    }
}
