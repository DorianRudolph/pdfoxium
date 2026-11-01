// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Vertical single substitutions from `core/fxge/cfx_cttgsubtable.{h,cpp}`.

use std::collections::BTreeSet;

const VRT2: u32 = u32::from_be_bytes(*b"vrt2");
const VERT: u32 = u32::from_be_bytes(*b"vert");

fn at(data: &[u8], offset: usize) -> Option<&[u8]> {
    data.get(offset..)
}

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn count_and_words(data: &[u8], count_offset: usize, items_offset: usize) -> Option<Vec<u16>> {
    let count = usize::from(u16_at(data, count_offset)?);
    let bytes = count.checked_mul(2)?;
    let items = data.get(items_offset..items_offset.checked_add(bytes)?)?;
    Some(
        items
            .chunks_exact(2)
            .map(|item| u16::from_be_bytes([item[0], item[1]]))
            .collect(),
    )
}

#[derive(Default)]
struct FeatureRecord {
    tag: u32,
    lookup_indices: Vec<u16>,
}

enum Coverage {
    None,
    Glyphs(Vec<u16>),
    Ranges(Vec<(u16, u16, u16)>),
}

impl Coverage {
    fn index(&self, glyph: u32) -> Option<usize> {
        match self {
            Self::None => None,
            Self::Glyphs(glyphs) => glyphs.iter().position(|&value| u32::from(value) == glyph),
            Self::Ranges(ranges) => ranges.iter().find_map(|&(start, end, first)| {
                (u32::from(start) <= glyph && glyph <= u32::from(end))
                    .then(|| usize::from(first) + (glyph - u32::from(start)) as usize)
            }),
        }
    }
}

struct SubTable {
    coverage: Coverage,
    replacement: Replacement,
}

enum Replacement {
    None,
    Delta(i16),
    Glyphs(Vec<u16>),
}

#[derive(Default)]
struct Lookup {
    lookup_type: u16,
    sub_tables: Vec<SubTable>,
}

#[derive(Default)]
pub struct CfxCttgsubTable {
    feature_set: BTreeSet<usize>,
    features: Vec<FeatureRecord>,
    lookups: Vec<Lookup>,
}

impl CfxCttgsubTable {
    pub fn new(gsub: &[u8]) -> Self {
        let mut table = Self::default();
        if u32_at(gsub, 0) != Some(0x0001_0000) {
            return table;
        }
        let Some((script_list, feature_list, lookup_list)) = (|| {
            Some((
                at(gsub, usize::from(u16_at(gsub, 4)?))?,
                at(gsub, usize::from(u16_at(gsub, 6)?))?,
                at(gsub, usize::from(u16_at(gsub, 8)?))?,
            ))
        })() else {
            return table;
        };
        let Some(script_features) = parse_script_list(script_list) else {
            return table;
        };
        let Some(features) = parse_feature_list(feature_list) else {
            return table;
        };
        let Some(lookups) = parse_lookup_list(lookup_list) else {
            return table;
        };
        table.features = features;
        table.lookups = lookups;
        for index in script_features {
            let index = usize::from(index);
            if table
                .features
                .get(index)
                .is_some_and(|feature| is_vertical_tag(feature.tag))
            {
                table.feature_set.insert(index);
            }
        }
        if table.feature_set.is_empty() {
            for (index, feature) in table.features.iter().enumerate() {
                if is_vertical_tag(feature.tag) {
                    table.feature_set.insert(index);
                }
            }
        }
        table
    }

    pub fn get_vertical_glyph(&self, glyph: u32) -> u32 {
        for &feature_index in &self.feature_set {
            let Some(feature) = self.features.get(feature_index) else {
                continue;
            };
            for &lookup_index in &feature.lookup_indices {
                let Some(lookup) = self.lookups.get(usize::from(lookup_index)) else {
                    continue;
                };
                if lookup.lookup_type != 1 {
                    continue;
                }
                for sub_table in &lookup.sub_tables {
                    let Some(index) = sub_table.coverage.index(glyph) else {
                        continue;
                    };
                    let replacement = match &sub_table.replacement {
                        Replacement::None => None,
                        Replacement::Delta(delta) => {
                            Some(glyph.wrapping_add_signed(i32::from(*delta)))
                        }
                        Replacement::Glyphs(glyphs) => {
                            glyphs.get(index).map(|&value| u32::from(value))
                        }
                    };
                    if let Some(replacement) = replacement {
                        return replacement;
                    }
                }
            }
        }
        0
    }
}

fn is_vertical_tag(tag: u32) -> bool {
    tag == VRT2 || tag == VERT
}

fn parse_script_list(data: &[u8]) -> Option<Vec<u16>> {
    let count = usize::from(u16_at(data, 0)?);
    let records = data.get(2..2usize.checked_add(count.checked_mul(6)?)?)?;
    let mut features = Vec::new();
    for record in records.chunks_exact(6) {
        let script = at(
            data,
            usize::from(u16::from_be_bytes([record[4], record[5]])),
        )?;
        let lang_count = usize::from(u16_at(script, 2)?);
        let languages = script.get(4..4usize.checked_add(lang_count.checked_mul(6)?)?)?;
        for language in languages.chunks_exact(6) {
            let lang = at(
                script,
                usize::from(u16::from_be_bytes([language[4], language[5]])),
            )?;
            features.extend(count_and_words(lang, 4, 6)?);
        }
    }
    Some(features)
}

fn parse_feature_list(data: &[u8]) -> Option<Vec<FeatureRecord>> {
    let count = usize::from(u16_at(data, 0)?);
    let records = data.get(2..2usize.checked_add(count.checked_mul(6)?)?)?;
    let mut features = Vec::with_capacity(count);
    for record in records.chunks_exact(6) {
        let tag = u32::from_be_bytes(record[..4].try_into().ok()?);
        let body = at(
            data,
            usize::from(u16::from_be_bytes([record[4], record[5]])),
        )?;
        features.push(FeatureRecord {
            tag,
            lookup_indices: count_and_words(body, 2, 4)?,
        });
    }
    Some(features)
}

fn parse_lookup_list(data: &[u8]) -> Option<Vec<Lookup>> {
    let offsets = count_and_words(data, 0, 2)?;
    offsets
        .into_iter()
        .map(|offset| parse_lookup(at(data, usize::from(offset))?))
        .collect()
}

fn parse_lookup(data: &[u8]) -> Option<Lookup> {
    let lookup_type = u16_at(data, 0)?;
    let offsets = count_and_words(data, 4, 6)?;
    let mut lookup = Lookup {
        lookup_type,
        sub_tables: Vec::new(),
    };
    if lookup_type == 1 {
        for offset in offsets {
            lookup
                .sub_tables
                .push(parse_single_subst(at(data, usize::from(offset))?)?);
        }
    }
    Some(lookup)
}

fn parse_coverage(data: &[u8]) -> Option<Coverage> {
    match u16_at(data, 0)? {
        1 => Some(Coverage::Glyphs(count_and_words(data, 2, 4)?)),
        2 => {
            let count = usize::from(u16_at(data, 2)?);
            let records = data.get(4..4usize.checked_add(count.checked_mul(6)?)?)?;
            Some(Coverage::Ranges(
                records
                    .chunks_exact(6)
                    .map(|record| {
                        (
                            u16::from_be_bytes([record[0], record[1]]),
                            u16::from_be_bytes([record[2], record[3]]),
                            u16::from_be_bytes([record[4], record[5]]),
                        )
                    })
                    .collect(),
            ))
        }
        _ => Some(Coverage::None),
    }
}

fn parse_single_subst(data: &[u8]) -> Option<SubTable> {
    let format = u16_at(data, 0)?;
    if format != 1 && format != 2 {
        return Some(SubTable {
            coverage: Coverage::None,
            replacement: Replacement::None,
        });
    }
    let coverage = parse_coverage(at(data, usize::from(u16_at(data, 2)?))?)?;
    let replacement = if format == 1 {
        Replacement::Delta(u16_at(data, 4)? as i16)
    } else {
        Replacement::Glyphs(count_and_words(data, 4, 6)?)
    };
    Some(SubTable {
        coverage,
        replacement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::cfx_face::CfxFace;

    #[test]
    fn vertical_glyphs_match_pdfium_font_fixture() {
        let bytes = include_bytes!(
            "../../../../upstream/pdfium/third_party/NotoSansCJK/NotoSansSC-Regular.subset.otf"
        );
        let face = CfxFace::new(bytes, 0).unwrap();
        let tag = u32::from_be_bytes(*b"GSUB");
        let length = face.get_sfnt_table(tag, &mut []);
        let mut gsub = vec![0; length];
        assert_eq!(face.get_sfnt_table(tag, &mut gsub), length);
        let table = CfxCttgsubTable::new(&gsub);
        assert_eq!(table.get_vertical_glyph(0), 0);
        assert_eq!(table.get_vertical_glyph(1), 0);
        assert_eq!(table.get_vertical_glyph(2), 10);
        assert_eq!(table.get_vertical_glyph(3), 0);
        assert_eq!(face.parse_gsub_table().unwrap().get_vertical_glyph(2), 10);
    }

    #[test]
    fn damaged_gsub_is_empty() {
        let table = CfxCttgsubTable::new(&[0, 1, 0, 0, 255]);
        assert_eq!(table.get_vertical_glyph(2), 0);
    }
}
