// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxge/dib/scanlinecomposer_iface.h`.

use super::fx_dib::FxdibFormat;

pub trait ScanlineComposerIface {
    fn compose_scanline(&mut self, line: i32, scanline: &[u8]);
    /// The source format must be more than one bit per pixel.
    fn set_info(
        &mut self,
        width: i32,
        height: i32,
        src_format: FxdibFormat,
        src_palette: Vec<u32>,
    ) -> bool;
}
