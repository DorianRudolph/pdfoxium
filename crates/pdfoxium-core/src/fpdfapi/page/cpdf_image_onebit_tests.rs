// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_object::retain_object;

fn bit(x: usize, y: usize) -> bool {
    (x + 2 * y) % 3 != 0
}
fn source(width: i32, format: FxdibFormat) -> CfxDibitmap {
    let mut bitmap = CfxDibitmap::create(width, 3, format, 8).unwrap();
    bitmap.buffer_mut().fill(0xa5);
    for y in 0..3 {
        let row = bitmap.scanline_mut(y).unwrap();
        row[..(width as usize).div_ceil(8)].fill(255);
        for x in 0..width as usize {
            let mask = 0x80 >> (x % 8);
            if bit(x, y as usize) {
                row[x / 8] |= mask;
            } else {
                row[x / 8] &= !mask;
            }
        }
    }
    bitmap
}
fn empty_image() -> CpdfImage {
    CpdfImage::from_stream(retain_object(CpdfStream::from_bytes(&[])), false).unwrap()
}
fn packed(bitmap: &CfxDibitmap) -> Vec<u8> {
    let count = (bitmap.width() as usize).div_ceil(8);
    let mut out = Vec::new();
    for y in 0..bitmap.height() {
        out.extend_from_slice(&bitmap.scanline(y).unwrap()[..count]);
    }
    out
}
fn raw(image: &CpdfImage) -> Vec<u8> {
    image
        .stream()
        .borrow()
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .get_in_memory_raw_data()
        .to_vec()
}
fn array(dict: &CpdfDictionary, name: &[u8]) -> ObjectRef {
    dict.get_array_for(name).unwrap()
}
fn verify_mask(image: &CpdfImage, width: i32, inverted: bool) {
    let dict = image.dict();
    let borrowed = dict.borrow();
    let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
    assert!(dict.get_boolean_for(b"ImageMask", false));
    assert_eq!(dict.get_integer_for(b"BitsPerComponent", 0), 1);
    assert!(!dict.key_exist(b"ColorSpace"));
    assert!(!dict.key_exist(b"SMask"));
    if inverted {
        let decode = array(dict, b"Decode");
        let b = decode.borrow();
        let decode = b.as_any().downcast_ref::<CpdfArray>().unwrap();
        assert_eq!(decode.get_integer_at(0), 1);
        assert_eq!(decode.get_integer_at(1), 0);
    } else {
        assert!(!dict.key_exist(b"Decode"));
    }
    drop(borrowed);
    let mut decoded = image
        .decode_image_data(None)
        .unwrap()
        .bitmap
        .realize()
        .unwrap();
    assert!(decoded.is_mask_format());
    assert!(decoded.convert_format(FxdibFormat::EightBppMask));
    for y in 0..3 {
        for x in 0..width as usize {
            let painted = if inverted { bit(x, y) } else { !bit(x, y) };
            assert_eq!(
                decoded.scanline(y as i32).unwrap()[x],
                if painted { 255 } else { 0 },
                "{width} {x},{y}"
            );
        }
    }
}
fn verify_colors(image: &CpdfImage, width: i32, palette: [u32; 2]) {
    let dict = image.dict();
    let b = dict.borrow();
    let dict = b.as_any().downcast_ref::<CpdfDictionary>().unwrap();
    assert_eq!(dict.get_integer_for(b"Width", 0), width);
    assert_eq!(dict.get_integer_for(b"Height", 0), 3);
    assert_eq!(dict.get_integer_for(b"BitsPerComponent", 0), 1);
    assert!(!dict.key_exist(b"ImageMask"));
    assert!(!dict.key_exist(b"Decode"));
    assert!(!dict.key_exist(b"SMask"));
    let cs = array(dict, b"ColorSpace");
    let a = cs.borrow();
    let cs = a.as_any().downcast_ref::<CpdfArray>().unwrap();
    assert_eq!(cs.get_byte_string_at(0), b"Indexed");
    assert_eq!(cs.get_byte_string_at(1), b"DeviceRGB");
    assert_eq!(cs.get_integer_at(2), 1);
    let expected: Vec<u8> = palette
        .into_iter()
        .flat_map(|c| [(c >> 16) as u8, (c >> 8) as u8, c as u8])
        .collect();
    assert_eq!(cs.get_byte_string_at(3), expected);
    drop(a);
    drop(b);
    let mut decoded = image
        .decode_image_data(None)
        .unwrap()
        .bitmap
        .realize()
        .unwrap();
    if decoded.bpp() == 1 {
        assert!(decoded.convert_format(FxdibFormat::Bgr));
    }
    assert!(decoded.convert_format(FxdibFormat::Bgra));
    for y in 0..3 {
        for x in 0..width as usize {
            let color = palette[usize::from(bit(x, y))];
            assert_eq!(
                &decoded.scanline(y as i32).unwrap()[x * 4..x * 4 + 4],
                &[color as u8, (color >> 8) as u8, (color >> 16) as u8, 255]
            );
        }
    }
}
#[test]
fn odd_width_color_images_copy_logical_packed_rows_without_stride_poison() {
    let objects = CpdfIndirectObjectHolder::new();
    for width in [1, 7, 8, 9] {
        let mut bitmap = source(width, FxdibFormat::OneBppRgb);
        let palette = [0xff145adc, 0xffe6284f];
        bitmap.set_palette(&palette);
        let before = bitmap.buffer().to_vec();
        let payload = packed(&bitmap);
        let image = empty_image();
        assert!(image.set_bitmap(&bitmap, &objects));
        assert_eq!(raw(&image), payload);
        assert_eq!(raw(&image).len(), 3 * (width as usize).div_ceil(8));
        assert_eq!(bitmap.buffer(), before);
        bitmap.buffer_mut().fill(0);
        assert_eq!(raw(&image), payload);
        verify_colors(&image, width, palette);
    }
}
#[test]
fn implicit_one_bit_palette_is_encoded_as_explicit_black_and_white() {
    let objects = CpdfIndirectObjectHolder::new();
    let bitmap = source(9, FxdibFormat::OneBppRgb);
    let image = empty_image();
    assert!(bitmap.palette().is_empty());
    assert!(image.set_bitmap(&bitmap, &objects));
    verify_colors(&image, 9, [0xff000000, 0xffffffff]);
}
#[test]
fn transparent_zero_palette_entry_uses_inverted_stencil_coverage() {
    let objects = CpdfIndirectObjectHolder::new();
    for width in [1, 7, 8, 9] {
        let mut bitmap = source(width, FxdibFormat::OneBppRgb);
        bitmap.set_palette(&[0x00112233, 0xff145adc]);
        let payload = packed(&bitmap);
        let image = empty_image();
        assert!(image.set_bitmap(&bitmap, &objects));
        assert_eq!(raw(&image), payload);
        verify_mask(&image, width, true);
    }
}
#[test]
fn transparent_one_palette_entry_uses_default_stencil_coverage() {
    let objects = CpdfIndirectObjectHolder::new();
    for width in [1, 7, 8, 9] {
        let mut bitmap = source(width, FxdibFormat::OneBppRgb);
        bitmap.set_palette(&[0xff145adc, 0x00445566]);
        let payload = packed(&bitmap);
        let image = empty_image();
        assert!(image.set_bitmap(&bitmap, &objects));
        assert_eq!(raw(&image), payload);
        verify_mask(&image, width, false);
    }
}
#[test]
fn one_bit_mask_storage_preserves_set_bit_coverage_after_embedding() {
    let objects = CpdfIndirectObjectHolder::new();
    for width in [1, 7, 8, 9] {
        let bitmap = source(width, FxdibFormat::OneBppMask);
        let payload = packed(&bitmap);
        let image = empty_image();
        assert!(image.set_bitmap(&bitmap, &objects));
        assert!(image.is_mask());
        assert_eq!(raw(&image), payload);
        verify_mask(&image, width, true);
    }
}
#[test]
fn replacing_a_mask_with_color_does_not_mutate_an_already_referenced_stream() {
    let objects = CpdfIndirectObjectHolder::new();
    let mask = source(7, FxdibFormat::OneBppMask);
    let image = empty_image();
    assert!(image.set_bitmap(&mask, &objects));
    let old_payload = raw(&image);
    image.convert_stream_to_indirect_object(&objects);
    let old = image.stream();
    let number = old.borrow().obj_num();
    assert_ne!(number, 0);
    let mut color = source(9, FxdibFormat::OneBppRgb);
    let palette = [0xff145adc, 0xffe6284f];
    color.set_palette(&palette);
    assert!(image.set_bitmap(&color, &objects));
    assert!(!image.is_mask());
    assert_eq!((image.pixel_width(), image.pixel_height()), (9, 3));
    assert!(!Rc::ptr_eq(&old, &image.stream()));
    assert!(Rc::ptr_eq(
        &old,
        &objects.borrow().get_indirect_object(number).unwrap()
    ));
    assert_eq!(
        old.borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_in_memory_raw_data(),
        old_payload
    );
    let previous = CpdfImage::from_stream(old, false).unwrap();
    verify_mask(&previous, 7, true);
    verify_colors(&image, 9, palette);
}
