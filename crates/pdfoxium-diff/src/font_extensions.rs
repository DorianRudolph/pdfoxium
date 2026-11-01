//! Optional pdfoxium ABI; the native oracle and older builds need not export it.
use super::{library_path, Result};

type Set = unsafe extern "C" fn(u64, u64) -> i32;
type Get = unsafe extern "C" fn(*mut u64, *mut u64) -> i32;
type Trim = unsafe extern "C" fn() -> i32;

pub struct FontExtensions {
    _library: libloading::Library,
    set: Set,
    limits: Get,
    usage: Get,
    trim: Trim,
}
impl FontExtensions {
    pub fn load(backend: &str) -> Option<Self> {
        if backend != "rust" {
            return None;
        }
        // SAFETY: Explicit library path; these signatures are declared in
        // pdfoxium.h. The owner keeps all loaded function pointers valid.
        unsafe {
            let library = libloading::Library::new(library_path(backend)).ok()?;
            Some(Self {
                set: *library.get(b"PDFOXIUM_SetFontMemoryLimits\0").ok()?,
                limits: *library.get(b"PDFOXIUM_GetFontMemoryLimits\0").ok()?,
                usage: *library.get(b"PDFOXIUM_GetFontMemoryUsage\0").ok()?,
                trim: *library.get(b"PDFOXIUM_TrimFontMemory\0").ok()?,
                _library: library,
            })
        }
    }
    fn get(&self, f: Get) -> Option<(u64, u64)> {
        let (mut a, mut b) = (0, 0);
        // SAFETY: Both writable outputs are live for this call.
        (unsafe { f(&mut a, &mut b) } != 0).then_some((a, b))
    }
    pub fn limits(&self) -> Option<(u64, u64)> {
        self.get(self.limits)
    }
    pub fn usage(&self) -> Option<(u64, u64)> {
        self.get(self.usage)
    }
    pub fn set(&self, cache: u64, scratch: u64) -> Result<()> {
        // SAFETY: The function takes only byte counts.
        if unsafe { (self.set)(cache, scratch) } == 0 {
            return Err("font memory configuration failed".into());
        }
        Ok(())
    }
    pub fn trim(&self) -> Result<()> {
        // SAFETY: This is the initialized library's no-argument trim API.
        if unsafe { (self.trim)() } == 0 {
            return Err("font memory trim failed".into());
        }
        Ok(())
    }
}

// Test harness controls, not renderer environment-variable configuration.
pub fn configure_test_limits(backend: &str) -> Result<()> {
    let cache = std::env::var("PDFOXIUM_TEST_CACHE_BYTES").ok();
    let scratch = std::env::var("PDFOXIUM_TEST_SCRATCH_BYTES").ok();
    if backend != "rust" || (cache.is_none() && scratch.is_none()) {
        return Ok(());
    }
    let ext = FontExtensions::load(backend).ok_or("font memory extensions unavailable")?;
    let limits = ext
        .limits()
        .ok_or("font limits require library initialization")?;
    ext.set(
        cache.map(|s| s.parse()).transpose()?.unwrap_or(limits.0),
        scratch.map(|s| s.parse()).transpose()?.unwrap_or(limits.1),
    )
}
