// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_simple_parser_unittest.cpp` at the pinned PDFium revision.

#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfSimpleParser;

#[test]
fn GetWord() {
    let test_data: &[(&[u8], &[u8])] = &[
        (b"", b""),
        (b" \t \0 \n", b""),
        (b"%this is a test case\r\n%2nd line", b""),
        (b" \t \0%try()%haha\n %another line \x07a", b""),
        (b"/", b""),
        (b"/99", b""),
        (b"/99}", b"/99"),
        (b" /Tester ", b"/Tester"),
        (b"\t(nice day)!\n ", b"(nice day)"),
        (b"\t(It is a (long) day)!\n ", b"(It is a (long) day)"),
        (
            b"\t(It is a \\(long\\) day!)hi\n ",
            b"(It is a \\(long\\) day!)",
        ),
        (b"<", b"<"),
        (b">", b">"),
        (b" \n<4545acdfedertt>abc ", b"<4545acdfedertt>"),
        (b" \n<4545a<ed>ertt>abc ", b"<4545a<ed>"),
        (b"<</oc 234 /color 2 3 R>>", b"<<"),
        (b"\t\t<< /abc>>", b"<<"),
        (b"(\\", b"(\\"),
        (b"> little bear", b">"),
        (b") another bear", b")"),
        (b">> end ", b">>"),
        (b"(sdfgfgbcv", b"(sdfgfgbcv"),
        (b"}", b"}"),
        (b"apple pear", b"apple"),
        (b" pi=3.1415 ", b"pi=3.1415"),
        (b" p t x c ", b"p"),
        (b" pt\0xc ", b"pt"),
        (b" $^&&*\t\0sdff ", b"$^&&*"),
        (b"\n\r+3.5656 -11.0", b"+3.5656"),
    ];
    for (i, &(input, expected)) in test_data.iter().enumerate() {
        let mut parser = CpdfSimpleParser::new(input);
        assert_eq!(parser.get_word(), expected, "for case {i}");
    }
}

#[test]
fn Bug358381390() {
    // Native as_byte_span(kInput) includes the array's terminating NUL.
    let kInput = b"1 beginbfchar\n<01> <>\nendbfchar\n1 beginbfchar\0";
    let mut parser = CpdfSimpleParser::new(kInput);
    assert_eq!(parser.get_word(), b"1");
    assert_eq!(parser.get_word(), b"beginbfchar");
    assert_eq!(parser.get_word(), b"<01>");
    assert_eq!(parser.get_word(), b"<>");
    assert_eq!(parser.get_word(), b"endbfchar");
    assert_eq!(parser.get_word(), b"1");
    assert_eq!(parser.get_word(), b"beginbfchar");
    assert_eq!(parser.get_word(), b"");
}
