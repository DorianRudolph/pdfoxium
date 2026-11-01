// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxge/agg/cfx_agg_cliprgn.{h,cpp}`.

use std::rc::Rc;

use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::FxdibFormat;

#[derive(Clone)]
pub struct CfxAggClipRgn {
    box_rect: FxRect,
    mask: Option<Rc<CfxDibitmap>>,
}

impl CfxAggClipRgn {
    pub fn new(device_width: i32, device_height: i32) -> Self {
        Self {
            box_rect: FxRect::new(0, 0, device_width, device_height),
            mask: None,
        }
    }

    pub fn box_rect(&self) -> FxRect {
        self.box_rect
    }

    pub fn mask(&self) -> Option<&CfxDibitmap> {
        self.mask.as_deref()
    }

    pub fn intersect_rect(&mut self, rect: FxRect) -> bool {
        let Some(mask) = self.mask.clone() else {
            self.box_rect.intersect(rect);
            return true;
        };
        self.intersect_mask_and_rect(rect, self.box_rect, mask)
    }

    pub fn intersect_mask(&mut self, left: i32, top: i32, mask: Option<Rc<CfxDibitmap>>) -> bool {
        let (width, height) = mask
            .as_ref()
            .map(|mask| (mask.width(), mask.height()))
            .unwrap_or((0, 0));
        let Some(right) = left.checked_add(width) else {
            return false;
        };
        let Some(bottom) = top.checked_add(height) else {
            return false;
        };
        let mask_box = FxRect::new(left, top, right, bottom);
        if !mask_box.is_empty()
            && mask
                .as_ref()
                .is_some_and(|m| m.format() != FxdibFormat::EightBppMask)
        {
            return false;
        }
        let Some(mask) = mask else {
            self.box_rect.intersect(mask_box);
            self.mask = None;
            return true;
        };
        if self.mask.is_none() {
            return self.intersect_mask_and_rect(self.box_rect, mask_box, mask);
        }

        let mut new_box = self.box_rect;
        new_box.intersect(mask_box);
        if new_box.is_empty() {
            self.mask = None;
            self.box_rect = new_box;
            return true;
        }
        let Some(mut new_bitmap) = CfxDibitmap::create(
            new_box.width(),
            new_box.height(),
            FxdibFormat::EightBppMask,
            0,
        ) else {
            return false;
        };
        let old_mask = self.mask.as_ref().expect("clip mask disappeared");
        for row in new_box.top..new_box.bottom {
            let old_scan = old_mask
                .scanline(row - self.box_rect.top)
                .expect("old mask row");
            let mask_scan = mask.scanline(row - top).expect("new mask row");
            let new_scan = new_bitmap
                .scanline_mut(row - new_box.top)
                .expect("merged mask row");
            for col in new_box.left..new_box.right {
                new_scan[(col - new_box.left) as usize] =
                    (u32::from(old_scan[(col - self.box_rect.left) as usize])
                        * u32::from(mask_scan[(col - left) as usize])
                        / 255) as u8;
            }
        }
        self.box_rect = new_box;
        self.mask = Some(Rc::new(new_bitmap));
        true
    }

    fn intersect_mask_and_rect(
        &mut self,
        rect: FxRect,
        mask_rect: FxRect,
        old_mask: Rc<CfxDibitmap>,
    ) -> bool {
        let mut new_box = rect;
        new_box.intersect(mask_rect);
        if new_box.is_empty() {
            self.box_rect = new_box;
            self.mask = None;
            return true;
        }
        if new_box == mask_rect {
            self.box_rect = new_box;
            self.mask = Some(old_mask);
            return true;
        }
        let Some(mut bitmap) = CfxDibitmap::create(
            new_box.width(),
            new_box.height(),
            FxdibFormat::EightBppMask,
            0,
        ) else {
            return false;
        };
        let offset = (new_box.left - mask_rect.left) as usize;
        let width = new_box.width() as usize;
        for row in new_box.top..new_box.bottom {
            let source = old_mask
                .scanline(row - mask_rect.top)
                .expect("source mask row");
            let destination = bitmap
                .scanline_mut(row - new_box.top)
                .expect("clipped mask row");
            destination[..width].copy_from_slice(&source[offset..offset + width]);
        }
        self.box_rect = new_box;
        self.mask = Some(Rc::new(bitmap));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersects_masks_with_row_offsets_and_alpha_multiplication() {
        let mut first = CfxDibitmap::create(3, 2, FxdibFormat::EightBppMask, 0).unwrap();
        first.scanline_mut(0).unwrap()[..3].copy_from_slice(&[255, 128, 64]);
        first.scanline_mut(1).unwrap()[..3].copy_from_slice(&[64, 128, 255]);
        let mut second = CfxDibitmap::create(2, 2, FxdibFormat::EightBppMask, 0).unwrap();
        second.scanline_mut(0).unwrap()[..2].copy_from_slice(&[255, 128]);
        second.scanline_mut(1).unwrap()[..2].copy_from_slice(&[128, 255]);
        let mut region = CfxAggClipRgn::new(4, 3);
        assert!(region.intersect_mask(0, 0, Some(Rc::new(first))));
        assert!(region.intersect_mask(1, 0, Some(Rc::new(second))));
        assert_eq!(region.box_rect(), FxRect::new(1, 0, 3, 2));
        assert_eq!(
            &region.mask().unwrap().scanline(0).unwrap()[..2],
            &[128, 32]
        );
        assert_eq!(
            &region.mask().unwrap().scanline(1).unwrap()[..2],
            &[64, 255]
        );
        assert!(region.intersect_rect(FxRect::new(2, 0, 3, 2)));
        assert_eq!(region.box_rect(), FxRect::new(2, 0, 3, 2));
        assert_eq!(region.mask().unwrap().scanline(0).unwrap()[0], 32);
    }
}
