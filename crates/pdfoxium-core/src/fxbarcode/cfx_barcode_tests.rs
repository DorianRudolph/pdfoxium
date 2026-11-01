// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Native barcode input and bitmap fixtures, through the Rust AGG device.
use super::*;
use crate::{
    fdrm::fx_crypt::crypt_md5_generate,
    fxge::dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
};
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn bitmap_case(kind: BcType, contents: &str, expected: &str) {
    let mut bitmap = CfxDibitmap::create(640, 480, FxdibFormat::Bgrx, 0).unwrap();
    let mut code = CfxBarcode::new(kind);
    code.set_height(298);
    code.set_width(418);
    assert!(code.encode(&wide(contents)));
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        assert!(code.render_device(&mut device, CfxMatrix::default()));
    }
    // Geometric clipping matches the pinned native bitmap, including AA at
    // the background stroke's corner. No checksum normalization is needed.
    let checksum: String = crypt_md5_generate(bitmap.buffer())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(checksum, expected);
}

#[test]
fn background_stroke_corner_matches_translated_crop_with_small_aa_difference() {
    let mut code = CfxBarcode::new(BcType::QrCode);
    code.set_height(298);
    code.set_width(418);
    assert!(code.encode(&wide("clams")));
    let mut clipped = CfxDibitmap::create(640, 480, FxdibFormat::Bgrx, 0).unwrap();
    let mut translated = CfxDibitmap::create(644, 484, FxdibFormat::Bgrx, 0).unwrap();
    {
        let mut driver = CfxAggDeviceDriver::new(&mut clipped, false, None);
        assert!(code.render_device(&mut driver, CfxMatrix::default()));
    }
    {
        let mut driver = CfxAggDeviceDriver::new(&mut translated, false, None);
        let matrix = CfxMatrix {
            e: 2.0,
            f: 2.0,
            ..CfxMatrix::default()
        };
        assert!(code.render_device(&mut driver, matrix));
    }
    // Moving the background stroke entirely inside the surface changes its
    // rounded geometric intersection by one AA level, without moving the edge.
    assert_eq!(&clipped.scanline(298).unwrap()[..4], &[128, 128, 128, 0]);
    assert_eq!(
        &translated.scanline(300).unwrap()[8..12],
        &[127, 127, 127, 0]
    );
    for y in 0..480 {
        let actual = &clipped.scanline(y).unwrap()[..640 * 4];
        let expected = &translated.scanline(y + 2).unwrap()[2 * 4..642 * 4];
        for (channel, (&a, &b)) in actual.iter().zip(expected).enumerate() {
            assert!(a.abs_diff(b) <= 2, "row {y}, channel {channel}: {a} vs {b}");
        }
    }
}

#[test]
fn code39() {
    bitmap_case(BcType::Code39, "CLAMS", "cd4cd3f36da38ff58d9f621827018903");
}

#[test]
fn coda_bar() {
    bitmap_case(
        BcType::Codabar,
        "$123-456",
        "5fad4fc19f099001a0fe83c89430c977",
    );
}

#[test]
fn coda_bar_letters() {
    assert!(!CfxBarcode::new(BcType::Codabar).encode(&wide("clams")));
}

#[test]
fn code128() {
    bitmap_case(BcType::Code128, "Clams", "6351f0f6e997050e4658bbb4777aef74");
}

#[test]
fn code128_b() {
    bitmap_case(
        BcType::Code128B,
        "Clams",
        "6351f0f6e997050e4658bbb4777aef74",
    );
}

#[test]
fn code128_c() {
    bitmap_case(
        BcType::Code128C,
        "123456",
        "fba730a807ba6363f9bd2bc7f8c56d1f",
    );
}

#[test]
fn code128_cletters() {
    bitmap_case(
        BcType::Code128C,
        "clams",
        "6284ec8503d5a948c9518108da33cdd3",
    );
}

#[test]
fn ean8() {
    bitmap_case(BcType::Ean8, "123456", "aff88491ac46ca6217d780d185300cde");
}

#[test]
fn ean8_letters() {
    assert!(!CfxBarcode::new(BcType::Ean8).encode(&wide("clams")));
}

#[test]
fn upca() {
    bitmap_case(BcType::Upca, "123456", "fe26a5714cff7ffe3f9b02183efc435b");
}

#[test]
fn upcaletters() {
    assert!(!CfxBarcode::new(BcType::Upca).encode(&wide("clams")));
}

#[test]
fn ean13() {
    bitmap_case(BcType::Ean13, "123456", "72d2190b98d635c32834bf67552e561e");
}

#[test]
fn ean13_letters() {
    assert!(!CfxBarcode::new(BcType::Ean13).encode(&wide("clams")));
}

#[test]
fn pdf417() {
    bitmap_case(BcType::Pdf417, "clams", "191e35d11613901b7d5d51033689aa89");
}

#[test]
fn data_matrix() {
    bitmap_case(
        BcType::DataMatrix,
        "clams",
        "5e5cd9a680b86fcd4ffd53ed36e3c980",
    );
}

#[test]
fn qr_code() {
    bitmap_case(BcType::QrCode, "clams", "4751c6e0f67749fabe24f787128decee");
}

#[test]
fn settings_retain_ranges_and_format_restrictions() {
    let mut code = CfxBarcode::new(BcType::Code128);
    assert_eq!(code.get_type(), BcType::Code128);
    for kind in [BcType::Code128B, BcType::Code128C] {
        assert_eq!(CfxBarcode::new(kind).get_type(), BcType::Code128);
    }
    for dimension in [0, -1, 11, i32::MAX] {
        assert!(!code.set_module_height(dimension));
        assert!(!code.set_module_width(dimension));
    }
    for dimension in [1, 10] {
        assert!(code.set_module_height(dimension));
        assert!(code.set_module_width(dimension));
    }
    assert!(code.set_data_length(5));
    assert!(code.set_print_checksum(false));
    assert!(code.set_cal_checksum(true));
    assert!(!code.set_error_correction_level(1));
    let mut qr = CfxBarcode::new(BcType::QrCode);
    for level in 0..4 {
        assert!(qr.set_error_correction_level(level));
    }
    assert!(!qr.set_error_correction_level(-1));
    assert!(!qr.set_error_correction_level(4));
    assert!(!qr.set_data_length(5));
    assert!(!qr.set_print_checksum(false));
    assert!(!qr.set_cal_checksum(true));
    assert!(!qr.set_start_char(b'A'));
    assert!(!qr.set_wide_narrow_ratio(2));
    let mut unknown = CfxBarcode::new(BcType::Unknown);
    assert!(!unknown.encode(&wide("value")));
}

#[test]
fn failed_two_dimensional_results_discard_the_previous_drawing() {
    use crate::fxbarcode::bc_two_dim_writer::{CbcTwoDimWriter, EncodeResult};
    let mut writer = CbcTwoDimWriter::new(true);
    writer.writer.set_width(8);
    writer.writer.set_height(8);
    assert!(writer.render_result(&EncodeResult::new(vec![1, 0, 0, 1], 2, 2)));
    let mut bitmap = CfxDibitmap::create(8, 8, FxdibFormat::Bgra, 0).unwrap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        assert!(writer.render_device_result(&mut device, CfxMatrix::default()));
    }
    assert_eq!(
        &bitmap.buffer()[3 * bitmap.pitch() as usize + 3 * 4..][..4],
        &[0, 0, 0, 255]
    );
    assert_eq!(
        &bitmap.buffer()[2 * bitmap.pitch() as usize + 2 * 4..][..4],
        &[255, 255, 255, 255]
    );
    assert!(!writer.render_result(&EncodeResult::new(vec![1], 2, 2)));
    let before = bitmap.buffer().to_vec();
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        assert!(!writer.render_device_result(&mut device, CfxMatrix::default()));
    }
    assert_eq!(bitmap.buffer(), before);
    assert!(!writer.render_result(&EncodeResult::new(vec![1], i32::MAX, i32::MAX)));
}
