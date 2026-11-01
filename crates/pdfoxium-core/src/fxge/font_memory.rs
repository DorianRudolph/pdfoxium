//! Per-library retention policy. Working allocations and caller-held results
//! are deliberately outside these budgets; this is not an allocator/RSS limit.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FontMemoryLimits {
    pub glyph_cache_bytes: usize,
    pub glyph_scratch_bytes: usize,
}

impl Default for FontMemoryLimits {
    fn default() -> Self {
        Self {
            glyph_cache_bytes: 64 * 1024 * 1024,
            glyph_scratch_bytes: 4 * 1024 * 1024,
        }
    }
}

impl FontMemoryLimits {
    pub const UNLIMITED: Self = Self {
        glyph_cache_bytes: usize::MAX,
        glyph_scratch_bytes: usize::MAX,
    };
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FontMemoryUsage {
    pub glyph_cache_bytes: usize,
    pub glyph_scratch_bytes: usize,
    pub glyph_cache_evictions: u64,
    pub glyph_scratch_evictions: u64,
    pub active_scratch: usize,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Kind {
    Cache,
    Scratch,
}

pub(crate) trait Resource {
    fn account(&self) -> &Account;
    fn evict(&self) -> bool;
}

/// An independent library's font-retention controller. Clone the Rc handle to
/// share budgets across its fonts; constructing another controller isolates it.
pub struct FontMemory {
    limits: Cell<FontMemoryLimits>,
    usage: Cell<FontMemoryUsage>,
    clock: Cell<u64>,
    next_id: Cell<u64>,
    generation: Cell<u64>,
    resources: RefCell<BTreeMap<u64, Weak<dyn Resource>>>,
    dead: RefCell<Vec<u64>>,
}

impl Default for FontMemory {
    fn default() -> Self {
        Self::new(FontMemoryLimits::default())
    }
}

impl FontMemory {
    pub fn new(limits: FontMemoryLimits) -> Self {
        Self {
            limits: Cell::new(limits),
            usage: Cell::new(FontMemoryUsage::default()),
            clock: Cell::new(0),
            next_id: Cell::new(0),
            generation: Cell::new(0),
            resources: RefCell::new(BTreeMap::new()),
            dead: RefCell::new(Vec::new()),
        }
    }
    pub fn limits(&self) -> FontMemoryLimits {
        self.limits.get()
    }
    pub fn usage(&self) -> FontMemoryUsage {
        self.usage.get()
    }
    /// Enforce new limits on idle retention immediately. Active scratch is
    /// checked when returned, including early/failed glyph renders.
    pub fn set_limits(&self, limits: FontMemoryLimits) {
        self.limits.set(limits);
        self.make_room(Kind::Cache, 0);
        self.make_room(Kind::Scratch, 0);
    }
    /// Flush idle cache/scratch retention without changing limits. Scratch
    /// checked out during this call is discarded when returned.
    pub fn trim(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        for resource in self.live_resources() {
            resource.evict();
        }
    }
    fn live_resources(&self) -> Vec<Rc<dyn Resource>> {
        self.reap();
        self.resources
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .collect()
    }
    fn reap(&self) {
        let dead = std::mem::take(&mut *self.dead.borrow_mut());
        let mut resources = self.resources.borrow_mut();
        for id in dead {
            resources.remove(&id);
        }
    }
    pub(crate) fn register(&self, resource: Rc<dyn Resource>) {
        self.reap();
        self.resources
            .borrow_mut()
            .insert(resource.account().id, Rc::downgrade(&resource));
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation.get()
    }
    pub(crate) fn make_room(&self, kind: Kind, bytes: usize) -> bool {
        let limit = match kind {
            Kind::Cache => self.limits.get().glyph_cache_bytes,
            Kind::Scratch => self.limits.get().glyph_scratch_bytes,
        };
        if bytes > limit {
            return false;
        }
        let fits = || {
            let usage = self.usage.get();
            let used = match kind {
                Kind::Cache => usage.glyph_cache_bytes,
                Kind::Scratch => usage.glyph_scratch_bytes,
            };
            used.checked_add(bytes).is_some_and(|total| total <= limit)
        };
        if fits() {
            return true;
        }
        let mut resources = self.live_resources();
        resources.sort_unstable_by_key(|r| r.account().last_used.get());
        for resource in resources {
            if resource.account().kind == kind && resource.account().bytes.get() != 0 {
                resource.evict();
                if fits() {
                    return true;
                }
            }
        }
        fits()
    }
}

pub(crate) struct Account {
    memory: Rc<FontMemory>,
    id: u64,
    kind: Kind,
    bytes: Cell<usize>,
    last_used: Cell<u64>,
}

impl Account {
    pub(crate) fn new(memory: Rc<FontMemory>, kind: Kind) -> Self {
        let id = memory
            .next_id
            .get()
            .checked_add(1)
            .expect("font resource identities exhausted");
        memory.next_id.set(id);
        Self {
            memory,
            id,
            kind,
            bytes: Cell::new(0),
            last_used: Cell::new(0),
        }
    }
    pub(crate) fn memory(&self) -> &Rc<FontMemory> {
        &self.memory
    }
    pub(crate) fn bytes(&self) -> usize {
        self.bytes.get()
    }
    pub(crate) fn touch(&self) {
        let tick = self.memory.clock.get().saturating_add(1);
        self.memory.clock.set(tick);
        self.last_used.set(tick);
    }
    pub(crate) fn set(&self, bytes: usize) {
        let old = self.bytes.replace(bytes);
        let mut usage = self.memory.usage.get();
        let value = match self.kind {
            Kind::Cache => &mut usage.glyph_cache_bytes,
            Kind::Scratch => &mut usage.glyph_scratch_bytes,
        };
        *value = value
            .checked_sub(old)
            .and_then(|v| v.checked_add(bytes))
            .expect("font memory accounting overflow");
        self.memory.usage.set(usage);
    }
    pub(crate) fn evicted(&self) {
        let mut usage = self.memory.usage.get();
        match self.kind {
            Kind::Cache => usage.glyph_cache_evictions += 1,
            Kind::Scratch => usage.glyph_scratch_evictions += 1,
        }
        self.memory.usage.set(usage);
        self.set(0);
    }
    fn active(&self, add: bool) {
        let mut usage = self.memory.usage.get();
        if add {
            usage.active_scratch += 1;
        } else {
            usage.active_scratch -= 1;
        }
        self.memory.usage.set(usage);
    }
}

impl Drop for Account {
    fn drop(&mut self) {
        self.set(0);
        self.memory.dead.borrow_mut().push(self.id);
    }
}

pub(crate) trait Scratch: Default {
    fn retained_bytes(&self) -> usize;
}

pub(crate) struct ScratchResource<T: Scratch> {
    account: Account,
    buffer: RefCell<Option<T>>,
}

impl<T: Scratch + 'static> ScratchResource<T> {
    pub(crate) fn new(memory: Rc<FontMemory>) -> Rc<Self> {
        let resource = Rc::new(Self {
            account: Account::new(memory.clone(), Kind::Scratch),
            buffer: RefCell::new(Some(T::default())),
        });
        memory.register(resource.clone());
        resource
    }
    pub(crate) fn checkout(&self) -> ScratchLease<'_, T> {
        let buffer = self
            .buffer
            .borrow_mut()
            .take()
            .expect("reentrant glyph scratch use");
        self.account.set(0);
        self.account.active(true);
        ScratchLease {
            resource: self,
            buffer: Some(buffer),
            generation: self.account.memory.generation(),
        }
    }
}

impl<T: Scratch> Resource for ScratchResource<T> {
    fn account(&self) -> &Account {
        &self.account
    }
    fn evict(&self) -> bool {
        let mut buffer = self.buffer.borrow_mut();
        if buffer.is_none() {
            return false;
        }
        if self.account.bytes() != 0 {
            *buffer = Some(T::default());
            self.account.evicted();
        }
        true
    }
}

pub(crate) struct ScratchLease<'a, T: Scratch> {
    resource: &'a ScratchResource<T>,
    buffer: Option<T>,
    generation: u64,
}
impl<T: Scratch> std::ops::Deref for ScratchLease<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.buffer.as_ref().unwrap()
    }
}
impl<T: Scratch> std::ops::DerefMut for ScratchLease<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.buffer.as_mut().unwrap()
    }
}
impl<T: Scratch> Drop for ScratchLease<'_, T> {
    fn drop(&mut self) {
        let account = &self.resource.account;
        let buffer = self.buffer.take().unwrap();
        let bytes = buffer.retained_bytes();
        let keep = self.generation == account.memory.generation()
            && account.memory.make_room(Kind::Scratch, bytes);
        *self.resource.buffer.borrow_mut() = Some(if keep {
            account.set(bytes);
            account.touch();
            buffer
        } else {
            T::default()
        });
        account.active(false);
    }
}

/// Full-node-per-entry allowance covers sparse std BTreeMap nodes without
/// depending on their private capacity/layout. Allocator overhead is excluded.
pub(crate) fn cache_entry_bytes<K, V>() -> usize {
    11 * std::mem::size_of::<(K, V)>() + 16 * std::mem::size_of::<usize>()
}

pub fn font_memory() -> Rc<FontMemory> {
    super::cfx_fontmgr::with_font_manager(|manager| manager.font_memory())
}
pub fn set_font_memory_limits(limits: FontMemoryLimits) {
    font_memory().set_limits(limits);
}
pub fn font_memory_limits() -> FontMemoryLimits {
    font_memory().limits()
}
pub fn font_memory_usage() -> FontMemoryUsage {
    font_memory().usage()
}
pub fn trim_font_memory() {
    font_memory().trim();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::fx_coordinates::CfxMatrix;
    use crate::fxge::{
        cfx_face::CfxFace, cfx_font::CfxFont, cfx_glyphcache::CfxGlyphCache,
        fx_font::FontAntiAliasingMode,
    };
    const FONT: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
    ));

    #[derive(Default)]
    struct Buffer(Vec<u8>);
    impl Scratch for Buffer {
        fn retained_bytes(&self) -> usize {
            self.0.capacity()
        }
    }

    #[test]
    fn scratch_global_lru_active_adjustment_trim_and_early_return() {
        let memory = Rc::new(FontMemory::new(FontMemoryLimits {
            glyph_cache_bytes: 0,
            glyph_scratch_bytes: 128,
        }));
        let a = ScratchResource::<Buffer>::new(memory.clone());
        let b = ScratchResource::<Buffer>::new(memory.clone());
        {
            let mut lease = a.checkout();
            lease.0 = Vec::with_capacity(96);
        }
        assert_eq!(memory.usage().glyph_scratch_bytes, 96);
        {
            let mut lease = a.checkout();
            assert_eq!(lease.0.capacity(), 96);
            lease.0.push(7);
        }
        {
            let mut lease = b.checkout();
            lease.0 = Vec::with_capacity(96);
        }
        assert_eq!(memory.usage().glyph_scratch_bytes, 96);
        assert_eq!(a.checkout().0.capacity(), 0);
        let mut lease = b.checkout();
        assert_eq!(lease.0.capacity(), 96);
        assert_eq!(memory.usage().active_scratch, 1);
        assert_eq!(memory.usage().glyph_scratch_bytes, 0);
        memory.set_limits(FontMemoryLimits {
            glyph_cache_bytes: 0,
            glyph_scratch_bytes: 32,
        });
        lease.0.push(1);
        drop(lease);
        assert_eq!(memory.usage().glyph_scratch_bytes, 0);
        memory.set_limits(FontMemoryLimits {
            glyph_cache_bytes: 0,
            glyph_scratch_bytes: 128,
        });
        let mut lease = a.checkout();
        lease.0 = Vec::with_capacity(64);
        memory.trim();
        drop(lease);
        assert_eq!(a.checkout().0.capacity(), 0);
        fn failed(resource: &ScratchResource<Buffer>) -> Option<()> {
            let mut lease = resource.checkout();
            lease.0 = Vec::with_capacity(1024);
            None
        }
        assert_eq!(failed(&a), None);
        assert_eq!(memory.usage().glyph_scratch_bytes, 0);
        assert_eq!(memory.usage().active_scratch, 0);
        assert_eq!(b.checkout().0.capacity(), 0);
    }

    #[test]
    fn cache_budget_evicts_lru_fonts_and_preserves_external_results() {
        let memory = Rc::new(FontMemory::default());
        let face = CfxFace::new_with_memory(FONT, 0, memory.clone()).unwrap();
        let font = CfxFont::from_face(&face, false, 0);
        let glyph = face.unicode_char_index('A' as u32);
        let matrix = CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0);
        let mut a = CfxGlyphCache::with_memory(memory.clone());
        let mut b = CfxGlyphCache::with_memory(memory.clone());
        let mut c = CfxGlyphCache::with_memory(memory.clone());
        let image = a
            .load_glyph_bitmap(&font, glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        let pixels = image.bitmap().buffer().to_vec();
        let one = memory.usage().glyph_cache_bytes;
        assert!(one > pixels.len());
        memory.set_limits(FontMemoryLimits {
            glyph_cache_bytes: one * 2,
            glyph_scratch_bytes: 0,
        });
        b.load_glyph_bitmap(&font, glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        assert_eq!(memory.usage().glyph_cache_bytes, one * 2);
        assert!(Rc::ptr_eq(
            &image,
            &a.load_glyph_bitmap(&font, glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
                .unwrap()
        ));
        c.load_glyph_bitmap(&font, glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        assert_eq!(
            (a.bitmap_count(), b.bitmap_count(), c.bitmap_count()),
            (1, 0, 1)
        );
        assert!(memory.usage().glyph_cache_bytes <= one * 2);
        memory.set_limits(FontMemoryLimits {
            glyph_cache_bytes: 0,
            glyph_scratch_bytes: 0,
        });
        assert_eq!(memory.usage().glyph_cache_bytes, 0);
        assert_eq!(image.bitmap().buffer(), pixels);
        let fresh = a
            .load_glyph_bitmap(&font, glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        assert_eq!(fresh.bitmap().buffer(), pixels);
        assert_eq!(a.bitmap_count(), 0);
        assert_eq!(memory.usage().glyph_cache_bytes, 0);
        memory.set_limits(FontMemoryLimits::UNLIMITED);
        a.load_glyph_path(&font, glyph, 0).unwrap();
        a.get_glyph_width(&font, glyph, 0, 0);
        a.load_glyph_path(&font, u32::MAX - 1, 0);
        assert!(memory.usage().glyph_cache_bytes > 0);
        memory.trim();
        assert_eq!((a.path_count(), a.width_count()), (0, 0));
        assert_eq!(memory.usage().glyph_cache_bytes, 0);
        assert_eq!(memory.limits(), FontMemoryLimits::UNLIMITED);
        drop((a, b, c));
        assert_eq!(memory.usage().glyph_cache_bytes, 0);
    }

    #[test]
    fn independent_instances_and_rejected_real_glyphs_release_scratch() {
        let a = Rc::new(FontMemory::default());
        let b = Rc::new(FontMemory::default());
        let face = CfxFace::new_with_memory(FONT, 0, a.clone()).unwrap();
        let glyph = face.unicode_char_index('A' as u32);
        let draw = |scale| {
            face.render_glyph(
                glyph,
                false,
                false,
                CfxMatrix::new(scale, 0.0, 0.0, scale, 0.0, 0.0),
                0,
                FontAntiAliasingMode::Lcd,
                None,
            )
        };
        let image = draw(24.0).unwrap();
        assert!(a.usage().glyph_scratch_bytes > 0);
        b.set_limits(FontMemoryLimits {
            glyph_cache_bytes: 0,
            glyph_scratch_bytes: 0,
        });
        b.trim();
        assert!(a.usage().glyph_scratch_bytes > 0);
        a.set_limits(FontMemoryLimits {
            glyph_cache_bytes: 0,
            glyph_scratch_bytes: 64,
        });
        assert_eq!(a.usage().glyph_scratch_bytes, 0);
        assert!(draw(4096.0).is_none());
        assert!(a.usage().glyph_scratch_bytes <= 64);
        assert_eq!(a.usage().active_scratch, 0);
        assert_eq!(
            draw(24.0).unwrap().bitmap().buffer(),
            image.bitmap().buffer()
        );
        a.trim();
        assert_eq!(a.usage().glyph_scratch_bytes, 0);
    }
}
