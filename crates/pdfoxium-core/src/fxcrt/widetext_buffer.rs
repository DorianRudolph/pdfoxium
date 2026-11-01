// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `core/fxcrt/widetext_buffer.{h,cpp}`. Typed u32 backing preserves native
//! wchar32 spans without reinterpreting an unaligned byte allocation.
#[derive(Default)]
pub struct WideTextBuffer {
    buffer: Vec<u32>,
    length: usize,
    alloc_step: usize,
}
impl WideTextBuffer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_length(&self) -> usize {
        self.length
    }
    pub fn get_size(&self) -> usize {
        self.length * 4
    }
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
    pub fn clear(&mut self) {
        self.length = 0;
    }
    pub fn set_alloc_step(&mut self, bytes: usize) {
        self.alloc_step = bytes;
    }
    pub fn get_wide_span(&self) -> &[u32] {
        &self.buffer[..self.length]
    }
    pub fn get_wide_span_mut(&mut self) -> &mut [u32] {
        &mut self.buffer[..self.length]
    }
    pub fn as_string_view(&self) -> &[u32] {
        self.get_wide_span()
    }
    pub fn make_string(&self) -> Vec<u32> {
        self.as_string_view().to_vec()
    }
    pub fn into_string(mut self) -> Vec<u32> {
        self.buffer.truncate(self.length);
        self.buffer
    }
    pub fn detach_buffer(&mut self) -> Vec<u32> {
        let mut result = std::mem::take(&mut self.buffer);
        result.truncate(self.length);
        self.length = 0;
        result
    }
    pub fn estimate_size(&mut self, bytes: usize) {
        let units = bytes.checked_add(3).expect("wide size overflow") / 4;
        if self.buffer.len() < units {
            self.reserve_units(units);
        }
    }
    fn reserve_units(&mut self, units: usize) {
        if units <= self.buffer.len() {
            return;
        }
        let bytes = units.checked_mul(4).expect("wide buffer size overflow");
        let allocated = self
            .buffer
            .len()
            .checked_mul(4)
            .expect("wide buffer size overflow");
        let step = 128.max(if self.alloc_step != 0 {
            self.alloc_step
        } else {
            allocated / 4
        });
        let bytes = bytes
            .checked_add(step - 1)
            .expect("wide buffer size overflow")
            / step
            * step;
        let units = bytes.checked_add(3).expect("wide buffer size overflow") / 4;
        self.buffer.resize(units, 0);
    }
    pub fn expand_wide_buf(&mut self, count: usize) -> &mut [u32] {
        let start = self.length;
        let end = start.checked_add(count).expect("wide buffer size overflow");
        self.reserve_units(end);
        self.length = end;
        &mut self.buffer[start..end]
    }
    pub fn append_char(&mut self, ch: u32) {
        self.expand_wide_buf(1)[0] = ch;
    }
    pub fn append_wide_string(&mut self, value: &[u32]) {
        self.expand_wide_buf(value.len()).copy_from_slice(value);
    }
    pub fn append_ascii(&mut self, value: &[u8]) {
        for (slot, &byte) in self.expand_wide_buf(value.len()).iter_mut().zip(value) {
            *slot = u32::from(byte);
        }
    }
    pub fn append_buffer(&mut self, value: &Self) {
        self.append_wide_string(value.as_string_view());
    }
    pub fn delete(&mut self, start: usize, count: usize) {
        let Some(end) = start.checked_add(count).filter(|&end| end <= self.length) else {
            return;
        };
        self.buffer.copy_within(end..self.length, start);
        self.length -= count;
    }
}
impl Extend<u32> for WideTextBuffer {
    fn extend<T: IntoIterator<Item = u32>>(&mut self, iter: T) {
        for ch in iter {
            self.append_char(ch);
        }
    }
}
#[cfg(test)]
#[path = "widetext_buffer_unittest.rs"]
mod upstream_tests;
