// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Two-component JPEGs carry raw DeviceN samples, not RGB. The selected
//! libjpeg-turbo-rs codec supports gray/RGB/CMYK models but cannot decode this
//! case. Retain the previous safe Rust Zune path only for these JPEGs, including
//! its borrowed reader and box reduction. See libjpeg's JCS_UNKNOWN pass-through
//! and PDFium's unchanged component interpretation in CPDF_DIB.

use super::{endpoint, patch_known_bad_height, JpegInfo, JpegPixels};
use std::borrow::Cow;
use std::num::NonZeroU32;
use zune_jpeg::zune_core::bytestream::{ZByteIoError, ZByteReaderTrait, ZSeekFrom};
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::{JpegDecoder, SampleRatios};

// Zune's blanket BufRead+Seek adapter implements exact reads with a loop and
// rewinds after short reads. JPEG entropy decoding refills four bytes at a
// time, so use the already-borrowed input directly. In a std build ZCursor also
// uses that blanket adapter; deliberately implement only ZByteReaderTrait.
// Keep std::io::Cursor's EOF and seeking behavior, including short-read output
// and an unchanged position after a failed exact read.
struct SliceReader<'a> {
    source: &'a [u8],
    position: u64,
}

impl<'a> SliceReader<'a> {
    fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            position: 0,
        }
    }

    #[inline]
    fn remaining(&self) -> &[u8] {
        usize::try_from(self.position)
            .ok()
            .and_then(|position| self.source.get(position..))
            .unwrap_or_default()
    }
}

impl ZByteReaderTrait for &mut SliceReader<'_> {
    #[inline]
    fn read_byte_no_error(&mut self) -> u8 {
        match self.remaining().first() {
            Some(&byte) => {
                self.position += 1;
                byte
            }
            None => 0,
        }
    }

    #[inline]
    fn read_exact_bytes(&mut self, buffer: &mut [u8]) -> Result<(), ZByteIoError> {
        let available = self.remaining();
        if buffer.len() > available.len() {
            let count = available.len();
            buffer[..count].copy_from_slice(available);
            return Err(ZByteIoError::NotEnoughBytes(count, buffer.len()));
        }
        buffer.copy_from_slice(&available[..buffer.len()]);
        self.position += buffer.len() as u64;
        Ok(())
    }

    #[inline]
    fn read_bytes(&mut self, buffer: &mut [u8]) -> Result<usize, ZByteIoError> {
        let count = self.peek_bytes(buffer)?;
        self.position += count as u64;
        Ok(count)
    }

    #[inline]
    fn peek_bytes(&mut self, buffer: &mut [u8]) -> Result<usize, ZByteIoError> {
        let available = self.remaining();
        let count = buffer.len().min(available.len());
        buffer[..count].copy_from_slice(&available[..count]);
        Ok(count)
    }

    #[inline]
    fn peek_exact_bytes(&mut self, buffer: &mut [u8]) -> Result<(), ZByteIoError> {
        let position = self.position;
        let result = self.read_exact_bytes(buffer);
        self.position = position;
        result
    }

    #[inline]
    fn z_seek(&mut self, from: ZSeekFrom) -> Result<u64, ZByteIoError> {
        let (base, offset) = match from {
            ZSeekFrom::Start(position) => {
                self.position = position;
                return Ok(position);
            }
            ZSeekFrom::Current(offset) => (self.position, offset),
            ZSeekFrom::End(offset) => (self.source.len() as u64, offset),
        };
        let position = u64::try_from(i128::from(base) + i128::from(offset))
            .map_err(|_| ZByteIoError::SeekError("Invalid JPEG source position"))?;
        self.position = position;
        Ok(position)
    }

    #[inline]
    fn is_eof(&mut self) -> Result<bool, ZByteIoError> {
        Ok(self.position >= self.source.len() as u64)
    }

    #[inline]
    fn z_position(&mut self) -> Result<u64, ZByteIoError> {
        Ok(self.position)
    }

    fn read_remaining(&mut self, sink: &mut Vec<u8>) -> Result<usize, ZByteIoError> {
        let available = self.remaining();
        let count = available.len();
        sink.extend_from_slice(available);
        self.position += count as u64;
        Ok(count)
    }
}

fn options() -> DecoderOptions {
    DecoderOptions::default()
        .set_max_width(65500)
        .set_max_height(65500)
        .set_strict_mode(false)
}

fn info_from_decoder<T: ZByteReaderTrait>(decoder: &JpegDecoder<T>) -> Option<JpegInfo> {
    let info = decoder.info()?;
    if info.components != 2 {
        return None;
    }
    let (max_h_samp, max_v_samp) = match info.sample_ratio {
        SampleRatios::None => (1, 1),
        SampleRatios::H => (2, 1),
        SampleRatios::V => (1, 2),
        SampleRatios::HV => (2, 2),
        SampleRatios::Generic(h, v) => (h as usize, v as usize),
    };
    Some(JpegInfo {
        width: i32::from(info.width),
        height: i32::from(info.height),
        components: 2,
        bits_per_components: 8,
        // JCS_UNKNOWN: the two channels are passed to the PDF color space.
        color_transform: false,
        max_h_samp: max_h_samp.max(1),
        max_v_samp: max_v_samp.max(1),
    })
}

pub(super) fn load_info(source: &[u8]) -> Option<JpegInfo> {
    let mut reader = SliceReader::new(source);
    let mut decoder = JpegDecoder::new_with_options(&mut reader, options());
    decoder.decode_headers().ok()?;
    info_from_decoder(&decoder)
}

pub(super) fn decode(
    source: &mut Cow<'_, [u8]>,
    width: i32,
    height: i32,
    components: i32,
    scale_denom: u32,
) -> Option<JpegPixels> {
    let mut accept_known_bad_header = true;
    loop {
        let mut reader = SliceReader::new(source.as_ref());
        let mut decoder = JpegDecoder::new_with_options(&mut reader, options());
        if decoder.decode_headers().is_err() {
            drop(decoder);
            if !accept_known_bad_header || !patch_known_bad_height(source, width, height) {
                return None;
            }
            accept_known_bad_header = false;
            continue;
        }
        let info = info_from_decoder(&decoder)?;
        if (info.components as i32) < components || (info.width as u32) < width as u32 {
            return None;
        }
        let orig_width = info.width as usize;
        let orig_height = info.height as usize;
        let scale = if orig_width % (info.max_h_samp * 8) == 0
            && orig_height % (info.max_v_samp * 8) == 0
        {
            scale_denom as usize
        } else {
            1
        };
        // Set the raw two-band output before decode allocates its destination.
        // Zune recognizes this source model during setup_component_params;
        // its header-only input_colorspace still defaults to YCbCr.
        decoder.set_options(
            decoder
                .options()
                .jpeg_set_out_colorspace(ColorSpace::MultiBand(NonZeroU32::new(2)?)),
        );
        let mut pixels = decoder.decode().ok()?;
        drop(decoder);
        let src_row = orig_width.checked_mul(2)?;
        if pixels.len() != src_row.checked_mul(orig_height)? {
            return None;
        }
        let out_width = orig_width.div_ceil(scale);
        let out_height = orig_height.div_ceil(scale);
        let dest_row = out_width.checked_mul(2)?;
        if scale != 1 {
            // Preserve the former adapter's in-place, single-rounding box
            // reduction. These two-component images never used its SIMD path.
            for out_y in 0..out_height {
                let y_start = out_y * scale;
                let y_end = (y_start + scale).min(orig_height);
                for out_x in 0..out_width {
                    let x_start = out_x * scale;
                    let x_end = (x_start + scale).min(orig_width);
                    let count = ((y_end - y_start) * (x_end - x_start)) as u32;
                    for component in 0..2 {
                        let mut sum = 0;
                        for sy in y_start..y_end {
                            for sx in x_start..x_end {
                                sum += u32::from(pixels[sy * src_row + sx * 2 + component]);
                            }
                        }
                        pixels[out_y * dest_row + out_x * 2 + component] =
                            ((sum + count / 2) / count) as u8;
                    }
                }
            }
        }
        pixels.truncate(dest_row.checked_mul(out_height)?);
        return Some(JpegPixels {
            info,
            pixels,
            width: out_width as i32,
            height: out_height as i32,
            components: 2,
            bytes_consumed: endpoint::jpeg_endpoint(source)?,
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::fxcodec::jpeg::{create_decoder, decode, load_info};
    use crate::fxcodec::scanlinedecoder::ScanlineDecoder;

    // Source-derived JPEG framing as in the public C API constant-DCT tests:
    // one quantizer, two 1x1 components, DC=+64/-64 and zero AC coefficients.
    // The JPEG level shift and 8x8 IDCT therefore yield raw samples136/120.
    // This keeps the regression small and independent of an external encoder.
    fn constant_two_band_jpeg(width: u16, height: u16) -> Vec<u8> {
        fn segment(output: &mut Vec<u8>, marker: u8, payload: &[u8]) {
            output.extend_from_slice(&[0xff, marker]);
            output.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
            output.extend_from_slice(payload);
        }
        let mut output = vec![0xff, 0xd8];
        let mut quantizer = vec![1; 65];
        quantizer[0] = 0;
        segment(&mut output, 0xdb, &quantizer);
        let mut frame = vec![8];
        frame.extend_from_slice(&height.to_be_bytes());
        frame.extend_from_slice(&width.to_be_bytes());
        frame.extend_from_slice(&[2, 1, 0x11, 0, 2, 0x11, 0]);
        segment(&mut output, 0xc0, &frame);
        // DC codes0,10,110 map to categories0,7,11; AC code0 is EOB.
        let mut huffman = vec![0, 1, 1, 1];
        huffman.resize(17, 0);
        huffman.extend_from_slice(&[0, 7, 11, 0x10, 1]);
        huffman.resize(37, 0);
        huffman.push(0);
        segment(&mut output, 0xc4, &huffman);
        segment(&mut output, 0xda, &[2, 1, 0, 2, 0, 0, 63, 0]);
        let mut bits = Vec::new();
        for block in 0..usize::from(width).div_ceil(8) * usize::from(height).div_ceil(8) {
            for component in 0..2 {
                if block == 0 {
                    bits.extend_from_slice(&[true, false]); // DC category7.
                    let value = if component == 0 { 64u8 } else { 63u8 };
                    for shift in (0..7).rev() {
                        bits.push(value & (1 << shift) != 0);
                    }
                } else {
                    bits.push(false); // Subsequent DC difference0.
                }
                bits.push(false); // AC EOB.
            }
        }
        bits.resize(bits.len().div_ceil(8) * 8, true);
        for byte in bits.chunks_exact(8) {
            let value = byte
                .iter()
                .fold(0u8, |value, &bit| (value << 1) | u8::from(bit));
            output.push(value);
            if value == 0xff {
                output.push(0);
            }
        }
        output.extend_from_slice(&[0xff, 0xd9]);
        output
    }

    #[test]
    fn two_component_raw_samples_metadata_scales_and_inline_endpoint() {
        let jpeg = constant_two_band_jpeg(16, 8);
        let info = load_info(&jpeg).unwrap();
        assert_eq!((info.width, info.height, info.components), (16, 8, 2));
        assert!(!info.color_transform);
        let mut source = vec![b'j', 0]; // Scan to SOI as native does.
        source.extend_from_slice(&jpeg);
        source.extend_from_slice(b" EI q /Next Do Q");
        for transform in [false, true] {
            for scale in [1, 2, 4, 8] {
                let image = decode(&source, 16, 8, transform, scale).unwrap();
                assert_eq!(
                    (image.width, image.height, image.components),
                    (16 / scale as i32, 8 / scale as i32, 2)
                );
                assert_eq!(image.bytes_consumed, jpeg.len() as u32);
                assert_eq!(
                    image.pixels,
                    [136u8, 120].repeat((image.width * image.height) as usize)
                );
                let mut decoder =
                    create_decoder(&source, 16, 8, 2, transform, scale, false).unwrap();
                assert_eq!(decoder.count_comps(), 2);
                let first = decoder.get_scanline(0).to_vec();
                assert_eq!(first, [136u8, 120].repeat(image.width as usize));
                assert_eq!(decoder.get_scanline(image.height - 1), first);
                assert_eq!(decoder.get_scanline(0), first); // Rewind.
                assert_eq!(decoder.get_src_offset(), jpeg.len() as u32);
            }
        }
        assert!(decode(&jpeg, 17, 8, false, 1).is_none());
        assert!(create_decoder(&jpeg, 16, 8, 3, false, 1, false).is_none());
    }

    #[test]
    fn two_component_padded_edges_disable_reduction_and_keep_dword_rows() {
        let jpeg = constant_two_band_jpeg(15, 7);
        for scale in [1, 2, 4, 8] {
            let image = decode(&jpeg, 15, 7, false, scale).unwrap();
            assert_eq!((image.width, image.height), (15, 7));
            assert_eq!(image.pixels, [136u8, 120].repeat(15 * 7));
            let mut decoder = create_decoder(&jpeg, 15, 7, 2, false, scale, false).unwrap();
            let row = decoder.get_scanline(0);
            assert_eq!(row.len(), 32);
            assert_eq!(&row[..30], [136u8, 120].repeat(15));
            assert_eq!(&row[30..], &[0, 0]);
        }
    }
}
