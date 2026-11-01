// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! JPEG scanline state from `rust_jpeg_scanline_decoder.{h,cpp}`, with
//! libjpeg_scanline_decoder's requested width/component validation and source
//! offset boundary. The Rust codec currently decodes eagerly; a JPEG marker
//! walk supplies the consumed offset after all scanlines are read.

use super::{decode_with_validation, JpegPixels};
use crate::fxcodec::scanlinedecoder::{ScanlineDecoder, ScanlineState};
use crate::fxge::calculate_pitch::calculate_pitch32;

pub struct JpegScanlineDecoder {
    state: ScanlineState,
    decoded_image: Vec<u8>,
    next_row: u32,
    current_row: Option<usize>,
    source_offset: u32,
}
impl JpegScanlineDecoder {
    pub fn create(
        source: &[u8],
        width: u32,
        height: u32,
        components: i32,
        color_transform: bool,
        scale_denom: u32,
    ) -> Option<Self> {
        let decoded = decode_with_validation(
            source,
            width as i32,
            height as i32,
            components,
            color_transform,
            scale_denom,
        )?;
        Self::from_decoded(decoded)
    }

    /// Move the planned Rust codec's completed allocation into native row
    /// state. CPDF_DIB has already performed its header/fallback decisions.
    pub(crate) fn from_decoded(decoded: JpegPixels) -> Option<Self> {
        let info = decoded.info;
        let pitch = calculate_pitch32((8 * decoded.components) as i32, decoded.width)?;
        let row_bytes = (decoded.width as usize).checked_mul(decoded.components as usize)?;
        let size = (pitch as usize).checked_mul(decoded.height as usize)?;
        let mut pixels = decoded.pixels;
        pixels.resize(size, 0);
        // Expand to native DWORD pitch in place, working backwards so packed
        // source rows survive until their destination row has been written.
        if pitch as usize != row_bytes {
            for row in (0..decoded.height as usize).rev() {
                let destination = row * pitch as usize;
                pixels.copy_within(row * row_bytes..(row + 1) * row_bytes, destination);
                pixels[destination + row_bytes..destination + pitch as usize].fill(0);
            }
        }
        Some(Self {
            state: ScanlineState::new(
                info.width,
                info.height,
                decoded.width,
                decoded.height,
                decoded.components as i32,
                8,
                pitch,
            ),
            decoded_image: pixels,
            next_row: 0,
            current_row: None,
            source_offset: decoded.bytes_consumed,
        })
    }
}
impl ScanlineDecoder for JpegScanlineDecoder {
    fn state(&self) -> &ScanlineState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut ScanlineState {
        &mut self.state
    }
    fn rewind(&mut self) -> bool {
        self.next_row = 0;
        true
    }
    fn get_next_line(&mut self) -> bool {
        if self.next_row >= self.state.output_height as u32 {
            return false;
        }
        let offset = self.next_row as usize * self.state.pitch as usize;
        if offset + self.state.pitch as usize > self.decoded_image.len() {
            return false;
        }
        self.next_row += 1;
        self.current_row = Some(offset);
        true
    }
    fn current_line(&self) -> &[u8] {
        self.current_row.map_or(&[], |offset| {
            &self.decoded_image[offset..offset + self.state.pitch as usize]
        })
    }
    fn get_src_offset(&self) -> u32 {
        self.source_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcodec::jpeg::{decode, load_info};
    #[test]
    fn native_padded_rows_repeat_rewind_and_source_endpoint() {
        let jpeg = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/gray.jpg"
        ));
        let info = load_info(jpeg).unwrap();
        let mut source = jpeg.to_vec();
        source.extend_from_slice(b" EI q");
        let decoded = decode(&source, 0, 0, true, 1).unwrap();
        let mut scanline = JpegScanlineDecoder::create(&source, 0, 0, 0, true, 1).unwrap();
        let width = info.width as usize;
        let pitch = scanline.state.pitch as usize;
        for row in 0..info.height {
            let line = scanline.get_scanline(row);
            assert_eq!(
                &line[..width],
                &decoded.pixels[row as usize * width..(row as usize + 1) * width]
            );
            assert!(line[width..].iter().all(|&byte| byte == 0));
        }
        assert_eq!(scanline.get_src_offset(), jpeg.len() as u32);
        assert_eq!(scanline.get_scanline(0).len(), pitch);
        assert_eq!(&scanline.get_scanline(0)[..width], &decoded.pixels[..width]);
        assert!(scanline.get_scanline(info.height).is_empty());
        assert!(JpegScanlineDecoder::create(jpeg, info.width as u32 + 1, 0, 0, true, 1).is_none());
        assert!(JpegScanlineDecoder::create(jpeg, 0, 0, 3, true, 1).is_none());
    }
}
