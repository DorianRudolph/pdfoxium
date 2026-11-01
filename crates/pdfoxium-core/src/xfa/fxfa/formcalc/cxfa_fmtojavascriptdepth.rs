// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Per-translation counterpart of `cxfa_fmtojavascriptdepth.{h,cpp}`.
//! Depth belongs to the emitter's explicit work stack rather than a process
//! global counter. Separate and nested translations cannot disturb each other.
#[derive(Clone, Copy, Debug, Default)]
pub struct CxfaFmToJavaScriptDepth {
    depth: usize,
}
impl CxfaFmToJavaScriptDepth {
    pub const MAX_DEPTH: usize = 5000;
    pub fn root() -> Self {
        Self::default()
    }
    pub fn descend(self) -> Self {
        Self {
            depth: self.depth.saturating_add(1),
        }
    }
    pub fn is_within_max_depth(self) -> bool {
        self.depth <= Self::MAX_DEPTH
    }
}
