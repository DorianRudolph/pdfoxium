// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/dib/cstretchengine_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::fpdfapi::page::cpdf_dib::CpdfDib;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use std::collections::BTreeMap;

const kTooBigSrcLen: i32 = 20;
const kTooBigDestLen: i32 = 32 * 1024 * 1024 + 1;

#[derive(Default)]
struct ScanlineRecorder {
    rows: BTreeMap<i32, Vec<u8>>,
}
impl ScanlineRecorder {
    fn HasRow(&self, line: i32) -> bool {
        self.rows.contains_key(&line)
    }
    fn RowIsAllZero(&self, line: i32) -> bool {
        self.rows
            .get(&line)
            .expect("recorded row")
            .iter()
            .all(|&byte| byte == 0)
    }
}
impl ScanlineComposerIface for ScanlineRecorder {
    fn compose_scanline(&mut self, line: i32, row: &[u8]) {
        self.rows.insert(line, row.to_vec());
    }
    fn set_info(&mut self, _: i32, _: i32, _: FxdibFormat, _: Vec<u32>) -> bool {
        true
    }
}
fn PixelWeightSum(weights: PixelWeight<'_>) -> u32 {
    weights
        .weights
        .iter()
        .fold(0u32, |sum, weight| sum.wrapping_add(*weight))
}
fn ExecuteOneStretchTest(dest_width: i32, src_width: i32, options: FxdibResampleOptions) {
    let mut table = WeightTable::new();
    assert!(table.calculate_weights(dest_width, 0, dest_width, src_width, 0, src_width, options));
    for i in 0..dest_width {
        assert_eq!(
            FIXED_POINT_ONE,
            PixelWeightSum(table.get_pixel_weight(i)),
            "for {{ {src_width}, {dest_width} }} at {i}"
        );
    }
}
fn ExecuteOneReversedStretchTest(dest_width: i32, src_width: i32, options: FxdibResampleOptions) {
    let mut table = WeightTable::new();
    assert!(table.calculate_weights(-dest_width, 0, dest_width, src_width, 0, src_width, options));
    for i in 0..dest_width {
        assert_eq!(
            FIXED_POINT_ONE,
            PixelWeightSum(table.get_pixel_weight(i)),
            "for {{ {src_width}, {dest_width} }} at {i} (reversed)"
        );
    }
}
fn ExecuteStretchTests(options: FxdibResampleOptions) {
    let dest_widths = [1, 2, 337, 512, 808, 2550];
    let src_widths = [1, 2, 187, 256, 809, 1110];
    for src_width in src_widths {
        for dest_width in dest_widths {
            ExecuteOneStretchTest(dest_width, src_width, options);
            ExecuteOneReversedStretchTest(dest_width, src_width, options);
        }
    }
}

// CpdfDib::load returns None, releasing a failed source. Preserve the native
// failed object's metadata for this constructor-only test, without allocating
// a bitmap or exposing a partly loaded DIB as a production object.
struct FailedDibMetadata {
    width: i32,
    height: i32,
}
impl StretchSource for FailedDibMetadata {
    type Row<'a> = &'a [u8];
    type Palette<'a> = &'a [u32];
    fn width(&self) -> i32 {
        self.width
    }
    fn height(&self) -> i32 {
        self.height
    }
    fn format(&self) -> FxdibFormat {
        FxdibFormat::Invalid
    }
    fn pitch(&self) -> u32 {
        0
    }
    fn palette(&self) -> &[u32] {
        &[]
    }
    fn scanline(&self, _: i32) -> &[u8] {
        unreachable!("constructor-only failed DIB")
    }
}
#[test]
fn OverflowInCtor() {
    let clip_rect = FxRect::default();
    let mut dict_obj = CpdfDictionary::new();
    dict_obj.set_new_for(b"Width", CpdfScalarObject::number_from_signed(71000));
    dict_obj.set_new_for(b"Height", CpdfScalarObject::number_from_signed(12500));
    let metadata = FailedDibMetadata {
        width: dict_obj.get_integer_for(b"Width", 0),
        height: dict_obj.get_integer_for(b"Height", 0),
    };
    let stream = retain_object(CpdfStream::from_data_and_dict(
        Vec::new(),
        retain_object(dict_obj),
    ));
    assert!(CpdfDib::load(stream, None).is_none());
    let engine = CStretchEngine::new(
        Rc::new(metadata),
        FxdibFormat::EightBppRgb,
        500,
        500,
        clip_rect,
        FxdibResampleOptions::default(),
    );
    assert!(engine.resample_options().interpolate_bilinear);
    assert!(!engine.resample_options().halftone);
    assert!(!engine.resample_options().no_smoothing);
    assert!(!engine.resample_options().lossy);
}

#[test]
fn WeightRounding() {
    let options = FxdibResampleOptions::default();
    ExecuteStretchTests(options);
}

#[test]
fn WeightRoundingNoSmoothing() {
    let mut options = FxdibResampleOptions::default();
    options.no_smoothing = true;
    ExecuteStretchTests(options);
}

#[test]
fn WeightRoundingBilinear() {
    let mut options = FxdibResampleOptions::default();
    options.interpolate_bilinear = true;
    ExecuteStretchTests(options);
}

#[test]
fn WeightRoundingNoSmoothingBilinear() {
    let mut options = FxdibResampleOptions::default();
    options.no_smoothing = true;
    options.interpolate_bilinear = true;
    ExecuteStretchTests(options);
}

#[test]
fn ZeroLengthSrc() {
    let options = FxdibResampleOptions::default();
    let mut table = WeightTable::new();
    assert!(table.calculate_weights(100, 0, 100, 0, 0, 0, options));
}

#[test]
fn ZeroLengthSrcNoSmoothing() {
    let mut options = FxdibResampleOptions::default();
    options.no_smoothing = true;
    let mut table = WeightTable::new();
    assert!(table.calculate_weights(100, 0, 100, 0, 0, 0, options));
}

#[test]
fn ZeroLengthSrcBilinear() {
    let mut options = FxdibResampleOptions::default();
    options.interpolate_bilinear = true;
    let mut table = WeightTable::new();
    assert!(table.calculate_weights(100, 0, 100, 0, 0, 0, options));
}

#[test]
fn ZeroLengthSrcNoSmoothingBilinear() {
    let mut options = FxdibResampleOptions::default();
    options.no_smoothing = true;
    options.interpolate_bilinear = true;
    let mut table = WeightTable::new();
    assert!(table.calculate_weights(100, 0, 100, 0, 0, 0, options));
}

// A source clip box that is empty in x leaves `src_max` at 0, so the
// area-average branch clamps a destination pixel's tap position from above
// against `src_max - 1`, i.e. -1. Regression test for storing that negative
// position; the degenerate entry must contribute no weight.
#[test]
fn EmptySourceRangeDownscale() {
    let options = FxdibResampleOptions::default();
    let mut table = WeightTable::new();
    assert!(table.calculate_weights(1, 0, 1, 100, 0, 0, options));
    assert_eq!(0u32, PixelWeightSum(table.get_pixel_weight(0)));
}

// Same degenerate source range, reached through the constructor rather than
// by calling CalculateWeights() directly: a horizontally mirrored
// destination whose clip box maps the source's right edge onto x == 0 leaves
// a source clip of zero width but non-zero height.
#[test]
fn MirroredDestinationEmptySourceClip() {
    let mut source = CfxDibitmap::default();
    assert!(source.create_in_place(100, 50, FxdibFormat::Bgra, 0));
    let mut engine = CStretchEngine::new(
        Rc::new(source),
        FxdibFormat::Bgra,
        -100,
        50,
        FxRect::new(100, 0, 200, 50),
        FxdibResampleOptions::default(),
    );
    assert!(engine.start_stretch_horz());
}

// The mirror image of the case above: here the source clip degenerates at the
// right edge, so `src_min` and `src_max` are both the source width. Clamping
// the tap position up to `src_min` would put it one past the last source
// pixel, which the horizontal pass then reads.
//
// This drives the horizontal pass directly instead of via Continue(), which
// would go on to run StretchVert() and dereference the null
// ScanlineComposerIface*. The tap position is consumed by the horizontal
// pass, so stopping there still covers the case.
#[test]
fn EmptySourceClipAtRightEdge() {
    let mut source = CfxDibitmap::default();
    assert!(source.create_in_place(100, 50, FxdibFormat::Bgra, 0));
    let mut engine = CStretchEngine::new(
        Rc::new(source),
        FxdibFormat::Bgra,
        100,
        50,
        FxRect::new(100, 0, 200, 50),
        FxdibResampleOptions::default(),
    );
    assert!(engine.start_stretch_horz());
    assert!(!engine.continue_stretch_horz(None));
}

// The vertical pass can be handed an empty tap range even when the source clip
// is not degenerate, which is the case the three tests above cannot reach --
// StartStretchHorz() rejects a zero-height source clip, so `src_min ==
// src_max` never survives to StretchVert().
//
// The geometry that does reach it: a vertically mirrored destination
// (`dest_height < 0`) whose clip box straddles the bottom edge of the
// destination. The constructor maps the clip box back to source space and then
// clamps it with `src_clip_.Intersect()`, raising `src_clip_.top` from -25 to
// 0; the per-row tap position that CalculateWeights() computes afterwards is
// not clamped the same way, so the lower destination rows land above the
// clamped clip and get `src_start = src_min`, `src_end = pixel_pos < src_min`
// -- an empty range.
//
// This matters because StretchVert()'s row-wide accumulator is allocated once
// outside the row loop and reused. A row with no taps writes nothing into it,
// so failing to zero it for that row would repeat the last row that did have
// taps. The source here is opaque white, which makes a repeated row and a
// zeroed row tell apart.
#[test]
fn EmptySourceRangeInVerticalPass() {
    let mut source = CfxDibitmap::default();
    assert!(source.create_in_place(100, 50, FxdibFormat::Bgr, 0));
    source.clear(0xffffffff);

    let mut options = FxdibResampleOptions::default();
    options.no_smoothing = true;

    let mut recorder = ScanlineRecorder::default();
    let mut engine = CStretchEngine::new(
        Rc::new(source),
        FxdibFormat::Bgr,
        100,
        -50,
        FxRect::new(0, 25, 100, 75),
        options,
    );
    assert!(engine.start_stretch_horz());
    assert!(!engine.continue_stretch_horz(None));
    engine.stretch_vert(&mut recorder);

    // Destination rows 25..49 tap source rows 24..0 and must be opaque; the
    // clip box runs to 75, and rows 50..74 tap nothing.
    for line in 0..25 {
        assert!(recorder.HasRow(line), "line {line}");
        assert!(!recorder.RowIsAllZero(line), "line {line}");
    }
    for line in 25..50 {
        assert!(recorder.HasRow(line), "line {line}");
        assert!(recorder.RowIsAllZero(line), "line {line}");
    }
}

#[test]
fn ZeroLengthDest() {
    let options = FxdibResampleOptions::default();
    let mut table = WeightTable::new();
    assert!(table.calculate_weights(0, 0, 0, 100, 0, 100, options));
}

#[test]
fn TooManyWeights() {
    let options = FxdibResampleOptions::default();
    let mut table = WeightTable::new();
    assert!(!table.calculate_weights(
        kTooBigDestLen,
        0,
        kTooBigDestLen,
        kTooBigSrcLen,
        0,
        kTooBigSrcLen,
        options
    ));
}
