//! Parsing of layers and their segments, as specified in Annex B.

use alloc::boxed::Box;

use super::build::Segment;
use super::codestream::markers::{EPH, SOP};
use super::codestream::{ComponentInfo, Header};
use super::decode::DecompositionStorage;
use super::progression::ProgressionData;
use super::tile::{Tile, TilePart};
use crate::error::{Result, TileError, bail};
use crate::reader::BitReader;

pub(crate) const MAX_BITPLANE_COUNT: u8 = 32;

// Native j2k.c concatenates tile-part bodies; t2.c traverses the expected
// packets with a separate global PPM or per-tile PPT cursor. Segment ranges
// index the owned native body so storage can reuse allocations without making
// self references. BSD-2-Clause: ../../LICENSE-OPENJPEG-J2K / -T2.
pub(crate) fn parse_pdfium(
    tile: &Tile<'_>,
    reduce: u8,
    progression: Box<dyn Iterator<Item = ProgressionData> + '_>,
    ppm: &mut Option<BitReader<'_>>,
    storage: &mut DecompositionStorage<'_>,
) -> Result<()> {
    let mut body = core::mem::take(&mut storage.pdfium_body);
    body.clear();
    let size = tile
        .tile_parts
        .iter()
        .try_fold(0u32, |size, part| {
            let length = u32::try_from(part.body_data()?.len()).ok()?;
            size.checked_add(length)
        })
        .filter(|&size| size <= u32::MAX - 2)
        .ok_or(TileError::Invalid)?;
    body.try_reserve_exact(size as usize)
        .map_err(|_| TileError::Invalid)?;
    for part in &tile.tile_parts {
        body.extend_from_slice(part.body_data().ok_or(TileError::Invalid)?);
    }
    let ppt = if tile.pdfium_ppt.is_empty() {
        None
    } else {
        Some(super::pdfium_packed::merge_ppt(&tile.pdfium_ppt).ok_or(TileError::Invalid)?)
    };
    // Copy only the borrowed slice/cursor scalars. Owning this local reader
    // lets its borrow shorten to the PPT/body lifetime without tying a mutable
    // reference to the global PPM reader to local storage.
    let mut packed = ppm
        .as_ref()
        .cloned()
        .or_else(|| ppt.as_deref().map(BitReader::new));
    let mut reader = BitReader::new(&body);
    storage
        .pdfium_resno_decoded
        .resize(tile.component_infos.len(), 0);
    let mut first_pass_failed = alloc::vec![true; tile.component_infos.len()];
    let mut poc = 0;
    let result = (|| {
        for packet in progression {
            if packet.pdfium_poc != poc {
                first_pass_failed.fill(true);
                poc = packet.pdfium_poc;
            }
            let component = usize::from(packet.component);
            let minimum = tile.component_infos[component].num_resolution_levels() - reduce;
            // opj_t2_decode_packets updates resolution even for empty packets.
            // A first skipped packet supplies the native zero-dimension fallback.
            let decoded = &mut storage.pdfium_resno_decoded[component];
            if packet.resolution < minimum {
                first_pass_failed[component] = false;
                *decoded = (*decoded).max(packet.resolution);
            } else if first_pass_failed[component] && *decoded == 0 {
                *decoded = minimum - 1;
            }
            let info = &tile.component_infos[usize::from(packet.component)];
            if info.coding_style.flags.may_use_sop_markers()
                && reader.tail()?.len() >= 6
                && reader.peek_marker() == Some(SOP)
            {
                reader.read_marker().ok()?;
                reader.skip_bytes(4)?;
            }
            let header = packed.as_mut().unwrap_or(&mut reader);
            header.begin_pdfium_packet();
            let present = header.read_bits_with_stuffing(1)? != 0;
            let sub_bands = storage.tile_decompositions[usize::from(packet.component)]
                .sub_band_iter(packet.resolution, &storage.decompositions);
            if present {
                for sub_band in sub_bands.clone() {
                    resolve_segments(sub_band, &packet, header, storage, info, true)?;
                }
            }
            header.end_pdfium_packet()?;
            if info.coding_style.flags.uses_eph_marker() && header.read_marker().ok()? != EPH {
                return None;
            }
            if !present {
                continue;
            }
            for sub_band in sub_bands {
                let sub_band = &storage.sub_bands[sub_band];
                let precinct =
                    storage.precincts[sub_band.precincts.clone()].get(packet.precinct as usize)?;
                for block in &storage.code_blocks[precinct.code_blocks.clone()] {
                    let layer =
                        &storage.layers[block.layers.clone()][usize::from(packet.layer_num)];
                    if let Some(segments) = &layer.segments {
                        for segment in &mut storage.segments[segments.clone()] {
                            let start = reader.offset();
                            reader.skip_bytes(segment.data_length as usize)?;
                            segment.pdfium_range = Some(start..reader.offset());
                        }
                    }
                }
            }
        }
        Some(())
    })();
    // Restore ownership even when parsing fails; no encoded buffer is cloned
    // to work around the simultaneous header/body/storage borrows.
    let packed_offset = packed.as_ref().map(BitReader::offset);
    drop(packed);
    storage.pdfium_body = body;
    if result.is_some() {
        if let (Some(global), Some(offset)) = (ppm, packed_offset) {
            global
                .skip_bytes(offset - global.offset())
                .ok_or(TileError::Invalid)?;
        }
    }
    result.ok_or(TileError::Invalid.into())
}

pub(crate) fn parse<'a, 'b>(
    tile: &'b Tile<'a>,
    mut progression_iterator: Box<dyn Iterator<Item = ProgressionData> + '_>,
    header: &Header<'_>,
    storage: &mut DecompositionStorage<'a>,
) -> Result<()> {
    for tile_part in &tile.tile_parts {
        if parse_inner(
            tile_part.clone(),
            &mut progression_iterator,
            &tile.component_infos,
            storage,
        )
        .is_none()
            && header.strict
        {
            bail!(TileError::Invalid);
        }
    }

    Ok(())
}

fn parse_inner<'a>(
    mut tile_part: TilePart<'a>,
    progression_iterator: &mut dyn Iterator<Item = ProgressionData>,
    component_infos: &[ComponentInfo],
    storage: &mut DecompositionStorage<'a>,
) -> Option<()> {
    while !tile_part.header().at_end() {
        let progression_data = progression_iterator.next()?;
        let resolution = progression_data.resolution;
        let component_info = &component_infos[progression_data.component as usize];
        let tile_decompositions =
            &mut storage.tile_decompositions[progression_data.component as usize];
        let sub_band_iter = tile_decompositions.sub_band_iter(resolution, &storage.decompositions);

        let body_reader = tile_part.body();

        if component_info.coding_style.flags.may_use_sop_markers()
            && body_reader.peek_marker() == Some(SOP)
        {
            body_reader.read_marker().ok()?;
            body_reader.skip_bytes(4)?;
        }

        let header_reader = tile_part.header();

        let zero_length = header_reader.read_bits_with_stuffing(1)? == 0;

        // B.10.3 Zero length packet
        // "The first bit in the packet header denotes whether the packet has a length of zero
        // (empty packet). The value 0 indicates a zero length; no code-blocks are included in this
        // case. The value 1 indicates a non-zero length."
        if !zero_length {
            for sub_band in sub_band_iter.clone() {
                resolve_segments(
                    sub_band,
                    &progression_data,
                    header_reader,
                    storage,
                    component_info,
                    false,
                )?;
            }
        }

        header_reader.align();

        if component_info.coding_style.flags.uses_eph_marker()
            && header_reader.read_marker().ok()? != EPH
        {
            return None;
        }

        // Now read the packet body.
        let body_reader = tile_part.body();

        if !zero_length {
            for sub_band in sub_band_iter {
                let sub_band = &mut storage.sub_bands[sub_band];
                let precinct = &mut storage.precincts[sub_band.precincts.clone()]
                    [progression_data.precinct as usize];
                let code_blocks = &mut storage.code_blocks[precinct.code_blocks.clone()];

                for code_block in code_blocks {
                    let layer = &mut storage.layers[code_block.layers.clone()]
                        [progression_data.layer_num as usize];

                    if let Some(segments) = layer.segments.clone() {
                        let segments = &mut storage.segments[segments.clone()];

                        for segment in segments {
                            segment.data = body_reader.read_bytes(segment.data_length as usize)?;
                        }
                    }
                }
            }
        }
    }

    Some(())
}

fn resolve_segments(
    sub_band_dx: usize,
    progression_data: &ProgressionData,
    reader: &mut BitReader<'_>,
    storage: &mut DecompositionStorage<'_>,
    component_info: &ComponentInfo,
    pdfium: bool,
) -> Option<()> {
    // We don't support more than 32-bit precision.
    const MAX_CODING_PASSES: u8 = 1 + 3 * (MAX_BITPLANE_COUNT - 1);

    let sub_band = &storage.sub_bands[sub_band_dx];
    let precincts = &mut storage.precincts[sub_band.precincts.clone()];
    let Some(precinct) = precincts.get_mut(progression_data.precinct as usize) else {
        // An invalid file could trigger this code path.
        warn!("progression data yielded invalid precinct index");

        return None;
    };
    let code_blocks = &mut storage.code_blocks[precinct.code_blocks.clone()];

    for code_block in code_blocks {
        // B.10.4 Code-block inclusion
        let is_included = if code_block.has_been_included {
            // "For code-blocks that have been included in a previous packet,
            // a single bit is used to represent the information, where a 1
            // means that the code-block is included in this layer and a 0 means
            // that it is not."
            reader.read_bits_with_stuffing(1)? == 1
        } else {
            // "For code-blocks that have not been previously included in any packet,
            // this information is signalled with a separate tag tree code for each precinct
            // as confined to a sub-band. The values in this tag tree are the number of the
            // layer in which the current code-block is first included. Although the exact
            // sequence of bits that represent the inclusion tag tree appears in the bit
            // stream, only the bits needed for determining whether the code-block is
            // included are placed in the packet header. If some of the tag tree is already
            // known from previous code-blocks or previous layers, it is not repeated.
            // Likewise, only as much of the tag tree as is needed to determine inclusion in
            // the current layer is included. If a code-block is not included until a later
            // layer, then only a partial tag tree is included at that point in the bit
            // stream."
            precinct.code_inclusion_tree.read(
                code_block.x_idx,
                code_block.y_idx,
                reader,
                progression_data.layer_num as u32 + 1,
                &mut storage.tag_tree_nodes,
            )? <= progression_data.layer_num as u32
        };

        trace!("code-block inclusion: {}", is_included);

        if !is_included {
            continue;
        }

        let layer =
            &mut storage.layers[code_block.layers.clone()][progression_data.layer_num as usize];

        let included_first_time = is_included && !code_block.has_been_included;

        // B.10.5 Zero bit-plane information
        // "If a code-block is included for the first time, the packet header contains
        // information identifying the actual number of bit-planes used to represent
        // coefficients from the code-block. The maximum number of bit-planes available
        // for the representation of coefficients in any sub-band, b, is given by Mb as
        // defined in Equation (E-2). In general, however, the
        // number of actual bit-planes for which coding passes are generated is Mb – P,
        // where the number of missing most significant bit-planes, P, may vary from
        // code-block to code-block; these missing bit-planes are all taken to be zero. The
        // value of P is coded in the packet header with a separate tag tree for every
        // precinct, in the same manner as the code block inclusion information."
        if included_first_time {
            let missing = precinct.zero_bitplane_tree.read(
                code_block.x_idx,
                code_block.y_idx,
                reader,
                u32::MAX,
                &mut storage.tag_tree_nodes,
            )?;
            code_block.pdfium_missing_bitplanes = missing;
            code_block.missing_bit_planes = missing as u8;
            trace!(
                "zero bit-plane information: {}",
                code_block.missing_bit_planes
            );
        }

        code_block.has_been_included |= is_included;

        // B.10.6 Number of coding passes
        // "The number of coding passes included in this packet from each code-block is
        // identified in the packet header using the codewords shown in Table B.4. This
        // table provides for the possibility of signalling up to 164 coding passes."
        let added_coding_passes = if reader.peak_bits_with_stuffing(9) == Some(0x1ff) {
            reader.read_bits_with_stuffing(9)?;
            reader.read_bits_with_stuffing(7)? + 37
        } else if reader.peak_bits_with_stuffing(4) == Some(0x0f) {
            reader.read_bits_with_stuffing(4)?;
            reader.read_bits_with_stuffing(5)? + 6
        } else if reader.peak_bits_with_stuffing(4) == Some(0b1110) {
            reader.read_bits_with_stuffing(4)?;
            5
        } else if reader.peak_bits_with_stuffing(4) == Some(0b1101) {
            reader.read_bits_with_stuffing(4)?;
            4
        } else if reader.peak_bits_with_stuffing(4) == Some(0b1100) {
            reader.read_bits_with_stuffing(4)?;
            3
        } else if reader.peak_bits_with_stuffing(2) == Some(0b10) {
            reader.read_bits_with_stuffing(2)?;
            2
        } else if reader.peak_bits_with_stuffing(1) == Some(0) {
            reader.read_bits_with_stuffing(1)?;
            1
        } else {
            return None;
        } as u8;

        trace!("number of coding passes: {}", added_coding_passes);

        let mut k = 0u32;

        while reader.read_bits_with_stuffing(1)? == 1 {
            k = k.checked_add(1u32)?;
        }

        code_block.l_block = code_block.l_block.checked_add(k)?;
        if code_block.l_block > 32 {
            return None;
        }

        if pdfium {
            // Native T2 segment state is independent of layers and uses full
            // pass counts. TERMALL/LAZY select maxpasses; HT has a separate
            // new-pass grouping rule. Do not apply the backend's 94-pass cap.
            let style = component_info.code_block_style();
            let next_max = |previous: Option<u32>| {
                if style.termination_on_each_pass {
                    1
                } else if style.selective_arithmetic_coding_bypass {
                    match previous {
                        None => 10,
                        Some(1 | 10) => 2,
                        _ => 1,
                    }
                } else {
                    109
                }
            };
            let mut state =
                code_block
                    .pdfium_segments
                    .unwrap_or(super::build::PdfiumSegmentState {
                        index: 0,
                        passes: 0,
                        max_passes: next_max(None),
                    });
            if state.passes == state.max_passes {
                state.index = state.index.checked_add(1)?;
                state.passes = 0;
                state.max_passes = next_max(Some(state.max_passes));
            }
            let start = storage.segments.len();
            let mut remaining = u32::from(added_coding_passes);
            loop {
                let passes = if style.high_throughput {
                    if state.index == 0 { 1 } else { remaining }
                } else {
                    remaining.min(state.max_passes.checked_sub(state.passes)?)
                };
                if passes == 0 {
                    return None;
                }
                let bits = code_block.l_block.checked_add(passes.ilog2())?;
                if bits > 32 {
                    return None;
                }
                storage.segments.push(Segment {
                    idx: 0,
                    coding_pases: passes as u8,
                    data_length: reader.read_bits_with_stuffing(bits as u8)?,
                    data: &[],
                    pdfium_range: None,
                    pdfium_index: state.index,
                });
                state.passes = state.passes.checked_add(passes)?;
                remaining -= passes;
                if remaining == 0 {
                    break;
                }
                state.index = state.index.checked_add(1)?;
                state.passes = 0;
                state.max_passes = next_max(Some(state.max_passes));
            }
            code_block.pdfium_segments = Some(state);
            code_block.pdfium_coding_passes = code_block
                .pdfium_coding_passes
                .checked_add(u32::from(added_coding_passes))?;
            layer.segments = Some(start..storage.segments.len());
            continue;
        }

        let previous_layers_passes = code_block.number_of_coding_passes;
        let cumulative_passes = previous_layers_passes.checked_add(added_coding_passes)?;

        if cumulative_passes > MAX_CODING_PASSES {
            return None;
        }

        let get_segment_idx = |pass_idx: u8| {
            if component_info.code_block_style().termination_on_each_pass {
                // If we terminate on each pass, the segment is just the index
                // of the pass.
                pass_idx
            } else if component_info
                .code_block_style()
                .selective_arithmetic_coding_bypass
            {
                // Use the formula derived from the table in the spec.
                segment_idx_for_bypass(pass_idx)
            } else {
                // If none of the above flags is activated, the number of
                // segments just corresponds to the number of layers.
                code_block.non_empty_layer_count
            }
        };

        let start = storage.segments.len();

        let mut push_segment = |segment: u8, coding_passes_for_segment: u8| {
            let length = {
                assert!(coding_passes_for_segment > 0);

                // "A codeword segment is the number of bytes contributed to a packet by a
                // code-block. The length of a codeword segment is represented by a binary number of length:
                // bits = Lblock + floor(log_2(coding passes added))
                // where Lblock is a code-block state variable. A separate Lblock is used for each
                // code-block in the precinct. The value of Lblock is initially set to three. The
                // number of bytes contributed by each code-block is preceded by signalling bits
                // that increase the value of Lblock, as needed. A signalling bit of zero indicates
                // the current value of Lblock is sufficient. If there are k ones followed by a
                // zero, the value of Lblock is incremented by k. While Lblock can only increase,
                // the number of bits used to signal the length of the code-block contribution can
                // increase or decrease depending on the number of coding passes included."
                let length_bits = code_block.l_block + coding_passes_for_segment.ilog2();
                reader.read_bits_with_stuffing(length_bits as u8)
            }?;

            storage.segments.push(Segment {
                idx: segment,
                data_length: length,
                coding_pases: coding_passes_for_segment,
                // Will be set later.
                data: &[],
                pdfium_range: None,
                pdfium_index: u32::MAX,
            });

            trace!("length({segment}) {}", length);

            Some(())
        };

        let mut last_segment = get_segment_idx(previous_layers_passes);
        let mut coding_passes_for_segment = 0;

        for coding_pass in previous_layers_passes..cumulative_passes {
            let segment = get_segment_idx(coding_pass);

            if segment != last_segment {
                push_segment(last_segment, coding_passes_for_segment)?;
                last_segment = segment;
                coding_passes_for_segment = 1;
            } else {
                coding_passes_for_segment += 1;
            }
        }

        // Flush the final segment if applicable.
        if coding_passes_for_segment > 0 {
            push_segment(last_segment, coding_passes_for_segment)?;
        }

        let end = storage.segments.len();
        layer.segments = Some(start..end);
        code_block.number_of_coding_passes += added_coding_passes;
        code_block.non_empty_layer_count += 1;
    }

    Some(())
}

/// Calculate the segment index for the given pass in arithmetic decoder
/// bypass (see section D.6, Table D.9).
fn segment_idx_for_bypass(pass_idx: u8) -> u8 {
    if pass_idx < 10 {
        0
    } else {
        1 + (2 * ((pass_idx - 10) / 3)) + (if ((pass_idx - 10) % 3) == 2 { 1 } else { 0 })
    }
}
