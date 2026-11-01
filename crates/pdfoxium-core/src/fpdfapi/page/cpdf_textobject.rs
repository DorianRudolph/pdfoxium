// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Text object positions from `core/fpdfapi/page/cpdf_textobject.{h,cpp}`.
//! Retained font metrics, segment kerning, bounds, and rendering snapshots.
//! Includes text replacement, render-mode editing and word queries. Text-page
//! extraction consumes the same stored character positions and font metrics.

use super::cpdf_pageobject::{CpdfPageObject, PageObject, PageObjectType};
use super::cpdf_textstate::{text_rendering_mode_is_stroke_mode, TextRenderingMode};
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};

pub struct CpdfTextObject {
    base: CpdfPageObject,
    char_codes: Vec<u32>,
    char_kernings: Vec<f32>,
    char_positions: Vec<f32>,
    vertical: bool,
    position: CfxPointF,
}

impl CpdfTextObject {
    pub fn new(content_stream: i32) -> Self {
        Self {
            base: CpdfPageObject::new(content_stream),
            char_codes: Vec::new(),
            char_kernings: Vec::new(),
            char_positions: Vec::new(),
            vertical: false,
            position: CfxPointF::default(),
        }
    }

    pub fn char_codes(&self) -> &[u32] {
        &self.char_codes
    }

    pub fn font(&self) -> Option<CpdfFont> {
        self.base.text_state().get_font().cloned()
    }

    pub fn font_size(&self) -> f32 {
        self.base.text_state().get_font_size()
    }

    pub fn text_render_mode(&self) -> TextRenderingMode {
        self.base.text_state().get_text_mode()
    }

    pub fn set_text_render_mode(&mut self, mode: TextRenderingMode) {
        self.base.mutable_text_state().set_text_mode(mode);
        self.base.set_dirty(true);
    }

    pub fn set_text(&mut self, text: &[u8]) {
        self.set_segments_from_iter(std::iter::once(text), &[]);
        self.calc_position_data_internal();
        self.base.set_dirty(true);
    }

    /// `CPDF_TextObject::SetAbsolutePositions()`: positions are text-space
    /// offsets before applying the text matrix, one for each subsequent glyph.
    pub fn set_absolute_positions(&mut self, positions: &[f32]) -> bool {
        if self.char_codes.len() <= 1 || positions.len() != self.char_codes.len() - 1 {
            return false;
        }
        let font_size = self.font_size();
        if font_size == 0.0 {
            return false;
        }
        let Some(font) = self.font() else {
            return false;
        };
        let char_space = self.base.text_state().get_char_space();
        let word_space = self.base.text_state().get_word_space();
        let mut current_position = 0.0;
        for (i, &next_position) in positions.iter().enumerate() {
            let code = self.char_codes[i];
            let width = match &font {
                CpdfFont::Cid(cid) if cid.metrics.borrow().is_vertical() => {
                    let metrics = cid.metrics.borrow();
                    i32::from(metrics.get_vert_width(metrics.cid_from_char_code(code)))
                }
                _ => font.get_char_width(code),
            };
            current_position += width as f32 * font_size / 1000.0;
            let word_space_applies = code == 32
                && match &font {
                    CpdfFont::Cid(cid) => cid.metrics.borrow().get_char_size(32) == 1,
                    _ => true,
                };
            if word_space_applies {
                current_position += word_space;
            }
            current_position += char_space;
            self.char_kernings[i] = (current_position - next_position) / (font_size / 1000.0);
            current_position = next_position;
        }
        *self.char_kernings.last_mut().expect("multiple glyphs") = 0.0;
        self.calc_position_data_internal();
        true
    }

    pub fn char_kernings(&self) -> &[f32] {
        &self.char_kernings
    }

    pub fn char_positions(&self) -> &[f32] {
        &self.char_positions
    }

    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    pub fn position(&self) -> CfxPointF {
        self.position
    }

    pub fn set_position(&mut self, position: CfxPointF) {
        self.position = position;
    }

    pub fn set_text_matrix(&mut self, matrix: CfxMatrix) {
        *self.base.mutable_text_state().get_mutable_matrix() =
            [matrix.a, matrix.c, matrix.b, matrix.d];
        self.position = CfxPointF::new(matrix.e, matrix.f);
        self.calc_position_data_internal();
    }

    pub fn text_matrix(&self) -> CfxMatrix {
        let matrix = self.base.text_state().get_matrix();
        CfxMatrix::new(
            matrix[0],
            matrix[2],
            matrix[1],
            matrix[3],
            self.position.x,
            self.position.y,
        )
    }

    pub fn set_segments(&mut self, strings: &[Vec<u8>], kernings: &[f32]) {
        self.set_segments_from_iter(strings.iter().map(Vec::as_slice), kernings);
    }

    fn set_segments_from_iter<'a>(
        &mut self,
        strings: impl ExactSizeIterator<Item = &'a [u8]> + Clone,
        kernings: &[f32],
    ) {
        let segments_count = strings.len();
        assert!(segments_count != 0);
        self.char_codes.clear();
        self.char_kernings.clear();
        self.char_positions.clear();
        let font = self.base.text_state().get_font().expect("text font");
        let count = strings.clone().map(|s| font.count_char(s)).sum();
        assert!(count > 0);
        self.char_codes.resize(count, 0);
        self.char_kernings.resize(count, 0.0);
        self.char_positions.resize(count, 0.0);
        let mut index = 0;
        for (segment_index, segment) in strings.enumerate() {
            let mut offset = 0;
            while offset < segment.len() {
                self.char_codes[index] = font.get_next_char(segment, &mut offset);
                index += 1;
            }
            if segment_index + 1 != segments_count {
                self.char_kernings[index - 1] = kernings[segment_index];
            }
        }
    }

    pub fn calc_position_data(&mut self, horz_scale: f32) -> CfxPointF {
        let position = self.calc_position_data_internal();
        if self.vertical {
            CfxPointF::new(0.0, position)
        } else {
            CfxPointF::new(position * horz_scale, 0.0)
        }
    }

    fn calc_position_data_internal(&mut self) -> f32 {
        // Retain the font without holding a text-state or metrics borrow while
        // Type 3 glyph loading recursively parses content.
        let font = self
            .base
            .text_state()
            .get_font()
            .expect("text font")
            .clone();
        let cid = match &font {
            CpdfFont::Cid(font) => Some(font),
            _ => None,
        };
        self.vertical = cid.is_some_and(|font| font.metrics.borrow().is_vertical());
        let font_size = self.base.text_state().get_font_size();
        let char_space = self.base.text_state().get_char_space();
        let word_space = self.base.text_state().get_word_space();
        let mut current_position = 0.0;
        let (mut min_x, mut max_x, mut min_y, mut max_y) =
            (10000.0_f32, -10000.0_f32, 10000.0_f32, -10000.0_f32);
        for (i, &code) in self.char_codes.iter().enumerate() {
            self.char_positions[i] = current_position;
            let rect = font.get_char_bbox(code);
            let width;
            if self.vertical {
                let metrics = cid.unwrap().metrics.borrow();
                let cid = metrics.cid_from_char_code(code);
                let origin = metrics.get_vert_origin(cid);
                // Widen offsets to define overflow of malformed glyph bounds.
                let left = (i64::from(rect.left) - i64::from(origin.x)) as f32;
                let right = (i64::from(rect.right) - i64::from(origin.x)) as f32;
                let top = (i64::from(rect.top) - i64::from(origin.y)) as f32;
                let bottom = (i64::from(rect.bottom) - i64::from(origin.y)) as f32;
                min_x = min_x.min(left).min(right);
                max_x = max_x.max(left).max(right);
                let top = current_position + top * font_size / 1000.0;
                let bottom = current_position + bottom * font_size / 1000.0;
                min_y = min_y.min(top).min(bottom);
                max_y = max_y.max(top).max(bottom);
                width = f32::from(metrics.get_vert_width(cid)) * font_size / 1000.0;
            } else {
                min_y = min_y.min(rect.top as f32).min(rect.bottom as f32);
                max_y = max_y.max(rect.top as f32).max(rect.bottom as f32);
                let left = current_position + rect.left as f32 * font_size / 1000.0;
                let right = current_position + rect.right as f32 * font_size / 1000.0;
                min_x = min_x.min(left).min(right);
                max_x = max_x.max(left).max(right);
                width = font.get_char_width(code) as f32 * font_size / 1000.0;
            }
            current_position += width;
            if code == 32 && cid.is_none_or(|font| font.metrics.borrow().get_char_size(32) == 1) {
                current_position += word_space;
            }
            current_position += char_space;
            current_position -= self.char_kernings[i] * font_size / 1000.0;
        }
        if self.vertical {
            min_x = min_x * font_size / 1000.0;
            max_x = max_x * font_size / 1000.0;
        } else {
            min_y = min_y * font_size / 1000.0;
            max_y = max_y * font_size / 1000.0;
        }
        self.base
            .set_original_rect(CfxFloatRect::new(min_x, min_y, max_x, max_y));
        let mut rect = self
            .text_matrix()
            .transform_float_rect(self.base.original_rect());
        if text_rendering_mode_is_stroke_mode(self.base.text_state().get_text_mode()) {
            let half_width = self.base.graph_state().get_line_width() / 2.0;
            rect.inflate_xy(half_width, half_width);
        }
        self.base.set_rect(rect);
        current_position
    }
}

impl PageObject for CpdfTextObject {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn object_type(&self) -> PageObjectType {
        PageObjectType::Text
    }
    fn transform(&mut self, matrix: CfxMatrix) {
        let transformed = self.text_matrix() * matrix;
        *self.base.mutable_text_state().get_mutable_matrix() =
            [transformed.a, transformed.c, transformed.b, transformed.d];
        self.position = CfxPointF::new(transformed.e, transformed.f);
        self.calc_position_data_internal();
        self.base.set_dirty(true);
    }
    fn base(&self) -> &CpdfPageObject {
        &self.base
    }
    fn base_mut(&mut self) -> &mut CpdfPageObject {
        &mut self.base
    }
}

impl Clone for CpdfTextObject {
    fn clone(&self) -> Self {
        let mut object = Self::new(CpdfPageObject::NO_CONTENT_STREAM);
        object.base.copy_data(&self.base);
        object.char_codes = self.char_codes.clone();
        object.char_kernings = self.char_kernings.clone();
        object.char_positions = self.char_positions.clone();
        object.vertical = self.vertical;
        object.position = self.position;
        object
    }
}

#[cfg(test)]
mod editing_tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
    use crate::fxge::cfx_fontmapper::CfxFontMapper;

    fn text_object(font: CpdfFont) -> CpdfTextObject {
        let mut object = CpdfTextObject::new(CpdfPageObject::NO_CONTENT_STREAM);
        let dictionary = font.font_dict();
        object
            .base_mut()
            .mutable_text_state()
            .set_font(Some(font), dictionary);
        object.base_mut().mutable_text_state().set_font_size(12.0);
        object
    }

    #[test]
    fn set_text_matrix_replaces_components_and_recalculates_without_setting_dirty() {
        let data = CpdfDocPageData::new();
        let font = CpdfFont::Simple(
            data.get_stock_font(b"Helvetica", &mut CfxFontMapper::new())
                .unwrap(),
        );
        let mut object = text_object(font);
        object.set_text(b"AB");
        object.base_mut().set_dirty(false);
        let matrix = CfxMatrix::new(2., 0., 0., 3., 40., 50.);
        object.set_text_matrix(matrix);
        assert_eq!(object.text_matrix(), matrix);
        assert_eq!(object.position(), CfxPointF::new(40., 50.));
        assert_eq!(
            object.base().rect(),
            matrix.transform_float_rect(object.base().original_rect())
        );
        assert!(!object.base().is_dirty());
    }

    #[test]
    fn replacement_resets_segment_kerning_and_recalculates_bounds() {
        let data = CpdfDocPageData::new();
        let font = CpdfFont::Simple(
            data.get_stock_font(b"Helvetica", &mut CfxFontMapper::new())
                .unwrap(),
        );
        let mut object = text_object(font.clone());
        object.set_segments(&[b"AB".to_vec(), b"C".to_vec()], &[120.0]);
        object.calc_position_data(1.0);
        assert_eq!(object.char_kernings(), &[0.0, 120.0, 0.0]);
        assert!(!object.base().is_dirty());

        object.set_text(b"C");
        assert_eq!(object.char_codes(), &[u32::from(b'C')]);
        assert_eq!(object.char_kernings(), &[0.0]);
        assert_eq!(object.char_positions(), &[0.0]);
        assert!(object.base().is_dirty());
        let mut fresh = text_object(font);
        fresh.set_text(b"C");
        assert_eq!(object.base().rect(), fresh.base().rect());
        assert_eq!(object.base().original_rect(), fresh.base().original_rect());

        object.base_mut().set_dirty(false);
        let rect = object.base().rect();
        object.set_text_render_mode(TextRenderingMode::StrokeClip);
        assert_eq!(object.text_render_mode(), TextRenderingMode::StrokeClip);
        assert!(object.base().is_dirty());
        // SetTextRenderMode changes the state without recalculating position.
        assert_eq!(object.base().rect(), rect);
    }

    #[test]
    #[should_panic]
    fn replacement_preserves_upstream_nonempty_character_check() {
        let data = CpdfDocPageData::new();
        let font = CpdfFont::Simple(
            data.get_stock_font(b"Helvetica", &mut CfxFontMapper::new())
                .unwrap(),
        );
        text_object(font).set_text(b"");
    }
}
