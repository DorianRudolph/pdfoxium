// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Complete native PDF417 high-level and writer test vectors.
use super::bc_pdf417::CbcPdf417;
use super::bc_pdf417_error_correction::CbcPdf417ErrorCorrection as EC;
use super::bc_pdf417_high_level_encoder::{CbcPdf417HighLevelEncoder as H, EncodingMode, SubMode};
use super::bc_pdf417_writer::CbcPdf417Writer;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

#[test]
fn native_encode_high_level() {
    let cases: &[(&[u32], &[u16])] = &[
        (&[], &[]),
        (&[11, 11, 48], &[901, 11, 11, 48]),
        (
            &[120, 120, 120, 120, 120, 120, 88, 120],
            &[900, 833, 713, 713, 717, 713],
        ),
        (
            &[120, 120, 120, 120, 120, 120, 33, 120],
            &[900, 833, 713, 713, 719, 323],
        ),
        (
            &[120, 120, 120, 120, 120, 120, 48, 120],
            &[900, 833, 713, 713, 718, 27, 719],
        ),
        (
            &[120, 120, 120, 120, 120, 120, 48, 88, 32],
            &[900, 833, 713, 713, 718, 28, 716],
        ),
        (
            &[120, 120, 120, 120, 120, 120, 11],
            &[900, 833, 713, 713, 719, 913, 901, 11],
        ),
        (
            &[48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48],
            &[902, 15, 217, 379, 11, 100],
        ),
    ];
    for (i, &(input, expected)) in cases.iter().enumerate() {
        assert_eq!(
            H::encode_high_level(input).as_deref(),
            Some(expected),
            "case {i}"
        );
    }
}
#[test]
fn native_encode_binary() {
    let cases: &[(&[u8], usize, usize, EncodingMode, &[u16])] = &[
        (&[], 0, 0, EncodingMode::Text, &[]),
        (&[120], 0, 1, EncodingMode::Text, &[913, 901, 120]),
        (
            &[120, 120, 120, 120, 120],
            0,
            5,
            EncodingMode::Text,
            &[901, 120, 120, 120, 120, 120],
        ),
        (
            &[120, 120, 120, 120, 120, 120],
            0,
            6,
            EncodingMode::Text,
            &[924, 201, 799, 298, 210, 720],
        ),
        (
            &[120, 120, 120, 120, 120, 120],
            0,
            6,
            EncodingMode::Numeric,
            &[924, 201, 799, 298, 210, 720],
        ),
    ];
    for (i, &(input, offset, count, mode, expected)) in cases.iter().enumerate() {
        let mut result = Vec::new();
        assert_eq!(
            H::encode_binary(input, offset, count, mode, &mut result),
            Some(())
        );
        assert_eq!(result, expected, "case {i}");
    }
}
#[test]
fn native_encode_numeric() {
    let cases: &[(&[u32], usize, usize, &[u16])] = &[
        (&[], 0, 0, &[]),
        (&[48], 0, 1, &[10]),
        (&[56, 48, 48], 0, 3, &[2, 0]),
        (&[49, 50, 51, 52, 53, 54], 0, 6, &[1, 348, 256]),
        (&[49, 50, 51, 52, 53, 54], 0, 5, &[124, 745]),
        (&[49, 50, 51, 52, 53, 54], 1, 5, &[137, 156]),
        (&[49, 50, 51, 52, 53, 54], 2, 2, &[134]),
        (
            &[
                48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
                48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
                48, 48,
            ],
            0,
            44,
            &[
                437, 111, 716, 132, 444, 118, 179, 92, 496, 847, 486, 144, 523, 411, 100,
            ],
        ),
        (
            &[
                48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
                48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
                48, 48, 48,
            ],
            0,
            45,
            &[
                437, 111, 716, 132, 444, 118, 179, 92, 496, 847, 486, 144, 523, 411, 100, 10,
            ],
        ),
        (
            &[
                48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
                48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
                48, 48, 56, 48, 48,
            ],
            0,
            47,
            &[
                437, 111, 716, 132, 444, 118, 179, 92, 496, 847, 486, 144, 523, 411, 100, 2, 0,
            ],
        ),
        (
            &[
                49, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
                48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
                48, 48, 48, 48, 48, 48, 48, 48,
            ],
            0,
            50,
            &[
                480, 752, 877, 685, 668, 490, 17, 11, 726, 572, 264, 699, 35, 722, 200, 1, 211, 100,
            ],
        ),
    ];
    for (i, &(input, offset, count, expected)) in cases.iter().enumerate() {
        let mut result = Vec::new();
        assert_eq!(
            H::encode_numeric(input, offset, count, &mut result),
            Some(())
        );
        assert_eq!(result, expected, "case {i}");
    }
}
#[test]
fn native_determine_consecutive_digit_count() {
    let cases: &[(&[u32], usize, usize)] = &[
        (&[], 0, 0),
        (&[88], 0, 0),
        (&[88, 49, 50, 51], 0, 0),
        (&[49], 0, 1),
        (&[49, 90], 0, 1),
        (&[49, 50, 51, 70, 79, 79, 52, 53, 54, 55, 56], 0, 3),
        (&[49, 50, 51, 70, 79, 79, 52, 53, 54, 55, 56], 3, 0),
        (&[49, 50, 51, 70, 79, 79, 52, 53, 54, 55, 56], 3, 0),
        (&[49, 50, 51, 70, 79, 79, 52, 53, 54, 55, 56], 6, 5),
    ];
    for (i, &(input, offset, expected)) in cases.iter().enumerate() {
        assert_eq!(
            H::determine_consecutive_digit_count(input, offset),
            expected,
            "case {i}"
        );
    }
}
#[test]
fn native_determine_consecutive_text_count() {
    let cases: &[(&[u32], usize, usize)] = &[
        (&[], 0, 0),
        (&[88], 0, 1),
        (&[88, 49, 50, 51], 0, 4),
        (&[49, 50, 51, 88], 0, 4),
        (&[65, 66, 67, 1, 88, 88, 88, 88], 0, 3),
        (&[65, 66, 67, 256, 88, 88, 88, 88], 0, 3),
        (&[65, 66, 67, 1, 88, 88, 88, 88], 4, 4),
        (&[65, 66, 67, 256, 88, 88, 88, 88], 4, 4),
        (&[256, 88, 88, 88], 0, 0),
        (&[49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 49, 50], 0, 12),
        (&[49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 49, 50, 51], 0, 0),
        (&[88, 49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 49, 50], 0, 13),
        (
            &[88, 49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 49, 50, 51],
            0,
            1,
        ),
        (
            &[
                88, 88, 88, 49, 50, 49, 88, 88, 88, 49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 49, 50,
                51, 52,
            ],
            0,
            9,
        ),
    ];
    for (i, &(input, offset, expected)) in cases.iter().enumerate() {
        assert_eq!(
            H::determine_consecutive_text_count(input, offset),
            expected,
            "case {i}"
        );
    }
}

// Both of these test declarations are empty TODOs upstream; cover the actual
// helpers here, including the corrected nonzero-offset punctuation lookahead.
#[test]
fn native_encode_text() {
    let mut result = Vec::new();
    assert_eq!(
        H::encode_text(&wide("ABCD"), 0, 4, SubMode::Alpha, &mut result),
        Some(SubMode::Alpha)
    );
    assert_eq!(result, [1, 63]);
    result.clear();
    assert_eq!(
        H::encode_text(&wide("abc"), 0, 3, SubMode::Alpha, &mut result),
        Some(SubMode::Lower)
    );
    assert_eq!(result, [810, 32]);
    result.clear();
    let original = wide("0!!");
    let offset = wide("prefix0!!suffix");
    assert_eq!(
        H::encode_text(&original, 0, 3, SubMode::Mixed, &mut result),
        Some(SubMode::Punctuation)
    );
    assert_eq!(result, [25, 310]);
    let mut offset_result = Vec::new();
    assert_eq!(
        H::encode_text(&offset, 6, 3, SubMode::Mixed, &mut offset_result),
        Some(SubMode::Punctuation)
    );
    assert_eq!(offset_result, result);
}
#[test]
fn native_consecutive_binary_count() {
    for (input, expected) in [
        ("", 0),
        ("\u{b}\u{b}0", 3),
        ("xxxxx", 0),
        ("\u{b}abcde", 1),
        ("1234567890123", 0),
        ("\u{b}1234567890123", 1),
    ] {
        assert_eq!(
            H::determine_consecutive_binary_count(&wide(input), input.as_bytes(), 0),
            Some(expected),
            "{input:?}"
        );
    }
    assert_eq!(H::determine_consecutive_binary_count(&[256], b"?", 0), None);
    assert_eq!(H::determine_consecutive_binary_count(&[256], b"", 0), None);
}
#[test]
fn native_writer_encode() {
    let mut writer = CbcPdf417Writer::new();
    for (input, expected) in [
        ("", include_bytes!("writer_empty.bin").as_slice()),
        (
            "hello world",
            include_bytes!("writer_hello_world.bin").as_slice(),
        ),
    ] {
        let result = writer.encode(&wide(input));
        assert_eq!(result.width, 579);
        assert_eq!(result.height, 16);
        assert_eq!(result.code.len(), expected.len());
        assert_eq!(result.code, expected, "{input:?}");
    }
}
#[test]
fn correction_levels_and_invalid_input() {
    let mut writer = CbcPdf417Writer::new();
    for level in 0..=8 {
        assert!(writer.set_error_correction_level(level));
        assert_eq!(
            EC::get_error_correction_codeword_count(level),
            1 << (level + 1)
        );
        let encoded = writer.encode(&wide("PDF417 numeric 123456789012345 and binary \u{1f600}"));
        assert!(!encoded.code.is_empty(), "level {level}");
        assert_eq!(
            encoded.code.len(),
            (encoded.width * encoded.height) as usize
        );
        assert!(encoded.code.iter().all(|&bit| bit <= 1));
    }
    assert!(!writer.set_error_correction_level(-1));
    assert!(!writer.set_error_correction_level(9));
    assert_eq!(EC::get_error_correction_codeword_count(-1), -1);
    assert_eq!(EC::get_error_correction_codeword_count(9), -1);
    assert!(EC::generate_error_correction(&[929], 0).is_none());
    assert!(EC::generate_error_correction(&[1], 9).is_none());
    assert!(H::encode_high_level(&[0x110000]).is_none());
    let mut encoder = CbcPdf417::new();
    encoder.set_dimensions(0, 0, 3, 3);
    assert!(!encoder.generate_barcode_logic(&wide("hello"), 0));
    assert!(!CbcPdf417::new().generate_barcode_logic(&vec![256; 4000], 0));
}
