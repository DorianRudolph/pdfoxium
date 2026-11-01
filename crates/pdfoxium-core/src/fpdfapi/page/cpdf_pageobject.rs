// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Shared page object data from `core/fpdfapi/page/cpdf_pageobject.{h,cpp}`.
//! Concrete text, path, image, shading, and form objects embed this base.

use super::cpdf_clippath::CpdfClipPath;
use super::cpdf_colorstate::CpdfColorState;
use super::cpdf_contentmarks::CpdfContentMarks;
use super::cpdf_generalstate::CpdfGeneralState;
use super::cpdf_graphicstates::CpdfGraphicStates;
use super::cpdf_textstate::CpdfTextState;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, FxRect};
use crate::fxge::cfx_graphstate::CfxGraphState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum PageObjectType {
    Text = 1,
    Path = 2,
    Image = 3,
    Shading = 4,
    Form = 5,
}

pub struct CpdfPageObject {
    graphic_states: CpdfGraphicStates,
    rect: CfxFloatRect,
    original_rect: CfxFloatRect,
    original_matrix: CfxMatrix,
    content_marks: CpdfContentMarks,
    dirty: bool,
    matrix_dirty: bool,
    is_active: bool,
    content_stream: i32,
    resource_name: Vec<u8>,
}

impl CpdfPageObject {
    pub const NO_CONTENT_STREAM: i32 = -1;

    pub fn new(content_stream: i32) -> Self {
        Self {
            graphic_states: CpdfGraphicStates::default(),
            rect: CfxFloatRect::default(),
            original_rect: CfxFloatRect::default(),
            original_matrix: CfxMatrix::default(),
            content_marks: CpdfContentMarks::default(),
            dirty: false,
            matrix_dirty: false,
            is_active: true,
            content_stream,
            resource_name: Vec::new(),
        }
    }
    pub fn set_dirty(&mut self, value: bool) {
        self.dirty = value;
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty || self.matrix_dirty
    }
    pub fn set_matrix_dirty(&mut self, value: bool) {
        self.matrix_dirty = value;
    }
    pub fn set_is_active(&mut self, value: bool) {
        if self.is_active != value {
            self.is_active = value;
            self.dirty = true;
        }
    }
    pub fn is_active(&self) -> bool {
        self.is_active
    }
    pub fn transform_clip_path(&mut self, matrix: CfxMatrix) {
        if self.graphic_states.clip_path().has_ref() {
            self.graphic_states.mutable_clip_path().transform(matrix);
            self.dirty = true;
        }
    }
    pub fn set_original_rect(&mut self, rect: CfxFloatRect) {
        self.original_rect = rect;
    }
    pub fn original_rect(&self) -> CfxFloatRect {
        self.original_rect
    }
    pub fn set_rect(&mut self, rect: CfxFloatRect) {
        self.rect = rect;
    }
    pub fn rect(&self) -> CfxFloatRect {
        self.rect
    }
    pub fn get_bbox(&self) -> FxRect {
        self.rect.get_outer_rect()
    }
    pub fn get_transformed_bbox(&self, matrix: CfxMatrix) -> FxRect {
        matrix.transform_float_rect(self.rect).get_outer_rect()
    }
    pub fn content_marks(&self) -> &CpdfContentMarks {
        &self.content_marks
    }
    pub fn mutable_content_marks(&mut self) -> &mut CpdfContentMarks {
        &mut self.content_marks
    }
    pub fn set_content_marks(&mut self, marks: CpdfContentMarks) {
        self.content_marks = marks;
    }
    pub fn content_stream(&self) -> i32 {
        self.content_stream
    }
    pub fn set_content_stream(&mut self, value: i32) {
        self.content_stream = value;
    }
    pub fn resource_name(&self) -> &[u8] {
        &self.resource_name
    }
    pub fn set_resource_name(&mut self, name: &[u8]) {
        self.resource_name = name.to_vec();
    }
    pub fn graphics_resource_names(&self) -> std::cell::Ref<'_, [Vec<u8>]> {
        self.graphic_states
            .general_state()
            .get_graphics_resource_names()
    }
    pub fn clip_path(&self) -> &CpdfClipPath {
        self.graphic_states.clip_path()
    }
    pub fn mutable_clip_path(&mut self) -> &mut CpdfClipPath {
        self.graphic_states.mutable_clip_path()
    }
    pub fn graph_state(&self) -> &CfxGraphState {
        self.graphic_states.graph_state()
    }
    pub fn mutable_graph_state(&mut self) -> &mut CfxGraphState {
        self.graphic_states.mutable_graph_state()
    }
    pub fn color_state(&self) -> &CpdfColorState {
        self.graphic_states.color_state()
    }
    pub fn mutable_color_state(&mut self) -> &mut CpdfColorState {
        self.graphic_states.mutable_color_state()
    }
    pub fn text_state(&self) -> &CpdfTextState {
        self.graphic_states.text_state()
    }
    pub fn mutable_text_state(&mut self) -> &mut CpdfTextState {
        self.graphic_states.mutable_text_state()
    }
    pub fn general_state(&self) -> &CpdfGeneralState {
        self.graphic_states.general_state()
    }
    pub fn mutable_general_state(&mut self) -> &mut CpdfGeneralState {
        self.graphic_states.mutable_general_state()
    }
    pub fn graphic_states(&self) -> &CpdfGraphicStates {
        &self.graphic_states
    }
    pub fn set_default_states(&mut self) {
        self.graphic_states.set_default_states();
    }
    pub fn original_matrix(&self) -> CfxMatrix {
        self.original_matrix
    }
    pub fn initialize_original_matrix(&mut self, matrix: CfxMatrix) {
        self.original_matrix = matrix;
    }
    pub fn copy_data(&mut self, source: &Self) {
        self.graphic_states = source.graphic_states.clone();
        self.rect = source.rect;
        self.dirty = true;
    }
}

/// Runtime behavior supplied by concrete page objects.
pub trait PageObject {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
    fn object_type(&self) -> PageObjectType;
    fn transform(&mut self, matrix: CfxMatrix);
    fn base(&self) -> &CpdfPageObject;
    fn base_mut(&mut self) -> &mut CpdfPageObject;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_data_only_copies_graphics_and_rect() {
        let mut source = CpdfPageObject::new(4);
        source.set_rect(CfxFloatRect::new(1.2, 2.2, 5.1, 7.1));
        source.set_original_rect(CfxFloatRect::new(10.0, 10.0, 20.0, 20.0));
        source.set_resource_name(b"Im1");
        source.mutable_graph_state().set_line_width(3.0);
        let mut target = CpdfPageObject::new(CpdfPageObject::NO_CONTENT_STREAM);
        target.copy_data(&source);
        assert!(target.is_dirty());
        assert_eq!(target.get_bbox(), source.get_bbox());
        assert_eq!(target.graph_state().get_line_width(), 3.0);
        assert_ne!(target.original_rect(), source.original_rect());
        assert!(target.resource_name().is_empty());
        assert_eq!(target.content_stream(), CpdfPageObject::NO_CONTENT_STREAM);
    }

    #[test]
    fn activation_marks_dirty_only_on_change() {
        let mut object = CpdfPageObject::new(0);
        object.set_is_active(true);
        assert!(!object.is_dirty());
        object.set_is_active(false);
        assert!(object.is_dirty());
        object.set_dirty(false);
        object.set_matrix_dirty(true);
        assert!(object.is_dirty());
    }
}
