// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_basedcs.{h,cpp}`.

use super::cpdf_colorspace::{ColorSpaceFamily, ColorSpaceRef, CpdfColorSpaceData};

pub struct CpdfBasedCs {
    data: CpdfColorSpaceData,
    base_cs: Option<ColorSpaceRef>,
}

impl CpdfBasedCs {
    pub fn new(family: ColorSpaceFamily) -> Self {
        Self {
            data: CpdfColorSpaceData::new(family, 0),
            base_cs: None,
        }
    }

    pub fn data(&self) -> &CpdfColorSpaceData {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut CpdfColorSpaceData {
        &mut self.data
    }

    pub fn base_cs(&self) -> Option<&ColorSpaceRef> {
        self.base_cs.as_ref()
    }

    pub fn set_base_cs(&mut self, base_cs: Option<ColorSpaceRef>) {
        self.base_cs = base_cs;
    }

    pub fn enable_std_conversion(&self, enabled: bool) {
        self.data.enable_std_conversion(enabled);
        if let Some(base_cs) = &self.base_cs {
            base_cs.enable_std_conversion(enabled);
        }
    }
}
