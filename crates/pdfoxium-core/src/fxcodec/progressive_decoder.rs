// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Portable XFA image selection/decode pipeline from `progressive_decoder`.
//! Rust codec adapters borrow retained compressed bytes. Output is copied in
//! bounded row batches; explicit ownership replaces the native delegate cycle.
//! StartDecode requires original dimensions, so PDFium's resampling weights are
//! the identity and scanline conversion needs no separate stretch engine.
use super::{
    bmp,
    fx_codec::CfxDibAttribute,
    fx_codec_def::{FxcodecImageType, FxcodecStatus},
    gif, jpeg, png,
    progressive_decoder_context::*,
    tiff,
};
use crate::{
    fxcrt::fx_stream::SeekableReadStream,
    fxge::dib::{
        cfx_cmyk_to_srgb::adobe_cmyk_to_standard_rgb, cfx_dibitmap::CfxDibitmap,
        fx_dib::FxdibFormat,
    },
};
use std::{cell::RefCell, rc::Rc};
pub struct ProgressiveDecoder {
    status: FxcodecStatus,
    image_type: FxcodecImageType,
    source: Vec<u8>,
    info: Option<ImageInfo>,
    frame_count: usize,
    decoded: Option<DecodedImage>,
    bitmap: Option<Rc<RefCell<CfxDibitmap>>>,
    next_row: usize,
    error: Option<CodecError>,
}
impl Default for ProgressiveDecoder {
    fn default() -> Self {
        Self::new()
    }
}
impl ProgressiveDecoder {
    pub fn new() -> Self {
        Self {
            status: FxcodecStatus::DecodeFinished,
            image_type: FxcodecImageType::Unknown,
            source: Vec::new(),
            info: None,
            frame_count: 0,
            decoded: None,
            bitmap: None,
            next_row: 0,
            error: None,
        }
    }
    pub fn width(&self) -> i32 {
        self.info.as_ref().map_or(0, |info| info.width)
    }
    pub fn height(&self) -> i32 {
        self.info.as_ref().map_or(0, |info| info.height)
    }
    pub fn image_type(&self) -> FxcodecImageType {
        self.image_type
    }
    pub fn last_error(&self) -> Option<CodecError> {
        self.error
    }
    pub fn status(&self) -> FxcodecStatus {
        self.status
    }
    fn fail(&mut self, error: CodecError) -> FxcodecStatus {
        self.error = Some(error);
        self.status = FxcodecStatus::Error;
        self.bitmap = None;
        self.decoded = None;
        self.status
    }
    pub fn load_image_info(
        &mut self,
        file: Rc<dyn SeekableReadStream + '_>,
        image_type: FxcodecImageType,
        attribute: &mut CfxDibAttribute,
        skip_fallback_type_detection: bool,
    ) -> FxcodecStatus {
        if !matches!(
            self.status,
            FxcodecStatus::Error | FxcodecStatus::DecodeFinished
        ) {
            return FxcodecStatus::Error;
        }
        self.info = None;
        self.bitmap = None;
        self.decoded = None;
        self.frame_count = 0;
        self.image_type = FxcodecImageType::Unknown;
        self.error = None;
        let Ok(size) = usize::try_from(file.get_size()) else {
            return self.fail(CodecError::Io);
        };
        let mut source = match allocate(size) {
            Ok(buffer) => buffer,
            Err(error) => return self.fail(error),
        };
        // Bounded reads preserve random-access stream semantics and permit short
        // final blocks. Retain compressed input once for the Rust codec readers.
        for (block, output) in source.chunks_mut(4096).enumerate() {
            if !file.read_block_at_offset(output, (block * 4096) as i64) {
                return self.fail(CodecError::Io);
            }
        }
        self.source = source;
        if image_type != FxcodecImageType::Unknown {
            if let Ok(info) = self.detect(image_type) {
                self.set_info(info, image_type, attribute);
                return self.status;
            }
        }
        if skip_fallback_type_detection {
            return self.fail(CodecError::InvalidData);
        }
        for image_type in [
            FxcodecImageType::Jpg,
            FxcodecImageType::Bmp,
            FxcodecImageType::Png,
            FxcodecImageType::Gif,
            FxcodecImageType::Tiff,
        ] {
            if let Ok(info) = self.detect(image_type) {
                self.set_info(info, image_type, attribute);
                return self.status;
            }
        }
        self.fail(CodecError::InvalidData)
    }
    fn detect(&self, image_type: FxcodecImageType) -> CodecResult<ImageInfo> {
        match image_type {
            FxcodecImageType::Bmp => {
                let header = bmp::read_header(&self.source)?;
                if matches!(header.compression, 0 | 3) {
                    bytes(
                        &self.source,
                        header.data_offset,
                        header
                            .source_pitch
                            .checked_mul(header.info.height as usize)
                            .ok_or(CodecError::InvalidData)?,
                    )?;
                }
                Ok(header.info)
            }
            FxcodecImageType::Gif => Ok(gif::read_header(&self.source)?.info),
            FxcodecImageType::Png => png::read_header(&self.source),
            FxcodecImageType::Tiff => tiff::read_header(&self.source),
            FxcodecImageType::Jpg => {
                let header = jpeg::load_info(&self.source).ok_or(CodecError::InvalidData)?;
                let format = match header.components {
                    1 => Format::Gray,
                    3 => Format::Rgb,
                    4 => Format::Cmyk,
                    _ => return Err(CodecError::InvalidData),
                };
                let mut info =
                    ImageInfo::new(header.width as usize, header.height as usize, format)?;
                read_jpeg_density(&self.source, &mut info.attribute);
                Ok(info)
            }
            _ => Err(CodecError::InvalidData),
        }
    }
    fn set_info(
        &mut self,
        info: ImageInfo,
        image_type: FxcodecImageType,
        attribute: &mut CfxDibAttribute,
    ) {
        *attribute = info.attribute.clone();
        self.info = Some(info);
        self.image_type = image_type;
        self.status = FxcodecStatus::FrameReady;
    }
    pub fn get_bitmap_format(&self) -> FxdibFormat {
        match self.image_type {
            // A BMP with an explicit alpha mask needs alpha storage. Native's
            // blanket Bgrx choice discards that channel in the XFA image caller.
            FxcodecImageType::Bmp
                if self.info.as_ref().is_some_and(|i| i.format == Format::Bgra) =>
            {
                FxdibFormat::Bgra
            }
            FxcodecImageType::Jpg | FxcodecImageType::Bmp => {
                if self
                    .info
                    .as_ref()
                    .is_some_and(|info| info.format.bpp() > 24)
                {
                    FxdibFormat::Bgrx
                } else {
                    FxdibFormat::Bgr
                }
            }
            _ => FxdibFormat::Bgra,
        }
    }
    pub fn get_frames(&mut self) -> (FxcodecStatus, usize) {
        if !matches!(
            self.status,
            FxcodecStatus::FrameReady | FxcodecStatus::FrameToBeContinued
        ) {
            return (FxcodecStatus::Error, 0);
        }
        self.frame_count = if self.image_type == FxcodecImageType::Gif {
            let frames = gif::read_header(&self.source)
                .and_then(|header| gif::get_frames(&self.source, &header));
            match frames {
                Ok(frames) => frames.len(),
                Err(error) => {
                    self.fail(error);
                    return (self.status, 0);
                }
            }
        } else {
            1
        };
        self.status = FxcodecStatus::DecodeReady;
        (self.status, self.frame_count)
    }
    pub fn start_decode(&mut self, bitmap: Rc<RefCell<CfxDibitmap>>) -> FxcodecStatus {
        if self.status != FxcodecStatus::DecodeReady || self.frame_count == 0 {
            return FxcodecStatus::Error;
        }
        let Ok(target) = bitmap.try_borrow() else {
            return FxcodecStatus::Error;
        };
        if target.width() != self.width()
            || target.height() != self.height()
            || !matches!(
                target.format(),
                FxdibFormat::Bgr | FxdibFormat::Bgrx | FxdibFormat::Bgra
            )
            || (matches!(
                self.image_type,
                FxcodecImageType::Png | FxcodecImageType::Tiff
            ) && target.format() != FxdibFormat::Bgra)
        {
            return FxcodecStatus::Error;
        }
        drop(target);
        self.bitmap = Some(bitmap);
        self.next_row = 0;
        self.status = FxcodecStatus::DecodeToBeContinued;
        self.status
    }
    fn decode_source(&self, alpha: bool) -> CodecResult<DecodedImage> {
        match self.image_type {
            FxcodecImageType::Bmp => bmp::decode(&self.source),
            FxcodecImageType::Gif => gif::decode(&self.source, alpha),
            FxcodecImageType::Png => png::decode(&self.source),
            FxcodecImageType::Tiff => tiff::decode(&self.source),
            FxcodecImageType::Jpg => {
                let header = jpeg::load_info(&self.source).ok_or(CodecError::InvalidData)?;
                let decoded = jpeg::decode(
                    &self.source,
                    header.width,
                    header.height,
                    header.color_transform,
                    1,
                )
                .ok_or(CodecError::InvalidData)?;
                let info = self.info.clone().ok_or(CodecError::InvalidData)?;
                let pitch = info.row_bytes()?;
                if decoded.width != info.width
                    || decoded.height != info.height
                    || decoded.components as usize != info.format.components()
                    || decoded.pixels.len() != pitch * info.height as usize
                {
                    return Err(CodecError::InvalidData);
                }
                Ok(DecodedImage {
                    info,
                    pixels: decoded.pixels,
                    pitch,
                })
            }
            _ => Err(CodecError::InvalidData),
        }
    }
    pub fn continue_decode(&mut self) -> FxcodecStatus {
        if self.status != FxcodecStatus::DecodeToBeContinued {
            return FxcodecStatus::Error;
        }
        let Some(bitmap) = self.bitmap.clone() else {
            return self.fail(CodecError::InvalidData);
        };
        let alpha = match bitmap.try_borrow() {
            Ok(bitmap) => bitmap.format() == FxdibFormat::Bgra,
            Err(_) => return self.fail(CodecError::InvalidData),
        };
        if self.decoded.is_none() {
            match self.decode_source(alpha) {
                Ok(decoded) => self.decoded = Some(decoded),
                Err(error) => return self.fail(error),
            }
        }
        let image = self.decoded.as_ref().unwrap();
        let Ok(mut target) = bitmap.try_borrow_mut() else {
            return self.fail(CodecError::InvalidData);
        };
        let end = (self.next_row + 64).min(image.info.height as usize);
        for y in self.next_row..end {
            if let Err(error) = write_scanline(image, y, &mut target) {
                drop(target);
                return self.fail(error);
            }
        }
        drop(target);
        self.next_row = end;
        if end == self.height() as usize {
            self.decoded = None;
            self.bitmap = None;
            self.source.clear();
            self.status = FxcodecStatus::DecodeFinished;
        }
        self.status
    }
}
fn write_scanline(image: &DecodedImage, y: usize, target: &mut CfxDibitmap) -> CodecResult<()> {
    let components = (target.bpp() / 8) as usize;
    let format = target.format();
    let dest = target
        .scanline_mut(y as i32)
        .ok_or(CodecError::InvalidData)?;
    let row = image.row(y).ok_or(CodecError::InvalidData)?;
    for (dest, source) in dest[..image.info.width as usize * components]
        .chunks_exact_mut(components)
        .zip(row.chunks_exact(image.info.format.components()))
    {
        let (blue, green, red, alpha) = match image.info.format {
            Format::Gray => (source[0], source[0], source[0], 255),
            Format::Indexed => {
                let color = *image
                    .info
                    .palette
                    .get(source[0] as usize)
                    .ok_or(CodecError::InvalidData)?;
                (
                    color as u8,
                    (color >> 8) as u8,
                    (color >> 16) as u8,
                    (color >> 24) as u8,
                )
            }
            Format::Rgb => (source[2], source[1], source[0], 255),
            Format::Bgr | Format::Bgrx => (source[0], source[1], source[2], 255),
            Format::Bgra => (source[0], source[1], source[2], source[3]),
            Format::Cmyk => {
                let color = adobe_cmyk_to_standard_rgb(
                    255 - source[0],
                    255 - source[1],
                    255 - source[2],
                    255 - source[3],
                );
                (color.blue, color.green, color.red, 255)
            }
            Format::Invalid => return Err(CodecError::InvalidData),
        };
        if format == FxdibFormat::Bgra {
            dest.copy_from_slice(&[blue, green, red, alpha]);
        } else {
            dest[..3].copy_from_slice(&[blue, green, red]);
        }
    }
    Ok(())
}
fn read_jpeg_density(data: &[u8], attribute: &mut CfxDibAttribute) {
    use super::fx_codec::ResolutionUnit;
    let mut cursor = 2;
    while data.get(cursor) == Some(&255) {
        let Some(&marker) = data.get(cursor + 1) else {
            return;
        };
        cursor += 2;
        if marker == 0xff {
            cursor -= 1;
            continue;
        }
        if marker == 0xd9 || marker == 0xda {
            return;
        }
        if matches!(marker, 0x01 | 0xd0..=0xd7) {
            continue;
        }
        let Some(length) = data
            .get(cursor..cursor + 2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]) as usize)
        else {
            return;
        };
        if length < 2 {
            return;
        }
        let Some(segment) = data.get(cursor..cursor.saturating_add(length)) else {
            return;
        };
        if marker == 0xe0 && segment.get(2..7) == Some(b"JFIF\0") && segment.len() >= 14 {
            attribute.dpi_unit = match segment[9] {
                1 => ResolutionUnit::Inch,
                2 => ResolutionUnit::Centimeter,
                _ => ResolutionUnit::None,
            };
            attribute.x_dpi = i32::from(u16::from_be_bytes([segment[10], segment[11]]));
            attribute.y_dpi = i32::from(u16::from_be_bytes([segment[12], segment[13]]));
            return;
        }
        cursor += length;
    }
}
#[cfg(test)]
#[path = "progressive_decoder_tests.rs"]
mod tests;
