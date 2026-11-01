// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Small real TIFFs exercise the RGBA/decompression boundary, not mock samples.
use super::*;
use std::collections::BTreeMap;
#[derive(Clone)]
enum Field {
    Shorts(Vec<u16>),
    Longs(Vec<u32>),
    Rationals(Vec<(u32, u32)>),
}
impl Field {
    fn bytes(&self, little: bool) -> (u16, usize, Vec<u8>) {
        match self {
            Self::Shorts(v) => (
                3,
                v.len(),
                v.iter()
                    .flat_map(|n| {
                        if little {
                            n.to_le_bytes()
                        } else {
                            n.to_be_bytes()
                        }
                    })
                    .collect(),
            ),
            Self::Longs(v) => (
                4,
                v.len(),
                v.iter()
                    .flat_map(|n| {
                        if little {
                            n.to_le_bytes()
                        } else {
                            n.to_be_bytes()
                        }
                    })
                    .collect(),
            ),
            Self::Rationals(v) => (
                5,
                v.len(),
                v.iter()
                    .flat_map(|(n, d)| [*n, *d])
                    .flat_map(|n| {
                        if little {
                            n.to_le_bytes()
                        } else {
                            n.to_be_bytes()
                        }
                    })
                    .collect(),
            ),
        }
    }
}
fn short(tag: u16, value: u16) -> (u16, Field) {
    (tag, Field::Shorts(vec![value]))
}
fn long(tag: u16, value: u32) -> (u16, Field) {
    (tag, Field::Longs(vec![value]))
}
fn put(value: u64, size: usize, little: bool) -> Vec<u8> {
    if little {
        value.to_le_bytes()[..size].to_vec()
    } else {
        value.to_be_bytes()[8 - size..].to_vec()
    }
}
fn fixture(
    width: u32,
    height: u32,
    bits: u16,
    samples: u16,
    photo: u16,
    fields: Vec<(u16, Field)>,
    chunks: &[Vec<u8>],
    little: bool,
    big: bool,
) -> Vec<u8> {
    let mut tags = BTreeMap::from([
        long(256, width),
        long(257, height),
        (258, Field::Shorts(vec![bits; samples as usize])),
        short(259, 1),
        short(262, photo),
        short(277, samples),
        long(278, height),
        short(284, 1),
    ]);
    tags.extend(fields);
    let tiled = tags.contains_key(&322);
    if tiled {
        tags.remove(&278);
    }
    let offset_tag = if tiled { 324 } else { 273 };
    let count_tag = if tiled { 325 } else { 279 };
    tags.insert(offset_tag, Field::Longs(vec![0; chunks.len()]));
    tags.insert(
        count_tag,
        Field::Longs(chunks.iter().map(|c| c.len() as u32).collect()),
    );
    let (header_size, count_size, entry_size, pointer_size) =
        if big { (16, 8, 20, 8) } else { (8, 2, 12, 4) };
    let table_end = header_size + count_size + entry_size * tags.len() + pointer_size;
    let extra_len: usize = tags
        .values()
        .map(|v| v.bytes(little).2.len())
        .filter(|&len| len > pointer_size)
        .sum();
    let data_start = table_end + extra_len;
    let mut offset = data_start;
    let mut offsets = Vec::new();
    for chunk in chunks {
        offsets.push(offset as u32);
        offset += chunk.len();
    }
    tags.insert(offset_tag, Field::Longs(offsets));
    let mut data = if little {
        b"II".to_vec()
    } else {
        b"MM".to_vec()
    };
    data.extend(put(if big { 43 } else { 42 }, 2, little));
    if big {
        data.extend(put(8, 2, little));
        data.extend(put(0, 2, little));
    }
    data.extend(put(header_size as u64, pointer_size, little));
    data.extend(put(tags.len() as u64, count_size, little));
    let mut extra = Vec::new();
    for (tag, value) in tags {
        let (kind, count, bytes) = value.bytes(little);
        data.extend(put(u64::from(tag), 2, little));
        data.extend(put(u64::from(kind), 2, little));
        data.extend(put(count as u64, pointer_size, little));
        if bytes.len() <= pointer_size {
            data.extend(&bytes);
            data.resize(data.len() + pointer_size - bytes.len(), 0);
        } else {
            data.extend(put((table_end + extra.len()) as u64, pointer_size, little));
            extra.extend(bytes);
        }
    }
    data.extend(put(0, pointer_size, little));
    data.extend(extra);
    assert_eq!(data.len(), data_start);
    for chunk in chunks {
        data.extend(chunk);
    }
    data
}
fn gray_values(image: &DecodedImage) -> Vec<u8> {
    image
        .pixels
        .chunks_exact(4)
        .map(|p| {
            assert_eq!(p, [p[0], p[0], p[0], 255]);
            p[0]
        })
        .collect()
}
fn rgb(red: u8, green: u8, blue: u8) -> Vec<u8> {
    vec![blue, green, red, 255]
}

#[test]
fn palettes_all_packed_depths_orders_and_big_tiff() {
    for bits in [1u8, 2, 4, 8] {
        let colors = 1usize << bits;
        let map = (0..3 * colors)
            .map(|i| ((i % colors) * 255 / (colors - 1)) as u16 * 257)
            .collect::<Vec<_>>();
        for (little, big) in [(true, false), (false, false), (true, true), (false, true)] {
            let pixels = if bits == 8 {
                vec![0, 255]
            } else {
                vec![(colors as u8 - 1) << (8 - 2 * bits)]
            };
            let data = fixture(
                2,
                1,
                u16::from(bits),
                1,
                3,
                vec![(320, Field::Shorts(map.clone()))],
                &[pixels],
                little,
                big,
            );
            assert_eq!(
                gray_values(&decode(&data).unwrap()),
                [0, 255],
                "bits={bits} little={little} big={big}"
            );
        }
    }
    // Old TIFF writers used 8-bit values in a SHORT colormap. Native accepts
    // these without shifting; a truly 16-bit table is shifted as a whole.
    let data = fixture(
        2,
        1,
        1,
        1,
        3,
        vec![(320, Field::Shorts(vec![10, 20, 30, 40, 50, 60]))],
        &[vec![0x40]],
        true,
        false,
    );
    assert_eq!(
        decode(&data).unwrap().pixels,
        [50, 30, 10, 255, 60, 40, 20, 255]
    );
}
#[test]
fn palette_packbits_compressed_multiple_strips() {
    let data = fixture(
        2,
        2,
        8,
        1,
        3,
        vec![
            short(259, 32773),
            long(278, 1),
            (
                320,
                Field::Shorts({
                    let mut map = vec![0; 768];
                    map[1] = 65535;
                    map[256 + 2] = 65535;
                    map[512 + 3] = 65535;
                    map
                }),
            ),
        ],
        &[vec![1, 1, 2], vec![1, 3, 1]],
        true,
        false,
    );
    assert_eq!(
        decode(&data).unwrap().pixels,
        [
            rgb(255, 0, 0),
            rgb(0, 255, 0),
            rgb(0, 0, 255),
            rgb(255, 0, 0)
        ]
        .concat()
    );
}
#[test]
fn planar_rgb_and_alpha_extra_samples() {
    let data = fixture(
        2,
        1,
        8,
        3,
        2,
        vec![short(284, 2)],
        &[vec![10, 40], vec![20, 50], vec![30, 60]],
        false,
        false,
    );
    assert_eq!(
        decode(&data).unwrap().pixels,
        [30, 20, 10, 255, 60, 50, 40, 255]
    );
    for alpha in [0, 1, 2] {
        let data = fixture(
            1,
            1,
            8,
            4,
            2,
            vec![short(284, 2), (338, Field::Shorts(vec![alpha]))],
            &[vec![100], vec![60], vec![20], vec![128]],
            true,
            true,
        );
        let expected = if alpha == 2 {
            [10, 30, 50, 128]
        } else {
            [20, 60, 100, 128]
        };
        assert_eq!(decode(&data).unwrap().pixels, expected);
    }
    let data = fixture(
        1,
        1,
        8,
        4,
        2,
        vec![],
        &[vec![100, 60, 20, 128]],
        true,
        false,
    );
    assert_eq!(decode(&data).unwrap().pixels, [20, 60, 100, 128]);
}
#[test]
fn extra_channels_are_ignored_and_gray_alpha_is_not_inverted() {
    let data = fixture(
        1,
        1,
        8,
        5,
        2,
        vec![(338, Field::Shorts(vec![0, 0]))],
        &[vec![100, 60, 20, 128, 99]],
        true,
        false,
    );
    assert_eq!(decode(&data).unwrap().pixels, [20, 60, 100, 128]);
    let data = fixture(
        1,
        1,
        8,
        2,
        0,
        vec![(338, Field::Shorts(vec![2]))],
        &[vec![20, 128]],
        true,
        false,
    );
    assert_eq!(decode(&data).unwrap().pixels, [235, 235, 235, 128]);
}
#[test]
fn gray_bit_order_and_native_sixteen_bit_rounding() {
    let data = fixture(
        3,
        1,
        1,
        1,
        1,
        vec![short(266, 2)],
        &[vec![0b00000101]],
        true,
        false,
    );
    assert_eq!(gray_values(&decode(&data).unwrap()), [255, 0, 255]);
    // RGB rounds through (value+128)/257; gray instead uses the high byte.
    let data = fixture(
        1,
        1,
        16,
        3,
        2,
        vec![],
        &[vec![0, 1, 255, 0, 128, 128]],
        true,
        false,
    );
    assert_eq!(decode(&data).unwrap().pixels, [128, 1, 1, 255]);
    let data = fixture(1, 1, 16, 1, 1, vec![], &[vec![255, 0]], true, false);
    assert_eq!(gray_values(&decode(&data).unwrap()), [0]);
}
fn ycbcr_clumps(width: usize, height: usize, h: usize, v: usize, values: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for cy in 0..height.div_ceil(v) {
        for cx in 0..width.div_ceil(h) {
            for yy in 0..v {
                for xx in 0..h {
                    let x = cx * h + xx;
                    let y = cy * v + yy;
                    output.push(if x < width && y < height {
                        values[y * width + x]
                    } else {
                        0
                    });
                }
            }
            output.extend([128, 128]);
        }
    }
    output
}
#[test]
fn ycbcr_native_subsampling_table_and_partial_clumps() {
    let values = [
        10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150,
    ];
    for (h, v) in [(1, 1), (1, 2), (2, 1), (2, 2), (4, 1), (4, 2), (4, 4)] {
        let source = ycbcr_clumps(5, 3, h, v, &values);
        let data = fixture(
            5,
            3,
            8,
            3,
            6,
            vec![(530, Field::Shorts(vec![h as u16, v as u16]))],
            &[source],
            true,
            false,
        );
        assert_eq!(gray_values(&decode(&data).unwrap()), values, "{h}x{v}");
    }
}
#[test]
fn ycbcr_packbits_strips_and_tiled_edge_padding() {
    let values = [10, 20, 30, 40, 50, 60, 70, 80];
    let mut chunks = Vec::new();
    for row in values.chunks(4) {
        let clump = ycbcr_clumps(2, 2, 2, 2, row);
        let mut encoded = vec![(clump.len() - 1) as u8];
        encoded.extend(clump);
        chunks.push(encoded);
    }
    let data = fixture(
        2,
        4,
        8,
        3,
        6,
        vec![
            short(259, 32773),
            long(278, 2),
            (530, Field::Shorts(vec![2, 2])),
        ],
        &chunks,
        true,
        false,
    );
    assert_eq!(gray_values(&decode(&data).unwrap()), values);
    let values = [10, 20, 30, 40, 50, 60, 70, 80, 90];
    let tiles = [
        vec![10, 20, 40, 50],
        vec![30, 0, 60, 0],
        vec![70, 80, 0, 0],
        vec![90, 0, 0, 0],
    ];
    let chunks = tiles
        .iter()
        .map(|values| ycbcr_clumps(2, 2, 2, 2, values))
        .collect::<Vec<_>>();
    let data = fixture(
        3,
        3,
        8,
        3,
        6,
        vec![long(322, 2), long(323, 2), (530, Field::Shorts(vec![2, 2]))],
        &chunks,
        true,
        true,
    );
    assert_eq!(gray_values(&decode(&data).unwrap()), values);
}
#[test]
fn ycbcr_reference_ranges_and_invalid_conversion_metadata() {
    let data = fixture(
        1,
        1,
        8,
        3,
        6,
        vec![
            (530, Field::Shorts(vec![1, 1])),
            (
                532,
                Field::Rationals(vec![
                    (16, 1),
                    (235, 1),
                    (128, 1),
                    (240, 1),
                    (128, 1),
                    (240, 1),
                ]),
            ),
        ],
        &[vec![235, 128, 128]],
        true,
        false,
    );
    assert_eq!(decode(&data).unwrap().pixels, [255; 4]);
    let data = fixture(
        1,
        1,
        8,
        3,
        6,
        vec![(
            529,
            Field::Rationals(vec![(299, 1000), (0, 1), (114, 1000)]),
        )],
        &[vec![0; 6]],
        true,
        false,
    );
    assert!(matches!(decode(&data), Err(CodecError::InvalidData)));
}
#[test]
fn cielab_native_display_white_black_and_signed_chroma() {
    for bits in [8, 16] {
        let source = if bits == 8 {
            vec![0, 0, 0, 255, 0, 0, 128, 30, 226]
        } else {
            vec![
                0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 0, 0, 128, 128, 0, 30, 0, 226,
            ]
        };
        let data = fixture(3, 1, bits, 3, 8, vec![], &[source], true, false);
        let image = decode(&data).unwrap();
        assert_eq!(&image.pixels[..4], [0, 0, 0, 255]);
        assert!(image.pixels[4] > 215 && image.pixels[4] < 230);
        assert!(image.pixels[5..7].iter().all(|v| *v > 245));
        assert_ne!(&image.pixels[8..11], [128; 3]);
        assert_eq!(image.pixels[11], 255);
    }
}
#[test]
fn native_orientation_is_requested_without_reorienting_stored_samples() {
    for orientation in 1..=8 {
        let data = fixture(
            2,
            2,
            8,
            1,
            1,
            vec![short(274, orientation)],
            &[vec![10, 20, 30, 40]],
            true,
            false,
        );
        assert_eq!(gray_values(&decode(&data).unwrap()), [10, 20, 30, 40]);
    }
}
#[test]
fn native_next_literals_spans_runs_and_truncation() {
    // literal row, literal byte span over the initial white row, constant runs
    let data = fixture(
        8,
        3,
        2,
        1,
        1,
        vec![short(259, 32766)],
        &[vec![0, 0x1b, 0xe4, 0x40, 0, 1, 0, 1, 0, 0x42, 0x82, 0xc4]],
        true,
        false,
    );
    assert_eq!(
        gray_values(&decode(&data).unwrap()),
        [
            0, 85, 170, 255, 255, 170, 85, 0, 255, 255, 255, 255, 0, 0, 0, 0, 85, 85, 170, 170,
            255, 255, 255, 255
        ]
    );
    let data = fixture(
        8,
        1,
        2,
        1,
        1,
        vec![short(259, 32766)],
        &[vec![0, 0x1b]],
        true,
        false,
    );
    assert!(decode(&data).is_err());
}
#[test]
fn native_thunder_runs_raw_and_both_delta_widths() {
    // 3, 3, 3; then +1, skip, -1; then +2, -2 => 3,3,3,4,3,5,3
    let data = fixture(
        7,
        1,
        4,
        1,
        1,
        vec![short(259, 32809)],
        &[vec![0xc3, 2, 0x5b, 0x96]],
        true,
        false,
    );
    assert_eq!(
        gray_values(&decode(&data).unwrap()),
        [51, 51, 51, 68, 51, 85, 51]
    );
    let data = fixture(
        2,
        1,
        4,
        1,
        1,
        vec![short(259, 32809)],
        &[vec![0xc3, 3]],
        true,
        false,
    );
    assert!(decode(&data).is_err());
}
fn bits(text: &str) -> Vec<u8> {
    let mut output = Vec::new();
    for part in text.as_bytes().chunks(8) {
        let mut byte = 0;
        for (index, &bit) in part.iter().enumerate() {
            if bit == b'1' {
                byte |= 128 >> index;
            }
        }
        output.push(byte);
    }
    output
}
#[test]
fn ccitt_rle_group3_group4_and_fill_order() {
    // White run(8) = 10011. Group3 adds its EOL, Group4 all-white row
    // uses vertical(0) = 1 against the imaginary all-white reference line.
    for (compression, encoded) in [
        (2, bits("10011")),
        (3, bits("00000000000110011")),
        (4, bits("1")),
    ] {
        for fill in [1, 2] {
            let bytes = if fill == 2 {
                encoded.iter().map(|b| b.reverse_bits()).collect()
            } else {
                encoded.clone()
            };
            let data = fixture(
                8,
                1,
                1,
                1,
                0,
                vec![short(259, compression), short(266, fill)],
                &[bytes],
                true,
                false,
            );
            assert_eq!(gray_values(&decode(&data).unwrap()), [255; 8]);
        }
    }
}
#[test]
fn malformed_palette_dimensions_tags_and_unknown_codecs_are_errors() {
    for (photo, depth, compression) in [(2, 8, 1), (8, 8, 1), (32845, 16, 34676)] {
        let data = fixture(
            1,
            1,
            depth,
            3,
            photo,
            vec![short(259, compression), short(338, 1)],
            &[vec![0; 12]],
            true,
            false,
        );
        assert!(matches!(decode(&data), Err(CodecError::Unsupported)));
    }

    let data = fixture(
        1,
        1,
        8,
        1,
        3,
        vec![(320, Field::Shorts(vec![0; 3]))],
        &[vec![0]],
        true,
        false,
    );
    assert!(decode(&data).is_err());
    let data = fixture(
        1,
        1,
        8,
        3,
        6,
        vec![(530, Field::Shorts(vec![0, 2]))],
        &[vec![0; 6]],
        true,
        false,
    );
    assert!(decode(&data).is_err());
    let data = fixture(
        1,
        1,
        8,
        1,
        1,
        vec![short(259, 65000)],
        &[vec![0]],
        true,
        false,
    );
    assert!(matches!(decode(&data), Err(CodecError::Unsupported)));
    for end in [0, 2, 4, 8, 16, data.len() / 2] {
        assert!(decode(&data[..end]).is_err());
    }
}
#[test]
fn compressed_fill_order_and_word_aligned_ccitt_rle() {
    let encoded = [1u8, 17, 238];
    let data = fixture(
        2,
        1,
        8,
        1,
        1,
        vec![short(259, 32773), short(266, 2)],
        &[encoded.iter().map(|b| b.reverse_bits()).collect()],
        true,
        false,
    );
    assert_eq!(gray_values(&decode(&data).unwrap()), [17, 238]);
    // Each 5-bit white row ends at a 16-bit boundary in CCITTRLEW.
    let data = fixture(
        8,
        2,
        1,
        1,
        0,
        vec![short(259, 32771)],
        &[vec![0x98, 0, 0x98, 0]],
        true,
        false,
    );
    assert_eq!(gray_values(&decode(&data).unwrap()), [255; 16]);
}
#[test]
fn sgi_logarithmic_gray_and_both_color_storage_types() {
    let data = fixture(
        3,
        1,
        16,
        1,
        32844,
        vec![short(259, 34676)],
        &[vec![3, 0, 0x3f, 0x40, 3, 0, 0, 0]],
        true,
        false,
    );
    assert_eq!(gray_values(&decode(&data).unwrap()), [0, 181, 255]);
    let data = fixture(
        2,
        1,
        16,
        3,
        32845,
        vec![short(259, 34676)],
        &[vec![2, 0, 0x40, 2, 0, 0, 2, 0, 86, 2, 0, 194]],
        false,
        true,
    );
    let output = decode(&data).unwrap();
    assert_eq!(&output.pixels[..4], [0, 0, 0, 255]);
    assert!(output.pixels[4..7].iter().all(|v| *v > 250));
    let data = fixture(
        1,
        1,
        16,
        3,
        32845,
        vec![short(259, 34677)],
        &[vec![0, 0, 0]],
        true,
        false,
    );
    assert_eq!(decode(&data).unwrap().pixels, [0, 0, 0, 255]);
}
#[test]
fn pixar_logarithmic_predictor_uses_existing_rust_inflater() {
    let words = [0u16, 1000, 1250, 1250, 0u16.wrapping_sub(1000), 0];
    for little in [true, false] {
        let encoded = words
            .iter()
            .flat_map(|v| {
                if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                }
            })
            .collect::<Vec<_>>();
        let mut compressed = vec![0; 1024];
        let (encoded, status) =
            zlib_rs::compress_slice(&mut compressed, &encoded, zlib_rs::DeflateConfig::default());
        assert_eq!(status, zlib_rs::ReturnCode::Ok);
        for depth in [8, 16] {
            let data = fixture(
                2,
                1,
                depth,
                3,
                2,
                vec![short(259, 32909)],
                &[encoded.to_vec()],
                little,
                false,
            );
            assert_eq!(
                decode(&data).unwrap().pixels,
                [255, 94, 0, 255, 255, 0, 255, 255]
            );
        }
    }
}

#[test]
fn omitted_photometric_uses_color_channels_excluding_extra_samples() {
    for (samples, photo, values, expected) in [
        (4, 2, vec![60, 40, 20, 128], vec![20, 40, 60, 128]),
        (2, 1, vec![60, 128], vec![60, 60, 60, 128]),
    ] {
        let mut data = fixture(
            1,
            1,
            8,
            samples,
            photo,
            vec![(338, Field::Shorts(vec![1]))],
            &[values],
            true,
            false,
        );
        let count = u16::from_le_bytes(data[8..10].try_into().unwrap()) as usize;
        let entry = data[10..10 + count * 12]
            .chunks_exact_mut(12)
            .find(|e| u16::from_le_bytes(e[..2].try_into().unwrap()) == 262)
            .unwrap();
        entry[..2].copy_from_slice(&65000u16.to_le_bytes());
        assert_eq!(decode(&data).unwrap().pixels, expected);
    }
}
#[test]
fn frame_selection_retains_original_chunk_offsets_and_rejects_directory_cycles() {
    let mut first = fixture(1, 1, 8, 1, 1, vec![], &[vec![11]], true, false);
    let mut second = fixture(2, 1, 8, 1, 1, vec![], &[vec![22, 33]], true, false);
    let first_count = u16::from_le_bytes(first[8..10].try_into().unwrap()) as usize;
    let second_count = u16::from_le_bytes(second[8..10].try_into().unwrap()) as usize;
    let shift = first.len() as u32 - 8;
    for entry in second[10..10 + second_count * 12].chunks_exact_mut(12) {
        if u16::from_le_bytes(entry[..2].try_into().unwrap()) == 273 {
            let offset = u32::from_le_bytes(entry[8..12].try_into().unwrap());
            entry[8..12].copy_from_slice(&(offset + shift).to_le_bytes());
        }
    }
    let next = 10 + first_count * 12;
    let second_offset = first.len() as u32;
    first[next..next + 4].copy_from_slice(&second_offset.to_le_bytes());
    first.extend(&second[8..]);
    assert_eq!(
        (
            read_frame_header(&first, 1).unwrap().width,
            read_frame_header(&first, 1).unwrap().height
        ),
        (2, 1)
    );
    assert_eq!(gray_values(&decode_frame(&first, 0).unwrap()), [11]);
    assert_eq!(gray_values(&decode_frame(&first, 1).unwrap()), [22, 33]);
    assert!(decode_frame(&first, 2).is_err());
    first[next..next + 4].copy_from_slice(&8u32.to_le_bytes());
    assert!(read_frame_header(&first, 1).is_err());
}
