// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_syntax_parser_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfSyntaxParser;
use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
use std::rc::Rc;

fn parser(data: &'static [u8]) -> CpdfSyntaxParser {
    CpdfSyntaxParser::from_file(Rc::new(CfxReadOnlySpanStream::new(data)))
}

#[test]
fn ReadHexString() {
    // Each row is a separate source parser, with the original output and position.
    for (data, expected, pos) in [
        (&b""[..], &b""[..], 0),
        (&b"  "[..], &b""[..], 2),
        (&b"z12b"[..], &b"\x12\xb0"[..], 4),
        (&b"*<&*#$^&@1"[..], &b"\x10"[..], 10),
        (&b"\x80zab"[..], &b"\xab"[..], 4),
        (&b"\xffzab"[..], &b"\xab"[..], 4),
        (&b"1A2b>abcd"[..], &b"\x1a\x2b"[..], 5),
    ] {
        let mut parser = parser(data);
        assert_eq!(parser.read_hex_string(), expected);
        assert_eq!(parser.get_pos(), pos);
    }
    {
        // Position out of bounds, then return to the beginning.
        let mut parser = parser(b"12ab>");
        parser.set_pos(5);
        assert!(parser.read_hex_string().is_empty());
        parser.set_pos(6);
        assert!(parser.read_hex_string().is_empty());
        parser.set_pos(i64::MAX);
        assert!(parser.read_hex_string().is_empty());
        parser.set_pos(0);
        assert_eq!(parser.read_hex_string(), &[0x12, 0xab]);
    }
    for (data, expected, pos) in [
        (&b"1A2b"[..], &b"\x1a\x2b"[..], 4),
        (&b"12abz"[..], &b"\x12\xab"[..], 5),
        (&b"1A2>asdf"[..], &b"\x1a\x20"[..], 4),
        (&b"1A2zasdf"[..], &b"\x1a\x2a\xdf"[..], 8),
        (&b">"[..], &b""[..], 1),
    ] {
        let mut parser = parser(data);
        assert_eq!(parser.read_hex_string(), expected);
        assert_eq!(parser.get_pos(), pos);
    }
}

#[test]
fn GetInvalidReference() {
    let mut parser = parser(b"4294967295 0 R");
    let reference = parser.get_object_body(None);
    assert!(reference.is_none());
}

#[test]
fn PeekNextWord() {
    // The native uint8_t array includes its terminating NUL.
    let mut parser = parser(b"    WORD \0");
    assert_eq!(parser.peek_next_word(), b"WORD");
    assert_eq!(parser.get_next_word().word, b"WORD");
}
