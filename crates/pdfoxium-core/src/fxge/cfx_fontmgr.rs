// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Fontations/AGG font manager from `core/fxge/cfx_fontmgr.{h,cpp}`.
//! The C ABI initializes it explicitly; Rust core clients can use it lazily.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};
use std::rc::{Rc, Weak};

use super::cfx_font::CfxFont;
use super::cfx_fontmapper::CfxFontMapper;
use super::cfx_glyphcache::CfxGlyphCache;

pub(crate) type SharedGlyphCache = Rc<RefCell<CfxGlyphCache>>;

#[derive(Default)]
pub struct CfxFontMgr {
    font_memory: Rc<super::font_memory::FontMemory>,
    builtin_mapper: reentry::BuiltinMapperSlot,
    glyph_cache_map: RefCell<BTreeMap<u64, Weak<RefCell<CfxGlyphCache>>>>,
    glyph_cache_sweep: Cell<Option<u64>>,
}

impl CfxFontMgr {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_memory_limits(limits: super::font_memory::FontMemoryLimits) -> Self {
        Self {
            font_memory: Rc::new(super::font_memory::FontMemory::new(limits)),
            ..Self::default()
        }
    }

    pub fn font_memory(&self) -> Rc<super::font_memory::FontMemory> {
        Rc::clone(&self.font_memory)
    }

    pub fn builtin_mapper(&self) -> BuiltinMapperGuard<'_> {
        self.builtin_mapper.checkout()
    }

    pub fn builtin_mapper_mut(&self) -> BuiltinMapperGuard<'_> {
        self.builtin_mapper.checkout()
    }

    pub(crate) fn get_glyph_cache(&self, font: &CfxFont<'_>) -> SharedGlyphCache {
        // Stable face identities replace C++ pointer keys, including when an
        // owned Rust face moves. Zero is the source's null-face cache entry.
        let identity = font.face().map_or(0, |face| face.cache_id());
        let mut caches = self.glyph_cache_map.borrow_mut();
        if let Some(cache) = caches.get(&identity).and_then(Weak::upgrade) {
            return cache;
        }
        // Face identities are never reused. Sweep two index entries per new
        // cache to reclaim expired Weak/Rc control blocks across documents.
        // Rotate through the map, including past still-live fonts, without an
        // O(n) scan on every insertion into a font-heavy document. Cache hits
        // and live bitmap/path contents remain untouched.
        for _ in 0..2 {
            let next = self
                .glyph_cache_sweep
                .get()
                .and_then(|key| caches.range((Excluded(key), Unbounded)).next())
                .or_else(|| caches.first_key_value())
                .map(|(&key, cache)| (key, cache.strong_count() == 0));
            let Some((key, expired)) = next else {
                self.glyph_cache_sweep.set(None);
                break;
            };
            self.glyph_cache_sweep.set(Some(key));
            if expired {
                caches.remove(&key);
            }
        }
        let cache = Rc::new(RefCell::new(CfxGlyphCache::with_memory(self.font_memory())));
        caches.insert(identity, Rc::downgrade(&cache));
        cache
    }
}

pub fn initialize_font_manager() {
    super::cfx_gemodule::initialize_ge_module(None);
}

pub fn destroy_font_manager() {
    super::cfx_gemodule::destroy_ge_module();
}

pub fn with_font_manager<R>(f: impl FnOnce(&CfxFontMgr) -> R) -> R {
    super::cfx_gemodule::with_ge_module(|module| f(module.font_mgr()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_glyph_cache_indices_do_not_accumulate_and_live_caches_stay_shared() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
        ));
        let manager = CfxFontMgr::new();
        let live_font = CfxFont::from_span(data, false, 0).unwrap();
        let live_cache = manager.get_glyph_cache(&live_font);
        for _ in 0..64 {
            let font = CfxFont::from_span(data, false, 0).unwrap();
            let cache = manager.get_glyph_cache(&font);
            assert_eq!(manager.glyph_cache_map.borrow().len(), 2);
            assert!(Rc::ptr_eq(
                &live_cache,
                &manager.get_glyph_cache(&live_font)
            ));
            let expired = Rc::downgrade(&cache);
            drop(cache);
            drop(font);
            assert!(expired.upgrade().is_none());
        }
        drop(live_cache);
        drop(live_font);
        let font = CfxFont::from_span(data, false, 0).unwrap();
        let cache = manager.get_glyph_cache(&font);
        assert_eq!(manager.glyph_cache_map.borrow().len(), 1);
        assert!(Rc::ptr_eq(&cache, &manager.get_glyph_cache(&font)));
    }
}

#[path = "cfx_fontmgr_reentry.rs"]
mod reentry;
pub use reentry::{BuiltinMapperGuard, FontMapperFactory};
