// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Page content graphics and text state from `core/fpdfapi/page/cpdf_allstates.{h,cpp}`.
//! Font resolution and background-alpha notification are supplied by the
//! content parser.

use super::cpdf_clippath::CpdfClipPath;
use super::cpdf_colorstate::CpdfColorState;
use super::cpdf_generalstate::CpdfGeneralState;
use super::cpdf_graphicstates::CpdfGraphicStates;
use super::cpdf_textstate::CpdfTextState;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use crate::fxge::cfx_graphstate::CfxGraphState;
use crate::fxge::cfx_graphstatedata::{LineCap, LineJoin};
use crate::fxge::dib::fx_dib::BlendMode;

#[derive(Clone)]
pub struct CpdfAllStates {
    graphic_states: CpdfGraphicStates,
    text_matrix: CfxMatrix,
    ctm: CfxMatrix,
    parent_matrix: CfxMatrix,
    text_pos: CfxPointF,
    text_line_pos: CfxPointF,
    text_leading: f32,
    text_rise: f32,
    text_horz_scale: f32,
}

impl Default for CpdfAllStates {
    fn default() -> Self {
        Self {
            graphic_states: CpdfGraphicStates::default(),
            text_matrix: CfxMatrix::default(),
            ctm: CfxMatrix::default(),
            parent_matrix: CfxMatrix::default(),
            text_pos: CfxPointF::default(),
            text_line_pos: CfxPointF::default(),
            text_leading: 0.0,
            text_rise: 0.0,
            text_horz_scale: 1.0,
        }
    }
}

impl CpdfAllStates {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_default_states(&mut self) {
        self.graphic_states.set_default_states();
    }
    pub fn graphic_states(&self) -> &CpdfGraphicStates {
        &self.graphic_states
    }
    pub fn mutable_graphic_states(&mut self) -> &mut CpdfGraphicStates {
        &mut self.graphic_states
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
    pub fn text_matrix(&self) -> CfxMatrix {
        self.text_matrix
    }
    pub fn set_text_matrix(&mut self, matrix: CfxMatrix) {
        self.text_matrix = matrix;
    }
    pub fn current_transformation_matrix(&self) -> CfxMatrix {
        self.ctm
    }
    pub fn set_current_transformation_matrix(&mut self, matrix: CfxMatrix) {
        self.ctm = matrix;
    }
    pub fn prepend_to_current_transformation_matrix(&mut self, matrix: CfxMatrix) {
        self.ctm = matrix * self.ctm;
    }
    pub fn parent_matrix(&self) -> CfxMatrix {
        self.parent_matrix
    }
    pub fn set_parent_matrix(&mut self, matrix: CfxMatrix) {
        self.parent_matrix = matrix;
    }
    pub fn set_text_leading(&mut self, value: f32) {
        self.text_leading = value;
    }
    pub fn set_text_rise(&mut self, value: f32) {
        self.text_rise = value;
    }
    pub fn text_horz_scale(&self) -> f32 {
        self.text_horz_scale
    }
    pub fn set_text_horz_scale(&mut self, value: f32) {
        self.text_horz_scale = value;
    }

    pub fn set_line_dash(&mut self, array: &CpdfArray, phase: f32) {
        let dashes = (0..array.size())
            .map(|index| array.get_float_at(index))
            .collect();
        self.graphic_states
            .mutable_graph_state()
            .set_line_dash(dashes, phase);
    }

    /// Returns whether the page holder needs background alpha for a blend mode.
    /// The resolver supplies the retained font and dictionary selected by the parser.
    pub fn process_ext_gs(
        &mut self,
        gs: &CpdfDictionary,
        mut resolve_font: impl FnMut(
            &[u8],
        ) -> (
            Option<crate::fpdfapi::font::cpdf_font::CpdfFont>,
            Option<ObjectRef>,
        ),
    ) -> bool {
        let mut background_alpha_needed = false;
        for key in gs.get_keys() {
            let Some(object) = gs.get_direct_object_for(&key) else {
                continue;
            };
            match key.as_slice() {
                b"LW" => self
                    .graphic_states
                    .mutable_graph_state()
                    .set_line_width(object.borrow().get_number()),
                b"LC" => self
                    .graphic_states
                    .mutable_graph_state()
                    .set_line_cap(LineCap::from_pdf_integer(object.borrow().get_integer())),
                b"LJ" => self
                    .graphic_states
                    .mutable_graph_state()
                    .set_line_join(LineJoin::from_pdf_integer(object.borrow().get_integer())),
                b"ML" => self
                    .graphic_states
                    .mutable_graph_state()
                    .set_miter_limit(object.borrow().get_number()),
                b"D" => {
                    let borrowed = object.borrow();
                    if let Some(dash) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                        if let Some(array) = dash.get_array_at(0) {
                            let borrowed_array = array.borrow();
                            if let Some(array) = borrowed_array.as_any().downcast_ref::<CpdfArray>()
                            {
                                self.set_line_dash(array, dash.get_float_at(1));
                            }
                        }
                    }
                }
                b"RI" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_render_intent(&object.borrow().get_string()),
                b"Font" => {
                    let borrowed = object.borrow();
                    if let Some(font) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                        let name = font.get_byte_string_at(0);
                        self.graphic_states
                            .mutable_text_state()
                            .set_font_size(font.get_float_at(1));
                        let (font, dict) = resolve_font(&name);
                        self.graphic_states
                            .mutable_text_state()
                            .set_font(font, dict);
                    }
                }
                b"TR" | b"TR2" => {
                    if key == b"TR" && gs.key_exist(b"TR2") {
                        continue;
                    }
                    let value = (object.borrow().object_type() != ObjectType::Name)
                        .then_some(object.clone());
                    self.graphic_states.mutable_general_state().set_tr(value);
                }
                b"BM" => {
                    let borrowed = object.borrow();
                    let mode = borrowed.as_any().downcast_ref::<CpdfArray>().map_or_else(
                        || borrowed.get_string(),
                        |array| array.get_byte_string_at(0),
                    );
                    self.graphic_states
                        .mutable_general_state()
                        .set_blend_mode(&mode);
                    background_alpha_needed |= self.graphic_states.general_state().get_blend_type()
                        as i32
                        > BlendMode::Multiply as i32;
                }
                b"SMask" => {
                    let mask = (object.borrow().object_type() == ObjectType::Dictionary)
                        .then_some(object.clone());
                    self.graphic_states
                        .mutable_general_state()
                        .set_soft_mask(mask.clone());
                    if mask.is_some() {
                        self.graphic_states
                            .mutable_general_state()
                            .set_smask_matrix(self.ctm);
                    }
                }
                b"CA" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_stroke_alpha(object.borrow().get_number().clamp(0.0, 1.0)),
                b"ca" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_fill_alpha(object.borrow().get_number().clamp(0.0, 1.0)),
                b"OP" => {
                    let value = object.borrow().get_integer() != 0;
                    self.graphic_states
                        .mutable_general_state()
                        .set_stroke_op(value);
                    if !gs.key_exist(b"op") {
                        self.graphic_states
                            .mutable_general_state()
                            .set_fill_op(value);
                    }
                }
                b"op" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_fill_op(object.borrow().get_integer() != 0),
                b"OPM" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_op_mode(object.borrow().get_integer()),
                b"BG" | b"BG2" => {
                    if key == b"BG" && gs.key_exist(b"BG2") {
                        continue;
                    }
                    self.graphic_states
                        .mutable_general_state()
                        .set_bg(Some(object));
                }
                b"UCR" | b"UCR2" => {
                    if key == b"UCR" && gs.key_exist(b"UCR2") {
                        continue;
                    }
                    self.graphic_states
                        .mutable_general_state()
                        .set_ucr(Some(object));
                }
                b"HT" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_ht(Some(object)),
                b"FL" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_flatness(object.borrow().get_number()),
                b"SM" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_smoothness(object.borrow().get_number()),
                b"SA" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_stroke_adjust(object.borrow().get_integer() != 0),
                b"AIS" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_alpha_source(object.borrow().get_integer() != 0),
                b"TK" => self
                    .graphic_states
                    .mutable_general_state()
                    .set_text_knockout(object.borrow().get_integer() != 0),
                _ => {}
            }
        }
        background_alpha_needed
    }

    pub fn reset_text_position(&mut self) {
        self.text_line_pos = CfxPointF::default();
        self.text_pos = CfxPointF::default();
    }
    pub fn get_transformed_text_position(&self) -> CfxPointF {
        self.ctm
            .transform(self.text_matrix.transform(CfxPointF::new(
                self.text_pos.x,
                self.text_pos.y + self.text_rise,
            )))
    }
    pub fn move_text_point(&mut self, point: CfxPointF) {
        self.text_line_pos += point;
        self.text_pos = self.text_line_pos;
    }
    pub fn move_text_to_next_line(&mut self) {
        self.text_line_pos.y -= self.text_leading;
        self.text_pos = self.text_line_pos;
    }
    pub fn increment_text_position_x(&mut self, value: f32) {
        self.text_pos.x += value;
    }
    pub fn increment_text_position_y(&mut self, value: f32) {
        self.text_pos.y += value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_name::CpdfName;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn text_position_composes_matrices_and_rise() {
        let mut states = CpdfAllStates::new();
        assert_eq!(states.text_horz_scale(), 1.0);
        states.set_text_matrix(CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 4.0, 5.0));
        states.set_current_transformation_matrix(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 7.0, 11.0));
        states.move_text_point(CfxPointF::new(3.0, 4.0));
        states.set_text_rise(2.0);
        assert_eq!(
            states.get_transformed_text_position(),
            CfxPointF::new(17.0, 34.0)
        );
        states.set_text_leading(4.0);
        states.move_text_to_next_line();
        assert_eq!(
            states.get_transformed_text_position(),
            CfxPointF::new(17.0, 22.0)
        );
    }

    #[test]
    fn ext_gs_clamps_alpha_and_updates_blend_mode() {
        let mut gs = CpdfDictionary::new();
        gs.set_new_for(b"ca", CpdfScalarObject::number_from_float(1.5));
        gs.set_new_for(b"CA", CpdfScalarObject::number_from_float(-0.5));
        let mut states = CpdfAllStates::new();
        assert!(!states.process_ext_gs(&gs, |_| (None, None)));
        assert_eq!(
            states.graphic_states().general_state().get_fill_alpha(),
            1.0
        );
        assert_eq!(
            states.graphic_states().general_state().get_stroke_alpha(),
            0.0
        );
    }

    #[test]
    fn ext_gs_resolves_font_and_dash_and_marks_nontrivial_blend() {
        let mut gs = CpdfDictionary::new();
        let mut font = CpdfArray::new();
        font.append_new(CpdfName::new(None, b"F1"));
        font.append_new(CpdfScalarObject::number_from_float(12.0));
        gs.set_new_for(b"Font", font);
        let mut dashes = CpdfArray::new();
        dashes.append_new(CpdfScalarObject::number_from_float(3.0));
        dashes.append_new(CpdfScalarObject::number_from_float(1.0));
        let mut dash = CpdfArray::new();
        dash.append_new(dashes);
        dash.append_new(CpdfScalarObject::number_from_float(2.0));
        gs.set_new_for(b"D", dash);
        gs.set_new_for(b"BM", CpdfName::new(None, b"Screen"));
        let font_dict = retain_object(CpdfDictionary::new());
        let mut states = CpdfAllStates::new();
        assert!(states.process_ext_gs(&gs, |name| {
            assert_eq!(name, b"F1");
            (None, Some(font_dict.clone()))
        }));
        assert_eq!(states.graph_state().get_line_dash_array(), [3.0, 1.0]);
        assert_eq!(states.graph_state().get_line_dash_phase(), 2.0);
        assert_eq!(states.text_state().get_font_size(), 12.0);
        assert!(std::rc::Rc::ptr_eq(
            &states.text_state().get_font_dict().unwrap(),
            &font_dict
        ));
        assert_eq!(states.general_state().get_blend_type(), BlendMode::Screen);
    }

    #[test]
    fn ext_gs_prefers_tr2_and_preserves_soft_mask_ctm() {
        let mut gs = CpdfDictionary::new();
        gs.set_new_for(b"TR", CpdfScalarObject::number_from_float(0.25));
        gs.set_new_for(b"TR2", CpdfScalarObject::number_from_float(0.75));
        gs.set_new_for(b"SMask", CpdfDictionary::new());
        let mut states = CpdfAllStates::new();
        let ctm = CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 4.0, 5.0);
        states.set_current_transformation_matrix(ctm);
        assert!(!states.process_ext_gs(&gs, |_| (None, None)));
        assert_eq!(
            states
                .general_state()
                .get_tr()
                .unwrap()
                .borrow()
                .get_number(),
            0.75
        );
        assert_eq!(states.general_state().get_smask_matrix(), Some(ctm));
    }
}
