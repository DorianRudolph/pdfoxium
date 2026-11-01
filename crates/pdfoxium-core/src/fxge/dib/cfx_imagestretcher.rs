// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxge/dib/cfx_imagestretcher.{h,cpp}`. The composer is passed to each
//! stage so an owning transformer needs no self reference to its bitmap storer.

use super::cstretchengine::{CStretchEngine, StretchSource};
use super::fx_dib::{
    argb_a, argb_b, argb_encode, argb_g, argb_r, FxdibFormat, FxdibResampleOptions,
};
use super::scanlinecomposer_iface::ScanlineComposerIface;
use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxcrt::pauseindicator_iface::PauseIndicatorIface;
use std::rc::Rc;

pub struct CfxImageStretcher<S: StretchSource> {
    source: Rc<S>,
    engine: Option<CStretchEngine<S>>,
    options: FxdibResampleOptions,
    dest_width: i32,
    dest_height: i32,
    clip: FxRect,
    dest_format: FxdibFormat,
}

impl<S: StretchSource> CfxImageStretcher<S> {
    pub fn new(
        source: Rc<S>,
        dest_width: i32,
        dest_height: i32,
        clip: FxRect,
        options: FxdibResampleOptions,
    ) -> Self {
        assert!(clip.valid());
        let dest_format = match source.format() {
            FxdibFormat::OneBppMask => FxdibFormat::EightBppMask,
            FxdibFormat::OneBppRgb => FxdibFormat::EightBppRgb,
            FxdibFormat::EightBppRgb if !source.palette().is_empty() => FxdibFormat::Bgr,
            format => format,
        };
        Self {
            source,
            engine: None,
            options,
            dest_width,
            dest_height,
            clip,
            dest_format,
        }
    }

    pub fn source(&self) -> Rc<S> {
        Rc::clone(&self.source)
    }

    /// True means the caller must Continue(); false also includes a completed
    /// synchronous stretch, matching PDFium's progressive-operation contract.
    pub fn start(&mut self, dest: &mut dyn ScanlineComposerIface) -> bool {
        if self.dest_width == 0 || self.dest_height == 0 {
            return false;
        }
        let palette = if self.source.format() == FxdibFormat::OneBppRgb
            && !self.source.palette().is_empty()
        {
            let source = self.source.palette();
            let (first, second) = (source[0], source[1]);
            assert_eq!(argb_a(first), 255);
            assert_eq!(argb_a(second), 255);
            let mut palette = Vec::with_capacity(256);
            for index in 0..256 {
                let interpolate =
                    |a: u8, b: u8| i32::from(a) + (i32::from(b) - i32::from(a)) * index / 255;
                palette.push(argb_encode(
                    255,
                    interpolate(argb_r(first), argb_r(second)) as u32,
                    interpolate(argb_g(first), argb_g(second)) as u32,
                    interpolate(argb_b(first), argb_b(second)) as u32,
                ));
            }
            palette
        } else {
            Vec::new()
        };
        if !dest.set_info(
            self.clip.width(),
            self.clip.height(),
            self.dest_format,
            palette,
        ) {
            return false;
        }
        let mut engine = CStretchEngine::new(
            Rc::clone(&self.source),
            self.dest_format,
            self.dest_width,
            self.dest_height,
            self.clip,
            self.options,
        );
        engine.start_stretch_horz();
        let height = self.source.height();
        let synchronous = height == 0 || self.source.width() < 1_000_000 / height;
        if synchronous {
            engine.continue_stretch(dest, None);
        }
        self.engine = Some(engine);
        !synchronous
    }

    pub fn continue_stretch(
        &mut self,
        dest: &mut dyn ScanlineComposerIface,
        pause: Option<&mut dyn PauseIndicatorIface>,
    ) -> bool {
        self.engine
            .as_mut()
            .is_some_and(|engine| engine.continue_stretch(dest, pause))
    }
}

#[cfg(test)]
mod tests {
    use super::super::cfx_bitmapstorer::CfxBitmapStorer;
    use super::super::cfx_dibitmap::CfxDibitmap;
    use super::*;

    #[test]
    fn one_bit_palette_expands_and_small_stretch_completes_in_start() {
        let mut source = CfxDibitmap::create(2, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        source.take_palette(vec![0xff123456, 0xffabcdef]);
        source.buffer_mut()[0] = 0x40;
        let mut stretcher = CfxImageStretcher::new(
            Rc::new(source),
            2,
            1,
            FxRect::new(0, 0, 2, 1),
            FxdibResampleOptions::default(),
        );
        let mut storer = CfxBitmapStorer::new();
        assert!(!stretcher.start(&mut storer));
        assert!(!stretcher.continue_stretch(&mut storer, None));
        let bitmap = storer.detach().unwrap();
        let bitmap = bitmap.borrow();
        assert_eq!(bitmap.format(), FxdibFormat::EightBppRgb);
        assert_eq!(&bitmap.buffer()[..2], &[0, 255]);
        assert_eq!(bitmap.palette()[0], 0xff123456);
        assert_eq!(bitmap.palette()[255], 0xffabcdef);
        assert_eq!(bitmap.palette()[128], 0xff5e80a2);
    }
}
