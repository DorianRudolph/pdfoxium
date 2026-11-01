// Adapted from PDFium's Anti-Grain Geometry 2.3 for pdfoxium.
// Distributed under the original permissive license notice below.
//
// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Safe slice and row-offset translation of `third_party/agg23/agg_rendering_buffer.h`.

#[derive(Clone, Copy, Debug)]
pub struct RowData<'a> {
    pub x1: i32,
    pub x2: i32,
    pub ptr: Option<&'a [u8]>,
}

impl<'a> RowData<'a> {
    pub const fn new(x1: i32, x2: i32, ptr: Option<&'a [u8]>) -> Self {
        Self { x1, x2, ptr }
    }
}

#[derive(Debug)]
pub struct SpanData<'a> {
    pub x: i32,
    pub len: u32,
    pub ptr: Option<&'a mut [u8]>,
}

impl<'a> SpanData<'a> {
    pub const fn empty() -> Self {
        Self {
            x: 0,
            len: 0,
            ptr: None,
        }
    }

    pub const fn new(x: i32, len: u32, ptr: Option<&'a mut [u8]>) -> Self {
        Self { x, len, ptr }
    }
}

pub struct RenderingBuffer<'a> {
    buffer: Option<&'a mut [u8]>,
    width: u32,
    height: u32,
    stride: i32,
    row_offsets: Vec<usize>,
}

impl<'a> Default for RenderingBuffer<'a> {
    fn default() -> Self {
        Self {
            buffer: None,
            width: 0,
            height: 0,
            stride: 0,
            row_offsets: Vec::new(),
        }
    }
}

impl<'a> RenderingBuffer<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_buffer(buffer: &'a mut [u8], width: u32, height: u32, stride: i32) -> Option<Self> {
        let mut result = Self::new();
        result
            .attach(buffer, width, height, stride)
            .then_some(result)
    }

    pub fn attach(&mut self, buffer: &'a mut [u8], width: u32, height: u32, stride: i32) -> bool {
        let row_size = stride.unsigned_abs() as usize;
        let Some(required) = row_size.checked_mul(height as usize) else {
            return false;
        };
        if buffer.len() < required {
            return false;
        }
        self.row_offsets.clear();
        if self.row_offsets.try_reserve(height as usize).is_err() {
            return false;
        }
        for row in 0..height as usize {
            let index = if stride < 0 {
                height as usize - row - 1
            } else {
                row
            };
            self.row_offsets.push(index * row_size);
        }
        self.buffer = Some(buffer);
        self.width = width;
        self.height = height;
        self.stride = stride;
        true
    }

    pub fn buffer(&self) -> Option<&[u8]> {
        self.buffer.as_deref()
    }

    pub fn buffer_mut(&mut self) -> Option<&mut [u8]> {
        self.buffer.as_deref_mut()
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn stride(&self) -> i32 {
        self.stride
    }

    pub fn stride_abs(&self) -> u32 {
        self.stride.unsigned_abs()
    }

    pub fn row(&self, y: u32) -> Option<&[u8]> {
        let start = *self.row_offsets.get(y as usize)?;
        self.buffer()?
            .get(start..start + self.stride_abs() as usize)
    }

    pub fn row_mut(&mut self, y: u32) -> Option<&mut [u8]> {
        let start = *self.row_offsets.get(y as usize)?;
        let size = self.stride_abs() as usize;
        self.buffer_mut()?.get_mut(start..start + size)
    }

    pub fn next_row(&self, y: u32) -> Option<&[u8]> {
        self.row(y.checked_add(1)?)
    }

    pub fn row_offsets(&self) -> &[usize] {
        &self.row_offsets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_stride_reverses_rows_and_attach_reuses_storage() {
        let mut pixels = [1, 2, 3, 4, 5, 6];
        let mut buffer = RenderingBuffer::from_buffer(&mut pixels, 2, 3, -2).unwrap();
        assert_eq!(buffer.stride_abs(), 2);
        assert_eq!(buffer.row_offsets(), &[4, 2, 0]);
        assert_eq!(buffer.row(0), Some(&[5, 6][..]));
        assert_eq!(buffer.next_row(0), Some(&[3, 4][..]));
        buffer.row_mut(1).unwrap()[0] = 9;
        assert_eq!(buffer.row(1), Some(&[9, 4][..]));
        assert!(buffer.row(3).is_none());

        let mut other = [7, 8];
        assert!(buffer.attach(&mut other, 2, 1, 2));
        assert_eq!(buffer.row(0), Some(&[7, 8][..]));
        let mut too_short = [0; 1];
        assert!(!buffer.attach(&mut too_short, 2, 1, 2));
        assert_eq!(buffer.row(0), Some(&[7, 8][..]));
    }
}
