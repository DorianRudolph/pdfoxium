// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/span_io_unittest.cpp portable and POSIX cases. File implements
//! the selected safe read/write primitives; StdFileAccess exercises actual
//! production reads in the native FD cases. Fixtures preserve terminating NUL,
//! buffer lengths, writes, rewinds and unlink/close lifetimes.
#![allow(non_snake_case, non_upper_case_globals)]

use super::fileaccess_iface::{FileAccessIface, StdFileAccess};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct TempFile {
    file: File,
    path: PathBuf,
}
impl TempFile {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "pdfoxium-span-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        Self { file, path }
    }
    fn rewind(&mut self) {
        self.file.seek(SeekFrom::Start(0)).unwrap();
    }
}
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
fn spanread<'a>(buffer: &'a mut [u8], file: &mut File) -> &'a [u8] {
    let count = file.read(buffer).unwrap_or(0);
    &buffer[..count]
}
fn assert_cstr_eq(left: &[u8], right: &[u8]) {
    let end = left.iter().position(|&c| c == 0).unwrap();
    let right_end = right.iter().position(|&c| c == 0).unwrap();
    assert_eq!(&left[..end], &right[..right_end]);
}
#[test]
fn Spanread_ReadNormal() {
    let mut scoped_f = TempFile::new();
    let kData = b"hello world\0";
    assert_eq!(scoped_f.file.write(kData).unwrap(), kData.len());
    scoped_f.rewind();
    let mut buffer = [0u8; 100];
    let read_span = spanread(&mut buffer, &mut scoped_f.file);
    assert_eq!(read_span.len(), kData.len());
    assert_cstr_eq(read_span, kData);
}
#[test]
fn Spanread_ReadTruncated() {
    let mut scoped_f = TempFile::new();
    let kData = b"hello world\0";
    assert_eq!(scoped_f.file.write(kData).unwrap(), kData.len());
    scoped_f.rewind();
    let mut buffer = [0u8; 5];
    let read_span = spanread(&mut buffer, &mut scoped_f.file);
    assert_eq!(read_span.len(), 5usize);
    assert_eq!(read_span, b"hello");
}
#[test]
fn Spanread_ReadEmpty() {
    let mut scoped_f = TempFile::new();
    let empty_span: &mut [u8] = &mut [];
    let read_span = spanread(empty_span, &mut scoped_f.file);
    assert!(read_span.is_empty());
}
#[test]
fn Spanwrite_WriteNormal() {
    let mut scoped_f = TempFile::new();
    let kData = b"hello world\0";
    assert_eq!(scoped_f.file.write(kData).unwrap(), kData.len());
    scoped_f.rewind();
    let mut buffer = [0u8; 100];
    let read = scoped_f.file.read(&mut buffer[..kData.len()]).unwrap();
    assert_eq!(read, kData.len());
    assert_cstr_eq(&buffer, kData);
}
#[test]
fn Spanwrite_WriteEmpty() {
    let mut scoped_f = TempFile::new();
    let empty_span: &[u8] = &[];
    assert_eq!(scoped_f.file.write(empty_span).unwrap(), 0usize);
}
#[test]
fn SpanreadFD_ReadNormal() {
    let mut scoped_fd = TempFile::new();
    let kData = b"hello world\0";
    assert_eq!(scoped_fd.file.write(kData).unwrap(), kData.len());
    scoped_fd.rewind();
    let mut fd = StdFileAccess::new();
    assert!(fd.open_path(&scoped_fd.path));
    assert_eq!(fd.set_position(0), 0);
    let mut buffer = [0u8; 100];
    let count = fd.read(&mut buffer);
    let read_span = &buffer[..count];
    assert_eq!(read_span.len(), kData.len());
    assert_cstr_eq(read_span, kData);
    std::fs::remove_file(&scoped_fd.path).unwrap();
}
#[test]
fn SpanreadFD_ReadEmpty() {
    let scoped_fd = TempFile::new();
    let mut fd = StdFileAccess::new();
    assert!(fd.open_path(&scoped_fd.path));
    let empty_span: &mut [u8] = &mut [];
    let count = fd.read(empty_span);
    let read_span = &empty_span[..count];
    assert!(read_span.is_empty());
    std::fs::remove_file(&scoped_fd.path).unwrap();
}
#[test]
fn SpanwriteFD_WriteNormal() {
    let mut scoped_fd = TempFile::new();
    let kData = b"hello world\0";
    assert_eq!(scoped_fd.file.write(kData).unwrap(), kData.len());
    scoped_fd.rewind();
    let mut buffer = [0u8; 100];
    let bytes_read = scoped_fd.file.read(&mut buffer[..kData.len()]).unwrap();
    assert_eq!(bytes_read, kData.len());
    assert_cstr_eq(&buffer, kData);
    std::fs::remove_file(&scoped_fd.path).unwrap();
}
#[test]
fn SpanwriteFD_WriteEmpty() {
    let mut scoped_fd = TempFile::new();
    let empty_span: &[u8] = &[];
    assert_eq!(scoped_fd.file.write(empty_span).unwrap(), 0usize);
    std::fs::remove_file(&scoped_fd.path).unwrap();
}
