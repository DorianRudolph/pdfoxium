// Native decoder packet progression, translated from pinned OpenJPEG pi.c and
// j2k.c::opj_j2k_read_poc. BSD-2-Clause notices are retained in
// ../../LICENSE-OPENJPEG-PI and ../../LICENSE-OPENJPEG-J2K.

use alloc::boxed::Box;
use alloc::vec::Vec;

use super::codestream::ProgressionOrder;
use super::progression::ProgressionData;
use super::tile::{ComponentTile, ResolutionTile, Tile};
use crate::error::{DecodingError, Result, ValidationError, bail};
use crate::reader::BitReader;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Poc {
    resno0: u8,
    compno0: u16,
    layno1: u16,
    resno1: u8,
    compno1: u16,
    prg: u8,
}

pub(crate) fn read_poc(
    reader: &mut BitReader<'_>,
    components: u16,
    layers: u16,
    pocs: &mut Vec<Poc>,
) -> Option<()> {
    let bytes = reader.read_u16()?.checked_sub(2)? as usize;
    let comp_room = if components <= 256 { 1 } else { 2 };
    let chunk_size = 5 + 2 * comp_room;
    let count = bytes / chunk_size;
    if count == 0 || bytes % chunk_size != 0 || pocs.len() + count >= 32 {
        return None;
    }
    let component = |reader: &mut BitReader<'_>| {
        if comp_room == 1 {
            reader.read_byte().map(u16::from)
        } else {
            reader.read_u16()
        }
    };
    for _ in 0..count {
        pocs.push(Poc {
            resno0: reader.read_byte()?,
            compno0: component(reader)?,
            layno1: reader.read_u16()?.min(layers),
            resno1: reader.read_byte()?,
            compno1: component(reader)?.min(components),
            prg: reader.read_byte()?,
        });
    }
    Some(())
}

fn resolution<'a>(tile: &'a Tile<'a>, comp: u16, res: u8) -> Option<ResolutionTile<'a>> {
    let info = tile.component_infos.get(usize::from(comp))?;
    (res < info.num_resolution_levels())
        .then(|| ResolutionTile::new(ComponentTile::new(tile, info), res))
}

fn packets(
    comp: u16,
    res: u8,
    precinct: u64,
    layers: u16,
) -> impl Iterator<Item = ProgressionData> {
    (0..layers).map(move |layer_num| ProgressionData {
        pdfium_poc: 0,
        layer_num,
        resolution: res,
        component: comp,
        precinct,
    })
}

// opj_pi_next_rpcl/pcrl use minima over every component and resolution; CPRL
// computes the same minima for the current component only. Overflowing steps
// are excluded from the minima, as in native's explicit UINT_MAX checks.
fn steps(tile: &Tile<'_>, component: Option<u16>) -> (u32, u32) {
    let mut dx = 0;
    let mut dy = 0;
    for (index, info) in tile.component_infos.iter().enumerate() {
        if component.is_some_and(|comp| usize::from(comp) != index) {
            continue;
        }
        for res in 0..info.num_resolution_levels() {
            let level = u32::from(info.num_resolution_levels() - 1 - res);
            let (pdx, pdy) = info.coding_style.parameters.precinct_exponents[res as usize];
            let step = |sampling: u8, exponent: u8| {
                1u32.checked_shl(u32::from(exponent) + level)?
                    .checked_mul(u32::from(sampling))
            };
            if let Some(value) = step(info.size_info.horizontal_resolution, pdx) {
                dx = if dx == 0 { value } else { dx.min(value) };
            }
            if let Some(value) = step(info.size_info.vertical_resolution, pdy) {
                dy = if dy == 0 { value } else { dy.min(value) };
            }
        }
    }
    (dx, dy)
}

fn positions(tile: &Tile<'_>, (dx, dy): (u32, u32)) -> impl Iterator<Item = (u32, u32)> {
    let rect = tile.rect;
    let axis = |start: u32, end: u32, step: u32| {
        core::iter::successors((step != 0 && start < end).then_some(start), move |&value| {
            value
                .checked_add(step - value % step)
                .filter(|&next| next < end)
        })
    };
    axis(rect.y0, rect.y1, dy).flat_map(move |y| axis(rect.x0, rect.x1, dx).map(move |x| (x, y)))
}

// The common position-to-precinct calculation from all three native position
// progressions, including the partial first precinct at the tile's origin.
fn position_precinct(tile: &Tile<'_>, comp: u16, res: u8, x: u32, y: u32) -> Option<u64> {
    let resolution = resolution(tile, comp, res)?;
    let info = resolution.component_tile.component_info;
    let level = u32::from(info.num_resolution_levels() - 1 - res);
    let (pdx, pdy) = info.coding_style.parameters.precinct_exponents[res as usize];
    let rpx = u32::from(pdx) + level;
    let rpy = u32::from(pdy) + level;
    let dx = u64::from(info.size_info.horizontal_resolution).checked_mul(1u64 << level)?;
    let dy = u64::from(info.size_info.vertical_resolution).checked_mul(1u64 << level)?;
    let precinct_dx = u64::from(info.size_info.horizontal_resolution).checked_mul(1u64 << rpx)?;
    let precinct_dy = u64::from(info.size_info.vertical_resolution).checked_mul(1u64 << rpy)?;
    let rect = tile.rect;
    let trx0 = u64::from(rect.x0).div_ceil(dx);
    let try0 = u64::from(rect.y0).div_ceil(dy);
    let trx1 = u64::from(rect.x1).div_ceil(dx);
    let try1 = u64::from(rect.y1).div_ceil(dy);
    if !(u64::from(y) % precinct_dy == 0 || (y == rect.y0 && (try0 << level) % (1u64 << rpy) != 0))
        || !(u64::from(x) % precinct_dx == 0
            || (x == rect.x0 && (trx0 << level) % (1u64 << rpx) != 0))
        || trx0 == trx1
        || try0 == try1
    {
        return None;
    }
    let pw = trx1.div_ceil(1u64 << pdx) - (trx0 >> pdx);
    let ph = try1.div_ceil(1u64 << pdy) - (try0 >> pdy);
    if pw == 0 || ph == 0 {
        return None;
    }
    let prci = (u64::from(x).div_ceil(dx) >> pdx).checked_sub(trx0 >> pdx)?;
    let prcj = (u64::from(y).div_ceil(dy) >> pdy).checked_sub(try0 >> pdy)?;
    let precinct = prci + prcj * pw;
    (precinct < pw * ph).then_some(precinct)
}

fn progression<'a>(tile: &'a Tile<'a>, poc: Poc) -> Box<dyn Iterator<Item = ProgressionData> + 'a> {
    // Native invalid component bounds terminate this POC rather than indexing
    // component metadata. Empty/reversed bounds also yield no packets.
    if usize::from(poc.compno0) >= tile.component_infos.len()
        || usize::from(poc.compno1) > tile.component_infos.len()
    {
        return Box::new(core::iter::empty());
    }
    let components = poc.compno0..poc.compno1;
    let resolutions = poc.resno0..poc.resno1;
    match poc.prg {
        0 | 1 => {
            // opj_pi_next_lrcp and opj_pi_next_rlcp differ only in the two
            // outer loops. Each component can have its own resolution count.
            let pairs: Box<dyn Iterator<Item = (u16, u8)>> = if poc.prg == 0 {
                Box::new(
                    (0..poc.layno1)
                        .flat_map(move |layer| resolutions.clone().map(move |res| (layer, res))),
                )
            } else {
                Box::new(
                    resolutions.flat_map(move |res| (0..poc.layno1).map(move |layer| (layer, res))),
                )
            };
            Box::new(pairs.flat_map(move |(layer, res)| {
                components.clone().flat_map(move |comp| {
                    let count = resolution(tile, comp, res).map_or(0, |res| res.num_precincts());
                    (0..count).map(move |precinct| ProgressionData {
                        pdfium_poc: 0,
                        layer_num: layer,
                        resolution: res,
                        component: comp,
                        precinct,
                    })
                })
            }))
        }
        2 => {
            let step = steps(tile, None);
            Box::new(resolutions.flat_map(move |res| {
                let components = components.clone();
                positions(tile, step).flat_map(move |(x, y)| {
                    components.clone().flat_map(move |comp| {
                        position_precinct(tile, comp, res, x, y)
                            .into_iter()
                            .flat_map(move |precinct| packets(comp, res, precinct, poc.layno1))
                    })
                })
            }))
        }
        3 => Box::new(positions(tile, steps(tile, None)).flat_map(move |(x, y)| {
            let resolutions = resolutions.clone();
            components.clone().flat_map(move |comp| {
                resolutions.clone().flat_map(move |res| {
                    position_precinct(tile, comp, res, x, y)
                        .into_iter()
                        .flat_map(move |precinct| packets(comp, res, precinct, poc.layno1))
                })
            })
        })),
        4 => Box::new(components.flat_map(move |comp| {
            let resolutions = resolutions.clone();
            positions(tile, steps(tile, Some(comp))).flat_map(move |(x, y)| {
                resolutions.clone().flat_map(move |res| {
                    position_precinct(tile, comp, res, x, y)
                        .into_iter()
                        .flat_map(move |precinct| packets(comp, res, precinct, poc.layno1))
                })
            })
        })),
        // POC stores the raw order. opj_pi_next returns false for an unknown
        // value (COD's OPJ_PROG_UNKNOWN has a separate tier-2 failure path).
        _ => Box::new(core::iter::empty()),
    }
}

pub(crate) fn iterator<'a>(
    tile: &'a Tile<'a>,
) -> Result<Box<dyn Iterator<Item = ProgressionData> + 'a>> {
    if tile.pdfium_pocs.is_empty() && matches!(tile.progression_order, ProgressionOrder::Unknown) {
        bail!(DecodingError::InvalidProgressionIterator);
    }
    let max_res = tile
        .component_infos
        .iter()
        .map(|info| info.num_resolution_levels())
        .max()
        .unwrap_or(0);
    let max_prec = tile
        .component_tiles()
        .flat_map(|comp| {
            (0..comp.component_info.num_resolution_levels())
                .map(move |res| ResolutionTile::new(comp, res).num_precincts())
        })
        .max()
        .unwrap_or(0);
    // opj_pi_create_decode: one shared inclusion table for every POC. Keep its
    // checked 32-bit strides and the extra layer, using byte flags in Rust.
    let step_c = u32::try_from(max_prec).map_err(|_| ValidationError::ImageTooLarge)?;
    let step_r = step_c
        .checked_mul(tile.component_infos.len() as u32)
        .ok_or(ValidationError::ImageTooLarge)?;
    let step_l = step_r
        .checked_mul(u32::from(max_res))
        .ok_or(ValidationError::ImageTooLarge)?;
    let size = step_l
        .checked_mul(u32::from(tile.num_layers) + 1)
        .ok_or(ValidationError::ImageTooLarge)?;
    let mut include = Vec::new();
    include
        .try_reserve_exact(size as usize)
        .map_err(|_| ValidationError::ImageTooLarge)?;
    include.resize(size as usize, 0u8);
    let default = Poc {
        resno0: 0,
        compno0: 0,
        layno1: tile.num_layers,
        resno1: max_res,
        compno1: tile.component_infos.len() as u16,
        prg: tile.progression_order as u8,
    };
    let pocs = if tile.pdfium_pocs.is_empty() {
        core::slice::from_ref(&default)
    } else {
        &tile.pdfium_pocs
    };
    // Only small POC scalars are copied. Packet enumeration stays lazy and
    // follows the native nested loops without sorting/materializing packets.
    let mut iterators = Vec::with_capacity(pocs.len());
    for (index, poc) in pocs.iter().enumerate() {
        let mut poc = *poc;
        poc.layno1 = poc.layno1.min(tile.num_layers);
        iterators.push(progression(tile, poc).map(move |mut packet| {
            packet.pdfium_poc = index as u8;
            packet
        }));
    }
    Ok(Box::new(iterators.into_iter().flatten().filter(
        move |packet| {
            let index = u64::from(packet.layer_num) * u64::from(step_l)
                + u64::from(packet.resolution) * u64::from(step_r)
                + u64::from(packet.component) * u64::from(step_c)
                + packet.precinct;
            let Some(flag) = usize::try_from(index)
                .ok()
                .and_then(|index| include.get_mut(index))
            else {
                return false;
            };
            let unseen = *flag == 0;
            *flag = 1;
            unseen
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile() -> Tile<'static> {
        let source = include_bytes!("../../../../crates/pdfoxium-core/tests/fixtures/rgb.j2c");
        let parsed =
            crate::j2c::parse_raw_mode(source, &crate::DecodeSettings::default(), true).unwrap();
        let mut tile = Tile::new(0, &parsed.header);
        tile.num_layers = 2;
        for (index, info) in tile.component_infos.iter_mut().enumerate() {
            let parameters = &mut info.coding_style.parameters;
            parameters.num_resolution_levels = if index == 0 { 1 } else { 2 };
            parameters.num_decomposition_levels = parameters.num_resolution_levels - 1;
        }
        tile
    }

    #[test]
    fn native_packet_orders_skip_missing_component_resolutions() {
        let mut tile = tile();
        // All resolutions have one precinct. The first component has only
        // resolution zero; every traversal must skip its nonexistent level.
        for order in 0..5 {
            tile.pdfium_pocs = alloc::vec![Poc {
                resno0: 0,
                compno0: 0,
                layno1: 2,
                resno1: 2,
                compno1: 4,
                prg: order
            }];
            let actual: Vec<_> = iterator(&tile)
                .unwrap()
                .map(|p| (p.layer_num, p.resolution, p.component))
                .collect();
            let mut expected = Vec::new();
            for layer in 0..2 {
                for res in 0..2 {
                    for comp in 0..4 {
                        if comp != 0 || res == 0 {
                            expected.push((layer, res, comp));
                        }
                    }
                }
            }
            match order {
                0 => {}
                1 => expected.sort_by_key(|&(layer, res, comp)| (res, layer, comp)),
                2 => expected.sort_by_key(|&(layer, res, comp)| (res, comp, layer)),
                3 | 4 => expected.sort_by_key(|&(layer, res, comp)| (comp, res, layer)),
                _ => unreachable!(),
            }
            assert_eq!(actual, expected, "order {order}");
        }
    }

    #[test]
    fn native_poc_ranges_share_inclusion_flags_and_start_each_layer_at_zero() {
        let mut tile = tile();
        tile.pdfium_pocs = alloc::vec![
            Poc {
                resno0: 0,
                compno0: 1,
                layno1: 1,
                resno1: 2,
                compno1: 3,
                prg: 0
            },
            Poc {
                resno0: 0,
                compno0: 0,
                layno1: 2,
                resno1: 2,
                compno1: 4,
                prg: 4
            },
        ];
        let packets: Vec<_> = iterator(&tile).unwrap().collect();
        assert_eq!(packets.len(), 14);
        let prefix: Vec<_> = packets[..4]
            .iter()
            .map(|p| (p.layer_num, p.resolution, p.component))
            .collect();
        assert_eq!(prefix, [(0, 0, 1), (0, 0, 2), (0, 1, 1), (0, 1, 2)]);
        for (index, packet) in packets.iter().enumerate() {
            assert_eq!(packet.pdfium_poc, u8::from(index >= 4));
            assert!(!packets[..index].iter().any(|previous| {
                (
                    previous.layer_num,
                    previous.resolution,
                    previous.component,
                    previous.precinct,
                ) == (
                    packet.layer_num,
                    packet.resolution,
                    packet.component,
                    packet.precinct,
                )
            }));
        }
        tile.pdfium_pocs[0].compno0 = 4; // Invalid start terminates this POC.
        assert_eq!(iterator(&tile).unwrap().count(), 14);
        tile.pdfium_pocs[1].layno1 = 0;
        assert_eq!(iterator(&tile).unwrap().count(), 0);
    }

    #[test]
    fn native_position_orders_cover_partial_origins_and_subsampled_precincts() {
        let mut tile = tile();
        tile.rect = super::super::rect::IntRect::from_ltrb(3, 5, 15, 16);
        for (index, info) in tile.component_infos.iter_mut().enumerate() {
            info.coding_style.parameters.precinct_exponents.fill((1, 1));
            if index != 0 {
                info.size_info.horizontal_resolution = 2;
                info.size_info.vertical_resolution = 2;
            }
        }
        let reference: Vec<_> = iterator(&tile).unwrap().collect();
        // Component zero: 7x6 precincts. Each subsampled component: 2x1
        // low-resolution plus 3x3 full-resolution precincts. Two layers.
        assert_eq!(reference.len(), 2 * (42 + 3 * (2 + 9)));
        for prg in 2..5 {
            tile.pdfium_pocs = alloc::vec![Poc {
                resno0: 0,
                compno0: 0,
                layno1: 2,
                resno1: 2,
                compno1: 4,
                prg
            }];
            let actual: Vec<_> = iterator(&tile).unwrap().collect();
            assert_eq!(actual.len(), reference.len(), "order {prg}");
            for packet in &reference {
                assert!(actual.contains(packet), "order {prg}: {packet:?}");
            }
        }
    }
}
