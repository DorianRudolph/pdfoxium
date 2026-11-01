// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxge/agg/cfx_agg_imagerenderer.{h,cpp}`. The device borrow keeps the
//! destination and clip alive throughout stretching/transform continuations.

use super::cfx_agg_bitmapcomposer::CfxAggBitmapComposer;
use super::cfx_agg_cliprgn::CfxAggClipRgn;
use crate::fxcrt::fx_coordinates::{CfxMatrix, FxRect};
use crate::fxcrt::pauseindicator_iface::PauseIndicatorIface;
#[cfg(test)]
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmapData;
use crate::fxge::dib::cfx_imagestretcher::CfxImageStretcher;
use crate::fxge::dib::cfx_imagetransformer::CfxImageTransformer;
use crate::fxge::dib::cstretchengine::StretchSource;
use crate::fxge::dib::fx_dib::{
    argb_mul_alpha, get_is_mask_from_format, BlendMode, FxdibResampleOptions,
};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Initial,
    Stretching,
    Transforming,
}

pub struct CfxAggImageRenderer<'a, 'storage, S: StretchSource> {
    transformer: Option<CfxImageTransformer<S>>,
    stretcher: Option<CfxImageStretcher<S>>,
    composer: CfxAggBitmapComposer<'a, 'storage>,
    alpha: f32,
    mask_color: u32,
    state: State,
}

fn relative_clip(rect: FxRect, origin: FxRect) -> Option<FxRect> {
    Some(FxRect::new(
        rect.left.checked_sub(origin.left)?,
        rect.top.checked_sub(origin.top)?,
        rect.right.checked_sub(origin.left)?,
        rect.bottom.checked_sub(origin.top)?,
    ))
}

impl<'a, 'storage, S: StretchSource> CfxAggImageRenderer<'a, 'storage, S> {
    pub fn new(
        device: &'a mut CfxDibitmapData<'storage>,
        clip: Option<&'a CfxAggClipRgn>,
        source: Rc<S>,
        alpha: f32,
        mask_color: u32,
        matrix: CfxMatrix,
        options: FxdibResampleOptions,
        rgb_byte_order: bool,
    ) -> Self {
        Self::new_with_blend(
            device,
            clip,
            source,
            alpha,
            mask_color,
            matrix,
            options,
            rgb_byte_order,
            BlendMode::Normal,
        )
    }

    pub fn new_with_blend(
        device: &'a mut CfxDibitmapData<'storage>,
        clip: Option<&'a CfxAggClipRgn>,
        source: Rc<S>,
        alpha: f32,
        mask_color: u32,
        matrix: CfxMatrix,
        options: FxdibResampleOptions,
        rgb_byte_order: bool,
        blend_mode: BlendMode,
    ) -> Self {
        let image_rect = matrix.get_unit_rect().get_outer_rect();
        let mut clip_box = clip.map_or_else(
            || FxRect::new(0, 0, device.width(), device.height()),
            CfxAggClipRgn::box_rect,
        );
        clip_box.intersect(image_rect);
        let complex =
            matrix.b.abs() >= 0.5 || matrix.a == 0.0 || matrix.c.abs() >= 0.5 || matrix.d == 0.0;
        let vertical = complex
            && matrix.a.abs() < matrix.b.abs() / 20.0
            && matrix.d.abs() < matrix.c.abs() / 20.0
            && matrix.a.abs() < 0.5
            && matrix.d.abs() < 0.5;
        let mut renderer = Self {
            transformer: None,
            stretcher: None,
            composer: CfxAggBitmapComposer::compose(
                device,
                clip,
                alpha,
                mask_color,
                clip_box,
                vertical,
                vertical && matrix.c > 0.0,
                vertical && matrix.b < 0.0,
                rgb_byte_order,
                blend_mode,
            ),
            alpha,
            mask_color,
            state: State::Initial,
        };
        if clip_box.is_empty() {
            return renderer;
        }
        if complex && !vertical {
            renderer.state = State::Transforming;
            renderer.transformer = Some(CfxImageTransformer::new(
                source,
                matrix,
                options,
                Some(clip_box),
            ));
            return renderer;
        }
        let Some(bitmap_clip) = relative_clip(clip_box, image_rect) else {
            return renderer;
        };
        let Some(mut width) = image_rect.right.checked_sub(image_rect.left) else {
            return renderer;
        };
        let Some(mut height) = image_rect.bottom.checked_sub(image_rect.top) else {
            return renderer;
        };
        if vertical {
            let bitmap_clip =
                bitmap_clip.swapped_clip_box(width, height, matrix.c > 0.0, matrix.b < 0.0);
            let mut stretcher = CfxImageStretcher::new(source, height, width, bitmap_clip, options);
            if stretcher.start(&mut renderer.composer) {
                renderer.state = State::Stretching;
            }
            renderer.stretcher = Some(stretcher);
            return renderer;
        }
        if matrix.a < 0.0 {
            width = -width;
        }
        if matrix.d > 0.0 {
            height = -height;
        }
        if width == 0 || height == 0 {
            return renderer;
        }
        renderer.state = State::Stretching;
        let mut stretcher = CfxImageStretcher::new(source, width, height, bitmap_clip, options);
        stretcher.start(&mut renderer.composer);
        renderer.stretcher = Some(stretcher);
        renderer
    }

    pub fn continue_render(&mut self, pause: Option<&mut dyn PauseIndicatorIface>) -> bool {
        if self.state == State::Stretching {
            return self
                .stretcher
                .as_mut()
                .expect("image stretcher")
                .continue_stretch(&mut self.composer, pause);
        }
        if self.state != State::Transforming {
            return false;
        }
        let transformer = self.transformer.as_mut().expect("image transformer");
        if transformer.continue_transform(pause) {
            return true;
        }
        let Some(bitmap) = transformer.detach_bitmap() else {
            return false;
        };
        let mut bitmap = bitmap.borrow_mut();
        if bitmap.buffer().is_empty() {
            return false;
        }
        if get_is_mask_from_format(bitmap.format()) && self.alpha != 1.0 {
            self.mask_color = argb_mul_alpha(self.mask_color, (self.alpha * 255.0).round() as u32);
        } else if !get_is_mask_from_format(bitmap.format()) {
            bitmap.multiply_alpha(self.alpha);
        }
        let result = transformer.result();
        self.composer
            .composite_transformed(&bitmap, self.mask_color, result.left, result.top);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    #[test]
    fn requested_blend_survives_stretch_rotation_transform_and_retained_tasks() {
        use super::super::cfx_agg_devicedriver::CfxAggDeviceDriver;
        for matrix in [
            CfxMatrix::new(4.0, 0.0, 0.0, -4.0, 1.0, 5.0),
            CfxMatrix::new(0.0, 4.0, 4.0, 0.0, 1.0, 1.0),
            CfxMatrix::new(4.0, 1.0, 1.0, -4.0, 0.0, 4.0),
        ] {
            for retained in [false, true] {
                for is_mask in [false, true] {
                    let format = if is_mask {
                        FxdibFormat::EightBppMask
                    } else {
                        FxdibFormat::Bgr
                    };
                    let mut source = CfxDibitmap::create(1, 1, format, 0).unwrap();
                    if is_mask {
                        source.buffer_mut()[0] = 255;
                    } else {
                        source.buffer_mut()[..3].copy_from_slice(&[0, 255, 255]);
                    }
                    let mut dest = CfxDibitmap::create(8, 8, FxdibFormat::Bgra, 0).unwrap();
                    dest.clear(0xff0000ff);
                    let options = FxdibResampleOptions {
                        no_smoothing: true,
                        ..Default::default()
                    };
                    if retained {
                        let mut task = {
                            let mut driver = CfxAggDeviceDriver::new(&mut dest, false, None);
                            driver
                                .start_dibits_with_blend(
                                    Rc::new(source),
                                    1.0,
                                    0xffffff00,
                                    matrix,
                                    options,
                                    BlendMode::Darken,
                                )
                                .into_task()
                        };
                        while task.continue_render(&mut dest, None) {}
                    } else {
                        let mut driver = CfxAggDeviceDriver::new(&mut dest, false, None);
                        let mut renderer = driver.start_dibits_with_blend(
                            Rc::new(source),
                            1.0,
                            0xffffff00,
                            matrix,
                            options,
                            BlendMode::Darken,
                        );
                        while renderer.continue_render(None) {}
                    }
                    let pixel = &dest.scanline(3).unwrap()[12..16];
                    // Affine mask resampling uses fixed-point coverage: a
                    // fully covered source can lose two coverage levels.
                    // Darken must still suppress both yellow channels;
                    // Normal would instead produce bright red and green.
                    assert!(
                        pixel[0] <= 2,
                        "{pixel:?}, retained={retained}, mask={is_mask}"
                    );
                    assert_eq!(&pixel[1..], &[0, 0, 255]);
                    assert_eq!(&dest.scanline(7).unwrap()[28..32], &[255, 0, 0, 255]);
                }
            }
        }
    }

    #[test]
    fn rotated_composition_preserves_all_axis_flips() {
        let colors = [
            [0, 0, 255, 255],
            [0, 255, 0, 255],
            [255, 0, 0, 255],
            [255, 255, 255, 255],
        ];
        for (matrix, expected) in [
            (CfxMatrix::new(0.0, 2.0, 2.0, 0.0, 0.0, 0.0), [2, 0, 3, 1]),
            (CfxMatrix::new(0.0, 2.0, -2.0, 0.0, 2.0, 0.0), [0, 2, 1, 3]),
            (CfxMatrix::new(0.0, -2.0, 2.0, 0.0, 0.0, 2.0), [3, 1, 2, 0]),
            (CfxMatrix::new(0.0, -2.0, -2.0, 0.0, 2.0, 2.0), [1, 3, 0, 2]),
        ] {
            let mut source = CfxDibitmap::create(2, 2, FxdibFormat::Bgra, 0).unwrap();
            for (dest, color) in source.buffer_mut().chunks_exact_mut(4).zip(colors) {
                dest.copy_from_slice(&color);
            }
            let mut dest = CfxDibitmap::create(2, 2, FxdibFormat::Bgra, 0).unwrap();
            {
                let mut renderer = CfxAggImageRenderer::new(
                    &mut dest,
                    None,
                    Rc::new(source),
                    1.0,
                    0,
                    matrix,
                    FxdibResampleOptions {
                        no_smoothing: true,
                        ..Default::default()
                    },
                    false,
                );
                assert!(!renderer.continue_render(None));
            }
            for (pixel, index) in dest.buffer().chunks_exact(4).zip(expected) {
                assert_eq!(pixel, colors[index]);
            }
        }
    }

    #[test]
    fn affine_mask_composition_multiplies_alpha_and_swaps_rgb_bytes() {
        let mut source = CfxDibitmap::create(1, 1, FxdibFormat::EightBppMask, 0).unwrap();
        source.buffer_mut()[0] = 255;
        let mut dest = CfxDibitmap::create(3, 3, FxdibFormat::Bgra, 0).unwrap();
        {
            let mut renderer = CfxAggImageRenderer::new(
                &mut dest,
                None,
                Rc::new(source),
                0.5,
                0xc8112233,
                CfxMatrix::new(2.0, 0.6, 0.0, -2.0, 0.0, 2.0),
                FxdibResampleOptions::default(),
                true,
            );
            assert!(!renderer.continue_render(None));
        }
        let occupied: Vec<_> = dest
            .buffer()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] != 0)
            .collect();
        assert!(!occupied.is_empty());
        assert!(occupied
            .iter()
            .all(|pixel| pixel[..3] == [0x11, 0x22, 0x33] && pixel[3] == 99));
    }
}

#[path = "cfx_agg_imagerenderer_task.rs"]
mod task;
pub use task::CfxAggImageRenderTask;
