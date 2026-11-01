// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Pixel translation from `core/fpdfapi/page/cpdf_transferfuncdib.{h,cpp}`.
//! Retained transfer wrappers translate one guarded scanline at a time.

use super::cpdf_transferfunc::CpdfTransferFunc;
use crate::fxge::calculate_pitch::calculate_pitch32;
#[cfg(test)]
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::cstretchengine::StretchSource;
use crate::fxge::dib::fx_dib::FxdibFormat;
use std::cell::{RefCell, RefMut};
use std::rc::Rc;

fn dest_format(format: FxdibFormat) -> Option<FxdibFormat> {
    Some(match format {
        FxdibFormat::OneBppMask | FxdibFormat::EightBppMask => FxdibFormat::EightBppMask,
        FxdibFormat::Bgra => FxdibFormat::Bgra,
        FxdibFormat::Invalid => return None,
        _ => FxdibFormat::Bgrx,
    })
}

fn translate_scanline(
    source_format: FxdibFormat,
    palette: &[u32],
    width: usize,
    input: &[u8],
    output: &mut [u8],
    transfer: &CpdfTransferFunc,
) -> Option<()> {
    let format = dest_format(source_format)?;
    let red = transfer.samples_r();
    let green = transfer.samples_g();
    let blue = transfer.samples_b();
    match source_format {
        FxdibFormat::OneBppRgb | FxdibFormat::OneBppMask => {
            for x in 0..width {
                let value = if input[x / 8] & (0x80 >> (x % 8)) != 0 {
                    255
                } else {
                    0
                };
                if format == FxdibFormat::EightBppMask {
                    output[x] = red[value];
                } else {
                    output[x * 4..x * 4 + 4].copy_from_slice(&[
                        blue[value],
                        green[value],
                        red[value],
                        0,
                    ]);
                }
            }
        }
        FxdibFormat::EightBppMask => {
            for x in 0..width {
                output[x] = red[input[x] as usize];
            }
        }
        FxdibFormat::EightBppRgb => {
            for x in 0..width {
                let index = input[x] as usize;
                let [r, g, b] = if palette.is_empty() {
                    [index, index, index]
                } else {
                    let color = *palette.get(index)?;
                    [
                        (color & 255) as usize,
                        ((color >> 8) & 255) as usize,
                        ((color >> 16) & 255) as usize,
                    ]
                };
                // Native palette translation uses blue as the red-table index
                // and red as the blue-table index; preserve that ordering.
                output[x * 4..x * 4 + 4].copy_from_slice(&[blue[b], green[g], red[r], 0]);
            }
        }
        FxdibFormat::Bgr => translate_rgb_line::<3, false>(input, output, width, red, green, blue),
        FxdibFormat::Bgrx => translate_rgb_line::<4, false>(input, output, width, red, green, blue),
        FxdibFormat::Bgra => translate_rgb_line::<4, true>(input, output, width, red, green, blue),
        FxdibFormat::Invalid => unreachable!(),
    }

    Some(())
}

fn translate_rgb_line<const STRIDE: usize, const ALPHA: bool>(
    input: &[u8],
    output: &mut [u8],
    width: usize,
    red: &[u8; 256],
    green: &[u8; 256],
    blue: &[u8; 256],
) {
    for (src, dst) in input[..width * STRIDE]
        .chunks_exact(STRIDE)
        .zip(output[..width * 4].chunks_exact_mut(4))
    {
        dst.copy_from_slice(&[
            blue[usize::from(src[0])],
            green[usize::from(src[1])],
            red[usize::from(src[2])],
            if ALPHA { src[3] } else { 0 },
        ]);
    }
}

pub struct CpdfTransferFuncDib<S: StretchSource> {
    source: Rc<S>,
    transfer: Rc<CpdfTransferFunc>,
    format: FxdibFormat,
    pitch: u32,
    scanline: RefCell<Vec<u8>>,
}

impl<S: StretchSource> CpdfTransferFuncDib<S> {
    pub fn new(source: Rc<S>, transfer: Rc<CpdfTransferFunc>) -> Option<Self> {
        let format = dest_format(source.format())?;
        let bpp = crate::fxge::dib::fx_dib::get_bpp_from_format(format) as i32;
        let pitch = calculate_pitch32(bpp, source.width())?;
        Some(Self {
            source,
            transfer,
            format,
            pitch,
            scanline: RefCell::new(vec![0; pitch as usize]),
        })
    }
    pub fn get_scanline(&self, row: i32) -> RefMut<'_, [u8]> {
        let input = self.source.scanline(row);
        let palette = self.source.palette();
        let mut output = self.scanline.borrow_mut();
        translate_scanline(
            self.source.format(),
            &palette,
            self.source.width() as usize,
            &input,
            &mut output,
            &self.transfer,
        )
        .expect("valid source palette");
        RefMut::map(output, |output| output.as_mut_slice())
    }
}

impl<S: StretchSource> StretchSource for CpdfTransferFuncDib<S> {
    type Row<'a>
        = RefMut<'a, [u8]>
    where
        Self: 'a;
    type Palette<'a>
        = &'a [u32]
    where
        Self: 'a;
    fn width(&self) -> i32 {
        self.source.width()
    }
    fn height(&self) -> i32 {
        self.source.height()
    }
    fn format(&self) -> FxdibFormat {
        self.format
    }
    fn pitch(&self) -> u32 {
        self.pitch
    }
    fn palette(&self) -> &[u32] {
        &[]
    }
    fn scanline(&self, row: i32) -> RefMut<'_, [u8]> {
        self.get_scanline(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inverted_transfer() -> Rc<CpdfTransferFunc> {
        let inverted = std::array::from_fn(|i| 255 - i as u8);
        Rc::new(CpdfTransferFunc::new(false, inverted, inverted, inverted))
    }

    #[test]
    fn rgb_rows_preserve_channel_tables_alpha_and_unaligned_guards() {
        let transfer = CpdfTransferFunc::new(
            false,
            std::array::from_fn(|i| i as u8 ^ 0x95),
            std::array::from_fn(|i| 255 - i as u8),
            std::array::from_fn(|i| (i as u8).rotate_left(3)),
        );
        for width in [0usize, 1, 2, 7, 16, 33, 257] {
            for format in [FxdibFormat::Bgr, FxdibFormat::Bgrx, FxdibFormat::Bgra] {
                let stride = if format == FxdibFormat::Bgr { 3 } else { 4 };
                let source: Vec<u8> = (0..3 + width * stride + 5)
                    .map(|i| (i * 73 + 19) as u8)
                    .collect();
                let mut actual = vec![0xa5; 1 + width * 4 + 7];
                let mut expected = actual.clone();
                for (input, output) in source[3..3 + width * stride]
                    .chunks_exact(stride)
                    .zip(expected[1..1 + width * 4].chunks_exact_mut(4))
                {
                    output.copy_from_slice(&[
                        input[0].rotate_left(3),
                        255 - input[1],
                        input[2] ^ 0x95,
                        if format == FxdibFormat::Bgra {
                            input[3]
                        } else {
                            0
                        },
                    ]);
                }
                translate_scanline(
                    format,
                    &[],
                    width,
                    &source[3..],
                    &mut actual[1..],
                    &transfer,
                )
                .unwrap();
                assert_eq!(actual, expected, "{format:?} width={width}");
            }
        }
    }

    #[test]
    fn wrapper_retains_source_and_tables_and_reads_only_the_requested_row() {
        let mut bitmap = CfxDibitmap::create(1, 2, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0x80102030);
        let source = Rc::new(RefCell::new(bitmap));
        let weak_source = Rc::downgrade(&source);
        let transfer = inverted_transfer();
        let weak_transfer = Rc::downgrade(&transfer);
        let wrapper = CpdfTransferFuncDib::new(Rc::clone(&source), transfer).unwrap();
        // Mutation after wrapper construction must be visible when sampled.
        source.borrow_mut().scanline_mut(1).unwrap()[..4].copy_from_slice(&[1, 2, 3, 99]);
        drop(source);
        assert!(weak_transfer.upgrade().is_some());
        assert_eq!(&*wrapper.get_scanline(1), &[254, 253, 252, 99]);
        assert_eq!(&*wrapper.get_scanline(0), &[207, 223, 239, 128]);
        assert_eq!(wrapper.estimated_image_memory_burden(), 0);
        drop(wrapper);
        assert!(weak_source.upgrade().is_none());
        assert!(weak_transfer.upgrade().is_none());
    }

    #[test]
    fn translates_bgra_without_changing_alpha() {
        let mut source = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        source.scanline_mut(0).unwrap()[..4].copy_from_slice(&[10, 20, 30, 40]);
        let output = inverted_transfer()
            .translate_image(Rc::new(source))
            .unwrap();
        assert_eq!(output.format(), FxdibFormat::Bgra);
        assert_eq!(&output.scanline(0)[..4], &[245, 235, 225, 40]);
    }

    #[test]
    fn translates_indexed_color_and_bit_mask() {
        let mut indexed = CfxDibitmap::create(1, 1, FxdibFormat::EightBppRgb, 0).unwrap();
        indexed.take_palette(vec![0xff10_2030]);
        indexed.scanline_mut(0).unwrap()[0] = 0;
        let output = inverted_transfer()
            .translate_image(Rc::new(indexed))
            .unwrap();
        assert_eq!(&output.scanline(0)[..3], &[239, 223, 207]);

        let mut mask = CfxDibitmap::create(2, 1, FxdibFormat::OneBppMask, 0).unwrap();
        mask.scanline_mut(0).unwrap()[0] = 0x80;
        let output = inverted_transfer().translate_image(Rc::new(mask)).unwrap();
        assert_eq!(output.format(), FxdibFormat::EightBppMask);
        assert_eq!(&output.scanline(0)[..2], &[0, 255]);
    }
}
