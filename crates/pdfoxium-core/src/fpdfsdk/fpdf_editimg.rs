// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Safe image editing/query operations from `fpdfsdk/fpdf_editimg.cpp`.

use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::page::cpdf_image::{CpdfImage, CpdfImageLoadContext};
use crate::fpdfapi::page::cpdf_imageobject::CpdfImageObject;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::cstretchengine::StretchSource;
use crate::fxge::dib::fx_dib::{get_bpp_from_format, FxdibFormat};
use std::rc::Rc;

pub fn new_image_object(page_data: &Rc<CpdfDocPageData>) -> CpdfImageObject {
    let mut object = CpdfImageObject::default();
    object.set_page_data(Rc::downgrade(page_data));
    object.set_image(Some(Rc::new(CpdfImage::empty_with_page_data(page_data))));
    object
}

pub fn set_bitmap<S: StretchSource>(object: &mut CpdfImageObject, bitmap: &S) -> bool {
    let Some(page_data) = object.page_data_ref() else {
        return false;
    };
    let Some(objects) = page_data.document_objects() else {
        return false;
    };
    let Some(image) = object.image() else {
        return false;
    };
    if !image.set_bitmap(bitmap, &objects) {
        return false;
    }
    object.calc_bounding_box();
    object.base_mut().set_dirty(true);
    true
}

pub fn get_bitmap(image: &CpdfImage) -> Option<CfxDibitmap> {
    if !image.has_stream() {
        return None;
    }
    let page_data = CpdfDocPageData::new();
    let (mut source, mut state) =
        image.start_dib(CpdfImageLoadContext::new(&page_data, None), false)?;
    use crate::fpdfapi::page::cpdf_dib::LoadState;
    while state == LoadState::Continue {
        state = source.continue_load(None);
    }
    if state != LoadState::Success {
        return None;
    }
    let has_palette = !source.palette().is_empty();
    let format = match source.format() {
        FxdibFormat::OneBppMask | FxdibFormat::EightBppMask => Some(FxdibFormat::EightBppRgb),
        FxdibFormat::OneBppRgb => Some(if has_palette {
            FxdibFormat::Bgr
        } else {
            FxdibFormat::EightBppRgb
        }),
        FxdibFormat::EightBppRgb | FxdibFormat::Bgr | FxdibFormat::Bgrx | FxdibFormat::Bgra => {
            has_palette.then_some(FxdibFormat::Bgr)
        }
        FxdibFormat::Invalid => return None,
    };
    match format {
        Some(format) => crate::fxge::dib::cfx_dibbase::convert_to(source.as_ref(), format),
        None => crate::fxge::dib::cfx_dibbase::realize(source.as_ref()),
    }
}

pub fn with_image_data<R>(
    image: &CpdfImage,
    decoded: bool,
    consume: impl FnOnce(&[u8]) -> R,
) -> Option<R> {
    if !image.has_stream() {
        return None;
    }
    let stream = image.stream();
    let borrowed = stream.borrow();
    let stream = borrowed.as_any().downcast_ref::<CpdfStream>()?;
    let mut access = CpdfStreamAcc::new(stream);
    if decoded {
        access.load_all_data_filtered();
    } else {
        access.load_all_data_raw();
    }
    Some(consume(access.get_span()))
}

pub fn filter_count(image: &CpdfImage) -> usize {
    if !image.has_stream() {
        return 0;
    }
    let dictionary = image.dict();
    let borrowed = dictionary.borrow();
    let dictionary = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
    let Some(filter) = dictionary.get_direct_object_for(b"Filter") else {
        return 0;
    };
    let borrowed = filter.borrow();
    if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
        array.size()
    } else if borrowed.object_type() == crate::fpdfapi::parser::cpdf_scalar_object::ObjectType::Name
    {
        1
    } else {
        0
    }
}

pub fn filter(image: &CpdfImage, index: usize) -> Option<Vec<u8>> {
    if index >= filter_count(image) {
        return None;
    }
    let dictionary = image.dict();
    let filter = dictionary
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_direct_object_for(b"Filter")?;
    let borrowed = filter.borrow();
    if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
        Some(array.get_byte_string_at(index))
    } else {
        Some(borrowed.get_string())
    }
}

pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub dpi: Option<(f32, f32)>,
    pub bits_per_pixel: u32,
    pub colorspace: i32,
    pub marked_content_id: i32,
}

pub fn metadata(object: &CpdfImageObject, page: Option<&CpdfPage>) -> Option<ImageMetadata> {
    let image = object.image()?;
    let rect = object.base().rect();
    let mut result = ImageMetadata {
        width: image.pixel_width() as u32,
        height: image.pixel_height() as u32,
        dpi: (rect.width() != 0.0 && rect.height() != 0.0).then(|| {
            (
                image.pixel_width() as f32 / rect.width() * 72.0,
                image.pixel_height() as f32 / rect.height() * 72.0,
            )
        }),
        bits_per_pixel: 0,
        colorspace: 0,
        marked_content_id: object.base().content_marks().marked_content_id(),
    };
    let Some(page) = page.filter(|_| image.has_stream()) else {
        return Some(result);
    };
    if object
        .page_data_ref()
        .is_some_and(|data| !Rc::ptr_eq(&data, &page.page_data_ref()))
    {
        return None;
    }
    let resources = page.resources();
    if let Some((source, _)) = image.start_dib(
        CpdfImageLoadContext::new(page.page_data(), resources.as_ref()),
        false,
    ) {
        result.bits_per_pixel = get_bpp_from_format(source.format()) as u32;
        result.colorspace = source
            .color_space()
            .map_or(0, |space| space.family() as i32);
    }
    Some(result)
}

pub fn with_icc_profile<R>(
    image: &CpdfImage,
    page: &CpdfPage,
    consume: impl FnOnce(&[u8]) -> R,
) -> Option<R> {
    if !image.has_stream() {
        return None;
    }
    let resources = page.resources()?;
    let dictionary = image.dict();
    let object = dictionary
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_direct_object_for(b"ColorSpace")?;
    let space = page
        .page_data()
        .get_color_space(&object, Some(&resources))?;
    let profile = space
        .get_icc_profile()
        .filter(|profile| profile.is_valid())?;
    let data = profile.data();
    let result = consume(&data);
    Some(result)
}

pub fn rendered_bitmap(
    page_data: &Rc<CpdfDocPageData>,
    optional_page: Option<&CpdfPage>,
    object: &CpdfImageObject,
) -> Option<CfxDibitmap> {
    let matrix = object.matrix();
    let width = matrix.a.hypot(matrix.c).ceil();
    let height = matrix.b.hypot(matrix.d).ceil();
    if !width.is_finite()
        || !height.is_finite()
        || width >= i32::MAX as f32
        || height >= i32::MAX as f32
    {
        return None;
    }
    let mut bitmap = CfxDibitmap::create(width as i32, height as i32, FxdibFormat::Bgra, 0)?;
    let temporary_page;
    let page = match optional_page {
        Some(page) => page,
        None => {
            temporary_page = CpdfPage::new_with_page_data(
                retain_object(CpdfDictionary::new()),
                page_data.clone(),
            )?;
            &temporary_page
        }
    };
    let flip = CfxMatrix::new(1.0, 0.0, 0.0, -1.0, 0.0, height);
    let mut render = flip;
    render.translate(
        -(matrix.e + matrix.a.min(matrix.c)),
        matrix.f + matrix.b.min(matrix.d),
    );
    let mut clip = CfxMatrix::default();
    clip.match_rect(
        CfxFloatRect::new(0.0, 0.0, width, height),
        object
            .base()
            .original_matrix()
            .transform_float_rect(CfxFloatRect::new(0.0, 0.0, 1.0, 1.0)),
    );
    clip.concat(flip);
    crate::fpdfapi::render::cpdf_renderstatus::render_image_for_sdk(
        page,
        object,
        &mut bitmap,
        render,
        clip,
    )
    .then_some(bitmap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    #[test]
    fn bitmap_embedding_preserves_rgb_samples_alpha_and_pixel_dimensions() {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let mut object = new_image_object(&document.page_data_ref());
        let mut bitmap = CfxDibitmap::create(2, 1, FxdibFormat::Bgra, 0).unwrap();
        bitmap.buffer_mut()[..8].copy_from_slice(&[10, 20, 30, 40, 50, 60, 70, 80]);
        assert!(set_bitmap(&mut object, &bitmap));
        let image = object.image().unwrap();
        assert_eq!((image.pixel_width(), image.pixel_height()), (2, 1));
        with_image_data(&image, false, |data| {
            assert_eq!(data, &[30, 20, 10, 70, 60, 50])
        })
        .unwrap();
        let decoded = image.decode_image_data(None).unwrap();
        assert_eq!(&decoded.mask.unwrap().scanline(0)[..2], &[40, 80]);
        let public_bitmap = get_bitmap(&image).unwrap();
        assert!(public_bitmap.palette().is_empty());
        assert_eq!(&public_bitmap.buffer()[..6], &[10, 20, 30, 50, 60, 70]);
    }

    #[test]
    fn jpeg_embedding_retains_file_or_inline_storage_and_failed_load_keeps_image() {
        use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/gray.jpg"
        ));
        for inline in [false, true] {
            let image = CpdfImage::empty();
            assert!(image.set_jpeg_image(
                Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
                inline
            ));
            assert_eq!(filter_count(&image), 1);
            assert_eq!(filter(&image, 0).unwrap(), b"DCTDecode");
            let stream = image.stream();
            assert_eq!(
                stream
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfStream>()
                    .unwrap()
                    .is_memory_based(),
                inline
            );
            assert!(!image.set_jpeg_image(
                Rc::new(CfxReadOnlyContainerStream::new(vec![1, 2, 3])),
                inline
            ));
            assert!(Rc::ptr_eq(&stream, &image.stream()));
        }
    }
    #[test]
    fn embedded_bitmap_survives_content_generation_save_reload_and_rendering() {
        use crate::fpdfapi::edit::cpdf_creator::CpdfCreator;
        use crate::fpdfapi::edit::cpdf_pagecontentgenerator::CpdfPageContentGenerator;
        use crate::fpdfapi::parser::cpdf_parser::ParserError;
        use crate::fpdfapi::render::cpdf_renderoptions::CpdfRenderOptions;
        use crate::fpdfapi::render::cpdf_renderstatus::render_page_objects;
        use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
        use crate::fxcrt::fx_coordinates::FxRect;
        use crate::fxcrt::fx_stream::WriteStream;
        use std::cell::RefCell;
        struct Sink(Vec<u8>);
        impl WriteStream for Sink {
            fn write_block(&mut self, data: &[u8]) -> bool {
                self.0.extend_from_slice(data);
                true
            }
        }
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let mut page = super::super::fpdf_editpage::new_page(&document, 0, 20.0, 20.0).unwrap();
        let mut object = new_image_object(&document.borrow().page_data_ref());
        // Exercise replacement of a parsed inline image: SetBitmap must turn
        // it into an XObject that dirty content generation can serialize.
        let inline = Rc::new(
            CpdfImage::from_stream(retain_object(CpdfStream::from_bytes(&[])), true).unwrap(),
        );
        assert!(inline.is_inline());
        object.set_image(Some(inline.clone()));
        let mut source = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        source.buffer_mut()[..4].copy_from_slice(&[20, 80, 240, 128]);
        assert!(set_bitmap(&mut object, &source));
        assert!(!inline.is_inline());
        object.set_image_matrix(CfxMatrix::new(12.0, 0.0, 0.0, 12.0, 3.0, 4.0));
        page.mutable_holder().append_page_object(Box::new(object));
        CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut document.borrow_mut()))
            .generate_content();
        let render = |page: &CpdfPage| {
            let mut bitmap = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
            bitmap.clear(0xffffffff);
            assert!(render_page_objects(
                page,
                &mut bitmap,
                page.display_matrix_for_rect(FxRect::new(0, 0, 20, 20), 0),
                &CpdfRenderOptions::new(),
                false
            ));
            bitmap
        };
        let before = render(&page);
        assert_ne!(&before.scanline(10).unwrap()[40..44], &[255; 4]);
        let mut sink = Sink(Vec::new());
        assert!(CpdfCreator::new(document.clone(), &mut sink).create(0, 0));
        let mut reloaded = CpdfDocument::new();
        assert_eq!(
            reloaded.load_doc(Rc::new(CfxReadOnlyContainerStream::new(sink.0)), b""),
            ParserError::Success
        );
        let mut page = CpdfPage::from_document(&mut reloaded, 0).unwrap();
        page.parse_path_content();
        let after = render(&page);
        assert_eq!(before.buffer(), after.buffer());
        let object = page
            .holder()
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfImageObject>()
            .unwrap();
        let data = metadata(object, Some(&page)).unwrap();
        assert_eq!((data.width, data.height, data.colorspace), (1, 1, 2));
        assert_eq!(data.dpi, Some((6.0, 6.0)));
    }
}
