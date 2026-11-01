// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Sample tables and color conversion from `cpdf_transferfunc.{h,cpp}`.

use super::cpdf_transferfuncdib::CpdfTransferFuncDib;
use crate::fxge::dib::fx_dib::{
    fxsys_bgr, fxsys_get_b_value, fxsys_get_g_value, fxsys_get_r_value, FxColorRef,
};
use std::rc::Rc;

pub const CHANNEL_SAMPLE_SIZE: usize = 256;

pub struct CpdfTransferFunc {
    identity: bool,
    samples_r: [u8; CHANNEL_SAMPLE_SIZE],
    samples_g: [u8; CHANNEL_SAMPLE_SIZE],
    samples_b: [u8; CHANNEL_SAMPLE_SIZE],
}

impl CpdfTransferFunc {
    pub fn new(
        identity: bool,
        samples_r: [u8; CHANNEL_SAMPLE_SIZE],
        samples_g: [u8; CHANNEL_SAMPLE_SIZE],
        samples_b: [u8; CHANNEL_SAMPLE_SIZE],
    ) -> Self {
        Self {
            identity,
            samples_r,
            samples_g,
            samples_b,
        }
    }

    pub fn identity(&self) -> bool {
        self.identity
    }
    pub fn samples_r(&self) -> &[u8; CHANNEL_SAMPLE_SIZE] {
        &self.samples_r
    }
    pub fn samples_g(&self) -> &[u8; CHANNEL_SAMPLE_SIZE] {
        &self.samples_g
    }
    pub fn samples_b(&self) -> &[u8; CHANNEL_SAMPLE_SIZE] {
        &self.samples_b
    }

    pub fn translate_color(&self, color: FxColorRef) -> FxColorRef {
        fxsys_bgr(
            self.samples_b[fxsys_get_b_value(color) as usize],
            self.samples_g[fxsys_get_g_value(color) as usize],
            self.samples_r[fxsys_get_r_value(color) as usize],
        )
    }

    pub fn translate_image<S: crate::fxge::dib::cstretchengine::StretchSource>(
        self: &Rc<Self>,
        source: Rc<S>,
    ) -> Option<CpdfTransferFuncDib<S>> {
        CpdfTransferFuncDib::new(source, Rc::clone(self))
    }
}
