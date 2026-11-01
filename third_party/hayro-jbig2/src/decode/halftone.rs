// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
//! Halftone region segment parsing and decoding (7.4.5, 6.6).

// PDFium compatibility: Copyright 2014 The PDFium Authors.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// These branches use the BSD-style license in ../../LICENSE-PDFIUM.

use alloc::vec;
use alloc::vec::Vec;

use super::RegionBitmap;
use super::pattern::PatternDictionary;
use super::{CombinationOperator, RegionSegmentInfo, Template, parse_region_segment_info};
use crate::ScratchBuffers;
use crate::bitmap::{Bitmap, MAX_DIMENSION, WORD_BITS};
use crate::error::{OverflowError, ParseError, RegionError, Result, bail};
use crate::gray_scale::{GrayScaleImage, GrayScaleParams, decode_gray_scale_image};
use crate::reader::Reader;

/// Decode a halftone region segment (7.4.5.2, 6.6).
pub(crate) fn decode(
    header: &HalftoneRegionHeader<'_>,
    pattern_dict: &PatternDictionary,
    ctx: &mut ScratchBuffers,
) -> Result<(RegionBitmap, usize)> {
    let region = &header.region_info;
    // Native DecodeImage constructs HTREG after decoding all GSPLANES.
    let decoded_grid = if header.pdfium {
        Some(decode_grid(header, pattern_dict, ctx)?)
    } else {
        None
    };

    let mut htreg = Bitmap::new_region(
        region.width,
        region.height,
        region.x_location,
        region.y_location,
        header.flags.initial_pixel_color,
        header.pdfium,
    )?;

    htreg.pdfium = header.pdfium;

    let consumed = if let Some((gi, consumed)) = decoded_grid {
        render_patterns(&mut htreg, &gi, header, pattern_dict)?;
        consumed
    } else {
        decode_into(header, pattern_dict, &mut htreg, ctx)?
    };

    Ok((
        RegionBitmap {
            bitmap: htreg,
            combination_operator: region.combination_operator,
        },
        consumed,
    ))
}

pub(crate) fn decode_into(
    header: &HalftoneRegionHeader<'_>,
    pattern_dict: &PatternDictionary,
    htreg: &mut Bitmap,
    ctx: &mut ScratchBuffers,
) -> Result<usize> {
    let (gi, consumed) = decode_grid(header, pattern_dict, ctx)?;
    render_patterns(htreg, &gi, header, pattern_dict)?;
    Ok(consumed)
}

fn decode_grid(
    header: &HalftoneRegionHeader<'_>,
    pattern_dict: &PatternDictionary,
    ctx: &mut ScratchBuffers,
) -> Result<(GrayScaleImage, usize)> {
    if header.pdfium
        && !Bitmap::is_valid_owned_size(
            header.grid_position_and_size.width,
            header.grid_position_and_size.height,
        )
    {
        // Reject the empty native plane before allocating expanded values or
        // a skip mask for dimensions whose CJBig2_Image has no pixel storage.
        bail!(OverflowError::GridDimension);
    }
    let skip_bitmap = if header.flags.enable_skip && (!header.pdfium || !header.flags.mmr) {
        Some(compute_skip_bitmap(header, pattern_dict)?)
    } else {
        None
    };

    // "3) Set HBPP to ⌈log₂(HNUMPATS)⌉." (6.6.5)
    let bits_per_pixel = (pattern_dict.patterns.len() as u32)
        .saturating_sub(1)
        .checked_ilog2()
        .map_or(1, |n| n + 1);

    // "4) Decode an image GI of size HGW by HGH with HBPP bits per pixel using
    // the gray-scale image decoding procedure as described in Annex C." (6.6.5)
    let gs_params = GrayScaleParams {
        pdfium: header.pdfium,
        use_mmr: header.flags.mmr,
        bits_per_pixel,
        width: header.grid_position_and_size.width,
        height: header.grid_position_and_size.height,
        template: header.flags.template,
        skip_mask: skip_bitmap.as_deref(),
    };
    decode_gray_scale_image(header.data, &gs_params, ctx)
}

/// Parse a halftone region segment header (7.4.5.1).
pub(crate) fn parse<'a>(reader: &mut Reader<'a>) -> Result<HalftoneRegionHeader<'a>> {
    let region_info = parse_region_segment_info(reader)?;
    let flags_byte = reader.read_byte().ok_or(ParseError::UnexpectedEof)?;
    let mmr = flags_byte & 0x01 != 0;
    let template = Template::from_byte(flags_byte >> 1);
    let enable_skip = flags_byte & 0x08 != 0;
    let combination_operator = if reader.is_pdfium() {
        CombinationOperator::from_pdfium_value(flags_byte >> 4)
    } else {
        CombinationOperator::from_value(flags_byte >> 4)?
    };
    let initial_pixel_color = flags_byte & 0x80 != 0;

    let flags = HalftoneRegionFlags {
        mmr,
        template,
        enable_skip,
        combination_operator,
        initial_pixel_color,
    };

    let grid_width = reader.read_u32().ok_or(ParseError::UnexpectedEof)?;
    let grid_height = reader.read_u32().ok_or(ParseError::UnexpectedEof)?;

    if grid_width > MAX_DIMENSION || grid_height > MAX_DIMENSION {
        bail!(OverflowError::GridDimension);
    }

    if reader.is_pdfium()
        && (!Bitmap::is_valid_image_size(grid_width, grid_height)
            || !Bitmap::is_valid_image_size(region_info.width, region_info.height))
    {
        bail!(OverflowError::GridDimension);
    }

    // TODO: Check grid offsets as well.

    let grid_horizontal_offset = reader.read_i32().ok_or(ParseError::UnexpectedEof)?;
    let grid_vertical_offset = reader.read_i32().ok_or(ParseError::UnexpectedEof)?;

    let grid_position_and_size = HalftoneGridPositionAndSize {
        width: grid_width,
        height: grid_height,
        horizontal_offset: grid_horizontal_offset,
        vertical_offset: grid_vertical_offset,
    };

    let grid_x_vector = reader.read_u16().ok_or(ParseError::UnexpectedEof)?;
    let grid_y_vector = reader.read_u16().ok_or(ParseError::UnexpectedEof)?;

    let grid_vector = HalftoneGridVector {
        x_vector: grid_x_vector,
        y_vector: grid_y_vector,
    };

    let data = reader.tail().ok_or(ParseError::UnexpectedEof)?;

    Ok(HalftoneRegionHeader {
        pdfium: reader.is_pdfium(),
        region_info,
        flags,
        grid_position_and_size,
        grid_vector,
        data,
    })
}

/// Parsed halftone region segment flags (7.4.5.1.1).
#[derive(Debug, Clone)]
pub(crate) struct HalftoneRegionFlags {
    pub(crate) mmr: bool,
    pub(crate) template: Template,
    pub(crate) enable_skip: bool,
    pub(crate) combination_operator: CombinationOperator,
    pub(crate) initial_pixel_color: bool,
}

/// Halftone grid position and size (7.4.5.1.2).
#[derive(Debug, Clone)]
pub(crate) struct HalftoneGridPositionAndSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) horizontal_offset: i32,
    pub(crate) vertical_offset: i32,
}

/// Halftone grid vector (7.4.5.1.3).
#[derive(Debug, Clone)]
pub(crate) struct HalftoneGridVector {
    /// `HRX` - 256 times the horizontal coordinate of the halftone grid vector.
    pub(crate) x_vector: u16,
    /// `HRY` - 256 times the vertical coordinate of the halftone grid vector.
    pub(crate) y_vector: u16,
}

/// Parsed halftone region segment header (7.4.5.1).
#[derive(Debug, Clone)]
pub(crate) struct HalftoneRegionHeader<'a> {
    pub(crate) pdfium: bool,
    pub(crate) region_info: RegionSegmentInfo,
    pub(crate) flags: HalftoneRegionFlags,
    pub(crate) grid_position_and_size: HalftoneGridPositionAndSize,
    pub(crate) grid_vector: HalftoneGridVector,
    pub(crate) data: &'a [u8],
}

struct GridCoords {
    x: i64,
    y: i64,
    row_x: i64,
    row_y: i64,
    hrx: i64,
    hry: i64,
}

impl GridCoords {
    fn new(grid: &HalftoneGridPositionAndSize, vector: &HalftoneGridVector) -> Self {
        Self {
            x: grid.horizontal_offset as i64,
            y: grid.vertical_offset as i64,
            row_x: grid.horizontal_offset as i64,
            row_y: grid.vertical_offset as i64,
            hrx: vector.x_vector as i64,
            hry: vector.y_vector as i64,
        }
    }

    #[inline]
    fn get(&self) -> (i32, i32) {
        ((self.x >> 8) as i32, (self.y >> 8) as i32)
    }

    #[inline]
    fn advance_col(&mut self) {
        self.x += self.hrx;
        self.y -= self.hry;
    }

    #[inline]
    fn advance_row(&mut self) {
        self.row_x += self.hry;
        self.row_y += self.hrx;
        self.x = self.row_x;
        self.y = self.row_y;
    }
}

/// Compute the HSKIP bitmap (6.6.5.1).
fn compute_skip_bitmap(
    header: &HalftoneRegionHeader<'_>,
    pattern_dict: &PatternDictionary,
) -> Result<Vec<u32>> {
    let grid = &header.grid_position_and_size;
    let pattern_width = pattern_dict.pattern_width as i32;
    let pattern_height = pattern_dict.pattern_height as i32;
    let region_width = header.region_info.width as i32;
    let region_height = header.region_info.height as i32;

    let stride = grid.width.div_ceil(32);
    let len = stride
        .checked_mul(grid.height)
        .ok_or(OverflowError::BitmapDimension)? as usize;
    let mut hskip = vec![0_u32; len];
    let mut coords = GridCoords::new(grid, &header.grid_vector);

    for m_g in 0..grid.height {
        for n_g in 0..grid.width {
            let (x, y) = coords.get();

            let skip = (x + pattern_width <= 0)
                || (x >= region_width)
                || (y + pattern_height <= 0)
                || (y >= region_height);

            if skip {
                let word_idx = (m_g * stride + n_g / 32) as usize;
                let bit_pos = 31 - (n_g % 32);
                hskip[word_idx] |= 1 << bit_pos;
            }

            coords.advance_col();
        }
        coords.advance_row();
    }

    Ok(hskip)
}

/// Render patterns into the target region (6.6.5.2).
fn render_patterns(
    region: &mut Bitmap,
    gi: &GrayScaleImage,
    header: &HalftoneRegionHeader<'_>,
    pattern_dict: &PatternDictionary,
) -> Result<()> {
    let grid = &header.grid_position_and_size;
    let op = header.flags.combination_operator;
    let pw = pattern_dict.pattern_width;
    let ph = pattern_dict.pattern_height;
    let region_w = region.width as i32;
    let region_h = region.height as i32;
    let stride = region.stride;
    let num_patterns = pattern_dict.patterns.len();

    // The original pattern mask has 1s in the top `pw` bits.
    let original_mask = if pw >= WORD_BITS {
        !0_u32
    } else {
        !0_u32 << (WORD_BITS - pw)
    };

    macro_rules! blit_rows {
        ($data:expr, $shifted:expr, $yu:expr, $first_word:expr, $two:expr,
         |$d0:ident, $s0:ident| $op0:expr, |$d1:ident, $s1:ident| $op1:expr) => {
            if $two {
                for row in 0..ph {
                    let base = (($yu + row) * stride + $first_word) as usize;
                    let si = row as usize * 2;
                    let $d0 = $data[base];
                    let $s0 = $shifted[si];
                    $data[base] = $op0;
                    let $d1 = $data[base + 1];
                    let $s1 = $shifted[si + 1];
                    $data[base + 1] = $op1;
                }
            } else {
                for row in 0..ph {
                    let base = (($yu + row) * stride + $first_word) as usize;
                    let $d0 = $data[base];
                    let $s0 = $shifted[row as usize * 2];
                    $data[base] = $op0;
                }
            }
        };
    }

    let mut coords = GridCoords::new(grid, &header.grid_vector);
    let mut gi_idx = 0;

    for mg in 0..grid.height {
        for ng in 0..grid.width {
            let (x, y) = coords.get();

            let pattern_index = if header.pdfium {
                (gi.value_at(gi_idx, ng, mg) as usize).min(num_patterns - 1)
            } else {
                gi.value_at(gi_idx, ng, mg) as usize
            };
            gi_idx += 1;

            if pattern_index >= num_patterns {
                bail!(RegionError::GrayScaleOutOfRange);
            }

            // Check whether we can use the blit fast path.
            let shifted =
                if x >= 0 && y >= 0 && x + pw as i32 <= region_w && y + ph as i32 <= region_h {
                    pattern_dict.shifted_pattern(pattern_index, x as u32 % WORD_BITS)
                } else {
                    None
                };

            if let Some(shifted) = shifted {
                let xu = x as u32;
                let yu = y as u32;
                let first_word = xu / WORD_BITS;
                let offset = xu % WORD_BITS;
                let two = offset != 0 && first_word + 1 < stride;

                // TODO: It seems our current test suite only triggers for
                // `Or`.

                match op {
                    CombinationOperator::Or => {
                        blit_rows!(
                            region.data,
                            shifted,
                            yu,
                            first_word,
                            two,
                            |d, s| d | s,
                            |d, s| d | s
                        );
                    }
                    CombinationOperator::Xor => {
                        blit_rows!(
                            region.data,
                            shifted,
                            yu,
                            first_word,
                            two,
                            |d, s| d ^ s,
                            |d, s| d ^ s
                        );
                    }
                    CombinationOperator::And => {
                        let inv_mask0 = !(original_mask >> offset);
                        let inv_mask1 = if offset == 0 {
                            !0
                        } else {
                            !(original_mask << (WORD_BITS - offset))
                        };
                        blit_rows!(
                            region.data,
                            shifted,
                            yu,
                            first_word,
                            two,
                            |d, s| d & (s | inv_mask0),
                            |d, s| d & (s | inv_mask1)
                        );
                    }
                    CombinationOperator::Replace => {
                        let inv_mask0 = !(original_mask >> offset);
                        let inv_mask1 = if offset == 0 {
                            !0
                        } else {
                            !(original_mask << (WORD_BITS - offset))
                        };
                        blit_rows!(
                            region.data,
                            shifted,
                            yu,
                            first_word,
                            two,
                            |d, s| (d & inv_mask0) | s,
                            |d, s| (d & inv_mask1) | s
                        );
                    }
                    CombinationOperator::Xnor => {
                        let mask0 = original_mask >> offset;
                        let mask1 = if offset == 0 {
                            0
                        } else {
                            original_mask << (WORD_BITS - offset)
                        };
                        let inv_mask0 = !mask0;
                        let inv_mask1 = !mask1;
                        blit_rows!(
                            region.data,
                            shifted,
                            yu,
                            first_word,
                            two,
                            |d, s| (d & inv_mask0) | (!(d ^ s) & mask0),
                            |d, s| (d & inv_mask1) | (!(d ^ s) & mask1)
                        );
                    }
                }
            } else {
                let pattern = &pattern_dict.patterns[pattern_index];
                region.combine(pattern, x, y, op);
            }

            coords.advance_col();
        }
        coords.advance_row();
    }

    Ok(())
}
