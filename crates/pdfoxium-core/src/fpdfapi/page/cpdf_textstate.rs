// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Text graphics state from `core/fpdfapi/page/cpdf_textstate.{h,cpp}`.
//! Copy-on-write state retains handles to document-cached PDF font objects.

use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fxcrt::shared_copy_on_write::SharedCopyOnWrite;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum TextRenderingMode {
    Unknown = -1,
    Fill = 0,
    Stroke = 1,
    FillStroke = 2,
    Invisible = 3,
    FillClip = 4,
    StrokeClip = 5,
    FillStrokeClip = 6,
    Clip = 7,
}

pub fn set_text_rendering_mode_from_int(value: i32) -> Option<TextRenderingMode> {
    Some(match value {
        0 => TextRenderingMode::Fill,
        1 => TextRenderingMode::Stroke,
        2 => TextRenderingMode::FillStroke,
        3 => TextRenderingMode::Invisible,
        4 => TextRenderingMode::FillClip,
        5 => TextRenderingMode::StrokeClip,
        6 => TextRenderingMode::FillStrokeClip,
        7 => TextRenderingMode::Clip,
        _ => return None,
    })
}

pub fn text_rendering_mode_is_clip_mode(mode: TextRenderingMode) -> bool {
    matches!(
        mode,
        TextRenderingMode::FillClip
            | TextRenderingMode::StrokeClip
            | TextRenderingMode::FillStrokeClip
            | TextRenderingMode::Clip
    )
}

pub fn text_rendering_mode_is_stroke_mode(mode: TextRenderingMode) -> bool {
    matches!(
        mode,
        TextRenderingMode::Stroke
            | TextRenderingMode::FillStroke
            | TextRenderingMode::StrokeClip
            | TextRenderingMode::FillStrokeClip
    )
}

#[derive(Clone)]
struct TextData {
    font_dict: Option<ObjectRef>,
    font: Option<CpdfFont>,
    font_size: f32,
    char_space: f32,
    word_space: f32,
    text_rendering_mode: TextRenderingMode,
    matrix: [f32; 4],
    ctm: [f32; 4],
}

impl Default for TextData {
    fn default() -> Self {
        Self {
            font_dict: None,
            font: None,
            font_size: 1.0,
            char_space: 0.0,
            word_space: 0.0,
            text_rendering_mode: TextRenderingMode::Fill,
            matrix: [1.0, 0.0, 0.0, 1.0],
            ctm: [1.0, 0.0, 0.0, 1.0],
        }
    }
}

#[derive(Clone, Default)]
pub struct CpdfTextState {
    data: SharedCopyOnWrite<TextData>,
}

impl CpdfTextState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn emplace(&mut self) {
        self.data.emplace(TextData::default());
    }
    fn data(&self) -> &TextData {
        self.data.get_object().expect("text state has no reference")
    }
    fn private_data(&mut self) -> &mut TextData {
        self.data.get_private_copy(TextData::default)
    }

    pub fn get_font_dict(&self) -> Option<ObjectRef> {
        self.data().font_dict.clone()
    }
    pub fn set_font_dict(&mut self, font_dict: Option<ObjectRef>) {
        let data = self.private_data();
        data.font_dict = font_dict;
        data.font = None;
    }
    pub fn get_font(&self) -> Option<&CpdfFont> {
        self.data().font.as_ref()
    }
    pub fn set_font(&mut self, font: Option<CpdfFont>, font_dict: Option<ObjectRef>) {
        let data = self.private_data();
        data.font = font;
        data.font_dict = font_dict;
    }
    pub fn get_font_size(&self) -> f32 {
        self.data().font_size
    }
    pub fn set_font_size(&mut self, size: f32) {
        if !self.data.is_some() || self.get_font_size() != size {
            self.private_data().font_size = size;
        }
    }
    pub fn get_matrix(&self) -> &[f32; 4] {
        &self.data().matrix
    }
    pub fn get_mutable_matrix(&mut self) -> &mut [f32; 4] {
        &mut self.private_data().matrix
    }
    pub fn get_char_space(&self) -> f32 {
        self.data().char_space
    }
    pub fn set_char_space(&mut self, space: f32) {
        if !self.data.is_some() || self.get_char_space() != space {
            self.private_data().char_space = space;
        }
    }
    pub fn get_word_space(&self) -> f32 {
        self.data().word_space
    }
    pub fn set_word_space(&mut self, space: f32) {
        if !self.data.is_some() || self.get_word_space() != space {
            self.private_data().word_space = space;
        }
    }
    pub fn get_font_size_h(&self) -> f32 {
        (self.data().matrix[0].hypot(self.data().matrix[2]) * self.data().font_size).abs()
    }
    pub fn get_text_mode(&self) -> TextRenderingMode {
        self.data().text_rendering_mode
    }
    pub fn set_text_mode(&mut self, mode: TextRenderingMode) {
        if !self.data.is_some() || self.get_text_mode() != mode {
            self.private_data().text_rendering_mode = mode;
        }
    }
    pub fn get_ctm(&self) -> &[f32; 4] {
        &self.data().ctm
    }
    pub fn get_mutable_ctm(&mut self) -> &mut [f32; 4] {
        &mut self.private_data().ctm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendering_modes_and_text_size() {
        assert_eq!(set_text_rendering_mode_from_int(-1), None);
        assert_eq!(set_text_rendering_mode_from_int(8), None);
        assert!(text_rendering_mode_is_clip_mode(
            TextRenderingMode::FillStrokeClip
        ));
        assert!(text_rendering_mode_is_stroke_mode(
            TextRenderingMode::FillStrokeClip
        ));
        assert!(!text_rendering_mode_is_stroke_mode(
            TextRenderingMode::FillClip
        ));
        let mut state = CpdfTextState::new();
        state.emplace();
        state.set_font_size(12.0);
        *state.get_mutable_matrix() = [0.0, 1.0, 2.0, 0.0];
        assert_eq!(state.get_font_size_h(), 24.0);
        let original = state.clone();
        state.set_text_mode(TextRenderingMode::Stroke);
        assert_eq!(original.get_text_mode(), TextRenderingMode::Fill);
    }
}
