// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_graphicstates.{h,cpp}`.

use super::cpdf_clippath::CpdfClipPath;
use super::cpdf_colorstate::CpdfColorState;
use super::cpdf_generalstate::CpdfGeneralState;
use super::cpdf_textstate::CpdfTextState;
use crate::fxge::cfx_graphstate::CfxGraphState;

#[derive(Clone, Default)]
pub struct CpdfGraphicStates {
    clip_path: CpdfClipPath,
    graph_state: CfxGraphState,
    color_state: CpdfColorState,
    text_state: CpdfTextState,
    general_state: CpdfGeneralState,
}

impl CpdfGraphicStates {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_default_states(&mut self) {
        self.color_state.emplace();
        self.color_state.set_default();
    }

    pub fn clip_path(&self) -> &CpdfClipPath {
        &self.clip_path
    }
    pub fn mutable_clip_path(&mut self) -> &mut CpdfClipPath {
        &mut self.clip_path
    }
    pub fn graph_state(&self) -> &CfxGraphState {
        &self.graph_state
    }
    pub fn mutable_graph_state(&mut self) -> &mut CfxGraphState {
        &mut self.graph_state
    }
    pub fn color_state(&self) -> &CpdfColorState {
        &self.color_state
    }
    pub fn mutable_color_state(&mut self) -> &mut CpdfColorState {
        &mut self.color_state
    }
    pub fn text_state(&self) -> &CpdfTextState {
        &self.text_state
    }
    pub fn mutable_text_state(&mut self) -> &mut CpdfTextState {
        &mut self.text_state
    }
    pub fn general_state(&self) -> &CpdfGeneralState {
        &self.general_state
    }
    pub fn mutable_general_state(&mut self) -> &mut CpdfGeneralState {
        &mut self.general_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_only_initialize_color_state() {
        let mut states = CpdfGraphicStates::new();
        states.mutable_graph_state().set_line_width(3.0);
        states.set_default_states();
        assert!(states.color_state().has_fill_color());
        assert_eq!(states.graph_state().get_line_width(), 3.0);
        let mut copy = states.clone();
        copy.mutable_color_state().set_fill_color(None, vec![0.5]);
        assert_eq!(states.color_state().get_fill_color_ref(), 0);
        assert_eq!(copy.color_state().get_fill_color_ref(), 0x0080_8080);
    }
}
