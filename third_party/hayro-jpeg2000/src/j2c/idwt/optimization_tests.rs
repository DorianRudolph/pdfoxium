//! Regression oracle for the fused inverse 5/3 row reconstruction.
//!
//! JPEG2000 Annex F.3.3 interleaves the low/high subbands before F.3.8.1
//! reconstructs the row. Keep that two-step scalar route independent of the
//! optimized direct-from-subband implementation, including its edge handling.

use super::*;
use alloc::vec;
use alloc::vec::Vec;

fn reference_row(low: &[f32], high: &[f32], origin: usize) -> Vec<f32> {
    let width = low.len() + high.len();
    let mut row = vec![0.0; width];
    let mut low_samples = low.iter();
    let mut high_samples = high.iter();
    for (column, sample) in row.iter_mut().enumerate() {
        *sample = if (column + origin) % 2 == 0 {
            *low_samples.next().unwrap()
        } else {
            *high_samples.next().unwrap()
        };
    }
    assert!(low_samples.next().is_none());
    assert!(high_samples.next().is_none());
    filter_row(
        &mut row,
        width,
        origin,
        WaveletTransform::Reversible53,
        true,
    );
    row
}

fn check_row<S: Simd>(simd: S, interleaved: &[f32], origin: usize, label: &str) {
    let mut low = Vec::new();
    let mut high = Vec::new();
    for (column, &value) in interleaved.iter().enumerate() {
        if (column + origin) % 2 == 0 {
            low.push(value);
        } else {
            high.push(value);
        }
    }
    let expected = reference_row(&low, &high, origin);
    // Nonzero sentinels verify the passed output subslice remains the only
    // destination, including for singleton and incomplete SIMD groups.
    let sentinel = f32::from_bits(0xdead_beef);
    let width = interleaved.len();
    let mut actual = vec![sentinel; width + 4];
    reconstruct_reversible_row(simd, &low, &high, &mut actual[2..width + 2], origin);
    for index in [0, 1, width + 2, width + 3] {
        assert_eq!(actual[index].to_bits(), sentinel.to_bits());
    }
    for (column, (&actual, &expected)) in actual[2..width + 2].iter().zip(&expected).enumerate() {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "width={width} origin={origin} column={column} pattern={label}"
        );
    }
}

#[test]
fn fused_reversible_rows_match_scalar_interleave_then_lift() {
    dispatch!(Level::new(), simd => {
        for width in (1usize..=40).chain([63, 64, 65, 127, 128, 129, 1023]) {
            for origin in 0..4 {
                for magnitude in [
                    f32::MIN_POSITIVE,
                    1e-30f32,
                    0.25,
                    1.0,
                    65536.0,
                    16777216.0,
                    1e30,
                ] {
                    let row: Vec<f32> = (0..width)
                        .map(|column| {
                            let integer = ((column * 137 + column / 7 * 31) % 513) as i32 - 256;
                            integer as f32 * magnitude
                        })
                        .collect();
                    check_row(simd, &row, origin, "scaled signed samples");
                    let fractional: Vec<f32> = row.iter().map(|sample| sample + 0.5).collect();
                    check_row(simd, &fractional, origin, "fractional signed samples");
                }
                for bits in [0x0000_0000, 0x8000_0000, 0x0000_0001, 0x8000_0001] {
                    let constant = vec![f32::from_bits(bits); width];
                    check_row(simd, &constant, origin, "constant signed zero/subnormal");
                    let alternating: Vec<f32> = (0..width)
                        .map(|column| f32::from_bits(bits ^ ((column as u32 & 1) << 31)))
                        .collect();
                    check_row(simd, &alternating, origin, "alternating signed zero/subnormal");
                }
                for position in [0, width / 2, width - 1] {
                    let mut impulse = vec![0.0; width];
                    impulse[position] = -32767.5;
                    check_row(simd, &impulse, origin, "negative impulse");
                }
            }
        }
    });
}

#[test]
fn fused_reversible_rows_preserve_rounding_for_varied_finite_bit_patterns() {
    dispatch!(Level::new(), simd => {
        let mut state = 0x6a09_e667_f3bc_c909u64;
        for width in (1usize..=40).chain([63, 64, 65, 127, 128, 129, 1023]) {
            for origin in 0..4 {
                for _ in 0..8 {
                    let row: Vec<f32> = (0..width).map(|_| {
                        state ^= state << 13;
                        state ^= state >> 7;
                        state ^= state << 17;
                        let bits = state as u32;
                        // Vary mantissas, signs, subnormals and widely separated
                        // exponents. Bound the exponent so intermediate sums
                        // stay finite, making bit equality unambiguous.
                        let exponent = ((state >> 32) as u32 % 248) << 23;
                        f32::from_bits((bits & 0x807f_ffff) | exponent)
                    }).collect();
                    check_row(simd, &row, origin, "deterministic finite bit patterns");
                }
            }
        }
    });
}
