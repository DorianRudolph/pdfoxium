// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! The existing per-pixel ICC route is an independent oracle for decoded-row
//! batching, including both float/byte quantization boundaries. Profiles are
//! generated from small public moxcms representations, not ignored book data.

use super::*;
use crate::fpdfapi::page::cpdf_iccbasedcs::CpdfIccBasedCs;
use crate::fpdfapi::page::cpdf_iccprofile::CpdfIccProfile;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
use crate::fpdfapi::parser::cpdf_object::{make_reference, retain_object};
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use moxcms::{
    ColorProfile, DataColorSpace, LutDataType, LutStore, LutType, LutWarehouse, Matrix3d,
    ProfileClass,
};

fn cmyk_profile() -> Vec<u8> {
    let mut profile = ColorProfile::default();
    profile.color_space = DataColorSpace::Cmyk;
    profile.pcs = DataColorSpace::Xyz;
    profile.profile_class = ProfileClass::InputDevice;
    profile.white_point = ColorProfile::new_srgb().white_point;
    profile.media_white_point = Some(profile.white_point);
    // A tiny, deterministic CMYK->XYZ CLUT with two nodes per dimension.
    // Its PCS samples use ICC's u1.15 encoding. The row oracle uses the same
    // parsed profile, so this tests batching without depending on a printer's
    // proprietary characterization data or assuming a particular RGB result.
    let mut clut = Vec::new();
    for c in [0.0f64, 1.0] {
        for m in [0.0f64, 1.0] {
            for y in [0.0f64, 1.0] {
                for k in [0.0f64, 1.0] {
                    let r = (1.0 - c) * (1.0 - k);
                    let g = (1.0 - m) * (1.0 - k);
                    let b = (1.0 - y) * (1.0 - k);
                    for xyz in [
                        0.436_074_7 * r + 0.385_064_9 * g + 0.143_080_4 * b,
                        0.222_504_5 * r + 0.716_878_6 * g + 0.060_616_9 * b,
                        0.013_932_2 * r + 0.097_104_5 * g + 0.714_173_3 * b,
                    ] {
                        clut.push((xyz * 32768.0).round().clamp(0.0, 65535.0) as u16);
                    }
                }
            }
        }
    }
    profile.lut_a_to_b_perceptual = Some(LutWarehouse::Lut(LutDataType {
        num_input_channels: 4,
        num_output_channels: 3,
        num_clut_grid_points: 2,
        matrix: Matrix3d::IDENTITY,
        num_input_table_entries: 2,
        num_output_table_entries: 2,
        input_table: LutStore::Store16([0, 65535].repeat(4)),
        clut_table: LutStore::Store16(clut),
        output_table: LutStore::Store16([0, 65535].repeat(3)),
        lut_type: LutType::Lut16,
    }));
    profile.encode().unwrap()
}

fn icc_space(bytes: Vec<u8>, components: usize) -> ColorSpaceRef {
    let mut dictionary = CpdfDictionary::new();
    dictionary.set_new_for(
        b"N",
        CpdfScalarObject::number_from_signed(components as i32),
    );
    let stream = retain_object(CpdfStream::from_data_and_dict(
        bytes,
        retain_object(dictionary),
    ));
    let holder = CpdfIndirectObjectHolder::new();
    holder.borrow_mut().add_indirect_object(stream.clone());
    let mut array = CpdfArray::new();
    array.append_new_name(b"ICCBased");
    array.append(make_reference(&stream, &holder));
    Rc::new(
        CpdfIccBasedCs::load(
            &retain_object(array),
            |_| None,
            |stream, count| CpdfIccProfile::new(stream, count).map(Rc::new),
        )
        .unwrap(),
    )
}

fn make_dib(
    space: ColorSpaceRef,
    width: usize,
    bits: i32,
    decode: Option<&[(f32, f32)]>,
    color_key: bool,
) -> CpdfDib {
    let components = space.component_count() as usize;
    let mut dictionary = CpdfDictionary::new();
    for (name, value) in [
        (b"Width".as_slice(), width as i32),
        (b"Height".as_slice(), 1),
        (b"BitsPerComponent".as_slice(), bits),
    ] {
        dictionary.set_new_for(name, CpdfScalarObject::number_from_signed(value));
    }
    // The resolved resource is supplied separately, as by the real caller.
    dictionary.set_new_name_for(b"ColorSpace", b"ICC0");
    if let Some(decode) = decode {
        let mut values = CpdfArray::new();
        for &(min, max) in decode {
            values.append_new(CpdfScalarObject::number_from_float(min));
            values.append_new(CpdfScalarObject::number_from_float(max));
        }
        dictionary.set_new_for(b"Decode", values);
    }
    if color_key {
        let mut mask = CpdfArray::new();
        for _ in 0..components {
            mask.append_new(CpdfScalarObject::number_from_signed(0));
            mask.append_new(CpdfScalarObject::number_from_signed(63));
        }
        dictionary.set_new_for(b"Mask", mask);
    }
    let bytes = vec![0; width * components * (bits as usize / 8)];
    CpdfDib::load(
        retain_object(CpdfStream::from_data_and_dict(
            bytes,
            retain_object(dictionary),
        )),
        Some(space),
    )
    .unwrap()
}

#[test]
fn normal_icc_decode_rows_match_per_pixel_quantization_at_all_byte_values_and_tails() {
    for (components, bytes) in [
        (1, ColorProfile::new_gray_with_gamma(2.2).encode().unwrap()),
        (3, ColorProfile::new_adobe_rgb().encode().unwrap()),
        (4, cmyk_profile()),
    ] {
        let space = icc_space(bytes, components);
        let profile = space.get_icc_profile().unwrap();
        assert!(profile.is_supported());
        assert!(profile.is_normal());
        for pattern in 0..3 {
            let decode: Vec<_> = (0..components)
                .map(|channel| match pattern {
                    0 => (1.0, 0.0),
                    1 => (
                        0.125 + channel as f32 * 0.0625,
                        0.875 - channel as f32 * 0.0625,
                    ),
                    _ => (-1.0 + channel as f32 * 0.25, 2.0 - channel as f32 * 0.125),
                })
                .collect();
            for width in [
                1usize, 2, 3, 4, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 257,
            ] {
                for color_key in [false, true] {
                    let dib = make_dib(space.clone(), width, 8, Some(&decode), color_key);
                    assert!(!dib.default_decode);
                    for row in 0..3 {
                        let mut source = vec![0xa5; 5 + width * components + 13];
                        for col in 0..width {
                            for channel in 0..components {
                                source[5 + col * components + channel] =
                                    (col * 73 + channel * 41 + row * 19) as u8;
                            }
                        }
                        let before = source.clone();
                        let mut expected = vec![0x6b; 3 + width * 3 + 11];
                        dib.translate_decoded_scanline_scalar(&mut expected[3..], &source[5..]);
                        let mut actual = vec![0x6b; expected.len()];
                        assert!(
                            dib.try_translate_icc_decode_scanline(&mut actual[3..], &source[5..])
                        );
                        assert_eq!(actual, expected, "components={components} pattern={pattern} width={width} color_key={color_key} row={row}");
                        assert_eq!(source, before);
                    }
                }
            }
        }
    }
}

#[test]
fn icc_decode_batch_declines_default_srgb_signature_lab_and_nonbyte_rows_without_writes() {
    let rgb = icc_space(ColorProfile::new_srgb().encode().unwrap(), 3);
    let default = make_dib(rgb.clone(), 7, 8, None, false);
    let sixteen_bit = make_dib(rgb, 7, 16, Some(&[(1.0, 0.0); 3]), false);
    let mut signature = vec![0; 3144];
    signature[400..417].copy_from_slice(b"sRGB IEC61966-2.1");
    let srgb = make_dib(icc_space(signature, 3), 7, 8, Some(&[(1.0, 0.0); 3]), false);
    assert!(srgb
        .color_space
        .as_ref()
        .unwrap()
        .get_icc_profile()
        .unwrap()
        .is_srgb());
    let lab_space = icc_space(ColorProfile::new_lab().encode().unwrap(), 3);
    let lab_profile = lab_space.get_icc_profile().unwrap();
    assert!(lab_profile.is_supported());
    assert!(!lab_profile.is_normal());
    let lab = make_dib(lab_space, 7, 8, Some(&[(1.0, 0.0); 3]), false);
    for dib in [default, sixteen_bit, srgb, lab] {
        let mut dest = [0x73; 31];
        let source = [0x39; 64];
        assert!(!dib.try_translate_icc_decode_scanline(&mut dest, &source));
        assert_eq!(dest, [0x73; 31]);
        assert_eq!(source, [0x39; 64]);
    }
}

#[test]
fn changing_decode_parameters_invalidates_cached_icc_tables_through_real_scanlines() {
    let space = icc_space(ColorProfile::new_adobe_rgb().encode().unwrap(), 3);
    let mut dib = make_dib(space, 9, 8, Some(&[(1.0, 0.0); 3]), false);
    // The zero source row initially decodes to white. Go through GetScanline
    // so this exercises the shared profile cache through the real caller.
    let original = dib.get_scanline(0).to_vec();

    dib.comp_data.clear();
    let mut dictionary = CpdfDictionary::new();
    let mut decode = CpdfArray::new();
    for _ in 0..3 {
        decode.append_new(CpdfScalarObject::number_from_float(0.0));
        decode.append_new(CpdfScalarObject::number_from_float(0.5));
    }
    dictionary.set_new_for(b"Decode", decode);
    dib.get_decode_and_mask_array(&dictionary);

    // The same encoded zero row now decodes to black. A stale inverse table
    // would keep returning the first row, even though scalar translation uses
    // the current component metadata.
    let actual = dib.get_scanline(0).to_vec();
    let mut expected = vec![0; 27];
    dib.translate_decoded_scanline_scalar(&mut expected, &[0; 27]);
    assert_ne!(actual, original);
    assert_eq!(actual, expected);
}

#[test]
fn decoded_icc_rows_keep_color_keys_in_original_sample_space() {
    let space = icc_space(ColorProfile::new_adobe_rgb().encode().unwrap(), 3);
    let dib = make_dib(space, 9, 8, Some(&[(1.0, 0.0); 3]), true);
    // Raw zero samples are inside the key ranges; decoded white samples are
    // outside them. Alpha must be extracted before Decode and ICC conversion.
    let mut colors = vec![0; 27];
    dib.translate_decoded_scanline_scalar(&mut colors, &[0; 27]);
    let expected: Vec<u8> = colors
        .chunks_exact(3)
        .flat_map(|p| [p[0], p[1], p[2], 0])
        .collect();
    assert_eq!(&*dib.get_scanline(0), expected);
}
