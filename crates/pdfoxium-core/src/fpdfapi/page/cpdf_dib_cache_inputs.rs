// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Owned decoder inputs for the page cache's reuse proof.
use super::*;
use crate::fpdfapi::page::cpdf_image::{CpdfImage, CpdfImageLoadContext};

#[derive(Clone)]
enum MaskInput {
    None,
    Inline,
    External(ObjectRef, Option<ColorSpaceRef>),
}

#[derive(Clone)]
pub(crate) struct ImageCacheInputs {
    // A higher-detail plane is not interchangeable with a lower-detail JPEG/JPX
    // decode: resampling it can produce different pixels. Keep the normalized
    // requested level, not target dimensions, so extents in one bucket still hit.
    resolution_request: u8,
    color_space: Option<ColorSpaceRef>,
    mask: MaskInput,
    std_cs: bool,
    group_family: ColorSpaceFamily,
    load_mask: bool,
    conversion_revision: u64,
}

pub(super) fn resolution_levels(width: i32, height: i32, size: CfxSize) -> u8 {
    if size.width != 0 && size.height != 0 {
        (width / size.width)
            .min(height / size.height)
            .max(1)
            .ilog2() as u8
    } else {
        0
    }
}

pub(super) fn cache_resolution_levels(filter: &[u8], levels: u8) -> u8 {
    match filter {
        b"DCTDecode" => levels.min(3),
        b"JPXDecode" => levels,
        _ => 0,
    }
}

fn same_space(a: &Option<ColorSpaceRef>, b: &Option<ColorSpaceRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl ImageCacheInputs {
    // Resolving references/profile streams can call the embedder. Call this
    // with no page-cache, entry, or current-state borrow held.
    pub(crate) fn for_request(
        image: &CpdfImage,
        context: CpdfImageLoadContext<'_>,
    ) -> Option<Self> {
        let color_space = image.load_color_space(context);
        let dictionary = image.dict();
        let (inline, soft_mask, mask, resolution_request) = {
            let borrowed = dictionary.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            let decoders = get_decoder_array(dict)?;
            let filter = decoders
                .last()
                .map(|(name, _)| crate::fxcrt::bytestring::c_str(name))
                .unwrap_or_default();
            let width = dict.get_integer_for(b"Width", 0);
            let height = dict.get_integer_for(b"Height", 0);
            if width <= 0 || height <= 0 {
                return None;
            }
            (
                dict.get_object_for(b"SMaskInData"),
                dict.get_object_for(b"SMask"),
                dict.get_object_for(b"Mask"),
                cache_resolution_levels(filter, resolution_levels(width, height, context.max_size)),
            )
        };
        let direct = crate::fpdfapi::parser::cpdf_object::get_direct_object;
        let inline = inline
            .and_then(|object| direct(&object))
            .is_some_and(|object| object.borrow().get_integer() != 0);
        let stream = |object: Option<ObjectRef>| {
            object
                .and_then(|object| direct(&object))
                .filter(|object| object.borrow().as_any().is::<CpdfStream>())
        };
        let external = stream(soft_mask).or_else(|| stream(mask));
        let mask = if inline {
            MaskInput::Inline
        } else if let Some(stream) = external {
            let dictionary = stream
                .borrow()
                .as_any()
                .downcast_ref::<CpdfStream>()?
                .get_dict();
            let object = {
                let borrowed = dictionary.borrow();
                borrowed
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()?
                    .get_object_for(b"ColorSpace")
            };
            let space = object.and_then(|object| context.page_data.get_color_space(&object, None));
            MaskInput::External(stream, space)
        } else {
            MaskInput::None
        };
        Some(Self {
            resolution_request,
            color_space,
            mask,
            std_cs: context.std_cs,
            group_family: context.group_family,
            load_mask: context.load_mask,
            conversion_revision: super::super::cpdf_colorspace::cmyk_conversion_revision()?,
        })
    }

    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.resolution_request == other.resolution_request
            && self.std_cs == other.std_cs
            && self.group_family == other.group_family
            && self.load_mask == other.load_mask
            && self.conversion_revision == other.conversion_revision
            && same_space(&self.color_space, &other.color_space)
            && match (&self.mask, &other.mask) {
                (MaskInput::None, MaskInput::None) | (MaskInput::Inline, MaskInput::Inline) => true,
                (MaskInput::External(a, acs), MaskInput::External(b, bcs)) => {
                    Rc::ptr_eq(a, b) && same_space(acs, bcs)
                }
                _ => false,
            }
    }

    pub(crate) fn conversion_is_unchanged(&self) -> bool {
        super::super::cpdf_colorspace::cmyk_conversion_revision() == Some(self.conversion_revision)
    }
}

impl CpdfDib {
    // Preserve the earlier huge-image DeviceRGB/Gray optimization. These
    // lazy planes need no context-dependent color conversion; other spaces
    // must be realized before the new full-resolution hit is enabled.
    pub(crate) fn has_unmasked_device_color_space(&self) -> bool {
        if self.image_mask
            || self.inline_mask.borrow().is_some()
            || self.external_mask.borrow().is_some()
        {
            return false;
        }
        let stream = self.stream_acc.get_stream().borrow();
        let Some(stream) = stream.as_any().downcast_ref::<CpdfStream>() else {
            return false;
        };
        let dictionary = stream.get_dict();
        let borrowed = dictionary.borrow();
        let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
            return false;
        };
        !dict.key_exist(b"SMask")
            && !dict.key_exist(b"Mask")
            && !dict.key_exist(b"SMaskInData")
            && matches!(
                (self.family, dict.get_name_for(b"ColorSpace").as_slice()),
                (ColorSpaceFamily::DeviceRgb, b"DeviceRGB")
                    | (ColorSpaceFamily::DeviceGray, b"DeviceGray")
            )
    }

    fn has_all_source_detail(&self) -> bool {
        let size = self.cache_native_size;
        size.width > 0
            && size.height > 0
            && self.width >= size.width
            && self.height >= size.height
            && (self.resolution_levels_to_skip == 0
                || !matches!(
                    crate::fxcrt::bytestring::c_str(self.stream_acc.get_image_decoder()),
                    b"DCTDecode" | b"JPXDecode"
                ))
    }

    pub(crate) fn cache_size_was_set(&self) -> bool {
        self.cache_size_request_set
    }

    pub(crate) fn has_full_source_detail(&self) -> bool {
        self.has_all_source_detail()
            && self
                .inline_mask
                .borrow()
                .as_ref()
                .is_none_or(|mask| mask.width() >= self.width && mask.height() >= self.height)
            && self
                .external_mask
                .borrow()
                .as_ref()
                .is_none_or(|mask| mask.has_all_source_detail())
    }

    // Build from actual inputs owned by this completed DIB, including an
    // external mask's original input space. Pending loads therefore keep their
    // own proof through callbacks; a failed reload cannot replace a cached key.
    pub(crate) fn cache_inputs(&self) -> Option<ImageCacheInputs> {
        let revision = self.cache_conversion_revision?;
        if super::super::cpdf_colorspace::cmyk_conversion_revision() != Some(revision) {
            return None;
        }
        let mask = if self.has_mask && self.inline_mask.borrow().is_some() {
            MaskInput::Inline
        } else if let Some(mask) = self.external_mask.borrow().as_ref() {
            MaskInput::External(
                Rc::clone(mask.stream_acc.get_stream()),
                mask.cache_input_color_space.as_ref().map(Rc::clone),
            )
        } else {
            // A missing/unresolved mask is not evidence of a complete decode.
            // In particular, two failed reference lookups must not turn a
            // declared soft mask into the same proof as an absent mask.
            if self.has_mask {
                let stream = self.stream_acc.get_stream().borrow();
                let stream = stream.as_any().downcast_ref::<CpdfStream>()?;
                let dictionary = stream.get_dict();
                let borrowed = dictionary.borrow();
                let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
                if dict.key_exist(b"SMask")
                    || dict.key_exist(b"SMaskInData")
                    || (dict.key_exist(b"Mask") && !self.color_key)
                {
                    return None;
                }
            }
            MaskInput::None
        };
        Some(ImageCacheInputs {
            resolution_request: self.cache_resolution_request,
            color_space: self.cache_input_color_space.as_ref().map(Rc::clone),
            mask,
            std_cs: self.std_cs,
            group_family: self.group_family,
            load_mask: self.load_mask,
            conversion_revision: revision,
        })
    }
}
