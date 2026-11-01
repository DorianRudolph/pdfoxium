// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxge/cfx_glyphbitmap.{h,cpp}`: glyph origin and retained bitmap.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use crate::fxcrt::fx_coordinates::CfxPoint;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;

pub struct CfxGlyphBitmap {
    top_left_origin: CfxPoint,
    bitmap: Rc<RefCell<CfxDibitmap>>,
}

impl CfxGlyphBitmap {
    pub(crate) fn retained_bytes(&self) -> usize {
        // Glyph Rc, bitmap Rc/RefCell and the bitmap's owned pixel/palette capacity.
        std::mem::size_of::<Self>()
            + 4 * std::mem::size_of::<usize>()
            + std::mem::size_of::<RefCell<CfxDibitmap>>()
            + self.bitmap.borrow().retained_bytes()
    }
    pub fn new(top_left_origin: CfxPoint, bitmap: Rc<RefCell<CfxDibitmap>>) -> Self {
        Self {
            top_left_origin,
            bitmap,
        }
    }

    pub fn top_left_origin(&self) -> CfxPoint {
        self.top_left_origin
    }

    pub fn bitmap(&self) -> Ref<'_, CfxDibitmap> {
        self.bitmap.borrow()
    }

    pub fn writable_bitmap(&self) -> RefMut<'_, CfxDibitmap> {
        self.bitmap.borrow_mut()
    }

    pub fn retained_bitmap(&self) -> Rc<RefCell<CfxDibitmap>> {
        Rc::clone(&self.bitmap)
    }
}
