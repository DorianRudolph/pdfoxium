// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Safe ICC adapter for `core/fxcodec/icc/icc_transform.{h,cpp}`.
//! LittleCMS is replaced by the `moxcms` codec behind PDFium's input and BGR
//! output rules. Color-managed byte results may differ between the codecs.

use std::sync::Arc;

use moxcms::{
    BarycentricWeightScale, ColorProfile, DataColorSpace, InterpolationMethod, Layout,
    Transform8BitExecutor, TransformF32Executor, TransformOptions,
};

// Shared with decoded image rows: retain GetRGB's exact float-to-byte input
// encoding before batching transforms. Inverted PDF Decode arrays can round
// differently from an integer 255 - sample shortcut.
pub(crate) fn encode_normal_component(value: f32) -> u8 {
    (value * 255.0).clamp(0.0, 255.0) as i32 as u8
}

pub struct IccTransform {
    transform: Arc<Transform8BitExecutor>,
    lab_transform: Option<Arc<TransformF32Executor>>,
    src_components: usize,
    normal: bool,
}

impl IccTransform {
    pub fn is_valid_icc_components(components: i32) -> bool {
        matches!(components, 1 | 3 | 4)
    }

    pub fn create_transform_srgb(bytes: &[u8]) -> Option<Self> {
        let source = ColorProfile::new_from_slice(bytes).ok()?;
        let (components, layout) = match source.color_space {
            DataColorSpace::Gray => (1, Layout::Gray),
            DataColorSpace::Cmyk | DataColorSpace::Color4 => (4, Layout::Rgba),
            DataColorSpace::Color2
            | DataColorSpace::Color5
            | DataColorSpace::Color6
            | DataColorSpace::Color7
            | DataColorSpace::Color8
            | DataColorSpace::Color9
            | DataColorSpace::Color10
            | DataColorSpace::Color11
            | DataColorSpace::Color12
            | DataColorSpace::Color13
            | DataColorSpace::Color14
            | DataColorSpace::Color15 => return None,
            _ => (3, Layout::Rgb),
        };
        let destination = ColorProfile::new_srgb();
        if destination.color_space != DataColorSpace::Rgb {
            return None;
        }
        let options = TransformOptions {
            rendering_intent: moxcms::RenderingIntent::Perceptual,
            allow_use_cicp_transfer: false,
            interpolation_method: InterpolationMethod::Tetrahedral,
            ..TransformOptions::default()
        };
        let transform = source
            .create_transform_8bit(layout, &destination, Layout::Rgb, options)
            .ok()?;
        let lab = source.color_space == DataColorSpace::Lab;
        let lab_transform = if lab {
            Some(
                source
                    .create_transform_f32(
                        layout,
                        &destination,
                        Layout::Rgb,
                        TransformOptions {
                            barycentric_weight_scale: BarycentricWeightScale::High,
                            ..options
                        },
                    )
                    .ok()?,
            )
        } else {
            None
        };
        let normal = matches!(
            source.color_space,
            DataColorSpace::Gray | DataColorSpace::Rgb | DataColorSpace::Cmyk
        );
        Some(Self {
            transform,
            lab_transform,
            src_components: components,
            normal,
        })
    }

    pub fn components(&self) -> usize {
        self.src_components
    }
    pub fn is_normal(&self) -> bool {
        self.normal
    }

    /// Preserve PDFium's fractional Lab inputs. Quantize only the resulting RGB,
    /// matching its byte-output ICC transform; other source spaces remain bytes.
    pub fn translate(&self, src: &[f32], dest: &mut [f32; 3]) -> bool {
        if src.len() < self.src_components {
            return false;
        }
        if let Some(transform) = &self.lab_transform {
            // moxcms float inputs are normalized encoded Lab. Clamping retains
            // the previous byte adapter's treatment of out-of-range operands.
            let normalize = |v: f32| if v.is_nan() { 0. } else { v.clamp(0., 1.) };
            let input = [
                normalize(src[0] / 100.),
                normalize((src[1] + 128.) / 255.),
                normalize((src[2] + 128.) / 255.),
            ];
            let mut output = [0.; 3];
            if transform.transform(&input, &mut output).is_err() {
                return false;
            }
            for (dest, value) in dest.iter_mut().zip(output) {
                *dest = f32::from((value * 255.).round().clamp(0., 255.) as u8) / 255.;
            }
            return true;
        }
        let mut input = [0u8; 4];
        for (input, &value) in input.iter_mut().zip(src).take(self.src_components) {
            *input = encode_normal_component(value);
        }
        let mut output = [0u8; 3];
        if self
            .transform
            .transform(&input[..self.src_components], &mut output)
            .is_err()
        {
            return false;
        }
        dest[0] = f32::from(output[0]) / 255.0;
        dest[1] = f32::from(output[1]) / 255.0;
        dest[2] = f32::from(output[2]) / 255.0;
        true
    }

    /// The input is packed source-profile bytes; PDFium stores output BGR.
    pub fn translate_scanline(&self, dest: &mut [u8], src: &[u8], pixels: usize) -> bool {
        let Some(src_len) = pixels.checked_mul(self.src_components) else {
            return false;
        };
        let Some(dst_len) = pixels.checked_mul(3) else {
            return false;
        };
        if src.len() < src_len || dest.len() < dst_len {
            return false;
        }
        let dest = &mut dest[..dst_len];
        if self.transform.transform(&src[..src_len], dest).is_err() {
            return false;
        }
        for pixel in dest.chunks_exact_mut(3) {
            pixel.swap(0, 2);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moxcms::{
        LutMultidimensionalType, LutWarehouse, Matrix3d, ProfileClass, ToneReprCurve, Vector3d,
    };

    fn lab_profile() -> Vec<u8> {
        // Original synthetic Lab->Lab identity profile; no external fixture.
        let mut profile = ColorProfile::default();
        profile.color_space = DataColorSpace::Lab;
        profile.pcs = DataColorSpace::Lab;
        profile.profile_class = ProfileClass::InputDevice;
        profile.white_point = ColorProfile::new_srgb().white_point;
        profile.media_white_point = Some(profile.white_point);
        profile.lut_a_to_b_perceptual =
            Some(LutWarehouse::Multidimensional(LutMultidimensionalType {
                num_input_channels: 3,
                num_output_channels: 3,
                grid_points: [0; 16],
                clut: None,
                a_curves: Vec::new(),
                m_curves: Vec::new(),
                b_curves: vec![ToneReprCurve::Lut(Vec::new()); 3],
                matrix: Matrix3d::IDENTITY,
                bias: Vector3d::default(),
            }));
        profile.encode().unwrap()
    }

    fn color(transform: &IccTransform, lab: [f32; 3]) -> [u8; 3] {
        let mut rgb = [0.; 3];
        assert!(transform.translate(&lab, &mut rgb));
        rgb.map(|v| (v * 255.).round() as u8)
    }

    #[test]
    fn fractional_lab_tints_keep_near_white_neutral() {
        let transform = IccTransform::create_transform_srgb(&lab_profile()).unwrap();
        // The first nonzero sample of a white->Pantone-blue tint function.
        // Independent CIE Lab/D50->sRGB evaluation gives [254,254,255].
        assert_eq!(
            color(&transform, [99.74164, -0.007843137, -0.2509804]),
            [254, 254, 255]
        );
        assert_eq!(color(&transform, [100., 0., 0.]), [255; 3]);
        assert_eq!(color(&transform, [0., 0., 0.]), [0; 3]);
        // Both a/b pairs collapsed to bytes [128,128] in the old adapter.
        assert_ne!(
            color(&transform, [80., 0., 0.]),
            color(&transform, [80., 0.9, 0.9])
        );
    }

    #[test]
    fn lab_byte_scanlines_agree_with_fractional_color_inputs() {
        let transform = IccTransform::create_transform_srgb(&lab_profile()).unwrap();
        let src: Vec<u8> = [0, 1, 64, 128, 254, 255]
            .into_iter()
            .flat_map(|l| [l, 128, 128])
            .collect();
        let mut bgr = vec![0; src.len()];
        assert!(transform.translate_scanline(&mut bgr, &src, 6));
        for ((src, bgr), expected) in src
            .chunks_exact(3)
            .zip(bgr.chunks_exact(3))
            .zip([0u8, 1, 60, 119, 254, 255])
        {
            let rgb = color(&transform, [f32::from(src[0]) * 100. / 255., 0., 0.]);
            // Independent neutral CIE Lab -> sRGB values. Integer and float
            // executors use different gamma LUT resolutions; the byte route
            // may differ by one channel level, never by a visible tint.
            assert_eq!(rgb, [expected; 3]);
            assert!(
                bgr.iter().all(|&v| v.abs_diff(expected) <= 1),
                "{src:?}: {bgr:?}"
            );
        }
    }
}
