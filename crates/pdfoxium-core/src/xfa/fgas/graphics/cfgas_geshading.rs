// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/graphics/cfgas_geshading.{h,cpp}`. Source 256-step channel
//! interpolation retains signed deltas and truncation before channel addition.

use crate::fxcrt::fx_coordinates::CfxPointF;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ShadingType {
    Axial = 1,
    Radial = 2,
}

#[derive(Clone, Debug)]
pub struct CfgasGeShading {
    shading_type: ShadingType,
    begin_point: CfxPointF,
    end_point: CfxPointF,
    begin_radius: f32,
    end_radius: f32,
    is_extended_begin: bool,
    is_extended_end: bool,
    argb_array: [u32; 256],
}

impl CfgasGeShading {
    pub fn axial(
        begin_point: CfxPointF,
        end_point: CfxPointF,
        is_extended_begin: bool,
        is_extended_end: bool,
        begin_argb: u32,
        end_argb: u32,
    ) -> Self {
        Self {
            shading_type: ShadingType::Axial,
            begin_point,
            end_point,
            begin_radius: 0.0,
            end_radius: 0.0,
            is_extended_begin,
            is_extended_end,
            argb_array: Self::init_argb_array(begin_argb, end_argb),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn radial(
        begin_point: CfxPointF,
        end_point: CfxPointF,
        begin_radius: f32,
        end_radius: f32,
        is_extended_begin: bool,
        is_extended_end: bool,
        begin_argb: u32,
        end_argb: u32,
    ) -> Self {
        Self {
            shading_type: ShadingType::Radial,
            begin_point,
            end_point,
            begin_radius,
            end_radius,
            is_extended_begin,
            is_extended_end,
            argb_array: Self::init_argb_array(begin_argb, end_argb),
        }
    }
    fn init_argb_array(begin_argb: u32, end_argb: u32) -> [u32; 256] {
        let channels = [24, 16, 8, 0].map(|shift| {
            let begin = ((begin_argb >> shift) & 255) as i32;
            let end = ((end_argb >> shift) & 255) as i32;
            (begin, (end - begin) as f32 / 255.0)
        });
        std::array::from_fn(|i| {
            let mut color = 0;
            for (begin, scale) in channels {
                color = (color << 8) | ((i as f32 * scale) as i32 + begin) as u32;
            }
            color
        })
    }
    pub fn get_type(&self) -> ShadingType {
        self.shading_type
    }
    pub fn get_begin_point(&self) -> CfxPointF {
        self.begin_point
    }
    pub fn get_end_point(&self) -> CfxPointF {
        self.end_point
    }
    pub fn get_begin_radius(&self) -> f32 {
        self.begin_radius
    }
    pub fn get_end_radius(&self) -> f32 {
        self.end_radius
    }
    pub fn is_extended_begin(&self) -> bool {
        self.is_extended_begin
    }
    pub fn is_extended_end(&self) -> bool {
        self.is_extended_end
    }
    pub fn get_argb(&self, value: f32) -> u32 {
        // Native callers supply a unit interval. Bound external/invalid input
        // instead of permitting an out-of-range float-to-index conversion.
        self.argb_array[(value.clamp(0.0, 1.0) * 255.0) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_steps_preserve_signed_truncation_and_bounds() {
        let shading = CfgasGeShading::axial(
            CfxPointF::new(1.0, 2.0),
            CfxPointF::new(3.0, 4.0),
            true,
            false,
            0x6496c832,
            0x96c864c8,
        );
        assert_eq!(shading.get_type(), ShadingType::Axial);
        assert_eq!(shading.get_argb(0.0), 0x6496c832);
        assert_eq!(shading.get_argb(127.0 / 255.0), 0x7cae977c);
        assert_eq!(shading.get_argb(1.0), 0x96c864c8);
        assert_eq!(shading.get_argb(-1.0), shading.get_argb(0.0));
        assert_eq!(shading.get_argb(2.0), shading.get_argb(1.0));
        assert_eq!(shading.get_argb(f32::NAN), shading.get_argb(0.0));
        assert!(shading.is_extended_begin());
        assert!(!shading.is_extended_end());
        let radial = CfgasGeShading::radial(
            CfxPointF::new(2.0, 3.0),
            CfxPointF::new(5.0, 7.0),
            4.0,
            8.0,
            false,
            true,
            0xff00ff00,
            0x00ff00ff,
        );
        assert_eq!(radial.get_type(), ShadingType::Radial);
        assert_eq!(radial.get_begin_point(), CfxPointF::new(2.0, 3.0));
        assert_eq!(radial.get_end_point(), CfxPointF::new(5.0, 7.0));
        assert_eq!(radial.get_begin_radius(), 4.0);
        assert_eq!(radial.get_end_radius(), 8.0);
        for i in 0..=255_u32 {
            let color = radial.argb_array[i as usize];
            assert_eq!(color, ((255 - i) << 24) | (i << 16) | ((255 - i) << 8) | i);
        }
    }
}
