// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Selected `core/fpdfdoc/cpvt_variabletext_unittest.cpp` cases.
#![allow(non_snake_case)]
use super::upstream_test_support::{wide, LayoutFixture};
use super::*;

#[test]
fn LTRTextLayout() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    vt.set_plate_rect(CfxFloatRect::new(0., 0., 100., 100.));
    vt.set_font_size(10.0);
    vt.settings.multi_line = false;
    vt.settings.limit_width = false;
    vt.initialize();

    // "hello" in English (Left-to-Right layout)
    vt.set_text(&wide("hello"));
    vt.rearrange_all();

    let mut it = vt.iterator();
    it.set_at_index(1); // Set to first word index (Place(0, 0, 0))
    let mut word: CpvtWord;
    let next_word = it.word();
    assert!(next_word.is_some());
    word = next_word.unwrap();
    assert_eq!(b'h' as u16, word.word);
    crate::test_support::assert_float_eq(0.1f32, word.caret_x());
    let first_x = word.location.x;

    assert!(it.next_word());
    let next_word = it.word();
    assert!(next_word.is_some());
    word = next_word.unwrap();
    assert_eq!(b'e' as u16, word.word);
    crate::test_support::assert_float_eq(0.2f32, word.caret_x());
    let second_x = word.location.x;

    assert!(it.next_word());
    let next_word = it.word();
    assert!(next_word.is_some());
    word = next_word.unwrap();
    assert_eq!(b'l' as u16, word.word);
    crate::test_support::assert_float_eq(0.3f32, word.caret_x());
    let third_x = word.location.x;

    assert!(it.next_word());
    let next_word = it.word();
    assert!(next_word.is_some());
    word = next_word.unwrap();
    assert_eq!(b'l' as u16, word.word);
    crate::test_support::assert_float_eq(0.4f32, word.caret_x());
    let fourth_x = word.location.x;

    assert!(it.next_word());
    let next_word = it.word();
    assert!(next_word.is_some());
    word = next_word.unwrap();
    assert_eq!(b'o' as u16, word.word);
    crate::test_support::assert_float_eq(0.5f32, word.caret_x());
    let fifth_x = word.location.x;

    assert!(!it.next_word());

    // In LTR, coordinates go left-to-right (0, 10, 20, 30, 40).
    assert!(first_x < second_x);
    assert!(second_x < third_x);
    assert!(third_x < fourth_x);
    assert!(fourth_x < fifth_x);
}

#[test]
fn RTLTextLayout() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    vt.set_plate_rect(CfxFloatRect::new(0., 0., 100., 100.));
    vt.set_font_size(10.0);
    vt.settings.multi_line = false;
    vt.settings.limit_width = false;
    vt.initialize();

    // Shalom in Hebrew (U+05E9, U+05DC, U+05D5, U+05DD)
    // Logically ordered but should be displayed from right to left.
    vt.set_text(&wide("\u{05E9}\u{05DC}\u{05D5}\u{05DD}"));
    vt.rearrange_all();

    let mut it = vt.iterator();
    it.set_at_index(1); // Set to first word index (Place(0, 0, 0))
    let mut word: CpvtWord;
    let next_word = it.word();
    assert!(next_word.is_some());
    word = next_word.unwrap();
    assert_eq!(0x05E9, word.word);
    crate::test_support::assert_float_eq(0.3f32, word.caret_x());
    let first_x = word.location.x;

    assert!(it.next_word());
    let next_word = it.word();
    assert!(next_word.is_some());
    word = next_word.unwrap();
    assert_eq!(0x05DC, word.word);
    crate::test_support::assert_float_eq(0.2f32, word.caret_x());
    let second_x = word.location.x;

    assert!(it.next_word());
    let next_word = it.word();
    assert!(next_word.is_some());
    word = next_word.unwrap();
    assert_eq!(0x05D5, word.word);
    crate::test_support::assert_float_eq(0.1f32, word.caret_x());
    let third_x = word.location.x;

    assert!(it.next_word());
    let next_word = it.word();
    assert!(next_word.is_some());
    word = next_word.unwrap();
    assert_eq!(0x05DD, word.word);
    crate::test_support::assert_float_eq(0.0f32, word.caret_x());
    let fourth_x = word.location.x;

    assert!(!it.next_word());

    // The first logical character has the largest X coordinate.
    assert!(first_x > second_x);
    assert!(second_x > third_x);
    assert!(third_x > fourth_x);
}

#[test]
fn GetLineCaretX() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    vt.set_plate_rect(CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
    vt.set_font_size(10.0);
    vt.set_multi_line(false);
    vt.set_auto_return(false);
    vt.initialize();
    let first_word_place = CpvtWordPlace::new(0, 0, -1);

    // Empty text, left alignment.
    vt.set_alignment(Alignment::Left);
    vt.set_text(&[]);
    vt.rearrange_all();
    {
        let mut it = vt.iterator();
        it.set_at(first_word_place);
        let line = it.line();
        assert!(line.is_some());
        crate::test_support::assert_float_eq(0.0, it.line_caret_x(&line.unwrap()));
        assert_eq!(first_word_place, it.word_place());
    }

    // Empty text, center alignment.
    vt.set_alignment(Alignment::Center);
    vt.rearrange_all();
    {
        let mut it = vt.iterator();
        it.set_at(first_word_place);
        let line = it.line();
        assert!(line.is_some());
        crate::test_support::assert_float_eq(50.0, it.line_caret_x(&line.unwrap()));
        assert_eq!(first_word_place, it.word_place());
    }

    // Empty text, right alignment.
    vt.set_alignment(Alignment::Right);
    vt.rearrange_all();
    {
        let mut it = vt.iterator();
        it.set_at(first_word_place);
        let line = it.line();
        assert!(line.is_some());
        crate::test_support::assert_float_eq(100.0, it.line_caret_x(&line.unwrap()));
        assert_eq!(first_word_place, it.word_place());
    }

    // LTR text starts at the left caret.
    vt.set_alignment(Alignment::Left);
    vt.set_text(&wide("hello"));
    vt.rearrange_all();
    {
        let mut it = vt.iterator();
        it.set_at(first_word_place);
        let line = it.line();
        assert!(line.is_some());
        crate::test_support::assert_float_eq(0.0, it.line_caret_x(&line.unwrap()));
        assert_eq!(first_word_place, it.word_place());
    }

    // RTL text starts at the right caret.
    vt.set_text(&wide("\u{05E9}\u{05DC}\u{05D5}\u{05DD}"));
    vt.rearrange_all();
    let mut it = vt.iterator();
    it.set_at(first_word_place);
    let line = it.line();
    assert!(line.is_some());
    crate::test_support::assert_float_eq(0.4, it.line_caret_x(&line.unwrap()));
    assert_eq!(first_word_place, it.word_place());
}
