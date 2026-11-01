//! Decoding JPEG2000 code streams.
//!
//! This is the "core" module of the crate that orchestrates all
//! stages in such a way that a given codestream is decoded into its
//! component channels.

use alloc::boxed::Box;
use alloc::vec::Vec;

use super::bitplane::{BitPlaneDecodeBuffers, BitPlaneDecodeContext};
use super::build::{CodeBlock, Decomposition, Layer, Precinct, Segment, SubBand, SubBandType};
use super::codestream::{
    ComponentInfo, Header, ProgressionOrder, QuantizationStyle, WaveletTransform,
};
use super::idwt::IDWTOutput;
use super::progression::{
    IteratorInput, ProgressionData, component_position_resolution_layer_progression,
    layer_resolution_component_position_progression,
    position_component_resolution_layer_progression,
    resolution_layer_component_position_progression,
    resolution_position_component_layer_progression,
};
use super::tag_tree::TagNode;
use super::tile::{ComponentTile, ResolutionTile, Tile};
use super::{ComponentData, bitplane, build, idwt, mct, segment, tile};
use crate::error::{DecodingError, Result, TileError, ValidationError, bail};
use crate::j2c::segment::MAX_BITPLANE_COUNT;
use crate::math::SimdBuffer;
use crate::reader::BitReader;
use core::ops::Range;

pub(crate) fn decode_pdfium<'a>(
    data: &'a [u8],
    header: &'a Header<'a>,
) -> Result<(Vec<ComponentData>, usize)> {
    let mut ctx = DecoderContext {
        pdfium: true,
        ..DecoderContext::default()
    };
    decode(data, header, &mut ctx)?;
    Ok((core::mem::take(&mut ctx.channel_data), ctx.pdfium_data_end))
}

pub(crate) fn decode<'a>(
    data: &'a [u8],
    header: &'a Header<'a>,
    ctx: &mut DecoderContext<'a>,
) -> Result<()> {
    let mut reader = BitReader::new(data);
    let tiles = tile::parse(&mut reader, header, ctx.pdfium)?;

    if tiles.is_empty() {
        bail!(TileError::Invalid);
    }

    ctx.reset(header, &tiles[0])?;
    ctx.pdfium_data_end = reader.offset();

    let single_tile = ctx.pdfium && super::pdfium::single_tile_buffer(header);
    let mut decoded_tile = false;
    let mut tile_order: Vec<_> = tiles.iter().collect();
    if ctx.pdfium {
        tile_order.sort_by_key(|tile| tile.pdfium_decode_order);
    }
    let mut ppm = header.pdfium_ppm.as_deref().map(BitReader::new);
    for tile in tile_order {
        if ctx.pdfium {
            if tile
                .component_infos
                .iter()
                .any(|info| info.num_resolution_levels() <= header.skipped_resolution_levels)
            {
                bail!(ValidationError::TooManySkippedResolutionLevels);
            }
            if !tile.pdfium_decode {
                continue;
            }
        }
        trace!(
            "tile {} rect [{},{} {}x{}]",
            tile.idx,
            tile.rect.x0,
            tile.rect.y0,
            tile.rect.width(),
            tile.rect.height(),
        );

        let progression_iterator: Box<dyn Iterator<Item = ProgressionData>> = if ctx.pdfium {
            super::pdfium_progression::iterator(tile)?
        } else {
            let iter_input = IteratorInput::new(tile);
            match tile.progression_order {
                ProgressionOrder::Unknown => bail!(DecodingError::InvalidProgressionIterator),
                ProgressionOrder::LayerResolutionComponentPosition => {
                    Box::new(layer_resolution_component_position_progression(iter_input))
                }
                ProgressionOrder::ResolutionLayerComponentPosition => {
                    Box::new(resolution_layer_component_position_progression(iter_input))
                }
                ProgressionOrder::ResolutionPositionComponentLayer => Box::new(
                    resolution_position_component_layer_progression(iter_input)
                        .ok_or(DecodingError::InvalidProgressionIterator)?,
                ),
                ProgressionOrder::PositionComponentResolutionLayer => Box::new(
                    position_component_resolution_layer_progression(iter_input)
                        .ok_or(DecodingError::InvalidProgressionIterator)?,
                ),
                ProgressionOrder::ComponentPositionResolutionLayer => Box::new(
                    component_position_resolution_layer_progression(iter_input)
                        .ok_or(DecodingError::InvalidProgressionIterator)?,
                ),
            }
        };

        let channels = if ctx.pdfium && !single_tile {
            super::pdfium::prepare_tile(header, tile, &mut ctx.pdfium_tile_channels)?;
            &mut ctx.pdfium_tile_channels
        } else {
            &mut ctx.channel_data
        };
        decode_tile(
            tile,
            header,
            progression_iterator,
            &mut ctx.tile_decode_context,
            channels,
            &mut ctx.storage,
            ctx.pdfium,
            &mut ppm,
        )?;
        if ctx.pdfium {
            super::pdfium::finish_tile(header, tile, &ctx.storage.pdfium_resno_decoded, channels)?;
            if !single_tile {
                super::pdfium::copy_tile(
                    header,
                    tile,
                    &ctx.storage.pdfium_resno_decoded,
                    &ctx.pdfium_tile_channels,
                    &mut ctx.channel_data,
                )?;
            }
        }
        decoded_tile = true;
    }

    // Note that this assumes that either all tiles have MCT or none of them.
    // In theory, only some could have it... But hopefully no such cursed
    // images exist!
    if !ctx.pdfium && tiles[0].mct {
        mct::apply_inverse(&mut ctx.channel_data, &tiles[0].component_infos, header)?;
    }

    if ctx.pdfium && !decoded_tile {
        bail!(TileError::Invalid);
    }
    if !ctx.pdfium {
        apply_sign_shift(&mut ctx.channel_data, &header.component_infos);
    }

    Ok(())
}

/// A decoder context for decoding JPEG2000 images.
#[derive(Default)]
pub struct DecoderContext<'a> {
    pdfium: bool,
    pdfium_data_end: usize,
    pdfium_tile_channels: Vec<ComponentData>,
    tile_decode_context: TileDecodeContext,
    /// The raw, decoded samples for each channel.
    pub(crate) channel_data: Vec<ComponentData>,
    storage: DecompositionStorage<'a>,
}

impl DecoderContext<'_> {
    fn reset(&mut self, header: &Header<'_>, initial_tile: &Tile<'_>) -> Result<()> {
        self.tile_decode_context.reset();
        self.storage.reset();

        self.channel_data.clear();
        let default_sample_count = if self.pdfium {
            0
        } else {
            (header.size_data.image_width() as usize)
                .checked_mul(header.size_data.image_height() as usize)
                .ok_or(ValidationError::ImageTooLarge)?
        };
        // TODO: SIMD Buffers should be reused across runs!
        for info in &initial_tile.component_infos {
            let sample_count = if self.pdfium {
                let rect = super::pdfium::component_rect(header, &info.size_info)?;
                (rect.width() as usize)
                    .checked_mul(rect.height() as usize)
                    .ok_or(ValidationError::ImageTooLarge)?
            } else {
                default_sample_count
            };
            self.channel_data.push(ComponentData {
                container: SimdBuffer::zeros(sample_count),
                bit_depth: info.size_info.precision,
            });
        }

        Ok(())
    }
}

fn decode_tile<'a, 'b>(
    tile: &'b Tile<'a>,
    header: &Header<'_>,
    progression_iterator: Box<dyn Iterator<Item = ProgressionData> + '_>,
    tile_ctx: &mut TileDecodeContext,
    channel_data: &mut [ComponentData],
    storage: &mut DecompositionStorage<'a>,
    pdfium: bool,
    ppm: &mut Option<BitReader<'_>>,
) -> Result<()> {
    storage.reset();

    // This is the method that orchestrates all steps.

    // First, we build the decompositions, including their sub-bands, precincts
    // and code blocks.
    build::build(tile, storage, header.skipped_resolution_levels)?;
    // Next, we parse the layers/segments for each code block.
    if pdfium {
        segment::parse_pdfium(
            tile,
            header.skipped_resolution_levels,
            progression_iterator,
            ppm,
            storage,
        )?;
    } else {
        segment::parse(tile, progression_iterator, header, storage)?;
    }

    // Interleave bitplane decoding, IDWT, and storage for each component tile
    // so we can reuse the coefficient and IDWT buffers.
    for (idx, component_info) in header.component_infos.iter().enumerate() {
        if idx > 0 {
            let count = storage.tile_decompositions[idx].coefficient_count;
            storage.coefficients[..count].fill(0.0);
        }
        // We then decode the bitplanes of each code block, yielding the
        // (possibly dequantized) coefficients of each code block.
        decode_component_tile_bit_planes(idx, tile, tile_ctx, storage, header, pdfium)?;
        // Next, we apply the inverse discrete wavelet transform.
        idwt::apply(
            storage,
            tile_ctx,
            idx,
            header,
            if pdfium {
                tile.component_infos[idx].wavelet_transform()
            } else {
                component_info.wavelet_transform()
            },
            pdfium,
        );
        // Finally, we store the raw samples for the tile area in the correct
        // location. Note that in case we have MCT, we are not applying it yet.
        // It will be applied in the very end once all tiles have been processed.
        // The reason we do this is that applying MCT requires access to the
        // data from _all_ components. If we didn't defer this until the end
        // we would have to collect the IDWT outputs of all components before
        // applying it. By not applying MCT here, we can get away with doing
        // IDWT and store on a per-component basis. Thus, we only need to
        // store one IDWT output at a time, allowing for better reuse of
        // allocations.
        if pdfium {
            super::pdfium::store(
                header,
                tile,
                tile_ctx,
                &mut channel_data[idx],
                &tile.component_infos[idx],
            )?;
        } else {
            store(
                tile,
                header,
                tile_ctx,
                &mut channel_data[idx],
                component_info,
            );
        }
    }

    Ok(())
}

/// All decompositions for a single tile.
#[derive(Clone)]
pub(crate) struct TileDecompositions {
    pub(crate) first_ll_sub_band: usize,
    pub(crate) decompositions: Range<usize>,
    pub(crate) coefficient_count: usize,
}

impl TileDecompositions {
    pub(crate) fn sub_band_iter(
        &self,
        resolution: u8,
        decompositions: &[Decomposition],
    ) -> SubBandIter {
        let indices = if resolution == 0 {
            [
                self.first_ll_sub_band,
                self.first_ll_sub_band,
                self.first_ll_sub_band,
            ]
        } else {
            decompositions[self.decompositions.clone()][resolution as usize - 1].sub_bands
        };

        SubBandIter {
            next_idx: 0,
            indices,
            resolution,
        }
    }
}

#[derive(Clone)]
pub(crate) struct SubBandIter {
    resolution: u8,
    next_idx: usize,
    indices: [usize; 3],
}

impl Iterator for SubBandIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let value = if self.resolution == 0 {
            if self.next_idx > 0 {
                None
            } else {
                Some(self.indices[0])
            }
        } else if self.next_idx >= self.indices.len() {
            None
        } else {
            Some(self.indices[self.next_idx])
        };

        self.next_idx += 1;

        value
    }
}

/// A buffer so that we can reuse allocations for layers/code blocks/etc.
/// across different tiles.
#[derive(Default)]
pub(crate) struct DecompositionStorage<'a> {
    pub(crate) segments: Vec<Segment<'a>>,
    pub(crate) pdfium_body: Vec<u8>,
    pub(crate) pdfium_resno_decoded: Vec<u8>,
    pub(crate) layers: Vec<Layer>,
    pub(crate) code_blocks: Vec<CodeBlock>,
    pub(crate) precincts: Vec<Precinct>,
    pub(crate) tag_tree_nodes: Vec<TagNode>,
    pub(crate) coefficients: Vec<f32>,
    pub(crate) sub_bands: Vec<SubBand>,
    pub(crate) decompositions: Vec<Decomposition>,
    pub(crate) tile_decompositions: Vec<TileDecompositions>,
}

impl DecompositionStorage<'_> {
    fn reset(&mut self) {
        self.segments.clear();
        self.pdfium_body.clear();
        self.pdfium_resno_decoded.clear();
        self.layers.clear();
        self.code_blocks.clear();
        // No need to clear the coefficients, as they will be resized
        // and then overridden.
        // self.coefficients.clear();
        self.precincts.clear();
        self.sub_bands.clear();
        self.decompositions.clear();
        self.tile_decompositions.clear();
        self.tag_tree_nodes.clear();
    }
}

/// A reusable context used during the decoding of a single tile.
///
/// Some of the fields are temporary in nature and reset after moving on to the
/// next tile, some contain global state.
#[derive(Default)]
pub(crate) struct TileDecodeContext {
    /// A reusable buffer for the IDWT output.
    pub(crate) idwt_output: IDWTOutput,
    /// A scratch buffer used during IDWT.
    pub(crate) idwt_scratch_buffer: Vec<f32>,
    /// A reusable context for decoding code blocks.
    pub(crate) bit_plane_decode_context: BitPlaneDecodeContext,
    pub(crate) ht_decode_context: super::ht::Context,
    /// Reusable buffers for decoding bitplanes.
    pub(crate) bit_plane_decode_buffers: BitPlaneDecodeBuffers,
}

impl TileDecodeContext {
    fn reset(&mut self) {
        // This method doesn't do anything, just keeping it there in case
        // it's needed in the future.
        // Bitplane decode context and buffers will be reset in the
        // corresponding methods. IDWT output and scratch buffer will be
        // overridden on demand, so those don't need to be reset either.
    }
}

fn decode_component_tile_bit_planes<'a>(
    component_idx: usize,
    tile: &Tile<'a>,
    tile_ctx: &mut TileDecodeContext,
    storage: &mut DecompositionStorage<'a>,
    header: &Header<'_>,
    pdfium: bool,
) -> Result<()> {
    let component_info = &tile.component_infos[component_idx];

    // Only decode the resolution levels we actually care about.
    for resolution in 0..component_info.num_resolution_levels() - header.skipped_resolution_levels {
        let tile_composition = &storage.tile_decompositions[component_idx];
        let sub_band_iter = tile_composition.sub_band_iter(resolution, &storage.decompositions);

        for sub_band_idx in sub_band_iter {
            decode_sub_band_bitplanes(
                sub_band_idx,
                resolution,
                component_info,
                tile_ctx,
                storage,
                header,
                pdfium,
            )?;
        }
    }

    Ok(())
}

fn decode_sub_band_bitplanes(
    sub_band_idx: usize,
    resolution: u8,
    component_info: &ComponentInfo,
    tile_ctx: &mut TileDecodeContext,
    storage: &mut DecompositionStorage<'_>,
    header: &Header<'_>,
    pdfium: bool,
) -> Result<()> {
    let sub_band = &storage.sub_bands[sub_band_idx];

    let quantised = !matches!(
        component_info.quantization_info.quantization_style,
        QuantizationStyle::NoQuantization | QuantizationStyle::Native(0)
    );
    let irreversible = component_info.wavelet_transform() == WaveletTransform::Irreversible97;
    let dequantization_step = {
        if (pdfium && !irreversible) || (!pdfium && !quantised) {
            1.0
        } else {
            let (exponent, mantissa) =
                component_info.exponent_mantissa(sub_band.sub_band_type, resolution)?;

            let r_b = {
                let log_gain = match sub_band.sub_band_type {
                    SubBandType::LowLow => 0,
                    SubBandType::LowHigh => 1,
                    SubBandType::HighLow => 1,
                    SubBandType::HighHigh => 2,
                };

                // Native decoding pairs zero band gain with two_invK in
                // irreversible IDWT (BUG_WEIRD_TWO_INVK in tcd.c/dwt.c).
                component_info.size_info.precision as u16 + if pdfium { 0 } else { log_gain }
            };

            crate::math::pow2i(r_b as i32 - exponent as i32) * (1.0 + (mantissa as f32) / 2048.0)
        }
    };

    let (exponent, _) = component_info.exponent_mantissa(sub_band.sub_band_type, resolution)?;
    let raw_bitplanes =
        i32::from(component_info.quantization_info.guard_bits) + i32::from(exponent) - 1;
    let num_bitplanes = if pdfium {
        0
    } else {
        let count = u8::try_from(raw_bitplanes).map_err(|_| DecodingError::InvalidBitplaneCount)?;
        if count > MAX_BITPLANE_COUNT {
            bail!(DecodingError::TooManyBitplanes);
        }
        count
    };

    for precinct in sub_band
        .precincts
        .clone()
        .map(|idx| &storage.precincts[idx])
    {
        for code_block in precinct
            .code_blocks
            .clone()
            .map(|idx| &storage.code_blocks[idx])
        {
            if component_info.code_block_style().high_throughput {
                let combined = &mut tile_ctx.bit_plane_decode_buffers.combined_layers;
                combined.clear();
                let mut lengths = [0usize; 2];
                let mut passes = [0u32; 2];
                let mut has_chunks = false;
                for layer in &storage.layers[code_block.layers.clone()] {
                    if let Some(segments) = &layer.segments {
                        for segment in &storage.segments[segments.clone()] {
                            has_chunks = true;
                            let data = if let Some(range) = &segment.pdfium_range {
                                storage
                                    .pdfium_body
                                    .get(range.clone())
                                    .ok_or(DecodingError::CodeBlockDecodeFailure)?
                            } else {
                                segment.data
                            };
                            combined
                                .try_reserve(data.len())
                                .map_err(|_| DecodingError::CodeBlockDecodeFailure)?;
                            combined.extend_from_slice(data);
                            if segment.pdfium_index < 2 {
                                let index = segment.pdfium_index as usize;
                                lengths[index] = lengths[index]
                                    .checked_add(data.len())
                                    .ok_or(DecodingError::CodeBlockDecodeFailure)?;
                                passes[index] = passes[index]
                                    .checked_add(u32::from(segment.coding_pases))
                                    .ok_or(DecodingError::CodeBlockDecodeFailure)?;
                            }
                        }
                    }
                }
                let mb = if code_block.has_been_included {
                    raw_bitplanes as u32
                } else {
                    0
                };
                let block = super::ht::Block {
                    width: code_block.rect.width() as usize,
                    height: code_block.rect.height() as usize,
                    mb,
                    numbps: mb.wrapping_sub(code_block.pdfium_missing_bitplanes),
                    has_chunks,
                    num_passes: passes[0]
                        .checked_add(passes[1])
                        .ok_or(DecodingError::CodeBlockDecodeFailure)?,
                    lengths1: lengths[0],
                    lengths2: lengths[1],
                    roi_shift: component_info.pdfium_roi_shift,
                    stripe_causal: component_info.code_block_style().vertically_causal_context,
                };
                tile_ctx
                    .ht_decode_context
                    .decode(combined, &block)
                    .ok_or(DecodingError::CodeBlockDecodeFailure)?;
                let x = (code_block.rect.x0 - sub_band.rect.x0) as usize;
                let y = (code_block.rect.y0 - sub_band.rect.y0) as usize;
                let stride = sub_band.rect.width() as usize;
                let output = &mut storage.coefficients[sub_band.coefficients.clone()];
                if block.width != 0 {
                    for (row, samples) in tile_ctx
                        .ht_decode_context
                        .coefficients
                        .chunks_exact(block.width)
                        .enumerate()
                    {
                        let output = &mut output[(y + row) * stride + x..][..block.width];
                        for (output, &word) in output.iter_mut().zip(samples) {
                            let magnitude = (word & 0x7fffffff) as i32;
                            let signed = if word & 0x80000000 != 0 {
                                -magnitude
                            } else {
                                magnitude
                            };
                            *output = if irreversible {
                                signed as f32 * 0.5
                            } else {
                                (signed / 2) as f32
                            };
                            *output *= dequantization_step;
                        }
                    }
                }
                continue;
            }
            let roi_shift = component_info.pdfium_roi_shift;
            let native_bitplanes = if pdfium {
                // Native stores Mb/numbps as signed/unsigned 32-bit fields.
                // Never-included blocks retain initialized numbps=0. A nonpositive
                // final bpno performs no passes; >=31 fails before chunk lookup.
                let declared = if code_block.has_been_included {
                    raw_bitplanes.wrapping_sub(code_block.pdfium_missing_bitplanes as i32)
                } else {
                    0
                };
                let actual = declared.wrapping_add(i32::from(roi_shift));
                if actual >= 31 {
                    bail!(DecodingError::TooManyBitplanes);
                }
                Some(actual.max(0) as u8)
            } else {
                None
            };
            bitplane::decode(
                code_block,
                sub_band.sub_band_type,
                num_bitplanes,
                &component_info.coding_style.parameters.code_block_style,
                tile_ctx,
                storage,
                header.strict,
                roi_shift,
                native_bitplanes,
            )?;

            // Turn the signs and magnitudes into singular coefficients and
            // copy them into the sub-band.

            let x_offset = code_block.rect.x0 - sub_band.rect.x0;
            let y_offset = code_block.rect.y0 - sub_band.rect.y0;

            let base_store = &mut storage.coefficients[sub_band.coefficients.clone()];
            let mut base_idx = (y_offset * sub_band.rect.width()) as usize + x_offset as usize;

            for (coefficients, coefficient_states) in
                tile_ctx.bit_plane_decode_context.coefficient_rows()
            {
                let out_row = &mut base_store[base_idx..];

                for ((output, coefficient), coefficient_state) in
                    out_row.iter_mut().zip(coefficients).zip(coefficient_states)
                {
                    *output = if pdfium && roi_shift != 0 {
                        coefficient.reconstructed_pdfium_roi(
                            coefficient_state,
                            irreversible,
                            roi_shift,
                        )
                    } else {
                        coefficient.reconstructed(coefficient_state, irreversible)
                    };
                    *output *= dequantization_step;
                }

                base_idx += sub_band.rect.width() as usize;
            }
        }
    }

    Ok(())
}

fn apply_sign_shift(channel_data: &mut [ComponentData], component_infos: &[ComponentInfo]) {
    use crate::math::{Level, dispatch, f32x8};

    for (channel, component_info) in channel_data.iter_mut().zip(component_infos.iter()) {
        let offset = (1_u32 << (component_info.size_info.precision - 1)) as f32;
        dispatch!(Level::new(), simd => {
            let offset_v = f32x8::splat(simd, offset);
            for chunk in channel.container.chunks_exact_mut(8) {
                let v = f32x8::from_slice(simd, chunk);
                (v + offset_v).store(chunk);
            }
        });
    }
}

fn store<'a>(
    tile: &'a Tile<'a>,
    header: &Header<'_>,
    tile_ctx: &mut TileDecodeContext,
    channel_data: &mut ComponentData,
    component_info: &ComponentInfo,
) {
    let idwt_output = &mut tile_ctx.idwt_output;

    let component_tile = ComponentTile::new(tile, component_info);
    let resolution_tile = ResolutionTile::new(
        component_tile,
        component_info.num_resolution_levels() - 1 - header.skipped_resolution_levels,
    );

    let (scale_x, scale_y) = (
        component_info.size_info.horizontal_resolution,
        component_info.size_info.vertical_resolution,
    );

    let (image_x_offset, image_y_offset) = (
        header.size_data.image_area_x_offset,
        header.size_data.image_area_y_offset,
    );

    if scale_x == 1 && scale_y == 1 {
        // If no sub-sampling, use a fast path where we copy rows of coefficients
        // at once.

        // The rect of the IDWT output corresponds to the rect of the highest
        // decomposition level of the tile, which is usually not 1:1 aligned
        // with the actual tile rectangle. We also need to account for the
        // offset of the reference grid.

        let skip_x = image_x_offset.saturating_sub(idwt_output.rect.x0);
        let skip_y = image_y_offset.saturating_sub(idwt_output.rect.y0);

        let input_row_iter = idwt_output
            .coefficients
            .chunks_exact(idwt_output.rect.width() as usize)
            .skip(skip_y as usize)
            .take(idwt_output.rect.height() as usize);

        let output_row_iter = channel_data
            .container
            .chunks_exact_mut(header.size_data.image_width() as usize)
            .skip(resolution_tile.rect.y0.saturating_sub(image_y_offset) as usize);

        for (input_row, output_row) in input_row_iter.zip(output_row_iter) {
            let input_row = &input_row[skip_x as usize..];
            let output_row = &mut output_row
                [resolution_tile.rect.x0.saturating_sub(image_x_offset) as usize..]
                [..input_row.len()];

            output_row.copy_from_slice(input_row);
        }
    } else {
        let image_width = header.size_data.image_width();
        let image_height = header.size_data.image_height();

        let x_shrink_factor = header.size_data.x_shrink_factor;
        let y_shrink_factor = header.size_data.y_shrink_factor;

        let x_offset = header
            .size_data
            .image_area_x_offset
            .div_ceil(x_shrink_factor);
        let y_offset = header
            .size_data
            .image_area_y_offset
            .div_ceil(y_shrink_factor);

        // Otherwise, copy sample by sample.
        for y in resolution_tile.rect.y0..resolution_tile.rect.y1 {
            let relative_y = (y - component_tile.rect.y0) as usize;
            let reference_grid_y = (scale_y as u32 * y) / y_shrink_factor;

            for x in resolution_tile.rect.x0..resolution_tile.rect.x1 {
                let relative_x = (x - component_tile.rect.x0) as usize;
                let reference_grid_x = (scale_x as u32 * x) / x_shrink_factor;

                let sample = idwt_output.coefficients
                    [relative_y * idwt_output.rect.width() as usize + relative_x];

                for x_position in u32::max(reference_grid_x, x_offset)
                    ..u32::min(reference_grid_x + scale_x as u32, image_width + x_offset)
                {
                    for y_position in u32::max(reference_grid_y, y_offset)
                        ..u32::min(reference_grid_y + scale_y as u32, image_height + y_offset)
                    {
                        let pos = (y_position - y_offset) as usize * image_width as usize
                            + (x_position - x_offset) as usize;

                        channel_data.container[pos] = sample;
                    }
                }
            }
        }
    }
}
