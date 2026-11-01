// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Additional regressions for source-defined error and plane-boundary behavior.

use super::CfxXmlParser;
use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};
use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
use std::rc::Rc;

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

fn parse(input: &[u8]) -> Option<CfxXmlDocument> {
    CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(input))).parse()
}

fn root_text(input: &[u8]) -> Vec<u32> {
    let doc = parse(input).unwrap();
    doc.root()
        .first_child_named(&wide("r"))
        .unwrap()
        .text_data()
}

#[test]
fn eof_is_permissive_but_structural_and_attribute_errors_are_not() {
    assert!(parse(b"").is_none());
    let only_nul = parse(b"\0").unwrap();
    assert!(only_nul.root().first_child().is_none());
    for input in [b"<r>tail".as_slice(), b"<r a='unterminated".as_slice()] {
        let doc = parse(input).unwrap();
        let element = doc.root().first_child_named(&wide("r")).unwrap();
        // Pending text/attribute data is not materialized at a truncated EOF.
        assert!(element.text_data().is_empty());
        assert!(element.get_attribute(&wide("a")).is_empty());
    }
    for input in [
        b"outside<r/>".as_slice(),
        b"<r></wrong>".as_slice(),
        b"<r a='&amp'/>".as_slice(),
        b"<r\x0b/>".as_slice(),
    ] {
        assert!(parse(input).is_none(), "{input:?}");
    }
    // ICU whitespace outside an element is broader than XML syntax whitespace.
    let doc = parse(b"\x1c<r/>").unwrap();
    assert!(doc.root().first_child_named(&wide("r")).is_some());
}

#[test]
fn entity_prefixes_wrap_unsigned_and_keep_raw_wchar32_values() {
    let input = "<r>&#65junk;&#X41;&#4294967361;&#x100000041;&#55296;&#xı;</r>";
    assert_eq!(
        root_text(input.as_bytes()),
        vec![b'A' as u32, b'A' as u32, b'A' as u32, 0xd800, 1]
    );
}

#[test]
fn recognized_instruction_tokens_keep_source_question_mark_reprocessing() {
    let doc = parse(b"<?xml version='1.0'?><?acrobat \"a b\" c?x?><r/>").unwrap();
    let instruction = doc.root().first_child().unwrap();
    assert_eq!(instruction.name().as_ref(), wide("acrobat").as_slice());
    let data = instruction.target_data();
    assert_eq!(data.len(), 2);
    assert_eq!(data.get(0).unwrap().as_ref(), wide("a b").as_slice());
    assert_eq!(data.get(1).unwrap().as_ref(), wide("cxx").as_slice());
    assert!(instruction.next_sibling().unwrap().first_child().is_none());
}

#[test]
fn decoding_accepts_utf16_code_units_without_scalar_normalization() {
    for big_endian in [false, true] {
        let mut bytes = if big_endian {
            vec![0xfe, 0xff]
        } else {
            vec![0xff, 0xfe]
        };
        for unit in "<r>"
            .encode_utf16()
            .chain([0xd800, 0x0041])
            .chain("</r>".encode_utf16())
        {
            bytes.extend_from_slice(&if big_endian {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            });
        }
        assert_eq!(root_text(&bytes), [0xd800, 0x41]);
    }
}

#[test]
fn delimiters_cross_planes_without_losing_cdata_or_following_elements() {
    for encoding in 0..3 {
        for plane in [1024, 2048] {
            for offset in plane - 8..plane + 3 {
                // Move both opening and closing delimiters across every split.
                for (before, delimiter, after, expected) in [
                    ("<r>", "<![CDATA[", "data]]><s/>tail</r>", "datatail"),
                    ("<r><![CDATA[", "]]>", "<s/>tail</r>", "tail"),
                    ("<r>", "<!--", "comment--><s/>tail</r>", "tail"),
                    ("<r><!--", "-->", "<s/>tail</r>", "tail"),
                ] {
                    let padding = "x".repeat(offset - before.len());
                    let input = format!("{before}{padding}{delimiter}{after}");
                    let bytes = if encoding == 0 {
                        input.as_bytes().to_vec()
                    } else {
                        let mut bytes = if encoding == 1 {
                            vec![0xff, 0xfe]
                        } else {
                            vec![0xfe, 0xff]
                        };
                        for unit in input.encode_utf16() {
                            bytes.extend_from_slice(&if encoding == 1 {
                                unit.to_le_bytes()
                            } else {
                                unit.to_be_bytes()
                            });
                        }
                        bytes
                    };
                    let doc = parse(&bytes).unwrap();
                    let root = doc.root().first_child_named(&wide("r")).unwrap();
                    assert!(
                        root.first_child_named(&wide("s")).is_some(),
                        "encoding={encoding}, offset={offset}, delimiter={delimiter}"
                    );
                    let expected = if before == "<r><!--" {
                        expected.to_owned()
                    } else {
                        padding + expected
                    };
                    assert_eq!(
                        root.text_data(),
                        wide(&expected),
                        "encoding={encoding}, offset={offset}, delimiter={delimiter}"
                    );
                }
            }
        }
    }
}

#[test]
fn utf8_scalars_cross_text_and_cdata_planes_without_loss() {
    for offset in 1019..1027 {
        for scalar in ["é", "€", "😀"] {
            for (open, close) in [("<r>", "</r>"), ("<r><![CDATA[", "]]></r>")] {
                let prefix = "a".repeat(offset - open.len());
                let text = format!("{prefix}{scalar}b");
                let input = format!("{open}{text}{close}");
                assert_eq!(
                    root_text(input.as_bytes()),
                    wide(&text),
                    "offset={offset}, scalar={scalar}, open={open}"
                );
            }
        }
    }
}

struct FailedRead;
impl StreamWithSize for FailedRead {
    fn get_size(&self) -> i64 {
        32
    }
}
impl SeekableReadStream for FailedRead {
    fn read_block_at_offset(&self, output: &mut [u8], _offset: i64) -> bool {
        if let Some(first) = output.first_mut() {
            *first = b'<';
        }
        false
    }
}

#[test]
fn failing_stream_read_is_successful_partial_parse_as_in_native() {
    let mut parser = CfxXmlParser::new(Rc::new(FailedRead));
    let doc = parser.parse().unwrap();
    assert!(doc.root().first_child().is_none());
    assert!(parser.current_node.is_none());
}

#[test]
fn parser_restores_current_node_after_success_failure_and_second_parse() {
    let mut parser = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(b"<r>tail")));
    let doc = parser.parse().unwrap();
    assert!(doc.root().first_child_named(&wide("r")).is_some());
    assert!(parser.current_node.is_none());
    let second = parser.parse().unwrap();
    assert!(second.root().first_child().is_none());
    assert!(parser.current_node.is_none());

    let mut parser = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(b"<r a=1>")));
    assert!(parser.parse().is_none());
    assert!(parser.current_node.is_none());
}
