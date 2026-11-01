// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Source-derived `CPDF_Parser::WriteToArchive` block and failure contracts.
use super::*;

struct Input {
    bytes: Vec<u8>,
    fail_at: Cell<Option<usize>>,
    reads: RefCell<Vec<(usize, usize)>>,
}
impl StreamWithSize for Input {
    fn get_size(&self) -> i64 {
        self.bytes.len() as i64
    }
}
impl SeekableReadStream for Input {
    fn read_block_at_offset(&self, out: &mut [u8], offset: i64) -> bool {
        let Ok(start) = usize::try_from(offset) else {
            return false;
        };
        self.reads.borrow_mut().push((start, out.len()));
        if self.fail_at.get().is_some_and(|at| start >= at) {
            return false;
        }
        let Some(bytes) = self
            .bytes
            .get(start..)
            .and_then(|rest| rest.get(..out.len()))
        else {
            return false;
        };
        out.copy_from_slice(bytes);
        true
    }
}
#[derive(Default)]
struct Output {
    bytes: Vec<u8>,
    attempts: Vec<usize>,
    fail_on: Option<usize>,
}
impl WriteStream for Output {
    fn write_block(&mut self, bytes: &[u8]) -> bool {
        self.attempts.push(bytes.len());
        if self.fail_on == Some(self.attempts.len()) {
            return false;
        }
        self.bytes.extend_from_slice(bytes);
        true
    }
}
fn pattern(size: usize) -> Vec<u8> {
    (0..size).map(|n| (n % 251) as u8).collect()
}
fn input(bytes: Vec<u8>) -> Rc<Input> {
    Rc::new(Input {
        bytes,
        fail_at: Cell::new(None),
        reads: RefCell::new(Vec::new()),
    })
}
fn syntax(source: &Rc<Input>, offset: i64) -> CpdfSyntaxParser {
    CpdfSyntaxParser::new(
        Rc::new(CpdfReadValidator::new(source.clone(), None)),
        offset,
    )
}

#[test]
fn archive_copy_keeps_exact_bytes_at_every_block_boundary() {
    for size in [0, 1, 4095, 4096, 4097, 8192, 8193] {
        let source = input(pattern(size));
        let mut parser = syntax(&source, 0);
        let mut output = Output::default();
        assert!(write_to_archive(&mut parser, &mut output, size as i64));
        assert_eq!(output.bytes, source.bytes, "length {size}");
        let mut expected = vec![4096; size / 4096];
        if size % 4096 != 0 {
            expected.push(size % 4096);
        }
        assert_eq!(output.attempts, expected, "length {size}");
        assert_eq!(parser.get_pos(), size as i64);
        assert_eq!(source.reads.borrow().len(), output.attempts.len());
    }
}

#[test]
fn archive_copy_always_restarts_at_the_pdf_header() {
    let source = input(pattern(9000));
    let mut parser = syntax(&source, 0);
    parser.set_pos(7000);
    let mut output = Output::default();
    assert!(write_to_archive(&mut parser, &mut output, 777));
    assert_eq!(output.bytes, source.bytes[..777]);
    assert_eq!(&*source.reads.borrow(), &[(0, 777)]);
    assert_eq!(parser.get_pos(), 777);
}

#[test]
fn archive_copy_excludes_a_transport_prefix_at_the_header_offset() {
    let body = pattern(4097);
    let mut bytes = b"transport-prefix---".to_vec();
    let offset = bytes.len();
    bytes.extend_from_slice(&body);
    let source = input(bytes);
    let mut parser = syntax(&source, offset as i64);
    let mut output = Output::default();
    assert!(write_to_archive(
        &mut parser,
        &mut output,
        body.len() as i64
    ));
    assert_eq!(output.bytes, body);
    assert_eq!(
        &*source.reads.borrow(),
        &[(offset, 4096), (offset + 4096, 1)]
    );
    assert_eq!(parser.get_pos(), 4097);
}

#[test]
fn a_negative_copy_size_preserves_the_cursor_and_performs_no_io() {
    let source = input(pattern(100));
    let mut parser = syntax(&source, 0);
    parser.set_pos(17);
    let mut output = Output::default();
    assert!(!write_to_archive(&mut parser, &mut output, -1));
    assert_eq!(parser.get_pos(), 17);
    assert!(source.reads.borrow().is_empty());
    assert!(output.attempts.is_empty());
}

#[test]
fn a_rejected_first_write_stops_after_one_read_and_no_published_bytes() {
    let source = input(pattern(9000));
    let mut parser = syntax(&source, 0);
    let mut output = Output {
        fail_on: Some(1),
        ..Output::default()
    };
    assert!(!write_to_archive(&mut parser, &mut output, 9000));
    assert!(output.bytes.is_empty());
    assert_eq!(output.attempts, [4096]);
    assert_eq!(&*source.reads.borrow(), &[(0, 4096)]);
    assert_eq!(parser.get_pos(), 4096);
}

#[test]
fn a_midstream_write_failure_retains_only_the_committed_prefix_and_can_retry() {
    let source = input(pattern(9000));
    let mut parser = syntax(&source, 0);
    let mut failed = Output {
        fail_on: Some(2),
        ..Output::default()
    };
    assert!(!write_to_archive(&mut parser, &mut failed, 9000));
    assert_eq!(failed.bytes, source.bytes[..4096]);
    assert_eq!(failed.attempts, [4096, 4096]);
    assert_eq!(parser.get_pos(), 8192);
    let mut retry = Output::default();
    assert!(write_to_archive(&mut parser, &mut retry, 9000));
    assert_eq!(retry.bytes, source.bytes);
    assert_eq!(retry.attempts, [4096, 4096, 808]);
}

#[test]
fn a_first_read_failure_never_calls_the_archive() {
    let source = input(pattern(9000));
    source.fail_at.set(Some(0));
    let mut parser = syntax(&source, 0);
    let mut output = Output::default();
    assert!(!write_to_archive(&mut parser, &mut output, 9000));
    assert!(output.attempts.is_empty());
    assert_eq!(parser.get_pos(), 0);
    assert_eq!(&*source.reads.borrow(), &[(0, 4096)]);
}

#[test]
fn a_midstream_read_failure_never_publishes_a_stale_buffer_or_reads_a_later_block() {
    let source = input(pattern(9000));
    source.fail_at.set(Some(4096));
    let mut parser = syntax(&source, 0);
    let mut output = Output::default();
    assert!(!write_to_archive(&mut parser, &mut output, 9000));
    assert_eq!(output.bytes, source.bytes[..4096]);
    assert_eq!(output.attempts, [4096]);
    assert_eq!(&*source.reads.borrow(), &[(0, 4096), (4096, 4096)]);
    assert_eq!(parser.get_pos(), 4096);
}

#[test]
fn a_requested_tail_beyond_the_file_cannot_publish_unread_bytes() {
    let source = input(pattern(5000));
    let mut parser = syntax(&source, 0);
    let mut output = Output::default();
    assert!(!write_to_archive(&mut parser, &mut output, 8193));
    assert_eq!(output.bytes, source.bytes[..4096]);
    assert_eq!(output.attempts, [4096]);
    assert_eq!(parser.get_pos(), 4096);
    assert!(source
        .reads
        .borrow()
        .iter()
        .all(|&(offset, count)| offset + count <= source.bytes.len()));
}
