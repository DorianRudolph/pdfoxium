// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
//! All three CBCDataMatrixWriterTest native tests, including all four matrices.
use super::bc_data_matrix_writer::CbcDataMatrixWriter;
fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}
#[test]
fn encode() {
    let mut writer = CbcDataMatrixWriter::new();
    {
        let expected: &[u8] = &[
            1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 1, 0, 1, 0, 0, 0, 0, 1,
            0, 1, 1, 1, 1, 0, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 1, 1, 0, 1, 0, 0, 0,
            0, 1, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 0, 0, 0, 0,
            0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        ];
        let result = writer.encode(&wide(""));
        assert_eq!(result.code.len(), expected.len());
        assert_eq!((result.width, result.height), (10, 10));
        assert_eq!(result.code, expected);
    }
    {
        let expected: &[u8] = &[
            1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1,
            0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1,
            1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 1, 1, 1, 1, 1, 0,
            1, 1, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 1, 0, 1, 0, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0, 0,
            0, 0, 1, 1, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 1, 0, 1,
            1, 1, 0, 1, 0, 0, 1, 1, 0, 1, 0, 1, 0, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1,
            0, 1, 0, 1, 1, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        ];
        let result = writer.encode(&wide("helloworld"));
        assert_eq!(result.code.len(), expected.len());
        assert_eq!((result.width, result.height), (14, 14));
        assert_eq!(result.code, expected);
    }
    {
        let expected: &[u8] = &[
            1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 1, 0, 1, 1,
            0, 1, 1, 0, 0, 1, 1, 0, 1, 0, 1, 1, 1, 0, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 1, 1, 1,
            1, 1, 1, 1, 0, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 0,
            1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        ];
        let result = writer.encode(&wide("12345"));
        assert_eq!(result.code.len(), expected.len());
        assert_eq!((result.width, result.height), (10, 10));
        assert_eq!(result.code, expected);
    }
    {
        let expected: &[u8] = &[
            1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 0, 1, 0, 0,
            1, 1, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 1,
            1, 0, 0, 1, 0, 0, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 1, 1, 1, 1, 0, 1, 0, 1,
            1, 0, 0, 1, 1, 0, 0, 0, 1, 1, 0, 1, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1, 0,
            0, 0, 0, 0, 1, 1, 0, 1, 0, 0, 1, 1, 1, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1,
            1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 0, 0, 1, 1, 0, 0,
            1, 0, 1, 1, 1, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0, 1,
            1, 1, 1, 1, 0, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 1, 0, 0, 0, 0, 1, 1, 0, 1, 1, 0, 0, 1, 0,
            1, 0, 1, 1, 0, 1, 1, 1, 0, 0, 0, 1, 0, 1, 1, 0, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0, 0, 1, 1,
            1, 0, 1, 0, 0, 1, 1, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1,
            1, 1, 0, 1, 0, 1, 0, 0, 0, 1, 1, 1, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
            1, 1, 1, 1, 1,
        ];
        let result = writer.encode(&wide("abcdefghijklmnopqrst"));
        assert_eq!(result.code.len(), expected.len());
        assert_eq!((result.width, result.height), (18, 18));
        assert_eq!(result.code, expected);
    }
    let result = writer.encode(&wide("hello world"));
    assert!(result.code.is_empty());
    assert_eq!((result.width, result.height), (0, 0));
}
#[test]
fn encode_limit_alpha_numeric() {
    let mut writer = CbcDataMatrixWriter::new();
    let mut input = vec![u32::from('a'); 2335];
    let result = writer.encode(&input);
    assert_eq!(result.code.len(), 20736);
    assert_eq!((result.width, result.height), (144, 144));
    input.push(u32::from('a'));
    let result = writer.encode(&input);
    assert!(result.code.is_empty());
    assert_eq!((result.width, result.height), (0, 0));
}
#[test]
fn encode_limit_numbers() {
    let mut writer = CbcDataMatrixWriter::new();
    let mut input = vec![u32::from('1'); 3116];
    let result = writer.encode(&input);
    assert_eq!(result.code.len(), 20736);
    assert_eq!((result.width, result.height), (144, 144));
    input.push(u32::from('1'));
    let result = writer.encode(&input);
    assert!(result.code.is_empty());
    assert_eq!((result.width, result.height), (0, 0));
}

#[test]
fn mode_codeword_vectors() {
    use super::{
        bc_encoder::{encode, Encoding},
        bc_encoder_context::CbcEncoderContext,
    };
    // Values calculated from the ECC200 triplet and six-bit packing definitions.
    for (mode, input, latch, expected) in [
        (Encoding::C40, "ABC", 230, &[230, 89, 233][..]),
        (Encoding::Text, "abc", 239, &[239, 89, 233][..]),
        (Encoding::X12, "*>\r", 238, &[238, 6, 145][..]),
        (Encoding::Edifact, "ABCD", 240, &[240, 4, 32, 196][..]),
    ] {
        let mut context = CbcEncoderContext::new(&wide(input));
        context.codewords.push(latch);
        assert!(encode(mode, &mut context));
        assert_eq!(context.codewords, expected, "{mode:?}");
        assert!(!context.has_more_characters());
    }
}

#[test]
fn base256_exact_capacity_and_two_byte_length() {
    use super::{bc_base256_encoder, bc_encoder_context::CbcEncoderContext};
    let unrandomize = |value: u8, position: usize| {
        (usize::from(value) + 256 - ((149 * position) % 255 + 1)) % 256
    };
    for count in [1, 249, 250, 1556] {
        let mut context = CbcEncoderContext::new(&[]);
        // Test the encoder's bytes directly: the outer writer supplies UTF-8.
        context.msg = vec![128; count];
        context.codewords.push(231);
        assert!(bc_base256_encoder::encode(&mut context));
        let decoded: Vec<usize> = context
            .codewords
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, &byte)| unrandomize(byte, index + 1))
            .collect();
        if count == 1 || count == 1556 {
            // A zero length consumes the remainder of a completely full symbol.
            assert_eq!(decoded[0], 0);
            assert_eq!(decoded[1..], vec![128; count]);
            assert_eq!(context.codewords.len(), context.capacity());
        } else if count == 249 {
            assert_eq!(decoded[0], 249);
            assert_eq!(decoded[1..], vec![128; count]);
        } else {
            assert_eq!(&decoded[..2], &[250, 0]);
            assert_eq!(decoded[2..], vec![128; count]);
        }
    }
}

#[test]
fn macro_headers_utf8_and_invalid_scalar_boundaries() {
    use super::{bc_encoder_context::CbcEncoderContext, bc_high_level_encoder::encode_high_level};
    assert_eq!(CbcEncoderContext::new(&wide("é")).msg, [0xc3, 0xa9]);
    assert_eq!(
        encode_high_level(&wide("[)>\x1e05\x1e\x04")),
        [236, 129, 70]
    );
    assert_eq!(
        encode_high_level(&wide("[)>\x1e06\x1e\x04")),
        [237, 129, 70]
    );
    let mut invalid = vec![0x110000; 7];
    invalid.push(4);
    // FX_UTF8Encode skips out-of-range scalars; macro matching uses source scalars.
    assert_eq!(encode_high_level(&invalid), [5, 129, 70]);
}

#[test]
fn all_symbol_regions_ecc_and_placement() {
    use super::{
        bc_default_placement::CbcDefaultPlacement,
        bc_error_correction::encode_ecc200,
        bc_symbol_info::{CbcSymbolInfo, SYMBOLS},
    };
    for symbol in &SYMBOLS {
        let data = vec![0; symbol.data_capacity];
        let encoded = encode_ecc200(&data, symbol).expect("supported ECC polynomial");
        assert_eq!(encoded.len(), symbol.data_capacity + symbol.error_codewords);
        // Zero is a codeword of every Reed-Solomon block, including 144x144.
        assert!(encoded.iter().all(|&byte| byte == 0));
        assert!(CbcDefaultPlacement::new(
            encoded,
            symbol.symbol_data_width(),
            symbol.symbol_data_height()
        )
        .is_some());
        assert!(encode_ecc200(&[], symbol).is_none());
    }
    let largest = CbcSymbolInfo::lookup(1558, false).unwrap();
    assert_eq!(largest.interleaved_block_count(), 10);
    assert_eq!(
        (0..10)
            .map(|i| largest.data_length_for_interleaved_block(i))
            .sum::<usize>(),
        1558
    );
    assert!(CbcSymbolInfo::lookup(1559, true).is_none());
    assert!(CbcDefaultPlacement::new(Vec::new(), 0, 8).is_none());
    assert!(CbcDefaultPlacement::new(Vec::new(), 8, 8).is_none());
}
