//! Shared stroke-tolerant raster comparison for C API tests and diff tools.
//! Pure Rust, with no image decoder, renderer, or experimental-tool dependencies.

pub mod transport;

/// Strongest eight-connected residual error mass for tightly packed BGRA images.
/// Composites onto white, permits radius-2 coverage redistribution and 10% local
/// stroke-contrast leeway, then sums each region above the .01 noise floor.
/// One score unit is one fully wrong pixel of maximum RGB-channel error.
/// Acceptance limits belong to callers; this function does not decide pass/fail.
/// Panics for zero dimensions, overflow, or mismatched buffer lengths.
pub fn score_bgra(a: &[u8], b: &[u8], width: usize, height: usize) -> f64 {
    analyze_bgra(a, b, width, height)
        .strongest_region
        .map_or(0., |r| r.mass)
}

/// Diagnostics for the same comparison, without changing its score.
#[derive(Debug, Default)]
pub struct RasterAnalysis {
    pub strongest_region: Option<ResidualRegion>,
    pub affected_pixels: usize,
    pub total_mass: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct ResidualRegion {
    pub mass: f64,
    pub pixels: usize,
    pub peak: f32,
    /// Inclusive bounds in the original image, [left, top, right, bottom].
    pub bounds: [usize; 4],
}

pub fn analyze_bgra(a: &[u8], b: &[u8], width: usize, height: usize) -> RasterAnalysis {
    analyze_bgra_with_leeway(a, b, width, height, 0)
}

/// Corpus-only calibration can allow bounded channel quantization after
/// transport. Zero preserves the C API metric's existing exact behavior.
pub fn analyze_bgra_with_leeway(
    a: &[u8],
    b: &[u8],
    width: usize,
    height: usize,
    channel_leeway: u8,
) -> RasterAnalysis {
    analyze_bgra_with_options(a, b, width, height, channel_leeway, 1.)
}

/// An edge mass divisor discounts coverage imbalance only where both images
/// contain local contrast. Missing features and new seams on flat backgrounds
/// cannot provide their own allowance. The legacy C API metric uses 1.
pub fn analyze_bgra_with_options(
    a: &[u8],
    b: &[u8],
    width: usize,
    height: usize,
    channel_leeway: u8,
    edge_mass_divisor: f32,
) -> RasterAnalysis {
    assert!(edge_mass_divisor.is_finite() && edge_mass_divisor >= 1.);
    assert!(width > 0 && height > 0);
    let len = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .expect("raster dimensions overflow");
    assert_eq!(a.len(), len);
    assert_eq!(b.len(), len);
    // Crop unchanged margins, retaining the full radius-2 contrast context.
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (p, (a, b)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        if a != b {
            left = left.min(p % width);
            right = right.max(p % width);
            top = top.min(p / width);
            bottom = bottom.max(p / width);
        }
    }
    if left == width {
        return RasterAnalysis::default();
    }
    left = left.saturating_sub(2);
    top = top.saturating_sub(2);
    right = (right + 2).min(width - 1);
    bottom = (bottom + 2).min(height - 1);
    let (w, h) = (right - left + 1, bottom - top + 1);
    let rgb = |image: &[u8]| {
        let mut result = Vec::with_capacity(w * h * 3);
        for y in top..=bottom {
            for x in left..=right {
                let p = &image[(y * width + x) * 4..][..4];
                for c in [2, 1, 0] {
                    result.push(
                        ((u32::from(p[c]) * u32::from(p[3]) + 255 * (255 - u32::from(p[3])) + 127)
                            / 255) as u8,
                    );
                }
            }
        }
        result
    };
    let (a, b) = (rgb(a), rgb(b));
    let mut residual = transport::compare_with_tolerances(
        &a,
        &b,
        w,
        h,
        2,
        true,
        0.1,
        channel_leeway,
        edge_mass_divisor,
    )
    .tolerant_residual;
    let reverse = transport::compare_with_tolerances(
        &b,
        &a,
        w,
        h,
        2,
        true,
        0.1,
        channel_leeway,
        edge_mass_divisor,
    )
    .tolerant_residual;
    for (a, b) in residual.iter_mut().zip(reverse) {
        *a = a.max(b);
    }
    let mut analysis = RasterAnalysis::default();
    let mut stack = Vec::new();
    for seed in 0..residual.len() {
        if residual[seed] <= 0.01 {
            continue;
        }
        let mut mass = f64::from(residual[seed]);
        let mut pixels = 1;
        let mut peak = residual[seed];
        let (sx, sy) = (seed % w + left, seed / w + top);
        let mut bounds = [sx, sy, sx, sy];
        residual[seed] = 0.;
        stack.push(seed);
        while let Some(p) = stack.pop() {
            let (x, y) = (p % w, p / w);
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let q = yy * w + xx;
                    if residual[q] > 0.01 {
                        mass += f64::from(residual[q]);
                        pixels += 1;
                        peak = peak.max(residual[q]);
                        bounds[0] = bounds[0].min(xx + left);
                        bounds[1] = bounds[1].min(yy + top);
                        bounds[2] = bounds[2].max(xx + left);
                        bounds[3] = bounds[3].max(yy + top);
                        residual[q] = 0.;
                        stack.push(q);
                    }
                }
            }
        }
        analysis.affected_pixels += pixels;
        analysis.total_mass += mass;
        if analysis.strongest_region.is_none_or(|r| mass > r.mass) {
            analysis.strongest_region = Some(ResidualRegion {
                mass,
                pixels,
                peak,
                bounds,
            });
        }
    }
    analysis
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_edge_weighting_keeps_missing_features_and_flat_seams_strict() {
        let (w, h) = (64, 64);
        let score = |a: &[u8], b: &[u8]| {
            analyze_bgra_with_options(a, b, w, h, 0, 16.)
                .strongest_region
                .map_or(0., |r| r.mass)
        };
        let white = vec![255; w * h * 4];
        let mut dot = white.clone();
        for y in 20..22 {
            for x in 20..22 {
                dot[(y * w + x) * 4..][..3].fill(0);
            }
        }
        assert!(
            score(&dot, &white) > 2.,
            "a missing isolated 2x2 dot must fail"
        );
        let mut stem = white.clone();
        for y in 8..40 {
            stem[(y * w + 20) * 4..][..3].fill(0);
        }
        assert!(score(&stem, &white) > 2., "a missing thin stem must fail");
        let mut adjusted = stem.clone();
        for y in 8..40 {
            adjusted[(y * w + 20) * 4..][..3].fill(64);
        }
        assert!(score_bgra(&stem, &adjusted, w, h) > 2.);
        assert!(
            score(&stem, &adjusted) <= 2.,
            "coverage imbalance on existing strokes is discounted"
        );
        let mut gray = vec![128; w * h * 4];
        for p in gray.chunks_exact_mut(4) {
            p[3] = 255;
        }
        let mut seam = gray.clone();
        for y in 8..40 {
            seam[(y * w + 20) * 4..][..3].fill(160);
        }
        assert!(
            score(&gray, &seam) > 2.,
            "a new seam cannot supply contrast in both images"
        );
        let mut tiny_rounding = gray.clone();
        for p in tiny_rounding.chunks_exact_mut(4) {
            p[..3].fill(130);
        }
        assert_eq!(score(&gray, &tiny_rounding), 0.);
        for p in tiny_rounding.chunks_exact_mut(4) {
            p[..3].fill(132);
        }
        assert!(
            score(&gray, &tiny_rounding) > 2.,
            "four levels in a flat fill must remain visible to the gate"
        );
        let mut wrong_fill = gray.clone();
        for p in wrong_fill.chunks_exact_mut(4) {
            p[..3].fill(144);
        }
        assert!(score(&gray, &wrong_fill) > 2.);
        assert_eq!(score(&dot, &white), score(&white, &dot));
    }
    #[test]
    fn missing_ink_and_seams_fail_but_small_stroke_changes_pass() {
        let mut a = vec![255; 32 * 32 * 4];
        for y in 8..24 {
            a[(y * 32 + 16) * 4..][..3].fill(0);
        }
        assert_eq!(score_bgra(&a, &a, 32, 32), 0.);
        let mut lighter = a.clone();
        for y in 8..24 {
            lighter[(y * 32 + 16) * 4..][..3].fill(13);
        }
        assert_eq!(score_bgra(&a, &lighter, 32, 32), 0.);
        assert!(score_bgra(&a, &vec![255; a.len()], 32, 32) > 1.);
        let flat = vec![128; 32 * 32 * 4];
        let mut seam = flat.clone();
        for y in 0..32 {
            seam[(y * 32 + 16) * 4..][..3].fill(160);
        }
        assert!(score_bgra(&flat, &seam, 32, 32) > 1.);
    }
}
