// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! FaxModule::CreateDecoder and FaxDecoder from `fxcodec/fax/faxmodule.cpp`.
//! Safe codec row state retains native bit offsets, partial rows and rewind.

use crate::fxcodec::scanlinedecoder::{ScanlineDecoder, ScanlineState};

pub struct FaxScanlineDecoder<'a> {
    state: ScanlineState,
    decoder: hayro_ccitt::pdfium::FaxDecoder<'a>,
}

impl<'a> FaxScanlineDecoder<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        data: &'a [u8],
        width: i32,
        height: i32,
        k: i32,
        end_of_line: bool,
        byte_align: bool,
        black: bool,
        columns: i32,
        rows: i32,
    ) -> Option<Self> {
        let width = if columns != 0 { columns } else { width };
        let height = if rows != 0 { rows } else { height };
        let decoder = hayro_ccitt::pdfium::FaxDecoder::new(
            data,
            width.try_into().ok()?,
            height.try_into().ok()?,
            k,
            end_of_line,
            byte_align,
            black,
        )?;
        let pitch = decoder.pitch() as u32;
        Some(Self {
            state: ScanlineState::new(width, height, width, height, 1, 1, pitch),
            decoder,
        })
    }

    pub(crate) fn retain_source(self, source: std::rc::Rc<Vec<u8>>) -> FaxScanlineDecoder<'static> {
        FaxScanlineDecoder {
            state: self.state,
            decoder: self.decoder.retain_source(source),
        }
    }
}

impl ScanlineDecoder for FaxScanlineDecoder<'_> {
    fn state(&self) -> &ScanlineState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut ScanlineState {
        &mut self.state
    }
    fn rewind(&mut self) -> bool {
        self.decoder.rewind();
        true
    }
    fn get_next_line(&mut self) -> bool {
        self.decoder.next_line().is_some()
    }
    fn current_line(&self) -> &[u8] {
        self.decoder.current_line()
    }
    fn get_src_offset(&self) -> u32 {
        self.decoder.source_offset()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fax_nonzero_padding_disables_alignment_and_rewind_preserves_that_state() {
        let mut decoder =
            FaxScanlineDecoder::create(&[255, 128], 8, 9, -1, false, true, false, 0, 0).unwrap();
        for row in 0..9 {
            assert_eq!(decoder.get_scanline(row), &[255; 4]);
        }
        assert_eq!(decoder.get_src_offset(), 2);
        assert!(decoder.get_scanline(9).is_empty());
        for row in 0..9 {
            assert_eq!(decoder.get_scanline(row), &[255; 4]);
        }
        let mut aligned =
            FaxScanlineDecoder::create(&[128, 128], 8, 2, -1, false, true, true, 0, 0).unwrap();
        assert_eq!(aligned.get_scanline(0), &[0; 4]);
        assert_eq!(aligned.get_scanline(1), &[0; 4]);
        assert_eq!(aligned.get_src_offset(), 2);
        assert!(aligned.get_scanline(2).is_empty());
    }

    #[test]
    fn fax_partial_rows_and_overlong_runs_keep_native_resynchronization() {
        let mut decoder =
            FaxScanlineDecoder::create(&[0x35, 0xc0], 8, 2, 0, false, false, false, 0, 0).unwrap();
        // White run zero, black run two, then a truncated white code.
        assert_eq!(decoder.get_scanline(0), &[0x3f, 255, 255, 255]);
        assert_eq!(decoder.get_src_offset(), 2);
        assert!(decoder.get_scanline(1).is_empty());
        let mut decoder =
            FaxScanlineDecoder::create(&[0x9c], 1, 2, 0, false, false, false, 0, 0).unwrap();
        // An overlong white run stops immediately. It does not acquire the
        // invalid-code branch's scan to the next one bit.
        assert_eq!(decoder.get_scanline(0), &[255; 4]);
        assert_eq!(decoder.get_scanline(1), &[255; 4]);
        assert!(decoder.get_scanline(2).is_empty());
    }

    #[test]
    fn fax_g4_keeps_unrounded_bit_positions_and_fills_rows_after_eof() {
        let mut pixels = [0; 8];
        assert_eq!(
            hayro_ccitt::pdfium::decode_g4(&[0x18], 3, 8, 2, 4, &mut pixels),
            Some(5)
        );
        assert_eq!(pixels, [255; 8]);
        assert_eq!(
            hayro_ccitt::pdfium::decode_g4(&[], 0, 8, 2, 4, &mut pixels),
            Some(0)
        );
        assert_eq!(pixels, [255; 8]);
        // The native extension branch advances across a partial EOFB even
        // when it passes the end of the available byte.
        assert_eq!(
            hayro_ccitt::pdfium::decode_g4(&[0], 0, 8, 1, 4, &mut pixels[..4]),
            Some(12)
        );
    }
}
