// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Cached-cell and direct tiling from `core/fpdfapi/render/cpdf_rendertiling.{h,cpp}`.

use super::cpdf_rendercontext::CpdfRenderContext;
use super::cpdf_renderoptions::{CpdfRenderOptions, RenderColorMode, RenderOptionsFlags};
use super::cpdf_renderstatus::{object_color, render_holder, RenderResources};
use crate::fpdfapi::page::cpdf_form::CpdfForm;
use crate::fpdfapi::page::cpdf_graphicstates::CpdfGraphicStates;
use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use crate::fpdfapi::page::cpdf_tilingpattern::CpdfTilingPattern;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF, FxRect};
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::{BlendMode, FxdibFormat};
use std::cell::Cell;
use std::rc::Rc;

fn checked_float_to_int(value: f32) -> Option<i32> {
    (value.is_finite() && value as f64 >= i32::MIN as f64 && value as f64 <= i32::MAX as f64)
        .then_some(value as i32)
}

pub(crate) fn draw_pattern_bitmap(
    context: &CpdfRenderContext<'_>,
    pattern: &CpdfTilingPattern,
    form: &CpdfForm,
    mut matrix: CfxMatrix,
    width: i32,
    height: i32,
    draw_options: &RenderOptionsFlags,
) -> Option<CfxDibitmap> {
    // Fitting the complete cell into its bitmap cancels device translation.
    // Remove it before floating-point bounds/subtraction lose local precision.
    matrix.e = 0.;
    matrix.f = 0.;
    let mut bitmap = CfxDibitmap::create(
        width,
        height,
        if pattern.colored() {
            FxdibFormat::Bgra
        } else {
            FxdibFormat::EightBppMask
        },
        0,
    )?;
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        device.set_group_knockout(true);
        let cell_bbox = pattern
            .pattern()
            .pattern_to_form()
            .transform_float_rect(pattern.bbox());
        let cell_bbox = matrix.transform_float_rect(cell_bbox);
        let mut adjust = CfxMatrix::default();
        adjust.match_rect(
            CfxFloatRect::new(0.0, 0.0, width as f32, height as f32),
            cell_bbox,
        );
        let mut options = CpdfRenderOptions::new();
        if !pattern.colored() {
            options.set_color_mode(RenderColorMode::Alpha);
        }
        *options.options_mut() = *draw_options;
        options.options_mut().force_halftone = true;
        // Cell rendering has no page resource scope, but uses the same page
        // cache and document data. The parsed form owns its own resources.
        let mut cell_context = CpdfRenderContext::with_page_resources(context.page(), None);
        cell_context.append_layer(form.holder(), matrix * adjust);
        cell_context.render(&mut device, None, Some(&options), None);
    }
    Some(bitmap)
}

fn clone_obj_states(object: &dyn PageObject, stroke: bool) -> CpdfGraphicStates {
    let mut states = object.base().graphic_states().clone();
    if stroke {
        states
            .mutable_general_state()
            .set_fill_alpha(object.base().general_state().get_stroke_alpha());
    }
    let colors = object.base().color_state();
    let color = if stroke {
        colors.get_stroke_color()
    } else {
        colors.get_fill_color()
    };
    if color.is_some_and(|color| !color.is_null()) {
        let reference = if stroke {
            colors.get_stroke_color_ref()
        } else {
            colors.get_fill_color_ref()
        };
        states.mutable_color_state().set_fill_color_ref(reference);
        states.mutable_color_state().set_stroke_color_ref(reference);
    }
    states
}

// A large offscreen painted object must not force an unbounded cached cell.
// Four million pixels bound the BGRA temporary to 16 MiB, independent of tiles.
const MAX_CACHED_CELL_PIXELS: i64 = 4 * 1024 * 1024;

fn direct_cell(width: i32, height: i32, painted_width: f32, painted_height: f32) -> bool {
    i64::from(width) * i64::from(height) > MAX_CACHED_CELL_PIXELS
        || width as f32 > painted_width.ceil()
        || height as f32 > painted_height.ceil()
}

pub(super) fn draw(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object: &dyn PageObject,
    pattern: &CpdfTilingPattern,
    form: &CpdfForm,
    matrix: CfxMatrix,
    clip: FxRect,
    stroke: bool,
    options: &CpdfRenderOptions,
    resources: RenderResources<'_>,
    depth: usize,
) -> Option<CfxDibitmap> {
    let pattern_to_device = pattern.pattern().pattern_to_form() * matrix;
    let mut cell_matrix = pattern_to_device;
    cell_matrix.e = 0.;
    cell_matrix.f = 0.;
    let cell_bbox = cell_matrix.transform_float_rect(pattern.bbox());
    let width = checked_float_to_int(cell_bbox.width().ceil())?.max(1);
    let height = checked_float_to_int(cell_bbox.height().ceil())?.max(1);
    let (x_step, y_step) = (pattern.x_step(), pattern.y_step());
    if !x_step.is_finite() || !y_step.is_finite() || x_step == 0.0 || y_step == 0.0 {
        return None;
    }
    let clip_pattern = pattern_to_device
        .get_inverse()
        .transform_float_rect(CfxFloatRect::from_fx_rect(clip));
    let bbox = pattern.bbox();
    let mut min_col = checked_float_to_int(((clip_pattern.left - bbox.right) / x_step).ceil())?;
    let mut max_col = checked_float_to_int(((clip_pattern.right - bbox.left) / x_step).floor())?;
    let mut min_row = checked_float_to_int(((clip_pattern.bottom - bbox.top) / y_step).ceil())?;
    let mut max_row = checked_float_to_int(((clip_pattern.top - bbox.bottom) / y_step).floor())?;
    let area = i64::from(width) * i64::from(height);
    // Cached-cell and direct-cell rendering have different rounding. Select
    // between them using complete painted extents, independent of translation
    // and destination clipping, so every region uses the same algorithm.
    let mut extent_matrix = matrix;
    extent_matrix.e = 0.;
    extent_matrix.f = 0.;
    let painted = extent_matrix.transform_float_rect(object.base().rect());
    if direct_cell(width, height, painted.width(), painted.height()) {
        let states = if !pattern.colored() {
            Some(clone_obj_states(object, stroke))
        } else if object.as_any().is::<CpdfPathObject>() {
            let mut states = CpdfGraphicStates::new();
            states.set_default_states();
            states
                .mutable_general_state()
                .set_fill_alpha(object.base().general_state().get_fill_alpha());
            Some(states)
        } else {
            None
        };
        for col in min_col..=max_col {
            for row in min_row..=max_row {
                let original = pattern_to_device
                    .transform(CfxPointF::new(col as f32 * x_step, row as f32 * y_step));
                let mut cell_matrix = matrix;
                cell_matrix.translate(
                    original.x - pattern_to_device.e,
                    original.y - pattern_to_device.f,
                );
                let stopped = Cell::new(false);
                let mut child = RenderResources::new(resources.context, &stopped);
                child.transparency = form.holder().transparency();
                let form_scope = super::cpdf_renderstatus::form_resources(form);
                child.form_resources = form_scope.as_ref();
                if let Some(states) = &states {
                    let colors = states.color_state();
                    child.initial_fill_alpha = states.general_state().get_fill_alpha();
                    child.initial_fill_color = if colors.has_fill_color() {
                        Some(colors)
                    } else {
                        resources.initial_fill_color
                    };
                    child.initial_stroke_color = if colors.has_stroke_color() {
                        Some(colors)
                    } else {
                        resources.initial_stroke_color
                    };
                }
                driver.save_state();
                render_holder(
                    form.holder(),
                    driver,
                    cell_matrix,
                    options,
                    child,
                    depth + 1,
                    None,
                    &[],
                );
                driver.restore_state(false);
            }
        }
        return None;
    }
    let aligned = bbox.left == 0.0
        && bbox.bottom == 0.0
        && bbox.right == x_step
        && bbox.top == y_step
        && (pattern_to_device.is_scaled() || pattern_to_device.is_90_rotated());
    let orig_x = pattern_to_device.e.round() as i32;
    let orig_y = pattern_to_device.f.round() as i32;
    if aligned {
        min_col = clip.left.checked_sub(orig_x)? / width;
        if clip.left < orig_x {
            min_col = min_col.checked_sub(1)?;
        }
        max_col = clip.right.checked_sub(orig_x)? / width;
        if clip.right <= orig_x {
            max_col = max_col.checked_sub(1)?;
        }
        min_row = clip.top.checked_sub(orig_y)? / height;
        if clip.top < orig_y {
            min_row = min_row.checked_sub(1)?;
        }
        max_row = clip.bottom.checked_sub(orig_y)? / height;
        if clip.bottom <= orig_y {
            max_row = max_row.checked_sub(1)?;
        }
    }
    let left_offset = cell_bbox.left;
    let top_offset = cell_bbox.bottom;
    let mut pattern_bitmap = if area < 16 {
        let enlarged = draw_pattern_bitmap(
            resources.context,
            pattern,
            form,
            matrix,
            8,
            8,
            options.options(),
        )?;
        let resized = crate::fxge::dib::cfx_dibbase::stretch_to(
            Rc::new(enlarged),
            width,
            height,
            Default::default(),
            None,
        )?;
        // StretchTo returns a uniquely owned destination; transfer it without
        // copying its pixel buffer.
        Rc::try_unwrap(resized).ok()?.into_inner()
    } else {
        draw_pattern_bitmap(
            resources.context,
            pattern,
            form,
            matrix,
            width,
            height,
            options.options(),
        )?
    };
    if options.color_mode_is(RenderColorMode::Gray) {
        pattern_bitmap.convert_color_scale(false);
    }
    // Native's cached-cell path always reads the fill color, which also
    // gives stroke patterns the wrong caller color. Select the painted side.
    let fill_argb = object_color(object, !stroke, options, resources);
    // The parsed cell inherited the caller's opacity (and may override it).
    // Its alpha-only bitmap already contains that coverage. Applying the
    // caller's alpha again while coloring the mask squares its opacity,
    // unlike the direct-cell path and the one-pixel cached-cell path.
    let fill_argb = if pattern.colored() {
        fill_argb
    } else {
        fill_argb | 0xff00_0000
    };
    if aligned && area >= 16 {
        paint_aligned_cells(
            driver,
            &pattern_bitmap,
            pattern.colored(),
            fill_argb,
            orig_x,
            orig_y,
            min_col..=max_col,
            min_row..=max_row,
        )?;
        return None;
    }
    let mut screen = CfxDibitmap::create(clip.width(), clip.height(), FxdibFormat::Bgra, 0)?;
    for col in min_col..=max_col {
        for row in min_row..=max_row {
            let (start_x, start_y) = if aligned {
                (
                    orig_x
                        .checked_add(col.checked_mul(width)?)?
                        .checked_sub(clip.left)?,
                    orig_y
                        .checked_add(row.checked_mul(height)?)?
                        .checked_sub(clip.top)?,
                )
            } else {
                let original = pattern_to_device
                    .transform(CfxPointF::new(col as f32 * x_step, row as f32 * y_step));
                (
                    ((original.x + left_offset).round() as i32).checked_sub(clip.left)?,
                    ((original.y + top_offset).round() as i32).checked_sub(clip.top)?,
                )
            };
            if width == 1 && height == 1 {
                if start_x < 0 || start_x >= clip.width() || start_y < 0 || start_y >= clip.height()
                {
                    continue;
                }
                let color = if pattern.colored() {
                    u32::from_le_bytes(pattern_bitmap.buffer()[..4].try_into().unwrap())
                } else {
                    (u32::from(pattern_bitmap.buffer()[0]) << 24) | (fill_argb & 0xffffff)
                };
                screen.scanline_mut(start_y).unwrap()
                    [start_x as usize * 4..start_x as usize * 4 + 4]
                    .copy_from_slice(&color.to_le_bytes());
            } else if pattern.colored() {
                screen.composite_bitmap(
                    start_x,
                    start_y,
                    width,
                    height,
                    &pattern_bitmap,
                    0,
                    0,
                    BlendMode::Normal,
                );
            } else {
                screen.composite_mask_normal(
                    start_x,
                    start_y,
                    width,
                    height,
                    &pattern_bitmap,
                    fill_argb,
                    0,
                    0,
                    false,
                );
            }
        }
    }
    Some(screen)
}

// Aligned cached cells occupy disjoint integer rectangles. Composite them
// directly under the existing device clip instead of allocating and compositing
// an additional clip-sized screen. Keep the cached-cell rasterization unchanged.
fn paint_aligned_cells(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    bitmap: &CfxDibitmap,
    colored: bool,
    fill_argb: u32,
    orig_x: i32,
    orig_y: i32,
    columns: std::ops::RangeInclusive<i32>,
    rows: std::ops::RangeInclusive<i32>,
) -> Option<()> {
    let width = bitmap.width();
    let height = bitmap.height();
    let mut source_left = 0;
    let mut source_top = 0;
    let colored_mask;
    let source = if colored {
        bitmap
    } else {
        // Preserve the mask-to-transparent-BGRA stage and its integer alpha
        // rounding before the final destination/clip composition.
        // A narrow region often sees only the empty edge of a large cell.
        // Color only the source rectangle that can reach the current clip.
        // Multiple visible cells use the complete axis, including wraparound.
        let visible_axis = |start: i32, end: i32, origin: i32, extent: i32| {
            let start = i64::from(start) - i64::from(origin);
            let end = i64::from(end) - i64::from(origin);
            let extent = i64::from(extent);
            if start.div_euclid(extent) == (end - 1).div_euclid(extent) {
                (start.rem_euclid(extent) as i32, (end - start) as i32)
            } else {
                (0, extent as i32)
            }
        };
        let clip = driver.clip_box();
        if clip.is_empty() {
            return Some(());
        }
        let (left, visible_width) = visible_axis(clip.left, clip.right, orig_x, width);
        let (top, visible_height) = visible_axis(clip.top, clip.bottom, orig_y, height);
        source_left = left;
        source_top = top;
        let mut cell = CfxDibitmap::create(visible_width, visible_height, FxdibFormat::Bgra, 0)?;
        cell.composite_mask_normal(
            0,
            0,
            visible_width,
            visible_height,
            bitmap,
            fill_argb,
            left,
            top,
            false,
        );
        colored_mask = cell;
        &colored_mask
    };
    // Validate the complete range before painting so overflow cannot leave a
    // partially painted pattern where the intermediate-screen path drew none.
    for col in [*columns.start(), *columns.end()] {
        orig_x
            .checked_add(col.checked_mul(width)?)?
            .checked_add(source_left)?;
    }
    for row in [*rows.start(), *rows.end()] {
        orig_y
            .checked_add(row.checked_mul(height)?)?
            .checked_add(source_top)?;
    }
    let source_rect = FxRect::new(0, 0, source.width(), source.height());
    for col in columns {
        let x = orig_x
            .checked_add(col.checked_mul(width)?)?
            .checked_add(source_left)?;
        for row in rows.clone() {
            let y = orig_y
                .checked_add(row.checked_mul(height)?)?
                .checked_add(source_top)?;
            driver.set_dibits_normal(source, 0, source_rect, x, y);
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_page::CpdfPage;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    fn render_pattern(size: i32, recursive: bool) -> CfxDibitmap {
        render_pattern_at(size, recursive, false, CfxMatrix::default(), 4, 4)
    }

    fn render_pattern_at(
        size: i32,
        recursive: bool,
        half_cell: bool,
        matrix: CfxMatrix,
        width: i32,
        height: i32,
    ) -> CfxDibitmap {
        let objects = CpdfIndirectObjectHolder::new();
        let mut dict = CpdfDictionary::new();
        for (key, value) in [
            (b"PatternType".as_slice(), 1),
            (b"PaintType", if recursive { 1 } else { 2 }),
            (b"XStep", size),
            (b"YStep", size),
        ] {
            dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        dict.set_rect_for(
            b"BBox",
            CfxFloatRect::new(0.0, 0.0, size as f32, size as f32),
        );
        let dict = retain_object(dict);
        let body = if recursive {
            b"/Pattern cs /P scn 0 0 1 1 re f".to_vec()
        } else {
            let painted = if half_cell {
                size as f32 / 2.
            } else {
                size as f32
            };
            format!("0 0 {painted} {painted} re f").into_bytes()
        };
        let stream = retain_object(CpdfStream::from_data_and_dict(body, dict.clone()));
        let number = objects.borrow_mut().add_indirect_object(stream);
        let mut patterns = CpdfDictionary::new();
        patterns.set_new_for(b"P", CpdfReference::new(&objects, number));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Pattern", patterns);
        let mut cs = CpdfArray::new();
        cs.append_new_name(b"Pattern");
        cs.append_new_name(b"DeviceRGB");
        let mut spaces = CpdfDictionary::new();
        spaces.set_new_for(b"PatRGB", cs);
        resources.set_new_for(b"ColorSpace", spaces);
        let mut half = CpdfDictionary::new();
        half.set_new_for(b"ca", CpdfScalarObject::number_from_float(0.5));
        let mut states = CpdfDictionary::new();
        states.set_new_for(b"Half", half);
        resources.set_new_for(b"ExtGState", states);
        let resources = retain_object(resources);
        if recursive {
            dict.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_for(b"Resources", Some(resources.clone()));
        }
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(if recursive {
                b"/Pattern cs /P scn 0 0 4 4 re f"
            } else {
                b"/Half gs /PatRGB cs 0 0 1 /P scn 0 0 4 4 re f"
            });
        let mut page = CpdfDictionary::new();
        page.set_for(b"Resources", Some(resources));
        page.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(width, height, FxdibFormat::Bgra, 0).unwrap();
        assert!(super::super::cpdf_renderstatus::render_page_objects(
            &page,
            &mut bitmap,
            matrix,
            &CpdfRenderOptions::new(),
            false
        ));
        bitmap
    }

    #[test]
    fn aligned_cells_match_intermediate_screen_with_alpha_clips_and_byte_order() {
        use crate::fxge::cfx_fillrenderoptions::{CfxFillRenderOptions, FillType};
        use crate::fxge::cfx_path::{CfxPath, PointType};

        // Swapped cell dimensions and translated origins also cover the integer
        // placement used by quarter-turn and reflected pattern transforms.
        for (width, height, orig_x, orig_y) in [(4, 7, -2, 3), (7, 4, 3, -2)] {
            for colored in [false, true] {
                let format = if colored {
                    FxdibFormat::Bgra
                } else {
                    FxdibFormat::EightBppMask
                };
                let mut cell = CfxDibitmap::create(width, height, format, 0).unwrap();
                for y in 0..height {
                    for x in 0..width {
                        let alpha = [0, 1, 63, 127, 128, 254, 255][((x + y * width) % 7) as usize];
                        if colored {
                            cell.scanline_mut(y).unwrap()[x as usize * 4..x as usize * 4 + 4]
                                .copy_from_slice(&[17 + x as u8, 211 - y as u8, 89, alpha]);
                        } else {
                            cell.scanline_mut(y).unwrap()[x as usize] = alpha;
                        }
                    }
                }
                for background in [0, 0x01335577, 0x80335577, 0xff335577] {
                    for rgb_order in [false, true] {
                        for knockout in [false, true] {
                            for clip_kind in 0..6 {
                                let render = |direct: bool| {
                                    let mut bitmap =
                                        CfxDibitmap::create(17, 19, FxdibFormat::Bgra, 0).unwrap();
                                    bitmap.clear(background);
                                    {
                                        let mut driver =
                                            CfxAggDeviceDriver::new(&mut bitmap, rgb_order, None);
                                        driver.set_group_knockout(knockout);
                                        if clip_kind >= 3 {
                                            let clip = match clip_kind {
                                                3 => FxRect::new(3, 4, 5, 6),
                                                4 => FxRect::new(3, 0, 5, 19),
                                                _ => FxRect::new(0, 4, 17, 6),
                                            };
                                            assert!(driver.set_clip_rect(clip));
                                        } else if clip_kind == 1 {
                                            assert!(driver.set_clip_rect(FxRect::new(2, 3, 15, 17)));
                                        } else if clip_kind == 2 {
                                            let mut path = CfxPath::new();
                                            path.append_point(
                                                CfxPointF::new(1.25, 2.75),
                                                PointType::Move,
                                            );
                                            path.append_point(
                                                CfxPointF::new(15.5, 6.25),
                                                PointType::Line,
                                            );
                                            path.append_point_and_close(
                                                CfxPointF::new(9.75, 17.5),
                                                PointType::Line,
                                            );
                                            assert!(driver.set_clip_path_fill(
                                                &path,
                                                None,
                                                CfxFillRenderOptions {
                                                    fill_type: FillType::Winding,
                                                    ..Default::default()
                                                }
                                            ));
                                        }
                                        if direct {
                                            paint_aligned_cells(
                                                &mut driver,
                                                &cell,
                                                colored,
                                                0xff336699,
                                                orig_x,
                                                orig_y,
                                                -1..=5,
                                                -1..=5,
                                            )
                                            .unwrap();
                                        } else {
                                            let mut screen =
                                                CfxDibitmap::create(17, 19, FxdibFormat::Bgra, 0)
                                                    .unwrap();
                                            for col in -1..=5 {
                                                for row in -1..=5 {
                                                    let x = orig_x + col * width;
                                                    let y = orig_y + row * height;
                                                    if colored {
                                                        screen.composite_bitmap(
                                                            x,
                                                            y,
                                                            width,
                                                            height,
                                                            &cell,
                                                            0,
                                                            0,
                                                            BlendMode::Normal,
                                                        );
                                                    } else {
                                                        screen.composite_mask_normal(
                                                            x, y, width, height, &cell, 0xff336699,
                                                            0, 0, false,
                                                        );
                                                    }
                                                }
                                            }
                                            assert!(driver.set_dibits_normal(
                                                &screen,
                                                0,
                                                FxRect::new(0, 0, 17, 19),
                                                0,
                                                0
                                            ));
                                        }
                                    }
                                    bitmap
                                };
                                assert_eq!(render(true).buffer(), render(false).buffer(),
                                    "colored={colored}, background={background:x}, rgb={rgb_order}, knockout={knockout}, clip={clip_kind}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cached_cell_budget_is_independent_of_device_clip() {
        assert!(!direct_cell(2048, 2048, 4096., 4096.));
        assert!(direct_cell(2049, 2048, 4096., 4096.));
        assert!(direct_cell(150, 150, 88., 300.));
        assert!(!direct_cell(150, 150, 300., 300.));
    }

    #[test]
    fn aligned_cached_cells_keep_rotated_column_and_row_pixels() {
        let scale = 300. / 72.;
        let extent = 17;
        for matrix in [
            CfxMatrix::new(scale, 0., 0., scale, 0., 0.),
            CfxMatrix::new(0., scale, -scale, 0., extent as f32, 0.),
            CfxMatrix::new(-scale, 0., 0., -scale, extent as f32, extent as f32),
            CfxMatrix::new(0., -scale, scale, 0., 0., extent as f32),
        ] {
            let whole = render_pattern_at(2, false, true, matrix, extent, extent);
            assert!(whole.buffer().iter().any(|&byte| byte != 0));
            for (x, y, width, height) in [(10, 0, 7, 17), (0, 10, 17, 7), (10, 10, 7, 7)] {
                let mut translated = matrix;
                translated.e -= x as f32;
                translated.f -= y as f32;
                let part = render_pattern_at(2, false, true, translated, width, height);
                for row in 0..height {
                    assert_eq!(
                        &whole.scanline(y + row).unwrap()[x as usize * 4..(x + width) as usize * 4],
                        &part.scanline(row).unwrap()[..width as usize * 4],
                        "matrix={matrix:?}, crop={x},{y},{width},{height}, row={row}"
                    );
                }
            }
        }
    }

    #[test]
    fn clipped_pattern_uses_the_same_cell_algorithm_as_the_complete_object() {
        for (size, scale) in [(2, 0.75_f32), (2, 300. / 72.), (8, 300. / 72.)] {
            let extent = (4. * scale).ceil() as i32;
            let x = (2.5 * scale).floor() as i32;
            let width = extent - x;
            let whole = render_pattern_at(
                size,
                false,
                true,
                CfxMatrix::new(scale, 0., 0., scale, 0., 0.),
                extent,
                extent,
            );
            let part = render_pattern_at(
                size,
                false,
                true,
                CfxMatrix::new(scale, 0., 0., scale, -(x as f32), 0.),
                width,
                extent,
            );
            for y in 0..extent {
                assert_eq!(
                    &whole.scanline(y).unwrap()[x as usize * 4..extent as usize * 4],
                    &part.scanline(y).unwrap()[..width as usize * 4],
                    "scale={scale} row={y}"
                );
            }
        }
    }

    #[test]
    fn uncolored_tiling_applies_alpha_once_for_single_pixel_cached_and_direct_cells() {
        for size in [1, 2, 8] {
            let bitmap = render_pattern(size, false);
            assert_eq!(
                &bitmap.scanline(2).unwrap()[8..12],
                &[255, 0, 0, 127],
                "cell size {size}"
            );
        }
    }

    #[test]
    fn recursive_cached_pattern_contexts_share_the_render_recursion_limit() {
        let bitmap = render_pattern(1, true);
        assert!(bitmap.buffer().iter().all(|&value| value == 0));
        // The thread counter must be restored for the next independent render.
        assert_eq!(render_pattern(1, false).scanline(2).unwrap()[11], 127);
    }
}
