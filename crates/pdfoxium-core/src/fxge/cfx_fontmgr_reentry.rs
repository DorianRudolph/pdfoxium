// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Loan the real mapper out of its slot before host font callbacks. A callback
//! may replace the provider or perform a nested lookup without aliasing the
//! mapper currently executing its outer request.
use super::*;
use std::ops::{Deref, DerefMut};
// Retained FGAS managers can outlive graphics-module replacement. A new
// manager must not reuse its predecessor's provider generation.
thread_local! {
    static NEXT_SYSTEM_FONT_GENERATION: Cell<u64> = const { Cell::new(1) };
}
fn next_system_font_generation() -> u64 {
    NEXT_SYSTEM_FONT_GENERATION.with(|next| {
        let generation = next.get();
        next.set(generation.wrapping_add(1).max(1));
        generation
    })
}
pub type FontMapperFactory = Rc<dyn Fn() -> CfxFontMapper>;
pub(super) struct BuiltinMapperSlot {
    mapper: RefCell<Option<CfxFontMapper>>,
    generation: Cell<u64>,
    reentry_factory: RefCell<Option<FontMapperFactory>>,
}
impl Default for BuiltinMapperSlot {
    fn default() -> Self {
        Self {
            mapper: RefCell::new(Some(CfxFontMapper::new())),
            generation: Cell::new(next_system_font_generation()),
            reentry_factory: RefCell::new(None),
        }
    }
}
pub struct BuiltinMapperGuard<'a> {
    slot: &'a BuiltinMapperSlot,
    mapper: Option<CfxFontMapper>,
    generation: u64,
    restore: bool,
}
impl Deref for BuiltinMapperGuard<'_> {
    type Target = CfxFontMapper;
    fn deref(&self) -> &Self::Target {
        self.mapper.as_ref().expect("active mapper loan")
    }
}
impl DerefMut for BuiltinMapperGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.mapper.as_mut().expect("active mapper loan")
    }
}
impl Drop for BuiltinMapperGuard<'_> {
    fn drop(&mut self) {
        if self.restore && self.slot.generation.get() == self.generation {
            let prior = self.slot.mapper.replace(self.mapper.take());
            debug_assert!(prior.is_none());
            drop(prior);
        }
        // Replaced or nested mappers are released after every slot borrow.
    }
}
impl BuiltinMapperSlot {
    pub(super) fn checkout(&self) -> BuiltinMapperGuard<'_> {
        let current = self.mapper.borrow_mut().take();
        let restore = current.is_some();
        let mapper = current.unwrap_or_else(|| {
            let factory = self.reentry_factory.borrow().clone();
            factory.map_or_else(CfxFontMapper::new, |factory| factory())
        });
        BuiltinMapperGuard {
            slot: self,
            mapper: Some(mapper),
            generation: self.generation.get(),
            restore,
        }
    }
    fn replace(
        &self,
        mapper: CfxFontMapper,
        factory: Option<FontMapperFactory>,
    ) -> Option<CfxFontMapper> {
        self.generation.set(next_system_font_generation());
        let previous = self.mapper.replace(Some(mapper));
        let previous_factory = self.reentry_factory.replace(factory);
        drop(previous_factory);
        previous
    }
}
impl CfxFontMgr {
    pub fn system_font_generation(&self) -> u64 {
        self.builtin_mapper.generation.get()
    }
    /// Install an actual mapper and its external-provider factory. A busy outer
    /// loan keeps its provider alive until that request completes, then drops it
    /// instead of overwriting this newer installation.
    pub fn replace_builtin_mapper(
        &self,
        mapper: CfxFontMapper,
        factory: Option<FontMapperFactory>,
    ) -> Option<CfxFontMapper> {
        self.builtin_mapper.replace(mapper, factory)
    }
}
