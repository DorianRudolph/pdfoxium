// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Source-derived save/reload contracts for new, table-xref and stream-xref
//! documents. Native differential loading is tested separately at the C ABI.

use super::*;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_parser::ParserError;
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
use crate::fxcrt::fx_coordinates::CfxFloatRect;

#[derive(Default)]
struct Sink(Vec<u8>);
impl WriteStream for Sink {
    fn write_block(&mut self, bytes: &[u8]) -> bool {
        self.0.extend_from_slice(bytes);
        true
    }
}
fn save(document: &Rc<RefCell<CpdfDocument>>, flags: u32, version: i32) -> Vec<u8> {
    let mut sink = Sink::default();
    {
        let mut creator = CpdfCreator::new(Rc::clone(document), &mut sink);
        assert!(creator.create(flags, version));
    }
    sink.0
}
fn load(bytes: Vec<u8>) -> Rc<RefCell<CpdfDocument>> {
    let mut document = CpdfDocument::new();
    assert_eq!(
        document.load_doc(Rc::new(CfxReadOnlyContainerStream::new(bytes)), b""),
        ParserError::Success
    );
    assert!(!document.parser().xref_table_rebuilt());
    Rc::new(RefCell::new(document))
}

pub(super) fn saved_table_trailer(bytes: &[u8]) -> ObjectRef {
    use crate::fpdfapi::parser::cpdf_parser::{
        load_trailer, parse_cross_ref_table, parse_start_xref,
    };
    use crate::fpdfapi::parser::cpdf_read_validator::CpdfReadValidator;
    use crate::fpdfapi::parser::cpdf_syntax_parser::CpdfSyntaxParser;
    let source = Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec()));
    let mut syntax = CpdfSyntaxParser::new(Rc::new(CpdfReadValidator::new(source, None)), 0);
    let offset = parse_start_xref(&mut syntax);
    assert!(offset > 0);
    syntax.set_pos(offset);
    assert!(parse_cross_ref_table(&mut syntax, None));
    load_trailer(&mut syntax, None).unwrap()
}
fn fixture(filename: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../upstream/pdfium/testing/resources")
            .join(filename),
    )
    .unwrap()
}
fn read_dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
    let object = object.borrow();
    f(object.as_any().downcast_ref::<CpdfDictionary>().unwrap())
}
fn write_dict<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut object = object.borrow_mut();
    f(object
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap())
}
fn stream_bytes(stream: &ObjectRef) -> Vec<u8> {
    let borrowed = stream.borrow();
    let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
    let mut access = CpdfStreamAcc::new(stream);
    access.load_all_data_filtered();
    access.get_span().to_vec()
}
fn install_marker(document: &Rc<RefCell<CpdfDocument>>, marker: &[u8]) {
    let root = document.borrow().root().unwrap();
    write_dict(&root, |root| {
        root.set_new_string_for(b"SaveMarker", marker);
    });
}
fn marker(document: &Rc<RefCell<CpdfDocument>>) -> Vec<u8> {
    read_dict(&document.borrow().root().unwrap(), |root| {
        root.get_byte_string_for(b"SaveMarker", b"")
    })
}

#[test]
fn new_documents_roundtrip_binary_streams_strings_pages_and_requested_version() {
    // Incremental saving has no original file for a new document and must
    // produce a complete independently readable file, as the source specifies.
    for flags in [CpdfCreator::NONE, CpdfCreator::INCREMENTAL] {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let page = document.borrow_mut().create_new_page(0).unwrap();
        write_dict(&page, |d| {
            d.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 340.0, 570.0))
        });
        document.borrow_mut().create_new_page(1).unwrap();
        let binary = b"\0\xffbinary\r\nendstream\n()\\[]<>%";
        let objects = document.borrow().holder();
        let stream = objects.borrow_mut().new_indirect_stream_from_bytes(binary);
        let root = document.borrow().root().unwrap();
        write_dict(&root, |d| {
            d.set_new_for(
                b"BinaryFixture",
                CpdfReference::new(&objects, stream.borrow().obj_num()),
            );
        });
        let info = document.borrow_mut().info().unwrap();
        write_dict(&info, |d| {
            d.set_new_string_for(b"Title", b"escaped (title) \\ and\0NUL");
        });
        install_marker(&document, b"new document");
        let bytes = save(&document, flags, 17);
        assert!(bytes.starts_with(b"%PDF-1.7\r\n"));
        assert!(bytes.ends_with(b"%%EOF\r\n"));
        let reloaded = load(bytes);
        assert_eq!(reloaded.borrow().page_count(), 2);
        assert_eq!(reloaded.borrow().parser().file_version(), 17);
        assert_eq!(marker(&reloaded), b"new document");
        let root = reloaded.borrow().root().unwrap();
        let stream = read_dict(&root, |d| d.get_stream_for(b"BinaryFixture")).unwrap();
        assert_eq!(stream_bytes(&stream), binary);
        let info = reloaded.borrow_mut().info().unwrap();
        assert_eq!(
            read_dict(&info, |d| d.get_byte_string_for(b"Title", b"")),
            b"escaped (title) \\ and\0NUL"
        );
        let page = CpdfPage::from_document(&mut reloaded.borrow_mut(), 0).unwrap();
        assert_eq!((page.width(), page.height()), (340.0, 570.0));
    }
}

#[test]
fn loaded_table_xrefs_save_edits_and_preserve_existing_content_in_both_modes() {
    for flags in [CpdfCreator::NONE, CpdfCreator::INCREMENTAL] {
        let original = fixture("hello_world.pdf");
        let document = load(original.clone());
        assert!(!document.borrow().parser().xref_stream());
        let page = document.borrow_mut().page_dictionary(0).unwrap();
        let content = read_dict(&page, |d| d.get_stream_for(b"Contents")).unwrap();
        let expected_content = stream_bytes(&content);
        assert!(!expected_content.is_empty());
        let new_page = document.borrow_mut().create_new_page(1).unwrap();
        write_dict(&new_page, |d| {
            d.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 72.0, 144.0))
        });
        install_marker(&document, b"edited loaded document");
        let bytes = save(&document, flags, 0);
        if flags == CpdfCreator::INCREMENTAL {
            assert!(bytes.starts_with(&original));
        }
        let reloaded = load(bytes);
        assert_eq!(reloaded.borrow().page_count(), 2);
        assert_eq!(marker(&reloaded), b"edited loaded document");
        let page = reloaded.borrow_mut().page_dictionary(0).unwrap();
        let content = read_dict(&page, |d| d.get_stream_for(b"Contents")).unwrap();
        assert_eq!(stream_bytes(&content), expected_content);
        let mut first_page = CpdfPage::from_document(&mut reloaded.borrow_mut(), 0).unwrap();
        first_page.parse_path_content();
        assert!(first_page.holder().page_object_count() > 0);
        let second_page = CpdfPage::from_document(&mut reloaded.borrow_mut(), 1).unwrap();
        assert_eq!((second_page.width(), second_page.height()), (72.0, 144.0));
        assert!(!reloaded.borrow().parser().xref_stream());
    }
}

#[test]
fn compressed_objects_survive_full_and_incremental_xref_stream_saves() {
    for flags in [CpdfCreator::NONE, CpdfCreator::INCREMENTAL] {
        let original = fixture("bug_1399.pdf");
        let document = load(original.clone());
        assert!(document.borrow().parser().xref_stream());
        let page_count = document.borrow().page_count();
        // Object 2 is compressed in the fixture's object stream. Parse and
        // update it so an incremental save must replace it with a new entry.
        let pages =
            CpdfIndirectObjectHolder::get_or_parse_indirect_object(&document.borrow().holder(), 2)
                .unwrap();
        assert_eq!(read_dict(&pages, |d| d.get_name_for(b"Type")), b"Pages");
        write_dict(&pages, |d| {
            d.set_new_string_for(b"CompressedObjectEdit", b"now written independently");
        });
        install_marker(&document, b"xref-stream edit");
        let bytes = save(&document, flags, 0);
        if flags == CpdfCreator::INCREMENTAL {
            assert!(bytes.starts_with(&original));
        }
        let reloaded = load(bytes);
        assert_eq!(reloaded.borrow().page_count(), page_count);
        assert_eq!(marker(&reloaded), b"xref-stream edit");
        assert_eq!(
            reloaded.borrow().parser().xref_stream(),
            flags == CpdfCreator::INCREMENTAL
        );
        let pages = reloaded.borrow().parser().parse_indirect_object(2).unwrap();
        assert_eq!(
            read_dict(&pages, |d| d
                .get_byte_string_for(b"CompressedObjectEdit", b"")),
            b"now written independently"
        );
        for index in 0..page_count {
            assert!(reloaded.borrow_mut().page_dictionary(index).is_some());
        }
    }
}

#[test]
fn successive_incremental_saves_follow_prev_and_keep_each_original_prefix() {
    let mut previous = fixture("hello_world.pdf");
    for revision in 1..=3 {
        let document = load(previous.clone());
        install_marker(&document, format!("revision {revision}").as_bytes());
        document.borrow_mut().create_new_page(revision).unwrap();
        let saved = save(&document, CpdfCreator::INCREMENTAL, 0);
        assert!(saved.starts_with(&previous));
        // The parser's merged trailer intentionally replaces /Prev while
        // walking older revisions. Check the actual newly serialized trailer.
        let trailer = saved_table_trailer(&saved);
        assert_eq!(
            read_dict(&trailer, |d| d.get_integer_for(b"Prev", 0)),
            document.borrow().parser().last_xref_offset() as i32
        );
        let reloaded = load(saved.clone());
        assert_eq!(reloaded.borrow().page_count(), revision as usize + 1);
        assert_eq!(marker(&reloaded), format!("revision {revision}").as_bytes());
        previous = saved;
    }
}
