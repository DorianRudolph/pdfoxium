// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Published Skera replaces the native subset call, retaining glyph IDs and
//! .notdef outlines. Validate the result before creating any PDF overrides:
//! Skera can succeed while dropping an unsupported outline table. In particular,
//! CFF support is pending Fontations #2248 / #2331. There is no format blacklist;
//! a future published release can satisfy the same checks without another path.

use std::collections::BTreeSet;

use read_fonts::collections::IntSet;
use read_fonts::ps::cff::CffFontRef;
use read_fonts::types::{GlyphId, NameId, Tag};
use read_fonts::{FontRef, TableProvider};
use skera::{Plan, SubsetFlags, DEFAULT_DROP_TABLES, DEFAULT_LAYOUT_FEATURES};

#[derive(Debug)]
pub(super) enum SubsetError {
    InvalidFont,
    Skera,
    InvalidOutput,
}

pub(super) fn subset_font_data(data: &[u8], gids: &BTreeSet<u32>) -> Result<Vec<u8>, SubsetError> {
    let font = FontRef::from_index(data, 0).map_err(|_| SubsetError::InvalidFont)?;
    let glyphs: IntSet<GlyphId> = gids.iter().copied().map(GlyphId::new).collect();
    let names: IntSet<NameId> = (0..=6).map(NameId::new).collect();
    let plan = Plan::new(
        &glyphs,
        &IntSet::empty(),
        &font,
        SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS | SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
        &DEFAULT_DROP_TABLES.iter().copied().collect(),
        &IntSet::all(),
        &DEFAULT_LAYOUT_FEATURES.iter().copied().collect(),
        &names,
        &[0x0409].into_iter().collect(),
    );
    if plan.old_to_new_glyph_mapping().any(|(old, new)| old != new) {
        return Err(SubsetError::InvalidOutput);
    }
    let subset = skera::subset_font(&font, &plan).map_err(|_| SubsetError::Skera)?;
    validate_subset(&font, &subset, gids).ok_or(SubsetError::InvalidOutput)?;
    Ok(subset)
}

pub(super) fn validate_subset(
    original: &FontRef<'_>,
    data: &[u8],
    gids: &BTreeSet<u32>,
) -> Option<()> {
    let subset = FontRef::new(data).ok()?;
    let original_count = u32::from(original.maxp().ok()?.num_glyphs());
    let count = u32::from(subset.maxp().ok()?.num_glyphs());
    let head = subset.head().ok()?;
    subset.hhea().ok()?;
    subset.hmtx().ok()?;
    if count == 0 || head.units_per_em() != original.head().ok()?.units_per_em() {
        return None;
    }
    // HarfBuzz ignores requested glyph IDs outside the original face.
    let retained = std::iter::once(0).chain(gids.iter().copied().filter(|&g| g < original_count));
    if retained.clone().any(|gid| gid >= count) {
        return None;
    }

    // Preserve outline format because the PDFium save path chooses FontFile2/
    // FontFile3 and CIDFontType2/CIDFontType0 from the original font's flavor.
    // Checking the table directory alone is insufficient: parse the glyph data.
    let mut has_outlines = false;
    if original.data_for_tag(Tag::new(b"glyf")).is_some() {
        has_outlines = true;
        let glyf = subset.glyf().ok()?;
        let loca = subset.loca(None).ok()?;
        if loca.len() < count as usize || !loca.all_offsets_are_ascending() {
            return None;
        }
        for gid in 0..count {
            loca.get(GlyphId::new(gid), &glyf)?;
        }
        let original_glyf = original.glyf().ok()?;
        let original_loca = original.loca(None).ok()?;
        for gid in retained.clone() {
            let old = original_loca.get(GlyphId::new(gid), &original_glyf)?;
            let new = loca.get(GlyphId::new(gid), &glyf)?;
            if old.glyph().is_some() && new.glyph().is_none() {
                return None;
            }
        }
    }
    for tag in [Tag::new(b"CFF "), Tag::new(b"CFF2")] {
        if let Some(original_table) = original.data_for_tag(tag) {
            has_outlines = true;
            let table = subset.data_for_tag(tag)?;
            let old = CffFontRef::new(original_table.as_bytes(), 0, None).ok()?;
            let new = CffFontRef::new(table.as_bytes(), 0, None).ok()?;
            if new.version() != old.version() || new.num_glyphs() != count {
                return None;
            }
            for gid in retained.clone() {
                if !old.charstrings().get(gid as usize)?.is_empty()
                    && new.charstrings().get(gid as usize)?.is_empty()
                {
                    return None;
                }
            }
        }
    }
    has_outlines.then_some(())
}
