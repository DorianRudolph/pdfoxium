// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxge/cfx_textrenderoptions.h`.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum AliasingType {
    Aliasing,
    AntiAliasing,
    Lcd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CfxTextRenderOptions {
    pub aliasing_type: AliasingType,
    pub font_is_cid: bool,
    // Retained for source correspondence. The Rust AGG renderer never uses
    // platform text drawing, so FPDF_NO_NATIVETEXT does not change its output.
    pub native_text: bool,
}

impl Default for CfxTextRenderOptions {
    fn default() -> Self {
        Self::new(AliasingType::AntiAliasing)
    }
}

impl CfxTextRenderOptions {
    pub const fn new(aliasing_type: AliasingType) -> Self {
        Self {
            aliasing_type,
            font_is_cid: false,
            native_text: true,
        }
    }

    pub const fn is_smooth(&self) -> bool {
        matches!(
            self.aliasing_type,
            AliasingType::AntiAliasing | AliasingType::Lcd
        )
    }
}
