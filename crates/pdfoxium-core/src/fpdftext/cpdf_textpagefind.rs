// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Text search, whitespace matching and character-index translation.
use super::cpdf_textpage::CpdfTextPage;
use std::rc::Rc;
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub match_case: bool,
    pub match_whole_word: bool,
    pub consecutive: bool,
}
fn ignore_space(character: u32) -> bool {
    !(character < 255
        || (0x600..=0x6ff).contains(&character)
        || (0xfe70..=0xfeff).contains(&character)
        || (0xfb50..=0xfdff).contains(&character)
        || (0x400..=0x52f).contains(&character)
        || (0xa640..=0xa69f).contains(&character)
        || (0x2de0..=0x2dff).contains(&character)
        || character == 8467
        || (0x2000..=0x206f).contains(&character))
}
fn space(character: u32) -> bool {
    matches!(character, 10 | 13 | 32 | 160)
}
type SourcePositions = Option<Vec<(usize, usize)>>;

// Decode/fold in one pass. Most page text already uses one entry per scalar:
// its source mapping is implicit. Only allocate a map after an actual pair
// changes the indices. Queries need no map at all.
fn normalize<const TRACK_POSITIONS: bool>(
    text: &[u32],
    match_case: bool,
) -> (Vec<u32>, SourcePositions) {
    let mut scalars = Vec::with_capacity(text.len());
    let mut positions: SourcePositions = None;
    let mut index = 0;
    while index < text.len() {
        let first = index;
        let mut scalar = text[index];
        if (0xd800..=0xdbff).contains(&scalar)
            && text
                .get(index + 1)
                .is_some_and(|low| (0xdc00..=0xdfff).contains(low))
        {
            scalar = 0x10000 + ((scalar - 0xd800) << 10) + text[index + 1] - 0xdc00;
            if TRACK_POSITIONS && positions.is_none() {
                let mut mapped = Vec::with_capacity(text.len());
                mapped.extend((0..scalars.len()).map(|index| (index, index)));
                positions = Some(mapped);
            }
            index += 1;
        }
        if !match_case {
            scalar = if scalar <= 0x7f {
                (scalar as u8).to_ascii_lowercase().into()
            } else {
                char::from_u32(scalar).map_or(scalar, |ch| ch.to_lowercase().next().unwrap() as u32)
            };
        }
        scalars.push(scalar);
        if let Some(positions) = &mut positions {
            positions.push((first, index));
        }
        index += 1;
    }
    (scalars, positions)
}
fn search_starts(
    len: usize,
    positions: &SourcePositions,
    start: Option<usize>,
) -> (Option<usize>, Option<usize>) {
    if len == 0 {
        (None, None)
    } else if let Some(positions) = positions {
        (
            start.and_then(|start| {
                let index = positions.partition_point(|&(first, _)| first < start);
                (index < len).then_some(index)
            }),
            start.map_or(Some(len - 1), |start| {
                positions
                    .partition_point(|&(_, last)| last <= start)
                    .checked_sub(1)
            }),
        )
    } else {
        (
            start.filter(|&index| index < len),
            Some(start.unwrap_or(len - 1).min(len - 1)),
        )
    }
}
fn whole_word(text: &[u32], start: usize, end: usize) -> bool {
    if start > end {
        return false;
    }
    if start == end && text[start] > 255 {
        return true;
    }
    let word = |c: u32| {
        (65..=90).contains(&c)
            || (97..=122).contains(&c)
            || (48..=57).contains(&c)
            || (0xfb00..=0xfb06).contains(&c)
    };
    start.checked_sub(1).is_none_or(|i| !word(text[i]))
        && text.get(end + 1).is_none_or(|&c| !word(c))
}
fn extract_find_what(query: &[u32]) -> Vec<Vec<u32>> {
    if query.iter().all(|&c| c == 32) {
        return vec![query.to_vec()];
    }
    let mut words = Vec::new();
    let mut begin = 0;
    loop {
        let end = query[begin..]
            .iter()
            .position(|&c| c == 32)
            .map_or(query.len(), |i| begin + i);
        let word = &query[begin..end];
        if word.is_empty() {
            words.push(Vec::new());
        } else {
            let mut start = 0;
            for (index, &character) in word.iter().enumerate() {
                if !ignore_space(character) || (index > start && character == 0x2019) {
                    continue;
                }
                if index > start {
                    words.push(word[start..index].to_vec());
                }
                words.push(vec![character]);
                start = index + 1;
            }
            if start < word.len() {
                words.push(word[start..].to_vec());
            }
        }
        if end == query.len() {
            break;
        }
        begin = end + 1;
        while begin < query.len() && query[begin] == 32 {
            begin += 1;
        }
    }
    words
}
pub struct CpdfTextPageFind {
    text_page: Rc<CpdfTextPage>,
    text: Vec<u32>,
    text_positions: SourcePositions,
    words: Vec<Vec<u32>>,
    options: Options,
    next: Option<usize>,
    previous: Option<usize>,
    result_start: usize,
    result_end: usize,
    has_result: bool,
}
impl CpdfTextPageFind {
    pub fn new(
        text_page: Rc<CpdfTextPage>,
        query: &[u32],
        options: Options,
        start: Option<usize>,
    ) -> Self {
        // ToUnicode maps retain UTF-16 units, while ActualText and public
        // search queries can contain full scalars. Match equivalent Unicode
        // text, retaining original positions for the public character APIs.
        let (text, text_positions) =
            normalize::<true>(text_page.all_page_text(), options.match_case);
        let (query, _) = normalize::<false>(query, options.match_case);
        let words = extract_find_what(&query);
        let (next, previous) = search_starts(text.len(), &text_positions, start);
        Self {
            text_page,
            text,
            text_positions,
            words,
            options,
            next,
            previous,
            result_start: 0,
            result_end: 0,
            has_result: false,
        }
    }
    fn update_starts(&mut self) {
        if self.options.consecutive {
            self.next = self.result_start.checked_add(1);
            self.previous = self.result_end.checked_sub(1);
        } else {
            self.next = self.result_end.checked_add(1);
            self.previous = self.result_start.checked_sub(1);
        }
    }
    pub fn find_next(&mut self) -> bool {
        let Some(mut start) = self.next.filter(|&start| start < self.text.len()) else {
            return false;
        };
        if self.words.is_empty() || self.words.iter().all(Vec::is_empty) {
            return false;
        }
        let mut index = 0;
        let mut result = 0;
        let mut space_start = false;
        while index < self.words.len() {
            let word = &self.words[index];
            if word.is_empty() {
                if index == self.words.len() - 1 {
                    let Some(&character) = self.text.get(start) else {
                        return false;
                    };
                    if space(character) {
                        result = start + 1;
                        break;
                    }
                    // Retry the first word strictly after this failed candidate.
                    start = self.result_start.saturating_add(1);
                    index = 0;
                    continue;
                } else if index == 0 {
                    space_start = true;
                }
                index += 1;
                continue;
            }
            let Some(found) = self
                .text
                .get(start..)
                .and_then(|text| text.windows(word.len()).position(|part| part == word))
                .map(|offset| start + offset)
            else {
                return false;
            };
            result = found;
            let end = found + word.len() - 1;
            if index == 0 {
                self.result_start = found;
            }
            let mut matched = true;
            if index != 0 && (!space_start || index > 1) {
                let previous = self.words[index - 1].last().copied().unwrap_or(32);
                if start == found && !(ignore_space(previous) || ignore_space(word[0])) {
                    matched = false;
                }
                if !self.text[start..found].iter().copied().all(space) {
                    matched = false;
                }
            } else if space_start {
                if found > 0 {
                    if space(self.text[found - 1]) {
                        let candidate_start = found - 1;
                        // A leading query space belongs to the match. Do not
                        // move its range before this attempt's search start:
                        // consecutive stepping (also used by FindPrev) must
                        // not rediscover the previous word with its old space.
                        if candidate_start < start {
                            start = found + 1;
                            index = 0;
                            continue;
                        }
                        self.result_start = candidate_start;
                    } else {
                        matched = false;
                        self.result_start = found;
                    }
                }
            }
            if self.options.match_whole_word && matched {
                matched = whole_word(&self.text, found, end);
            }
            if matched {
                start = end + 1;
                index += 1;
            } else {
                let first = usize::from(space_start);
                start = self
                    .result_start
                    .saturating_add(self.words.get(first).map_or(1, |word| word.len().max(1)));
                index = 0;
            }
        }
        let last_len = self.words.last().map_or(0, Vec::len);
        let Some(end) = result
            .checked_add(last_len)
            .and_then(|end| end.checked_sub(1))
        else {
            return false;
        };
        self.result_end = end;
        self.has_result = true;
        self.update_starts();
        true
    }
    pub fn find_prev(&mut self) -> bool {
        let Some(previous) = self.previous else {
            return false;
        };
        let mut search = Self {
            text_page: self.text_page.clone(),
            text: self.text.clone(),
            text_positions: self.text_positions.clone(),
            words: self.words.clone(),
            // Scan every candidate to choose the rightmost previous match.
            // Outer stepping still controls overlap between returned matches.
            options: Options {
                consecutive: true,
                ..self.options
            },
            next: Some(0),
            previous: Some(0),
            result_start: 0,
            result_end: 0,
            has_result: false,
        };
        let mut best = None;
        while search.find_next() {
            if search.result_end > previous {
                break;
            }
            best = Some((search.result_start, search.result_end));
        }
        let Some((start, end)) = best else {
            return false;
        };
        self.result_start = start;
        self.result_end = end;
        self.has_result = true;
        self.update_starts();
        true
    }
    pub fn get_cur_order(&self) -> i32 {
        self.source_position(self.result_start)
            .and_then(|(first, _)| i32::try_from(first).ok())
            .map_or(-1, |i| self.text_page.char_index_from_text_index(i))
    }
    pub fn get_matched_count(&self) -> i32 {
        if !self.has_result {
            return 0;
        }
        let start = self.get_cur_order();
        let end = self
            .source_position(self.result_end)
            .and_then(|(_, last)| i32::try_from(last).ok())
            .map_or(-1, |i| self.text_page.char_index_from_text_index(i));
        if start < 0 || end < start {
            0
        } else {
            end - start + 1
        }
    }
    fn source_position(&self, index: usize) -> Option<(usize, usize)> {
        match &self.text_positions {
            Some(positions) => positions.get(index).copied(),
            None => (index < self.text.len()).then_some((index, index)),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::{
        font::cpdf_font::CpdfFont,
        page::{
            cpdf_docpagedata::CpdfDocPageData, cpdf_page::CpdfPage, cpdf_pageobject::PageObject,
            cpdf_textobject::CpdfTextObject,
        },
        parser::{cpdf_dictionary::CpdfDictionary, cpdf_object::retain_object},
    };
    use crate::{
        fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix},
        fxge::cfx_fontmapper::CfxFontMapper,
    };
    fn page(text: &[u8]) -> Rc<CpdfTextPage> {
        let mut dictionary = CpdfDictionary::new();
        dictionary.set_new_name_for(b"Type", b"Page");
        dictionary.set_rect_for(b"MediaBox", CfxFloatRect::new(0., 0., 400., 400.));
        let mut page = CpdfPage::new(retain_object(dictionary)).unwrap();
        let font = CpdfFont::Simple(
            CpdfDocPageData::new()
                .get_stock_font(b"Helvetica", &mut CfxFontMapper::new())
                .unwrap(),
        );
        let mut object = CpdfTextObject::new(0);
        let dict = font.font_dict();
        object
            .base_mut()
            .mutable_text_state()
            .set_font(Some(font), dict);
        object.base_mut().mutable_text_state().set_font_size(12.);
        object.set_text(text);
        object.set_text_matrix(CfxMatrix::new(1., 0., 0., 1., 20., 300.));
        page.mutable_holder().append_page_object(Box::new(object));
        Rc::new(CpdfTextPage::new(&page, false))
    }
    fn query(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    #[test]
    fn supplementary_search_normalization_retains_original_character_spans() {
        let (text, positions) =
            normalize::<true>(&[0x41, 0xd83d, 0xde00, 0x1f642, 0xd83d, 0x42], true);
        assert_eq!(text, [0x41, 0x1f600, 0x1f642, 0xd83d, 0x42]);
        assert_eq!(positions.unwrap(), [(0, 0), (1, 2), (3, 3), (4, 4), (5, 5)]);
        assert_eq!(
            normalize::<true>(&[0xd83d, 0xde00], true).0,
            normalize::<true>(&[0x1f600], true).0
        );
    }
    #[test]
    fn fused_normalization_preserves_unicode_folding_identity_and_partial_pairs() {
        let cases: &[(&[u32], &[u32], SourcePositions)] = &[
            (&[], &[], None),
            (&[65, 90, 32, 97, 122], &[97, 122, 32, 97, 122], None),
            (
                &[0xc4, 0x130, 0x3a3, 0x1e9e, 0x1d45e, 0x110000],
                &[0xe4, 0x69, 0x3c3, 0xdf, 0x1d45e, 0x110000],
                None,
            ),
            (
                &[65, 0xd835, 0xdc5e, 0x1d45e, 0xd800, 66, 0xdc00],
                &[97, 0x1d45e, 0x1d45e, 0xd800, 98, 0xdc00],
                Some(vec![(0, 0), (1, 2), (3, 3), (4, 4), (5, 5), (6, 6)]),
            ),
            (
                &[0xd835, 0xdc5e, 0xd835, 0xdc5e],
                &[0x1d45e, 0x1d45e],
                Some(vec![(0, 1), (2, 3)]),
            ),
            (&[0xdc5e, 65, 0xd835], &[0xdc5e, 97, 0xd835], None),
        ];
        for &(source, folded, ref expected_positions) in cases {
            let (actual, positions) = normalize::<true>(source, false);
            assert_eq!(actual, folded);
            assert_eq!(&positions, expected_positions);
            let (query, unused_positions) = normalize::<false>(source, false);
            assert_eq!(query, folded);
            assert!(unused_positions.is_none());
            let (sensitive, sensitive_positions) = normalize::<true>(source, true);
            assert_eq!(sensitive_positions, positions);
            // Case-sensitive normalization keeps each source scalar/code unit;
            // only the explicitly paired entries can consume two source slots.
            for (index, &scalar) in sensitive.iter().enumerate() {
                let (first, last) = positions.as_ref().map_or((index, index), |p| p[index]);
                let expected = if first == last {
                    source[first]
                } else {
                    char::decode_utf16(source[first..=last].iter().map(|&v| v as u16))
                        .next()
                        .unwrap()
                        .unwrap() as u32
                };
                assert_eq!(scalar, expected);
            }
            let full_positions = positions
                .clone()
                .unwrap_or_else(|| (0..actual.len()).map(|index| (index, index)).collect());
            for start in (0..=source.len() + 2)
                .map(Some)
                .chain([None, Some(usize::MAX)])
            {
                let expected = (
                    start.and_then(|s| full_positions.iter().position(|&(first, _)| first >= s)),
                    start.map_or_else(
                        || full_positions.len().checked_sub(1),
                        |s| full_positions.iter().rposition(|&(_, last)| last <= s),
                    ),
                );
                assert_eq!(search_starts(actual.len(), &positions, start), expected);
            }
        }
    }
    #[test]
    fn forward_backward_case_wholeword_overlap_and_bounds() {
        let text = page(b"banana BANANA");
        let mut search = CpdfTextPageFind::new(
            text.clone(),
            &query("ana"),
            Options {
                consecutive: true,
                ..Default::default()
            },
            Some(0),
        );
        assert_eq!(search.get_matched_count(), 0);
        assert!(search.find_next());
        assert_eq!((search.get_cur_order(), search.get_matched_count()), (1, 3));
        assert!(search.find_next());
        assert_eq!(search.get_cur_order(), 3);
        assert!(search.find_prev());
        assert_eq!(search.get_cur_order(), 1);
        assert!(!search.find_prev());
        let mut backwards =
            CpdfTextPageFind::new(text.clone(), &query("ana"), Options::default(), None);
        assert!(!backwards.find_next());
        assert!(backwards.find_prev());
        assert_eq!(backwards.get_cur_order(), 10);
        let mut case = CpdfTextPageFind::new(
            text,
            &query("BANANA"),
            Options {
                match_case: true,
                match_whole_word: true,
                ..Default::default()
            },
            Some(0),
        );
        assert!(case.find_next());
        assert_eq!(case.get_cur_order(), 7);
        assert!(!case.find_next());
        let mut whole = CpdfTextPageFind::new(
            page(b"foobar foo foo1 [foo]"),
            &query("foo"),
            Options {
                match_whole_word: true,
                ..Default::default()
            },
            Some(0),
        );
        assert!(whole.find_next());
        assert_eq!(whole.get_cur_order(), 7);
        assert!(whole.find_next());
        assert_eq!(whole.get_cur_order(), 17);
        assert!(!whole.find_next());
        for value in ["", "absent"] {
            assert!(!CpdfTextPageFind::new(
                page(b"hello"),
                &query(value),
                Options::default(),
                Some(0)
            )
            .find_next());
        }
        assert!(!CpdfTextPageFind::new(
            page(b"hello"),
            &query("hello"),
            Options::default(),
            Some(usize::MAX)
        )
        .find_next());
    }
    #[test]
    fn word_spacing_leading_trailing_spaces_and_query_tokenization() {
        let extracted = page(b"foo  bar fooXbar foo bar");
        // CPDF_TextPage suppresses consecutive spaces while extracting.
        assert_eq!(
            extracted.get_all_page_text(),
            query("foo bar fooXbar foo bar")
        );
        let mut search =
            CpdfTextPageFind::new(extracted, &query("foo bar"), Options::default(), Some(0));
        assert!(search.find_next());
        assert_eq!((search.get_cur_order(), search.get_matched_count()), (0, 7));
        assert!(search.find_next());
        assert_eq!(
            (search.get_cur_order(), search.get_matched_count()),
            (16, 7)
        );
        assert!(!search.find_next());
        let mut padding = CpdfTextPageFind::new(
            page(b"x foo y"),
            &query(" foo "),
            Options::default(),
            Some(0),
        );
        assert!(padding.find_next());
        assert_eq!(
            (padding.get_cur_order(), padding.get_matched_count()),
            (1, 5)
        );
        let mut blanks =
            CpdfTextPageFind::new(page(b"a   b"), &query("  "), Options::default(), Some(0));
        assert!(!blanks.find_next());
        assert_eq!(
            extract_find_what(&query("中文 don’t")),
            vec![query("中"), query("文"), query("don’t")]
        );
        assert!(whole_word(&query("(foo)"), 1, 3));
        assert!(!whole_word(&query("foo2"), 0, 2));
        assert!(whole_word(&query("中文"), 0, 0));
    }

    fn expect_search_ranges(
        text: Rc<CpdfTextPage>,
        needle: &str,
        consecutive: bool,
        backwards: bool,
        expected: &[(i32, i32)],
    ) {
        let mut search = CpdfTextPageFind::new(
            text,
            &query(needle),
            Options {
                match_case: true,
                consecutive,
                ..Default::default()
            },
            if backwards { None } else { Some(0) },
        );
        for &range in expected {
            assert!(
                if backwards {
                    search.find_prev()
                } else {
                    search.find_next()
                },
                "{needle:?}, consecutive={consecutive}, backwards={backwards}, expected={range:?}"
            );
            assert_eq!((search.get_cur_order(), search.get_matched_count()), range);
        }
        assert!(
            !(if backwards {
                search.find_prev()
            } else {
                search.find_next()
            })
        );
    }

    #[test]
    fn padded_query_ranges_progress_in_both_directions_without_losing_words() {
        for (source, needle, expected) in [
            (b"Xcat cat! cat.".as_slice(), " cat", vec![(4, 4), (9, 4)]),
            (
                b"catX cat catY cat Z".as_slice(),
                "cat ",
                vec![(5, 4), (14, 4)],
            ),
            (
                b"Xfoo bar foo bar! foo bar Z".as_slice(),
                " foo bar ",
                vec![(17, 9)],
            ),
        ] {
            let text = page(source);
            for consecutive in [false, true] {
                expect_search_ranges(text.clone(), needle, consecutive, false, &expected);
                let reverse: Vec<_> = expected.iter().rev().copied().collect();
                expect_search_ranges(text.clone(), needle, consecutive, true, &reverse);
            }
        }
    }

    #[test]
    fn leading_query_space_does_not_rewind_a_requested_start_into_an_old_match() {
        let text = page(b"Xcat cat! cat.");
        for consecutive in [false, true] {
            for (start, expected) in [(5, Some((9, 4))), (10, None)] {
                let mut search = CpdfTextPageFind::new(
                    text.clone(),
                    &query(" cat"),
                    Options {
                        consecutive,
                        ..Default::default()
                    },
                    Some(start),
                );
                assert_eq!(search.find_next(), expected.is_some());
                if let Some(range) = expected {
                    assert_eq!((search.get_cur_order(), search.get_matched_count()), range);
                    assert!(!search.find_next());
                }
            }
        }
    }

    #[test]
    fn padded_query_progress_preserves_overlapping_banana_search_partitioning() {
        let text = page(b"banana banana");
        expect_search_ranges(text.clone(), "ana", false, false, &[(1, 3), (8, 3)]);
        expect_search_ranges(text.clone(), "ana", false, true, &[(10, 3), (3, 3)]);
        expect_search_ranges(
            text.clone(),
            "ana",
            true,
            false,
            &[(1, 3), (3, 3), (8, 3), (10, 3)],
        );
        expect_search_ranges(text, "ana", true, true, &[(10, 3), (8, 3), (3, 3), (1, 3)]);
    }
}
