// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxge/dib/cfx_imagetransformer.{h,cpp}`: staged stretching, rotation,
//! and fixed-point bilinear affine transformation.

use super::cfx_bitmapstorer::CfxBitmapStorer;
use super::cfx_dibitmap::CfxDibitmap;
use super::cfx_imagestretcher::CfxImageStretcher;
use super::cstretchengine::StretchSource;
use super::fx_dib::{argb_encode, FxdibFormat, FxdibResampleOptions};
use crate::fxcrt::fx_coordinates::{CfxMatrix, FxRect};
use crate::fxcrt::pauseindicator_iface::PauseIndicatorIface;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Eq, PartialEq)]
enum StretchType {
    None,
    Normal,
    Rotate,
    Other,
}

pub struct CfxImageTransformer<S: StretchSource> {
    source: Rc<S>,
    matrix: CfxMatrix,
    stretch_clip: FxRect,
    result: FxRect,
    sample_origin: (i32, i32),
    sample_offset: (i32, i32),
    dest_to_stretch: CfxMatrix,
    stretcher: Option<CfxImageStretcher<S>>,
    storer: CfxBitmapStorer,
    stretch_type: StretchType,
}

impl<S: StretchSource> CfxImageTransformer<S> {
    pub fn new(
        source: Rc<S>,
        matrix: CfxMatrix,
        options: FxdibResampleOptions,
        clip: Option<FxRect>,
    ) -> Self {
        let mut transformer = Self {
            source,
            matrix,
            stretch_clip: FxRect::default(),
            result: FxRect::default(),
            sample_origin: (0, 0),
            sample_offset: (0, 0),
            dest_to_stretch: CfxMatrix::default(),
            stretcher: None,
            storer: CfxBitmapStorer::new(),
            stretch_type: StretchType::None,
        };
        let result_rect = matrix.get_unit_rect().get_closest_rect();
        if !result_rect.valid() {
            return transformer;
        }
        let mut result_clip = result_rect;
        if let Some(clip) = clip {
            result_clip.intersect(clip);
        }
        if result_clip.is_empty() {
            return transformer;
        }
        transformer.result = result_clip;
        let Some(relative) = relative_clip(result_clip, result_rect) else {
            return transformer;
        };
        transformer.sample_origin = (result_rect.left, result_rect.top);
        transformer.sample_offset = (relative.left, relative.top);
        if matrix.a.abs() < matrix.b.abs() / 20.0
            && matrix.d.abs() < matrix.c.abs() / 20.0
            && matrix.a.abs() < 0.5
            && matrix.d.abs() < 0.5
        {
            let (width, height) = (result_rect.width(), result_rect.height());
            let Some(relative) = relative_clip(result_clip, result_rect) else {
                return transformer;
            };
            let relative = relative.swapped_clip_box(width, height, matrix.c > 0.0, matrix.b < 0.0);
            transformer.start_stretcher(height, width, relative, options);
            transformer.stretch_type = StretchType::Rotate;
            return transformer;
        }
        if matrix.b.abs() < 0.05 && matrix.c.abs() < 0.05 {
            let width = if matrix.a > 0.0 {
                matrix.a.ceil()
            } else {
                matrix.a.floor()
            } as i32;
            let height = if matrix.d > 0.0 {
                -matrix.d.ceil()
            } else {
                -matrix.d.floor()
            } as i32;
            let Some(relative) = relative_clip(result_clip, result_rect) else {
                return transformer;
            };
            transformer.start_stretcher(width, height, relative, options);
            transformer.stretch_type = StretchType::Normal;
            return transformer;
        }
        let width = matrix.a.hypot(matrix.b).ceil() as i32;
        if width == 0 {
            return transformer;
        }
        let height = matrix.c.hypot(matrix.d).ceil() as i32;
        if height == 0 {
            return transformer;
        }
        let mut stretch_to_dest = CfxMatrix::new(1.0, 0.0, 0.0, -1.0, 0.0, height as f32);
        stretch_to_dest.concat(CfxMatrix::new(
            matrix.a / width as f32,
            matrix.b / width as f32,
            matrix.c / height as f32,
            matrix.d / height as f32,
            matrix.e,
            matrix.f,
        ));
        let dest_to_stretch = stretch_to_dest.get_inverse();
        transformer.dest_to_stretch = dest_to_stretch;
        // Quantize the sampling matrix once in the full image's coordinate
        // frame. Quantizing after each clip-origin translation changes its
        // phase, so a region cannot reproduce the whole-image pixels.
        let fixed = BilinearMatrix::new(transformer.sampling_matrix());
        let mut bounds = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for (x, y) in [
            (relative.left, relative.top),
            (relative.right - 1, relative.top),
            (relative.left, relative.bottom - 1),
            (relative.right - 1, relative.bottom - 1),
        ] {
            let (x, y, _, _) = fixed.transform(x, y);
            bounds.0 = bounds.0.min(x);
            bounds.1 = bounds.1.min(y);
            bounds.2 = bounds.2.max(x);
            bounds.3 = bounds.3.max(y);
        }
        // The sampler accepts coordinates exactly at the source extent and
        // clamps them to the last pixel. Keep that pixel even when every
        // sample in a thin affine image lies on this inclusive boundary.
        if bounds.0 == width {
            bounds.0 -= 1;
        }
        if bounds.1 == height {
            bounds.1 -= 1;
        }
        // Keep the right/lower neighbor required by bilinear interpolation.
        let mut stretch_clip = FxRect::new(
            bounds.0,
            bounds.1,
            bounds.2.saturating_add(2),
            bounds.3.saturating_add(2),
        );
        if !stretch_clip.valid() {
            return transformer;
        }
        stretch_clip.intersect(FxRect::new(0, 0, width, height));
        if !stretch_clip.valid() {
            return transformer;
        }
        transformer.stretch_clip = stretch_clip;
        transformer.start_stretcher(width, height, stretch_clip, options);
        transformer.stretch_type = StretchType::Other;
        transformer
    }

    fn start_stretcher(
        &mut self,
        width: i32,
        height: i32,
        clip: FxRect,
        options: FxdibResampleOptions,
    ) {
        let mut stretcher =
            CfxImageStretcher::new(Rc::clone(&self.source), width, height, clip, options);
        stretcher.start(&mut self.storer);
        self.stretcher = Some(stretcher);
    }

    pub fn result(&self) -> FxRect {
        self.result
    }
    pub fn detach_bitmap(&mut self) -> Option<Rc<RefCell<CfxDibitmap>>> {
        self.storer.detach()
    }

    pub fn continue_transform(&mut self, pause: Option<&mut dyn PauseIndicatorIface>) -> bool {
        if self.stretch_type == StretchType::None {
            return false;
        }
        if self
            .stretcher
            .as_mut()
            .expect("stretcher")
            .continue_stretch(&mut self.storer, pause)
        {
            return true;
        }
        match self.stretch_type {
            StretchType::None => unreachable!(),
            StretchType::Normal => {}
            StretchType::Rotate => {
                if let Some(bitmap) = self.storer.bitmap() {
                    let bitmap = bitmap
                        .borrow()
                        .swap_xy(self.matrix.c > 0.0, self.matrix.b < 0.0);
                    self.storer
                        .replace(bitmap.map(|bitmap| Rc::new(RefCell::new(bitmap))));
                }
            }
            StretchType::Other => self.continue_other(),
        }
        false
    }

    fn sampling_matrix(&self) -> CfxMatrix {
        let mut matrix = CfxMatrix::new(
            1.,
            0.,
            0.,
            1.,
            self.sample_origin.0 as f32,
            self.sample_origin.1 as f32,
        );
        matrix.concat(self.dest_to_stretch);
        matrix
    }

    fn continue_other(&mut self) {
        let Some(bitmap) = self.storer.bitmap() else {
            return;
        };
        let source = bitmap.borrow();
        let dest_format = if is_mask(self.source.format()) {
            FxdibFormat::EightBppMask
        } else {
            FxdibFormat::Bgra
        };
        let Some(mut transformed) =
            CfxDibitmap::create(self.result.width(), self.result.height(), dest_format, 0)
        else {
            return;
        };
        let fixed = BilinearMatrix::new(self.sampling_matrix());
        let source_pitch = source.pitch() as usize;
        let source_buf = source.buffer();
        // Select the format outside the pixel loop, as CalcAlpha/Mono/Color do
        // upstream. Color samples share their four source-pixel addresses.
        if is_mask(source.format()) {
            bilinear_loop(
                &mut transformed,
                &fixed,
                self.sample_offset,
                self.stretch_clip,
                source_pitch,
                |data| [bilinear_interpolate(source_buf, data, 1, 0)],
            );
        } else if source.format() == FxdibFormat::EightBppRgb {
            let mut argb = [0u32; 256];
            if source.palette().is_empty() {
                for (index, entry) in argb.iter_mut().enumerate() {
                    *entry = argb_encode(255, index as u32, index as u32, index as u32);
                }
            } else {
                argb.copy_from_slice(&source.palette()[..256]);
            }
            bilinear_loop(
                &mut transformed,
                &fixed,
                self.sample_offset,
                self.stretch_clip,
                source_pitch,
                |data| {
                    argb[usize::from(bilinear_interpolate(source_buf, data, 1, 0))].to_le_bytes()
                },
            );
        } else {
            match source.format() {
                FxdibFormat::Bgr => bilinear_loop(
                    &mut transformed,
                    &fixed,
                    self.sample_offset,
                    self.stretch_clip,
                    source_pitch,
                    |data| bilinear_color::<3, false>(source_buf, data),
                ),
                FxdibFormat::Bgrx => bilinear_loop(
                    &mut transformed,
                    &fixed,
                    self.sample_offset,
                    self.stretch_clip,
                    source_pitch,
                    |data| bilinear_color::<4, false>(source_buf, data),
                ),
                FxdibFormat::Bgra => bilinear_loop(
                    &mut transformed,
                    &fixed,
                    self.sample_offset,
                    self.stretch_clip,
                    source_pitch,
                    |data| bilinear_color::<4, true>(source_buf, data),
                ),
                _ => unreachable!("stretched color format"),
            }
        }
        drop(source);
        self.storer
            .replace(Some(Rc::new(RefCell::new(transformed))));
    }
}

fn is_mask(format: FxdibFormat) -> bool {
    matches!(format, FxdibFormat::OneBppMask | FxdibFormat::EightBppMask)
}

fn relative_clip(rect: FxRect, origin: FxRect) -> Option<FxRect> {
    Some(FxRect::new(
        rect.left.checked_sub(origin.left)?,
        rect.top.checked_sub(origin.top)?,
        rect.right.checked_sub(origin.left)?,
        rect.bottom.checked_sub(origin.top)?,
    ))
}

struct BilinearMatrix {
    a: i32,
    b: i32,
    c: i32,
    d: i32,
    e: i32,
    f: i32,
}
impl BilinearMatrix {
    fn new(matrix: CfxMatrix) -> Self {
        let fixed = |value: f32| (value * 256.0).round() as i32;
        Self {
            a: fixed(matrix.a),
            b: fixed(matrix.b),
            c: fixed(matrix.c),
            d: fixed(matrix.d),
            e: fixed(matrix.e),
            f: fixed(matrix.f),
        }
    }
    fn transform(&self, x: i32, y: i32) -> (i32, i32, i32, i32) {
        let xx = self.a as f32 * x as f32 + self.c as f32 * y as f32 + self.e as f32 + 128.0;
        let yy = self.b as f32 * x as f32 + self.d as f32 * y as f32 + self.f as f32 + 128.0;
        let mut res_x = xx as i32 % 256;
        let mut res_y = yy as i32 % 256;
        if res_x < 0 && res_x > -256 {
            res_x += 256;
        }
        if res_y < 0 && res_y > -256 {
            res_y += 256;
        }
        ((xx / 256.0) as i32, (yy / 256.0) as i32, res_x, res_y)
    }
}

#[derive(Clone, Copy)]
struct BilinearData {
    res_x: i32,
    res_y: i32,
    left: usize,
    right: usize,
    upper: usize,
    lower: usize,
}

fn adjust_coords(clip: FxRect, col: &mut i32, row: &mut i32) {
    if *col == clip.width() {
        *col -= 1;
    }
    if *row == clip.height() {
        *row -= 1;
    }
}

fn bilinear_loop<const N: usize>(
    dest: &mut CfxDibitmap,
    matrix: &BilinearMatrix,
    offset: (i32, i32),
    clip: FxRect,
    source_pitch: usize,
    sample: impl Fn(BilinearData) -> [u8; N],
) {
    let width = dest.width() as usize;
    for row in 0..dest.height() {
        let dest = &mut dest.scanline_mut(row).expect("destination row")[..width * N];
        for (col, pixel) in dest.chunks_exact_mut(N).enumerate() {
            let (mut x, mut y, res_x, res_y) = matrix.transform(
                (col as i32).saturating_add(offset.0),
                row.saturating_add(offset.1),
            );
            x = x.saturating_sub(clip.left);
            y = y.saturating_sub(clip.top);
            if x < 0 || x > clip.width() || y < 0 || y > clip.height() {
                continue;
            }
            adjust_coords(clip, &mut x, &mut y);
            let (mut right, mut bottom) = (x + 1, y + 1);
            adjust_coords(clip, &mut right, &mut bottom);
            pixel.copy_from_slice(&sample(BilinearData {
                res_x,
                res_y,
                left: x as usize,
                right: right as usize,
                upper: y as usize * source_pitch,
                lower: bottom as usize * source_pitch,
            }));
        }
    }
}

#[inline]
fn bilinear_color<const N: usize, const ALPHA: bool>(buf: &[u8], data: BilinearData) -> [u8; 4] {
    // Four 16-bit lanes hold the B/G/R/A channels. Weights add to 255,
    // so every weighted sum is <= 65025 and cannot carry into another lane.
    // Mask after each >> 8 to preserve upstream's two truncation steps.
    const LANES: u64 = 0x00ff_00ff_00ff_00ff;
    let load = |start: usize| {
        let pixel = &buf[start..][..N];
        u64::from(pixel[0])
            | (u64::from(pixel[1]) << 16)
            | (u64::from(pixel[2]) << 32)
            | if ALPHA { u64::from(pixel[3]) << 48 } else { 0 }
    };
    let left = data.left * N;
    let right = data.right * N;
    let x = data.res_x as u64;
    let y = data.res_y as u64;
    let upper = ((load(data.upper + left) * (255 - x) + load(data.upper + right) * x) >> 8) & LANES;
    let lower = ((load(data.lower + left) * (255 - x) + load(data.lower + right) * x) >> 8) & LANES;
    let value = ((upper * (255 - y) + lower * y) >> 8) & LANES;
    [
        value as u8,
        (value >> 16) as u8,
        (value >> 32) as u8,
        if ALPHA { (value >> 48) as u8 } else { 255 },
    ]
}

#[inline]
fn bilinear_interpolate(
    buf: &[u8],
    data: BilinearData,
    bytes_per_pixel: usize,
    channel: usize,
) -> u8 {
    let left = data.left * bytes_per_pixel + channel;
    let right = data.right * bytes_per_pixel + channel;
    let upper = (i32::from(buf[data.upper + left]) * (255 - data.res_x)
        + i32::from(buf[data.upper + right]) * data.res_x)
        >> 8;
    let lower = (i32::from(buf[data.lower + left]) * (255 - data.res_x)
        + i32::from(buf[data.lower + right]) * data.res_x)
        >> 8;
    ((upper * (255 - data.res_y) + lower * data.res_y) >> 8) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_mask_keeps_samples_clamped_at_the_source_extent() {
        let mut source = CfxDibitmap::create(6, 48, FxdibFormat::EightBppMask, 0).unwrap();
        for row in 0..48 {
            source.scanline_mut(row).unwrap()[..6].copy_from_slice(&[255, 0, 0, 0, 0, 255]);
        }
        let mut transformer = CfxImageTransformer::new(
            Rc::new(source),
            CfxMatrix::new(0.48, -0.48, 3.84, 0., 0.48, -0.48),
            Default::default(),
            None,
        );
        while transformer.continue_transform(None) {}
        assert_eq!(transformer.result(), FxRect::new(0, -1, 5, 0));
        let bitmap = transformer
            .detach_bitmap()
            .expect("edge-clamped samples remain visible");
        assert_eq!(
            &bitmap.borrow().scanline(0).unwrap()[..5],
            &[0, 83, 83, 83, 83]
        );
    }

    #[test]
    fn affine_region_pixels_equal_the_whole_transform_crop() {
        let mut source = CfxDibitmap::create(16, 16, FxdibFormat::Bgr, 0).unwrap();
        for y in 0..16 {
            for (x, byte) in source.scanline_mut(y).unwrap()[..48].iter_mut().enumerate() {
                *byte = ((x * 53 + y as usize * 79) % 256) as u8;
            }
        }
        let source = Rc::new(source);
        for matrix in [
            CfxMatrix::new(32., 0., 32., -32., 0., 32.),
            CfxMatrix::new(40., 10., 5., -40., 0., 50.),
        ] {
            let render = |matrix, clip| {
                let mut transformer =
                    CfxImageTransformer::new(Rc::clone(&source), matrix, Default::default(), clip);
                while transformer.continue_transform(None) {}
                let rect = transformer.result();
                (rect, transformer.detach_bitmap().unwrap())
            };
            let (whole_rect, whole) = render(matrix, None);
            let whole = whole.borrow();
            let (rect, part) = render(matrix, Some(FxRect::new(24, 8, 40, 24)));
            let mut translated = matrix;
            translated.e -= 24.;
            translated.f -= 8.;
            let (_, local) = render(translated, Some(FxRect::new(0, 0, 16, 16)));
            assert_eq!(part.borrow().buffer(), local.borrow().buffer());
            let part = part.borrow();
            for y in 0..rect.height() {
                let start = (rect.left - whole_rect.left) as usize * 4;
                assert_eq!(
                    &whole.scanline(rect.top - whole_rect.top + y).unwrap()
                        [start..start + rect.width() as usize * 4],
                    &part.scanline(y).unwrap()[..rect.width() as usize * 4],
                    "matrix={matrix:?},row={y}"
                );
            }
        }
    }

    #[test]
    fn packed_color_matches_scalar_at_every_subpixel_weight_and_clamped_edge() {
        fn check<const N: usize, const ALPHA: bool>() {
            // Tight rows include a final three-byte pixel with no readable
            // fourth byte. Distinct channel patterns catch cross-lane carries.
            for values in [
                [0u8; 16],
                [255; 16],
                [
                    0, 255, 1, 254, 127, 128, 17, 201, 255, 0, 254, 1, 128, 127, 201, 17,
                ],
            ] {
                let buf = &values[..4 * N];
                for (left, right, upper, lower) in [(0, 1, 0, 2 * N), (1, 1, 2 * N, 2 * N)] {
                    for res_y in 0..256 {
                        for res_x in 0..256 {
                            let data = BilinearData {
                                res_x,
                                res_y,
                                left,
                                right,
                                upper,
                                lower,
                            };
                            let mut expected = [255; 4];
                            for (channel, value) in
                                expected
                                    .iter_mut()
                                    .enumerate()
                                    .take(if ALPHA { 4 } else { 3 })
                            {
                                *value = bilinear_interpolate(buf, data, N, channel);
                            }
                            assert_eq!(
                                bilinear_color::<N, ALPHA>(buf, data),
                                expected,
                                "stride={N} alpha={ALPHA} phase={res_x},{res_y}"
                            );
                        }
                    }
                }
            }
        }
        check::<3, false>();
        check::<4, false>();
        check::<4, true>();
    }

    #[test]
    fn normal_rotation_and_clipping_preserve_scanline_orientation() {
        let mut source = CfxDibitmap::create(2, 2, FxdibFormat::EightBppMask, 0).unwrap();
        source.scanline_mut(0).unwrap()[..2].copy_from_slice(&[11, 22]);
        source.scanline_mut(1).unwrap()[..2].copy_from_slice(&[33, 44]);
        let source = Rc::new(source);
        let mut normal = CfxImageTransformer::new(
            Rc::clone(&source),
            CfxMatrix::new(2.0, 0.0, 0.0, -2.0, 3.0, 6.0),
            FxdibResampleOptions::default(),
            Some(FxRect::new(4, 4, 5, 6)),
        );
        assert!(!normal.continue_transform(None));
        assert_eq!(normal.result(), FxRect::new(4, 4, 5, 6));
        let bitmap = normal.detach_bitmap().unwrap();
        assert_eq!(bitmap.borrow().scanline(0).unwrap()[0], 22);
        assert_eq!(bitmap.borrow().scanline(1).unwrap()[0], 44);
        let mut rotated = CfxImageTransformer::new(
            source,
            CfxMatrix::new(0.0, 2.0, -2.0, 0.0, 5.0, 3.0),
            FxdibResampleOptions::default(),
            None,
        );
        assert!(!rotated.continue_transform(None));
        assert_eq!(rotated.result(), FxRect::new(3, 3, 5, 5));
        let bitmap = rotated.detach_bitmap().unwrap();
        assert_eq!(&bitmap.borrow().scanline(0).unwrap()[..2], &[11, 33]);
        assert_eq!(&bitmap.borrow().scanline(1).unwrap()[..2], &[22, 44]);
    }

    #[test]
    fn affine_interpolation_keeps_255_based_two_stage_rounding() {
        for (format, expected) in [
            (FxdibFormat::EightBppMask, &[253u8][..]),
            (FxdibFormat::EightBppRgb, &[253, 253, 253, 255][..]),
            (FxdibFormat::Bgr, &[253, 253, 253, 255][..]),
            (FxdibFormat::Bgra, &[253, 253, 253, 253][..]),
        ] {
            let mut source = CfxDibitmap::create(2, 2, format, 0).unwrap();
            source.buffer_mut().fill(255);
            let mut transformer = CfxImageTransformer::new(
                Rc::new(source),
                CfxMatrix::new(4.0, 1.0, 1.0, -4.0, 1.0, 5.0),
                FxdibResampleOptions::default(),
                None,
            );
            assert!(!transformer.continue_transform(None));
            assert_eq!(transformer.result(), FxRect::new(1, 1, 6, 6));
            let bitmap = transformer.detach_bitmap().unwrap();
            let bitmap = bitmap.borrow();
            let pixel = &bitmap.scanline(2).unwrap()[2 * expected.len()..3 * expected.len()];
            assert_eq!(pixel, expected, "{format:?}");
        }
    }

    #[test]
    fn one_bit_axis_swap_keeps_unused_bits_set() {
        let mut source = CfxDibitmap::create(2, 3, FxdibFormat::OneBppMask, 0).unwrap();
        for (row, value) in [0x80, 0x40, 0xc0].into_iter().enumerate() {
            source.scanline_mut(row as i32).unwrap()[0] = value;
        }
        let swapped = source.swap_xy(false, false).unwrap();
        assert_eq!((swapped.width(), swapped.height()), (3, 2));
        assert_eq!(swapped.scanline(0).unwrap(), &[0xbf, 0xff, 0xff, 0xff]);
        assert_eq!(swapped.scanline(1).unwrap(), &[0x7f, 0xff, 0xff, 0xff]);
        let flipped = source.swap_xy(true, true).unwrap();
        assert_eq!(flipped.scanline(0).unwrap()[0], 0xdf);
        assert_eq!(flipped.scanline(1).unwrap()[0], 0xbf);
    }
}
