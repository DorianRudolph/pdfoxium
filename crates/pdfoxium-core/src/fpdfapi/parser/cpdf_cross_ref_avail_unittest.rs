// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_cross_ref_avail_unittest.cpp`.
#![allow(non_snake_case)]
use super::*;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
use crate::fxcrt::fx_system::itoa;
use std::rc::Rc;

fn MakeParserForBuffer(buffer: impl AsRef<[u8]> + 'static) -> CpdfSyntaxParser {
    // Owned constructed tables or static literal slices keep all source bytes
    // alive through the parser without cloning a whole buffer for its borrow.
    CpdfSyntaxParser::from_file(Rc::new(CfxReadOnlyContainerStream::new(buffer)))
}
fn format_int(value: i32, buffer: &mut [u8; 100]) -> &str {
    std::str::from_utf8(itoa(value, buffer, 10)).unwrap()
}

#[test]
fn CheckCrossRefTable() {
    let xref_table: &[u8] = concat!(
        "xref \n",
        "0 6 \n",
        "0000000003 65535 f \n",
        "0000000017 00000 n \n",
        "0000000081 00000 n \n",
        "0000000000 00007 f \n",
        "0000000331 00000 n \n",
        "0000000409 00000 n \n",
        "trailer\n",
        "<</Root 14 0 R/ID ",
        "[<afbb0f593c2d2aea5b519cb61da1c17b><4f9bb2e7978401808f8f1f2a75c322c8>]",
        "/Info 15 0 R/Size 16>>",
        "\0"
    )
    .as_bytes();
    let last_crossref_offset: i64 = 0;

    let mut parser = MakeParserForBuffer(xref_table);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);

    assert_eq!(DocAvailStatus::DataAvailable, cross_ref_avail.check_avail());
}

#[test]
fn CheckCrossRefStream() {
    let xref_stream: &[u8] = concat!(
        "16 0 obj\n",
        "<</Filter /FlateDecode>>",
        " stream \n",
        "STREAM DATA STREAM DATA STREAM DATA\n",
        "endstream\n",
        "endobj\n",
        "\0"
    )
    .as_bytes();
    let last_crossref_offset: i64 = 0;

    let mut parser = MakeParserForBuffer(xref_stream);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);

    assert_eq!(DocAvailStatus::DataAvailable, cross_ref_avail.check_avail());
}

#[test]
fn IncorrectStartOffset() {
    let xref_stream: &[u8] = concat!(
        "16 0 obj\n",
        "<</Filter /FlateDecode>>",
        " stream \n",
        "STREAM DATA STREAM DATA STREAM DATA\n",
        "endstream\n",
        "endobj\n",
        "\0"
    )
    .as_bytes();

    let last_crossref_offset: i64 = 70000;

    let mut parser = MakeParserForBuffer(xref_stream);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);

    assert_eq!(DocAvailStatus::DataError, cross_ref_avail.check_avail());
}

#[test]
fn IncorrectPrevOffset() {
    let xref_stream: &[u8] = concat!(
        "16 0 obj\n",
        "<</Type /XRef /Prev 70000>>",
        " stream \n",
        "STREAM DATA STREAM DATA STREAM DATA\n",
        "endstream\n",
        "endobj\n",
        "\0"
    )
    .as_bytes();
    let last_crossref_offset: i64 = 0;

    let mut parser = MakeParserForBuffer(xref_stream);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);
    assert_eq!(DocAvailStatus::DataError, cross_ref_avail.check_avail());
}

#[test]
fn IncorrectPrevStreamOffset() {
    let xref_table: &[u8] = concat!(
        "xref \n",
        "0 6 \n",
        "0000000003 65535 f \n",
        "0000000017 00000 n \n",
        "0000000081 00000 n \n",
        "0000000000 00007 f \n",
        "0000000331 00000 n \n",
        "0000000409 00000 n \n",
        "trailer\n",
        "<</Root 14 0 R/ID ",
        "[<afbb0f593c2d2aea5b519cb61da1c17b><4f9bb2e7978401808f8f1f2a75c322c8>]",
        "/Info 15 0 R/Size 16 /XRefStm 70000>>",
        "\0"
    )
    .as_bytes();
    let last_crossref_offset: i64 = 0;

    let mut parser = MakeParserForBuffer(xref_table);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);
    assert_eq!(DocAvailStatus::DataError, cross_ref_avail.check_avail());
}

#[test]
fn IncorrectData() {
    let incorrect_data: &[u8] = concat!(
        "fiajaoilf w9ifaoihwoiafhja wfijaofijoiaw fhj oiawhfoiah ",
        "wfoihoiwfghouiafghwoigahfi",
        "\0"
    )
    .as_bytes();
    let last_crossref_offset: i64 = 0;

    let mut parser = MakeParserForBuffer(incorrect_data);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);
    assert_eq!(DocAvailStatus::DataError, cross_ref_avail.check_avail());
}

#[test]
fn ThreeCrossRefTable() {
    let mut int_buffer = [0u8; 100];
    let mut table = String::from("pdf blah blah blah\n");
    let mut cur_offset = table.len();
    table.push_str(
        &(concat!(
            "xref \n",
            "0 6 \n",
            "0000000003 65535 f \n",
            "trailer\n",
            "<</Root 14 0 R/ID ",
            "[<afbb0f593c2d2aea5b519cb61da1c17b><4f9bb2e7978401808f8f1f2a75c322c8>]",
            "/Info 15 0 R/Size 16>>\n"
        )),
    );
    table.push_str(&("Dummy Data jgwhughouiwbahng"));
    let mut prev_offset = cur_offset;
    cur_offset = table.len();
    table.push_str(
        &(String::from(concat!(
            "xref \n",
            "0 6 \n",
            "0000000003 65535 f \n",
            "trailer\n",
            "<</Root 14 0 R/ID ",
            "[<afbb0f593c2d2aea5b519cb61da1c17b><",
            "4f9bb2e7978401808f8f1f2a75c322c8>]",
            "/Info 15 0 R/Size 16",
            "/Prev "
        )) + format_int(prev_offset as i32, &mut int_buffer)
            + ">>\n"),
    );
    table.push_str(&("More Dummy Data jgwhughouiwbahng"));
    prev_offset = cur_offset;
    cur_offset = table.len();
    table.push_str(
        &(String::from(concat!(
            "xref \n",
            "0 6 \n",
            "0000000003 65535 f \n",
            "trailer\n",
            "<</Root 14 0 R/ID ",
            "[<afbb0f593c2d2aea5b519cb61da1c17b><",
            "4f9bb2e7978401808f8f1f2a75c322c8>]",
            "/Info 15 0 R/Size 16",
            "/Prev "
        )) + format_int(prev_offset as i32, &mut int_buffer)
            + ">>\n"),
    );
    let last_crossref_offset: i64 = cur_offset as i64;

    let mut parser = MakeParserForBuffer(table);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);
    assert_eq!(DocAvailStatus::DataAvailable, cross_ref_avail.check_avail());
}

#[test]
fn ThreeCrossRefStream() {
    let mut int_buffer = [0u8; 100];
    let mut table = String::from("pdf blah blah blah\n");
    let mut cur_offset = table.len();
    table.push_str(
        &(concat!(
            "16 0 obj\n",
            "<</Type /XRef>>",
            " stream \n",
            "STREAM DATA STREAM DATA STREAM DATA ahfcuabfkuabfu\n",
            "endstream\n",
            "endobj\n"
        )),
    );
    table.push_str(&("Dummy Data jgwhughouiwbahng"));

    let mut prev_offset = cur_offset;
    cur_offset = table.len();
    table.push_str(
        &(String::from(concat!("55 0 obj\n", "<</Type /XRef /Prev "))
            + format_int(prev_offset as i32, &mut int_buffer)
            + concat!(
                ">>",
                " stream \n",
                "STREAM DATA STREAM DATA STREAM DATA\n",
                "endstream\n",
                "endobj\n"
            )),
    );
    table.push_str(&("More Dummy Data jgwhughouiwbahng"));
    prev_offset = cur_offset;
    cur_offset = table.len();
    table.push_str(
        &(String::from(concat!("88 0 obj\n", "<</Type /XRef /NNNN /Prev "))
            + format_int(prev_offset as i32, &mut int_buffer)
            + concat!(
                ">>",
                " stream \n",
                "STREAM DATA STREAM DATA STREAM DATA favav\n",
                "endstream\n",
                "endobj\n"
            )),
    );
    let last_crossref_offset: i64 = cur_offset as i64;

    let mut parser = MakeParserForBuffer(table);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);
    assert_eq!(DocAvailStatus::DataAvailable, cross_ref_avail.check_avail());
}

#[test]
fn Mixed() {
    let mut int_buffer = [0u8; 100];
    let mut table = String::from("pdf blah blah blah\n");

    let first_stream_offset = table.len() as i32;
    table.push_str(
        &(concat!(
            "16 0 obj\n",
            "<</Type /XRef>>",
            " stream \n",
            "STREAM DATA STREAM DATA STREAM DATA ahfcuabfkuabfu\n",
            "endstream\n",
            "endobj\n"
        )),
    );
    table.push_str(&("Dummy Data jgwhughouiwbahng"));

    let second_table_offset = table.len() as i32;
    table.push_str(
        &(String::from(concat!(
            "xref \n",
            "0 6 \n",
            "0000000003 65535 f \n",
            "trailer\n",
            "<</Root 14 0 R/ID ",
            "[<afbb0f593c2d2aea5b519cb61da1c17b><",
            "4f9bb2e7978401808f8f1f2a75c322c8>]",
            "/Info 15 0 R/Size 16",
            "/Prev "
        )) + format_int(first_stream_offset, &mut int_buffer)
            + ">>\n"),
    );
    table.push_str(&("More Dummy Data jgwhughouiwbahng"));

    let last_table_offset = table.len() as i32;
    table.push_str(
        &(String::from(concat!(
            "xref \n",
            "0 6 \n",
            "0000000003 65535 f \n",
            "trailer\n",
            "<</Root 14 0 R/ID ",
            "[<afbb0f593c2d2aea5b519cb61da1c17b><",
            "4f9bb2e7978401808f8f1f2a75c322c8>]",
            "/Info 15 0 R/Size 16",
            "/Prev "
        )) + format_int(second_table_offset, &mut int_buffer)
            + " /XRefStm "
            + format_int(first_stream_offset, &mut int_buffer)
            + ">>\n"),
    );
    let last_crossref_offset: i64 = i64::from(last_table_offset);

    let mut parser = MakeParserForBuffer(table);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);
    assert_eq!(DocAvailStatus::DataAvailable, cross_ref_avail.check_avail());
}

#[test]
fn CrossRefStreamIsNotStream() {
    let invalid_xref_stream: &[u8] = concat!(
        "16 0 obj\n",
        "[/array /object]\n",
        "endstream\n",
        "endobj\n",
        "\0"
    )
    .as_bytes();
    let last_crossref_offset: i64 = 0;

    let mut parser = MakeParserForBuffer(invalid_xref_stream);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);
    assert_eq!(DocAvailStatus::DataError, cross_ref_avail.check_avail());
}

#[test]
fn CrossRefTableWithEncryptRef() {
    let xref_table: &[u8] = concat!(
        "xref \n",
        "0 6 \n",
        "0000000003 65535 f \n",
        "0000000017 00000 n \n",
        "0000000081 00000 n \n",
        "0000000000 00007 f \n",
        "0000000331 00000 n \n",
        "0000000409 00000 n \n",
        "trailer\n",
        "<</Root 14 0 R/ID ",
        "[<afbb0f593c2d2aea5b519cb61da1c17b><4f9bb2e7978401808f8f1f2a75c322c8>]",
        "/Encrypt 77 0 R",
        "/Info 15 0 R/Size 16>>",
        "\0"
    )
    .as_bytes();
    let last_crossref_offset: i64 = 0;

    let mut parser = MakeParserForBuffer(xref_table);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);
    assert_eq!(DocAvailStatus::DataError, cross_ref_avail.check_avail());
}

#[test]
fn CrossRefStreamWithEncryptRef() {
    let xref_stream: &[u8] = concat!(
        "16 0 obj\n",
        "<</Filter /FlateDecode /Encrypt 77 0 R>>",
        " stream \n",
        "STREAM DATA STREAM DATA STREAM DATA\n",
        "endstream\n",
        "endobj\n",
        "\0"
    )
    .as_bytes();
    let last_crossref_offset: i64 = 0;

    let mut parser = MakeParserForBuffer(xref_stream);
    let mut cross_ref_avail = CpdfCrossRefAvail::new(&mut parser, last_crossref_offset);
    assert_eq!(DocAvailStatus::DataError, cross_ref_avail.check_avail());
}
