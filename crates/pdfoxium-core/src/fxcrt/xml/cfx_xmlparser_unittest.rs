// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete cases from `core/fxcrt/xml/cfx_xmlparser_unittest.cpp`.
#![allow(non_snake_case)]

use super::CfxXmlParser;
use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
use crate::fxcrt::xml::cfx_xmlnode::XmlNodeType;
use std::rc::Rc;

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

fn parse(input: &[u8]) -> Option<CfxXmlDocument> {
    // Native spans over its literal/static char arrays include the trailing NUL.
    let mut terminated = input.to_vec();
    terminated.push(0);
    let mut parser = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&terminated)));
    parser.parse()
}

fn script_text(input: &[u8]) -> Vec<u32> {
    let doc = parse(input).expect("native parser succeeds");
    let script = doc.root().first_child_named(&wide("script")).unwrap();
    script.text_data()
}

#[test]
fn AttributesMustBeQuoted() {
    let input = b"<script display=1>\n</script>";
    assert!(parse(input).is_none());
}

#[test]
fn Attributes() {
    let input = b"<script contentType=\"application/x-javascript\" display=\"1\">\n</script>";
    let doc = parse(input).unwrap();
    let script = doc.root().first_child_named(&wide("script")).unwrap();
    assert_eq!(
        script.get_attribute(&wide("contentType")).as_ref(),
        wide("application/x-javascript").as_slice()
    );
    assert_eq!(
        script.get_attribute(&wide("display")).as_ref(),
        wide("1").as_slice()
    );
}

#[test]
fn CData() {
    let input =
        b"<script>\n  <![CDATA[\n    if (a[1] < 3)\n      app.alert(\"Tclams\");\n  ]]>\n</script>";
    let cdata = "\n  \n    if (a[1] < 3)\n      app.alert(\"Tclams\");\n  \n";
    assert_eq!(script_text(input), wide(cdata));
}

#[test]
fn CDataWithInnerScript() {
    let input = b"<script>\n  <![CDATA[\n    if (a[1] < 3)\n      app.alert(\"Tclams\");\n    </script>\n  ]]>\n</script>";
    let cdata = "\n  \n    if (a[1] < 3)\n      app.alert(\"Tclams\");\n    </script>\n  \n";
    assert_eq!(script_text(input), wide(cdata));
}

#[test]
fn ArrowBangArrow() {
    let input = b"<script>\n  <!>\n</script>";
    assert_eq!(script_text(input), wide("\n  \n"));
}

#[test]
fn ArrowBangBracketArrow() {
    let input = b"<script>\n  <![>\n</script>";
    assert_eq!(script_text(input), wide("\n  "));
}

#[test]
fn IncompleteCData() {
    let input = b"<script>\n  <![CDATA>\n</script>";
    assert_eq!(script_text(input), wide("\n  "));
}

#[test]
fn UnClosedCData() {
    let input = b"<script>\n  <![CDATA[\n</script>";
    assert_eq!(script_text(input), wide("\n  "));
}

#[test]
fn EmptyCData() {
    let input = b"<script>\n  <![CDATA[]]>\n</script>";
    assert_eq!(script_text(input), wide("\n  \n"));
}

#[test]
fn Comment() {
    let input = b"<script>\n  <!-- A Comment -->\n</script>";
    assert_eq!(script_text(input), wide("\n  \n"));
}

#[test]
fn IncorrectCommentStart() {
    let input = b"<script>\n  <!- A Comment -->\n</script>";
    assert_eq!(script_text(input), wide("\n  \n"));
}

#[test]
fn CommentEmpty() {
    let input = b"<script>\n  <!---->\n</script>";
    assert_eq!(script_text(input), wide("\n  \n"));
}

#[test]
fn CommentThreeDash() {
    let input = b"<script>\n  <!--->\n</script>";
    assert_eq!(script_text(input), wide("\n  "));
}

#[test]
fn CommentTwoDash() {
    let input = b"<script>\n  <!-->\n</script>";
    assert_eq!(script_text(input), wide("\n  "));
}

#[test]
fn Entities() {
    let input = concat!(
        "<script>",
        "&#66;",
        "&#x54;",
        "&#x6a;",
        "&#x00000000000000000048;",
        "&#x0000000000000000AB48;",
        "&#x0000000000000000000;",
        "&amp;",
        "&lt;",
        "&gt;",
        "&apos;",
        "&quot;",
        "&something_else;",
        "</script>",
    );
    assert_eq!(script_text(input.as_bytes()), wide("BTjH\u{ab48}&<>'\""));
}

#[test]
fn EntityOverflowHex() {
    let input = b"<script>&#xaDBDFFFFF;&#xafffffffffffffffffffffffffffffffff;</script>";
    assert_eq!(script_text(input), wide("  "));
}

#[test]
fn EntityOverflowDecimal() {
    let input = b"<script>&#2914910205;&#29149102052342342134521341234512351234213452315;</script>";
    assert_eq!(script_text(input), wide("  "));
}

#[test]
fn IsXMLNameChar() {
    let cases = [
        ('-' as u32, true, false),
        ('-' as u32, false, true),
        ('.' as u32, true, false),
        ('.' as u32, false, true),
        ('0' as u32, true, false),
        ('0' as u32, false, true),
        ('a' as u32, true, true),
        ('a' as u32, false, true),
        ('A' as u32, true, true),
        ('A' as u32, false, true),
        ('(' as u32, false, false),
        ('(' as u32, true, false),
        (')' as u32, false, false),
        (')' as u32, true, false),
        ('[' as u32, false, false),
        ('[' as u32, true, false),
        (']' as u32, false, false),
        (']' as u32, true, false),
        (0x2069, true, false),
        (0x2070, true, true),
        (0x2073, true, true),
        (0x218f, true, true),
        (0x2190, true, false),
        (0xfdef, true, false),
        (0xfdf0, true, true),
        (0xfdf1, true, true),
        (0xfffd, true, true),
        (0xfffe, true, false),
    ];
    for (character, first, expected) in cases {
        assert_eq!(
            CfxXmlParser::is_xml_name_char(character, first),
            expected,
            "{character:#x}/{first}"
        );
    }
}

#[test]
fn BadElementClose() {
    assert!(parse(b"</endtag>").is_none());
}

#[test]
fn DoubleElementClose() {
    assert!(parse(b"<p></p></p>").is_none());
}

#[test]
fn ParseInstruction() {
    let input = b"<?originalXFAVersion http://www.xfa.org/schema/xfa-template/3.3/ ?><form></form>";
    let doc = parse(input).unwrap();
    let first = doc.root().first_child().unwrap();
    assert_eq!(first.node_type(), XmlNodeType::Instruction);
    assert!(first.is_original_xfa_version());
}

#[test]
fn BadEntity() {
    let input = b"<script>Test &<p>; thing</script>";
    assert!(parse(input).is_none());
}
