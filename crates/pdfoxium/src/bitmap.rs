// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Owned pixels and checked, borrowed render targets.

use crate::{Error, Result};
use std::io::{BufWriter, Write};
use std::path::Path;

/// Four-byte pixels with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PixelFormat {
    /// Blue, green, red, alpha: the renderer's native byte order.
    #[default]
    Bgra8,
    /// Red, green, blue, alpha.
    Rgba8,
}

/// An sRGB byte color, with straight alpha.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self::rgba(red, green, blue, 255)
    }

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    fn bytes(self, format: PixelFormat) -> [u8; 4] {
        match format {
            PixelFormat::Bgra8 => [self.blue, self.green, self.red, self.alpha],
            PixelFormat::Rgba8 => [self.red, self.green, self.blue, self.alpha],
        }
    }
}

pub(crate) fn checked_layout(width: u32, height: u32, stride: usize) -> Result<usize> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err(Error::InvalidArgument(
            "pixel dimensions must be positive and fit i32",
        ));
    }
    let row = (width as usize)
        .checked_mul(4)
        .ok_or(Error::InvalidArgument("pixel width overflow"))?;
    if stride < row || stride > u32::MAX as usize {
        return Err(Error::InvalidArgument(
            "pixel stride is too small or too large",
        ));
    }
    let length = stride
        .checked_mul(height as usize)
        .ok_or(Error::InvalidArgument("pixel buffer size overflow"))?;
    // The core DIB uses a checked u32 pitch * height calculation.
    if length > u32::MAX as usize || length > isize::MAX as usize {
        return Err(Error::InvalidArgument(
            "pixel buffer exceeds the renderer's size limit",
        ));
    }
    Ok(length)
}

/// An owned bitmap. It has no document references and can be sent to another thread.
#[derive(Debug)]
pub struct Bitmap {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: usize,
    pub(crate) format: PixelFormat,
    pub(crate) data: Vec<u8>,
}

impl Bitmap {
    /// Allocate a transparent, tightly packed bitmap.
    pub fn new(width: u32, height: u32, format: PixelFormat) -> Result<Self> {
        let stride = (width as usize)
            .checked_mul(4)
            .ok_or(Error::InvalidArgument("pixel width overflow"))?;
        let length = checked_layout(width, height, stride)?;
        let mut data = Vec::new();
        data.try_reserve_exact(length)
            .map_err(|_| Error::Allocation)?;
        data.resize(length, 0);
        Ok(Self {
            width,
            height,
            stride,
            format,
            data,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn stride(&self) -> usize {
        self.stride
    }
    pub fn format(&self) -> PixelFormat {
        self.format
    }
    pub fn pixels(&self) -> &[u8] {
        &self.data
    }
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
    pub fn into_pixels(self) -> Vec<u8> {
        self.data
    }

    pub fn view(&self) -> Pixels<'_> {
        Pixels {
            data: &self.data,
            width: self.width,
            height: self.height,
            stride: self.stride,
            format: self.format,
        }
    }

    pub fn view_mut(&mut self) -> PixelsMut<'_> {
        PixelsMut {
            data: &mut self.data,
            width: self.width,
            height: self.height,
            stride: self.stride,
            format: self.format,
        }
    }

    /// Write a PNG file, converting BGRA to RGBA one row at a time if needed.
    pub fn write_png(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut out = BufWriter::new(std::fs::File::create(path)?);
        self.encode_png(&mut out)?;
        out.flush()?;
        Ok(())
    }

    /// Write a PNG to a caller-owned writer. No full-image conversion copy is made.
    pub fn encode_png(&self, out: &mut impl Write) -> Result<()> {
        let mut encoder = png::Encoder::new(out, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(png_error)?;
        let row_bytes = self.width as usize * 4;
        let mut converted = Vec::new();
        if self.format == PixelFormat::Bgra8 {
            converted
                .try_reserve_exact(row_bytes)
                .map_err(|_| Error::Allocation)?;
            converted.resize(row_bytes, 0);
        }
        {
            let mut stream = writer.stream_writer().map_err(png_error)?;
            for row in self.data.chunks_exact(self.stride) {
                let row = &row[..row_bytes];
                if self.format == PixelFormat::Rgba8 {
                    stream.write_all(row)?;
                } else {
                    for (input, output) in row.chunks_exact(4).zip(converted.chunks_exact_mut(4)) {
                        output.copy_from_slice(&[input[2], input[1], input[0], input[3]]);
                    }
                    stream.write_all(&converted)?;
                }
            }
            stream.finish().map_err(png_error)?;
        }
        writer.finish().map_err(png_error)?;
        Ok(())
    }
}

fn png_error(error: png::EncodingError) -> Error {
    Error::Io(std::io::Error::other(error))
}

/// Read-only pixels. A view borrowed from a job is valid only until its next mutable operation.
#[derive(Clone, Copy, Debug)]
pub struct Pixels<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: usize,
    pub(crate) format: PixelFormat,
}

impl Pixels<'_> {
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn stride(&self) -> usize {
        self.stride
    }
    pub fn format(&self) -> PixelFormat {
        self.format
    }
    pub fn data(&self) -> &[u8] {
        self.data
    }
}

/// A validated, exclusively borrowed pixel buffer. Padding bytes are never cleared.
#[derive(Debug)]
pub struct PixelsMut<'a> {
    pub(crate) data: &'a mut [u8],
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: usize,
    pub(crate) format: PixelFormat,
}

impl<'a> PixelsMut<'a> {
    pub fn new(
        data: &'a mut [u8],
        width: u32,
        height: u32,
        stride: usize,
        format: PixelFormat,
    ) -> Result<Self> {
        let length = checked_layout(width, height, stride)?;
        let data = data
            .get_mut(..length)
            .ok_or(Error::InvalidArgument("pixel buffer is too short"))?;
        Ok(Self {
            data,
            width,
            height,
            stride,
            format,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn stride(&self) -> usize {
        self.stride
    }
    pub fn format(&self) -> PixelFormat {
        self.format
    }
    pub fn data(&self) -> &[u8] {
        self.data
    }
    pub fn data_mut(&mut self) -> &mut [u8] {
        self.data
    }

    pub fn view(&self) -> Pixels<'_> {
        Pixels {
            data: self.data,
            width: self.width,
            height: self.height,
            stride: self.stride,
            format: self.format,
        }
    }

    pub(crate) fn reborrow(&mut self) -> PixelsMut<'_> {
        PixelsMut {
            data: self.data,
            width: self.width,
            height: self.height,
            stride: self.stride,
            format: self.format,
        }
    }

    pub(crate) fn clear(&mut self, color: Color) {
        let color = color.bytes(self.format);
        for row in self.data.chunks_exact_mut(self.stride) {
            for pixel in row[..self.width as usize * 4].chunks_exact_mut(4) {
                pixel.copy_from_slice(&color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_layout_checks_and_clear_preserve_padding_and_tail() {
        let mut bytes = [0x7b; 30];
        {
            let mut pixels = PixelsMut::new(&mut bytes, 2, 2, 12, PixelFormat::Rgba8).unwrap();
            pixels.clear(Color::rgba(1, 2, 3, 4));
            assert_eq!(pixels.data().len(), 24);
        }
        assert_eq!(&bytes[..8], &[1, 2, 3, 4, 1, 2, 3, 4]);
        assert_eq!(&bytes[8..12], &[0x7b; 4]);
        assert_eq!(&bytes[12..20], &[1, 2, 3, 4, 1, 2, 3, 4]);
        assert_eq!(&bytes[20..], &[0x7b; 10]);
        for (w, h, stride) in [(0, 1, 4), (1, 0, 4), (2, 1, 7), (1, 8, 4), (u32::MAX, 1, 4)] {
            assert!(matches!(
                PixelsMut::new(&mut bytes, w, h, stride, PixelFormat::Bgra8),
                Err(Error::InvalidArgument(_))
            ));
        }
        assert!(checked_layout(1, 2, u32::MAX as usize).is_err());
    }

    #[test]
    fn png_is_rgba_for_both_pixel_orders_and_propagates_write_errors() {
        for format in [PixelFormat::Bgra8, PixelFormat::Rgba8] {
            let mut bitmap = Bitmap::new(2, 2, format).unwrap();
            bitmap.view_mut().clear(Color::rgba(30, 70, 190, 90));
            let mut encoded = Vec::new();
            bitmap.encode_png(&mut encoded).unwrap();
            let mut decoder = png::Decoder::new(std::io::Cursor::new(encoded))
                .read_info()
                .unwrap();
            let mut decoded = vec![0; decoder.output_buffer_size().unwrap()];
            let info = decoder.next_frame(&mut decoded).unwrap();
            assert_eq!(
                (info.width, info.height, info.color_type),
                (2, 2, png::ColorType::Rgba)
            );
            assert_eq!(decoded, [30, 70, 190, 90].repeat(4));
        }
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "fixture",
                ))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        assert!(matches!(
            Bitmap::new(1, 1, PixelFormat::Bgra8)
                .unwrap()
                .encode_png(&mut Broken),
            Err(Error::Io(_))
        ));
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Bitmap>();
    }
}
