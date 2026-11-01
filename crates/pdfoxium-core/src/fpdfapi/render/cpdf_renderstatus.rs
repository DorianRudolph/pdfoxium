// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Page object dispatch from `core/fpdfapi/render/cpdf_renderstatus.cpp`.
//! Static AGG object dispatch, initial states, colors, clips, transparency,
//! Type 3 glyphs and drawing fallbacks; codec/device/font adapters live nearby.

use super::cpdf_imagerenderer::{
    calculate_draw_image, draw_dib_base_with_blend, draw_masked_image, get_unit_rect,
    image_blend_type, image_decode_size, start_bitmap_alpha,
};
use super::cpdf_rendercontext::CpdfRenderContext;
use super::cpdf_textrenderer::{draw_pdf_normal_text_agg, draw_pdf_text_path_agg};
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::page::cpdf_form::{CpdfForm, CpdfType3Form};
use crate::fpdfapi::page::cpdf_formobject::CpdfFormObject;
use crate::fpdfapi::page::cpdf_function::CpdfFunction;
use crate::fpdfapi::page::cpdf_imageloader::CpdfImageLoader;
use crate::fpdfapi::page::cpdf_imageobject::CpdfImageObject;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fpdfapi::page::cpdf_pageobjectholder::CpdfPageObjectHolder;
use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use crate::fpdfapi::page::cpdf_pattern::PatternKind;
use crate::fpdfapi::page::cpdf_shadingobject::CpdfShadingObject;
use crate::fpdfapi::page::cpdf_shadingpattern::CpdfShadingPattern;
use crate::fpdfapi::page::cpdf_textobject::CpdfTextObject;
use crate::fpdfapi::page::cpdf_textstate::TextRenderingMode;
use crate::fpdfapi::page::cpdf_tilingpattern::CpdfTilingPattern;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::render::cpdf_renderoptions::{CpdfRenderOptions, RenderColorMode};
use crate::fpdfdoc::cpdf_annotlist::CpdfAnnotList;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPoint, FxRect};
use crate::fxge::agg::cfx_agg_cliprgn::CfxAggClipRgn;
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use crate::fxge::cfx_fillrenderoptions::{CfxFillRenderOptions, FillType};
use crate::fxge::cfx_fontmapper::CfxFontMapper;
use crate::fxge::cfx_fontmgr::with_font_manager;
use crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap;
#[cfg(test)]
use crate::fxge::cfx_linuxfontinfo::CfxLinuxFontInfo;
use crate::fxge::cfx_path::CfxPath;
use crate::fxge::dib::cfx_dibitmap::{CfxDibitmap, CfxDibitmapData};
use crate::fxge::dib::fx_dib::{alpha_and_color_ref_to_argb, argb_a, rgb_to_gray, FxdibFormat};
use crate::fxge::fx_font::get_glyphs_bbox;
use crate::fxge::text_glyph_pos::TextGlyphPos;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

thread_local! {
    // Native uses g_CurrentRecursionDepth with an AutoRestorer. Keep one
    // counter for all nested contexts on the rendering thread.
    static CURRENT_RENDER_DEPTH: Cell<u32> = const { Cell::new(0) };
}

struct RenderRecursionGuard(u32);
impl RenderRecursionGuard {
    fn enter() -> Option<Self> {
        CURRENT_RENDER_DEPTH.with(|depth| {
            let previous = depth.get();
            if previous >= 64 {
                return None;
            }
            depth.set(previous + 1);
            Some(Self(previous))
        })
    }
}
impl Drop for RenderRecursionGuard {
    fn drop(&mut self) {
        CURRENT_RENDER_DEPTH.with(|depth| depth.set(self.0));
    }
}

// The selected RenderContext resources travel with every nested render status.
// The page owns its cache; forms, patterns and annotation layers borrow it.
#[derive(Clone, Copy)]
pub(super) struct RenderResources<'a> {
    pub(super) context: &'a CpdfRenderContext<'a>,
    pub(super) progressive_root: bool,
    pub(super) stop_object: Option<&'a dyn PageObject>,
    pub(super) stopped: &'a Cell<bool>,
    pub(super) device_matrix: CfxMatrix,
    pub(super) transparency: crate::fpdfapi::page::cpdf_transparency::CpdfTransparency,
    pub(super) in_group: bool,
    pub(super) initial_fill_alpha: f32,
    pub(super) type3_fill_color: Option<u32>,
    pub(super) is_type3_char: bool,
    pub(super) type3_colored: bool,
    pub(super) form_resources: Option<&'a crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    pub(super) initial_fill_color:
        Option<&'a crate::fpdfapi::page::cpdf_colorstate::CpdfColorState>,
    pub(super) initial_stroke_color:
        Option<&'a crate::fpdfapi::page::cpdf_colorstate::CpdfColorState>,
    pub(super) data: &'a Rc<CpdfDocPageData>,
    pub(super) std_cs: bool,
    pub(super) group_family: crate::fpdfapi::page::cpdf_colorspace::ColorSpaceFamily,
    pub(super) load_mask: bool,
    pub(super) page_resources: Option<&'a crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    pub(super) image_cache: &'a crate::fpdfapi::page::cpdf_pageimagecache::CpdfPageImageCache,
}

impl<'a> RenderResources<'a> {
    pub(super) fn new(context: &'a CpdfRenderContext<'a>, stopped: &'a Cell<bool>) -> Self {
        let page = context.page();
        Self {
            context,
            progressive_root: false,
            stop_object: None,
            stopped,
            device_matrix: CfxMatrix::default(),
            transparency: Default::default(),
            in_group: false,
            initial_fill_alpha: 1.0,
            type3_fill_color: None,
            is_type3_char: false,
            type3_colored: false,
            form_resources: None,
            initial_fill_color: None,
            initial_stroke_color: None,
            data: page.page_data_handle(),
            std_cs: false,
            group_family: crate::fpdfapi::page::cpdf_colorspace::ColorSpaceFamily::Unknown,
            load_mask: false,
            page_resources: context.page_resources(),
            image_cache: page.image_cache(),
        }
    }
}

impl std::ops::Deref for RenderResources<'_> {
    type Target = Rc<CpdfDocPageData>;
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

pub(crate) fn with_render_font_mapper<R>(f: impl FnOnce(&mut CfxFontMapper) -> R) -> R {
    with_font_manager(|manager| {
        let mut mapper = manager.builtin_mapper_mut();
        f(&mut mapper)
    })
}

fn render_simple_text(
    object: &CpdfTextObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
    page_resources: Option<&crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    depth: usize,
    clipping_path: Option<&mut CfxPath>,
    active_type3_fonts: &[usize],
) -> bool {
    if object.char_codes().is_empty()
        || object.base().text_state().get_text_mode() == TextRenderingMode::Invisible
    {
        return true;
    }
    let Some(font_dict_ref) = object.base().text_state().get_font_dict() else {
        return true;
    };
    let Some(font) =
        object.base().text_state().get_font().cloned().or_else(|| {
            with_render_font_mapper(|mapper| page_data.get_font(&font_dict_ref, mapper))
        })
    else {
        return true;
    };
    if matches!(font, CpdfFont::Type3(_)) {
        return render_type3_text(
            object,
            driver,
            object_to_device,
            options,
            page_data,
            page_resources,
            depth,
            &font_dict_ref,
            active_type3_fonts,
        );
    }
    draw_page_text(
        object,
        driver,
        object_to_device,
        options,
        &font,
        clipping_path,
        page_data,
        page_resources,
        depth,
    )
}

fn render_type3_text(
    object: &CpdfTextObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
    _page_resources: Option<&crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    depth: usize,
    font_dict: &crate::fpdfapi::parser::cpdf_object::ObjectRef,
    active_type3_fonts: &[usize],
) -> bool {
    if matches!(
        object.base().text_state().get_text_mode(),
        TextRenderingMode::Invisible
    ) {
        return true;
    }
    let Some(font) = (match object.base().text_state().get_font() {
        Some(CpdfFont::Type3(font)) => Some(Rc::clone(font)),
        _ => page_data.get_type3_font(font_dict),
    }) else {
        return true;
    };
    let identity = Rc::as_ptr(&font) as usize;
    if active_type3_fonts.contains(&identity) {
        return true;
    }
    // Nested uncolored Type3 text paints with the caller's color, just as
    // paths and ordinary text in the same uncolored glyph do.
    let fill_color = object_color(object, true, options, page_data);
    let fill_alpha = argb_a(fill_color);
    let mut char_matrix = font.font_matrix;
    let font_size = object.base().text_state().get_font_size();
    char_matrix.scale(font_size, font_size);
    // Cache owners precede glyph handles so their destruction order also
    // matches the source, even though the Rust glyphs own their pixel data.
    let mut cache = None;
    // IsPrint() is false for the selected AGG display device, even when
    // annotation/optional-content options request print usage.
    let mut glyphs: Vec<Option<(Rc<CfxGlyphBitmap>, CfxPoint)>> =
        vec![None; object.char_codes().len()];
    let mut child_fonts = active_type3_fonts.to_vec();
    child_fonts.push(identity);
    for (index, (&code, &position)) in object
        .char_codes()
        .iter()
        .zip(object.char_positions())
        .enumerate()
    {
        if code == u32::MAX {
            continue;
        }
        let Some(glyph) = font.load_char(code) else {
            continue;
        };
        let mut matrix = char_matrix;
        matrix.e += position;
        matrix.concat(object.text_matrix());
        matrix.concat(object_to_device);
        let bitmap_loaded = glyph.borrow_mut().load_bitmap_from_sole_image_of_form();
        if !bitmap_loaded {
            if !glyphs.is_empty() {
                for (bitmap, origin) in glyphs[..index].iter().flatten() {
                    paint_type3_bitmap(driver, bitmap, *origin, fill_color);
                }
                glyphs.clear();
            }
            let (form, colored) = {
                let glyph = glyph.borrow();
                (glyph.retained_form(), glyph.colored())
            };
            let Some(form) = form else {
                continue;
            };
            let form_any: &dyn std::any::Any = form.as_ref();
            let Some(form) = form_any.downcast_ref::<CpdfType3Form>() else {
                continue;
            };
            let mut glyph_options = options.clone();
            glyph_options.options_mut().force_halftone = true;
            glyph_options.options_mut().rect_aa = true;
            let inherited_color = (!colored).then_some(fill_color);
            let stopped = Cell::new(false);
            let mut glyph_resources = RenderResources::new(page_data.context, &stopped);
            glyph_resources.transparency = form.form().holder().transparency();
            glyph_resources.type3_fill_color = Some(fill_color);
            glyph_resources.is_type3_char = true;
            glyph_resources.type3_colored = colored;
            let glyph_scope = form_resources(form.form());
            glyph_resources.form_resources = glyph_scope.as_ref();
            if fill_alpha == 255 {
                driver.save_state();
                let success = render_holder(
                    form.form().holder(),
                    driver,
                    matrix,
                    &glyph_options,
                    glyph_resources,
                    depth + 1,
                    inherited_color,
                    &child_fonts,
                );
                driver.restore_state(false);
                if !success {
                    return false;
                }
            } else {
                let rect = matrix
                    .transform_float_rect(form.form().calc_bounding_box())
                    .get_outer_rect();
                if !rect.valid() {
                    continue;
                }
                let Some(mut pixels) =
                    CfxDibitmap::create(rect.width(), rect.height(), FxdibFormat::Bgra, 0)
                else {
                    return true;
                };
                matrix.translate(-(rect.left as f32), -(rect.top as f32));
                {
                    let mut target = CfxAggDeviceDriver::new(&mut pixels, false, None);
                    if !render_holder(
                        form.form().holder(),
                        &mut target,
                        matrix,
                        &glyph_options,
                        glyph_resources,
                        depth + 1,
                        inherited_color,
                        &child_fonts,
                    ) {
                        return false;
                    }
                }
                // Colored character procedures supply their own RGB values,
                // so they do not inherit type3_fill_color's alpha. Apply the
                // invoking text's opacity to this local plane exactly once.
                // Uncolored procedures already paint with that fill alpha.
                if colored {
                    pixels.multiply_alpha(fill_alpha as f32 / 255.0);
                }
                driver.set_dibits_normal(
                    &pixels,
                    0,
                    FxRect::new(0, 0, pixels.width(), pixels.height()),
                    rect.left,
                    rect.top,
                );
            }
        } else if glyph.borrow().get_bitmap().is_some() {
            let cache =
                cache.get_or_insert_with(|| page_data.render_data().get_cached_type3(&font));
            let Some(bitmap) = cache.load_glyph_bitmap(code, matrix) else {
                continue;
            };
            let origin = CfxPoint::new(matrix.e.round() as i32, matrix.f.round() as i32);
            if glyphs.is_empty() {
                paint_type3_bitmap(driver, &bitmap, origin, fill_color);
            } else {
                glyphs[index] = Some((bitmap, origin));
            }
        }
    }
    if glyphs.is_empty() {
        return true;
    }
    let positions: Vec<_> = glyphs
        .iter()
        .flatten()
        .map(|(glyph, origin)| TextGlyphPos {
            glyph: Some(glyph.as_ref()),
            origin: *origin,
            ..TextGlyphPos::default()
        })
        .collect();
    let rect = get_glyphs_bbox(&positions, false);
    let (Some(width), Some(height)) = (
        rect.right.checked_sub(rect.left),
        rect.bottom.checked_sub(rect.top),
    ) else {
        return true;
    };
    let Some(mut bitmap) = CfxDibitmap::create(width, height, FxdibFormat::EightBppMask, 0) else {
        return true;
    };
    for position in positions {
        let glyph = position.glyph.expect("populated glyph");
        let source = glyph.bitmap();
        if !matches!(
            source.format(),
            FxdibFormat::OneBppMask | FxdibFormat::EightBppMask
        ) {
            continue;
        }
        let Some(point) = position.get_origin(CfxPoint::new(rect.left, rect.top)) else {
            continue;
        };
        bitmap.composite_mask_normal(
            point.x,
            point.y,
            source.width(),
            source.height(),
            &source,
            // This plane accumulates glyph coverage, not the paint opacity.
            // set_dibits_normal below applies fill_color's alpha once; using
            // it here too would square the text opacity for bitmap batches.
            0xffff_ffff,
            0,
            0,
            false,
        );
    }
    driver.set_dibits_normal(
        &bitmap,
        fill_color,
        FxRect::new(0, 0, width, height),
        rect.left,
        rect.top,
    );
    true
}

fn paint_type3_bitmap(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    bitmap: &CfxGlyphBitmap,
    origin: CfxPoint,
    color: u32,
) {
    let position = TextGlyphPos {
        glyph: Some(bitmap),
        origin,
        ..TextGlyphPos::default()
    };
    let Some(point) = position.get_origin(CfxPoint::new(0, 0)) else {
        return;
    };
    let bitmap = bitmap.bitmap();
    driver.set_dibits_normal(
        &bitmap,
        color,
        FxRect::new(0, 0, bitmap.width(), bitmap.height()),
        point.x,
        point.y,
    );
}

fn is_available_matrix(matrix: CfxMatrix) -> bool {
    if matrix.a == 0.0 || matrix.d == 0.0 {
        matrix.b != 0.0 && matrix.c != 0.0
    } else if matrix.b == 0.0 || matrix.c == 0.0 {
        matrix.a != 0.0 && matrix.d != 0.0
    } else {
        true
    }
}

fn text_path_fill_options(
    object: &CpdfTextObject,
    options: &CpdfRenderOptions,
    stroke: bool,
    fill: bool,
) -> CfxFillRenderOptions {
    let mut result = CfxFillRenderOptions::default();
    if stroke && fill {
        result.stroke = true;
        result.stroke_text_mode = true;
    }
    result.adjust_stroke = object.base().general_state().get_stroke_adjust();
    result.aliased_path = options.options().no_text_smooth;
    result
}

fn draw_page_text(
    object: &CpdfTextObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    font: &CpdfFont,
    clipping_path: Option<&mut CfxPath>,
    page_data: RenderResources<'_>,
    page_resources: Option<&crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    depth: usize,
) -> bool {
    let mode = object.base().text_state().get_text_mode();
    let (fill, stroke) = if clipping_path.is_some() {
        (false, false)
    } else {
        let has_face = font.font().is_some_and(|font| font.face().is_some());
        match mode {
            TextRenderingMode::Fill | TextRenderingMode::FillClip => (true, false),
            TextRenderingMode::Stroke | TextRenderingMode::StrokeClip => (!has_face, has_face),
            TextRenderingMode::FillStroke | TextRenderingMode::FillStrokeClip => (true, has_face),
            TextRenderingMode::Invisible | TextRenderingMode::Clip => return true,
            TextRenderingMode::Unknown => unreachable!("unknown text rendering mode"),
        }
    };
    let fill_pattern = fill
        && object
            .base()
            .color_state()
            .get_fill_color()
            .is_some_and(|color| color.is_pattern());
    let stroke_pattern = stroke
        && object
            .base()
            .color_state()
            .get_stroke_color()
            .is_some_and(|color| color.is_pattern());
    let fill_color = if fill && !fill_pattern {
        object_color(object, true, options, page_data)
    } else {
        0
    };
    let stroke_color = if stroke && !stroke_pattern {
        object_color(object, false, options, page_data)
    } else {
        0
    };
    let mut text_matrix = object.text_matrix();
    if !is_available_matrix(text_matrix) {
        return true;
    }
    let font_size = object.base().text_state().get_font_size();
    if fill_pattern || stroke_pattern {
        return draw_text_path_with_pattern(
            object,
            driver,
            object_to_device,
            options,
            font,
            font_size,
            text_matrix,
            fill,
            stroke,
            page_data,
            page_resources,
            depth,
        );
    }
    if clipping_path.is_some() || stroke {
        let mut device_matrix = object_to_device;
        if stroke {
            let ctm = object.base().text_state().get_ctm();
            if ctm[0] != 1.0 || ctm[3] != 1.0 {
                let ctm = CfxMatrix::new(ctm[0], ctm[1], ctm[2], ctm[3], 0.0, 0.0);
                text_matrix *= ctm.get_inverse();
                device_matrix = ctm * object_to_device;
            }
        }
        return draw_pdf_text_path_agg(
            driver,
            object.char_codes(),
            object.char_positions(),
            font,
            font_size,
            text_matrix,
            Some(device_matrix),
            object.base().graph_state().get_object(),
            fill_color,
            stroke_color,
            clipping_path,
            text_path_fill_options(object, options, stroke, fill),
        );
    }
    draw_pdf_normal_text_agg(
        driver,
        object.char_codes(),
        object.char_positions(),
        font,
        font_size,
        text_matrix * object_to_device,
        fill_color,
        options,
    )
}

fn draw_text_path_with_pattern(
    object: &CpdfTextObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    font: &CpdfFont,
    font_size: f32,
    text_matrix: CfxMatrix,
    fill: bool,
    stroke: bool,
    page_data: RenderResources<'_>,
    _page_resources: Option<&crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    depth: usize,
) -> bool {
    if !stroke {
        let mut path = CpdfPathObject::default();
        path.set_filltype(FillType::Winding);
        // Parent clips are already active in this per-object saved device
        // state. Apply only the newly cloned text layer, avoiding a second
        // multiplication of the parent's antialiased clip coverage.
        path.base_mut()
            .mutable_clip_path()
            .append_texts(&mut vec![Box::new(object.clone())]);
        *path.base_mut().mutable_color_state() = object.base().color_state().clone();
        *path.base_mut().mutable_general_state() = object.base().general_state().clone();
        path.mutable_path().append_float_rect(object.base().rect());
        path.base_mut().set_rect(object.base().rect());
        driver.save_state();
        let result = render_single_object(
            &path,
            driver,
            object_to_device,
            options,
            page_data,
            depth,
            None,
            &[],
            None,
        );
        driver.restore_state(false);
        return result;
    }
    let positions = with_render_font_mapper(|mapper| {
        font.get_char_pos_list(
            object.char_codes(),
            object.char_positions(),
            font_size,
            mapper,
        )
    });
    for position in positions {
        let Some(font) = font.font_at(position.fallback_font_position) else {
            continue;
        };
        let Some(glyph_path) = font.load_glyph_path(position.glyph_index, position.font_char_width)
        else {
            continue;
        };
        let mut path = CpdfPathObject::default();
        *path.base_mut().mutable_graph_state() = object.base().graph_state().clone();
        *path.base_mut().mutable_color_state() = object.base().color_state().clone();
        // Reconstructed glyph geometry retains text opacity/transfer state.
        *path.base_mut().mutable_general_state() = object.base().general_state().clone();
        let mut matrix = position.get_effective_matrix(CfxMatrix::new(
            font_size,
            0.0,
            0.0,
            font_size,
            position.origin.x,
            position.origin.y,
        ));
        matrix.concat(text_matrix);
        path.set_stroke(stroke);
        path.set_filltype(if fill {
            FillType::Winding
        } else {
            FillType::NoFill
        });
        path.mutable_path().append(&glyph_path, Some(&matrix));
        path.set_path_matrix(CfxMatrix::default());
        if !process_path(
            &path,
            driver,
            object_to_device,
            options,
            page_data,
            depth,
            None,
        ) {
            return false;
        }
    }
    true
}

fn draw_pattern_image(
    object: &CpdfImageObject,
    source: Rc<crate::fpdfapi::page::cpdf_image::CpdfImageBitmap>,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    image_matrix: CfxMatrix,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
    depth: usize,
    resample: crate::fxge::dib::fx_dib::FxdibResampleOptions,
    matte_color: u32,
) {
    let Some(mut rect) = get_unit_rect(image_matrix) else {
        return;
    };
    rect.intersect(driver.clip_box());
    if rect.is_empty() {
        return;
    }
    let Some(mut pixels) = CfxDibitmap::create(rect.width(), rect.height(), FxdibFormat::Bgra, 0)
    else {
        return;
    };
    let mut pattern_matrix = object_to_device;
    pattern_matrix.translate(-(rect.left as f32), -(rect.top as f32));
    {
        let mut target = CfxAggDeviceDriver::new(&mut pixels, false, None);
        let stopped = Cell::new(false);
        let mut bitmap_resources = RenderResources::new(page_data.context, &stopped);
        bitmap_resources.std_cs = true;
        render_pattern_paint(
            object,
            &mut target,
            pattern_matrix,
            options,
            bitmap_resources,
            depth,
            false,
        );
    }
    let mut local_matrix = image_matrix;
    local_matrix.translate(-(rect.left as f32), -(rect.top as f32));
    let Some(mask) = calculate_draw_image(&mut pixels, source, local_matrix, resample, matte_color)
    else {
        return;
    };
    pixels.multiply_alpha_mask(&mask);
    driver.set_dibits_normal(
        &pixels,
        0,
        FxRect::new(0, 0, pixels.width(), pixels.height()),
        rect.left,
        rect.top,
    );
}

fn render_simple_image(
    object: &CpdfImageObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
    resources: Option<&crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    _inherited_color: Option<u32>,
    depth: usize,
    require_image: bool,
) -> bool {
    let Some(image) = object.image() else {
        return !require_image;
    };
    if !options.check_ocg_dict_visible(image.oc().as_ref()) {
        return true;
    }
    let matrix = object.matrix() * object_to_device;
    if get_unit_rect(matrix).is_none() {
        return !require_image;
    }
    let mut loader = CpdfImageLoader::new();
    let pending = loader.start_with_context(
        object,
        Some(page_data.image_cache),
        crate::fpdfapi::page::cpdf_image::CpdfImageLoadContext {
            page_data: page_data.data,
            form_resources: resources,
            page_resources: page_data.page_resources,
            std_cs: page_data.std_cs,
            group_family: page_data.group_family,
            load_mask: page_data.load_mask,
            max_size: image_decode_size(matrix),
        },
    );
    if pending {
        while loader.continue_load(None) {}
    }
    if loader.bitmap().is_none() {
        return !require_image;
    }
    if options.color_mode_is(RenderColorMode::Alpha) && loader.mask().is_none() {
        if let Some(source) = loader.bitmap_ref() {
            start_bitmap_alpha(
                driver,
                source,
                object.base().general_state().get_fill_alpha(),
                matrix,
            );
        }
        return true;
    }
    if let Some(transfer) = object_transfer_function(object, page_data) {
        if !transfer.identity() {
            let _ = loader.translate_image(transfer);
        }
    }
    let Some(mut source) = loader.bitmap_ref() else {
        return true;
    };
    let is_mask = source.is_mask_format();
    if !is_mask && options.color_mode_is(RenderColorMode::Gray) {
        let Some(mut gray) = source.realize() else {
            return true;
        };
        gray.convert_color_scale(true);
        source = Rc::new(crate::fpdfapi::page::cpdf_image::CpdfImageBitmap::Bitmap(
            gray,
        ));
    }
    let mask_color = if is_mask {
        object_color(object, true, options, page_data)
    } else {
        0
    };
    let resample = crate::fxge::dib::fx_dib::FxdibResampleOptions {
        halftone: options.options().force_halftone,
        no_smoothing: options.options().no_image_smooth,
        interpolate_bilinear: !options.options().no_image_smooth && image.is_interpol(),
        ..Default::default()
    };
    let alpha = object.base().general_state().get_fill_alpha();
    if let Some(mask) = loader.mask_ref() {
        draw_masked_image(
            driver,
            source,
            mask,
            matrix,
            resample,
            loader.matte_color(),
            alpha,
        );
    } else if is_mask
        && object
            .base()
            .color_state()
            .get_fill_color()
            .is_some_and(|color| color.is_pattern() && color.get_pattern().is_some())
    {
        draw_pattern_image(
            object,
            source,
            driver,
            matrix,
            object_to_device,
            options,
            page_data,
            depth,
            resample,
            loader.matte_color(),
        );
    } else {
        let blend_type = image_blend_type(object, page_data.data, page_data.page_resources);
        // A stencil's mask color already contains its fill opacity. Applying
        // it again as the bitmap opacity would square ca.
        let bitmap_alpha = if is_mask { 1.0 } else { alpha };
        draw_dib_base_with_blend(
            driver,
            source,
            bitmap_alpha,
            mask_color,
            matrix,
            resample,
            blend_type,
        );
    }
    true
}

fn render_function_shading(
    object: &CpdfShadingObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
) -> bool {
    let mut rect = object.base().get_transformed_bbox(object_to_device);
    rect.intersect(driver.clip_box());
    if !rect.is_empty() {
        super::cpdf_rendershading::draw_shading(
            driver,
            object.pattern(),
            object.matrix() * object_to_device,
            rect,
            (object.base().general_state().get_fill_alpha() * 255.0).round() as i32,
            options,
        );
    }
    true
}

// Rendering mutates the per-object COW state to retain the document cache's
// weak transfer-function result, just as GetFillArgb/StartRenderDIBBase do.
fn object_transfer_function(
    object: &dyn PageObject,
    page_data: RenderResources<'_>,
) -> Option<Rc<crate::fpdfapi::page::cpdf_transferfunc::CpdfTransferFunc>> {
    let state = object.base().general_state();
    let function = state.get_tr()?;
    if let Some(transfer) = state.get_transfer_func() {
        return Some(transfer);
    }
    let transfer = page_data.render_data().get_transfer_func(&function);
    state.set_transfer_func(transfer.clone());
    transfer
}

pub(super) fn object_color(
    object: &dyn PageObject,
    fill: bool,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
) -> u32 {
    let colors = object.base().color_state();
    if let Some(color) = page_data.type3_fill_color {
        if !page_data.type3_colored
            || (fill && !colors.has_fill_color())
            || (!fill && !colors.has_stroke_color())
        {
            return color;
        }
    }
    object_color_for_type3(object, fill, options, page_data)
}

fn object_color_for_type3(
    object: &dyn PageObject,
    fill: bool,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
) -> u32 {
    let state = object.base().color_state();
    let mut color_ref = if fill {
        if state.has_fill_color() {
            state.get_fill_color_ref()
        } else {
            page_data
                .initial_fill_color
                .map_or(0, |colors| colors.get_fill_color_ref())
        }
    } else if state.has_stroke_color() {
        state.get_stroke_color_ref()
    } else {
        page_data
            .initial_stroke_color
            .map_or(0, |colors| colors.get_stroke_color_ref())
    };
    if color_ref == u32::MAX {
        return 0;
    }
    let alpha = if fill {
        object.base().general_state().get_fill_alpha()
    } else {
        object.base().general_state().get_stroke_alpha()
    };
    if let Some(transfer) = object_transfer_function(object, page_data) {
        color_ref = transfer.translate_color(color_ref);
    }
    let argb = alpha_and_color_ref_to_argb((alpha * 255.0) as i32, color_ref);
    if fill {
        options.translate_object_fill_color(argb, object.object_type())
    } else {
        options.translate_object_stroke_color(argb, object.object_type())
    }
}

fn clip_pattern(
    object: &dyn PageObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    stroke: bool,
    options: &CpdfRenderOptions,
) -> bool {
    if let Some(path) = object.as_any().downcast_ref::<CpdfPathObject>() {
        let Some(geometry) = path.path().get_object() else {
            return false;
        };
        let matrix = path.matrix() * object_to_device;
        if stroke {
            return path
                .base()
                .graph_state()
                .get_object()
                .is_some_and(|graph| driver.set_clip_path_stroke(geometry, Some(matrix), graph));
        }
        let mut fill_options = CfxFillRenderOptions::new(path.filltype());
        fill_options.aliased_path = options.options().no_path_smooth;
        return driver.set_clip_path_fill(geometry, Some(matrix), fill_options);
    }
    if object.as_any().is::<CpdfImageObject>() {
        return driver.set_clip_rect(object.base().get_transformed_bbox(object_to_device));
    }
    false
}

fn render_pattern_paint(
    object: &dyn PageObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
    depth: usize,
    stroke: bool,
) -> Option<bool> {
    let color = if stroke {
        object.base().color_state().get_stroke_color()?
    } else {
        object.base().color_state().get_fill_color()?
    };
    if !color.is_pattern() {
        return None;
    }
    let pattern = color.get_pattern()?;
    if matches!(pattern.kind(), PatternKind::Shading { .. }) {
        let shading = CpdfShadingPattern::retained(Rc::clone(&pattern))?;
        if !shading.load(&page_data) {
            return Some(true);
        }
        let mut shading_object = CpdfShadingObject::new(0, shading, pattern.pattern_to_form());
        shading_object.base_mut().set_rect(object.base().rect());
        *shading_object.base_mut().mutable_general_state() = object.base().general_state().clone();
        if stroke {
            shading_object
                .base_mut()
                .mutable_general_state()
                .set_fill_alpha(object.base().general_state().get_stroke_alpha());
        }
        driver.save_state();
        let clipped = clip_pattern(object, driver, object_to_device, stroke, options);
        let success =
            clipped && render_function_shading(&shading_object, driver, object_to_device, options);
        driver.restore_state(false);
        return Some(success);
    }
    if !matches!(pattern.kind(), PatternKind::Tiling { .. }) {
        return None;
    }
    let mut tiling =
        CpdfTilingPattern::new(pattern.pattern_object().clone(), pattern.parent_matrix())?;
    let form = tiling.load_with_fill_alpha(
        object,
        Rc::clone(page_data.data),
        stroke.then(|| object.base().general_state().get_stroke_alpha()),
    )?;
    driver.save_state();
    if clip_pattern(object, driver, object_to_device, stroke, options) {
        let clip = driver.clip_box();
        if !clip.is_empty() {
            if let Some(screen) = super::cpdf_rendertiling::draw(
                driver,
                object,
                &tiling,
                &form,
                object_to_device,
                clip,
                stroke,
                options,
                page_data,
                depth,
            ) {
                driver.set_dibits_normal(
                    &screen,
                    0,
                    FxRect::new(0, 0, screen.width(), screen.height()),
                    clip.left,
                    clip.top,
                );
            }
        }
    }
    driver.restore_state(false);
    Some(true)
}

// CPDF_RenderStatus::last_clip_path_ reuses consecutive shared clip paths.
// Scope this to one holder: its base device clip and object-to-device matrix
// stay fixed. Keep a shallow region snapshot because this port restores the
// holder's base state after each object instead of retaining its applied clip.
#[derive(Default)]
struct ClipPathCache {
    path: crate::fpdfapi::page::cpdf_clippath::CpdfClipPath,
    region: Option<CfxAggClipRgn>,
}

impl ClipPathCache {
    fn clear(&mut self) {
        self.path.set_null();
        self.region = None;
    }

    fn restore(
        &self,
        clip: &crate::fpdfapi::page::cpdf_clippath::CpdfClipPath,
        driver: &mut CfxAggDeviceDriver<'_, '_>,
    ) -> bool {
        if !clip.has_ref() || clip != &self.path {
            return false;
        }
        driver.restore_clip_region(self.region.clone());
        true
    }

    fn record(
        &mut self,
        clip: &crate::fpdfapi::page::cpdf_clippath::CpdfClipPath,
        driver: &CfxAggDeviceDriver<'_, '_>,
        success: bool,
    ) {
        if success && clip.has_ref() {
            self.path = clip.clone();
            self.region = driver.clip_region_snapshot();
        } else {
            // Preserve the port's retries after a device clip failure.
            self.clear();
        }
    }
}

fn process_clip_path(
    clip: &crate::fpdfapi::page::cpdf_clippath::CpdfClipPath,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
    page_resources: Option<&crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    depth: usize,
    active_type3_fonts: &[usize],
) -> bool {
    // Apply a changed clip against the holder's saved base state.
    if !clip.has_ref() {
        return true;
    }
    for index in 0..clip.path_count() {
        let path = clip.path(index);
        let Some(path) = path.get_object() else {
            continue;
        };
        let success = if path.points().is_empty() {
            let mut empty_path = CfxPath::new();
            empty_path.append_rect(-1.0, -1.0, 0.0, 0.0);
            driver.set_clip_path_fill(&empty_path, None, CfxFillRenderOptions::winding_options())
        } else {
            driver.set_clip_path_fill(
                path,
                Some(object_to_device),
                CfxFillRenderOptions::new(clip.clip_type(index)),
            )
        };
        if !success {
            return false;
        }
    }
    if clip.text_count() == 0 || !driver.render_cap_soft_clip() {
        return true;
    }
    let mut clipping_path = None;
    for index in 0..clip.text_count() {
        if let Some(text) = clip.text(index) {
            let path = clipping_path.get_or_insert_with(CfxPath::new);
            render_simple_text(
                text,
                driver,
                object_to_device,
                options,
                page_data,
                page_resources,
                depth,
                Some(path),
                active_type3_fonts,
            );
        } else if let Some(path) = clipping_path.take() {
            let mut fill_options = CfxFillRenderOptions::winding_options();
            fill_options.aliased_path = options.options().no_text_smooth;
            if !driver.set_clip_path_fill(&path, None, fill_options) {
                return false;
            }
        }
    }
    true
}

fn process_path(
    path: &CpdfPathObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
    depth: usize,
    _inherited_color: Option<u32>,
) -> bool {
    let Some(geometry) = path.path().get_object() else {
        return true;
    };
    let mut fill_type = path.filltype();
    let mut stroke = path.stroke();
    // ProcessPathPattern consumes each patterned paint before normal path
    // drawing, including a pattern that failed to load.
    if fill_type != FillType::NoFill
        && path
            .base()
            .color_state()
            .get_fill_color()
            .is_some_and(|color| color.is_pattern())
    {
        render_pattern_paint(
            path,
            driver,
            object_to_device,
            options,
            page_data,
            depth,
            false,
        );
        fill_type = FillType::NoFill;
    }
    if stroke
        && path
            .base()
            .color_state()
            .get_stroke_color()
            .is_some_and(|color| color.is_pattern())
    {
        render_pattern_paint(
            path,
            driver,
            object_to_device,
            options,
            page_data,
            depth,
            true,
        );
        stroke = false;
    }
    if fill_type == FillType::NoFill && !stroke {
        return true;
    }
    if options.color_mode_is(RenderColorMode::ForcedColor)
        && options.options().convert_fill_to_stroke
        && fill_type != FillType::NoFill
    {
        stroke = true;
        fill_type = FillType::NoFill;
    }
    let fill = if fill_type != FillType::NoFill {
        object_color(path, true, options, page_data)
    } else {
        0
    };
    let stroke_color = if stroke {
        object_color(path, false, options, page_data)
    } else {
        0
    };
    let matrix = path.matrix() * object_to_device;
    if !is_available_matrix(matrix) {
        return true;
    }
    let mut fill_options = CfxFillRenderOptions::new(fill_type);
    fill_options.rect_aa = fill_type != FillType::NoFill && options.options().rect_aa;
    fill_options.aliased_path = options.options().no_path_smooth;
    fill_options.adjust_stroke = path.base().general_state().get_stroke_adjust();
    fill_options.stroke = stroke;
    fill_options.text_mode = page_data.is_type3_char;
    crate::fxge::cfx_renderdevice::draw_path_agg(
        driver,
        geometry,
        Some(matrix),
        path.base().graph_state().get_object(),
        fill,
        stroke_color,
        fill_options,
    )
}

/// Culling must include raster coverage outside a geometric object bound.
/// AGG widens subpixel strokes to a device pixel and antialiases glyph edges.
/// Expanding only the culling rectangle preserves the actual device clip and
/// avoids allocating/copying a guard-band bitmap for each region.
#[derive(Clone, Copy, Default)]
pub(super) struct CullingRects {
    geometric: crate::fxcrt::fx_coordinates::CfxFloatRect,
    fringe: crate::fxcrt::fx_coordinates::CfxFloatRect,
}
pub(super) fn culling_rect(matrix: CfxMatrix, clip: FxRect) -> CullingRects {
    let inverse = matrix.get_inverse();
    let geometric = crate::fxcrt::fx_coordinates::CfxFloatRect::from_fx_rect(clip);
    let mut fringe = geometric;
    fringe.inflate_xy(2.0, 2.0);
    CullingRects {
        geometric: inverse.transform_float_rect(geometric),
        fringe: inverse.transform_float_rect(fringe),
    }
}
fn expanded_culling_rect(
    matrix: CfxMatrix,
    clip: FxRect,
    margin: f32,
) -> crate::fxcrt::fx_coordinates::CfxFloatRect {
    let mut clip = crate::fxcrt::fx_coordinates::CfxFloatRect::from_fx_rect(clip);
    clip.left -= margin;
    clip.bottom -= margin;
    clip.right += margin;
    clip.top += margin;
    matrix.get_inverse().transform_float_rect(clip)
}
pub(super) fn object_intersects_culling_rect(
    object: &dyn PageObject,
    matrix: CfxMatrix,
    device_clip: FxRect,
    clip: CullingRects,
    options: &CpdfRenderOptions,
) -> bool {
    let rect = object.base().rect();
    let intersects = |clip: crate::fxcrt::fx_coordinates::CfxFloatRect| {
        rect.left <= clip.right
            && rect.right >= clip.left
            && rect.bottom <= clip.top
            && rect.top >= clip.bottom
    };
    if intersects(clip.fringe) {
        if !intersects(clip.geometric) && !options.options().convert_fill_to_stroke {
            if let Some(path) = object.as_any().downcast_ref::<CpdfPathObject>() {
                if !path.stroke() && !path.has_no_filltype() {
                    if let Some(geometry) = path.path().get_object() {
                        return crate::fxge::agg::cfx_agg_devicedriver::filled_bezier_path_intersects_clip(
                            geometry, path.matrix() * matrix, device_clip,
                        );
                    }
                }
            }
        }
        return true;
    }
    let stroked = object
        .as_any()
        .downcast_ref::<CpdfPathObject>()
        .is_some_and(|path| path.stroke())
        || object
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .is_some_and(|text| {
                crate::fpdfapi::page::cpdf_textstate::text_rendering_mode_is_stroke_mode(
                    text.text_render_mode(),
                )
            });
    if !stroked {
        return false;
    }
    // Widening a hairline also widens its miter. The AGG minimum width uses
    // the average transformed unit length; the longest axis is at most twice
    // that average, so miter_limit device pixels plus antialias fringe is a
    // conservative bound. Most objects use the cached two-pixel test above.
    let margin = object.base().graph_state().get_miter_limit().abs() + 1.0;
    !margin.is_finite() || intersects(expanded_culling_rect(matrix, device_clip, margin.max(2.0)))
}

pub(super) fn render_holder(
    holder: &CpdfPageObjectHolder,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    page_data: RenderResources<'_>,
    depth: usize,
    inherited_color: Option<u32>,
    active_type3_fonts: &[usize],
) -> bool {
    let clip_rect = culling_rect(object_to_device, driver.clip_box());
    let mut clip_cache = ClipPathCache::default();
    for object in holder.iter() {
        let object = object.as_ref();
        if page_data
            .stop_object
            .is_some_and(|stop| std::ptr::addr_eq(stop, object))
        {
            page_data.stopped.set(true);
            break;
        }
        if !object.base().is_active() {
            continue;
        }
        if !object_intersects_culling_rect(
            object,
            object_to_device,
            driver.clip_box(),
            clip_rect,
            options,
        ) {
            continue;
        }
        driver.save_state();
        let success = if page_data.progressive_root {
            // ContinueSingleObject does not increment the RenderSingleObject
            // recursion counter at this root level.
            process_single_object(
                object,
                driver,
                object_to_device,
                options,
                page_data,
                depth,
                inherited_color,
                active_type3_fonts,
                Some(&mut clip_cache),
            )
        } else {
            render_single_object(
                object,
                driver,
                object_to_device,
                options,
                page_data,
                depth,
                inherited_color,
                active_type3_fonts,
                Some(&mut clip_cache),
            )
        };
        driver.restore_state(false);
        if !success {
            return false;
        }
        if page_data.progressive_root
            && object.as_any().is::<CpdfImageObject>()
            && options.options().limited_image_cache
        {
            page_data
                .image_cache
                .cache_optimization(options.cache_size_limit() as i32);
        }
        if page_data.stopped.get() {
            break;
        }
    }
    true
}

// GetBackgroundColor(): BC is interpreted through the group's color space,
// without resource lookup. Unsupported spaces leave both black and Unknown.
fn soft_mask_background(
    background: Option<&crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    group_stream: &crate::fpdfapi::parser::cpdf_object::ObjectRef,
    page_data: &CpdfDocPageData,
) -> (u32, crate::fpdfapi::page::cpdf_colorspace::ColorSpaceFamily) {
    use crate::fpdfapi::page::cpdf_colorspace::ColorSpaceFamily;
    use crate::fxge::dib::fx_dib::argb_encode;
    let default = (0xff000000, ColorSpaceFamily::Unknown);
    let Some(background) = background else {
        return default;
    };
    let group = {
        let borrowed = group_stream.borrow();
        let Some(stream) = borrowed.as_any().downcast_ref::<CpdfStream>() else {
            return default;
        };
        let dict = stream.get_dict();
        let borrowed = dict.borrow();
        let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
            return default;
        };
        dict.get_dict_for(b"Group")
    };
    let space = group
        .and_then(|group| {
            group
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_direct_object_for(b"CS")
        })
        .and_then(|object| page_data.get_color_space(&object, None));
    let Some(space) = space else { return default };
    let family = space.family();
    if family == ColorSpaceFamily::Lab
        || space.is_special()
        || (family == ColorSpaceFamily::IccBased && !space.is_normal())
    {
        return default;
    }
    let mut values = vec![0.0; 8.max(space.component_count() as usize)];
    {
        let borrowed = background.borrow();
        let Some(background) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
            return default;
        };
        for (i, value) in values.iter_mut().take(8.min(background.size())).enumerate() {
            *value = background.get_float_at(i);
        }
    }
    let rgb = space.get_rgb_or_zeros_on_error(&values);
    (
        argb_encode(
            255,
            (rgb.red * 255.0) as i32 as u32,
            (rgb.green * 255.0) as i32 as u32,
            (rgb.blue * 255.0) as i32 as u32,
        ),
        family,
    )
}

fn load_object_soft_mask(
    object: &dyn PageObject,
    clip_rect: FxRect,
    object_to_device: CfxMatrix,
    page_data: RenderResources<'_>,
    depth: usize,
) -> Option<CfxDibitmap> {
    let state = object.base().general_state();
    let mask_object = effective_soft_mask(object)?;
    let (group, luminosity, function, background) = {
        let borrowed = mask_object.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        (
            dict.get_stream_for(b"G")?,
            dict.get_byte_string_for(b"S", b"") != b"Alpha",
            dict.get_direct_object_for(b"TR"),
            dict.get_array_for(b"BC"),
        )
    };
    let transfer = function.and_then(|object| {
        let is_function = {
            let borrowed = object.borrow();
            borrowed.as_any().is::<CpdfDictionary>() || borrowed.as_any().is::<CpdfStream>()
        };
        is_function.then(|| CpdfFunction::load(&object)).flatten()
    });
    let mut form = CpdfForm::new(group.clone(), page_data.page_resources.cloned(), None)?;
    form.parse_content(
        None,
        None,
        Rc::clone(page_data.data),
        Rc::new(RefCell::new(HashSet::new())),
        0,
    );
    let (width, height) = (clip_rect.width(), clip_rect.height());
    // PDFium uses an alpha-only surface for Alpha masks. Luminosity uses an
    // opaque BGR surface (BGRX on Apple), so paint alpha blends with BC first.
    let format = if !luminosity {
        FxdibFormat::EightBppMask
    } else if cfg!(target_vendor = "apple") {
        FxdibFormat::Bgrx
    } else {
        FxdibFormat::Bgr
    };
    let mut pixels = CfxDibitmap::create(width, height, format, 0)?;
    let (background_color, family) = if luminosity {
        soft_mask_background(background.as_ref(), &group, page_data.data)
    } else {
        (
            0,
            crate::fpdfapi::page::cpdf_colorspace::ColorSpaceFamily::Unknown,
        )
    };
    pixels.clear(background_color);
    let mut matrix = state.get_smask_matrix().unwrap_or_default() * object_to_device;
    matrix.translate(-(clip_rect.left as f32), -(clip_rect.top as f32));
    {
        let mut mask_driver = CfxAggDeviceDriver::new(&mut pixels, false, None);
        let stopped = Cell::new(false);
        let mut resources = RenderResources::new(page_data.context, &stopped);
        let form_scope = form_resources(&form);
        resources.form_resources = form_scope.as_ref();
        resources.std_cs = true;
        resources.group_family = family;
        resources.load_mask = luminosity;
        let mut options = CpdfRenderOptions::new();
        options.set_color_mode(if luminosity {
            RenderColorMode::Normal
        } else {
            RenderColorMode::Alpha
        });
        if !render_holder(
            form.holder(),
            &mut mask_driver,
            matrix,
            &options,
            resources,
            depth + 1,
            None,
            &[],
        ) {
            return None;
        }
    }
    let mut mask = CfxDibitmap::create(width, height, FxdibFormat::EightBppMask, 0)?;
    let mut transfers = std::array::from_fn::<_, 256, _>(|i| i as u8);
    if let Some(function) = &transfer {
        let mut results = vec![0.0; function.output_count() as usize];
        for (i, value) in transfers.iter_mut().enumerate() {
            // Native ignores Call's result and retains the previous outputs on
            // failure. OutputCount is nonzero for a successfully loaded function.
            function.call(&[i as f32 / 255.0], &mut results);
            *value = (results[0] * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    if luminosity {
        let step = crate::fxge::dib::fx_dib::get_bpp_from_format(format) as usize / 8;
        for y in 0..height {
            let source = pixels.scanline(y)?;
            let row = mask.scanline_mut(y)?;
            for (pixel, value) in source.chunks_exact(step).take(width as usize).zip(row) {
                *value = transfers[rgb_to_gray(
                    i32::from(pixel[2]),
                    i32::from(pixel[1]),
                    i32::from(pixel[0]),
                ) as usize];
            }
        }
    } else {
        // Alpha masks have identical row strides. Native also transfers padding.
        for (&source, dest) in pixels.buffer().iter().zip(mask.buffer_mut()) {
            *dest = transfers[source as usize];
        }
    }
    Some(mask)
}

fn render_single_object(
    object: &dyn PageObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    matrix: CfxMatrix,
    options: &CpdfRenderOptions,
    resources: RenderResources<'_>,
    depth: usize,
    inherited_color: Option<u32>,
    active_type3_fonts: &[usize],
    clip_cache: Option<&mut ClipPathCache>,
) -> bool {
    let Some(_recursion) = RenderRecursionGuard::enter() else {
        return true;
    };
    process_single_object(
        object,
        driver,
        matrix,
        options,
        resources,
        depth,
        inherited_color,
        active_type3_fonts,
        clip_cache,
    )
}

fn process_single_object(
    object: &dyn PageObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    matrix: CfxMatrix,
    options: &CpdfRenderOptions,
    resources: RenderResources<'_>,
    depth: usize,
    inherited_color: Option<u32>,
    active_type3_fonts: &[usize],
    mut clip_cache: Option<&mut ClipPathCache>,
) -> bool {
    if !options.check_page_object_visible(object) {
        return true;
    }
    let clip = object.base().clip_path();
    if !clip_cache
        .as_ref()
        .is_some_and(|cache| cache.restore(clip, driver))
    {
        if let Some(cache) = clip_cache.as_deref_mut() {
            // Release the old retained mask before building a different one,
            // as native RestoreState does on a clip change.
            cache.clear();
        }
        // Native ProcessClipPath has no failure result. Device clip failures do
        // not propagate as a failure of the whole page-render call.
        let success = process_clip_path(
            clip,
            driver,
            matrix,
            options,
            resources,
            resources.page_resources,
            depth,
            active_type3_fonts,
        );
        if let Some(cache) = clip_cache {
            cache.record(clip, driver, success);
        }
    }
    process_transparency(
        object,
        driver,
        matrix,
        options,
        resources,
        depth,
        inherited_color,
        active_type3_fonts,
    )
    .unwrap_or_else(|| {
        process_object_no_clip(
            object,
            driver,
            matrix,
            options,
            resources,
            depth,
            inherited_color,
            active_type3_fonts,
        )
    })
}

pub(super) fn form_resources(
    form: &CpdfForm,
) -> Option<crate::fpdfapi::parser::cpdf_object::ObjectRef> {
    form.holder()
        .dict()
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_dict_for(b"Resources")
}

fn process_object_no_clip(
    object: &dyn PageObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    matrix: CfxMatrix,
    options: &CpdfRenderOptions,
    resources: RenderResources<'_>,
    depth: usize,
    inherited_color: Option<u32>,
    active_type3_fonts: &[usize],
) -> bool {
    if !process_object_no_clip_impl(
        object,
        driver,
        matrix,
        options,
        resources,
        depth,
        inherited_color,
        active_type3_fonts,
    ) {
        draw_object_with_background(object, driver, matrix, options, resources, depth);
    }
    true
}

fn draw_object_with_background(
    object: &dyn PageObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    matrix: CfxMatrix,
    options: &CpdfRenderOptions,
    resources: RenderResources<'_>,
    depth: usize,
) {
    let mut rect = object.base().get_transformed_bbox(matrix);
    rect.intersect(driver.clip_box());
    if rect.is_empty() {
        return;
    }
    // AGG advertises readback; the scaled printer buffer is Windows-only.
    debug_assert!(driver.render_cap_get_bits());
    let stopped = Cell::new(false);
    let mut child = RenderResources::new(resources.context, &stopped);
    child.transparency = resources.transparency;
    child.in_group = resources.in_group;
    let form_scope = object
        .as_any()
        .downcast_ref::<CpdfFormObject>()
        .and_then(|form| form_resources(form.form()));
    child.form_resources = form_scope.as_ref();
    render_single_object(
        object,
        driver,
        matrix,
        options,
        child,
        depth + 1,
        None,
        &[],
        None,
    );
}

fn process_object_no_clip_impl(
    object: &dyn PageObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    resources: RenderResources<'_>,
    depth: usize,
    inherited_color: Option<u32>,
    active_type3_fonts: &[usize],
) -> bool {
    if let Some(path) = object.as_any().downcast_ref::<CpdfPathObject>() {
        process_path(
            path,
            driver,
            object_to_device,
            options,
            resources,
            depth,
            inherited_color,
        )
    } else if let Some(form) = object.as_any().downcast_ref::<CpdfFormObject>() {
        let visible = {
            let borrowed = form.form().holder().dict().borrow();
            let dict = borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .expect("form dictionary");
            options.check_ocg_dict_visible(dict.get_dict_for(b"OC").as_ref())
        };
        if !visible {
            return true;
        }
        // ProcessForm creates a fresh status. It inherits transparency and
        // in-group state, but not standard-CS/mask decoding or device matrix.
        let mut child = RenderResources::new(resources.context, resources.stopped);
        child.stop_object = resources.stop_object;
        child.transparency = resources.transparency;
        child.in_group = resources.in_group;
        let form_scope = form_resources(form.form());
        child.form_resources = form_scope.as_ref();
        child.initial_fill_alpha = form.base().general_state().get_fill_alpha();
        let colors = form.base().color_state();
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
        render_holder(
            form.form().holder(),
            driver,
            form.form_matrix() * object_to_device,
            options,
            child,
            depth + 1,
            inherited_color,
            &[],
        )
    } else if let Some(text) = object.as_any().downcast_ref::<CpdfTextObject>() {
        render_simple_text(
            text,
            driver,
            object_to_device,
            options,
            resources,
            resources.page_resources,
            depth,
            None,
            active_type3_fonts,
        )
    } else if let Some(image) = object.as_any().downcast_ref::<CpdfImageObject>() {
        render_simple_image(
            image,
            driver,
            object_to_device,
            options,
            resources,
            resources.form_resources,
            inherited_color,
            depth,
            false,
        )
    } else if let Some(shading) = object.as_any().downcast_ref::<CpdfShadingObject>() {
        render_function_shading(shading, driver, object_to_device, options)
    } else {
        true
    }
}

fn process_transparency(
    object: &dyn PageObject,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    resources: RenderResources<'_>,
    depth: usize,
    inherited_color: Option<u32>,
    active_type3_fonts: &[usize],
) -> Option<bool> {
    use crate::fxge::dib::fx_dib::BlendMode;
    let state = object.base().general_state();
    let blend_type = state.get_blend_type();
    let has_mask = effective_soft_mask(object).is_some();
    let form = object.as_any().downcast_ref::<CpdfFormObject>();
    let (group_alpha, initial_alpha, transparency, form_resources) = if let Some(form) = form {
        (
            state.get_fill_alpha(),
            resources.initial_fill_alpha,
            form.form().holder().transparency(),
            form_resources(form.form()),
        )
    } else {
        (1.0, 1.0, resources.transparency, None)
    };
    // The selected AGG device advertises soft clipping, so text clips do not
    // require the alternate mask surface in ProcessTransparency.
    if !has_mask
        && group_alpha == 1.0
        && blend_type == BlendMode::Normal
        && !(form.is_some() && transparency.is_isolated())
        && initial_alpha == 1.0
    {
        return None;
    }
    let mut rect = object.base().get_transformed_bbox(object_to_device);
    rect.intersect(driver.clip_box());
    if rect.is_empty() {
        return Some(true);
    }
    let (width, height) = (rect.width(), rect.height());
    let backdrop = if !transparency.is_isolated() && driver.render_cap_get_bits() {
        let Some(mut backdrop) = driver.create_compatible_bitmap(width, height) else {
            return Some(true);
        };
        driver.get_dibits(&mut backdrop, rect.left, rect.top);
        Some(backdrop)
    } else {
        None
    };
    let Some(mut pixels) = CfxDibitmap::create(width, height, FxdibFormat::Bgra, 0) else {
        return Some(true);
    };
    let mut local_matrix = object_to_device;
    local_matrix.translate(-(rect.left as f32), -(rect.top as f32));
    {
        let mut target = CfxAggDeviceDriver::new(&mut pixels, false, backdrop.as_ref());
        let mut child = RenderResources::new(resources.context, resources.stopped);
        child.std_cs = true;
        child.form_resources = form_resources.as_ref();
        child.in_group = transparency.is_group();
        child.stop_object = resources.stop_object;
        if !process_object_no_clip(
            object,
            &mut target,
            local_matrix,
            options,
            child,
            depth,
            inherited_color,
            active_type3_fonts,
        ) {
            return Some(false);
        }
    }
    if has_mask {
        if let Some(mask) = load_object_soft_mask(object, rect, object_to_device, resources, depth)
        {
            pixels.multiply_alpha_mask(&mask);
        }
    }
    if transparency.is_group() {
        pixels.multiply_alpha(group_alpha);
    }
    if initial_alpha != 1.0 && !resources.in_group {
        pixels.multiply_alpha(initial_alpha);
    }
    let mut output_transparency = resources.transparency;
    if form.is_some() {
        output_transparency.set_group();
    }
    composite_dibitmap(
        &pixels,
        driver,
        object,
        rect.left,
        rect.top,
        blend_type,
        output_transparency,
        options,
        resources,
    );
    Some(true)
}

// Static, color-bitmap branches of CompositeDIBitmap. The mask-bitmap branch
// exists only under Windows; callers here always supply the BGRA group plane.
fn composite_dibitmap(
    bitmap: &CfxDibitmap,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    object: &dyn PageObject,
    mut left: i32,
    mut top: i32,
    blend_mode: crate::fxge::dib::fx_dib::BlendMode,
    transparency: crate::fpdfapi::page::cpdf_transparency::CpdfTransparency,
    options: &CpdfRenderOptions,
    resources: RenderResources<'_>,
) {
    use crate::fxge::dib::fx_dib::BlendMode;
    let source_rect = FxRect::new(0, 0, bitmap.width(), bitmap.height());
    if blend_mode == BlendMode::Normal
        && driver.set_dibits_normal(bitmap, 0, source_rect, left, top)
    {
        return;
    }
    let isolated = transparency.is_isolated();
    let back_alpha_required = blend_mode != BlendMode::Normal && isolated;
    let get_background =
        driver.render_cap_alpha_output() || (driver.render_cap_get_bits() && !back_alpha_required);
    if get_background {
        if isolated || !transparency.is_group() {
            driver.set_dibits(bitmap, 0, source_rect, left, top, blend_mode);
            return;
        }
        let mut rect = FxRect::new(left, top, left + bitmap.width(), top + bitmap.height());
        rect.intersect(driver.clip_box());
        if let Some(backdrop) = driver.backdrop() {
            let Some(mut clone) = backdrop.clip_to(rect) else {
                return;
            };
            clone.composite_bitmap(
                0,
                0,
                clone.width(),
                clone.height(),
                driver.bitmap(),
                rect.left,
                rect.top,
                BlendMode::Normal,
            );
            // Pinned source uses min here, including negative source offsets.
            left = left.min(0);
            top = top.min(0);
            clone.composite_bitmap(
                0,
                0,
                clone.width(),
                clone.height(),
                bitmap,
                left,
                top,
                blend_mode,
            );
            driver.set_dibits_normal(
                &clone,
                0,
                FxRect::new(0, 0, clone.width(), clone.height()),
                rect.left,
                rect.top,
            );
        } else {
            driver.set_dibits(bitmap, 0, source_rect, rect.left, rect.top, blend_mode);
        }
        return;
    }
    let mut bbox = FxRect::new(left, top, left + bitmap.width(), top + bitmap.height());
    bbox.intersect(driver.clip_box());
    let Some(mut backdrop) = get_backdrop(
        object,
        bbox,
        back_alpha_required,
        driver,
        options,
        resources,
    ) else {
        return;
    };
    backdrop.composite_bitmap(
        left - bbox.left,
        top - bbox.top,
        bitmap.width(),
        bitmap.height(),
        bitmap,
        0,
        0,
        blend_mode,
    );
    let Some(mut opaque) =
        CfxDibitmap::create(backdrop.width(), backdrop.height(), FxdibFormat::Bgrx, 0)
    else {
        return;
    };
    opaque.clear(0xffffffff);
    opaque.composite_bitmap(
        0,
        0,
        opaque.width(),
        opaque.height(),
        &backdrop,
        0,
        0,
        BlendMode::Normal,
    );
    driver.set_dibits_normal(
        &opaque,
        0,
        FxRect::new(0, 0, opaque.width(), opaque.height()),
        bbox.left,
        bbox.top,
    );
}

fn get_backdrop(
    object: &dyn PageObject,
    bbox: FxRect,
    back_alpha_required: bool,
    driver: &CfxAggDeviceDriver<'_, '_>,
    options: &CpdfRenderOptions,
    resources: RenderResources<'_>,
) -> Option<CfxDibitmap> {
    let mut backdrop = if back_alpha_required {
        CfxDibitmap::create(bbox.width(), bbox.height(), FxdibFormat::Bgra, 0)?
    } else {
        driver.create_compatible_bitmap(bbox.width(), bbox.height())?
    };
    let fetch = if backdrop.format() == FxdibFormat::Bgra {
        driver.render_cap_alpha_output()
    } else {
        driver.render_cap_get_bits()
    };
    if fetch {
        driver.get_dibits(&mut backdrop, bbox.left, bbox.top);
    } else {
        let mut matrix = resources.device_matrix;
        matrix.translate(-(bbox.left as f32), -(bbox.top as f32));
        if backdrop.format() != FxdibFormat::Bgra {
            backdrop.clear(0xffffffff);
        }
        let mut target = CfxAggDeviceDriver::new(&mut backdrop, false, None);
        resources
            .context
            .render(&mut target, Some(object), Some(options), Some(matrix));
    }
    Some(backdrop)
}

fn effective_soft_mask(
    object: &dyn PageObject,
) -> Option<crate::fpdfapi::parser::cpdf_object::ObjectRef> {
    let mask = object.base().general_state().get_soft_mask()?;
    if object
        .as_any()
        .downcast_ref::<CpdfImageObject>()
        .and_then(CpdfImageObject::image)
        .is_some_and(|image| {
            image
                .dict()
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .is_some_and(|dict| dict.key_exist(b"SMask"))
        })
    {
        return None;
    }
    Some(mask)
}

/// RenderContext::Render(): each borrowed layer gets a fresh status, saved
/// device state, stop cursor, and optional final device transform.
pub(crate) fn render_context_layers(
    context: &CpdfRenderContext<'_>,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    stop_object: Option<&dyn PageObject>,
    options: Option<&CpdfRenderOptions>,
    last_matrix: Option<CfxMatrix>,
) -> bool {
    render_layers(context, driver, stop_object, options, last_matrix, false)
}

pub(super) fn render_layers(
    context: &CpdfRenderContext<'_>,
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    stop_object: Option<&dyn PageObject>,
    options: Option<&CpdfRenderOptions>,
    last_matrix: Option<CfxMatrix>,
    progressive: bool,
) -> bool {
    let defaults = CpdfRenderOptions::new();
    let options = options.unwrap_or(&defaults);
    for layer in context.layers() {
        let stopped = Cell::new(false);
        let mut resources = RenderResources::new(context, &stopped);
        resources.stop_object = stop_object;
        resources.progressive_root = progressive;
        resources.transparency = layer.object_holder.transparency();
        resources.device_matrix = last_matrix.unwrap_or_default();
        let matrix = last_matrix.map_or(layer.matrix, |last| layer.matrix * last);
        driver.save_state();
        let success = render_holder(
            layer.object_holder,
            driver,
            matrix,
            options,
            resources,
            0,
            None,
            &[],
        );
        driver.restore_state(false);
        if !progressive && options.options().limited_image_cache {
            resources
                .image_cache
                .cache_optimization(options.cache_size_limit() as i32);
        }
        if !success {
            return false;
        }
        if stopped.get() {
            break;
        }
    }
    true
}

/// Image-only SDK rendering keeps the source's separate clip transform and
/// reports image decode failures, while ordinary page rendering skips them.
pub(crate) fn render_image_for_sdk(
    page: &CpdfPage,
    image: &CpdfImageObject,
    bitmap: &mut CfxDibitmapData<'_>,
    matrix: CfxMatrix,
    clip_matrix: CfxMatrix,
) -> bool {
    let mut driver = CfxAggDeviceDriver::new(bitmap, false, None);
    let context = CpdfRenderContext::new(page);
    let stopped = Cell::new(false);
    let cache = crate::fpdfapi::page::cpdf_pageimagecache::CpdfPageImageCache::with_page_data(
        page.page_data(),
    );
    let mut resources = RenderResources::new(&context, &stopped);
    resources.image_cache = &cache;
    let options = CpdfRenderOptions::new();
    process_clip_path(
        image.base().clip_path(),
        &mut driver,
        clip_matrix,
        &options,
        resources,
        resources.page_resources,
        0,
        &[],
    );
    render_simple_image(
        image,
        &mut driver,
        matrix,
        &options,
        resources,
        None,
        None,
        0,
        true,
    )
}

/// Render the currently translated page object types to an AGG bitmap.
pub fn render_page_objects(
    page: &CpdfPage,
    bitmap: &mut CfxDibitmapData<'_>,
    page_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    rgb_byte_order: bool,
) -> bool {
    render_page_objects_with_clip(page, bitmap, page_to_device, options, rgb_byte_order, None)
}

/// Limit drawing to the requested device rectangle while preserving pixels
/// outside it, as the bitmap render entry point requires.
pub fn render_page_objects_with_clip(
    page: &CpdfPage,
    bitmap: &mut CfxDibitmapData<'_>,
    page_to_device: CfxMatrix,
    options: &CpdfRenderOptions,
    rgb_byte_order: bool,
    clip: Option<FxRect>,
) -> bool {
    let mut driver = CfxAggDeviceDriver::new(bitmap, rgb_byte_order, None);
    if let Some(clip) = clip {
        let mut path = CfxPath::new();
        path.append_rect(
            clip.left as f32,
            clip.top as f32,
            clip.right as f32,
            clip.bottom as f32,
        );
        if !driver.set_clip_path_fill(&path, None, CfxFillRenderOptions::winding_options()) {
            return false;
        }
    }
    let mut annotations = options.draw_annots().then(|| CpdfAnnotList::new(page));
    let mut context = CpdfRenderContext::new(page);
    context.append_layer(page.holder(), page_to_device);
    if let Some(annotations) = &mut annotations {
        annotations.display_annots(&mut context, options.printing(), page_to_device);
    }
    super::cpdf_progressiverenderer::render_static(&context, &mut driver, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    use crate::fxge::cfx_face::CfxFace;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    #[test]
    fn fill_culling_rejects_empty_curve_fringe_but_keeps_strokes_and_forced_strokes() {
        use crate::fxcrt::fx_coordinates::CfxPointF;
        use crate::fxge::cfx_path::PointType;
        let mut path = CpdfPathObject::new(0);
        path.set_winding_filltype();
        for (x, y, kind) in [
            (14.0, 4.0, PointType::Move),
            (15.5, 3.0, PointType::Bezier),
            (15.5, 7.0, PointType::Bezier),
            (14.0, 4.0, PointType::Bezier),
        ] {
            path.mutable_path().append_point(CfxPointF::new(x, y), kind);
        }
        path.calc_bounding_box();
        let device_clip = FxRect::new(16, 0, 32, 16);
        let matrix = CfxMatrix::default();
        let bounds = culling_rect(matrix, device_clip);
        let mut options = CpdfRenderOptions::default();
        assert!(!object_intersects_culling_rect(
            &path,
            matrix,
            device_clip,
            bounds,
            &options
        ));
        path.set_stroke(true);
        assert!(object_intersects_culling_rect(
            &path,
            matrix,
            device_clip,
            bounds,
            &options
        ));
        path.set_stroke(false);
        options.options_mut().convert_fill_to_stroke = true;
        assert!(object_intersects_culling_rect(
            &path,
            matrix,
            device_clip,
            bounds,
            &options
        ));
        options.options_mut().convert_fill_to_stroke = false;
        path.set_path_matrix(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 0.75, 0.0));
        assert!(object_intersects_culling_rect(
            &path,
            matrix,
            device_clip,
            bounds,
            &options
        ));
    }

    #[test]
    fn clip_cache_retains_mask_and_checks_shared_identity_and_failure() {
        use crate::fpdfapi::page::cpdf_clippath::CpdfClipPath;
        use crate::fpdfapi::page::cpdf_path::CpdfPath;
        use crate::fxcrt::fx_coordinates::CfxPointF;
        use crate::fxge::cfx_path::PointType;

        let mut path = CpdfPath::new();
        path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
        path.append_point(CfxPointF::new(12.0, 0.0), PointType::Line);
        path.append_point_and_close(CfxPointF::new(0.0, 12.0), PointType::Line);
        let mut clip = CpdfClipPath::new();
        clip.append_path(path.clone(), FillType::Winding);
        let shared = clip.clone();
        let mut independent = CpdfClipPath::new();
        independent.append_path(path.clone(), FillType::Winding);
        assert_ne!(clip, independent);

        let mut bitmap = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
        let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let base = FxRect::new(2, 2, 18, 18);
        assert!(driver.set_clip_rect(base));
        driver.save_state();
        assert!(driver.set_clip_path_fill(
            path.get_object().unwrap(),
            None,
            CfxFillRenderOptions::winding_options()
        ));
        let applied_box = driver.clip_box();
        let mut cache = ClipPathCache::default();
        cache.record(&clip, &driver, true);
        let retained = cache.region.as_ref().unwrap().mask().unwrap();
        driver.restore_state(false);
        assert_eq!(driver.clip_box(), base);
        assert!(!cache.restore(&independent, &mut driver));
        assert!(cache.restore(&shared, &mut driver));
        assert_eq!(driver.clip_box(), applied_box);
        let restored = driver.clip_region_snapshot().unwrap();
        assert!(std::ptr::eq(retained, restored.mask().unwrap()));
        assert!(driver.set_clip_rect(FxRect::new(4, 4, 6, 6)));
        assert_eq!(cache.region.as_ref().unwrap().box_rect(), applied_box);

        let mut changed = shared;
        changed.transform(CfxMatrix::default());
        assert_ne!(changed, clip);
        assert!(!cache.restore(&changed, &mut driver));
        cache.record(&clip, &driver, false);
        assert!(!cache.restore(&clip, &mut driver));
        cache.record(&clip, &driver, true);
        cache.record(&CpdfClipPath::new(), &driver, true);
        assert!(!cache.restore(&clip, &mut driver));
    }

    #[test]
    fn region_culling_keeps_subpixel_hairlines_outside_geometric_clip() {
        for content in [
            b"0.01 w 0 G 15.9 2 m 15.9 18 l S".as_slice(),
            b"0.01 w 0 G 0 8.375 m 13 10 l 0 11.625 l S".as_slice(),
        ] {
            let objects = CpdfIndirectObjectHolder::new();
            // The geometric stroke misses x=16. AGG widens a subpixel stroke to
            // one device pixel, so its antialias fringe still reaches that column.
            let stream = objects.borrow_mut().new_indirect_stream_from_bytes(content);
            let mut dict = CpdfDictionary::new();
            dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, stream.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(dict)).unwrap();
            page.parse_path_content();
            let context = CpdfRenderContext::new(&page);
            let stopped = Cell::new(false);
            let resources = RenderResources::new(&context, &stopped);
            let options = CpdfRenderOptions::new();
            let mut whole = CfxDibitmap::create(32, 20, FxdibFormat::Bgra, 0).unwrap();
            let mut region = CfxDibitmap::create(16, 20, FxdibFormat::Bgra, 0).unwrap();
            for (bitmap, offset) in [(&mut whole, 0.0), (&mut region, -16.0)] {
                let mut driver = CfxAggDeviceDriver::new(bitmap, false, None);
                assert!(render_holder(
                    page.holder(),
                    &mut driver,
                    CfxMatrix::new(1.0, 0.0, 0.0, 1.0, offset, 0.0),
                    &options,
                    resources,
                    0,
                    None,
                    &[]
                ));
            }
            assert!(whole.scanline(10).unwrap()[16 * 4 + 3] != 0);
            for y in 0..20 {
                assert_eq!(
                    &whole.scanline(y).unwrap()[16 * 4..32 * 4],
                    &region.scanline(y).unwrap()[..16 * 4]
                );
            }
        }
    }

    #[test]
    fn holder_clip_cache_matches_per_object_clipping_across_scopes() {
        let cases: &[&[u8]] = &[
            b"q 2 2 m 18 2 l 2 18 l h W n 1 0 0 rg 0 0 20 20 re f 0 0 1 rg 0 0 8 8 re f Q 0 1 0 rg 18 18 2 2 re f",
            b"q 2 2 16 16 re 6 6 8 8 re W* n 1 0 0 rg 0 0 20 20 re f 0 0 1 rg 0 0 8 8 re f Q 0 1 0 rg 18 18 2 2 re f",
            b"q 2 2 m 18 2 l 2 18 l h W n 1 0 0 rg 0 0 20 20 re f q 0 0 0 0 re W n 0 0 1 rg 0 0 20 20 re f Q 1 1 0 rg 0 0 8 8 re f Q 0 1 0 rg 18 18 2 2 re f",
        ];
        for content in cases {
            let objects = CpdfIndirectObjectHolder::new();
            let stream = objects.borrow_mut().new_indirect_stream_from_bytes(content);
            let mut dict = CpdfDictionary::new();
            dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, stream.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(dict)).unwrap();
            page.parse_path_content();
            let context = CpdfRenderContext::new(&page);
            let stopped = Cell::new(false);
            let resources = RenderResources::new(&context, &stopped);
            let options = CpdfRenderOptions::new();
            for matrix in [
                CfxMatrix::default(),
                CfxMatrix::new(0.75, 0.0, 0.0, 0.75, 2.0, 1.0),
            ] {
                let mut cached = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
                let mut uncached = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
                let base = FxRect::new(1, 1, 19, 19);
                {
                    let mut driver = CfxAggDeviceDriver::new(&mut cached, false, None);
                    assert!(driver.set_clip_rect(base));
                    assert!(render_holder(
                        page.holder(),
                        &mut driver,
                        matrix,
                        &options,
                        resources,
                        0,
                        None,
                        &[]
                    ));
                    assert_eq!(driver.clip_box(), base);
                }
                {
                    let mut driver = CfxAggDeviceDriver::new(&mut uncached, false, None);
                    assert!(driver.set_clip_rect(base));
                    for object in page.holder().iter() {
                        driver.save_state();
                        assert!(render_single_object(
                            object.as_ref(),
                            &mut driver,
                            matrix,
                            &options,
                            resources,
                            0,
                            None,
                            &[],
                            None
                        ));
                        driver.restore_state(false);
                    }
                    assert_eq!(driver.clip_box(), base);
                }
                assert_eq!(
                    cached.buffer(),
                    uncached.buffer(),
                    "content={:?}, matrix={matrix:?}",
                    String::from_utf8_lossy(content)
                );
                assert!(cached.buffer().iter().any(|&byte| byte != 0));
            }
        }
    }

    #[test]
    fn interpolated_image_colors_ignore_fully_transparent_neighbors() {
        let mut image = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
        image.scanline_mut(0).unwrap()[..8].copy_from_slice(&[0, 0, 255, 0, 255, 0, 0, 255]);
        let mut dest = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
        {
            let mut driver = CfxAggDeviceDriver::new(&mut dest, false, None);
            let mut renderer = driver.start_dibits(
                Rc::new(image),
                1.0,
                0,
                CfxMatrix::new(3.0, 0.0, 0.0, -1.0, 0.0, 1.0),
                crate::fxge::dib::fx_dib::FxdibResampleOptions {
                    interpolate_bilinear: true,
                    ..Default::default()
                },
            );
            while renderer.continue_render(None) {}
        }
        assert_eq!(&dest.buffer()[4..7], &[255, 0, 0]);
        assert!((127..=128).contains(&dest.buffer()[7]));
    }

    #[test]
    fn annotation_appearance_renders_only_when_requested() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut appearance_dict = CpdfDictionary::new();
        appearance_dict.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
        appearance_dict.set_matrix_for(b"Matrix", CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 5.0, 0.0));
        let appearance = retain_object(CpdfStream::from_data_and_dict(
            b"1 0 0 rg 0 0 10 10 re f".to_vec(),
            retain_object(appearance_dict),
        ));
        objects.borrow_mut().add_indirect_object(appearance.clone());
        let mut ap = CpdfDictionary::new();
        ap.set_new_for(
            b"N",
            CpdfReference::new(&objects, appearance.borrow().obj_num()),
        );
        let mut annot = CpdfDictionary::new();
        annot.set_new_name_for(b"Subtype", b"Square");
        annot.set_rect_for(b"Rect", CfxFloatRect::new(10.0, 20.0, 30.0, 40.0));
        annot.set_new_for(b"AP", ap);
        let mut annots = CpdfArray::new();
        annots.append_new(annot);
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
        page_dict.set_new_for(b"Annots", annots);
        let page = CpdfPage::new(retain_object(page_dict)).unwrap();
        let matrix = page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0);
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        let mut options = CpdfRenderOptions::new();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            matrix,
            &options,
            false
        ));
        let offset = 70 * bitmap.pitch() as usize + 20 * 4;
        assert_eq!(&bitmap.buffer()[offset..offset + 4], &[0; 4]);
        options.set_draw_annots(true);
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            matrix,
            &options,
            false
        ));
        assert_eq!(&bitmap.buffer()[offset..offset + 4], &[0, 0, 255, 255]);

        let annots = page.annots_array().unwrap();
        let annot = annots
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .get_direct_object_at(0)
            .unwrap();
        annot
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"F", CpdfScalarObject::number_from_signed(4 | 32));
        bitmap.clear(0);
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            matrix,
            &options,
            false
        ));
        assert_eq!(&bitmap.buffer()[offset..offset + 4], &[0; 4]);
        options.set_printing(true);
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            matrix,
            &options,
            false
        ));
        assert_eq!(&bitmap.buffer()[offset..offset + 4], &[0, 0, 255, 255]);
    }

    fn gradient_page(
        shading_type: i32,
        coordinate_values: &[i32],
        extend_ends: bool,
        bbox: Option<CfxFloatRect>,
    ) -> CfxDibitmap {
        let objects = CpdfIndirectObjectHolder::new();
        let mut function = CpdfDictionary::new();
        function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        let mut domain = CpdfArray::new();
        for value in [0, 1] {
            domain.append_new(CpdfScalarObject::number_from_signed(value));
        }
        function.set_new_for(b"Domain", domain);
        let mut c0 = CpdfArray::new();
        let mut c1 = CpdfArray::new();
        for (first, last) in [(1, 0), (0, 0), (0, 1)] {
            c0.append_new(CpdfScalarObject::number_from_signed(first));
            c1.append_new(CpdfScalarObject::number_from_signed(last));
        }
        function.set_new_for(b"C0", c0);
        function.set_new_for(b"C1", c1);
        let mut shading = CpdfDictionary::new();
        shading.set_new_for(
            b"ShadingType",
            CpdfScalarObject::number_from_signed(shading_type),
        );
        shading.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        shading.set_new_for(b"Function", function);
        if let Some(bbox) = bbox {
            shading.set_rect_for(b"BBox", bbox);
        }
        let mut coords = CpdfArray::new();
        for &value in coordinate_values {
            coords.append_new(CpdfScalarObject::number_from_signed(value));
        }
        shading.set_new_for(b"Coords", coords);
        if extend_ends {
            let mut extend = CpdfArray::new();
            extend.append_new(CpdfScalarObject::boolean(true));
            extend.append_new(CpdfScalarObject::boolean(true));
            shading.set_new_for(b"Extend", extend);
        }
        let mut shadings = CpdfDictionary::new();
        shadings.set_new_for(b"S1", shading);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Shading", shadings);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"/S1 sh");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        assert_eq!(page.holder().page_object_count(), 1);
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false,
        ));
        bitmap
    }

    #[test]
    fn axial_shading_function_reaches_page_bitmap() {
        let bitmap = gradient_page(2, &[20, 0, 80, 0], true, None);
        let pixel = |x: usize| {
            let offset = 50 * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        assert!(pixel(10)[2] > 200 && pixel(10)[0] < 50);
        assert!(pixel(90)[0] > 200 && pixel(90)[2] < 50);
        assert!(pixel(50)[0] > 100 && pixel(50)[2] > 100);
    }

    #[test]
    fn shading_bbox_limits_painted_area() {
        let bitmap = gradient_page(
            2,
            &[20, 0, 80, 0],
            true,
            Some(CfxFloatRect::new(25.0, 25.0, 75.0, 75.0)),
        );
        let pixel = |x: usize, y: usize| {
            let offset = y * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        assert_eq!(pixel(10, 50), &[0; 4]);
        assert_eq!(pixel(50, 10), &[0; 4]);
        assert_eq!(pixel(90, 50), &[0; 4]);
        assert_eq!(pixel(50, 90), &[0; 4]);
        assert_eq!(pixel(50, 50)[3], 255);
    }

    #[test]
    fn radial_shading_function_reaches_page_bitmap() {
        let bitmap = gradient_page(3, &[50, 50, 0, 50, 50, 45], false, None);
        let pixel = |x: usize| {
            let offset = 50 * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        assert!(pixel(50)[2] > 240 && pixel(50)[0] < 15);
        assert!(pixel(90)[0] > 200 && pixel(90)[2] < 50);
        assert!(pixel(70)[0] > 90 && pixel(70)[2] > 90);
        assert_eq!(pixel(99), &[0; 4]);
    }

    #[test]
    fn decreasing_radial_shading_uses_inner_circle_at_end() {
        let bitmap = gradient_page(3, &[50, 50, 45, 50, 50, 0], false, None);
        let row = bitmap.scanline(50).unwrap();
        assert_eq!(&row[50 * 4..51 * 4], &[254, 0, 1, 255]);
        assert_eq!(&row[95 * 4..96 * 4], &[0, 0, 255, 255]);
    }

    #[test]
    fn sampled_function_shading_reaches_page_bitmap() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut function_dict = CpdfDictionary::new();
        function_dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(0));
        function_dict.set_new_for(b"BitsPerSample", CpdfScalarObject::number_from_signed(8));
        let mut size = CpdfArray::new();
        size.append_new(CpdfScalarObject::number_from_signed(2));
        size.append_new(CpdfScalarObject::number_from_signed(2));
        function_dict.set_new_for(b"Size", size);
        let mut domain = CpdfArray::new();
        for value in [0, 100, 0, 100] {
            domain.append_new(CpdfScalarObject::number_from_signed(value));
        }
        function_dict.set_new_for(b"Domain", domain);
        let mut range = CpdfArray::new();
        for _ in 0..3 {
            range.append_new(CpdfScalarObject::number_from_signed(0));
            range.append_new(CpdfScalarObject::number_from_signed(1));
        }
        function_dict.set_new_for(b"Range", range);
        let function = retain_object(CpdfStream::from_data_and_dict(
            vec![255, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 255],
            retain_object(function_dict),
        ));
        objects.borrow_mut().add_indirect_object(function.clone());
        let mut shading = CpdfDictionary::new();
        shading.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(1));
        shading.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        shading.set_new_for(
            b"Function",
            CpdfReference::new(&objects, function.borrow().obj_num()),
        );
        let mut shading_domain = CpdfArray::new();
        for value in [0, 100, 0, 100] {
            shading_domain.append_new(CpdfScalarObject::number_from_signed(value));
        }
        shading.set_new_for(b"Domain", shading_domain);
        let mut shadings = CpdfDictionary::new();
        shadings.set_new_for(b"S1", shading);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Shading", shadings);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"/S1 sh");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false,
        ));
        let pixel = |x: usize| {
            let offset = 50 * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        assert!(pixel(10)[2] > 200 && pixel(10)[0] < 50);
        assert!(pixel(90)[0] > 200 && pixel(90)[2] < 50);
    }

    fn mesh_page(shading_type: i32, data: Vec<u8>) -> CfxDibitmap {
        let objects = CpdfIndirectObjectHolder::new();
        let mut shading = CpdfDictionary::new();
        shading.set_new_for(
            b"ShadingType",
            CpdfScalarObject::number_from_signed(shading_type),
        );
        shading.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        shading.set_new_for(
            b"BitsPerCoordinate",
            CpdfScalarObject::number_from_signed(8),
        );
        shading.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        if matches!(shading_type, 4 | 6 | 7) {
            shading.set_new_for(b"BitsPerFlag", CpdfScalarObject::number_from_signed(8));
        } else {
            shading.set_new_for(b"VerticesPerRow", CpdfScalarObject::number_from_signed(2));
        }
        let mut decode = CpdfArray::new();
        for value in [0, 100, 0, 100, 0, 1, 0, 1, 0, 1] {
            decode.append_new(CpdfScalarObject::number_from_signed(value));
        }
        shading.set_new_for(b"Decode", decode);
        let stream = retain_object(CpdfStream::from_data_and_dict(data, retain_object(shading)));
        objects.borrow_mut().add_indirect_object(stream.clone());
        let mut shadings = CpdfDictionary::new();
        shadings.set_new_for(
            b"S1",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Shading", shadings);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"/S1 sh");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        assert_eq!(page.holder().page_object_count(), 1);
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false
        ));
        bitmap
    }

    #[test]
    fn free_form_triangle_mesh_reaches_page_bitmap() {
        let bitmap = mesh_page(
            4,
            vec![
                0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255,
            ],
        );
        let red_offset = 90 * bitmap.pitch() as usize + 10 * 4;
        assert!(bitmap.buffer()[red_offset + 2] > bitmap.buffer()[red_offset]);
        assert_eq!(bitmap.buffer()[red_offset + 3], 255);
        let empty_offset = 10 * bitmap.pitch() as usize + 90 * 4;
        assert_eq!(&bitmap.buffer()[empty_offset..empty_offset + 4], &[0; 4]);
    }

    #[test]
    fn lattice_triangle_mesh_reaches_page_bitmap() {
        let bitmap = mesh_page(
            5,
            vec![
                0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        );
        let red_offset = 90 * bitmap.pitch() as usize + 10 * 4;
        assert!(bitmap.buffer()[red_offset + 2] > bitmap.buffer()[red_offset]);
        let white_offset = 10 * bitmap.pitch() as usize + 90 * 4;
        assert!(bitmap.buffer()[white_offset..white_offset + 4]
            .iter()
            .all(|&value| value > 200));
    }

    #[test]
    fn coons_and_tensor_patches_reach_page_bitmap() {
        let boundary = [
            (0, 0),
            (0, 85),
            (0, 170),
            (0, 255),
            (85, 255),
            (170, 255),
            (255, 255),
            (255, 170),
            (255, 85),
            (255, 0),
            (170, 0),
            (85, 0),
        ];
        for shading_type in [6, 7] {
            let mut data = vec![0];
            for (x, y) in boundary {
                data.extend_from_slice(&[x, y]);
            }
            if shading_type == 7 {
                for (x, y) in [(85, 85), (85, 170), (170, 170), (170, 85)] {
                    data.extend_from_slice(&[x, y]);
                }
            }
            data.extend_from_slice(&[255, 0, 0, 0, 0, 255, 255, 255, 255, 0, 255, 0]);
            let bitmap = mesh_page(shading_type, data);
            let red_offset = 90 * bitmap.pitch() as usize + 10 * 4;
            let white_offset = 10 * bitmap.pitch() as usize + 90 * 4;
            assert!(bitmap.buffer()[red_offset + 2] > bitmap.buffer()[red_offset]);
            assert!(bitmap.buffer()[white_offset..white_offset + 4]
                .iter()
                .all(|&value| value > 200));
        }
    }

    #[test]
    fn adjacent_coons_patches_share_edges_and_extend_bounds() {
        let mut data = vec![0];
        let first = [
            (0, 0),
            (0, 85),
            (0, 170),
            (0, 255),
            (28, 255),
            (57, 255),
            (85, 255),
            (85, 170),
            (85, 85),
            (85, 0),
            (57, 0),
            (28, 0),
        ];
        for (x, y) in first {
            data.extend_from_slice(&[x, y]);
        }
        for _ in 0..4 {
            data.extend_from_slice(&[255, 0, 0]);
        }
        for continuation in [
            [
                (113, 0),
                (142, 0),
                (170, 0),
                (170, 85),
                (170, 170),
                (170, 255),
                (142, 255),
                (113, 255),
            ],
            [
                (198, 255),
                (227, 255),
                (255, 255),
                (255, 170),
                (255, 85),
                (255, 0),
                (227, 0),
                (198, 0),
            ],
        ] {
            data.push(2);
            for (x, y) in continuation {
                data.extend_from_slice(&[x, y]);
            }
            for _ in 0..2 {
                data.extend_from_slice(&[255, 0, 0]);
            }
        }
        let bitmap = mesh_page(6, data);
        for x in [15, 50, 85] {
            let offset = 50 * bitmap.pitch() as usize + x * 4;
            assert_eq!(&bitmap.buffer()[offset..offset + 4], &[0, 0, 255, 255]);
        }
    }

    #[test]
    fn colored_tiling_pattern_fills_only_its_path() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut pattern_dict = CpdfDictionary::new();
        pattern_dict.set_new_name_for(b"Type", b"Pattern");
        pattern_dict.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(1));
        pattern_dict.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(1));
        pattern_dict.set_new_for(b"XStep", CpdfScalarObject::number_from_signed(10));
        pattern_dict.set_new_for(b"YStep", CpdfScalarObject::number_from_signed(10));
        pattern_dict.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            b"1 0 0 rg 0 0 5 5 re f".to_vec(),
            retain_object(pattern_dict),
        ));
        objects.borrow_mut().add_indirect_object(stream.clone());
        let mut patterns = CpdfDictionary::new();
        patterns.set_new_for(
            b"P1",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Pattern", patterns);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"/Pattern cs /P1 scn 0 0 40 40 re f");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 50.0, 50.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(50, 50, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 50, 50), 0),
            &CpdfRenderOptions::new(),
            false
        ));
        let pixel = |x: usize, y: usize| {
            let offset = y * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        assert_eq!(pixel(2, 47), &[0, 0, 255, 255]);
        assert_eq!(pixel(7, 47), &[0; 4]);
        assert_eq!(pixel(12, 47), &[0, 0, 255, 255]);
        assert_eq!(pixel(42, 47), &[0; 4]);
    }

    #[test]
    fn uncolored_tiling_pattern_uses_base_color() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut pattern_dict = CpdfDictionary::new();
        pattern_dict.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(1));
        pattern_dict.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(2));
        pattern_dict.set_new_for(b"XStep", CpdfScalarObject::number_from_signed(10));
        pattern_dict.set_new_for(b"YStep", CpdfScalarObject::number_from_signed(10));
        pattern_dict.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            b"0 0 5 5 re f".to_vec(),
            retain_object(pattern_dict),
        ));
        objects.borrow_mut().add_indirect_object(stream.clone());
        let mut patterns = CpdfDictionary::new();
        patterns.set_new_for(
            b"P1",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        let mut pattern_cs = CpdfArray::new();
        pattern_cs.append_new_name(b"Pattern");
        pattern_cs.append_new_name(b"DeviceRGB");
        let mut color_spaces = CpdfDictionary::new();
        color_spaces.set_new_for(b"PatRGB", pattern_cs);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Pattern", patterns);
        resources.set_new_for(b"ColorSpace", color_spaces);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"/PatRGB cs 0 0 1 /P1 scn 0 0 20 20 re f");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 20.0, 20.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 20, 20), 0),
            &CpdfRenderOptions::new(),
            false
        ));
        let pixel = |x: usize| {
            let offset = 17 * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        assert_eq!(pixel(2), &[255, 0, 0, 255]);
        // Native mask composition retains the base RGB under zero alpha.
        assert_eq!(pixel(7), &[255, 0, 0, 0]);
        assert_eq!(pixel(12), &[255, 0, 0, 255]);
    }

    #[test]
    fn shading_pattern_respects_path_and_background() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut function = CpdfDictionary::new();
        function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        let mut domain = CpdfArray::new();
        for value in [0, 1] {
            domain.append_new(CpdfScalarObject::number_from_signed(value));
        }
        function.set_new_for(b"Domain", domain);
        let mut c0 = CpdfArray::new();
        let mut c1 = CpdfArray::new();
        for (start, end) in [(1, 0), (0, 0), (0, 1)] {
            c0.append_new(CpdfScalarObject::number_from_signed(start));
            c1.append_new(CpdfScalarObject::number_from_signed(end));
        }
        function.set_new_for(b"C0", c0);
        function.set_new_for(b"C1", c1);
        let mut shading = CpdfDictionary::new();
        shading.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(2));
        shading.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        shading.set_new_for(b"Function", function);
        let mut coords = CpdfArray::new();
        for value in [10, 0, 30, 0] {
            coords.append_new(CpdfScalarObject::number_from_signed(value));
        }
        shading.set_new_for(b"Coords", coords);
        let mut background = CpdfArray::new();
        for value in [0, 1, 0] {
            background.append_new(CpdfScalarObject::number_from_signed(value));
        }
        shading.set_new_for(b"Background", background);
        let mut pattern = CpdfDictionary::new();
        pattern.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(2));
        pattern.set_new_for(b"Shading", shading);
        let mut patterns = CpdfDictionary::new();
        patterns.set_new_for(b"P1", pattern);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Pattern", patterns);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"/Pattern cs /P1 scn 0 0 40 40 re f");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 50.0, 50.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(50, 50, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 50, 50), 0),
            &CpdfRenderOptions::new(),
            false
        ));
        let pixel = |x: usize| {
            let offset = 25 * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        assert_eq!(pixel(5), &[0, 255, 0, 255]);
        assert!(pixel(20)[0] > 100 && pixel(20)[2] > 100);
        assert_eq!(pixel(35), &[0, 255, 0, 255]);
        assert_eq!(pixel(45), &[0; 4]);
    }

    #[test]
    fn tiling_pattern_stroke_precedes_normal_fill() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut pattern_dict = CpdfDictionary::new();
        pattern_dict.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(1));
        pattern_dict.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(1));
        pattern_dict.set_new_for(b"XStep", CpdfScalarObject::number_from_signed(10));
        pattern_dict.set_new_for(b"YStep", CpdfScalarObject::number_from_signed(10));
        pattern_dict.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            b"1 0 0 rg 0 0 10 10 re f".to_vec(),
            retain_object(pattern_dict),
        ));
        objects.borrow_mut().add_indirect_object(stream.clone());
        let mut patterns = CpdfDictionary::new();
        patterns.set_new_for(
            b"P1",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Pattern", patterns);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"0 0 1 rg /Pattern CS /P1 SCN 8 w 10 10 30 30 re B");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 50.0, 50.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(50, 50, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 50, 50), 0),
            &CpdfRenderOptions::new(),
            false
        ));
        let pixel = |x: usize| {
            let offset = 25 * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        // ProcessPathPattern draws the patterned stroke first; normal fill
        // subsequently covers its inner half.
        assert_eq!(pixel(8), &[0, 0, 255, 255]);
        assert_eq!(pixel(11), &[255, 0, 0, 255]);
        assert_eq!(pixel(25), &[255, 0, 0, 255]);
        assert_eq!(pixel(3), &[0; 4]);
    }

    #[test]
    fn forced_color_can_convert_fills_to_strokes() {
        use super::super::cpdf_renderoptions::RenderColorScheme;
        let objects = CpdfIndirectObjectHolder::new();
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"1 0 0 rg 2 2 4 4 re f");
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(dict)).unwrap();
        page.parse_path_content();
        let mut options = CpdfRenderOptions::new();
        options.set_color_mode(RenderColorMode::ForcedColor);
        options.set_color_scheme(RenderColorScheme {
            path_fill_color: 0xffff0000,
            path_stroke_color: 0xff00ff00,
            ..Default::default()
        });
        options.options_mut().convert_fill_to_stroke = true;
        options.options_mut().no_path_smooth = true;
        let mut bitmap = CfxDibitmap::create(8, 8, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            CfxMatrix::default(),
            &options,
            false
        ));
        assert_eq!(&bitmap.scanline(4).unwrap()[16..20], &[0; 4]);
        assert!(bitmap.buffer().chunks_exact(4).any(|pixel| pixel[3] != 0));
        assert!(bitmap
            .buffer()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] != 0)
            .all(|pixel| pixel[..3] == [0, 255, 0]));
    }

    #[test]
    fn separable_path_blend_modes_composite_with_alpha() {
        for (mode, expected_red, expected_blue) in [
            (b"Multiply".as_slice(), 128, 0),
            (b"Screen".as_slice(), 255, 128),
        ] {
            let objects = CpdfIndirectObjectHolder::new();
            let mut graphics = CpdfDictionary::new();
            graphics.set_new_name_for(b"BM", mode);
            graphics.set_new_for(b"ca", CpdfScalarObject::number_from_float(0.5));
            let mut states = CpdfDictionary::new();
            states.set_new_for(b"Blend", graphics);
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"ExtGState", states);
            let content = objects.borrow_mut().new_indirect_stream_from_bytes(
                b"1 0 0 rg 0 0 20 20 re f /Blend gs 0 0 1 rg 4 4 12 12 re f",
            );
            let mut page_dict = CpdfDictionary::new();
            page_dict.set_new_name_for(b"Type", b"Page");
            page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 20.0, 20.0));
            page_dict.set_new_for(b"Resources", resources);
            page_dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, content.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
            page.parse_path_content();
            // Opaque targets must recover an alpha backdrop by rendering the
            // context up to this object; BGRA can read it back directly.
            for format in [FxdibFormat::Bgra, FxdibFormat::Bgr, FxdibFormat::Bgrx] {
                let mut bitmap = CfxDibitmap::create(20, 20, format, 0).unwrap();
                assert!(render_page_objects(
                    &page,
                    &mut bitmap,
                    page.display_matrix_for_rect(FxRect::new(0, 0, 20, 20), 0),
                    &CpdfRenderOptions::new(),
                    false
                ));
                let step = crate::fxge::dib::fx_dib::get_bpp_from_format(format) as usize / 8;
                let row = bitmap.scanline(10).unwrap();
                let pixel = &row[10 * step..11 * step];
                assert!((i32::from(pixel[0]) - expected_blue).abs() <= 1);
                assert_eq!(pixel[1], 0);
                assert!((i32::from(pixel[2]) - expected_red).abs() <= 1);
                if format == FxdibFormat::Bgra {
                    assert_eq!(pixel[3], 255);
                }
                assert_eq!(&row[2 * step..2 * step + 3], &[0, 0, 255]);
            }
        }
    }

    #[test]
    fn image_blend_mode_composites_over_page_path() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_name_for(b"Subtype", b"Image");
        image_dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        let image = retain_object(CpdfStream::from_data_and_dict(
            vec![0, 0, 255],
            retain_object(image_dict),
        ));
        let image_number = objects.borrow_mut().add_indirect_object(image);
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Im1", CpdfReference::new(&objects, image_number));
        let mut blend = CpdfDictionary::new();
        blend.set_new_name_for(b"BM", b"Multiply");
        blend.set_new_for(b"ca", CpdfScalarObject::number_from_float(0.5));
        let mut states = CpdfDictionary::new();
        states.set_new_for(b"Blend", blend);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"XObject", xobjects);
        resources.set_new_for(b"ExtGState", states);
        let content = objects.borrow_mut().new_indirect_stream_from_bytes(
            b"1 0 0 rg 0 0 20 20 re f q /Blend gs 20 0 0 20 0 0 cm /Im1 Do Q",
        );
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 20.0, 20.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 20, 20), 0),
            &CpdfRenderOptions::new(),
            false
        ));
        let offset = 10 * bitmap.pitch() as usize + 10 * 4;
        let pixel = &bitmap.buffer()[offset..offset + 4];
        assert_eq!(pixel[0], 0);
        assert_eq!(pixel[1], 0);
        assert!((i32::from(pixel[2]) - 128).abs() <= 1);
        assert_eq!(pixel[3], 255);
    }

    #[test]
    fn object_transfer_function_changes_image_fill_and_stroke_and_is_retained() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_name_for(b"Subtype", b"Image");
        image_dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        let image = retain_object(CpdfStream::from_data_and_dict(
            vec![10, 20, 30],
            retain_object(image_dict),
        ));
        let image_number = objects.borrow_mut().add_indirect_object(image);
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Im1", CpdfReference::new(&objects, image_number));
        let mut function = CpdfDictionary::new();
        function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        for (key, values) in [
            (b"Domain".as_slice(), [0, 1]),
            (b"C0", [1, 1]),
            (b"C1", [0, 0]),
        ] {
            let mut array = CpdfArray::new();
            for value in values {
                array.append_new(CpdfScalarObject::number_from_signed(value));
            }
            function.set_new_for(key, array);
        }
        let mut ext_gs = CpdfDictionary::new();
        ext_gs.set_new_for(b"TR2", function);
        let mut states = CpdfDictionary::new();
        states.set_new_for(b"Invert", ext_gs);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"XObject", xobjects);
        resources.set_new_for(b"ExtGState", states);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"q /Invert gs 10 0 0 10 0 0 cm /Im1 Do Q /Invert gs 1 0 0 rg 0 0 4 10 re f 0 1 0 RG 2 w 7 0 m 7 10 l S");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(10, 10, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 10, 10), 0),
            &CpdfRenderOptions::new(),
            false,
        ));
        let offset = 5 * bitmap.pitch() as usize + 5 * 4;
        assert_eq!(&bitmap.buffer()[offset..offset + 4], &[225, 235, 245, 255]);
        let row = bitmap.scanline(5).unwrap();
        assert_eq!(&row[8..12], &[255, 255, 0, 255]);
        assert_eq!(&row[28..32], &[255, 0, 255, 255]);
        for object in page.holder().iter() {
            let state = object.base().general_state();
            let function = state.get_tr().unwrap();
            let retained = state.get_transfer_func().unwrap();
            assert!(Rc::ptr_eq(
                &retained,
                &page
                    .page_data()
                    .render_data()
                    .get_transfer_func(&function)
                    .unwrap()
            ));
        }
    }

    #[test]
    fn form_group_opacity_applies_after_children_composite() {
        for isolated in [false, true] {
            let objects = CpdfIndirectObjectHolder::new();
            let mut group = CpdfDictionary::new();
            group.set_new_name_for(b"S", b"Transparency");
            group.set_new_for(b"I", CpdfScalarObject::boolean(isolated));
            let mut form_dict = CpdfDictionary::new();
            form_dict.set_new_name_for(b"Subtype", b"Form");
            form_dict.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 20.0, 20.0));
            form_dict.set_new_for(b"Group", group);
            let form = retain_object(CpdfStream::from_data_and_dict(
                b"1 0 0 rg 0 0 20 20 re f 0 0 1 rg 0 0 10 20 re f".to_vec(),
                retain_object(form_dict),
            ));
            let form_number = objects.borrow_mut().add_indirect_object(form);
            let mut xobjects = CpdfDictionary::new();
            xobjects.set_new_for(b"Fm1", CpdfReference::new(&objects, form_number));
            let mut half = CpdfDictionary::new();
            half.set_new_for(b"ca", CpdfScalarObject::number_from_float(0.5));
            let mut states = CpdfDictionary::new();
            states.set_new_for(b"Half", half);
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"XObject", xobjects);
            resources.set_new_for(b"ExtGState", states);
            let content = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(b"1 1 1 rg 0 0 20 20 re f /Half gs /Fm1 Do");
            let mut page_dict = CpdfDictionary::new();
            page_dict.set_new_name_for(b"Type", b"Page");
            page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 20.0, 20.0));
            page_dict.set_new_for(b"Resources", resources);
            page_dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, content.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
            page.parse_path_content();
            let mut bitmap = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
            assert!(render_page_objects(
                &page,
                &mut bitmap,
                page.display_matrix_for_rect(FxRect::new(0, 0, 20, 20), 0),
                &CpdfRenderOptions::new(),
                false
            ));
            let pixel = |x: usize| {
                let offset = 10 * bitmap.pitch() as usize + x * 4;
                &bitmap.buffer()[offset..offset + 4]
            };
            assert_eq!(pixel(5), &[255, 128, 128, 255]);
            assert_eq!(pixel(15), &[128, 128, 255, 255]);
        }
    }

    #[test]
    fn graphics_state_soft_mask_limits_path_or_form_in_local_offscreen() {
        for (subtype, mask_content, paint_form) in [
            (b"Alpha".as_slice(), b"0 0 10 20 re f".as_slice(), false),
            (b"Alpha".as_slice(), b"0 0 10 20 re f".as_slice(), true),
            (
                b"Luminosity".as_slice(),
                b"1 1 1 rg 0 0 10 20 re f".as_slice(),
                false,
            ),
        ] {
            let objects = CpdfIndirectObjectHolder::new();
            let mut mask_form_dict = CpdfDictionary::new();
            mask_form_dict.set_new_name_for(b"Subtype", b"Form");
            mask_form_dict.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 20.0, 20.0));
            let mask_form = retain_object(CpdfStream::from_data_and_dict(
                mask_content.to_vec(),
                retain_object(mask_form_dict),
            ));
            objects.borrow_mut().add_indirect_object(mask_form.clone());
            let mut soft_mask = CpdfDictionary::new();
            soft_mask.set_new_name_for(b"S", subtype);
            soft_mask.set_new_for(
                b"G",
                CpdfReference::new(&objects, mask_form.borrow().obj_num()),
            );
            let mut state = CpdfDictionary::new();
            state.set_new_for(b"SMask", soft_mask);
            let mut states = CpdfDictionary::new();
            states.set_new_for(b"Soft", state);
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"ExtGState", states);
            let content_bytes = if paint_form {
                let mut group = CpdfDictionary::new();
                group.set_new_name_for(b"S", b"Transparency");
                let mut form = CpdfDictionary::new();
                form.set_new_name_for(b"Subtype", b"Form");
                form.set_rect_for(b"BBox", CfxFloatRect::new(5.0, 5.0, 15.0, 15.0));
                form.set_new_for(b"Group", group);
                let form = retain_object(CpdfStream::from_data_and_dict(
                    b"1 0 0 rg 5 5 10 10 re f".to_vec(),
                    retain_object(form),
                ));
                let number = objects.borrow_mut().add_indirect_object(form);
                let mut xobjects = CpdfDictionary::new();
                xobjects.set_new_for(b"Paint", CpdfReference::new(&objects, number));
                resources.set_new_for(b"XObject", xobjects);
                b"q /Soft gs /Paint Do Q".as_slice()
            } else {
                b"q /Soft gs 1 0 0 rg 0 0 20 20 re f Q".as_slice()
            };
            let content = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(content_bytes);
            let mut page_dict = CpdfDictionary::new();
            page_dict.set_new_name_for(b"Type", b"Page");
            page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 20.0, 20.0));
            page_dict.set_new_for(b"Resources", resources);
            page_dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, content.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
            page.parse_path_content();
            let mut bitmap = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
            assert!(render_page_objects(
                &page,
                &mut bitmap,
                page.display_matrix_for_rect(FxRect::new(0, 0, 20, 20), 0),
                &CpdfRenderOptions::new(),
                false
            ));
            let pixel = |x: usize| {
                let offset = 10 * bitmap.pitch() as usize + x * 4;
                &bitmap.buffer()[offset..offset + 4]
            };
            assert_eq!(pixel(7), &[0, 0, 255, 255]);
            // Native BGRA composition copies RGB into a transparent backdrop
            // even when the soft mask makes the source alpha zero.
            assert_eq!(pixel(12), &[0, 0, 255, 0]);
            if paint_form {
                assert_eq!(pixel(2), &[0; 4]);
            }
        }
    }

    #[test]
    fn luminosity_mask_converts_group_backdrop_and_accepts_multi_output_transfer() {
        for (space, components, invert, expected) in [
            (b"DeviceRGB".as_slice(), vec![0.0, 1.0, 0.0], false, 150),
            (b"DeviceGray".as_slice(), vec![0.25], false, 63),
            (b"DeviceRGB".as_slice(), vec![0.0, 1.0, 0.0], true, 105),
            (b"Missing".as_slice(), vec![1.0], false, 0),
        ] {
            let objects = CpdfIndirectObjectHolder::new();
            let mut group = CpdfDictionary::new();
            group.set_new_name_for(b"CS", space);
            let mut form = CpdfDictionary::new();
            form.set_new_for(b"Group", group);
            form.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 3.0, 1.0));
            let form = retain_object(CpdfStream::from_data_and_dict(
                Vec::new(),
                retain_object(form),
            ));
            objects.borrow_mut().add_indirect_object(form.clone());
            let mut mask = CpdfDictionary::new();
            // Missing S selects luminosity, as does any name other than Alpha.
            mask.set_new_for(b"G", CpdfReference::new(&objects, form.borrow().obj_num()));
            let mut background = CpdfArray::new();
            for value in components {
                background.append_new(CpdfScalarObject::number_from_float(value));
            }
            mask.set_new_for(b"BC", background);
            if invert {
                let mut function = CpdfDictionary::new();
                function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
                function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
                for (key, values) in [
                    (b"Domain".as_slice(), [0, 1]),
                    (b"C0", [1, 0]),
                    (b"C1", [0, 1]),
                ] {
                    let mut array = CpdfArray::new();
                    for value in values {
                        array.append_new(CpdfScalarObject::number_from_signed(value));
                    }
                    function.set_new_for(key, array);
                }
                mask.set_new_for(b"TR", function);
            }
            let mut path = CpdfPathObject::default();
            path.base_mut()
                .mutable_general_state()
                .set_soft_mask(Some(retain_object(mask)));
            let page = CpdfPage::new(retain_object(CpdfDictionary::new())).unwrap();
            let mut pixels = CfxDibitmap::create(3, 1, FxdibFormat::Bgra, 0).unwrap();
            let _driver = CfxAggDeviceDriver::new(&mut pixels, false, None);
            let context = CpdfRenderContext::new(&page);
            let stopped = Cell::new(false);
            let mask = load_object_soft_mask(
                &path,
                FxRect::new(0, 0, 3, 1),
                CfxMatrix::default(),
                RenderResources::new(&context, &stopped),
                0,
            )
            .unwrap();
            assert_eq!(&mask.scanline(0).unwrap()[..3], &[expected; 3]);
        }
    }

    #[test]
    fn parsed_fill_stroke_and_clip_reach_bitmap() {
        let objects = CpdfIndirectObjectHolder::new();
        let stream = objects.borrow_mut().new_indirect_stream_from_bytes(
            b"0 0 6 8 re W n 1 0 0 rg 0 0 10 10 re f 0 0 1 RG 1 w 2 2 m 8 2 l S",
        );
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Type", b"Page");
        dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(12, 12, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            CfxMatrix::default(),
            &CpdfRenderOptions::new(),
            false,
        ));
        let pixel = |x: usize, y: usize| {
            let offset = y * bitmap.pitch() as usize + x * 4;
            bitmap.buffer()[offset..offset + 4].to_vec()
        };
        assert_eq!(pixel(4, 5), [0, 0, 255, 255]);
        assert_eq!(pixel(8, 5), [0, 0, 0, 0]);
        assert!(pixel(3, 2)[0] > 0);
    }

    #[test]
    fn hint_reliant_bounds_keep_visible_glyphs_when_origin_is_outside_page() {
        for (font_bytes, expected_right) in [
            (
                include_bytes!("../../../tests/fixtures/hint-reliant.ttf").as_slice(),
                10.0,
            ),
            (
                include_bytes!("../../../tests/fixtures/hint-optional.ttf").as_slice(),
                -10.0,
            ),
        ] {
            let objects = CpdfIndirectObjectHolder::new();
            let font_stream = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(font_bytes);
            let mut descriptor = CpdfDictionary::new();
            descriptor.set_new_for(
                b"FontFile2",
                CpdfReference::new(&objects, font_stream.borrow().obj_num()),
            );
            let mut widths = CpdfArray::new();
            widths.append_new(CpdfScalarObject::number_from_signed(700));
            let mut font = CpdfDictionary::new();
            font.set_new_name_for(b"Subtype", b"TrueType");
            font.set_new_name_for(b"BaseFont", b"Outline-Test");
            font.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
            font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
            font.set_new_for(b"Widths", widths);
            font.set_new_for(b"FontDescriptor", descriptor);
            let mut fonts = CpdfDictionary::new();
            fonts.set_new_for(b"F1", font);
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"Font", fonts);
            // Both source outlines end at 400 units. Required mono hinting at
            // 1000 ppem moves the right edge to 600. At 100pt with x=-50, the
            // required glyph is visible in x=0..10; the ordinary one is outside.
            let content = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(b"BT /F1 100 Tf 1 0 0 1 -50 10 Tm (A) Tj ET");
            let mut dict = CpdfDictionary::new();
            dict.set_new_name_for(b"Type", b"Page");
            dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
            dict.set_new_for(b"Resources", resources);
            dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, content.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(dict)).unwrap();
            page.parse_path_content();
            let rect = page.holder().iter().next().unwrap().base().rect();
            assert!((rect.right - expected_right).abs() < 0.001, "{rect:?}");
            let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
            assert!(render_page_objects(
                &page,
                &mut bitmap,
                page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
                &CpdfRenderOptions::new(),
                false,
            ));
            let alpha = bitmap.scanline(50).unwrap()[5 * 4 + 3];
            if expected_right > 0.0 {
                assert!(alpha > 192);
            } else {
                assert_eq!(alpha, 0);
            }
        }
    }

    #[test]
    fn embedded_font_formats_share_mapping_face_and_reach_agg_bitmap() {
        let cases: [(&[u8], &[u8], &[u8]); 3] = [
            (
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
                )),
                b"TrueType",
                b"FontFile2",
            ),
            (
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/triangle.pfa"
                )),
                b"Type1",
                b"FontFile",
            ),
            (
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../upstream/pdfium/testing/resources/fonts/foxit_fixed.cff"
                )),
                b"Type1",
                b"FontFile3",
            ),
        ];
        for (font_bytes, subtype, file_key) in cases {
            let objects = CpdfIndirectObjectHolder::new();
            let font_stream = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(font_bytes);
            let mut descriptor = CpdfDictionary::new();
            descriptor.set_new_for(
                file_key,
                CpdfReference::new(&objects, font_stream.borrow().obj_num()),
            );
            let mut widths = CpdfArray::new();
            widths.append_new(CpdfScalarObject::number_from_signed(1000));
            let mut font = CpdfDictionary::new();
            font.set_new_name_for(b"Subtype", subtype);
            font.set_new_name_for(b"BaseFont", b"PdfoxiumEmbeddedTest");
            font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
            font.set_new_for(b"Widths", widths);
            font.set_new_for(b"FontDescriptor", descriptor);
            let mut fonts = CpdfDictionary::new();
            fonts.set_new_for(b"F1", font);
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"Font", fonts);
            let content = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(b"BT /F1 50 Tf 1 0 0 1 10 20 Tm (A) Tj ET");
            let mut page_dict = CpdfDictionary::new();
            page_dict.set_new_name_for(b"Type", b"Page");
            let mut media_box = CpdfArray::new();
            for edge in [0, 0, 100, 100] {
                media_box.append_new(CpdfScalarObject::number_from_signed(edge));
            }
            page_dict.set_new_for(b"MediaBox", media_box);
            page_dict.set_new_for(b"Resources", resources);
            page_dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, content.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
            let cached_face = page
                .page_data()
                .get_font_file_face(&font_stream)
                .unwrap()
                .unwrap();
            assert_eq!(cached_face.face().current_charmap(), None);
            page.parse_path_content();
            assert_eq!(page.holder().page_object_count(), 1);
            assert!(cached_face.face().current_charmap().is_some());
            let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
            assert!(render_page_objects(
                &page,
                &mut bitmap,
                page.display_matrix_for_rect(
                    crate::fxcrt::fx_coordinates::FxRect::new(0, 0, 100, 100),
                    0
                ),
                &CpdfRenderOptions::new(),
                false,
            ));
            assert!(cached_face.face().current_charmap().is_some());
            assert!(
                bitmap.buffer().chunks_exact(4).any(|pixel| pixel[3] != 0),
                "{}",
                String::from_utf8_lossy(file_key)
            );
        }
    }

    #[test]
    fn text_painting_handles_pattern_fill_stroke_and_scaled_stroke_width() {
        let cases: &[(&[u8], &[(usize, usize, [u8; 4])])] = &[
            (
                b"/Half gs /Pattern CS /P1 SCN 8 w BT /F1 50 Tf 1 Tr 1 0 0 1 10 20 Tm (A) Tj ET",
                &[
                    (12, 50, [0, 0, 255, 127]),
                    (17, 50, [0; 4]),
                    (62, 50, [0, 0, 255, 127]),
                    (70, 50, [0; 4]),
                ],
            ),
            (
                b"/Pattern cs /P1 scn BT /F1 50 Tf 1 0 0 1 10 20 Tm (A) Tj ET",
                &[
                    (12, 50, [0, 0, 255, 255]),
                    (17, 50, [0; 4]),
                    (62, 50, [0; 4]),
                ],
            ),
            (
                b"0 0 1 rg /Pattern CS /P1 SCN 8 w BT /F1 50 Tf 2 Tr 1 0 0 1 10 20 Tm (A) Tj ET",
                &[
                    (12, 50, [255, 0, 0, 255]),
                    (17, 50, [255, 0, 0, 255]),
                    (62, 50, [0, 0, 255, 255]),
                    (70, 50, [0; 4]),
                ],
            ),
            (
                b"2 0 0 3 0 0 cm 2 w 0 1 0 RG BT /F1 10 Tf 1 Tr 1 0 0 1 10 10 Tm (A) Tj ET",
                &[
                    (18, 60, [0, 255, 0, 255]),
                    (30, 60, [0; 4]),
                    (16, 60, [0; 4]),
                ],
            ),
        ];
        for &(content_bytes, pixels) in cases {
            let objects = CpdfIndirectObjectHolder::new();
            let font_stream = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(include_bytes!(
                    "../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
                ));
            let mut descriptor = CpdfDictionary::new();
            descriptor.set_new_for(
                b"FontFile2",
                CpdfReference::new(&objects, font_stream.borrow().obj_num()),
            );
            descriptor.set_new_for(b"Flags", CpdfScalarObject::number_from_signed(32));
            let mut font = CpdfDictionary::new();
            font.set_new_name_for(b"Subtype", b"TrueType");
            font.set_new_for(b"FontDescriptor", descriptor);
            let mut fonts = CpdfDictionary::new();
            fonts.set_new_for(b"F1", font);
            let mut pattern_dict = CpdfDictionary::new();
            pattern_dict.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(1));
            pattern_dict.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(1));
            pattern_dict.set_new_for(b"XStep", CpdfScalarObject::number_from_signed(10));
            pattern_dict.set_new_for(b"YStep", CpdfScalarObject::number_from_signed(10));
            pattern_dict.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
            let stream = retain_object(CpdfStream::from_data_and_dict(
                b"1 0 0 rg 0 0 5 10 re f".to_vec(),
                retain_object(pattern_dict),
            ));
            objects.borrow_mut().add_indirect_object(stream.clone());
            let mut patterns = CpdfDictionary::new();
            patterns.set_new_for(
                b"P1",
                CpdfReference::new(&objects, stream.borrow().obj_num()),
            );
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"Font", fonts);
            resources.set_new_for(b"Pattern", patterns);
            let mut half = CpdfDictionary::new();
            half.set_new_for(b"CA", CpdfScalarObject::number_from_float(0.5));
            let mut graphics = CpdfDictionary::new();
            graphics.set_new_for(b"Half", half);
            resources.set_new_for(b"ExtGState", graphics);
            let content = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(content_bytes);
            let mut page_dict = CpdfDictionary::new();
            page_dict.set_new_name_for(b"Type", b"Page");
            page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
            page_dict.set_new_for(b"Resources", resources);
            page_dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, content.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
            page.parse_path_content();
            let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
            let mut options = CpdfRenderOptions::new();
            options.options_mut().no_text_smooth = true;
            assert!(render_page_objects(
                &page,
                &mut bitmap,
                page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
                &options,
                false
            ));
            for &(x, y, expected) in pixels {
                let offset = y * bitmap.pitch() as usize + x * 4;
                assert_eq!(
                    &bitmap.buffer()[offset..offset + 4],
                    &expected,
                    "{} at {x},{y}",
                    String::from_utf8_lossy(content_bytes)
                );
            }
        }
    }

    #[test]
    fn retained_text_clip_layers_union_then_intersect() {
        let font_bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
        ));
        let objects = CpdfIndirectObjectHolder::new();
        let font_stream = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(font_bytes);
        let mut descriptor = CpdfDictionary::new();
        descriptor.set_new_for(
            b"FontFile2",
            CpdfReference::new(&objects, font_stream.borrow().obj_num()),
        );
        let mut widths = CpdfArray::new();
        widths.append_new(CpdfScalarObject::number_from_signed(1000));
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"TrueType");
        font.set_new_name_for(b"BaseFont", b"Ahem");
        font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        font.set_new_for(b"Widths", widths);
        font.set_new_for(b"FontDescriptor", descriptor);
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let content = objects.borrow_mut().new_indirect_stream_from_bytes(
            b"BT /F1 50 Tf 7 Tr 1 0 0 1 10 20 Tm (A) Tj 1 0 0 1 60 20 Tm (A) Tj ET BT 1 0 0 1 40 20 Tm (A) Tj ET 1 0 0 rg 0 0 100 100 re f",
        );
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        assert_eq!(page.holder().page_object_count(), 4);
        let clip = page
            .holder()
            .page_object_by_index(3)
            .unwrap()
            .base()
            .clip_path();
        assert_eq!(clip.path_count(), 0);
        assert_eq!(clip.text_count(), 5);
        assert!(clip.text(2).is_none());
        assert!(clip.text(4).is_none());
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false
        ));
        let pixel = |x: usize| {
            let offset = 50 * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        assert_eq!(pixel(5), &[0; 4]);
        assert_eq!(pixel(30), &[0; 4]);
        assert_eq!(pixel(50), &[0, 0, 255, 255]);
        assert_eq!(pixel(70), &[0, 0, 255, 255]);
        assert_eq!(pixel(95), &[0; 4]);
    }

    #[test]
    fn embedded_cid_truetype_text_reaches_bitmap() {
        let font_bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
        ));
        let glyph = CfxFace::new(font_bytes, 0).unwrap().get_char_index(65) as u16;
        assert_ne!(glyph, 0);
        let objects = CpdfIndirectObjectHolder::new();
        let font_stream = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(font_bytes);
        let mut gid_map = vec![0; 132];
        gid_map[130..132].copy_from_slice(&glyph.to_be_bytes());
        let gid_stream = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(&gid_map);
        let mut descriptor = CpdfDictionary::new();
        descriptor.set_new_for(
            b"FontFile2",
            CpdfReference::new(&objects, font_stream.borrow().obj_num()),
        );
        let mut descendant = CpdfDictionary::new();
        descendant.set_new_name_for(b"Subtype", b"CIDFontType2");
        descendant.set_new_name_for(b"BaseFont", b"Ahem");
        descendant.set_new_for(b"FontDescriptor", descriptor);
        descendant.set_new_for(
            b"CIDToGIDMap",
            CpdfReference::new(&objects, gid_stream.borrow().obj_num()),
        );
        let mut descendants = CpdfArray::new();
        descendants.append_new(descendant);
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"Type0");
        font.set_new_name_for(b"Encoding", b"Identity-H");
        font.set_new_for(b"DescendantFonts", descendants);
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"BT /F1 50 Tf 1 0 0 1 10 20 Tm <0041> Tj ET");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        let mut media_box = CpdfArray::new();
        for edge in [0, 0, 100, 100] {
            media_box.append_new(CpdfScalarObject::number_from_signed(edge));
        }
        page_dict.set_new_for(b"MediaBox", media_box);
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false,
        ));
        assert!(bitmap.buffer().chunks_exact(4).any(|pixel| pixel[3] != 0));
    }

    #[test]
    fn nonembedded_cid_text_uses_tounicode_and_substitute() {
        let path = std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem"
        ));
        with_render_font_mapper(|mapper| {
            mapper.set_system_font_info(Some(Box::new(CfxLinuxFontInfo::from_paths([path]))));
        });
        let objects = CpdfIndirectObjectHolder::new();
        let unicode = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"1 beginbfchar <0041> <0041> endbfchar");
        let mut descendant = CpdfDictionary::new();
        descendant.set_new_name_for(b"Subtype", b"CIDFontType2");
        descendant.set_new_name_for(b"BaseFont", b"Ahem");
        let mut descendants = CpdfArray::new();
        descendants.append_new(descendant);
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"Type0");
        font.set_new_name_for(b"Encoding", b"Identity-H");
        font.set_new_for(b"DescendantFonts", descendants);
        font.set_new_for(
            b"ToUnicode",
            CpdfReference::new(&objects, unicode.borrow().obj_num()),
        );
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"BT /F1 50 Tf 1 0 0 1 10 20 Tm <0041> Tj ET");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        let mut media_box = CpdfArray::new();
        for edge in [0, 0, 100, 100] {
            media_box.append_new(CpdfScalarObject::number_from_signed(edge));
        }
        page_dict.set_new_for(b"MediaBox", media_box);
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false,
        ));
        assert!(bitmap.buffer().chunks_exact(4).any(|pixel| pixel[3] != 0));
    }

    #[test]
    fn nonembedded_type1_text_uses_installed_sfnt_face() {
        let path = std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem"
        ));
        with_render_font_mapper(|mapper| {
            mapper.set_system_font_info(Some(Box::new(CfxLinuxFontInfo::from_paths([path]))));
        });
        let mut widths = CpdfArray::new();
        widths.append_new(CpdfScalarObject::number_from_signed(1000));
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"Type1");
        font.set_new_name_for(b"BaseFont", b"Ahem");
        font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        font.set_new_for(b"Widths", widths);
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let objects = CpdfIndirectObjectHolder::new();
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"BT /F1 50 Tf 1 0 0 1 10 20 Tm (A) Tj ET");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        let mut media_box = CpdfArray::new();
        for edge in [0, 0, 100, 100] {
            media_box.append_new(CpdfScalarObject::number_from_signed(edge));
        }
        page_dict.set_new_for(b"MediaBox", media_box);
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false,
        ));
        assert!(bitmap.buffer().chunks_exact(4).any(|pixel| pixel[3] != 0));
    }

    #[test]
    fn parsed_rgb_image_reaches_page_bitmap() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_name_for(b"Subtype", b"Image");
        image_dict.set_new_name_for(b"ColorSpace", b"ImageRGB");
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(2));
        image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        image_dict.set_new_for(b"Interpolate", CpdfScalarObject::boolean(true));
        let image = retain_object(CpdfStream::from_data_and_dict(
            vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
            retain_object(image_dict),
        ));
        let image_number = objects.borrow_mut().add_indirect_object(image);
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Im1", CpdfReference::new(&objects, image_number));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"XObject", xobjects);
        let mut color_spaces = CpdfDictionary::new();
        color_spaces.set_new_name_for(b"ImageRGB", b"DeviceRGB");
        resources.set_new_for(b"ColorSpace", color_spaces);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"q 100 0 0 100 0 0 cm /Im1 Do Q");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        let mut media_box = CpdfArray::new();
        for edge in [0, 0, 100, 100] {
            media_box.append_new(CpdfScalarObject::number_from_signed(edge));
        }
        page_dict.set_new_for(b"MediaBox", media_box);
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        assert_eq!(page.holder().page_object_count(), 1);
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false,
        ));
        let pixel = |x: usize, y: usize| {
            let offset = y * bitmap.pitch() as usize + x * 4;
            bitmap.buffer()[offset..offset + 4].to_vec()
        };
        assert_eq!(pixel(10, 10), [0, 0, 255, 255]);
        assert_eq!(pixel(90, 10), [0, 255, 0, 255]);
        assert_eq!(pixel(10, 90), [255, 0, 0, 255]);
        assert_eq!(pixel(90, 90), [255, 255, 255, 255]);
        let center = pixel(50, 50);
        assert!(center[..3]
            .iter()
            .all(|&channel| (50..220).contains(&channel)));
        let mut sharp = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        let mut options = CpdfRenderOptions::new();
        options.options_mut().no_image_smooth = true;
        assert!(render_page_objects(
            &page,
            &mut sharp,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &options,
            false,
        ));
        let offset = 50 * sharp.pitch() as usize + 50 * 4;
        assert_eq!(&sharp.buffer()[offset..offset + 4], &[255, 255, 255, 255]);
    }

    #[test]
    fn type3_image_masks_flush_around_forms_and_skip_direct_font_recursion() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_name_for(b"Subtype", b"Image");
        image_dict.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(2));
        image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(1));
        let image = retain_object(CpdfStream::from_data_and_dict(
            vec![0, 0],
            retain_object(image_dict),
        ));
        let image_num = objects.borrow_mut().add_indirect_object(image);
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Im1", CpdfReference::new(&objects, image_num));
        let mut font_resources = CpdfDictionary::new();
        font_resources.set_new_for(b"XObject", xobjects);
        let font_resources = retain_object(font_resources);
        let mut procs = CpdfDictionary::new();
        for (name, data) in [
            (
                b"A".as_slice(),
                b"500 0 0 0 500 700 d1 500 0 0 700 0 0 cm /Im1 Do".as_slice(),
            ),
            (
                b"B".as_slice(),
                b"500 0 0 0 500 700 d1 0 0 500 700 re f".as_slice(),
            ),
            (
                b"C".as_slice(),
                b"500 0 0 0 500 700 d1 0 0 500 700 re f BT /F1 1 Tf (C) Tj ET".as_slice(),
            ),
        ] {
            let stream = objects.borrow_mut().new_indirect_stream_from_bytes(data);
            procs.set_new_for(
                name,
                CpdfReference::new(&objects, stream.borrow().obj_num()),
            );
        }
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"Type3");
        font.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
        font.set_matrix_for(
            b"FontMatrix",
            CfxMatrix::new(0.001, 0.0, 0.0, 0.001, 0.0, 0.0),
        );
        font.set_new_for(b"CharProcs", procs);
        font.set_for(b"Resources", Some(Rc::clone(&font_resources)));
        font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        let mut widths = CpdfArray::new();
        for _ in 0..3 {
            widths.append_new(CpdfScalarObject::number_from_signed(500));
        }
        font.set_new_for(b"Widths", widths);
        let font_num = objects
            .borrow_mut()
            .add_indirect_object(retain_object(font));
        let mut nested_fonts = CpdfDictionary::new();
        nested_fonts.set_new_for(b"F1", CpdfReference::new(&objects, font_num));
        font_resources
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Font", nested_fonts);
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", CpdfReference::new(&objects, font_num));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"1 0 0 rg BT /F1 20 Tf 10 20 Td (AABAC) Tj ET");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        for printing in [false, true] {
            let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
            let mut options = CpdfRenderOptions::new();
            options.set_printing(printing);
            assert!(render_page_objects(
                &page,
                &mut bitmap,
                page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
                &options,
                false
            ));
            let row = bitmap.scanline(73).unwrap();
            for x in [15, 25, 35, 45, 55] {
                assert_eq!(&row[x * 4..x * 4 + 4], &[0, 0, 255, 255]);
            }
            assert_eq!(&row[70 * 4..70 * 4 + 4], &[0, 0, 0, 0]);
        }
        let text = page
            .holder()
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        let Some(CpdfFont::Type3(font)) = text.base().text_state().get_font() else {
            panic!("Type 3 font");
        };
        let glyph = font.load_char(65).unwrap();
        assert!(glyph.borrow().get_bitmap().is_some());
        assert!(glyph.borrow().form().is_none());
    }

    #[test]
    fn type3_opacity_applies_once_to_bitmap_batches_and_colored_forms() {
        for colored in [false, true] {
            let objects = CpdfIndirectObjectHolder::new();
            let mut image_dict = CpdfDictionary::new();
            image_dict.set_new_name_for(b"Subtype", b"Image");
            image_dict.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
            image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
            image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(2));
            image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(1));
            let image = retain_object(CpdfStream::from_data_and_dict(
                vec![0, 0],
                retain_object(image_dict),
            ));
            let image_num = objects.borrow_mut().add_indirect_object(image);
            let glyph = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(if colored {
                    b"1000 0 d0 1 0 0 rg 0 0 1000 1000 re f"
                } else {
                    b"1000 0 0 0 1000 1000 d1 1000 0 0 1000 0 0 cm /Im Do"
                });
            let mut procs = CpdfDictionary::new();
            procs.set_new_for(b"A", CpdfReference::new(&objects, glyph.borrow().obj_num()));
            let mut xobjects = CpdfDictionary::new();
            xobjects.set_new_for(b"Im", CpdfReference::new(&objects, image_num));
            let mut glyph_resources = CpdfDictionary::new();
            glyph_resources.set_new_for(b"XObject", xobjects);
            let mut font = CpdfDictionary::new();
            font.set_new_name_for(b"Subtype", b"Type3");
            font.set_matrix_for(
                b"FontMatrix",
                CfxMatrix::new(0.001, 0.0, 0.0, 0.001, 0.0, 0.0),
            );
            font.set_rect_for(b"FontBBox", CfxFloatRect::new(0.0, 0.0, 1000.0, 1000.0));
            let mut differences = CpdfArray::new();
            differences.append_new(CpdfScalarObject::number_from_signed(65));
            differences.append_new_name(b"A");
            let mut encoding = CpdfDictionary::new();
            encoding.set_new_for(b"Differences", differences);
            font.set_new_for(b"Encoding", encoding);
            font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
            let mut widths = CpdfArray::new();
            widths.append_new(CpdfScalarObject::number_from_signed(1000));
            font.set_new_for(b"Widths", widths);
            font.set_new_for(b"CharProcs", procs);
            font.set_new_for(b"Resources", glyph_resources);
            let font_num = objects
                .borrow_mut()
                .add_indirect_object(retain_object(font));
            let mut fonts = CpdfDictionary::new();
            fonts.set_new_for(b"F", CpdfReference::new(&objects, font_num));
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"Font", fonts);
            let content = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(b".2 .4 .8 rg BT /F 20 Tf 10 30 Td (AA) Tj ET");
            let mut page_dict = CpdfDictionary::new();
            page_dict.set_new_name_for(b"Type", b"Page");
            page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
            page_dict.set_new_for(b"Resources", resources);
            page_dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, content.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
            page.parse_path_content();
            let mut cached_bytes = None;
            // Return to half opacity after opaque/quarter renders: neither
            // rendering route may modify a retained glyph's cached coverage.
            for opacity in [0.5, 1.0, 0.25, 0.5] {
                page.mutable_holder()
                    .page_object_by_index_mut(0)
                    .unwrap()
                    .base_mut()
                    .mutable_general_state()
                    .set_fill_alpha(opacity);
                let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
                bitmap.clear(0xffff_ffff);
                assert!(render_page_objects(
                    &page,
                    &mut bitmap,
                    page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
                    &CpdfRenderOptions::new(),
                    false
                ));
                let source = if colored { [0, 0, 255] } else { [204, 102, 51] };
                let alpha = (opacity * 255.0) as i32;
                let expected =
                    source.map(|channel| ((255 * (255 - alpha) + channel * alpha) / 255) as u8);
                for x in [20, 40] {
                    let pixel = &bitmap.scanline(60).unwrap()[x * 4..x * 4 + 4];
                    assert_eq!(
                        &pixel[..3],
                        &expected,
                        "colored={colored} opacity={opacity}"
                    );
                    assert_eq!(pixel[3], 255);
                }
                if !colored {
                    let font = match page
                        .holder()
                        .page_object_by_index(0)
                        .unwrap()
                        .base()
                        .text_state()
                        .get_font()
                    {
                        Some(CpdfFont::Type3(font)) => font,
                        _ => panic!("Type3 test font"),
                    };
                    let glyph = font.load_char(65).unwrap();
                    let cached = glyph.borrow().get_bitmap().unwrap();
                    let bytes = cached.borrow().buffer().to_vec();
                    if let Some(previous) = &cached_bytes {
                        assert_eq!(&bytes, previous);
                    } else {
                        cached_bytes = Some(bytes);
                    }
                }
            }
        }
    }

    #[test]
    fn nested_uncolored_type3_fonts_preserve_the_callers_color() {
        use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;

        fn type3_font(
            objects: &HolderRef,
            code: i32,
            name: &[u8],
            program: &[u8],
            resources: CpdfDictionary,
        ) -> CpdfDictionary {
            let glyph = objects.borrow_mut().new_indirect_stream_from_bytes(program);
            let mut procedures = CpdfDictionary::new();
            procedures.set_new_for(name, CpdfReference::new(objects, glyph.borrow().obj_num()));
            let mut differences = CpdfArray::new();
            differences.append_new(CpdfScalarObject::number_from_signed(code));
            differences.append_new_name(name);
            let mut encoding = CpdfDictionary::new();
            encoding.set_new_for(b"Differences", differences);
            let mut matrix = CpdfArray::new();
            for value in [0.001, 0.0, 0.0, 0.001, 0.0, 0.0] {
                matrix.append_new(CpdfScalarObject::number_from_float(value));
            }
            let mut widths = CpdfArray::new();
            widths.append_new(CpdfScalarObject::number_from_signed(500));
            let mut font = CpdfDictionary::new();
            font.set_new_name_for(b"Subtype", b"Type3");
            font.set_new_for(b"CharProcs", procedures);
            font.set_new_for(b"Encoding", encoding);
            font.set_new_for(b"FontMatrix", matrix);
            font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(code));
            font.set_new_for(b"Widths", widths);
            font.set_rect_for(b"FontBBox", CfxFloatRect::new(0.0, 0.0, 500.0, 700.0));
            font.set_new_for(b"Resources", resources);
            font
        }

        for (child_program, expected) in [
            (
                b"500 0 0 0 500 700 d1 0 0 500 700 re f".as_slice(),
                [255, 0, 0, 255],
            ),
            (
                b"500 0 d0 1 0 0 rg 0 0 500 700 re f".as_slice(),
                [0, 0, 255, 255],
            ),
        ] {
            let objects = CpdfIndirectObjectHolder::new();
            let child = type3_font(&objects, 66, b"B", child_program, CpdfDictionary::new());
            let mut child_fonts = CpdfDictionary::new();
            child_fonts.set_new_for(b"Child", child);
            let mut child_resources = CpdfDictionary::new();
            child_resources.set_new_for(b"Font", child_fonts);
            let outer = type3_font(
                &objects,
                65,
                b"A",
                b"500 0 0 0 500 700 d1 BT /Child 1000 Tf 0 0 Td (B) Tj ET",
                child_resources,
            );
            let mut fonts = CpdfDictionary::new();
            fonts.set_new_for(b"Outer", outer);
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"Font", fonts);
            let content = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(b"0 0 1 rg BT /Outer 40 Tf 10 20 Td (A) Tj ET");
            let mut dictionary = CpdfDictionary::new();
            dictionary.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
            dictionary.set_new_for(b"Resources", resources);
            dictionary.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, content.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(dictionary)).unwrap();
            page.parse_path_content();
            let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
            assert!(render_page_objects(
                &page,
                &mut bitmap,
                page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
                &CpdfRenderOptions::new(),
                false,
            ));
            let offset = 60 * bitmap.pitch() as usize + 15 * 4;
            assert_eq!(&bitmap.buffer()[offset..offset + 4], &expected);
            let outside = 60 * bitmap.pitch() as usize + 35 * 4;
            assert_eq!(&bitmap.buffer()[outside..outside + 4], &[0; 4]);
        }
    }

    #[test]
    fn type3_char_proc_path_reaches_page_bitmap() {
        let objects = CpdfIndirectObjectHolder::new();
        let glyph = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"500 0 0 0 500 700 d1 0 0 500 700 re f");
        let colored_glyph = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"500 0 d0 0 0 1 rg 0 0 500 700 re f");
        let mut char_procs = CpdfDictionary::new();
        char_procs.set_new_for(b"A", CpdfReference::new(&objects, glyph.borrow().obj_num()));
        char_procs.set_new_for(
            b"B",
            CpdfReference::new(&objects, colored_glyph.borrow().obj_num()),
        );
        let mut differences = CpdfArray::new();
        differences.append_new(CpdfScalarObject::number_from_signed(65));
        differences.append_new_name(b"A");
        differences.append_new_name(b"B");
        let mut encoding = CpdfDictionary::new();
        encoding.set_new_for(b"Differences", differences);
        let mut font_matrix = CpdfArray::new();
        for value in [0.001, 0.0, 0.0, 0.001, 0.0, 0.0] {
            font_matrix.append_new(CpdfScalarObject::number_from_float(value));
        }
        let mut widths = CpdfArray::new();
        widths.append_new(CpdfScalarObject::number_from_signed(500));
        widths.append_new(CpdfScalarObject::number_from_signed(500));
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"Type3");
        font.set_new_for(b"CharProcs", char_procs);
        font.set_new_for(b"Encoding", encoding);
        font.set_new_for(b"FontMatrix", font_matrix);
        font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        font.set_new_for(b"Widths", widths);
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"1 0 0 rg BT /F1 40 Tf 10 20 Td (AB) Tj ET");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        let mut media_box = CpdfArray::new();
        for edge in [0, 0, 100, 100] {
            media_box.append_new(CpdfScalarObject::number_from_signed(edge));
        }
        page_dict.set_new_for(b"MediaBox", media_box);
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false,
        ));
        let pixel = |x: usize, y: usize| {
            let offset = y * bitmap.pitch() as usize + x * 4;
            bitmap.buffer()[offset..offset + 4].to_vec()
        };
        assert_eq!(pixel(15, 60), [0, 0, 255, 255]);
        assert_eq!(pixel(35, 60), [255, 0, 0, 255]);
        assert_eq!(pixel(55, 60), [0, 0, 0, 0]);
    }

    #[test]
    fn type3_text_bypasses_text_clipping() {
        let objects = CpdfIndirectObjectHolder::new();
        let glyph = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"500 0 0 0 500 700 d1 0 0 500 700 re f");
        let mut char_procs = CpdfDictionary::new();
        char_procs.set_new_for(b"A", CpdfReference::new(&objects, glyph.borrow().obj_num()));
        let mut differences = CpdfArray::new();
        differences.append_new(CpdfScalarObject::number_from_signed(65));
        differences.append_new_name(b"A");
        let mut encoding = CpdfDictionary::new();
        encoding.set_new_for(b"Differences", differences);
        let mut font_matrix = CpdfArray::new();
        for value in [0.001, 0.0, 0.0, 0.001, 0.0, 0.0] {
            font_matrix.append_new(CpdfScalarObject::number_from_float(value));
        }
        let mut widths = CpdfArray::new();
        widths.append_new(CpdfScalarObject::number_from_signed(500));
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"Type3");
        font.set_new_for(b"CharProcs", char_procs);
        font.set_new_for(b"Encoding", encoding);
        font.set_new_for(b"FontMatrix", font_matrix);
        font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        font.set_new_for(b"Widths", widths);
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let content = objects.borrow_mut().new_indirect_stream_from_bytes(
            b"BT /F1 40 Tf 7 Tr 10 20 Td (A) Tj ET 1 0 0 rg 0 0 100 100 re f",
        );
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false
        ));
        let pixel = |x: usize, y: usize| {
            let offset = y * bitmap.pitch() as usize + x * 4;
            &bitmap.buffer()[offset..offset + 4]
        };
        assert_eq!(pixel(15, 60), &[0, 0, 255, 255]);
        assert_eq!(pixel(35, 60), &[0, 0, 255, 255]);
        assert_eq!(pixel(15, 30), &[0, 0, 255, 255]);
    }

    #[test]
    fn image_grayscale_keeps_cached_colors_and_direct_oc_controls_visibility() {
        use crate::fpdfapi::page::cpdf_occontext::{CpdfOcContext, UsageType};
        use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
        let objects = CpdfIndirectObjectHolder::new();
        let mut view = CpdfDictionary::new();
        view.set_new_name_for(b"ViewState", b"OFF");
        let mut usage = CpdfDictionary::new();
        usage.set_new_for(b"View", view);
        let mut ocg = CpdfDictionary::new();
        ocg.set_new_for(b"Usage", usage);
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"Image");
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        for (key, value) in [
            (b"Width".as_slice(), 1),
            (b"Height", 1),
            (b"BitsPerComponent", 8),
        ] {
            dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        let mut mask_dict = CpdfDictionary::new();
        mask_dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        for (key, value) in [
            (b"Width".as_slice(), 1),
            (b"Height", 1),
            (b"BitsPerComponent", 8),
        ] {
            mask_dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        let mask_stream = retain_object(CpdfStream::from_data_and_dict(
            vec![128],
            retain_object(mask_dict),
        ));
        objects
            .borrow_mut()
            .add_indirect_object(mask_stream.clone());
        dict.set_new_for(
            b"SMask",
            CpdfReference::new(&objects, mask_stream.borrow().obj_num()),
        );
        dict.set_new_for(b"OC", ocg);
        let image = retain_object(CpdfStream::from_data_and_dict(
            vec![255, 0, 0],
            retain_object(dict),
        ));
        objects.borrow_mut().add_indirect_object(image.clone());
        let mut images = CpdfDictionary::new();
        images.set_new_for(
            b"Im",
            CpdfReference::new(&objects, image.borrow().obj_num()),
        );
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"XObject", images);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"20 0 0 20 0 0 cm /Im Do");
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Type", b"Page");
        dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 20.0, 20.0));
        dict.set_new_for(b"Resources", resources);
        dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(dict)).unwrap();
        page.parse_path_content();
        let document = CpdfDocument::new();
        for (mode, hidden, expected) in [
            (RenderColorMode::Gray, false, [76, 76, 76, 128]),
            (RenderColorMode::Normal, false, [0, 0, 255, 128]),
            (RenderColorMode::Normal, true, [0; 4]),
        ] {
            let mut options = CpdfRenderOptions::new();
            options.set_color_mode(mode);
            if hidden {
                options.set_oc_context(Some(Rc::new(CpdfOcContext::new(
                    &document,
                    UsageType::View,
                ))));
            }
            let mut bitmap = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
            assert!(render_page_objects(
                &page,
                &mut bitmap,
                page.display_matrix_for_rect(FxRect::new(0, 0, 20, 20), 0),
                &options,
                false
            ));
            let offset = 10 * bitmap.pitch() as usize + 10 * 4;
            assert_eq!(&bitmap.buffer()[offset..offset + 4], &expected);
        }
    }

    #[test]
    fn image_stencils_clip_patterns_and_apply_opacity_once_across_pattern_classes() {
        for kind in 0..3 {
            let objects = CpdfIndirectObjectHolder::new();
            let mut image_dict = CpdfDictionary::new();
            image_dict.set_new_name_for(b"Subtype", b"Image");
            image_dict.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
            for (key, value) in [
                (b"Width".as_slice(), 2),
                (b"Height", 1),
                (b"BitsPerComponent", 1),
            ] {
                image_dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
            }
            let image = retain_object(CpdfStream::from_data_and_dict(
                vec![0x40],
                retain_object(image_dict),
            ));
            objects.borrow_mut().add_indirect_object(image.clone());
            let mut images = CpdfDictionary::new();
            images.set_new_for(
                b"Im",
                CpdfReference::new(&objects, image.borrow().obj_num()),
            );
            let mut patterns = CpdfDictionary::new();
            if kind < 2 {
                let mut pattern = CpdfDictionary::new();
                pattern.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(1));
                pattern.set_new_for(
                    b"PaintType",
                    CpdfScalarObject::number_from_signed(if kind == 0 { 1 } else { 2 }),
                );
                pattern.set_new_for(b"XStep", CpdfScalarObject::number_from_signed(10));
                pattern.set_new_for(b"YStep", CpdfScalarObject::number_from_signed(10));
                pattern.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
                let paint = if kind == 0 {
                    b"1 0 0 rg 0 0 5 10 re f".as_slice()
                } else {
                    b"0 0 5 10 re f"
                };
                let stream = retain_object(CpdfStream::from_data_and_dict(
                    paint.to_vec(),
                    retain_object(pattern),
                ));
                objects.borrow_mut().add_indirect_object(stream.clone());
                patterns.set_new_for(
                    b"P",
                    CpdfReference::new(&objects, stream.borrow().obj_num()),
                );
            } else {
                let mut function = CpdfDictionary::new();
                function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
                function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
                let numbers = |values: &[i32]| {
                    let mut array = CpdfArray::new();
                    for &value in values {
                        array.append_new(CpdfScalarObject::number_from_signed(value));
                    }
                    array
                };
                function.set_new_for(b"Domain", numbers(&[0, 1]));
                function.set_new_for(b"C0", numbers(&[1, 0, 0]));
                function.set_new_for(b"C1", numbers(&[1, 0, 0]));
                let mut shading = CpdfDictionary::new();
                shading.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(2));
                shading.set_new_name_for(b"ColorSpace", b"DeviceRGB");
                shading.set_new_for(b"Coords", numbers(&[0, 0, 40, 0]));
                shading.set_new_for(b"Function", function);
                let mut pattern = CpdfDictionary::new();
                pattern.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(2));
                pattern.set_new_for(b"Shading", shading);
                patterns.set_new_for(b"P", pattern);
            }
            let mut states = CpdfDictionary::new();
            let mut alpha = CpdfDictionary::new();
            alpha.set_new_for(b"ca", CpdfScalarObject::number_from_float(0.5));
            states.set_new_for(b"Alpha", alpha);
            let mut pattern_cs = CpdfArray::new();
            pattern_cs.append_new_name(b"Pattern");
            pattern_cs.append_new_name(b"DeviceRGB");
            let mut spaces = CpdfDictionary::new();
            spaces.set_new_for(b"PatRGB", pattern_cs);
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"XObject", images);
            resources.set_new_for(b"Pattern", patterns);
            resources.set_new_for(b"ExtGState", states);
            resources.set_new_for(b"ColorSpace", spaces);
            let selection = if kind == 1 {
                b"/PatRGB cs 0 0 1 /P scn".as_slice()
            } else {
                b"/Pattern cs /P scn"
            };
            let mut content = selection.to_vec();
            content.extend_from_slice(b" /Alpha gs 40 0 0 20 0 0 cm /Im Do");
            let content = objects
                .borrow_mut()
                .new_indirect_stream_from_bytes(&content);
            let mut page_dict = CpdfDictionary::new();
            page_dict.set_new_name_for(b"Type", b"Page");
            page_dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 40.0, 20.0));
            page_dict.set_new_for(b"Resources", resources);
            page_dict.set_new_for(
                b"Contents",
                CpdfReference::new(&objects, content.borrow().obj_num()),
            );
            let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
            page.parse_path_content();
            let mut pixels = CfxDibitmap::create(40, 20, FxdibFormat::Bgra, 0).unwrap();
            let mut options = CpdfRenderOptions::new();
            options.options_mut().no_image_smooth = true;
            assert!(render_page_objects(
                &page,
                &mut pixels,
                page.display_matrix_for_rect(FxRect::new(0, 0, 40, 20), 0),
                &options,
                false
            ));
            let pixel = |x: usize| &pixels.scanline(5).unwrap()[x * 4..x * 4 + 4];
            let expected = match kind {
                0 => [0, 0, 255, 127],
                // Every pattern applies its inherited opacity once. Paths
                // truncate 0.5*255 while shading explicitly rounds it.
                1 => [255, 0, 0, 127],
                _ => [0, 0, 255, 128],
            };
            assert_eq!(pixel(2), expected, "pattern kind {kind}");
            assert_eq!(pixel(12), expected, "pattern kind {kind}");
            assert_eq!(pixel(22)[3], 0, "pattern kind {kind}");
            if kind < 2 {
                assert_eq!(pixel(7)[3], 0, "pattern kind {kind}");
            }
        }
    }

    #[test]
    fn image_mask_uses_current_fill_color() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_name_for(b"Subtype", b"Image");
        image_dict.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(1));
        let image = retain_object(CpdfStream::from_data_and_dict(
            vec![0x40],
            retain_object(image_dict),
        ));
        let image_number = objects.borrow_mut().add_indirect_object(image);
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Im1", CpdfReference::new(&objects, image_number));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"XObject", xobjects);
        let content = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"1 0 0 rg q 100 0 0 100 0 0 cm /Im1 Do Q");
        let mut page_dict = CpdfDictionary::new();
        page_dict.set_new_name_for(b"Type", b"Page");
        let mut media_box = CpdfArray::new();
        for edge in [0, 0, 100, 100] {
            media_box.append_new(CpdfScalarObject::number_from_signed(edge));
        }
        page_dict.set_new_for(b"MediaBox", media_box);
        page_dict.set_new_for(b"Resources", resources);
        page_dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, content.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(page_dict)).unwrap();
        page.parse_path_content();
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &CpdfRenderOptions::new(),
            false,
        ));
        let pixel = |x: usize| {
            let offset = 50 * bitmap.pitch() as usize + x * 4;
            bitmap.buffer()[offset..offset + 4].to_vec()
        };
        assert_eq!(pixel(25), [0, 0, 255, 255]);
        // ByteMask2Bgra copies mask RGB even when both alphas are zero.
        assert_eq!(pixel(75), [0, 0, 255, 0]);
    }
}

/// `CPDF_RenderStatus::RenderSingleObject()` for SDK object bitmap extraction.
/// Resources and caches are borrowed from the selected page for this call.
pub fn render_single_page_object_for_sdk(
    page: &CpdfPage,
    object: &dyn PageObject,
    bitmap: &mut CfxDibitmapData<'_>,
    matrix: CfxMatrix,
    device_matrix: CfxMatrix,
) -> bool {
    let context = CpdfRenderContext::new(page);
    let stopped = Cell::new(false);
    let mut resources = RenderResources::new(&context, &stopped);
    resources.device_matrix = device_matrix;
    let mut driver = CfxAggDeviceDriver::new(bitmap, false, None);
    render_single_object(
        object,
        &mut driver,
        matrix,
        &CpdfRenderOptions::new(),
        resources,
        0,
        None,
        &[],
        None,
    )
}

#[path = "cpdf_renderstatus_progressive.rs"]
mod progressive;
pub(crate) use progressive::ProgressiveRenderStatus;
