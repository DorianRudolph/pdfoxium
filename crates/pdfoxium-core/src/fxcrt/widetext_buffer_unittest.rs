// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! All four native `widetext_buffer_unittest.cpp` cases.
use super::WideTextBuffer;
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
#[test]
fn empty_buf() {
    let buffer = WideTextBuffer::new();
    assert!(buffer.get_wide_span().is_empty());
    assert!(buffer.as_string_view().is_empty());
    assert!(buffer.make_string().is_empty());
}
#[test]
fn operator_lt_lt() {
    let mut buffer = WideTextBuffer::new();
    buffer.append_ascii(b"clams");
    buffer.append_wide_string(&wide("\u{208c}\u{208e}"));
    assert_eq!(buffer.make_string(), wide("clams\u{208c}\u{208e}"));
}
#[test]
fn deletion() {
    let mut buffer = WideTextBuffer::new();
    buffer.append_wide_string(&wide("ABCDEFG"));
    assert_eq!(buffer.as_string_view(), wide("ABCDEFG"));
    buffer.delete(1, 3);
    assert_eq!(buffer.as_string_view(), wide("AEFG"));
    buffer.delete(1, 0);
    assert_eq!(buffer.as_string_view(), wide("AEFG"));
    buffer.delete(0, 2);
    assert_eq!(buffer.as_string_view(), wide("FG"));
    buffer.delete(0, 2);
    assert!(buffer.as_string_view().is_empty());
    buffer.delete(0, 0);
    assert!(buffer.as_string_view().is_empty());
}
#[test]
fn move_buffer() {
    let mut buffer = WideTextBuffer::new();
    buffer.append_ascii(b"clams");
    assert_eq!(buffer.make_string(), wide("clams"));
    let mut second = std::mem::take(&mut buffer);
    assert!(buffer.make_string().is_empty());
    assert_eq!(second.make_string(), wide("clams"));
    let third = std::mem::take(&mut second);
    assert!(second.make_string().is_empty());
    assert_eq!(third.make_string(), wide("clams"));
}
#[test]
fn typed_spans_checked_deletion_and_detachment_preserve_full_units() {
    let mut buffer = WideTextBuffer::new();
    buffer.estimate_size(5);
    buffer.append_wide_string(&[0, 0x1f600, 0xffffffff]);
    assert_eq!(buffer.get_size(), 12);
    assert_eq!(buffer.get_length(), 3);
    buffer.get_wide_span_mut()[0] = 65;
    buffer.delete(usize::MAX, 1);
    buffer.delete(1, 3);
    assert_eq!(buffer.as_string_view(), [65, 0x1f600, 0xffffffff]);
    assert_eq!(buffer.detach_buffer(), [65, 0x1f600, 0xffffffff]);
    assert!(buffer.is_empty());
    buffer.append_char(9);
    assert_eq!(buffer.into_string(), [9]);
}
