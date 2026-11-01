// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Format0 fallback enumeration: the byte map must remain usable when no
//! Unicode charmap or real post glyph names exist. Expected tuples are authored
//! independently of Fontations' charmap iterators.
use super::*;
use std::collections::BTreeMap;
const AHEM: &[u8] =
    include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
fn word(data: &[u8], at: usize) -> u16 {
    u16::from_be_bytes(data[at..at + 2].try_into().unwrap())
}
fn long(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(data[at..at + 4].try_into().unwrap())
}
fn put_word(data: &mut [u8], at: usize, value: u16) {
    data[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn put_long(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn tag(name: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*name)
}
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut bytes = [0; 4];
        bytes[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(bytes))
    })
}
fn format0(entries: &[(u8, u8)], names: u8, glyph_count: Option<u16>) -> Vec<u8> {
    let original = if glyph_count.is_some() {
        include_bytes!("../../tests/fixtures/hint-optional.ttf").as_slice()
    } else {
        AHEM
    };
    let mut tables = BTreeMap::new();
    for i in 0..usize::from(word(original, 4)) {
        let row = 12 + i * 16;
        let start = long(original, row + 8) as usize;
        let length = long(original, row + 12) as usize;
        tables.insert(
            long(original, row),
            original[start..start + length].to_vec(),
        );
    }
    let mut cmap = vec![0; 12 + 262];
    put_word(&mut cmap, 2, 1);
    put_word(&mut cmap, 4, PLATFORM_MAC);
    put_word(&mut cmap, 6, MAC_ENCODING_ROMAN);
    put_long(&mut cmap, 8, 12);
    put_word(&mut cmap, 14, 262);
    for &(code, gid) in entries {
        cmap[18 + usize::from(code)] = gid;
    }
    tables.insert(tag(b"cmap"), cmap);
    if names == 0 {
        let post = tables.get_mut(&tag(b"post")).unwrap();
        post.truncate(32);
        put_long(post, 0, 0x00030000);
    }
    if names == 1 {
        tables.remove(&tag(b"post"));
    }
    if let Some(count) = glyph_count {
        assert_eq!(word(tables.get(&tag(b"maxp")).unwrap(), 4), count);
    }
    serialize(tables)
}
fn serialize(mut tables: BTreeMap<u32, Vec<u8>>) -> Vec<u8> {
    put_long(tables.get_mut(&tag(b"head")).unwrap(), 8, 0);
    let n = tables.len();
    let mut result = vec![0; 12 + n * 16];
    put_long(&mut result, 0, 0x00010000);
    put_word(&mut result, 4, n as u16);
    let mut power = 1usize;
    let mut selector = 0;
    while power * 2 <= n {
        power *= 2;
        selector += 1;
    }
    put_word(&mut result, 6, (power * 16) as u16);
    put_word(&mut result, 8, selector);
    put_word(&mut result, 10, (n * 16 - power * 16) as u16);
    let mut head = 0;
    for (i, (&name, data)) in tables.iter().enumerate() {
        while result.len() % 4 != 0 {
            result.push(0);
        }
        let offset = result.len();
        let row = 12 + i * 16;
        put_long(&mut result, row, name);
        put_long(&mut result, row + 4, checksum(data));
        put_long(&mut result, row + 8, offset as u32);
        put_long(&mut result, row + 12, data.len() as u32);
        if name == tag(b"head") {
            head = offset;
        }
        result.extend_from_slice(data);
    }
    while result.len() % 4 != 0 {
        result.push(0);
    }
    let adjustment = 0xb1b0afbau32.wrapping_sub(checksum(&result));
    put_long(&mut result, head + 8, adjustment);
    assert_eq!(checksum(&result), 0xb1b0afba);
    result
}
// Replace a cmap while preserving the real two-glyph outlines and metrics.
fn unicode_map(entries: &[(u16, u16)], format: u16) -> Vec<u8> {
    let original = format0(&[], 0, Some(2));
    let mut tables = BTreeMap::new();
    for i in 0..usize::from(word(&original, 4)) {
        let row = 12 + i * 16;
        let start = long(&original, row + 8) as usize;
        let length = long(&original, row + 12) as usize;
        tables.insert(
            long(&original, row),
            original[start..start + length].to_vec(),
        );
    }
    let mut cmap = if format == 0 {
        let mut cmap = vec![0; 12 + 262];
        put_word(&mut cmap, 14, 262);
        for &(code, gid) in entries {
            cmap[18 + usize::from(code)] = gid as u8;
        }
        cmap
    } else {
        assert_eq!(format, 4);
        let count = entries.len() + 1;
        let mut cmap = vec![0; 12 + 16 + 8 * count];
        put_word(&mut cmap, 12, 4);
        put_word(&mut cmap, 14, (16 + 8 * count) as u16);
        put_word(&mut cmap, 18, (count * 2) as u16);
        let power = 1usize << count.ilog2();
        put_word(&mut cmap, 20, (power * 2) as u16);
        put_word(&mut cmap, 22, count.ilog2() as u16);
        put_word(&mut cmap, 24, ((count - power) * 2) as u16);
        let ends = 26;
        let starts = ends + count * 2 + 2;
        let deltas = starts + count * 2;
        for (i, &(code, gid)) in entries
            .iter()
            .chain(std::iter::once(&(0xffff, 0)))
            .enumerate()
        {
            put_word(&mut cmap, ends + i * 2, code);
            put_word(&mut cmap, starts + i * 2, code);
            put_word(&mut cmap, deltas + i * 2, gid.wrapping_sub(code));
        }
        cmap
    };
    put_word(&mut cmap, 2, 1);
    // Unicode platform: format 0 uses the fallback; format 4 uses mappings().
    put_word(&mut cmap, 4, 0);
    put_word(&mut cmap, 6, 3);
    put_long(&mut cmap, 8, 12);
    tables.insert(tag(b"cmap"), cmap);
    serialize(tables)
}
fn tuples(face: &CfxFace<'_>, limit: u32) -> Vec<(u32, u32)> {
    face.get_char_codes_and_indices(limit)
        .into_iter()
        .map(|entry| (entry.char_code, entry.glyph_index))
        .collect()
}

#[test]
fn nameless_format0_enumerates_sparse_nonzero_glyphs_in_character_order() {
    let data = format0(&[(255, 9), (65, 7), (32, 5)], 0, None);
    let face = CfxFace::new(&data, 0).unwrap();
    assert!(face.synthesized_unicode().is_none());
    assert!(!face.sfnt().charmap().has_map());
    assert_eq!(tuples(&face, u32::MAX), [(32, 5), (65, 7), (255, 9)]);
    assert_eq!(tuples(&face, u32::MAX), [(32, 5), (65, 7), (255, 9)]);
}
#[test]
fn format0_limit_is_inclusive_and_does_not_fabricate_a_notdef_below_the_first_mapping() {
    let data = format0(&[(32, 5), (65, 7), (255, 9)], 0, None);
    let face = CfxFace::new(&data, 0).unwrap();
    for (limit, expected) in [
        (0, vec![]),
        (31, vec![]),
        (32, vec![(32, 5)]),
        (64, vec![(32, 5)]),
        (65, vec![(32, 5), (65, 7)]),
        (254, vec![(32, 5), (65, 7)]),
        (255, vec![(32, 5), (65, 7), (255, 9)]),
    ] {
        assert_eq!(tuples(&face, limit), expected, "limit {limit}");
    }
}
#[test]
fn mapped_character_zero_is_distinct_from_skipped_zero_glyph_slots() {
    let data = format0(&[(0, 3), (1, 0), (2, 7), (255, 0)], 0, None);
    let face = CfxFace::new(&data, 0).unwrap();
    assert_eq!(tuples(&face, 0), [(0, 3)]);
    assert_eq!(tuples(&face, 255), [(0, 3), (2, 7)]);
    assert!(face.select_charmap(FontEncoding::AppleRoman));
    assert_eq!(face.get_char_index(0), 3);
    assert_eq!(face.get_char_index(1), 0);
    assert_eq!(face.get_char_index(2), 7);
}
#[test]
fn an_empty_format0_map_reports_the_freetype_first_character_notdef_sentinel() {
    let data = format0(&[], 0, None);
    let face = CfxFace::new(&data, 0).unwrap();
    for limit in [0, 1, 65, 255, u32::MAX] {
        assert_eq!(tuples(&face, limit), [(0, 0)]);
    }
}
#[test]
fn aliases_are_enumerated_per_character_without_deduplicating_their_glyph() {
    let data = format0(&[(65, 9), (66, 9), (67, 9), (255, 4)], 0, None);
    let face = CfxFace::new(&data, 0).unwrap();
    assert_eq!(tuples(&face, 67), [(65, 9), (66, 9), (67, 9)]);
    assert!(face.select_charmap(FontEncoding::AppleRoman));
    for code in [65, 66, 67] {
        assert_eq!(face.get_char_index(code), 9);
    }
}
#[test]
fn absent_and_nameless_post_tables_do_not_replace_the_byte_map_with_diagnostic_glyph_names() {
    for names in [0, 1] {
        let data = format0(&[(65, 7), (90, 9)], names, None);
        let face = CfxFace::new(&data, 0).unwrap();
        assert_eq!(tuples(&face, 255), [(65, 7), (90, 9)]);
        assert!(!face.has_glyph_names());
        assert_eq!(face.name_index("gid7"), 0);
        assert_eq!(face.name_index("A"), 0);
        assert!(face.glyph_name(face.glyph_count()).is_empty());
        assert!(face.font_glyph_bbox(face.glyph_count()).is_none());
    }
}
#[test]
fn real_post_names_take_priority_over_a_conflicting_mac_byte_mapping() {
    let original = CfxFace::new(AHEM, 0).unwrap();
    let a = original.unicode_char_index(65);
    let blank = original.unicode_char_index(32);
    let data = format0(&[(65, blank as u8)], 2, None);
    let face = CfxFace::new(&data, 0).unwrap();
    assert!(face.synthesized_unicode().is_some());
    assert_eq!(face.name_index("A"), a);
    assert!(tuples(&face, 65).contains(&(65, a)));
    assert!(face.select_charmap(FontEncoding::AppleRoman));
    assert_eq!(face.get_char_index(65), blank);
}
#[test]
fn a_format0_mapping_past_maxp_is_not_a_valid_selected_glyph_or_glyph_name() {
    let data = format0(&[(65, 1), (66, 2), (67, 255)], 0, Some(2));
    let face = CfxFace::new(&data, 0).unwrap();
    assert_eq!(face.glyph_count(), 2);
    assert!(face.select_charmap(FontEncoding::AppleRoman));
    assert_eq!(face.get_char_index(65), 1);
    assert_eq!(face.get_char_index(66), 0);
    assert_eq!(face.get_char_index(67), 0);
    assert!(face.glyph_name(2).is_empty());
    assert!(face.font_glyph_bbox(2).is_none());
    assert_eq!(face.name_index("gid2"), 0);
}

#[test]
fn format0_enumeration_skips_every_glyph_outside_the_real_maxp_count() {
    for unicode in [false, true] {
        let data = if unicode {
            unicode_map(&[(65, 1), (66, 2), (67, 255)], 0)
        } else {
            format0(&[(65, 1), (66, 2), (67, 255)], 0, Some(2))
        };
        let face = CfxFace::new(&data, 0).unwrap();
        assert!(!face.sfnt().charmap().has_map());
        assert_eq!(tuples(&face, 64), []);
        assert_eq!(tuples(&face, 65), [(65, 1)]);
        assert_eq!(tuples(&face, u32::MAX), [(65, 1)]);
    }
}
#[test]
fn exclusively_invalid_format0_mappings_report_the_empty_map_sentinel() {
    let data = format0(&[(65, 2), (67, 255)], 0, Some(2));
    let face = CfxFace::new(&data, 0).unwrap();
    assert_eq!(tuples(&face, 0), [(0, 0)]);
    assert_eq!(tuples(&face, u32::MAX), [(0, 0)]);
}
#[test]
fn invalid_format0_entries_before_a_valid_mapping_do_not_change_the_limit_contract() {
    let data = format0(&[(65, 2), (200, 1)], 0, Some(2));
    let face = CfxFace::new(&data, 0).unwrap();
    assert_eq!(tuples(&face, 199), []);
    assert_eq!(tuples(&face, 200), [(200, 1)]);
}
#[test]
fn unicode_format4_mappings_skip_out_of_range_glyphs_and_obey_the_valid_limit() {
    let data = unicode_map(&[(65, 1), (66, 2), (67, 255)], 4);
    let face = CfxFace::new(&data, 0).unwrap();
    assert!(face.sfnt().charmap().has_map());
    assert_eq!(tuples(&face, 64), []);
    assert_eq!(tuples(&face, 65), [(65, 1)]);
    assert_eq!(tuples(&face, u32::MAX), [(65, 1)]);
    let data = unicode_map(&[(65, 2), (200, 1)], 4);
    let face = CfxFace::new(&data, 0).unwrap();
    assert_eq!(tuples(&face, 199), []);
    assert_eq!(tuples(&face, 200), [(200, 1)]);
}
#[test]
fn exclusively_invalid_unicode_format4_mappings_report_the_empty_map_sentinel() {
    let data = unicode_map(&[(65, 2), (67, 255)], 4);
    let face = CfxFace::new(&data, 0).unwrap();
    assert!(face.sfnt().charmap().has_map());
    assert_eq!(tuples(&face, 0), [(0, 0)]);
    assert_eq!(tuples(&face, u32::MAX), [(0, 0)]);
}
