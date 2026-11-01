// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Published pure Rust PNG adapter from `png/rust_png_ffi.rs` and context.
use super::progressive_decoder_context::*;
use std::io::Cursor;
pub fn read_header(data: &[u8]) -> CodecResult<ImageInfo> {
    // Same fast header path as PDFium's Rust decoder; CRC and compressed data
    // are validated by the published decoder before any pixels are accepted.
    if bytes(data, 0, 8)? != b"\x89PNG\r\n\x1a\n" || bytes(data, 12, 4)? != b"IHDR" {
        return Err(CodecError::InvalidData);
    }
    let width = u32::from_be_bytes(bytes(data, 16, 4)?.try_into().unwrap());
    let height = u32::from_be_bytes(bytes(data, 20, 4)?.try_into().unwrap());
    ImageInfo::new(width as usize, height as usize, Format::Bgra)
}
pub fn decode(data: &[u8]) -> CodecResult<DecodedImage> {
    let info = read_header(data)?;
    let mut decoder = ::png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(::png::Transformations::EXPAND | ::png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| CodecError::InvalidData)?;
    let mut decoded = allocate(reader.output_buffer_size().ok_or(CodecError::InvalidData)?)?;
    let frame = reader
        .next_frame(&mut decoded)
        .map_err(|_| CodecError::InvalidData)?;
    if frame.width != info.width as u32
        || frame.height != info.height as u32
        || frame.bit_depth != ::png::BitDepth::Eight
    {
        return Err(CodecError::InvalidData);
    }
    const DEFAULT_GAMMA: f64 = 0.45455;
    let gamma = if reader.info().srgb.is_some() {
        DEFAULT_GAMMA
    } else {
        reader
            .info()
            .gama_chunk
            .map(|gamma| f64::from(gamma.into_value()))
            .filter(|gamma| *gamma > 0.0)
            .unwrap_or(DEFAULT_GAMMA)
    };
    let exponent = 1.0 / (gamma * 2.2);
    let lut: [u8; 256] = std::array::from_fn(|i| {
        if (exponent - 1.0).abs() < 1e-4 {
            i as u8
        } else {
            ((i as f64 / 255.0).powf(exponent) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        }
    });
    let mut image = DecodedImage::new(info)?;
    let components = match frame.color_type {
        ::png::ColorType::Grayscale => 1,
        ::png::ColorType::GrayscaleAlpha => 2,
        ::png::ColorType::Rgb => 3,
        ::png::ColorType::Rgba => 4,
        _ => return Err(CodecError::InvalidData),
    };
    for (dest, source) in image
        .pixels
        .chunks_exact_mut(4)
        .zip(decoded[..frame.buffer_size()].chunks_exact(components))
    {
        let (red, green, blue, alpha) = match components {
            1 => (source[0], source[0], source[0], 255),
            2 => (source[0], source[0], source[0], source[1]),
            3 => (source[0], source[1], source[2], 255),
            _ => (source[0], source[1], source[2], source[3]),
        };
        dest.copy_from_slice(&[
            lut[blue as usize],
            lut[green as usize],
            lut[red as usize],
            alpha,
        ]);
    }
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grayscale_alpha_rgb_rgba_and_sixteen_bit_samples() {
        for (color, depth, samples, expected) in [
            (
                ::png::ColorType::Grayscale,
                ::png::BitDepth::Eight,
                vec![40],
                vec![40, 40, 40, 255],
            ),
            (
                ::png::ColorType::GrayscaleAlpha,
                ::png::BitDepth::Eight,
                vec![40, 80],
                vec![40, 40, 40, 80],
            ),
            (
                ::png::ColorType::Rgb,
                ::png::BitDepth::Eight,
                vec![10, 20, 30],
                vec![30, 20, 10, 255],
            ),
            (
                ::png::ColorType::Rgba,
                ::png::BitDepth::Eight,
                vec![10, 20, 30, 80],
                vec![30, 20, 10, 80],
            ),
            (
                ::png::ColorType::Grayscale,
                ::png::BitDepth::Sixteen,
                vec![0xab, 0xcd],
                vec![0xab, 0xab, 0xab, 255],
            ),
        ] {
            let mut output = Vec::new();
            {
                let mut encoder = ::png::Encoder::new(&mut output, 1, 1);
                encoder.set_color(color);
                encoder.set_depth(depth);
                encoder
                    .write_header()
                    .unwrap()
                    .write_image_data(&samples)
                    .unwrap();
            }
            assert_eq!(decode(&output).unwrap().pixels, expected);
        }
    }
    #[test]
    fn indexed_palette_transparency_and_crc_rejection() {
        let mut output = Vec::new();
        {
            let mut encoder = ::png::Encoder::new(&mut output, 2, 1);
            encoder.set_color(::png::ColorType::Indexed);
            encoder.set_depth(::png::BitDepth::One);
            encoder.set_palette(vec![10, 20, 30, 40, 50, 60]);
            encoder.set_trns(vec![0, 80]);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[0x40])
                .unwrap();
        }
        assert_eq!(
            decode(&output).unwrap().pixels,
            [30, 20, 10, 0, 60, 50, 40, 80]
        );
        output[29] ^= 1;
        assert!(decode(&output).is_err());
    }
}
