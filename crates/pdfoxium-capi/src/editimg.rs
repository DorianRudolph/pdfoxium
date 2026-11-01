// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `fpdfsdk/fpdf_editimg.cpp` C pointer/handle adapter.
use super::*;
use pdfoxium_core::fpdfapi::page::cpdf_image::CpdfImage;
use pdfoxium_core::fpdfapi::page::cpdf_imageobject::CpdfImageObject;
use pdfoxium_core::fpdfsdk::fpdf_editimg as image_sdk;
use pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix;

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct FPDF_IMAGEOBJ_METADATA {
    pub width: u32,
    pub height: u32,
    pub horizontal_dpi: f32,
    pub vertical_dpi: f32,
    pub bits_per_pixel: u32,
    pub colorspace: c_int,
    pub marked_content_id: c_int,
}

unsafe fn image_from_handle(handle: *mut c_void) -> Option<Rc<CpdfImage>> {
    unsafe {
        with_page_object(handle, |object| {
            object.as_any().downcast_ref::<CpdfImageObject>()?.image()
        })
    }
    .flatten()
}

/// Pages supplied by the embedder are live for this synchronous mutation.
unsafe fn reset_page_caches(pages: *const *mut c_void, count: c_int, image: &CpdfImage) {
    if pages.is_null() || count <= 0 {
        return;
    }
    for &page in unsafe { std::slice::from_raw_parts(pages, count as usize) } {
        if let Some(page) = unsafe { pdf_handle(page) }.and_then(PageHandle::pdf_page) {
            image.reset_cache(page);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_NewImageObj(document: *mut c_void) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let data = document.document.borrow().page_data_ref();
    new_owned_page_object(Box::new(image_sdk::new_image_object(&data)))
}

unsafe fn load_jpeg(
    pages: *const *mut c_void,
    count: c_int,
    object: *mut c_void,
    access: *const FPDF_FILEACCESS,
    inline: bool,
) -> c_int {
    let Some(image) = (unsafe { image_from_handle(object) }) else {
        return 0;
    };
    let Some(access) = (unsafe { access.as_ref() }) else {
        return 0;
    };
    if access.m_GetBlock.is_none() {
        return 0;
    }
    // Copy the descriptor before callbacks; retain no handle/page borrow while
    // user GetBlock may re-enter the library. File mode retains this adapter.
    let file: Rc<dyn SeekableReadStream> = Rc::new(CustomAccessStream {
        access: FPDF_FILEACCESS {
            m_FileLen: access.m_FileLen,
            m_GetBlock: access.m_GetBlock,
            m_Param: access.m_Param,
        },
    });
    unsafe {
        reset_page_caches(pages, count, &image);
    }
    image.set_jpeg_image(file, inline);
    unsafe {
        with_page_object(object, |object| object.base_mut().set_dirty(true));
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_LoadJpegFile(
    pages: *const *mut c_void,
    count: c_int,
    object: *mut c_void,
    access: *const FPDF_FILEACCESS,
) -> c_int {
    unsafe { load_jpeg(pages, count, object, access, false) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_LoadJpegFileInline(
    pages: *const *mut c_void,
    count: c_int,
    object: *mut c_void,
    access: *const FPDF_FILEACCESS,
) -> c_int {
    unsafe { load_jpeg(pages, count, object, access, true) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_SetMatrix(
    object: *mut c_void,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
) -> c_int {
    unsafe {
        with_page_object(object, |object| {
            let Some(image) = object.as_any_mut().downcast_mut::<CpdfImageObject>() else {
                return 0;
            };
            image.set_image_matrix(CfxMatrix::new(
                a as f32, b as f32, c as f32, d as f32, e as f32, f as f32,
            ));
            image.base_mut().set_dirty(true);
            1
        })
    }
    .unwrap_or(0)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_SetBitmap(
    pages: *const *mut c_void,
    count: c_int,
    object: *mut c_void,
    bitmap: *mut c_void,
) -> c_int {
    let Some(image) = (unsafe { image_from_handle(object) }) else {
        return 0;
    };
    let Some(bitmap) = (unsafe { bitmap.cast::<BitmapHandle>().as_mut() }) else {
        return 0;
    };
    unsafe {
        reset_page_caches(pages, count, &image);
    }
    match &mut *bitmap.storage.borrow_mut() {
        BitmapStorage::Owned(bitmap) => unsafe {
            with_page_object(object, |object| {
                object
                    .as_any_mut()
                    .downcast_mut::<CpdfImageObject>()
                    .is_some_and(|object| image_sdk::set_bitmap(object, bitmap))
                    as c_int
            })
        }
        .unwrap_or(0),
        BitmapStorage::External {
            width,
            height,
            format,
            pitch,
            buffer,
        } => {
            // SAFETY: CreateEx caller owns writable pitch*height bytes until
            // bitmap destruction; this view lasts only for sample encoding.
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(buffer.as_ptr(), *pitch as usize * *height as usize)
            };
            let Some(bitmap) =
                CfxDibitmapData::from_buffer(*width, *height, *format, *pitch, bytes)
            else {
                return 0;
            };
            unsafe {
                with_page_object(object, |object| {
                    object
                        .as_any_mut()
                        .downcast_mut::<CpdfImageObject>()
                        .is_some_and(|object| image_sdk::set_bitmap(object, &bitmap))
                        as c_int
                })
            }
            .unwrap_or(0)
        }
    }
}

fn bitmap_handle(bitmap: Option<CfxDibitmap>) -> *mut c_void {
    bitmap.map_or(std::ptr::null_mut(), |bitmap| {
        Box::into_raw(Box::new(BitmapHandle {
            storage: Rc::new(RefCell::new(BitmapStorage::Owned(bitmap))),
        }))
        .cast()
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_GetBitmap(object: *mut c_void) -> *mut c_void {
    bitmap_handle(
        unsafe { image_from_handle(object) }.and_then(|image| image_sdk::get_bitmap(&image)),
    )
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_GetRenderedBitmap(
    document: *mut c_void,
    page: *mut c_void,
    object: *mut c_void,
) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let page = unsafe { pdf_handle(page) };
    if page.is_some_and(|page| !Rc::ptr_eq(&document.document, &page._document)) {
        return std::ptr::null_mut();
    }
    let data = document.document.borrow().page_data_ref();
    let bitmap = unsafe {
        with_page_object_ref(object, |object| {
            image_sdk::rendered_bitmap(
                &data,
                page.and_then(PageHandle::pdf_page),
                object.as_any().downcast_ref::<CpdfImageObject>()?,
            )
        })
    }
    .flatten();
    bitmap_handle(bitmap)
}

/// The caller supplies buffer_len writable bytes; a short/null buffer is a
/// length query and receives no partial output, as in the source helpers.
unsafe fn copy_bytes(bytes: &[u8], buffer: *mut c_void, buffer_len: c_ulong) -> c_ulong {
    if !buffer.is_null() && buffer_len as usize >= bytes.len() {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), bytes.len());
        }
    }
    bytes.len() as c_ulong
}
unsafe fn image_data(
    object: *mut c_void,
    buffer: *mut c_void,
    buffer_len: c_ulong,
    decoded: bool,
) -> c_ulong {
    let Some(image) = (unsafe { image_from_handle(object) }) else {
        return 0;
    };
    image_sdk::with_image_data(&image, decoded, |bytes| unsafe {
        copy_bytes(bytes, buffer, buffer_len)
    })
    .unwrap_or(0)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_GetImageDataDecoded(
    object: *mut c_void,
    buffer: *mut c_void,
    buffer_len: c_ulong,
) -> c_ulong {
    unsafe { image_data(object, buffer, buffer_len, true) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_GetImageDataRaw(
    object: *mut c_void,
    buffer: *mut c_void,
    buffer_len: c_ulong,
) -> c_ulong {
    unsafe { image_data(object, buffer, buffer_len, false) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_GetImageFilterCount(object: *mut c_void) -> c_int {
    unsafe { image_from_handle(object) }.map_or(0, |image| image_sdk::filter_count(&image) as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_GetImageFilter(
    object: *mut c_void,
    index: c_int,
    buffer: *mut c_void,
    buffer_len: c_ulong,
) -> c_ulong {
    let Some(image) = (unsafe { image_from_handle(object) }) else {
        return 0;
    };
    let Some(mut filter) = usize::try_from(index)
        .ok()
        .and_then(|index| image_sdk::filter(&image, index))
    else {
        return 0;
    };
    filter.push(0);
    unsafe { copy_bytes(&filter, buffer, buffer_len) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_GetImageMetadata(
    object: *mut c_void,
    page: *mut c_void,
    metadata: *mut FPDF_IMAGEOBJ_METADATA,
) -> c_int {
    if metadata.is_null() {
        return 0;
    }
    let page = unsafe { pdf_handle(page) };
    let value = unsafe {
        with_page_object_ref(object, |object| {
            image_sdk::metadata(
                object.as_any().downcast_ref::<CpdfImageObject>()?,
                page.and_then(PageHandle::pdf_page),
            )
        })
    }
    .flatten();
    let Some(value) = value else {
        return 0;
    };
    let metadata = unsafe { &mut *metadata };
    metadata.width = value.width;
    metadata.height = value.height;
    if let Some((horizontal, vertical)) = value.dpi {
        metadata.horizontal_dpi = horizontal;
        metadata.vertical_dpi = vertical;
    }
    metadata.bits_per_pixel = value.bits_per_pixel;
    metadata.colorspace = value.colorspace;
    metadata.marked_content_id = value.marked_content_id;
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_GetImagePixelSize(
    object: *mut c_void,
    width: *mut u32,
    height: *mut u32,
) -> c_int {
    if width.is_null() || height.is_null() {
        return 0;
    }
    let Some(image) = (unsafe { image_from_handle(object) }) else {
        return 0;
    };
    unsafe {
        *width = image.pixel_width() as u32;
        *height = image.pixel_height() as u32;
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFImageObj_GetIccProfileDataDecoded(
    object: *mut c_void,
    page: *mut c_void,
    buffer: *mut u8,
    buffer_len: usize,
    output_len: *mut usize,
) -> c_int {
    if output_len.is_null() {
        return 0;
    }
    let Some(image) = (unsafe { image_from_handle(object) }) else {
        return 0;
    };
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return 0;
    };
    image_sdk::with_icc_profile(&image, page.pdf_page().unwrap(), |bytes| {
        unsafe {
            *output_len = bytes.len();
        }
        if !buffer.is_null() && buffer_len >= bytes.len() {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
            }
        }
    })
    .is_some() as c_int
}
