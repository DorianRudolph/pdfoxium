// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_parser_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals, non_camel_case_types)]
use super::*;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fxcrt::{
    cfx_fileaccess_stream::CfxFileAccessStream,
    cfx_read_only_container_stream::CfxReadOnlyContainerStream,
    fx_stream::{SeekableReadStream, StreamWithSize},
};

const _: () = {
    // Both native XrefObject boundary cases assert this constant.
    assert!(MAX_OBJECT_NUMBER == 25165824);
    assert!(MAX_OBJECT_NUMBER == 25165824);
};
fn TestFile(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../upstream/pdfium/testing/resources")
        .join(name);
    if path.is_file() {
        path.to_str().unwrap().to_owned()
    } else {
        String::new()
    }
}
struct CPDF_TestParser {
    parser: CpdfParser,
    // Keep the mocked return object alive until parser teardown, like the native fixture.
    _dummy_root: Option<ObjectRef>,
}
impl std::ops::Deref for CPDF_TestParser {
    type Target = CpdfParser;
    fn deref(&self) -> &CpdfParser {
        &self.parser
    }
}
impl std::ops::DerefMut for CPDF_TestParser {
    fn deref_mut(&mut self) -> &mut CpdfParser {
        &mut self.parser
    }
}
impl CPDF_TestParser {
    fn new() -> Self {
        let parser = CpdfParser::new();
        // Native TestObjectsHolder's unconfigured GMock returns null; TryInit returns true.
        parser.holder().borrow_mut().set_parser(None);
        Self {
            parser,
            _dummy_root: None,
        }
    }
    fn ParserXRefTest() -> Self {
        let mut result = Self::new();
        let dummy_root = retain_object(CpdfDictionary::new());
        let weak_root = Rc::downgrade(&dummy_root);
        result
            .holder()
            .borrow_mut()
            .set_parser(Some(Rc::new(move |_| weak_root.upgrade())));
        result._dummy_root = Some(dummy_root);
        result
    }
    fn SetSyntaxParserForTesting(&mut self, file: Rc<dyn SeekableReadStream>, header_offset: i64) {
        let validator = Rc::new(CpdfReadValidator::new(file, None));
        *self.state.validator.borrow_mut() = Some(validator.clone());
        self.state.header_offset.set(header_offset);
        self.syntax = Some(CpdfSyntaxParser::new(validator, header_offset));
    }
    fn InitTestFromFile(&mut self, path: &str) -> bool {
        let Some(stream) = CfxFileAccessStream::create_from_filename(path) else {
            return false;
        };
        self.SetSyntaxParserForTesting(Rc::new(stream), 0);
        true
    }
    fn InitTestFromBufferWithOffset(
        &mut self,
        buffer: impl AsRef<[u8]> + 'static,
        header_offset: i64,
    ) -> bool {
        self.SetSyntaxParserForTesting(
            Rc::new(CfxReadOnlyContainerStream::new(buffer)),
            header_offset,
        );
        true
    }
    fn InitTestFromBuffer(&mut self, buffer: impl AsRef<[u8]> + 'static) -> bool {
        self.InitTestFromBufferWithOffset(buffer, 0)
    }
    fn RebuildCrossRef(&mut self) -> bool {
        rebuild_cross_ref(
            self.parser.syntax.as_mut().unwrap(),
            &mut self.parser.state.cross_ref_table.borrow_mut(),
        )
    }
    fn LoadCrossRefTable(&mut self, pos: i64, skip: bool) -> bool {
        load_cross_ref_table(
            self.parser.syntax.as_mut().unwrap(),
            &mut self.parser.state.cross_ref_table.borrow_mut(),
            pos,
            skip,
        )
    }
    fn ParseStartXRef(&mut self) -> i64 {
        parse_start_xref(self.syntax.as_mut().unwrap())
    }
    fn ParseLinearizedHeader(&mut self) -> Option<CpdfLinearizedHeader> {
        CpdfLinearizedHeader::parse(self.syntax.as_mut().unwrap())
    }
}
fn GetObjInfo(parser: &CpdfParser, obj_num: usize) -> ObjectInfo {
    parser
        .cross_ref_table()
        .get_object_info(obj_num as u32)
        .copied()
        .unwrap_or_default()
}
// Native ObjectInfo equality compares only the active union and generation,
// deliberately ignoring the separate is_object_stream_flag.
fn AssertObjectsInfo(actual: &BTreeMap<u32, ObjectInfo>, expected: &[(u32, ObjectInfo)]) {
    assert_eq!(expected.len(), actual.len());
    for ((number, info), (expected_number, expected_info)) in actual.iter().zip(expected) {
        assert_eq!(expected_number, number);
        assert_eq!(expected_info.object_type, info.object_type);
        assert_eq!(expected_info.gen_num, info.gen_num);
        match info.object_type {
            ObjectType::Free => {}
            ObjectType::Normal => assert_eq!(expected_info.pos, info.pos),
            ObjectType::Compressed => {
                assert_eq!(expected_info.archive_obj_num, info.archive_obj_num);
                assert_eq!(expected_info.archive_obj_index, info.archive_obj_index);
            }
        }
    }
}

#[test]
fn RebuildCrossRefCorrectly() {
    let mut parser = CPDF_TestParser::new();
    let test_file = TestFile("parser_rebuildxref_correct.pdf");
    assert!(!test_file.is_empty());
    assert!(parser.InitTestFromFile(&test_file), "{test_file}");

    assert!(parser.RebuildCrossRef());
    let offsets: [i64; 7] = [0, 15, 61, 154, 296, 374, 450];
    let versions: [u16; 7] = [0, 0, 2, 4, 6, 8, 0];
    for i in 0..offsets.len() {
        assert_eq!(offsets[i], GetObjInfo(&parser, i).pos);
    }
    for i in 0..versions.len() {
        assert_eq!(versions[i], GetObjInfo(&parser, i).gen_num);
    }

    let cross_ref_table = parser.cross_ref_table();
    // cross_ref_table is non-null by return type.
    assert_eq!(0u32, cross_ref_table.trailer_object_number());
}

#[test]
fn RebuildCrossRefFailed() {
    let mut parser = CPDF_TestParser::new();
    let test_file = TestFile("parser_rebuildxref_error_notrailer.pdf");
    assert!(!test_file.is_empty());
    assert!(parser.InitTestFromFile(&test_file), "{test_file}");

    assert!(!parser.RebuildCrossRef());
}

#[test]
fn LoadCrossRefTable() {
    {
        const kXrefTable: &[u8] = b"xref \n0 6 \n0000000003 65535 f \n0000000017 00000 n \n0000000081 00000 n \n0000000000 00007 f \n0000000331 00000 n \n0000000409 00000 n \ntrail\0"; // Needed to end cross ref table reading.
        let kExpected: &[(i64, ObjectType)] = &[
            (0, ObjectType::Free),
            (17, ObjectType::Normal),
            (81, ObjectType::Normal),
            (0, ObjectType::Free),
            (331, ObjectType::Normal),
            (409, ObjectType::Normal),
        ];
        let mut parser = CPDF_TestParser::new();
        assert!(parser.InitTestFromBuffer(kXrefTable));
        assert!(parser.LoadCrossRefTable(0, false));
        for i in 0..kExpected.len() {
            assert_eq!(kExpected[i].0, GetObjInfo(&parser, i).pos);
            assert_eq!(kExpected[i].1, GetObjInfo(&parser, i).object_type);
        }
    }
    {
        const kXrefTable: &[u8] = b"xref \n0 1 \n0000000000 65535 f \n3 1 \n0000025325 00000 n \n8 2 \n0000025518 00002 n \n0000025635 00000 n \n12 1 \n0000025777 00000 n \ntrail\0"; // Needed to end cross ref table reading.
        let kExpected: &[(i64, ObjectType)] = &[
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (25325, ObjectType::Normal),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (25518, ObjectType::Normal),
            (25635, ObjectType::Normal),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (25777, ObjectType::Normal),
        ];
        let mut parser = CPDF_TestParser::new();
        assert!(parser.InitTestFromBuffer(kXrefTable));
        assert!(parser.LoadCrossRefTable(0, false));
        for i in 0..kExpected.len() {
            assert_eq!(kExpected[i].0, GetObjInfo(&parser, i).pos);
            assert_eq!(kExpected[i].1, GetObjInfo(&parser, i).object_type);
        }
    }
    {
        const kXrefTable: &[u8] = b"xref \n0 1 \n0000000000 65535 f \n3 1 \n0000025325 00000 n \n8 2 \n0000000000 65535 f \n0000025635 00000 n \n12 1 \n0000025777 00000 n \ntrail\0"; // Needed to end cross ref table reading.
        let kExpected: &[(i64, ObjectType)] = &[
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (25325, ObjectType::Normal),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (25635, ObjectType::Normal),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (25777, ObjectType::Normal),
        ];
        let mut parser = CPDF_TestParser::new();
        assert!(parser.InitTestFromBuffer(kXrefTable));
        assert!(parser.LoadCrossRefTable(0, false));
        for i in 0..kExpected.len() {
            assert_eq!(kExpected[i].0, GetObjInfo(&parser, i).pos);
            assert_eq!(kExpected[i].1, GetObjInfo(&parser, i).object_type);
        }
    }
    {
        const kXrefTable: &[u8] = b"xref \n0 7 \n0000000002 65535 f \n0000000023 00000 n \n0000000003 65535 f \n0000000004 65535 f \n0000000000 65535 f \n0000000045 00000 n \n0000000179 00000 n \ntrail\0"; // Needed to end cross ref table reading.
        let kExpected: &[(i64, ObjectType)] = &[
            (0, ObjectType::Free),
            (23, ObjectType::Normal),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (0, ObjectType::Free),
            (45, ObjectType::Normal),
            (179, ObjectType::Normal),
        ];
        let mut parser = CPDF_TestParser::new();
        assert!(parser.InitTestFromBuffer(kXrefTable));
        assert!(parser.LoadCrossRefTable(0, false));
        for i in 0..kExpected.len() {
            assert_eq!(kExpected[i].0, GetObjInfo(&parser, i).pos);
            assert_eq!(kExpected[i].1, GetObjInfo(&parser, i).object_type);
        }
    }
    {
        // Regression test for https://crbug.com/945624 - Make sure the parser
        // can correctly handle table sizes that are multiples of the read size,
        // which is 1024.
        let mut xref_table = String::from("xref \n 0 2048 \n");
        xref_table.reserve(41000 - xref_table.len());
        for i in 0..2048 {
            let mut buffer = [0u8; 21];
            let line = format!("{:010} 00000 n \n", i + 1);
            buffer[..line.len()].copy_from_slice(line.as_bytes());
            xref_table.push_str(std::str::from_utf8(&buffer[..line.len()]).unwrap());
        }
        xref_table.push_str("trail"); // Needed to end cross ref table reading.
        let mut parser = CPDF_TestParser::new();
        assert!(parser.InitTestFromBuffer(xref_table));
        assert!(parser.LoadCrossRefTable(0, false));
        for i in 0..2048 {
            assert_eq!(i as i64 + 1, GetObjInfo(&parser, i).pos);
            assert_eq!(ObjectType::Normal, GetObjInfo(&parser, i).object_type);
        }
    }
}

#[test]
fn ParseStartXRef() {
    let mut parser = CPDF_TestParser::new();
    let test_file = TestFile("annotation_stamp_with_ap.pdf");
    assert!(!test_file.is_empty());
    assert!(parser.InitTestFromFile(&test_file), "{test_file}");

    assert_eq!(100940, parser.ParseStartXRef());
    let cross_ref_stream_obj = parser.parse_indirect_object_at(100940, 0);
    assert!(cross_ref_stream_obj.is_some());
    let cross_ref_stream_obj = cross_ref_stream_obj.unwrap();
    assert_eq!(75u32, cross_ref_stream_obj.borrow().obj_num());
}

#[test]
fn ParseStartXRefWithHeaderOffset() {
    const kTestHeaderOffset: i64 = 765;
    let test_file = TestFile("annotation_stamp_with_ap.pdf");
    assert!(!test_file.is_empty());
    let pFileAccess = CfxFileAccessStream::create_from_filename(&test_file);
    assert!(pFileAccess.is_some());
    let pFileAccess = pFileAccess.unwrap();

    let mut data = vec![0u8; (pFileAccess.get_size() + kTestHeaderOffset) as usize];
    assert!(pFileAccess.read_block_at_offset(&mut data[kTestHeaderOffset as usize..], 0));
    let mut parser = CPDF_TestParser::new();
    parser.InitTestFromBufferWithOffset(data, kTestHeaderOffset);

    assert_eq!(100940, parser.ParseStartXRef());
    let cross_ref_stream_obj = parser.parse_indirect_object_at(100940, 0);
    assert!(cross_ref_stream_obj.is_some());
    let cross_ref_stream_obj = cross_ref_stream_obj.unwrap();
    assert_eq!(75u32, cross_ref_stream_obj.borrow().obj_num());
}

#[test]
fn ParseLinearizedWithHeaderOffset() {
    const kTestHeaderOffset: i64 = 765;
    let test_file = TestFile("linearized.pdf");
    assert!(!test_file.is_empty());
    let pFileAccess = CfxFileAccessStream::create_from_filename(&test_file);
    assert!(pFileAccess.is_some());
    let pFileAccess = pFileAccess.unwrap();

    let mut data = vec![0u8; (pFileAccess.get_size() + kTestHeaderOffset) as usize];
    assert!(pFileAccess.read_block_at_offset(&mut data[kTestHeaderOffset as usize..], 0));

    let mut parser = CPDF_TestParser::new();
    parser.InitTestFromBufferWithOffset(data, kTestHeaderOffset);
    assert!(parser.ParseLinearizedHeader().is_some());

    let cross_ref_table = parser.cross_ref_table();
    // cross_ref_table is non-null by return type.
    assert_eq!(0u32, cross_ref_table.trailer_object_number());
}

#[test]
fn BadStartXrefShouldNotBuildCrossRefTable() {
    const kData: &[u8] = b"%PDF1-7 0 obj <</Size 2 /W [0 0 0]\n>>\nstream\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nendstream\nendobj\nstartxref\n6\n%%EOF\n\0";
    let mut parser = CPDF_TestParser::new();
    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::FormatError, parser.start_parse_internal());
    // cross_ref_table is non-null by return type.
    assert_eq!(0usize, parser.cross_ref_table().objects_info().len());
}

#[test]
fn XrefObjectHighestIndex() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // The object number will reach `kMaxObjectNumber`.

    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Index [25165824 1]\n  /Root 1 0 R\n  /Size 25165825\n  /W [1 1 1]\n>>\nstream\n01 00 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";
    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::Success, parser.start_parse_internal());
    assert!(!parser.xref_table_rebuilt());
    // cross_ref_table is non-null by return type.
    let cross_ref_table = parser.cross_ref_table();
    let objects_info = cross_ref_table.objects_info();

    let only_valid_object = ObjectInfo {
        object_type: ObjectType::Normal,
        pos: 0,
        ..ObjectInfo::default()
    };
    AssertObjectsInfo(objects_info, &[(25165824, only_valid_object)]);
}

#[test]
fn XrefObjectIndicesTooBig() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // Since /Index starts at 25165824, the object number will go past
    // `kMaxObjectNumber`.

    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Index [25165824 2]\n  /Root 1 0 R\n  /Size 25165826\n  /W [1 1 1]\n>>\nstream\n01 00 00\n01 0F 00\n01 12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";
    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::FormatError, parser.start_parse_internal());
}

#[test]
fn XrefHasInvalidArchiveObjectNumber() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // 0xFF in the first object in the xref object stream is invalid.
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 3\n  /W [1 1 1]\n>>\nstream\n02 FF 00\n01 0F 00\n01 12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";
    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::Success, parser.start_parse_internal());
    assert!(!parser.xref_table_rebuilt());

    let cross_ref_table = parser.cross_ref_table();
    // cross_ref_table is non-null by return type.
    assert_eq!(7u32, cross_ref_table.trailer_object_number());
    let objects_info = cross_ref_table.objects_info();

    // The expectation is for the parser to skip over the first object, and
    // continue parsing the remaining objects. So these are the second and third
    // objects.
    let expected_objects: [ObjectInfo; 2] = [
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 15,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 18,
            ..ObjectInfo::default()
        },
    ];

    AssertObjectsInfo(
        objects_info,
        &[(1, expected_objects[0]), (2, expected_objects[1])],
    );
}

#[test]
fn XrefHasInvalidObjectType() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // The XRef object is a dictionary and not a stream.
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 3\n  /W [1 1 1]\n>>\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::FormatError, parser.start_parse_internal());
}

#[test]
fn XrefHasInvalidPrevValue() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // The /Prev value is an absolute offset, so it should never be negative.
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 3\n  /W [1 1 1]\n  /Prev -1\n>>\nstream\n02 FF 00\n01 0F 00\n01 12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::FormatError, parser.start_parse_internal());
}

#[test]
fn XrefHasInvalidSizeValue() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // The /Size value should never be negative.
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 3\n  /W [1 1 1]\n  /Size -1\n>>\nstream\n02 FF 00\n01 0F 00\n01 12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::FormatError, parser.start_parse_internal());
}

#[test]
fn XrefHasZeroSizeValue() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 0\n  /W [1 1 1]\n  /Size 0\n>>\nstream\n02 FF 00\n01 0F 00\n01 12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::Success, parser.start_parse_internal());
    assert!(!parser.xref_table_rebuilt());
    // cross_ref_table is non-null by return type.
    let cross_ref_table = parser.cross_ref_table();
    let objects_info = cross_ref_table.objects_info();
    assert!(objects_info.is_empty());
}

#[test]
fn XrefHasInvalidWidth() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // The /W array needs to have at least 3 values.
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 3\n  /W [1 1]\n>>\nstream\n02 FF 00\n01 0F 00\n01 12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));

    // StartParseInternal() succeeded not because XRef parsing succeeded, but
    // because RebuildCrossRef() got lucky with the data stream. Therefore, don't
    // bother checking the garbage output.
    assert_eq!(ParserError::Success, parser.start_parse_internal());
    assert!(parser.xref_table_rebuilt());
}

#[test]
fn XrefFirstWidthEntryIsZero() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // When the first /W array entry is 0, it implies the objects are all of the
    // normal type.
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 2\n  /W [0 1 1]\n>>\nstream\n0F 00\n12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::Success, parser.start_parse_internal());
    assert!(!parser.xref_table_rebuilt());
    // cross_ref_table is non-null by return type.
    let cross_ref_table = parser.cross_ref_table();
    let objects_info = cross_ref_table.objects_info();

    let expected_result: [ObjectInfo; 2] = [
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 15,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 18,
            ..ObjectInfo::default()
        },
    ];

    AssertObjectsInfo(
        objects_info,
        &[(0, expected_result[0]), (1, expected_result[1])],
    );
}

#[test]
fn XrefWithValidIndex() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // The /Index specifies objects (2), (4, 5), (80, 81, 82).
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 83\n  /Index [2 1 4 2 80 3]\n  /W [1 1 1]\n>>\nstream\n01 00 00\n01 0F 00\n01 12 00\n01 20 00\n01 22 00\n01 25 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::Success, parser.start_parse_internal());
    assert!(!parser.xref_table_rebuilt());
    // cross_ref_table is non-null by return type.
    let cross_ref_table = parser.cross_ref_table();
    let objects_info = cross_ref_table.objects_info();

    let expected_result: [ObjectInfo; 6] = [
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 0,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 15,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 18,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 32,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 34,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 37,
            ..ObjectInfo::default()
        },
    ];

    AssertObjectsInfo(
        objects_info,
        &[
            (2, expected_result[0]),
            (4, expected_result[1]),
            (5, expected_result[2]),
            (80, expected_result[3]),
            (81, expected_result[4]),
            (82, expected_result[5]),
        ],
    );
}

#[test]
fn XrefIndexWithRepeatedObject() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // The /Index specifies objects (2, 3), (3). AKA the sub-sections overlap.
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 4\n  /Index [2 2 3 1]\n  /W [1 1 1]\n>>\nstream\n01 00 00\n01 0F 00\n01 12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::Success, parser.start_parse_internal());
    assert!(!parser.xref_table_rebuilt());
    // cross_ref_table is non-null by return type.
    let cross_ref_table = parser.cross_ref_table();
    let objects_info = cross_ref_table.objects_info();

    let expected_result: [ObjectInfo; 2] = [
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 0,
            ..ObjectInfo::default()
        },
        // Since the /Index does not follow the spec, this is one of the 2
        // possible values that a parser can come up with.
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 18,
            ..ObjectInfo::default()
        },
    ];

    AssertObjectsInfo(
        objects_info,
        &[(2, expected_result[0]), (3, expected_result[1])],
    );
}

#[test]
fn XrefIndexWithOutOfOrderObjects() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // The /Index specifies objects (3, 4), (2), which is not in ascending order.
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 5\n  /Index [3 2 2 1]\n  /W [1 1 1]\n>>\nstream\n01 00 00\n01 0F 00\n01 12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::Success, parser.start_parse_internal());
    assert!(!parser.xref_table_rebuilt());
    // cross_ref_table is non-null by return type.
    let cross_ref_table = parser.cross_ref_table();
    let objects_info = cross_ref_table.objects_info();

    // Although the /Index does not follow the spec, the parser tolerates it.
    let expected_result: [ObjectInfo; 3] = [
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 18,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 0,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 15,
            ..ObjectInfo::default()
        },
    ];

    AssertObjectsInfo(
        objects_info,
        &[
            (2, expected_result[0]),
            (3, expected_result[1]),
            (4, expected_result[2]),
        ],
    );
}

#[test]
fn XrefWithIndexAndWrongSize() {
    let mut parser = CPDF_TestParser::ParserXRefTest();

    // The /Index specifies objects (2), (80, 81), so the /Size should be 82,
    // but is actually 81.
    const kData: &[u8] = b"%PDF1-7\n%\xa0\xf2\xa4\xf4\n7 0 obj <<\n  /Filter /ASCIIHexDecode\n  /Root 1 0 R\n  /Size 81\n  /Index [2 1 80 2]\n  /W [1 1 1]\n>>\nstream\n01 00 00\n01 0F 00\n01 12 00\nendstream\nendobj\nstartxref\n14\n%%EOF\n\0";

    assert!(parser.InitTestFromBuffer(kData));
    assert_eq!(ParserError::Success, parser.start_parse_internal());
    assert!(!parser.xref_table_rebuilt());
    // cross_ref_table is non-null by return type.
    let cross_ref_table = parser.cross_ref_table();
    let objects_info = cross_ref_table.objects_info();

    let expected_result: [ObjectInfo; 3] = [
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 0,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 15,
            ..ObjectInfo::default()
        },
        ObjectInfo {
            object_type: ObjectType::Normal,
            pos: 18,
            ..ObjectInfo::default()
        },
    ];

    AssertObjectsInfo(
        objects_info,
        &[
            (2, expected_result[0]),
            (80, expected_result[1]),
            (81, expected_result[2]),
        ],
    );
}
