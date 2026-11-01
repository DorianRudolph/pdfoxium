// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors.
// Use of this source code is governed by a BSD-style license that can be
// found in ../LICENSE-PDFIUM.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Packed-row control flow from PDFium's core/fxcodec/fax/faxmodule.cpp.

use crate::state_machine::{BLACK_STATES, INVALID, TERMINAL, VALUE_MASK, WHITE_STATES};
use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;

enum Source<'a> {
    Borrowed(&'a [u8]),
    Shared(Rc<Vec<u8>>),
}

impl core::ops::Deref for Source<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Borrowed(data) => data,
            Self::Shared(data) => data,
        }
    }
}

struct Stream<'a> {
    data: Source<'a>,
    bitpos: u32,
}

impl Stream<'_> {
    fn bitsize(&self) -> u32 {
        (self.data.len() * 8) as u32
    }
    fn next(&mut self) -> Option<bool> {
        if self.bitpos >= self.bitsize() {
            return None;
        }
        let pos = self.bitpos;
        self.bitpos += 1;
        Some(self.data[(pos / 8) as usize] & (1 << (7 - pos % 8)) != 0)
    }

    // FaxGetRun reads one makeup/terminating code, without opposite-color
    // fallback. Invalid prefixes still consume through the native code limit.
    fn run(&mut self, white: bool) -> Option<i32> {
        let states = if white {
            &WHITE_STATES[..]
        } else {
            &BLACK_STATES[..]
        };
        let mut state = Some(0usize);
        for _ in 0..if white { 12 } else { 13 } {
            let bit = self.next()?;
            if let Some(index) = state {
                let transition = if bit {
                    states[index].on_1
                } else {
                    states[index].on_0
                };
                if transition == INVALID {
                    state = None;
                } else if transition & TERMINAL != 0 {
                    return Some(i32::from(transition & VALUE_MASK));
                } else {
                    state = Some(transition as usize);
                }
            }
        }
        None
    }

    fn skip_eol(&mut self) {
        let start = self.bitpos;
        while let Some(bit) = self.next() {
            if !bit {
                continue;
            }
            if self.bitpos - start <= 11 {
                self.bitpos = start;
            }
            return;
        }
    }
}

fn find_bit(data: &[u8], max_pos: i32, mut start: i32, bit: bool) -> i32 {
    assert!(start >= 0);
    if start >= max_pos {
        return max_pos;
    }
    let xor = if bit { 0u8 } else { 255u8 };
    let offset = start % 8;
    if offset != 0 {
        let byte = start / 8;
        let value = (data[byte as usize] ^ xor) & (255 >> offset);
        if value != 0 {
            return byte * 8 + value.leading_zeros() as i32;
        }
        start += 7;
    }
    let max_byte = (max_pos as u32).div_ceil(8) as i32;
    let mut byte = start / 8;
    let word_xor = if bit { 0 } else { u32::MAX };
    while byte + 4 <= max_byte {
        let pos = byte as usize;
        let word = u32::from_be_bytes(data[pos..pos + 4].try_into().unwrap()) ^ word_xor;
        if word != 0 {
            return (byte * 8 + word.leading_zeros() as i32).min(max_pos);
        }
        byte += 4;
    }
    while byte < max_byte {
        let value = data[byte as usize] ^ xor;
        if value != 0 {
            return (byte * 8 + value.leading_zeros() as i32).min(max_pos);
        }
        byte += 1;
    }
    max_pos
}

// Vertical mode needs only b1. Keep the b2 search in the pass-mode wrapper so
// vertical codes do not scan the following run only to discard its endpoint.
#[inline]
fn find_b1(reference: &[u8], columns: i32, a0: i32, color: bool) -> (i32, bool) {
    let mut first = a0 < 0 || reference[(a0 / 8) as usize] & (1 << (7 - a0 % 8)) != 0;
    let mut b1 = find_bit(reference, columns, a0 + 1, !first);
    if b1 >= columns {
        return (columns, first);
    }
    if first == !color {
        b1 = find_bit(reference, columns, b1 + 1, first);
        first = !first;
    }
    if b1 >= columns {
        return (columns, first);
    }
    (b1, first)
}

fn find_b1_b2(reference: &[u8], columns: i32, a0: i32, color: bool) -> (i32, i32) {
    let (b1, first) = find_b1(reference, columns, a0, color);
    if b1 >= columns {
        return (columns, columns);
    }
    (b1, find_bit(reference, columns, b1 + 1, first))
}

fn fill_bits(dest: &mut [u8], columns: i32, start: i32, end: i32) {
    let start = start.max(0);
    let end = end.clamp(0, columns);
    if start >= end {
        return;
    }
    let first = (start / 8) as usize;
    let last = ((end - 1) / 8) as usize;
    // Subtract the sum of the selected powers of two in one operation.
    // Wrapping subtraction is associative even for an already modified byte.
    let first_mask = u8::MAX >> (start % 8);
    let last_mask = u8::MAX << (7 - (end - 1) % 8);
    if first == last {
        dest[first] = dest[first].wrapping_sub(first_mask & last_mask);
        return;
    }
    dest[first] = dest[first].wrapping_sub(first_mask);
    dest[last] = dest[last].wrapping_sub(last_mask);
    if last > first + 1 {
        dest[first + 1..last].fill(0);
    }
}

fn run_length(stream: &mut Stream<'_>, white: bool, columns: i32) -> Option<i32> {
    let mut length = 0i32;
    loop {
        let run = stream.run(white)?;
        length = length.checked_add(run)?;
        if length > columns {
            return None;
        }
        if run < 64 {
            return Some(length);
        }
    }
}

fn get_g4_row(
    stream: &mut Stream<'_>,
    dest: &mut [u8],
    reference: &[u8],
    columns: i32,
) -> Option<()> {
    let mut a0: i32 = -1;
    let mut color = true;
    loop {
        let mut delta = 0;
        if !stream.next()? {
            let bit1 = stream.next()?;
            let bit2 = stream.next()?;
            if bit1 {
                delta = if bit2 { 1 } else { -1 };
            } else if bit2 {
                let mut length = run_length(stream, color, columns)?;
                if a0 < 0 {
                    length += 1;
                }
                let a1 = a0.checked_add(length)?;
                if !color {
                    fill_bits(dest, columns, a0, a1);
                }
                let length = run_length(stream, !color, columns)?;
                let a2 = a1.checked_add(length)?;
                if color {
                    fill_bits(dest, columns, a1, a2);
                }
                a0 = a2;
                if a0 < columns {
                    continue;
                }
                return Some(());
            } else {
                if stream.next()? {
                    let (_, b2) = find_b1_b2(reference, columns, a0, color);
                    if !color {
                        fill_bits(dest, columns, a0, b2);
                    }
                    if b2 >= columns {
                        return Some(());
                    }
                    a0 = b2;
                    continue;
                }
                let bit1 = stream.next()?;
                let bit2 = stream.next()?;
                if bit1 {
                    delta = if bit2 { 2 } else { -2 };
                } else if bit2 {
                    delta = if stream.next()? { 3 } else { -3 };
                } else {
                    if stream.next()? {
                        stream.bitpos = stream.bitpos.wrapping_add(3);
                        continue;
                    }
                    stream.bitpos = stream.bitpos.wrapping_add(5);
                    return None;
                }
            }
        }
        let (b1, _) = find_b1(reference, columns, a0, color);
        let a1 = b1.checked_add(delta)?;
        if !color {
            fill_bits(dest, columns, a0, a1);
        }
        if a1 >= columns {
            return Some(());
        }
        if a0 >= a1 {
            return None;
        }
        a0 = a1;
        color = !color;
    }
}

fn get_1d_line(stream: &mut Stream<'_>, dest: &mut [u8], columns: i32) {
    let mut color = true;
    let mut start = 0;
    loop {
        if stream.bitpos >= stream.bitsize() {
            return;
        }
        let mut length = 0i32;
        loop {
            let Some(run) = stream.run(color) else {
                while let Some(bit) = stream.next() {
                    if bit {
                        return;
                    }
                }
                return;
            };
            length += run;
            if length > columns {
                return;
            }
            if run < 64 {
                break;
            }
        }
        if !color {
            fill_bits(dest, columns, start, start + length);
        }
        start += length;
        if start >= columns {
            return;
        }
        color = !color;
    }
}

/// PDFium's synchronous FaxDecoder row state, borrowing encoded input.
/// Rows use native white-one polarity and DWORD padding before optional inversion.
pub struct FaxDecoder<'a> {
    stream: Stream<'a>,
    encoding: i32,
    byte_align: bool,
    end_of_line: bool,
    black: bool,
    width: u32,
    height: u32,
    scanline: Vec<u8>,
    reference: Vec<u8>,
}

impl<'a> FaxDecoder<'a> {
    /// Create a decoder with the native dimension limits and initial Rewind state.
    pub fn new(
        data: &'a [u8],
        width: u32,
        height: u32,
        encoding: i32,
        end_of_line: bool,
        byte_align: bool,
        black: bool,
    ) -> Option<Self> {
        if width == 0
            || height == 0
            || width > 65535
            || height > 65535
            || data.len() > (u32::MAX / 8) as usize
        {
            return None;
        }
        let pitch = width.div_ceil(32) as usize * 4;
        Some(Self {
            stream: Stream {
                data: Source::Borrowed(data),
                bitpos: 0,
            },
            encoding,
            byte_align,
            end_of_line,
            black,
            width,
            height,
            scanline: vec![0; pitch],
            reference: vec![255; pitch],
        })
    }

    /// Retain the exact source allocation used by the native-style accessor.
    /// Only ownership changes; packet position and reference rows are preserved.
    pub fn retain_source(self, source: Rc<Vec<u8>>) -> FaxDecoder<'static> {
        assert_eq!(self.stream.data.as_ptr(), source.as_ptr());
        assert_eq!(self.stream.data.len(), source.len());
        FaxDecoder {
            stream: Stream {
                data: Source::Shared(source),
                bitpos: self.stream.bitpos,
            },
            encoding: self.encoding,
            byte_align: self.byte_align,
            end_of_line: self.end_of_line,
            black: self.black,
            width: self.width,
            height: self.height,
            scanline: self.scanline,
            reference: self.reference,
        }
    }

    /// Decoded pixel width.
    pub fn width(&self) -> u32 {
        self.width
    }
    /// Declared decoder row count; GetNextLine may be called beyond this count.
    pub fn height(&self) -> u32 {
        self.height
    }
    /// DWORD-aligned output pitch.
    pub fn pitch(&self) -> usize {
        self.scanline.len()
    }
    /// Reset reference pixels and position. A previously disabled byte alignment
    /// remains disabled, matching native Rewind.
    pub fn rewind(&mut self) {
        self.reference.fill(255);
        self.stream.bitpos = 0;
    }
    /// The source offset rounded up to bytes and clamped to available input.
    pub fn source_offset(&self) -> u32 {
        self.stream
            .bitpos
            .div_ceil(8)
            .min(self.stream.data.len() as u32)
    }
    /// Borrow the row buffer. Callers track whether the last decode returned a row.
    pub fn current_line(&self) -> &[u8] {
        &self.scanline
    }
    /// Decode the next row, preserving partial rows on invalid codes. None is
    /// returned only when the initial optional EOL scan reaches the input end.
    pub fn next_line(&mut self) -> Option<&[u8]> {
        self.stream.skip_eol();
        if self.stream.bitpos >= self.stream.bitsize() {
            return None;
        }
        self.scanline.fill(255);
        if self.encoding < 0 {
            let _ = get_g4_row(
                &mut self.stream,
                &mut self.scanline,
                &self.reference,
                self.width as i32,
            );
            self.reference.copy_from_slice(&self.scanline);
        } else if self.encoding == 0 {
            get_1d_line(&mut self.stream, &mut self.scanline, self.width as i32);
        } else {
            if self.stream.next()? {
                get_1d_line(&mut self.stream, &mut self.scanline, self.width as i32);
            } else {
                let _ = get_g4_row(
                    &mut self.stream,
                    &mut self.scanline,
                    &self.reference,
                    self.width as i32,
                );
            }
            self.reference.copy_from_slice(&self.scanline);
        }
        if self.end_of_line {
            self.stream.skip_eol();
        }
        if self.byte_align && self.stream.bitpos < self.stream.bitsize() {
            let mut pos = self.stream.bitpos;
            let aligned = pos.div_ceil(8) * 8;
            while self.byte_align && pos < aligned {
                if self.stream.data[(pos / 8) as usize] & (1 << (7 - pos % 8)) != 0 {
                    self.byte_align = false;
                } else {
                    pos += 1;
                }
            }
            if self.byte_align {
                self.stream.bitpos = aligned;
            }
        }
        if self.black {
            for byte in &mut self.scanline {
                *byte = !*byte;
            }
        }
        Some(&self.scanline)
    }
}

/// Native FaxModule::FaxG4Decode, retaining partial rows and the exact ending
/// bit position. The destination must contain exactly height * pitch bytes.
/// This helper has the native image allocation bounds rather than the fax
/// scanline decoder's 65535 dimension limit.
pub fn decode_g4(
    data: &[u8],
    starting_bitpos: u32,
    width: u32,
    height: u32,
    pitch: usize,
    dest: &mut [u8],
) -> Option<u32> {
    let columns = i32::try_from(width).ok()?;
    if width == 0
        || height == 0
        || pitch < width.div_ceil(8) as usize
        || pitch.checked_mul(height as usize)? != dest.len()
        || data.len() > (u32::MAX / 8) as usize
    {
        return None;
    }
    let mut reference = vec![255; pitch];
    let mut stream = Stream {
        data: Source::Borrowed(data),
        bitpos: starting_bitpos,
    };
    for line in dest.chunks_exact_mut(pitch) {
        line.fill(255);
        let _ = get_g4_row(&mut stream, line, &reference, columns);
        reference.copy_from_slice(line);
    }
    Some(stream.bitpos)
}

/// Packed native G4 rows for codecs whose destination storage uses words.
/// The two row buffers avoid allocating or copying an entire image for conversion.
pub struct G4Decoder<'a> {
    stream: Stream<'a>,
    columns: i32,
    scanline: Vec<u8>,
    reference: Vec<u8>,
}

impl<'a> G4Decoder<'a> {
    /// Create a native G4 row decoder at an exact starting bit position.
    pub fn new(data: &'a [u8], starting_bitpos: u32, width: u32, pitch: usize) -> Option<Self> {
        let columns = i32::try_from(width).ok()?;
        if width == 0 || pitch < width.div_ceil(8) as usize || data.len() > (u32::MAX / 8) as usize
        {
            return None;
        }
        Some(Self {
            stream: Stream {
                data: Source::Borrowed(data),
                bitpos: starting_bitpos,
            },
            columns,
            scanline: vec![0; pitch],
            reference: vec![255; pitch],
        })
    }
    /// Decode a row, including the native white/partial result after an error or EOF.
    pub fn next_line(&mut self) -> &[u8] {
        self.scanline.fill(255);
        let _ = get_g4_row(
            &mut self.stream,
            &mut self.scanline,
            &self.reference,
            self.columns,
        );
        self.reference.copy_from_slice(&self.scanline);
        &self.scanline
    }
    /// Ending bit position, before the caller aligns to the next byte.
    pub fn bit_position(&self) -> u32 {
        self.stream.bitpos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_bits_masks_preserve_wrapping_subtraction_and_middle_zero_fill() {
        for columns in [1, 7, 8, 9, 16, 31, 32, 33, 63, 64, 65] {
            for start in -2..columns + 2 {
                for end in -2..columns + 2 {
                    for initial in [0u8, 0x35, 0x80, 0xff] {
                        let mut expected = [initial; 12];
                        let mut actual = expected;
                        let lo = start.max(0);
                        let hi = end.clamp(0, columns);
                        if lo < hi {
                            let first = lo / 8;
                            let last = (hi - 1) / 8;
                            for pos in lo..hi {
                                let byte = pos as usize / 8;
                                if byte as i32 > first && (byte as i32) < last {
                                    expected[byte] = 0;
                                } else {
                                    expected[byte] = expected[byte].wrapping_sub(0x80 >> (pos % 8));
                                }
                            }
                        }
                        fill_bits(&mut actual, columns, start, end);
                        assert_eq!(actual, expected, "{columns} {start} {end} {initial}");
                    }
                }
            }
        }
    }

    #[test]
    fn vertical_and_pass_endpoints_match_bitwise_reference_transitions() {
        fn bit(data: &[u8], pos: i32) -> bool {
            data[pos as usize / 8] & (0x80 >> (pos % 8)) != 0
        }
        fn scan(data: &[u8], columns: i32, start: i32, target: bool) -> i32 {
            if start >= columns {
                return columns;
            }
            // Native FindBit searches the rest of an initial partial byte
            // before clamping subsequent byte/word searches to columns.
            let partial_end = if start % 8 != 0 {
                (start / 8 + 1) * 8
            } else {
                start
            };
            (start..partial_end.max(columns))
                .find(|&pos| bit(data, pos) == target)
                .unwrap_or(columns)
        }
        for columns in [1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65] {
            // Every 16-bit reference row, plus byte/word boundaries and
            // unaligned widths with both padding polarities.
            let patterns = if columns == 16 { 65536 } else { 256 };
            for pattern in 0..patterns {
                let mut reference = [0u8; 9];
                for (i, byte) in reference.iter_mut().enumerate() {
                    *byte = if columns == 16 {
                        (pattern >> (8 * (1 - i % 2))) as u8
                    } else {
                        (pattern as u32).wrapping_mul(73 + i as u32 * 32) as u8
                    };
                }
                for a0 in -1..columns {
                    for color in [false, true] {
                        let mut first = a0 < 0 || bit(&reference, a0);
                        let mut b1 = scan(&reference, columns, a0 + 1, !first).min(columns);
                        if b1 < columns && first == !color {
                            b1 = scan(&reference, columns, b1 + 1, first).min(columns);
                            first = !first;
                        }
                        let b2 = if b1 >= columns {
                            columns
                        } else {
                            scan(&reference, columns, b1 + 1, first)
                        };
                        assert_eq!(find_b1(&reference, columns, a0, color).0, b1);
                        assert_eq!(find_b1_b2(&reference, columns, a0, color), (b1, b2));
                    }
                }
            }
        }
    }
}
