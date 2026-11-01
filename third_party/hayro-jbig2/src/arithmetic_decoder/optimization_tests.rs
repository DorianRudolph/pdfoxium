// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in ../../LICENSE-PDFIUM.

//! Arithmetic optimization regressions against the branchy equations in pinned
//! CJBig2_ArithDecoder, with an independent table and single-bit renormalization.

use alloc::{vec, vec::Vec};

use super::{ArithmeticDecoder, ArithmeticDecoderContext};
use crate::error::{DecodeError, ParseError};

// Table C.2 / jbig2_arith_decoder.cpp. Keep this test oracle independent of the
// production table's representation, packing and transition implementation.
const REFERENCE_QE: [(u32, u8, u8, bool); 47] = [
    (0x5601, 1, 1, true),
    (0x3401, 2, 6, false),
    (0x1801, 3, 9, false),
    (0x0ac1, 4, 12, false),
    (0x0521, 5, 29, false),
    (0x0221, 38, 33, false),
    (0x5601, 7, 6, true),
    (0x5401, 8, 14, false),
    (0x4801, 9, 14, false),
    (0x3801, 10, 14, false),
    (0x3001, 11, 17, false),
    (0x2401, 12, 18, false),
    (0x1c01, 13, 20, false),
    (0x1601, 29, 21, false),
    (0x5601, 15, 14, true),
    (0x5401, 16, 14, false),
    (0x5101, 17, 15, false),
    (0x4801, 18, 16, false),
    (0x3801, 19, 17, false),
    (0x3401, 20, 18, false),
    (0x3001, 21, 19, false),
    (0x2801, 22, 19, false),
    (0x2401, 23, 20, false),
    (0x2201, 24, 21, false),
    (0x1c01, 25, 22, false),
    (0x1801, 26, 23, false),
    (0x1601, 27, 24, false),
    (0x1401, 28, 25, false),
    (0x1201, 29, 26, false),
    (0x1101, 30, 27, false),
    (0x0ac1, 31, 28, false),
    (0x09c1, 32, 29, false),
    (0x08a1, 33, 30, false),
    (0x0521, 34, 31, false),
    (0x0441, 35, 32, false),
    (0x02a1, 36, 33, false),
    (0x0221, 37, 34, false),
    (0x0141, 38, 35, false),
    (0x0111, 39, 36, false),
    (0x0085, 40, 37, false),
    (0x0049, 41, 38, false),
    (0x0025, 42, 39, false),
    (0x0015, 43, 40, false),
    (0x0009, 44, 41, false),
    (0x0005, 45, 42, false),
    (0x0001, 45, 43, false),
    (0x5601, 46, 46, false),
];

#[derive(Clone, Copy)]
struct ReferenceContext {
    index: u8,
    mps: u32,
}

struct ReferenceDecoder<'a> {
    data: &'a [u8],
    pdfium: bool,
    completed_rows: u32,
    c: u32,
    a: u32,
    position: u32,
    count: u32,
}

impl<'a> ReferenceDecoder<'a> {
    fn new(data: &'a [u8], pdfium: bool) -> Self {
        let mut decoder = Self {
            data,
            pdfium,
            completed_rows: 0,
            c: (u32::from(data.first().copied().unwrap_or(0xff)) ^ 0xff) << 16,
            a: 0,
            position: 0,
            count: 0,
        };
        decoder.byte_in();
        decoder.c <<= 7;
        decoder.count -= 7;
        decoder.a = 0x8000;
        decoder
    }

    fn byte_at(&self, position: u32) -> u32 {
        u32::from(self.data.get(position as usize).copied().unwrap_or(0xff))
    }

    fn byte_in(&mut self) {
        if self.byte_at(self.position) == 0xff {
            let next = self.byte_at(self.position + 1);
            if next > 0x8f {
                self.count = 8;
            } else {
                self.position += 1;
                self.c = self.c.wrapping_add(0xfe00).wrapping_sub(next << 9);
                self.count = 7;
            }
        } else {
            self.position += 1;
            self.c = self
                .c
                .wrapping_add(0xff00)
                .wrapping_sub(self.byte_at(self.position) << 8);
            self.count = 8;
        }
    }

    fn renormalize(&mut self) {
        // Native ReadValueA shifts once per iteration. The production decoder
        // may batch these shifts, but must preserve the exact register state.
        loop {
            if self.count == 0 {
                self.byte_in();
            }
            self.a <<= 1;
            self.c <<= 1;
            self.count -= 1;
            if self.a & 0x8000 != 0 {
                break;
            }
        }
    }

    fn nlps(context: &mut ReferenceContext, next: u8, switch: bool) -> u32 {
        let bit = 1 - context.mps;
        if switch {
            context.mps = 1 - context.mps;
        }
        context.index = next;
        bit
    }

    fn nmps(context: &mut ReferenceContext, next: u8) -> u32 {
        context.index = next;
        context.mps
    }

    fn read(&mut self, context: &mut ReferenceContext) -> Result<u32, DecodeError> {
        // Preserve the existing adapter's checked boundary, including failure
        // before state/context mutation. Permissive reads extend with FF bytes.
        if self.pdfium && self.position as usize >= self.data.len() {
            return Err(DecodeError::Parse(ParseError::UnexpectedEof));
        }
        let (qe, nmps, nlps, switch) = REFERENCE_QE[context.index as usize];
        self.a -= qe;
        if self.c >> 16 < self.a {
            if self.a & 0x8000 != 0 {
                return Ok(context.mps);
            }
            let bit = if self.a < qe {
                Self::nlps(context, nlps, switch)
            } else {
                Self::nmps(context, nmps)
            };
            self.renormalize();
            Ok(bit)
        } else {
            self.c -= self.a << 16;
            let bit = if self.a < qe {
                Self::nmps(context, nmps)
            } else {
                Self::nlps(context, nlps, switch)
            };
            self.a = qe;
            self.renormalize();
            Ok(bit)
        }
    }
}

fn context(index: u8, mps: u32) -> ArithmeticDecoderContext {
    let mut context = ArithmeticDecoderContext::default();
    context.set_index(index);
    context.xor_mps(mps);
    context
}

fn registers(decoder: &ArithmeticDecoder<'_>) -> (u32, bool, u32, u32, u32, u32) {
    (
        decoder.completed_rows,
        decoder.pdfium,
        decoder.c,
        decoder.a,
        decoder.base_pointer,
        decoder.shift_count,
    )
}

fn assert_state(actual: &ArithmeticDecoder<'_>, expected: &ReferenceDecoder<'_>) {
    assert_eq!(
        registers(actual),
        (
            expected.completed_rows,
            expected.pdfium,
            expected.c,
            expected.a,
            expected.position,
            expected.count,
        )
    );
}

fn assert_read(
    actual: &mut ArithmeticDecoder<'_>,
    context: &mut ArithmeticDecoderContext,
    expected: &mut ReferenceDecoder<'_>,
    reference_context: &mut ReferenceContext,
) -> bool {
    let result = actual.read_bitmap_bit(context);
    assert_eq!(result, expected.read(reference_context));
    assert_eq!(
        (context.index(), context.mps()),
        (u32::from(reference_context.index), reference_context.mps)
    );
    assert_state(actual, expected);
    result.is_ok()
}

fn streams() -> Vec<Vec<u8>> {
    let mut streams = vec![
        vec![],
        vec![0],
        vec![0xff],
        vec![0xff, 0],
        vec![0xff, 0x8f],
        vec![0xff, 0x90],
        vec![0xff, 0xac],
        vec![0xff, 0xff],
        vec![0xfe, 0xff, 0],
        vec![0, 0xff, 0x8f, 0xff, 0x90, 0xff, 0xac],
        vec![0; 96],
        vec![0xff; 96],
        vec![0xaa; 96],
        vec![0x55; 96],
    ];
    for seed in 1u32..=8 {
        let mut state = seed;
        let mut data = Vec::new();
        for _ in 0..96 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            data.push((state >> 24) as u8);
        }
        streams.push(data);
    }
    // Prefixes exercise initialization and refills at every nearby EOF offset.
    let data = streams.last().unwrap().clone();
    for length in 2..=20 {
        streams.push(data[..length].to_vec());
    }
    streams
}

#[test]
fn all_context_states_and_modes_match_native_equations_through_refills() {
    for data in streams() {
        for pdfium in [false, true] {
            for index in 0..47 {
                for mps in 0..=1 {
                    let mut actual = ArithmeticDecoder::new_with_pdfium(&data, pdfium);
                    let mut expected = ReferenceDecoder::new(&data, pdfium);
                    let mut ctx = context(index, mps);
                    let mut reference_context = ReferenceContext { index, mps };
                    assert_state(&actual, &expected);
                    for _ in 0..256 {
                        if !assert_read(
                            &mut actual,
                            &mut ctx,
                            &mut expected,
                            &mut reference_context,
                        ) {
                            let before = registers(&actual);
                            let before_context = (ctx.index(), ctx.mps());
                            assert!(!assert_read(
                                &mut actual,
                                &mut ctx,
                                &mut expected,
                                &mut reference_context
                            ));
                            assert_eq!(registers(&actual), before);
                            assert_eq!((ctx.index(), ctx.mps()), before_context);
                            break;
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn interval_boundaries_and_renormalization_match_bitwise_reference() {
    let data = [0x31, 0xff, 0x8f, 0xff, 0x90, 0, 0xff, 0xac, 0x55];
    for index in 0..47 {
        let qe = REFERENCE_QE[index as usize].0;
        for mps in 0..=1 {
            for a in [0x8000, 0x8001, 0xc000, 0xffff] {
                let reduced = a - qe;
                for high in [0, reduced.saturating_sub(1), reduced, a - 1, 0xffff] {
                    for low in [0, 0xffff] {
                        for count in [0, 1, 7, 8] {
                            for position in [0, 1, 2, 3, 4, 7, 8] {
                                let mut actual = ArithmeticDecoder::new(&data);
                                actual.a = a;
                                actual.c = (high << 16) | low;
                                actual.shift_count = count;
                                actual.base_pointer = position;
                                let mut expected = ReferenceDecoder::new(&data, false);
                                expected.a = a;
                                expected.c = (high << 16) | low;
                                expected.count = count;
                                expected.position = position;
                                let mut ctx = context(index, mps);
                                let mut reference_context = ReferenceContext { index, mps };
                                assert!(assert_read(
                                    &mut actual,
                                    &mut ctx,
                                    &mut expected,
                                    &mut reference_context
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn save_resume_keeps_checked_boundary_and_shared_context_bank() {
    for data in streams() {
        for pdfium in [false, true] {
            let mut actual = ArithmeticDecoder::new_with_pdfium(&data, pdfium);
            let mut expected = ReferenceDecoder::new(&data, pdfium);
            let mut contexts: Vec<_> = (0..16)
                .map(|i| context((i * 11 % 47) as u8, i & 1))
                .collect();
            let mut reference_contexts: Vec<_> = (0..16)
                .map(|i| ReferenceContext {
                    index: (i * 11 % 47) as u8,
                    mps: i & 1,
                })
                .collect();
            for step in 0..512 {
                let label = ((step * 13) ^ (step >> 3)) & 15;
                actual.completed_rows = step as u32 / 29;
                expected.completed_rows = actual.completed_rows;
                actual = ArithmeticDecoder::resume(&data, actual.save());
                assert_state(&actual, &expected);
                if !assert_read(
                    &mut actual,
                    &mut contexts[label],
                    &mut expected,
                    &mut reference_contexts[label],
                ) {
                    let saved = actual.save();
                    actual = ArithmeticDecoder::resume(&data, saved);
                    assert!(!assert_read(
                        &mut actual,
                        &mut contexts[label],
                        &mut expected,
                        &mut reference_contexts[label]
                    ));
                    break;
                }
            }
        }
    }
}

// Available only when this cfg(test) child module is compiled. Generic-loop
// regressions need explicit low-Qe states to exercise long white runs.
impl ArithmeticDecoderContext {
    pub(crate) fn optimization_test_context(index: u8, mps: u32) -> Self {
        context(index, mps)
    }
}

#[test]
fn zero_byte_shortcut_matches_eight_checked_reads_or_changes_nothing() {
    let mut successes = 0;
    for data in [
        vec![],
        vec![0],
        vec![0xff],
        vec![0xff, 0x90],
        vec![0x55; 32],
    ] {
        for pdfium in [false, true] {
            for index in 0..47 {
                let qe = REFERENCE_QE[index as usize].0;
                for mps in 0..=1 {
                    let mut intervals = vec![0x8000, 0x8001, 0xc000, 0xffff];
                    for a in [qe * 8 + 0x7fff, qe * 8 + 0x8000] {
                        if a <= 0xffff {
                            intervals.push(a);
                        }
                    }
                    for a in intervals {
                        let threshold = a.saturating_sub(qe * 8);
                        for high in [0, threshold.saturating_sub(1), threshold, 0xffff] {
                            for position in [0, data.len() as u32, data.len() as u32 + 1] {
                                let mut actual = ArithmeticDecoder::new_with_pdfium(&data, pdfium);
                                actual.a = a;
                                actual.c = (high << 16) | 0xa55a;
                                actual.shift_count = 0;
                                actual.base_pointer = position;
                                actual.completed_rows = 7;
                                let ctx = context(index, mps);
                                let before = actual.save();
                                if actual.try_read_zero_bitmap_byte(ctx) {
                                    successes += 1;
                                    let mut scalar = ArithmeticDecoder::resume(&data, before);
                                    let mut scalar_context = ctx;
                                    for _ in 0..8 {
                                        assert_eq!(
                                            scalar.read_bitmap_bit(&mut scalar_context),
                                            Ok(0)
                                        );
                                    }
                                    assert_eq!(actual.save(), scalar.save());
                                    assert_eq!(
                                        (ctx.index(), ctx.mps()),
                                        (scalar_context.index(), scalar_context.mps())
                                    );
                                } else {
                                    assert_eq!(actual.save(), before);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        successes > 0,
        "low-Qe, zero-MPS intervals must reach the shortcut"
    );
}
