// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Safe port of `bmp/cfx_bmpdecompressor`, `cfx_bmpcontext` and `fx_bmp`.
//! Header parsing is transactional. RGB rows and RLE commands are bounded before
//! writing, including palette validation and top-down/bottom-up orientation.
use super::{fx_codec::ResolutionUnit, progressive_decoder_context::*};

#[derive(Clone, Debug)]
pub struct BmpHeader {
    pub info: ImageInfo,
    pub bit_count: u16,
    pub compression: u32,
    pub data_offset: usize,
    pub source_pitch: usize,
    pub top_down: bool,
    pub masks: [u32; 3],
    pub alpha_mask: u32,
    pub extended_bitfields: bool,
}
pub fn read_header(data: &[u8]) -> CodecResult<BmpHeader> {
    if bytes(data, 0, 2)? != b"BM" {
        return Err(CodecError::InvalidData);
    }
    bytes(data, 0, 18)?;
    let mut offset = le32(data, 10)? as usize;
    let header_size = le32(data, 14)? as usize;
    let header = bytes(data, 14, header_size)?;
    let (width, height, bit_count, compression, colors, top_down, old) = if header_size == 12 {
        (
            le16(header, 4)? as usize,
            le16(header, 6)? as usize,
            le16(header, 10)?,
            0,
            0,
            false,
            true,
        )
    } else if header_size >= 40 {
        let width = le32(header, 4)? as i32;
        let signed_height = le32(header, 8)? as i32;
        let height = signed_height.checked_abs().ok_or(CodecError::InvalidData)?;
        let compression = le32(header, 16)?;
        let colors = le32(header, 32)?;
        if width <= 0 || le16(header, 12)? != 1 {
            return Err(CodecError::InvalidData);
        }
        (
            width as usize,
            height as usize,
            le16(header, 14)?,
            compression,
            colors,
            signed_height < 0,
            false,
        )
    } else {
        return Err(CodecError::InvalidData);
    };
    if !matches!(bit_count, 1 | 2 | 4 | 8 | 16 | 24 | 32)
        || compression > 3
        || (bit_count < 32 && colors > 1u32 << bit_count)
    {
        return Err(CodecError::InvalidData);
    }
    if (compression == 1 && bit_count != 8)
        || (compression == 2 && bit_count != 4)
        || (compression == 3 && !matches!(bit_count, 16 | 32))
    {
        return Err(CodecError::InvalidData);
    }
    let format = match bit_count {
        1 | 2 | 4 | 8 => Format::Indexed,
        16 | 24 => Format::Bgr,
        _ => Format::Bgrx,
    };
    let mut info = ImageInfo::new(width, height, format)?;
    if !old {
        info.attribute.x_dpi = le32(header, 24)? as i32;
        info.attribute.y_dpi = le32(header, 28)? as i32;
    }
    info.attribute.dpi_unit = ResolutionUnit::Meter;
    let source_pitch = (width
        .checked_mul(bit_count as usize)
        .ok_or(CodecError::InvalidData)?
        + 31)
        / 32
        * 4;
    let mut end = 14 + header_size;
    let mut masks = [0x7c00, 0x03e0, 0x001f];
    let mut alpha_mask = 0;
    let extended_bitfields = compression == 3 && header_size >= 52;
    if compression == 3 {
        let mask_offset = if extended_bitfields { 14 + 40 } else { end };
        masks = [
            le32(data, mask_offset)?,
            le32(data, mask_offset + 4)?,
            le32(data, mask_offset + 8)?,
        ];
        if header_size >= 56 {
            alpha_mask = le32(data, 14 + 52)?;
        }
        if !extended_bitfields {
            end += 12;
        }
        if alpha_mask != 0 {
            if masks.iter().any(|&mask| mask & alpha_mask != 0) {
                return Err(CodecError::InvalidData);
            }
            let shifted = alpha_mask >> alpha_mask.trailing_zeros();
            if shifted & shifted.wrapping_add(1) != 0 || (bit_count == 16 && alpha_mask > 0xffff) {
                return Err(CodecError::InvalidData);
            }
            info.format = Format::Bgra;
        }
        if masks[0] & masks[1] != 0 || masks[0] & masks[2] != 0 || masks[1] & masks[2] != 0 {
            return Err(CodecError::InvalidData);
        }
        // Reject noncontiguous/empty masks rather than performing invalid shifts.
        for mask in masks {
            let shifted = mask >> mask.trailing_zeros().min(31);
            if mask == 0
                || shifted & shifted.wrapping_add(1) != 0
                || (bit_count == 16 && mask > 0xffff)
            {
                return Err(CodecError::InvalidData);
            }
        }
    } else if bit_count < 16 {
        let count = if colors == 0 {
            1usize << bit_count
        } else {
            colors as usize
        };
        let channels = if old { 3 } else { 4 };
        let palette = bytes(data, end, count * channels)?;
        end += palette.len();
        for entry in palette.chunks_exact(channels) {
            info.palette.push(
                0xff000000
                    | u32::from(entry[2]) << 16
                    | u32::from(entry[1]) << 8
                    | u32::from(entry[0]),
            );
        }
    }
    // Preserve PDFium's corrupt-offset tolerance without ever seeking backwards
    // into the DIB header or palette.
    offset = offset.max(end);
    Ok(BmpHeader {
        info,
        bit_count,
        compression,
        data_offset: offset,
        source_pitch,
        top_down,
        masks,
        alpha_mask,
        extended_bitfields,
    })
}
fn channel(value: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let value = (value & mask) >> mask.trailing_zeros();
    let bits = mask.count_ones();
    if bits >= 8 {
        (value >> (bits - 8)) as u8
    } else {
        (value << (8 - bits)) as u8
    }
}
// image's bitfield expansion: long masks keep their high eight bits; shorter
// masks span the full channel range, including alpha. Older C++ BMP fields keep
// their selected left-shift behavior through channel().
fn scaled_channel(value: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let mut value = (value & mask) >> mask.trailing_zeros();
    let mut bits = mask.count_ones();
    if bits > 8 {
        value >>= bits - 8;
        bits = 8;
    }
    let max = (1u32 << bits) - 1;
    ((value * 255 + max / 2) / max) as u8
}
pub fn decode(data: &[u8]) -> CodecResult<DecodedImage> {
    let header = read_header(data)?;
    let mut image = DecodedImage::new(header.info.clone())?;
    let width = header.info.width as usize;
    let height = header.info.height as usize;
    if matches!(header.compression, 1 | 2) {
        decode_rle(data, &header, &mut image)?;
        return Ok(image);
    }
    let raster = bytes(
        data,
        header.data_offset,
        header
            .source_pitch
            .checked_mul(height)
            .ok_or(CodecError::InvalidData)?,
    )?;
    for y in 0..height {
        let source = &raster[y * header.source_pitch..(y + 1) * header.source_pitch];
        let row = if header.top_down { y } else { height - 1 - y };
        let output = &mut image.pixels[row * image.pitch..(row + 1) * image.pitch];
        match header.bit_count {
            1 | 2 | 4 | 8 => {
                for (x, dest) in output.iter_mut().enumerate() {
                    let index = match header.bit_count {
                        1 => (source[x / 8] >> (7 - x % 8)) & 1,
                        2 => (source[x / 4] >> (6 - 2 * (x % 4))) & 3,
                        4 => (source[x / 2] >> if x % 2 == 0 { 4 } else { 0 }) & 15,
                        _ => source[x],
                    };
                    if index as usize >= image.info.palette.len() {
                        return Err(CodecError::InvalidData);
                    }
                    *dest = index;
                }
            }
            16 | 32 if header.compression == 3 || header.bit_count == 16 => {
                let channels = image.info.format.components();
                for (x, pixel) in output.chunks_exact_mut(channels).enumerate() {
                    let value = if header.bit_count == 16 {
                        u32::from(le16(source, x * 2)?)
                    } else {
                        le32(source, x * 4)?
                    };
                    let component = |mask| {
                        if header.extended_bitfields {
                            scaled_channel(value, mask)
                        } else {
                            channel(value, mask)
                        }
                    };
                    pixel[..3].copy_from_slice(&[
                        component(header.masks[2]),
                        component(header.masks[1]),
                        component(header.masks[0]),
                    ]);
                    if channels == 4 {
                        pixel[3] = if header.alpha_mask == 0 {
                            255
                        } else {
                            scaled_channel(value, header.alpha_mask)
                        };
                    }
                }
            }
            _ => output.copy_from_slice(&source[..width * image.info.format.components()]),
        }
    }
    Ok(image)
}
fn decode_rle(data: &[u8], header: &BmpHeader, image: &mut DecodedImage) -> CodecResult<()> {
    let mut position = header.data_offset;
    let mut row = 0usize;
    let mut col = 0usize;
    let width = image.info.width as usize;
    let height = image.info.height as usize;
    loop {
        let pair = bytes(data, position, 2)?;
        position += 2;
        let count = pair[0] as usize;
        let code = pair[1];
        if count == 0 {
            match code {
                0 => {
                    if row >= height {
                        return Err(CodecError::InvalidData);
                    }
                    row += 1;
                    col = 0;
                }
                1 => return Ok(()),
                2 => {
                    let delta = bytes(data, position, 2)?;
                    position += 2;
                    col = col
                        .checked_add(delta[0] as usize)
                        .ok_or(CodecError::InvalidData)?;
                    row = row
                        .checked_add(delta[1] as usize)
                        .ok_or(CodecError::InvalidData)?;
                    if col >= width || row >= height {
                        return Err(CodecError::InvalidData);
                    }
                }
                _ => {
                    let count = code as usize;
                    let packed = if header.compression == 2 {
                        count.div_ceil(2)
                    } else {
                        count
                    };
                    let values = bytes(data, position, (packed + 1) & !1)?;
                    position += (packed + 1) & !1;
                    write_run(image, header, row, col, count, |x| {
                        if header.compression == 2 {
                            (values[x / 2] >> if x % 2 == 0 { 4 } else { 0 }) & 15
                        } else {
                            values[x]
                        }
                    })?;
                    col += count;
                }
            }
        } else {
            write_run(image, header, row, col, count, |x| {
                if header.compression == 2 {
                    (code >> if x % 2 == 0 { 4 } else { 0 }) & 15
                } else {
                    code
                }
            })?;
            col += count;
        }
    }
}
fn write_run(
    image: &mut DecodedImage,
    header: &BmpHeader,
    row: usize,
    col: usize,
    count: usize,
    value: impl Fn(usize) -> u8,
) -> CodecResult<()> {
    if row >= image.info.height as usize
        || col
            .checked_add(count)
            .is_none_or(|end| end > image.info.width as usize)
    {
        return Err(CodecError::InvalidData);
    }
    let y = if header.top_down {
        row
    } else {
        image.info.height as usize - 1 - row
    };
    for x in 0..count {
        let index = value(x);
        if index as usize >= image.info.palette.len() {
            return Err(CodecError::InvalidData);
        }
        image.pixels[y * image.pitch + col + x] = index;
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
