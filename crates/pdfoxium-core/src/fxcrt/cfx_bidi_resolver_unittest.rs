// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/cfx_bidi_resolver_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::{CfxBidiResolver, ParagraphDirection, ResolvedRun};

#[test]
fn BidiResolverPureLTR() {
    let resolver = CfxBidiResolver::create(&[0x61, 0x62, 0x63], ParagraphDirection::LeftToRight);
    assert!(resolver.is_some());
    assert_eq!(
        resolver.unwrap().get_visual_runs_for_line(0, 3),
        [ResolvedRun {
            start: 0,
            length: 3,
            is_rtl: false
        }]
    );
}

#[test]
fn BidiResolverPureRTL() {
    let resolver =
        CfxBidiResolver::create(&[0x05d0, 0x05d1, 0x05d2], ParagraphDirection::RightToLeft);
    assert!(resolver.is_some());
    assert_eq!(
        resolver.unwrap().get_visual_runs_for_line(0, 3),
        [ResolvedRun {
            start: 0,
            length: 3,
            is_rtl: true
        }]
    );
}

#[test]
fn BidiResolverMixed() {
    let resolver = CfxBidiResolver::create(
        &[0x61, 0x62, 0x20, 0x05d0, 0x05d1, 0x20, 0x63, 0x64],
        ParagraphDirection::LeftToRight,
    );
    assert!(resolver.is_some());
    assert_eq!(
        resolver.unwrap().get_visual_runs_for_line(0, 8),
        [
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
}

#[test]
fn BidiResolverLineWrap() {
    let text: Vec<u16> = "abc def ghi".encode_utf16().collect();
    let resolver = CfxBidiResolver::create(&text, ParagraphDirection::LeftToRight);
    assert!(resolver.is_some());
    assert_eq!(
        resolver.unwrap().get_visual_runs_for_line(4, 4),
        [ResolvedRun {
            start: 4,
            length: 4,
            is_rtl: false
        }]
    );
}

#[test]
fn BidiResolverEmpty() {
    let resolver = CfxBidiResolver::create(&[], ParagraphDirection::Auto);
    assert!(resolver.is_none());
}

#[test]
fn BidiResolverOutOfBounds() {
    let resolver = CfxBidiResolver::create(&[0x61, 0x62, 0x63], ParagraphDirection::LeftToRight);
    assert!(resolver.is_some());
    let resolver = resolver.unwrap();
    assert!(resolver.get_visual_runs_for_line(-1, 3).is_empty());
    assert!(resolver.get_visual_runs_for_line(0, -1).is_empty());
    assert!(resolver.get_visual_runs_for_line(0, 0).is_empty());
    assert!(resolver.get_visual_runs_for_line(1, 3).is_empty());
    assert!(resolver.get_visual_runs_for_line(4, 1).is_empty());
}

#[test]
fn BidiResolverInvalidUTF16() {
    let resolver = CfxBidiResolver::create(&[0xdc00, 0xdc00], ParagraphDirection::LeftToRight);
    assert!(resolver.is_some());
    assert_eq!(
        resolver.unwrap().get_visual_runs_for_line(0, 2),
        [ResolvedRun {
            start: 0,
            length: 2,
            is_rtl: false
        }]
    );
}
