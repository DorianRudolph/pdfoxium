// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcrt/cfx_seekablestreamproxy.{h,cpp}` for wchar32.

use std::rc::Rc;

use super::fx_codepage::{get_acp, FxCodePage};
use super::fx_stream::SeekableReadStream;
use super::fx_types::FxFileSize;

// Returns {src bytes consumed, dst chars produced}.
// Invalid sequences are silently not output.
fn utf8_decode(
    source: &[u8],
    destination: &mut [u32],
    code: &mut u32,
    pending: &mut i32,
) -> (usize, usize) {
    debug_assert!(!destination.is_empty());

    let mut source_count = 0;
    let mut destination_count = 0;
    for &byte in source {
        if destination_count == destination.len() {
            break;
        }
        source_count += 1;
        if byte < 0x80 {
            *pending = 0;
            destination[destination_count] = u32::from(byte);
            destination_count += 1;
        } else if byte < 0xc0 {
            if *pending < 1 {
                continue;
            }

            *code <<= 6;
            *code |= u32::from(byte & 0x3f);
            *pending -= 1;
            if *pending == 0 {
                destination[destination_count] = *code;
                destination_count += 1;
            }
        } else if byte < 0xe0 {
            *pending = 1;
            *code = u32::from(byte & 0x1f);
        } else if byte < 0xf0 {
            *pending = 2;
            *code = u32::from(byte & 0x0f);
        } else if byte < 0xf8 {
            *pending = 3;
            *code = u32::from(byte & 0x07);
        } else if byte < 0xfc {
            *pending = 4;
            *code = u32::from(byte & 0x03);
        } else if byte < 0xfe {
            *pending = 5;
            *code = u32::from(byte & 0x01);
        }
    }
    (source_count, destination_count)
}

// Returns the code page indicated by a byte-order mark at the start of a
// stream, or None if no BOM is present.
fn detect_byte_order_mark(leading_bytes: u32) -> Option<FxCodePage> {
    const UTF8_MASK: u32 = 0x00ff_ffff;
    const UTF8_MARKER: u32 = 0x00bf_bbef;
    const UTF16_MASK: u32 = 0x0000_ffff;
    const UTF16_BE_MARKER: u32 = 0x0000_fffe;
    const UTF16_LE_MARKER: u32 = 0x0000_feff;

    if leading_bytes & UTF8_MASK == UTF8_MARKER {
        return Some(FxCodePage::Utf8);
    }
    match leading_bytes & UTF16_MASK {
        UTF16_BE_MARKER => Some(FxCodePage::Utf16Be),
        UTF16_LE_MARKER => Some(FxCodePage::Utf16Le),
        _ => None,
    }
}

enum From {
    Begin,
    Current,
}

pub struct CfxSeekableStreamProxy<'a> {
    code_page: FxCodePage,
    bom_length: usize,
    position: FxFileSize,
    stream: Rc<dyn SeekableReadStream + 'a>,
    // C++ aliases the wchar32 output as bytes/uint16_t for UTF16 reads. Safe
    // Rust stages those bytes, preserving raw partial/failed-read mutations
    // before expanding code units in reverse. Retain scratch capacity between
    // reads; it contains no cached decoding result or borrowed caller memory.
    byte_buffer: Vec<u8>,
    // Carry the source decoder's partial sequence across input blocks. Native
    // resets these locals per read, losing valid scalars at decoding boundaries.
    utf8_code: u32,
    utf8_pending: i32,
}

impl<'a> CfxSeekableStreamProxy<'a> {
    pub fn new(stream: Rc<dyn SeekableReadStream + 'a>) -> Self {
        let mut proxy = Self {
            code_page: FxCodePage::DefAnsi,
            bom_length: 0,
            position: 0,
            stream,
            byte_buffer: Vec::new(),
            utf8_code: 0,
            utf8_pending: 0,
        };
        proxy.seek(From::Begin, 0);

        let mut leading_bytes = [0; 4];
        proxy.read_data(&mut leading_bytes[..3]);
        if let Some(bom) = detect_byte_order_mark(u32::from_ne_bytes(leading_bytes)) {
            proxy.code_page = bom;
            proxy.bom_length = if bom == FxCodePage::Utf8 { 3 } else { 2 };
        } else {
            proxy.code_page = get_acp();
            proxy.bom_length = 0;
        }
        proxy.seek(From::Begin, proxy.bom_length as FxFileSize);
        proxy
    }

    /// Estimate under worst possible expansion.
    pub fn get_size(&self) -> FxFileSize {
        self.stream.get_size()
    }

    fn get_position(&self) -> FxFileSize {
        self.position
    }

    pub fn is_eof(&self) -> bool {
        self.position >= self.get_size()
    }

    fn seek(&mut self, from: From, offset: FxFileSize) {
        self.position = match from {
            From::Begin => offset,
            From::Current => self.position.checked_add(offset).unwrap_or(FxFileSize::MAX),
        };
        self.position = self.position.clamp(0, self.get_size());
    }

    pub fn get_code_page(&self) -> FxCodePage {
        self.code_page
    }

    pub fn set_code_page(&mut self, code_page: FxCodePage) {
        if self.bom_length > 0 {
            return;
        }
        if self.code_page != code_page {
            self.utf8_code = 0;
            self.utf8_pending = 0;
            self.code_page = code_page;
        }
    }

    fn read_data(&mut self, buffer: &mut [u8]) -> usize {
        debug_assert!(!buffer.is_empty());
        let remaining = (self.get_size() - self.position) as usize;
        let read_size = buffer.len().min(remaining);
        if read_size == 0 {
            return 0;
        }
        if !self
            .stream
            .read_block_at_offset(&mut buffer[..read_size], self.position)
        {
            return 0;
        }
        let new_position = self.position.checked_add(read_size as FxFileSize);
        self.position = new_position.unwrap_or(self.position);
        if new_position.is_some() {
            read_size
        } else {
            0
        }
    }

    /// Returns the number of wchar32 elements placed into `buffer`.
    pub fn read_block(&mut self, buffer: &mut [u32]) -> usize {
        if buffer.is_empty() {
            return 0;
        }
        if self.code_page == FxCodePage::Utf16Le || self.code_page == FxCodePage::Utf16Be {
            let bytes_to_read = buffer.len() * std::mem::size_of::<u16>();
            let mut byte_buffer = std::mem::take(&mut self.byte_buffer);
            byte_buffer.resize(bytes_to_read, 0);
            // Initialize with the caller's current bytes: native failed reads
            // and odd byte tails may leave portions of the original words.
            for (index, bytes) in byte_buffer.chunks_mut(4).enumerate() {
                bytes.copy_from_slice(&buffer[index].to_ne_bytes()[..bytes.len()]);
            }
            let bytes_read = self.read_data(&mut byte_buffer);
            let elements = bytes_read / std::mem::size_of::<u16>();
            if self.code_page == FxCodePage::Utf16Be {
                for pair in byte_buffer[..elements * 2].chunks_exact_mut(2) {
                    pair.swap(0, 1);
                }
            }
            // Preserve writes to the aliased output representation before
            // performing UTF16ToWChar's self-intersecting reverse expansion.
            for (index, bytes) in byte_buffer.chunks(4).enumerate() {
                let mut word = buffer[index].to_ne_bytes();
                word[..bytes.len()].copy_from_slice(bytes);
                buffer[index] = u32::from_ne_bytes(word);
            }
            for index in (0..elements).rev() {
                buffer[index] = u32::from(u16::from_ne_bytes([
                    byte_buffer[index * 2],
                    byte_buffer[index * 2 + 1],
                ]));
            }
            self.byte_buffer = byte_buffer;
            return elements;
        }

        loop {
            let position = self.get_position();
            let bytes_to_read = buffer.len().min((self.get_size() - position) as usize);
            if bytes_to_read == 0 {
                return 0;
            }
            let mut byte_buffer = std::mem::take(&mut self.byte_buffer);
            byte_buffer.resize(bytes_to_read, 0);
            // Native constructs a fresh zero-filled DataVector for this path.
            byte_buffer.fill(0);
            let bytes_read = self.read_data(&mut byte_buffer);
            if self.code_page != FxCodePage::Utf8 {
                self.byte_buffer = byte_buffer;
                return 0;
            }
            let (source_bytes_consumed, destination_wchars_produced) = utf8_decode(
                &byte_buffer[..bytes_read],
                buffer,
                &mut self.utf8_code,
                &mut self.utf8_pending,
            );
            self.byte_buffer = byte_buffer;
            self.seek(
                From::Current,
                source_bytes_consumed as FxFileSize - bytes_read as FxFileSize,
            );
            if destination_wchars_produced != 0 || bytes_read == 0 || self.is_eof() {
                return destination_wchars_produced;
            }
            // A small output plane can end inside a scalar without producing any
            // character. Continue decoding rather than reporting a false EOF.
        }
    }
}

#[cfg(test)]
#[path = "cfx_seekablestreamproxy_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
    use crate::fxcrt::fx_stream::StreamWithSize;

    #[test]
    fn bom_is_immutable_and_non_bom_code_page_is_selectable() {
        let mut proxy =
            CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(b"\xef\xbb\xbfabc")));
        assert_eq!(proxy.get_size(), 6);
        assert_eq!(proxy.get_code_page(), FxCodePage::Utf8);
        proxy.set_code_page(FxCodePage::Utf16Le);
        assert_eq!(proxy.get_code_page(), FxCodePage::Utf8);
        assert_eq!(proxy.read_block(&mut []), 0);
        assert_eq!(proxy.get_position(), 3);
        let mut output = [0xbd; 3];
        assert_eq!(proxy.read_block(&mut output), 3);
        assert_eq!(output, [97, 98, 99]);
        assert!(proxy.is_eof());

        let mut proxy = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(b"abc")));
        proxy.set_code_page(FxCodePage::Utf8);
        assert_eq!(proxy.read_block(&mut output), 3);
        assert_eq!(output, [97, 98, 99]);
        assert!(proxy.is_eof());
    }

    #[test]
    fn utf8_keeps_source_invalid_sequence_handling_and_retains_split_scalars() {
        let input = b"\xef\xbb\xbf\x80\xc0\x80\xed\xa0\x80\xf8\x88\x80\x80\x80\xffX";
        let mut proxy = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(input)));
        let mut output = [0xbd; 32];
        assert_eq!(proxy.read_block(&mut output), 4);
        assert_eq!(&output[..4], &[0, 0xd800, 0x20_0000, b'X' as u32]);
        assert_eq!(output[4], 0xbd);
        assert!(proxy.is_eof());

        // A one-character output must still decode a multibyte scalar.
        let mut proxy = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(
            b"\xef\xbb\xbf\xc2\xa2Z",
        )));
        let mut output = [0xbd];
        assert_eq!(proxy.read_block(&mut output), 1);
        assert_eq!(output, [0xa2]);
        assert_eq!(proxy.read_block(&mut output), 1);
        assert_eq!(output, [b'Z' as u32]);
        assert_eq!(proxy.read_block(&mut output), 0);
    }

    #[test]
    fn utf8_small_planes_preserve_two_three_and_four_byte_scalars_and_recovery() {
        for capacity in 1..=8 {
            for text in ["¢Z", "€Z", "😀Z", "a¢€😀¢€😀z"] {
                let mut proxy = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(
                    text.as_bytes(),
                )));
                proxy.set_code_page(FxCodePage::Utf8);
                let mut buffer = vec![0; capacity];
                let mut decoded = Vec::new();
                loop {
                    let count = proxy.read_block(&mut buffer);
                    if count == 0 {
                        break;
                    }
                    decoded.extend_from_slice(&buffer[..count]);
                }
                assert_eq!(
                    decoded,
                    text.chars().map(u32::from).collect::<Vec<_>>(),
                    "capacity={capacity}"
                );
                assert!(proxy.is_eof());
            }
            let mut proxy = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(
                b"A\x80\xc2B\xe2\x82",
            )));
            proxy.set_code_page(FxCodePage::Utf8);
            let mut buffer = vec![0; capacity];
            let mut decoded = Vec::new();
            loop {
                let count = proxy.read_block(&mut buffer);
                if count == 0 {
                    break;
                }
                decoded.extend_from_slice(&buffer[..count]);
            }
            assert_eq!(decoded, [b'A' as u32, b'B' as u32]);
            assert!(proxy.is_eof());
        }
    }

    #[test]
    fn utf16_preserves_raw_surrogates_and_odd_tail_byte_mutation() {
        for data in [
            &b"\xff\xfe\x3d\xd8\x00\xde"[..],
            &b"\xfe\xff\xd8\x3d\xde\x00"[..],
        ] {
            let mut proxy = CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(data)));
            let mut output = [0xbd; 3];
            assert_eq!(proxy.read_block(&mut output), 2);
            assert_eq!(output, [0xd83d, 0xde00, 0xbd]);
            assert!(proxy.is_eof());
        }
        let mut proxy =
            CfxSeekableStreamProxy::new(Rc::new(CfxReadOnlySpanStream::new(b"\xff\xfe\x99")));
        let mut output = [u32::from_ne_bytes([0x11, 0x22, 0x33, 0x44])];
        assert_eq!(proxy.read_block(&mut output), 0);
        assert_eq!(output[0].to_ne_bytes(), [0x99, 0x22, 0x33, 0x44]);
        assert!(proxy.is_eof());
    }

    struct FailsAfterMutatingOutput;
    impl StreamWithSize for FailsAfterMutatingOutput {
        fn get_size(&self) -> i64 {
            5
        }
    }
    impl SeekableReadStream for FailsAfterMutatingOutput {
        fn read_block_at_offset(&self, buffer: &mut [u8], offset: i64) -> bool {
            if offset == 0 {
                buffer.copy_from_slice(&b"\xff\xfeX"[..buffer.len()]);
                return true;
            }
            buffer[0] = 0x7a;
            false
        }
    }

    #[test]
    fn failed_utf16_read_keeps_native_raw_mutation_and_position() {
        let mut proxy = CfxSeekableStreamProxy::new(Rc::new(FailsAfterMutatingOutput));
        let mut output = [u32::from_ne_bytes([0x11, 0x22, 0x33, 0x44]); 2];
        assert_eq!(proxy.get_position(), 2);
        assert_eq!(proxy.read_block(&mut output), 0);
        assert_eq!(output[0].to_ne_bytes(), [0x7a, 0x22, 0x33, 0x44]);
        assert_eq!(output[1].to_ne_bytes(), [0x11, 0x22, 0x33, 0x44]);
        assert_eq!(proxy.get_position(), 2);
        assert!(!proxy.is_eof());
    }
}
