// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Selected AGG driver from `core/fxge/agg/cfx_agg_devicedriver.{h,cpp}`.
//! The source's span compositor lives in `cfx_agg_renderer`; image jobs borrow
//! the destination and clip for their entire continuation lifetime.

use crate::agg::agg_color_gray::Gray8;
use crate::agg::agg_conv_dash::ConvDash;
use crate::agg::agg_conv_stroke::ConvStroke;
use crate::agg::agg_curves::{Curve4, Curve4Points};
use crate::agg::agg_math_stroke::{LineCap as AggLineCap, LineJoin as AggLineJoin};
use crate::agg::agg_path_storage::PathStorage;
use crate::agg::agg_pixfmt_gray::PixfmtGray8;
use crate::agg::agg_rasterizer_scanline_aa::{FillingRule, RasterizerScanlineAa};
use crate::agg::agg_render_scanlines::render_scanlines;
use crate::agg::agg_render_scanlines::{ColoredScanlineRenderer, ScanlineRenderer};
use crate::agg::agg_renderer_base::RendererBaseGray;
use crate::agg::agg_rendering_buffer::RenderingBuffer;
use crate::agg::agg_scanline_u::ScanlineU8;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF, FxRect};
use crate::fxge::cfx_fillrenderoptions::{CfxFillRenderOptions, FillType};
use crate::fxge::cfx_graphstatedata::{
    CfxGraphStateData, LineCap as GraphLineCap, LineJoin as GraphLineJoin,
};
use crate::fxge::cfx_path::{CfxPath, PointType};
use crate::fxge::dib::cfx_dibitmap::{CfxDibitmap, CfxDibitmapData};
use crate::fxge::dib::fx_dib::{
    alpha_merge, argb_a, argb_b, argb_g, argb_r, argb_to_bgr_order_dib, get_bpp_from_format,
    get_is_alpha_from_format, get_is_mask_from_format, BlendMode, FxdibFormat,
};
use std::rc::Rc;

use super::cfx_agg_cliprgn::CfxAggClipRgn;
use super::cfx_agg_renderer::CfxAggRenderer;

const MAX_POS: f32 = 32000.0;

fn rgb_byte_order_composite_rect(bitmap: &mut CfxDibitmapData<'_>, rect: FxRect, color: u32) {
    let src_alpha = i32::from(argb_a(color));
    if src_alpha == 0 {
        return;
    }
    let rgb = [argb_r(color), argb_g(color), argb_b(color)];
    let bpp = (get_bpp_from_format(bitmap.format()) / 8) as usize;
    let alpha_format = get_is_alpha_from_format(bitmap.format());
    let opaque = argb_to_bgr_order_dib(color).to_le_bytes();
    for y in rect.top..rect.bottom {
        let row = bitmap.scanline_mut(y).expect("bitmap row");
        for x in rect.left..rect.right {
            let pixel = &mut row[x as usize * bpp..(x as usize + 1) * bpp];
            if src_alpha == 255 {
                if bpp == 4 {
                    pixel.copy_from_slice(&opaque);
                } else {
                    pixel[..3].copy_from_slice(&rgb);
                }
            } else if alpha_format {
                if pixel[3] == 0 {
                    pixel[..3].copy_from_slice(&rgb);
                    pixel[3] = src_alpha as u8;
                } else {
                    let back_alpha = i32::from(pixel[3]);
                    let dest_alpha = back_alpha + src_alpha - back_alpha * src_alpha / 255;
                    let alpha_ratio = src_alpha * 255 / dest_alpha;
                    for channel in 0..3 {
                        pixel[channel] = alpha_merge(
                            i32::from(pixel[channel]),
                            i32::from(rgb[channel]),
                            alpha_ratio,
                        ) as u8;
                    }
                    // The pinned RGB-order helper does not write dest_alpha.
                }
            } else {
                for channel in 0..3 {
                    pixel[channel] = alpha_merge(
                        i32::from(pixel[channel]),
                        i32::from(rgb[channel]),
                        src_alpha,
                    ) as u8;
                }
            }
        }
    }
}

/// The AGG driver's RGB-byte-order bitmap transfer, including its supported
/// BGR/BGRx/BGRA format conversions.
fn rgb_byte_order_transfer_bitmap(
    dest: &mut CfxDibitmapData<'_>,
    width: i32,
    height: i32,
    source: &CfxDibitmapData<'_>,
    src_left: i32,
    src_top: i32,
) -> bool {
    let (src_bpp, dest_bpp, force_opaque) = match (dest.format(), source.format()) {
        (FxdibFormat::Bgr, FxdibFormat::Bgr) => (3, 3, false),
        (FxdibFormat::Bgrx, FxdibFormat::Bgrx) | (FxdibFormat::Bgra, FxdibFormat::Bgra) => {
            (4, 4, false)
        }
        (FxdibFormat::Bgr, FxdibFormat::Bgrx) => (4, 3, false),
        (FxdibFormat::Bgra | FxdibFormat::Bgrx, FxdibFormat::Bgr) => (3, 4, true),
        (FxdibFormat::Bgra, FxdibFormat::Bgrx) => (4, 4, true),
        _ => return false,
    };
    if width <= 0 || height <= 0 {
        return true;
    }
    let src_left = i64::from(src_left);
    let src_top = i64::from(src_top);
    let left = 0i64.max(-src_left);
    let top = 0i64.max(-src_top);
    let right = i64::from(width)
        .min(i64::from(dest.width()))
        .min(i64::from(source.width()) - src_left);
    let bottom = i64::from(height)
        .min(i64::from(dest.height()))
        .min(i64::from(source.height()) - src_top);
    for y in top..bottom {
        let source_row = source.scanline((src_top + y) as i32).expect("source row");
        let dest_row = dest.scanline_mut(y as i32).expect("bitmap row");
        for x in left..right {
            let src_offset = (src_left + x) as usize * src_bpp;
            let dest_offset = x as usize * dest_bpp;
            let input = &source_row[src_offset..src_offset + src_bpp];
            let output = &mut dest_row[dest_offset..dest_offset + dest_bpp];
            output[..3].copy_from_slice(&[input[2], input[1], input[0]]);
            if dest_bpp == 4 {
                output[3] = if force_opaque { 255 } else { input[3] };
            }
        }
    }
    true
}

pub fn build_agg_path(path: &CfxPath, object_to_device: Option<CfxMatrix>) -> PathStorage {
    let mut agg_path = PathStorage::new();
    let mut curve = Curve4::new();
    build_agg_path_into(path, object_to_device, &mut agg_path, &mut curve);
    agg_path
}

// Glyph rendering retains these two buffers between cache misses. The same
// subdivision and vertex order are used by the ordinary path-building caller.
pub(crate) fn build_agg_path_into(
    path: &CfxPath,
    object_to_device: Option<CfxMatrix>,
    agg_path: &mut PathStorage,
    curve: &mut Curve4,
) {
    build_agg_path_into_with_origin(path, object_to_device, agg_path, curve, (0, 0));
}

fn build_agg_path_into_with_origin(
    path: &CfxPath,
    object_to_device: Option<CfxMatrix>,
    agg_path: &mut PathStorage,
    curve: &mut Curve4,
    origin: (i32, i32),
) {
    let hard_clip = |point: CfxPointF| {
        CfxPointF::new(
            point
                .x
                .clamp(-MAX_POS - origin.0 as f32, MAX_POS - origin.0 as f32),
            point
                .y
                .clamp(-MAX_POS - origin.1 as f32, MAX_POS - origin.1 as f32),
        )
    };
    agg_path.clear();
    let points = path.points();
    let mut index = 0;
    while index < points.len() {
        let point = points[index];
        let mut pos = point.point;
        if let Some(matrix) = object_to_device {
            pos = matrix.transform(pos);
        }
        pos = hard_clip(pos);
        match point.point_type {
            PointType::Move => agg_path.move_to(pos.x, pos.y),
            PointType::Line => {
                if index > 0
                    && points[index - 1].point_type == PointType::Move
                    && !points[index - 1].close_figure
                    && (index + 1 == points.len()
                        || points[index + 1].point_type == PointType::Move
                            && !points[index + 1].close_figure)
                    && point.point == points[index - 1].point
                {
                    pos.x += 1.0;
                }
                agg_path.line_to(pos.x, pos.y);
            }
            PointType::Bezier => {
                if index > 0 && index + 2 < points.len() {
                    let mut pos0 = points[index - 1].point;
                    let mut pos2 = points[index + 1].point;
                    let mut pos3 = points[index + 2].point;
                    if let Some(matrix) = object_to_device {
                        pos0 = matrix.transform(pos0);
                        pos2 = matrix.transform(pos2);
                        pos3 = matrix.transform(pos3);
                    }
                    pos0 = hard_clip(pos0);
                    pos2 = hard_clip(pos2);
                    pos3 = hard_clip(pos3);
                    curve.init(Curve4Points::new([
                        pos0.x, pos0.y, pos.x, pos.y, pos2.x, pos2.y, pos3.x, pos3.y,
                    ]));
                    index += 2;
                    agg_path.add_path(curve, 0, true);
                }
            }
        }
        if points[index].close_figure {
            agg_path.end_poly();
        }
        index += 1;
    }
}

// Subdivide curves before applying an integer bitmap translation. Otherwise
// midpoint arithmetic can change at tile origins, altering a few edge pixels.
fn build_agg_path_with_origin(
    path: &CfxPath,
    matrix: Option<CfxMatrix>,
) -> (PathStorage, Option<(i32, i32)>) {
    if !path
        .points()
        .iter()
        .any(|point| point.point_type == PointType::Bezier)
    {
        return (build_agg_path(path, matrix), None);
    }
    let (matrix, origin) = split_path_matrix_origin(matrix.unwrap_or_default());
    let mut path_data = PathStorage::new();
    let mut curve = Curve4::new();
    build_agg_path_into_with_origin(path, Some(matrix), &mut path_data, &mut curve, origin);
    (path_data, Some(origin))
}

fn split_path_matrix_origin(mut matrix: CfxMatrix) -> (CfxMatrix, (i32, i32)) {
    let origin = (matrix.e.floor() as i32, matrix.f.floor() as i32);
    matrix.e -= origin.0 as f32;
    matrix.f -= origin.1 as f32;
    (matrix, origin)
}

/// A secondary rejection for filled Bezier contours near a device clip.
/// Use the builder's exact matrix/origin arithmetic rather than an inverse
/// transformed object bound. Curve subdivision only averages control points;
/// its vertices stay within their hull, as do monotonic fixed-point rounding
/// and the filled edges. Antialias coverage cannot extend beyond that hull.
/// Lines are deliberately excluded: PDFium can paint retraced/degenerate fill
/// paths as cosmetic strokes, and rectangles have a separate snapping path.
#[inline(never)]
pub(crate) fn filled_bezier_path_intersects_clip(
    path: &CfxPath,
    matrix: CfxMatrix,
    clip: FxRect,
) -> bool {
    let points = path.points();
    if points.len() < 4
        || points
            .iter()
            .any(|point| point.point_type == PointType::Line)
        || !points
            .iter()
            .any(|point| point.point_type == PointType::Bezier)
        || ![matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f]
            .iter()
            .all(|value| value.is_finite())
    {
        return true;
    }
    // Malformed Bezier sequences can enter the zero-area path shortcuts too.
    // Accept only complete cubic triples, each following a move or cubic.
    let mut index = 0;
    while index < points.len() {
        if points[index].point_type != PointType::Move {
            return true;
        }
        index += 1;
        while index < points.len() && points[index].point_type != PointType::Move {
            let Some(cubic) = points.get(index..index + 3) else {
                return true;
            };
            if cubic
                .iter()
                .any(|point| point.point_type != PointType::Bezier)
                || cubic[..2].iter().any(|point| point.close_figure)
            {
                return true;
            }
            index += 3;
        }
    }
    let (matrix, origin) = split_path_matrix_origin(matrix);
    let quantize = |value: f32, offset: i32| {
        ((f64::from(value) * 256.0).floor() as i64)
            .saturating_add(i64::from(offset) * 256)
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX))
    };
    let (mut left, mut bottom) = (i64::MAX, i64::MAX);
    let (mut right, mut top) = (i64::MIN, i64::MIN);
    for point in points {
        let point = matrix.transform(point.point);
        // Keep unusual nonfinite, huge, or hard-clipped inputs on the existing
        // conservative path. Bounded local inputs also prevent midpoint overflow.
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x.abs() > MAX_POS
            || point.y.abs() > MAX_POS
            || point.x < -MAX_POS - origin.0 as f32
            || point.x > MAX_POS - origin.0 as f32
            || point.y < -MAX_POS - origin.1 as f32
            || point.y > MAX_POS - origin.1 as f32
        {
            return true;
        }
        let x = quantize(point.x, origin.0);
        let y = quantize(point.y, origin.1);
        left = left.min(x);
        right = right.max(x);
        bottom = bottom.min(y);
        top = top.max(y);
    }
    right > i64::from(clip.left) * 256
        && left < i64::from(clip.right) * 256
        && top > i64::from(clip.top) * 256
        && bottom < i64::from(clip.bottom) * 256
}

pub fn rasterize_stroke(
    rasterizer: &mut RasterizerScanlineAa,
    path_data: &mut PathStorage,
    object_to_device: Option<CfxMatrix>,
    graph_state: &CfxGraphStateData,
    scale: f32,
    _text_mode: bool,
) {
    let cap = if graph_state.line_cap() == GraphLineCap::ROUND {
        AggLineCap::Round
    } else if graph_state.line_cap() == GraphLineCap::SQUARE {
        AggLineCap::Square
    } else {
        AggLineCap::Butt
    };
    let join = if graph_state.line_join() == GraphLineJoin::ROUND {
        AggLineJoin::Round
    } else if graph_state.line_join() == GraphLineJoin::BEVEL {
        AggLineJoin::Bevel
    } else {
        AggLineJoin::MiterRevert
    };
    let mut width = graph_state.line_width() * scale;
    let mut unit = 1.0;
    if let Some(matrix) = object_to_device {
        unit = 1.0 / ((matrix.get_x_unit() + matrix.get_y_unit()) / 2.0);
    }
    if width < unit {
        width = unit;
    }

    let dash_array = graph_state.dash_array();
    let mut should_apply_dash_pattern = !dash_array.is_empty();
    if should_apply_dash_pattern {
        let mut dash_cycle_len = 0.0;
        for &value in dash_array {
            if !value.is_finite() {
                should_apply_dash_pattern = false;
                break;
            }
            dash_cycle_len += value.max(0.0);
        }
        if dash_cycle_len * scale < 0.1 {
            should_apply_dash_pattern = false;
        }
    }

    if should_apply_dash_pattern {
        let mut dash = ConvDash::new(path_data);
        for &length in dash_array {
            let dash_len = if length <= 0.000001 { 0.1 } else { length };
            dash.add_dash((dash_len * scale).abs());
        }
        dash.dash_start(graph_state.dash_phase() * scale);
        let mut stroke = ConvStroke::new(&mut dash);
        stroke.set_line_join(join);
        stroke.set_line_cap(cap);
        stroke.set_miter_limit(graph_state.miter_limit());
        stroke.set_width(width);
        rasterizer.add_path_transformed(&mut stroke, object_to_device, 0);
        return;
    }

    let mut stroke = ConvStroke::new(path_data);
    stroke.set_line_join(join);
    stroke.set_line_cap(cap);
    stroke.set_miter_limit(graph_state.miter_limit());
    stroke.set_width(width);
    rasterizer.add_path_transformed(&mut stroke, object_to_device, 0);
}

pub fn get_alternate_or_winding_fill_type(options: CfxFillRenderOptions) -> FillingRule {
    if options.fill_type == FillType::Winding {
        FillingRule::NonZero
    } else {
        FillingRule::EvenOdd
    }
}

pub struct CfxAggDeviceDriver<'a, 'storage> {
    bitmap: &'a mut CfxDibitmapData<'storage>,
    clip_region: Option<CfxAggClipRgn>,
    state_stack: Vec<Option<CfxAggClipRgn>>,
    fill_options: CfxFillRenderOptions,
    rgb_byte_order: bool,
    group_knockout: bool,
    backdrop_bitmap: Option<&'a CfxDibitmap>,
}

impl<'a, 'storage> CfxAggDeviceDriver<'a, 'storage> {
    pub fn new(
        bitmap: &'a mut CfxDibitmapData<'storage>,
        rgb_byte_order: bool,
        backdrop_bitmap: Option<&'a CfxDibitmap>,
    ) -> Self {
        assert_ne!(get_bpp_from_format(bitmap.format()), 1);
        Self {
            bitmap,
            clip_region: None,
            state_stack: Vec::new(),
            fill_options: CfxFillRenderOptions::default(),
            rgb_byte_order,
            group_knockout: false,
            backdrop_bitmap,
        }
    }

    pub fn pixel_width(&self) -> i32 {
        self.bitmap.width()
    }

    pub fn set_group_knockout(&mut self, knockout: bool) {
        self.group_knockout = knockout;
    }

    pub fn pixel_height(&self) -> i32 {
        self.bitmap.height()
    }

    pub fn bitmap(&self) -> &CfxDibitmapData<'storage> {
        self.bitmap
    }

    pub fn backdrop(&self) -> Option<&CfxDibitmap> {
        self.backdrop_bitmap
    }

    pub fn create_compatible_bitmap(&self, width: i32, height: i32) -> Option<CfxDibitmap> {
        let format = if self.render_cap_byte_mask_output() {
            FxdibFormat::EightBppMask
        } else if self.render_cap_alpha_output() {
            FxdibFormat::Bgra
        } else if cfg!(target_vendor = "apple") {
            FxdibFormat::Bgrx
        } else {
            FxdibFormat::Bgr
        };
        CfxDibitmap::create(width, height, format, 0)
    }

    pub fn bits_per_pixel(&self) -> i32 {
        get_bpp_from_format(self.bitmap.format())
    }

    pub fn render_cap_alpha_output(&self) -> bool {
        get_is_alpha_from_format(self.bitmap.format())
    }

    pub fn render_cap_byte_mask_output(&self) -> bool {
        get_is_mask_from_format(self.bitmap.format())
    }

    pub fn render_cap_get_bits(&self) -> bool {
        true
    }

    pub fn render_cap_alpha_image(&self) -> bool {
        true
    }

    pub fn render_cap_blend_mode(&self) -> bool {
        true
    }

    pub fn render_cap_soft_clip(&self) -> bool {
        true
    }

    pub fn driver_type(&self) -> i32 {
        1
    }

    pub fn clear(&mut self, color: u32) {
        self.bitmap.clear(color);
    }

    pub fn multiply_alpha(&mut self, alpha: f32) -> bool {
        self.bitmap.multiply_alpha(alpha)
    }

    pub fn multiply_alpha_mask(&mut self, mask: &CfxDibitmap) -> bool {
        self.bitmap.multiply_alpha_mask(mask)
    }

    pub fn clip_box(&self) -> FxRect {
        self.clip_region.as_ref().map_or_else(
            || FxRect::new(0, 0, self.pixel_width(), self.pixel_height()),
            CfxAggClipRgn::box_rect,
        )
    }

    pub fn get_dibits(&self, destination: &mut CfxDibitmapData<'_>, left: i32, top: i32) -> bool {
        if self.bitmap.buffer().is_empty() {
            return true;
        }
        let mut clipped_backdrop = None;
        if let Some(backdrop) = self.backdrop_bitmap {
            let Some(right) = left.checked_add(destination.width()) else {
                return false;
            };
            let Some(bottom) = top.checked_add(destination.height()) else {
                return false;
            };
            let Some(mut clipped) = backdrop.clip_to(FxRect::new(left, top, right, bottom)) else {
                return true;
            };
            clipped.composite_bitmap_normal(
                0,
                0,
                clipped.width(),
                clipped.height(),
                self.bitmap,
                0,
                0,
            );
            clipped_backdrop = Some(clipped);
        }
        let source: &CfxDibitmapData<'_> = match clipped_backdrop.as_ref() {
            Some(bitmap) => bitmap,
            None => self.bitmap,
        };
        let src_left = if clipped_backdrop.is_some() {
            left.min(0)
        } else {
            left
        };
        let src_top = if clipped_backdrop.is_some() {
            top.min(0)
        } else {
            top
        };
        if self.rgb_byte_order {
            rgb_byte_order_transfer_bitmap(
                destination,
                destination.width(),
                destination.height(),
                source,
                src_left,
                src_top,
            );
            true
        } else {
            destination.transfer_bitmap_direct_color(
                destination.width(),
                destination.height(),
                source,
                src_left,
                src_top,
            )
        }
    }

    pub fn start_dibits<S: crate::fxge::dib::cstretchengine::StretchSource>(
        &mut self,
        source: std::rc::Rc<S>,
        alpha: f32,
        mask_color: u32,
        matrix: CfxMatrix,
        options: crate::fxge::dib::fx_dib::FxdibResampleOptions,
    ) -> super::cfx_agg_imagerenderer::CfxAggImageRenderer<'_, 'storage, S> {
        self.start_dibits_with_blend(
            source,
            alpha,
            mask_color,
            matrix,
            options,
            BlendMode::Normal,
        )
    }

    pub fn start_dibits_with_blend<S: crate::fxge::dib::cstretchengine::StretchSource>(
        &mut self,
        source: std::rc::Rc<S>,
        alpha: f32,
        mask_color: u32,
        matrix: CfxMatrix,
        options: crate::fxge::dib::fx_dib::FxdibResampleOptions,
        blend_type: BlendMode,
    ) -> super::cfx_agg_imagerenderer::CfxAggImageRenderer<'_, 'storage, S> {
        // Unlike pinned AGG, retain the requested blend while resampling.
        // In particular, image overprint selects Darken in the PDF renderer.
        super::cfx_agg_imagerenderer::CfxAggImageRenderer::new_with_blend(
            self.bitmap,
            self.clip_region.as_ref(),
            source,
            alpha,
            mask_color,
            matrix,
            options,
            self.rgb_byte_order,
            blend_type,
        )
    }

    pub fn set_dibits_normal(
        &mut self,
        source: &CfxDibitmapData<'_>,
        argb: u32,
        src_rect: FxRect,
        left: i32,
        top: i32,
    ) -> bool {
        self.set_dibits(source, argb, src_rect, left, top, BlendMode::Normal)
    }

    pub fn set_dibits<S: crate::fxge::dib::cstretchengine::StretchSource>(
        &mut self,
        source: &S,
        argb: u32,
        src_rect: FxRect,
        left: i32,
        top: i32,
        blend_mode: BlendMode,
    ) -> bool {
        if self.bitmap.buffer().is_empty() {
            return true;
        }
        let Some(width) = src_rect.right.checked_sub(src_rect.left) else {
            return false;
        };
        let Some(height) = src_rect.bottom.checked_sub(src_rect.top) else {
            return false;
        };
        let Some(right) = left.checked_add(width) else {
            return false;
        };
        let Some(bottom) = top.checked_add(height) else {
            return false;
        };
        let mut target = FxRect::new(left, top, right, bottom);
        target.intersect(self.clip_box());
        if target.is_empty() {
            return true;
        }
        let Ok(src_left) =
            i32::try_from(i64::from(src_rect.left) + i64::from(target.left) - i64::from(left))
        else {
            return false;
        };
        let Ok(src_top) =
            i32::try_from(i64::from(src_rect.top) + i64::from(target.top) - i64::from(top))
        else {
            return false;
        };
        let clip = self
            .clip_region
            .as_ref()
            .and_then(CfxAggClipRgn::mask)
            .map(|mask| (mask, self.clip_box()));
        if get_is_mask_from_format(source.format()) {
            self.bitmap.composite_mask_clipped(
                target.left,
                target.top,
                target.width(),
                target.height(),
                source,
                argb,
                src_left,
                src_top,
                blend_mode,
                self.rgb_byte_order,
                clip,
            )
        } else {
            self.bitmap.composite_bitmap_clipped(
                target.left,
                target.top,
                target.width(),
                target.height(),
                source,
                src_left,
                src_top,
                blend_mode,
                clip,
                self.rgb_byte_order,
            )
        }
    }

    pub fn stretch_dibits<S: crate::fxge::dib::cstretchengine::StretchSource>(
        &mut self,
        source: Rc<S>,
        argb: u32,
        dest_left: i32,
        dest_top: i32,
        dest_width: i32,
        dest_height: i32,
        clip: FxRect,
        options: crate::fxge::dib::fx_dib::FxdibResampleOptions,
        blend_mode: BlendMode,
    ) -> bool {
        if self.bitmap.buffer().is_empty() {
            return true;
        }
        if dest_width == source.width() && dest_height == source.height() {
            return self.set_dibits(
                source.as_ref(),
                argb,
                FxRect::new(0, 0, dest_width, dest_height),
                dest_left,
                dest_top,
                blend_mode,
            );
        }
        let Some(right) = dest_left.checked_add(dest_width) else {
            return false;
        };
        let Some(bottom) = dest_top.checked_add(dest_height) else {
            return false;
        };
        let mut rect = FxRect::new(dest_left, dest_top, right, bottom);
        rect.normalize();
        let mut dest_clip = rect;
        dest_clip.intersect(clip);
        if dest_clip.is_empty() {
            return true;
        }
        let mut composer = super::cfx_agg_bitmapcomposer::CfxAggBitmapComposer::compose(
            self.bitmap,
            self.clip_region.as_ref(),
            1.0,
            argb,
            dest_clip,
            false,
            false,
            false,
            self.rgb_byte_order,
            blend_mode,
        );
        let Some(left) = dest_clip.left.checked_sub(rect.left) else {
            return false;
        };
        let Some(top) = dest_clip.top.checked_sub(rect.top) else {
            return false;
        };
        let Some(right) = dest_clip.right.checked_sub(rect.left) else {
            return false;
        };
        let Some(bottom) = dest_clip.bottom.checked_sub(rect.top) else {
            return false;
        };
        let mut stretcher = crate::fxge::dib::cfx_imagestretcher::CfxImageStretcher::new(
            source,
            dest_width,
            dest_height,
            FxRect::new(left, top, right, bottom),
            options,
        );
        if stretcher.start(&mut composer) {
            stretcher.continue_stretch(&mut composer, None);
        }
        true
    }

    pub fn set_clip_rect(&mut self, rect: FxRect) -> bool {
        self.fill_options = CfxFillRenderOptions::winding_options();
        if self.clip_region.is_none() {
            self.clip_region = Some(CfxAggClipRgn::new(self.pixel_width(), self.pixel_height()));
        }
        self.clip_region
            .as_mut()
            .expect("clip region")
            .intersect_rect(rect)
    }

    // A shallow snapshot: masks are immutable and retained through Rc. This
    // lets a render holder reuse its last clip while keeping object save/restore.
    pub(crate) fn clip_region_snapshot(&self) -> Option<CfxAggClipRgn> {
        self.clip_region.clone()
    }

    pub(crate) fn restore_clip_region(&mut self, region: Option<CfxAggClipRgn>) {
        self.clip_region = region;
    }

    pub fn save_state(&mut self) {
        self.state_stack.push(self.clip_region.clone());
    }

    pub fn restore_state(&mut self, keep_saved: bool) {
        self.clip_region = None;
        if keep_saved {
            self.clip_region = self.state_stack.last().cloned().flatten();
        } else if let Some(region) = self.state_stack.pop() {
            self.clip_region = region;
        }
    }

    fn render_rasterizer(
        &mut self,
        rasterizer: &mut RasterizerScanlineAa,
        color: u32,
        full_cover: bool,
        group_knockout: bool,
    ) -> bool {
        let backdrop = if group_knockout {
            self.backdrop_bitmap
        } else {
            None
        };
        let mut renderer = CfxAggRenderer::new(
            self.bitmap,
            backdrop,
            self.clip_region.as_ref(),
            color,
            full_cover,
            self.rgb_byte_order,
        );
        let mut scanline = ScanlineU8::new();
        render_scanlines(
            rasterizer,
            &mut scanline,
            &mut renderer,
            self.fill_options.aliased_path,
        )
    }

    fn set_clip_mask(&mut self, rasterizer: &mut RasterizerScanlineAa) -> bool {
        let clip_box = self.clip_box();
        let Some(region) = self.clip_region.as_mut() else {
            return false;
        };
        if !rasterizer.sort() {
            return region.intersect_mask(clip_box.left, clip_box.top, None);
        }
        let mut path_rect = FxRect::new(
            rasterizer.min_x(),
            rasterizer.min_y(),
            rasterizer.max_x().saturating_add(1),
            rasterizer.max_y().saturating_add(1),
        );
        path_rect.intersect(clip_box);
        if path_rect.is_empty() {
            return region.intersect_mask(path_rect.left, path_rect.top, None);
        }
        let Some(mut bitmap) = CfxDibitmap::create(
            path_rect.width(),
            path_rect.height(),
            FxdibFormat::EightBppMask,
            0,
        ) else {
            return false;
        };
        let (width, height) = (bitmap.width() as u32, bitmap.height() as u32);
        let Ok(stride) = i32::try_from(bitmap.pitch()) else {
            return false;
        };
        {
            let Some(mut raw) =
                RenderingBuffer::from_buffer(bitmap.buffer_mut(), width, height, stride)
            else {
                return false;
            };
            let mut pixels = PixfmtGray8::new(&mut raw);
            let mut base = RendererBaseGray::with_pixel_format(&mut pixels);
            let mut renderer =
                RendererScanlineAaOffset::new(&mut base, path_rect.left, path_rect.top);
            renderer.set_color(Gray8::opaque(255));
            let mut scanline = ScanlineU8::new();
            if !render_scanlines(
                rasterizer,
                &mut scanline,
                &mut renderer,
                self.fill_options.aliased_path,
            ) {
                return false;
            }
        }
        region.intersect_mask(path_rect.left, path_rect.top, Some(Rc::new(bitmap)))
    }

    pub fn set_clip_path_fill(
        &mut self,
        path: &CfxPath,
        object_to_device: Option<CfxMatrix>,
        options: CfxFillRenderOptions,
    ) -> bool {
        debug_assert_ne!(options.fill_type, FillType::NoFill);
        self.fill_options = options;
        if self.clip_region.is_none() {
            self.clip_region = Some(CfxAggClipRgn::new(self.pixel_width(), self.pixel_height()));
        }
        if let Some(mut rect) = path.rect(object_to_device.as_ref()) {
            rect.intersect(CfxFloatRect::new(
                0.0,
                0.0,
                self.pixel_width() as f32,
                self.pixel_height() as f32,
            ));
            return self
                .clip_region
                .as_mut()
                .unwrap()
                .intersect_rect(rect.get_outer_rect());
        }
        let (mut path_data, origin) = build_agg_path_with_origin(path, object_to_device);
        path_data.end_poly();
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.clip_box(
            0.0,
            0.0,
            self.pixel_width() as f32,
            self.pixel_height() as f32,
        );
        if let Some(origin) = origin {
            rasterizer.add_path_offset(&mut path_data, 0, origin.0, origin.1);
        } else {
            rasterizer.add_path(&mut path_data, 0);
        }
        rasterizer.set_filling_rule(get_alternate_or_winding_fill_type(options));
        self.set_clip_mask(&mut rasterizer)
    }

    pub fn set_clip_path_stroke(
        &mut self,
        path: &CfxPath,
        object_to_device: Option<CfxMatrix>,
        graph_state: &CfxGraphStateData,
    ) -> bool {
        if self.clip_region.is_none() {
            self.clip_region = Some(CfxAggClipRgn::new(self.pixel_width(), self.pixel_height()));
        }
        let mut path_data = build_agg_path(path, None);
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.clip_box(
            0.0,
            0.0,
            self.pixel_width() as f32,
            self.pixel_height() as f32,
        );
        rasterize_stroke(
            &mut rasterizer,
            &mut path_data,
            object_to_device,
            graph_state,
            1.0,
            false,
        );
        rasterizer.set_filling_rule(FillingRule::NonZero);
        self.set_clip_mask(&mut rasterizer)
    }

    pub fn draw_path(
        &mut self,
        path: &CfxPath,
        object_to_device: Option<CfxMatrix>,
        graph_state: Option<&CfxGraphStateData>,
        fill_color: u32,
        stroke_color: u32,
        group_knockout: bool,
        options: CfxFillRenderOptions,
    ) -> bool {
        let group_knockout = group_knockout || self.group_knockout;
        if self.bitmap.buffer().is_empty() {
            return true;
        }
        self.fill_options = options;
        if options.fill_type != FillType::NoFill && fill_color != 0 {
            let (mut path_data, origin) = build_agg_path_with_origin(path, object_to_device);
            let mut rasterizer = RasterizerScanlineAa::new();
            rasterizer.clip_box(
                0.0,
                0.0,
                self.pixel_width() as f32,
                self.pixel_height() as f32,
            );
            if let Some(origin) = origin {
                rasterizer.add_path_offset(&mut path_data, 0, origin.0, origin.1);
            } else {
                rasterizer.add_path(&mut path_data, 0);
            }
            rasterizer.set_filling_rule(get_alternate_or_winding_fill_type(options));
            if !self.render_rasterizer(&mut rasterizer, fill_color, options.full_cover, false) {
                return false;
            }
        }
        let Some(graph_state) = graph_state else {
            return true;
        };
        if stroke_color >> 24 == 0 {
            return true;
        }
        if options.zero_area {
            let mut path_data = build_agg_path(path, object_to_device);
            let mut rasterizer = RasterizerScanlineAa::new();
            rasterizer.clip_box(
                0.0,
                0.0,
                self.pixel_width() as f32,
                self.pixel_height() as f32,
            );
            rasterize_stroke(
                &mut rasterizer,
                &mut path_data,
                None,
                graph_state,
                1.0,
                options.stroke_text_mode,
            );
            return self.render_rasterizer(
                &mut rasterizer,
                stroke_color,
                options.full_cover,
                group_knockout,
            );
        }
        let (mut matrix1, mut matrix2) = if let Some(matrix) = object_to_device {
            let scale = matrix.a.abs().max(matrix.b.abs());
            let mut matrix2 = CfxMatrix::new(
                matrix.a / scale,
                matrix.b / scale,
                matrix.c / scale,
                matrix.d / scale,
                0.0,
                0.0,
            );
            let mut matrix1 = matrix * matrix2.get_inverse();
            // Translation belongs to the final device transform, not the path
            // on which the stroke is constructed. Passing it through the inverse
            // and back can move an exact pixel boundary by one subpixel (for
            // example 4 -> 3.9999998), making a region differ from a whole crop.
            matrix1.e = 0.0;
            matrix1.f = 0.0;
            matrix2.e = matrix.e;
            matrix2.f = matrix.f;
            (matrix1, matrix2)
        } else {
            (CfxMatrix::default(), CfxMatrix::default())
        };
        // Stroke construction happens before matrix2's translation. Center only
        // unusually large source coordinates so the safety clamp cannot discard
        // an on-page stroke. The center depends on the path and linear scale,
        // not on the tile origin; ordinary paths retain their exact arithmetic.
        let bounds = matrix1.transform_float_rect(path.bounding_box());
        if ![bounds.left, bounds.right, bounds.bottom, bounds.top]
            .iter()
            .all(|v| v.is_finite())
        {
            return false;
        }
        if bounds.left < -MAX_POS
            || bounds.right > MAX_POS
            || bounds.bottom < -MAX_POS
            || bounds.top > MAX_POS
        {
            let x = ((f64::from(bounds.left) + f64::from(bounds.right)) * 0.5).floor();
            let y = ((f64::from(bounds.bottom) + f64::from(bounds.top)) * 0.5).floor();
            matrix1.e = -x as f32;
            matrix1.f = -y as f32;
            matrix2.e =
                (f64::from(matrix2.e) + x * f64::from(matrix2.a) + y * f64::from(matrix2.c)) as f32;
            matrix2.f =
                (f64::from(matrix2.f) + x * f64::from(matrix2.b) + y * f64::from(matrix2.d)) as f32;
        }
        let mut path_data = build_agg_path(path, Some(matrix1));
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.clip_box(
            0.0,
            0.0,
            self.pixel_width() as f32,
            self.pixel_height() as f32,
        );
        rasterize_stroke(
            &mut rasterizer,
            &mut path_data,
            Some(matrix2),
            graph_state,
            matrix1.a,
            options.stroke_text_mode,
        );
        self.render_rasterizer(
            &mut rasterizer,
            stroke_color,
            options.full_cover,
            group_knockout,
        )
    }

    pub fn fill_rect(&mut self, rect: FxRect, fill_color: u32) -> bool {
        if self.bitmap.buffer().is_empty() {
            return true;
        }
        let clip_rect = self.clip_box();
        let mut draw_rect = clip_rect;
        draw_rect.intersect(rect);
        if draw_rect.is_empty() {
            return true;
        }
        if let Some(mask) = self.clip_region.as_ref().and_then(CfxAggClipRgn::mask) {
            self.bitmap.composite_mask_normal(
                draw_rect.left,
                draw_rect.top,
                draw_rect.width(),
                draw_rect.height(),
                mask,
                fill_color,
                draw_rect.left - clip_rect.left,
                draw_rect.top - clip_rect.top,
                self.rgb_byte_order,
            );
        } else if self.rgb_byte_order {
            rgb_byte_order_composite_rect(self.bitmap, draw_rect, fill_color);
        } else {
            self.bitmap.composite_rect(
                draw_rect.left,
                draw_rect.top,
                draw_rect.width(),
                draw_rect.height(),
                fill_color,
            );
        }
        // Native FillRect ignores the bitmap compositor's return value.
        true
    }
}

pub struct RendererScanlineAaOffset<'a, 'p, 'r, 'b> {
    renderer: &'a mut RendererBaseGray<'p, 'r, 'b>,
    color: Gray8,
    left: i32,
    top: i32,
}

impl<'a, 'p, 'r, 'b> RendererScanlineAaOffset<'a, 'p, 'r, 'b> {
    pub fn new(renderer: &'a mut RendererBaseGray<'p, 'r, 'b>, left: i32, top: i32) -> Self {
        Self {
            renderer,
            color: Gray8::new(0, 0),
            left,
            top,
        }
    }

    pub fn set_color(&mut self, color: Gray8) {
        self.color = color;
    }

    pub fn color(&self) -> Gray8 {
        self.color
    }
}

impl ScanlineRenderer for RendererScanlineAaOffset<'_, '_, '_, '_> {
    fn supports_solid_spans(&self) -> bool {
        true
    }
    fn prepare(&mut self, _max_span_len: u32) {}

    fn render(&mut self, scanline: &ScanlineU8) {
        let y = scanline.y();
        for span in scanline.spans() {
            let x = i32::from(span.x);
            if span.solid {
                self.renderer.blend_hline(
                    x - self.left,
                    y - self.top,
                    x + i32::from(span.len) - 1 - self.left,
                    self.color,
                    span.covers[0],
                );
            } else if span.len > 0 {
                self.renderer.blend_solid_hspan(
                    x - self.left,
                    y - self.top,
                    i32::from(span.len),
                    self.color,
                    span.covers,
                );
            } else {
                self.renderer.blend_hline(
                    x - self.left,
                    y - self.top,
                    x - i32::from(span.len) - 1 - self.left,
                    self.color,
                    span.covers[0],
                );
            }
        }
    }
}

impl ColoredScanlineRenderer<Gray8> for RendererScanlineAaOffset<'_, '_, '_, '_> {
    fn color(&mut self, color: Gray8) {
        self.set_color(color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agg::agg_basics::{PATH_CMD_END_POLY, PATH_CMD_LINE_TO, PATH_FLAGS_CLOSE};
    use crate::agg::agg_path_storage::PathStorage;
    use crate::agg::agg_pixfmt_gray::PixfmtGray8;
    use crate::agg::agg_rasterizer_scanline_aa::RasterizerScanlineAa;
    use crate::agg::agg_render_scanlines::render_scanlines;
    use crate::agg::agg_rendering_buffer::RenderingBuffer;

    #[test]
    fn rasterized_clip_mask_respects_bitmap_offset() {
        let mut path = PathStorage::new();
        path.move_to(2.0, 3.0);
        path.line_to(4.0, 3.0);
        path.line_to(4.0, 5.0);
        path.line_to(2.0, 5.0);
        path.end_poly();
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.add_path(&mut path, 0);
        let mut bytes = [0u8; 9];
        let mut buffer = RenderingBuffer::from_buffer(&mut bytes, 3, 3, 3).unwrap();
        let mut pixels = PixfmtGray8::new(&mut buffer);
        let mut base = RendererBaseGray::with_pixel_format(&mut pixels);
        let mut renderer = RendererScanlineAaOffset::new(&mut base, 1, 2);
        renderer.set_color(Gray8::opaque(255));
        let mut scanline = ScanlineU8::new();
        assert!(render_scanlines(
            &mut rasterizer,
            &mut scanline,
            &mut renderer,
            false
        ));
        assert_eq!(base.pixel_format().pixel(1, 1).unwrap().v, 255);
        assert_eq!(base.pixel_format().pixel(2, 2).unwrap().v, 255);
        assert_eq!(base.pixel_format().pixel(0, 0).unwrap().v, 0);
        assert_eq!(base.pixel_format().pixel(0, 1).unwrap().v, 0);
    }

    #[test]
    fn filled_curve_culling_preserves_transformed_edge_pixels() {
        let mut path = CfxPath::new();
        for (x, y, kind) in [
            (0.13, 5.27, PointType::Move),
            (-2.21, -3.73, PointType::Bezier),
            (14.63, -1.17, PointType::Bezier),
            (10.39, 6.81, PointType::Bezier),
            (6.19, 14.37, PointType::Bezier),
            (-3.41, 11.23, PointType::Bezier),
            (0.13, 5.27, PointType::Bezier),
        ] {
            path.append_point(CfxPointF::new(x, y), kind);
        }
        let clip = FxRect::new(0, 0, 16, 16);
        let mut rejected = 0;
        let mut painted = 0;
        for (a, b, c, d) in [
            (1.0, 0.0, 0.0, 1.0),
            (-1.0, 0.0, 0.0, 1.0),
            (0.5, 0.25, -0.25, 0.75),
            (0.0, 2.0, -1.0, 0.0),
        ] {
            for offset in [-32.0, -16.001, -0.001, 0.0, 15.999, 16.0, 32.0] {
                let matrix = CfxMatrix::new(a, b, c, d, offset, offset * 0.5);
                let intersects = filled_bezier_path_intersects_clip(&path, matrix, clip);
                let mut bitmap = CfxDibitmap::create(16, 16, FxdibFormat::Bgra, 0).unwrap();
                {
                    let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
                    assert!(driver.draw_path(
                        &path,
                        Some(matrix),
                        None,
                        0xff13579b,
                        0,
                        false,
                        CfxFillRenderOptions::winding_options()
                    ));
                }
                let visible = bitmap.buffer().iter().any(|&byte| byte != 0);
                if !intersects {
                    rejected += 1;
                    assert!(!visible, "rejected visible curve at {matrix:?}");
                }
                painted += usize::from(visible);
            }
        }
        assert!(rejected > 5 && painted > 5);
        // Uncertain inputs and cosmetic-line/rectangle paths retain the broad
        // culling behavior instead of assuming ordinary filled-curve semantics.
        assert!(filled_bezier_path_intersects_clip(
            &path,
            CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 1e20, 0.0),
            clip
        ));
        assert!(filled_bezier_path_intersects_clip(
            &path,
            CfxMatrix::new(f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0),
            clip
        ));
        let mut malformed = CfxPath::new();
        for kind in [
            PointType::Move,
            PointType::Bezier,
            PointType::Bezier,
            PointType::Bezier,
            PointType::Bezier,
        ] {
            malformed.append_point(CfxPointF::new(-10.0, -10.0), kind);
        }
        assert!(filled_bezier_path_intersects_clip(
            &malformed,
            CfxMatrix::default(),
            clip
        ));
        let mut line = CfxPath::new();
        line.append_point(CfxPointF::new(-10.0, 0.0), PointType::Move);
        line.append_point(CfxPointF::new(-10.0, 2.0), PointType::Line);
        assert!(filled_bezier_path_intersects_clip(
            &line,
            CfxMatrix::default(),
            clip
        ));
    }

    #[test]
    fn path_builder_preserves_degenerate_line_and_curve_closure() {
        let mut path = CfxPath::new();
        path.append_point(CfxPointF::new(2.0, 3.0), PointType::Move);
        path.append_point(CfxPointF::new(2.0, 3.0), PointType::Line);
        let agg_path = build_agg_path(&path, None);
        let (mut x, mut y) = (0.0, 0.0);
        assert_eq!(agg_path.vertex_at(1, &mut x, &mut y), PATH_CMD_LINE_TO);
        assert_eq!((x, y), (3.0, 3.0));

        path.clear();
        path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
        path.append_point(CfxPointF::new(1.0, 0.0), PointType::Bezier);
        path.append_point(CfxPointF::new(2.0, 1.0), PointType::Bezier);
        path.append_point_and_close(CfxPointF::new(3.0, 1.0), PointType::Bezier);
        let agg_path = build_agg_path(&path, None);
        assert!(agg_path.total_vertices() > 3);
        assert_eq!(
            agg_path.last_vertex(&mut x, &mut y),
            PATH_CMD_END_POLY | PATH_FLAGS_CLOSE
        );
    }

    #[test]
    fn dashed_stroke_leaves_the_gap_uncovered() {
        let mut path = PathStorage::new();
        path.move_to(0.0, 0.0);
        path.line_to(10.0, 0.0);
        let mut graph_state = CfxGraphStateData::default();
        graph_state.set_line_width(2.0);
        graph_state.set_dash_array(vec![3.0, 2.0]);
        let mut rasterizer = RasterizerScanlineAa::new();
        rasterizer.clip_box(0.0, -2.0, 10.0, 2.0);
        rasterize_stroke(&mut rasterizer, &mut path, None, &graph_state, 1.0, false);
        assert!(rasterizer.rewind_scanlines());
        let mut scanline = ScanlineU8::new();
        assert!(scanline.reset(rasterizer.min_x(), rasterizer.max_x()));
        let mut covered_x = Vec::new();
        while rasterizer.sweep_scanline(&mut scanline, false) {
            if scanline.y() == 0 {
                for span in scanline.spans() {
                    covered_x
                        .extend((0..span.len).map(|offset| i32::from(span.x) + i32::from(offset)));
                }
            }
        }
        assert!(covered_x.contains(&1));
        assert!(covered_x.contains(&6));
        assert!(!covered_x.contains(&4));
    }

    #[test]
    fn filled_curves_preserve_coverage_and_holes_across_tile_origins() {
        let mut path = CfxPath::new();
        path.append_point(CfxPointF::new(-12.35, 15.7), PointType::Move);
        path.append_point(CfxPointF::new(19.81, -18.2), PointType::Bezier);
        path.append_point(CfxPointF::new(84.7, 18.13), PointType::Bezier);
        path.append_point(CfxPointF::new(56.13, 57.81), PointType::Bezier);
        path.append_point(CfxPointF::new(22.17, 92.7), PointType::Bezier);
        path.append_point(CfxPointF::new(-19.31, 47.2), PointType::Bezier);
        path.append_point_and_close(CfxPointF::new(-12.35, 15.7), PointType::Bezier);
        path.append_rect(15.3, 17.6, 35.8, 37.9);
        for options in [
            CfxFillRenderOptions::winding_options(),
            CfxFillRenderOptions {
                fill_type: FillType::EvenOdd,
                ..Default::default()
            },
        ] {
            let render = |x: i32, y: i32, width: i32, height: i32| {
                let mut bitmap = CfxDibitmap::create(width, height, FxdibFormat::Bgra, 0).unwrap();
                let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
                assert!(driver.draw_path(
                    &path,
                    Some(CfxMatrix::new(
                        1.25,
                        0.0,
                        0.0,
                        1.25,
                        12.5 - x as f32,
                        10.25 - y as f32
                    )),
                    None,
                    0xff193b7c,
                    0,
                    false,
                    options
                ));
                drop(driver);
                bitmap
            };
            let whole = render(0, 0, 100, 100);
            for (x, y, width, height) in [
                (0, 0, 13, 17),
                (13, 17, 31, 37),
                (44, 54, 56, 46),
                (33, 0, 67, 100),
            ] {
                let tile = render(x, y, width, height);
                for row in 0..height {
                    let expected =
                        &whole.scanline(y + row).unwrap()[x as usize * 4..(x + width) as usize * 4];
                    let actual = &tile.scanline(row).unwrap()[..width as usize * 4];
                    // Rounded geometric intersections may change AA coverage,
                    // but holes and edge positions must remain intact.
                    for (channel, (&a, &b)) in expected.iter().zip(actual).enumerate() {
                        assert!(
                            a.abs_diff(b) <= 2,
                            "origin {x},{y}, row {row}, channel {channel}: {a} vs {b}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn translated_large_source_strokes_keep_their_visible_borders() {
        let render = |offset: CfxPointF, tile_x: i32, tile_y: i32| {
            let mut bitmap =
                CfxDibitmap::create(160 - tile_x, 128 - tile_y, FxdibFormat::Bgra, 0).unwrap();
            bitmap.clear(0xffffffff);
            let mut path = CfxPath::new();
            path.append_rect(
                10. + offset.x,
                10. + offset.y,
                40. + offset.x,
                30. + offset.y,
            );
            let mut graph = CfxGraphStateData::default();
            graph.set_line_width(1.);
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            assert!(driver.draw_path(
                &path,
                Some(CfxMatrix::new(
                    3.,
                    0.,
                    0.,
                    3.,
                    -3. * offset.x - tile_x as f32,
                    -3. * offset.y - tile_y as f32
                )),
                Some(&graph),
                0,
                0xff000000,
                false,
                CfxFillRenderOptions::default()
            ));
            drop(driver);
            bitmap
        };
        let reference = render(CfxPointF::default(), 0, 0);
        // Independent visible ink and blank interior, in addition to the
        // equivalent-coordinate control. Both sides of the safety bound matter.
        assert_eq!(
            &reference.scanline(50).unwrap()[30 * 4..31 * 4],
            &[0, 0, 0, 255]
        );
        assert_eq!(&reference.scanline(50).unwrap()[70 * 4..71 * 4], &[255; 4]);
        for offset in [
            CfxPointF::new(10800., 0.),
            CfxPointF::new(-10800., 0.),
            CfxPointF::new(0., 10800.),
            CfxPointF::new(0., -10800.),
        ] {
            let actual = render(offset, 0, 0);
            assert_eq!(actual.buffer(), reference.buffer(), "offset {offset:?}");
            let tile = render(offset, 7, 11);
            for row in 0..tile.height() {
                assert_eq!(
                    tile.scanline(row).unwrap(),
                    &reference.scanline(row + 11).unwrap()[7 * 4..],
                    "tile offset {offset:?}, row {row}"
                );
            }
        }
    }

    #[test]
    fn stroke_translation_preserves_exact_region_boundary_coverage() {
        fn render(width: i32, height: i32, matrix: CfxMatrix) -> CfxDibitmap {
            let mut bitmap = CfxDibitmap::create(width, height, FxdibFormat::Bgra, 0).unwrap();
            let mut path = CfxPath::new();
            path.append_point(CfxPointF::new(30.8, 0.0), PointType::Move);
            path.append_point(CfxPointF::new(30.8, 127.0), PointType::Line);
            let mut graph = CfxGraphStateData::default();
            graph.set_line_width(0.0);
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            assert!(driver.draw_path(
                &path,
                Some(matrix),
                Some(&graph),
                0,
                0xff5078d2,
                false,
                CfxFillRenderOptions::default()
            ));
            drop(driver);
            bitmap
        }
        // The anisotropic scale comes from independently rounded pixel dimensions
        // of a 173x127 page at 1.25x. The final four-row tile ends exactly at y=4.
        let matrix = CfxMatrix::new(217.0 / 173.0, 0.0, 0.0, -159.0 / 127.0, 0.0, 159.0);
        let whole = render(217, 159, matrix);
        let tile = render(
            31,
            4,
            CfxMatrix {
                e: -31.0,
                f: 4.0,
                ..matrix
            },
        );
        assert!(whole.scanline(158).unwrap()[38 * 4 + 3] > 0);
        for row in 0..4 {
            assert_eq!(
                &whole.scanline(155 + row).unwrap()[31 * 4..62 * 4],
                &tile.scanline(row).unwrap()[..31 * 4]
            );
        }
    }

    #[test]
    fn draw_path_fills_owned_bgra_bitmap() {
        let mut bitmap = CfxDibitmap::create(4, 4, FxdibFormat::Bgra, 0).unwrap();
        let mut path = CfxPath::new();
        path.append_rect(1.0, 1.0, 3.0, 3.0);
        {
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            assert!(driver.draw_path(
                &path,
                None,
                None,
                0xff112233,
                0,
                false,
                CfxFillRenderOptions::winding_options(),
            ));
        }
        assert_eq!(
            &bitmap.scanline(1).unwrap()[4..8],
            &[0x33, 0x22, 0x11, 0xff]
        );
        assert_eq!(
            &bitmap.scanline(2).unwrap()[8..12],
            &[0x33, 0x22, 0x11, 0xff]
        );
        assert_eq!(&bitmap.scanline(0).unwrap()[4..8], &[0, 0, 0, 0]);
        assert_eq!(&bitmap.scanline(3).unwrap()[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn nonrectangular_clip_mask_limits_path_fill() {
        let mut bitmap = CfxDibitmap::create(5, 5, FxdibFormat::Bgra, 0).unwrap();
        let mut clip_path = CfxPath::new();
        clip_path.append_point(CfxPointF::new(2.0, 0.0), PointType::Move);
        clip_path.append_point(CfxPointF::new(4.0, 2.0), PointType::Line);
        clip_path.append_point(CfxPointF::new(2.0, 4.0), PointType::Line);
        clip_path.append_point_and_close(CfxPointF::new(0.0, 2.0), PointType::Line);
        let mut fill_path = CfxPath::new();
        fill_path.append_rect(0.0, 0.0, 5.0, 5.0);
        {
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            assert!(driver.set_clip_path_fill(
                &clip_path,
                None,
                CfxFillRenderOptions::winding_options(),
            ));
            assert!(driver.draw_path(
                &fill_path,
                None,
                None,
                0xff0000ff,
                0,
                false,
                CfxFillRenderOptions::winding_options(),
            ));
        }
        assert_eq!(bitmap.scanline(2).unwrap()[2 * 4], 255);
        assert_eq!(bitmap.scanline(0).unwrap()[0], 0);
        assert_eq!(bitmap.scanline(4).unwrap()[4 * 4], 0);
    }

    #[test]
    fn fill_rect_multiplies_nonrectangular_clip_mask() {
        let mut bitmap = CfxDibitmap::create(5, 5, FxdibFormat::Bgra, 0).unwrap();
        let mut clip_path = CfxPath::new();
        clip_path.append_point(CfxPointF::new(2.0, 0.0), PointType::Move);
        clip_path.append_point(CfxPointF::new(4.0, 2.0), PointType::Line);
        clip_path.append_point(CfxPointF::new(2.0, 4.0), PointType::Line);
        clip_path.append_point_and_close(CfxPointF::new(0.0, 2.0), PointType::Line);
        {
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            assert!(driver.set_clip_path_fill(
                &clip_path,
                None,
                CfxFillRenderOptions::winding_options(),
            ));
            assert!(driver.fill_rect(FxRect::new(0, 0, 5, 5), 0xff00ff00));
        }
        assert_eq!(&bitmap.scanline(2).unwrap()[8..12], &[0, 255, 0, 255]);
        assert_eq!(&bitmap.scanline(0).unwrap()[..4], &[0, 255, 0, 0]);
    }

    #[test]
    fn rgb_order_rectangle_keeps_source_alpha_behavior() {
        let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        bitmap.scanline_mut(0).unwrap()[..4].copy_from_slice(&[0, 0, 255, 128]);
        {
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, true, None);
            assert!(driver.fill_rect(FxRect::new(0, 0, 1, 1), 0x80ff0000));
        }
        assert_eq!(&bitmap.scanline(0).unwrap()[..4], &[170, 0, 85, 128]);
    }

    #[test]
    fn rgb_order_readback_swaps_channels_and_preserves_alpha() {
        let mut bitmap = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
        bitmap.scanline_mut(0).unwrap()[..8].copy_from_slice(&[10, 20, 30, 40, 50, 60, 70, 80]);
        let driver = CfxAggDeviceDriver::new(&mut bitmap, true, None);
        let mut destination = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
        assert!(driver.get_dibits(&mut destination, 0, 0));
        assert_eq!(
            &destination.scanline(0).unwrap()[..8],
            &[30, 20, 10, 40, 70, 60, 50, 80]
        );

        let mut offset = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        assert!(driver.get_dibits(&mut offset, -1, 0));
        assert_eq!(&offset.scanline(0).unwrap()[..4], &[0, 0, 0, 0]);
        assert_eq!(&offset.scanline(0).unwrap()[4..8], &[30, 20, 10, 40]);

        let mut opaque = CfxDibitmap::create(2, 1, FxdibFormat::Bgr, 0).unwrap();
        opaque.scanline_mut(0).unwrap()[..6].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        let opaque_driver = CfxAggDeviceDriver::new(&mut opaque, true, None);
        let mut argb = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
        assert!(opaque_driver.get_dibits(&mut argb, 0, 0));
        assert_eq!(
            &argb.scanline(0).unwrap()[..8],
            &[3, 2, 1, 255, 6, 5, 4, 255]
        );

        let mut bgrx = CfxDibitmap::create(1, 1, FxdibFormat::Bgrx, 0).unwrap();
        bgrx.scanline_mut(0).unwrap()[..4].copy_from_slice(&[12, 34, 56, 78]);
        let bgrx_driver = CfxAggDeviceDriver::new(&mut bgrx, true, None);
        let mut rgb = CfxDibitmap::create(1, 1, FxdibFormat::Bgr, 0).unwrap();
        assert!(bgrx_driver.get_dibits(&mut rgb, 0, 0));
        assert_eq!(&rgb.scanline(0).unwrap()[..3], &[56, 34, 12]);
    }

    #[test]
    fn readback_composes_foreground_over_backdrop() {
        let mut foreground = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        foreground.clear(0x80ff0000);
        let mut backdrop = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        backdrop.clear(0xff0000ff);
        let driver = CfxAggDeviceDriver::new(&mut foreground, false, Some(&backdrop));
        let mut destination = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        assert!(driver.get_dibits(&mut destination, 0, 0));
        assert_eq!(&destination.scanline(0).unwrap()[..4], &[127, 0, 128, 255]);
    }

    #[test]
    fn set_dibits_dispatches_mask_and_color_with_rectangular_clip() {
        let mut bitmap = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        let mut mask = CfxDibitmap::create(2, 1, FxdibFormat::EightBppMask, 0).unwrap();
        mask.scanline_mut(0).unwrap()[..2].copy_from_slice(&[255, 128]);
        let mut color = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
        color.scanline_mut(0).unwrap()[..8].copy_from_slice(&[0, 255, 0, 255, 0, 0, 255, 255]);
        {
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut clip = CfxPath::new();
            clip.append_rect(1.0, 0.0, 3.0, 1.0);
            assert!(driver.set_clip_path_fill(
                &clip,
                None,
                CfxFillRenderOptions::winding_options(),
            ));
            assert!(driver.set_dibits_normal(&mask, 0xffff0000, FxRect::new(0, 0, 2, 1), 0, 0));
            assert!(driver.set_dibits_normal(&color, 0, FxRect::new(0, 0, 2, 1), 1, 0));
        }
        assert_eq!(&bitmap.scanline(0).unwrap()[..4], &[0, 0, 0, 0]);
        assert_eq!(&bitmap.scanline(0).unwrap()[4..8], &[0, 255, 0, 255]);
        assert_eq!(&bitmap.scanline(0).unwrap()[8..12], &[0, 0, 255, 255]);
    }

    #[test]
    fn set_dibits_multiplies_mask_by_nonrectangular_clip() {
        let mut bitmap = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        let mut clip_mask = CfxDibitmap::create(3, 1, FxdibFormat::EightBppMask, 0).unwrap();
        clip_mask.scanline_mut(0).unwrap()[..3].copy_from_slice(&[255, 128, 0]);
        let mut mask = CfxDibitmap::create(3, 1, FxdibFormat::EightBppMask, 0).unwrap();
        mask.scanline_mut(0).unwrap()[..3].fill(255);
        let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut clip_region = CfxAggClipRgn::new(3, 1);
        assert!(clip_region.intersect_mask(0, 0, Some(Rc::new(clip_mask))));
        driver.clip_region = Some(clip_region);
        assert!(driver.set_dibits_normal(&mask, 0xffff0000, FxRect::new(0, 0, 3, 1), 0, 0));
        assert_eq!(
            &bitmap.scanline(0).unwrap()[..12],
            &[0, 0, 255, 255, 0, 0, 255, 128, 0, 0, 255, 0]
        );

        let mut bitmap = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        let mut color = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        color.clear(0xff00ff00);
        let mut region = CfxAggClipRgn::new(3, 1);
        let mut coverage = CfxDibitmap::create(3, 1, FxdibFormat::EightBppMask, 0).unwrap();
        coverage.scanline_mut(0).unwrap()[..3].copy_from_slice(&[255, 128, 0]);
        assert!(region.intersect_mask(0, 0, Some(Rc::new(coverage))));
        let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        driver.clip_region = Some(region);
        assert!(driver.set_dibits_normal(&color, 0, FxRect::new(0, 0, 3, 1), 0, 0));
        assert_eq!(
            &bitmap.scanline(0).unwrap()[..12],
            &[0, 255, 0, 255, 0, 255, 0, 128, 0, 255, 0, 0]
        );

        let mut rgb_bitmap = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        let mut bgra_source = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        bgra_source.scanline_mut(0).unwrap()[..4].copy_from_slice(&[10, 20, 30, 128]);
        let mut rgb_driver = CfxAggDeviceDriver::new(&mut rgb_bitmap, true, None);
        assert!(rgb_driver.set_dibits_normal(&bgra_source, 0, FxRect::new(0, 0, 1, 1), 0, 0,));
        assert_eq!(&rgb_bitmap.scanline(0).unwrap()[..4], &[30, 20, 10, 128]);

        rgb_bitmap.clear(0);
        let mut clip_mask = CfxDibitmap::create(1, 1, FxdibFormat::EightBppMask, 0).unwrap();
        clip_mask.scanline_mut(0).unwrap()[0] = 128;
        let mut region = CfxAggClipRgn::new(1, 1);
        assert!(region.intersect_mask(0, 0, Some(Rc::new(clip_mask))));
        let mut rgb_driver = CfxAggDeviceDriver::new(&mut rgb_bitmap, true, None);
        rgb_driver.clip_region = Some(region);
        assert!(rgb_driver.set_dibits_normal(&bgra_source, 0, FxRect::new(0, 0, 1, 1), 0, 0,));
        assert_eq!(&rgb_bitmap.scanline(0).unwrap()[..4], &[30, 20, 10, 64]);
    }
    #[test]
    fn stretched_and_unstretched_bitmaps_dispatch_non_normal_blending() {
        use crate::fxge::dib::fx_dib::FxdibResampleOptions;
        let mut source = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        source.clear(0xff808080);
        let source = Rc::new(source);
        for width in [1, 2] {
            let mut bitmap = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
            bitmap.clear(0xff808080);
            {
                let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
                assert!(driver.stretch_dibits(
                    Rc::clone(&source),
                    0,
                    0,
                    0,
                    width,
                    1,
                    FxRect::new(0, 0, 2, 1),
                    FxdibResampleOptions {
                        no_smoothing: true,
                        ..Default::default()
                    },
                    BlendMode::Multiply
                ));
            }
            assert_eq!(&bitmap.buffer()[..4], &[64, 64, 64, 255]);
        }
    }
}

#[path = "cfx_agg_devicedriver_task.rs"]
mod task;
