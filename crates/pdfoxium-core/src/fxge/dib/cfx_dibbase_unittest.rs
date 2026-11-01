// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/dib/cfx_dibbase_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::get_overlap_rect;
use crate::fxcrt::fx_coordinates::{CfxPoint, CfxSize};
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::FxdibFormat;

struct Input {
    src_top_left: CfxPoint,
    src_size: CfxSize,
    dest_top_left: CfxPoint,
    overlap_size: CfxSize,
}
struct Output {
    src_top_left: CfxPoint,
    dest_top_left: CfxPoint,
    overlap_size: CfxSize,
}
fn RunOverlapRectTest(bitmap: &CfxDibitmap, input: &Input, expected_output: Option<&Output>) {
    let mut src_left = input.src_top_left.x;
    let mut src_top = input.src_top_left.y;
    let mut dest_left = input.dest_top_left.x;
    let mut dest_top = input.dest_top_left.y;
    let mut overlap_width = input.overlap_size.width;
    let mut overlap_height = input.overlap_size.height;
    let success = get_overlap_rect(
        bitmap.width(),
        bitmap.height(),
        &mut dest_left,
        &mut dest_top,
        &mut overlap_width,
        &mut overlap_height,
        input.src_size.width,
        input.src_size.height,
        &mut src_left,
        &mut src_top,
        None,
    );
    assert_eq!(success, expected_output.is_some());
    if let Some(expected_output) = expected_output {
        assert_eq!(expected_output.src_top_left.x, src_left);
        assert_eq!(expected_output.src_top_left.y, src_top);
        assert_eq!(expected_output.dest_top_left.x, dest_left);
        assert_eq!(expected_output.dest_top_left.y, dest_top);
        assert_eq!(expected_output.overlap_size.width, overlap_width);
        assert_eq!(expected_output.overlap_size.height, overlap_height);
    }
}

#[test]
fn GetOverlapRectTrivialOverlap() {
    let mut bitmap = CfxDibitmap::default();
    assert!(bitmap.create_in_place(400, 300, FxdibFormat::OneBppRgb, 0));

    let kInput = Input {
        src_top_left: CfxPoint::new(0, 0),
        src_size: CfxSize::new(400, 300),
        dest_top_left: CfxPoint::new(0, 0),
        overlap_size: CfxSize::new(400, 300),
    };
    let kExpectedOutput = Output {
        src_top_left: CfxPoint::new(0, 0),
        dest_top_left: CfxPoint::new(0, 0),
        overlap_size: CfxSize::new(400, 300),
    };
    RunOverlapRectTest(&bitmap, &kInput, Some(&kExpectedOutput));
}

#[test]
fn GetOverlapRectOverlapNoLimit() {
    let mut bitmap = CfxDibitmap::default();
    assert!(bitmap.create_in_place(400, 300, FxdibFormat::OneBppRgb, 0));

    let kInput = Input {
        src_top_left: CfxPoint::new(35, 41),
        src_size: CfxSize::new(400, 300),
        dest_top_left: CfxPoint::new(123, 137),
        overlap_size: CfxSize::new(200, 100),
    };
    let kExpectedOutput = Output {
        src_top_left: CfxPoint::new(35, 41),
        dest_top_left: CfxPoint::new(123, 137),
        overlap_size: CfxSize::new(200, 100),
    };
    RunOverlapRectTest(&bitmap, &kInput, Some(&kExpectedOutput));
}

#[test]
fn GetOverlapRectOverlapLimitedBySource() {
    let mut bitmap = CfxDibitmap::default();
    assert!(bitmap.create_in_place(400, 300, FxdibFormat::OneBppRgb, 0));

    let kInput = Input {
        src_top_left: CfxPoint::new(141, 154),
        src_size: CfxSize::new(400, 300),
        dest_top_left: CfxPoint::new(35, 41),
        overlap_size: CfxSize::new(270, 160),
    };
    let kExpectedOutput = Output {
        src_top_left: CfxPoint::new(141, 154),
        dest_top_left: CfxPoint::new(35, 41),
        overlap_size: CfxSize::new(259, 146),
    };
    RunOverlapRectTest(&bitmap, &kInput, Some(&kExpectedOutput));
}

#[test]
fn GetOverlapRectOverlapLimitedByDestination() {
    let mut bitmap = CfxDibitmap::default();
    assert!(bitmap.create_in_place(400, 300, FxdibFormat::OneBppRgb, 0));

    let kInput = Input {
        src_top_left: CfxPoint::new(35, 41),
        src_size: CfxSize::new(400, 300),
        dest_top_left: CfxPoint::new(123, 137),
        overlap_size: CfxSize::new(280, 170),
    };
    let kExpectedOutput = Output {
        src_top_left: CfxPoint::new(35, 41),
        dest_top_left: CfxPoint::new(123, 137),
        overlap_size: CfxSize::new(277, 163),
    };
    RunOverlapRectTest(&bitmap, &kInput, Some(&kExpectedOutput));
}

#[test]
fn GetOverlapRectBadInputs() {
    let mut bitmap = CfxDibitmap::default();
    assert!(bitmap.create_in_place(400, 300, FxdibFormat::OneBppRgb, 0));

    let kEmptyInputs = [
        Input {
            src_top_left: CfxPoint::new(0, 0),
            src_size: CfxSize::new(0, 0),
            dest_top_left: CfxPoint::new(0, 0),
            overlap_size: CfxSize::new(400, 300),
        },
        Input {
            src_top_left: CfxPoint::new(0, 0),
            src_size: CfxSize::new(400, 300),
            dest_top_left: CfxPoint::new(0, 0),
            overlap_size: CfxSize::new(0, 0),
        },
        Input {
            src_top_left: CfxPoint::new(-400, 0),
            src_size: CfxSize::new(400, 300),
            dest_top_left: CfxPoint::new(0, 0),
            overlap_size: CfxSize::new(400, 300),
        },
    ];
    for input in &kEmptyInputs {
        RunOverlapRectTest(&bitmap, input, None);
    }

    let kOutOfBoundInputs = [
        Input {
            src_top_left: CfxPoint::new(400, 0),
            src_size: CfxSize::new(400, 300),
            dest_top_left: CfxPoint::new(0, 0),
            overlap_size: CfxSize::new(400, 300),
        },
        Input {
            src_top_left: CfxPoint::new(0, 300),
            src_size: CfxSize::new(400, 300),
            dest_top_left: CfxPoint::new(0, 0),
            overlap_size: CfxSize::new(400, 300),
        },
        Input {
            src_top_left: CfxPoint::new(0, -300),
            src_size: CfxSize::new(400, 300),
            dest_top_left: CfxPoint::new(0, 0),
            overlap_size: CfxSize::new(400, 300),
        },
        Input {
            src_top_left: CfxPoint::new(0, 0),
            src_size: CfxSize::new(400, 300),
            dest_top_left: CfxPoint::new(-400, 0),
            overlap_size: CfxSize::new(400, 300),
        },
        Input {
            src_top_left: CfxPoint::new(0, 0),
            src_size: CfxSize::new(400, 300),
            dest_top_left: CfxPoint::new(400, 0),
            overlap_size: CfxSize::new(400, 300),
        },
        Input {
            src_top_left: CfxPoint::new(0, 0),
            src_size: CfxSize::new(400, 300),
            dest_top_left: CfxPoint::new(0, -300),
            overlap_size: CfxSize::new(400, 300),
        },
        Input {
            src_top_left: CfxPoint::new(0, 0),
            src_size: CfxSize::new(400, 300),
            dest_top_left: CfxPoint::new(0, 300),
            overlap_size: CfxSize::new(400, 300),
        },
    ];
    for input in &kOutOfBoundInputs {
        RunOverlapRectTest(&bitmap, input, None);
    }
}
