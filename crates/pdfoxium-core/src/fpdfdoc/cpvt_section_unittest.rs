// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfdoc/cpvt_section_unittest.cpp` cases. Hosted under
//! variabletext so its borrowed Layout replaces the native section backpointer.
#![allow(non_snake_case, non_upper_case_globals)]
use super::upstream_test_support::LayoutFixture;
use super::*;
use crate::fpdfdoc::cpvt_wordrange::CpvtWordRange;
const kHebrewAlef: u16 = 0x05d0;
const kHebrewBet: u16 = 0x05d1;
const kHebrewGimel: u16 = 0x05d2;
fn PopulateSectionWithText(section: &mut CpvtSection, text: &[u32]) {
    section.sec_place = CpvtWordPlace::new(0, 0, -1);
    for i in 0..text.len() {
        section.add_word(
            CpvtWordPlace::new(0, 0, i as i32),
            CpvtWordInfo::new(text[i] as u16, FxCharset::Default, 0),
        );
    }
}
fn PopulateSectionWithEnglishAndHebrew(section: &mut CpvtSection) {
    PopulateSectionWithText(
        section,
        &[
            b'A' as u32,
            b' ' as u32,
            b'B' as u32,
            b' ' as u32,
            b'C' as u32,
            b' ' as u32,
            kHebrewAlef.into(),
            b' ' as u32,
            kHebrewBet.into(),
        ],
    );
}
fn PopulateSectionWithHebrewAndEnglish(section: &mut CpvtSection) {
    PopulateSectionWithText(
        section,
        &[
            kHebrewAlef.into(),
            b' ' as u32,
            kHebrewBet.into(),
            b' ' as u32,
            kHebrewGimel.into(),
            b' ' as u32,
            b'A' as u32,
            b' ' as u32,
            b'B' as u32,
        ],
    );
}
fn SetVariableTextDefaults(vt: &mut CpvtVariableText<'_>) {
    vt.set_font_size(10.0);
    vt.set_plate_rect(CfxFloatRect::new(0., 0., 1000., 1000.));
}
fn PopulateSectionWithHello(section: &mut CpvtSection) {
    section.sec_place = CpvtWordPlace::new(0, 0, -1);
    section.add_word(
        CpvtWordPlace::new(0, 0, 0),
        CpvtWordInfo::new(b'h' as u16, FxCharset::Default, 0),
    );
    section.add_word(
        CpvtWordPlace::new(0, 0, 1),
        CpvtWordInfo::new(b'e' as u16, FxCharset::Default, 0),
    );
    section.add_word(
        CpvtWordPlace::new(0, 0, 2),
        CpvtWordInfo::new(b'l' as u16, FxCharset::Default, 0),
    );
    section.add_word(
        CpvtWordPlace::new(0, 0, 3),
        CpvtWordInfo::new(b'l' as u16, FxCharset::Default, 0),
    );
    section.add_word(
        CpvtWordPlace::new(0, 0, 4),
        CpvtWordInfo::new(b'o' as u16, FxCharset::Default, 0),
    );
    let lineinfo = CpvtLineInfo {
        begin_word_index: 0,
        end_word_index: 4,
        ..CpvtLineInfo::default()
    };
    section.add_line(lineinfo);
    section.reset_line_place();
}
fn Rearrange(section: &mut CpvtSection, vt: &CpvtVariableText<'_>) -> CpvtFloatRect {
    section.rearrange(&Layout {
        settings: &vt.settings,
        provider: vt.provider,
    })
}
#[test]
fn ClearLeftWords() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    vt.initialize();
    let mut section = CpvtSection::default();
    PopulateSectionWithHello(&mut section);
    section.clear_words(CpvtWordRange::new(
        CpvtWordPlace::new(-1, 0, -1),
        CpvtWordPlace::new(0, 0, 2),
    ));
    assert_eq!(section.word_array.len(), 2);
    let word0 = section.word_array.first().expect("first retained word");
    assert_eq!(word0.word, b'l' as u16);
    let word1 = section.word_array.get(1).expect("second retained word");
    assert_eq!(word1.word, b'o' as u16);
}
#[test]
fn ClearRightWords() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    vt.initialize();
    let mut section = CpvtSection::default();
    PopulateSectionWithHello(&mut section);
    section.clear_words(CpvtWordRange::new(
        CpvtWordPlace::new(0, 0, 1),
        CpvtWordPlace::new(1, 0, -1),
    ));
    assert_eq!(section.word_array.len(), 2);
    let word0 = section.word_array.first().expect("first retained word");
    assert_eq!(word0.word, b'h' as u16);
    let word1 = section.word_array.get(1).expect("second retained word");
    assert_eq!(word1.word, b'e' as u16);
}
#[test]
fn ClearMidWords() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    vt.initialize();
    let mut section = CpvtSection::default();
    PopulateSectionWithHello(&mut section);
    section.clear_words(CpvtWordRange::new(
        CpvtWordPlace::new(0, 0, 0),
        CpvtWordPlace::new(0, 0, 3),
    ));
    assert_eq!(section.word_array.len(), 2);
    let word0 = section.word_array.first().expect("first retained word");
    assert_eq!(word0.word, b'h' as u16);
    let word1 = section.word_array.get(1).expect("second retained word");
    assert_eq!(word1.word, b'o' as u16);
}
#[test]
fn ClearAllWords() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    vt.initialize();
    let mut section = CpvtSection::default();
    PopulateSectionWithHello(&mut section);
    let range = CpvtWordRange::new(CpvtWordPlace::new(-1, 0, -1), CpvtWordPlace::new(1, 0, -1));
    section.clear_words(range);
    assert_eq!(0usize, section.word_array.len());
}
#[test]
fn OutputLines() {
    #[derive(Clone, Copy)]
    enum TextContent {
        EnglishAndHebrew,
        HebrewAndEnglish,
    }
    const kEnglishAndHebrewLtr: &[usize] = &[0, 1, 2, 3, 4, 5, 8, 7, 6];
    const kEnglishAndHebrewRtl: &[usize] = &[8, 7, 6, 5, 0, 1, 2, 3, 4];
    const kHebrewAndEnglishRtl: &[usize] = &[6, 7, 8, 5, 4, 3, 2, 1, 0];
    const kHebrewAndEnglishLtr: &[usize] = &[4, 3, 2, 1, 0, 5, 6, 7, 8];
    use ParagraphDirection::*;
    use TextContent::*;
    let cases = [
        (EnglishAndHebrew, Auto, kEnglishAndHebrewLtr),
        (EnglishAndHebrew, LeftToRight, kEnglishAndHebrewLtr),
        (EnglishAndHebrew, RightToLeft, kEnglishAndHebrewRtl),
        (HebrewAndEnglish, Auto, kHebrewAndEnglishRtl),
        (HebrewAndEnglish, LeftToRight, kHebrewAndEnglishLtr),
        (HebrewAndEnglish, RightToLeft, kHebrewAndEnglishRtl),
    ];
    for data in cases {
        let fixture = LayoutFixture::new();
        let mut vt = CpvtVariableText::new(Some(&fixture.provider));
        SetVariableTextDefaults(&mut vt);
        vt.settings.text_direction = data.1;
        vt.initialize();
        let mut section = CpvtSection::default();
        match data.0 {
            EnglishAndHebrew => PopulateSectionWithEnglishAndHebrew(&mut section),
            HebrewAndEnglish => PopulateSectionWithHebrewAndEnglish(&mut section),
        }
        Rearrange(&mut section, &vt);
        assert_eq!(data.2.len() as i32, section.word_array.len() as i32);
        for i in 0..data.2.len() - 1 {
            assert!(
                section.word_array[data.2[i]].word_x < section.word_array[data.2[i + 1]].word_x
            );
        }
    }
}
#[test]
fn OutputLines_Multiline_EnglishAndHebrew() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    vt.set_font_size(10.0);
    vt.set_plate_rect(CfxFloatRect::new(0., 0., 0.45, 1000.));
    vt.settings.limit_width = true; // Native SetAutoReturn(true).
    vt.initialize();
    let mut section = CpvtSection::default();
    PopulateSectionWithEnglishAndHebrew(&mut section);
    Rearrange(&mut section, &vt);
    // Both wrapped lines retain the paragraph's LTR context.
    for i in 0..3 {
        assert!(section.word_array[i].word_x < section.word_array[i + 1].word_x);
    }
    for i in 4..7 {
        assert!(section.word_array[i].word_x < section.word_array[i + 1].word_x);
    }
}
#[test]
fn OutputLines_EmptySection() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    SetVariableTextDefaults(&mut vt);
    vt.initialize();
    let mut section = CpvtSection::default();
    assert_eq!(0usize, section.word_array.len());
    assert_eq!(0usize, section.line_array.len());
    let rect = Rearrange(&mut section, &vt);
    assert!(rect.height() > 0.0);
    assert_eq!(0usize, section.word_array.len());
    assert_eq!(1usize, section.line_array.len());
    let line = section.line_array.first();
    assert!(line.is_some());
    let line = line.unwrap();
    assert_eq!(-1, line.info.begin_word_index);
    assert_eq!(-1, line.info.end_word_index);
    crate::test_support::assert_float_eq(0.0, line.info.line_width);
}
#[test]
fn OutputLines_ParagraphSeparator() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    SetVariableTextDefaults(&mut vt);
    vt.initialize();
    let mut section = CpvtSection::default();
    PopulateSectionWithText(&mut section, &[0x001d, 0x1d1d]);
    Rearrange(&mut section, &vt);
    assert_eq!(2usize, section.word_array.len());
    assert!(section.word_array[0].word_x < section.word_array[1].word_x);
}
#[test]
fn OutputLines_Multiline_CurrencySymbol() {
    let fixture = LayoutFixture::new();
    let mut vt = CpvtVariableText::new(Some(&fixture.provider));
    vt.set_font_size(10.0);
    vt.set_plate_rect(CfxFloatRect::new(0., 0., 0.35, 1000.));
    vt.settings.limit_width = true; // Native SetAutoReturn(true).
    vt.initialize();
    // Dollar and native 0x0080 Euro prefix symbols stay with the following digit.
    for text in [
        [b'A' as u32, b' ' as u32, b'$' as u32, b'1' as u32],
        [b'A' as u32, b' ' as u32, 0x0080, b'1' as u32],
    ] {
        let mut section = CpvtSection::default();
        PopulateSectionWithText(&mut section, &text);
        Rearrange(&mut section, &vt);
        assert_eq!(2usize, section.line_array.len());
        let line0 = section.line_array.first();
        assert!(line0.is_some());
        let line0 = line0.unwrap();
        assert_eq!(0, line0.info.begin_word_index);
        assert_eq!(1, line0.info.end_word_index);
        let line1 = section.line_array.get(1);
        assert!(line1.is_some());
        let line1 = line1.unwrap();
        assert_eq!(2, line1.info.begin_word_index);
        assert_eq!(3, line1.info.end_word_index);
    }
}
