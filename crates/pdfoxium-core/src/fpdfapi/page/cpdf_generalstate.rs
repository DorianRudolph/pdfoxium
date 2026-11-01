// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_generalstate.{h,cpp}`.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use super::cpdf_transferfunc::CpdfTransferFunc;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fxcrt::fx_coordinates::CfxMatrix;
use crate::fxcrt::shared_copy_on_write::SharedCopyOnWrite;
use crate::fxge::dib::fx_dib::BlendMode;

struct StateData {
    blend_mode: Vec<u8>,
    blend_type: BlendMode,
    soft_mask: Option<ObjectRef>,
    smask_matrix: CfxMatrix,
    stroke_alpha: f32,
    fill_alpha: f32,
    tr: Option<ObjectRef>,
    transfer_func: Option<Rc<CpdfTransferFunc>>,
    render_intent: i32,
    stroke_adjust: bool,
    alpha_source: bool,
    text_knockout: bool,
    stroke_op: bool,
    fill_op: bool,
    opmode: i32,
    bg: Option<ObjectRef>,
    ucr: Option<ObjectRef>,
    ht: Option<ObjectRef>,
    flatness: f32,
    smoothness: f32,
    graphics_resource_names: Vec<Vec<u8>>,
}

impl Default for StateData {
    fn default() -> Self {
        Self {
            blend_mode: b"Normal".to_vec(),
            blend_type: BlendMode::Normal,
            soft_mask: None,
            smask_matrix: CfxMatrix::default(),
            stroke_alpha: 1.0,
            fill_alpha: 1.0,
            tr: None,
            transfer_func: None,
            render_intent: 0,
            stroke_adjust: false,
            alpha_source: false,
            text_knockout: false,
            stroke_op: false,
            fill_op: false,
            opmode: 0,
            bg: None,
            ucr: None,
            ht: None,
            flatness: 1.0,
            smoothness: 0.0,
            graphics_resource_names: Vec::new(),
        }
    }
}

impl Clone for StateData {
    fn clone(&self) -> Self {
        Self {
            blend_mode: self.blend_mode.clone(),
            blend_type: self.blend_type,
            soft_mask: self.soft_mask.clone(),
            smask_matrix: self.smask_matrix,
            stroke_alpha: self.stroke_alpha,
            fill_alpha: self.fill_alpha,
            tr: self.tr.clone(),
            transfer_func: self.transfer_func.clone(),
            render_intent: self.render_intent,
            stroke_adjust: self.stroke_adjust,
            alpha_source: self.alpha_source,
            text_knockout: self.text_knockout,
            stroke_op: self.stroke_op,
            fill_op: self.fill_op,
            opmode: self.opmode,
            bg: self.bg.clone(),
            ucr: self.ucr.clone(),
            ht: self.ht.clone(),
            flatness: self.flatness,
            smoothness: self.smoothness,
            // PDFium's StateData copy constructor omits resource names.
            graphics_resource_names: Vec::new(),
        }
    }
}

fn render_intent_id(name: &[u8]) -> i32 {
    if name.starts_with(b"Abso") {
        1
    } else if name.starts_with(b"Satu") {
        2
    } else if name.starts_with(b"Perc") {
        3
    } else {
        0
    }
}

fn blend_type_for_mode(mode: &[u8]) -> BlendMode {
    let id = crate::fxcrt::bytestring::get_id(mode).to_be_bytes();
    match &id {
        b"Norm" | b"Comp" => BlendMode::Normal,
        b"Mult" => BlendMode::Multiply,
        b"Scre" => BlendMode::Screen,
        b"Over" => BlendMode::Overlay,
        b"Dark" => BlendMode::Darken,
        b"Ligh" => BlendMode::Lighten,
        b"Colo" => match mode.len() {
            10 => BlendMode::ColorDodge,
            9 => BlendMode::ColorBurn,
            _ => BlendMode::Color,
        },
        b"Hard" => BlendMode::HardLight,
        b"Soft" => BlendMode::SoftLight,
        b"Diff" => BlendMode::Difference,
        b"Excl" => BlendMode::Exclusion,
        b"Hue\0" => BlendMode::Hue,
        b"Satu" => BlendMode::Saturation,
        b"Lumi" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

#[derive(Clone, Default)]
pub struct CpdfGeneralState {
    data: RefCell<SharedCopyOnWrite<StateData>>,
}

impl CpdfGeneralState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn emplace(&mut self) {
        self.data.get_mut().emplace(StateData::default());
    }
    pub fn has_ref(&self) -> bool {
        self.data.borrow().is_some()
    }
    fn private_data(&self) -> RefMut<'_, StateData> {
        RefMut::map(self.data.borrow_mut(), |data| {
            data.get_private_copy(StateData::default)
        })
    }

    pub fn set_render_intent(&mut self, intent: &[u8]) {
        self.private_data().render_intent = render_intent_id(intent);
    }
    pub fn render_intent(&self) -> i32 {
        self.data
            .borrow()
            .get_object()
            .map_or(0, |data| data.render_intent)
    }

    pub fn get_blend_mode(&self) -> Vec<u8> {
        let name = match self.get_blend_type() {
            BlendMode::Normal => b"Normal".as_slice(),
            BlendMode::Multiply => b"Multiply",
            BlendMode::Screen => b"Screen",
            BlendMode::Overlay => b"Overlay",
            BlendMode::Darken => b"Darken",
            BlendMode::Lighten => b"Lighten",
            BlendMode::ColorDodge => b"ColorDodge",
            BlendMode::ColorBurn => b"ColorBurn",
            BlendMode::HardLight => b"HardLight",
            BlendMode::SoftLight => b"SoftLight",
            BlendMode::Difference => b"Difference",
            BlendMode::Exclusion => b"Exclusion",
            BlendMode::Hue => b"Hue",
            BlendMode::Saturation => b"Saturation",
            BlendMode::Color => b"Color",
            BlendMode::Luminosity => b"Luminosity",
        };
        name.to_vec()
    }

    pub fn get_blend_type(&self) -> BlendMode {
        self.data
            .borrow()
            .get_object()
            .map_or(BlendMode::Normal, |data| data.blend_type)
    }
    pub fn set_blend_type(&mut self, blend: BlendMode) {
        if self.get_blend_type() != blend {
            self.private_data().blend_type = blend;
        }
    }
    pub fn set_blend_mode(&mut self, mode: &[u8]) {
        let mut data = self.private_data();
        data.blend_mode = mode.to_vec();
        data.blend_type = blend_type_for_mode(mode);
    }

    pub fn get_fill_alpha(&self) -> f32 {
        self.data
            .borrow()
            .get_object()
            .map_or(1.0, |data| data.fill_alpha)
    }
    pub fn set_fill_alpha(&mut self, alpha: f32) {
        if self.get_fill_alpha() != alpha {
            self.private_data().fill_alpha = alpha;
        }
    }
    pub fn get_stroke_alpha(&self) -> f32 {
        self.data
            .borrow()
            .get_object()
            .map_or(1.0, |data| data.stroke_alpha)
    }
    pub fn set_stroke_alpha(&mut self, alpha: f32) {
        if self.get_stroke_alpha() != alpha {
            self.private_data().stroke_alpha = alpha;
        }
    }

    pub fn get_soft_mask(&self) -> Option<ObjectRef> {
        self.data.borrow().get_object()?.soft_mask.clone()
    }
    pub fn get_mutable_soft_mask(&self) -> Option<ObjectRef> {
        self.get_soft_mask()
    }
    pub fn set_soft_mask(&mut self, mask: Option<ObjectRef>) {
        self.private_data().soft_mask = mask;
    }
    pub fn get_smask_matrix(&self) -> Option<CfxMatrix> {
        self.data
            .borrow()
            .get_object()
            .map(|data| data.smask_matrix)
    }
    pub fn set_smask_matrix(&mut self, matrix: CfxMatrix) {
        self.private_data().smask_matrix = matrix;
    }

    pub fn get_tr(&self) -> Option<ObjectRef> {
        self.data.borrow().get_object()?.tr.clone()
    }
    pub fn set_tr(&mut self, object: Option<ObjectRef>) {
        self.private_data().tr = object;
    }
    pub fn get_transfer_func(&self) -> Option<Rc<CpdfTransferFunc>> {
        self.data.borrow().get_object()?.transfer_func.clone()
    }
    pub fn set_transfer_func(&self, function: Option<Rc<CpdfTransferFunc>>) {
        self.private_data().transfer_func = function;
    }

    pub fn get_fill_op(&self) -> bool {
        self.data
            .borrow()
            .get_object()
            .is_some_and(|data| data.fill_op)
    }
    pub fn set_fill_op(&mut self, value: bool) {
        self.private_data().fill_op = value;
    }
    pub fn get_stroke_op(&self) -> bool {
        self.data
            .borrow()
            .get_object()
            .is_some_and(|data| data.stroke_op)
    }
    pub fn set_stroke_op(&mut self, value: bool) {
        self.private_data().stroke_op = value;
    }
    pub fn get_op_mode(&self) -> i32 {
        self.data
            .borrow()
            .get_object()
            .expect("general state has no reference")
            .opmode
    }
    pub fn set_op_mode(&mut self, value: i32) {
        self.private_data().opmode = value;
    }

    pub fn set_bg(&mut self, object: Option<ObjectRef>) {
        self.private_data().bg = object;
    }
    pub fn set_ucr(&mut self, object: Option<ObjectRef>) {
        self.private_data().ucr = object;
    }
    pub fn set_ht(&mut self, object: Option<ObjectRef>) {
        self.private_data().ht = object;
    }
    pub fn set_flatness(&mut self, value: f32) {
        self.private_data().flatness = value;
    }
    pub fn set_smoothness(&mut self, value: f32) {
        self.private_data().smoothness = value;
    }
    pub fn get_stroke_adjust(&self) -> bool {
        self.data
            .borrow()
            .get_object()
            .is_some_and(|data| data.stroke_adjust)
    }
    pub fn set_stroke_adjust(&mut self, value: bool) {
        self.private_data().stroke_adjust = value;
    }
    pub fn set_alpha_source(&mut self, value: bool) {
        self.private_data().alpha_source = value;
    }
    pub fn set_text_knockout(&mut self, value: bool) {
        self.private_data().text_knockout = value;
    }

    pub fn set_graphics_resource_names(&mut self, names: Vec<Vec<u8>>) {
        self.private_data().graphics_resource_names = names;
    }
    pub fn append_graphics_resource_name(&mut self, name: Vec<u8>) {
        self.private_data().graphics_resource_names.push(name);
    }
    pub fn get_graphics_resource_names(&self) -> Ref<'_, [Vec<u8>]> {
        Ref::map(self.data.borrow(), |data| {
            data.get_object().map_or(&[] as &[Vec<u8>], |data| {
                data.graphics_resource_names.as_slice()
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_mode_and_copy_on_write_resource_names() {
        let mut state = CpdfGeneralState::new();
        assert!(!state.has_ref());
        assert_eq!(state.get_blend_type(), BlendMode::Normal);
        assert_eq!(state.get_fill_alpha(), 1.0);
        state.set_blend_mode(b"ColorDodge");
        state.set_fill_alpha(0.4);
        state.append_graphics_resource_name(b"GS0".to_vec());
        let copy = state.clone();
        state.set_stroke_alpha(0.6);
        assert_eq!(state.get_blend_type(), BlendMode::ColorDodge);
        assert_eq!(state.get_blend_mode(), b"ColorDodge");
        assert_eq!(copy.get_fill_alpha(), 0.4);
        assert_eq!(&*copy.get_graphics_resource_names(), &[b"GS0".to_vec()]);
        assert!(state.get_graphics_resource_names().is_empty());
        assert_eq!(state.get_stroke_alpha(), 0.6);
        assert_eq!(copy.get_stroke_alpha(), 1.0);
    }

    #[test]
    fn rendering_retains_transfer_function_in_the_objects_private_state() {
        let mut original = CpdfGeneralState::new();
        original.append_graphics_resource_name(b"GS0".to_vec());
        let rendered = original.clone();
        let identity = std::array::from_fn(|i| i as u8);
        let transfer = Rc::new(CpdfTransferFunc::new(true, identity, identity, identity));
        rendered.set_transfer_func(Some(transfer.clone()));
        assert!(original.get_transfer_func().is_none());
        assert_eq!(&*original.get_graphics_resource_names(), &[b"GS0".to_vec()]);
        // StateData's native copy constructor omits resource names.
        assert!(rendered.get_graphics_resource_names().is_empty());
        let copy_after_rendering = rendered.clone();
        assert!(Rc::ptr_eq(
            &transfer,
            &copy_after_rendering.get_transfer_func().unwrap()
        ));
        rendered.set_transfer_func(None);
        assert!(copy_after_rendering.get_transfer_func().is_some());
    }
}
