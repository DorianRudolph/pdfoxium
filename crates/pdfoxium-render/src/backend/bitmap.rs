use crate::backend::{PdfColor, PdfiumError};
#[cfg(feature = "image_api")]
use image::DynamicImage;
use std::ops::{Deref, DerefMut};

pub(crate) enum Storage<'a> {
    Owned(Vec<u8>),
    Borrowed(&'a mut [u8]),
}
impl Deref for Storage<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Owned(v) => v,
            Self::Borrowed(v) => v,
        }
    }
}
impl DerefMut for Storage<'_> {
    fn deref_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Owned(v) => v,
            Self::Borrowed(v) => v,
        }
    }
}

pub type Pixels = i32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PdfBitmapFormat {
    Gray = 1,
    BGR = 2,
    BGRx = 3,
    #[default]
    BGRA = 4,
}
impl PdfBitmapFormat {
    pub(crate) fn bytes_per_pixel(self) -> i32 {
        match self {
            Self::Gray => 1,
            Self::BGR => 3,
            _ => 4,
        }
    }
    pub(crate) fn core(self) -> pdfoxium_core::fxge::dib::fx_dib::FxdibFormat {
        match self {
            Self::Gray => pdfoxium_core::fxge::dib::fx_dib::FxdibFormat::EightBppRgb,
            Self::BGR => pdfoxium_core::fxge::dib::fx_dib::FxdibFormat::Bgr,
            Self::BGRx => pdfoxium_core::fxge::dib::fx_dib::FxdibFormat::Bgrx,
            Self::BGRA => pdfoxium_core::fxge::dib::fx_dib::FxdibFormat::Bgra,
        }
    }
}

/// Owned pixels or an exclusively borrowed caller buffer. Borrowed storage never moves.
pub struct PdfBitmap<'a> {
    pub(crate) data: Storage<'a>,
    width: Pixels,
    height: Pixels,
    stride: usize,
    format: PdfBitmapFormat,
    reversed: bool,
}
impl<'a> PdfBitmap<'a> {
    pub fn preferred_stride_bytes(width: Pixels, format: PdfBitmapFormat) -> Option<i32> {
        if width < 0 {
            return None;
        }
        width
            .checked_mul(format.bytes_per_pixel())?
            .checked_add(3)
            .map(|n| n & !3)
    }
    pub fn bytes_required_for_size_and_format(
        width: Pixels,
        height: Pixels,
        format: PdfBitmapFormat,
    ) -> usize {
        if width < 0 || height < 0 {
            return 0;
        }
        Self::preferred_stride_bytes(width, format).map_or_else(
            || Self::bytes_required_for_size(width, height),
            |stride| (stride as usize).saturating_mul(height as usize),
        )
    }
    pub fn bytes_required_for_size(width: Pixels, height: Pixels) -> usize {
        if width < 0 || height < 0 {
            return 0;
        }
        (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4)
    }
    fn layout(
        width: Pixels,
        height: Pixels,
        format: PdfBitmapFormat,
    ) -> Result<(usize, usize), PdfiumError> {
        let stride = Self::preferred_stride_bytes(width, format)
            .ok_or(PdfiumError::ImageSizeOutOfBounds)? as usize;
        if width <= 0 || height <= 0 {
            return Err(PdfiumError::ImageSizeOutOfBounds);
        }
        let len = stride
            .checked_mul(height as usize)
            .filter(|n| *n <= i32::MAX as usize)
            .ok_or(PdfiumError::ImageSizeOutOfBounds)?;
        Ok((stride, len))
    }
    pub fn empty(
        width: Pixels,
        height: Pixels,
        format: PdfBitmapFormat,
    ) -> Result<PdfBitmap<'a>, PdfiumError> {
        let (stride, len) = Self::layout(width, height, format)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(len)
            .map_err(|_| PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        bytes.resize(len, 0);
        Ok(Self {
            data: Storage::Owned(bytes),
            width,
            height,
            stride,
            format,
            reversed: false,
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_bytes(
        width: Pixels,
        height: Pixels,
        format: PdfBitmapFormat,
        buffer: &'a mut [u8],
    ) -> Result<PdfBitmap<'a>, PdfiumError> {
        let (stride, len) = Self::layout(width, height, format)?;
        if buffer.len() < len {
            return Err(PdfiumError::ImageBufferTooSmall);
        }
        Ok(Self {
            data: Storage::Borrowed(&mut buffer[..len]),
            width,
            height,
            stride,
            format,
            reversed: false,
        })
    }
    pub(crate) fn set_reversed(&mut self, value: bool) {
        self.reversed = value;
    }
    pub fn width(&self) -> Pixels {
        self.width
    }
    pub fn height(&self) -> Pixels {
        self.height
    }
    pub fn format(&self) -> Result<PdfBitmapFormat, PdfiumError> {
        Ok(self.format)
    }
    pub fn as_raw_bytes(&self) -> Vec<u8> {
        self.data.to_vec()
    }
    pub fn as_raw_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
    pub fn as_rgba_bytes(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.width as usize * self.height as usize * 4);
        for row in self.data.chunks_exact(self.stride) {
            for p in row[..self.width as usize * self.format.bytes_per_pixel() as usize]
                .chunks_exact(self.format.bytes_per_pixel() as usize)
            {
                match self.format {
                    PdfBitmapFormat::Gray => result.extend_from_slice(&[p[0], p[0], p[0], 255]),
                    PdfBitmapFormat::BGR | PdfBitmapFormat::BGRx => {
                        result.extend_from_slice(&[p[2], p[1], p[0], 255])
                    }
                    PdfBitmapFormat::BGRA => result.extend_from_slice(&[p[2], p[1], p[0], p[3]]),
                }
            }
        }
        if self.reversed {
            for p in result.chunks_exact_mut(4) {
                p.swap(0, 2);
            }
        }
        result
    }
    #[cfg(feature = "image_api")]
    pub fn as_image(&self) -> Result<DynamicImage, PdfiumError> {
        if self.format == PdfBitmapFormat::Gray {
            let bytes = self
                .data
                .chunks_exact(self.stride)
                .flat_map(|row| row[..self.width as usize].iter().copied())
                .collect();
            return image::GrayImage::from_raw(self.width as u32, self.height as u32, bytes)
                .map(DynamicImage::ImageLuma8)
                .ok_or(PdfiumError::ImageError);
        }
        image::RgbaImage::from_raw(self.width as u32, self.height as u32, self.as_rgba_bytes())
            .map(image::DynamicImage::ImageRgba8)
            .ok_or(PdfiumError::ImageError)
    }
    pub fn fill_rect(
        &mut self,
        left: Pixels,
        top: Pixels,
        width: Pixels,
        height: Pixels,
        color: PdfColor,
    ) {
        let x0 = left.max(0).min(self.width) as usize;
        let y0 = top.max(0).min(self.height) as usize;
        let x1 = left.saturating_add(width).max(0).min(self.width) as usize;
        let y1 = top.saturating_add(height).max(0).min(self.height) as usize;
        let p = if self.reversed {
            [color.red(), color.green(), color.blue(), color.alpha()]
        } else {
            [color.blue(), color.green(), color.red(), color.alpha()]
        };
        let bpp = self.format.bytes_per_pixel() as usize;
        let stride = self.stride;
        let format = self.format;
        let bytes = &mut self.data;
        for y in y0..y1 {
            for x in x0..x1 {
                let dest = &mut bytes[y * stride + x * bpp..y * stride + (x + 1) * bpp];
                if format == PdfBitmapFormat::Gray {
                    dest[0] = ((color.red() as u32 * 77
                        + color.green() as u32 * 150
                        + color.blue() as u32 * 29)
                        >> 8) as u8;
                } else {
                    dest.copy_from_slice(&p[..bpp]);
                }
            }
        }
    }
    pub(crate) fn dib_mut(
        &mut self,
    ) -> Result<pdfoxium_core::fxge::dib::cfx_dibitmap::CfxDibitmapData<'_>, PdfiumError> {
        pdfoxium_core::fxge::dib::cfx_dibitmap::CfxDibitmapData::from_buffer(
            self.width,
            self.height,
            self.format.core(),
            self.stride as u32,
            &mut self.data,
        )
        .ok_or(PdfiumError::DataBufferLengthMismatch)
    }
}

impl PdfBitmap<'static> {
    pub(crate) fn from_core(
        mut bitmap: pdfoxium_core::fxge::dib::cfx_dibitmap::CfxDibitmap,
    ) -> Result<PdfBitmap<'static>, PdfiumError> {
        if !bitmap.convert_to_bgra() {
            return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
        }
        let mut result = Self::empty(bitmap.width(), bitmap.height(), PdfBitmapFormat::BGRA)?;
        let stride = result.stride;
        for y in 0..bitmap.height() as usize {
            result.data[y * stride..(y + 1) * stride].copy_from_slice(
                &bitmap.buffer()[y * bitmap.pitch() as usize..y * bitmap.pitch() as usize + stride],
            );
        }
        Ok(result)
    }
}

impl pdfoxium_core::fxge::dib::cstretchengine::StretchSource for PdfBitmap<'_> {
    type Row<'a>
        = &'a [u8]
    where
        Self: 'a;
    type Palette<'a>
        = &'a [u32]
    where
        Self: 'a;
    fn width(&self) -> i32 {
        self.width
    }
    fn height(&self) -> i32 {
        self.height
    }
    fn format(&self) -> pdfoxium_core::fxge::dib::fx_dib::FxdibFormat {
        self.format.core()
    }
    fn pitch(&self) -> u32 {
        self.stride as u32
    }
    fn palette(&self) -> &[u32] {
        &[]
    }
    fn scanline(&self, row: i32) -> &[u8] {
        let Some(start) = usize::try_from(row)
            .ok()
            .and_then(|row| row.checked_mul(self.stride))
        else {
            return &[];
        };
        self.data
            .get(start..start.saturating_add(self.stride))
            .unwrap_or(&[])
    }
}
