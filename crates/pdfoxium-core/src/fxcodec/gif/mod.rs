// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! GIF header, extension, frame-table, LZW and first-frame output ports from
//! `cfx_gif{,context}.{h,cpp}`. Metadata for every frame is retained; native XFA
//! renders frame zero. GIF disposal/delay metadata is preserved for callers.
pub mod lzw_decompressor;
use super::cfx_codec_memory::CfxCodecMemory;
use super::progressive_decoder_context::*;
use lzw_decompressor::{LzwDecompressor, LzwStatus};
pub const GIF_SIGNATURE87: &[u8; 6] = b"GIF87a";
pub const GIF_SIGNATURE89: &[u8; 6] = b"GIF89a";
#[derive(Clone, Debug, Default)]
pub struct GraphicControl {
    pub delay: u16,
    pub transparent_index: Option<u8>,
    pub disposal: u8,
    pub user_input: bool,
}
#[derive(Clone, Debug)]
pub struct GifFrame {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub interlaced: bool,
    pub local_palette: Vec<u32>,
    pub palette_exp: u8,
    pub code_exp: u8,
    pub data_position: usize,
    pub data_end: usize,
    pub control: Option<GraphicControl>,
}
#[derive(Clone, Debug)]
pub struct GifHeader {
    pub info: ImageInfo,
    pub background_index: u8,
    pub palette_exp: u8,
    pub sort: bool,
    pub color_resolution: u8,
    pub data_position: usize,
}
/// Low-level all-or-none header reader, retaining native cursor behavior.
#[derive(Default)]
pub struct CfxGifContext {
    pub input_buffer: CfxCodecMemory,
    pub width: u16,
    pub height: u16,
    pub global_palette: Vec<u32>,
    pub background_index: u8,
    pub palette_exp: u8,
    pub sort: bool,
    pub color_resolution: u8,
}
impl CfxGifContext {
    pub fn set_input_buffer(&mut self, buffer: CfxCodecMemory) {
        self.input_buffer = buffer;
    }
    pub fn get_avail_input(&self) -> usize {
        self.input_buffer.unconsumed().len()
    }
    pub fn read_all_or_none(&mut self, dest: &mut [u8]) -> bool {
        !dest.is_empty() && self.input_buffer.read_all_or_none(dest)
    }
    pub fn read_gif_signature(&mut self) -> Status {
        let mut signature = [0; 6];
        if !self.read_all_or_none(&mut signature) {
            return Status::Continue;
        }
        if &signature == GIF_SIGNATURE87 || &signature == GIF_SIGNATURE89 {
            Status::Success
        } else {
            Status::Error
        }
    }
    pub fn read_logical_screen_descriptor(&mut self) -> Status {
        let start = self.input_buffer.position();
        let mut lsd = [0; 7];
        if !self.read_all_or_none(&mut lsd) {
            return Status::Continue;
        }
        if lsd[4] & 128 != 0 {
            let count = 2usize << (lsd[4] & 7);
            if lsd[5] as usize >= count {
                return Status::Error;
            }
            let mut palette = vec![0; count * 3];
            if !self.read_all_or_none(&mut palette) {
                self.input_buffer.seek(start);
                return Status::Continue;
            }
            self.global_palette = palette
                .chunks_exact(3)
                .map(|p| {
                    0xff000000 | u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2])
                })
                .collect();
            self.background_index = lsd[5];
            self.palette_exp = lsd[4] & 7;
            self.sort = lsd[4] & 8 != 0;
            self.color_resolution = (lsd[4] >> 4) & 7;
        }
        self.width = u16::from_le_bytes([lsd[0], lsd[1]]);
        self.height = u16::from_le_bytes([lsd[2], lsd[3]]);
        Status::Success
    }
    pub fn read_header(&mut self) -> Status {
        let status = self.read_gif_signature();
        if status != Status::Success {
            return status;
        }
        self.read_logical_screen_descriptor()
    }
}
pub fn read_header(data: &[u8]) -> CodecResult<GifHeader> {
    let signature = bytes(data, 0, 6)?;
    if signature != GIF_SIGNATURE87 && signature != GIF_SIGNATURE89 {
        return Err(CodecError::InvalidData);
    }
    let lsd = bytes(data, 6, 7)?;
    let flags = lsd[4];
    let mut info = ImageInfo::new(
        le16(lsd, 0)? as usize,
        le16(lsd, 2)? as usize,
        Format::Indexed,
    )?;
    let mut position = 13;
    let mut background = 0;
    if flags & 128 != 0 {
        let count = 2usize << (flags & 7);
        if lsd[5] as usize >= count {
            return Err(CodecError::InvalidData);
        }
        info.palette = read_palette(data, &mut position, count)?;
        background = lsd[5];
    }
    Ok(GifHeader {
        info,
        background_index: background,
        palette_exp: flags & 7,
        sort: flags & 8 != 0,
        color_resolution: (flags >> 4) & 7,
        data_position: position,
    })
}
fn read_palette(data: &[u8], position: &mut usize, count: usize) -> CodecResult<Vec<u32>> {
    let palette = bytes(data, *position, count * 3)?;
    *position += palette.len();
    Ok(palette
        .chunks_exact(3)
        .map(|p| 0xff000000 | u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2]))
        .collect())
}
fn skip_blocks(data: &[u8], position: &mut usize) -> CodecResult<()> {
    loop {
        let size = bytes(data, *position, 1)?[0] as usize;
        *position += 1;
        if size == 0 {
            return Ok(());
        }
        bytes(data, *position, size)?;
        *position += size;
    }
}
pub fn get_frames(data: &[u8], header: &GifHeader) -> CodecResult<Vec<GifFrame>> {
    let mut position = header.data_position;
    let mut control = None;
    let mut frames = Vec::new();
    while let Some(tag) = data.get(position).copied() {
        position += 1;
        match tag {
            0x3b => return Ok(frames),
            0x21 => {
                let label = bytes(data, position, 1)?[0];
                position += 1;
                if label == 0xf9 {
                    let gce = bytes(data, position, 6)?;
                    position += 6;
                    if gce[0] != 4 || gce[5] != 0 {
                        return Err(CodecError::InvalidData);
                    }
                    control = Some(GraphicControl {
                        delay: le16(gce, 2)?,
                        transparent_index: (gce[1] & 1 != 0).then_some(gce[4]),
                        disposal: (gce[1] >> 2) & 7,
                        user_input: gce[1] & 2 != 0,
                    });
                } else {
                    if label == 1 {
                        control = None;
                    }
                    skip_blocks(data, &mut position)?;
                }
            }
            0x2c => {
                let descriptor = bytes(data, position, 9)?;
                position += 9;
                let left = le16(descriptor, 0)?;
                let top = le16(descriptor, 2)?;
                let width = le16(descriptor, 4)?;
                let height = le16(descriptor, 6)?;
                let flags = descriptor[8];
                if u32::from(left) + u32::from(width) > header.info.width as u32
                    || u32::from(top) + u32::from(height) > header.info.height as u32
                {
                    return Err(CodecError::InvalidData);
                }
                let local_palette = if flags & 128 != 0 {
                    read_palette(data, &mut position, 2usize << (flags & 7))?
                } else {
                    Vec::new()
                };
                let palette_exp = if local_palette.is_empty() {
                    header.palette_exp
                } else {
                    flags & 7
                };
                if control
                    .as_ref()
                    .and_then(|gce: &GraphicControl| gce.transparent_index)
                    .is_some_and(|index| index as usize >= 2usize << palette_exp)
                {
                    return Err(CodecError::InvalidData);
                }
                let code_exp = bytes(data, position, 1)?[0];
                position += 1;
                let data_position = position;
                skip_blocks(data, &mut position)?;
                frames.push(GifFrame {
                    left,
                    top,
                    width,
                    height,
                    interlaced: flags & 64 != 0,
                    local_palette,
                    palette_exp,
                    code_exp,
                    data_position,
                    data_end: position,
                    control: control.take(),
                });
            }
            _ => {} // PDFium tolerates non-standard record tags and absent trailer.
        }
    }
    Ok(frames)
}
pub fn decode_frame(
    data: &[u8],
    header: &GifHeader,
    frame: &GifFrame,
    alpha: bool,
) -> CodecResult<DecodedImage> {
    if frame.width == 0 || frame.height == 0 {
        return Err(CodecError::InvalidData);
    }
    let mut info = header.info.clone();
    if !frame.local_palette.is_empty() {
        info.palette = frame.local_palette.clone();
    }
    if info.palette.is_empty() {
        return Err(CodecError::InvalidData);
    }
    let transparent = frame
        .control
        .as_ref()
        .and_then(|gce| gce.transparent_index)
        .filter(|index| (*index as usize) < info.palette.len());
    if let Some(index) = transparent {
        info.palette[index as usize] &= 0x00ffffff;
    }
    let background = if alpha {
        transparent.unwrap_or(header.background_index)
    } else {
        header.background_index
    };
    if background as usize >= info.palette.len() {
        return Err(CodecError::InvalidData);
    }
    let mut image = DecodedImage::new(info)?;
    image.pixels.fill(background);
    let size = (frame.width as usize)
        .checked_mul(frame.height as usize)
        .ok_or(CodecError::InvalidData)?;
    let mut indexes = allocate(size.checked_add(1).ok_or(CodecError::InvalidData)?)?;
    let mut decoder = LzwDecompressor::create(frame.palette_exp, frame.code_exp)
        .ok_or(CodecError::InvalidData)?;
    let mut position = frame.data_position;
    let mut written = 0;
    let mut ended = false;
    while position < frame.data_end {
        let count = bytes(data, position, 1)?[0] as usize;
        position += 1;
        if count == 0 {
            break;
        }
        let block = bytes(data, position, count)?;
        position += count;
        let (status, _, count) = decoder.decode(block, &mut indexes[written..]);
        written += count;
        if written > size {
            return Err(CodecError::InvalidData);
        }
        match status {
            LzwStatus::Error | LzwStatus::InsufficientDestSize => {
                return Err(CodecError::InvalidData)
            }
            LzwStatus::Success => {
                ended = true;
                break;
            }
            LzwStatus::Unfinished => {}
        }
    }
    if !ended || written != size {
        return Err(CodecError::InvalidData);
    }
    let mut source_row = 0;
    let passes: &[(usize, usize)] = if frame.interlaced {
        &[(0, 8), (4, 8), (2, 4), (1, 2)]
    } else {
        &[(0, 1)]
    };
    for &(start, step) in passes {
        for y in (start..frame.height as usize).step_by(step) {
            let source = &indexes
                [source_row * frame.width as usize..(source_row + 1) * frame.width as usize];
            source_row += 1;
            let offset = (frame.top as usize + y) * image.pitch + frame.left as usize;
            for (target, &index) in image.pixels[offset..offset + frame.width as usize]
                .iter_mut()
                .zip(source)
            {
                if index as usize >= image.info.palette.len() {
                    return Err(CodecError::InvalidData);
                }
                *target = if !alpha && Some(index) == transparent {
                    header.background_index
                } else {
                    index
                };
            }
        }
    }
    Ok(image)
}
pub fn decode(data: &[u8], alpha: bool) -> CodecResult<DecodedImage> {
    let header = read_header(data)?;
    let frames = get_frames(data, &header)?;
    decode_frame(
        data,
        &header,
        frames.first().ok_or(CodecError::InvalidData)?,
        alpha,
    )
}
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
