// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcodec/flate/flatemodule.{h,cpp}` using pure Rust
//! zlib-rs. Deflate streams need not be byte-identical to native zlib; the
//! upstream Encode vectors use the documented decoded-content equivalence.

pub mod lzw;
pub mod scanline;

pub use scanline::FlateScanlineDecoder;

use zlib_rs::{Inflate, InflateFlush, Status};

use crate::fxcodec::data_and_bytes_consumed::DataAndBytesConsumed;
use crate::fxcodec::image_predictors::apply_predictor;

const MAX_TOTAL_OUT_SIZE: usize = 1024 * 1024 * 1024;
const MAX_INITIAL_ALLOC_SIZE: usize = 10_000_000;

struct InflateResult {
    bytes_consumed: usize,
    bytes_written: usize,
    continuing: bool,
}

// Match PDFium's FlateOutput, including bytes produced before an error.
// Callers retain partial output and zero-fill unused scanline bytes.
fn flate_output(state: &mut Inflate, src: &[u8], dest: &mut [u8]) -> InflateResult {
    #[cfg(feature = "render-profile")]
    let _profile = crate::render_profile::Scope::new("Flate inflate");
    let before_in = state.total_in();
    let before_out = state.total_out();
    let status = state.decompress(src, dest, InflateFlush::SyncFlush);
    InflateResult {
        bytes_consumed: (state.total_in() - before_in) as usize,
        bytes_written: (state.total_out() - before_out) as usize,
        continuing: status == Ok(Status::Ok),
    }
}

fn flate_uncompress(src: &[u8], estimated_size: u32) -> DataAndBytesConsumed {
    // PDFium caps the initial allocation and truncates zlib's reported total
    // output at 1 GiB. Grow one chunk at a time without copying prior chunks.
    let chunk_size = (if estimated_size != 0 {
        estimated_size as usize
    } else {
        src.len().saturating_mul(2)
    })
    .min(MAX_INITIAL_ALLOC_SIZE)
    .max(1);
    let mut state = Inflate::new(true, 15);
    let mut output = Vec::new();
    let mut consumed = 0usize;
    loop {
        let available = chunk_size.min(MAX_TOTAL_OUT_SIZE - output.len());
        if available == 0 {
            break;
        }
        let start = output.len();
        output.resize(start + available, 0);
        let result = flate_output(&mut state, &src[consumed..], &mut output[start..]);
        consumed += result.bytes_consumed;
        output.truncate(start + result.bytes_written);
        if !result.continuing
            || result.bytes_written < available
            || (result.bytes_consumed == 0 && result.bytes_written == 0)
        {
            break;
        }
    }
    DataAndBytesConsumed::new(output, consumed.min(u32::MAX as usize) as u32)
}

pub fn flate_or_lzw_decode(
    lzw: bool,
    src: &[u8],
    early_change: bool,
    predictor: i32,
    colors: i32,
    bits_per_component: i32,
    columns: i32,
    estimated_size: u32,
) -> DataAndBytesConsumed {
    let result = if lzw {
        let result = lzw::lzw_decode(src, early_change);
        if result.bytes_consumed == u32::MAX {
            return result;
        }
        result
    } else {
        flate_uncompress(src, estimated_size)
    };
    apply_predictor(
        result.data,
        predictor,
        colors,
        bits_per_component,
        columns,
        result.bytes_consumed,
    )
}

pub fn encode(src: &[u8]) -> Vec<u8> {
    // Preserve FlateModule::Encode's checked output capacity and failure path.
    // zlib's compress() uses its default compression level (6).
    let capacity = src
        .len()
        .checked_add(src.len() / 1000)
        .and_then(|size| size.checked_add(12))
        .expect("Flate encode size overflow");
    let mut output = vec![0; capacity];
    let (compressed, status) =
        zlib_rs::compress_slice(&mut output, src, zlib_rs::DeflateConfig::default());
    let size = if status == zlib_rs::ReturnCode::Ok {
        compressed.len()
    } else {
        0
    };
    output.truncate(size);
    output
}

#[allow(clippy::too_many_arguments)]
pub fn create_decoder<'a>(
    src: &'a [u8],
    width: i32,
    height: i32,
    comps: i32,
    bpc: i32,
    predictor: i32,
    colors: i32,
    bits_per_component: i32,
    columns: i32,
) -> Box<dyn crate::fxcodec::scanlinedecoder::ScanlineDecoder + 'a> {
    Box::new(FlateScanlineDecoder::new(
        src,
        width,
        height,
        comps,
        bpc,
        predictor,
        colors,
        bits_per_component,
        columns,
    ))
}

#[cfg(test)]
mod tests {
    use super::{encode, flate_or_lzw_decode};

    #[test]
    fn decode_drains_pending_dictionary_output_before_reading_more_input() {
        let input: Vec<u8> = (0..100_000).map(|index| (index % 251) as u8).collect();
        let encoded = encode(&input);
        for estimate in [0, 1_000, 32_769, 100_000] {
            let result = flate_or_lzw_decode(false, &encoded, false, 0, 0, 0, 0, estimate);
            assert_eq!(result.data.len(), input.len(), "estimate {estimate}");
            assert_eq!(result.data, input, "estimate {estimate}");
            assert_eq!(result.bytes_consumed, encoded.len() as u32);
        }
    }

    #[test]
    fn source_decode_examples_and_consumption() {
        let cases: &[(&[u8], &[u8], u32)] = &[
            (b"", b"", 0),
            (b"preposterous nonsense", b"", 2),
            (b"\x78\x9c\x03\x00\x00\x00\x00\x01", b"", 8),
            (b"\x78\x9c\x53\x00\x00\x00\x21\x00\x21", b" ", 9),
            (b"\x78\x9c\x33\x34\x32\x06\x00\x01\x2d\x00\x97", b"123", 11),
        ];
        for &(input, expected, consumed) in cases {
            let actual = flate_or_lzw_decode(false, input, false, 0, 0, 0, 0, 0);
            assert_eq!(actual.data, expected);
            assert_eq!(actual.bytes_consumed, consumed);
        }
    }

    #[test]
    fn partial_output_checksum_errors_and_trailing_input() {
        // A final uncompressed DEFLATE block containing "abc", followed by
        // its Adler-32. zlib retains produced bytes on truncation/error and
        // stops at the end of the first stream, ignoring trailing input.
        let encoded = b"\x78\x01\x01\x03\x00\xfc\xffabc\x02\x4d\x01\x27";
        for end in 7..=encoded.len() {
            let expected = &b"abc"[..(end - 7).min(3)];
            for estimate in [0, 1, 3, 100] {
                let actual = super::flate_uncompress(&encoded[..end], estimate);
                assert_eq!(actual.data, expected, "end {end}, estimate {estimate}");
                assert_eq!(actual.bytes_consumed, end as u32);
            }
        }
        let mut corrupt = encoded.to_vec();
        *corrupt.last_mut().unwrap() ^= 1;
        let actual = super::flate_uncompress(&corrupt, 0);
        assert_eq!(actual.data, b"abc");
        assert_eq!(actual.bytes_consumed, encoded.len() as u32);

        let mut trailing = encoded.to_vec();
        trailing.extend_from_slice(b"trailing input");
        for estimate in [0, 1, 3, 100] {
            let actual = super::flate_uncompress(&trailing, estimate);
            assert_eq!(actual.data, b"abc");
            assert_eq!(actual.bytes_consumed, encoded.len() as u32);
        }
    }

    #[test]
    fn zlib_round_trip_and_predictor_dispatch() {
        assert_eq!(encode(b""), b"\x78\x9c\x03\x00\x00\x00\x00\x01");
        assert_eq!(encode(b" "), b"\x78\x9c\x53\x00\x00\x00\x21\x00\x21");
        let input = b"stream data with repeated repeated repeated bytes";
        let encoded = encode(input);
        let decoded = flate_or_lzw_decode(false, &encoded, false, 0, 0, 0, 0, 1);
        assert_eq!(decoded.data, input);
        assert_eq!(decoded.bytes_consumed, encoded.len() as u32);

        let predicted = encode(&[1, 1, 1, 1]);
        let decoded = flate_or_lzw_decode(false, &predicted, false, 12, 1, 8, 3, 0);
        assert_eq!(decoded.data, [1, 2, 3]);
    }
}

#[cfg(test)]
#[path = "flatemodule_unittest.rs"]
mod upstream_tests;
