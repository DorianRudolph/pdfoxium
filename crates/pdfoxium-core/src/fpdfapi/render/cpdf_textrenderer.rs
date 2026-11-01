// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Font positioning and AGG dispatch from `core/fpdfapi/render/cpdf_textrenderer.{h,cpp}`.

use super::cpdf_renderoptions::CpdfRenderOptions;
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fxcrt::fx_coordinates::CfxMatrix;
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions;
use crate::fxge::cfx_fontmgr::with_font_manager;
use crate::fxge::cfx_graphstatedata::CfxGraphStateData;
use crate::fxge::cfx_path::CfxPath;
use crate::fxge::cfx_renderdevice::{draw_normal_text_agg, draw_text_path_agg};
use crate::fxge::cfx_textrenderoptions::{AliasingType, CfxTextRenderOptions};
use crate::fxge::text_char_pos::TextCharPos;

fn positions(font: &CpdfFont, codes: &[u32], char_pos: &[f32], font_size: f32) -> Vec<TextCharPos> {
    with_font_manager(|manager| {
        font.get_char_pos_list(
            codes,
            char_pos,
            font_size,
            &mut manager.builtin_mapper_mut(),
        )
    })
}

fn for_font_runs(pos: &[TextCharPos], mut draw: impl FnMut(i32, &[TextCharPos]) -> bool) -> bool {
    let Some(first) = pos.first() else {
        return true;
    };
    let mut result = true;
    let mut font_position = first.fallback_font_position;
    let mut start = 0;
    for (index, item) in pos.iter().enumerate() {
        if item.fallback_font_position == font_position {
            continue;
        }
        if !draw(font_position, &pos[start..index]) {
            result = false;
        }
        font_position = item.fallback_font_position;
        start = index;
    }
    if !draw(font_position, &pos[start..]) {
        result = false;
    }
    result
}

/// Accumulate the same user-space outlines as text clipping in the renderer,
/// without allocating a render device or creating additional logical PDF text.
pub(crate) fn append_pdf_text_clip_path(
    path: &mut CfxPath,
    codes: &[u32],
    char_pos: &[f32],
    font: &CpdfFont,
    font_size: f32,
    text_to_user: CfxMatrix,
) -> bool {
    if matches!(font, CpdfFont::Type3(_)) {
        return true;
    }
    let pos = positions(font, codes, char_pos, font_size);
    for_font_runs(&pos, |position, run| {
        let Some(current) = font.font_at(position) else {
            return false;
        };
        for ch in run {
            if let Some(outline) = crate::fxge::cfx_renderdevice::text_glyph_outline(
                ch,
                current,
                font_size,
                text_to_user,
            ) {
                path.append(&outline, None);
            }
        }
        true
    })
}

pub fn draw_pdf_text_path_agg(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    codes: &[u32],
    char_pos: &[f32],
    font: &CpdfFont,
    font_size: f32,
    text_to_user: CfxMatrix,
    user_to_device: Option<CfxMatrix>,
    graph_state: Option<&CfxGraphStateData>,
    fill_color: u32,
    stroke_color: u32,
    mut clipping_path: Option<&mut CfxPath>,
    fill_options: CfxFillRenderOptions,
) -> bool {
    let pos = positions(font, codes, char_pos, font_size);
    for_font_runs(&pos, |position, run| {
        let Some(current) = font.font_at(position) else {
            return false;
        };
        draw_text_path_agg(
            driver,
            run,
            current,
            font_size,
            text_to_user,
            user_to_device,
            graph_state,
            fill_color,
            stroke_color,
            clipping_path.as_deref_mut(),
            fill_options,
        )
    })
}

pub fn draw_pdf_normal_text_agg(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    codes: &[u32],
    char_pos: &[f32],
    font: &CpdfFont,
    font_size: f32,
    text_to_device: CfxMatrix,
    fill_color: u32,
    options: &CpdfRenderOptions,
) -> bool {
    let pos = positions(font, codes, char_pos, font_size);
    let mut text_options = CfxTextRenderOptions::default();
    text_options.font_is_cid = matches!(font, CpdfFont::Cid(_));
    if options.options().no_text_smooth {
        text_options.aliasing_type = AliasingType::Aliasing;
    } else if options.options().clear_type {
        text_options.aliasing_type = AliasingType::Lcd;
    }
    if options.options().no_native_text {
        text_options.native_text = false;
    }
    for_font_runs(&pos, |position, run| {
        let Some(current) = font.font_at(position) else {
            return false;
        };
        draw_normal_text_agg(
            driver,
            run,
            current,
            font_size,
            text_to_device,
            fill_color,
            text_options,
        )
    })
}

pub fn draw_pdf_text_string_agg(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    origin_x: f32,
    origin_y: f32,
    font: &CpdfFont,
    font_size: f32,
    matrix: CfxMatrix,
    bytes: &[u8],
    fill_color: u32,
    options: &CpdfRenderOptions,
) {
    if matches!(font, CpdfFont::Type3(_)) {
        return;
    }
    let count = font.count_char(bytes);
    if count == 0 {
        return;
    }
    let mut codes = Vec::with_capacity(count);
    let mut char_pos = Vec::with_capacity(count);
    let mut offset = 0;
    let mut current_pos = 0.0;
    for _ in 0..count {
        let code = font.get_next_char(bytes, &mut offset);
        codes.push(code);
        char_pos.push(current_pos);
        current_pos += font.get_char_width(code) as f32 * font_size / 1000.0;
    }
    let mut matrix = matrix;
    matrix.e = origin_x;
    matrix.f = origin_y;
    draw_pdf_normal_text_agg(
        driver, &codes, &char_pos, font, font_size, matrix, fill_color, options,
    );
}
