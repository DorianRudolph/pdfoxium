// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/graphics/cfgas_gecolor.{h,cpp}`. Shared owners replace source
//! non-owning pointers so saved graphics states cannot retain dangling colors.

use super::{cfgas_gepattern::CfgasGePattern, cfgas_geshading::CfgasGeShading};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorType {
    Invalid,
    Solid,
    Pattern,
    Shading,
}

#[derive(Clone, Debug, Default)]
pub enum CfgasGeColor {
    #[default]
    Invalid,
    Solid(u32),
    Pattern {
        pattern: Rc<CfgasGePattern>,
        argb: u32,
    },
    Shading(Rc<CfgasGeShading>),
}

impl CfgasGeColor {
    pub fn get_type(&self) -> ColorType {
        match self {
            Self::Invalid => ColorType::Invalid,
            Self::Solid(_) => ColorType::Solid,
            Self::Pattern { .. } => ColorType::Pattern,
            Self::Shading(_) => ColorType::Shading,
        }
    }
    pub fn get_argb(&self) -> Option<u32> {
        match self {
            Self::Solid(argb) | Self::Pattern { argb, .. } => Some(*argb),
            _ => None,
        }
    }
    pub fn get_pattern(&self) -> Option<&CfgasGePattern> {
        match self {
            Self::Pattern { pattern, .. } => Some(pattern),
            _ => None,
        }
    }
    pub fn get_shading(&self) -> Option<&CfgasGeShading> {
        match self {
            Self::Shading(shading) => Some(shading),
            _ => None,
        }
    }
    pub fn color_to_string(argb: u32) -> Vec<u8> {
        format!(
            "{},{},{}",
            (argb >> 16) & 255,
            (argb >> 8) & 255,
            argb & 255
        )
        .into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfa::fgas::graphics::cfgas_gepattern::HatchStyle;
    #[test]
    fn color_channels_and_shared_pattern_lifetime() {
        assert_eq!(CfgasGeColor::color_to_string(0x001234ef), b"18,52,239");
        assert_eq!(CfgasGeColor::color_to_string(0xff1234ef), b"18,52,239");
        let pattern = Rc::new(CfgasGePattern::new(
            HatchStyle::Cross,
            0xff123456,
            0xffabcdef,
        ));
        let color = CfgasGeColor::Pattern {
            pattern: pattern.clone(),
            argb: 0x12345678,
        };
        let saved = color.clone();
        drop(color);
        drop(pattern);
        assert_eq!(saved.get_type(), ColorType::Pattern);
        assert_eq!(saved.get_argb(), Some(0x12345678));
        assert_eq!(saved.get_pattern().unwrap().get_fore_argb(), 0xff123456);
        assert_eq!(saved.get_pattern().unwrap().get_back_argb(), 0xffabcdef);
        assert_eq!(
            saved.get_pattern().unwrap().get_hatch_style(),
            HatchStyle::Cross
        );
        assert!(saved.get_shading().is_none());
        assert!(CfgasGeColor::Invalid.get_argb().is_none());
    }
}
