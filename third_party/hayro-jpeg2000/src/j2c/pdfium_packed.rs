// Packed packet header procedures from pinned j2k.c::opj_j2k_read_ppm/ppt and
// opj_j2k_merge_ppm/ppt. BSD-2-Clause notices: ../../LICENSE-OPENJPEG-J2K.

use crate::reader::BitReader;
use alloc::vec::Vec;

pub(crate) type Markers<'a> = Vec<Option<&'a [u8]>>;

pub(crate) fn read_marker<'a>(reader: &mut BitReader<'a>, markers: &mut Markers<'a>) -> Option<()> {
    let length = reader.read_u16()?.checked_sub(2)?;
    // Native requires Zppx and at least one Nppm/Ippx byte.
    if length < 2 {
        return None;
    }
    let index = usize::from(reader.read_byte()?);
    if markers.len() <= index {
        markers.resize(index + 1, None);
    }
    if markers[index].is_some() {
        return None;
    }
    markers[index] = Some(reader.read_bytes(usize::from(length) - 1)?);
    Some(())
}

fn ppm_fragments(
    markers: &Markers<'_>,
    mut consume: impl FnMut(&[u8]) -> Option<()>,
) -> Option<()> {
    let mut remaining = 0usize;
    for mut data in markers.iter().flatten().copied() {
        if remaining != 0 {
            let count = remaining.min(data.len());
            consume(&data[..count])?;
            data = &data[count..];
            remaining -= count;
        }
        while !data.is_empty() {
            // Nppm is a four-byte field. Its payload may cross marker
            // boundaries; native rejects a split Nppm field itself.
            let count = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
            data = &data[4..];
            let available = count.min(data.len());
            consume(&data[..available])?;
            data = &data[available..];
            remaining = count - available;
        }
    }
    (remaining == 0).then_some(())
}

pub(crate) fn merge_ppm(markers: &Markers<'_>) -> Option<Vec<u8>> {
    let mut size = 0u32;
    ppm_fragments(markers, |part| {
        size = size.checked_add(u32::try_from(part.len()).ok()?)?;
        Some(())
    })?;
    let mut merged = Vec::new();
    merged.try_reserve_exact(size as usize).ok()?;
    ppm_fragments(markers, |part| {
        merged.extend_from_slice(part);
        Some(())
    })?;
    Some(merged)
}

pub(crate) fn merge_ppt(markers: &Markers<'_>) -> Option<Vec<u8>> {
    let size: usize = markers.iter().flatten().map(|data| data.len()).sum();
    let mut merged = Vec::new();
    merged.try_reserve_exact(size).ok()?;
    for data in markers.iter().flatten() {
        merged.extend_from_slice(data);
    }
    Some(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_nonempty_headers_keep_body_segment_ranges() {
        let source = include_bytes!("../../../../crates/pdfoxium-core/tests/fixtures/rgb.j2c");
        let parsed =
            crate::j2c::parse_raw_mode(source, &crate::DecodeSettings::default(), true).unwrap();
        let mut reader = BitReader::new(parsed.data);
        let tiles = super::super::tile::parse(&mut reader, &parsed.header, true).unwrap();
        let tile = &tiles[0];
        let mut storage = super::super::decode::DecompositionStorage::default();
        super::super::build::build(tile, &mut storage, 0).unwrap();
        let progression = super::super::pdfium_progression::iterator(tile).unwrap();
        super::super::segment::parse_pdfium(tile, 0, progression, &mut None, &mut storage).unwrap();
        let mut ranges: Vec<_> = storage
            .segments
            .iter()
            .filter_map(|segment| segment.pdfium_range.clone())
            .collect();
        ranges.sort_by_key(|range| range.start);
        let mut headers = Vec::new();
        let mut body = Vec::new();
        let mut position = 0;
        for range in ranges {
            headers.extend_from_slice(&storage.pdfium_body[position..range.start]);
            body.extend_from_slice(&storage.pdfium_body[range.clone()]);
            position = range.end;
        }
        headers.extend_from_slice(&storage.pdfium_body[position..]);
        assert!(!headers.is_empty() && !body.is_empty());
        let expected = crate::pdfium::Codestream::new(source, 0)
            .unwrap()
            .decode_components()
            .unwrap();
        let sot = source
            .windows(2)
            .position(|marker| marker == [255, 144])
            .unwrap();
        for ppm in [false, true] {
            let mut packed_headers = Vec::new();
            if ppm {
                packed_headers.extend_from_slice(&(headers.len() as u32).to_be_bytes());
            }
            packed_headers.extend_from_slice(&headers);
            let mut markers = Vec::new();
            // Discontiguous, reversed marker indices. A one-byte header
            // fragment also separates bytes inside a nonempty packet header.
            let split = if ppm { 5 } else { 1 };
            for (index, fragment) in [
                (3u8, &packed_headers[split..]),
                (0, &packed_headers[..split]),
            ] {
                markers.extend_from_slice(&[255, if ppm { 96 } else { 97 }]);
                markers.extend_from_slice(&(3 + fragment.len() as u16).to_be_bytes());
                markers.push(index);
                markers.extend_from_slice(fragment);
            }
            let mut encoded = source[..sot].to_vec();
            if ppm {
                encoded.extend_from_slice(&markers);
            }
            let mut tile_header = source[sot..sot + 12].to_vec();
            let length = 14 + body.len() + if ppm { 0 } else { markers.len() };
            tile_header[6..10].copy_from_slice(&(length as u32).to_be_bytes());
            encoded.extend_from_slice(&tile_header);
            if !ppm {
                encoded.extend_from_slice(&markers);
            }
            encoded.extend_from_slice(&[255, 147]);
            encoded.extend_from_slice(&body);
            encoded.extend_from_slice(&[255, 217]);
            let actual = crate::pdfium::Codestream::new(&encoded, 0)
                .unwrap()
                .decode_components()
                .unwrap();
            for (actual, expected) in actual.iter().zip(&expected) {
                assert_eq!(actual.samples, expected.samples);
            }
        }
    }
}
