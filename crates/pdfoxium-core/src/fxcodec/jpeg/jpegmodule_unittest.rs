// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcodec/jpeg/jpegmodule_unittest.cpp`.
#![allow(non_snake_case)]

use super::{create_decoder, load_info};
use crate::fxcodec::scanlinedecoder::ScanlineDecoder;

fn LoadMonaLisa() -> Vec<u8> {
    include_bytes!("../../../../../upstream/pdfium/testing/resources/mona_lisa.jpg").to_vec()
}

#[test]
fn LoadInfoInvalid() {
    assert!(load_info(&[]).is_none());
    let invalid = vec![0x00, 0x01, 0x02, 0x03];
    assert!(load_info(&invalid).is_none());
}

#[test]
fn LoadInfoValid() {
    let jpeg_data = LoadMonaLisa();
    assert!(!jpeg_data.is_empty());
    let info = load_info(&jpeg_data);
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.width, 120);
    assert_eq!(info.height, 120);
    assert_eq!(info.components, 3);
    assert_eq!(info.bits_per_components, 8);
}

#[test]
fn CreateDecoderInvalid() {
    assert!(create_decoder(&[], 120, 120, 3, true, 1, false).is_none());
    let invalid = vec![0x00, 0x01, 0x02, 0x03];
    assert!(create_decoder(&invalid, 120, 120, 3, true, 1, false).is_none());
}

#[test]
fn CreateDecoderValid() {
    let jpeg_data = LoadMonaLisa();
    assert!(!jpeg_data.is_empty());
    let decoder = create_decoder(&jpeg_data, 120, 120, 3, true, 1, false);
    assert!(decoder.is_some());
    let mut decoder = decoder.unwrap();
    assert_eq!(decoder.get_width(), 120);
    assert_eq!(decoder.get_height(), 120);
    assert_eq!(decoder.count_comps(), 3);
    assert_eq!(decoder.get_bpc(), 8);
    for line in 0..120 {
        let scanline = decoder.get_scanline(line);
        assert!(!scanline.is_empty(), "line {line}");
        assert!(scanline.len() >= 360, "line {line}");
    }
    assert!(decoder.get_scanline(120).is_empty());
    let scanline0 = decoder.get_scanline(0);
    assert!(!scanline0.is_empty());
    assert!(scanline0.len() >= 360);
}

#[test]
fn CreateDecoderDownscaled() {
    let jpeg_data = LoadMonaLisa();
    assert!(!jpeg_data.is_empty());
    let decoder2 = create_decoder(&jpeg_data, 120, 120, 3, true, 2, false);
    assert!(decoder2.is_some());
    let mut decoder2 = decoder2.unwrap();
    assert_eq!(decoder2.get_width(), 60);
    assert_eq!(decoder2.get_height(), 60);
    for line in 0..60 {
        let scanline = decoder2.get_scanline(line);
        assert!(!scanline.is_empty(), "line {line}");
        assert!(scanline.len() >= 180, "line {line}");
    }
    let decoder4 = create_decoder(&jpeg_data, 120, 120, 3, true, 4, false);
    assert!(decoder4.is_some());
    let mut decoder4 = decoder4.unwrap();
    assert_eq!(decoder4.get_width(), 30);
    assert_eq!(decoder4.get_height(), 30);
    for line in 0..30 {
        let scanline = decoder4.get_scanline(line);
        assert!(!scanline.is_empty(), "line {line}");
        assert!(scanline.len() >= 90, "line {line}");
    }
    let decoder8 = create_decoder(&jpeg_data, 120, 120, 3, true, 8, false);
    assert!(decoder8.is_some());
    let mut decoder8 = decoder8.unwrap();
    assert_eq!(decoder8.get_width(), 15);
    assert_eq!(decoder8.get_height(), 15);
    for line in 0..15 {
        let scanline = decoder8.get_scanline(line);
        assert!(!scanline.is_empty(), "line {line}");
        assert!(scanline.len() >= 45, "line {line}");
    }
}

#[test]
fn CreateDecoderBgrOutput() {
    let jpeg_data = LoadMonaLisa();
    assert!(!jpeg_data.is_empty());
    let rgb_decoder = create_decoder(&jpeg_data, 120, 120, 3, true, 1, false);
    assert!(rgb_decoder.is_some());
    let mut rgb_decoder = rgb_decoder.unwrap();
    assert!(!rgb_decoder.scanlines_are_bgr());
    let bgr_decoder = create_decoder(&jpeg_data, 120, 120, 3, true, 1, true);
    assert!(bgr_decoder.is_some());
    let mut bgr_decoder = bgr_decoder.unwrap();
    // The selected Rust backend may leave BGR preference unfulfilled, exactly
    // as permitted by the native conditional. Preserve its full pixel checks.
    if bgr_decoder.scanlines_are_bgr() {
        assert_eq!(bgr_decoder.count_comps(), 3);
        assert_eq!(bgr_decoder.get_bpc(), 8);
        for line in 0..120 {
            let rgb_bytes = rgb_decoder.get_scanline(line);
            let bgr_bytes = bgr_decoder.get_scanline(line);
            assert_eq!(rgb_bytes.len(), 360, "line {line}");
            assert_eq!(bgr_bytes.len(), 360, "line {line}");
            for i in 0..120 {
                let rgb = &rgb_bytes[i * 3..i * 3 + 3];
                let bgr = &bgr_bytes[i * 3..i * 3 + 3];
                assert_eq!(rgb[0], bgr[2], "line {line} px {i}");
                assert_eq!(rgb[1], bgr[1], "line {line} px {i}");
                assert_eq!(rgb[2], bgr[0], "line {line} px {i}");
            }
        }
    }
}

#[test]
fn ReducedIdctMatchesPinnedLibjpegSamples() {
    // Independently generated with PDFium's pinned libjpeg-turbo revision
    // 640f254ad0fa03f6b1f29f89b7dd9366f2f6e533: JDCT_ISLOW, fancy upsampling,
    // scale_num=1, scale_denom=1/2/4/8, JCS_RGB or jpeg_color_space (raw YCbCr).
    // Hash all packed samples, excluding uninitialized native DWORD padding.
    // These references distinguish reduced IDCT from full decode + box resize.
    let references = [
        (
            true,
            1,
            0xb718bf5b83d7e061,
            [158, 160, 111],
            [149, 115, 51],
            [37, 27, 38],
        ),
        (
            true,
            2,
            0x788b915e3d3503b5,
            [157, 159, 109],
            [137, 106, 51],
            [30, 27, 34],
        ),
        (
            true,
            4,
            0xff03ebc20cb7962f,
            [164, 162, 113],
            [112, 88, 44],
            [24, 27, 32],
        ),
        (
            true,
            8,
            0x83e23b43fe9c1dad,
            [170, 164, 114],
            [161, 126, 62],
            [30, 29, 35],
        ),
        (
            false,
            1,
            0x837dddf8c3d50fc9,
            [154, 104, 131],
            [118, 90, 150],
            [31, 132, 132],
        ),
        (
            false,
            2,
            0x0496845ef99d4ac6,
            [153, 103, 131],
            [109, 95, 148],
            [29, 131, 129],
        ),
        (
            false,
            4,
            0xdcb5f551a0f4f566,
            [157, 103, 133],
            [90, 102, 144],
            [27, 131, 126],
        ),
        (
            false,
            8,
            0x08c7ac7f8f31c293,
            [160, 102, 135],
            [129, 90, 151],
            [30, 131, 128],
        ),
    ];
    let jpeg = LoadMonaLisa();
    for (transform, scale, expected_hash, first, middle, last) in references {
        let mut decoder = create_decoder(&jpeg, 120, 120, 3, transform, scale, false).unwrap();
        let side = (120 / scale) as usize;
        assert_eq!(decoder.get_width(), side as i32);
        assert_eq!(decoder.get_height(), side as i32);
        let mut hash = 0xcbf29ce484222325_u64;
        for y in 0..side {
            let line = decoder.get_scanline(y as i32);
            let pixels = &line[..side * 3];
            for &sample in pixels {
                hash = (hash ^ u64::from(sample)).wrapping_mul(0x100000001b3);
            }
            if y == 0 {
                assert_eq!(&pixels[..3], &first);
            }
            if y == side / 2 {
                assert_eq!(&pixels[(side / 2) * 3..][..3], &middle);
            }
            if y == side - 1 {
                assert_eq!(&pixels[(side - 1) * 3..][..3], &last);
            }
        }
        assert_eq!(hash, expected_hash, "transform={transform}, scale={scale}");
        assert!(decoder.get_scanline(side as i32).is_empty());
        assert_eq!(&decoder.get_scanline(0)[..3], &first);
    }
}

#[test]
fn PaddedEdgeMcuDisablesReducedDecoding() {
    // The one-row fixtures necessarily have padded bottom MCUs. PDFium's
    // LibjpegScanlineDecoder::InitDecode resets the requested scale to one so
    // encoder padding cannot darken a reduced right/bottom edge.
    for jpeg in [
        include_bytes!("../../../tests/fixtures/gray.jpg").as_slice(),
        include_bytes!("../../../tests/fixtures/cmyk.jpg").as_slice(),
    ] {
        let info = load_info(jpeg).unwrap();
        assert_eq!((info.width, info.height), (16, 1));
        let expected = super::decode(jpeg, 16, 1, true, 1).unwrap();
        for scale in [2, 4, 8] {
            let actual = super::decode(jpeg, 16, 1, true, scale).unwrap();
            assert_eq!((actual.width, actual.height), (16, 1));
            assert_eq!(actual.components, expected.components);
            assert_eq!(actual.pixels, expected.pixels);
        }
    }
}

#[test]
fn ConsumedOffsetSkipsHeaderPayloadAndStopsBeforeFollowingJpeg() {
    let jpeg = LoadMonaLisa();
    // A false EOI inside APP15 must not terminate the first image. A second
    // complete JPEG after it must not be included in the inline-image extent.
    let mut first = jpeg[..2].to_vec();
    first.extend_from_slice(&[0xff, 0xef, 0, 6, 0xff, 0xd9, 0xff, 0xda]);
    first.extend_from_slice(&jpeg[2..]);
    let endpoint = first.len();
    first.extend_from_slice(b" EI q ");
    first.extend_from_slice(&jpeg);
    let actual = super::decode(&first, 120, 120, true, 1).unwrap();
    let expected = super::decode(&jpeg, 120, 120, true, 1).unwrap();
    assert_eq!(actual.pixels, expected.pixels);
    assert_eq!(actual.bytes_consumed, endpoint as u32);
}

#[test]
fn GrayAndAdobeYcckKeepNativeSampleOrdering() {
    let gray = include_bytes!("../../../tests/fixtures/gray.jpg");
    let cmyk = include_bytes!("../../../tests/fixtures/cmyk.jpg");
    for transform in [false, true] {
        let gray = super::decode(gray, 16, 1, transform, 1).unwrap();
        assert_eq!(gray.components, 1);
        assert_eq!(gray.pixels, [vec![255; 8], vec![0; 8]].concat());
        // The fixture has APP14 Adobe transform=2 (YCCK), whose native default
        // output is CMYK regardless of the PDF ColorTransform setting. These
        // samples were independently checked with pinned C libjpeg-turbo.
        let cmyk = super::decode(cmyk, 16, 1, transform, 1).unwrap();
        assert_eq!(cmyk.components, 4);
        let expected = [[255, 0, 0, 255].repeat(8), [0, 0, 255, 255].repeat(8)].concat();
        assert_eq!(cmyk.pixels, expected);
    }
}

#[test]
fn RgbEncodedJpegPassesThroughWithEitherColorTransformSetting() {
    let original = LoadMonaLisa();
    // Re-label the three independently encoded 1x1 component planes as R/G/B.
    // Drop JFIF (which would force YCbCr interpretation), then change the SOF
    // component IDs and the corresponding SOS selectors. Entropy is unchanged.
    assert_eq!(
        &original[2..11],
        &[0xff, 0xe0, 0, 16, b'J', b'F', b'I', b'F', 0]
    );
    let mut rgb = original[..2].to_vec();
    rgb.extend_from_slice(&original[20..]);
    let sof = super::find_sof_marker(&rgb).unwrap();
    for (offset, id) in [(10, b'R'), (13, b'G'), (16, b'B')] {
        rgb[sof + offset] = id;
    }
    let mut offset = 2;
    while rgb[offset + 1] != 0xda {
        let length = u16::from_be_bytes([rgb[offset + 2], rgb[offset + 3]]) as usize;
        offset += length + 2;
    }
    for (relative, id) in [(5, b'R'), (7, b'G'), (9, b'B')] {
        rgb[offset + relative] = id;
    }
    assert!(!load_info(&rgb).unwrap().color_transform);
    let raw_planes = super::decode(&original, 120, 120, false, 1).unwrap();
    for transform in [false, true] {
        let actual = super::decode(&rgb, 120, 120, transform, 1).unwrap();
        assert_eq!(actual.components, 3);
        assert_eq!(actual.pixels, raw_planes.pixels);
    }
}

#[test]
fn AdobeMarkerOverridesDisabledPdfColorTransform() {
    let jpeg = LoadMonaLisa();
    let mut adobe = jpeg[..2].to_vec();
    adobe.extend_from_slice(&[
        0xff, 0xee, 0, 14, b'A', b'd', b'o', b'b', b'e', 0, 100, 0, 0, 0, 0, 1,
    ]);
    adobe.extend_from_slice(&jpeg[2..]);
    let expected = super::decode(&jpeg, 120, 120, true, 2).unwrap();
    let actual = super::decode(&adobe, 120, 120, false, 2).unwrap();
    assert_eq!((actual.width, actual.height), (60, 60));
    assert_eq!(actual.pixels, expected.pixels);
    let raw = super::decode(&jpeg, 120, 120, false, 2).unwrap();
    assert_ne!(actual.pixels, raw.pixels);
}
