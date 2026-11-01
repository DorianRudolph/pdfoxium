/*
 * // Copyright (c) Radzivon Bartoshyk 3/2025. All rights reserved.
 * //
 * // Redistribution and use in source and binary forms, with or without modification,
 * // are permitted provided that the following conditions are met:
 * //
 * // 1.  Redistributions of source code must retain the above copyright notice, this
 * // list of conditions and the following disclaimer.
 * //
 * // 2.  Redistributions in binary form must reproduce the above copyright notice,
 * // this list of conditions and the following disclaimer in the documentation
 * // and/or other materials provided with the distribution.
 * //
 * // 3.  Neither the name of the copyright holder nor the names of its
 * // contributors may be used to endorse or promote products derived from
 * // this software without specific prior written permission.
 * //
 * // THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
 * // AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * // IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
 * // DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
 * // FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * // DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
 * // SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
 * // CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * // OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
 * // OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */
#![cfg(feature = "lut")]
// CMYK black-point detection is adapted from LittleCMS's
// BlackPointAsDarkerColorant (Copyright (c) 2023 Marti Maria Saguer).
// Its MIT license is retained in LICENSE-LCMS.
use crate::mlaf::fmla;
use crate::{Chromaticity, ColorProfile, DataColorSpace, ProfileVersion, RenderingIntent, Xyz};

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub(crate) enum TransformDirection {
    DeviceToPcs,
    PcsToDevice,
}

impl ColorProfile {
    pub(crate) fn detect_black_point(
        &self,
        intent: RenderingIntent,
        transform_direction: TransformDirection,
    ) -> Option<Xyz> {
        static DEFAULT_V4_BLACK_POINT: Xyz = Xyz::new(0.003357, 0.003479, 0.002869);
        if intent != RenderingIntent::Perceptual && intent != RenderingIntent::Saturation {
            return None;
        }
        if self.color_space == DataColorSpace::Cmyk {
            if matches!(
                self.profile_class,
                crate::ProfileClass::DeviceLink
                    | crate::ProfileClass::Abstract
                    | crate::ProfileClass::Named
            ) {
                return None;
            }
            if self.version_internal >= ProfileVersion::V4_0 {
                return Some(DEFAULT_V4_BLACK_POINT);
            }
            if transform_direction != TransformDirection::DeviceToPcs {
                return None;
            }
            return self.cmyk_source_black_point(intent);
        } else if self.color_space == DataColorSpace::Rgb {
            if self.version_internal < ProfileVersion::V4_0 {
                return None;
            }
            return match transform_direction {
                TransformDirection::DeviceToPcs => {
                    if self.has_device_to_pcs_lut() {
                        return Some(DEFAULT_V4_BLACK_POINT);
                    }
                    Some(Xyz::default())
                }
                TransformDirection::PcsToDevice => {
                    if self.has_pcs_to_device_lut() {
                        return Some(DEFAULT_V4_BLACK_POINT);
                    }
                    Some(Xyz::default())
                }
            };
        }
        None
    }

    /// Evaluate the darkest ink endpoint, as LittleCMS BlackPointAsDarkerColorant
    /// does for v2 perceptual/saturation input. This is setup work, not per-pixel.
    fn cmyk_source_black_point(&self, intent: RenderingIntent) -> Option<Xyz> {
        let mut pcs = [0.; 3];
        match self.get_device_to_pcs(intent)? {
            crate::LutWarehouse::Lut(lut) => {
                let stage = crate::conversions::lut4::stage_lut_4x3(
                    lut,
                    crate::TransformOptions::default(),
                    self.pcs,
                )
                .ok()?;
                stage.transform(&[1.; 4], &mut pcs).ok()?;
            }
            crate::LutWarehouse::Multidimensional(mab) => {
                let output = crate::conversions::mab4x3::prepare_mab_4x3(
                    mab,
                    &mut [1.; 4],
                    crate::TransformOptions::default(),
                    self.pcs,
                )
                .ok()?;
                pcs.copy_from_slice(&output);
            }
        }
        let mut lab = match self.pcs {
            DataColorSpace::Lab => {
                if self.device_to_pcs_is_lut16(intent) {
                    for component in &mut pcs {
                        *component *= 65535. / 65280.;
                    }
                }
                crate::Lab::new(pcs[0] * 100., pcs[1] * 255. - 128., pcs[2] * 255. - 128.)
            }
            DataColorSpace::Xyz => crate::Lab::from_xyz(Xyz::new(pcs[0], pcs[1], pcs[2])),
            _ => return None,
        };
        // Retain endpoint chroma. Only lightness is constrained, including
        // the native guard against synthetic profiles with inverted endpoints.
        lab.l = if lab.l > 95. {
            0.
        } else {
            lab.l.clamp(0., 50.)
        };
        let xyz = lab.to_xyz();
        // Lab helpers use encoded PCS XYZ; compensation takes physical XYZ.
        const SCALE: f32 = 1. + 32767. / 32768.;
        Some(Xyz::new(xyz.x * SCALE, xyz.y * SCALE, xyz.z * SCALE))
    }
}

pub(crate) fn compensate_bpc_in_lut(lut_xyz: &mut [f32], src_bp: Xyz, dst_bp: Xyz) {
    if src_bp.eq(&Xyz::default()) && dst_bp.eq(&Xyz::default()) {
        return;
    }
    const WP: Xyz = Chromaticity::D50.to_xyz();

    // a = (dst_wp - dst_bp) / (src_wp - src_bp)  — scale
    // b = dst_bp - a * src_bp                     — offset
    // out = a * in + b
    let compute = |src_bp_c: f32, dst_bp_c: f32, wp_c: f32| -> (f32, f32) {
        let denom = wp_c - src_bp_c;
        if denom.abs() < 1e-10 {
            return (1.0, 0.0); // degenerate — pass through
        }
        let a = (wp_c - dst_bp_c) / denom;
        let b = dst_bp_c - a * src_bp_c;
        (a, b)
    };

    let (ax, mut bx) = compute(src_bp.x, dst_bp.x, WP.x);
    let (ay, mut by) = compute(src_bp.y, dst_bp.y, WP.y);
    let (az, mut bz) = compute(src_bp.z, dst_bp.z, WP.z);

    const S: f32 = 1.0 / (1.0 + 32767.0 / 32768.0);
    bx *= S;
    by *= S;
    bz *= S;

    for dst in lut_xyz.as_chunks_mut::<3>().0.iter_mut() {
        dst[0] = fmla(dst[0], ax, bx); // ax * in + bx
        dst[1] = fmla(dst[1], ay, by);
        dst[2] = fmla(dst[2], az, bz);
    }
}
