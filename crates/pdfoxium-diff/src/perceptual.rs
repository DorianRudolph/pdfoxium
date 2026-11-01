//! Experimental measurements only: no universal visual acceptance threshold.
use flip_rs::ErrorMap;
use serde::Serialize;
use std::collections::BTreeMap;

/// FLIP compares sRGB; composite alpha onto the corpus's white viewing surface.
/// Alpha differences themselves remain covered by the existing raw RGBA metric.
pub fn rgb_over_white(
    rgba: &image::RgbaImage,
) -> Result<flip_rs::RgbImage<u8>, flip_rs::FlipError> {
    let pixels = rgba
        .pixels()
        .flat_map(|p| {
            [0, 1, 2].map(|c| {
                ((u32::from(p[c]) * u32::from(p[3]) + 255 * (255 - u32::from(p[3])) + 127) / 255)
                    as u8
            })
        })
        .collect();
    flip_rs::RgbImage::new(rgba.width() as usize, rgba.height() as usize, pixels)
}

#[derive(Debug, Serialize)]
pub struct WindowMaximum {
    pub mean: f64,
    /// Top-left bitmap coordinates, with ties resolved in row-major order.
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Serialize)]
pub struct Metrics {
    pub mean: f32,
    pub peak: f32,
    pub counts_above: BTreeMap<String, usize>,
    pub max_mean_4x4: WindowMaximum,
    pub max_mean_8x8: WindowMaximum,
}

pub fn summarize(map: &ErrorMap) -> Metrics {
    // ErrorMap::statistics also clones/sorts the full map for quartiles. Only
    // compute the mean and peak we actually use; keep all pooling linear.
    let mut sum = 0.0_f32;
    let mut peak = 0.0_f32;
    let thresholds = [0.05_f32, 0.1, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7];
    let mut counts = [0; 10];
    for &value in map.pixels() {
        sum += value;
        peak = peak.max(value);
        for (count, threshold) in counts.iter_mut().zip(thresholds) {
            *count += usize::from(value > threshold);
        }
    }
    Metrics {
        mean: sum / map.pixels().len() as f32,
        peak,
        counts_above: thresholds
            .into_iter()
            .zip(counts)
            .map(|(t, n)| (format!("{t:.2}"), n))
            .collect(),
        max_mean_4x4: max_window_mean(map.pixels(), map.width(), map.height(), 4),
        max_mean_8x8: max_window_mean(map.pixels(), map.width(), map.height(), 8),
    }
}

/// Maximum sliding box mean for a nonnegative finite scalar map, including
/// every valid right/bottom edge position. Tiny images use available dimensions.
/// Panics for zero dimensions/kernel size or a map with the wrong length.
/// Two running box sums take O(width * height) time and O(width) scratch,
/// independent of kernel area; f64 sums limit cancellation from sliding.
pub fn max_window_mean(values: &[f32], width: usize, height: usize, side: usize) -> WindowMaximum {
    assert!(width > 0 && height > 0 && side > 0);
    assert_eq!(values.len(), width.checked_mul(height).unwrap());
    let wx = side.min(width);
    let wy = side.min(height);
    let mut columns = vec![0.0_f64; width];
    let mut best = WindowMaximum {
        mean: 0.,
        x: 0,
        y: 0,
        width: wx,
        height: wy,
    };
    let mut best_sum = 0.;
    for y in 0..height {
        for (x, sum) in columns.iter_mut().enumerate() {
            *sum += f64::from(values[y * width + x]);
            if y >= wy {
                *sum -= f64::from(values[(y - wy) * width + x]);
            }
        }
        if y + 1 < wy {
            continue;
        }
        let mut sum = columns[..wx].iter().sum::<f64>();
        for x in 0..=width - wx {
            if x > 0 {
                sum += columns[x + wx - 1] - columns[x - 1];
            }
            if sum > best_sum {
                best_sum = sum;
                best.x = x;
                best.y = y + 1 - wy;
            }
        }
    }
    best.mean = best_sum / (wx * wy) as f64;
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_use_unquantized_strict_thresholds_and_rgba_uses_white_background() {
        let map = ErrorMap::new(3, 1, vec![0., 0.5, 0.75]).unwrap();
        let metrics = summarize(&map);
        assert_eq!(metrics.counts_above["0.50"], 1);
        assert_eq!(metrics.max_mean_4x4.width, 3);
        assert_eq!(metrics.max_mean_4x4.height, 1);
        assert_eq!(metrics.max_mean_4x4.mean, 1.25 / 3.);
        let rgba =
            image::RgbaImage::from_raw(3, 1, vec![0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 128]).unwrap();
        assert_eq!(
            rgb_over_white(&rgba).unwrap().pixels(),
            &[0, 0, 0, 255, 255, 255, 127, 127, 127]
        );
    }

    #[test]
    fn running_sums_match_exhaustive_windows_including_tiny_images_and_edges() {
        let mut seed = 123456789_u32;
        for (w, h) in [(1, 1), (2, 13), (17, 3), (19, 21), (65, 33)] {
            let values: Vec<_> = (0..w * h)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed >> 8) as f32 / 16777216.
                })
                .collect();
            for side in [4, 8] {
                let result = max_window_mean(&values, w, h, side);
                let (wx, wy) = (side.min(w), side.min(h));
                let mut expected = (0., 0, 0);
                for y in 0..=h - wy {
                    for x in 0..=w - wx {
                        let sum: f64 = (y..y + wy)
                            .flat_map(|yy| {
                                values[yy * w + x..yy * w + x + wx]
                                    .iter()
                                    .map(|&v| f64::from(v))
                            })
                            .sum();
                        let mean = sum / (wx * wy) as f64;
                        if mean > expected.0 {
                            expected = (mean, x, y);
                        }
                    }
                }
                assert!((result.mean - expected.0).abs() < 1e-12);
                assert_eq!((result.x, result.y), (expected.1, expected.2));
            }
        }
    }

    #[test]
    fn detects_bottom_right_hotspot_without_dilution_from_blank_margins() {
        let (w, h) = (91, 77);
        let mut values = vec![0.; w * h];
        for y in h - 4..h {
            values[y * w + w - 4..(y + 1) * w].fill(0.75);
        }
        let best = max_window_mean(&values, w, h, 4);
        assert_eq!((best.mean, best.x, best.y), (0.75, w - 4, h - 4));
        assert_eq!(max_window_mean(&values, w, h, 8).mean, 0.75 / 4.);
        let zero = max_window_mean(&vec![0.; w * h], w, h, 4);
        assert_eq!((zero.mean, zero.x, zero.y), (0., 0, 0));
    }
}
