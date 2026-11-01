// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Source-derived archive, flag, ID and callback lifetime regressions for
//! `core/fpdfapi/edit/cpdf_creator.cpp`.
use super::*;
use crate::fpdfapi::parser::cpdf_parser::ParserError;
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
use crate::fxcrt::fx_stream::StreamWithSize;
use std::cell::Cell;
use std::rc::Weak;

#[derive(Default)]
struct Sink {
    bytes: Vec<u8>,
    calls: usize,
    fail_call: Option<usize>,
}
impl WriteStream for Sink {
    fn write_block(&mut self, data: &[u8]) -> bool {
        self.calls += 1;
        if self.fail_call == Some(self.calls) {
            return false;
        }
        self.bytes.extend_from_slice(data);
        true
    }
}
fn document() -> Rc<RefCell<CpdfDocument>> {
    let mut d = CpdfDocument::new();
    d.create_new_doc();
    d.create_new_page(0).unwrap();
    Rc::new(RefCell::new(d))
}
fn save(doc: &Rc<RefCell<CpdfDocument>>) -> Vec<u8> {
    let mut sink = Sink::default();
    {
        let mut c = CpdfCreator::new(Rc::clone(doc), &mut sink);
        assert!(c.create(0, 0));
    }
    sink.bytes
}

#[test]
fn archive_flushes_at_capacity_and_drop_flushes_partial_tail() {
    let mut sink = Sink::default();
    let first = vec![b'a'; ARCHIVE_BUFFER_SIZE - 2];
    {
        let mut a = FileBufferArchive::new(&mut sink);
        assert!(a.write_block(&[]));
        assert_eq!(a.current_offset(), 0);
        assert!(a.write_block(&first));
        assert_eq!(a.used, ARCHIVE_BUFFER_SIZE - 2);
        assert!(a.write_block(b"bcdef"));
        assert_eq!(a.current_offset(), (ARCHIVE_BUFFER_SIZE + 3) as i64);
        assert_eq!(a.used, 3);
        assert!(a.flush());
        assert_eq!(a.used, 0);
        assert!(a.write_block(b"tail"));
    }
    assert_eq!(sink.calls, 3);
    assert_eq!(&sink.bytes[..first.len()], first);
    assert_eq!(&sink.bytes[first.len()..], b"bcdeftail");
}
#[test]
fn archive_failure_resets_buffer_before_callback_and_does_not_advance_offset() {
    let mut sink = Sink {
        fail_call: Some(1),
        ..Sink::default()
    };
    {
        let mut a = FileBufferArchive::new(&mut sink);
        assert!(a.write_block(b"prefix"));
        assert!(!a.write_block(&vec![b'x'; ARCHIVE_BUFFER_SIZE]));
        assert_eq!(a.current_offset(), 6);
        assert_eq!(a.used, 0);
        assert!(a.write_block(b"after"));
        assert_eq!(a.current_offset(), 11);
    }
    assert_eq!(sink.calls, 2);
    assert_eq!(sink.bytes, b"after");
}
#[test]
fn archive_detects_offset_overflow_after_buffering_and_drop_still_flushes() {
    let mut sink = Sink::default();
    {
        let mut a = FileBufferArchive::new(&mut sink);
        a.offset = i64::MAX - 1;
        assert!(!a.write_block(b"ab"));
        assert_eq!(a.current_offset(), i64::MAX - 1);
        assert_eq!(a.used, 2);
        assert!(a.write_block(&[]));
    }
    assert_eq!(sink.bytes, b"ab");
}
#[test]
fn output_index_uses_low_four_offset_bytes_and_checks_each_write() {
    struct Bytes {
        bytes: Vec<u8>,
        fail: usize,
    }
    impl WriteStream for Bytes {
        fn write_block(&mut self, b: &[u8]) -> bool {
            if self.bytes.len() == self.fail {
                return false;
            }
            self.bytes.extend_from_slice(b);
            true
        }
    }
    impl ArchiveStream for Bytes {
        fn current_offset(&self) -> i64 {
            self.bytes.len() as i64
        }
    }
    for count in 0..=5 {
        let mut bytes = Bytes {
            bytes: Vec::new(),
            fail: count,
        };
        assert_eq!(output_index(&mut bytes, 0x7f_12345678), count == 5);
        assert_eq!(bytes.bytes, &[0x12, 0x34, 0x56, 0x78, 0][..count]);
    }
    let mut negative = Bytes {
        bytes: Vec::new(),
        fail: usize::MAX,
    };
    assert!(output_index(&mut negative, -1));
    assert_eq!(negative.bytes, [255, 255, 255, 255, 0]);
}
#[test]
fn flags_and_versions_preserve_normalization_and_new_document_full_save() {
    for flags in [0, 1, 2, 3, 4, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, u32::MAX] {
        let doc = document();
        let mut sink = Sink::default();
        {
            let mut creator = CpdfCreator::new(doc, &mut sink);
            assert!(creator.create(flags, 0));
            let flags = if flags & !15 != 0 { 0 } else { flags };
            assert!(!creator.is_incremental);
            assert_eq!(creator.is_original, flags & 3 != 2);
            assert_eq!(creator.security_changed, flags == 3 || flags & 4 != 0);
            assert_eq!(creator.subset_new_fonts, flags & 8 != 0);
        }
        assert!(sink.bytes.starts_with(b"%PDF-1.7\r\n"));
    }
    for version in [-1, 0, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18] {
        let mut sink = Sink::default();
        {
            let mut c = CpdfCreator::new(document(), &mut sink);
            assert!(c.create(0, version));
        }
        let expected = if (10..=17).contains(&version) {
            version % 10
        } else {
            7
        };
        assert!(sink
            .bytes
            .starts_with(format!("%PDF-1.{}\r\n", expected).as_bytes()));
    }
}
#[test]
fn new_document_ids_are_hex_clones_and_trailer_offsets_match_actual_bytes() {
    let doc = document();
    let mut sink = Sink::default();
    let xref;
    {
        let mut c = CpdfCreator::new(Rc::clone(&doc), &mut sink);
        assert!(!c.has_parser);
        assert!(c.create(0, 0));
        xref = c.xref_start as usize;
        let id = c.id_array.as_ref().unwrap();
        let first = array(id, |a| a.get_object_at(0)).unwrap();
        let second = array(id, |a| a.get_object_at(1)).unwrap();
        assert!(!Rc::ptr_eq(&first, &second));
        assert_eq!(first.borrow().get_string().len(), 16);
        assert_eq!(first.borrow().get_string(), second.borrow().get_string());
        assert!(first
            .borrow()
            .as_any()
            .downcast_ref::<CpdfString>()
            .unwrap()
            .is_hex());
        assert!(second
            .borrow()
            .as_any()
            .downcast_ref::<CpdfString>()
            .unwrap()
            .is_hex());
    }
    assert_eq!(&sink.bytes[xref..xref + 6], b"xref\r\n");
    assert!(sink
        .bytes
        .ends_with(format!("\r\nstartxref\r\n{}\r\n%%EOF\r\n", xref).as_bytes()));
    let mut loaded = CpdfDocument::new();
    assert_eq!(
        loaded.load_doc(Rc::new(CfxReadOnlyContainerStream::new(sink.bytes)), b""),
        ParserError::Success
    );
    assert_eq!(loaded.page_count(), 1);
}
#[test]
fn final_flush_failure_is_ignored_but_full_buffer_failure_reaches_create() {
    let mut small = Sink {
        fail_call: Some(1),
        ..Sink::default()
    };
    {
        let mut c = CpdfCreator::new(document(), &mut small);
        assert!(c.create(0, 0));
    }
    assert_eq!(small.calls, 1);
    assert!(small.bytes.is_empty());
    let doc = document();
    let root = doc.borrow().root().unwrap();
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(
            b"Large",
            CpdfString::from_bytes(None, &vec![b'x'; ARCHIVE_BUFFER_SIZE + 100]),
        );
    let mut large = Sink {
        fail_call: Some(1),
        ..Sink::default()
    };
    {
        let mut c = CpdfCreator::new(doc, &mut large);
        assert!(!c.create(0, 0));
    }
    assert_eq!(large.calls, 1);
    assert!(large.bytes.is_empty());
}
#[test]
fn writer_callback_can_query_document_during_full_buffer_flush_and_drop() {
    struct Query {
        doc: Rc<RefCell<CpdfDocument>>,
        calls: usize,
        bytes: usize,
    }
    impl WriteStream for Query {
        fn write_block(&mut self, b: &[u8]) -> bool {
            assert_eq!(self.doc.borrow().page_count(), 1);
            self.calls += 1;
            self.bytes += b.len();
            true
        }
    }
    let doc = document();
    let root = doc.borrow().root().unwrap();
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(
            b"Large",
            CpdfString::from_bytes(None, &vec![b'q'; ARCHIVE_BUFFER_SIZE + 17]),
        );
    let mut sink = Query {
        doc: Rc::clone(&doc),
        calls: 0,
        bytes: 0,
    };
    {
        let mut c = CpdfCreator::new(doc, &mut sink);
        assert!(c.create(0, 0));
    }
    assert_eq!(sink.calls, 2);
    assert!(sink.bytes > ARCHIVE_BUFFER_SIZE);
}
#[test]
fn original_source_read_callback_can_query_document_between_copy_chunks() {
    struct Source {
        bytes: Vec<u8>,
        doc: RefCell<Weak<RefCell<CpdfDocument>>>,
        queries: Cell<usize>,
        reference: RefCell<Option<ObjectRef>>,
    }
    impl StreamWithSize for Source {
        fn get_size(&self) -> i64 {
            self.bytes.len() as i64
        }
    }
    impl SeekableReadStream for Source {
        fn read_block_at_offset(&self, b: &mut [u8], offset: i64) -> bool {
            if let Some(doc) = self.doc.borrow().upgrade() {
                assert_eq!(doc.borrow_mut().page_count(), 1);
                self.queries.set(self.queries.get() + 1);
                if let Some(reference) = self.reference.borrow().as_ref() {
                    // Native GetDirect evaluates the target before this input
                    // callback, but does not lock the reference object itself.
                    reference.borrow_mut().set_gen_num(0);
                }
            }
            let Ok(start) = usize::try_from(offset) else {
                return false;
            };
            let Some(bytes) = self.bytes.get(start..start + b.len()) else {
                return false;
            };
            b.copy_from_slice(bytes);
            true
        }
    }
    let initial = document();
    let objects = initial.borrow().holder();
    let lazy = objects.borrow_mut().new_indirect_dictionary();
    let number = lazy.borrow().obj_num();
    let root = initial.borrow().root().unwrap();
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"Lazy", CpdfReference::new(&objects, number));
    let original = save(&initial);
    let source = Rc::new(Source {
        bytes: original,
        doc: RefCell::new(Weak::new()),
        queries: Cell::new(0),
        reference: RefCell::new(None),
    });
    let mut doc = CpdfDocument::new();
    assert_eq!(doc.load_doc(source.clone(), b""), ParserError::Success);
    let doc = Rc::new(RefCell::new(doc));
    *source.doc.borrow_mut() = Rc::downgrade(&doc);
    let root = doc.borrow().root().unwrap();
    *source.reference.borrow_mut() = dict(&root, |d| d.get_object_for(b"Lazy"));
    let mut sink = Sink::default();
    {
        let mut c = CpdfCreator::new(Rc::clone(&doc), &mut sink);
        assert!(c.create(CpdfCreator::INCREMENTAL, 0));
    }
    assert!(source.queries.get() > 0);
    assert!(sink.bytes.starts_with(&source.bytes));
    // A fresh lazy object is reached during the old-object reference walk.
    // The input callback can obtain a mutable document borrow in that walk.
    let lazy_queries = source.queries.get();
    let _full = save(&doc);
    assert!(source.queries.get() > lazy_queries);
}
#[test]
fn xref_table_formats_native_signed_int_offset_word() {
    let mut sink = Sink::default();
    {
        let mut c = CpdfCreator::new(document(), &mut sink);
        assert!(c.write_xref_offset(0x1_80000001));
    }
    assert_eq!(sink.bytes, b"-2147483647 00000 n\r\n");
}

#[test]
fn final_writer_callback_runs_before_creator_releases_document() {
    struct Query {
        document: Weak<RefCell<CpdfDocument>>,
        calls: usize,
    }
    impl WriteStream for Query {
        fn write_block(&mut self, _: &[u8]) -> bool {
            assert_eq!(self.document.upgrade().unwrap().borrow().page_count(), 1);
            self.calls += 1;
            true
        }
    }
    let document = document();
    let mut sink = Query {
        document: Rc::downgrade(&document),
        calls: 0,
    };
    {
        let mut creator = CpdfCreator::new(document, &mut sink);
        assert!(creator.create(0, 0));
    }
    assert_eq!(sink.calls, 1);
    assert!(sink.document.upgrade().is_none());
}

#[test]
fn new_trailer_reads_root_and_info_numbers_after_successful_prefix_writes() {
    struct Mutate {
        object: ObjectRef,
        bytes: Vec<u8>,
        calls: usize,
    }
    impl WriteStream for Mutate {
        fn write_block(&mut self, bytes: &[u8]) -> bool {
            if self.calls == 0 {
                self.object.borrow_mut().set_obj_num(9001);
            }
            self.calls += 1;
            self.bytes.extend_from_slice(bytes);
            true
        }
    }
    for target_info in [false, true] {
        let document = document();
        let root = document.borrow().root().unwrap();
        let info = document.borrow_mut().info().unwrap();
        let root_number = root.borrow().obj_num();
        let prefix_size = b"trailer\r\n<<".len()
            + b"\r\n/Root ".len()
            + if target_info {
                root_number.to_string().len() + b" 0 R\r\n/Info ".len()
            } else {
                0
            };
        let mut sink = Mutate {
            object: if target_info { info } else { root },
            bytes: Vec::new(),
            calls: 0,
        };
        {
            let mut creator = CpdfCreator::new(document, &mut sink);
            creator.archive.used = ARCHIVE_BUFFER_SIZE - prefix_size;
            creator.stage = Stage::WriteTrailerAndFinish90;
            assert_eq!(creator.write_doc_stage4(), Stage::Complete100);
        }
        let expected = if target_info {
            b"/Info 9001 0 R".as_slice()
        } else {
            b"/Root 9001 0 R"
        };
        assert!(sink
            .bytes
            .windows(expected.len())
            .any(|part| part == expected));
    }
}
