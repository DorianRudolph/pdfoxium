// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `FlateScanlineDecoder` and `FlatePredictorScanlineDecoder` from
//! `core/fxcodec/flate/flatemodule.cpp`.

use std::rc::Rc;

use zlib_rs::Inflate;

use crate::fxcodec::image_predictors::{
    get_predictor, png_predict_line, tiff_predict_line, PredictorType,
};
use crate::fxcodec::scanlinedecoder::{ScanlineDecoder, ScanlineSource, ScanlineState};
use crate::fxge::calculate_pitch::calculate_pitch8_or_die;

struct Inflater<'a> {
    src: ScanlineSource<'a>,
    state: Inflate,
    consumed: usize,
}

impl<'a> Inflater<'a> {
    fn new(src: &'a [u8]) -> Self {
        Self {
            src: ScanlineSource::Borrowed(src),
            state: Inflate::new(true, 15),
            consumed: 0,
        }
    }

    fn rewind(&mut self) {
        self.state.reset(true);
        self.consumed = 0;
    }

    fn output(&mut self, dest: &mut [u8]) {
        let result = super::flate_output(&mut self.state, &self.src[self.consumed..], dest);
        self.consumed += result.bytes_consumed;
        dest[result.bytes_written..].fill(0);
    }

    fn source_offset(&self) -> u32 {
        self.consumed.min(u32::MAX as usize) as u32
    }
}

struct PredictorState {
    predictor: PredictorType,
    colors: i32,
    bits_per_component: i32,
    columns: i32,
    predict_pitch: usize,
    left_over: usize,
    last_line: Vec<u8>,
    predict_buffer: Vec<u8>,
    predict_raw: Vec<u8>,
}

impl PredictorState {
    fn new(
        predictor: PredictorType,
        width: i32,
        comps: i32,
        bpc: i32,
        mut colors: i32,
        mut bits_per_component: i32,
        mut columns: i32,
    ) -> Self {
        if bits_per_component == 0 || colors == 0 || columns == 0 {
            bits_per_component = bpc;
            colors = comps;
            columns = width;
        }
        let predict_pitch = calculate_pitch8_or_die(
            bits_per_component
                .try_into()
                .expect("negative predictor bpc"),
            colors.try_into().expect("negative predictor components"),
            columns,
        ) as usize;
        Self {
            predictor,
            colors,
            bits_per_component,
            columns,
            predict_pitch,
            left_over: 0,
            last_line: vec![0; predict_pitch],
            predict_buffer: vec![0; predict_pitch],
            predict_raw: vec![
                0;
                predict_pitch
                    .checked_add(1)
                    .expect("predictor pitch overflow")
            ],
        }
    }

    fn bytes_per_pixel(&self) -> u32 {
        let bits = self
            .bits_per_component
            .checked_mul(self.colors)
            .expect("predictor bits per pixel overflow");
        u32::try_from(bits)
            .expect("negative predictor bits")
            .div_ceil(8)
    }

    fn next_line(
        &mut self,
        inflater: &mut Inflater<'_>,
        scanline: &mut [u8],
        output_width: i32,
        comps: i32,
        bpc: i32,
    ) {
        if scanline.len() == self.predict_pitch {
            match self.predictor {
                PredictorType::Png => {
                    inflater.output(&mut self.predict_raw);
                    png_predict_line(
                        scanline,
                        &self.predict_raw,
                        &self.last_line,
                        self.predict_pitch,
                        self.bytes_per_pixel(),
                    );
                    self.last_line.copy_from_slice(scanline);
                }
                PredictorType::Tiff => {
                    inflater.output(scanline);
                    tiff_predict_line(scanline, bpc, comps, output_width);
                }
                PredictorType::None => unreachable!(),
            }
            return;
        }

        let mut bytes_to_go = scanline.len();
        let read_leftover = self.left_over.min(bytes_to_go);
        if read_leftover != 0 {
            let start = self.predict_pitch - self.left_over;
            scanline[..read_leftover]
                .copy_from_slice(&self.predict_buffer[start..start + read_leftover]);
            self.left_over -= read_leftover;
            bytes_to_go -= read_leftover;
        }
        while bytes_to_go != 0 {
            match self.predictor {
                PredictorType::Png => {
                    inflater.output(&mut self.predict_raw);
                    let bytes_per_pixel = self.bytes_per_pixel();
                    png_predict_line(
                        &mut self.predict_buffer,
                        &self.predict_raw,
                        &self.last_line,
                        self.predict_pitch,
                        bytes_per_pixel,
                    );
                    self.last_line.copy_from_slice(&self.predict_buffer);
                }
                PredictorType::Tiff => {
                    inflater.output(&mut self.predict_buffer);
                    tiff_predict_line(
                        &mut self.predict_buffer,
                        self.bits_per_component,
                        self.colors,
                        self.columns,
                    );
                }
                PredictorType::None => unreachable!(),
            }
            let read_bytes = self.predict_pitch.min(bytes_to_go);
            let dest_start = scanline.len() - bytes_to_go;
            scanline[dest_start..dest_start + read_bytes]
                .copy_from_slice(&self.predict_buffer[..read_bytes]);
            self.left_over += self.predict_pitch - read_bytes;
            bytes_to_go -= read_bytes;
        }
    }
}

pub struct FlateScanlineDecoder<'a> {
    state: ScanlineState,
    inflater: Inflater<'a>,
    scanline: Vec<u8>,
    predictor: Option<PredictorState>,
}

impl<'a> FlateScanlineDecoder<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        src: &'a [u8],
        width: i32,
        height: i32,
        comps: i32,
        bpc: i32,
        predictor: i32,
        colors: i32,
        bits_per_component: i32,
        columns: i32,
    ) -> Self {
        let pitch = calculate_pitch8_or_die(
            bpc.try_into().expect("negative bits per component"),
            comps.try_into().expect("negative components"),
            width,
        );
        let predictor_type = get_predictor(predictor);
        let predictor = (predictor_type != PredictorType::None).then(|| {
            PredictorState::new(
                predictor_type,
                width,
                comps,
                bpc,
                colors,
                bits_per_component,
                columns,
            )
        });
        Self {
            state: ScanlineState::new(width, height, width, height, comps, bpc, pitch),
            inflater: Inflater::new(src),
            scanline: vec![0; pitch as usize],
            predictor,
        }
    }

    /// Retain the exact input allocation whose span was used at creation.
    /// Native CPDF_DIB owns its stream accessor before its decoder; sharing
    /// that immutable allocation expresses the same lifetime without a copy.
    pub(crate) fn retain_source(self, source: Rc<Vec<u8>>) -> FlateScanlineDecoder<'static> {
        assert_eq!(self.inflater.src.as_ptr(), source.as_ptr());
        assert_eq!(self.inflater.src.len(), source.len());
        FlateScanlineDecoder {
            state: self.state,
            inflater: Inflater {
                src: ScanlineSource::Shared(source),
                state: self.inflater.state,
                consumed: self.inflater.consumed,
            },
            scanline: self.scanline,
            predictor: self.predictor,
        }
    }
}

impl ScanlineDecoder for FlateScanlineDecoder<'_> {
    fn state(&self) -> &ScanlineState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ScanlineState {
        &mut self.state
    }

    fn rewind(&mut self) -> bool {
        self.inflater.rewind();
        if let Some(predictor) = &mut self.predictor {
            predictor.left_over = 0;
        }
        true
    }

    fn get_next_line(&mut self) -> bool {
        if let Some(predictor) = &mut self.predictor {
            predictor.next_line(
                &mut self.inflater,
                &mut self.scanline,
                self.state.output_width,
                self.state.comps,
                self.state.bpc,
            );
        } else {
            self.inflater.output(&mut self.scanline);
        }
        true
    }

    fn current_line(&self) -> &[u8] {
        &self.scanline
    }

    fn get_src_offset(&self) -> u32 {
        self.inflater.source_offset()
    }
}

#[cfg(test)]
mod tests {
    use super::FlateScanlineDecoder;
    use crate::fxcodec::flate::encode;
    use crate::fxcodec::scanlinedecoder::ScanlineDecoder;

    #[test]
    fn rows_cross_dictionary_boundaries_without_padding_or_shifting() {
        let pixels: Vec<u8> = (0..100_000).map(|index| (index % 251) as u8).collect();
        for predictor in [1, 12] {
            let mut raw = Vec::new();
            for row in pixels.chunks_exact(1_000) {
                if predictor == 12 {
                    raw.push(0); // PNG predictor: no per-row transform.
                }
                raw.extend_from_slice(row);
            }
            let encoded = encode(&raw);
            let mut decoder =
                FlateScanlineDecoder::new(&encoded, 1_000, 100, 1, 8, predictor, 1, 8, 1_000);
            for (index, expected) in pixels.chunks_exact(1_000).enumerate() {
                assert_eq!(
                    decoder.get_scanline(index as i32),
                    expected,
                    "row {index}, predictor {predictor}"
                );
            }
            assert_eq!(decoder.get_src_offset(), encoded.len() as u32);
            assert_eq!(decoder.get_scanline(0), &pixels[..1_000]);
        }
    }

    #[test]
    fn plain_rows_and_truncated_padding() {
        let data = encode(&[1, 2, 3, 4]);
        let mut decoder = FlateScanlineDecoder::new(&data, 2, 2, 1, 8, 1, 0, 0, 0);
        assert_eq!(decoder.get_scanline(0), [1, 2]);
        assert_eq!(decoder.get_scanline(1), [3, 4]);
        assert_eq!(decoder.get_src_offset(), data.len() as u32);
        assert_eq!(decoder.get_scanline(0), [1, 2]);

        let short = encode(&[5]);
        let mut decoder = FlateScanlineDecoder::new(&short, 3, 1, 1, 8, 1, 0, 0, 0);
        assert_eq!(decoder.get_scanline(0), [5, 0, 0]);
    }

    #[test]
    fn png_predictor_with_equal_and_different_row_pitches() {
        let data = encode(&[1, 1, 1, 1, 2, 1, 1, 1]);
        let mut decoder = FlateScanlineDecoder::new(&data, 3, 2, 1, 8, 12, 1, 8, 3);
        assert_eq!(decoder.get_scanline(0), [1, 2, 3]);
        assert_eq!(decoder.get_scanline(1), [2, 3, 4]);

        let data = encode(&[1, 1, 1, 1, 1, 2, 2, 2]);
        let mut decoder = FlateScanlineDecoder::new(&data, 2, 3, 1, 8, 12, 1, 8, 3);
        assert_eq!(decoder.get_scanline(0), [1, 2]);
        assert_eq!(decoder.get_scanline(1), [3, 2]);
        assert_eq!(decoder.get_scanline(2), [4, 6]);
    }

    #[test]
    fn tiff_predictor_with_different_row_pitch() {
        let data = encode(&[1, 1, 1, 2, 2, 2]);
        let mut decoder = FlateScanlineDecoder::new(&data, 2, 3, 1, 8, 2, 1, 8, 3);
        assert_eq!(decoder.get_scanline(0), [1, 2]);
        assert_eq!(decoder.get_scanline(1), [3, 2]);
        assert_eq!(decoder.get_scanline(2), [4, 6]);
    }
}
