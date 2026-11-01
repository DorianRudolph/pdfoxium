// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/fileaccess_iface.h`. `StdFileAccess` provides
//! the portable file operations used in place of PDFium's platform backends.

use super::fx_types::FxFileSize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};

pub trait FileAccessIface {
    fn open(&mut self, file_name: &str) -> bool;
    fn close(&mut self);
    fn get_size(&self) -> FxFileSize;
    fn get_position(&mut self) -> FxFileSize;
    fn set_position(&mut self, pos: FxFileSize) -> FxFileSize;
    fn read(&mut self, buffer: &mut [u8]) -> usize;
    fn write(&mut self, buffer: &[u8]) -> bool;
    fn read_pos(&mut self, buffer: &mut [u8], pos: FxFileSize) -> usize;
    fn flush(&mut self) -> bool;
    fn truncate(&mut self, size: FxFileSize) -> bool;
}

#[derive(Default)]
pub struct StdFileAccess {
    file: Option<File>,
}

impl StdFileAccess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_path(&mut self, file_name: &std::path::Path) -> bool {
        if self.file.is_some() {
            return false;
        }
        match File::open(file_name) {
            Ok(file) => {
                self.file = Some(file);
                true
            }
            Err(_) => false,
        }
    }
}

impl FileAccessIface for StdFileAccess {
    fn open(&mut self, file_name: &str) -> bool {
        self.open_path(std::path::Path::new(file_name))
    }

    fn close(&mut self) {
        self.file = None;
    }

    fn get_size(&self) -> FxFileSize {
        self.file.as_ref().map_or(0, |file| {
            file.metadata().ok().map_or(0, |metadata| {
                FxFileSize::try_from(metadata.len()).expect("file too large")
            })
        })
    }

    fn get_position(&mut self) -> FxFileSize {
        self.file.as_mut().map_or(-1, |file| {
            file.stream_position()
                .ok()
                .and_then(|pos| FxFileSize::try_from(pos).ok())
                .unwrap_or(-1)
        })
    }

    fn set_position(&mut self, pos: FxFileSize) -> FxFileSize {
        let Some(file) = self.file.as_mut() else {
            return -1;
        };
        let Ok(pos) = u64::try_from(pos) else {
            return -1;
        };
        file.seek(SeekFrom::Start(pos))
            .ok()
            .and_then(|new_pos| FxFileSize::try_from(new_pos).ok())
            .unwrap_or(-1)
    }

    fn read(&mut self, buffer: &mut [u8]) -> usize {
        self.file
            .as_mut()
            .and_then(|file| file.read(buffer).ok())
            .unwrap_or(0)
    }

    fn write(&mut self, buffer: &[u8]) -> bool {
        self.file.as_mut().and_then(|file| file.write(buffer).ok()) == Some(buffer.len())
    }

    fn read_pos(&mut self, buffer: &mut [u8], pos: FxFileSize) -> usize {
        if self.file.is_none() || pos >= self.get_size() || self.set_position(pos) == -1 {
            return 0;
        }
        self.read(buffer)
    }

    fn flush(&mut self) -> bool {
        self.file
            .as_mut()
            .is_some_and(|file| file.sync_all().is_ok())
    }

    fn truncate(&mut self, size: FxFileSize) -> bool {
        let Ok(size) = u64::try_from(size) else {
            return false;
        };
        self.file
            .as_mut()
            .is_some_and(|file| file.set_len(size).is_ok())
    }
}
