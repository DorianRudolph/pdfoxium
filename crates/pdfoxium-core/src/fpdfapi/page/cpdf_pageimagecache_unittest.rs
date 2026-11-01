// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_pageimagecache_unittest.cpp`.
//! Rust pages always own their image cache. AsPDFPage is a concrete page type;
//! native ClearView is a no-op here because these fixtures install no view.
#![allow(non_snake_case)]
use super::*;
use crate::fpdfapi::page::cpdf_colorspace::ColorSpaceFamily;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_pagemodule::{destroy_page_module, initialize_page_module};
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_parser::ParserError;
use crate::fxcrt::cfx_fileaccess_stream::CfxFileAccessStream;
use crate::fxcrt::fx_coordinates::CfxSize;

struct TestWithPageModule;
impl TestWithPageModule {
    fn new() -> Self {
        initialize_page_module();
        Self
    }
}
impl Drop for TestWithPageModule {
    fn drop(&mut self) {
        destroy_page_module();
    }
}

fn load_page(filename: &str) -> (CpdfDocument, CpdfPage) {
    let file_path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources"
    ))
    .join(filename);
    assert!(!file_path.as_os_str().is_empty());
    let mut document = CpdfDocument::new();
    assert_eq!(
        document.load_doc(
            Rc::new(
                CfxFileAccessStream::create_from_filename(file_path.to_str().unwrap()).unwrap()
            ),
            b""
        ),
        ParserError::Success
    );
    let page_dict = document.page_dictionary(0);
    assert!(page_dict.is_some());
    drop(page_dict);
    let mut page = CpdfPage::from_document(&mut document, 0).unwrap();
    // Construction adds the page-owned image cache in the Rust interface.
    page.parse_path_content();
    (document, page)
}

#[test]
fn RenderBug1924() {
    let _fixture = TestWithPageModule::new();
    // A small cached JPEG2000 image must be regenerated for a larger request.
    {
        let (_document, page) = load_page("jpx_lzw.pdf");
        let page_image_cache = page.image_cache();
        let page_obj = page.holder().page_object_by_index(0);
        assert!(page_obj.is_some());
        let image = page_obj.unwrap().as_any().downcast_ref::<CpdfImageObject>();
        assert!(image.is_some());
        let image = image.unwrap();
        let resources = page.resources();
        let mut context = CpdfImageLoadContext::new(page.page_data(), resources.as_ref());
        context.std_cs = true;
        context.group_family = ColorSpaceFamily::IccBased;
        context.load_mask = false;
        context.max_size = CfxSize::new(50, 50);
        // Render with small scale.
        let mut should_continue =
            page_image_cache.start_get_cached_bitmap(image.image().unwrap(), context);
        while should_continue {
            should_continue = page_image_cache.continue_load(None);
        }
        let bitmap_small = page_image_cache.detach_current_bitmap().unwrap();
        // And render with large scale.
        context.max_size = CfxSize::new(100, 100);
        should_continue = page_image_cache.start_get_cached_bitmap(image.image().unwrap(), context);
        while should_continue {
            should_continue = page_image_cache.continue_load(None);
        }
        let bitmap_large = page_image_cache.detach_current_bitmap().unwrap();
        assert!(bitmap_large.width() > bitmap_small.width());
        assert!(bitmap_large.height() > bitmap_small.height());
        let _: &CpdfPage = &page;
    }
}

#[test]
fn RenderReducedDctThenFullSize() {
    let _fixture = TestWithPageModule::new();
    // Original 400x400 MCU-aligned JPEG allows 1/8 and 1/4 decoding.
    let (_document, page) = load_page("jpeg_reduced_size.pdf");
    let page_image_cache = page.image_cache();
    let page_obj = page.holder().page_object_by_index(0);
    assert!(page_obj.is_some());
    let image = page_obj.unwrap().as_any().downcast_ref::<CpdfImageObject>();
    assert!(image.is_some());
    let image = image.unwrap();
    let resources = page.resources();
    let mut context = CpdfImageLoadContext::new(page.page_data(), resources.as_ref());
    context.std_cs = true;
    context.group_family = ColorSpaceFamily::Unknown;
    context.load_mask = false;
    context.max_size = CfxSize::new(50, 50);
    // Render with small scale: 400/50 -> 1/8, so a 50x50 bitmap.
    let mut should_continue =
        page_image_cache.start_get_cached_bitmap(image.image().unwrap(), context);
    while should_continue {
        should_continue = page_image_cache.continue_load(None);
    }
    let bitmap_small = page_image_cache.detach_current_bitmap();
    assert!(bitmap_small.is_some());
    let bitmap_small = bitmap_small.unwrap();
    assert_eq!(bitmap_small.width(), 50);
    assert_eq!(bitmap_small.height(), 50);
    // A larger request must re-decode at 1/4 rather than upscale the cache.
    context.max_size = CfxSize::new(100, 100);
    should_continue = page_image_cache.start_get_cached_bitmap(image.image().unwrap(), context);
    while should_continue {
        should_continue = page_image_cache.continue_load(None);
    }
    let bitmap_large = page_image_cache.detach_current_bitmap();
    assert!(bitmap_large.is_some());
    let bitmap_large = bitmap_large.unwrap();
    assert_eq!(bitmap_large.width(), 100);
    assert_eq!(bitmap_large.height(), 100);
    let _: &CpdfPage = &page;
}

#[test]
fn RenderReducedDctWithSMask() {
    let _fixture = TestWithPageModule::new();
    let (_document, page) = load_page("jpeg_reduced_size_with_smask.pdf");
    let page_image_cache = page.image_cache();
    let page_obj = page.holder().page_object_by_index(0);
    assert!(page_obj.is_some());
    let image = page_obj.unwrap().as_any().downcast_ref::<CpdfImageObject>();
    assert!(image.is_some());
    let image = image.unwrap();
    let resources = page.resources();
    let mut context = CpdfImageLoadContext::new(page.page_data(), resources.as_ref());
    context.std_cs = true;
    context.group_family = ColorSpaceFamily::Unknown;
    context.load_mask = true;
    context.max_size = CfxSize::new(50, 50);
    let mut should_continue =
        page_image_cache.start_get_cached_bitmap(image.image().unwrap(), context);
    while should_continue {
        should_continue = page_image_cache.continue_load(None);
    }
    let bitmap = page_image_cache.detach_current_bitmap();
    let mask = page_image_cache.detach_current_mask();
    assert!(bitmap.is_some());
    assert!(mask.is_some());
    let bitmap = bitmap.unwrap();
    let mask = mask.unwrap();
    // The base reduces; the mask loads at full size and scales independently.
    assert_eq!(bitmap.width(), 50);
    assert_eq!(bitmap.height(), 50);
    assert_eq!(mask.width(), 400);
    assert_eq!(mask.height(), 400);
    let _: &CpdfPage = &page;
}

#[test]
fn NonMcuAlignedDctIsNotReduced() {
    let _fixture = TestWithPageModule::new();
    // This 408x408 4:2:0 image has 16x16 MCUs: 408 % 16 == 8.
    let (_document, page) = load_page("jpeg_unaligned_no_reduce.pdf");
    let page_image_cache = page.image_cache();
    let page_obj = page.holder().page_object_by_index(0);
    assert!(page_obj.is_some());
    let image = page_obj.unwrap().as_any().downcast_ref::<CpdfImageObject>();
    assert!(image.is_some());
    let image = image.unwrap();
    let resources = page.resources();
    let mut context = CpdfImageLoadContext::new(page.page_data(), resources.as_ref());
    context.std_cs = true;
    context.group_family = ColorSpaceFamily::Unknown;
    context.load_mask = false;
    context.max_size = CfxSize::new(50, 50);
    let mut should_continue =
        page_image_cache.start_get_cached_bitmap(image.image().unwrap(), context);
    while should_continue {
        should_continue = page_image_cache.continue_load(None);
    }
    let bitmap = page_image_cache.detach_current_bitmap();
    assert!(bitmap.is_some());
    let bitmap = bitmap.unwrap();
    assert_eq!(bitmap.width(), 408);
    assert_eq!(bitmap.height(), 408);
    let _: &CpdfPage = &page;
}
