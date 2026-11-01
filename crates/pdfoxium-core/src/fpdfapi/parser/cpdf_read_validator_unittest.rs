// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_read_validator_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

const kTestDataSize: i64 = 64 * 1024 - 467;
fn MakeRange(start: i64, end: i64) -> (i64, i64) {
    (start, end)
}
#[derive(Default)]
struct MockFileAvail {
    available_range: Cell<(i64, i64)>,
}
impl MockFileAvail {
    fn SetAvailableRange(&self, range: (i64, i64)) {
        self.available_range.set(range);
    }
}
impl FileAvail for MockFileAvail {
    fn is_data_avail(&self, offset: i64, size: usize) -> bool {
        let range = self.available_range.get();
        range.0 <= offset && range.1 >= offset + size as i64
    }
}
#[derive(Clone, Default)]
struct MockDownloadHints {
    last_requested_range: Rc<Cell<(i64, i64)>>,
}
impl MockDownloadHints {
    fn GetLastRequstedRange(&self) -> (i64, i64) {
        self.last_requested_range.get()
    }
    fn Reset(&self) {
        self.last_requested_range.set(MakeRange(0, 0));
    }
}
impl DownloadHints for MockDownloadHints {
    fn add_segment(&mut self, offset: i64, size: usize) {
        self.last_requested_range
            .set((offset, offset + size as i64));
    }
}
// testing/invalid_seekable_read_stream.{h,cpp}: reads always fail.
struct InvalidSeekableReadStream {
    data_size: i64,
}
impl StreamWithSize for InvalidSeekableReadStream {
    fn get_size(&self) -> i64 {
        self.data_size
    }
}
impl SeekableReadStream for InvalidSeekableReadStream {
    fn read_block_at_offset(&self, _: &mut [u8], _: i64) -> bool {
        false
    }
}

#[test]
fn UnavailableData() {
    let test_data = vec![0u8; kTestDataSize as usize];
    let file = Rc::new(CfxReadOnlyContainerStream::new(test_data));
    let file_avail = Rc::new(MockFileAvail::default());
    let file_avail_iface: Rc<dyn FileAvail> = file_avail.clone();
    let validator = Rc::new(CpdfReadValidator::new(file, Some(&file_avail_iface)));

    let mut read_buffer = vec![0u8; 100 as usize];
    assert!(!validator.read_block_at_offset(&mut read_buffer, 5000));
    assert!(!validator.read_error());
    assert!(validator.has_unavailable_data());

    validator.reset_errors();
    file_avail.SetAvailableRange(MakeRange(5000, 5000 + read_buffer.len() as i64));
    assert!(validator.read_block_at_offset(&mut read_buffer, 5000));
    assert!(!validator.read_error());
    assert!(!validator.has_unavailable_data());
}

#[test]
fn UnavailableDataWithHints() {
    let test_data = vec![0u8; kTestDataSize as usize];
    let file = Rc::new(CfxReadOnlyContainerStream::new(test_data));
    let file_avail = Rc::new(MockFileAvail::default());
    let file_avail_iface: Rc<dyn FileAvail> = file_avail.clone();
    let validator = Rc::new(CpdfReadValidator::new(file, Some(&file_avail_iface)));

    let hints = MockDownloadHints::default();
    let hints_iface: Rc<RefCell<dyn DownloadHints>> = Rc::new(RefCell::new(hints.clone()));
    validator.set_download_hints(Some(&hints_iface));

    let mut read_buffer = vec![0u8; 100 as usize];
    assert!(!validator.read_block_at_offset(&mut read_buffer, 5000));
    assert!(!validator.read_error());
    assert!(validator.has_unavailable_data());

    // Requested range should be enlarged and aligned.
    assert_eq!(MakeRange(4608, 5120), hints.GetLastRequstedRange());

    file_avail.SetAvailableRange(hints.GetLastRequstedRange());
    hints.Reset();

    validator.reset_errors();
    assert!(validator.read_block_at_offset(&mut read_buffer, 5000));
    // No new request on already available data.
    assert_eq!(MakeRange(0, 0), hints.GetLastRequstedRange());
    assert!(!validator.read_error());
    assert!(!validator.has_unavailable_data());

    validator.reset_errors();
    // Try read unavailable data at file end.
    let offset = validator.get_size() - read_buffer.len() as i64;
    assert!(!validator.read_block_at_offset(&mut read_buffer, offset));
    // Should not enlarge request at file end.
    assert_eq!(validator.get_size(), hints.GetLastRequstedRange().1);
    assert!(!validator.read_error());
    assert!(validator.has_unavailable_data());

    validator.set_download_hints(None);
}

#[test]
fn ReadError() {
    let file = Rc::new(InvalidSeekableReadStream {
        data_size: kTestDataSize,
    });
    let validator = Rc::new(CpdfReadValidator::new(file, None));

    let kBufferSize = 3 * 1000;
    let mut buffer = vec![0u8; kBufferSize as usize];

    assert!(!validator.read_block_at_offset(&mut buffer[..100], 5000));
    assert!(validator.read_error());
    assert!(validator.has_unavailable_data());
}

#[test]
fn IntOverflow() {
    let test_data = vec![0u8; kTestDataSize as usize];
    let file = Rc::new(CfxReadOnlyContainerStream::new(test_data));
    let file_avail = Rc::new(MockFileAvail::default());
    let file_avail_iface: Rc<dyn FileAvail> = file_avail.clone();
    let validator = Rc::new(CpdfReadValidator::new(file, Some(&file_avail_iface)));

    let mut read_buffer = vec![0u8; 100 as usize];

    // If we have int overflow, this is equal reading after file end. This is not
    // read_error, and in this case we have not unavailable data. It is just error
    // of input params.
    assert!(!validator.read_block_at_offset(&mut read_buffer, i64::MAX - 1));
    assert!(!validator.read_error());
    assert!(!validator.has_unavailable_data());
}

#[test]
fn Session() {
    let mut test_data = vec![0u8; kTestDataSize as usize];

    let file = Rc::new(InvalidSeekableReadStream {
        data_size: kTestDataSize,
    });
    let file_avail = Rc::new(MockFileAvail::default());
    let file_avail_iface: Rc<dyn FileAvail> = file_avail.clone();
    let hints = MockDownloadHints::default();
    let hints_iface: Rc<RefCell<dyn DownloadHints>> = Rc::new(RefCell::new(hints.clone()));
    let validator = Rc::new(CpdfReadValidator::new(file, Some(&file_avail_iface)));
    validator.set_download_hints(Some(&hints_iface));

    let _read_session = ScopedSession::new(validator.clone());
    assert!(!validator.has_read_problems());

    // Data is unavailable
    assert!(!validator.read_block_at_offset(&mut test_data[..100], 0));
    assert!(validator.has_read_problems());
    assert!(validator.has_unavailable_data());
    assert!(!validator.read_error());

    {
        let _read_subsession = ScopedSession::new(validator.clone());
        // The read problems should be hidden.
        assert!(!validator.has_read_problems());

        file_avail.SetAvailableRange(MakeRange(0, 100));
        // Read fail.
        assert!(!validator.read_block_at_offset(&mut test_data[..100], 0));
        assert!(validator.has_read_problems());
        assert!(validator.has_unavailable_data());
        assert!(validator.read_error());
    }

    // The problems should be merged
    assert!(validator.has_read_problems());
    assert!(validator.has_unavailable_data());
    assert!(validator.read_error());
}

#[test]
fn SessionReset() {
    let mut test_data = vec![0u8; kTestDataSize as usize];

    let file = Rc::new(InvalidSeekableReadStream {
        data_size: kTestDataSize,
    });
    let file_avail = Rc::new(MockFileAvail::default());
    let file_avail_iface: Rc<dyn FileAvail> = file_avail.clone();
    let hints = MockDownloadHints::default();
    let hints_iface: Rc<RefCell<dyn DownloadHints>> = Rc::new(RefCell::new(hints.clone()));
    let validator = Rc::new(CpdfReadValidator::new(file, Some(&file_avail_iface)));
    validator.set_download_hints(Some(&hints_iface));

    let _read_session = ScopedSession::new(validator.clone());
    assert!(!validator.has_read_problems());

    // Data is unavailable
    assert!(!validator.read_block_at_offset(&mut test_data[..100], 0));
    assert!(validator.has_read_problems());
    assert!(validator.has_unavailable_data());
    assert!(!validator.read_error());

    {
        let _read_subsession = ScopedSession::new(validator.clone());
        // The read problems should be hidden.
        assert!(!validator.has_read_problems());

        file_avail.SetAvailableRange(MakeRange(0, 100));
        // Read fail.
        assert!(!validator.read_block_at_offset(&mut test_data[..100], 0));
        assert!(validator.has_read_problems());
        assert!(validator.has_unavailable_data());
        assert!(validator.read_error());

        // Reset session.
        validator.reset_errors();
        assert!(!validator.has_read_problems());
    }

    // The problems should be restored.
    assert!(validator.has_read_problems());
    assert!(validator.has_unavailable_data());
    assert!(!validator.read_error());
}

#[test]
fn CheckDataRangeAndRequestIfUnavailable() {
    let test_data = vec![0u8; kTestDataSize as usize];
    let file = Rc::new(CfxReadOnlyContainerStream::new(test_data));
    let file_avail = Rc::new(MockFileAvail::default());
    let file_avail_iface: Rc<dyn FileAvail> = file_avail.clone();
    let validator = Rc::new(CpdfReadValidator::new(file, Some(&file_avail_iface)));

    let hints = MockDownloadHints::default();
    let hints_iface: Rc<RefCell<dyn DownloadHints>> = Rc::new(RefCell::new(hints.clone()));
    validator.set_download_hints(Some(&hints_iface));

    assert!(!validator.check_data_range_and_request_if_unavailable(5000, 100));
    assert!(!validator.read_error());
    assert!(validator.has_unavailable_data());

    // Requested range should be enlarged and aligned.
    assert_eq!(MakeRange(4608, 5632), hints.GetLastRequstedRange());

    file_avail.SetAvailableRange(hints.GetLastRequstedRange());
    hints.Reset();

    validator.reset_errors();
    assert!(validator.check_data_range_and_request_if_unavailable(5000, 100));
    // No new request on already available data.
    assert_eq!(MakeRange(0, 0), hints.GetLastRequstedRange());
    assert!(!validator.read_error());
    assert!(!validator.has_unavailable_data());

    let mut read_buffer = vec![0u8; 100 as usize];
    assert!(validator.read_block_at_offset(&mut read_buffer, 5000));
    // No new request on already available data.
    assert_eq!(MakeRange(0, 0), hints.GetLastRequstedRange());
    assert!(!validator.read_error());
    assert!(!validator.has_unavailable_data());

    validator.reset_errors();
    // Try request unavailable data at file end.
    assert!(!validator.check_data_range_and_request_if_unavailable(validator.get_size() - 100, 100));

    // Should not enlarge request at file end.
    assert_eq!(validator.get_size(), hints.GetLastRequstedRange().1);
    assert!(!validator.read_error());
    assert!(validator.has_unavailable_data());

    validator.reset_errors();
    // Offset > file size should yield |true| and not cause a fetch.
    assert!(validator.check_data_range_and_request_if_unavailable(kTestDataSize + 1, 1));
    // No new request on already available data.
    assert!(!validator.read_error());
    assert!(!validator.has_unavailable_data());

    validator.set_download_hints(None);
}
