// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! AGG path preparation, glyph positioning and mask/LCD drawing from
//! `core/fxge/cfx_renderdevice.cpp`; Windows/Skia backends are excluded.

use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPoint, CfxPointF, FxRect};
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use crate::fxge::cfx_fillrenderoptions::{CfxFillRenderOptions, FillType};
use crate::fxge::cfx_font::CfxFont;
use crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap;
use crate::fxge::cfx_graphstatedata::CfxGraphStateData;
use crate::fxge::cfx_path::CfxPath;
use crate::fxge::cfx_textrenderoptions::{AliasingType, CfxTextRenderOptions};
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::{alpha_merge, argb_a, FxdibFormat};
use crate::fxge::fx_font::FontAntiAliasingMode;
use std::rc::Rc;

use super::fx_font::{get_glyphs_bbox, get_glyphs_bbox_iter};
use super::text_char_pos::TextCharPos;
use super::text_glyph_pos::TextGlyphPos;

// `kTextGammaAdjust` from CFX_RenderDevice::DrawNormalText().
const TEXT_GAMMA_ADJUST: [u8; 256] = [
    0, 2, 3, 4, 6, 7, 8, 10, 11, 12, 13, 15, 16, 17, 18, 19, 21, 22, 23, 24, 25, 26, 27, 29, 30,
    31, 32, 33, 34, 35, 36, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 51, 52, 53, 54, 55, 56,
    57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81,
    82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103,
    104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122,
    123, 124, 125, 126, 127, 128, 129, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140,
    141, 142, 143, 144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 156, 157, 158,
    159, 160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 174, 175, 176,
    177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 190, 191, 192, 193, 194,
    195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 204, 205, 206, 207, 208, 209, 210, 211, 212,
    213, 214, 215, 216, 217, 217, 218, 219, 220, 221, 222, 223, 224, 225, 226, 227, 228, 228, 229,
    230, 231, 232, 233, 234, 235, 236, 237, 238, 239, 239, 240, 241, 242, 243, 244, 245, 246, 247,
    248, 249, 250, 250, 251, 252, 253, 254, 255,
];

/// CFX_RenderDevice::DrawPath for the selected AGG display device. This layer
/// handles rectangle rounding, zero-area fills and fill/stroke knockout before
/// dispatching to CFX_AggDeviceDriver::DrawPath.
pub fn draw_path_agg(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    path: &CfxPath,
    matrix: Option<CfxMatrix>,
    graph: Option<&CfxGraphStateData>,
    fill_color: u32,
    stroke_color: u32,
    options: CfxFillRenderOptions,
) -> bool {
    let fill = options.fill_type != FillType::NoFill;
    let fill_alpha = if fill { argb_a(fill_color) } else { 0 };
    let stroke_alpha = graph.map_or(0, |_| argb_a(stroke_color));
    let points = path.points();
    if stroke_alpha == 0 && points.len() == 2 {
        let first = matrix.map_or(points[0].point, |matrix| matrix.transform(points[0].point));
        let second = matrix.map_or(points[1].point, |matrix| matrix.transform(points[1].point));
        draw_cosmetic_line_agg(driver, first, second, fill_color, options);
        return true;
    }
    if stroke_alpha == 0 && !options.rect_aa {
        // Snap before integer device translation. Adding a tile offset in f32
        // can change which of two nearly equal margins wins the edge-trimming
        // comparison (for example an annotation's 10..50 rectangle at 20/3x).
        let mut rectangle_matrix = matrix;
        let mut origin = (0, 0);
        if let Some(local) = rectangle_matrix.as_mut() {
            let x = f64::from(local.e).floor();
            let y = f64::from(local.f).floor();
            if (i32::MIN as f64..=i32::MAX as f64).contains(&x)
                && (i32::MIN as f64..=i32::MAX as f64).contains(&y)
            {
                origin = (x as i32, y as i32);
                local.e -= origin.0 as f32;
                local.f -= origin.1 as f32;
            }
        }
        if let Some(rect) = path.rect(rectangle_matrix.as_ref()) {
            let mut bounds = rect.get_outer_rect();
            if !bounds.valid() {
                return false;
            }
            let mut width = (rect.right - rect.left).ceil() as i32;
            if width < 1 {
                width = 1;
                if bounds.left == bounds.right {
                    let Some(right) = bounds.right.checked_add(1) else {
                        return false;
                    };
                    bounds.right = right;
                }
            }
            let mut height = (rect.top - rect.bottom).ceil() as i32;
            if height < 1 {
                height = 1;
                if bounds.bottom == bounds.top {
                    let Some(bottom) = bounds.bottom.checked_add(1) else {
                        return false;
                    };
                    bounds.bottom = bottom;
                }
            }
            if i64::from(bounds.width()) >= i64::from(width) + 1 {
                if rect.left - bounds.left as f32 > bounds.right as f32 - rect.right {
                    let Some(left) = bounds.left.checked_add(1) else {
                        return false;
                    };
                    bounds.left = left;
                } else {
                    let Some(right) = bounds.right.checked_sub(1) else {
                        return false;
                    };
                    bounds.right = right;
                }
            }
            if i64::from(bounds.height()) >= i64::from(height) + 1 {
                if rect.top - bounds.top as f32 > bounds.bottom as f32 - rect.bottom {
                    let Some(top) = bounds.top.checked_add(1) else {
                        return false;
                    };
                    bounds.top = top;
                } else {
                    let Some(bottom) = bounds.bottom.checked_sub(1) else {
                        return false;
                    };
                    bounds.bottom = bottom;
                }
            }
            let (Some(left), Some(top), Some(right), Some(bottom)) = (
                bounds.left.checked_add(origin.0),
                bounds.top.checked_add(origin.1),
                bounds.right.checked_add(origin.0),
                bounds.bottom.checked_add(origin.1),
            ) else {
                return false;
            };
            if driver.fill_rect(FxRect::new(left, top, right, bottom), fill_color) {
                return true;
            }
        }
    }
    if fill && stroke_alpha == 0 && !options.stroke && !options.text_mode {
        // Borrow each contiguous subpath rather than copying it to a temporary
        // vector. The source's synthesized zero-area path is still independent.
        let mut start = 0;
        for i in 0..points.len() {
            if points[i].point_type == crate::fxge::cfx_path::PointType::Move {
                draw_zero_area_path_agg(
                    driver,
                    &points[start..i],
                    matrix,
                    options.aliased_path,
                    fill_color,
                    fill_alpha,
                );
                start = i;
            }
        }
        draw_zero_area_path_agg(
            driver,
            &points[start..],
            matrix,
            options.aliased_path,
            fill_color,
            fill_alpha,
        );
    }
    if fill && fill_alpha != 0 && stroke_alpha < 255 && options.stroke {
        return draw_fill_stroke_path_agg(
            driver,
            path,
            matrix,
            graph,
            fill_color,
            stroke_color,
            options,
        );
    }
    driver.draw_path(
        path,
        matrix,
        graph,
        fill_color,
        stroke_color,
        false,
        options,
    )
}

fn draw_fill_stroke_path_agg(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    path: &CfxPath,
    matrix: Option<CfxMatrix>,
    graph: Option<&CfxGraphStateData>,
    fill_color: u32,
    stroke_color: u32,
    options: CfxFillRenderOptions,
) -> bool {
    if !driver.render_cap_get_bits() {
        return false;
    }
    let mut bbox = graph.map_or_else(
        || path.bounding_box(),
        |graph| path.bounding_box_for_stroke_path(graph.line_width(), graph.miter_limit()),
    );
    if let Some(matrix) = matrix {
        bbox = matrix.transform_float_rect(bbox);
    }
    let rect = bbox.get_outer_rect();
    if !rect.valid() {
        return false;
    }
    let Some(mut bitmap) = driver.create_compatible_bitmap(rect.width(), rect.height()) else {
        return false;
    };
    let mut backdrop = CfxDibitmap::default();
    if bitmap.format() != FxdibFormat::Bgra && !driver.get_dibits(&mut bitmap, rect.left, rect.top)
    {
        return false;
    }
    // This explicit backdrop snapshot is required by DrawFillStrokePath. It
    // lets the stroke knock out the fill instead of accumulating both alphas.
    if !backdrop.copy_from(&bitmap) {
        return false;
    }
    {
        let mut target = CfxAggDeviceDriver::new(&mut bitmap, false, Some(&backdrop));
        let mut matrix = matrix.unwrap_or_default();
        matrix.translate(-(rect.left as f32), -(rect.top as f32));
        if !target.draw_path(
            path,
            Some(matrix),
            graph,
            fill_color,
            stroke_color,
            true,
            options,
        ) {
            return false;
        }
    }
    driver.set_dibits_normal(
        &bitmap,
        0,
        FxRect::new(0, 0, rect.width(), rect.height()),
        rect.left,
        rect.top,
    )
}

fn draw_cosmetic_line_agg(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    first: crate::fxcrt::fx_coordinates::CfxPointF,
    second: crate::fxcrt::fx_coordinates::CfxPointF,
    color: u32,
    options: CfxFillRenderOptions,
) -> bool {
    use crate::fxge::cfx_path::PointType;
    // AGG has no native cosmetic-line implementation; the portable fallback
    // draws a default-width path through the device driver.
    let mut path = CfxPath::new();
    path.append_point(first, PointType::Move);
    path.append_point(second, PointType::Line);
    driver.draw_path(
        &path,
        None,
        Some(&CfxGraphStateData::default()),
        0,
        color,
        false,
        options,
    )
}

fn get_zero_area_path(
    points: &[crate::fxge::cfx_path::PathPoint],
    matrix: Option<CfxMatrix>,
    adjust: bool,
) -> Option<(CfxPath, bool, bool)> {
    use crate::fxge::cfx_path::PointType;
    let mut path = CfxPath::new();
    if points.len() < 2 {
        return None;
    }
    // CheckSimpleLinePath(): A->B, or A->B->A.
    if (points.len() == 2 || points.len() == 3)
        && points[0].point_type == PointType::Move
        && points[1].point_type == PointType::Line
        && (points.len() == 2
            || (points[2].point_type == PointType::Line && points[0].point == points[2].point))
    {
        if points[0].point == points[1].point {
            return Some((path, false, false));
        }
        for point in &points[..2] {
            let mut position = point.point;
            if adjust {
                if let Some(matrix) = matrix {
                    position = matrix.transform(position);
                }
                position.x = position.x as i32 as f32 + 0.5;
                position.y = position.y as i32 as f32 + 0.5;
            }
            path.append_point(position, point.point_type);
        }
        return Some((path, true, adjust && matrix.is_some()));
    }
    // CheckPalindromicPath(): a line-only path that retraces itself.
    if points.len() > 3 && points.len() % 2 == 1 {
        let mid = points.len() / 2;
        let palindromic = (0..mid).all(|i| {
            let left = points[mid - i - 1];
            let right = points[mid + i + 1];
            left.point == right.point
                && left.point_type != PointType::Bezier
                && right.point_type != PointType::Bezier
        });
        if palindromic {
            for i in 0..mid {
                path.append_point(points[mid - i].point, PointType::Move);
                path.append_point(points[mid - i - 1].point, PointType::Line);
            }
            return Some((path, true, false));
        }
    }
    let mut i = 0;
    while i < points.len() {
        match points[i].point_type {
            PointType::Move => {
                i += 1;
                continue;
            }
            PointType::Bezier => {
                i = i.saturating_add(3);
                continue;
            }
            PointType::Line => {}
        }
        if i == 0 {
            i += 1;
            continue;
        } // Malformed path: no predecessor.
        let next = points[(i + 1) % points.len()];
        if next.point_type == PointType::Line {
            let (prev, cur) = (points[i - 1], points[i]);
            let (a, b, c) = (prev.point, cur.point, next.point);
            let vertical = a.x == b.x && b.x == c.x && (b.y - a.y) * (b.y - c.y) > 0.0;
            let horizontal = a.y == b.y && b.y == c.y && (b.x - a.x) * (b.x - c.x) > 0.0;
            let diagonal = a.x != b.x
                && c.x != b.x
                && a.y != b.y
                && c.y != b.y
                && (a.y - b.y) * (c.x - b.x) == (c.y - b.y) * (a.x - b.x);
            if vertical || horizontal || diagonal {
                let use_prev = if vertical {
                    (b.y - a.y).abs() < (b.y - c.y).abs()
                } else {
                    (b.x - a.x).abs() < (b.x - c.x).abs()
                };
                let (start, end) = if use_prev { (prev, cur) } else { (cur, next) };
                path.append_point(start.point, PointType::Move);
                path.append_point(end.point, PointType::Line);
            }
        }
        i += 1;
    }
    let count = path.points().len();
    (count != 0).then_some((path, points.len() > 3 && count > 0, false))
}

fn draw_zero_area_path_agg(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    points: &[crate::fxge::cfx_path::PathPoint],
    matrix: Option<CfxMatrix>,
    aliased: bool,
    fill_color: u32,
    fill_alpha: u8,
) {
    let Some((path, thin, identity)) =
        get_zero_area_path(points, matrix, driver.driver_type() != 0)
    else {
        return;
    };
    let mut graph = CfxGraphStateData::default();
    graph.set_line_width(0.0);
    let stroke_color = if thin {
        (u32::from(fill_alpha >> 2) << 24) | (fill_color & 0xffffff)
    } else {
        fill_color
    };
    let matrix = matrix.filter(|matrix| !matrix.is_identity() && !identity);
    let options = CfxFillRenderOptions {
        zero_area: true,
        aliased_path: aliased,
        ..Default::default()
    };
    driver.draw_path(&path, matrix, Some(&graph), 0, stroke_color, false, options);
}

/// StretchBitMaskWithFlags() for the selected AGG device.
pub fn stretch_bit_mask_agg<S: crate::fxge::dib::cstretchengine::StretchSource>(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    source: Rc<S>,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    color: u32,
    options: crate::fxge::dib::fx_dib::FxdibResampleOptions,
) -> bool {
    let Some(right) = left.checked_add(width) else {
        return false;
    };
    let Some(bottom) = top.checked_add(height) else {
        return false;
    };
    let mut clip = driver.clip_box();
    clip.intersect(FxRect::new(left, top, right, bottom));
    driver.stretch_dibits(
        source,
        color,
        left,
        top,
        width,
        height,
        clip,
        options,
        crate::fxge::dib::fx_dib::BlendMode::Normal,
    )
}

/// AGG display-device selection from DrawNormalText(), including the native
/// oversized-text path fallback, extended to both axes and glyph adjustments.
/// Native platform text and printers are excluded.
pub fn draw_normal_text_agg(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    chars: &[TextCharPos],
    font: &CfxFont<'_>,
    font_size: f32,
    text_to_device: CfxMatrix,
    fill_color: u32,
    options: CfxTextRenderOptions,
) -> bool {
    if (font_size as f64) < 0.0001 && (font_size as f64) > -0.0001 {
        return true;
    }
    let mut char_to_device = text_to_device;
    char_to_device.scale(font_size, -font_size);
    // Native checks only the X basis. A large Y basis (or per-glyph
    // adjustment) can also exceed the bounded glyph bitmap. Preserve these
    // transforms and use AGG's device-clipped outline rendering instead.
    // In particular, do not emulate FreeType replacing a poorly conditioned
    // but invertible transform with identity.
    let needs_path = |matrix: CfxMatrix| {
        matrix.a.abs() + matrix.b.abs() > 50.0 || matrix.c.abs() + matrix.d.abs() > 50.0
    };
    if font.face().is_some()
        && (needs_path(char_to_device)
            || chars
                .iter()
                .any(|ch| ch.glyph_adjust && needs_path(ch.get_effective_matrix(char_to_device))))
    {
        // Keep integer region translation out of the glyph outline's f32
        // control points. The device applies it after curve subdivision, at
        // fixed-point raster precision, so a translated crop uses the same
        // curve samples as the whole page.
        let translation = CfxMatrix::new(
            1.0,
            0.0,
            0.0,
            1.0,
            text_to_device.e.floor(),
            text_to_device.f.floor(),
        );
        let mut local = text_to_device;
        local.e -= translation.e;
        local.f -= translation.f;
        return draw_text_path_agg(
            driver,
            chars,
            font,
            font_size,
            local,
            Some(translation),
            None,
            fill_color,
            0,
            None,
            CfxFillRenderOptions {
                aliased_path: !options.is_smooth(),
                ..Default::default()
            },
        );
    }
    if !options.is_smooth() {
        return draw_normal_text_mask(
            driver,
            chars,
            font,
            font_size,
            text_to_device,
            fill_color,
            true,
        );
    }
    if driver.bits_per_pixel() < 16 {
        return draw_normal_text_mask(
            driver,
            chars,
            font,
            font_size,
            text_to_device,
            fill_color,
            false,
        );
    }
    let normalize = driver.render_cap_alpha_output()
        || font.face().is_none()
        || options.aliasing_type != AliasingType::Lcd;
    draw_normal_text_lcd_impl(
        driver,
        chars,
        font,
        font_size,
        text_to_device,
        fill_color,
        normalize,
    )
}

/// Shared glyph-outline transform used by painting, clipping and save-time
/// clipping serialization. Keep CID adjustment/fallback positioning identical.
pub(crate) fn text_glyph_outline(
    ch: &TextCharPos,
    font: &CfxFont<'_>,
    font_size: f32,
    text_to_user: CfxMatrix,
) -> Option<CfxPath> {
    let path = font.load_glyph_path(ch.glyph_index, ch.font_char_width)?;
    let matrix = CfxMatrix::new(font_size, 0.0, 0.0, font_size, ch.origin.x, ch.origin.y);
    let mut matrix = ch.get_effective_matrix(matrix);
    matrix.concat(text_to_user);
    let mut transformed = CfxPath::new();
    transformed.append(&path, Some(&matrix));
    Some(transformed)
}

/// `DrawTextPath()` for the AGG device, including text fill, stroke and clip
/// path accumulation. The transformed path is distinct from the cached glyph
/// path because the latter is shared by later text operations.
pub fn draw_text_path_agg(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    chars: &[TextCharPos],
    font: &CfxFont<'_>,
    font_size: f32,
    text_to_user: CfxMatrix,
    user_to_device: Option<CfxMatrix>,
    graph_state: Option<&CfxGraphStateData>,
    fill_color: u32,
    stroke_color: u32,
    mut clipping_path: Option<&mut CfxPath>,
    fill_options: CfxFillRenderOptions,
) -> bool {
    for ch in chars {
        let Some(transformed) = text_glyph_outline(ch, font, font_size, text_to_user) else {
            continue;
        };
        if fill_color != 0 || stroke_color != 0 {
            let mut options = fill_options;
            if fill_color != 0 {
                options.fill_type = FillType::Winding;
            }
            options.text_mode = true;
            if !draw_path_agg(
                driver,
                &transformed,
                user_to_device,
                graph_state,
                fill_color,
                stroke_color,
                options,
            ) {
                return false;
            }
        }
        if let Some(clip) = clipping_path.as_deref_mut() {
            clip.append(&transformed, user_to_device.as_ref());
        }
    }
    true
}

// Retain PDFium's f32 transform and LCD multiplication rounding, but perform
// them before integer device translation. This keeps third-pixel ties such as
// 8.333333f32 * 3 == 25 while preventing a tile offset from changing the phase.
// The split depends only on the text run, not on the individual glyph.
struct GlyphPositionTransform {
    local: CfxMatrix,
    offset_x: f64,
    offset_y: f64,
}
impl GlyphPositionTransform {
    fn new(mut matrix: CfxMatrix) -> Self {
        let offset_x = if matrix.e.is_finite() {
            matrix.e.floor()
        } else {
            0.0
        };
        let offset_y = if matrix.f.is_finite() {
            matrix.f.floor()
        } else {
            0.0
        };
        matrix.e -= offset_x;
        matrix.f -= offset_y;
        Self {
            local: matrix,
            offset_x: f64::from(offset_x),
            offset_y: f64::from(offset_y),
        }
    }
    fn translate(&self, local: CfxPointF) -> (f64, f64) {
        (
            f64::from(local.x) + self.offset_x,
            f64::from(local.y) + self.offset_y,
        )
    }
}

fn lcd_phase(local_x: f32) -> i32 {
    let subpixel = (local_x * 3.0).floor();
    // Ordinary device coordinates fit i32. Integer remainder avoids a floating
    // remainder library call per glyph, while the rare large/nonfinite path
    // retains the existing saturating conversion and f32 remainder behavior.
    if (-2147483648.0..2147483648.0).contains(&subpixel) {
        (subpixel as i32).rem_euclid(3)
    } else {
        subpixel.rem_euclid(3.0) as i32
    }
}

#[cfg(test)]
fn glyph_device_position(matrix: CfxMatrix, origin: CfxPointF) -> (f64, f64, i32) {
    let position = GlyphPositionTransform::new(matrix);
    let local = position.local.transform(origin);
    let (x, y) = position.translate(local);
    (x, y, lcd_phase(local.x))
}

/// The AGG monochrome and grayscale branches of `DrawNormalText()`. Glyph
/// bitmaps remain owned by the font cache while `TextGlyphPos` borrows them.
pub fn draw_normal_text_mask(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    chars: &[TextCharPos],
    font: &CfxFont<'_>,
    font_size: f32,
    text_to_device: CfxMatrix,
    fill_color: u32,
    mono: bool,
) -> bool {
    if font_size == 0.0 || chars.is_empty() {
        return true;
    }
    let anti_alias = if mono {
        FontAntiAliasingMode::Mono
    } else {
        FontAntiAliasingMode::Normal
    };
    let mut char_to_device = text_to_device;
    char_to_device.scale(font_size, -font_size);
    let loaded: Vec<_> = chars
        .iter()
        .map(|ch| {
            font.load_glyph_bitmap(
                ch.glyph_index,
                ch.is_cid_font,
                ch.get_effective_matrix(char_to_device),
                ch.font_char_width,
                anti_alias,
            )
        })
        .collect();
    let position = GlyphPositionTransform::new(text_to_device);
    let mut glyphs: Vec<_> = chars
        .iter()
        .zip(&loaded)
        .map(|(ch, bitmap)| {
            let (x, y) = position.translate(position.local.transform(ch.origin));
            let device_origin = CfxPointF::new(x as f32, y as f32);
            TextGlyphPos {
                glyph: bitmap.as_deref(),
                origin: CfxPoint::new((x + 0.5).floor() as i32, (y + 0.5).floor() as i32),
                device_origin,
            }
        })
        .collect();
    if glyphs.len() > 1 {
        adjust_glyph_space(&mut glyphs);
    }
    let mut bounds = get_glyphs_bbox(&glyphs, false);
    bounds.intersect(driver.clip_box());
    if bounds.is_empty() {
        return true;
    }
    let Some(mask) = compose_glyph_mask(&glyphs, bounds, mono, fill_color) else {
        return false;
    };
    let result = driver.set_dibits_normal(
        &mask,
        fill_color,
        FxRect::new(0, 0, mask.width(), mask.height()),
        bounds.left,
        bounds.top,
    );
    // The monochrome branch returns SetBitMask's result; the grayscale branch
    // ignores the final device result after successful mask construction.
    !mono || result
}

/// The LCD branch of `DrawNormalText()`. Alpha devices use the source's
/// normalized three-subpixel average; opaque RGB devices blend each channel.
fn draw_normal_text_helper(
    bitmap: &mut CfxDibitmap,
    glyph: &CfxDibitmap,
    rows: i32,
    left: i32,
    top: i32,
    start_col: i32,
    end_col: i32,
    normalize: bool,
    subpixel: i32,
    color: [u8; 4],
) {
    let has_alpha = bitmap.format() == FxdibFormat::Bgra;
    let bytes = crate::fxge::dib::fx_dib::get_bpp_from_format(bitmap.format()) as usize / 8;
    // Native treats every nonzero subpixel other than 1 as the shift-2 case,
    // including the negative result of C++'s signed remainder.
    let shift = if subpixel == 0 {
        0
    } else if subpixel == 1 {
        1
    } else {
        2
    };
    for row in 0..rows {
        let Some(dest_y) = row.checked_add(top) else {
            continue;
        };
        if dest_y < 0 || dest_y >= bitmap.height() {
            continue;
        }
        let Some(source) = glyph.scanline(row) else {
            continue;
        };
        let target = bitmap.scanline_mut(dest_y).unwrap();
        for col in start_col..end_col {
            let base = (i64::from(col) - i64::from(left)) * 3 - i64::from(shift);
            let coverage = if base < 0 {
                if shift == 1 {
                    [0, source[0], source[1]]
                } else {
                    [0, 0, source[0]]
                }
            } else {
                let base = base as usize;
                let triplet = &source[base..base + 3];
                [triplet[0], triplet[1], triplet[2]]
            };
            let pixel = &mut target[col as usize * bytes..col as usize * bytes + bytes];
            if normalize {
                let average =
                    (u32::from(coverage[0]) + u32::from(coverage[1]) + u32::from(coverage[2])) / 3;
                let alpha =
                    i32::from(TEXT_GAMMA_ADJUST[average as usize]) * i32::from(color[3]) / 255;
                if has_alpha {
                    // NormalizeSrc (first shifted pixel) skips zero alpha;
                    // NormalizeDest copies RGB into a transparent destination
                    // even when its resulting alpha is zero.
                    if col == start_col && shift != 0 && alpha == 0 {
                        continue;
                    }
                    let backdrop = i32::from(pixel[3]);
                    if backdrop == 0 {
                        pixel[..3].copy_from_slice(&color[..3]);
                        pixel[3] = alpha as u8;
                    } else if alpha != 0 {
                        let output = backdrop + alpha - backdrop * alpha / 255;
                        let ratio = alpha * 255 / output;
                        for channel in 0..3 {
                            pixel[channel] = alpha_merge(
                                i32::from(pixel[channel]),
                                i32::from(color[channel]),
                                ratio,
                            ) as u8;
                        }
                        pixel[3] = output as u8;
                    }
                } else if alpha != 0 {
                    for channel in 0..3 {
                        pixel[channel] = alpha_merge(
                            i32::from(pixel[channel]),
                            i32::from(color[channel]),
                            alpha,
                        ) as u8;
                    }
                }
            } else {
                for channel in 0..3 {
                    let alpha = i32::from(TEXT_GAMMA_ADJUST[coverage[2 - channel] as usize])
                        * i32::from(color[3])
                        / 255;
                    pixel[channel] =
                        alpha_merge(i32::from(pixel[channel]), i32::from(color[channel]), alpha)
                            as u8;
                }
                if has_alpha {
                    pixel[3] = 255;
                }
            }
        }
    }
}

pub fn draw_normal_text_lcd(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    chars: &[TextCharPos],
    font: &CfxFont<'_>,
    font_size: f32,
    text_to_device: CfxMatrix,
    fill_color: u32,
) -> bool {
    let normalize = driver.render_cap_alpha_output() || font.face().is_none();
    draw_normal_text_lcd_impl(
        driver,
        chars,
        font,
        font_size,
        text_to_device,
        fill_color,
        normalize,
    )
}

// Retain each cached bitmap and its placement together. LCD needs no mutable
// glyph-spacing pass, so a second vector of borrowed positions is unnecessary.
struct LcdGlyph {
    bitmap: Option<Rc<CfxGlyphBitmap>>,
    origin: CfxPoint,
    local_origin: CfxPointF,
}
impl LcdGlyph {
    fn position(&self) -> TextGlyphPos<'_> {
        TextGlyphPos {
            glyph: self.bitmap.as_deref(),
            origin: self.origin,
            device_origin: self.local_origin,
        }
    }
}

fn draw_normal_text_lcd_impl(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    chars: &[TextCharPos],
    font: &CfxFont<'_>,
    font_size: f32,
    text_to_device: CfxMatrix,
    fill_color: u32,
    normalize: bool,
) -> bool {
    if font_size == 0.0 || chars.is_empty() {
        return true;
    }
    let alpha_output = driver.render_cap_alpha_output();
    if driver.bits_per_pixel() < 24 {
        return false;
    }
    let mut char_to_device = text_to_device;
    char_to_device.scale(font_size, -font_size);
    let position = GlyphPositionTransform::new(text_to_device);
    let glyphs: Vec<_> = chars
        .iter()
        .map(|ch| {
            let bitmap = font.load_glyph_bitmap(
                ch.glyph_index,
                ch.is_cid_font,
                ch.get_effective_matrix(char_to_device),
                ch.font_char_width,
                FontAntiAliasingMode::Lcd,
            );
            let local_origin = position.local.transform(ch.origin);
            let (x, y) = position.translate(local_origin);
            LcdGlyph {
                bitmap,
                origin: CfxPoint::new(x.floor() as i32, (y + 0.5).floor() as i32),
                local_origin,
            }
        })
        .collect();
    let mut bounds = get_glyphs_bbox_iter(glyphs.iter().map(LcdGlyph::position), true);
    bounds.intersect(driver.clip_box());
    if bounds.is_empty() {
        return true;
    }
    let Some(mut bitmap) = driver.create_compatible_bitmap(bounds.width(), bounds.height()) else {
        return false;
    };
    if !alpha_output {
        bitmap.clear(0xffffffff);
        if !driver.get_dibits(&mut bitmap, bounds.left, bounds.top) {
            return false;
        }
    }
    let color = fill_color.to_le_bytes();
    for glyph in &glyphs {
        let glyph = glyph.position();
        let Some(glyph_bitmap) = glyph.glyph else {
            continue;
        };
        let Some(origin) = glyph.get_origin(CfxPoint::new(bounds.left, bounds.top)) else {
            continue;
        };
        let source = glyph_bitmap.bitmap();
        let glyph_width = source.width() / 3;
        // Use the fractional device position, including for glyphs whose
        // origin lies left of a region. Truncation plus signed remainder (as
        // upstream uses) changes phase when an integer tile offset crosses zero.
        let shift = lcd_phase(glyph.device_origin.x);
        let start = origin.x.max(0);
        let Some(end) = origin.x.checked_add(glyph_width) else {
            continue;
        };
        let end = end.min(bitmap.width());
        if start >= end {
            continue;
        }
        draw_normal_text_helper(
            &mut bitmap,
            &source,
            source.height(),
            origin.x,
            origin.y,
            start,
            end,
            normalize,
            shift,
            color,
        );
    }
    driver.set_dibits_normal(
        &bitmap,
        0,
        FxRect::new(0, 0, bitmap.width(), bitmap.height()),
        bounds.left,
        bounds.top,
    );
    true
}

pub fn adjust_glyph_space(glyphs: &mut [TextGlyphPos<'_>]) {
    if glyphs.len() <= 2 {
        return;
    }
    let vertical = glyphs.last().unwrap().origin.x == glyphs.first().unwrap().origin.x;
    if !vertical && glyphs.last().unwrap().origin.y != glyphs.first().unwrap().origin.y {
        return;
    }
    for index in (2..glyphs.len()).rev() {
        let next = &glyphs[index];
        let next_origin = if vertical {
            next.origin.y
        } else {
            next.origin.x
        };
        let next_origin_f = if vertical {
            next.device_origin.y
        } else {
            next.device_origin.x
        };
        let current = &mut glyphs[index - 1];
        let current_origin = if vertical {
            &mut current.origin.y
        } else {
            &mut current.origin.x
        };
        let current_origin_f = if vertical {
            current.device_origin.y
        } else {
            current.device_origin.x
        };
        let Some(space) = next_origin.checked_sub(*current_origin) else {
            continue;
        };
        let space_f = next_origin_f - current_origin_f;
        let error = space_f.abs() - (space as f32).abs();
        if error <= 0.5 {
            continue;
        }
        if let Some(adjusted) = current_origin.checked_add(if space > 0 { -1 } else { 1 }) {
            *current_origin = adjusted;
        }
    }
}

/// The mono and 8-bit mask accumulation loops in `DrawNormalText()`.
pub fn compose_glyph_mask(
    glyphs: &[TextGlyphPos<'_>],
    bounds: FxRect,
    mono: bool,
    fill_color: u32,
) -> Option<CfxDibitmap> {
    let width = bounds.right.checked_sub(bounds.left)?;
    let height = bounds.bottom.checked_sub(bounds.top)?;
    let format = if mono {
        FxdibFormat::OneBppMask
    } else {
        FxdibFormat::EightBppMask
    };
    let mut bitmap = CfxDibitmap::create(width, height, format, 0)?;
    for glyph in glyphs {
        let Some(glyph_bitmap) = glyph.glyph else {
            continue;
        };
        let Some(point) = glyph.get_origin(CfxPoint::new(bounds.left, bounds.top)) else {
            continue;
        };
        let source = glyph_bitmap.bitmap();
        if mono {
            bitmap.composite_one_bpp_mask(
                point.x,
                point.y,
                source.width(),
                source.height(),
                &source,
                0,
                0,
            );
        } else if !bitmap.composite_mask_normal(
            point.x,
            point.y,
            source.width(),
            source.height(),
            &source,
            // Accumulate glyph coverage independently of the text opacity.
            // The final SetBitMask applies fill_color's alpha once. Passing
            // that alpha here too squares opacity on Gray8 destinations.
            fill_color | 0xff00_0000,
            0,
            0,
            false,
        ) {
            return None;
        }
    }
    Some(bitmap)
}

pub fn draw_glyph_mask(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    glyphs: &[TextGlyphPos<'_>],
    bounds: FxRect,
    mono: bool,
    fill_color: u32,
) -> bool {
    if bounds.is_empty() {
        return true;
    }
    let Some(mask) = compose_glyph_mask(glyphs, bounds, mono, fill_color) else {
        return false;
    };
    driver.set_dibits_normal(
        &mask,
        fill_color,
        FxRect::new(0, 0, mask.width(), mask.height()),
        bounds.left,
        bounds.top,
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::fxcrt::fx_coordinates::{CfxPoint, CfxPointF};
    use crate::fxge::cfx_font::CfxFont;
    use crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap;

    #[test]
    fn lcd_normalization_and_shifted_first_pixel_preserve_native_alpha_rules() {
        let mut glyph = CfxDibitmap::create(3, 1, FxdibFormat::EightBppMask, 0).unwrap();
        glyph.buffer_mut()[0] = 255;
        for normalize in [false, true] {
            let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::Bgr, 0).unwrap();
            draw_normal_text_helper(&mut bitmap, &glyph, 1, 0, 0, 0, 1, normalize, 0, [255; 4]);
            let pixel = &bitmap.buffer()[..3];
            if normalize {
                assert!(pixel[0] > 0 && pixel[0] < 255);
                assert_eq!(pixel[0], pixel[1]);
                assert_eq!(pixel[1], pixel[2]);
            } else {
                assert_eq!(pixel, &[0, 0, 255]);
            }
        }
        glyph.clear(0);
        for shift in [0, 1, 2, -1] {
            let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
            draw_normal_text_helper(
                &mut bitmap,
                &glyph,
                1,
                0,
                0,
                0,
                1,
                true,
                shift,
                [17, 34, 51, 255],
            );
            assert_eq!(
                &bitmap.buffer()[..4],
                if shift == 0 {
                    &[17, 34, 51, 0]
                } else {
                    &[0; 4]
                }
            );
        }
    }

    #[test]
    fn lcd_phase_integer_fast_path_matches_float_phase_at_boundaries() {
        for step in -8192..8192 {
            let x = step as f32 / 256.0;
            assert_eq!(lcd_phase(x), (x * 3.0).floor().rem_euclid(3.0) as i32);
        }
        for x in [
            -1.0e30,
            -2147483648.0,
            -715827904.0,
            -715827840.0,
            -8.333333,
            -0.0,
            8.333333,
            715827840.0,
            715827904.0,
            2147483648.0,
            1.0e30,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            assert_eq!(lcd_phase(x), (x * 3.0).floor().rem_euclid(3.0) as i32);
        }
    }

    #[test]
    fn lcd_third_pixel_tie_keeps_native_rounding_across_device_origins() {
        // XFA x-small Ahem advances by this stored f32. Native f32 arithmetic
        // chooses the second LCD phase; widening before *3 incorrectly picks 0.
        let origin = CfxPointF::new(8.333333, 20.5);
        for offset in [-512.0, -16.0, 0.0, 16.0, 512.0] {
            let matrix = CfxMatrix {
                e: offset,
                f: offset,
                ..CfxMatrix::default()
            };
            let (x, y, phase) = glyph_device_position(matrix, origin);
            assert_eq!(phase, 1);
            assert_eq!(x.floor() as i32, 8 + offset as i32);
            assert_eq!((y + 0.5).floor() as i32, 21 + offset as i32);
        }
    }

    #[test]
    fn rectangle_snap_margins_are_invariant_under_integer_device_translation() {
        fn render(width: i32, height: i32, offset: (i32, i32)) -> CfxDibitmap {
            let mut bitmap = CfxDibitmap::create(width, height, FxdibFormat::Bgra, 0).unwrap();
            let mut path = CfxPath::new();
            path.append_rect(10.0, 10.0, 90.0, 50.0);
            let matrix = CfxMatrix::new(
                1.6 * (1067.0 / 256.0),
                0.0,
                0.0,
                -1.6 * (800.0 / 192.0),
                200.0625 - offset.0 as f32,
                650.0 - offset.1 as f32,
            );
            {
                let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
                assert!(draw_path_agg(
                    &mut driver,
                    &path,
                    Some(matrix),
                    None,
                    0xffffcc00,
                    0,
                    CfxFillRenderOptions::winding_options()
                ));
            }
            bitmap
        }
        let whole = render(1067, 800, (0, 0));
        assert!(whole.buffer().iter().any(|&byte| byte != 0));
        for (x, y, width, height) in [
            (0, 512, 512, 288),
            (512, 512, 512, 288),
            (0, 266, 1067, 525),
            (355, 200, 531, 523),
        ] {
            let part = render(width, height, (x, y));
            for row in 0..height {
                assert_eq!(
                    &whole.scanline(y + row).unwrap()[x as usize * 4..(x + width) as usize * 4],
                    &part.scanline(row).unwrap()[..width as usize * 4],
                    "region {x},{y},{width},{height}, row {row}"
                );
            }
        }
    }

    #[test]
    fn device_rectangles_snap_unless_rectangle_antialiasing_is_requested() {
        let mut path = CfxPath::new();
        path.append_rect(1.1, 1.1, 2.1, 2.1);
        for antialias in [false, true] {
            let mut bitmap = CfxDibitmap::create(4, 4, FxdibFormat::Bgra, 0).unwrap();
            let mut options = CfxFillRenderOptions::winding_options();
            options.rect_aa = antialias;
            {
                let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
                assert!(draw_path_agg(
                    &mut driver,
                    &path,
                    None,
                    None,
                    0xffff0000,
                    0,
                    options
                ));
            }
            let alpha = bitmap.scanline(1).unwrap()[7];
            if antialias {
                assert!(alpha > 0 && alpha < 255);
            } else {
                assert_eq!(alpha, 255);
                assert_eq!(&bitmap.scanline(2).unwrap()[8..12], &[0; 4]);
            }
        }
    }

    #[test]
    fn translucent_fill_stroke_uses_knockout_against_the_original_backdrop() {
        let mut path = CfxPath::new();
        path.append_rect(2.0, 2.0, 6.0, 6.0);
        let mut graph = CfxGraphStateData::default();
        graph.set_line_width(2.0);
        let mut options = CfxFillRenderOptions::winding_options();
        options.stroke = true;
        options.aliased_path = true;
        for format in [FxdibFormat::Bgra, FxdibFormat::Bgr] {
            let mut bitmap = CfxDibitmap::create(8, 8, format, 0).unwrap();
            if format == FxdibFormat::Bgr {
                bitmap.clear(0xffffffff);
            }
            {
                let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
                assert!(draw_path_agg(
                    &mut driver,
                    &path,
                    None,
                    Some(&graph),
                    0x80ff0000,
                    0x800000ff,
                    options
                ));
            }
            let row = bitmap.scanline(4).unwrap();
            if format == FxdibFormat::Bgra {
                assert_eq!(&row[8..12], &[255, 0, 0, 128]);
                assert_eq!(&row[16..20], &[0, 0, 255, 128]);
            } else {
                assert_eq!(&row[6..9], &[255, 127, 127]);
                assert_eq!(&row[12..15], &[127, 127, 255]);
            }
        }
    }

    #[test]
    fn retraced_zero_area_lines_snap_in_device_space_and_palindromes_keep_segment_order() {
        use crate::fxge::cfx_path::PointType;
        let mut path = CfxPath::new();
        let first = CfxPointF::new(-1.7, -2.3);
        let second = CfxPointF::new(2.1, 3.8);
        path.append_point(first, PointType::Move);
        path.append_point(second, PointType::Line);
        path.append_point(first, PointType::Line);
        let (line, thin, identity) = get_zero_area_path(
            path.points(),
            Some(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 1.0, 1.0)),
            true,
        )
        .unwrap();
        assert!(thin && identity);
        assert_eq!(line.points()[0].point, CfxPointF::new(0.5, -0.5));
        assert_eq!(line.points()[1].point, CfxPointF::new(3.5, 4.5));
        let middle = CfxPointF::new(4.0, 1.0);
        let mut palindrome = CfxPath::new();
        for (i, point) in [first, second, middle, second, first]
            .into_iter()
            .enumerate()
        {
            palindrome.append_point(
                point,
                if i == 0 {
                    PointType::Move
                } else {
                    PointType::Line
                },
            );
        }
        let (line, thin, identity) = get_zero_area_path(palindrome.points(), None, true).unwrap();
        assert!(thin && !identity);
        assert_eq!(
            line.points()
                .iter()
                .map(|point| point.point)
                .collect::<Vec<_>>(),
            [middle, second, second, first]
        );
    }

    #[test]
    fn normal_text_draws_cached_agg_glyph_to_page_bitmap() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let font = CfxFont::from_span(data, false, 0).unwrap();
        let char_pos = TextCharPos {
            origin: CfxPointF::new(8.0, -20.0),
            glyph_index: font.get_char_index(u32::from(b'A')),
            ..TextCharPos::default()
        };
        for mono in [false, true] {
            let mut target = CfxDibitmap::create(40, 40, FxdibFormat::Bgra, 0).unwrap();
            let mut driver = CfxAggDeviceDriver::new(&mut target, false, None);
            assert!(draw_normal_text_mask(
                &mut driver,
                &[char_pos],
                &font,
                16.0,
                CfxMatrix::new(1.0, 0.0, 0.0, -1.0, 0.0, 0.0),
                0xff112233,
                mono,
            ));
            assert!(target.buffer().iter().any(|&byte| byte != 0));
        }
    }

    #[test]
    fn oversized_y_and_glyph_adjustments_render_visible_transformed_outlines() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let font = CfxFont::from_span(data, false, 0).unwrap();
        // Ahem A is a filled unit square. This invertible, near-singular
        // transform makes a long diagonal ribbon, far beyond a glyph bitmap.
        // FreeType would discard its linear transform and draw at 64 ppem.
        let linear = CfxMatrix::new(0.0, 11.0, 9552.0, -9552.455, 0.0, 0.0);
        let memory = crate::fxge::font_memory::font_memory();
        let scratch_before = memory.usage().glyph_scratch_bytes;
        assert_ne!(linear.a * linear.d - linear.b * linear.c, 0.0);
        for adjusted in [false, true] {
            let ch = TextCharPos {
                origin: if adjusted {
                    CfxPointF::new(8.0, 56.0)
                } else {
                    CfxPointF::new(0.0, 0.0)
                },
                glyph_index: font.get_char_index(u32::from(b'A')),
                glyph_adjust: adjusted,
                adjust_matrix: [linear.a, linear.b, linear.c, linear.d],
                ..TextCharPos::default()
            };
            let matrix = if adjusted {
                CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            } else {
                CfxMatrix::new(linear.a, linear.b, linear.c, linear.d, 8.0, 56.0)
            };
            for aliasing in [
                AliasingType::Aliasing,
                AliasingType::AntiAliasing,
                AliasingType::Lcd,
            ] {
                // Exercise the same path fallback on alpha, opaque and mask
                // targets, with an explicit clip smaller than the destination.
                for format in [
                    FxdibFormat::Bgra,
                    FxdibFormat::Bgr,
                    FxdibFormat::EightBppMask,
                ] {
                    let mut target = CfxDibitmap::create(96, 96, format, 0).unwrap();
                    let capacity = target.buffer().len();
                    {
                        let mut driver = CfxAggDeviceDriver::new(&mut target, false, None);
                        assert!(driver.set_clip_rect(FxRect::new(16, 16, 72, 72)));
                        assert!(draw_normal_text_agg(
                            &mut driver,
                            &[ch],
                            &font,
                            1.0,
                            matrix,
                            0xffffffff,
                            CfxTextRenderOptions::new(aliasing),
                        ));
                    }
                    let bytes = target.bpp() as usize / 8;
                    let pixel = |x: usize, y| target.scanline(y).unwrap()[x * bytes];
                    assert_eq!(pixel(28, 41), 255, "transformed ribbon is visible");
                    assert_eq!(pixel(28, 25), 0, "do not replace with a compact glyph");
                    assert_eq!(pixel(8, 61), 0, "respect the device clip");
                    assert_eq!(target.buffer().len(), capacity);
                    assert_eq!(memory.usage().glyph_scratch_bytes, scratch_before);
                    for y in 0..96 {
                        for x in 0..96 {
                            if !(16..72).contains(&x) || !(16..72).contains(&y) {
                                assert_eq!(pixel(x, y as i32), 0);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn lcd_text_draws_to_alpha_and_opaque_page_bitmaps() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let font = CfxFont::from_span(data, false, 0).unwrap();
        let char_pos = TextCharPos {
            origin: CfxPointF::new(8.4, -20.0),
            glyph_index: font.get_char_index(u32::from(b'A')),
            ..TextCharPos::default()
        };
        for format in [FxdibFormat::Bgra, FxdibFormat::Bgr] {
            let mut target = CfxDibitmap::create(40, 40, format, 0).unwrap();
            let mut driver = CfxAggDeviceDriver::new(&mut target, false, None);
            assert!(draw_normal_text_lcd(
                &mut driver,
                &[char_pos],
                &font,
                16.0,
                CfxMatrix::new(1.0, 0.0, 0.0, -1.0, 0.0, 0.0),
                0xff112233,
            ));
            assert!(target.buffer().iter().any(|&byte| byte != 0));
        }
    }

    #[test]
    fn text_regions_preserve_subpixel_phase_and_half_pixel_rounding() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let font = CfxFont::from_span(data, false, 0).unwrap();
        for format in [FxdibFormat::Bgra, FxdibFormat::Bgr] {
            for fraction in [0.1, 0.4, 0.5, 0.8, 1.0 / 3.0] {
                for top in [0, 24] {
                    for lcd in [false, true] {
                        let ch = TextCharPos {
                            origin: CfxPointF::new(8.0 + fraction, -20.5),
                            glyph_index: font.get_char_index(u32::from(b'A')),
                            ..TextCharPos::default()
                        };
                        let mut whole = CfxDibitmap::create(64, 64, format, 0).unwrap();
                        let mut region = CfxDibitmap::create(48, 64 - top, format, 0).unwrap();
                        for (bitmap, x, y) in
                            [(&mut whole, 0.0, 0.0), (&mut region, -16.0, -(top as f32))]
                        {
                            bitmap.clear(0xffffffff);
                            let mut driver = CfxAggDeviceDriver::new(bitmap, false, None);
                            let matrix = CfxMatrix::new(1.0, 0.0, 0.0, -1.0, x, y);
                            assert!(if lcd {
                                draw_normal_text_lcd(
                                    &mut driver,
                                    &[ch],
                                    &font,
                                    32.0,
                                    matrix,
                                    0xff112233,
                                )
                            } else {
                                draw_normal_text_mask(
                                    &mut driver,
                                    &[ch],
                                    &font,
                                    32.0,
                                    matrix,
                                    0xff112233,
                                    false,
                                )
                            });
                        }
                        let bytes = if format == FxdibFormat::Bgra { 4 } else { 3 };
                        for y in top..64 {
                            assert_eq!(
                                &whole.scanline(y).unwrap()[16 * bytes..64 * bytes],
                                &region.scanline(y - top).unwrap()[..48 * bytes],
                                "format={format:?} fraction={fraction} top={top} lcd={lcd} row={y}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn text_path_draws_glyph_and_accumulates_clip() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let font = CfxFont::from_span(data, false, 0).unwrap();
        let char_pos = TextCharPos {
            origin: CfxPointF::new(8.0, 20.0),
            glyph_index: font.get_char_index(u32::from(b'A')),
            ..TextCharPos::default()
        };
        let mut clip = CfxPath::new();
        let mut target = CfxDibitmap::create(40, 40, FxdibFormat::Bgra, 0).unwrap();
        let mut driver = CfxAggDeviceDriver::new(&mut target, false, None);
        assert!(draw_text_path_agg(
            &mut driver,
            &[char_pos],
            &font,
            16.0,
            CfxMatrix::new(1.0, 0.0, 0.0, -1.0, 0.0, 40.0),
            None,
            None,
            0xff112233,
            0,
            Some(&mut clip),
            CfxFillRenderOptions::default(),
        ));
        assert!(!clip.points().is_empty());
        assert!(target.buffer().iter().any(|&byte| byte != 0));
    }

    #[test]
    fn glyph_spacing_adjusts_intermediate_rounded_origin() {
        let mut glyphs = [TextGlyphPos::default(); 3];
        for (index, glyph) in glyphs.iter_mut().enumerate() {
            glyph.origin = CfxPoint::new(index as i32, 0);
            glyph.device_origin = CfxPointF::new([0.0, 1.1, 2.9][index], 0.0);
        }
        adjust_glyph_space(&mut glyphs);
        assert_eq!(
            [glyphs[0].origin.x, glyphs[1].origin.x, glyphs[2].origin.x],
            [0, 0, 2]
        );
    }

    #[test]
    fn overlapping_normal_glyph_masks_apply_text_opacity_once_on_gray_and_alpha_targets() {
        let mut first = CfxDibitmap::create(2, 1, FxdibFormat::EightBppMask, 0).unwrap();
        first.scanline_mut(0).unwrap()[..2].copy_from_slice(&[255, 128]);
        let mut second = CfxDibitmap::create(2, 1, FxdibFormat::EightBppMask, 0).unwrap();
        second.scanline_mut(0).unwrap()[..2].copy_from_slice(&[128, 255]);
        let first = CfxGlyphBitmap::new(CfxPoint::new(0, 0), Rc::new(RefCell::new(first)));
        let second = CfxGlyphBitmap::new(CfxPoint::new(0, 0), Rc::new(RefCell::new(second)));
        let glyphs = [
            TextGlyphPos {
                glyph: Some(&first),
                origin: CfxPoint::new(0, 0),
                device_origin: CfxPointF::new(0.0, 0.0),
            },
            TextGlyphPos {
                glyph: Some(&second),
                origin: CfxPoint::new(1, 0),
                device_origin: CfxPointF::new(1.0, 0.0),
            },
        ];
        // Opaque endpoint coverage and the overlapping half-covered edges
        // must retain their geometry for every text alpha, including zero.
        let coverage = [255u8, 192, 255];
        for alpha in [0u32, 127, 128, 255] {
            for rgb in [0x00ff_0000u32, 0x0020_4080] {
                let color = alpha << 24 | rgb;
                let mask =
                    compose_glyph_mask(&glyphs, FxRect::new(0, 0, 3, 1), false, color).unwrap();
                assert_eq!(&mask.scanline(0).unwrap()[..3], &coverage);
                for format in [FxdibFormat::EightBppRgb, FxdibFormat::Bgra] {
                    let mut target = CfxDibitmap::create(3, 1, format, 0).unwrap();
                    target.clear(0xffff_ffff);
                    {
                        let mut driver = CfxAggDeviceDriver::new(&mut target, false, None);
                        assert!(draw_glyph_mask(
                            &mut driver,
                            &glyphs,
                            FxRect::new(0, 0, 3, 1),
                            false,
                            color,
                        ));
                    }
                    let red = (rgb >> 16) & 255;
                    let green = (rgb >> 8) & 255;
                    let blue = rgb & 255;
                    let row = target.scanline(0).unwrap();
                    for (x, cover) in coverage.into_iter().enumerate() {
                        let opacity = u32::from(cover) * alpha / 255;
                        let over_white =
                            |source: u32| ((255 * (255 - opacity) + source * opacity) / 255) as u8;
                        if format == FxdibFormat::EightBppRgb {
                            let gray = (red * 30 + green * 59 + blue * 11) / 100;
                            assert_eq!(row[x], over_white(gray));
                        } else {
                            assert_eq!(
                                &row[x * 4..x * 4 + 4],
                                &[over_white(blue), over_white(green), over_white(red), 255],
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn glyph_mask_accumulation_unions_overlapping_coverage() {
        let mut first = CfxDibitmap::create(2, 1, FxdibFormat::EightBppMask, 0).unwrap();
        first.scanline_mut(0).unwrap()[..2].copy_from_slice(&[255, 128]);
        let mut second = CfxDibitmap::create(2, 1, FxdibFormat::EightBppMask, 0).unwrap();
        second.scanline_mut(0).unwrap()[..2].copy_from_slice(&[128, 255]);
        let first = CfxGlyphBitmap::new(CfxPoint::new(0, 0), Rc::new(RefCell::new(first)));
        let second = CfxGlyphBitmap::new(CfxPoint::new(0, 0), Rc::new(RefCell::new(second)));
        let glyphs = [
            TextGlyphPos {
                glyph: Some(&first),
                origin: CfxPoint::new(0, 0),
                device_origin: CfxPointF::new(0.0, 0.0),
            },
            TextGlyphPos {
                glyph: Some(&second),
                origin: CfxPoint::new(1, 0),
                device_origin: CfxPointF::new(1.0, 0.0),
            },
        ];
        let mask = compose_glyph_mask(&glyphs, FxRect::new(0, 0, 3, 1), false, 0xffff0000).unwrap();
        assert_eq!(&mask.scanline(0).unwrap()[..3], &[255, 192, 255]);

        let mut target = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        let mut driver = CfxAggDeviceDriver::new(&mut target, false, None);
        assert!(draw_glyph_mask(
            &mut driver,
            &glyphs,
            FxRect::new(0, 0, 3, 1),
            false,
            0xffff0000,
        ));
        assert_eq!(
            &target.scanline(0).unwrap()[..12],
            &[0, 0, 255, 255, 0, 0, 255, 192, 0, 0, 255, 255,]
        );
    }
}

#[cfg(test)]
#[path = "cfx_renderdevice_unittest.rs"]
mod upstream_tests;
