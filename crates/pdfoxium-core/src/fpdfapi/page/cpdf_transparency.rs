// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/page/cpdf_transparency.{h,cpp}`.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpdfTransparency {
    group: bool,
    isolated: bool,
}

impl CpdfTransparency {
    pub fn is_group(self) -> bool {
        self.group
    }
    pub fn is_isolated(self) -> bool {
        self.isolated
    }
    pub fn set_group(&mut self) {
        self.group = true;
    }
    pub fn set_isolated(&mut self) {
        self.isolated = true;
    }
}
