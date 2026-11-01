// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Shared bitmap realization, conversion and transforms from
//! `core/fxge/dib/cfx_dibbase.{h,cpp}`. Indexed palette mutation lives on
//! CfxDibitmap; borrowed source views also support lazy scanline producers.

use super::cfx_bitmapstorer::CfxBitmapStorer;
use super::cfx_dibitmap::CfxDibitmap;
use super::cfx_imagestretcher::CfxImageStretcher;
use super::cfx_imagetransformer::CfxImageTransformer;
use super::cstretchengine::StretchSource;
use super::fx_dib::{
    argb_b, argb_g, argb_r, get_bpp_from_format, get_is_mask_from_format, rgb_to_gray, FxdibFormat,
    FxdibResampleOptions,
};
use crate::fxcrt::fx_coordinates::{CfxMatrix, FxRect};
use std::cell::RefCell;
use std::rc::Rc;

pub type DibitmapRef = Rc<RefCell<CfxDibitmap>>;

pub fn required_palette_size(format: FxdibFormat) -> usize {
    if get_is_mask_from_format(format) {
        return 0;
    }
    match get_bpp_from_format(format) {
        1 => 2,
        8 => 256,
        _ => 0,
    }
}

/// CFX_DIBBase accounts for the required palette even when no palette was
/// allocated. Concrete DIBitmap adds its owned pixel allocation separately.
pub fn estimated_image_memory_burden(format: FxdibFormat) -> usize {
    required_palette_size(format) * std::mem::size_of::<u32>()
}

pub fn realize<S: StretchSource>(source: &S) -> Option<CfxDibitmap> {
    clip_to(source, FxRect::new(0, 0, source.width(), source.height()))
}

pub fn clip_to<S: StretchSource>(source: &S, mut rect: FxRect) -> Option<CfxDibitmap> {
    rect.intersect(FxRect::new(0, 0, source.width(), source.height()));
    if rect.is_empty() {
        return None;
    }
    let mut result = CfxDibitmap::create(rect.width(), rect.height(), source.format(), 0)?;
    result.take_palette(source.palette().to_vec());
    let bpp = get_bpp_from_format(source.format()) as usize;
    for row in rect.top..rect.bottom {
        let src = source.scanline(row);
        let dest = result.scanline_mut(row - rect.top)?;
        if bpp == 1 && rect.left % 8 != 0 {
            // Use bounded MSB-first bit accesses instead of the pinned source's
            // unaligned dword reads. Unused destination bits stay zero.
            for col in 0..rect.width() as usize {
                let src_col = rect.left as usize + col;
                if src[src_col / 8] & (0x80 >> (src_col % 8)) != 0 {
                    dest[col / 8] |= 0x80 >> (col % 8);
                }
            }
        } else {
            let count = (rect.width() as usize * bpp).div_ceil(8).min(src.len());
            let offset = rect.left as usize * bpp / 8;
            dest[..count].copy_from_slice(src.get(offset..offset.checked_add(count)?)?);
        }
    }
    Some(result)
}

pub fn clone_alpha_mask<S: StretchSource>(source: &S) -> Option<CfxDibitmap> {
    assert_eq!(source.format(), FxdibFormat::Bgra);
    let mut result = CfxDibitmap::create(
        source.width(),
        source.height(),
        FxdibFormat::EightBppMask,
        0,
    )?;
    for row in 0..source.height() {
        let src = source.scanline(row);
        let dest = result.scanline_mut(row)?;
        for (pixel, alpha) in src[..source.width() as usize * 4].chunks_exact(4).zip(dest) {
            *alpha = pixel[3];
        }
    }
    Some(result)
}

pub fn convert_to<S: StretchSource>(source: &S, format: FxdibFormat) -> Option<CfxDibitmap> {
    assert!(matches!(
        format,
        FxdibFormat::Bgr | FxdibFormat::EightBppRgb
    ));
    assert_ne!(source.format(), format);
    let mut result = CfxDibitmap::create(source.width(), source.height(), format, 0)?;
    let pitch = result.pitch() as usize;
    let palette = convert_buffer(
        format,
        result.buffer_mut(),
        pitch,
        source.width(),
        source.height(),
        source,
        0,
        0,
    )?;
    if !palette.is_empty() {
        result.take_palette(palette);
    }
    Some(result)
}

fn convert_rows<S: StretchSource>(
    dest: &mut [u8],
    pitch: usize,
    width: usize,
    height: usize,
    source: &S,
    left: usize,
    top: i32,
    mut convert: impl FnMut(&mut [u8], &[u8], usize, usize),
) -> Option<()> {
    for row in 0..height {
        let src = source.scanline(top.checked_add(i32::try_from(row).ok()?)?);
        let offset = row.checked_mul(pitch)?;
        let tail = dest.get_mut(offset..)?;
        let count = tail.len().min(pitch);
        convert(&mut tail[..count], &src, left, width);
    }
    Some(())
}

/// The non-Skia ConvertBuffer helpers. Destination alpha and padding are
/// preserved; ConvertFormat initializes opaque alpha before calling this.
pub fn convert_buffer<S: StretchSource>(
    format: FxdibFormat,
    dest: &mut [u8],
    pitch: usize,
    width: i32,
    height: i32,
    source: &S,
    left: i32,
    top: i32,
) -> Option<Vec<u32>> {
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    let left = usize::try_from(left).ok()?;
    if top < 0
        || left.checked_add(width)? > source.width() as usize
        || usize::try_from(top).ok()?.checked_add(height)? > source.height() as usize
    {
        return None;
    }
    let src_bpp = get_bpp_from_format(source.format());
    let palette = source.palette();
    let rows = |dest: &mut [u8], convert: &mut dyn FnMut(&mut [u8], &[u8], usize, usize)| {
        convert_rows(dest, pitch, width, height, source, left, top, convert)
    };
    if format == FxdibFormat::EightBppRgb {
        assert!(src_bpp == 1 || src_bpp == 8);
        if !palette.is_empty() {
            if src_bpp == 1 {
                rows(dest, &mut |dst, src, left, width| {
                    dst[..width].fill(255);
                    for (col, out) in dst[..width].iter_mut().enumerate() {
                        let bit = left + col;
                        if src[bit / 8] & (0x80 >> (bit % 8)) != 0 {
                            *out = 0;
                        }
                    }
                })?;
            } else {
                rows(dest, &mut |dst, src, left, width| {
                    dst[..width].copy_from_slice(&src[left..left + width])
                })?;
            }
            return Some(palette[..required_palette_size(source.format())].to_vec());
        }
    }
    match format {
        FxdibFormat::EightBppMask | FxdibFormat::EightBppRgb => match src_bpp {
            1 => {
                assert!(palette.is_empty());
                rows(dest, &mut |dst, src, left, width| {
                    dst[..width].fill(0);
                    for (col, out) in dst[..width].iter_mut().enumerate() {
                        let bit = left + col;
                        if src[bit / 8] & (0x80 >> (bit % 8)) != 0 {
                            *out = 255;
                        }
                    }
                })?;
            }
            8 if palette.is_empty() => {
                rows(dest, &mut |dst, src, left, width| {
                    dst[..width].copy_from_slice(&src[left..left + width])
                })?;
            }
            8 => {
                assert_eq!(palette.len(), 256);
                let mut gray = [0_u8; 256];
                for (&color, out) in palette.iter().zip(&mut gray) {
                    *out = rgb_to_gray(
                        i32::from(argb_r(color)),
                        i32::from(argb_g(color)),
                        i32::from(argb_b(color)),
                    ) as u8;
                }
                rows(dest, &mut |dst, src, left, width| {
                    for (&input, output) in src[left..left + width].iter().zip(dst) {
                        *output = gray[input as usize];
                    }
                })?;
            }
            24 | 32 => {
                let bytes = src_bpp as usize / 8;
                rows(dest, &mut |dst, src, left, width| {
                    for (input, output) in src[left * bytes..(left + width) * bytes]
                        .chunks_exact(bytes)
                        .zip(dst)
                    {
                        *output = rgb_to_gray(
                            i32::from(input[2]),
                            i32::from(input[1]),
                            i32::from(input[0]),
                        ) as u8;
                    }
                })?;
            }
            _ => panic!("invalid source bitmap format"),
        },
        FxdibFormat::Bgr | FxdibFormat::Bgrx | FxdibFormat::Bgra => {
            let bytes = get_bpp_from_format(format) as usize / 8;
            match src_bpp {
                1 => {
                    assert_eq!(format, FxdibFormat::Bgr);
                    let mut colors = [[0_u8; 3], [255_u8; 3]];
                    if !palette.is_empty() {
                        for (&color, out) in palette.iter().zip(&mut colors) {
                            out.copy_from_slice(&color.to_le_bytes()[..3]);
                        }
                    }
                    rows(dest, &mut |dst, src, left, width| {
                        for (col, out) in dst[..width * 3].chunks_exact_mut(3).enumerate() {
                            let bit = left + col;
                            let index = usize::from(src[bit / 8] & (0x80 >> (bit % 8)) != 0);
                            out.copy_from_slice(&colors[index]);
                        }
                    })?;
                }
                8 if palette.is_empty() => {
                    rows(dest, &mut |dst, src, left, width| {
                        for (&input, output) in src[left..left + width]
                            .iter()
                            .zip(dst.chunks_exact_mut(bytes))
                        {
                            output[..3].fill(input);
                        }
                    })?;
                }
                8 => {
                    assert_eq!(palette.len(), 256);
                    let mut colors = [[0_u8; 3]; 256];
                    for (&color, out) in palette.iter().zip(&mut colors) {
                        out.copy_from_slice(&color.to_le_bytes()[..3]);
                    }
                    rows(dest, &mut |dst, src, left, width| {
                        for (&input, output) in src[left..left + width]
                            .iter()
                            .zip(dst.chunks_exact_mut(bytes))
                        {
                            output[..3].copy_from_slice(&colors[input as usize]);
                        }
                    })?;
                }
                24 if bytes == 3 => {
                    rows(dest, &mut |dst, src, left, width| {
                        dst[..width * 3].copy_from_slice(&src[left * 3..(left + width) * 3])
                    })?;
                }
                24 | 32 => {
                    let src_bytes = src_bpp as usize / 8;
                    rows(dest, &mut |dst, src, left, width| {
                        for (input, output) in src[left * src_bytes..(left + width) * src_bytes]
                            .chunks_exact(src_bytes)
                            .zip(dst.chunks_exact_mut(bytes))
                        {
                            output[..3].copy_from_slice(&input[..3]);
                        }
                    })?;
                }
                _ => panic!("invalid source bitmap format"),
            }
        }
        _ => panic!("invalid destination bitmap format"),
    }
    Some(Vec::new())
}

/// Mutates output arguments in the source order, including on late failure.
pub fn get_overlap_rect(
    dest_width: i32,
    dest_height: i32,
    dest_left: &mut i32,
    dest_top: &mut i32,
    width: &mut i32,
    height: &mut i32,
    src_width: i32,
    src_height: i32,
    src_left: &mut i32,
    src_top: &mut i32,
    clip: Option<FxRect>,
) -> bool {
    let mut apply = || -> Option<()> {
        if *width == 0 || *height == 0 || *dest_left > dest_width || *dest_top > dest_height {
            return None;
        }
        debug_assert!(*width > 0 && *height > 0);
        let mut src = FxRect::new(
            *src_left,
            *src_top,
            src_left.checked_add(*width)?,
            src_top.checked_add(*height)?,
        );
        src.intersect(FxRect::new(0, 0, src_width, src_height));
        let x_offset = dest_left.checked_sub(*src_left)?;
        let y_offset = dest_top.checked_sub(*src_top)?;
        let mut dest = FxRect::new(
            x_offset.checked_add(src.left)?,
            y_offset.checked_add(src.top)?,
            x_offset.checked_add(src.right)?,
            y_offset.checked_add(src.bottom)?,
        );
        dest.intersect(FxRect::new(0, 0, dest_width, dest_height));
        if let Some(clip) = clip {
            dest.intersect(clip);
        }
        *dest_left = dest.left;
        *dest_top = dest.top;
        *src_left = dest_left.checked_sub(x_offset)?;
        *src_top = dest_top.checked_sub(y_offset)?;
        if dest.is_empty() {
            return None;
        }
        *width = dest.width();
        *height = dest.height();
        Some(())
    };
    apply().is_some()
}

pub fn swap_xy<S: StretchSource>(source: &S, flip_x: bool, flip_y: bool) -> Option<CfxDibitmap> {
    let width = source.width();
    let height = source.height();
    let mut result = CfxDibitmap::create(height, width, source.format(), 0)?;
    result.take_palette(source.palette().to_vec());
    let bpp = get_bpp_from_format(source.format());
    let dest_pitch = result.pitch() as usize;
    if bpp == 1 {
        result.buffer_mut().fill(255);
    }
    for row in 0..height {
        let src = source.scanline(row);
        let dest_col = if flip_x { height - row - 1 } else { row } as usize;
        for col in 0..width {
            let dest_row = if flip_y { width - col - 1 } else { col } as usize;
            let offset = dest_row * dest_pitch;
            if bpp == 1 {
                if src[col as usize / 8] & (1 << (7 - col % 8)) == 0 {
                    result.buffer_mut()[offset + dest_col / 8] &= !(1 << (7 - dest_col % 8));
                }
            } else {
                let bytes = bpp as usize / 8;
                let src_offset = col as usize * bytes;
                let dest_offset = offset + dest_col * bytes;
                result.buffer_mut()[dest_offset..dest_offset + bytes]
                    .copy_from_slice(&src[src_offset..src_offset + bytes]);
            }
        }
    }
    Some(result)
}

pub fn stretch_to<S: StretchSource>(
    source: Rc<S>,
    width: i32,
    height: i32,
    options: FxdibResampleOptions,
    clip: Option<FxRect>,
) -> Option<DibitmapRef> {
    let mut rect = FxRect::new(0, 0, width.checked_abs()?, height.checked_abs()?);
    if let Some(clip) = clip {
        rect.intersect(clip);
    }
    if rect.is_empty() {
        return None;
    }
    if width == source.width() && height == source.height() {
        return clip_to(source.as_ref(), rect).map(|bitmap| Rc::new(RefCell::new(bitmap)));
    }
    let mut storer = CfxBitmapStorer::new();
    let mut stretcher = CfxImageStretcher::new(source, width, height, rect, options);
    if stretcher.start(&mut storer) {
        stretcher.continue_stretch(&mut storer, None);
    }
    storer.detach()
}

/// Coordinates are returned even when bitmap creation fails, as with the
/// original two output parameters.
pub fn transform_to<S: StretchSource>(
    source: Rc<S>,
    matrix: CfxMatrix,
) -> (Option<DibitmapRef>, i32, i32) {
    let mut transformer =
        CfxImageTransformer::new(source, matrix, FxdibResampleOptions::default(), None);
    transformer.continue_transform(None);
    let rect = transformer.result();
    (transformer.detach_bitmap(), rect.left, rect.top)
}

#[cfg(test)]
mod tests {
    use super::super::fx_dib::FxdibFormat;
    use super::*;

    #[test]
    fn unchanged_stretch_copies_alpha_without_resampling() {
        let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        bitmap.buffer_mut()[..4].copy_from_slice(&[150, 100, 50, 128]);
        let source = Rc::new(RefCell::new(bitmap));
        let copied = stretch_to(
            Rc::clone(&source),
            1,
            1,
            FxdibResampleOptions::default(),
            None,
        )
        .unwrap();
        assert!(!Rc::ptr_eq(&source, &copied));
        assert_eq!(&copied.borrow().buffer()[..4], &[150, 100, 50, 128]);
        let (transformed, left, top) =
            transform_to(source, CfxMatrix::new(1.0, 0.0, 0.0, -1.0, 3.0, 4.0));
        assert_eq!((left, top), (3, 3));
        assert_eq!(
            &transformed.unwrap().borrow().buffer()[..4],
            &[150, 100, 50, 127]
        );
    }
    #[test]
    fn indexed_conversion_preserves_pinned_bit_inversion_and_palette_shape() {
        let mut source = CfxDibitmap::create(2, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        source.buffer_mut()[0] = 0x80;
        assert_eq!(source.required_palette_size(), 2);
        assert_eq!(source.estimated_image_memory_burden(), 12);
        assert_eq!(source.palette_argb(1), 0xffffffff);
        source.set_palette_argb(1, 0xffff0000);
        let converted = source.convert_to(FxdibFormat::EightBppRgb).unwrap();
        assert_eq!(&converted.buffer()[..2], &[0, 255]);
        assert_eq!(converted.palette().len(), 256);
        assert_eq!(converted.palette()[1], 0xffff0000);
        assert_eq!(converted.palette()[255], 0);
        let realized = source.realize().unwrap();
        assert_eq!(realized.palette(), source.palette());
        assert_eq!(realized.buffer()[0], 0x80);
        let rgb = source.convert_to(FxdibFormat::Bgr).unwrap();
        assert_eq!(&rgb.buffer()[..6], &[0, 0, 255, 0, 0, 0]);
    }

    #[test]
    fn shared_conversion_retains_destination_alpha_and_padding_at_offsets() {
        let mut source = CfxDibitmap::create(3, 2, FxdibFormat::Bgra, 0).unwrap();
        source.buffer_mut().copy_from_slice(&[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        ]);
        let mask = source.clone_alpha_mask().unwrap();
        assert_eq!(&mask.scanline(0).unwrap()[..3], &[4, 8, 12]);
        assert_eq!(&mask.scanline(1).unwrap()[..3], &[16, 20, 24]);
        let mut dest = [99_u8; 12];
        convert_buffer(FxdibFormat::Bgrx, &mut dest, 8, 1, 2, &source, 1, 0).unwrap();
        assert_eq!(dest, [5, 6, 7, 99, 99, 99, 99, 99, 17, 18, 19, 99]);
        assert!(source.convert_to_bgr());
        assert_eq!(
            &source.scanline(1).unwrap()[..9],
            &[13, 14, 15, 17, 18, 19, 21, 22, 23]
        );
        assert!(source.palette().is_empty());
    }

    #[test]
    fn overlap_checked_offsets_preserve_late_failure_mutations() {
        let (mut left, mut top, mut width, mut height, mut src_left, mut src_top) =
            (1, 2, 8, 7, -2, -1);
        assert!(get_overlap_rect(
            10,
            10,
            &mut left,
            &mut top,
            &mut width,
            &mut height,
            8,
            8,
            &mut src_left,
            &mut src_top,
            Some(FxRect::new(4, 4, 8, 9))
        ));
        assert_eq!(
            (left, top, width, height, src_left, src_top),
            (4, 4, 4, 5, 1, 1)
        );
        let (mut left, mut top, mut width, mut height, mut src_left, mut src_top) =
            (5, 6, 2, 2, 9, 9);
        assert!(!get_overlap_rect(
            10,
            10,
            &mut left,
            &mut top,
            &mut width,
            &mut height,
            4,
            4,
            &mut src_left,
            &mut src_top,
            None
        ));
        assert_eq!(
            (left, top, width, height, src_left, src_top),
            (0, 0, 2, 2, 4, 3)
        );
        src_left = i32::MAX;
        assert!(!get_overlap_rect(
            10,
            10,
            &mut left,
            &mut top,
            &mut width,
            &mut height,
            4,
            4,
            &mut src_left,
            &mut src_top,
            None
        ));
        assert_eq!(src_left, i32::MAX);
    }
}

#[cfg(test)]
#[path = "cfx_dibbase_unittest.rs"]
mod upstream_tests;
