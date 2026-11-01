// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by the BSD-style license in ../LICENSE-PDFIUM.
//! Source-derived segment/GRD continuation and replace-rectangle checks.
use super::*;
struct Words(Vec<Vec<u32>>);
impl Decoder for Words {
    fn push_packed_row(&mut self, words: &[u32], _: u32) -> bool {
        self.0.push(words.to_vec());
        true
    }
    fn push_pixel(&mut self, _: bool) {
        panic!("packed output expected");
    }
    fn push_pixel_chunk(&mut self, _: bool, _: u32) {
        panic!("packed output expected");
    }
    fn next_line(&mut self) {}
}
fn segment(output: &mut Vec<u8>, number: u32, kind: u8, payload: &[u8], unknown: bool) {
    output.extend_from_slice(&number.to_be_bytes());
    output.extend_from_slice(&[kind, 0, 1]);
    output.extend_from_slice(
        &(if unknown {
            u32::MAX
        } else {
            payload.len() as u32
        })
        .to_be_bytes(),
    );
    output.extend_from_slice(payload);
}
fn page(width: u32, height: u32, background: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&[background << 2, 0, 0]);
    bytes
}
fn region(template: u8, tpgdon: bool, custom: bool, operator: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&37u32.to_be_bytes());
    bytes.extend_from_slice(&5u32.to_be_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&[operator, (template << 1) | u8::from(tpgdon) << 3]);
    let at = if custom {
        [0xff, 0xff, 0xfc, 0xff, 1, 0xfe, 0xfd, 0xfe]
    } else {
        [3, 0xff, 0xfd, 0xff, 2, 0xfe, 0xfe, 0xfe]
    };
    if template == 0 {
        bytes.extend_from_slice(&at);
    } else {
        bytes.extend_from_slice(&[
            if custom {
                0xff
            } else if template == 1 {
                3
            } else {
                2
            },
            0xff,
        ]);
    }
    bytes.extend_from_slice(&[0x55; 256]);
    bytes
}
#[test]
fn generic_rows_resume_all_templates_prediction_and_custom_adaptive_pixels() {
    for template in 0..4 {
        for tpgdon in [false, true] {
            for custom in [false, true] {
                let mut source = Vec::new();
                segment(&mut source, 1, 48, &page(37, 5, 0), false);
                segment(
                    &mut source,
                    2,
                    38,
                    &region(template, tpgdon, custom, 2),
                    false,
                );
                // A second XOR region detects replay/recomposition of completed rows.
                segment(
                    &mut source,
                    3,
                    38,
                    &region(template, tpgdon, custom, 2),
                    false,
                );
                segment(&mut source, 4, 49, &[], false);
                let image = Image::new_embedded_with_canvas(&source, None, 37, 5).unwrap();
                let mut expected = Words(Vec::new());
                image
                    .decode_with_document_context(
                        &mut expected,
                        &mut PdfiumDocumentContext::default(),
                        0,
                    )
                    .unwrap();
                let mut state = PdfiumProgressiveDecoder::new(37, 5).unwrap();
                let mut rows = 0;
                let mut segments = 0;
                let mut document = PdfiumDocumentContext::default();
                loop {
                    match state.advance(&source, None, &mut document, 0, 1).unwrap() {
                        PdfiumDecodeBoundary::GenericRows { first, end } => {
                            assert_eq!(end, first + 1);
                            assert_eq!(state.completed_generic_rows(), Some(end));
                            rows += 1;
                        }
                        PdfiumDecodeBoundary::Segment => segments += 1,
                        PdfiumDecodeBoundary::PageReady => {}
                        PdfiumDecodeBoundary::Finished { default_pixel } => {
                            assert!(!default_pixel);
                            break;
                        }
                    }
                }
                assert_eq!(
                    rows, 10,
                    "template {template}, prediction {tpgdon}, custom {custom}"
                );
                assert_eq!(segments, 3);
                let mut actual = Words(Vec::new());
                state.emit(&mut actual).unwrap();
                assert_eq!(actual.0, expected.0);
                assert_eq!(
                    state.advance(&source, None, &mut document, 0, 1).unwrap(),
                    PdfiumDecodeBoundary::Finished {
                        default_pixel: false
                    }
                );
            }
        }
    }
}
#[test]
fn unknown_length_cursor_uses_consumed_arithmetic_offset_not_declared_tail() {
    let payload = region(0, false, false, 0);
    // Native aligns and advances two arithmetic marker bytes plus four bytes
    // following an unknown-length segment. Derive that actual decoder offset.
    let mut reader = Reader::new_pdfium(&payload);
    let header = generic::parse(&mut reader, true).unwrap();
    let (_, consumed, _) = generic::decode(&header, &mut ScratchBuffers::default()).unwrap();
    let end = reader.byte_pos() + consumed;
    let mut source = Vec::new();
    segment(&mut source, 1, 48, &page(37, 5, 0), false);
    let mut encoded = payload[..end].to_vec();
    encoded.extend_from_slice(&[0; 4]);
    segment(&mut source, 2, 38, &encoded, true);
    segment(&mut source, 3, 48, &page(37, 5, 1), false);
    segment(&mut source, 4, 49, &[], false);
    let mut state = PdfiumProgressiveDecoder::new(37, 5).unwrap();
    let mut document = PdfiumDocumentContext::default();
    loop {
        if let PdfiumDecodeBoundary::Finished { default_pixel } =
            state.advance(&source, None, &mut document, 0, 1).unwrap()
        {
            assert!(default_pixel);
            break;
        }
    }
    let mut actual = Words(Vec::new());
    state.emit(&mut actual).unwrap();
    assert!(actual.0.iter().flatten().all(|word| *word == u32::MAX));
}
#[test]
fn malformed_segment_is_sticky_after_successful_page_prefix() {
    let mut source = Vec::new();
    segment(&mut source, 1, 48, &page(37, 5, 0), false);
    segment(&mut source, 2, 38, &[], false);
    let mut state = PdfiumProgressiveDecoder::new(37, 5).unwrap();
    let mut document = PdfiumDocumentContext::default();
    assert_eq!(
        state.advance(&source, None, &mut document, 0, 1).unwrap(),
        PdfiumDecodeBoundary::PageReady
    );
    assert_eq!(
        state.advance(&source, None, &mut document, 0, 1).unwrap(),
        PdfiumDecodeBoundary::Segment
    );
    let error = state
        .advance(&source, None, &mut document, 0, 1)
        .unwrap_err();
    assert_eq!(
        state.advance(&source, None, &mut document, 0, 1),
        Err(error)
    );
    assert!(state.emit(&mut Words(Vec::new())).is_err());
}
