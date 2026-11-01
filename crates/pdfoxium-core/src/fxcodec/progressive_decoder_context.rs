// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2025 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Shared safe codec boundary. Explicit borrows replace C++ delegate backpointers.
use super::fx_codec::CfxDibAttribute;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecError {
    NeedMoreData,
    InvalidData,
    Unsupported,
    Allocation,
    Io,
}
pub type CodecResult<T> = Result<T, CodecError>;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Error,
    Success,
    Continue,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Format {
    Invalid = 0,
    Gray = 0x108,
    Indexed = 0x008,
    Bgr = 0x018,
    Rgb = 0x118,
    Bgrx = 0x020,
    Bgra = 0x220,
    Cmyk = 0x120,
}
impl Format {
    pub fn bpp(self) -> usize {
        (self as usize) & 255
    }
    pub fn components(self) -> usize {
        self.bpp() / 8
    }
}
#[derive(Clone, Debug)]
pub struct ImageInfo {
    pub width: i32,
    pub height: i32,
    pub format: Format,
    pub palette: Vec<u32>,
    pub attribute: CfxDibAttribute,
}
impl ImageInfo {
    pub fn new(width: usize, height: usize, format: Format) -> CodecResult<Self> {
        if width == 0 || height == 0 || width > 65535 || height > 65535 {
            return Err(CodecError::InvalidData);
        }
        Ok(Self {
            width: width as i32,
            height: height as i32,
            format,
            palette: Vec::new(),
            attribute: CfxDibAttribute::default(),
        })
    }
    pub fn row_bytes(&self) -> CodecResult<usize> {
        (self.width as usize)
            .checked_mul(self.format.components())
            .ok_or(CodecError::InvalidData)
    }
}
pub struct DecodedImage {
    pub info: ImageInfo,
    pub pixels: Vec<u8>,
    pub pitch: usize,
}
impl DecodedImage {
    pub fn new(info: ImageInfo) -> CodecResult<Self> {
        let pitch = info.row_bytes()?;
        let size = pitch
            .checked_mul(info.height as usize)
            .ok_or(CodecError::InvalidData)?;
        let pixels = allocate(size)?;
        Ok(Self {
            info,
            pixels,
            pitch,
        })
    }
    pub fn row(&self, row: usize) -> Option<&[u8]> {
        let start = row.checked_mul(self.pitch)?;
        self.pixels.get(start..start.checked_add(self.pitch)?)
    }
}
pub(crate) fn allocate(size: usize) -> CodecResult<Vec<u8>> {
    // Match the engine bitmap allocation ceiling, before requesting address space.
    if size > i32::MAX as usize {
        return Err(CodecError::Allocation);
    }
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(size)
        .map_err(|_| CodecError::Allocation)?;
    buffer.resize(size, 0);
    Ok(buffer)
}
pub(crate) fn bytes(data: &[u8], offset: usize, count: usize) -> CodecResult<&[u8]> {
    data.get(offset..offset.checked_add(count).ok_or(CodecError::InvalidData)?)
        .ok_or(CodecError::NeedMoreData)
}
pub(crate) fn le16(data: &[u8], offset: usize) -> CodecResult<u16> {
    Ok(u16::from_le_bytes(
        bytes(data, offset, 2)?.try_into().unwrap(),
    ))
}
pub(crate) fn le32(data: &[u8], offset: usize) -> CodecResult<u32> {
    Ok(u32::from_le_bytes(
        bytes(data, offset, 4)?.try_into().unwrap(),
    ))
}
