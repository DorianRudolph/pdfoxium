// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Image metadata and DIB ownership from `core/fpdfapi/page/cpdf_image.{h,cpp}`.
//! CpdfDib owns decoding; image state retains pending jobs and transfers ready
//! bitmap/mask handles through StartLoadDIBBase, Continue and Detach.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::cpdf_colorspace::{ColorSpaceFamily, ColorSpaceRef};
use super::cpdf_dib::{CpdfDib, LoadState};
use super::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_coordinates::CfxSize;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::cstretchengine::StretchSource;
use crate::fxge::dib::fx_dib::FxdibFormat;

/// The resource scopes and size request passed from RenderContext through
/// ImageLoader/PageImageCache to DIB loading. All references are borrowed.
#[derive(Clone, Copy)]
pub struct CpdfImageLoadContext<'a> {
    pub page_data: &'a CpdfDocPageData,
    pub form_resources: Option<&'a ObjectRef>,
    pub page_resources: Option<&'a ObjectRef>,
    pub max_size: CfxSize,
    pub std_cs: bool,
    pub group_family: ColorSpaceFamily,
    pub load_mask: bool,
}

impl<'a> CpdfImageLoadContext<'a> {
    pub fn new(page_data: &'a CpdfDocPageData, page_resources: Option<&'a ObjectRef>) -> Self {
        Self {
            page_data,
            form_resources: None,
            page_resources,
            max_size: CfxSize::new(0, 0),
            std_cs: false,
            group_family: ColorSpaceFamily::Unknown,
            load_mask: false,
        }
    }
}

/// CFX_DIBBase image handle: either retained PDF scanlines or realized pixels.
/// The enum keeps scanline dispatch static and borrows each row without copying.
pub enum CpdfImageBitmap {
    Dib(Box<CpdfDib>),
    Bitmap(CfxDibitmap),
    Transfer(super::cpdf_transferfuncdib::CpdfTransferFuncDib<CpdfImageBitmap>),
}

pub enum ImageScanline<'a> {
    Transfer(std::cell::RefMut<'a, [u8]>),
    Dib(super::cpdf_dib::DibScanline<'a>),
    Bitmap(&'a [u8]),
}

impl std::ops::Deref for ImageScanline<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Dib(row) => row,
            Self::Transfer(row) => row,
            Self::Bitmap(row) => row,
        }
    }
}

impl CpdfImageBitmap {
    pub fn width(&self) -> i32 {
        match self {
            Self::Dib(dib) => dib.width(),
            Self::Bitmap(bitmap) => bitmap.width(),
            Self::Transfer(transfer) => transfer.width(),
        }
    }
    pub fn height(&self) -> i32 {
        match self {
            Self::Dib(dib) => dib.height(),
            Self::Bitmap(bitmap) => bitmap.height(),
            Self::Transfer(transfer) => transfer.height(),
        }
    }
    pub fn pitch(&self) -> u32 {
        match self {
            Self::Dib(dib) => dib.pitch(),
            Self::Bitmap(bitmap) => bitmap.pitch(),
            Self::Transfer(transfer) => transfer.pitch(),
        }
    }
    pub fn format(&self) -> FxdibFormat {
        match self {
            Self::Dib(dib) => dib.format(),
            Self::Bitmap(bitmap) => bitmap.format(),
            Self::Transfer(transfer) => transfer.format(),
        }
    }
    pub fn palette(&self) -> &[u32] {
        match self {
            Self::Dib(dib) => dib.palette(),
            Self::Bitmap(bitmap) => bitmap.palette(),
            Self::Transfer(transfer) => transfer.palette(),
        }
    }
    pub fn is_mask_format(&self) -> bool {
        crate::fxge::dib::fx_dib::get_is_mask_from_format(self.format())
    }
    pub fn is_opaque_image(&self) -> bool {
        !self.is_mask_format() && self.format() != FxdibFormat::Bgra
    }
    pub fn realize(&self) -> Option<CfxDibitmap> {
        crate::fxge::dib::cfx_dibbase::realize(self)
    }
    pub fn estimated_image_memory_burden(&self) -> usize {
        match self {
            Self::Dib(dib) => dib.estimated_image_memory_burden(),
            Self::Bitmap(bitmap) => bitmap.estimated_image_memory_burden(),
            Self::Transfer(transfer) => transfer.estimated_image_memory_burden(),
        }
    }
    pub(crate) fn make_cached(self: Rc<Self>) -> Option<Rc<Self>> {
        if matches!(self.as_ref(), Self::Bitmap(_)) {
            return Some(self);
        }
        Some(Rc::new(Self::Bitmap(self.realize()?)))
    }
}

impl StretchSource for CpdfImageBitmap {
    type Row<'a> = ImageScanline<'a>;
    type Palette<'a> = &'a [u32];
    fn width(&self) -> i32 {
        self.width()
    }
    fn height(&self) -> i32 {
        self.height()
    }
    fn format(&self) -> FxdibFormat {
        self.format()
    }
    fn pitch(&self) -> u32 {
        self.pitch()
    }
    fn palette(&self) -> &[u32] {
        self.palette()
    }
    fn scanline(&self, row: i32) -> ImageScanline<'_> {
        match self {
            Self::Dib(dib) => ImageScanline::Dib(dib.get_scanline(row)),
            Self::Transfer(transfer) => ImageScanline::Transfer(transfer.get_scanline(row)),
            Self::Bitmap(bitmap) => ImageScanline::Bitmap(bitmap.scanline(row).expect("image row")),
        }
    }
    fn skip_to_scanline(
        &self,
        row: i32,
        pause: Option<&mut dyn crate::fxcrt::pauseindicator_iface::PauseIndicatorIface>,
    ) -> bool {
        // The retained image remains a native CPDF_DIB through CFX_DIBBase's
        // virtual SkipToScanline call. Realized and transfer bitmaps retain
        // CFX_DIBBase's default instead.
        match self {
            Self::Dib(dib) => dib.skip_to_scanline(row, pause),
            Self::Transfer(_) | Self::Bitmap(_) => false,
        }
    }
    fn estimated_image_memory_burden(&self) -> usize {
        self.estimated_image_memory_burden()
    }
}

/// Retained color/mask planes and matte state. Raw samples use PDF scanlines;
/// codec samples and mask continuation stay attached to the DIB until ready.
#[derive(Clone)]
pub struct CpdfImageData {
    pub bitmap: Rc<CpdfImageBitmap>,
    pub mask: Option<Rc<CpdfImageBitmap>>,
    pub matte_color: u32,
}

impl CpdfImageData {
    pub fn estimated_memory_burden(&self) -> usize {
        self.bitmap.estimated_image_memory_burden()
            + self
                .mask
                .as_ref()
                .map_or(0, |mask| mask.estimated_image_memory_burden())
    }
}

enum ImageLoadBitmap {
    Pending(Box<CpdfDib>),
    Ready(Rc<CpdfImageBitmap>),
}

pub struct CpdfImage {
    // Native GetDocument identity without retaining the document graph.
    document_owner: Option<Weak<RefCell<CpdfIndirectObjectHolder>>>,
    height: Cell<i32>,
    width: Cell<i32>,
    matte_color: Cell<u32>,
    is_inline: Cell<bool>,
    is_mask: Cell<bool>,
    interpolate: bool,
    will_be_destroyed: Cell<bool>,
    stream: RefCell<ObjectRef>,
    initialized: Cell<bool>,
    oc: Option<ObjectRef>,
    loaded_bitmap: RefCell<Option<ImageLoadBitmap>>,
    loaded_mask: RefCell<Option<Rc<CpdfImageBitmap>>>,
}

impl CpdfImage {
    /// The document-only constructor has no image stream until SetImage/JPEG.
    /// Keep an empty private stream as storage so static rendering callers can
    /// borrow a stream uniformly; `has_stream()` distinguishes the native null.
    pub fn empty() -> Self {
        let image = Self::from_stream(
            crate::fpdfapi::parser::cpdf_object::retain_object(CpdfStream::from_bytes(&[])),
            false,
        )
        .expect("empty stream dictionary");
        image.initialized.set(false);
        image.is_mask.set(false);
        image
    }

    pub fn empty_with_page_data(page_data: &CpdfDocPageData) -> Self {
        let mut image = Self::empty();
        image.document_owner = page_data.document_owner_identity();
        image
    }
    pub fn from_stream_with_page_data(
        stream: ObjectRef,
        is_inline: bool,
        page_data: &CpdfDocPageData,
    ) -> Option<Self> {
        let mut image = Self::from_stream(stream, is_inline)?;
        image.document_owner = page_data.document_owner_identity();
        Some(image)
    }
    pub(crate) fn document_owner_identity(
        &self,
    ) -> Option<&Weak<RefCell<CpdfIndirectObjectHolder>>> {
        self.document_owner.as_ref()
    }
    pub fn reset_cache(&self, page: &super::cpdf_page::CpdfPage) {
        page.image_cache().reset_bitmap_for_image(self);
    }

    fn replace_stream(&self, stream: CpdfStream, width: i32, height: i32, is_mask: bool) {
        *self.stream.borrow_mut() = crate::fpdfapi::parser::cpdf_object::retain_object(stream);
        self.width.set(width);
        self.height.set(height);
        self.is_mask.set(is_mask);
        self.initialized.set(true);
        // Replacing an inline image creates an external image stream, which
        // content generation must emit as an XObject rather than discard.
        self.is_inline.set(false);
        self.loaded_bitmap.borrow_mut().take();
        self.loaded_mask.borrow_mut().take();
    }

    fn image_dictionary(width: i32, height: i32) -> CpdfDictionary {
        use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
        let mut dictionary = CpdfDictionary::new();
        dictionary.set_new_name_for(b"Type", b"XObject");
        dictionary.set_new_name_for(b"Subtype", b"Image");
        dictionary.set_new_for(b"Width", CpdfScalarObject::number_from_signed(width));
        dictionary.set_new_for(b"Height", CpdfScalarObject::number_from_signed(height));
        dictionary
    }

    fn jpeg_dictionary(data: &[u8]) -> Option<(CpdfDictionary, i32, i32)> {
        use crate::fpdfapi::parser::cpdf_array::CpdfArray;
        use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
        let info = crate::fxcodec::jpeg::load_info(data)?;
        if !Self::is_valid_jpeg_component(info.components as i32)
            || !Self::is_valid_jpeg_bits_per_component(info.bits_per_components)
        {
            return None;
        }
        let mut dictionary = Self::image_dictionary(info.width, info.height);
        dictionary.set_new_name_for(
            b"ColorSpace",
            match info.components {
                1 => b"DeviceGray",
                3 => b"DeviceRGB",
                4 => b"DeviceCMYK",
                _ => unreachable!(),
            },
        );
        if info.components == 4 {
            let mut decode = CpdfArray::new();
            for _ in 0..4 {
                decode.append_new(CpdfScalarObject::number_from_signed(1));
                decode.append_new(CpdfScalarObject::number_from_signed(0));
            }
            dictionary.set_new_for(b"Decode", decode);
        }
        dictionary.set_new_for(
            b"BitsPerComponent",
            CpdfScalarObject::number_from_signed(info.bits_per_components),
        );
        dictionary.set_new_name_for(b"Filter", b"DCTDecode");
        if !info.color_transform {
            let mut parameters = CpdfDictionary::new();
            parameters.set_new_for(b"ColorTransform", CpdfScalarObject::number_from_signed(0));
            dictionary.set_new_for(b"DecodeParms", parameters);
        }
        Some((dictionary, info.width, info.height))
    }

    /// Header reads mirror InitJPEG's 8192-byte first pass. File mode retains
    /// the Rust read-stream adapter; inline mode owns the complete byte vector.
    pub fn set_jpeg_image(
        &self,
        file: Rc<dyn crate::fxcrt::fx_stream::SeekableReadStream>,
        inline: bool,
    ) -> bool {
        use crate::fpdfapi::parser::cpdf_object::retain_object;
        let Ok(size) = usize::try_from(file.get_size()) else {
            return false;
        };
        if size == 0 || size > i32::MAX as usize {
            return false;
        }
        let first_size = if inline { size } else { size.min(8192) };
        let mut data = vec![0; first_size];
        if !file.read_block_at_offset(&mut data, 0) {
            return false;
        }
        let mut metadata = Self::jpeg_dictionary(&data);
        if metadata.is_none() && size > first_size {
            data.resize(size, 0);
            if file.read_block_at_offset(&mut data, 0) {
                metadata = Self::jpeg_dictionary(&data);
            }
        }
        let Some((dictionary, width, height)) = metadata else {
            return false;
        };
        let stream = if inline {
            CpdfStream::from_data_and_dict(data, retain_object(dictionary))
        } else {
            CpdfStream::from_file(file, retain_object(dictionary))
        };
        self.replace_stream(stream, width, height, false);
        true
    }

    /// SetImage: write packed PDF samples, indexed palettes and an indirect
    /// grayscale SMask. The original bitmap is borrowed only while encoding.
    pub fn set_bitmap<S: crate::fxge::dib::cstretchengine::StretchSource>(
        &self,
        bitmap: &S,
        objects: &HolderRef,
    ) -> bool {
        use crate::fpdfapi::parser::cpdf_array::CpdfArray;
        use crate::fpdfapi::parser::cpdf_object::{make_reference, retain_object};
        use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
        use crate::fpdfapi::parser::cpdf_string::CpdfString;
        use crate::fxge::dib::fx_dib::{argb_a, argb_b, argb_g, argb_r};
        let (width, height) = (bitmap.width(), bitmap.height());
        if width < 1 || height < 1 {
            return false;
        }
        let mut dictionary = Self::image_dictionary(width, height);
        let bpp = crate::fxge::dib::fx_dib::get_bpp_from_format(bitmap.format());
        if !matches!(bpp, 1 | 8 | 24 | 32) {
            return false;
        }
        let Some(row_bits) = (width as usize).checked_mul(bpp as usize) else {
            return false;
        };
        let row_bytes = row_bits.div_ceil(8);
        if (0..height).any(|row| bitmap.scanline(row).len() < row_bytes) {
            return false;
        }
        let palette_argb = |index: usize| {
            let palette = bitmap.palette();
            if !palette.is_empty() {
                return palette[index];
            }
            let value = if bpp == 1 {
                u32::from(index != 0) * 255
            } else {
                index as u32
            };
            0xff000000 | (value << 16) | (value << 8) | value
        };
        let destination_pitch;
        if bpp == 1 {
            let (reset, set) = if crate::fxge::dib::fx_dib::get_is_mask_from_format(bitmap.format())
            {
                (0, 0)
            } else {
                (palette_argb(0), palette_argb(1))
            };
            if argb_a(reset) == 0 || argb_a(set) == 0 {
                dictionary.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
                if argb_a(reset) == 0 {
                    let mut decode = CpdfArray::new();
                    decode.append_new(CpdfScalarObject::number_from_signed(1));
                    decode.append_new(CpdfScalarObject::number_from_signed(0));
                    dictionary.set_new_for(b"Decode", decode);
                }
            } else {
                let mut space = CpdfArray::new();
                space.append_new(crate::fpdfapi::parser::cpdf_name::CpdfName::new(
                    None, b"Indexed",
                ));
                space.append_new(crate::fpdfapi::parser::cpdf_name::CpdfName::new(
                    None,
                    b"DeviceRGB",
                ));
                space.append_new(CpdfScalarObject::number_from_signed(1));
                space.append_new(CpdfString::from_bytes(
                    None,
                    &[
                        argb_r(reset),
                        argb_g(reset),
                        argb_b(reset),
                        argb_r(set),
                        argb_g(set),
                        argb_b(set),
                    ],
                ));
                dictionary.set_new_for(b"ColorSpace", space);
            }
            dictionary.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(1));
            destination_pitch = (width as usize).div_ceil(8);
        } else if bpp == 8 {
            let palette_size =
                crate::fxge::dib::cfx_dibbase::required_palette_size(bitmap.format());
            if palette_size > 0 {
                let mut space = CpdfArray::new();
                space.append_new(crate::fpdfapi::parser::cpdf_name::CpdfName::new(
                    None, b"Indexed",
                ));
                space.append_new(crate::fpdfapi::parser::cpdf_name::CpdfName::new(
                    None,
                    b"DeviceRGB",
                ));
                space.append_new(CpdfScalarObject::number_from_signed(
                    (palette_size - 1) as i32,
                ));
                let mut table = Vec::with_capacity(palette_size * 3);
                for index in 0..palette_size {
                    let color = palette_argb(index);
                    table.extend_from_slice(&[argb_r(color), argb_g(color), argb_b(color)]);
                }
                let stream = retain_object(CpdfStream::from_data_and_dict(
                    table,
                    retain_object(CpdfDictionary::new()),
                ));
                objects.borrow_mut().add_indirect_object(stream.clone());
                space.append(make_reference(&stream, objects));
                let space = retain_object(space);
                objects.borrow_mut().add_indirect_object(space.clone());
                dictionary.set_for(b"ColorSpace", Some(make_reference(&space, objects)));
            } else {
                dictionary.set_new_name_for(b"ColorSpace", b"DeviceGray");
            }
            dictionary.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
            destination_pitch = width as usize;
        } else {
            dictionary.set_new_name_for(b"ColorSpace", b"DeviceRGB");
            dictionary.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
            destination_pitch = width as usize * 3;
        }
        if crate::fxge::dib::fx_dib::get_is_alpha_from_format(bitmap.format()) {
            let Some(mask) = crate::fxge::dib::cfx_dibbase::clone_alpha_mask(bitmap) else {
                return false;
            };
            let mut data = Vec::with_capacity(width as usize * height as usize);
            for row in 0..height {
                let Some(source) = mask.scanline(row) else {
                    return false;
                };
                data.extend_from_slice(&source[..width as usize]);
            }
            let mut mask_dictionary = Self::image_dictionary(width, height);
            mask_dictionary.set_new_name_for(b"ColorSpace", b"DeviceGray");
            mask_dictionary
                .set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
            let stream = retain_object(CpdfStream::from_data_and_dict(
                data,
                retain_object(mask_dictionary),
            ));
            objects.borrow_mut().add_indirect_object(stream.clone());
            dictionary.set_for(b"SMask", Some(make_reference(&stream, objects)));
        }
        let Some(size) = destination_pitch
            .checked_mul(height as usize)
            .filter(|size| *size <= i32::MAX as usize)
        else {
            return false;
        };
        let mut data = vec![0; size];
        for row in 0..height {
            let source = bitmap.scanline(row);
            let destination =
                &mut data[row as usize * destination_pitch..(row as usize + 1) * destination_pitch];
            if bpp <= 8 {
                destination.copy_from_slice(&source[..destination_pitch]);
            } else {
                for (pixel, output) in source
                    .chunks_exact((bpp / 8) as usize)
                    .take(width as usize)
                    .zip(destination.chunks_exact_mut(3))
                {
                    output.copy_from_slice(&[pixel[2], pixel[1], pixel[0]]);
                }
            }
        }
        self.replace_stream(
            CpdfStream::from_data_and_dict(data, retain_object(dictionary)),
            width,
            height,
            crate::fxge::dib::fx_dib::get_is_mask_from_format(bitmap.format()),
        );
        true
    }
    pub fn is_valid_jpeg_component(comps: i32) -> bool {
        matches!(comps, 1 | 3 | 4)
    }
    pub fn is_valid_jpeg_bits_per_component(bpc: i32) -> bool {
        matches!(bpc, 1 | 2 | 4 | 8 | 16)
    }

    pub fn from_stream(stream: ObjectRef, is_inline: bool) -> Option<Self> {
        let dict = {
            let borrowed = stream.borrow();
            borrowed.as_any().downcast_ref::<CpdfStream>()?.get_dict()
        };
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        Some(Self {
            document_owner: None,
            height: Cell::new(dict.get_integer_for(b"Height", 0)),
            width: Cell::new(dict.get_integer_for(b"Width", 0)),
            matte_color: Cell::new(0),
            is_inline: Cell::new(is_inline),
            is_mask: Cell::new(
                !dict.key_exist(b"ColorSpace") || dict.get_boolean_for(b"ImageMask", false),
            ),
            interpolate: dict.get_integer_for(b"Interpolate", 0) != 0,
            will_be_destroyed: Cell::new(false),
            oc: dict.get_dict_for(b"OC"),
            loaded_bitmap: RefCell::new(None),
            loaded_mask: RefCell::new(None),
            stream: RefCell::new(stream),
            initialized: Cell::new(true),
        })
    }
    pub fn dict(&self) -> ObjectRef {
        let stream = self.stream();
        let borrowed = stream.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfStream>()
            .expect("image stream")
            .get_dict()
    }
    pub fn stream(&self) -> ObjectRef {
        Rc::clone(&self.stream.borrow())
    }
    pub fn has_stream(&self) -> bool {
        self.initialized.get()
    }
    /// `CPDF_Image::ConvertStreamToIndirectObject()`. The holder is the
    /// document's borrowed ownership context, replacing the native backpointer.
    pub fn convert_stream_to_indirect_object(&self, objects: &HolderRef) {
        let stream = self.stream();
        assert!(stream.borrow().is_inline());
        objects.borrow_mut().add_indirect_object(stream);
    }
    pub fn oc(&self) -> Option<ObjectRef> {
        self.oc.clone()
    }
    pub fn pixel_height(&self) -> i32 {
        self.height.get()
    }
    pub fn pixel_width(&self) -> i32 {
        self.width.get()
    }
    pub fn matte_color(&self) -> u32 {
        self.matte_color.get()
    }
    pub fn is_inline(&self) -> bool {
        self.is_inline.get()
    }
    pub fn is_mask(&self) -> bool {
        self.is_mask.get()
    }
    pub fn is_interpol(&self) -> bool {
        self.interpolate
    }
    pub fn will_be_destroyed(&self) {
        self.will_be_destroyed.set(true);
    }
    pub fn is_going_to_be_destroyed(&self) -> bool {
        self.will_be_destroyed.get()
    }

    pub fn decode_without_mask(&self, resources: Option<&ObjectRef>) -> Option<CfxDibitmap> {
        let page_data = CpdfDocPageData::new();
        let (mut source, mut state) =
            self.start_dib(CpdfImageLoadContext::new(&page_data, resources), false)?;
        while state == LoadState::Continue {
            state = source.continue_load(None);
        }
        if state != LoadState::Success {
            return None;
        }
        crate::fxge::dib::cfx_dibbase::realize(source.as_ref())
    }

    pub(crate) fn load_color_space(
        &self,
        context: CpdfImageLoadContext<'_>,
    ) -> Option<ColorSpaceRef> {
        let dict = self.dict();
        let object = {
            let borrowed = dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_object_for(b"ColorSpace")?
        };
        let object = super::super::parser::cpdf_object::get_direct_object(&object)?;
        let form_resources = context
            .form_resources
            .filter(|_| self.stream().borrow().is_inline());
        form_resources
            .and_then(|resources| context.page_data.get_color_space(&object, Some(resources)))
            .or_else(|| {
                context
                    .page_data
                    .get_color_space(&object, context.page_resources)
            })
    }

    pub(crate) fn start_dib(
        &self,
        context: CpdfImageLoadContext<'_>,
        has_mask: bool,
    ) -> Option<(Box<CpdfDib>, LoadState)> {
        let color_space = self.load_color_space(context);
        let (dib, state) =
            CpdfDib::start_load_with_context(self.stream(), color_space, context, has_mask)?;
        Some((Box::new(dib), state))
    }

    pub fn start_load_dib_base(&self, context: CpdfImageLoadContext<'_>) -> bool {
        let Some((source, state)) = self.start_dib(context, true) else {
            self.loaded_bitmap.borrow_mut().take();
            return false;
        };
        if state == LoadState::Continue {
            *self.loaded_bitmap.borrow_mut() = Some(ImageLoadBitmap::Pending(source));
            return true;
        }
        self.finish_load(source);
        false
    }

    pub fn continue_load(
        &self,
        pause: Option<&mut dyn crate::fxcrt::pauseindicator_iface::PauseIndicatorIface>,
    ) -> bool {
        // Take ownership before codec/pause callbacks, releasing the state borrow.
        let current = self.loaded_bitmap.borrow_mut().take();
        let Some(ImageLoadBitmap::Pending(mut source)) = current else {
            *self.loaded_bitmap.borrow_mut() = current;
            return false;
        };
        match source.continue_load(pause) {
            LoadState::Continue => {
                *self.loaded_bitmap.borrow_mut() = Some(ImageLoadBitmap::Pending(source));
                true
            }
            LoadState::Success => {
                self.finish_load(source);
                false
            }
            LoadState::Fail => false,
        }
    }

    fn finish_load(&self, source: Box<CpdfDib>) {
        *self.loaded_mask.borrow_mut() = source.detach_mask().map(Rc::new);
        self.matte_color.set(source.matte_color());
        *self.loaded_bitmap.borrow_mut() = Some(ImageLoadBitmap::Ready(Rc::new(
            CpdfImageBitmap::Dib(source),
        )));
    }

    pub fn detach_bitmap(&self) -> Option<Rc<CpdfImageBitmap>> {
        let current = self.loaded_bitmap.borrow_mut().take();
        match current {
            Some(ImageLoadBitmap::Ready(bitmap)) => Some(bitmap),
            pending => {
                *self.loaded_bitmap.borrow_mut() = pending;
                None
            }
        }
    }

    pub fn detach_mask(&self) -> Option<Rc<CpdfImageBitmap>> {
        self.loaded_mask.borrow_mut().take()
    }

    /// Synchronous convenience for StartLoadDIBBase()/Continue(): DIB owns
    /// child masks and determines each parent's continuation/failure outcome.
    pub fn decode_image_data(&self, resources: Option<&ObjectRef>) -> Option<CpdfImageData> {
        let page_data = CpdfDocPageData::new();
        self.decode_image_data_with_context(CpdfImageLoadContext::new(&page_data, resources))
    }

    pub fn decode_image_data_with_context(
        &self,
        context: CpdfImageLoadContext<'_>,
    ) -> Option<CpdfImageData> {
        if self.start_load_dib_base(context) {
            while self.continue_load(None) {}
        }
        let bitmap = self.detach_bitmap();
        let mask = self.detach_mask();
        Some(CpdfImageData {
            bitmap: bitmap?,
            mask,
            matte_color: self.matte_color(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::{make_reference, retain_object};
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    use crate::fxcodec::flate;

    #[test]
    fn stream_conversion_preserves_object_and_sample_storage_identity() {
        let objects = CpdfIndirectObjectHolder::new();
        let stream = retain_object(CpdfStream::from_bytes(&[0, 1, 2, 0xff]));
        let image = Rc::new(CpdfImage::from_stream(Rc::clone(&stream), false).unwrap());
        let data_address = {
            let borrowed = stream.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfStream>()
                .unwrap()
                .get_in_memory_raw_data()
                .as_ptr()
        };
        image.convert_stream_to_indirect_object(&objects);
        let number = stream.borrow().obj_num();
        assert_ne!(number, 0);
        assert!(Rc::ptr_eq(
            &stream,
            &objects.borrow().get_indirect_object(number).unwrap(),
        ));
        let page_data = CpdfDocPageData::new();
        let cached = page_data.get_image(&stream).unwrap();
        assert!(Rc::ptr_eq(&cached.stream(), &image.stream()));
        assert!(Rc::ptr_eq(&cached, &page_data.get_image(&stream).unwrap()));
        assert!(!cached.is_inline());
        let stream = cached.stream();
        let borrowed = stream.borrow();
        let data = borrowed
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_in_memory_raw_data();
        assert_eq!(data, &[0, 1, 2, 0xff]);
        assert_eq!(data.as_ptr(), data_address);
    }

    #[test]
    #[should_panic]
    fn stream_conversion_rejects_an_already_indirect_stream() {
        let objects = CpdfIndirectObjectHolder::new();
        let stream = retain_object(CpdfStream::from_bytes(b"image"));
        objects.borrow_mut().add_indirect_object(Rc::clone(&stream));
        CpdfImage::from_stream(stream, false)
            .unwrap()
            .convert_stream_to_indirect_object(&objects);
    }

    // Stable byte-format views for assertions; decoding itself always uses the
    // production DIB pipeline. Masks stay separate and are asserted separately.
    impl CpdfImage {
        fn test_bitmap(&self) -> Option<CfxDibitmap> {
            self.test_bitmap_with_resources(None)
        }
        fn test_bitmap_with_resources(&self, resources: Option<&ObjectRef>) -> Option<CfxDibitmap> {
            let mut bitmap = self.decode_without_mask(resources)?;
            if bitmap.is_mask_format() {
                return bitmap
                    .convert_format(FxdibFormat::EightBppMask)
                    .then_some(bitmap);
            }
            if bitmap.bpp() == 1 && !bitmap.convert_format(FxdibFormat::Bgr) {
                return None;
            }
            bitmap.convert_format(FxdibFormat::Bgra).then_some(bitmap)
        }
    }

    #[test]
    fn mask_continuation_failure_follows_parent_decoder_state() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut mask_dict = CpdfDictionary::new();
        mask_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        mask_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        mask_dict.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
        mask_dict.set_new_name_for(b"Filter", b"JBIG2Decode");
        let mask = retain_object(CpdfStream::from_data_and_dict(
            vec![0; 11],
            retain_object(mask_dict),
        ));
        objects.borrow_mut().add_indirect_object(mask.clone());
        let globals = retain_object(CpdfStream::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ))));
        objects.borrow_mut().add_indirect_object(globals.clone());
        for jbig_parent in [false, true] {
            let mut dictionary = CpdfDictionary::new();
            for (key, value) in [
                (b"Width".as_slice(), if jbig_parent { 1152 } else { 1 }),
                (b"Height", if jbig_parent { 720 } else { 1 }),
                (b"BitsPerComponent", if jbig_parent { 1 } else { 8 }),
            ] {
                dictionary.set_new_for(key, CpdfScalarObject::number_from_signed(value));
            }
            dictionary.set_new_name_for(b"ColorSpace", b"DeviceGray");
            dictionary.set_for(b"Mask", Some(make_reference(&mask, &objects)));
            let bytes = if jbig_parent {
                dictionary.set_new_name_for(b"Filter", b"JBIG2Decode");
                let mut params = CpdfDictionary::new();
                params.set_for(b"JBIG2Globals", Some(make_reference(&globals, &objects)));
                dictionary.set_new_for(b"DecodeParms", params);
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/jbig2_page.bin"
                ))
                .to_vec()
            } else {
                vec![128]
            };
            let image = CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    bytes,
                    retain_object(dictionary),
                )),
                false,
            )
            .unwrap();
            let data = CpdfDocPageData::new();
            assert!(image.start_load_dib_base(CpdfImageLoadContext::new(&data, None)));
            assert!(matches!(
                *image.loaded_bitmap.borrow(),
                Some(ImageLoadBitmap::Pending(_))
            ));
            assert!(!image.continue_load(None));
            let bitmap = image.detach_bitmap();
            assert_eq!(bitmap.is_some(), jbig_parent);
            assert!(image.detach_bitmap().is_none());
            assert!(image.detach_mask().is_none());
        }
    }

    #[test]
    fn jpx_inline_alpha_precedes_external_masks_and_preserves_straight_rgb() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut mask_dict = CpdfDictionary::new();
        for (key, value) in [
            (b"Width".as_slice(), 3),
            (b"Height", 1),
            (b"BitsPerComponent", 8),
        ] {
            mask_dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        mask_dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        let mask = retain_object(CpdfStream::from_data_and_dict(
            vec![255; 3],
            retain_object(mask_dict),
        ));
        objects.borrow_mut().add_indirect_object(mask.clone());
        for inline in [false, true] {
            let mut dictionary = CpdfDictionary::new();
            for (key, value) in [
                (b"Width".as_slice(), 3),
                (b"Height", 1),
                (b"SMaskInData", i32::from(inline)),
            ] {
                dictionary.set_new_for(key, CpdfScalarObject::number_from_signed(value));
            }
            dictionary.set_new_name_for(b"Filter", b"JPXDecode");
            dictionary.set_for(b"SMask", Some(make_reference(&mask, &objects)));
            let stream = retain_object(CpdfStream::from_data_and_dict(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/rgba.jp2"
                ))
                .to_vec(),
                retain_object(dictionary),
            ));
            let image = CpdfImage::from_stream(stream, false).unwrap();
            let data = image.decode_image_data(None).unwrap();
            assert_eq!(data.bitmap.format(), FxdibFormat::Bgr);
            let row = data.bitmap.scanline(0);
            // Both paths retain straight color samples. The selected mask
            // applies opacity later; prematting here would apply it twice.
            assert_eq!(&row[..9], &[0, 0, 255, 255, 0, 0, 0, 255, 0]);
            let coverage = data.mask.as_ref().unwrap().scanline(0);
            assert_eq!(
                &coverage[..3],
                if inline {
                    &[128, 0, 255]
                } else {
                    &[255, 255, 255]
                }
            );
            assert_eq!(data.matte_color, u32::MAX);
        }
    }

    #[test]
    fn dib_resource_scopes_use_form_then_page_only_for_inline_streams() {
        let resources = |space: &[u8]| {
            let mut spaces = CpdfDictionary::new();
            if !space.is_empty() {
                spaces.set_new_name_for(b"Named", space);
            }
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"ColorSpace", spaces);
            retain_object(resources)
        };
        let form = resources(b"DeviceGray");
        let missing = resources(b"");
        let page = resources(b"DeviceRGB");
        let objects = CpdfIndirectObjectHolder::new();
        let data = CpdfDocPageData::new();
        for (indirect, form_resources, expected) in [
            (false, &form, FxdibFormat::EightBppRgb),
            (false, &missing, FxdibFormat::Bgr),
            (true, &form, FxdibFormat::Bgr),
        ] {
            let mut dict = CpdfDictionary::new();
            for (key, value) in [
                (b"Width".as_slice(), 1),
                (b"Height", 1),
                (b"BitsPerComponent", 8),
            ] {
                dict.set_new_for(key, CpdfScalarObject::number_from_signed(value));
            }
            dict.set_new_name_for(b"ColorSpace", b"Named");
            let stream = retain_object(CpdfStream::from_data_and_dict(
                vec![128, 0, 255],
                retain_object(dict),
            ));
            if indirect {
                objects.borrow_mut().add_indirect_object(stream.clone());
            }
            let image = CpdfImage::from_stream(stream, false).unwrap();
            let mut context = CpdfImageLoadContext::new(&data, Some(&page));
            context.form_resources = Some(form_resources);
            let image = image.decode_image_data_with_context(context).unwrap();
            assert_eq!(image.bitmap.format(), expected);
            let row = image.bitmap.scanline(0);
            if expected == FxdibFormat::Bgr {
                assert_eq!(&row[..3], &[255, 0, 128]);
            } else {
                assert_eq!(row[0], 128);
            }
        }
    }

    #[test]
    fn reads_stream_metadata() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(32));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(16));
        dict.set_new_for(b"Interpolate", CpdfScalarObject::number_from_signed(1));
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        let stream = retain_object(CpdfStream::from_data_and_dict(
            vec![1, 2],
            retain_object(dict),
        ));
        let image = CpdfImage::from_stream(stream, true).unwrap();
        assert_eq!((image.pixel_width(), image.pixel_height()), (32, 16));
        assert!(image.is_interpol());
        assert!(!image.is_mask());
        assert!(image.is_inline());
        image.will_be_destroyed();
        assert!(image.is_going_to_be_destroyed());
        assert!(CpdfImage::is_valid_jpeg_component(4));
        assert!(!CpdfImage::is_valid_jpeg_bits_per_component(12));
    }

    #[test]
    fn retained_image_bitmap_forwards_decoder_skip_and_reentrant_pause() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        dict.set_new_name_for(b"Filter", b"FlateDecode");
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                flate::encode(&[64, 128]),
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        let data = CpdfDocPageData::new();
        assert!(!image.start_load_dib_base(CpdfImageLoadContext::new(&data, None)));
        let bitmap = image.detach_bitmap().unwrap();
        assert!(matches!(bitmap.as_ref(), CpdfImageBitmap::Dib(_)));
        struct Reenter<'a> {
            bitmap: &'a CpdfImageBitmap,
            calls: usize,
        }
        impl crate::fxcrt::pauseindicator_iface::PauseIndicatorIface for Reenter<'_> {
            fn need_to_pause_now(&mut self) -> bool {
                self.calls += 1;
                assert_eq!(&*self.bitmap.scanline(0), &[64]);
                true
            }
        }
        let mut pause = Reenter {
            bitmap: &bitmap,
            calls: 0,
        };
        assert!(bitmap.skip_to_scanline(1, Some(&mut pause)));
        assert_eq!(pause.calls, 1);
        assert!(!bitmap.skip_to_scanline(1, None));
        assert_eq!(&*bitmap.scanline(1), &[128]);
        // Rewinding also forwards through the public image's retained handle.
        assert!(!bitmap.skip_to_scanline(0, None));
        assert_eq!(&*bitmap.scanline(0), &[64]);
    }

    #[test]
    fn decodes_flate_gray_image_samples() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        dict.set_new_name_for(b"Filter", b"FlateDecode");
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                flate::encode(&[10, 200]),
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        let bitmap = image.test_bitmap().unwrap();
        assert_eq!(
            &bitmap.buffer()[..8],
            &[10, 10, 10, 255, 200, 200, 200, 255]
        );
    }

    #[test]
    fn decodes_dct_rgb_image_samples() {
        let jpeg = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/mona_lisa.jpg"
        ));
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(120));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(120));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        dict.set_new_name_for(b"Filter", b"DCTDecode");
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                jpeg.to_vec(),
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        let bitmap = image.test_bitmap().unwrap();
        assert_eq!((bitmap.width(), bitmap.height()), (120, 120));
        assert_eq!(bitmap.buffer()[3], 255);
        assert_ne!(
            &bitmap.buffer()[..3],
            &bitmap.buffer()[100 * 4..100 * 4 + 3]
        );
    }

    #[test]
    fn decodes_grayscale_and_cmyk_jpeg_samples() {
        for (color_space, bytes) in [
            (
                b"DeviceGray".as_slice(),
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/gray.jpg"
                ))
                .as_slice(),
            ),
            (
                b"DeviceCMYK".as_slice(),
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/cmyk.jpg"
                ))
                .as_slice(),
            ),
        ] {
            let mut dict = CpdfDictionary::new();
            dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(16));
            dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
            dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
            dict.set_new_name_for(b"ColorSpace", color_space);
            dict.set_new_name_for(b"Filter", b"DCTDecode");
            if color_space == b"DeviceCMYK" {
                let mut decode = CpdfArray::new();
                for value in [1, 0, 1, 0, 1, 0, 1, 0] {
                    decode.append_new(CpdfScalarObject::number_from_signed(value));
                }
                dict.set_new_for(b"Decode", decode);
            }
            let image = CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    bytes.to_vec(),
                    retain_object(dict),
                )),
                false,
            )
            .unwrap();
            let bitmap = image.test_bitmap().unwrap();
            assert_eq!((bitmap.width(), bitmap.height()), (16, 1));
            assert_eq!(bitmap.buffer()[3], 255);
            let first = &bitmap.buffer()[..3];
            let last = &bitmap.buffer()[15 * 4..15 * 4 + 3];
            if color_space == b"DeviceGray" {
                assert!(first.iter().all(|&channel| channel > 240));
                assert!(last.iter().all(|&channel| channel < 15));
            } else {
                assert!(first[2] > 200 && first[0] < 80);
                assert!(i32::from(last[0]) > i32::from(last[2]) + 75);
            }
        }
    }

    #[test]
    fn decodes_group_three_and_four_fax_image_samples() {
        for (k, bytes) in [
            (
                -1,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/fax_g4.bin"
                ))
                .as_slice(),
            ),
            (
                0,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/fax_g3.bin"
                ))
                .as_slice(),
            ),
        ] {
            let mut params = CpdfDictionary::new();
            params.set_new_for(b"K", CpdfScalarObject::number_from_signed(k));
            params.set_new_for(b"Columns", CpdfScalarObject::number_from_signed(16));
            let mut dict = CpdfDictionary::new();
            dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(16));
            dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(2));
            dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(1));
            dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
            dict.set_new_name_for(b"Filter", b"CCITTFaxDecode");
            dict.set_new_for(b"DecodeParms", params);
            let image = CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    bytes.to_vec(),
                    retain_object(dict),
                )),
                false,
            )
            .unwrap();
            let bitmap = image.test_bitmap().unwrap();
            assert_eq!(&bitmap.scanline(0).unwrap()[..4], &[255, 255, 255, 255]);
            assert_eq!(
                &bitmap.scanline(0).unwrap()[15 * 4..15 * 4 + 4],
                &[0, 0, 0, 255]
            );
            assert_eq!(&bitmap.scanline(1).unwrap()[..4], &[0, 0, 0, 255]);
            assert_eq!(
                &bitmap.scanline(1).unwrap()[15 * 4..15 * 4 + 4],
                &[255, 255, 255, 255]
            );
        }
    }

    #[test]
    fn ccitt_black_is_one_changes_stencil_polarity() {
        for black_is_one in [false, true] {
            let mut params = CpdfDictionary::new();
            params.set_new_for(b"K", CpdfScalarObject::number_from_signed(-1));
            params.set_new_for(b"Columns", CpdfScalarObject::number_from_signed(16));
            params.set_new_for(b"BlackIs1", CpdfScalarObject::boolean(black_is_one));
            let mut dict = CpdfDictionary::new();
            dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(16));
            dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(2));
            dict.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
            dict.set_new_name_for(b"Filter", b"CCITTFaxDecode");
            dict.set_new_for(b"DecodeParms", params);
            let image = CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    include_bytes!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/tests/fixtures/fax_g4.bin"
                    ))
                    .to_vec(),
                    retain_object(dict),
                )),
                false,
            )
            .unwrap();
            let bitmap = image.test_bitmap().unwrap();
            assert_eq!(
                bitmap.scanline(0).unwrap()[0],
                if black_is_one { 255 } else { 0 }
            );
            assert_eq!(
                bitmap.scanline(0).unwrap()[15],
                if black_is_one { 0 } else { 255 }
            );
        }
    }

    #[test]
    fn decodes_jbig2_image_with_global_segments() {
        let globals = retain_object(CpdfStream::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ))));
        let objects = CpdfIndirectObjectHolder::new();
        objects.borrow_mut().add_indirect_object(globals.clone());
        let mut params = CpdfDictionary::new();
        params.set_for(b"JBIG2Globals", Some(make_reference(&globals, &objects)));
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1152));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(720));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(1));
        dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        dict.set_new_name_for(b"Filter", b"JBIG2Decode");
        dict.set_new_for(b"DecodeParms", params);
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/jbig2_page.bin"
                ))
                .to_vec(),
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        let bitmap = image.test_bitmap().unwrap();
        assert_eq!((bitmap.width(), bitmap.height()), (1152, 720));
        assert!(bitmap.buffer().chunks_exact(4).any(|pixel| pixel[0] == 0));
        assert!(bitmap.buffer().chunks_exact(4).any(|pixel| pixel[0] == 255));
    }

    #[test]
    fn decodes_jpeg2000_rgb_without_pdf_color_space() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(16));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(16));
        dict.set_new_name_for(b"Filter", b"JPXDecode");
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/rgb.jp2"
                ))
                .to_vec(),
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        assert!(image.is_mask());
        let bitmap = image.test_bitmap().unwrap();
        assert_eq!((bitmap.width(), bitmap.height()), (16, 16));
        let left = &bitmap.scanline(8).unwrap()[..4];
        let right = &bitmap.scanline(8).unwrap()[15 * 4..15 * 4 + 4];
        assert!(left[2] > 200 && left[0] < 50);
        assert!(right[0] > 200 && right[2] < 50);
    }

    #[test]
    fn decodes_raw_jpeg2000_with_pdf_color_space() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(16));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(16));
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        dict.set_new_name_for(b"Filter", b"JPXDecode");
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/rgb.j2c"
                ))
                .to_vec(),
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        let bitmap = image.test_bitmap().unwrap();
        assert!(bitmap.scanline(8).unwrap()[2] > 200);
        assert!(bitmap.scanline(8).unwrap()[15 * 4] > 200);
    }

    #[test]
    fn decodes_device_cmyk_samples() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"DeviceCMYK");
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![0, 0, 0, 0, 0, 0, 0, 255],
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        let bitmap = image.test_bitmap().unwrap();
        let first = &bitmap.buffer()[..4];
        let second = &bitmap.buffer()[4..8];
        assert_eq!(first[3], 255);
        assert_eq!(second[3], 255);
        assert!(first[..3].iter().all(|&channel| channel > 200));
        assert!(second[..3].iter().all(|&channel| channel < 50));
    }

    #[test]
    fn image_mask_decode_array_reverses_painted_bits() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
        let mut decode = CpdfArray::new();
        decode.append_new(CpdfScalarObject::number_from_signed(1));
        decode.append_new(CpdfScalarObject::number_from_signed(0));
        dict.set_new_for(b"Decode", decode);
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![0x40],
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        let bitmap = image.test_bitmap().unwrap();
        assert_eq!(&bitmap.buffer()[..2], &[0, 255]);
    }

    #[test]
    fn decodes_two_bit_indexed_image() {
        let mut palette = CpdfArray::new();
        palette.append_new_name(b"Indexed");
        palette.append_new_name(b"DeviceRGB");
        palette.append_new(CpdfScalarObject::number_from_signed(3));
        palette.append_new(CpdfString::from_bytes(
            None,
            &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        ));
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(4));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"ColorSpace", palette);
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![0b00011011],
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        let bitmap = image.test_bitmap().unwrap();
        assert_eq!(
            &bitmap.buffer()[..16],
            &[0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255]
        );
    }

    #[test]
    fn resolves_indexed_image_color_space_from_resources() {
        let mut palette = CpdfArray::new();
        palette.append_new_name(b"Indexed");
        palette.append_new_name(b"DeviceRGB");
        palette.append_new(CpdfScalarObject::number_from_signed(1));
        palette.append_new(CpdfString::from_bytes(None, &[255, 0, 0, 0, 0, 255]));
        let mut colors = CpdfDictionary::new();
        colors.set_new_for(b"Palette", palette);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"ColorSpace", colors);
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(1));
        dict.set_new_name_for(b"ColorSpace", b"Palette");
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![0b01000000],
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        assert!(image.test_bitmap().is_none());
        assert_eq!(
            &image
                .test_bitmap_with_resources(Some(&retain_object(resources)))
                .unwrap()
                .buffer()[..8],
            &[0, 0, 255, 255, 255, 0, 0, 255]
        );
    }

    #[test]
    fn decode_ranges_invert_gray_and_remap_indexed_samples() {
        let mut gray = CpdfDictionary::new();
        gray.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        gray.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        gray.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        gray.set_new_name_for(b"ColorSpace", b"DeviceGray");
        let mut inverted = CpdfArray::new();
        inverted.append_new(CpdfScalarObject::number_from_signed(1));
        inverted.append_new(CpdfScalarObject::number_from_signed(0));
        gray.set_new_for(b"Decode", inverted);
        let gray = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![0, 255],
                retain_object(gray),
            )),
            false,
        )
        .unwrap();
        assert_eq!(
            &gray.test_bitmap().unwrap().buffer()[..8],
            &[255, 255, 255, 255, 0, 0, 0, 255]
        );

        let mut palette = CpdfArray::new();
        palette.append_new_name(b"Indexed");
        palette.append_new_name(b"DeviceRGB");
        palette.append_new(CpdfScalarObject::number_from_signed(1));
        palette.append_new(CpdfString::from_bytes(None, &[255, 0, 0, 0, 0, 255]));
        let mut indexed = CpdfDictionary::new();
        indexed.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        indexed.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        indexed.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(1));
        indexed.set_new_for(b"ColorSpace", palette);
        let mut reversed = CpdfArray::new();
        reversed.append_new(CpdfScalarObject::number_from_signed(1));
        reversed.append_new(CpdfScalarObject::number_from_signed(0));
        indexed.set_new_for(b"Decode", reversed);
        let indexed = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![0b01000000],
                retain_object(indexed),
            )),
            false,
        )
        .unwrap();
        assert_eq!(
            &indexed.test_bitmap().unwrap().buffer()[..8],
            &[255, 0, 0, 255, 0, 0, 255, 255]
        );
    }

    #[test]
    fn soft_mask_gray_samples_stay_separate_from_color() {
        let mut mask_dict = CpdfDictionary::new();
        mask_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        mask_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        mask_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        mask_dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        let mask = retain_object(CpdfStream::from_data_and_dict(
            vec![0, 128],
            retain_object(mask_dict),
        ));
        let objects = CpdfIndirectObjectHolder::new();
        objects.borrow_mut().add_indirect_object(mask.clone());
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        image_dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        image_dict.set_for(b"SMask", Some(make_reference(&mask, &objects)));
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![255, 0, 0, 0, 0, 255],
                retain_object(image_dict),
            )),
            false,
        )
        .unwrap();
        let data = image.decode_image_data(None).unwrap();
        assert_eq!(&data.bitmap.scanline(0)[..6], &[0, 0, 255, 255, 0, 0]);
        assert_eq!(&data.mask.as_ref().unwrap().scanline(0)[..2], &[0, 128]);
    }

    #[test]
    fn soft_mask_matte_retains_preblended_color_for_the_renderer() {
        let mut mask_dict = CpdfDictionary::new();
        mask_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        mask_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        mask_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        mask_dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        let mut matte = CpdfArray::new();
        for _ in 0..3 {
            matte.append_new(CpdfScalarObject::number_from_signed(1));
        }
        mask_dict.set_new_for(b"Matte", matte);
        let mask = retain_object(CpdfStream::from_data_and_dict(
            vec![128],
            retain_object(mask_dict),
        ));
        let objects = CpdfIndirectObjectHolder::new();
        objects.borrow_mut().add_indirect_object(mask.clone());
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        image_dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        image_dict.set_for(b"SMask", Some(make_reference(&mask, &objects)));
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![255, 127, 127],
                retain_object(image_dict),
            )),
            false,
        )
        .unwrap();
        let data = image.decode_image_data(None).unwrap();
        assert_eq!(&data.bitmap.scanline(0)[..3], &[127, 127, 255]);
        assert_eq!(data.mask.as_ref().unwrap().scanline(0)[0], 128);
        assert_eq!(data.matte_color, 0x00ff_ffff);
        assert_eq!(image.matte_color(), 0x00ff_ffff);
    }

    #[test]
    fn color_key_mask_makes_matching_rgb_sample_transparent() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        let mut mask = CpdfArray::new();
        for value in [255, 255, 0, 0, 0, 0] {
            mask.append_new(CpdfScalarObject::number_from_signed(value));
        }
        dict.set_new_for(b"Mask", mask);
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![255, 0, 0, 0, 0, 255],
                retain_object(dict),
            )),
            false,
        )
        .unwrap();
        assert_eq!(
            &image.test_bitmap().unwrap().buffer()[..8],
            &[0, 0, 255, 0, 255, 0, 0, 255]
        );
    }

    #[test]
    fn explicit_one_bit_mask_stays_separate_with_native_polarity() {
        let mut mask_dict = CpdfDictionary::new();
        mask_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        mask_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        mask_dict.set_new_for(b"ImageMask", CpdfScalarObject::boolean(true));
        mask_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(1));
        let mask = retain_object(CpdfStream::from_data_and_dict(
            vec![0b01000000],
            retain_object(mask_dict),
        ));
        let objects = CpdfIndirectObjectHolder::new();
        objects.borrow_mut().add_indirect_object(mask.clone());
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        image_dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        image_dict.set_for(b"Mask", Some(make_reference(&mask, &objects)));
        let image = CpdfImage::from_stream(
            retain_object(CpdfStream::from_data_and_dict(
                vec![255, 0, 0, 0, 0, 255],
                retain_object(image_dict),
            )),
            false,
        )
        .unwrap();
        let data = image.decode_image_data(None).unwrap();
        assert_eq!(&data.bitmap.scanline(0)[..6], &[0, 0, 255, 255, 0, 0]);
        let mask = data.mask.as_ref().unwrap();
        assert_eq!(mask.format(), FxdibFormat::OneBppMask);
        assert_eq!(mask.scanline(0)[0] & 0xc0, 0x80);
    }
}

#[cfg(test)]
#[path = "cpdf_image_onebit_tests.rs"]
mod onebit_tests;
