// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fxcodec/flate/flatemodule_unittest.cpp`: Decode and Encode. All source
//! vectors are retained. Encode uses user-authorized decoded-content equivalence
//! because pure Rust compression can produce different valid DEFLATE streams.
#![allow(non_snake_case)]

use super::flate_or_lzw_decode;

#[test]
fn Decode() {
    let flate_decode_cases: &[(&[u8], &[u8], u32)] = &[
        (b"", b"", 0),
        (b"preposterous nonsense", b"", 2),
        (b"x\x9c\x03\x00\x00\x00\x00\x01", b"", 8),
        (b"x\x9cS\x00\x00\x00!\x00!", b" ", 9),
        (b"x\x9c342\x06\x00\x01-\x00\x97", b"123", 11),
        (b"x\x9cc\xf8\x0f\x00\x01\x01\x01\x00", b"\x00\xff", 10),
        (b"x\x9c3T0\x00B]C\x05#K\x05s3c\x85\xe4\x5c.\x90\x80\xa9\xa9\xa9\x82\xb9\xb1\xa9BQ*W\xb8B\x1eW!\x92\xa0\x89\x9e\xb1\xa5\x09\x92\x84\x9e\x85\x81\x81%\xd8\x14$&\xd0\x18C\x05\x10\x0crW\x800\x8a\xd2\xb9\xf4\xdd\x0d\x14\xd2\x8b\xc1F\x99Y\x1a+X\x1a\x9a\x83\x8cI\xe3\x0a\x04B\x007L\x1bB", b"1 0 0 -1 29 763 cm\x0a0 0 555 735 re\x0aW n\x0aq\x0a0 0 555 734.394 re\x0aW n\x0aq\x0a0.8009 0 0 0.8009 0 0 cm\x0a1 1 1 RG 1 1 1 rg\x0a/G0 gs\x0a0 0 693 917 re\x0af\x0aQ\x0aQ\x0a", 96),
    ];
    for (i, &(input, expected, processed_size)) in flate_decode_cases.iter().enumerate() {
        let result = flate_or_lzw_decode(false, input, false, 0, 0, 0, 0, 0);
        assert_eq!(processed_size, result.bytes_consumed, "for case {i}");
        assert_eq!(result.data, expected, "for case {i}");
    }
}

#[test]
fn Encode() {
    let flate_encode_cases: &[(&[u8], &[u8])] = &[
        (b"", b"\x78\x9c\x03\x00\x00\x00\x00\x01"),
        (b" ", b"\x78\x9c\x53\x00\x00\x00\x21\x00\x21"),
        (b"123", b"\x78\x9c\x33\x34\x32\x06\x00\x01\x2d\x00\x97"),
        (b"\x00\xff", b"\x78\x9c\x63\xf8\x0f\x00\x01\x01\x01\x00"),
        (b"1 0 0 -1 29 763 cm\n0 0 555 735 re\nW n\nq\n0 0 555 734.394 re\nW n\nq\n0.8009 0 0 0.8009 0 0 cm\n1 1 1 RG 1 1 1 rg\n/G0 gs\n0 0 693 917 re\nf\nQ\nQ\n",
         b"\x78\x9c\x4d\x8c\x3b\x0e\x80\x20\x10\x05\xfb\x3d\xc5\xbb\x80\xb8\xc8\xcf\x3d\x01\x35\x36\x1e\x80\x28\x15\x26\xe2\xfd\x13\x83\x36\xe4\x35\x93\x79\xc9\x68\x30\x18\x93\xc6\x22\x08\xde\x20\x57\xea\xc2\x39\x87\x60\x1c\xda\x41\x3b\x2e\xba\x07\x69\x95\x11\x3b\x1c\x6a\x65\x96\xaf\x32\x60\xae\xa4\xd1\xb7\x45\xfc\xd0\x0a\xcd\x91\x51\x9e\x2f\xe5\xc5\x40\x74\xe8\x99\x93\x12\x25\x7a\x01\x37\x4c\x1b\x42"),
    ];
    for (i, &(input, native_expected)) in flate_encode_cases.iter().enumerate() {
        let result = super::encode(input);
        // Check both the Rust result and each complete native reference vector.
        // Compressed byte identity is not the contract at this codec boundary.
        for encoded in [result.as_slice(), native_expected] {
            let decoded = flate_or_lzw_decode(false, encoded, false, 0, 0, 0, 0, 0);
            assert_eq!(decoded.data, input, "for case {i}");
            assert_eq!(decoded.bytes_consumed as usize, encoded.len());
        }
    }
}
