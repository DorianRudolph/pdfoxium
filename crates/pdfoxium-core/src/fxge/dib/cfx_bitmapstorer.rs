// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxge/dib/cfx_bitmapstorer.{h,cpp}`.

use std::cell::RefCell;
use std::rc::Rc;

use super::cfx_dibitmap::CfxDibitmap;
use super::fx_dib::FxdibFormat;
use super::scanlinecomposer_iface::ScanlineComposerIface;

#[derive(Default)]
pub struct CfxBitmapStorer {
    bitmap: Option<Rc<RefCell<CfxDibitmap>>>,
}

impl CfxBitmapStorer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bitmap(&self) -> Option<Rc<RefCell<CfxDibitmap>>> {
        self.bitmap.clone()
    }

    pub fn detach(&mut self) -> Option<Rc<RefCell<CfxDibitmap>>> {
        self.bitmap.take()
    }

    pub fn replace(&mut self, bitmap: Option<Rc<RefCell<CfxDibitmap>>>) {
        self.bitmap = bitmap;
    }
}

impl ScanlineComposerIface for CfxBitmapStorer {
    fn compose_scanline(&mut self, line: i32, scanline: &[u8]) {
        let mut bitmap = self
            .bitmap
            .as_ref()
            .expect("bitmap not initialized")
            .borrow_mut();
        let dest = bitmap.scanline_mut(line).expect("scanline out of range");
        assert!(
            dest.len() >= scanline.len(),
            "scanline exceeds bitmap pitch"
        );
        dest[..scanline.len()].copy_from_slice(scanline);
    }

    fn set_info(
        &mut self,
        width: i32,
        height: i32,
        src_format: FxdibFormat,
        src_palette: Vec<u32>,
    ) -> bool {
        assert!(!matches!(
            src_format,
            FxdibFormat::OneBppMask | FxdibFormat::OneBppRgb
        ));
        let Some(mut bitmap) = CfxDibitmap::create(width, height, src_format, 0) else {
            return false;
        };
        if !src_palette.is_empty() {
            bitmap.take_palette(src_palette);
        }
        self.bitmap = Some(Rc::new(RefCell::new(bitmap)));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_scanlines_and_moves_bitmap_with_palette() {
        let mut storer = CfxBitmapStorer::new();
        assert!(storer.set_info(2, 2, FxdibFormat::EightBppRgb, vec![0x11223344]));
        storer.compose_scanline(1, &[7, 8]);
        let shared = storer.bitmap().unwrap();
        assert_eq!(shared.borrow().palette().len(), 256);
        assert_eq!(shared.borrow().palette()[0], 0x11223344);
        assert_eq!(&shared.borrow().scanline(1).unwrap()[..2], &[7, 8]);
        let detached = storer.detach().unwrap();
        assert!(Rc::ptr_eq(&shared, &detached));
        assert!(storer.bitmap().is_none());
        storer.replace(Some(detached));
        assert!(Rc::ptr_eq(&shared, &storer.bitmap().unwrap()));
        assert!(!storer.set_info(0, 2, FxdibFormat::Bgr, Vec::new()));
        assert!(Rc::ptr_eq(&shared, &storer.bitmap().unwrap()));
    }
}
