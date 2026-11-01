// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Portable `libtiff_tiff_context` adapter. Published Rust TIFF decompressors
//! supply raw samples; the pinned TIFF RGBA conversion policy stays here.
//! Unsupported compression/photometric combinations remain explicit errors.
mod color;
mod compression;
mod directory;
mod log_luv;
mod pixar_log;
use super::{fx_codec::ResolutionUnit, progressive_decoder_context::*};
use ::tiff::{
    decoder::{Decoder, DecodingResult},
    tags::Tag,
    TiffError,
};
use color::{CieLab, YCbCr};
use directory::{DirectoryView, Override};

type Reader<'a> = Decoder<DirectoryView<'a>>;
fn error(error: TiffError) -> CodecError {
    match error {
        TiffError::UnsupportedError(_) => CodecError::Unsupported,
        TiffError::LimitsExceeded => CodecError::Allocation,
        _ => CodecError::InvalidData,
    }
}
fn reader(data: &[u8], frame: usize) -> CodecResult<Reader<'_>> {
    let view = DirectoryView::new(data, frame)?;
    // libtiff infers a missing Photometric from the actual color channel count.
    let mut changes = Vec::new();
    if view
        .unsigned(262)?
        .is_some_and(|v| matches!(v, 32844 | 32845))
    {
        changes.push(Override::short(262, &[1], view.endian));
    }

    if view.unsigned(262)?.is_none() {
        let samples = view
            .unsigned(277)?
            .unwrap_or(1)
            .checked_sub(view.count(338)? as u64)
            .ok_or(CodecError::InvalidData)?;
        let compression = view.unsigned(259)?.unwrap_or(1);
        let photo = match samples {
            1 => {
                if matches!(compression, 2 | 3 | 4 | 32771) {
                    0
                } else {
                    1
                }
            }
            3 => 2,
            _ => return Err(CodecError::InvalidData),
        };
        changes.push(Override::short(262, &[photo], view.endian));
    }
    Decoder::new(view.overrides(changes)?).map_err(error)
}
fn unsigned(decoder: &mut Reader<'_>, tag: Tag, default: u32) -> CodecResult<u32> {
    decoder
        .find_tag_unsigned(tag)
        .map_err(error)
        .map(|value| value.unwrap_or(default))
}
fn header(decoder: &mut Reader<'_>) -> CodecResult<ImageInfo> {
    let (width, height) = decoder.dimensions().map_err(error)?;
    let mut info = ImageInfo::new(width as usize, height as usize, Format::Bgra)?;
    info.attribute.dpi_unit = match unsigned(decoder, Tag::ResolutionUnit, 2)? {
        1 => ResolutionUnit::None,
        2 => ResolutionUnit::Inch,
        3 => ResolutionUnit::Centimeter,
        _ => ResolutionUnit::None,
    };
    if let Some(value) = resolution(decoder, Tag::XResolution) {
        if value > 0.0 && value.is_finite() {
            info.attribute.x_dpi = (value + 0.5) as i32;
        }
    }
    if let Some(value) = resolution(decoder, Tag::YResolution) {
        if value > 0.0 && value.is_finite() {
            info.attribute.y_dpi = (value + 0.5) as i32;
        }
    }
    Ok(info)
}
fn resolution(decoder: &mut Reader<'_>, tag: Tag) -> Option<f32> {
    rational_values(decoder, tag).ok()?.into_iter().next()
}
fn rational_values(decoder: &mut Reader<'_>, tag: Tag) -> CodecResult<Vec<f32>> {
    use ::tiff::decoder::ifd::Value;
    fn number(value: Value) -> CodecResult<f32> {
        match value {
            Value::Rational(n, d) if d != 0 => Ok(n as f32 / d as f32),
            Value::SRational(n, d) if d != 0 => Ok(n as f32 / d as f32),
            Value::Float(v) => Ok(v),
            Value::Double(v) => Ok(v as f32),
            _ => Err(CodecError::InvalidData),
        }
    }
    match decoder.get_tag(tag).map_err(error)? {
        Value::List(values) => values.into_iter().map(number).collect(),
        value => Ok(vec![number(value)?]),
    }
}
/// Frame selection is explicit, as in native LoadFrameInfo. The progressive XFA
/// caller requests frame zero; standalone users can select any existing IFD.
pub fn read_frame_header(data: &[u8], frame: usize) -> CodecResult<ImageInfo> {
    header(&mut reader(data, frame)?)
}
pub fn read_header(data: &[u8]) -> CodecResult<ImageInfo> {
    read_frame_header(data, 0)
}
pub fn decode(data: &[u8]) -> CodecResult<DecodedImage> {
    decode_frame(data, 0)
}

struct Metadata {
    width: usize,
    height: usize,
    bits: u8,
    samples: usize,
    photo: u32,
    compression: u32,
    planar: bool,
    alpha: u16,
    fill_order: u32,
    palette: Vec<u16>,
    ycbcr: Option<YCbCr>,
    lab: Option<CieLab>,
    subsample: (usize, usize),
    tile: Option<(usize, usize)>,
    rows_per_strip: usize,
}
impl Metadata {
    fn read(
        decoder: &mut Reader<'_>,
        info: &ImageInfo,
        native_photo: Option<u32>,
    ) -> CodecResult<Self> {
        let samples = unsigned(decoder, Tag::SamplesPerPixel, 1)? as usize;
        if samples == 0 {
            return Err(CodecError::InvalidData);
        }
        let bits = decoder
            .find_tag_unsigned_vec::<u16>(Tag::BitsPerSample)
            .map_err(error)?
            .unwrap_or(vec![1]);
        let Some(&bits_per_sample) = bits.first() else {
            return Err(CodecError::InvalidData);
        };
        if bits.iter().any(|&value| value != bits_per_sample) {
            return Err(CodecError::Unsupported);
        }
        if !matches!(bits_per_sample, 1 | 2 | 4 | 8 | 16) {
            return Err(CodecError::Unsupported);
        }
        let formats = decoder
            .find_tag_unsigned_vec::<u16>(Tag::SampleFormat)
            .map_err(error)?
            .unwrap_or(vec![1]);
        if formats.iter().any(|&value| !matches!(value, 1 | 2 | 4)) {
            return Err(CodecError::Unsupported);
        }
        let photo = native_photo.unwrap_or(unsigned(decoder, Tag::PhotometricInterpretation, 1)?);
        let extras = decoder
            .find_tag_unsigned_vec::<u16>(Tag::ExtraSamples)
            .map_err(error)?
            .unwrap_or_default();
        if extras.len() > samples {
            return Err(CodecError::InvalidData);
        }
        let alpha = match extras.first().copied() {
            Some(1) => 1,
            Some(2) => 2,
            Some(0) if samples > 3 => 1,
            None if photo == 2 && samples == 4 => 1,
            _ => 0,
        };
        let planar = unsigned(decoder, Tag::PlanarConfiguration, 1)? == 2;
        let compression = unsigned(decoder, Tag::Compression, 1)?;
        match photo {
            0 | 1 | 3 if bits_per_sample < 8 && samples != 1 && !planar => {
                return Err(CodecError::Unsupported)
            }
            0 | 1 => {}
            2 if samples - extras.len() >= 3 && bits_per_sample >= 8 => {}
            3 if bits_per_sample <= 8 => {}
            5 if samples >= 4 && bits_per_sample >= 8 => {
                if unsigned(decoder, Tag::Unknown(332), 1)? != 1 {
                    return Err(CodecError::Unsupported);
                }
            }
            6 if samples == 3 && bits_per_sample == 8 => {}
            8 if samples == 3 && extras.is_empty() && bits_per_sample >= 8 && !planar => {}
            32844 if samples == 1 && compression == 34676 => {}
            32845
                if samples == 3
                    && extras.is_empty()
                    && !planar
                    && matches!(compression, 34676 | 34677) => {}
            _ => return Err(CodecError::Unsupported),
        }
        let palette = if photo == 3 {
            let mut values = decoder.get_tag_u16_vec(Tag::ColorMap).map_err(error)?;
            if values.len() != 3 * (1usize << bits_per_sample) {
                return Err(CodecError::InvalidData);
            }
            // checkcmap/cvtcmap accepts old files with an 8-bit colormap too.
            if values.iter().any(|&value| value >= 256) {
                for value in &mut values {
                    *value >>= 8;
                }
            }
            values
        } else {
            Vec::new()
        };
        let ycbcr = if photo == 6 {
            let coefficients = match decoder.find_tag(Tag::Unknown(529)).map_err(error)? {
                Some(_) => rational_values(decoder, Tag::Unknown(529))?,
                None => vec![0.299, 0.587, 0.114],
            };
            let reference = match decoder.find_tag(Tag::Unknown(532)).map_err(error)? {
                Some(_) => rational_values(decoder, Tag::Unknown(532))?,
                None => vec![0., 255., 128., 255., 128., 255.],
            };
            Some(YCbCr::new(&coefficients, &reference)?)
        } else {
            None
        };
        let lab = if photo == 8 {
            let white = match decoder.find_tag(Tag::Unknown(318)).map_err(error)? {
                Some(_) => rational_values(decoder, Tag::Unknown(318))?,
                None => {
                    let sum = 96.4250f32 + 100.0 + 82.4680;
                    vec![96.4250 / sum, 100.0 / sum]
                }
            };
            Some(CieLab::new(&white)?)
        } else {
            None
        };
        let sub = decoder
            .find_tag_unsigned_vec::<u16>(Tag::ChromaSubsampling)
            .map_err(error)?
            .unwrap_or(vec![2, 2]);
        if sub.len() != 2 {
            return Err(CodecError::InvalidData);
        }
        let subsample = (sub[0] as usize, sub[1] as usize);
        if photo == 6
            && !matches!(
                subsample,
                (1, 1) | (1, 2) | (2, 1) | (2, 2) | (4, 1) | (4, 2) | (4, 4)
            )
        {
            return Err(CodecError::Unsupported);
        }
        let tile = if decoder.get_chunk_type() == ::tiff::decoder::ChunkType::Tile {
            Some((
                unsigned(decoder, Tag::TileWidth, 0)? as usize,
                unsigned(decoder, Tag::TileLength, 0)? as usize,
            ))
        } else {
            None
        };
        let height = info.height as usize;
        let rows_per_strip = (unsigned(decoder, Tag::RowsPerStrip, u32::MAX)? as usize).min(height);
        if rows_per_strip == 0 || tile.is_some_and(|(w, h)| w == 0 || h == 0) {
            return Err(CodecError::InvalidData);
        }
        Ok(Self {
            width: info.width as usize,
            height,
            bits: bits_per_sample as u8,
            samples,
            photo,
            compression,
            planar,
            alpha,
            fill_order: unsigned(decoder, Tag::FillOrder, 1)?,
            palette,
            ycbcr,
            lab,
            subsample,
            tile,
            rows_per_strip,
        })
    }
    fn raw_overrides(&self, endian: directory::Endian) -> Vec<Override> {
        // Raw multiband samples avoid the crate's unsupported colormap/Lab
        // interpretations and WhiteIsZero inversion of alpha/extra channels.
        // JPEG's own colorspace is detected by its decoder, independent of this
        // raw-sample view. Subsampled non-JPEG chunks have a separate byte view.
        vec![
            Override::short(262, &[1], endian),
            Override::short(339, &[1], endian),
            Override::short(338, &[], endian),
        ]
    }
}

pub fn decode_frame(data: &[u8], frame: usize) -> CodecResult<DecodedImage> {
    let mut original = reader(data, frame)?;
    let info = header(&mut original)?;
    let native_photo = DirectoryView::new(data, frame)?
        .unsigned(262)?
        .map(|v| v as u32);
    let metadata = Metadata::read(&mut original, &info, native_photo)?;
    let mut image = DecodedImage::new(info)?;
    if matches!(metadata.compression, 32909 | 34676 | 34677) {
        decode_log_chunks(&mut original, data, frame, &metadata, &mut image)?;
        return Ok(image);
    }
    if matches!(metadata.compression, 2 | 3 | 4 | 32771 | 32766 | 32809) {
        decode_custom_chunks(&mut original, data, &metadata, &mut image)?;
        return Ok(image);
    }
    if metadata.photo == 6 && metadata.compression != 7 && metadata.subsample != (1, 1) {
        decode_subsampled(data, frame, &mut original, &metadata, &mut image)?;
        return Ok(image);
    }
    let view = DirectoryView::new(data, frame)?;
    let changes = metadata.raw_overrides(view.endian);
    let view = payload_view(view.overrides(changes)?, &mut original, &metadata)?;
    let mut decoder = Decoder::new(view).map_err(error)?;
    let mut result = DecodingResult::U8(Vec::new());
    // read_image() intentionally retains its historical first-plane-only
    // behavior in tiff 0.11. read_image_to_buffer() reads all planes.
    let layout = decoder.read_image_to_buffer(&mut result).map_err(error)?;
    if result.as_buffer(0).byte_len() < layout.complete_len {
        return Err(CodecError::Allocation);
    }
    let count = metadata.width * metadata.height;
    let samples = Samples::new(result)?;
    let packed = metadata.bits < 8;
    let row_samples = metadata.width * if metadata.planar { 1 } else { metadata.samples };
    let row_bytes = (row_samples * metadata.bits as usize).div_ceil(8);
    let plane_size = if packed {
        row_bytes * metadata.height
    } else {
        count
    };
    let expected = if packed {
        row_bytes * metadata.height * if metadata.planar { metadata.samples } else { 1 }
    } else {
        count * metadata.samples
    };
    if samples.len() != expected {
        return Err(CodecError::InvalidData);
    }
    for y in 0..metadata.height {
        for x in 0..metadata.width {
            let mut values = [0u16; 4];
            for (channel, value) in values.iter_mut().enumerate().take(metadata.samples.min(4)) {
                *value = if packed {
                    let bit = (x * if metadata.planar { 1 } else { metadata.samples }
                        + if metadata.planar { 0 } else { channel })
                        * metadata.bits as usize;
                    let index = if metadata.planar {
                        channel * plane_size
                    } else {
                        0
                    } + y * row_bytes
                        + bit / 8;
                    let byte = samples.at(index) as u8;
                    u16::from(
                        (byte >> (8 - metadata.bits - bit as u8 % 8)) & ((1 << metadata.bits) - 1),
                    )
                } else {
                    let pixel = y * metadata.width + x;
                    samples.at(if metadata.planar {
                        channel * plane_size + pixel
                    } else {
                        pixel * metadata.samples + channel
                    })
                };
            }
            let pixel = convert_pixel(&metadata, values)?;
            let start = y * image.pitch + x * 4;
            image.pixels[start..start + 4].copy_from_slice(&pixel);
        }
    }
    Ok(image)
}
fn chunk_ranges(original: &mut Reader<'_>, meta: &Metadata) -> CodecResult<Vec<(usize, usize)>> {
    let (offset, count) = if meta.tile.is_some() {
        (Tag::TileOffsets, Tag::TileByteCounts)
    } else {
        (Tag::StripOffsets, Tag::StripByteCounts)
    };
    let offsets = original.get_tag_u64_vec(offset).map_err(error)?;
    let counts = original.get_tag_u64_vec(count).map_err(error)?;
    if offsets.len() != counts.len() {
        return Err(CodecError::InvalidData);
    }
    offsets
        .into_iter()
        .zip(counts)
        .map(|(a, b)| {
            Ok((
                usize::try_from(a).map_err(|_| CodecError::InvalidData)?,
                usize::try_from(b).map_err(|_| CodecError::InvalidData)?,
            ))
        })
        .collect()
}
fn payload_view<'a>(
    view: DirectoryView<'a>,
    original: &mut Reader<'_>,
    meta: &Metadata,
) -> CodecResult<DirectoryView<'a>> {
    if meta.fill_order == 2 && meta.compression != 7 {
        view.reverse_payloads(chunk_ranges(original, meta)?)
    } else {
        Ok(view)
    }
}
fn decode_log_chunks(
    original: &mut Reader<'_>,
    data: &[u8],
    frame: usize,
    meta: &Metadata,
    image: &mut DecodedImage,
) -> CodecResult<()> {
    let ranges = chunk_ranges(original, meta)?;
    let (cw, ch) = meta.tile.unwrap_or((meta.width, meta.rows_per_strip));
    let across = meta.width.div_ceil(cw);
    let down = meta.height.div_ceil(ch);
    let planes = if meta.planar { meta.samples } else { 1 };
    if ranges.len() != across * down * planes {
        return Err(CodecError::InvalidData);
    }
    let count = meta.width * meta.height;
    let mut samples = Vec::new();
    let len = count
        .checked_mul(meta.samples)
        .ok_or(CodecError::InvalidData)?;
    if len > i32::MAX as usize / 2 {
        return Err(CodecError::Allocation);
    }
    samples
        .try_reserve_exact(len)
        .map_err(|_| CodecError::Allocation)?;
    samples.resize(len, 0u16);
    let little = DirectoryView::new(data, frame)?.endian.0;
    for (index, (start, length)) in ranges.into_iter().enumerate() {
        let reversed;
        let raw = bytes(data, start, length)?;
        let raw = if meta.fill_order == 2 {
            reversed = raw.iter().map(|b| b.reverse_bits()).collect::<Vec<_>>();
            reversed.as_slice()
        } else {
            raw
        };
        let plane = index / (across * down);
        let region = index % (across * down);
        let cx = region % across;
        let cy = region / across;
        let height = if meta.tile.is_some() {
            ch
        } else {
            ch.min(meta.height - cy * ch)
        };
        let channels = if meta.planar { 1 } else { meta.samples };
        let decoded = if meta.compression == 32909 {
            match pixar_log::decode(raw, cw, height, channels, meta.bits, little)? {
                pixar_log::Pixels::Bytes(v) => Samples::Bytes(v),
                pixar_log::Pixels::Words(v) => Samples::Words(v),
            }
        } else {
            Samples::Bytes(log_luv::decode(
                raw,
                cw,
                height,
                meta.photo,
                meta.compression,
            )?)
        };
        for y in 0..height.min(meta.height - cy * ch) {
            for x in 0..cw.min(meta.width - cx * cw) {
                let pixel = (cy * ch + y) * meta.width + cx * cw + x;
                for channel in 0..channels {
                    let target = pixel * meta.samples + if meta.planar { plane } else { channel };
                    samples[target] = decoded.at((y * cw + x) * channels + channel);
                }
            }
        }
    }
    for (dest, pixel) in image
        .pixels
        .chunks_exact_mut(4)
        .zip(samples.chunks_exact(meta.samples))
    {
        let mut values = [0; 4];
        let count = pixel.len().min(4);
        values[..count].copy_from_slice(&pixel[..count]);
        if meta.photo == 32844 {
            let gray = values[0] as u8;
            dest.copy_from_slice(&[gray, gray, gray, 255]);
        } else if meta.photo == 32845 {
            dest.copy_from_slice(&[values[2] as u8, values[1] as u8, values[0] as u8, 255]);
        } else {
            dest.copy_from_slice(&convert_pixel(meta, values)?);
        }
    }
    Ok(())
}
fn decode_custom_chunks(
    original: &mut Reader<'_>,
    data: &[u8],
    meta: &Metadata,
    image: &mut DecodedImage,
) -> CodecResult<()> {
    if meta.samples != 1 || !matches!(meta.photo, 0 | 1 | 3) {
        return Err(CodecError::Unsupported);
    }
    if meta.tile.is_some() && meta.compression == 32809 {
        return Err(CodecError::Unsupported);
    }
    let (offset_tag, count_tag) = if meta.tile.is_some() {
        (Tag::TileOffsets, Tag::TileByteCounts)
    } else {
        (Tag::StripOffsets, Tag::StripByteCounts)
    };
    let offsets = original.get_tag_u64_vec(offset_tag).map_err(error)?;
    let counts = original.get_tag_u64_vec(count_tag).map_err(error)?;
    let (chunk_width, chunk_height) = meta.tile.unwrap_or((meta.width, meta.rows_per_strip));
    let across = meta.width.div_ceil(chunk_width);
    let down = meta.height.div_ceil(chunk_height);
    if offsets.len() != across * down || counts.len() != offsets.len() {
        return Err(CodecError::InvalidData);
    }
    let options = unsigned(
        original,
        Tag::Unknown(if meta.compression == 4 { 293 } else { 292 }),
        0,
    )?;
    for index in 0..offsets.len() {
        let start = usize::try_from(offsets[index]).map_err(|_| CodecError::InvalidData)?;
        let length = usize::try_from(counts[index]).map_err(|_| CodecError::InvalidData)?;
        let source = bytes(data, start, length)?;
        let cy = index / across;
        let cx = index % across;
        let height = if meta.tile.is_some() {
            chunk_height
        } else {
            chunk_height.min(meta.height - cy * chunk_height)
        };
        let reversed;
        let source = if meta.fill_order == 2 && !matches!(meta.compression, 2 | 3 | 4 | 32771) {
            reversed = source.iter().map(|b| b.reverse_bits()).collect::<Vec<_>>();
            reversed.as_slice()
        } else {
            source
        };
        let output = compression::decode(
            source,
            chunk_width,
            height,
            meta.bits,
            meta.compression,
            meta.fill_order,
            options,
        )?;
        let pitch = (chunk_width * meta.bits as usize).div_ceil(8);
        for y in 0..height.min(meta.height - cy * chunk_height) {
            for x in 0..chunk_width.min(meta.width - cx * chunk_width) {
                let bit = x * meta.bits as usize;
                let value = (output[y * pitch + bit / 8] >> (8 - meta.bits - bit as u8 % 8))
                    & ((1 << meta.bits) - 1);
                let pixel = convert_pixel(meta, [u16::from(value), 0, 0, 0])?;
                let dest = (cy * chunk_height + y) * image.pitch + (cx * chunk_width + x) * 4;
                image.pixels[dest..dest + 4].copy_from_slice(&pixel);
            }
        }
    }
    Ok(())
}
enum Samples {
    Bytes(Vec<u8>),
    Words(Vec<u16>),
}
impl Samples {
    fn new(result: DecodingResult) -> CodecResult<Self> {
        match result {
            DecodingResult::U8(v) => Ok(Self::Bytes(v)),
            DecodingResult::U16(v) => Ok(Self::Words(v)),
            _ => Err(CodecError::Unsupported),
        }
    }
    fn len(&self) -> usize {
        match self {
            Self::Bytes(v) => v.len(),
            Self::Words(v) => v.len(),
        }
    }
    fn at(&self, index: usize) -> u16 {
        match self {
            Self::Bytes(v) => u16::from(v[index]),
            Self::Words(v) => v[index],
        }
    }
}
fn convert_pixel(meta: &Metadata, values: [u16; 4]) -> CodecResult<[u8; 4]> {
    let byte = |value: u16| {
        if meta.bits == 16 {
            ((u32::from(value) + 128) / 257) as u8
        } else {
            value as u8
        }
    };
    let (red, green, blue, alpha) = match meta.photo {
        0 | 1 => {
            // The native gray16 converter uses the high byte, while RGB16
            // rounds through Bitdepth16To8. Gray16 does not expose alpha.
            let gray = if meta.bits == 16 {
                (values[0] >> 8) as u8
            } else {
                (u32::from(values[0]) * 255 / ((1u32 << meta.bits) - 1)) as u8
            };
            let gray = if meta.photo == 0 { 255 - gray } else { gray };
            (
                gray,
                gray,
                gray,
                if meta.bits == 8 && meta.samples == 2 && meta.alpha != 0 {
                    values[1] as u8
                } else {
                    255
                },
            )
        }
        2 => {
            let mut rgb = [byte(values[0]), byte(values[1]), byte(values[2])];
            let alpha = if meta.alpha != 0 && meta.samples >= 4 {
                byte(values[3])
            } else {
                255
            };
            if meta.alpha == 2 {
                for v in &mut rgb {
                    *v = ((u16::from(*v) * u16::from(alpha) + 127) / 255) as u8;
                }
            }
            (rgb[0], rgb[1], rgb[2], alpha)
        }
        3 => {
            let index = values[0] as usize;
            let colors = 1usize << meta.bits;
            if index >= colors {
                return Err(CodecError::InvalidData);
            }
            (
                meta.palette[index] as u8,
                meta.palette[colors + index] as u8,
                meta.palette[2 * colors + index] as u8,
                255,
            )
        }
        5 => {
            let k = 255 - u16::from(byte(values[3]));
            (
                ((255 - u16::from(byte(values[0]))) * k / 255) as u8,
                ((255 - u16::from(byte(values[1]))) * k / 255) as u8,
                ((255 - u16::from(byte(values[2]))) * k / 255) as u8,
                255,
            )
        }
        6 => {
            let [r, g, b] = meta.ycbcr.as_ref().ok_or(CodecError::InvalidData)?.convert(
                values[0] as u8,
                values[1] as u8,
                values[2] as u8,
            );
            (r, g, b, 255)
        }
        8 => {
            let [r, g, b] = meta
                .lab
                .as_ref()
                .ok_or(CodecError::InvalidData)?
                .convert(values, meta.bits);
            (r, g, b, 255)
        }
        _ => return Err(CodecError::Unsupported),
    };
    Ok([blue, green, red, alpha])
}
fn decode_subsampled(
    data: &[u8],
    frame: usize,
    original: &mut Reader<'_>,
    meta: &Metadata,
    image: &mut DecodedImage,
) -> CodecResult<()> {
    if meta.planar {
        return Err(CodecError::Unsupported);
    }
    let (horizontal, vertical) = meta.subsample;
    let (chunk_width, chunk_height) = meta.tile.unwrap_or((meta.width, meta.rows_per_strip));
    if (meta.tile.is_some() && (chunk_width % horizontal != 0 || chunk_height % vertical != 0))
        || (meta.tile.is_none() && chunk_height < meta.height && chunk_height % vertical != 0)
    {
        return Err(CodecError::InvalidData);
    }
    let clump_size = horizontal * vertical + 2;
    let view = DirectoryView::new(data, frame)?;
    let endian = view.endian;
    let mut changes = meta.raw_overrides(endian);
    changes.extend([
        Override::long(
            256,
            (meta.width.div_ceil(horizontal) * clump_size) as u32,
            endian,
        ),
        Override::long(257, meta.height.div_ceil(vertical) as u32, endian),
        Override::short(277, &[1], endian),
        Override::short(258, &[8], endian),
    ]);
    if meta.tile.is_some() {
        changes.push(Override::long(
            322,
            (chunk_width.div_ceil(horizontal) * clump_size) as u32,
            endian,
        ));
        changes.push(Override::long(
            323,
            chunk_height.div_ceil(vertical) as u32,
            endian,
        ));
    } else {
        changes.push(Override::long(
            278,
            chunk_height.div_ceil(vertical) as u32,
            endian,
        ));
    }
    let view = payload_view(view.overrides(changes)?, original, meta)?;
    let mut decoder = Decoder::new(view).map_err(error)?;
    let across = meta.width.div_ceil(chunk_width);
    let down = meta.height.div_ceil(chunk_height);
    for cy in 0..down {
        for cx in 0..across {
            let index = cy * across + cx;
            let result = decoder.read_chunk(index as u32).map_err(error)?;
            let DecodingResult::U8(samples) = result else {
                return Err(CodecError::InvalidData);
            };
            let width = chunk_width.min(meta.width - cx * chunk_width);
            let height = chunk_height.min(meta.height - cy * chunk_height);
            let clumps = width.div_ceil(horizontal);
            let rows = height.div_ceil(vertical);
            if samples.len() != clumps * rows * clump_size {
                return Err(CodecError::InvalidData);
            }
            for y in 0..height {
                for x in 0..width {
                    let offset = ((y / vertical) * clumps + x / horizontal) * clump_size;
                    let yy = samples[offset + (y % vertical) * horizontal + x % horizontal];
                    let cb = samples[offset + horizontal * vertical];
                    let cr = samples[offset + horizontal * vertical + 1];
                    let pixel =
                        convert_pixel(meta, [u16::from(yy), u16::from(cb), u16::from(cr), 0])?;
                    let target = (cy * chunk_height + y) * image.pitch + (cx * chunk_width + x) * 4;
                    image.pixels[target..target + 4].copy_from_slice(&pixel);
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use ::tiff::encoder::{colortype, TiffEncoder};
    use std::io::Cursor;
    #[test]
    fn rgb_rgba_gray_and_sixteen_bit_tiff_roundtrip() {
        let mut output = Cursor::new(Vec::new());
        TiffEncoder::new(&mut output)
            .unwrap()
            .write_image::<colortype::RGB8>(2, 1, &[10, 20, 30, 40, 50, 60])
            .unwrap();
        let info = read_header(output.get_ref()).unwrap();
        assert_eq!((info.width, info.height), (2, 1));
        // The published encoder explicitly writes TIFF ResolutionUnit::None;
        // native default-to-inch applies only when that tag is absent.
        assert_eq!(info.attribute.dpi_unit, ResolutionUnit::None);
        assert_eq!(
            decode(output.get_ref()).unwrap().pixels,
            [30, 20, 10, 255, 60, 50, 40, 255]
        );
        let mut output = Cursor::new(Vec::new());
        TiffEncoder::new(&mut output)
            .unwrap()
            .write_image::<colortype::RGBA8>(1, 1, &[10, 20, 30, 64])
            .unwrap();
        assert_eq!(decode(output.get_ref()).unwrap().pixels, [30, 20, 10, 64]);
        let mut output = Cursor::new(Vec::new());
        TiffEncoder::new(&mut output)
            .unwrap()
            .write_image::<colortype::Gray8>(1, 1, &[127])
            .unwrap();
        assert_eq!(
            decode(output.get_ref()).unwrap().pixels,
            [127, 127, 127, 255]
        );
        let mut output = Cursor::new(Vec::new());
        TiffEncoder::new(&mut output)
            .unwrap()
            .write_image::<colortype::Gray16>(1, 1, &[0xabcd])
            .unwrap();
        assert_eq!(
            decode(output.get_ref()).unwrap().pixels,
            [0xab, 0xab, 0xab, 255]
        );
        let mut output = Cursor::new(Vec::new());
        {
            let mut encoder = TiffEncoder::new(&mut output).unwrap();
            let mut image = encoder.new_image::<colortype::Gray8>(1, 1).unwrap();
            image.resolution_unit(::tiff::tags::ResolutionUnit::Inch);
            image.x_resolution(::tiff::encoder::Rational { n: 145, d: 2 });
            image.y_resolution(::tiff::encoder::Rational { n: 144, d: 1 });
            image.write_data(&[127]).unwrap();
        }
        let info = read_header(output.get_ref()).unwrap();
        assert_eq!(info.attribute.dpi_unit, ResolutionUnit::Inch);
        assert_eq!((info.attribute.x_dpi, info.attribute.y_dpi), (73, 144));
    }
    #[test]
    fn cmyk_tiff_and_truncated_invalid_data() {
        let mut output = Cursor::new(Vec::new());
        TiffEncoder::new(&mut output)
            .unwrap()
            .write_image::<colortype::CMYK8>(1, 1, &[0, 255, 255, 0])
            .unwrap();
        assert_eq!(decode(output.get_ref()).unwrap().pixels, [0, 0, 255, 255]);
        for end in [0, 1, 4, 8, output.get_ref().len() / 2] {
            assert!(decode(&output.get_ref()[..end]).is_err());
        }
        assert!(read_header(b"not a tiff").is_err());
    }
}

#[cfg(test)]
#[path = "tiff_tests.rs"]
mod expanded_tests;
