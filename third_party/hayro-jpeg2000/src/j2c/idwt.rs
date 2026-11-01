//! Performing the inverse discrete wavelet transform, as specified in Annex F.

use alloc::vec;
use alloc::vec::Vec;

use super::build::{Decomposition, SubBand};
use super::codestream::WaveletTransform;
use super::decode::{DecompositionStorage, TileDecodeContext};
use super::rect::IntRect;
use crate::j2c::Header;
use crate::math::{self, Level, SIMD_WIDTH, Simd, dispatch, f32x8};

/// The output from performing the IDWT operation.
pub(crate) struct IDWTOutput {
    /// The buffer that will hold the final coefficients.
    pub(crate) coefficients: Vec<f32>,
    /// The rect that the coefficients belong to. This will be equivalent
    /// to the rectangle that forms the smallest decomposition level. It does
    /// not have to be equivalent to the original size of the tile, as the
    /// sub-bands that form a tile aren't necessarily aligned to it. Therefore,
    /// the samples need to be trimmed to the tile rectangle afterward.
    pub(crate) rect: IntRect,
}

impl Default for IDWTOutput {
    fn default() -> Self {
        Self {
            coefficients: vec![],
            rect: IntRect::from_ltrb(0, 0, u32::MAX, u32::MAX),
        }
    }
}

struct IDWTTempOutput {
    rect: IntRect,
}

/// Apply the inverse discrete wavelet transform (see Annex F). The output
/// will be transformed samples covering the rectangle of the smallest
/// decomposition level.
pub(crate) fn apply(
    storage: &DecompositionStorage<'_>,
    tile_ctx: &mut TileDecodeContext,
    component_idx: usize,
    header: &Header<'_>,
    transform: WaveletTransform,
    pdfium: bool,
) {
    let tile_decompositions = &storage.tile_decompositions[component_idx];

    let mut decompositions = &storage.decompositions[tile_decompositions.decompositions.clone()];
    // If we requested a lower resolution level, we can skip some decompositions.
    let count = if pdfium {
        usize::from(storage.pdfium_resno_decoded[component_idx])
    } else {
        decompositions
            .len()
            .saturating_sub(header.skipped_resolution_levels as usize)
    };
    decompositions = &decompositions[..count];
    let ll_sub_band = &storage.sub_bands[tile_decompositions.first_ll_sub_band];

    // To explain a bit why we have this scratch buffer and another coefficient
    // buffer: During IDWT, we need to continuously interleave the 4 sub-bands
    // into a new buffer, which is then either returned or used as the input
    // for the next decomposition, etc. It would be very inefficient if we
    // kept allocating new buffers each time. Therefore, we try to reuse them,
    // not only for all decompositions of a single tile, but all decompositions
    // of _all_ tiles.
    // Due to the fact that the output from the previous iteration might be
    // used as the input of the next, we need two separate buffers, which
    // are continuously swapped.
    let (scratch_buf, output) = (&mut tile_ctx.idwt_scratch_buffer, &mut tile_ctx.idwt_output);

    let estimate_buffer_size = |decomposition: &Decomposition| {
        let total_width = decomposition.rect.width() as usize;
        let total_height = decomposition.rect.height() as usize;

        let min = total_width * total_height;
        // Different sub-bands can have shifts by one, so add padding
        // for the maximum case.
        let max = (total_width + 1) * (total_height + 1);

        (min, max)
    };

    if decompositions.is_empty() {
        // Single decomposition, just copy the coefficients from the sub-band.
        output.coefficients.clear();
        output
            .coefficients
            .extend_from_slice(&storage.coefficients[ll_sub_band.coefficients.clone()]);

        output.rect = ll_sub_band.rect;

        return;
    }

    // The coefficient array will always be the one that holds the coefficients
    // from the highest decomposition. Therefore, reserve as much.
    let (s_min, s_max) = estimate_buffer_size(decompositions.last().unwrap());
    if output.coefficients.len() < s_min {
        output
            .coefficients
            .reserve_exact(s_max - output.coefficients.len());
    }

    if decompositions.len() > 1 {
        // Due to the above, the intermediate buffer will never need more than
        // the second-highest decomposition.
        let (s_min, s_max) = estimate_buffer_size(&decompositions[decompositions.len() - 2]);

        if scratch_buf.len() < s_min {
            scratch_buf.reserve_exact(s_max - scratch_buf.len());
        }
    }

    // Determine which buffer we should use first, such that the `coefficients`
    // array will always hold the final values.
    let mut use_scratch = decompositions.len().is_multiple_of(2);

    let mut temp_output = filter_2d(
        IDWTInput::from_sub_band(ll_sub_band, storage),
        if use_scratch {
            scratch_buf
        } else {
            &mut output.coefficients
        },
        &decompositions[0],
        transform,
        storage,
        pdfium,
    );

    for decomposition in decompositions.iter().skip(1) {
        use_scratch = !use_scratch;

        temp_output = if use_scratch {
            filter_2d(
                IDWTInput::from_output(&output.coefficients),
                scratch_buf,
                decomposition,
                transform,
                storage,
                pdfium,
            )
        } else {
            filter_2d(
                IDWTInput::from_output(scratch_buf),
                &mut output.coefficients,
                decomposition,
                transform,
                storage,
                pdfium,
            )
        };
    }

    output.rect = temp_output.rect;
}

struct IDWTInput<'a> {
    coefficients: &'a [f32],
}

impl<'a> IDWTInput<'a> {
    fn from_sub_band(sub_band: &'a SubBand, storage: &'a DecompositionStorage<'_>) -> Self {
        IDWTInput {
            coefficients: &storage.coefficients[sub_band.coefficients.clone()],
        }
    }

    fn from_output(coefficients: &'a [f32]) -> Self {
        IDWTInput { coefficients }
    }
}

/// The `2D_SR` procedure illustrated in Figure F.6.
fn filter_2d(
    // The LL sub band of the given decomposition level.
    input: IDWTInput<'_>,
    coefficients: &mut Vec<f32>,
    decomposition: &Decomposition,
    transform: WaveletTransform,
    storage: &DecompositionStorage<'_>,
    pdfium: bool,
) -> IDWTTempOutput {
    // Reconstruct the first horizontal 5/3 lift while interleaving the bands;
    // the 9/7 path retains its separate horizontal filter.
    interleave_samples(
        input,
        decomposition,
        coefficients,
        storage,
        transform == WaveletTransform::Reversible53,
    );

    if decomposition.rect.width() > 0 && decomposition.rect.height() > 0 {
        if transform != WaveletTransform::Reversible53 {
            filter_horizontal(coefficients, decomposition.rect, transform, pdfium);
        }
        filter_vertical(coefficients, decomposition.rect, transform, pdfium);
    }

    IDWTTempOutput {
        rect: decomposition.rect,
    }
}

/// The `2D_INTERLEAVE` procedure described in F.3.3.
fn interleave_samples(
    input: IDWTInput<'_>,
    decomposition: &Decomposition,
    coefficients: &mut Vec<f32>,
    storage: &DecompositionStorage<'_>,
    reconstruct_53: bool,
) {
    let level = Level::new();
    dispatch!(level, simd => {
        interleave_samples_inner::<_>(simd, input, decomposition, coefficients, storage, reconstruct_53);
    });
}

#[inline(always)]
fn interleave_samples_inner<S: Simd>(
    simd: S,
    input: IDWTInput<'_>,
    decomposition: &Decomposition,
    coefficients: &mut Vec<f32>,
    storage: &DecompositionStorage<'_>,
    reconstruct_53: bool,
) {
    let width = decomposition.rect.width() as usize;
    let height = decomposition.rect.height() as usize;

    // Just a sanity check. We should have allocated enough upfront before
    // starting the IDWT.
    assert!(coefficients.capacity() >= width * height);

    // The cleaner way would be to first clear and then resize, so that we
    // have a clean buffer with just zeroes. However, this is actually not
    // necessary, because when interleaving and generating the border values
    // we will replace all the data anyway, so we can save the cost of
    // the clear operation.
    coefficients.resize(width * height, 0.0);

    let IntRect {
        x0: u0,
        x1: u1,
        y0: v0,
        y1: v1,
    } = decomposition.rect;

    let ll = input.coefficients;
    let hl = &storage.coefficients[storage.sub_bands[decomposition.sub_bands[0]]
        .coefficients
        .clone()];
    let lh = &storage.coefficients[storage.sub_bands[decomposition.sub_bands[1]]
        .coefficients
        .clone()];
    let hh = &storage.coefficients[storage.sub_bands[decomposition.sub_bands[2]]
        .coefficients
        .clone()];

    // See Figure F.8.
    let num_u_low = (u1.div_ceil(2) - u0.div_ceil(2)) as usize;
    let num_u_high = (u1 / 2 - u0 / 2) as usize;
    let num_v_low = (v1.div_ceil(2) - v0.div_ceil(2)) as usize;
    let num_v_high = (v1 / 2 - v0 / 2) as usize;

    // Depending on whether the start row is even or odd, either LL/HL comes first
    // or HL/HH.

    let (first_w, second_w) = if u0 % 2 == 0 {
        (num_u_low, num_u_high)
    } else {
        (num_u_high, num_u_low)
    };

    let even_row_start = if v0 % 2 == 0 { 0 } else { 1 };
    let odd_row_start = if v0 % 2 == 0 { 1 } else { 0 };

    // Determine whether LL or HL is the band in the first column.
    let (first_even, second_even) = if u0 % 2 == 0 { (ll, hl) } else { (hl, ll) };
    interleave_rows(
        simd,
        first_even,
        second_even,
        first_w,
        second_w,
        coefficients,
        width,
        height,
        even_row_start,
        num_v_low,
        reconstruct_53,
        u0 as usize,
    );

    // Determine whether LH or HH is the band in the first column.
    let (first_odd, second_odd) = if u0 % 2 == 0 { (lh, hh) } else { (hh, lh) };
    interleave_rows(
        simd,
        first_odd,
        second_odd,
        first_w,
        second_w,
        coefficients,
        width,
        height,
        odd_row_start,
        num_v_high,
        reconstruct_53,
        u0 as usize,
    );
}

#[inline(always)]
fn interleave_rows<S: Simd>(
    simd: S,
    first_band: &[f32],
    second_band: &[f32],
    first_w: usize,
    second_w: usize,
    output: &mut [f32],
    width: usize,
    height: usize,
    start_row: usize,
    num_rows: usize,
    reconstruct_53: bool,
    x0: usize,
) {
    for v in 0..num_rows {
        let out_row = start_row + v * 2;
        if out_row >= height {
            break;
        }

        let first_row = &first_band[v * first_w..][..first_w];
        let second_row = &second_band[v * second_w..][..second_w];
        let out_slice = &mut output[out_row * width..][..width];

        if reconstruct_53 {
            let (low, high) = if x0 % 2 == 0 {
                (first_row, second_row)
            } else {
                (second_row, first_row)
            };
            reconstruct_reversible_row(simd, low, high, out_slice, x0);
        } else {
            interleave_row(simd, first_row, second_row, out_slice);
        }
    }
}

#[inline(always)]
fn interleave_row<S: Simd>(simd: S, first: &[f32], second: &[f32], output: &mut [f32]) {
    let num_pairs = first.len().min(second.len());
    let simd_chunks = num_pairs / SIMD_WIDTH;

    // Process as much as possible using SIMD.
    for i in 0..simd_chunks {
        let base = i * SIMD_WIDTH;
        let f = f32x8::from_slice(simd, &first[base..base + SIMD_WIDTH]);
        let s = f32x8::from_slice(simd, &second[base..base + SIMD_WIDTH]);

        f.zip_low(s).store(&mut output[base * 2..]);
        f.zip_high(s).store(&mut output[base * 2 + SIMD_WIDTH..]);
    }

    // Scalar remainder.
    for i in (simd_chunks * SIMD_WIDTH)..num_pairs {
        output[i * 2] = first[i];
        output[i * 2 + 1] = second[i];
    }

    // Handle extra element if first is longer.
    if first.len() > num_pairs {
        output[num_pairs * 2] = first[num_pairs];
    }
}

// Like pinned OpenJPEG's opj_idwt53_h, reconstruct from the contiguous
// low/high bands rather than interleaving then gathering the first lift back
// out of memory. Keep the existing float/FMA/floor order, including reflected
// edges; native's integer shifts are not interchangeable for large samples.
#[inline(always)]
fn reconstruct_reversible_row<S: Simd>(
    simd: S,
    low: &[f32],
    high: &[f32],
    output: &mut [f32],
    x0: usize,
) {
    let width = output.len();
    let first_low = x0 % 2;
    if width <= 1 {
        if let Some(value) = output.first_mut() {
            *value = if first_low == 0 {
                low[0]
            } else {
                high[0] * 0.5
            };
        }
        return;
    }
    let reconstruct = |k: usize| {
        let left = if first_low == 0 {
            k.saturating_sub(1)
        } else {
            k
        };
        let right = (k + first_low).min(high.len() - 1);
        low[k] - math::floor_f32(math::mul_add(high[left] + high[right], 0.25, 0.5))
    };
    let mut k = 0;
    if first_low == 0 {
        output[0] = reconstruct(0);
        output[1] = high[0];
        k = 1;
    }
    while k + SIMD_WIDTH <= low.len() && k + SIMD_WIDTH + first_low <= high.len() {
        let left_start = k - (1 - first_low);
        let right_start = k + first_low;
        let left = f32x8::from_slice(simd, &high[left_start..][..SIMD_WIDTH]);
        let right = f32x8::from_slice(simd, &high[right_start..][..SIMD_WIDTH]);
        let center = f32x8::from_slice(simd, &low[k..][..SIMD_WIDTH]);
        let even = center
            - (left + right)
                .mul_add_like_scalar(f32x8::splat(simd, 0.25), f32x8::splat(simd, 0.5))
                .floor();
        let odd = f32x8::from_slice(simd, &high[k..][..SIMD_WIDTH]);
        let (first, second) = if first_low == 0 {
            (even, odd)
        } else {
            (odd, even)
        };
        first.zip_low(second).store(&mut output[2 * k..]);
        first
            .zip_high(second)
            .store(&mut output[2 * k + SIMD_WIDTH..]);
        k += SIMD_WIDTH;
    }
    for i in k..low.len() {
        output[2 * i + first_low] = reconstruct(i);
        if i < high.len() {
            output[2 * i + 1 - first_low] = high[i];
        }
    }
    if high.len() > low.len() {
        output[width - 1] = high[high.len() - 1];
    }
    filter_step_horizontal_lanes(
        simd,
        output,
        width,
        1 - first_low,
        |s, left, right| s + math::floor_f32((left + right) * 0.5),
        |s, left, right| s + ((left + right) * 0.5).floor(),
    );
}

/// The `HOR_SR` procedure from F.3.4.
fn filter_horizontal(
    coefficients: &mut [f32],
    rect: IntRect,
    transform: WaveletTransform,
    pdfium: bool,
) {
    dispatch!(Level::new(), simd => {
        let width = rect.width() as usize;
        for row in coefficients.chunks_exact_mut(width).take(rect.height() as usize) {
            if width > 1 && transform == WaveletTransform::Irreversible97 {
                irreversible_filter_97i_impl(row, width, rect.x0 as usize, pdfium,
                    |row, first, factor| filter_step_horizontal_simd(simd, row, width, first, factor));
            } else if width > 1 && transform == WaveletTransform::Reversible53 {
                let first_even = rect.x0 as usize % 2;
                filter_step_horizontal_lanes(simd, row, width, first_even,
                    |s, left, right| s - math::floor_f32(math::mul_add(left + right, 0.25, 0.5)),
                    |s, left, right| s - (left + right).mul_add_like_scalar(
                        f32x8::splat(simd, 0.25), f32x8::splat(simd, 0.5)).floor());
                filter_step_horizontal_lanes(simd, row, width, 1 - first_even,
                    |s, left, right| s + math::floor_f32((left + right) * 0.5),
                    |s, left, right| s + ((left + right) * 0.5).floor());
            } else {
                filter_row(row, width, rect.x0 as usize, transform, pdfium);
            }
        }
    });
}

/// The `1D_SR` procedure from F.3.6.
fn filter_row(
    scanline: &mut [f32],
    width: usize,
    x0: usize,
    transform: WaveletTransform,
    pdfium: bool,
) {
    if width == 1 {
        if !x0.is_multiple_of(2) && !(pdfium && transform == WaveletTransform::Irreversible97) {
            scanline[0] *= 0.5;
        }

        return;
    }

    match transform {
        WaveletTransform::Reversible53 => reversible_filter_53r(scanline, width, x0),
        WaveletTransform::Irreversible97 => irreversible_filter_97i(scanline, width, x0, pdfium),
    }
}

/// The 1D FILTER 5-3R procedure from F.3.8.1.
fn reversible_filter_53r(scanline: &mut [f32], width: usize, x0: usize) {
    let first_even = x0 % 2;
    let first_odd = 1 - first_even;

    // Equation (F-5).
    // Originally: for i in (start / 2)..(end / 2 + 1).
    filter_step_horizontal(
        scanline,
        width,
        first_even,
        #[inline(always)]
        |s, left, right| s - math::floor_f32(math::mul_add(left + right, 0.25, 0.5)),
    );

    // Equation (F-6).
    // Originally: for i in (start / 2)..(end / 2).
    filter_step_horizontal(
        scanline,
        width,
        first_odd,
        #[inline(always)]
        |s, left, right| s + math::floor_f32((left + right) * 0.5),
    );
}

/// The 1D Filter 9-7I procedure from F.3.8.2.
fn irreversible_filter_97i(scanline: &mut [f32], width: usize, x0: usize, pdfium: bool) {
    irreversible_filter_97i_impl(scanline, width, x0, pdfium, |row, first, factor| {
        filter_step_horizontal(row, width, first, |s, left, right| {
            math::mul_add(left + right, factor, s)
        });
    });
}

#[inline(always)]
fn irreversible_filter_97i_impl(
    scanline: &mut [f32],
    width: usize,
    x0: usize,
    pdfium: bool,
    mut lift: impl FnMut(&mut [f32], usize, f32),
) {
    // Table F.4.
    const NEG_ALPHA: f32 = 1.586_134_3;
    const NEG_BETA: f32 = 0.052_980_117;
    const NEG_GAMMA: f32 = -0.882_911_1;
    const NEG_DELTA: f32 = -0.443_506_87;
    const KAPPA: f32 = 1.230_174_1;
    // Pinned dwt.c historic two_invK, coupled to native zero band gain in
    // tcd.c. BSD-2-Clause notices: ../../LICENSE-OPENJPEG-DWT.
    let inv_kappa_value: f32 = if pdfium { 1.625_732_422 } else { 1.0 / KAPPA };

    let first_even = x0 % 2;
    let first_odd = 1 - first_even;

    let (k0, k1) = if first_even == 0 {
        (KAPPA, inv_kappa_value)
    } else {
        (inv_kappa_value, KAPPA)
    };

    // Step 1 and 2.
    // Originally: for i in (start / 2 - 1)..(end / 2 + 2).
    // Originally: for i in (start / 2 - 2)..(end / 2 + 2).
    for i in (0..width.saturating_sub(1)).step_by(2) {
        scanline[i] *= k0;
        scanline[i + 1] *= k1;
    }
    if width % 2 == 1 {
        scanline[width - 1] *= k0;
    }

    lift(scanline, first_even, NEG_DELTA);
    lift(scanline, first_odd, NEG_GAMMA);
    lift(scanline, first_even, NEG_BETA);
    lift(scanline, first_odd, NEG_ALPHA);
}

// The four lifting stages are independent across samples of the same parity.
// Gather eight centers/neighbors from interleaved pairs and write the original
// opposite-parity values back alongside the reconstructed centers.
#[inline(always)]
fn filter_step_horizontal_simd<S: Simd>(
    simd: S,
    row: &mut [f32],
    width: usize,
    first: usize,
    factor: f32,
) {
    filter_step_horizontal_lanes(
        simd,
        row,
        width,
        first,
        |s, left, right| math::mul_add(left + right, factor, s),
        |s, left, right| (left + right).mul_add_like_scalar(f32x8::splat(simd, factor), s),
    );
}

// Both wavelets lift one parity using only opposite-parity neighbors. Keep the
// scalar operation order, symmetric edges and untouched neighbors while
// batching eight interleaved centers through the existing safe SIMD adapter.
#[inline(always)]
fn filter_step_horizontal_lanes<S: Simd>(
    simd: S,
    row: &mut [f32],
    width: usize,
    first: usize,
    scalar: impl Fn(f32, f32, f32) -> f32,
    vector: impl Fn(f32x8<S>, f32x8<S>, f32x8<S>) -> f32x8<S>,
) {
    if first == 0 {
        row[0] = scalar(row[0], row[1], row[1]);
    }
    let mut i = if first == 0 { 2 } else { 1 };
    while width - i >= 2 * SIMD_WIDTH {
        let a = f32x8::from_slice(simd, &row[i - 1..][..SIMD_WIDTH]);
        let b = f32x8::from_slice(simd, &row[i - 1 + SIMD_WIDTH..][..SIMD_WIDTH]);
        let c = f32x8::from_slice(simd, &row[i..][..SIMD_WIDTH]);
        let d = f32x8::from_slice(simd, &row[i + SIMD_WIDTH..][..SIMD_WIDTH]);
        let left = a.unzip_low(b);
        let center = a.unzip_high(b);
        let right = c.unzip_high(d);
        let result = vector(center, left, right);
        left.zip_low(result).store(&mut row[i - 1..]);
        left.zip_high(result).store(&mut row[i - 1 + SIMD_WIDTH..]);
        i += 2 * SIMD_WIDTH;
    }
    while i < width - 1 {
        row[i] = scalar(row[i], row[i - 1], row[i + 1]);
        i += 2;
    }
    if (width - 1) % 2 == first {
        row[width - 1] = scalar(row[width - 1], row[width - 2], row[width - 2]);
    }
}

#[inline(always)]
fn filter_step_horizontal(
    scanline: &mut [f32],
    width: usize,
    first: usize,
    f: impl Fn(f32, f32, f32) -> f32,
) {
    if first == 0 {
        let left = periodic_symmetric_extension_left(0, 1);
        let right = periodic_symmetric_extension_right(0, 1, width);
        scanline[0] = f(scanline[0], scanline[left], scanline[right]);
    }

    let middle_start = if first == 0 { 2 } else { 1 };
    for i in (middle_start..width - 1).step_by(2) {
        scanline[i] = f(scanline[i], scanline[i - 1], scanline[i + 1]);
    }

    if width > 1 && (width - 1) % 2 == first {
        let i = width - 1;
        let left = periodic_symmetric_extension_left(i, 1);
        let right = periodic_symmetric_extension_right(i, 1, width);
        scanline[i] = f(scanline[i], scanline[left], scanline[right]);
    }
}

#[inline(always)]
fn filter_step_vertical<S: Simd>(
    simd: S,
    scanline: &mut [f32],
    height: usize,
    width: usize,
    simd_width: usize,
    first: usize,
    f_simd: impl Fn(f32x8<S>, f32x8<S>, f32x8<S>) -> f32x8<S>,
    f_scalar: impl Fn(f32, f32, f32) -> f32,
) {
    for row in (first..height).step_by(2) {
        let row_above = periodic_symmetric_extension_left(row, 1);
        let row_below = periodic_symmetric_extension_right(row, 1, height);

        // Process SIMD chunks.
        for base_column in (0..simd_width).step_by(SIMD_WIDTH) {
            let s1 = f32x8::from_slice(simd, &scanline[row * width + base_column..][..SIMD_WIDTH]);
            let s2 = f32x8::from_slice(
                simd,
                &scanline[row_above * width + base_column..][..SIMD_WIDTH],
            );
            let s3 = f32x8::from_slice(
                simd,
                &scanline[row_below * width + base_column..][..SIMD_WIDTH],
            );

            let result = f_simd(s1, s2, s3);
            result.store(&mut scanline[row * width + base_column..][..SIMD_WIDTH]);
        }

        // Process scalar remainder.
        for col in simd_width..width {
            let s1 = scanline[row * width + col];
            let s2 = scanline[row_above * width + col];
            let s3 = scanline[row_below * width + col];
            scanline[row * width + col] = f_scalar(s1, s2, s3);
        }
    }
}

/// Part of the `1D_EXTR` procedure, defined in F.3.7.
///
/// Applies the period symmetric extension on the left side.
#[inline(always)]
fn periodic_symmetric_extension_left(idx: usize, offset: usize) -> usize {
    offset.abs_diff(idx)
}

/// Part of the `1D_EXTR` procedure, defined in F.3.7.
///
/// Applies the period symmetric extension on the right side.
#[inline(always)]
fn periodic_symmetric_extension_right(idx: usize, offset: usize, length: usize) -> usize {
    let new_idx = idx + offset;
    if new_idx >= length {
        let overshoot = new_idx - length;
        length - 2 - overshoot
    } else {
        new_idx
    }
}

/// The `VER_SR` procedure from F.3.5.
fn filter_vertical(
    coefficients: &mut [f32],
    rect: IntRect,
    transform: WaveletTransform,
    pdfium: bool,
) {
    dispatch!(Level::new(), simd => filter_vertical_impl(simd, coefficients, rect, transform, pdfium));
}

#[inline(always)]
fn filter_vertical_impl<S: Simd>(
    simd: S,
    scanline: &mut [f32],
    rect: IntRect,
    transform: WaveletTransform,
    pdfium: bool,
) {
    let width = rect.width() as usize;
    let height = rect.height() as usize;
    let y0 = rect.y0 as usize;

    if height == 1 {
        if !y0.is_multiple_of(2) && !(pdfium && transform == WaveletTransform::Irreversible97) {
            let simd_width = width / SIMD_WIDTH * SIMD_WIDTH;
            for base_column in (0..simd_width).step_by(SIMD_WIDTH) {
                let mut loaded = f32x8::from_slice(simd, &scanline[base_column..][..SIMD_WIDTH]);
                loaded *= 0.5;
                loaded.store(&mut scanline[base_column..][..SIMD_WIDTH]);
            }

            // Scalar remainder.
            #[allow(clippy::needless_range_loop)]
            for col in simd_width..width {
                scanline[col] *= 0.5;
            }
        }
        return;
    }

    match transform {
        WaveletTransform::Reversible53 => {
            reversible_filter_53r_simd(simd, scanline, height, width, y0);
        }
        WaveletTransform::Irreversible97 => {
            irreversible_filter_97i_simd(simd, scanline, height, width, y0, pdfium);
        }
    }
}

/// The 1D FILTER 5-3R procedure from F.3.8.1.
#[inline(always)]
fn reversible_filter_53r_simd<S: Simd>(
    simd: S,
    scanline: &mut [f32],
    height: usize,
    width: usize,
    y0: usize,
) {
    let first_even = y0 % 2;
    let simd_width = width / SIMD_WIDTH * SIMD_WIDTH;

    // As in pinned OpenJPEG dwt.c::opj_idwt53_v, combine the two
    // lifting steps: once an even row is reconstructed, the preceding odd
    // row can use it immediately. Walk whole rows to keep contiguous loads
    // and stores, without allocating a transposed column buffer.
    if first_even == 0 {
        // The top even row mirrors its only odd neighbor.
        filter_step_vertical(
            simd,
            &mut scanline[..2 * width],
            2,
            width,
            simd_width,
            0,
            |s, a, b| s - ((a + b + 2.0) * 0.25).floor(),
            |s, a, b| s - math::floor_f32(math::mul_add(a + b, 0.25, 0.5)),
        );
    }
    for row in (if first_even == 0 { 2 } else { 1 }..height).step_by(2) {
        let (before, current_and_after) = scanline.split_at_mut(row * width);
        let (even, after) = current_and_after.split_at_mut(width);
        let (earlier, odd) = before.split_at_mut((row - 1) * width);
        let previous_even = if row >= 2 {
            Some(&earlier[(row - 2) * width..])
        } else {
            None
        };
        reversible_vertical_pair(
            simd,
            previous_even,
            odd,
            even,
            after.get(..width),
            simd_width,
        );
    }
    if (height - 1) % 2 != first_even {
        // The final odd row mirrors the last reconstructed even row.
        filter_step_vertical(
            simd,
            &mut scanline[(height - 2) * width..height * width],
            2,
            width,
            simd_width,
            1,
            |s, a, b| s + ((a + b) * 0.5).floor(),
            |s, a, b| s + math::floor_f32((a + b) * 0.5),
        );
    }
}

#[inline(always)]
fn reversible_vertical_pair<S: Simd>(
    simd: S,
    previous_even: Option<&[f32]>,
    odd: &mut [f32],
    even: &mut [f32],
    next_odd: Option<&[f32]>,
    simd_width: usize,
) {
    for col in (0..simd_width).step_by(SIMD_WIDTH) {
        let above = f32x8::from_slice(simd, &odd[col..][..SIMD_WIDTH]);
        let below = next_odd.map_or(above, |row| {
            f32x8::from_slice(simd, &row[col..][..SIMD_WIDTH])
        });
        let current = f32x8::from_slice(simd, &even[col..][..SIMD_WIDTH])
            - ((above + below + 2.0) * 0.25).floor();
        let previous = previous_even.map_or(current, |row| {
            f32x8::from_slice(simd, &row[col..][..SIMD_WIDTH])
        });
        let result = above + ((previous + current) * 0.5).floor();
        current.store(&mut even[col..]);
        result.store(&mut odd[col..]);
    }
    for col in simd_width..even.len() {
        let above = odd[col];
        let below = next_odd.map_or(above, |row| row[col]);
        let current = even[col] - math::floor_f32(math::mul_add(above + below, 0.25, 0.5));
        let previous = previous_even.map_or(current, |row| row[col]);
        odd[col] = above + math::floor_f32((previous + current) * 0.5);
        even[col] = current;
    }
}

/// The 1D Filter 9-7I procedure from F.3.8.2.
#[inline(always)]
fn irreversible_filter_97i_simd<S: Simd>(
    simd: S,
    scanline: &mut [f32],
    height: usize,
    width: usize,
    y0: usize,
    pdfium: bool,
) {
    const NEG_ALPHA: f32 = 1.586_134_3;
    const NEG_BETA: f32 = 0.052_980_117;
    const NEG_GAMMA: f32 = -0.882_911_1;
    const NEG_DELTA: f32 = -0.443_506_87;
    const KAPPA: f32 = 1.230_174_1;

    // Pinned dwt.c historic two_invK, coupled to native zero band gain in
    // tcd.c. BSD-2-Clause notices: ../../LICENSE-OPENJPEG-DWT.
    let inv_kappa_value: f32 = if pdfium { 1.625_732_422 } else { 1.0 / KAPPA };

    let neg_alpha = f32x8::splat(simd, NEG_ALPHA);
    let neg_beta = f32x8::splat(simd, NEG_BETA);
    let neg_gamma = f32x8::splat(simd, NEG_GAMMA);
    let neg_delta = f32x8::splat(simd, NEG_DELTA);
    let kappa = f32x8::splat(simd, KAPPA);
    let inv_kappa = f32x8::splat(simd, inv_kappa_value);

    // Determine which local row indices correspond to even/odd global positions.
    let first_even = y0 % 2;
    let first_odd = 1 - first_even;
    let simd_width = width / SIMD_WIDTH * SIMD_WIDTH;

    let (k0, k1, k0_simd, k1_simd) = if first_even == 0 {
        (KAPPA, inv_kappa_value, kappa, inv_kappa)
    } else {
        (inv_kappa_value, KAPPA, inv_kappa, kappa)
    };

    // Step 1 and 2.
    // Originally: for i in (start / 2 - 1)..(end / 2 + 2).
    // Originally: for i in (start / 2 - 2)..(end / 2 + 2).
    for row in (0..height.saturating_sub(1)).step_by(2) {
        for base_column in (0..simd_width).step_by(SIMD_WIDTH) {
            let mut vals0 =
                f32x8::from_slice(simd, &scanline[row * width + base_column..][..SIMD_WIDTH]);
            let mut vals1 = f32x8::from_slice(
                simd,
                &scanline[(row + 1) * width + base_column..][..SIMD_WIDTH],
            );
            vals0 = vals0 * k0_simd;
            vals1 = vals1 * k1_simd;
            vals0.store(&mut scanline[row * width + base_column..][..SIMD_WIDTH]);
            vals1.store(&mut scanline[(row + 1) * width + base_column..][..SIMD_WIDTH]);
        }
        for col in simd_width..width {
            scanline[row * width + col] *= k0;
            scanline[(row + 1) * width + col] *= k1;
        }
    }

    if height % 2 == 1 {
        let row = height - 1;
        for base_column in (0..simd_width).step_by(SIMD_WIDTH) {
            let mut vals =
                f32x8::from_slice(simd, &scanline[row * width + base_column..][..SIMD_WIDTH]);
            vals = vals * k0_simd;
            vals.store(&mut scanline[row * width + base_column..][..SIMD_WIDTH]);
        }
        for col in simd_width..width {
            scanline[row * width + col] *= k0;
        }
    }

    // Step 3.
    // Originally: for i in (start / 2 - 1)..(end / 2 + 2).
    filter_step_vertical(
        simd,
        scanline,
        height,
        width,
        simd_width,
        first_even,
        #[inline(always)]
        |s1, s2, s3| (s2 + s3).mul_add(neg_delta, s1),
        #[inline(always)]
        |s1, s2, s3| math::mul_add(s2 + s3, NEG_DELTA, s1),
    );

    // Step 4.
    // Originally: for i in (start / 2 - 1)..(end / 2 + 1).
    filter_step_vertical(
        simd,
        scanline,
        height,
        width,
        simd_width,
        first_odd,
        #[inline(always)]
        |s1, s2, s3| (s2 + s3).mul_add(neg_gamma, s1),
        #[inline(always)]
        |s1, s2, s3| math::mul_add(s2 + s3, NEG_GAMMA, s1),
    );

    // Step 5.
    // Originally: for i in (start / 2)..(end / 2 + 1).
    filter_step_vertical(
        simd,
        scanline,
        height,
        width,
        simd_width,
        first_even,
        #[inline(always)]
        |s1, s2, s3| (s2 + s3).mul_add(neg_beta, s1),
        #[inline(always)]
        |s1, s2, s3| math::mul_add(s2 + s3, NEG_BETA, s1),
    );

    // Step 6.
    // Originally: for i in (start / 2)..(end / 2).
    filter_step_vertical(
        simd,
        scanline,
        height,
        width,
        simd_width,
        first_odd,
        #[inline(always)]
        |s1, s2, s3| (s2 + s3).mul_add(neg_alpha, s1),
        #[inline(always)]
        |s1, s2, s3| math::mul_add(s2 + s3, NEG_ALPHA, s1),
    );
}

#[cfg(test)]
mod pdfium_tests {
    use super::*;

    fn reference_reversible_vertical<S: Simd>(
        simd: S,
        scanline: &mut [f32],
        height: usize,
        width: usize,
        y0: usize,
    ) {
        let first_even = y0 % 2;
        let first_odd = 1 - first_even;
        let simd_width = width / SIMD_WIDTH * SIMD_WIDTH;

        // Equation (F-5).
        // Originally: for i in (start / 2)..(end / 2 + 1).
        filter_step_vertical(
            simd,
            scanline,
            height,
            width,
            simd_width,
            first_even,
            #[inline(always)]
            |s1, s2, s3| s1 - ((s2 + s3 + 2.0) * 0.25).floor(),
            #[inline(always)]
            |s1, s2, s3| s1 - math::floor_f32(math::mul_add(s2 + s3, 0.25, 0.5)),
        );

        // Equation (F-6).
        // Originally: for i in (start / 2)..(end / 2).
        filter_step_vertical(
            simd,
            scanline,
            height,
            width,
            simd_width,
            first_odd,
            #[inline(always)]
            |s1, s2, s3| s1 + ((s2 + s3) * 0.5).floor(),
            #[inline(always)]
            |s1, s2, s3| s1 + math::floor_f32((s2 + s3) * 0.5),
        );
    }

    #[test]
    fn horizontal_simd_matches_scalar_lifting_at_edges_tails_and_both_origins() {
        for width in (1usize..=40).chain([63, 64, 65, 127, 128, 129, 1023]) {
            for origin in 0..4 {
                for transform in [
                    WaveletTransform::Reversible53,
                    WaveletTransform::Irreversible97,
                ] {
                    for pdfium in [false, true] {
                        for magnitude in [1e-30f32, 1.0, 16777216.0, 1e30] {
                            let input: Vec<f32> = (0..4 * width)
                                .map(|i| {
                                    let value = ((i * 137 + i / width * 31) % 513) as i32 - 256;
                                    value as f32 * magnitude + 0.5
                                })
                                .collect();
                            let mut expected = input.clone();
                            for row in expected.chunks_exact_mut(width).take(3) {
                                filter_row(row, width, origin, transform, pdfium);
                            }
                            let mut actual = input;
                            filter_horizontal(
                                &mut actual,
                                IntRect::from_xywh(origin as u32, 0, width as u32, 3),
                                transform,
                                pdfium,
                            );
                            for (index, (a, e)) in actual.iter().zip(&expected).enumerate() {
                                assert_eq!(
                                    a.to_bits(),
                                    e.to_bits(),
                                    "width={width} origin={origin} transform={transform:?} pdfium={pdfium} magnitude={magnitude} index={index}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fused_reversible_vertical_matches_two_pass_filter_at_edges_and_simd_tails() {
        dispatch!(Level::new(), simd => {
            for width in [1usize, 2, 7, 8, 9, 15, 16, 17, 31, 64, 65, 127] {
                for height in [2usize, 3, 4, 5, 6, 7, 8, 17, 32, 63, 64, 65] {
                    for origin in [0usize, 1, 2, 3] {
                        for magnitude in [1.0f32, 65536.0, 16777216.0] {
                            let input: Vec<f32> = (0..width * height).map(|i| {
                                let value = ((i * 137 + i / width * 31) % 513) as i32 - 256;
                                value as f32 * magnitude + 0.5
                            }).collect();
                            let mut expected = input.clone();
                            reference_reversible_vertical(simd, &mut expected, height, width, origin);
                            let mut actual = input;
                            reversible_filter_53r_simd(simd, &mut actual, height, width, origin);
                            assert_eq!(actual, expected, "width={width} height={height} origin={origin} magnitude={magnitude}");
                        }
                    }
                }
            }
        });
    }

    #[test]
    fn native_two_invk_matches_pinned_inverse_filter_for_both_origins() {
        // Pinned dwt.c::opj_v8dwt_decode outputs, allowing f32 lifting/FMA
        // rounding differences in the planned codec's equivalent filter.
        for (origin, expected) in [
            (
                0,
                [
                    -0.634884417f32,
                    0.266154289,
                    -3.9999342,
                    -0.266284943,
                    -7.36498356,
                ],
            ),
            (
                1,
                [
                    0.83271265f32,
                    1.74997163,
                    0.0000658035278,
                    6.24989605,
                    -0.832580566,
                ],
            ),
        ] {
            let mut row = [0.0, 1.0, -2.0, 3.0, -4.0];
            filter_row(&mut row, 5, origin, WaveletTransform::Irreversible97, true);
            for (actual, expected) in row.iter().zip(expected) {
                assert!((actual - expected).abs() < 0.00001);
            }
            let mut columns = Vec::new();
            for value in [0.0, 1.0, -2.0, 3.0, -4.0] {
                columns.extend_from_slice(&[value; 9]);
            }
            filter_vertical(
                &mut columns,
                IntRect::from_xywh(0, origin as u32, 9, 5),
                WaveletTransform::Irreversible97,
                true,
            );
            for (row, expected) in columns.chunks_exact(9).zip(expected) {
                assert!(row.iter().all(|actual| (actual - expected).abs() < 0.00001));
            }
        }
        let mut singleton = [3.0];
        filter_row(&mut singleton, 1, 1, WaveletTransform::Irreversible97, true);
        assert_eq!(singleton, [3.0]);
    }
}

#[cfg(test)]
#[path = "idwt/optimization_tests.rs"]
mod optimization_tests;
