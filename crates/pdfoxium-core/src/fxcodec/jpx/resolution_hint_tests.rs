// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Display hints may exceed available COD/COC levels; exact codec requests stay strict.

use super::{CjpxDecoder, JpxCreateError};
use hayro_jpeg2000::{DecodeError, ValidationError};

fn same_planes(actual: &CjpxDecoder<'_>, expected: &CjpxDecoder<'_>) {
    assert_eq!(actual.info.width, expected.info.width);
    assert_eq!(actual.info.height, expected.info.height);
    assert_eq!(actual.info.channels, expected.info.channels);
    assert_eq!(actual.info.colorspace, expected.info.colorspace);
    assert_eq!(actual.components.len(), expected.components.len());
    for (a, b) in actual.components.iter().zip(&expected.components) {
        assert_eq!(
            (a.width, a.height, a.dx, a.dy),
            (b.width, b.height, b.dx, b.dy)
        );
        assert_eq!((a.precision, a.signed), (b.precision, b.signed));
        assert_eq!(a.samples, b.samples);
    }
}

#[test]
fn hint_keeps_available_reduction_and_caps_to_the_last_resolution() {
    let source = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/gray.jp2"
    ));
    // This checked-in stream has two decomposition levels (three resolutions).
    assert!(CjpxDecoder::create(source, false, 3).is_none());
    for (requested, expected_skip) in [(0, 0), (1, 1), (2, 2), (8, 2), (32, 2)] {
        let (actual, skip) =
            CjpxDecoder::create_with_resolution_hint(source, false, requested).unwrap();
        assert_eq!(skip, expected_skip);
        same_planes(
            &actual,
            &CjpxDecoder::create(source, false, expected_skip).unwrap(),
        );
    }
    assert!(CjpxDecoder::create_with_resolution_hint(source, false, 33).is_none());
}

#[test]
fn zero_decomposition_image_remains_visible_for_a_tiny_display_hint() {
    // A genuine zero-decomposition, one-component 32x32 stream with empty
    // packets. The native level shift makes every visible sample mid-gray.
    let mut source = super::tests::empty_packet_codestream(1, 1);
    for offset in [8, 12, 24, 28] {
        source[offset..offset + 4].copy_from_slice(&32u32.to_be_bytes());
    }
    assert!(matches!(
        CjpxDecoder::create_exact(&source, false, 5),
        Err(JpxCreateError::Codec(DecodeError::Validation(
            ValidationError::TooManySkippedResolutionLevels
        )))
    ));
    assert!(CjpxDecoder::create(&source, false, 5).is_none());
    let (decoder, skip) = CjpxDecoder::create_with_resolution_hint(&source, false, 5).unwrap();
    assert_eq!(skip, 0);
    assert_eq!((decoder.info.width, decoder.info.height), (32, 32));
    assert_eq!(decoder.components[0].samples, [128.0; 32 * 32]);
}

#[test]
fn main_and_tile_coding_overrides_report_the_same_reduction_limit() {
    let source = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/rgb.j2c"
    ));
    let cod = source
        .windows(2)
        .position(|bytes| bytes == [255, 82])
        .unwrap();
    let sot = source
        .windows(2)
        .position(|bytes| bytes == [255, 144])
        .unwrap();
    let original = &source[cod..cod + 14];
    let mut shallow_cod = original.to_vec();
    shallow_cod[9] = 0;
    let shallow_coc = [255, 83, 0, 9, 0, 0, 0, 4, 4, 0, 1];
    for tile in [false, true] {
        for shallow in [shallow_cod.as_slice(), shallow_coc.as_slice()] {
            let mut altered = source.to_vec();
            let mut markers = shallow.to_vec();
            // Even if later reset to the original COD, pinned OpenJPEG checks
            // the reduction against every encountered coding-style marker.
            markers.extend_from_slice(original);
            let insertion = if tile { sot + 12 } else { sot };
            if tile {
                let length = u32::from_be_bytes(altered[sot + 6..sot + 10].try_into().unwrap());
                altered[sot + 6..sot + 10]
                    .copy_from_slice(&(length + markers.len() as u32).to_be_bytes());
            }
            altered.splice(insertion..insertion, markers);
            assert!(matches!(
                CjpxDecoder::create_exact(&altered, false, 1),
                Err(JpxCreateError::Codec(DecodeError::Validation(
                    ValidationError::TooManySkippedResolutionLevels
                )))
            ));
            let (actual, skip) =
                CjpxDecoder::create_with_resolution_hint(&altered, false, 1).unwrap();
            assert_eq!(skip, 0);
            same_planes(&actual, &CjpxDecoder::create(&altered, false, 0).unwrap());
        }
    }
}

#[test]
fn invalid_containers_and_component_metadata_remain_failures() {
    // The strict JP2/raw-codestream wrapper can reject these at different
    // parsing stages. The rendering contract is a fatal error, never the
    // recoverable shortage of available resolution levels.
    let assert_fatal = |source: &[u8], skip| {
        match CjpxDecoder::create_exact(source, false, skip) {
            Err(JpxCreateError::Codec(DecodeError::Validation(
                ValidationError::TooManySkippedResolutionLevels,
            ))) => panic!("malformed input was classified as a recoverable reduction hint"),
            Err(_) => {}
            Ok(_) => panic!("malformed input was accepted"),
        }
        assert!(CjpxDecoder::create_with_resolution_hint(source, false, skip).is_none());
    };
    for source in [b"".as_slice(), b"not a JP2 codestream".as_slice()] {
        for skip in [0, 8] {
            assert_fatal(source, skip);
        }
    }
    let mut source = super::tests::empty_packet_codestream(1, 1);
    source[44] = 0; // Zero vertical component sampling is not a reduction limit.
    for skip in [0, 8] {
        assert_fatal(&source, skip);
    }
}
