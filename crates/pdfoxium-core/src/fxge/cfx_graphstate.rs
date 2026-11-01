// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxge/cfx_graphstate.{h,cpp}`.

use crate::fxcrt::shared_copy_on_write::SharedCopyOnWrite;

use super::cfx_graphstatedata::{CfxGraphStateData, LineCap, LineJoin};

#[derive(Clone, Default)]
pub struct CfxGraphState {
    reference: SharedCopyOnWrite<CfxGraphStateData>,
}

impl CfxGraphState {
    pub fn emplace(&mut self) {
        self.reference.emplace(CfxGraphStateData::default());
    }

    fn private_data(&mut self) -> &mut CfxGraphStateData {
        self.reference.get_private_copy(CfxGraphStateData::default)
    }

    pub fn set_line_dash(&mut self, dashes: Vec<f32>, phase: f32) {
        let data = self.private_data();
        data.set_dash_phase(phase);
        data.set_dash_array(dashes);
    }

    pub fn set_line_dash_phase(&mut self, phase: f32) {
        self.private_data().set_dash_phase(phase);
    }

    pub fn get_line_dash_array(&self) -> Vec<f32> {
        self.get_object()
            .map_or_else(Vec::new, |data| data.dash_array().to_vec())
    }

    pub fn get_line_dash_size(&self) -> usize {
        self.get_object().map_or(0, |data| data.dash_array().len())
    }

    pub fn get_line_dash_phase(&self) -> f32 {
        self.get_object().map_or(1.0, CfxGraphStateData::dash_phase)
    }

    pub fn get_line_width(&self) -> f32 {
        self.get_object().map_or(1.0, CfxGraphStateData::line_width)
    }

    pub fn set_line_width(&mut self, width: f32) {
        self.private_data().set_line_width(width);
    }

    pub fn get_line_cap(&self) -> LineCap {
        self.get_object()
            .map_or(LineCap::BUTT, CfxGraphStateData::line_cap)
    }

    pub fn set_line_cap(&mut self, cap: LineCap) {
        self.private_data().set_line_cap(cap);
    }

    pub fn get_line_join(&self) -> LineJoin {
        self.get_object()
            .map_or(LineJoin::MITER, CfxGraphStateData::line_join)
    }

    pub fn set_line_join(&mut self, join: LineJoin) {
        self.private_data().set_line_join(join);
    }

    pub fn get_miter_limit(&self) -> f32 {
        self.get_object()
            .map_or(10.0, CfxGraphStateData::miter_limit)
    }

    pub fn set_miter_limit(&mut self, limit: f32) {
        self.private_data().set_miter_limit(limit);
    }

    pub fn get_object(&self) -> Option<&CfxGraphStateData> {
        self.reference.get_object()
    }
}

#[cfg(test)]
mod tests {
    use super::{CfxGraphState, LineCap, LineJoin};

    #[test]
    fn absent_defaults_and_copy_on_write() {
        let mut a = CfxGraphState::default();
        assert!(a.get_object().is_none());
        assert_eq!(a.get_line_dash_phase(), 1.0);
        assert_eq!(a.get_line_width(), 1.0);
        assert_eq!(a.get_line_cap(), LineCap::BUTT);
        assert_eq!(a.get_line_join(), LineJoin::MITER);
        assert_eq!(a.get_miter_limit(), 10.0);
        a.emplace();
        assert_eq!(a.get_line_dash_phase(), 0.0);
        a.set_line_dash(vec![3.0, 1.0], 2.0);
        let mut b = a.clone();
        b.set_line_cap(LineCap::ROUND);
        b.set_line_join(LineJoin::BEVEL);
        b.set_line_width(4.0);
        assert_eq!(a.get_line_cap(), LineCap::BUTT);
        assert_eq!(a.get_line_dash_array(), [3.0, 1.0]);
        assert_eq!(b.get_line_dash_size(), 2);
        assert_eq!(b.get_line_dash_phase(), 2.0);
    }
}
