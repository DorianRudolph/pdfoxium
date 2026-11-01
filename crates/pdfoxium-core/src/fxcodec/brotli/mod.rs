// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Brotli stream decoding from `core/fxcodec/brotli/brotli_decoder.{h,cpp}`.

use std::sync::atomic::{AtomicBool, Ordering};

use brotli_decompressor::{BrotliDecompressStream, BrotliResult, BrotliState, StandardAlloc};

use super::data_and_bytes_consumed::DataAndBytesConsumed;
use super::image_predictors::{apply_predictor, INVALID_OFFSET};

const MAX_DECODE_BYTES: usize = 1024 * 1024 * 1024;
static BROTLI_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_brotli_enabled(enabled: bool) {
    BROTLI_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn get_brotli_enabled() -> bool {
    BROTLI_ENABLED.load(Ordering::Relaxed)
}

fn failed() -> DataAndBytesConsumed {
    DataAndBytesConsumed::new(Vec::new(), INVALID_OFFSET)
}

pub fn decode(
    source: &[u8],
    predictor: i32,
    colors: i32,
    bits_per_component: i32,
    columns: i32,
    estimated_decode_size: u32,
) -> DataAndBytesConsumed {
    assert!(get_brotli_enabled(), "Brotli decoder must be enabled");
    if source.is_empty() {
        return failed();
    }
    let estimated = if estimated_decode_size == 0 {
        source.len()
    } else {
        estimated_decode_size as usize
    };
    if estimated > MAX_DECODE_BYTES {
        return failed();
    }
    if u32::try_from(source.len()).is_err() {
        return failed();
    }
    let mut state = BrotliState::new(
        StandardAlloc::default(),
        StandardAlloc::default(),
        StandardAlloc::default(),
    );
    let mut decoded = vec![0; estimated];
    let mut available_in = source.len();
    let mut input_offset = 0;
    let mut output_offset = 0;
    let mut total_out = 0;
    loop {
        let mut available_out = decoded.len() - output_offset;
        match BrotliDecompressStream(
            &mut available_in,
            &mut input_offset,
            source,
            &mut available_out,
            &mut output_offset,
            &mut decoded,
            &mut total_out,
            &mut state,
        ) {
            BrotliResult::NeedsMoreOutput if decoded.len() < MAX_DECODE_BYTES => {
                decoded.resize(decoded.len().saturating_mul(2).min(MAX_DECODE_BYTES), 0);
            }
            BrotliResult::ResultSuccess => {
                decoded.truncate(total_out);
                return apply_predictor(
                    decoded,
                    predictor,
                    colors,
                    bits_per_component,
                    columns,
                    (source.len() - available_in) as u32,
                );
            }
            _ => return failed(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::fpdf_parser_decode::{a85_decode, pdf_data_decode};

    #[test]
    fn decodes_pdf_brotli_stream_and_reports_consumed_bytes() {
        let encoded = b")c6U.*/CeGjP!eUoW?7+QLZtts4^Yb+d<AVStLHH+JAcF3W0^A\"r7abEOe*FhG.`aHC8C'9p_mb75CJ9\nC3rDErhg/+~>";
        let compressed = a85_decode(encoded).data;
        set_brotli_enabled(true);
        let mut input = compressed.clone();
        input.extend_from_slice(b"trailing PDF data");
        let result = decode(&input, 0, 1, 8, 1, 0);
        let pipeline = pdf_data_decode(
            encoded,
            0,
            false,
            &vec![
                (b"ASCII85Decode".to_vec(), None),
                (b"BrotliDecode".to_vec(), None),
            ],
        )
        .unwrap();
        set_brotli_enabled(false);
        assert_eq!(result.bytes_consumed as usize, compressed.len());
        assert!(result.data.windows(5).any(|window| window == b"Hello"));
        assert_eq!(pipeline.data, result.data);
    }
}
