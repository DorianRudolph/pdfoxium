//! Differential C ABI checks using dynamically loaded PDFium bindings.

use std::path::{Path, PathBuf};

pub mod catalog;
pub mod fetch;
pub mod perceptual;
pub mod process;
pub mod raster;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Portable text rendering flag from pinned `public/fpdfview.h`.
/// pdfium-render does not publicly re-export the rendering flag constants.
pub const FPDF_NO_NATIVETEXT: i32 = 0x04;

pub fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn library_path(backend: &str) -> PathBuf {
    let root = project_root();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(|path| root.join(path))
        .unwrap_or_else(|| root.join("target"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return match backend {
        "native" => root.join("vendor/pdfium/chromium-8100/linux-x64/libpdfium.so"),
        "rust" => target.join(profile).join("libpdfoxium_capi.so"),
        _ => panic!("unknown backend: {backend}"),
    };
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return match backend {
        "native" => root.join("vendor/pdfium/chromium-8100/mac-arm64/libpdfium.dylib"),
        "rust" => target.join(profile).join("libpdfoxium_capi.dylib"),
        _ => panic!("unknown backend: {backend}"),
    };
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64")
    )))]
    panic!("Bundled oracle libraries support Linux x64 and macOS arm64");
}
