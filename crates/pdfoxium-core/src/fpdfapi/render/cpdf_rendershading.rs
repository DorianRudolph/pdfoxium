// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! AGG device-buffer dispatch, sampled function shadings, scanline Gouraud
//! meshes and adaptive Bézier patches from `cpdf_rendershading.{h,cpp}`.

use super::cpdf_devicebuffer::CpdfDeviceBuffer;
use super::cpdf_renderoptions::{CpdfRenderOptions, RenderColorMode};
use crate::fpdfapi::page::cpdf_colorspace::ColorSpaceRef;
use crate::fpdfapi::page::cpdf_function::CpdfFunction;
use crate::fpdfapi::page::cpdf_meshstream::{CpdfMeshStream, CpdfMeshVertex};
use crate::fpdfapi::page::cpdf_shadingpattern::{CpdfShadingPattern, ShadingType};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::get_dict_object;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, FxRect};
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::argb_encode;

const SHADING_STEPS: usize = 256;

#[cfg(feature = "simd")]
#[path = "gouraud_simd.rs"]
mod gouraud_simd;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_colorspace::{get_stock_device_cs, ColorSpaceFamily};
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    fn array(values: &[f32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_float(value));
        }
        array
    }
    #[test]
    fn axial_samples_use_256_denominator_integer_positions_and_truncated_indices() {
        let mut function = CpdfDictionary::new();
        function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        for (key, values) in [
            (b"Domain".as_slice(), &[0.0, 1.0][..]),
            (b"C0", &[0.0][..]),
            (b"C1", &[1.0][..]),
        ] {
            function.set_new_for(key, array(values));
        }
        let functions = [CpdfFunction::load(&retain_object(function))];
        let gray = get_stock_device_cs(ColorSpaceFamily::DeviceGray);
        let steps = get_shading_steps(0.0, 1.0, &functions, &gray, 255).unwrap();
        assert_eq!(steps[0], 0xff000000);
        assert_eq!(steps[255], 0xfffefefe);
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Coords", array(&[0.0, 0.0, 1.0, 0.0]));
        for (offset, expected) in [(0.001, [0, 0, 0, 255]), (0.01, [0; 4])] {
            let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
            draw_axial(
                &mut bitmap,
                CfxMatrix::new(1.0, 0.0, 0.0, 1.0, offset, 0.0),
                &dict,
                &functions,
                &gray,
                255,
            );
            assert_eq!(&bitmap.buffer()[..4], &expected);
        }
    }

    #[test]
    fn concentric_radial_specialization_matches_generic_pixels() {
        let steps = std::array::from_fn(|i| 0xff000000 | i as u32 * 0x010101);
        for matrix in [
            CfxMatrix::new(3.0, 0.0, 0.0, -3.0, 12.0, 10.0),
            CfxMatrix::new(2.5, 0.25, -0.5, 1.75, 4.125, 7.5),
            CfxMatrix::new(25000.0, 0.0, 0.0, -25000.0, 12.0, 10.0),
        ] {
            let inverse = ShadingSampleTransform::new(matrix);
            for zero in [0.0, -0.0] {
                for radius in [0.0, -0.0, 0.005, 4.0, -4.0, 1e-25, f32::MAX] {
                    for extend in [[false, false], [true, false], [false, true], [true, true]] {
                        let coords = [zero, zero, zero, -zero, -zero, radius];
                        assert!(concentric_zero_start(&inverse, coords, 24, 20));
                        let mut fast = CfxDibitmap::create(24, 20, FxdibFormat::Bgra, 0).unwrap();
                        let mut generic =
                            CfxDibitmap::create(24, 20, FxdibFormat::Bgra, 0).unwrap();
                        draw_radial_samples::<true>(&mut fast, &inverse, coords, &steps, extend);
                        draw_radial_samples::<false>(
                            &mut generic,
                            &inverse,
                            coords,
                            &steps,
                            extend,
                        );
                        assert_eq!(
                            fast.buffer(),
                            generic.buffer(),
                            "zero={zero} radius={radius} extend={extend:?}"
                        );
                    }
                }
            }
        }
        let huge = ShadingSampleTransform {
            inverse: CfxMatrix::new(f32::MAX, 0.0, 0.0, 1.0, 0.0, 0.0),
            origin: CfxPointF::default(),
        };
        assert!(!concentric_zero_start(
            &huge,
            [0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            24,
            20
        ));
        let identity = ShadingSampleTransform::new(CfxMatrix::default());
        for coords in [
            [0.0, 0.0, 1.0, 0.0, 0.0, 4.0],
            [0.0, 0.0, 0.0, 1.0, 0.0, 4.0],
            [0.0, 0.0, 0.0, 0.0, 1.0, 4.0],
            [0.0, 0.0, 0.0, 0.0, 0.0, f32::INFINITY],
            [f32::INFINITY, 0.0, 0.0, f32::INFINITY, 0.0, 4.0],
        ] {
            assert!(!concentric_zero_start(&identity, coords, 24, 20));
        }
    }

    #[test]
    fn radial_samples_are_independent_of_region_origin() {
        let mut function = CpdfDictionary::new();
        function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        function.set_new_for(b"Domain", array(&[0.0, 1.0]));
        function.set_new_for(b"C0", array(&[0.8, 0.1, 0.2]));
        function.set_new_for(b"C1", array(&[0.2, 0.025, 0.05]));
        let functions = [CpdfFunction::load(&retain_object(function))];
        let rgb = get_stock_device_cs(ColorSpaceFamily::DeviceRgb);
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Coords", array(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.005]));
        let mut extend = CpdfArray::new();
        extend.append_new(CpdfScalarObject::boolean(true));
        extend.append_new(CpdfScalarObject::boolean(true));
        dict.set_new_for(b"Extend", extend);
        let matrix = CfxMatrix::new(25000.0, 0.0, 0.0, -25000.0, 75.0, 150.0);
        let mut whole = CfxDibitmap::create(150, 200, FxdibFormat::Bgra, 0).unwrap();
        draw_radial(&mut whole, matrix, &dict, &functions, &rgb, 255);
        assert_ne!(
            &whole.scanline(0).unwrap()[..4],
            &whole.scanline(150).unwrap()[75 * 4..76 * 4]
        );
        for (x, y, w, h) in [(0, 66, 150, 134), (37, 0, 113, 200), (50, 50, 100, 150)] {
            let mut region = CfxDibitmap::create(w, h, FxdibFormat::Bgra, 0).unwrap();
            let translated = CfxMatrix {
                e: matrix.e - x as f32,
                f: matrix.f - y as f32,
                ..matrix
            };
            draw_radial(&mut region, translated, &dict, &functions, &rgb, 255);
            for row in 0..h {
                assert_eq!(
                    &whole.scanline(y + row).unwrap()[x as usize * 4..(x + w) as usize * 4],
                    &region.scanline(row).unwrap()[..w as usize * 4],
                    "region ({x},{y},{w},{h}) row {row}"
                );
            }
        }
        assert_eq!(
            ShadingSampleTransform::new(CfxMatrix::new(0.0, 0.0, 0.0, 0.0, 75.0, 150.0))
                .transform(3, 7),
            CfxPointF::new(3.0, 7.0)
        );
    }

    #[test]
    fn radial_common_tangent_does_not_paint_an_unextended_start() {
        let mut function = CpdfDictionary::new();
        function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        function.set_new_for(b"Domain", array(&[0.0, 1.0]));
        function.set_new_for(b"C0", array(&[0.0]));
        function.set_new_for(b"C1", array(&[1.0]));
        let functions = [CpdfFunction::load(&retain_object(function))];
        let gray = get_stock_device_cs(ColorSpaceFamily::DeviceGray);
        let mut dict = CpdfDictionary::new();
        // Internally tangent circles, sharing x=0, y=2. Along x=0 both
        // quadratic coefficients a and b vanish, giving an undefined scale.
        dict.set_new_for(b"Coords", array(&[3.0, 2.0, 3.0, 4.0, 2.0, 4.0]));
        for start_extend in [false, true] {
            for end_extend in [false, true] {
                let mut extend = CpdfArray::new();
                extend.append_new(CpdfScalarObject::boolean(start_extend));
                extend.append_new(CpdfScalarObject::boolean(end_extend));
                dict.set_new_for(b"Extend", extend);
                let mut bitmap = CfxDibitmap::create(9, 5, FxdibFormat::Bgra, 0).unwrap();
                draw_radial(
                    &mut bitmap,
                    CfxMatrix::default(),
                    &dict,
                    &functions,
                    &gray,
                    255,
                );
                for y in 0..5 {
                    assert_eq!(
                        &bitmap.scanline(y).unwrap()[..4],
                        if start_extend {
                            &[0, 0, 0, 255]
                        } else {
                            &[0; 4]
                        },
                        "row={y}, extend={start_extend}/{end_extend}"
                    );
                }
                assert_eq!(bitmap.scanline(2).unwrap()[7 * 4 + 3], 255);
                for scale in [
                    f32::NAN,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::MAX,
                    f32::MIN,
                ] {
                    assert_eq!(
                        shading_index(scale, [start_extend, end_extend]),
                        start_extend.then_some(0)
                    );
                }
            }
        }
    }

    #[test]
    fn gouraud_rgb_spans_preserve_repeated_addition_casts_and_guards() {
        for width in [0usize, 1, 15, 16, 17, 63, 257, 1024] {
            for (rgb, step) in [
                ([0.0, 0.25, 0.9], [0.001, -0.0001, 0.0]),
                ([1.0, 0.0, -0.0], [-0.0003, 0.0001, 0.0]),
                ([-1.0, 2.0, 127.0], [0.123, -0.4, 17.0]),
                ([f32::MAX, f32::MIN, f32::NAN], [1.0, -1.0, 0.5]),
                (
                    [0.0, f32::INFINITY, f32::NEG_INFINITY],
                    [f32::NAN, 1.0, -1.0],
                ),
            ] {
                for alpha in [0, 1, 127, 255] {
                    let mut actual = vec![0xa5; width * 4 + 7];
                    let mut expected = actual.clone();
                    let [mut r, mut g, mut b] = rgb;
                    for pixel in expected[3..3 + width * 4].chunks_exact_mut(4) {
                        r += step[0];
                        g += step[1];
                        b += step[2];
                        let color = argb_encode(
                            alpha as u32,
                            (r * 255.0) as i32 as u32,
                            (g * 255.0) as i32 as u32,
                            (b * 255.0) as i32 as u32,
                        );
                        pixel.copy_from_slice(&color.to_le_bytes());
                    }
                    gouraud_rgb_span(&mut actual[3..3 + width * 4], rgb, step, alpha);
                    assert_eq!(
                        actual, expected,
                        "width={width} rgb={rgb:?} step={step:?} alpha={alpha}"
                    );
                }
            }
        }
    }

    #[test]
    fn gouraud_scanline_advances_colors_before_writing_and_uses_mesh_sample_indices() {
        use crate::fxge::dib::fx_dib::FxRgbStruct;
        let mut triangle = [CpdfMeshVertex::default(); 3];
        triangle[0].position = CfxPointF::new(0.0, 0.0);
        triangle[1].position = CfxPointF::new(0.0, 2.0);
        triangle[2].position = CfxPointF::new(4.0, 0.0);
        triangle[2].rgb = FxRgbStruct {
            red: 1.0,
            green: 0.0,
            blue: 0.0,
        };
        let mut bitmap = CfxDibitmap::create(4, 3, FxdibFormat::Bgra, 0).unwrap();
        draw_gouraud(&mut bitmap, 255, triangle, None);
        assert_eq!(
            &bitmap.scanline(1).unwrap()[..8],
            &[0, 0, 63, 255, 0, 0, 127, 255]
        );
        assert!(bitmap.scanline(2).unwrap().iter().all(|&value| value == 0));
        let steps = std::array::from_fn(|i| argb_encode(255, 0, i as u32, 0));
        triangle[2].rgb.red = 255.0;
        bitmap.clear(0);
        draw_gouraud(&mut bitmap, 255, triangle, Some(&steps));
        assert_eq!(
            &bitmap.scanline(1).unwrap()[..8],
            &[0, 63, 0, 255, 0, 127, 0, 255]
        );
    }
}

fn count_outputs(functions: &[Option<CpdfFunction>]) -> u32 {
    functions
        .iter()
        .flatten()
        .try_fold(0u32, |total, f| total.checked_add(f.output_count()))
        .unwrap_or(0)
}
fn validated_outputs(functions: &[Option<CpdfFunction>], color_space: &ColorSpaceRef) -> u32 {
    let count = count_outputs(functions);
    if count == 0 {
        0
    } else {
        count.max(color_space.component_count())
    }
}
fn call_functions(functions: &[Option<CpdfFunction>], input: &[f32], results: &mut [f32]) {
    let mut offset = 0;
    for function in functions.iter().flatten() {
        if let Some(count) = function.call(input, &mut results[offset..]) {
            offset += count as usize;
        }
    }
}
fn get_shading_steps(
    t_min: f32,
    t_max: f32,
    functions: &[Option<CpdfFunction>],
    color_space: &ColorSpaceRef,
    alpha: i32,
) -> Option<[u32; SHADING_STEPS]> {
    let count = validated_outputs(functions, color_space);
    if count == 0 {
        return None;
    }
    let mut results = vec![0.0; count as usize];
    let mut steps = [0; SHADING_STEPS];
    let diff = t_max - t_min;
    for (i, color) in steps.iter_mut().enumerate() {
        let input = diff * i as f32 / SHADING_STEPS as f32 + t_min;
        call_functions(functions, &[input], &mut results);
        let rgb = color_space.get_rgb_or_zeros_on_error(&results);
        *color = argb_encode(
            alpha as u32,
            (rgb.red * 255.0).round() as i32 as u32,
            (rgb.green * 255.0).round() as i32 as u32,
            (rgb.blue * 255.0).round() as i32 as u32,
        );
    }
    Some(steps)
}
fn array_values<const N: usize>(dict: &CpdfDictionary, key: &[u8]) -> Option<[f32; N]> {
    let array = dict.get_array_for(key)?;
    let borrowed = array.borrow();
    let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
    Some(std::array::from_fn(|i| array.get_float_at(i)))
}
fn shading_domain_and_extend(dict: &CpdfDictionary) -> ([f32; 2], [bool; 2]) {
    let domain = array_values(dict, b"Domain").unwrap_or([0.0, 1.0]);
    let extend = dict.get_array_for(b"Extend").map_or([false; 2], |array| {
        let borrowed = array.borrow();
        let array = borrowed
            .as_any()
            .downcast_ref::<CpdfArray>()
            .expect("Extend array");
        [
            array.get_boolean_at(0, false),
            array.get_boolean_at(1, false),
        ]
    });
    (domain, extend)
}
fn shading_index(scale: f32, extend: [bool; 2]) -> Option<usize> {
    let scaled = scale * (SHADING_STEPS - 1) as f32;
    // At a common tangent, the radial solver can produce NaN (0/0) or
    // infinity. The native reference's float-to-int conversion yields INT_MIN
    // outside the i32 range. Rust's saturating cast would instead paint the
    // start color for NaN, or the end color for positive infinity, leaving
    // spurious stripes when that end is not extended.
    let index = if (-2147483648.0..2147483648.0).contains(&scaled) {
        scaled as i32
    } else {
        i32::MIN
    };
    if index < 0 {
        extend[0].then_some(0)
    } else if index >= SHADING_STEPS as i32 {
        extend[1].then_some(SHADING_STEPS - 1)
    } else {
        Some(index as usize)
    }
}
// Keep device translation out of the inverted coefficients. Rounding a small
// inverse translation separately from each pixel term can select a different
// shading step when exactly the same pixel is rendered in a region bitmap.
struct ShadingSampleTransform {
    inverse: CfxMatrix,
    origin: CfxPointF,
}
impl ShadingSampleTransform {
    fn new(matrix: CfxMatrix) -> Self {
        let mut inverse = matrix.get_inverse();
        let origin = if matrix.a * matrix.d - matrix.b * matrix.c == 0.0 {
            // Preserve CfxMatrix's identity fallback for singular transforms.
            CfxPointF::default()
        } else {
            CfxPointF::new(matrix.e, matrix.f)
        };
        inverse.e = 0.0;
        inverse.f = 0.0;
        Self { inverse, origin }
    }
    fn transform(&self, x: usize, y: i32) -> CfxPointF {
        let x = x as f64 - self.origin.x as f64;
        let y = y as f64 - self.origin.y as f64;
        CfxPointF::new(
            (self.inverse.a as f64 * x + self.inverse.c as f64 * y) as f32,
            (self.inverse.b as f64 * x + self.inverse.d as f64 * y) as f32,
        )
    }
}

fn draw_axial(
    bitmap: &mut CfxDibitmap,
    matrix: CfxMatrix,
    dict: &CpdfDictionary,
    functions: &[Option<CpdfFunction>],
    color_space: &ColorSpaceRef,
    alpha: i32,
) {
    let Some([start_x, start_y, end_x, end_y]) = array_values(dict, b"Coords") else {
        return;
    };
    let ([t_min, t_max], extend) = shading_domain_and_extend(dict);
    let Some(steps) = get_shading_steps(t_min, t_max, functions, color_space, alpha) else {
        return;
    };
    let x_span = end_x - start_x;
    let y_span = end_y - start_y;
    let length_squared = x_span * x_span + y_span * y_span;
    let inverse = ShadingSampleTransform::new(matrix);
    let width = bitmap.width() as usize;
    for row in 0..bitmap.height() {
        for (column, pixel) in bitmap.scanline_mut(row).unwrap()[..width * 4]
            .chunks_exact_mut(4)
            .enumerate()
        {
            let pos = inverse.transform(column, row);
            let scale = ((pos.x - start_x) * x_span + (pos.y - start_y) * y_span) / length_squared;
            if let Some(index) = shading_index(scale, extend) {
                pixel.copy_from_slice(&steps[index].to_le_bytes());
            }
        }
    }
}
fn float_zero(value: f32) -> bool {
    (value as f64) < 0.0001 && (value as f64) > -0.0001
}
fn draw_radial(
    bitmap: &mut CfxDibitmap,
    matrix: CfxMatrix,
    dict: &CpdfDictionary,
    functions: &[Option<CpdfFunction>],
    color_space: &ColorSpaceRef,
    alpha: i32,
) {
    let Some([start_x, start_y, start_r, end_x, end_y, end_r]) = array_values(dict, b"Coords")
    else {
        return;
    };
    let ([t_min, t_max], extend) = shading_domain_and_extend(dict);
    let Some(steps) = get_shading_steps(t_min, t_max, functions, color_space, alpha) else {
        return;
    };
    let coords = [start_x, start_y, start_r, end_x, end_y, end_r];
    let inverse = ShadingSampleTransform::new(matrix);
    if concentric_zero_start(&inverse, coords, bitmap.width(), bitmap.height()) {
        draw_radial_samples::<true>(bitmap, &inverse, coords, &steps, extend);
    } else {
        draw_radial_samples::<false>(bitmap, &inverse, coords, &steps, extend);
    }
}

fn concentric_zero_start(
    inverse: &ShadingSampleTransform,
    [start_x, start_y, start_r, end_x, end_y, end_r]: [f32; 6],
    width: i32,
    height: i32,
) -> bool {
    if width <= 0
        || height <= 0
        || start_r != 0.0
        || start_x != end_x
        || start_y != end_y
        || !end_r.is_finite()
    {
        return false;
    }
    // Affine extrema occur at the corners. Requiring finite differences also
    // excludes Inf * 0 in b: otherwise the generic solver would receive NaN.
    for y in [0, height - 1] {
        for x in [0, width as usize - 1] {
            let pos = inverse.transform(x, y);
            if !(pos.x - start_x).is_finite() || !(pos.y - start_y).is_finite() {
                return false;
            }
        }
    }
    true
}

fn draw_radial_samples<const CONCENTRIC_ZERO_START: bool>(
    bitmap: &mut CfxDibitmap,
    inverse: &ShadingSampleTransform,
    [start_x, start_y, start_r, end_x, end_y, end_r]: [f32; 6],
    steps: &[u32; SHADING_STEPS],
    extend: [bool; 2],
) {
    let dx = end_x - start_x;
    let dy = end_y - start_y;
    let dr = end_r - start_r;
    let a = dx * dx + dy * dy - dr * dr;
    let a_zero = float_zero(a);
    let decreasing = dr < 0.0 && ((dx.hypot(dy) as i32) as f32) < -dr;
    let width = bitmap.width() as usize;
    for row in 0..bitmap.height() {
        for (column, pixel) in bitmap.scanline_mut(row).unwrap()[..width * 4]
            .chunks_exact_mut(4)
            .enumerate()
        {
            let pos = inverse.transform(column, row);
            let pos_dx = pos.x - start_x;
            let pos_dy = pos.y - start_y;
            // In the finite concentric zero-start case b is signed zero for
            // every pixel. Its sign is unused by the zero-b solver branch.
            let b = if CONCENTRIC_ZERO_START {
                0.0
            } else {
                -2.0 * (pos_dx * dx + pos_dy * dy + start_r * dr)
            };
            let c = pos_dx * pos_dx + pos_dy * pos_dy - start_r * start_r;
            let scale = if CONCENTRIC_ZERO_START || float_zero(b) {
                (-c / a).sqrt()
            } else if a_zero {
                -c / b
            } else {
                let discriminant = b * b - 4.0 * (a * c);
                if discriminant < 0.0 {
                    continue;
                }
                let root = discriminant.sqrt();
                let mut first = (-b - root) / (2.0 * a);
                let mut second = (-b + root) / (2.0 * a);
                if a <= 0.0 {
                    std::mem::swap(&mut first, &mut second);
                }
                let scale = if decreasing {
                    if first >= 0.0 || extend[0] {
                        first
                    } else {
                        second
                    }
                } else if second <= 1.0 || extend[1] {
                    second
                } else {
                    first
                };
                if start_r + scale * dr < 0.0 {
                    continue;
                }
                scale
            };
            if let Some(index) = shading_index(scale, extend) {
                pixel.copy_from_slice(&steps[index].to_le_bytes());
            }
        }
    }
}
fn draw_function(
    bitmap: &mut CfxDibitmap,
    matrix: CfxMatrix,
    dict: &CpdfDictionary,
    functions: &[Option<CpdfFunction>],
    color_space: &ColorSpaceRef,
    alpha: i32,
) {
    let count = validated_outputs(functions, color_space);
    if count == 0 {
        return;
    }
    let [xmin, xmax, ymin, ymax] = array_values(dict, b"Domain").unwrap_or([0.0, 1.0, 0.0, 1.0]);
    let inverse = matrix.get_inverse() * dict.get_matrix_for(b"Matrix").get_inverse();
    let mut results = vec![0.0; count as usize];
    let width = bitmap.width() as usize;
    for row in 0..bitmap.height() {
        for (column, pixel) in bitmap.scanline_mut(row).unwrap()[..width * 4]
            .chunks_exact_mut(4)
            .enumerate()
        {
            let pos = inverse.transform(CfxPointF::new(column as f32, row as f32));
            if pos.x < xmin || pos.x > xmax || pos.y < ymin || pos.y > ymax {
                continue;
            }
            call_functions(functions, &[pos.x, pos.y], &mut results);
            let rgb = color_space.get_rgb_or_zeros_on_error(&results);
            pixel.copy_from_slice(
                &argb_encode(
                    alpha as u32,
                    (rgb.red * 255.0) as i32 as u32,
                    (rgb.green * 255.0) as i32 as u32,
                    (rgb.blue * 255.0) as i32 as u32,
                )
                .to_le_bytes(),
            );
        }
    }
}

pub(crate) fn draw_shading(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    pattern: &CpdfShadingPattern,
    matrix: CfxMatrix,
    mut clip: FxRect,
    alpha: i32,
    options: &CpdfRenderOptions,
) {
    let Some(color_space) = pattern.color_space() else {
        return;
    };
    let Some(shading) = pattern.shading_object() else {
        return;
    };
    let Some(dictionary) = get_dict_object(&shading) else {
        return;
    };
    let borrowed = dictionary.borrow();
    let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
        return;
    };
    let background = if !pattern.is_shading_object() {
        dict.get_array_for(b"Background")
            .and_then(|array| {
                let borrowed = array.borrow();
                let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
                let count = color_space.component_count() as usize;
                if array.size() < count {
                    return None;
                }
                let values: Vec<_> = (0..count).map(|i| array.get_float_at(i)).collect();
                let rgb = color_space.get_rgb_or_zeros_on_error(&values);
                Some(argb_encode(
                    255,
                    (rgb.red * 255.0) as i32 as u32,
                    (rgb.green * 255.0) as i32 as u32,
                    (rgb.blue * 255.0) as i32 as u32,
                ))
            })
            .unwrap_or(0)
    } else {
        0
    };
    if dict.key_exist(b"BBox") {
        clip.intersect(
            matrix
                .transform_float_rect(dict.get_rect_for(b"BBox"))
                .get_outer_rect(),
        );
    }
    let mut buffer = CpdfDeviceBuffer::new(clip, 150);
    let matrix = matrix * buffer.matrix();
    let Some(bitmap) = buffer.initialize() else {
        return;
    };
    if background != 0 {
        bitmap.clear(background);
    }
    match pattern.shading_type() {
        ShadingType::Invalid => return,
        ShadingType::FunctionBased => draw_function(
            bitmap,
            matrix,
            dict,
            &pattern.functions(),
            &color_space,
            alpha,
        ),
        ShadingType::Axial => draw_axial(
            bitmap,
            matrix,
            dict,
            &pattern.functions(),
            &color_space,
            alpha,
        ),
        ShadingType::Radial => draw_radial(
            bitmap,
            matrix,
            dict,
            &pattern.functions(),
            &color_space,
            alpha,
        ),
        _ => draw_mesh_shading(
            bitmap,
            pattern,
            &shading,
            matrix,
            dict,
            alpha,
            options.options().no_path_smooth,
        ),
    }
    if options.color_mode_is(RenderColorMode::Alpha) {
        bitmap.set_red_from_alpha();
    } else if options.color_mode_is(RenderColorMode::Gray) {
        bitmap.convert_color_scale(false);
    }
    buffer.output_to_device(driver);
}

fn component_to_shading_index(component: f32, min: f32, max: f32) -> f32 {
    if min == max {
        0.0
    } else {
        ((component - min) / (max - min)) * (SHADING_STEPS - 1) as f32
    }
}

fn get_scanline_intersect(y: i32, first: CfxPointF, second: CfxPointF) -> Option<f32> {
    if first.y == second.y {
        return None;
    }
    let y = y as f32;
    if first.y < second.y {
        if y < first.y || y > second.y {
            return None;
        }
    } else if y < second.y || y > first.y {
        return None;
    }
    Some(first.x + (second.x - first.x) * (y - first.y) / (second.y - first.y))
}

fn draw_gouraud(
    bitmap: &mut CfxDibitmap,
    alpha: i32,
    triangle: [CpdfMeshVertex; 3],
    shading_steps: Option<&[u32; SHADING_STEPS]>,
) {
    let mut min_y = triangle[0].position.y;
    let mut max_y = min_y;
    for vertex in &triangle[1..] {
        min_y = min_y.min(vertex.position.y);
        max_y = max_y.max(vertex.position.y);
    }
    if min_y == max_y {
        return;
    }
    let min_y = (min_y.floor() as i32).max(0);
    let max_y = (max_y.ceil() as i32).min(bitmap.height() - 1);
    let width = bitmap.width();
    for y in min_y..=max_y {
        let mut count = 0;
        let mut intersections = [0.0; 3];
        let mut red = [0.0; 3];
        let mut green = [0.0; 3];
        let mut blue = [0.0; 3];
        for i in 0..3 {
            let first = triangle[i];
            let second = triangle[(i + 1) % 3];
            let Some(x) = get_scanline_intersect(y, first.position, second.position) else {
                continue;
            };
            intersections[count] = x;
            let dist = (y as f32 - first.position.y) / (second.position.y - first.position.y);
            red[count] = first.rgb.red + (second.rgb.red - first.rgb.red) * dist;
            green[count] = first.rgb.green + (second.rgb.green - first.rgb.green) * dist;
            blue[count] = first.rgb.blue + (second.rgb.blue - first.rgb.blue) * dist;
            count += 1;
        }
        if count != 2 {
            continue;
        }
        let (start_index, end_index) = if intersections[0] < intersections[1] {
            (0, 1)
        } else {
            (1, 0)
        };
        let min_x = intersections[start_index].floor() as i32;
        let max_x = intersections[end_index].ceil() as i32;
        let start_x = min_x.clamp(0, width);
        let end_x = max_x.clamp(0, width);
        if start_x >= end_x {
            continue;
        }
        let range = max_x.saturating_sub(min_x) as f32;
        let r_unit = (red[end_index] - red[start_index]) / range;
        let g_unit = (green[end_index] - green[start_index]) / range;
        let b_unit = (blue[end_index] - blue[start_index]) / range;
        let diff = start_x.saturating_sub(min_x) as f32;
        let mut r_result = red[start_index] + diff * r_unit;
        let g_result = green[start_index] + diff * g_unit;
        let b_result = blue[start_index] + diff * b_unit;
        let row = bitmap.scanline_mut(y).unwrap();
        let pixels = &mut row[start_x as usize * 4..end_x as usize * 4];
        if let Some(steps) = shading_steps {
            for pixel in pixels.chunks_exact_mut(4) {
                r_result += r_unit;
                let color = steps[(r_result as i32).clamp(0, SHADING_STEPS as i32 - 1) as usize];
                pixel.copy_from_slice(&color.to_le_bytes());
            }
        } else {
            gouraud_rgb_span(
                pixels,
                [r_result, g_result, b_result],
                [r_unit, g_unit, b_unit],
                alpha,
            );
        }
    }
}

fn gouraud_rgb_span(dest: &mut [u8], mut rgb: [f32; 3], step: [f32; 3], alpha: i32) {
    #[cfg(feature = "simd")]
    if dest.len() >= 16 * 4 {
        gouraud_simd::fill(dest, rgb, step, alpha);
        return;
    }
    for pixel in dest.chunks_exact_mut(4) {
        for channel in 0..3 {
            rgb[channel] += step[channel];
        }
        let color = argb_encode(
            alpha as u32,
            (rgb[0] * 255.0) as i32 as u32,
            (rgb[1] * 255.0) as i32 as u32,
            (rgb[2] * 255.0) as i32 as u32,
        );
        pixel.copy_from_slice(&color.to_le_bytes());
    }
}

fn draw_mesh_shading(
    bitmap: &mut CfxDibitmap,
    pattern: &CpdfShadingPattern,
    shading: &crate::fpdfapi::parser::cpdf_object::ObjectRef,
    matrix: CfxMatrix,
    dict: &CpdfDictionary,
    alpha: i32,
    no_path_smooth: bool,
) {
    let borrowed = shading.borrow();
    let Some(stream) = borrowed.as_any().downcast_ref::<CpdfStream>() else {
        return;
    };
    let Some(color_space) = pattern.color_space() else {
        return;
    };
    let functions = pattern.functions();
    let Some(mut mesh) = CpdfMeshStream::load(
        pattern.shading_type(),
        !functions.is_empty(),
        stream,
        color_space.clone(),
    ) else {
        return;
    };
    let (min, max) = (mesh.component_min(0), mesh.component_max(0));
    let shading_steps = if functions.is_empty() {
        None
    } else {
        let Some(steps) = get_shading_steps(min, max, &functions, &color_space, alpha) else {
            return;
        };
        Some(steps)
    };
    let convert = |vertex: &mut CpdfMeshVertex| {
        if shading_steps.is_some() {
            vertex.rgb.red = component_to_shading_index(vertex.rgb.red, min, max);
        }
    };
    match pattern.shading_type() {
        ShadingType::FreeFormGouraudTriangleMesh => {
            let mut triangle = [CpdfMeshVertex::default(); 3];
            while !mesh.is_eof() {
                let Some((mut vertex, flag)) = mesh.read_vertex(matrix) else {
                    return;
                };
                convert(&mut vertex);
                if flag == 0 {
                    triangle[0] = vertex;
                    for target in &mut triangle[1..] {
                        let Some((mut vertex, _)) = mesh.read_vertex(matrix) else {
                            return;
                        };
                        convert(&mut vertex);
                        *target = vertex;
                    }
                } else {
                    if flag == 1 {
                        triangle[0] = triangle[1];
                    }
                    triangle[1] = triangle[2];
                    triangle[2] = vertex;
                }
                draw_gouraud(bitmap, alpha, triangle, shading_steps.as_ref());
            }
        }
        ShadingType::LatticeFormGouraudTriangleMesh => {
            let columns = dict.get_integer_for(b"VerticesPerRow", 0);
            if columns < 2 {
                return;
            }
            let mut previous = mesh.read_vertex_row(matrix, columns);
            if previous.is_empty() {
                return;
            }
            for vertex in &mut previous {
                convert(vertex);
            }
            loop {
                let mut next = mesh.read_vertex_row(matrix, columns);
                if next.is_empty() {
                    return;
                }
                for vertex in &mut next {
                    convert(vertex);
                }
                for i in 1..columns as usize {
                    draw_gouraud(
                        bitmap,
                        alpha,
                        [previous[i], next[i - 1], previous[i - 1]],
                        shading_steps.as_ref(),
                    );
                    draw_gouraud(
                        bitmap,
                        alpha,
                        [previous[i], next[i - 1], next[i]],
                        shading_steps.as_ref(),
                    );
                }
                previous = next;
            }
        }
        _ => {
            draw_coon_patch_meshes(
                &mut mesh,
                bitmap,
                matrix,
                alpha,
                no_path_smooth,
                shading_steps.as_ref(),
            );
        }
    }
}

#[derive(Clone, Copy, Default)]
struct CubicBezierPatch {
    points: [[CfxPointF; 4]; 4],
}
impl CubicBezierPatch {
    fn is_small(&self) -> bool {
        let bbox = crate::fxcrt::fx_coordinates::CfxFloatRect::get_bbox(self.points.as_flattened());
        bbox.width() < 2.0 && bbox.height() < 2.0
    }
    fn boundary(&self) -> [CfxPointF; 13] {
        let p = &self.points;
        [
            p[0][0], p[0][1], p[0][2], p[0][3], p[1][3], p[2][3], p[3][3], p[3][2], p[3][1],
            p[3][0], p[2][0], p[1][0], p[0][0],
        ]
    }
    fn subdivide_vertical(&self) -> (Self, Self) {
        let (mut top, mut bottom) = (Self::default(), Self::default());
        for x in 0..4 {
            let p = &self.points[x];
            let level1 = [
                0.5 * (p[0] + p[1]),
                0.5 * (p[1] + p[2]),
                0.5 * (p[2] + p[3]),
            ];
            let level2 = [0.5 * (level1[0] + level1[1]), 0.5 * (level1[1] + level1[2])];
            let level3 = 0.5 * (level2[0] + level2[1]);
            top.points[x] = [p[0], level1[0], level2[0], level3];
            bottom.points[x] = [level3, level2[1], level1[2], p[3]];
        }
        (top, bottom)
    }
    fn subdivide_horizontal(&self) -> (Self, Self) {
        let (mut left, mut right) = (Self::default(), Self::default());
        for y in 0..4 {
            let p = &self.points;
            let level1 = [
                0.5 * (p[0][y] + p[1][y]),
                0.5 * (p[1][y] + p[2][y]),
                0.5 * (p[2][y] + p[3][y]),
            ];
            let level2 = [0.5 * (level1[0] + level1[1]), 0.5 * (level1[1] + level1[2])];
            let level3 = 0.5 * (level2[0] + level2[1]);
            left.points[0][y] = p[0][y];
            left.points[1][y] = level1[0];
            left.points[2][y] = level2[0];
            left.points[3][y] = level3;
            right.points[0][y] = level3;
            right.points[1][y] = level2[1];
            right.points[2][y] = level1[2];
            right.points[3][y] = p[3][y];
        }
        (left, right)
    }
    fn subdivide(&self) -> (Self, Self, Self, Self) {
        let (top, bottom) = self.subdivide_vertical();
        let (top_left, top_right) = top.subdivide_horizontal();
        let (bottom_left, bottom_right) = bottom.subdivide_horizontal();
        (top_left, bottom_left, top_right, bottom_right)
    }
}

fn interpolate(first: i32, second: i32, delta: i32, scale: i32, overflow: &mut bool) -> i32 {
    let value = second
        .checked_sub(first)
        .and_then(|p| p.checked_mul(delta))
        .and_then(|p| p.checked_div(scale))
        .and_then(|p| p.checked_add(first));
    if value.is_none() {
        *overflow = true;
    }
    value.unwrap_or(0)
}
#[derive(Clone, Copy, Default)]
struct CoonColor {
    components: [i32; 3],
}
impl CoonColor {
    fn bi_interpol(colors: &[Self; 4], x: i32, y: i32, x_scale: i32, y_scale: i32) -> Option<Self> {
        let mut overflow = false;
        let components = std::array::from_fn(|i| {
            let first = interpolate(
                colors[0].components[i],
                colors[3].components[i],
                x,
                x_scale,
                &mut overflow,
            );
            let second = interpolate(
                colors[1].components[i],
                colors[2].components[i],
                x,
                x_scale,
                &mut overflow,
            );
            interpolate(first, second, y, y_scale, &mut overflow)
        });
        (!overflow).then_some(Self { components })
    }
    fn distance(self, other: Self) -> u32 {
        (0..3)
            .map(|i| self.components[i].abs_diff(other.components[i]))
            .max()
            .unwrap()
    }
}

struct PatchDrawer {
    path: crate::fxge::cfx_path::CfxPath,
    no_path_smooth: bool,
    alpha: i32,
    patch_colors: [CoonColor; 4],
}
impl PatchDrawer {
    fn new(alpha: i32, no_path_smooth: bool) -> Self {
        use crate::fxge::cfx_path::{CfxPath, PointType};
        let mut path = CfxPath::new();
        for i in 0..13 {
            path.append_point(
                CfxPointF::default(),
                if i == 0 {
                    PointType::Move
                } else {
                    PointType::Bezier
                },
            );
        }
        Self {
            path,
            no_path_smooth,
            alpha,
            patch_colors: [CoonColor::default(); 4],
        }
    }
    fn draw(
        &mut self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        x_scale: i32,
        y_scale: i32,
        left: i32,
        bottom: i32,
        patch: CubicBezierPatch,
        steps: Option<&[u32; SHADING_STEPS]>,
    ) {
        const THRESHOLD: u32 = 4;
        let small = patch.is_small();
        let Some(first) =
            CoonColor::bi_interpol(&self.patch_colors, left, bottom, x_scale, y_scale)
        else {
            return;
        };
        let mut differences = [0; 4]; // bottom, left, top, right.
        if !small {
            let Some(second) =
                CoonColor::bi_interpol(&self.patch_colors, left, bottom + 1, x_scale, y_scale)
            else {
                return;
            };
            let Some(third) =
                CoonColor::bi_interpol(&self.patch_colors, left + 1, bottom + 1, x_scale, y_scale)
            else {
                return;
            };
            let Some(fourth) =
                CoonColor::bi_interpol(&self.patch_colors, left + 1, bottom, x_scale, y_scale)
            else {
                return;
            };
            differences = [
                fourth.distance(first),
                second.distance(first),
                second.distance(third),
                third.distance(fourth),
            ];
        }
        let [d_bottom, d_left, d_top, d_right] = differences;
        if small || differences.iter().all(|&d| d < THRESHOLD) {
            for (point, position) in self.path.points_mut().iter_mut().zip(patch.boundary()) {
                point.point = position;
            }
            let mut options =
                crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions::winding_options();
            options.full_cover = true;
            options.aliased_path = self.no_path_smooth;
            let color = if let Some(steps) = steps {
                steps[first.components[0].clamp(0, SHADING_STEPS as i32 - 1) as usize]
            } else {
                argb_encode(
                    self.alpha as u32,
                    first.components[0] as u32,
                    first.components[1] as u32,
                    first.components[2] as u32,
                )
            };
            crate::fxge::cfx_renderdevice::draw_path_agg(
                device, &self.path, None, None, color, 0, options,
            );
        } else if d_bottom < THRESHOLD && d_top < THRESHOLD {
            let (top_patch, bottom_patch) = patch.subdivide_vertical();
            let (Some(y_scale), Some(bottom)) = (y_scale.checked_mul(2), bottom.checked_mul(2))
            else {
                return;
            };
            self.draw(device, x_scale, y_scale, left, bottom, top_patch, steps);
            self.draw(
                device,
                x_scale,
                y_scale,
                left,
                bottom + 1,
                bottom_patch,
                steps,
            );
        } else if d_left < THRESHOLD && d_right < THRESHOLD {
            let (left_patch, right_patch) = patch.subdivide_horizontal();
            let (Some(x_scale), Some(left)) = (x_scale.checked_mul(2), left.checked_mul(2)) else {
                return;
            };
            self.draw(device, x_scale, y_scale, left, bottom, left_patch, steps);
            self.draw(
                device,
                x_scale,
                y_scale,
                left + 1,
                bottom,
                right_patch,
                steps,
            );
        } else {
            let (top_left, bottom_left, top_right, bottom_right) = patch.subdivide();
            let (Some(x_scale), Some(y_scale), Some(left), Some(bottom)) = (
                x_scale.checked_mul(2),
                y_scale.checked_mul(2),
                left.checked_mul(2),
                bottom.checked_mul(2),
            ) else {
                return;
            };
            self.draw(device, x_scale, y_scale, left, bottom, top_left, steps);
            self.draw(
                device,
                x_scale,
                y_scale,
                left,
                bottom + 1,
                bottom_left,
                steps,
            );
            self.draw(device, x_scale, y_scale, left + 1, bottom, top_right, steps);
            self.draw(
                device,
                x_scale,
                y_scale,
                left + 1,
                bottom + 1,
                bottom_right,
                steps,
            );
        }
    }
}

fn draw_coon_patch_meshes(
    mesh: &mut CpdfMeshStream<'_>,
    bitmap: &mut CfxDibitmap,
    matrix: CfxMatrix,
    alpha: i32,
    no_path_smooth: bool,
    steps: Option<&[u32; SHADING_STEPS]>,
) {
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    let point_count = if mesh.shading_type() == ShadingType::TensorProductPatchMesh {
        16
    } else {
        12
    };
    let (min, max) = (mesh.component_min(0), mesh.component_max(0));
    let mut device = CfxAggDeviceDriver::new(bitmap, false, None);
    let mut drawer = PatchDrawer::new(alpha, no_path_smooth);
    let mut coords = [CfxPointF::default(); 16];
    while !mesh.is_eof() {
        if !mesh.can_read_flag() {
            break;
        }
        let flag = mesh.read_flag() as usize;
        let (start_point, start_color) = if flag != 0 {
            let shared = std::array::from_fn::<_, 4, _>(|i| coords[(flag * 3 + i) % 12]);
            coords[..4].copy_from_slice(&shared);
            let colors = [
                drawer.patch_colors[flag],
                drawer.patch_colors[(flag + 1) % 4],
            ];
            drawer.patch_colors[..2].copy_from_slice(&colors);
            (4, 2)
        } else {
            (0, 0)
        };
        for point in coords.iter_mut().take(point_count).skip(start_point) {
            if !mesh.can_read_coords() {
                break;
            }
            *point = matrix.transform(mesh.read_coords());
        }
        for color in drawer.patch_colors.iter_mut().skip(start_color) {
            if !mesh.can_read_color() {
                break;
            }
            let rgb = mesh.read_color();
            color.components = if steps.is_none() {
                [
                    (rgb.red * 255.0) as i32,
                    (rgb.green * 255.0) as i32,
                    (rgb.blue * 255.0) as i32,
                ]
            } else {
                [component_to_shading_index(rgb.red, min, max) as i32, 0, 0]
            };
        }
        // A partial record preserves earlier/default control points and colors,
        // just as the source's inner loops break without rejecting the patch.
        let bbox = CfxFloatRect::get_bbox(&coords[..point_count]);
        if bbox.right <= 0.0
            || bbox.left >= device.pixel_width() as f32
            || bbox.top <= 0.0
            || bbox.bottom >= device.pixel_height() as f32
        {
            continue;
        }
        let mut patch = CubicBezierPatch::default();
        let p = &mut patch.points;
        p[0].copy_from_slice(&coords[..4]);
        p[1][3] = coords[4];
        p[2][3] = coords[5];
        p[3][3] = coords[6];
        p[3][2] = coords[7];
        p[3][1] = coords[8];
        p[3][0] = coords[9];
        p[2][0] = coords[10];
        p[1][0] = coords[11];
        if point_count == 16 {
            p[1][1] = coords[12];
            p[1][2] = coords[13];
            p[2][2] = coords[14];
            p[2][1] = coords[15];
        } else {
            // ISO 32000-2:2020 8.7.4.5.8; retain the source's operation order.
            p[1][1] = (1.0 / 9.0)
                * (-4.0 * p[0][0] + 6.0 * (p[0][1] + p[1][0]) - 2.0 * (p[0][3] + p[3][0])
                    + 3.0 * (p[3][1] + p[1][3])
                    - 1.0 * p[3][3]);
            p[1][2] = (1.0 / 9.0)
                * (-4.0 * p[0][3] + 6.0 * (p[0][2] + p[1][3]) - 2.0 * (p[0][0] + p[3][3])
                    + 3.0 * (p[3][2] + p[1][0])
                    - 1.0 * p[3][0]);
            p[2][1] = (1.0 / 9.0)
                * (-4.0 * p[3][0] + 6.0 * (p[3][1] + p[2][0]) - 2.0 * (p[3][3] + p[0][0])
                    + 3.0 * (p[0][1] + p[2][3])
                    - 1.0 * p[0][3]);
            p[2][2] = (1.0 / 9.0)
                * (-4.0 * p[3][3] + 6.0 * (p[3][2] + p[2][3]) - 2.0 * (p[3][0] + p[0][3])
                    + 3.0 * (p[0][2] + p[2][0])
                    - 1.0 * p[0][0]);
        }
        drawer.draw(&mut device, 1, 1, 0, 0, patch, steps);
    }
}
