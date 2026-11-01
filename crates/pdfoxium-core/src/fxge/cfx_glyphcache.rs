// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Path, bitmap, and width caches from `core/fxge/cfx_glyphcache.{h,cpp}`.

use super::font_memory::{cache_entry_bytes, Account, FontMemory, Kind, Resource};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::fxcrt::fx_coordinates::CfxMatrix;
use crate::fxge::cfx_font::CfxFont;
use crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap;
use crate::fxge::cfx_path::CfxPath;
use crate::fxge::fx_font::FontAntiAliasingMode;

type PathMapKey = (u32, i32, i32, i32, bool);
type WidthMapKey = (u32, i32, i32);
type BitmapMapKey = ([i32; 6], Option<(i32, i32, bool)>, u32);

#[derive(Default)]
struct Maps {
    path_map: BTreeMap<PathMapKey, Option<Rc<CfxPath>>>,
    bitmap_map: BTreeMap<BitmapMapKey, Option<Rc<CfxGlyphBitmap>>>,
    width_map: BTreeMap<WidthMapKey, i32>,
}

struct Storage {
    account: Account,
    maps: RefCell<Maps>,
}
impl Resource for Storage {
    fn account(&self) -> &Account {
        &self.account
    }
    fn evict(&self) -> bool {
        let Ok(mut maps) = self.maps.try_borrow_mut() else {
            return false;
        };
        if self.account.bytes() != 0 {
            *maps = Maps::default();
            self.account.evicted();
        }
        true
    }
}

pub struct CfxGlyphCache {
    storage: Rc<Storage>,
}
impl Default for CfxGlyphCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CfxGlyphCache {
    pub fn new() -> Self {
        Self::with_memory(super::font_memory::font_memory())
    }

    pub fn with_memory(memory: Rc<FontMemory>) -> Self {
        let storage = Rc::new(Storage {
            account: Account::new(memory.clone(), Kind::Cache),
            maps: RefCell::new(Maps::default()),
        });
        memory.register(storage.clone());
        Self { storage }
    }
    fn admit(&self, bytes: usize, generation: u64) -> bool {
        let account = &self.storage.account;
        if generation != account.memory().generation()
            || !account.memory().make_room(Kind::Cache, bytes)
        {
            return false;
        }
        account.set(
            account
                .bytes()
                .checked_add(bytes)
                .expect("glyph accounting overflow"),
        );
        account.touch();
        true
    }

    pub fn load_glyph_path(
        &mut self,
        font: &CfxFont<'_>,
        glyph_index: u32,
        dest_width: i32,
    ) -> Option<Rc<CfxPath>> {
        if font.face().is_none() || glyph_index == u32::MAX {
            return None;
        }
        let key = Self::path_key(font, glyph_index, dest_width);
        self.storage.account.touch();
        if let Some(value) = self.storage.maps.borrow().path_map.get(&key) {
            return value.clone();
        }
        let generation = self.storage.account.memory().generation();
        let value = font
            .load_glyph_path_impl(glyph_index, dest_width)
            .map(Rc::new);
        let bytes = cache_entry_bytes::<PathMapKey, Option<Rc<CfxPath>>>()
            + value.as_ref().map_or(0, |path| {
                std::mem::size_of::<CfxPath>()
                    + 2 * std::mem::size_of::<usize>()
                    + path.retained_bytes()
            });
        if self.admit(bytes, generation) {
            self.storage
                .maps
                .borrow_mut()
                .path_map
                .insert(key, value.clone());
        }
        value
    }

    pub fn path_key(font: &CfxFont<'_>, glyph_index: u32, dest_width: i32) -> PathMapKey {
        let subst = font.subst_font();
        let weight = subst.map_or(0, |font| font.weight());
        let angle = subst.map_or(0, |font| font.italic_angle());
        let vertical = subst.is_some() && font.is_vertical();
        (glyph_index, dest_width, weight, angle, vertical)
    }

    pub fn load_glyph_bitmap(
        &mut self,
        font: &CfxFont<'_>,
        glyph_index: u32,
        is_cid_font: bool,
        matrix: CfxMatrix,
        dest_width: i32,
        anti_alias: FontAntiAliasingMode,
    ) -> Option<Rc<CfxGlyphBitmap>> {
        if glyph_index == u32::MAX {
            return None;
        }
        let subst = font.subst_font();
        let key = (
            [
                (matrix.a * 10000.0) as i32,
                (matrix.b * 10000.0) as i32,
                (matrix.c * 10000.0) as i32,
                (matrix.d * 10000.0) as i32,
                dest_width,
                anti_alias as i32,
            ],
            subst.map(|subst| (subst.weight(), subst.italic_angle(), font.is_vertical())),
            glyph_index,
        );
        #[cfg(feature = "render-profile")]
        if crate::render_profile::trace_glyphs() {
            if let Some(face) = font.face() {
                eprintln!(
                    "glyph-bitmap face={} glyph={} hit={} key={:?}",
                    face.cache_id(),
                    glyph_index,
                    self.storage.maps.borrow().bitmap_map.contains_key(&key),
                    key,
                );
            }
        }
        self.storage.account.touch();
        if let Some(value) = self.storage.maps.borrow().bitmap_map.get(&key) {
            return value.clone();
        }
        let generation = self.storage.account.memory().generation();
        let value = font
            .render_glyph_impl(glyph_index, is_cid_font, matrix, dest_width, anti_alias)
            .map(Rc::new);
        let bytes = cache_entry_bytes::<BitmapMapKey, Option<Rc<CfxGlyphBitmap>>>()
            + value.as_ref().map_or(0, |bitmap| bitmap.retained_bytes());
        if self.admit(bytes, generation) {
            self.storage
                .maps
                .borrow_mut()
                .bitmap_map
                .insert(key, value.clone());
        }
        value
    }

    pub fn get_glyph_width(
        &mut self,
        font: &CfxFont<'_>,
        glyph_index: u32,
        dest_width: i32,
        weight: i32,
    ) -> i32 {
        self.storage.account.touch();
        let key = (glyph_index, dest_width, weight);
        if let Some(&value) = self.storage.maps.borrow().width_map.get(&key) {
            return value;
        }
        let generation = self.storage.account.memory().generation();
        let value = font.get_glyph_width_impl(glyph_index, dest_width, weight);
        if self.admit(cache_entry_bytes::<WidthMapKey, i32>(), generation) {
            self.storage.maps.borrow_mut().width_map.insert(key, value);
        }
        value
    }

    pub fn path_count(&self) -> usize {
        self.storage.maps.borrow().path_map.len()
    }

    pub fn bitmap_count(&self) -> usize {
        self.storage.maps.borrow().bitmap_map.len()
    }

    pub fn width_count(&self) -> usize {
        self.storage.maps.borrow().width_map.len()
    }
}
