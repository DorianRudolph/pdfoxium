// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
fn encoded(
    bits: u16,
    compression: u32,
    width: u32,
    height: i32,
    palette: &[u32],
    pixels: &[u8],
) -> Vec<u8> {
    let mut result = vec![0; 54];
    result[..2].copy_from_slice(b"BM");
    result[14..18].copy_from_slice(&40u32.to_le_bytes());
    result[18..22].copy_from_slice(&width.to_le_bytes());
    result[22..26].copy_from_slice(&height.to_le_bytes());
    result[26..28].copy_from_slice(&1u16.to_le_bytes());
    result[28..30].copy_from_slice(&bits.to_le_bytes());
    result[30..34].copy_from_slice(&compression.to_le_bytes());
    result[46..50].copy_from_slice(&(palette.len() as u32).to_le_bytes());
    for color in palette {
        result.extend_from_slice(&color.to_le_bytes());
    }
    let offset = result.len() as u32;
    result[10..14].copy_from_slice(&offset.to_le_bytes());
    result.extend_from_slice(pixels);
    let size = result.len() as u32;
    result[2..6].copy_from_slice(&size.to_le_bytes());
    result
}
#[test]
fn rle4_encoded_absolute_delta_and_bottom_up() {
    let palette = [0, 0xff0000, 0x00ff00, 0x0000ff];
    let source = encoded(
        4,
        2,
        6,
        3,
        &palette,
        &[
            6, 0x12, 0, 0, 0, 2, 2, 0, 0, 4, 0x32, 0x13, 0, 0, 6, 0x23, 0, 1,
        ],
    );
    let image = decode(&source).unwrap();
    assert_eq!(
        image.pixels,
        [2, 3, 2, 3, 2, 3, 0, 0, 3, 2, 1, 3, 1, 2, 1, 2, 1, 2]
    );
}
#[test]
fn rle8_absolute_padding_and_delta_rows() {
    let source = encoded(
        8,
        1,
        5,
        -3,
        &[0, 1, 2, 3],
        &[0, 3, 1, 2, 3, 0, 0, 0, 0, 2, 1, 1, 4, 2, 0, 1],
    );
    let image = decode(&source).unwrap();
    assert_eq!(image.pixels, [1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 2]);
}
#[test]
fn malformed_runs_palette_and_truncated_commands_are_errors() {
    for pixels in [
        &[6, 1, 0, 1][..],
        &[1, 2, 0, 1],
        &[0, 2, 0, 4],
        &[0, 3, 1, 1],
        &[1][..],
    ] {
        assert!(decode(&encoded(8, 1, 5, 2, &[0, 1], pixels)).is_err());
    }
    let source = encoded(4, 0, 2, 1, &[0, 1], &[0x12, 0, 0, 0]);
    assert!(decode(&source).is_err());
}
#[test]
fn arbitrary_32bit_masks_are_applied_and_overlap_rejected() {
    let mut source = encoded(32, 3, 1, 1, &[], &[]);
    source.extend_from_slice(&0xffu32.to_le_bytes());
    source.extend_from_slice(&0xff00u32.to_le_bytes());
    source.extend_from_slice(&0xff0000u32.to_le_bytes());
    source[10..14].copy_from_slice(&66u32.to_le_bytes());
    source.extend_from_slice(&[10, 20, 30, 99]);
    assert_eq!(decode(&source).unwrap().pixels, [30, 20, 10, 255]);
    source[58..62].copy_from_slice(&0xffu32.to_le_bytes());
    assert!(read_header(&source).is_err());
}
#[test]
fn every_header_prefix_is_transactionally_incomplete() {
    let source = encoded(24, 0, 1, 1, &[], &[10, 20, 30, 0]);
    for prefix in 0..54 {
        assert_eq!(
            read_header(&source[..prefix]).unwrap_err(),
            CodecError::NeedMoreData
        );
    }
    assert_eq!(decode(&source).unwrap().pixels, [10, 20, 30]);
}
#[test]
fn two_bit_palette_and_v2_v3_v4_v5_embedded_bitfields() {
    let image = decode(&encoded(2, 0, 4, 1, &[0, 1, 2, 3], &[0x1b, 0, 0, 0])).unwrap();
    assert_eq!(image.pixels, [0, 1, 2, 3]);
    for size in [52u32, 56, 108, 124] {
        let mut source = encoded(32, 3, 1, -1, &[], &[]);
        source.resize(14 + size as usize, 0);
        source[14..18].copy_from_slice(&size.to_le_bytes());
        for (offset, mask) in [(54, 0x00ff0000u32), (58, 0x0000ff00), (62, 0x000000ff)] {
            source[offset..offset + 4].copy_from_slice(&mask.to_le_bytes());
        }
        if size >= 56 {
            source[66..70].copy_from_slice(&0xff000000u32.to_le_bytes());
        }
        let offset = source.len() as u32;
        source[10..14].copy_from_slice(&offset.to_le_bytes());
        source.extend([20, 40, 60, 80]);
        let image = decode(&source).unwrap();
        assert_eq!(
            image.pixels,
            if size >= 56 {
                [20, 40, 60, 80]
            } else {
                [20, 40, 60, 255]
            }
        );
        assert_eq!(
            image.info.format,
            if size >= 56 {
                Format::Bgra
            } else {
                Format::Bgrx
            }
        );
    }
}
#[test]
fn sixteen_bit_alpha_expansion_and_malformed_extended_masks() {
    let mut source = encoded(16, 3, 1, 1, &[], &[]);
    source.resize(70, 0);
    source[14..18].copy_from_slice(&56u32.to_le_bytes());
    for (offset, mask) in [(54, 0x0f00u32), (58, 0x00f0), (62, 0x000f), (66, 0xf000)] {
        source[offset..offset + 4].copy_from_slice(&mask.to_le_bytes());
    }
    source[10..14].copy_from_slice(&70u32.to_le_bytes());
    source.extend([0x1f, 0x82, 0, 0]);
    let image = decode(&source).unwrap();
    assert_eq!(image.info.format, Format::Bgra);
    assert_eq!(image.pixels, [255, 17, 34, 136]);
    source[66..70].copy_from_slice(&0xf100u32.to_le_bytes());
    assert!(read_header(&source).is_err());
    source[66..70].copy_from_slice(&0x0f00u32.to_le_bytes());
    assert!(read_header(&source).is_err());
}
