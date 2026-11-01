// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_iccprofile.{h,cpp}`.

use std::cell::RefCell;

use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream_acc::{CpdfRetainedStreamAcc, RetainedStreamSpan};
use crate::fxcodec::icc::icc_transform::IccTransform;

pub struct CpdfIccProfile {
    stream_acc: CpdfRetainedStreamAcc,
    transform: Option<IccTransform>,
    is_srgb: bool,
    src_components: u32,
    decoded_scanline: RefCell<Option<Box<DecodedScanline>>>,
}

#[derive(Default)]
struct DecodedScanline {
    key: Vec<[u32; 2]>,
    tables: Vec<[u8; 256]>,
    encoded: Vec<u8>,
}

impl CpdfIccProfile {
    pub fn new(stream: &ObjectRef, expected_components: u32) -> Option<Self> {
        Self::from_accessor(
            CpdfRetainedStreamAcc::load_filtered(stream.clone())?,
            expected_components,
        )
    }

    pub fn from_accessor(
        stream_acc: CpdfRetainedStreamAcc,
        expected_components: u32,
    ) -> Option<Self> {
        let data = stream_acc.get_span();
        let is_srgb = expected_components == 3
            && data.len() == 3144
            && &data[400..400 + b"sRGB IEC61966-2.1".len()] == b"sRGB IEC61966-2.1";
        let transform = if is_srgb {
            None
        } else {
            IccTransform::create_transform_srgb(&data)
        };
        let src_components = if is_srgb {
            3
        } else {
            transform
                .as_ref()
                .filter(|value| value.components() == expected_components as usize)
                .map_or(0, |_| expected_components)
        };
        let transform =
            transform.filter(|value| value.components() == expected_components as usize);
        drop(data);
        Some(Self {
            stream_acc,
            transform,
            is_srgb,
            src_components,
            decoded_scanline: RefCell::new(None),
        })
    }

    pub fn is_valid(&self) -> bool {
        self.is_srgb() || self.is_supported()
    }
    pub fn is_srgb(&self) -> bool {
        self.is_srgb
    }
    pub fn is_supported(&self) -> bool {
        self.transform.is_some()
    }
    pub fn components(&self) -> u32 {
        self.src_components
    }
    pub fn stream(&self) -> &ObjectRef {
        self.stream_acc.get_stream()
    }
    pub fn data(&self) -> RetainedStreamSpan<'_> {
        self.stream_acc.get_span()
    }

    pub fn is_normal(&self) -> bool {
        self.transform
            .as_ref()
            .expect("IsNormal requires a supported ICC profile")
            .is_normal()
    }

    pub fn translate(&self, src: &[f32], dest: &mut [f32; 3]) -> bool {
        self.transform
            .as_ref()
            .is_some_and(|value| value.translate(src, dest))
    }

    /// Match per-pixel GetRGB's byte encoding after PDF Decode, while using
    /// the same executor on an entire row. The cache belongs to this profile,
    /// so non-ICC image objects retain their original layout and hot path.
    pub(crate) fn translate_decoded_scanline(
        &self,
        dest: &mut [u8],
        src: &[u8],
        pixels: usize,
        decode: &[[f32; 2]],
    ) -> bool {
        let Some(transform) = self.transform.as_ref() else {
            return false;
        };
        let comps = transform.components();
        if !transform.is_normal() || decode.len() != comps {
            return false;
        }
        let Some(size) = pixels.checked_mul(comps) else {
            return false;
        };
        if src.len() < size || dest.len() / 3 < pixels {
            return false;
        }
        let mut slot = self.decoded_scanline.borrow_mut();
        let cache = slot.get_or_insert_with(Default::default);
        let same_key = cache.key.len() == comps
            && cache
                .key
                .iter()
                .zip(decode)
                .all(|(key, value)| *key == [value[0].to_bits(), value[1].to_bits()]);
        if !same_key {
            cache.key = decode
                .iter()
                .map(|v| [v[0].to_bits(), v[1].to_bits()])
                .collect();
            cache.tables = decode
                .iter()
                .map(|v| {
                    std::array::from_fn(|sample| {
                        crate::fxcodec::icc::icc_transform::encode_normal_component(
                            v[0] + v[1] * sample as f32,
                        )
                    })
                })
                .collect();
        }
        cache.encoded.resize(size, 0);
        for (input, output) in src[..size]
            .chunks_exact(comps)
            .zip(cache.encoded.chunks_exact_mut(comps))
        {
            for (component, table) in cache.tables.iter().enumerate() {
                output[component] = table[input[component] as usize];
            }
        }
        transform.translate_scanline(dest, &cache.encoded, pixels)
    }

    pub fn translate_scanline(&self, dest: &mut [u8], src: &[u8], pixels: usize) -> bool {
        self.transform
            .as_ref()
            .is_some_and(|value| value.translate_scanline(dest, src, pixels))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    #[test]
    fn pdfium_srgb_signature_and_component_count() {
        let mut bytes = vec![0; 3144];
        bytes[400..417].copy_from_slice(b"sRGB IEC61966-2.1");
        let stream = retain_object(CpdfStream::from_bytes(&bytes));
        let srgb = CpdfIccProfile::new(&stream, 3).unwrap();
        assert!(srgb.is_valid());
        assert!(srgb.is_srgb());
        assert!(!srgb.is_supported());
        assert_eq!(srgb.components(), 3);
        assert_eq!(&*srgb.data(), bytes);
        let wrong_components = CpdfIccProfile::new(&stream, 4).unwrap();
        assert!(!wrong_components.is_valid());
        assert_eq!(wrong_components.components(), 0);
    }
}
