// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Complete native `core/fxcrt/fx_bidi_unittest.cpp` declarations.
//! Segment cursors use checked slice indices in place of C++ iterators; native
//! inputs, intermediate segment assertions and end-boundary checks are retained.
#![allow(non_snake_case)]
use super::fx_bidi::{CfxBidiChar, CfxBidiString, Direction};
const NEUTRAL: u32 = 32;
const LEFT: u32 = 65;
const RIGHT: u32 = 1488;
const LEFT_WEAK: u32 = 46;

#[test]
fn BidiCharEmpty() {
    let mut bidi = CfxBidiChar::new();
    let info;

    info = *bidi.get_segment_info();
    assert_eq!(Direction::Neutral, info.direction);
    assert_eq!(0, info.start);
    assert_eq!(0, info.count);
    assert!(!bidi.end_char());
}

#[test]
fn BidiCharLeft() {
    let mut bidi = CfxBidiChar::new();
    let mut info;

    assert!(bidi.append_char(LEFT));
    info = *bidi.get_segment_info();
    assert_eq!(0, info.start);
    assert_eq!(0, info.count);

    assert!(!bidi.append_char(LEFT));
    assert!(!bidi.append_char(LEFT));

    info = *bidi.get_segment_info();
    assert_eq!(Direction::Neutral, info.direction);
    assert_eq!(0, info.start);
    assert_eq!(0, info.count);

    assert!(bidi.end_char());
    info = *bidi.get_segment_info();
    assert_eq!(Direction::Left, info.direction);
    assert_eq!(0, info.start);
    assert_eq!(3, info.count);

    assert!(!bidi.end_char());
}

#[test]
fn BidiCharLeftNeutralRight() {
    let mut bidi = CfxBidiChar::new();
    let mut info;

    assert!(bidi.append_char(LEFT));
    info = *bidi.get_segment_info();
    assert_eq!(0, info.start);
    assert_eq!(0, info.count);

    assert!(!bidi.append_char(LEFT));
    assert!(!bidi.append_char(LEFT));
    assert!(bidi.append_char(NEUTRAL));
    info = *bidi.get_segment_info();
    assert_eq!(0, info.start);
    assert_eq!(3, info.count);

    assert!(!bidi.append_char(NEUTRAL));
    assert!(!bidi.append_char(NEUTRAL));
    assert!(!bidi.append_char(NEUTRAL));
    assert!(bidi.append_char(RIGHT));
    info = *bidi.get_segment_info();
    assert_eq!(Direction::Neutral, info.direction);
    assert_eq!(3, info.start);
    assert_eq!(4, info.count);

    assert!(bidi.end_char());
    info = *bidi.get_segment_info();
    assert_eq!(Direction::Right, info.direction);
    assert_eq!(7, info.start);
    assert_eq!(1, info.count);

    assert!(!bidi.end_char());
}

#[test]
fn BidiCharLeftLeftWeakRight() {
    let mut bidi = CfxBidiChar::new();
    let mut info;

    assert!(bidi.append_char(LEFT));
    info = *bidi.get_segment_info();
    assert_eq!(0, info.start);
    assert_eq!(0, info.count);

    assert!(!bidi.append_char(LEFT));
    assert!(!bidi.append_char(LEFT));
    assert!(bidi.append_char(LEFT_WEAK));
    info = *bidi.get_segment_info();
    assert_eq!(0, info.start);
    assert_eq!(3, info.count);

    assert!(!bidi.append_char(LEFT_WEAK));
    assert!(!bidi.append_char(LEFT_WEAK));
    assert!(!bidi.append_char(LEFT_WEAK));
    assert!(bidi.append_char(RIGHT));
    info = *bidi.get_segment_info();
    assert_eq!(Direction::LeftWeak, info.direction);
    assert_eq!(3, info.start);
    assert_eq!(4, info.count);

    assert!(bidi.end_char());
    info = *bidi.get_segment_info();
    assert_eq!(Direction::Right, info.direction);
    assert_eq!(7, info.start);
    assert_eq!(1, info.count);

    assert!(!bidi.end_char());
}

#[test]
fn BidiCharLeftRightLeft() {
    let mut bidi = CfxBidiChar::new();
    let mut info;

    assert!(bidi.append_char(LEFT));
    info = *bidi.get_segment_info();
    assert_eq!(0, info.start);
    assert_eq!(0, info.count);

    assert!(!bidi.append_char(LEFT));
    assert!(!bidi.append_char(LEFT));
    assert!(bidi.append_char(RIGHT));
    info = *bidi.get_segment_info();
    assert_eq!(0, info.start);
    assert_eq!(3, info.count);

    assert!(!bidi.append_char(RIGHT));
    assert!(!bidi.append_char(RIGHT));
    assert!(!bidi.append_char(RIGHT));
    assert!(bidi.append_char(LEFT));
    info = *bidi.get_segment_info();
    assert_eq!(Direction::Right, info.direction);
    assert_eq!(3, info.start);
    assert_eq!(4, info.count);

    assert!(bidi.end_char());
    info = *bidi.get_segment_info();
    assert_eq!(Direction::Left, info.direction);
    assert_eq!(7, info.start);
    assert_eq!(1, info.count);

    assert!(!bidi.end_char());
}

#[test]
fn BidiStringEmpty() {
    let bidi = CfxBidiString::new(&[], true);
    assert_eq!(Direction::Left, bidi.overall_direction());
    assert!(bidi.segments().is_empty());
}

#[test]
fn BidiStringAllNeutral() {
    {
        let text: &[u32] = &[NEUTRAL];
        let bidi = CfxBidiString::new(text, true);
        assert_eq!(Direction::Left, bidi.overall_direction());

        let mut index = 0usize;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(1, bidi.segments()[index].count);
        assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
        index += 1;
        assert_eq!(index, bidi.segments().len());
    }
    {
        let text: &[u32] = &[NEUTRAL, NEUTRAL, NEUTRAL];
        let bidi = CfxBidiString::new(text, true);
        assert_eq!(Direction::Left, bidi.overall_direction());

        let mut index = 0usize;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(3, bidi.segments()[index].count);
        assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
        index += 1;
        assert_eq!(index, bidi.segments().len());
    }
}

#[test]
fn BidiStringAllLeft() {
    {
        let text: &[u32] = &[LEFT];
        let bidi = CfxBidiString::new(text, true);
        assert_eq!(Direction::Left, bidi.overall_direction());

        let mut index = 0usize;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(0, bidi.segments()[index].count);
        assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
        assert!(index < bidi.segments().len());

        index += 1;
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(1, bidi.segments()[index].count);
        assert_eq!(Direction::Left, bidi.segments()[index].direction);
        assert!(index < bidi.segments().len());

        index += 1;
        assert_eq!(index, bidi.segments().len());
    }
    {
        let text: &[u32] = &[LEFT, LEFT, LEFT];
        let bidi = CfxBidiString::new(text, true);
        assert_eq!(Direction::Left, bidi.overall_direction());

        let mut index = 0usize;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(0, bidi.segments()[index].count);
        assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
        assert!(index < bidi.segments().len());

        index += 1;
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(3, bidi.segments()[index].count);
        assert_eq!(Direction::Left, bidi.segments()[index].direction);
        assert!(index < bidi.segments().len());

        index += 1;
        assert_eq!(index, bidi.segments().len());
    }
}

#[test]
fn BidiStringAllLeftWeak() {
    {
        let text: &[u32] = &[LEFT_WEAK];
        let bidi = CfxBidiString::new(text, true);
        assert_eq!(Direction::Left, bidi.overall_direction());

        let mut index = 0usize;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(0, bidi.segments()[index].count);
        assert_eq!(Direction::Neutral, bidi.segments()[index].direction);

        index += 1;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(1, bidi.segments()[index].count);
        assert_eq!(Direction::LeftWeak, bidi.segments()[index].direction);

        index += 1;
        assert_eq!(index, bidi.segments().len());
    }
    {
        let text: &[u32] = &[LEFT_WEAK, LEFT_WEAK, LEFT_WEAK];
        let bidi = CfxBidiString::new(text, true);
        assert_eq!(Direction::Left, bidi.overall_direction());

        let mut index = 0usize;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(0, bidi.segments()[index].count);
        assert_eq!(Direction::Neutral, bidi.segments()[index].direction);

        index += 1;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(3, bidi.segments()[index].count);
        assert_eq!(Direction::LeftWeak, bidi.segments()[index].direction);

        index += 1;
        assert_eq!(index, bidi.segments().len());
    }
}

#[test]
fn BidiStringAllRight() {
    {
        let text: &[u32] = &[RIGHT];
        let bidi = CfxBidiString::new(text, true);
        assert_eq!(Direction::Right, bidi.overall_direction());

        let mut index = 0usize;
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(1, bidi.segments()[index].count);
        assert_eq!(Direction::Right, bidi.segments()[index].direction);
        assert!(index < bidi.segments().len());

        index += 1;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(0, bidi.segments()[index].count);
        assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
        assert!(index < bidi.segments().len());

        index += 1;
        assert_eq!(index, bidi.segments().len());
    }
    {
        let text: &[u32] = &[RIGHT, RIGHT, RIGHT];
        let bidi = CfxBidiString::new(text, true);
        assert_eq!(Direction::Right, bidi.overall_direction());

        let mut index = 0usize;
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(3, bidi.segments()[index].count);
        assert_eq!(Direction::Right, bidi.segments()[index].direction);
        assert!(index < bidi.segments().len());

        index += 1;
        assert!(index < bidi.segments().len());
        assert_eq!(0, bidi.segments()[index].start);
        assert_eq!(0, bidi.segments()[index].count);
        assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
        assert!(index < bidi.segments().len());

        index += 1;
        assert_eq!(index, bidi.segments().len());
    }
}

#[test]
fn BidiStringLeftNeutralLeftRight() {
    let text: &[u32] = &[LEFT, NEUTRAL, LEFT, RIGHT];
    let bidi = CfxBidiString::new(text, true);
    assert_eq!(Direction::Left, bidi.overall_direction());

    let mut index = 0usize;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(0, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Left, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert_eq!(1, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert_eq!(2, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Left, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert_eq!(3, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Right, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert_eq!(index, bidi.segments().len());
}

#[test]
fn BidiStringRightNeutralLeftRight() {
    let text: &[u32] = &[RIGHT, NEUTRAL, LEFT, RIGHT];
    let bidi = CfxBidiString::new(text, true);
    assert_eq!(Direction::Right, bidi.overall_direction());

    let mut index = 0usize;
    assert_eq!(3, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Right, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert_eq!(2, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Left, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert_eq!(1, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Right, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(0, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);
    assert!(index < bidi.segments().len());

    index += 1;
    assert_eq!(index, bidi.segments().len());
}

#[test]
fn BidiStringRightLeftWeakLeftRight() {
    let text: &[u32] = &[RIGHT, LEFT_WEAK, LEFT, RIGHT];
    let bidi = CfxBidiString::new(text, true);
    assert_eq!(Direction::Right, bidi.overall_direction());

    let mut index = 0usize;
    assert!(index < bidi.segments().len());
    assert_eq!(3, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Right, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(2, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Left, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(1, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::LeftWeak, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Right, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(0, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);

    index += 1;
    assert_eq!(index, bidi.segments().len());
}

#[test]
fn BidiStringReverse() {
    let text: &[u32] = &[LEFT, NEUTRAL, RIGHT, LEFT_WEAK, LEFT];
    let mut bidi = CfxBidiString::new(text, true);
    assert_eq!(Direction::Left, bidi.overall_direction());
    bidi.set_overall_direction_right();

    let mut index = 0usize;
    assert!(index < bidi.segments().len());
    assert_eq!(4, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Left, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(3, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::LeftWeak, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(2, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Right, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(1, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Left, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(0, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);

    index += 1;
    assert_eq!(index, bidi.segments().len());
}

#[test]
fn BidiStringLeftRightEqual() {
    let text: &[u32] = &[LEFT, RIGHT];
    let bidi = CfxBidiString::new(text, true);
    assert_eq!(Direction::Left, bidi.overall_direction());

    let mut index = 0usize;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(0, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Left, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(1, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Right, bidi.segments()[index].direction);

    index += 1;
    assert_eq!(index, bidi.segments().len());
}

#[test]
fn BidiStringNoAutoOrder() {
    // Line with 2 RTL and 1 LTR segments.
    let text: &[u32] = &[RIGHT, RIGHT, NEUTRAL, LEFT, NEUTRAL, RIGHT];
    let bidi = CfxBidiString::new(text, false);
    assert_eq!(Direction::Left, bidi.overall_direction());

    let mut index = 0usize;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(0, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(0, bidi.segments()[index].start);
    assert_eq!(2, bidi.segments()[index].count);
    assert_eq!(Direction::Right, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(2, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(3, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Left, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(4, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Neutral, bidi.segments()[index].direction);

    index += 1;
    assert!(index < bidi.segments().len());
    assert_eq!(5, bidi.segments()[index].start);
    assert_eq!(1, bidi.segments()[index].count);
    assert_eq!(Direction::Right, bidi.segments()[index].direction);

    index += 1;
    assert_eq!(index, bidi.segments().len());
}
