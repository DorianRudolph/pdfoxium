// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/parser/cpdf_read_validator.{h,cpp}`. The callback traits
//! correspond to `CPDF_DataAvail::FileAvail` and `DownloadHints`.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::cpdf_stream::FILE_BUF_SIZE;
use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};
use crate::fxcrt::fx_types::FxFileSize;

pub trait FileAvail {
    fn is_data_avail(&self, offset: FxFileSize, size: usize) -> bool;
}

pub trait DownloadHints {
    fn add_segment(&mut self, offset: FxFileSize, size: usize);
}

pub struct CpdfReadValidator {
    file_read: Rc<dyn SeekableReadStream>,
    file_avail: Option<Weak<dyn FileAvail>>,
    hints: RefCell<Option<Weak<RefCell<dyn DownloadHints>>>>,
    read_error: Cell<bool>,
    has_unavailable_data: Cell<bool>,
    whole_file_already_available: Cell<bool>,
    file_size: FxFileSize,
}

pub struct ScopedSession {
    validator: Rc<CpdfReadValidator>,
    saved_read_error: bool,
    saved_has_unavailable_data: bool,
}

impl ScopedSession {
    pub fn new(validator: Rc<CpdfReadValidator>) -> Self {
        let saved_read_error = validator.read_error();
        let saved_has_unavailable_data = validator.has_unavailable_data();
        validator.reset_errors();
        Self {
            validator,
            saved_read_error,
            saved_has_unavailable_data,
        }
    }
}

impl Drop for ScopedSession {
    fn drop(&mut self) {
        self.validator
            .read_error
            .set(self.validator.read_error() || self.saved_read_error);
        self.validator
            .has_unavailable_data
            .set(self.validator.has_unavailable_data() || self.saved_has_unavailable_data);
    }
}

impl CpdfReadValidator {
    pub fn new(
        file_read: Rc<dyn SeekableReadStream>,
        file_avail: Option<&Rc<dyn FileAvail>>,
    ) -> Self {
        let file_size = file_read.get_size();
        Self {
            file_read,
            file_avail: file_avail.map(Rc::downgrade),
            hints: RefCell::new(None),
            read_error: Cell::new(false),
            has_unavailable_data: Cell::new(false),
            whole_file_already_available: Cell::new(false),
            file_size,
        }
    }

    pub fn set_download_hints(&self, hints: Option<&Rc<RefCell<dyn DownloadHints>>>) {
        *self.hints.borrow_mut() = hints.map(Rc::downgrade);
    }

    pub fn read_error(&self) -> bool {
        self.read_error.get()
    }
    pub fn has_unavailable_data(&self) -> bool {
        self.has_unavailable_data.get()
    }
    pub fn has_read_problems(&self) -> bool {
        self.read_error() || self.has_unavailable_data()
    }

    pub fn reset_errors(&self) {
        self.read_error.set(false);
        self.has_unavailable_data.set(false);
    }

    fn align_down(offset: FxFileSize) -> FxFileSize {
        if offset > 0 {
            offset - offset % FILE_BUF_SIZE as FxFileSize
        } else {
            0
        }
    }

    fn align_up(offset: FxFileSize) -> FxFileSize {
        Self::align_down(offset)
            .checked_add(FILE_BUF_SIZE as FxFileSize)
            .unwrap_or(offset)
    }

    fn schedule_download(&self, offset: FxFileSize, size: usize) {
        self.has_unavailable_data.set(true);
        if size == 0 {
            return;
        }
        let hints = self.hints.borrow().as_ref().and_then(Weak::upgrade);
        let Some(hints) = hints else {
            return;
        };
        let start = Self::align_down(offset);
        let Some(end) = FxFileSize::try_from(size)
            .ok()
            .and_then(|size| offset.checked_add(size))
        else {
            return;
        };
        let end = self.file_size.min(Self::align_up(end));
        let Some(segment_size) = end
            .checked_sub(start)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return;
        };
        hints.borrow_mut().add_segment(start, segment_size);
    }

    fn is_data_range_available(&self, offset: FxFileSize, size: usize) -> bool {
        if self.whole_file_already_available.get() {
            return true;
        }
        self.file_avail
            .as_ref()
            .and_then(Weak::upgrade)
            .is_none_or(|avail| avail.is_data_avail(offset, size))
    }

    pub fn is_whole_file_available(&self) -> bool {
        if self.whole_file_already_available.get() {
            return true;
        }
        let Ok(size) = usize::try_from(self.file_size) else {
            return false;
        };
        let available = self.is_data_range_available(0, size);
        self.whole_file_already_available.set(available);
        available
    }

    pub fn check_data_range_and_request_if_unavailable(
        &self,
        offset: FxFileSize,
        size: usize,
    ) -> bool {
        if offset > self.file_size {
            return true;
        }
        let Some(end) = FxFileSize::try_from(size)
            .ok()
            .and_then(|size| offset.checked_add(size))
            .and_then(|end| end.checked_add(FILE_BUF_SIZE as FxFileSize))
        else {
            return false;
        };
        let end = self.file_size.min(end);
        let Some(segment_size) = end
            .checked_sub(offset)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return false;
        };
        if self.is_data_range_available(offset, segment_size) {
            return true;
        }
        self.schedule_download(offset, segment_size);
        false
    }

    pub fn check_whole_file_and_request_if_unavailable(&self) -> bool {
        if self.is_whole_file_available() {
            return true;
        }
        if let Ok(size) = usize::try_from(self.file_size) {
            self.schedule_download(0, size);
        }
        false
    }
}

impl StreamWithSize for CpdfReadValidator {
    fn get_size(&self) -> FxFileSize {
        self.file_size
    }
}

impl SeekableReadStream for CpdfReadValidator {
    fn read_block_at_offset(&self, buffer: &mut [u8], offset: FxFileSize) -> bool {
        if offset < 0 {
            return false;
        }
        let Some(end) = FxFileSize::try_from(buffer.len())
            .ok()
            .and_then(|size| offset.checked_add(size))
        else {
            return false;
        };
        if end > self.file_size {
            return false;
        }
        if !self.is_data_range_available(offset, buffer.len()) {
            self.schedule_download(offset, buffer.len());
            return false;
        }
        if self.file_read.read_block_at_offset(buffer, offset) {
            return true;
        }
        self.read_error.set(true);
        self.schedule_download(offset, buffer.len());
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{CpdfReadValidator, DownloadHints, FileAvail, ScopedSession};
    use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
    use crate::fxcrt::fx_stream::SeekableReadStream;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Unavailable;
    impl FileAvail for Unavailable {
        fn is_data_avail(&self, _offset: i64, _size: usize) -> bool {
            false
        }
    }
    struct Hints(Rc<RefCell<Vec<(i64, usize)>>>);
    impl DownloadHints for Hints {
        fn add_segment(&mut self, offset: i64, size: usize) {
            self.0.borrow_mut().push((offset, size));
        }
    }

    #[test]
    fn unavailable_read_requests_aligned_segment_and_session_restores_flags() {
        let file = Rc::new(CfxReadOnlySpanStream::new(&[0u8; 1500]));
        let availability: Rc<dyn FileAvail> = Rc::new(Unavailable);
        let validator = Rc::new(CpdfReadValidator::new(file, Some(&availability)));
        let segments = Rc::new(RefCell::new(Vec::new()));
        let hints: Rc<RefCell<dyn DownloadHints>> =
            Rc::new(RefCell::new(Hints(Rc::clone(&segments))));
        validator.set_download_hints(Some(&hints));
        assert!(!validator.read_block_at_offset(&mut [0u8; 3], 600));
        assert!(validator.has_unavailable_data());
        assert_eq!(segments.borrow()[0], (512, 512));
        {
            let _session = ScopedSession::new(Rc::clone(&validator));
            assert!(!validator.has_unavailable_data());
            assert!(!validator.check_data_range_and_request_if_unavailable(600, 3));
        }
        assert!(validator.has_unavailable_data());
        assert!(validator.check_data_range_and_request_if_unavailable(1501, 1));
    }
}

#[cfg(test)]
#[path = "cpdf_read_validator_unittest.rs"]
mod upstream_tests;
