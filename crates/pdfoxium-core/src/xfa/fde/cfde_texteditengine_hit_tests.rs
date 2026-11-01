// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Analytical hit tests for retained, already-laid-out directional runs.
//! Ten-point cells make the expected logical insertion boundaries independent
//! of font substitution, glyph hinting, and the breaker being exercised.
use super::*;

fn piece(start: i32, count: i32, x: f32, y: f32, rtl: bool) -> FdeTextEditPiece {
    FdeTextEditPiece {
        rect: CfxRectF::new(x, y, count as f32 * 10., 10.),
        start,
        count,
        bidi_level: i32::from(rtl),
        char_styles: if rtl {
            FX_TXTCHARSTYLE_ODD_BIDI_LEVEL
        } else {
            0
        },
    }
}
fn laid_out(text: &str, pieces: Vec<FdeTextEditPiece>) -> CfdeTextEditEngine {
    let mut engine = CfdeTextEditEngine::new();
    engine.insert(
        0,
        &text.chars().map(u32::from).collect::<Vec<_>>(),
        RecordOperation::SkipNotify,
    );
    engine.char_widths = vec![200_000; engine.text_length];
    engine.text_piece_info = pieces;
    engine.is_dirty = false;
    engine
}
fn hit(engine: &mut CfdeTextEditEngine, x: f32, y: f32) -> usize {
    engine.get_index_for_point(CfxPointF::new(x, y))
}

#[test]
fn every_directional_fragment_on_a_visual_row_is_hit_tested() {
    let mut engine = laid_out(
        "ABאבCD",
        vec![
            piece(2, 2, 40., 0., true),
            piece(0, 2, 0., 0., false),
            piece(4, 2, 80., 0., false),
        ],
    );
    for (x, expected) in [
        (2., 0),
        (18., 2),
        (42., 4),
        (48., 3),
        (52., 3),
        (58., 2),
        (82., 4),
        (98., 6),
    ] {
        assert_eq!(hit(&mut engine, x, 5.), expected, "x={x}");
    }
}

#[test]
fn visual_row_extrema_do_not_depend_on_piece_storage_order() {
    let mut engine = laid_out(
        "ABאבCD",
        vec![
            piece(2, 2, 40., 0., true),
            piece(0, 2, 0., 0., false),
            piece(4, 2, 80., 0., false),
        ],
    );
    assert_eq!(hit(&mut engine, -100., 5.), 0);
    assert_eq!(hit(&mut engine, 500., 5.), 6);
}

#[test]
fn glyph_midpoint_ties_use_the_following_visual_half() {
    let mut ltr = laid_out("AB", vec![piece(0, 2, 10., 0., false)]);
    for (x, expected) in [(14.99, 0), (15., 1), (25., 2)] {
        assert_eq!(hit(&mut ltr, x, 5.), expected);
    }
    let mut rtl = laid_out("אב", vec![piece(0, 2, 10., 0., true)]);
    for (x, expected) in [(14.99, 2), (15., 1), (25., 0)] {
        assert_eq!(hit(&mut rtl, x, 5.), expected);
    }
}

#[test]
fn outside_a_right_to_left_run_selects_the_corresponding_logical_edge() {
    let mut engine = laid_out("אב", vec![piece(0, 2, 10., 0., true)]);
    assert_eq!(hit(&mut engine, 5., 5.), 2, "visual left is logical end");
    assert_eq!(
        hit(&mut engine, 35., 5.),
        0,
        "visual right is logical start"
    );
}

#[test]
fn mixed_row_edge_uses_the_direction_of_its_visually_outer_piece() {
    let mut engine = laid_out(
        "ABאב",
        vec![piece(0, 2, 40., 0., false), piece(2, 2, 0., 0., true)],
    );
    assert_eq!(hit(&mut engine, -10., 5.), 4);
    assert_eq!(hit(&mut engine, 100., 5.), 2);
}

#[test]
fn wrapped_row_right_edge_keeps_the_breaking_space_on_its_own_line() {
    let mut engine = laid_out(
        "ab cd",
        vec![piece(0, 3, 0., 0., false), piece(3, 2, 0., 20., false)],
    );
    assert_eq!(hit(&mut engine, 200., 5.), 2);
    assert_eq!(hit(&mut engine, 200., 25.), 5);
    assert_eq!(hit(&mut engine, -10., 25.), 3);
}

#[test]
fn a_crlf_hit_never_splits_the_two_character_line_ending() {
    let mut engine = laid_out(
        "ab\r\ncd",
        vec![piece(0, 4, 0., 0., false), piece(4, 2, 0., 20., false)],
    );
    assert_eq!(hit(&mut engine, 32., 5.), 2, "inside LF snaps to CR");
    assert_eq!(
        hit(&mut engine, 200., 5.),
        2,
        "outside row also snaps to CR"
    );
    assert_eq!(hit(&mut engine, 2., 25.), 4);
}

#[test]
fn row_vertical_intervals_are_half_open_and_gaps_return_text_end() {
    let mut engine = laid_out(
        "ABCD",
        vec![piece(0, 2, 0., 0., false), piece(2, 2, 0., 20., false)],
    );
    for y in [-1., 10., 19.99, 30., 100.] {
        assert_eq!(hit(&mut engine, 2., y), 4, "y={y}");
    }
    assert_eq!(hit(&mut engine, 2., 0.), 0);
    assert_eq!(hit(&mut engine, 2., 20.), 2);
}

#[test]
fn final_line_sentinel_cannot_produce_an_index_past_text_length() {
    let mut engine = laid_out("AB", vec![piece(0, 3, 10., 0., false)]);
    assert_eq!(hit(&mut engine, 200., 5.), 2);
    assert_eq!(hit(&mut engine, 38., 5.), 2);
}
