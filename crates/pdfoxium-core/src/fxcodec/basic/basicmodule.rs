// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcodec/basic/basicmodule.h` and `.cpp`.

use crate::fxcodec::scanlinedecoder::{ScanlineDecoder, ScanlineSource, ScanlineState};
use crate::fxcrt::byteorder::get_uint32_msb_first;

pub struct BasicModule;

impl BasicModule {
    pub fn create_run_length_decoder<'a>(
        src: &'a [u8],
        width: i32,
        height: i32,
        comps: i32,
        bpc: i32,
    ) -> Option<Box<dyn ScanlineDecoder + 'a>> {
        Some(Box::new(RlScanlineDecoder::create(
            ScanlineSource::Borrowed(src),
            width,
            height,
            comps,
            bpc,
        )?))
    }

    /// CPDF_DIB retains its accessor's exact allocation for native row decoding.
    pub(crate) fn create_run_length_decoder_shared(
        src: std::rc::Rc<Vec<u8>>,
        width: i32,
        height: i32,
        comps: i32,
        bpc: i32,
    ) -> Option<Box<dyn ScanlineDecoder>> {
        Some(Box::new(RlScanlineDecoder::create(
            ScanlineSource::Shared(src),
            width,
            height,
            comps,
            bpc,
        )?))
    }

    pub fn run_length_encode(src: &[u8]) -> Vec<u8> {
        if src.is_empty() {
            return Vec::new();
        }
        if src.len() == 1 {
            return vec![0, src[0], 128];
        }

        let estimated_size = src
            .len()
            .div_ceil(3)
            .checked_mul(4)
            .and_then(|size| size.checked_add(1))
            .expect("encoded size overflow");
        let mut result = vec![0u8; estimated_size];
        let mut dest_pos = 0usize;
        let mut run_start = 0usize;
        let mut run_end = 1usize;
        let mut x = src[run_start];
        let mut y = src[run_end];
        while run_end < src.len() {
            let max_len = 128.min(src.len() - run_start);
            while x == y && run_end - run_start < max_len - 1 {
                run_end += 1;
                y = src[run_end];
            }
            if x == y {
                run_end += 1;
                if run_end < src.len() {
                    y = src[run_end];
                }
            }
            if run_end - run_start > 1 {
                result[dest_pos] = (257 - (run_end - run_start)) as u8;
                result[dest_pos + 1] = x;
                x = y;
                run_start = run_end;
                run_end += 1;
                if run_end < src.len() {
                    y = src[run_end];
                }
                dest_pos += 2;
                continue;
            }
            while x != y && run_end <= run_start + max_len {
                result[dest_pos + run_end - run_start] = x;
                x = y;
                run_end += 1;
                if run_end == src.len() {
                    if run_end <= run_start + max_len {
                        result[dest_pos + run_end - run_start] = x;
                        run_end += 1;
                    }
                    break;
                }
                y = src[run_end];
            }
            result[dest_pos] = (run_end - run_start - 2) as u8;
            dest_pos += run_end - run_start;
            run_start = run_end - 1;
        }
        if run_start < src.len() {
            result[dest_pos] = 0;
            result[dest_pos + 1] = x;
            dest_pos += 2;
        }
        result[dest_pos] = 128;
        result.truncate(dest_pos + 1);
        result
    }

    pub fn a85_encode(src: &[u8]) -> Vec<u8> {
        if src.is_empty() {
            return Vec::new();
        }
        let estimated_size = (src.len() / 4)
            .checked_mul(5)
            .and_then(|size| size.checked_add(4))
            .and_then(|size| size.checked_add(src.len() / 30))
            .and_then(|size| size.checked_add(2))
            .expect("encoded size overflow");
        let mut result = vec![0u8; estimated_size];
        let mut dest_pos = 0usize;
        let mut pos = 0usize;
        let mut line_length = 0u32;
        while src.len() >= 4 && pos < src.len() - 3 {
            let mut value = get_uint32_msb_first(src[pos..pos + 4].try_into().expect("four bytes"));
            pos += 4;
            if value == 0 {
                result[dest_pos] = b'z';
                dest_pos += 1;
                line_length += 1;
            } else {
                for i in (0..=4).rev() {
                    result[dest_pos + i] = (value % 85) as u8 + 33;
                    value /= 85;
                }
                dest_pos += 5;
                line_length += 5;
            }
            if line_length >= 75 {
                result[dest_pos] = b'\r';
                result[dest_pos + 1] = b'\n';
                dest_pos += 2;
                line_length = 0;
            }
        }
        if pos < src.len() {
            let mut value = 0u32;
            let mut count = 0;
            while pos < src.len() {
                value += u32::from(src[pos]) << (8 * (3 - count));
                count += 1;
                pos += 1;
            }
            for i in (0..=4).rev() {
                if i <= count {
                    result[dest_pos + i] = (value % 85) as u8 + 33;
                }
                value /= 85;
            }
            dest_pos += count + 1;
        }
        result[dest_pos] = b'~';
        result[dest_pos + 1] = b'>';
        result.truncate(dest_pos + 2);
        result
    }
}

pub struct RlScanlineDecoder<'a> {
    state: ScanlineState,
    scanline: Vec<u8>,
    src: ScanlineSource<'a>,
    line_bytes: usize,
    src_offset: usize,
    eod: bool,
    operator: u8,
    current_line_valid: bool,
}

impl<'a> RlScanlineDecoder<'a> {
    fn create(
        src: ScanlineSource<'a>,
        width: i32,
        height: i32,
        comps: i32,
        bpc: i32,
    ) -> Option<Self> {
        let bits = u32::try_from(width)
            .ok()?
            .checked_mul(u32::try_from(comps).ok()?)?
            .checked_mul(u32::try_from(bpc).ok()?)?;
        let pitch = (bits / 32 + u32::from(bits % 32 > 0)).checked_mul(4)?;
        let line_bytes = (bits / 8 + u32::from(bits % 8 > 0)) as usize;
        let mut decoder = Self {
            state: ScanlineState::new(width, height, width, height, comps, bpc, pitch),
            scanline: vec![0; pitch as usize],
            src,
            line_bytes,
            src_offset: 0,
            eod: false,
            operator: 0,
            current_line_valid: false,
        };
        if !decoder.check_dest_size() {
            return None;
        }
        Some(decoder)
    }

    fn check_dest_size(&mut self) -> bool {
        let mut i = 0usize;
        let mut dest_size = 0u32;
        while i < self.src.len() {
            let old_size = dest_size;
            if self.src[i] < 128 {
                dest_size = dest_size.wrapping_add(u32::from(self.src[i]) + 1);
                if dest_size < old_size {
                    return false;
                }
                i += self.src[i] as usize + 2;
            } else if self.src[i] > 128 {
                dest_size = dest_size.wrapping_add(257 - u32::from(self.src[i]));
                if dest_size < old_size {
                    return false;
                }
                i += 2;
            } else {
                break;
            }
        }
        u32::try_from(self.line_bytes)
            .ok()
            .and_then(|size| size.checked_mul(u32::try_from(self.state.orig_height).ok()?))
            .is_some_and(|bytes| bytes <= dest_size)
    }

    fn get_next_operator(&mut self) {
        if self.src_offset >= self.src.len() {
            self.operator = 128;
            return;
        }
        self.operator = self.src[self.src_offset];
        self.src_offset += 1;
    }

    fn update_operator(&mut self, used_bytes: u8) {
        if used_bytes == 0 {
            return;
        }
        if self.operator < 128 {
            debug_assert!(u16::from(self.operator) + 1 >= u16::from(used_bytes));
            if used_bytes == self.operator + 1 {
                self.src_offset += used_bytes as usize;
                self.get_next_operator();
                return;
            }
            self.operator -= used_bytes;
            self.src_offset += used_bytes as usize;
            if self.src_offset >= self.src.len() {
                self.operator = 128;
            }
            return;
        }
        let mut count = (257 - u16::from(self.operator)) as u8;
        debug_assert!(count >= used_bytes);
        if used_bytes == count {
            self.src_offset += 1;
            self.get_next_operator();
            return;
        }
        count -= used_bytes;
        self.operator = (257 - u16::from(count)) as u8;
    }
}

impl ScanlineDecoder for RlScanlineDecoder<'_> {
    fn state(&self) -> &ScanlineState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut ScanlineState {
        &mut self.state
    }

    fn rewind(&mut self) -> bool {
        self.scanline.fill(0);
        self.src_offset = 0;
        self.eod = false;
        self.operator = 0;
        self.current_line_valid = false;
        true
    }

    fn get_next_line(&mut self) -> bool {
        if self.src_offset == 0 {
            self.get_next_operator();
        } else if self.eod {
            self.current_line_valid = false;
            return false;
        }
        self.scanline.fill(0);
        let mut col_pos = 0usize;
        let mut eol = false;
        while self.src_offset < self.src.len() && !eol {
            if self.operator < 128 {
                let mut copy_len = self.operator as usize + 1;
                if col_pos + copy_len >= self.line_bytes {
                    copy_len = self.line_bytes - col_pos;
                    eol = true;
                }
                if copy_len >= self.src.len() - self.src_offset {
                    copy_len = self.src.len() - self.src_offset;
                    self.eod = true;
                }
                self.scanline[col_pos..col_pos + copy_len]
                    .copy_from_slice(&self.src[self.src_offset..self.src_offset + copy_len]);
                col_pos += copy_len;
                self.update_operator(copy_len as u8);
            } else if self.operator > 128 {
                let fill = self.src.get(self.src_offset).copied().unwrap_or(0);
                let mut duplicate_len = 257 - self.operator as usize;
                if col_pos + duplicate_len >= self.line_bytes {
                    duplicate_len = self.line_bytes - col_pos;
                    eol = true;
                }
                self.scanline[col_pos..col_pos + duplicate_len].fill(fill);
                col_pos += duplicate_len;
                self.update_operator(duplicate_len as u8);
            } else {
                self.eod = true;
                break;
            }
        }
        self.current_line_valid = !self.scanline.is_empty();
        self.current_line_valid
    }

    fn current_line(&self) -> &[u8] {
        if self.current_line_valid {
            &self.scanline
        } else {
            &[]
        }
    }

    fn get_src_offset(&self) -> u32 {
        u32::try_from(self.src_offset).expect("source offset exceeds uint32")
    }
}

#[cfg(test)]
mod tests {
    use super::BasicModule;

    fn reference_run_length_decode(encoded: &[u8]) -> Vec<u8> {
        let mut decoded = Vec::new();
        let mut pos = 0;
        while pos < encoded.len() {
            let tag = encoded[pos];
            pos += 1;
            match tag {
                0..=127 => {
                    let count = tag as usize + 1;
                    decoded.extend_from_slice(&encoded[pos..pos + count]);
                    pos += count;
                }
                128 => break,
                _ => {
                    decoded.extend(std::iter::repeat_n(encoded[pos], 257 - tag as usize));
                    pos += 1;
                }
            }
        }
        decoded
    }

    #[test]
    fn run_length_round_trip_across_128_byte_boundaries() {
        let repeated = vec![0x7f; 260];
        let cases: [&[u8]; 3] = [&[1], &[2, 2, 2, 2, 1, 2, 3, 4, 5, 6], &repeated];
        for source in cases {
            let encoded = BasicModule::run_length_encode(source);
            assert_eq!(encoded.last(), Some(&128));
            assert_eq!(reference_run_length_decode(&encoded), source);
        }
        let varying: Vec<_> = (0..260).map(|n| n as u8).collect();
        assert_eq!(
            reference_run_length_decode(&BasicModule::run_length_encode(&varying)),
            varying
        );
        assert!(BasicModule::run_length_encode(&[]).is_empty());
    }

    #[test]
    fn ascii85_zero_leftover_and_line_breaks() {
        assert_eq!(BasicModule::a85_encode(&[]), b"");
        assert_eq!(
            BasicModule::a85_encode(&[1, 2, 3, 4, 0, 0, 0, 0]),
            b"!<N?+z~>"
        );
        assert_eq!(BasicModule::a85_encode(&[1, 2, 3, 4, 255]), b"!<N?+rr~>");
        let mut source = [0u8; 131];
        for k in (0..116).step_by(4).chain((120..128).step_by(4)) {
            source[k..k + 4].copy_from_slice(&[1, 2, 3, 4]);
        }
        let encoded = BasicModule::a85_encode(&source);
        assert_eq!(encoded.len(), 166);
        assert_eq!(&encoded[75..77], b"\r\n");
        assert_eq!(&encoded[153..155], b"\r\n");
    }

    #[test]
    fn run_length_scanlines_rewind_and_keep_padding_zero() {
        let encoded = BasicModule::run_length_encode(b"abcdefghij");
        let mut decoder = BasicModule::create_run_length_decoder(&encoded, 5, 2, 1, 8).unwrap();
        assert_eq!(decoder.get_scanline(0), b"abcde\0\0\0");
        assert_eq!(decoder.get_scanline(1), b"fghij\0\0\0");
        assert_eq!(decoder.get_scanline(0), b"abcde\0\0\0");
        assert_eq!(decoder.get_width(), 5);
        assert_eq!(decoder.get_height(), 2);
    }
}

#[cfg(test)]
#[path = "a85_unittest.rs"]
mod upstream_tests_a85;

#[cfg(test)]
#[path = "rle_unittest.rs"]
mod upstream_tests_rle;

#[cfg(test)]
mod scanline_boundary_tests {
    use super::*;

    #[test]
    fn run_length_encoder_preserves_a_single_literal_after_a_repeat() {
        assert_eq!(
            BasicModule::run_length_encode(&[7, 7, 7, 9]),
            [254, 7, 0, 9, 128]
        );
    }

    #[test]
    fn repeat_and_literal_operators_continue_across_padded_rows() {
        let source = [251, b'x', 2, b'a', b'b', b'c', 128];
        let mut decoder = BasicModule::create_run_length_decoder(&source, 3, 3, 1, 8).unwrap();
        assert_eq!(decoder.get_scanline(0), b"xxx\0");
        assert_eq!(decoder.get_src_offset(), 1); // Repeat value retained until its final row.
        assert_eq!(decoder.get_scanline(1), b"xxx\0");
        assert_eq!(decoder.get_src_offset(), 3); // Next operator is already loaded.
        assert_eq!(decoder.get_scanline(2), b"abc\0");
        assert_eq!(decoder.get_src_offset(), 7);
        assert_eq!(decoder.get_scanline(0), b"xxx\0");
    }

    #[test]
    fn truncated_literal_reports_available_bytes_then_invalidates_the_line() {
        let mut decoder =
            RlScanlineDecoder::create(ScanlineSource::Borrowed(&[1, b'A']), 2, 1, 1, 8).unwrap();
        assert!(decoder.current_line().is_empty());
        assert!(decoder.get_next_line());
        assert_eq!(decoder.current_line(), b"A\0\0\0");
        assert_eq!(decoder.get_src_offset(), 2);
        assert!(!decoder.get_next_line());
        assert!(decoder.current_line().is_empty());
        assert!(decoder.rewind());
        assert!(decoder.current_line().is_empty());
        assert!(decoder.get_next_line());
        assert_eq!(decoder.current_line(), b"A\0\0\0");
    }

    #[test]
    fn explicit_end_marker_stops_before_trailing_bytes() {
        let source = [0, b'A', 128, 7, b'X'];
        let mut decoder =
            RlScanlineDecoder::create(ScanlineSource::Borrowed(&source), 1, 1, 1, 8).unwrap();
        assert!(decoder.get_next_line());
        assert_eq!(decoder.current_line(), b"A\0\0\0");
        assert_eq!(decoder.get_src_offset(), 3);
        assert!(decoder.get_next_line());
        assert_eq!(decoder.current_line(), &[0; 4]);
        assert_eq!(decoder.get_src_offset(), 3);
        assert!(!decoder.get_next_line());
        assert!(decoder.current_line().is_empty());
    }
}
