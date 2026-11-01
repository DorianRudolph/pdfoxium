// Translated from pinned libopenjpeg j2k.c/tcd.c.
// Copyright notices and BSD-2-Clause terms are retained in
// ../../LICENSE-OPENJPEG-J2K and ../../LICENSE-OPENJPEG-TCD.

//! Raw component storage for the PDFium adapter. Component rectangles and
//! level shifts follow pinned libopenjpeg j2k.c/tcd.c. Wavelet/codeblock decoding
//! stays in the planned safe Rust codec.

use super::ComponentData;
use super::codestream::{ComponentInfo, ComponentSizeInfo, Header, WaveletTransform};
use super::decode::TileDecodeContext;
use super::rect::IntRect;
use super::tile::{ComponentTile, ResolutionTile, Tile};
use crate::error::{ColorError, MarkerError, Result, TileError, ValidationError, bail};
use crate::reader::BitReader;
use alloc::vec::Vec;

/// Static opj_j2k_read_tile_header/decode_tile completion control. Packet and
/// coefficient decoding remains in the safe backend after its header sequence.
pub(super) fn parse_tiles<'a>(
    reader: &mut BitReader<'a>,
    header: &'a Header<'a>,
) -> Result<Vec<Tile<'a>>> {
    use super::codestream::markers::{EOC, SOT};
    use super::tile::{parse_tile_part, sot_marker};
    let tile_count = header.size_data.num_tiles()?;
    let mut tiles: Vec<_> = (0..tile_count)
        .map(|index| Tile::new(index, header))
        .collect();
    let mut correction = 0u16;
    let mut correction_checked = false;
    let mut completed = 0;
    let mut part_sequence = 0;
    loop {
        // This scalar reader copy borrows the same input. It keeps native SOT
        // validation before body parsing without passing state into the default
        // codec parser or copying any encoded/decoded buffer.
        let mut preview = reader.clone();
        if preview.read_marker()? != SOT {
            bail!(MarkerError::Expected("SOT"));
        }
        let part = sot_marker(&mut preview, true).ok_or(MarkerError::ParseFailure("SOT"))?;
        let index = usize::from(part.tile_index);
        let tile = tiles.get_mut(index).ok_or(TileError::InvalidIndex)?;
        let part_index = u16::from(part.part_index);
        if usize::from(part_index) != tile.tile_parts.len()
            || (tile.pdfium_declared_parts != 0 && part_index >= tile.pdfium_declared_parts)
        {
            bail!(TileError::Invalid);
        }
        if part.num_parts != 0 {
            let count = u16::from(part.num_parts) + correction;
            if part_index >= count {
                bail!(TileError::Invalid);
            }
            tile.pdfium_declared_parts = count;
        }
        parse_tile_part(reader, header, &mut tiles, part_sequence, true)?;
        part_sequence += 1;
        let declared = tiles[index].pdfium_declared_parts;
        let mut can_decode = declared != 0 && declared == part_index + 1;
        if can_decode && !correction_checked {
            correction_checked = true;
            if declared != 1 && needs_count_correction(reader, part.tile_index)? {
                correction = 1;
                for tile in &mut tiles {
                    if tile.pdfium_declared_parts != 0 {
                        tile.pdfium_declared_parts += 1;
                    }
                }
                can_decode = false;
            }
        }
        if can_decode {
            if !tiles[index].has_data() {
                bail!(TileError::Invalid);
            }
            mark_ready(&mut tiles[index], &mut completed);
        }

        let remaining = reader.tail().map_or(0, |data| data.len());
        if remaining < 2 {
            reader.skip_bytes(remaining).ok_or(TileError::Invalid)?;
            // Native's SPOT6 fallback applies while waiting for unspecified
            // tile-part counts, not after a tile was already ready to decode.
            if !can_decode
                && index + 1 == tiles.len()
                && tiles
                    .iter()
                    .any(|tile| tile.tile_parts.len() == 1 && tile.pdfium_declared_parts == 0)
            {
                mark_pending_tiles(&mut tiles, &mut completed);
                return Ok(tiles);
            }
            bail!(TileError::Invalid);
        }
        let marker = reader.peek_bytes(2).unwrap();
        if marker == [255, EOC] {
            reader.skip_bytes(2).ok_or(TileError::Invalid)?;
            mark_pending_tiles(&mut tiles, &mut completed);
            return Ok(tiles);
        }
        if marker == [255, SOT] {
            if can_decode && completed == tiles.len() {
                // DecodeTile has consumed the next marker before the outer
                // image loop stops. Its SOT body remains unread.
                reader.skip_bytes(2).ok_or(TileError::Invalid)?;
                return Ok(tiles);
            }
            if remaining == 2 {
                reader.skip_bytes(2).ok_or(TileError::Invalid)?;
                if let Some(tile) = tiles[index..]
                    .iter_mut()
                    .find(|tile| !tile.pdfium_decode && tile.has_data())
                {
                    mark_ready(tile, &mut completed);
                }
                return Ok(tiles);
            }
            continue;
        }
        reader.skip_bytes(2).ok_or(TileError::Invalid)?;
        if remaining == 2 {
            // DecodeTile accepts a non-EOC final pair at EOF. With unspecified
            // counts, ReadTileHeader reaches NEOC and decodes the first pending
            // tile from the current tile number before stopping.
            if !can_decode {
                if let Some(tile) = tiles[index..]
                    .iter_mut()
                    .find(|tile| !tile.pdfium_decode && tile.has_data())
                {
                    mark_ready(tile, &mut completed);
                }
            }
            return Ok(tiles);
        }
        bail!(MarkerError::Expected("SOT"));
    }
}

fn mark_ready(tile: &mut Tile<'_>, completed: &mut usize) {
    if !tile.pdfium_decode {
        tile.pdfium_decode = true;
        tile.pdfium_decode_order = *completed;
        *completed += 1;
    }
}

fn mark_pending_tiles(tiles: &mut [Tile<'_>], completed: &mut usize) {
    for tile in tiles {
        if tile.has_data() {
            mark_ready(tile, completed);
        }
    }
}

/// Native's one-time lookahead and seek-back repair for TPsot == TNsot files.
fn needs_count_correction(reader: &BitReader<'_>, tile_index: u16) -> Result<bool> {
    use super::codestream::markers::SOT;
    let mut lookahead = reader.clone();
    loop {
        let Some(marker) = lookahead.read_bytes(2) else {
            return Ok(false);
        };
        if marker != [255, SOT] {
            return Ok(false);
        }
        let part = super::tile::sot_marker(&mut lookahead, true).ok_or(TileError::Invalid)?;
        if part.tile_index == tile_index {
            return Ok(part.part_index == part.num_parts);
        }
        if part.tile_part_length < 14 {
            return Ok(false);
        }
        let count = (part.tile_part_length - 12) as usize;
        let remaining = lookahead.tail().map_or(0, |data| data.len());
        lookahead
            .skip_bytes(count.min(remaining))
            .ok_or(TileError::Invalid)?;
    }
}

pub(crate) fn component_rect(header: &Header<'_>, info: &ComponentSizeInfo) -> Result<IntRect> {
    let size = &header.size_data;
    if info.horizontal_resolution == 0
        || info.vertical_resolution == 0
        || !(1..=31).contains(&info.precision)
        || header.skipped_resolution_levels > 32
        || [
            size.image_area_x_offset,
            size.image_area_y_offset,
            size.reference_grid_width,
            size.reference_grid_height,
        ]
        .iter()
        .any(|&coordinate| coordinate > i32::MAX as u32)
    {
        bail!(ValidationError::InvalidComponentMetadata);
    }
    let factor = 1u64 << header.skipped_resolution_levels;
    let x = |coordinate: u32| {
        u64::from(coordinate.div_ceil(u32::from(info.horizontal_resolution))).div_ceil(factor)
            as u32
    };
    let y = |coordinate: u32| {
        u64::from(coordinate.div_ceil(u32::from(info.vertical_resolution))).div_ceil(factor) as u32
    };
    Ok(IntRect::from_ltrb(
        x(size.image_area_x_offset),
        y(size.image_area_y_offset),
        x(size.reference_grid_width),
        y(size.reference_grid_height),
    ))
}

/// The native single-tile fast path transfers TCD buffers directly. Its exact
/// geometry checks matter when a POC leaves only a smaller resolution decoded.
pub(crate) fn single_tile_buffer(header: &Header<'_>) -> bool {
    let size = &header.size_data;
    size.tile_x_offset == 0
        && size.tile_y_offset == 0
        && size.image_area_x_offset == 0
        && size.image_area_y_offset == 0
        && size.reference_grid_width == size.tile_width
        && size.reference_grid_height == size.tile_height
}

/// Native TCD owns a separate, reusable buffer for each tile component. These
/// allocations precede MCT/DC shifting and the copy into the whole image.
pub(crate) fn prepare_tile(
    header: &Header<'_>,
    tile: &Tile<'_>,
    channels: &mut Vec<ComponentData>,
) -> Result<()> {
    for (index, info) in tile.component_infos.iter().enumerate() {
        let rect = tile_rect(header, tile, info);
        let count = usize::try_from(rect.area()).map_err(|_| ValidationError::ImageTooLarge)?;
        if let Some(channel) = channels.get_mut(index) {
            channel.container.reset_zeroed(count);
            channel.bit_depth = info.size_info.precision;
        } else {
            channels.push(ComponentData {
                container: crate::math::SimdBuffer::zeros(count),
                bit_depth: info.size_info.precision,
            });
        }
    }
    Ok(())
}

/// Place the actual IDWT resolution at the start of the native tile buffer,
/// using the requested resolution's stride, as opj_dwt_decode does in place.
pub(crate) fn store(
    header: &Header<'_>,
    tile: &Tile<'_>,
    tile_ctx: &TileDecodeContext,
    channel: &mut ComponentData,
    info: &ComponentInfo,
) -> Result<()> {
    let stride = tile_rect(header, tile, info).width() as usize;
    let source = &tile_ctx.idwt_output;
    let width = source.rect.width() as usize;
    if width == 0 || source.rect.height() == 0 {
        return Ok(());
    }
    for y in 0..source.rect.height() as usize {
        let input = y * width;
        let output = y * stride;
        let input_row = source
            .coefficients
            .get(input..input + width)
            .ok_or(ValidationError::InvalidDimensions)?;
        let output_row = channel
            .container
            .get_mut(output..output + width)
            .ok_or(ValidationError::InvalidDimensions)?;
        output_row.copy_from_slice(input_row);
    }
    Ok(())
}

/// Native opj_tcd_mct_decode processes the entire minimum-resolution buffer;
/// opj_tcd_dc_level_shift_decode then visits only the actual decoded rectangle.
pub(crate) fn finish_tile(
    header: &Header<'_>,
    tile: &Tile<'_>,
    resolutions: &[u8],
    channels: &mut [ComponentData],
) -> Result<()> {
    if tile.mct && channels.len() >= 3 {
        let infos = &tile.component_infos;
        let transform = infos[0].wavelet_transform();
        let count = tile_rect(header, tile, &infos[0]).area() as usize;
        for index in 1..3 {
            if infos[index].num_resolution_levels() != infos[0].num_resolution_levels()
                || resolutions[index] != resolutions[0]
                || tile_rect(header, tile, &infos[index]).area() as usize != count
            {
                bail!(ColorError::Mct);
            }
            // Native selects the MCT interpretation from component zero and
            // aliases the other components' integer/float words accordingly.
            reinterpret_samples(
                &mut channels[index].container[..count],
                infos[index].wavelet_transform(),
                transform,
            );
        }
        let [red, green, blue] = &mut channels[..3] else {
            unreachable!()
        };
        super::mct::apply_tile_row(
            transform,
            &mut red.container[..count],
            &mut green.container[..count],
            &mut blue.container[..count],
        );
        for index in 1..3 {
            reinterpret_samples(
                &mut channels[index].container[..count],
                transform,
                infos[index].wavelet_transform(),
            );
        }
    }
    // Native MCT with fewer than three components logs a warning and continues.
    for (index, (channel, info)) in channels.iter_mut().zip(&tile.component_infos).enumerate() {
        let area = decoded_rect(tile, info, resolutions[index]);
        let stride = tile_rect(header, tile, info).width() as usize;
        let half = 1i64 << (info.size_info.precision - 1);
        let (min, max) = if info.size_info.is_signed {
            (-half, half - 1)
        } else {
            (0, half * 2 - 1)
        };
        let offset = i64::from(info.pdfium_dc_offset);
        if area.width() == 0 || area.height() == 0 {
            continue;
        }
        for y in 0..area.height() as usize {
            let start = y * stride;
            let row = &mut channel.container[start..start + area.width() as usize];
            if info.wavelet_transform() == WaveletTransform::Reversible53 {
                shift_reversible_row(row, min, max, info.pdfium_dc_offset);
            } else {
                shift_irreversible_row(row, min, max, offset, info.size_info.precision);
            }
        }
    }
    Ok(())
}

// Keep native signed-word wrapping before clamping. When both clamp bounds
// fit i32, the entire operation can stay in vectorized integer arithmetic rather
// than widening each sample to i64. Wider component ranges retain the general
// path; float-to-integer saturation and NaN conversion are unchanged.
fn shift_reversible_row(row: &mut [f32], min: i64, max: i64, offset: i32) {
    if let (Ok(min), Ok(max)) = (i32::try_from(min), i32::try_from(max)) {
        for value in row {
            *value = (*value as i32).wrapping_add(offset).clamp(min, max) as f32;
        }
    } else {
        for value in row {
            *value = i64::from((*value as i32).wrapping_add(offset)).clamp(min, max) as f32;
        }
    }
}

fn shift_irreversible_row(row: &mut [f32], min: i64, max: i64, offset: i64, precision: u8) {
    // Within these bounds, min/max, the shifted clamp bounds, rounded samples
    // inside them and the final sum are all exactly representable in f32.
    // Clamp before adding the offset to avoid a float->i64->float conversion
    // per sample and allow the compiler to vectorize native nearest-even
    // rounding. Retain the general integer path for higher precision/offsets.
    #[cfg(feature = "std")]
    if precision <= 23 && (-(1 << 23)..=(1 << 23)).contains(&offset) {
        let lower = (min - offset) as f32;
        let upper = (max - offset) as f32;
        let offset = offset as f32;
        for value in row {
            // The general path's saturating NaN-to-integer cast yields zero.
            let rounded = if value.is_nan() {
                0.0
            } else {
                value.round_ties_even()
            };
            *value = (rounded.max(lower).min(upper) + offset) + 0.0;
        }
        return;
    }
    #[cfg(not(feature = "std"))]
    let _ = precision;
    for value in row {
        if *value > i32::MAX as f32 {
            *value = max as f32;
        } else if *value < i32::MIN as f32 {
            *value = min as f32;
        } else {
            let rounded = crate::math::round_ties_even_f32(*value);
            *value = ((rounded as i64) + offset).clamp(min, max) as f32;
        }
    }
}

fn reinterpret_samples(samples: &mut [f32], from: WaveletTransform, to: WaveletTransform) {
    if from == to {
        return;
    }
    for value in samples {
        *value = match to {
            WaveletTransform::Irreversible97 => f32::from_bits((*value as i32) as u32),
            WaveletTransform::Reversible53 => (value.to_bits() as i32) as f32,
        };
    }
}

/// opj_j2k_update_image_data intersects actual resolution coordinates with the
/// requested output grid, reading from the minimum-resolution tile stride.
pub(crate) fn copy_tile(
    header: &Header<'_>,
    tile: &Tile<'_>,
    resolutions: &[u8],
    sources: &[ComponentData],
    channels: &mut [ComponentData],
) -> Result<()> {
    for (index, info) in tile.component_infos.iter().enumerate() {
        let image = component_rect(header, &info.size_info)?;
        let source = decoded_rect(tile, info, resolutions[index]);
        // Native rejects negative computed destination widths/heights.
        if source.x1 < image.x0
            || image.x1 < source.x0
            || source.y1 < image.y0
            || image.y1 < source.y0
        {
            bail!(ValidationError::InvalidDimensions);
        }
        let copy = source.intersect(image);
        if copy.width() == 0 || copy.height() == 0 {
            continue;
        }
        let stride = tile_rect(header, tile, info).width() as usize;
        for y in copy.y0..copy.y1 {
            let input = (y - source.y0) as usize * stride + (copy.x0 - source.x0) as usize;
            let output =
                (y - image.y0) as usize * image.width() as usize + (copy.x0 - image.x0) as usize;
            let count = copy.width() as usize;
            let input_row = sources[index]
                .container
                .get(input..input + count)
                .ok_or(ValidationError::InvalidDimensions)?;
            let output_row = channels[index]
                .container
                .get_mut(output..output + count)
                .ok_or(ValidationError::InvalidDimensions)?;
            output_row.copy_from_slice(input_row);
        }
    }
    Ok(())
}

fn decoded_rect(tile: &Tile<'_>, info: &ComponentInfo, resolution: u8) -> IntRect {
    ResolutionTile::new(ComponentTile::new(tile, info), resolution).rect
}

fn tile_rect(header: &Header<'_>, tile: &Tile<'_>, info: &ComponentInfo) -> IntRect {
    decoded_rect(
        tile,
        info,
        info.num_resolution_levels() - 1 - header.skipped_resolution_levels,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversible_word_shift_matches_wide_clamp_at_precision_and_overflow_boundaries() {
        let mut source: Vec<f32> = (-32768..=32768).map(|n| n as f32 + 0.5).collect();
        for value in [
            0.0f32,
            -0.0,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            i32::MIN as f32,
            i32::MAX as f32,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            source.extend([value.next_down(), value, value.next_up()]);
        }
        for precision in [1, 2, 8, 16, 24, 30, 31, 32, 38] {
            let half = 1i64 << (precision - 1);
            for (min, max) in [(-half, half - 1), (0, half * 2 - 1)] {
                for offset in [i32::MIN, -32768, -1, 0, 1, 128, 32768, i32::MAX] {
                    let mut actual = source.clone();
                    shift_reversible_row(&mut actual, min, max, offset);
                    for (&input, &value) in source.iter().zip(&actual) {
                        let expected =
                            i64::from((input as i32).wrapping_add(offset)).clamp(min, max) as f32;
                        assert_eq!(
                            value.to_bits(),
                            expected.to_bits(),
                            "precision={precision} min={min} max={max} offset={offset} input={input}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn native_level_shift_rounds_ties_even_before_offset_and_clamp() {
        let source = include_bytes!("../../../../crates/pdfoxium-core/tests/fixtures/rgb.j2c");
        let parsed =
            crate::j2c::parse_raw_mode(source, &crate::DecodeSettings::default(), true).unwrap();
        let header = &parsed.header;
        let mut tile = Tile::new(0, header);
        tile.mct = false;
        let mut samples = Vec::new();
        for integer in -32768..=32768 {
            let tie = integer as f32 + 0.5;
            samples.extend([tie.next_down(), tie, tie.next_up()]);
        }
        samples.extend([
            0.0,
            -0.0,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            i32::MIN as f32,
            i32::MAX as f32,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ]);
        for (precision, signed, offset) in [
            (8, false, 128),
            (8, true, 0),
            (16, false, 32768),
            (23, false, -(1 << 23)),
            (23, true, 1 << 23),
            (24, false, 1 << 23),
            (8, true, (1 << 23) + 1),
            (31, true, i32::MAX),
            (31, false, i32::MIN),
        ] {
            for info in &mut tile.component_infos {
                info.coding_style.parameters.transformation = WaveletTransform::Irreversible97;
                info.size_info.precision = precision;
                info.size_info.is_signed = signed;
                info.pdfium_dc_offset = offset;
            }
            let mut channels = Vec::new();
            prepare_tile(header, &tile, &mut channels).unwrap();
            let count = tile_rect(header, &tile, &tile.component_infos[0]).area() as usize;
            let half = 1i64 << (precision - 1);
            let (min, max) = if signed {
                (-half, half - 1)
            } else {
                (0, half * 2 - 1)
            };
            for chunk in samples.chunks(count) {
                for channel in &mut channels {
                    channel.container[..chunk.len()].copy_from_slice(chunk);
                }
                finish_tile(header, &tile, &[4; 4], &mut channels).unwrap();
                for channel in &channels {
                    for (&input, &actual) in chunk.iter().zip(channel.container.iter()) {
                        // Independent integer reference for the former scalar
                        // rounding, including overflow guards and NaN casts.
                        let expected = if input > i32::MAX as f32 {
                            max
                        } else if input < i32::MIN as f32 {
                            min
                        } else {
                            let lower = crate::math::floor_f32(input);
                            let fraction = input - lower;
                            let rounded =
                                if fraction < 0.5 || (fraction == 0.5 && (lower as i64) & 1 == 0) {
                                    lower as i64
                                } else {
                                    (lower + 1.0) as i64
                                };
                            (rounded + i64::from(offset)).clamp(min, max)
                        } as f32;
                        assert_eq!(
                            actual, expected,
                            "input={input}, precision={precision}, signed={signed}, offset={offset}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn native_mct_uses_component_zero_word_interpretation() {
        let source = include_bytes!("../../../../crates/pdfoxium-core/tests/fixtures/rgb.j2c");
        let parsed =
            crate::j2c::parse_raw_mode(source, &crate::DecodeSettings::default(), true).unwrap();
        let header = &parsed.header;
        let mut tile = Tile::new(0, header);
        tile.mct = true;
        tile.component_infos[0]
            .coding_style
            .parameters
            .transformation = WaveletTransform::Irreversible97;
        tile.component_infos[2]
            .coding_style
            .parameters
            .transformation = WaveletTransform::Irreversible97;
        let mut channels = Vec::new();
        prepare_tile(header, &tile, &mut channels).unwrap();
        channels[0].container[0] = 10.0;
        // Native treats this component's integer word as the float 1.0 for MCT.
        channels[1].container[0] = 0x3f800000i32 as f32;
        channels[2].container[0] = 2.0;
        finish_tile(header, &tile, &[4; 4], &mut channels).unwrap();
        assert_eq!(channels[0].container[0], 32781.0);
        // The MCT float result is then read as an integer during its own shift.
        assert_eq!(channels[1].container[0], 65535.0);
        assert_eq!(channels[2].container[0], 32780.0);
    }
}
