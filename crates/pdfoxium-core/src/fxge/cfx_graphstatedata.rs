// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxge/cfx_graphstatedata.{h,cpp}`. The retainable
//! subclass uses the wrapper in `cfx_graphstate`.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LineCap(pub u8);

impl LineCap {
    pub const BUTT: Self = Self(0);
    pub const ROUND: Self = Self(1);
    pub const SQUARE: Self = Self(2);

    pub const fn from_pdf_integer(value: i32) -> Self {
        Self(value as u8)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LineJoin(pub u8);

impl LineJoin {
    pub const MITER: Self = Self(0);
    pub const ROUND: Self = Self(1);
    pub const BEVEL: Self = Self(2);

    pub const fn from_pdf_integer(value: i32) -> Self {
        Self(value as u8)
    }
}

#[derive(Clone, Debug)]
pub struct CfxGraphStateData {
    line_cap: LineCap,
    line_join: LineJoin,
    miter_limit: f32,
    line_width: f32,
    dash_phase: f32,
    dash_array: Vec<f32>,
}

impl Default for CfxGraphStateData {
    fn default() -> Self {
        Self {
            line_cap: LineCap::BUTT,
            line_join: LineJoin::MITER,
            miter_limit: 10.0,
            line_width: 1.0,
            dash_phase: 0.0,
            dash_array: Vec::new(),
        }
    }
}

impl CfxGraphStateData {
    pub fn line_cap(&self) -> LineCap {
        self.line_cap
    }

    pub fn set_line_cap(&mut self, line_cap: LineCap) {
        self.line_cap = line_cap;
    }

    pub fn line_join(&self) -> LineJoin {
        self.line_join
    }

    pub fn set_line_join(&mut self, line_join: LineJoin) {
        self.line_join = line_join;
    }

    pub fn miter_limit(&self) -> f32 {
        self.miter_limit
    }

    pub fn set_miter_limit(&mut self, miter_limit: f32) {
        self.miter_limit = miter_limit;
    }

    pub fn line_width(&self) -> f32 {
        self.line_width
    }

    pub fn set_line_width(&mut self, line_width: f32) {
        self.line_width = line_width;
    }

    pub fn dash_phase(&self) -> f32 {
        self.dash_phase
    }

    pub fn set_dash_phase(&mut self, dash_phase: f32) {
        self.dash_phase = dash_phase;
    }

    pub fn dash_array(&self) -> &[f32] {
        &self.dash_array
    }

    pub fn set_dash_array(&mut self, dash_array: Vec<f32>) {
        self.dash_array = dash_array;
    }
}
