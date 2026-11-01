// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! GIF's little-endian LZW code table and resumable reversed-string extraction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LzwStatus {
    Error,
    Success,
    Unfinished,
    InsufficientDestSize,
}
#[derive(Clone, Copy, Default)]
struct CodeEntry {
    prefix: u16,
    suffix: u8,
}
pub struct LzwDecompressor {
    code_size: u8,
    current_size: u8,
    color_end: u16,
    clear: u16,
    end: u16,
    next: u16,
    first: u8,
    old: Option<u16>,
    store: u32,
    bits: u8,
    table: Box<[CodeEntry; 4096]>,
    decompressed: Vec<u8>,
    finished: bool,
}
impl LzwDecompressor {
    pub fn create(color_exp: u8, code_exp: u8) -> Option<Self> {
        if color_exp >= 12 || code_exp > 12 || code_exp < color_exp + 1 {
            return None;
        }
        let clear = 1u16 << code_exp;
        let mut result = Self {
            code_size: code_exp,
            current_size: 0,
            color_end: 1u16 << (color_exp + 1),
            clear,
            end: clear + 1,
            next: 0,
            first: 0,
            old: None,
            store: 0,
            bits: 0,
            table: Box::new([CodeEntry::default(); 4096]),
            decompressed: Vec::with_capacity(4096),
            finished: false,
        };
        result.clear_table();
        Some(result)
    }
    fn clear_table(&mut self) {
        self.current_size = self.code_size + 1;
        self.next = self.end + 1;
        self.old = None;
        self.table.fill(CodeEntry::default());
        for (i, entry) in self.table.iter_mut().take(self.clear as usize).enumerate() {
            entry.suffix = i as u8;
        }
        self.decompressed.clear();
    }
    fn add_code(&mut self, prefix: u16, suffix: u8) {
        if self.next >= 4096 {
            return;
        }
        self.table[self.next as usize] = CodeEntry { prefix, suffix };
        self.next += 1;
        if self.next < 4096 && self.next >> self.current_size != 0 {
            self.current_size += 1;
        }
    }
    fn decode_string(&mut self, mut code: u16) -> bool {
        self.decompressed.clear();
        while code >= self.clear && code <= self.next {
            let Some(entry) = self.table.get(code as usize) else {
                return false;
            };
            if code == entry.prefix || self.decompressed.len() >= 4095 {
                return false;
            }
            self.decompressed.push(entry.suffix);
            code = entry.prefix;
        }
        if code >= self.color_end {
            return false;
        }
        self.decompressed.push(code as u8);
        self.first = code as u8;
        true
    }
    fn extract_data(&mut self, dest: &mut [u8]) -> usize {
        let count = dest.len().min(self.decompressed.len());
        for byte in &mut dest[..count] {
            *byte = self.decompressed.pop().unwrap();
        }
        count
    }
    /// Returns status, input bytes consumed and output bytes written. Input and
    /// destination can both be replaced between calls; dictionary/bit state stays.
    pub fn decode(&mut self, source: &[u8], dest: &mut [u8]) -> (LzwStatus, usize, usize) {
        use LzwStatus::*;
        if self.finished {
            return (Success, 0, 0);
        }
        let mut read = 0;
        let mut written = self.extract_data(dest);
        if !self.decompressed.is_empty() {
            return (InsufficientDestSize, read, written);
        }
        loop {
            if self.current_size > 12 {
                return (Error, read, written);
            }
            while self.bits < self.current_size {
                let Some(byte) = source.get(read) else {
                    return (Unfinished, read, written);
                };
                self.store |= u32::from(*byte) << self.bits;
                self.bits += 8;
                read += 1;
            }
            let code = (self.store & ((1u32 << self.current_size) - 1)) as u16;
            self.store >>= self.current_size;
            self.bits -= self.current_size;
            if code == self.clear {
                self.clear_table();
                continue;
            }
            if code == self.end {
                self.finished = true;
                return (Success, read, written);
            }
            if let Some(old) = self.old {
                if code > self.next {
                    return (Error, read, written);
                }
                if code == self.next {
                    self.add_code(old, self.first);
                    if !self.decode_string(code) {
                        return (Error, read, written);
                    }
                } else {
                    if !self.decode_string(code) {
                        return (Error, read, written);
                    }
                    self.add_code(old, self.first);
                }
            } else if !self.decode_string(code) {
                return (Error, read, written);
            }
            self.old = Some(code);
            written += self.extract_data(&mut dest[written..]);
            if !self.decompressed.is_empty() {
                return (InsufficientDestSize, read, written);
            }
        }
    }
}
#[cfg(test)]
#[path = "lzw_tests.rs"]
mod tests;
