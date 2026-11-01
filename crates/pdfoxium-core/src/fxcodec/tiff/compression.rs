/*
 * Copyright (c) 1988-1997 Sam Leffler
 * Copyright (c) 1991-1997 Silicon Graphics, Inc.
 *
 * Permission to use, copy, modify, distribute, and sell this software and
 * its documentation for any purpose is hereby granted without fee, provided
 * that (i) the above copyright notices and this permission notice appear in
 * all copies of the software and related documentation, and (ii) the names of
 * Sam Leffler and Silicon Graphics may not be used in any advertising or
 * publicity relating to the software without the specific, prior written
 * permission of Sam Leffler and Silicon Graphics.
 *
 * THE SOFTWARE IS PROVIDED "AS-IS" AND WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS, IMPLIED OR OTHERWISE, INCLUDING WITHOUT LIMITATION, ANY
 * WARRANTY OF MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.
 *
 * IN NO EVENT SHALL SAM LEFFLER OR SILICON GRAPHICS BE LIABLE FOR
 * ANY SPECIAL, INCIDENTAL, INDIRECT OR CONSEQUENTIAL DAMAGES OF ANY KIND,
 * OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
 * WHETHER OR NOT ADVISED OF THE POSSIBILITY OF DAMAGE, AND ON ANY THEORY OF
 * LIABILITY, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE
 * OF THIS SOFTWARE.
 */

//! The small NeXT/ThunderScan codecs not exposed by the published TIFF crate.
//! The pinned literal/run/delta algorithms operate on bounded output rows.
//! CCITT reuses the engine's existing pure Rust fax decoder.
use super::super::progressive_decoder_context::{allocate, bytes, CodecError, CodecResult};

pub(super) fn decode(
    data: &[u8],
    width: usize,
    height: usize,
    bits: u8,
    compression: u32,
    fill_order: u32,
    options: u32,
) -> CodecResult<Vec<u8>> {
    match compression {
        32766 if bits == 2 => next(data, width, height),
        32809 if bits == 4 => thunder(data, width, height),
        2 | 3 | 4 | 32771 if bits == 1 => {
            fax(data, width, height, compression, fill_order, options)
        }
        _ => Err(CodecError::Unsupported),
    }
}
fn next(data: &[u8], width: usize, height: usize) -> CodecResult<Vec<u8>> {
    let pitch = width.div_ceil(4);
    let mut output = allocate(pitch.checked_mul(height).ok_or(CodecError::InvalidData)?)?;
    output.fill(255);
    let mut pos = 0;
    for row in output.chunks_exact_mut(pitch) {
        let code = *bytes(data, pos, 1)?.first().unwrap();
        pos += 1;
        match code {
            0 => {
                row.copy_from_slice(bytes(data, pos, pitch)?);
                pos += pitch;
            }
            0x40 => {
                let range = bytes(data, pos, 4)?;
                pos += 4;
                let start = u16::from_be_bytes([range[0], range[1]]) as usize;
                let length = u16::from_be_bytes([range[2], range[3]]) as usize;
                row.get_mut(start..start.checked_add(length).ok_or(CodecError::InvalidData)?)
                    .ok_or(CodecError::InvalidData)?
                    .copy_from_slice(bytes(data, pos, length)?);
                pos += length;
            }
            mut code => {
                let mut pixel = 0;
                loop {
                    let gray = code >> 6;
                    let count = (code & 63) as usize;
                    // Native clamps an oversized final run to this scanline.
                    for _ in 0..count.min(width - pixel) {
                        let shift = 6 - 2 * (pixel % 4);
                        row[pixel / 4] = (row[pixel / 4] & !(3 << shift)) | (gray << shift);
                        pixel += 1;
                    }
                    if pixel == width {
                        break;
                    }
                    code = bytes(data, pos, 1)?[0];
                    pos += 1;
                }
            }
        }
    }
    Ok(output)
}
fn put(row: &mut [u8], width: usize, pixel: &mut usize, last: &mut u8, value: u8) {
    *last = value & 15;
    if *pixel < width {
        if *pixel % 2 == 0 {
            row[*pixel / 2] = *last << 4
        } else {
            row[*pixel / 2] |= *last
        };
        *pixel += 1;
    }
}
fn thunder(data: &[u8], width: usize, height: usize) -> CodecResult<Vec<u8>> {
    let pitch = width.div_ceil(2);
    let mut output = allocate(pitch.checked_mul(height).ok_or(CodecError::InvalidData)?)?;
    let mut pos = 0;
    for row in output.chunks_exact_mut(pitch) {
        let mut pixel = 0;
        let mut last = 0u8;
        while pixel < width {
            let code = bytes(data, pos, 1)?[0];
            pos += 1;
            match code & 0xc0 {
                0 => {
                    let count = (code & 63) as usize;
                    if count > width - pixel {
                        return Err(CodecError::InvalidData);
                    }
                    let value = last;
                    for _ in 0..count {
                        put(row, width, &mut pixel, &mut last, value);
                    }
                }
                0x40 => {
                    let deltas = [0i8, 1, 0, -1];
                    let mut value = last;
                    for shift in [4, 2, 0] {
                        let d = (code >> shift) & 3;
                        if d != 2 {
                            value = value.wrapping_add_signed(deltas[d as usize]) & 15;
                            put(row, width, &mut pixel, &mut last, value);
                        }
                    }
                }
                0x80 => {
                    let deltas = [0i8, 1, 2, 3, 0, -3, -2, -1];
                    let mut value = last;
                    for shift in [3, 0] {
                        let d = (code >> shift) & 7;
                        if d != 4 {
                            value = value.wrapping_add_signed(deltas[d as usize]) & 15;
                            put(row, width, &mut pixel, &mut last, value);
                        }
                    }
                }
                _ => put(row, width, &mut pixel, &mut last, code),
            }
        }
    }
    Ok(output)
}
fn fax(
    data: &[u8],
    width: usize,
    height: usize,
    compression: u32,
    fill_order: u32,
    options: u32,
) -> CodecResult<Vec<u8>> {
    if compression == 32771 {
        let pitch = width.div_ceil(8);
        let mut output = allocate(pitch.checked_mul(height).ok_or(CodecError::InvalidData)?)?;
        let mut position = 0;
        for row in output.chunks_exact_mut(pitch) {
            let (decoded, consumed) = fax_data(
                data.get(position..).ok_or(CodecError::InvalidData)?,
                width,
                1,
                2,
                fill_order,
                options,
            )?;
            row.copy_from_slice(&decoded);
            position = position
                .checked_add(consumed.next_multiple_of(2))
                .ok_or(CodecError::InvalidData)?;
        }
        return Ok(output);
    }
    Ok(fax_data(data, width, height, compression, fill_order, options)?.0)
}
fn fax_data(
    data: &[u8],
    width: usize,
    height: usize,
    compression: u32,
    fill_order: u32,
    options: u32,
) -> CodecResult<(Vec<u8>, usize)> {
    use hayro_ccitt::{DecodeSettings, DecoderContext, EncodingMode};
    struct Sink {
        pixels: Vec<u8>,
        pitch: usize,
        width: usize,
        height: usize,
        row: usize,
        column: usize,
        invalid: bool,
    }
    impl hayro_ccitt::Decoder for Sink {
        fn push_pixels(&mut self, white: bool, count: u32) {
            if self.row >= self.height || count as usize > self.width.saturating_sub(self.column) {
                self.invalid = true;
                return;
            }
            for _ in 0..count {
                if !white {
                    self.pixels[self.row * self.pitch + self.column / 8] |=
                        128 >> (self.column % 8);
                }
                self.column += 1;
            }
        }
        fn next_line(&mut self) {
            if self.column != self.width {
                self.invalid = true;
            }
            self.row += 1;
            self.column = 0;
        }
    }
    let pitch = width.div_ceil(8);
    let mut sink = Sink {
        pixels: allocate(pitch.checked_mul(height).ok_or(CodecError::InvalidData)?)?,
        pitch,
        width,
        height,
        row: 0,
        column: 0,
        invalid: false,
    };
    let settings = DecodeSettings {
        columns: width as u32,
        rows: height as u32,
        end_of_block: false,
        end_of_line: compression == 3,
        rows_are_byte_aligned: compression == 2,
        encoding: match compression {
            4 => EncodingMode::Group4,
            3 if options & 1 != 0 => EncodingMode::Group3_2D { k: 2 },
            _ => EncodingMode::Group3_1D,
        },
        invert_black: false,
    };
    let reversed;
    let source = if fill_order == 2 {
        reversed = data.iter().map(|b| b.reverse_bits()).collect::<Vec<_>>();
        reversed.as_slice()
    } else {
        data
    };
    let consumed = hayro_ccitt::decode(source, &mut sink, &mut DecoderContext::new(settings))
        .map_err(|_| CodecError::InvalidData)?;
    if sink.invalid || sink.row != height {
        return Err(CodecError::InvalidData);
    }
    Ok((sink.pixels, consumed))
}
