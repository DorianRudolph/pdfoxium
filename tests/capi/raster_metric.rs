//! Thin test-only ABI bridge; never linked into either PDF engine.
// Compile the dependency-free normal comparison module directly, avoiding
// linking the diff tool's PDFium loader, networking, and image decoders here.
#[path = "../../crates/pdfoxium-diff/src/raster/mod.rs"]
mod raster;

/// Both pointers must reference width*height*4 initialized BGRA bytes, alive
/// and immutable for this call. The C++ harness validates dimensions/lengths.
#[no_mangle]
pub unsafe extern "C" fn pdfoxium_test_raster_score(
    a: *const u8,
    b: *const u8,
    width: usize,
    height: usize,
) -> f64 {
    let Some(len) = width.checked_mul(height).and_then(|n| n.checked_mul(4)) else {
        return f64::INFINITY;
    };
    if a.is_null() || b.is_null() || len == 0 || len > isize::MAX as usize {
        return f64::INFINITY;
    }
    std::panic::catch_unwind(|| {
        // SAFETY: the private test bridge's caller guarantees both buffers.
        raster::score_bgra(
            unsafe { std::slice::from_raw_parts(a, len) },
            unsafe { std::slice::from_raw_parts(b, len) },
            width,
            height,
        )
    })
    .unwrap_or(f64::INFINITY)
}
