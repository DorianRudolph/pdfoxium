// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_dib_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use crate::fpdfapi::page::cpdf_pagemodule::{destroy_page_module, initialize_page_module};
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

struct TestWithPageModule;
impl TestWithPageModule {
    fn set_up() -> Self {
        initialize_page_module();
        Self
    }
}
impl Drop for TestWithPageModule {
    fn drop(&mut self) {
        destroy_page_module();
    }
}

use super::CpdfDib;
use crate::fpdfapi::page::cpdf_image::CpdfImageLoadContext;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

const kWidth: i32 = 2;

fn LoadDeviceRgbImage(
    doc: &CpdfDocument,
    bits_per_component: i32,
    data: Vec<u8>,
) -> Option<CpdfDib> {
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"XObject");
    dict.set_new_name_for(b"Subtype", b"Image");
    dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(kWidth));
    dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
    dict.set_new_for(
        b"BitsPerComponent",
        CpdfScalarObject::number_from_signed(bits_per_component),
    );
    dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
    // Selected loading passes document/resource context explicitly.
    let color_space = doc
        .page_data()
        .get_color_space(&dict.get_direct_object_for(b"ColorSpace")?, None);
    let stream = retain_object(CpdfStream::from_data_and_dict(data, retain_object(dict)));
    CpdfDib::load_with_context(
        stream,
        color_space,
        CpdfImageLoadContext::new(doc.page_data(), None),
        false,
    )
}

#[test]
fn DefaultDecode4BitsPerComponent() {
    let _fixture = TestWithPageModule::set_up();
    let doc = CpdfDocument::new();
    let data = vec![0x08, 0xff, 0x08];
    let dib = LoadDeviceRgbImage(&doc, 4, data);
    assert!(dib.is_some());
    let dib = dib.unwrap();
    assert_eq!(&*dib.get_scanline(0), &[0xff, 0x88, 0x00, 0x88, 0x00, 0xff]);
}

#[test]
fn DefaultDecode8BitsPerComponent() {
    let _fixture = TestWithPageModule::set_up();
    let doc = CpdfDocument::new();
    let data = vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let dib = LoadDeviceRgbImage(&doc, 8, data);
    assert!(dib.is_some());
    let dib = dib.unwrap();
    assert_eq!(&*dib.get_scanline(0), &[0x33, 0x22, 0x11, 0x66, 0x55, 0x44]);
}

#[test]
fn DefaultDecode16BitsPerComponent() {
    let _fixture = TestWithPageModule::set_up();
    let doc = CpdfDocument::new();
    let data = vec![
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
    ];
    let dib = LoadDeviceRgbImage(&doc, 16, data);
    assert!(dib.is_some());
    let dib = dib.unwrap();
    assert_eq!(&*dib.get_scanline(0), &[0x55, 0x33, 0x11, 0xbb, 0x99, 0x77]);
}
