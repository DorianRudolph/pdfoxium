// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! ICCBased color space from `core/fpdfapi/page/cpdf_colorspace.cpp`.

use super::cpdf_basedcs::CpdfBasedCs;
use super::cpdf_colorspace::{
    get_stock_device_cs, ColorSpaceFamily, ColorSpaceRef, CpdfColorSpace, CpdfColorSpaceData,
};
use super::cpdf_iccprofile::CpdfIccProfile;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::fpdf_parser_utility::read_array_elements_to_vector;
use crate::fxcodec::icc::icc_transform::IccTransform;
use crate::fxge::dib::fx_dib::FxRgbStruct;
use std::rc::Rc;

pub struct CpdfIccBasedCs {
    based: CpdfBasedCs,
    profile: Rc<CpdfIccProfile>,
    ranges: Vec<f32>,
}

impl CpdfIccBasedCs {
    pub fn load(
        array_object: &ObjectRef,
        load_alternate: impl FnOnce(&ObjectRef) -> Option<ColorSpaceRef>,
        load_profile: impl FnOnce(&ObjectRef, u32) -> Option<Rc<CpdfIccProfile>>,
    ) -> Option<Self> {
        let stream = {
            let borrowed = array_object.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfArray>()?
                .get_stream_at(1)?
        };
        let dict = {
            let borrowed = stream.borrow();
            borrowed.as_any().downcast_ref::<CpdfStream>()?.get_dict()
        };
        let components = {
            let borrowed = dict.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            let components = dict.get_integer_for(b"N", 0);
            if !IccTransform::is_valid_icc_components(components) {
                return None;
            }
            components as u32
        };
        let profile = load_profile(&stream, components)?;
        let mut based = CpdfBasedCs::new(ColorSpaceFamily::IccBased);
        if !profile.is_supported() {
            let alternate = dict
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_direct_object_for(b"Alternate");
            let base = alternate
                .and_then(|object| load_alternate(&object))
                .filter(|space| {
                    space.family() != ColorSpaceFamily::Pattern
                        && space.component_count() == components
                })
                .unwrap_or_else(|| {
                    get_stock_device_cs(match components {
                        1 => ColorSpaceFamily::DeviceGray,
                        3 => ColorSpaceFamily::DeviceRgb,
                        4 => ColorSpaceFamily::DeviceCmyk,
                        _ => unreachable!("ICC component count was validated"),
                    })
                });
            based.set_base_cs(Some(base));
        }
        let ranges = {
            let borrowed = dict.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            let array = dict.get_array_for(b"Range");
            if let Some(array) = array {
                let borrowed = array.borrow();
                let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
                if array.size() >= components as usize * 2 {
                    read_array_elements_to_vector(array, components as usize * 2)
                } else {
                    (0..components).flat_map(|_| [0.0, 1.0]).collect()
                }
            } else {
                (0..components).flat_map(|_| [0.0, 1.0]).collect()
            }
        };
        based.data_mut().set_component_count(components);
        Some(Self {
            based,
            profile,
            ranges,
        })
    }

    pub fn ranges(&self) -> &[f32] {
        &self.ranges
    }
}

impl CpdfColorSpace for CpdfIccBasedCs {
    fn data(&self) -> &CpdfColorSpaceData {
        self.based.data()
    }

    fn get_icc_profile(&self) -> Option<&CpdfIccProfile> {
        Some(&self.profile)
    }

    fn enable_std_conversion(&self, enabled: bool) {
        self.based.enable_std_conversion(enabled);
    }

    fn get_rgb(&self, values: &[f32]) -> Option<FxRgbStruct<f32>> {
        if self.profile.is_srgb() {
            return Some(FxRgbStruct {
                red: *values.first()?,
                green: *values.get(1)?,
                blue: *values.get(2)?,
            });
        }
        if self.profile.is_supported() {
            let mut rgb = [0.0; 3];
            self.profile
                .translate(values.get(..self.component_count() as usize)?, &mut rgb)
                .then_some(FxRgbStruct {
                    red: rgb[0],
                    green: rgb[1],
                    blue: rgb[2],
                })
        } else if let Some(base_cs) = self.based.base_cs() {
            base_cs.get_rgb(values)
        } else {
            Some(FxRgbStruct::default())
        }
    }

    fn translate_image_line(
        &self,
        dest: &mut [u8],
        src: &[u8],
        pixels: usize,
        width: i32,
        height: i32,
        trans_mask: bool,
    ) {
        assert!(!trans_mask);
        if self.profile.is_srgb() {
            assert!(src.len() >= pixels * 3 && dest.len() >= pixels * 3);
            for (source, target) in src
                .chunks_exact(3)
                .zip(dest.chunks_exact_mut(3))
                .take(pixels)
            {
                target.copy_from_slice(&[source[2], source[1], source[0]]);
            }
        } else if let Some(base_cs) = self.based.base_cs() {
            base_cs.translate_image_line(dest, src, pixels, width, height, false);
        } else {
            assert!(
                self.profile.translate_scanline(dest, src, pixels),
                "ICC scanline transform failed"
            );
        }
    }

    fn is_normal(&self) -> bool {
        if self.profile.is_srgb() {
            true
        } else if self.profile.is_supported() {
            self.profile.is_normal()
        } else {
            self.based.base_cs().is_some_and(|space| space.is_normal())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::cpdf_colorspace::load_supported_cs;
    use super::*;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::{make_reference, retain_object};
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn invalid_icc_uses_matching_alternate_then_stock() {
        let holder = CpdfIndirectObjectHolder::new();
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"N", CpdfScalarObject::number_from_signed(3));
        dict.set_new_name_for(b"Alternate", b"DeviceRGB");
        let stream = retain_object(CpdfStream::from_data_and_dict(
            vec![1, 2, 3],
            retain_object(dict),
        ));
        holder.borrow_mut().add_indirect_object(stream.clone());
        let mut array = CpdfArray::new();
        array.append_new_name(b"ICCBased");
        array.append(make_reference(&stream, &holder));
        let object = retain_object(array);
        let cs = load_supported_cs(&object).unwrap();
        assert!(!cs.get_icc_profile().unwrap().is_valid());
        assert!(cs.is_normal());
        assert_eq!(
            cs.get_rgb(&[0.25, 0.5, 0.75]),
            Some(FxRgbStruct {
                red: 0.25,
                green: 0.5,
                blue: 0.75
            })
        );
        let mut bgr = [0; 3];
        cs.translate_image_line(&mut bgr, &[10, 20, 30], 1, 1, 1, false);
        assert_eq!(bgr, [30, 20, 10]);

        let dict = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_dict();
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_name_for(b"Alternate", b"DeviceGray");
        let cs = load_supported_cs(&object).unwrap();
        assert_eq!(
            cs.get_rgb(&[0.25, 0.5, 0.75]),
            Some(FxRgbStruct {
                red: 0.25,
                green: 0.5,
                blue: 0.75
            })
        );
    }
}
