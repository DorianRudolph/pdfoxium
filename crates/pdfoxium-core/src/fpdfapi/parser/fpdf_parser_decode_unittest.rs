// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/fpdf_parser_decode_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::fpdfapi::parser::{
    cpdf_indirect_object_holder::CpdfIndirectObjectHolder, cpdf_name::CpdfName,
    cpdf_object::retain_object, cpdf_reference::CpdfReference, cpdf_string::CpdfString,
};

#[test]
fn ValidateDecoderPipeline() {
    {
        // Empty decoder list is always valid.
        let decoders = CpdfArray::new();
        assert!(validate_decoder_pipeline(&decoders));
    }
    {
        // 1 decoder is almost always valid.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"FlateEncode"));
        assert!(validate_decoder_pipeline(&decoders));
    }
    {
        // 1 decoder is almost always valid, even with an unknown decoder.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"FooBar"));
        assert!(validate_decoder_pipeline(&decoders));
    }
    {
        // Valid 2 decoder pipeline.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"AHx"));
        decoders.append_new(CpdfName::new(None, b"LZWDecode"));
        assert!(validate_decoder_pipeline(&decoders));
    }
    {
        // Valid 2 decoder pipeline.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"ASCII85Decode"));
        decoders.append_new(CpdfName::new(None, b"ASCII85Decode"));
        assert!(validate_decoder_pipeline(&decoders));
    }
    {
        // Valid 5 decoder pipeline.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"ASCII85Decode"));
        decoders.append_new(CpdfName::new(None, b"A85"));
        decoders.append_new(CpdfName::new(None, b"RunLengthDecode"));
        decoders.append_new(CpdfName::new(None, b"FlateDecode"));
        decoders.append_new(CpdfName::new(None, b"RL"));
        assert!(validate_decoder_pipeline(&decoders));
    }
    {
        // Valid 5 decoder pipeline, with an image decoder at the end.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"RunLengthDecode"));
        decoders.append_new(CpdfName::new(None, b"ASCII85Decode"));
        decoders.append_new(CpdfName::new(None, b"FlateDecode"));
        decoders.append_new(CpdfName::new(None, b"LZW"));
        decoders.append_new(CpdfName::new(None, b"DCTDecode"));
        assert!(validate_decoder_pipeline(&decoders));
    }
    {
        // Invalid 1 decoder pipeline due to wrong type.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfString::from_bytes(None, b"FlateEncode"));
        assert!(!validate_decoder_pipeline(&decoders));
    }
    {
        // Invalid 2 decoder pipeline, with 2 image decoders.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"DCTDecode"));
        decoders.append_new(CpdfName::new(None, b"CCITTFaxDecode"));
        assert!(!validate_decoder_pipeline(&decoders));
    }
    {
        // Invalid 2 decoder pipeline, with 1 image decoder at the start.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"DCTDecode"));
        decoders.append_new(CpdfName::new(None, b"FlateDecode"));
        assert!(!validate_decoder_pipeline(&decoders));
    }
    {
        // Invalid 2 decoder pipeline due to wrong type.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfString::from_bytes(None, b"AHx"));
        decoders.append_new(CpdfName::new(None, b"LZWDecode"));
        assert!(!validate_decoder_pipeline(&decoders));
    }
    {
        // Invalid 5 decoder pipeline.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"FlateDecode"));
        decoders.append_new(CpdfName::new(None, b"FlateDecode"));
        decoders.append_new(CpdfName::new(None, b"DCTDecode"));
        decoders.append_new(CpdfName::new(None, b"FlateDecode"));
        decoders.append_new(CpdfName::new(None, b"FlateDecode"));
        assert!(!validate_decoder_pipeline(&decoders));
    }
    {
        // Invalid 5 decoder pipeline due to wrong type.
        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"ASCII85Decode"));
        decoders.append_new(CpdfName::new(None, b"A85"));
        decoders.append_new(CpdfName::new(None, b"RunLengthDecode"));
        decoders.append_new(CpdfName::new(None, b"FlateDecode"));
        decoders.append_new(CpdfString::from_bytes(None, b"RL"));
        assert!(!validate_decoder_pipeline(&decoders));
    }
}

#[test]
fn ValidateDecoderPipelineWithIndirectObjects() {
    {
        // Valid 2 decoder pipeline with indirect objects.
        let objects_holder = CpdfIndirectObjectHolder::new();
        let decoder = retain_object(CpdfName::new(None, b"FlateDecode"));
        let decoder_number = objects_holder.borrow_mut().add_indirect_object(decoder);

        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfReference::new(&objects_holder, decoder_number));
        decoders.append_new(CpdfName::new(None, b"LZW"));
        assert!(validate_decoder_pipeline(&decoders));
    }
    {
        // Valid 5 decoder pipeline with indirect objects, with an image decoder at
        // the end.
        let objects_holder = CpdfIndirectObjectHolder::new();
        let decoder = retain_object(CpdfName::new(None, b"LZW"));
        let decoder_number = objects_holder.borrow_mut().add_indirect_object(decoder);

        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfName::new(None, b"RunLengthDecode"));
        decoders.append_new(CpdfName::new(None, b"ASCII85Decode"));
        decoders.append_new(CpdfName::new(None, b"FlateDecode"));
        decoders.append_new(CpdfReference::new(&objects_holder, decoder_number));
        decoders.append_new(CpdfName::new(None, b"DCTDecode"));
        assert!(validate_decoder_pipeline(&decoders));
    }
    {
        // Invalid 2 decoder pipeline due to wrong type indirect object.
        let objects_holder = CpdfIndirectObjectHolder::new();
        let decoder = retain_object(CpdfString::from_bytes(None, b"FlateDecode"));
        let decoder_number = objects_holder.borrow_mut().add_indirect_object(decoder);

        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfReference::new(&objects_holder, decoder_number));
        decoders.append_new(CpdfName::new(None, b"LZW"));
        assert!(!validate_decoder_pipeline(&decoders));
    }
    {
        // Invalid 2 decoder pipeline due to invalid indirect object.
        let objects_holder = CpdfIndirectObjectHolder::new();
        let decoder = retain_object(CpdfName::new(None, b"DCTDecode"));
        let decoder_number = objects_holder.borrow_mut().add_indirect_object(decoder);

        let mut decoders = CpdfArray::new();
        decoders.append_new(CpdfReference::new(&objects_holder, decoder_number));
        decoders.append_new(CpdfName::new(None, b"LZW"));
        assert!(!validate_decoder_pipeline(&decoders));
    }
}

#[test]
fn GetDecoderArray() {
    {
        // Treat no filter as an empty filter array.
        let dict = CpdfDictionary::new();
        let decoder_array = get_decoder_array(&dict);
        assert!(decoder_array.is_some());
        let decoder_array = decoder_array.unwrap();
        assert!(decoder_array.is_empty());
    }
    {
        // Wrong filter type.
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Filter", CpdfString::from_bytes(None, b"RL"));
        let decoder_array = get_decoder_array(&dict);
        assert!(decoder_array.is_none());
    }
    {
        // Filter name.
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Filter", CpdfName::new(None, b"RL"));
        let decoder_array = get_decoder_array(&dict);
        assert!(decoder_array.is_some());
        let decoder_array = decoder_array.unwrap();
        assert_eq!(1usize, decoder_array.len());
        assert_eq!(b"RL", decoder_array[0].0.as_slice());
    }
    {
        // Empty filter array.
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Filter", CpdfArray::new());
        let decoder_array = get_decoder_array(&dict);
        assert!(decoder_array.is_some());
        let decoder_array = decoder_array.unwrap();
        assert!(decoder_array.is_empty());
    }
    {
        // Valid 1 element filter array.
        let mut dict = CpdfDictionary::new();
        let filter_array_object = dict.set_new_for(b"Filter", CpdfArray::new());
        let mut filter_array_borrow = filter_array_object.borrow_mut();
        let filter_array = filter_array_borrow
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap();
        filter_array.append_new(CpdfName::new(None, b"FooBar"));
        drop(filter_array_borrow);
        let decoder_array = get_decoder_array(&dict);
        assert!(decoder_array.is_some());
        let decoder_array = decoder_array.unwrap();
        assert_eq!(1usize, decoder_array.len());
        assert_eq!(b"FooBar", decoder_array[0].0.as_slice());
    }
    {
        // Valid 2 element filter array.
        let mut dict = CpdfDictionary::new();
        let filter_array_object = dict.set_new_for(b"Filter", CpdfArray::new());
        let mut filter_array_borrow = filter_array_object.borrow_mut();
        let filter_array = filter_array_borrow
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap();
        filter_array.append_new(CpdfName::new(None, b"AHx"));
        filter_array.append_new(CpdfName::new(None, b"LZWDecode"));
        drop(filter_array_borrow);
        let decoder_array = get_decoder_array(&dict);
        assert!(decoder_array.is_some());
        let decoder_array = decoder_array.unwrap();
        assert_eq!(2usize, decoder_array.len());
        assert_eq!(b"AHx", decoder_array[0].0.as_slice());
        assert_eq!(b"LZWDecode", decoder_array[1].0.as_slice());
    }
    {
        // Invalid 2 element filter array.
        let mut dict = CpdfDictionary::new();
        let invalid_filter_array_object = dict.set_new_for(b"Filter", CpdfArray::new());
        let mut invalid_filter_array_borrow = invalid_filter_array_object.borrow_mut();
        let invalid_filter_array = invalid_filter_array_borrow
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap();
        invalid_filter_array.append_new(CpdfName::new(None, b"DCTDecode"));
        invalid_filter_array.append_new(CpdfName::new(None, b"CCITTFaxDecode"));
        drop(invalid_filter_array_borrow);
        let decoder_array = get_decoder_array(&dict);
        assert!(decoder_array.is_none());
    }
}

#[test]
fn A85Decode() {
    let kTestData: &[(&[u8], &[u8], u32)] = &[
        (b"", b"", 0),
        (b"~>", b"", 0),
        (b"FCfN8~>", b"test", 7),
        (b"FCfN8~>FCfN8", b"test", 7),
        (b"\t F C\r\n \tf N 8 ~>", b"test", 17),
        (b"@3B0)DJj_BF*)>@Gp#-s", b"a funny story :)", 20),
        (b"12A", b"2k", 3),
        (b"FCfN8FCfN8vw", b"testtest", 11),
    ];
    for &(input, expected, processed_size) in kTestData {
        let result = a85_decode(input);
        assert_eq!(processed_size, result.bytes_consumed, "for case {input:?}");
        assert_eq!(result.data, expected, "for case {input:?}");
    }
}

#[test]
fn HexDecode() {
    let kTestData: &[(&[u8], &[u8], u32)] = &[
        (b"", b"", 0),
        (b">", b"", 1),
        (b"\t   \r\n>", b"", 7),
        (b"12Ac>zzz", b"\x12\xac", 5),
        (b"12 Ac\t02\r\nBF>zzz>", b"\x12\xac\x02\xbf", 13),
        (b"12A>zzz", b"\x12\xa0", 4),
        (b"12tk  \tAc>zzz", b"\x12\xac", 10),
        (b"12AcED3c3456", b"\x12\xac\xed\x3c\x34\x56", 12),
    ];
    for &(input, expected, processed_size) in kTestData {
        let result = hex_decode(input);
        assert_eq!(processed_size, result.bytes_consumed, "for case {input:?}");
        assert_eq!(result.data, expected, "for case {input:?}");
    }
}

#[test]
fn DecodeText() {
    crate::test_support::assert_wide_cstr_eq(&[], &pdf_decode_text(&[]));
    crate::test_support::assert_wide_cstr_eq(
        &[
            0x0074, 0x0068, 0x0065, 0x0020, 0x0071, 0x0075, 0x0069, 0x0063, 0x006b, 0x0009, 0x0066,
            0x006f, 0x0078,
        ],
        &pdf_decode_text(&[
            0x74, 0x68, 0x65, 0x20, 0x71, 0x75, 0x69, 0x63, 0x6b, 0x09, 0x66, 0x6f, 0x78,
        ]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x0330, 0x0331],
        &pdf_decode_text(&[0xef, 0xbb, 0xbf, 0xcc, 0xb0, 0xcc, 0xb1]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x0330, 0x0331],
        &pdf_decode_text(&[0xfe, 0xff, 0x03, 0x30, 0x03, 0x31]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[
            0x7f51, 0x9875, 0x0020, 0x56fe, 0x7247, 0x0020, 0x8d44, 0x8baf, 0x66f4, 0x591a, 0x0020,
            0x00bb,
        ],
        &pdf_decode_text(&[
            0xfe, 0xff, 0x7f, 0x51, 0x98, 0x75, 0x00, 0x20, 0x56, 0xfe, 0x72, 0x47, 0x00, 0x20,
            0x8d, 0x44, 0x8b, 0xaf, 0x66, 0xf4, 0x59, 0x1a, 0x00, 0x20, 0x00, 0xbb,
        ]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x1f3a8],
        &pdf_decode_text(&[0xef, 0xbb, 0xbf, 0xf0, 0x9f, 0x8e, 0xa8]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x1f3a8],
        &pdf_decode_text(&[0xfe, 0xff, 0xd8, 0x3c, 0xdf, 0xa8]),
    );
}

#[test]
fn DecodeTextWithUnicodeEscapes() {
    crate::test_support::assert_wide_cstr_eq(
        &[0x0020, 0x5370, 0x5237],
        &pdf_decode_text(&[
            0xef, 0xbb, 0xbf, 0x1b, 0x6a, 0x61, 0x1b, 0x20, 0xe5, 0x8d, 0xb0, 0xe5, 0x88, 0xb7,
        ]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x0020, 0x5370, 0x5237],
        &pdf_decode_text(&[
            0xfe, 0xff, 0x00, 0x1b, 0x6a, 0x61, 0x00, 0x1b, 0x00, 0x20, 0x53, 0x70, 0x52, 0x37,
        ]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x0020, 0x5370, 0x5237],
        &pdf_decode_text(&[
            0xfe, 0xff, 0x00, 0x1b, 0x6a, 0x61, 0x00, 0x1b, 0x00, 0x20, 0x53, 0x70, 0x52, 0x37,
            0x29,
        ]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x0020, 0x5370, 0x5237],
        &pdf_decode_text(&[
            0xfe, 0xff, 0x00, 0x1b, 0x6a, 0x61, 0x4a, 0x50, 0x00, 0x1b, 0x00, 0x20, 0x53, 0x70,
            0x52, 0x37,
        ]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x0020, 0x5237],
        &pdf_decode_text(&[
            0xfe, 0xff, 0x00, 0x20, 0x00, 0x1b, 0x6a, 0x61, 0x4a, 0x50, 0x00, 0x1b, 0x52, 0x37,
        ]),
    );
}

#[test]
fn DecodeTextWithInvalidUnicodeEscapes() {
    crate::test_support::assert_wide_cstr_eq(
        &[],
        &pdf_decode_text(&[0xef, 0xbb, 0xbf, 0x1b, 0x1b]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[],
        &pdf_decode_text(&[0xfe, 0xff, 0x00, 0x1b, 0x00, 0x1b]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[],
        &pdf_decode_text(&[0xfe, 0xff, 0x00, 0x1b, 0x00, 0x1b, 0x20]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x0020],
        &pdf_decode_text(&[0xef, 0xbb, 0xbf, 0x1b, 0x1b, 0x20]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x0020],
        &pdf_decode_text(&[0xfe, 0xff, 0x00, 0x1b, 0x00, 0x1b, 0x00, 0x20]),
    );
}

#[test]
fn DecodeTextWithUnpairedSurrogates() {
    crate::test_support::assert_wide_cstr_eq(
        &[0xd800],
        &pdf_decode_text(&[0xfe, 0xff, 0xd8, 0x00]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0xdc00],
        &pdf_decode_text(&[0xfe, 0xff, 0xdc, 0x00]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0xd800, 0x1f3a8],
        &pdf_decode_text(&[0xfe, 0xff, 0xd8, 0x00, 0xd8, 0x3c, 0xdf, 0xa8]),
    );
    crate::test_support::assert_wide_cstr_eq(
        &[0x1f3a8, 0xdc00],
        &pdf_decode_text(&[0xfe, 0xff, 0xd8, 0x3c, 0xdf, 0xa8, 0xdc, 0x00]),
    );
}

#[test]
fn EncodeText() {
    assert_eq!(&[] as &[u8], pdf_encode_text(&[]).as_slice());
    assert_eq!(
        &[0x74, 0x68, 0x65, 0x20, 0x71, 0x75, 0x69, 0x63, 0x6b, 0x09, 0x66, 0x6f, 0x78],
        pdf_encode_text(&[
            0x0074, 0x0068, 0x0065, 0x0020, 0x0071, 0x0075, 0x0069, 0x0063, 0x006b, 0x0009, 0x0066,
            0x006f, 0x0078
        ])
        .as_slice()
    );
    assert_eq!(
        &[0xfe, 0xff, 0x03, 0x30, 0x03, 0x31],
        pdf_encode_text(&[0x0330, 0x0331]).as_slice()
    );
    assert_eq!(
        &[
            0xfe, 0xff, 0x7f, 0x51, 0x98, 0x75, 0x00, 0x20, 0x56, 0xfe, 0x72, 0x47, 0x00, 0x20,
            0x8d, 0x44, 0x8b, 0xaf, 0x66, 0xf4, 0x59, 0x1a, 0x00, 0x20, 0x00, 0xbb
        ],
        pdf_encode_text(&[
            0x7f51, 0x9875, 0x0020, 0x56fe, 0x7247, 0x0020, 0x8d44, 0x8baf, 0x66f4, 0x591a, 0x0020,
            0x00bb
        ])
        .as_slice()
    );
    assert_eq!(
        &[0xfe, 0xff, 0xd8, 0x3c, 0xdf, 0xa8],
        pdf_encode_text(&[0x1f3a8]).as_slice()
    );
}

#[test]
fn RoundTripText() {
    for pdf_code_point in 0..256 {
        let original = [pdf_code_point as u8];
        let reencoded = pdf_encode_text(&pdf_decode_text(&original));
        match pdf_code_point {
            0x7f | 0x9f | 0xad => {
                assert_eq!([0u8], reencoded.as_slice(), "PDFDocEncoding undefined")
            }
            _ => assert_eq!(
                original,
                reencoded.as_slice(),
                "PDFDocEncoding: {pdf_code_point}"
            ),
        }
    }
}
