// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code is licensed as follows:
// Copyright 2010, 2011 ZXing authors
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! `BC_OneDimWriter.{h,cpp}`: module paths, text metrics and AGG rendering.
use crate::fxbarcode::{
    bc_library::BcTextLoc,
    bc_writer::{BarcodeFont, CbcWriter, BACKGROUND_COLOR, BAR_COLOR},
};
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxge::{
    agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
    cfx_fillrenderoptions::CfxFillRenderOptions,
    cfx_font::CfxFont,
    cfx_graphstatedata::CfxGraphStateData,
    cfx_path::CfxPath,
    cfx_renderdevice::{draw_normal_text_agg, draw_path_agg},
    cfx_textrenderoptions::{AliasingType, CfxTextRenderOptions},
    fx_fontencoding::FontEncoding,
    text_char_pos::TextCharPos,
};
use std::rc::Rc;

pub struct CbcOneDimWriter {
    pub writer: CbcWriter,
    pub print_checksum: bool,
    pub calc_checksum: bool,
    pub left_padding: bool,
    pub right_padding: bool,
    pub font: Option<Rc<dyn BarcodeFont>>,
    pub font_size: f32,
    pub font_style: i32,
    pub font_color: u32,
    pub text_location: BcTextLoc,
    pub data_length: i32,
    pub content_len: usize,
    pub output: Vec<CfxPath>,
    pub bar_width: i32,
    pub output_hscale: f32,
}
impl Default for CbcOneDimWriter {
    fn default() -> Self {
        Self {
            writer: CbcWriter::default(),
            print_checksum: true,
            calc_checksum: false,
            left_padding: false,
            right_padding: false,
            font: None,
            font_size: 10.,
            font_style: 0,
            font_color: BAR_COLOR,
            text_location: BcTextLoc::BelowEmbed,
            data_length: 0,
            content_len: 0,
            output: Vec::new(),
            bar_width: 0,
            output_hscale: 1.,
        }
    }
}
impl CbcOneDimWriter {
    pub fn has_valid_content_size(contents: &[u32]) -> bool {
        !contents.is_empty() && contents.len() <= 8192
    }
    pub fn set_font(&mut self, font: Rc<dyn BarcodeFont>) -> bool {
        self.font = Some(font);
        true
    }
    pub fn append_pattern(target: &mut Vec<u8>, pattern: &[u8], start_color: bool) {
        let mut color = start_color;
        for &width in pattern {
            target.extend(std::iter::repeat_n(u8::from(color), usize::from(width)));
            color = !color;
        }
    }
    pub fn render_result(&mut self, code: &[u8]) -> bool {
        self.output.clear();
        if code.is_empty() {
            return false;
        }
        self.writer.module_height = self.writer.module_height.max(20);
        let left = if self.left_padding { 7 } else { 0 };
        let right = if self.right_padding { 7 } else { 0 };
        let length = code.len() + left + right;
        self.output_hscale = if self.writer.width > 0 {
            self.writer.width as f32 / length as f32
        } else {
            1.
        };
        self.bar_width = self.writer.width;
        self.output.reserve(code.len());
        for (index, &value) in code.iter().enumerate() {
            if value == 1 {
                let mut path = CfxPath::new();
                let x = (index + left) as f32;
                path.append_rect(x, 0., x + 1., 1.);
                self.output.push(path);
            }
        }
        true
    }
    pub fn render_bars(&self, device: &mut CfxAggDeviceDriver<'_, '_>, matrix: CfxMatrix) -> bool {
        if self.output.is_empty() {
            return false;
        }
        let mut background = CfxPath::new();
        background.append_rect(0., 0., self.writer.width as f32, self.writer.height as f32);
        let state = CfxGraphStateData::default();
        if !draw_path_agg(
            device,
            &background,
            Some(matrix),
            Some(&state),
            BACKGROUND_COLOR,
            BACKGROUND_COLOR,
            CfxFillRenderOptions::even_odd_options(),
        ) {
            return false;
        }
        let mut scaled = CfxMatrix::new(
            self.output_hscale,
            0.,
            0.,
            self.writer.height as f32,
            0.,
            0.,
        );
        scaled.concat(matrix);
        for path in &self.output {
            if !draw_path_agg(
                device,
                path,
                Some(scaled),
                Some(&state),
                BAR_COLOR,
                0,
                CfxFillRenderOptions::winding_options(),
            ) {
                return false;
            }
        }
        true
    }
    pub fn render_device_result(
        &self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        contents: &[u32],
    ) -> bool {
        if !self.render_bars(device, matrix) {
            return false;
        }
        // Correct the upstream inverted space guard: any supplied human-readable
        // content is drawn. Without a selected font, retain useful bars-only output.
        self.text_location == BcTextLoc::None
            || self.font.is_none()
            || contents.is_empty()
            || self.show_chars(device, matrix, contents)
    }
    pub fn calc_text_info(
        text: &[u8],
        font: &CfxFont<'_>,
        width: f32,
        size: i32,
    ) -> (f32, Vec<TextCharPos>) {
        let mut encoding = FontEncoding::None;
        let face = font.face();
        if let Some(face) = face {
            for id in [
                FontEncoding::Symbol,
                FontEncoding::Unicode,
                FontEncoding::Sjis,
                FontEncoding::GB2312,
                FontEncoding::Big5,
                FontEncoding::Wansung,
                FontEncoding::Johab,
                FontEncoding::AdobeStandard,
                FontEncoding::AdobeExpert,
                FontEncoding::AdobeCustom,
                FontEncoding::Latin1,
                FontEncoding::OldLatin2,
                FontEncoding::AppleRoman,
            ] {
                if face.select_charmap(id) {
                    encoding = id;
                    break;
                }
            }
        }
        // Alternate charmap resolver from core/fxge/cfx_charmap_resolver.cpp.
        let mut glyph = |code: u8| -> u32 {
            let Some(face) = face else {
                return u32::from(code);
            };
            let index = face.get_char_index(u32::from(code));
            if index != 0 {
                return index;
            }
            for map in 0..face.charmap_count() {
                let id = crate::fxge::cfx_face::charmap_id_pair_to_font_encoding(
                    face.charmap_id_pair_by_index(map),
                );
                if id == encoding || !face.select_charmap(id) {
                    continue;
                }
                let index = face.get_char_index(u32::from(code));
                if index != 0 {
                    encoding = id;
                    return index;
                }
            }
            face.select_charmap(encoding);
            0
        };
        let mut positions: Vec<_> = text
            .iter()
            .map(|&ch| {
                let index = glyph(ch);
                TextCharPos {
                    glyph_index: index,
                    font_char_width: font.get_glyph_width(index, 0),
                    ..Default::default()
                }
            })
            .collect();
        let total = positions
            .iter()
            .map(|ch| ch.font_char_width as f32 * size as f32 / 1000.)
            .sum::<f32>();
        let mut left = (width - total) / 2.;
        if left < 0. && width == 0. {
            left = 0.;
        }
        let mut pen = left;
        let y = font.get_descent().unsigned_abs() as f32 * size as f32 / 1000.;
        for ch in &mut positions {
            ch.origin = CfxPointF::new(pen, y);
            pen += ch.font_char_width as f32 * size as f32 / 1000.;
        }
        (total, positions)
    }
    pub fn show_chars(
        &self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        contents: &[u32],
    ) -> bool {
        let Some(owner) = &self.font else {
            return false;
        };
        let font = owner.dev_font();
        let bytes = wide_to_utf8(contents);
        let size = self.font_size.abs() as i32;
        let text_height = size.saturating_add(1);
        let mut width = if matches!(self.text_location, BcTextLoc::Above | BcTextLoc::Below) {
            self.bar_width as f32
        } else {
            0.
        };
        let (char_width, positions) = Self::calc_text_info(&bytes, font, width, size);
        if char_width < 1. {
            return true;
        }
        let embedded = matches!(
            self.text_location,
            BcTextLoc::AboveEmbed | BcTextLoc::BelowEmbed
        );
        let x = if embedded {
            (self.bar_width as f32 - char_width) as i32 / 2
        } else {
            0
        };
        let y = if matches!(self.text_location, BcTextLoc::Above | BcTextLoc::AboveEmbed) {
            0
        } else {
            self.writer.height.saturating_sub(text_height)
        };
        if embedded {
            width = char_width;
        }
        let mut rect = CfxFloatRect::new(
            x as f32,
            y as f32,
            x as f32 + width,
            (y as f32) + text_height as f32,
        );
        if width != self.writer.width as f32 {
            rect.right -= 1.;
        }
        if !device.fill_rect(
            matrix.transform_float_rect(rect).get_outer_rect(),
            BACKGROUND_COLOR,
        ) {
            return false;
        }
        let mut text_matrix = CfxMatrix::new(1., 0., 0., -1., x as f32, y as f32 + size as f32);
        text_matrix.concat(matrix);
        draw_normal_text_agg(
            device,
            &positions,
            font,
            size as f32,
            text_matrix,
            self.font_color,
            CfxTextRenderOptions::new(AliasingType::Lcd),
        )
    }
    /// The EAN writers share clearing, metrics and drawing while retaining their
    /// separate source-defined groups, positions and widths.
    pub fn show_ean_chars(
        &self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        contents: &[u32],
        clear: &[(f32, f32)],
        groups: &[(usize, usize, f32, f32)],
    ) -> bool {
        let Some(owner) = &self.font else {
            return false;
        };
        let font = owner.dev_font();
        let bytes = wide_to_utf8(contents);
        let size = self.font_size.abs() as i32;
        let text_height = size.saturating_add(1);
        let y = self.writer.height as f32 - text_height as f32;
        let mut scaled = CfxMatrix::new(self.output_hscale, 0., 0., 1., 0., 0.);
        scaled.concat(matrix);
        for &(left, right) in clear {
            let rect = CfxFloatRect::new(left, y, right, self.writer.height as f32);
            if !device.fill_rect(
                scaled.transform_float_rect(rect).get_outer_rect(),
                BACKGROUND_COLOR,
            ) {
                return false;
            }
        }
        for &(offset, length, x, width) in groups {
            let Some(text) = bytes.get(offset..offset + length) else {
                return false;
            };
            let (_, positions) = Self::calc_text_info(text, font, width, size);
            let mut affine =
                CfxMatrix::new(1., 0., 0., -1., x * self.output_hscale, y + size as f32);
            affine.concat(matrix);
            if !draw_normal_text_agg(
                device,
                &positions,
                font,
                size as f32,
                affine,
                self.font_color,
                CfxTextRenderOptions::new(AliasingType::Lcd),
            ) {
                return false;
            }
        }
        true
    }
}
pub fn wide_to_utf8(contents: &[u32]) -> Vec<u8> {
    contents
        .iter()
        .filter_map(|&ch| char::from_u32(ch))
        .collect::<String>()
        .into_bytes()
}
pub fn filter_contents(
    contents: &[u32],
    mut accept: impl FnMut(u32, usize) -> Option<u32>,
) -> Vec<u32> {
    let mut result = Vec::with_capacity(contents.len());
    let mut i = 0;
    while i < contents.len() {
        let ch = contents[i];
        if ch > 175 {
            i += 2;
            continue;
        }
        if let Some(ch) = accept(ch, i) {
            result.push(ch);
        }
        i += 1;
    }
    result
}
