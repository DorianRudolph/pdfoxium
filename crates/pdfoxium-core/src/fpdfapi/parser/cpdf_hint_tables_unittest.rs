// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! All three cases of `cpdf_hint_tables_unittest.cpp`, retaining native fixtures
//! and every expected page/group field.
use super::*;
use crate::fpdfapi::parser::cpdf_data_avail::CpdfDataAvail;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../upstream/pdfium/testing/resources/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn available() -> CpdfDataAvail {
    let mut result = CpdfDataAvail::new(
        None,
        Rc::new(CfxReadOnlyContainerStream::new(fixture(
            "feature_linearized_loading.pdf",
        ))),
    );
    assert_eq!(result.is_doc_avail(None), DocAvailStatus::DataAvailable);
    assert!(result.hint_tables().is_some());
    result
}
#[test]
fn load() {
    let avail = available();
    let table = avail.hint_tables().unwrap();
    assert_eq!(table.get_page_pos(0), Some((777, 4328, 39)));
    assert_eq!(table.get_page_pos(1), Some((5105, 767, 1)));
    assert_eq!(table.get_page_pos(2), None);
}
#[test]
fn page_and_group_infos() {
    let avail = available();
    let table = avail.hint_tables().unwrap();
    let pages = table.page_infos();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].objects_count(), 5);
    assert_eq!(pages[0].page_offset(), 777);
    assert_eq!(pages[0].page_length(), 4328);
    assert_eq!(pages[0].start_obj_num(), 39);
    assert_eq!(pages[0].identifiers().len(), 2);
    assert_eq!(pages[0].identifiers(), [0, 0]);
    assert_eq!(pages[1].objects_count(), 3);
    assert_eq!(pages[1].page_offset(), 5105);
    assert_eq!(pages[1].page_length(), 767);
    assert_eq!(pages[1].start_obj_num(), 1);
    assert_eq!(pages[1].identifiers().len(), 3);
    assert_eq!(pages[1].identifiers(), [2, 5, 3]);
    let expected = [
        (777, 254, 39, 1),
        (1031, 389, 40, 1),
        (1420, 726, 41, 1),
        (2146, 290, 42, 1),
        (2436, 2669, 43, 1),
        (10939, 544, 4, 1),
    ];
    assert_eq!(table.shared_group_infos().len(), expected.len());
    for (group, (offset, length, start, count)) in table.shared_group_infos().iter().zip(expected) {
        assert_eq!(group.offset, offset);
        assert_eq!(group.length, length);
        assert_eq!(group.start_obj_num, start);
        assert_eq!(group.objects_count, count);
    }
}
#[test]
fn first_page_offset() {
    let mut syntax = CpdfSyntaxParser::from_file(Rc::new(CfxReadOnlyContainerStream::new(
        b"<< /Linearized 1 /L 19326762 /H [ 123730 3816 ] /O 5932 /E 639518 /N 102 /T 19220281 >>"
            .to_vec(),
    )));
    let dict = syntax.get_object_body(None).unwrap();
    let dict = dict.borrow();
    let header =
        CpdfLinearizedHeader::from_dict(dict.as_any().downcast_ref::<CpdfDictionary>().unwrap(), 0);
    let validator = Rc::new(CpdfReadValidator::new(
        Rc::new(CfxReadOnlyContainerStream::new(fixture(
            "hint_table_102p.bin",
        ))),
        None,
    ));
    let mut syntax = CpdfSyntaxParser::new(validator.clone(), 0);
    let stream = syntax.get_object_body(None).unwrap();
    let stream = stream.borrow();
    let stream = stream.as_any().downcast_ref::<CpdfStream>().unwrap();
    let mut table = CpdfHintTables::new(validator, &header);
    assert!(table.load_hint_stream(stream));
    assert_eq!(table.first_page_obj_offset(), 127546);
}

#[test]
fn truncated_shared_flags_and_object_counts_are_rejected() {
    let mut parser = CpdfSyntaxParser::from_file(Rc::new(CfxReadOnlyContainerStream::new(
        b"<< /Linearized 1 /L 1000 /H [100 50] /O 1 /E 500 /N 2 /T 900 >>".to_vec(),
    )));
    let dict = parser.get_object_body(None).unwrap();
    let dict = dict.borrow();
    let header =
        CpdfLinearizedHeader::from_dict(dict.as_any().downcast_ref::<CpdfDictionary>().unwrap(), 0);
    let validator = Rc::new(CpdfReadValidator::new(
        Rc::new(CfxReadOnlyContainerStream::new(Vec::new())),
        None,
    ));
    let mut data = vec![0u8; 27];
    let mut position = 0;
    for (value, width) in [
        (1u32, 32),
        (1, 32),
        (1, 32),
        (1, 32),
        (1, 16),
        (1, 32),
        (1, 16),
    ] {
        for bit in (0..width).rev() {
            data[position / 8] |= (((value >> bit) & 1) as u8) << (7 - position % 8);
            position += 1;
        }
    }
    // Group length delta, then byte-aligned signature flag, then count delta.
    data[26] = 0x80;
    let mut table = CpdfHintTables::new(validator.clone(), &header);
    table.first_page_obj_offset = 10;
    assert!(table.read_shared_obj_hint_table(&mut CfxBitStream::new(&data), 0));
    assert_eq!(table.shared_group_infos()[0].objects_count, 2);
    for size in 0..data.len() {
        let mut table = CpdfHintTables::new(validator.clone(), &header);
        table.first_page_obj_offset = 10;
        assert!(
            !table.read_shared_obj_hint_table(&mut CfxBitStream::new(&data[..size]), 0),
            "accepted truncated shared table of {size} bytes"
        );
    }
}
