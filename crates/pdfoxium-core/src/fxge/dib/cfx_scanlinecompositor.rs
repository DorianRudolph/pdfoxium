// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! AGG bitmap and mask scanline composition from
//! `core/fxge/dib/cfx_scanlinecompositor.{h,cpp}`.

use super::fx_dib::{
    alpha_merge, argb_a, argb_b, argb_g, argb_r, get_bpp_from_format, rgb_to_gray, BlendMode,
    FxdibFormat,
};

#[cfg(feature = "simd")]
#[path = "bitmap_simd.rs"]
mod bitmap_simd;

fn is_nonseparable(mode: BlendMode) -> bool {
    matches!(
        mode,
        BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity
    )
}

fn lum(color: [i32; 3]) -> i32 {
    (color[0] * 30 + color[1] * 59 + color[2] * 11) / 100
}

fn clip_color(mut color: [i32; 3]) -> [i32; 3] {
    let l = lum(color);
    let n = color[0].min(color[1].min(color[2]));
    let x = color[0].max(color[1].max(color[2]));
    if n < 0 {
        for value in &mut color {
            *value = l + (*value - l) * l / (l - n);
        }
    }
    if x > 255 {
        for value in &mut color {
            *value = l + (*value - l) * (255 - l) / (x - l);
        }
    }
    color
}

fn set_lum(mut color: [i32; 3], l: i32) -> [i32; 3] {
    let d = l - lum(color);
    for value in &mut color {
        *value += d;
    }
    clip_color(color)
}

fn sat(color: [i32; 3]) -> i32 {
    color[0].max(color[1].max(color[2])) - color[0].min(color[1].min(color[2]))
}

fn set_sat(mut color: [i32; 3], s: i32) -> [i32; 3] {
    let min = color[0].min(color[1].min(color[2]));
    let max = color[0].max(color[1].max(color[2]));
    if min == max {
        return [0; 3];
    }
    for value in &mut color {
        *value = (*value - min) * s / (max - min);
    }
    color
}

/// RgbBlend()/RGB_Blend(), with source integer arithmetic and operation order.
/// Inputs and output are RGB triples; channel blend results remain i32 until
/// the destination store, matching FX_RGB_STRUCT<int>.
pub fn rgb_blend(mode: BlendMode, source: [i32; 3], backdrop: [i32; 3]) -> [i32; 3] {
    match mode {
        BlendMode::Hue => set_lum(set_sat(source, sat(backdrop)), lum(backdrop)),
        BlendMode::Saturation => set_lum(set_sat(backdrop, sat(source)), lum(backdrop)),
        BlendMode::Color => set_lum(source, lum(backdrop)),
        BlendMode::Luminosity => set_lum(backdrop, lum(source)),
        _ => std::array::from_fn(|i| super::blend::blend(mode, backdrop[i], source[i])),
    }
}

fn alpha_union(back: i32, source: i32) -> i32 {
    back + source - back * source / 255
}

fn blend_gray(mode: BlendMode, source: i32, back: i32) -> i32 {
    if is_nonseparable(mode) {
        if mode == BlendMode::Luminosity {
            source
        } else {
            back
        }
    } else {
        super::blend::blend(mode, back, source)
    }
}

#[derive(Default)]
enum Palette {
    #[default]
    None,
    Gray(Vec<u8>),
    Argb(Vec<u32>),
}

impl Palette {
    fn gray(&self) -> &[u8] {
        match self {
            Self::None => &[],
            Self::Gray(palette) => palette,
            _ => panic!("palette width mismatch"),
        }
    }
    fn argb(&self) -> &[u32] {
        match self {
            Self::None => &[],
            Self::Argb(palette) => palette,
            _ => panic!("palette width mismatch"),
        }
    }
}

pub struct CfxScanlineCompositor {
    source_format: FxdibFormat,
    dest_format: FxdibFormat,
    palette: Palette,
    mask_color: u32,
    blend_type: BlendMode,
    rgb_byte_order: bool,
}

impl CfxScanlineCompositor {
    pub fn new(
        dest_format: FxdibFormat,
        source_format: FxdibFormat,
        source_palette: &[u32],
        mask_color: u32,
        blend_type: BlendMode,
        rgb_byte_order: bool,
    ) -> Option<Self> {
        if matches!(
            dest_format,
            FxdibFormat::Invalid | FxdibFormat::OneBppMask | FxdibFormat::OneBppRgb
        ) || (rgb_byte_order
            && matches!(
                dest_format,
                FxdibFormat::EightBppMask | FxdibFormat::EightBppRgb
            ))
        {
            return None;
        }
        let mut palette = Palette::None;
        if matches!(
            source_format,
            FxdibFormat::OneBppRgb | FxdibFormat::EightBppRgb
        ) && dest_format != FxdibFormat::EightBppMask
        {
            let count = 1usize << get_bpp_from_format(source_format);
            if dest_format == FxdibFormat::EightBppRgb {
                let colors = (0..count)
                    .map(|i| {
                        if !source_palette.is_empty() {
                            let color = source_palette[i];
                            rgb_to_gray(
                                i32::from(argb_r(color)),
                                i32::from(argb_g(color)),
                                i32::from(argb_b(color)),
                            ) as u8
                        } else if count == 2 {
                            (i * 255) as u8
                        } else {
                            i as u8
                        }
                    })
                    .collect();
                palette = Palette::Gray(colors);
            } else {
                let colors = (0..count)
                    .map(|i| {
                        if !source_palette.is_empty() {
                            source_palette[i]
                        } else if count == 2 {
                            if i == 0 {
                                0xff000000
                            } else {
                                0xffffffff
                            }
                        } else {
                            let v = i as u32;
                            v << 16 | v << 8 | v
                        }
                    })
                    .collect();
                palette = Palette::Argb(colors);
            }
        }
        Some(Self {
            source_format,
            dest_format,
            palette,
            mask_color,
            blend_type,
            rgb_byte_order,
        })
    }

    pub fn source_format(&self) -> FxdibFormat {
        self.source_format
    }

    pub(crate) fn source_palette_argb(&self) -> &[u32] {
        self.palette.argb()
    }

    /// Non-normal sparse BGRA surfaces commonly enclose thin paths. Skip
    /// transparent blocks before entering the scalar blend loop. A transparent
    /// BGRA backdrop still needs the native hidden-RGB copy, so only skip it
    /// when all backdrop alphas in the block are opaque.
    pub fn composite_rgb_bitmap_line(
        &self,
        dest: &mut [u8],
        source: &[u8],
        width: usize,
        clip: Option<&[u8]>,
    ) {
        const BLOCK: usize = 32;
        if width < 2 * BLOCK
            || self.blend_type == BlendMode::Normal
            || self.source_format != FxdibFormat::Bgra
            || !matches!(
                self.dest_format,
                FxdibFormat::Bgr | FxdibFormat::Bgrx | FxdibFormat::Bgra
            )
        {
            self.composite_rgb_bitmap_line_dense(dest, source, width, clip);
            return;
        }
        let stride = get_bpp_from_format(self.dest_format) as usize / 8;
        let source = &source[..width * 4];
        let dest = &mut dest[..width * stride];
        let clip = clip.filter(|c| !c.is_empty()).map(|c| &c[..width]);
        let mut start = 0;
        for col in (0..width / BLOCK * BLOCK).step_by(BLOCK) {
            let input = &source[col * 4..(col + BLOCK) * 4];
            if input[3] != 0
                || !alpha_block_matches(input, 0)
                || (self.dest_format == FxdibFormat::Bgra
                    && !alpha_block_matches(&dest[col * 4..(col + BLOCK) * 4], ALPHA_MASK))
            {
                continue;
            }
            if start < col {
                self.composite_rgb_bitmap_line_dense(
                    &mut dest[start * stride..col * stride],
                    &source[start * 4..col * 4],
                    col - start,
                    clip.map(|c| &c[start..col]),
                );
            }
            start = col + BLOCK;
        }
        if start < width {
            self.composite_rgb_bitmap_line_dense(
                &mut dest[start * stride..],
                &source[start * 4..],
                width - start,
                clip.map(|c| &c[start..]),
            );
        }
    }

    fn composite_rgb_bitmap_line_dense(
        &self,
        dest: &mut [u8],
        source: &[u8],
        width: usize,
        clip: Option<&[u8]>,
    ) {
        let clip = clip.filter(|clip| !clip.is_empty());
        assert!(matches!(
            self.source_format,
            FxdibFormat::Bgr | FxdibFormat::Bgrx | FxdibFormat::Bgra
        ));
        if self.blend_type == BlendMode::Normal {
            composite_rgb_bitmap_line_normal(
                dest,
                source,
                self.dest_format,
                self.source_format,
                width,
                clip,
                self.rgb_byte_order,
            );
            return;
        }
        // CompositeRowBgra2Bgra: select layout and clipping once per row,
        // and inspect alpha before loading/blending the color channels.
        if self.source_format == FxdibFormat::Bgra && self.dest_format == FxdibFormat::Bgra {
            match (self.rgb_byte_order, clip) {
                (false, None) => composite_bgra_blend_row::<false, false>(
                    dest,
                    source,
                    width,
                    &[],
                    self.blend_type,
                ),
                (true, None) => composite_bgra_blend_row::<true, false>(
                    dest,
                    source,
                    width,
                    &[],
                    self.blend_type,
                ),
                (false, Some(clip)) => composite_bgra_blend_row::<false, true>(
                    dest,
                    source,
                    width,
                    clip,
                    self.blend_type,
                ),
                (true, Some(clip)) => composite_bgra_blend_row::<true, true>(
                    dest,
                    source,
                    width,
                    clip,
                    self.blend_type,
                ),
            }
            return;
        }
        let src_bytes = get_bpp_from_format(self.source_format) as usize / 8;
        let has_alpha = self.source_format == FxdibFormat::Bgra;
        if self.dest_format == FxdibFormat::EightBppMask {
            for col in 0..width {
                let cover = i32::from(clip.map_or(255, |clip| clip[col]));
                let alpha = if has_alpha {
                    i32::from(source[col * src_bytes + 3]) * cover / 255
                } else {
                    cover
                };
                dest[col] = alpha_union(i32::from(dest[col]), alpha) as u8;
            }
            return;
        }
        if self.dest_format == FxdibFormat::EightBppRgb {
            for col in 0..width {
                let input = &source[col * src_bytes..(col + 1) * src_bytes];
                let cover = i32::from(clip.map_or(255, |clip| clip[col]));
                let alpha = if has_alpha {
                    i32::from(input[3]) * cover / 255
                } else {
                    cover
                };
                if alpha == 0 {
                    continue;
                }
                let gray = rgb_to_gray(
                    i32::from(input[2]),
                    i32::from(input[1]),
                    i32::from(input[0]),
                );
                let gray = blend_gray(self.blend_type, gray, i32::from(dest[col]));
                dest[col] = alpha_merge(i32::from(dest[col]), gray, alpha) as u8;
            }
            return;
        }
        let dest_bytes = get_bpp_from_format(self.dest_format) as usize / 8;
        let dest_alpha = self.dest_format == FxdibFormat::Bgra;
        for col in 0..width {
            let input = &source[col * src_bytes..(col + 1) * src_bytes];
            let output = &mut dest[col * dest_bytes..(col + 1) * dest_bytes];
            let src = [
                i32::from(input[2]),
                i32::from(input[1]),
                i32::from(input[0]),
            ];
            let back = self.read_rgb(output);
            let cover = i32::from(clip.map_or(255, |clip| clip[col]));
            let alpha = if has_alpha {
                i32::from(input[3]) * cover / 255
            } else {
                cover
            };
            if dest_alpha {
                let back_alpha = i32::from(output[3]);
                if back_alpha == 0 {
                    self.store_rgb(output, src);
                    // Bgr2Bgra_Blend_Clip copies RGB and retains zero alpha.
                    // BGRA input and the no-clip opaque branch assign alpha.
                    if has_alpha {
                        output[3] = alpha as u8;
                    } else if clip.is_none() {
                        output[3] = 255;
                    }
                    continue;
                }
                if alpha == 0 {
                    continue;
                }
                let blended = rgb_blend(self.blend_type, src, back);
                let blended = std::array::from_fn(|i| alpha_merge(src[i], blended[i], back_alpha));
                if !has_alpha && clip.is_none() {
                    self.store_rgb(output, blended);
                    output[3] = 255;
                } else {
                    let out_alpha = alpha_union(back_alpha, alpha);
                    let ratio = alpha * 255 / out_alpha;
                    self.store_rgb(
                        output,
                        std::array::from_fn(|i| alpha_merge(back[i], blended[i], ratio)),
                    );
                    output[3] = out_alpha as u8;
                }
            } else if alpha != 0 {
                let blended = rgb_blend(self.blend_type, src, back);
                self.store_rgb(
                    output,
                    std::array::from_fn(|i| alpha_merge(back[i], blended[i], alpha)),
                );
            }
        }
    }

    fn read_rgb(&self, pixel: &[u8]) -> [i32; 3] {
        if self.rgb_byte_order {
            [
                i32::from(pixel[0]),
                i32::from(pixel[1]),
                i32::from(pixel[2]),
            ]
        } else {
            [
                i32::from(pixel[2]),
                i32::from(pixel[1]),
                i32::from(pixel[0]),
            ]
        }
    }
    fn store_rgb(&self, pixel: &mut [u8], rgb: [i32; 3]) {
        if self.rgb_byte_order {
            pixel[..3].copy_from_slice(&rgb.map(|value| value as u8));
        } else {
            pixel[..3].copy_from_slice(&[rgb[2] as u8, rgb[1] as u8, rgb[0] as u8]);
        }
    }

    pub fn composite_pal_bitmap_line(
        &self,
        dest: &mut [u8],
        source: &[u8],
        source_left: usize,
        width: usize,
        clip: Option<&[u8]>,
    ) {
        assert!(matches!(
            self.source_format,
            FxdibFormat::OneBppRgb | FxdibFormat::EightBppRgb
        ));
        let clip = clip.filter(|clip| !clip.is_empty());
        if self.dest_format != FxdibFormat::EightBppRgb {
            // The pinned color-palette paths deliberately use normal blend.
            composite_palette_bitmap_line_normal(
                dest,
                source,
                if self.source_format == FxdibFormat::OneBppRgb {
                    source_left
                } else {
                    0
                },
                width,
                self.dest_format,
                self.source_format,
                self.palette.argb(),
                clip,
                self.rgb_byte_order,
            );
            return;
        }
        let palette = self.palette.gray();
        // CompositeRow_8bppPal2Gray: normal, unclipped rows replace the
        // destination. Dispatch before the pixel loop, as the source does.
        if self.source_format == FxdibFormat::EightBppRgb
            && self.blend_type == BlendMode::Normal
            && clip.is_none()
        {
            let palette: &[u8; 256] = palette.try_into().expect("8-bit gray palette");
            for (output, &index) in dest[..width].iter_mut().zip(&source[..width]) {
                *output = palette[usize::from(index)];
            }
            return;
        }
        for col in 0..width {
            let index = if self.source_format == FxdibFormat::OneBppRgb {
                let bit = source_left + col;
                usize::from(source[bit / 8] & (0x80 >> (bit % 8)) != 0)
            } else {
                source[col] as usize
            };
            let gray = blend_gray(
                self.blend_type,
                i32::from(palette[index]),
                i32::from(dest[col]),
            );
            let alpha = i32::from(clip.map_or(255, |clip| clip[col]));
            dest[col] = alpha_merge(i32::from(dest[col]), gray, alpha) as u8;
        }
    }

    pub fn composite_byte_mask_line(
        &self,
        dest: &mut [u8],
        source: &[u8],
        width: usize,
        clip: Option<&[u8]>,
    ) {
        assert_eq!(self.source_format, FxdibFormat::EightBppMask);
        self.composite_mask_line(dest, source, 0, width, clip, false);
    }

    pub fn composite_bit_mask_line(
        &self,
        dest: &mut [u8],
        source: &[u8],
        source_left: usize,
        width: usize,
        clip: Option<&[u8]>,
    ) {
        assert_eq!(self.source_format, FxdibFormat::OneBppMask);
        self.composite_mask_line(dest, source, source_left, width, clip, true);
    }

    fn composite_mask_line(
        &self,
        dest: &mut [u8],
        source: &[u8],
        source_left: usize,
        width: usize,
        clip: Option<&[u8]>,
        bits: bool,
    ) {
        let clip = clip.filter(|clip| !clip.is_empty());
        if self.blend_type == BlendMode::Normal || get_bpp_from_format(self.dest_format) == 8 {
            if bits {
                composite_bit_mask_line_normal(
                    dest,
                    source,
                    source_left,
                    width,
                    self.mask_color,
                    self.dest_format,
                    self.rgb_byte_order,
                    clip,
                );
            } else {
                composite_byte_mask_line_normal(
                    dest,
                    &source[..width],
                    self.mask_color,
                    self.dest_format,
                    self.rgb_byte_order,
                    clip,
                );
            }
            return;
        }
        let bytes = get_bpp_from_format(self.dest_format) as usize / 8;
        let src = [
            i32::from(argb_r(self.mask_color)),
            i32::from(argb_g(self.mask_color)),
            i32::from(argb_b(self.mask_color)),
        ];
        let mask_alpha = i32::from(argb_a(self.mask_color));
        for col in 0..width {
            let cover = if bits {
                let bit = source_left + col;
                if source[bit / 8] & (0x80 >> (bit % 8)) == 0 {
                    continue;
                }
                255
            } else {
                i32::from(source[col])
            };
            // Keep GetAlphaWithSrc's multiplication/division order.
            let mut alpha = mask_alpha * cover;
            if let Some(cover) = clip.and_then(|clip| clip.get(col)) {
                alpha = alpha * i32::from(*cover) / 255;
            }
            alpha /= 255;
            let output = &mut dest[col * bytes..(col + 1) * bytes];
            let back = self.read_rgb(output);
            if self.dest_format == FxdibFormat::Bgra {
                let back_alpha = i32::from(output[3]);
                if back_alpha == 0 {
                    self.store_rgb(output, src);
                    output[3] = alpha as u8;
                    continue;
                }
                if alpha == 0 {
                    continue;
                }
                let out_alpha = alpha_union(back_alpha, alpha);
                let ratio = alpha * 255 / out_alpha;
                let blended = rgb_blend(self.blend_type, src, back);
                let blended = if is_nonseparable(self.blend_type) {
                    blended
                } else {
                    std::array::from_fn(|i| alpha_merge(src[i], blended[i], back_alpha))
                };
                self.store_rgb(
                    output,
                    std::array::from_fn(|i| alpha_merge(back[i], blended[i], ratio)),
                );
                output[3] = out_alpha as u8;
            } else if alpha != 0 {
                let blended = rgb_blend(self.blend_type, src, back);
                self.store_rgb(
                    output,
                    std::array::from_fn(|i| alpha_merge(back[i], blended[i], alpha)),
                );
            }
        }
    }
}

pub fn composite_byte_mask_line_normal(
    dest: &mut [u8],
    source: &[u8],
    color: u32,
    format: FxdibFormat,
    rgb_byte_order: bool,
    clip: Option<&[u8]>,
) {
    if format == FxdibFormat::Bgra {
        let mut colors = [argb_b(color), argb_g(color), argb_r(color)];
        if rgb_byte_order {
            colors.reverse();
        }
        let alpha = argb_a(color);
        match clip {
            Some(clip) => {
                composite_byte_mask_bgra::<false, true>(dest, source, colors, alpha, clip)
            }
            None if alpha == 255 => {
                composite_byte_mask_bgra::<true, false>(dest, source, colors, alpha, &[])
            }
            None => composite_byte_mask_bgra::<false, false>(dest, source, colors, alpha, &[]),
        }
        return;
    }
    let mask_alpha = i32::from(argb_a(color));
    let source_colors = [argb_b(color), argb_g(color), argb_r(color)];
    let bytes_per_pixel = match format {
        FxdibFormat::EightBppMask | FxdibFormat::EightBppRgb => 1,
        FxdibFormat::Bgr => 3,
        FxdibFormat::Bgrx | FxdibFormat::Bgra => 4,
        _ => return,
    };
    for (index, &source_cover) in source.iter().enumerate() {
        let clip_cover = clip
            .and_then(|clip| clip.get(index))
            .copied()
            .unwrap_or(255);
        let mut source_alpha = mask_alpha * i32::from(source_cover);
        if clip.is_some() {
            source_alpha = source_alpha * i32::from(clip_cover) / 255;
        }
        source_alpha /= 255;
        let pixel = &mut dest[index * bytes_per_pixel..(index + 1) * bytes_per_pixel];
        match format {
            FxdibFormat::EightBppMask => {
                let back_alpha = pixel[0];
                if back_alpha == 0 {
                    pixel[0] = source_alpha as u8;
                } else if source_alpha != 0 {
                    pixel[0] = (i32::from(back_alpha) + source_alpha
                        - i32::from(back_alpha) * source_alpha / 255)
                        as u8;
                }
            }
            FxdibFormat::EightBppRgb => {
                if source_alpha != 0 {
                    let gray = rgb_to_gray(
                        i32::from(source_colors[2]),
                        i32::from(source_colors[1]),
                        i32::from(source_colors[0]),
                    );
                    pixel[0] = alpha_merge(i32::from(pixel[0]), gray, source_alpha) as u8;
                }
            }
            FxdibFormat::Bgr | FxdibFormat::Bgrx => {
                if source_alpha != 0 {
                    for channel in 0..3 {
                        let source = if rgb_byte_order {
                            source_colors[2 - channel]
                        } else {
                            source_colors[channel]
                        };
                        pixel[channel] =
                            alpha_merge(i32::from(pixel[channel]), i32::from(source), source_alpha)
                                as u8;
                    }
                }
            }
            _ => unreachable!(),
        }
    }
}

// Select row invariants once, keeping the BGRA loop free of format/order
// dispatch. An opaque backdrop has out_alpha=255 and ratio=source_alpha,
// so it does not need the variable division used by translucent backdrops.
fn composite_byte_mask_bgra<const OPAQUE_MASK: bool, const CLIPPED: bool>(
    dest: &mut [u8],
    source: &[u8],
    colors: [u8; 3],
    mask_alpha: u8,
    clip: &[u8],
) {
    let dest = &mut dest[..source.len() * 4];
    for (index, (pixel, &cover)) in dest.chunks_exact_mut(4).zip(source).enumerate() {
        let source_alpha = if OPAQUE_MASK {
            i32::from(cover)
        } else {
            let mut alpha = i32::from(mask_alpha) * i32::from(cover);
            if CLIPPED {
                alpha = alpha * i32::from(clip.get(index).copied().unwrap_or(255)) / 255;
            }
            alpha / 255
        };
        let back_alpha = i32::from(pixel[3]);
        if back_alpha == 0 {
            pixel[..3].copy_from_slice(&colors);
            pixel[3] = source_alpha as u8;
            continue;
        }
        if source_alpha == 0 {
            continue;
        }
        let ratio = if back_alpha == 255 {
            source_alpha
        } else {
            let out_alpha = back_alpha + source_alpha - back_alpha * source_alpha / 255;
            pixel[3] = out_alpha as u8;
            source_alpha * 255 / out_alpha
        };
        for channel in 0..3 {
            pixel[channel] =
                alpha_merge(i32::from(pixel[channel]), i32::from(colors[channel]), ratio) as u8;
        }
    }
}

pub fn composite_bit_mask_line_normal(
    dest: &mut [u8],
    source: &[u8],
    src_left: usize,
    pixel_count: usize,
    color: u32,
    format: FxdibFormat,
    rgb_byte_order: bool,
    clip: Option<&[u8]>,
) {
    let bytes_per_pixel = match format {
        FxdibFormat::EightBppMask | FxdibFormat::EightBppRgb => 1,
        FxdibFormat::Bgr => 3,
        FxdibFormat::Bgrx | FxdibFormat::Bgra => 4,
        _ => return,
    };
    for col in 0..pixel_count {
        let src_x = src_left + col;
        if source[src_x / 8] & (0x80 >> (src_x % 8)) == 0 {
            continue;
        }
        let destination = &mut dest[col * bytes_per_pixel..(col + 1) * bytes_per_pixel];
        if let Some(clip) = clip {
            composite_byte_mask_line_normal(
                destination,
                &[255],
                color,
                format,
                rgb_byte_order,
                Some(&clip[col..col + 1]),
            );
        } else {
            composite_byte_mask_line_normal(
                destination,
                &[255],
                color,
                format,
                rgb_byte_order,
                None,
            );
        }
    }
}

pub fn composite_rgb_bitmap_line_normal(
    dest: &mut [u8],
    source: &[u8],
    dest_format: FxdibFormat,
    source_format: FxdibFormat,
    pixel_count: usize,
    clip: Option<&[u8]>,
    rgb_byte_order: bool,
) -> bool {
    if source_format == FxdibFormat::Bgra && dest_format == FxdibFormat::Bgra {
        match (rgb_byte_order, clip) {
            (false, None) => composite_bgra_row::<false, false>(dest, source, pixel_count, &[]),
            (true, None) => composite_bgra_row::<true, false>(dest, source, pixel_count, &[]),
            (false, Some(clip)) => {
                composite_bgra_row::<false, true>(dest, source, pixel_count, clip)
            }
            (true, Some(clip)) => composite_bgra_row::<true, true>(dest, source, pixel_count, clip),
        }
        return true;
    }
    let source_bpp = match source_format {
        FxdibFormat::Bgr => 3,
        FxdibFormat::Bgrx | FxdibFormat::Bgra => 4,
        _ => return false,
    };
    if matches!(
        dest_format,
        FxdibFormat::EightBppMask | FxdibFormat::EightBppRgb
    ) {
        if rgb_byte_order {
            return false;
        }
        for col in 0..pixel_count {
            let input = &source[col * source_bpp..(col + 1) * source_bpp];
            let cover = i32::from(clip.map_or(255, |clip| clip[col]));
            let alpha = if source_format == FxdibFormat::Bgra {
                i32::from(input[3]) * cover / 255
            } else {
                cover
            };
            if dest_format == FxdibFormat::EightBppMask {
                if source_format != FxdibFormat::Bgra && clip.is_none() {
                    dest[col] = 255;
                } else {
                    dest[col] =
                        (i32::from(dest[col]) + alpha - i32::from(dest[col]) * alpha / 255) as u8;
                }
            } else if alpha != 0 {
                let gray = rgb_to_gray(
                    i32::from(input[2]),
                    i32::from(input[1]),
                    i32::from(input[0]),
                );
                dest[col] = alpha_merge(i32::from(dest[col]), gray, alpha) as u8;
            }
        }
        return true;
    }
    let dest_bpp = match dest_format {
        FxdibFormat::Bgr => 3,
        FxdibFormat::Bgrx | FxdibFormat::Bgra => 4,
        _ => return false,
    };
    // CompositeRow_Bgr2Bgra_NoBlend_NoClip: opaque input replaces all four
    // destination bytes, regardless of the destination's previous alpha.
    if source_format != FxdibFormat::Bgra && dest_format == FxdibFormat::Bgra && clip.is_none() {
        match (source_bpp, rgb_byte_order) {
            (3, false) => copy_opaque_bgra_row::<3, false>(dest, source, pixel_count),
            (3, true) => copy_opaque_bgra_row::<3, true>(dest, source, pixel_count),
            (4, false) => copy_opaque_bgra_row::<4, false>(dest, source, pixel_count),
            (4, true) => copy_opaque_bgra_row::<4, true>(dest, source, pixel_count),
            _ => unreachable!(),
        }
        return true;
    }
    if source_format != FxdibFormat::Bgra
        && source_format == dest_format
        && clip.is_none()
        && !rgb_byte_order
    {
        dest[..pixel_count * source_bpp].copy_from_slice(&source[..pixel_count * source_bpp]);
        return true;
    }
    // CompositeRow_Rgb2Rgb_NoBlend_NoClip with 3-byte input and 4-byte
    // output: copy BGR and preserve the destination's unused X byte.
    if source_format == FxdibFormat::Bgr
        && dest_format == FxdibFormat::Bgrx
        && clip.is_none()
        && !rgb_byte_order
    {
        for (output, input) in dest[..pixel_count * 4]
            .chunks_exact_mut(4)
            .zip(source[..pixel_count * 3].chunks_exact(3))
        {
            output[..3].copy_from_slice(input);
        }
        return true;
    }
    for col in 0..pixel_count {
        let input = &source[col * source_bpp..(col + 1) * source_bpp];
        let output = &mut dest[col * dest_bpp..(col + 1) * dest_bpp];
        let colors = if rgb_byte_order {
            [input[2], input[1], input[0]]
        } else {
            [input[0], input[1], input[2]]
        };
        let clip_alpha = i32::from(clip.map_or(255, |clip| clip[col]));
        if source_format != FxdibFormat::Bgra {
            if dest_format == FxdibFormat::Bgra {
                if clip_alpha == 255 {
                    output[..3].copy_from_slice(&colors);
                    output[3] = 255;
                } else if clip_alpha != 0 {
                    let back_alpha = i32::from(output[3]);
                    let out_alpha = back_alpha + clip_alpha - back_alpha * clip_alpha / 255;
                    let ratio = clip_alpha * 255 / out_alpha;
                    for channel in 0..3 {
                        output[channel] = alpha_merge(
                            i32::from(output[channel]),
                            i32::from(colors[channel]),
                            ratio,
                        ) as u8;
                    }
                    output[3] = out_alpha as u8;
                }
            } else if clip.is_none() && !rgb_byte_order && source_bpp == dest_bpp {
                output.copy_from_slice(input);
            } else if clip_alpha == 255 {
                output[..3].copy_from_slice(&colors);
            } else if clip_alpha != 0 {
                for channel in 0..3 {
                    output[channel] = alpha_merge(
                        i32::from(output[channel]),
                        i32::from(colors[channel]),
                        clip_alpha,
                    ) as u8;
                }
            }
            continue;
        }

        let source_alpha = i32::from(input[3]) * clip_alpha / 255;
        if dest_format != FxdibFormat::Bgra {
            if source_alpha == 255 {
                output[..3].copy_from_slice(&colors);
            } else if source_alpha != 0 {
                for channel in 0..3 {
                    output[channel] = alpha_merge(
                        i32::from(output[channel]),
                        i32::from(colors[channel]),
                        source_alpha,
                    ) as u8;
                }
            }
            continue;
        }

        let back_alpha = i32::from(output[3]);
        if back_alpha == 0 {
            output[..3].copy_from_slice(&colors);
            output[3] = source_alpha as u8;
        } else if source_alpha == 255 {
            output[..3].copy_from_slice(&colors);
            output[3] = 255;
        } else if source_alpha != 0 {
            let out_alpha = back_alpha + source_alpha - back_alpha * source_alpha / 255;
            let ratio = source_alpha * 255 / out_alpha;
            for channel in 0..3 {
                output[channel] = alpha_merge(
                    i32::from(output[channel]),
                    i32::from(colors[channel]),
                    ratio,
                ) as u8;
            }
            output[3] = out_alpha as u8;
        }
    }
    true
}

// Dispatch channel order and clipping once per row. For an opaque backdrop,
// AlphaUnion is exactly 255 and the source/destination ratio is source alpha.
fn composite_bgra_row<const RGB: bool, const CLIPPED: bool>(
    dest: &mut [u8],
    source: &[u8],
    pixel_count: usize,
    clip: &[u8],
) {
    let source = &source[..pixel_count * 4];
    let dest = &mut dest[..pixel_count * 4];
    let clip = if CLIPPED { &clip[..pixel_count] } else { &[] };
    #[cfg(feature = "simd")]
    let full = if pixel_count >= 32 {
        pixel_count / 8 * 8
    } else {
        0
    };
    #[cfg(not(feature = "simd"))]
    let full = 0;
    let (_pixels, dest) = dest.split_at_mut(full * 4);
    #[cfg(feature = "simd")]
    if full != 0 {
        bitmap_simd::composite::<RGB, CLIPPED>(_pixels, &source[..full * 4], clip);
    }
    let source = &source[full * 4..];
    let clip = if CLIPPED { &clip[full..] } else { &[] };
    for (index, (input, output)) in source
        .chunks_exact(4)
        .zip(dest.chunks_exact_mut(4))
        .enumerate()
    {
        let colors = if RGB {
            [input[2], input[1], input[0]]
        } else {
            [input[0], input[1], input[2]]
        };
        let alpha = if CLIPPED {
            i32::from(input[3]) * i32::from(clip[index]) / 255
        } else {
            i32::from(input[3])
        };
        let back = i32::from(output[3]);
        if back == 0 || alpha == 255 {
            output[..3].copy_from_slice(&colors);
            output[3] = alpha as u8;
        } else if alpha != 0 {
            let ratio = if back == 255 {
                alpha
            } else {
                let union = back + alpha - back * alpha / 255;
                output[3] = union as u8;
                alpha * 255 / union
            };
            for channel in 0..3 {
                output[channel] = alpha_merge(
                    i32::from(output[channel]),
                    i32::from(colors[channel]),
                    ratio,
                ) as u8;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn composite_palette_bitmap_line_normal(
    dest: &mut [u8],
    source: &[u8],
    src_left: usize,
    pixel_count: usize,
    dest_format: FxdibFormat,
    source_format: FxdibFormat,
    palette: &[u32],
    clip: Option<&[u8]>,
    rgb_byte_order: bool,
) -> bool {
    let source_bpp = match source_format {
        FxdibFormat::OneBppRgb => 1,
        FxdibFormat::EightBppRgb => 8,
        _ => return false,
    };
    let dest_bpp = match dest_format {
        FxdibFormat::EightBppMask | FxdibFormat::EightBppRgb if !rgb_byte_order => 1,
        FxdibFormat::Bgr => 3,
        FxdibFormat::Bgrx | FxdibFormat::Bgra => 4,
        _ => return false,
    };
    if !palette.is_empty() && palette.len() < 1usize << source_bpp {
        return false;
    }
    if source_bpp == 8 && dest_format == FxdibFormat::Bgra && clip.is_none() && pixel_count != 0 {
        let source = &source[src_left..src_left + pixel_count];
        let dest = &mut dest[..pixel_count * 4];
        if palette.is_empty() {
            composite_index8_bgra::<false, false>(dest, source, palette);
        } else if rgb_byte_order {
            composite_index8_bgra::<true, true>(dest, source, palette);
        } else {
            composite_index8_bgra::<true, false>(dest, source, palette);
        }
        return true;
    }
    for col in 0..pixel_count {
        let cover = i32::from(clip.map_or(255, |clip| clip[col]));
        let output = &mut dest[col * dest_bpp..(col + 1) * dest_bpp];
        if dest_format == FxdibFormat::EightBppMask {
            output[0] = if clip.is_some() {
                (i32::from(output[0]) + cover - i32::from(output[0]) * cover / 255) as u8
            } else {
                255
            };
            continue;
        }
        let index = if source_bpp == 1 {
            let x = src_left + col;
            usize::from(source[x / 8] & (0x80 >> (x % 8)) != 0)
        } else {
            usize::from(source[src_left + col])
        };
        let color = if palette.is_empty() {
            let v = if source_bpp == 1 {
                (index * 255) as u32
            } else {
                index as u32
            };
            0xff00_0000 | v << 16 | v << 8 | v
        } else {
            palette[index]
        };
        let channels = if rgb_byte_order {
            [argb_r(color), argb_g(color), argb_b(color)]
        } else {
            [argb_b(color), argb_g(color), argb_r(color)]
        };
        if dest_format == FxdibFormat::EightBppRgb {
            let gray = rgb_to_gray(
                i32::from(argb_r(color)),
                i32::from(argb_g(color)),
                i32::from(argb_b(color)),
            );
            output[0] = if cover == 255 {
                gray as u8
            } else {
                alpha_merge(i32::from(output[0]), gray, cover) as u8
            };
        } else if dest_format == FxdibFormat::Bgra {
            if cover == 255 {
                output[..3].copy_from_slice(&channels);
                output[3] = 255;
            } else if cover != 0 {
                let back_alpha = i32::from(output[3]);
                let out_alpha = back_alpha + cover - back_alpha * cover / 255;
                let ratio = cover * 255 / out_alpha;
                for channel in 0..3 {
                    output[channel] = alpha_merge(
                        i32::from(output[channel]),
                        i32::from(channels[channel]),
                        ratio,
                    ) as u8;
                }
                output[3] = out_alpha as u8;
            }
        } else if cover == 255 {
            output[..3].copy_from_slice(&channels);
        } else if cover != 0 {
            for channel in 0..3 {
                output[channel] = alpha_merge(
                    i32::from(output[channel]),
                    i32::from(channels[channel]),
                    cover,
                ) as u8;
            }
        }
    }
    true
}

// Four packed BGRA alpha bytes. Byte conversion is safe for unaligned rows
// and fixes the byte order independently of the host's endianness.
const ALPHA_MASK: u128 = 0xff00_0000_ff00_0000_ff00_0000_ff00_0000;

#[inline(always)]
fn alpha_block_matches(block: &[u8], value: u128) -> bool {
    // Callers supply complete 32-pixel blocks and a constant zero/opaque
    // alpha mask. Reduce the words first, avoiding a branch per four pixels.
    let chunks = block.as_chunks::<16>().0;
    let bits = if value == 0 {
        chunks
            .iter()
            .fold(0, |bits, bytes| bits | u128::from_le_bytes(*bytes))
    } else {
        chunks
            .iter()
            .fold(u128::MAX, |bits, bytes| bits & u128::from_le_bytes(*bytes))
    };
    bits & ALPHA_MASK == value
}

// CompositePixelBgra2BgraCommon followed by the separable/nonseparable blend
// operations. Transparent input still copies RGB when the backdrop alpha is
// zero, as required by the pinned source. All integer divisions retain their
// original order; opaque backdrops let the source merge and alpha union reduce
// exactly, without the per-pixel variable division for the alpha ratio.
fn composite_bgra_blend_row<const RGB: bool, const CLIP: bool>(
    dest: &mut [u8],
    source: &[u8],
    width: usize,
    clip: &[u8],
    mode: BlendMode,
) {
    let clip = if CLIP { &clip[..width] } else { &[] };
    for (col, (output, input)) in dest[..width * 4]
        .chunks_exact_mut(4)
        .zip(source[..width * 4].chunks_exact(4))
        .enumerate()
    {
        let alpha = if CLIP {
            i32::from(input[3]) * i32::from(clip[col]) / 255
        } else {
            i32::from(input[3])
        };
        let back_alpha = i32::from(output[3]);
        if back_alpha == 0 {
            let pixel = if RGB {
                [input[2], input[1], input[0], alpha as u8]
            } else {
                [input[0], input[1], input[2], alpha as u8]
            };
            output.copy_from_slice(&pixel);
            continue;
        }
        if alpha == 0 {
            continue;
        }
        let src = [
            i32::from(input[2]),
            i32::from(input[1]),
            i32::from(input[0]),
        ];
        let back = if RGB {
            [
                i32::from(output[0]),
                i32::from(output[1]),
                i32::from(output[2]),
            ]
        } else {
            [
                i32::from(output[2]),
                i32::from(output[1]),
                i32::from(output[0]),
            ]
        };
        let blended = rgb_blend(mode, src, back);
        let (out_alpha, ratio) = if back_alpha == 255 {
            (255, alpha)
        } else {
            let union = alpha_union(back_alpha, alpha);
            (union, alpha * 255 / union)
        };
        for channel in 0..3 {
            let blended = if back_alpha == 255 {
                blended[channel]
            } else {
                alpha_merge(src[channel], blended[channel], back_alpha)
            };
            output[if RGB { channel } else { 2 - channel }] =
                alpha_merge(back[channel], blended, ratio) as u8;
        }
        output[3] = out_alpha as u8;
    }
}

fn copy_opaque_bgra_row<const STRIDE: usize, const RGB: bool>(
    dest: &mut [u8],
    source: &[u8],
    width: usize,
) {
    for (output, input) in dest[..width * 4]
        .chunks_exact_mut(4)
        .zip(source[..width * STRIDE].chunks_exact(STRIDE))
    {
        let pixel = if RGB {
            [input[2], input[1], input[0], 255]
        } else {
            [input[0], input[1], input[2], 255]
        };
        output.copy_from_slice(&pixel);
    }
}

// Indexed input is opaque even when its palette's stored alpha differs. Select
// gray/palette and channel order outside the loop and write complete BGRA pixels.
fn composite_index8_bgra<const PALETTED: bool, const RGB: bool>(
    dest: &mut [u8],
    source: &[u8],
    palette: &[u32],
) {
    let palette = if PALETTED { &palette[..256] } else { &[] };
    for (pixel, &index) in dest.chunks_exact_mut(4).zip(source) {
        let color = if PALETTED {
            palette[usize::from(index)] | 0xff00_0000
        } else {
            0xff00_0000 | u32::from(index) * 0x0001_0101
        };
        let mut bytes = color.to_le_bytes();
        if RGB {
            bytes.swap(0, 2);
        }
        pixel.copy_from_slice(&bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_blends_preserve_alpha_pairs_clipping_order_and_row_guards() {
        let mut source = vec![0x5a; 1 + 256 * 4 + 5];
        for (alpha, pixel) in source[1..1 + 256 * 4].chunks_exact_mut(4).enumerate() {
            pixel.copy_from_slice(&[alpha as u8 ^ 0xa5, 255 - alpha as u8, 73, alpha as u8]);
        }
        let clip: Vec<u8> = (0..=255).map(|i| (i * 73 + 19) as u8).collect();
        for mode in [
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::HardLight,
            BlendMode::SoftLight,
            BlendMode::Difference,
            BlendMode::Exclusion,
            BlendMode::Hue,
            BlendMode::Saturation,
            BlendMode::Color,
            BlendMode::Luminosity,
        ] {
            for back_alpha in 0u64..=255 {
                for rgb in [false, true] {
                    for clipping in [None, Some(clip.as_slice()), Some(&[0; 256][..])] {
                        let mut actual = vec![0xa5; 3 + 256 * 4 + 7];
                        for pixel in actual[3..3 + 256 * 4].chunks_exact_mut(4) {
                            pixel.copy_from_slice(&[211, 137, 29, back_alpha as u8]);
                        }
                        let mut expected = actual.clone();
                        for (i, (input, output)) in source[1..1 + 256 * 4]
                            .chunks_exact(4)
                            .zip(expected[3..3 + 256 * 4].chunks_exact_mut(4))
                            .enumerate()
                        {
                            let alpha = u64::from(input[3])
                                * u64::from(clipping.map_or(255, |c| c[i]))
                                / 255;
                            let src = [
                                i32::from(input[2]),
                                i32::from(input[1]),
                                i32::from(input[0]),
                            ];
                            let back = if rgb { [211, 137, 29] } else { [29, 137, 211] };
                            let blended = rgb_blend(mode, src, back);
                            let union = back_alpha + alpha - back_alpha * alpha / 255;
                            for c in 0..3 {
                                let channel = if rgb { c } else { 2 - c };
                                output[channel] = if back_alpha == 0 {
                                    src[c] as u8
                                } else {
                                    let blend = (src[c] as u64 * (255 - back_alpha)
                                        + blended[c] as u64 * back_alpha)
                                        / 255;
                                    let ratio = alpha * 255 / union;
                                    ((back[c] as u64 * (255 - ratio) + blend * ratio) / 255) as u8
                                };
                            }
                            output[3] = union as u8;
                        }
                        let compositor = CfxScanlineCompositor::new(
                            FxdibFormat::Bgra,
                            FxdibFormat::Bgra,
                            &[],
                            0,
                            mode,
                            rgb,
                        )
                        .unwrap();
                        compositor.composite_rgb_bitmap_line(
                            &mut actual[3..],
                            &source[1..],
                            256,
                            clipping,
                        );
                        assert_eq!(
                            actual, expected,
                            "mode={mode:?}, alpha={back_alpha}, rgb={rgb}"
                        );
                        compositor.composite_rgb_bitmap_line(&mut [], &[], 0, clipping);
                    }
                }
            }
        }
    }

    #[test]
    fn normal_opaque_bgr_rows_replace_bgra_alpha_and_preserve_row_guards() {
        for (format, stride) in [(FxdibFormat::Bgr, 3), (FxdibFormat::Bgrx, 4)] {
            for width in (0..=130).chain([255, 256, 257, 1322]) {
                let source: Vec<u8> = (0..width * stride + 11)
                    .map(|i| (i * 71 + i / 3 + 19) as u8)
                    .collect();
                for rgb in [false, true] {
                    for empty_clip in [false, true] {
                        let mut dest: Vec<u8> =
                            (0..width * 4 + 13).map(|i| (i * 37 + 127) as u8).collect();
                        let original = dest.clone();
                        let compositor = CfxScanlineCompositor::new(
                            FxdibFormat::Bgra,
                            format,
                            &[],
                            0,
                            BlendMode::Normal,
                            rgb,
                        )
                        .unwrap();
                        compositor.composite_rgb_bitmap_line(
                            &mut dest[5..],
                            &source[2..],
                            width,
                            empty_clip.then_some(&[]),
                        );
                        for (i, &value) in dest.iter().enumerate() {
                            let expected = if i >= 5 && i < 5 + width * 4 {
                                let channel = (i - 5) % 4;
                                if channel == 3 {
                                    255
                                } else {
                                    source[2
                                        + (i - 5) / 4 * stride
                                        + if rgb { 2 - channel } else { channel }]
                                }
                            } else {
                                original[i]
                            };
                            assert_eq!(
                                value, expected,
                                "format={format:?}, width={width}, rgb={rgb}, byte={i}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn normal_bgr_to_bgrx_preserves_padding_and_row_guards() {
        for width in (0..=130).chain([255, 256, 257, 1330]) {
            let source: Vec<u8> = (0..width * 3 + 11)
                .map(|i| (i * 71 + i / 3 + 19) as u8)
                .collect();
            for rgb in [false, true] {
                for empty_clip in [false, true] {
                    let mut dest: Vec<u8> =
                        (0..width * 4 + 13).map(|i| (i * 37 + 127) as u8).collect();
                    let original = dest.clone();
                    let compositor = CfxScanlineCompositor::new(
                        FxdibFormat::Bgrx,
                        FxdibFormat::Bgr,
                        &[],
                        0,
                        BlendMode::Normal,
                        rgb,
                    )
                    .unwrap();
                    compositor.composite_rgb_bitmap_line(
                        &mut dest[5..],
                        &source[2..],
                        width,
                        empty_clip.then_some(&[]),
                    );
                    for (i, &value) in dest.iter().enumerate() {
                        let expected = if i >= 5 && i < 5 + width * 4 && (i - 5) % 4 < 3 {
                            let channel = (i - 5) % 4;
                            source[2 + (i - 5) / 4 * 3 + if rgb { 2 - channel } else { channel }]
                        } else {
                            original[i]
                        };
                        assert_eq!(value, expected, "width={width}, rgb={rgb}, byte={i}");
                    }
                }
            }
        }
    }

    #[test]
    fn normal_gray_palette_replaces_the_row_and_ignores_byte_source_left() {
        let colors: Vec<u32> = (0..256u32)
            .map(|i| (i * 31) << 24 | ((i * 41) & 255) << 16 | ((i * 73) & 255) << 8 | i)
            .collect();
        for palette in [&[][..], colors.as_slice()] {
            let compositor = CfxScanlineCompositor::new(
                FxdibFormat::EightBppRgb,
                FxdibFormat::EightBppRgb,
                palette,
                0,
                BlendMode::Normal,
                false,
            )
            .unwrap();
            for width in (0..=130).chain([255, 256, 257, 1330]) {
                let source: Vec<u8> = (0..width + 11).map(|i| (i * 73 + 19) as u8).collect();
                let mut dest = vec![173u8; width + 13];
                compositor.composite_pal_bitmap_line(
                    &mut dest[5..],
                    &source[2..],
                    303,
                    width,
                    None,
                );
                for (i, &value) in dest.iter().enumerate() {
                    let expected = if i >= 5 && i < 5 + width {
                        let sample = source[2 + i - 5];
                        if palette.is_empty() {
                            sample
                        } else {
                            let argb = palette[usize::from(sample)];
                            rgb_to_gray(
                                i32::from(argb_r(argb)),
                                i32::from(argb_g(argb)),
                                i32::from(argb_b(argb)),
                            ) as u8
                        }
                    } else {
                        173
                    };
                    assert_eq!(value, expected, "width={width}, byte={i}");
                }
            }
        }
    }

    #[test]
    fn indexed_bgra_rows_preserve_offsets_gray_palette_order_opacity_and_guards() {
        let source: Vec<u8> = (0..290).map(|i| (i * 73 + 19) as u8).collect();
        let palette: Vec<u32> = (0..256u32)
            .map(|i| (i << 24) | ((255 - i) << 16) | (((i * 31) & 255) << 8) | i)
            .collect();
        for colors in [&[][..], palette.as_slice()] {
            for rgb in [false, true] {
                for left in [0, 3, 17] {
                    let mut actual = vec![0xa5; 1 + 256 * 4 + 9];
                    let mut expected = actual.clone();
                    for (pixel, &index) in expected[1..1 + 256 * 4]
                        .chunks_exact_mut(4)
                        .zip(&source[left..left + 256])
                    {
                        let color = if colors.is_empty() {
                            [index, index, index]
                        } else {
                            let c = colors[usize::from(index)];
                            [c as u8, (c >> 8) as u8, (c >> 16) as u8]
                        };
                        for c in 0..3 {
                            pixel[c] = color[if rgb { 2 - c } else { c }];
                        }
                        pixel[3] = 255;
                    }
                    assert!(composite_palette_bitmap_line_normal(
                        &mut actual[1..],
                        &source,
                        left,
                        256,
                        FxdibFormat::Bgra,
                        FxdibFormat::EightBppRgb,
                        colors,
                        None,
                        rgb,
                    ));
                    assert_eq!(actual, expected);
                }
            }
        }
        let mut dest = [0xa5; 4];
        assert!(!composite_palette_bitmap_line_normal(
            &mut dest,
            &[0],
            0,
            1,
            FxdibFormat::Bgra,
            FxdibFormat::EightBppRgb,
            &palette[..255],
            None,
            false,
        ));
        assert_eq!(dest, [0xa5; 4]);
        // Empty rows never inspect the source offset, as in the generic loop.
        assert!(composite_palette_bitmap_line_normal(
            &mut [],
            &[],
            usize::MAX,
            0,
            FxdibFormat::Bgra,
            FxdibFormat::EightBppRgb,
            &[],
            None,
            false,
        ));
    }

    #[test]
    fn bgra_bitmap_rows_preserve_simd_boundaries_tails_and_uniform_alpha_fast_paths() {
        for width in [0usize, 1, 7, 8, 15, 16, 31, 32, 33, 39, 40, 41, 65, 257] {
            for alpha in [0u8, 1, 127, 254, 255] {
                let mut source = vec![0x5a; 3 + width * 4 + 5];
                for (i, pixel) in source[3..3 + width * 4].chunks_exact_mut(4).enumerate() {
                    pixel.copy_from_slice(&[
                        (i * 13) as u8,
                        (i * 73) as u8,
                        (i * 19 + 11) as u8,
                        alpha,
                    ]);
                }
                for backdrop in [0, 1, 2] {
                    for clip_mode in [0, 1, 2, 3] {
                        let covers: Vec<u8> = (0..width)
                            .map(|i| match clip_mode {
                                1 => 0,
                                2 => 255,
                                _ => (i * 73 + 19) as u8,
                            })
                            .collect();
                        let clip = (clip_mode != 0).then_some(covers.as_slice());
                        for rgb in [false, true] {
                            let mut actual = vec![0xa5; 1 + width * 4 + 7];
                            for (i, pixel) in
                                actual[1..1 + width * 4].chunks_exact_mut(4).enumerate()
                            {
                                pixel.copy_from_slice(&[
                                    211,
                                    137,
                                    29,
                                    match backdrop {
                                        0 => 0,
                                        1 => 255,
                                        _ => (i * 31) as u8,
                                    },
                                ]);
                            }
                            let mut expected = actual.clone();
                            for (i, (input, output)) in source[3..3 + width * 4]
                                .chunks_exact(4)
                                .zip(expected[1..1 + width * 4].chunks_exact_mut(4))
                                .enumerate()
                            {
                                let a =
                                    u64::from(alpha) * u64::from(clip.map_or(255, |c| c[i])) / 255;
                                let back = u64::from(output[3]);
                                let union = back + a - back * a / 255;
                                for c in 0..3 {
                                    let color = u64::from(input[if rgb { 2 - c } else { c }]);
                                    output[c] = if back == 0 {
                                        color as u8
                                    } else {
                                        let ratio = a * 255 / union;
                                        ((u64::from(output[c]) * (255 - ratio) + color * ratio)
                                            / 255) as u8
                                    };
                                }
                                output[3] = union as u8;
                            }
                            assert!(composite_rgb_bitmap_line_normal(
                                &mut actual[1..],
                                &source[3..],
                                FxdibFormat::Bgra,
                                FxdibFormat::Bgra,
                                width,
                                clip,
                                rgb
                            ));
                            assert_eq!(actual, expected, "width={width} alpha={alpha} backdrop={backdrop} clip={clip_mode} rgb={rgb}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn bgra_rows_preserve_every_alpha_pair_clipping_channel_order_and_guards() {
        let mut source = vec![0x5a; 1 + 256 * 4 + 5];
        for (alpha, pixel) in source[1..1 + 256 * 4].chunks_exact_mut(4).enumerate() {
            pixel.copy_from_slice(&[alpha as u8 ^ 0xa5, 255 - alpha as u8, 73, alpha as u8]);
        }
        let clip: Vec<u8> = (0..=255).map(|i| (i * 73 + 19) as u8).collect();
        for back in 0u8..=255 {
            for rgb in [false, true] {
                for clipping in [None, Some(clip.as_slice())] {
                    let mut actual = vec![0xa5; 1 + 256 * 4 + 7];
                    for pixel in actual[1..1 + 256 * 4].chunks_exact_mut(4) {
                        pixel.copy_from_slice(&[211, 137, 29, back]);
                    }
                    let mut expected = actual.clone();
                    for (i, (input, output)) in source[1..1 + 256 * 4]
                        .chunks_exact(4)
                        .zip(expected[1..1 + 256 * 4].chunks_exact_mut(4))
                        .enumerate()
                    {
                        let alpha =
                            u64::from(input[3]) * u64::from(clipping.map_or(255, |c| c[i])) / 255;
                        let backdrop = u64::from(back);
                        let union = backdrop + alpha - backdrop * alpha / 255;
                        for c in 0..3 {
                            let color = u64::from(input[if rgb { 2 - c } else { c }]);
                            output[c] = if backdrop == 0 {
                                color as u8
                            } else {
                                let ratio = alpha * 255 / union;
                                ((u64::from(output[c]) * (255 - ratio) + color * ratio) / 255) as u8
                            };
                        }
                        output[3] = union as u8;
                    }
                    assert!(composite_rgb_bitmap_line_normal(
                        &mut actual[1..],
                        &source[1..],
                        FxdibFormat::Bgra,
                        FxdibFormat::Bgra,
                        256,
                        clipping,
                        rgb,
                    ));
                    assert_eq!(actual, expected, "back={back} rgb={rgb}");
                }
            }
        }
    }

    #[test]
    fn bgra_byte_mask_specializations_preserve_integer_compositing_and_guards() {
        let covers: Vec<u8> = (0..=255).collect();
        let clip: Vec<u8> = (0..=255).map(|i| (i * 73 + 19) as u8).collect();
        for mask_alpha in [0u32, 1, 127, 254, 255] {
            for back_alpha in 0u8..=255 {
                for rgb in [false, true] {
                    for clipping in [None, Some(clip.as_slice()), Some(&clip[..117])] {
                        let mut actual = vec![0xa5; 1 + 256 * 4 + 7];
                        for pixel in actual[1..1 + 256 * 4].chunks_exact_mut(4) {
                            pixel.copy_from_slice(&[211, 137, 29, back_alpha]);
                        }
                        let mut expected = actual.clone();
                        let colors = if rgb { [255u64, 119, 0] } else { [0, 119, 255] };
                        for (i, pixel) in expected[1..1 + 256 * 4].chunks_exact_mut(4).enumerate() {
                            // Wide arithmetic reference: clipped coverage is a
                            // product divided by 255 squared, with no helper reuse.
                            let alpha = u64::from(mask_alpha)
                                * i as u64
                                * u64::from(
                                    clipping.and_then(|c| c.get(i)).copied().unwrap_or(255),
                                )
                                / (255 * 255);
                            let back = u64::from(back_alpha);
                            if back == 0 {
                                for c in 0..3 {
                                    pixel[c] = colors[c] as u8;
                                }
                                pixel[3] = alpha as u8;
                            } else {
                                let union = back + alpha - back * alpha / 255;
                                let ratio = alpha * 255 / union;
                                for c in 0..3 {
                                    pixel[c] = ((u64::from(pixel[c]) * (255 - ratio)
                                        + colors[c] * ratio)
                                        / 255) as u8;
                                }
                                pixel[3] = union as u8;
                            }
                        }
                        composite_byte_mask_line_normal(
                            &mut actual[1..],
                            &covers,
                            (mask_alpha << 24) | 0xff7700,
                            FxdibFormat::Bgra,
                            rgb,
                            clipping,
                        );
                        assert_eq!(
                            actual, expected,
                            "mask={mask_alpha} back={back_alpha} rgb={rgb}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn byte_mask_respects_source_and_clip_alpha() {
        let mut pixels = [0u8; 8];
        composite_byte_mask_line_normal(
            &mut pixels,
            &[255, 128],
            0xffff0000,
            FxdibFormat::Bgra,
            false,
            Some(&[128, 255]),
        );
        assert_eq!(pixels, [0, 0, 255, 128, 0, 0, 255, 128]);
    }

    #[test]
    fn bit_mask_uses_source_left_and_preserves_zero_bits() {
        let mut pixels = [0u8; 12];
        composite_bit_mask_line_normal(
            &mut pixels,
            &[0b0101_0000],
            1,
            3,
            0xffff0000,
            FxdibFormat::Bgra,
            false,
            None,
        );
        assert_eq!(pixels, [0, 0, 255, 255, 0, 0, 0, 0, 0, 0, 255, 255]);
    }

    #[test]
    fn normal_rgb_bitmap_lines_copy_opaque_and_merge_alpha() {
        let mut bgra = [10, 20, 30, 128, 1, 2, 3, 0];
        assert!(composite_rgb_bitmap_line_normal(
            &mut bgra,
            &[100, 120, 140, 128, 40, 50, 60, 0],
            FxdibFormat::Bgra,
            FxdibFormat::Bgra,
            2,
            None,
            false,
        ));
        assert_eq!(bgra, [70, 86, 103, 192, 40, 50, 60, 0]);

        let mut bgrx = [1, 2, 3, 77];
        assert!(composite_rgb_bitmap_line_normal(
            &mut bgrx,
            &[10, 20, 30],
            FxdibFormat::Bgrx,
            FxdibFormat::Bgr,
            1,
            None,
            false,
        ));
        assert_eq!(bgrx, [10, 20, 30, 77]);

        assert!(composite_rgb_bitmap_line_normal(
            &mut bgrx,
            &[100, 120, 140, 9],
            FxdibFormat::Bgrx,
            FxdibFormat::Bgrx,
            1,
            Some(&[255]),
            false,
        ));
        assert_eq!(bgrx, [100, 120, 140, 77]);

        assert!(composite_rgb_bitmap_line_normal(
            &mut bgrx,
            &[9, 40, 200, 1],
            FxdibFormat::Bgrx,
            FxdibFormat::Bgrx,
            1,
            None,
            true,
        ));
        assert_eq!(bgrx, [200, 40, 9, 77]);
    }

    #[test]
    fn indexed_bitmap_lines_use_palette_and_bit_offsets() {
        let mut pixels = [0u8; 12];
        assert!(composite_palette_bitmap_line_normal(
            &mut pixels,
            &[0b0101_0000],
            1,
            3,
            FxdibFormat::Bgra,
            FxdibFormat::OneBppRgb,
            &[0xff0000ff, 0xff00ff00],
            Some(&[255, 128, 0]),
            false,
        ));
        assert_eq!(pixels, [0, 255, 0, 255, 255, 0, 0, 128, 0, 0, 0, 0]);
    }
    #[test]
    fn nonseparable_colors_use_the_pinned_integer_steps() {
        for (mode, expected) in [
            (BlendMode::Hue, [61, 134, 191]),
            (BlendMode::Saturation, [195, 96, 35]),
            (BlendMode::Color, [48, 138, 208]),
            (BlendMode::Luminosity, [172, 92, 42]),
        ] {
            assert_eq!(rgb_blend(mode, [40, 130, 200], [180, 100, 50]), expected);
        }
    }

    #[test]
    fn clipped_opaque_blend_preserves_zero_destination_alpha() {
        let compositor = CfxScanlineCompositor::new(
            FxdibFormat::Bgra,
            FxdibFormat::Bgr,
            &[],
            0,
            BlendMode::Multiply,
            false,
        )
        .unwrap();
        let mut pixel = [1, 2, 3, 0];
        compositor.composite_rgb_bitmap_line(&mut pixel, &[40, 50, 60], 1, Some(&[128]));
        assert_eq!(pixel, [40, 50, 60, 0]);
        compositor.composite_rgb_bitmap_line(&mut pixel, &[70, 80, 90], 1, None);
        assert_eq!(pixel, [70, 80, 90, 255]);
    }

    #[test]
    fn nonseparable_masks_and_bitmaps_keep_their_distinct_alpha_rules() {
        let mask = CfxScanlineCompositor::new(
            FxdibFormat::Bgra,
            FxdibFormat::EightBppMask,
            &[],
            0x802882c8,
            BlendMode::Hue,
            false,
        )
        .unwrap();
        let mut mask_dest = [50, 100, 180, 128];
        mask.composite_byte_mask_line(&mut mask_dest, &[255], 1, None);
        assert_eq!(mask_dest, [144, 122, 100, 192]);
        let bitmap = CfxScanlineCompositor::new(
            FxdibFormat::Bgra,
            FxdibFormat::Bgra,
            &[],
            0,
            BlendMode::Hue,
            false,
        )
        .unwrap();
        let mut bitmap_dest = [50, 100, 180, 128];
        bitmap.composite_rgb_bitmap_line(&mut bitmap_dest, &[200, 130, 40, 128], 1, None);
        assert_eq!(bitmap_dest, [146, 121, 93, 192]);
        let bitmap_rgb = CfxScanlineCompositor::new(
            FxdibFormat::Bgra,
            FxdibFormat::Bgra,
            &[],
            0,
            BlendMode::Hue,
            true,
        )
        .unwrap();
        let mut rgb_dest = [180, 100, 50, 128];
        bitmap_rgb.composite_rgb_bitmap_line(&mut rgb_dest, &[200, 130, 40, 128], 1, None);
        assert_eq!(rgb_dest, [93, 121, 146, 192]);
    }

    #[test]
    fn gray_palette_blends_and_color_palette_ignores_blend_modes() {
        for (mode, expected) in [
            (BlendMode::Multiply, 33),
            (BlendMode::Hue, 50),
            (BlendMode::Luminosity, 71),
        ] {
            let compositor = CfxScanlineCompositor::new(
                FxdibFormat::EightBppRgb,
                FxdibFormat::EightBppRgb,
                &[0xffc83219; 256],
                0,
                mode,
                false,
            )
            .unwrap();
            let mut dest = [50];
            compositor.composite_pal_bitmap_line(&mut dest, &[0], 100, 1, Some(&[128]));
            assert_eq!(dest, [expected]); // Eight-bit source_left is already applied by its caller.
        }
        let compositor = CfxScanlineCompositor::new(
            FxdibFormat::Bgra,
            FxdibFormat::OneBppRgb,
            &[0x00112233, 0x80c83219],
            0,
            BlendMode::Multiply,
            false,
        )
        .unwrap();
        let mut dest = [100, 100, 100, 255];
        compositor.composite_pal_bitmap_line(&mut dest, &[0x20], 2, 1, None);
        assert_eq!(dest, [25, 50, 200, 255]);
        assert!(CfxScanlineCompositor::new(
            FxdibFormat::EightBppMask,
            FxdibFormat::Bgr,
            &[],
            0,
            BlendMode::Normal,
            true
        )
        .is_none());
    }

    #[test]
    fn byte_mask_accepts_a_partial_clip_span() {
        let compositor = CfxScanlineCompositor::new(
            FxdibFormat::Bgra,
            FxdibFormat::EightBppMask,
            &[],
            0xc8112233,
            BlendMode::Normal,
            false,
        )
        .unwrap();
        let mut dest = [0; 8];
        compositor.composite_byte_mask_line(&mut dest, &[255, 255], 2, Some(&[128]));
        assert_eq!(dest, [0x33, 0x22, 0x11, 100, 0x33, 0x22, 0x11, 200]);
    }
}

#[cfg(test)]
#[path = "cfx_scanlinecompositor_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "sparse_blend_tests.rs"]
mod sparse_blend_tests;

#[cfg(test)]
#[path = "cfx_scanlinecompositor_boundary_tests.rs"]
mod boundary_tests;
