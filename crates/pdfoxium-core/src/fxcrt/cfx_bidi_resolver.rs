// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fxcrt/cfx_bidi_resolver.{h,cpp}`. The safe Rust Unicode bidi engine
//! replaces ICU's UBiDi dependency. The caller retains the UTF-16 paragraph
//! for the resolver's lifetime instead of storing a pointer into an owned string.

use unicode_bidi::{utf16::BidiInfo, Level};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParagraphDirection {
    #[default]
    Auto,
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedRun {
    pub start: i32,
    pub length: i32,
    pub is_rtl: bool,
}

pub struct CfxBidiResolver<'a> {
    paragraph_bidi: BidiInfo<'a>,
}

impl<'a> CfxBidiResolver<'a> {
    pub fn create(text: &'a [u16], direction: ParagraphDirection) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        // Preserve PDFium's bound for ICU's run allocation arithmetic.
        assert!(text.len() <= i32::MAX as usize / (size_of::<i32>() * 3));
        let level = match direction {
            ParagraphDirection::Auto => None,
            ParagraphDirection::LeftToRight => Some(Level::ltr()),
            ParagraphDirection::RightToLeft => Some(Level::rtl()),
        };
        Some(Self {
            paragraph_bidi: BidiInfo::new(text, level),
        })
    }

    pub fn get_visual_runs_for_line(&self, line_start: i32, line_length: i32) -> Vec<ResolvedRun> {
        if line_start < 0 || line_length <= 0 {
            return Vec::new();
        }
        let Some(end) = line_start.checked_add(line_length) else {
            return Vec::new();
        };
        let start = line_start as usize;
        let end = end as usize;
        if end > self.paragraph_bidi.text.len() {
            return Vec::new();
        }
        // Like ubidi_setLine(), a line must belong to one paragraph.
        let Some(paragraph) = self
            .paragraph_bidi
            .paragraphs
            .iter()
            .find(|paragraph| paragraph.range.start <= start && end <= paragraph.range.end)
        else {
            return Vec::new();
        };
        let (levels, runs) = self.paragraph_bidi.visual_runs(paragraph, start..end);
        runs.into_iter()
            .map(|run| ResolvedRun {
                start: run.start as i32,
                length: (run.end - run.start) as i32,
                is_rtl: levels[run.start].is_rtl(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_visual_runs_and_line_limits() {
        let text: Vec<u16> = "ab אב cd".encode_utf16().collect();
        let resolver = CfxBidiResolver::create(&text, ParagraphDirection::LeftToRight).unwrap();
        assert_eq!(
            resolver.get_visual_runs_for_line(0, 8),
            vec![
                ResolvedRun {
                    start: 0,
                    length: 3,
                    is_rtl: false
                },
                ResolvedRun {
                    start: 3,
                    length: 2,
                    is_rtl: true
                },
                ResolvedRun {
                    start: 5,
                    length: 3,
                    is_rtl: false
                },
            ]
        );
        assert_eq!(
            resolver.get_visual_runs_for_line(3, 2),
            vec![ResolvedRun {
                start: 3,
                length: 2,
                is_rtl: true
            },]
        );
        for (start, length) in [(-1, 3), (0, -1), (0, 0), (1, 8), (9, 1), (1, i32::MAX)] {
            assert!(resolver.get_visual_runs_for_line(start, length).is_empty());
        }
        assert!(CfxBidiResolver::create(&[], ParagraphDirection::Auto).is_none());
        let invalid = [0xdc00, 0xdc00];
        assert_eq!(
            CfxBidiResolver::create(&invalid, ParagraphDirection::LeftToRight)
                .unwrap()
                .get_visual_runs_for_line(0, 2),
            vec![ResolvedRun {
                start: 0,
                length: 2,
                is_rtl: false
            },]
        );
    }
}

#[cfg(test)]
#[path = "cfx_bidi_resolver_unittest.rs"]
mod upstream_tests;
