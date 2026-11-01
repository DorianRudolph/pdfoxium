// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Safe packed-output adapters for PDFium's fax and JBIG2 decoder boundaries.
//! Codec algorithms use the planned safe hayro codecs with native scanline
//! and segment compatibility extensions.

use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::FxdibFormat;

pub struct FaxBitmap {
    pub bitmap: CfxDibitmap,
    pub rows: i32,
}

struct PackedSink<'a> {
    bitmap: &'a mut CfxDibitmap,
    x: usize,
    y: i32,
    white_is_one: bool,
    invalid: bool,
}

impl<'a> PackedSink<'a> {
    fn new(bitmap: &'a mut CfxDibitmap, white_is_one: bool) -> Self {
        Self::with_background(bitmap, white_is_one, true)
    }

    fn with_background(bitmap: &'a mut CfxDibitmap, white_is_one: bool, white: bool) -> Self {
        bitmap
            .buffer_mut()
            .fill(if white == white_is_one { 255 } else { 0 });
        Self {
            bitmap,
            x: 0,
            y: 0,
            white_is_one,
            invalid: false,
        }
    }

    fn push(&mut self, white: bool, count: usize) {
        let Some(end) = self.x.checked_add(count) else {
            self.invalid = true;
            return;
        };
        if end > self.bitmap.width() as usize || self.y >= self.bitmap.height() {
            self.invalid = true;
            return;
        }
        let row = self.bitmap.scanline_mut(self.y).unwrap();
        let value = white == self.white_is_one;
        let mut x = self.x;
        while x < end && x % 8 != 0 {
            let bit = 0x80 >> (x % 8);
            if value {
                row[x / 8] |= bit;
            } else {
                row[x / 8] &= !bit;
            }
            x += 1;
        }
        let bytes_end = end / 8;
        row[x / 8..bytes_end.max(x / 8)].fill(if value { 255 } else { 0 });
        x = x.max(bytes_end * 8);
        while x < end {
            let bit = 0x80 >> (x % 8);
            if value {
                row[x / 8] |= bit;
            } else {
                row[x / 8] &= !bit;
            }
            x += 1;
        }
        self.x = end;
    }

    fn next_line(&mut self) {
        self.invalid |= self.x != self.bitmap.width() as usize;
        self.x = 0;
        self.y += 1;
    }
}

impl hayro_jbig2::Decoder for PackedSink<'_> {
    fn push_packed_row(&mut self, words: &[u32], width: u32) -> bool {
        if self.x != 0
            || width != self.bitmap.width() as u32
            || self.y >= self.bitmap.height()
            || words.len() < (width as usize).div_ceil(32)
        {
            self.invalid = true;
            return true;
        }
        let row = self.bitmap.scanline_mut(self.y).unwrap();
        let invert = if self.white_is_one { u32::MAX } else { 0 };
        let full_bytes = width as usize / 8;
        for (dest, &word) in row[..full_bytes].chunks_mut(4).zip(words) {
            dest.copy_from_slice(&(word ^ invert).to_be_bytes()[..dest.len()]);
        }
        if width % 8 != 0 {
            let byte = (words[full_bytes / 4] ^ invert).to_be_bytes()[full_bytes % 4];
            let mask = 0xffu8 << (8 - width % 8);
            row[full_bytes] = (row[full_bytes] & !mask) | (byte & mask);
        }
        self.x = width as usize;
        true
    }

    fn push_pixel(&mut self, black: bool) {
        self.push(!black, 1);
    }
    fn push_pixel_chunk(&mut self, black: bool, count: u32) {
        if let Some(count) = (count as usize).checked_mul(8) {
            self.push(!black, count);
        } else {
            self.invalid = true;
        }
    }
    fn next_line(&mut self) {
        self.next_line();
    }
}

/// CreateFaxDecoder/FaxModule::CreateDecoder parameter selection. PDFium uses
/// Columns/Rows for validation and decoder pitch, and the DIB requests its own
/// number of rows. EndOfBlock is not a parameter of the native fax decoder.
pub fn decode_fax(
    data: &[u8],
    width: i32,
    height: i32,
    params: Option<&CpdfDictionary>,
) -> Option<FaxBitmap> {
    use crate::fxcodec::scanlinedecoder::ScanlineDecoder;
    let mut decoder = crate::fpdfapi::parser::fpdf_parser_decode::create_fax_decoder(
        data, width, height, params,
    )?;
    let actual_width = decoder.get_width();
    if crate::fxge::calculate_pitch::calculate_pitch8(1, 1, actual_width)?
        < crate::fxge::calculate_pitch::calculate_pitch8(1, 1, width)?
    {
        return None;
    }
    let mut bitmap = CfxDibitmap::create(actual_width, height, FxdibFormat::OneBppRgb, 0)?;
    let mut rows = 0;
    // DIB requests its own row count, independently of the fax Rows metadata.
    for row in 0..height {
        let line = decoder.get_scanline(row);
        if line.is_empty() {
            break;
        }
        bitmap.scanline_mut(row)?.copy_from_slice(line);
        rows += 1;
    }
    Some(FaxBitmap { bitmap, rows })
}

/// Decode into the target allocated by CPDF_DIB::CreateDecoder. Global data
/// is filtered and retained by the DIB's continuation state. The codec runs
/// synchronously between continuation boundaries and writes packed pixels.
pub fn decode_jbig2_into(
    data: &[u8],
    bitmap: &mut CfxDibitmap,
    globals_data: Option<&[u8]>,
) -> bool {
    decode_jbig2_with_context(
        data,
        bitmap,
        globals_data,
        &mut hayro_jbig2::PdfiumDocumentContext::default(),
        0,
    )
}

pub fn decode_jbig2_with_context(
    data: &[u8],
    bitmap: &mut CfxDibitmap,
    globals_data: Option<&[u8]>,
    context: &mut hayro_jbig2::PdfiumDocumentContext,
    global_key: u64,
) -> bool {
    let Ok(image) = hayro_jbig2::Image::new_embedded_with_canvas(
        data,
        globals_data,
        bitmap.width() as u32,
        bitmap.height() as u32,
    ) else {
        return false;
    };
    let height = bitmap.height();
    let (result, valid) = {
        let mut sink = PackedSink::new(bitmap, true);
        let result = image.decode_with_document_context(&mut sink, context, global_key);
        (result, !sink.invalid && sink.y == height && sink.x == 0)
    };
    let Ok(default_pixel) = result else {
        return false;
    };
    if !valid {
        return false;
    }
    // Native PageInfo fills the entire external buffer before composition and
    // inversion, including non-pixel bits. The codec emits only visible pixels.
    let width = bitmap.width() as usize;
    let used = width.div_ceil(8);
    let background = if default_pixel { 0 } else { 255 };
    for row in 0..height {
        let row = bitmap.scanline_mut(row).expect("target row");
        if width % 8 != 0 {
            let mask = 0xffu8 >> (width % 8);
            row[used - 1] = (row[used - 1] & !mask) | (background & mask);
        }
        row[used..].fill(background);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jbig2_packed_rows_match_pixel_output_and_preserve_padding() {
        use hayro_jbig2::Decoder;
        for width in 1usize..=97 {
            for white_is_one in [false, true] {
                for extra_pitch in [0, 8] {
                    let pitch = width.div_ceil(32) * 4 + extra_pitch;
                    let make = || {
                        CfxDibitmap::create(width as i32, 3, FxdibFormat::OneBppRgb, pitch as u32)
                            .unwrap()
                    };
                    let mut actual = make();
                    let mut expected = make();
                    let mut packed = PackedSink::new(&mut actual, white_is_one);
                    let mut pixels = PackedSink::new(&mut expected, white_is_one);
                    for y in 0..3 {
                        let words: Vec<u32> = (0..width.div_ceil(32))
                            .map(|i| [0x0123_4567, 0xffff_ffff, 0, 0xa5c3_7e81][(i + y) % 4])
                            .collect();
                        assert!(packed.push_packed_row(&words, width as u32));
                        for x in 0..width {
                            pixels.push_pixel(words[x / 32] & (1 << (31 - x % 32)) != 0);
                        }
                        packed.next_line();
                        pixels.next_line();
                    }
                    assert!(!packed.invalid && !pixels.invalid);
                    assert_eq!(actual.buffer(), expected.buffer(), "width={width}");
                }
            }
        }
    }

    fn page_only(width: u32, height: u32, black: bool) -> Vec<u8> {
        let mut data = Vec::new();
        // Segment 1: page information, no references, one-byte association.
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&[48, 0, 1]);
        data.extend_from_slice(&19u32.to_be_bytes());
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&[0; 8]); // Physical resolution.
        data.push(if black { 4 } else { 0 });
        data.extend_from_slice(&[0; 2]); // Striping information.
                                         // Segment 2: end of page.
        data.extend_from_slice(&2u32.to_be_bytes());
        data.extend_from_slice(&[49, 0, 1, 0, 0, 0, 0]);
        data
    }

    #[test]
    fn jbig2_external_canvas_uses_pdf_dimensions_default_pixels_and_padding() {
        for (stream_width, stream_height) in [(1, 1), (100, 100), (0, 0), (7, u32::MAX)] {
            for black in [false, true] {
                let data = page_only(stream_width, stream_height, black);
                let mut bitmap = CfxDibitmap::create(9, 3, FxdibFormat::OneBppRgb, 0).unwrap();
                assert!(decode_jbig2_into(&data, &mut bitmap, None));
                assert!(bitmap
                    .buffer()
                    .iter()
                    .all(|&byte| byte == if black { 0 } else { 255 }));
            }
        }
        // External buffers use native total-pixel/stride bounds, independently
        // of JBIG2's ordinary 65535-pixel region dimension limit.
        let mut wide = CfxDibitmap::create(65536, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        assert!(decode_jbig2_into(&page_only(1, 1, false), &mut wide, None));
        assert!(wide.buffer().iter().all(|&byte| byte == 255));
    }

    #[test]
    fn jbig2_regions_can_extend_beyond_declared_page_info() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        assert_eq!(&data[..7], &[0, 0, 0, 1, 48, 0, 1]);
        let image = hayro_jbig2::Image::new_embedded(data, Some(globals)).unwrap();
        let mut expected = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        let mut actual = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        assert!(decode_jbig2_into(data, &mut expected, Some(globals)));
        let mut changed = data.to_vec();
        changed[11..15].copy_from_slice(&1u32.to_be_bytes());
        changed[15..19].copy_from_slice(&1u32.to_be_bytes());
        assert!(decode_jbig2_into(&changed, &mut actual, Some(globals)));
        assert!(expected
            .buffer()
            .iter()
            .zip(actual.buffer())
            .all(|(a, b)| a == b));
    }

    #[test]
    fn jbig2_document_cache_reuses_globals_and_preserves_two_entry_order() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let image = hayro_jbig2::Image::new_embedded(data, Some(globals)).unwrap();
        let mut bitmap = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        let mut expected = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        assert!(decode_jbig2_into(data, &mut expected, Some(globals)));
        let mut context = hayro_jbig2::PdfiumDocumentContext::default();
        for key in [10, 20, 10, 30] {
            assert!(decode_jbig2_with_context(
                data,
                &mut bitmap,
                Some(globals),
                &mut context,
                key
            ));
            assert!(bitmap
                .buffer()
                .iter()
                .zip(expected.buffer())
                .all(|(a, b)| a == b));
        }
        assert_eq!(context.symbol_dictionary_count(), 2);
        assert_eq!(
            context.symbol_dictionary_keys().collect::<Vec<_>>(),
            [(30, 11), (10, 11)]
        );
        // A zero stream key is inserted but deliberately never hits.
        for _ in 0..2 {
            assert!(decode_jbig2_with_context(
                data,
                &mut bitmap,
                Some(globals),
                &mut context,
                0
            ));
        }
        assert_eq!(
            context.symbol_dictionary_keys().collect::<Vec<_>>(),
            [(0, 11), (0, 11)]
        );
    }

    #[test]
    fn jbig2_first_page_end_and_short_trailer_follow_native_framing() {
        let mut bitmap = CfxDibitmap::create(9, 2, FxdibFormat::OneBppRgb, 0).unwrap();
        let mut page = page_only(1, 1, false);
        // EndOfPage returns EndReached before reading a declared payload.
        let size = page.len();
        page[size - 4..].copy_from_slice(&u32::MAX.to_be_bytes());
        page.extend_from_slice(&[0xff; 20]);
        assert!(decode_jbig2_into(&page, &mut bitmap, None));
        assert!(bitmap.buffer().iter().all(|&byte| byte == 255));
        // DecodeSequential stops when fewer than 11 header bytes remain.
        let mut page = page_only(1, 1, true);
        page.truncate(30);
        page.extend_from_slice(&[0xa5; 10]);
        assert!(decode_jbig2_into(&page, &mut bitmap, None));
        assert!(bitmap.buffer().iter().all(|&byte| byte == 0));
    }

    #[test]
    fn jbig2_ignores_native_reserved_and_unused_region_symbol_flags() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let image = hayro_jbig2::Image::new_embedded(data, Some(globals)).unwrap();
        let mut expected = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        let mut actual = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        assert!(decode_jbig2_into(data, &mut expected, Some(globals)));
        let mut page = data.to_vec();
        assert_eq!(page[58], 0);
        page[58] = 0xf0; // Reserved outer-region bits are ignored.
        let mut globals = globals.to_vec();
        assert_eq!(&globals[11..13], &[0, 0]);
        // Unused Huffman selectors in an arithmetic dictionary are ignored.
        globals[12] |= 0xe8;
        globals[11] |= 0x20; // Refinement template bit 13 is also accepted.
        assert!(decode_jbig2_into(&page, &mut actual, Some(&globals)));
        assert!(actual
            .buffer()
            .iter()
            .zip(expected.buffer())
            .all(|(a, b)| a == b));
    }

    #[test]
    fn jbig2_no_page_info_unknown_segments_and_repeated_page_fill() {
        let mut bitmap = CfxDibitmap::create(9, 2, FxdibFormat::OneBppRgb, 0).unwrap();
        assert!(decode_jbig2_into(&[1], &mut bitmap, None));
        assert!(bitmap.buffer().iter().all(|&byte| byte == 255));
        let unknown = [0, 0, 0, 1, 60, 0, 1, 0, 0, 0, 20];
        // An unparsed segment's length is clamped to the available bytes.
        assert!(decode_jbig2_into(&unknown, &mut bitmap, None));
        assert!(bitmap.buffer().iter().all(|&byte| byte == 255));
        let mut first = page_only(1, 1, false);
        first.truncate(30);
        first[..4].copy_from_slice(&3u32.to_be_bytes());
        let mut second = page_only(1, 1, true);
        second[..4].copy_from_slice(&2u32.to_be_bytes());
        first.extend_from_slice(&second);
        assert!(decode_jbig2_into(&first, &mut bitmap, None));
        assert!(bitmap.buffer().iter().all(|&byte| byte == 0));
        // A global page and its terminator do not fill/terminate the PDF page.
        assert!(decode_jbig2_into(
            &second,
            &mut bitmap,
            Some(&page_only(1, 1, false))
        ));
        assert!(bitmap.buffer().iter().all(|&byte| byte == 0));
        for (width, height) in [(0, 0), (u32::MAX, 1), (1, u32::MAX)] {
            let globals = page_only(width, height, false);
            // Construction retains an empty owned image, but the native
            // PageInfo dispatch rejects it before accepting globals.
            assert!(!decode_jbig2_into(&second, &mut bitmap, Some(&globals)));
        }
    }

    #[test]
    fn jbig2_refinement_uses_local_references_and_native_page_subimages() {
        fn refinement(number: u32, kind: u8, width: u32, x: i32, references: &[u8]) -> Vec<u8> {
            let mut payload = Vec::new();
            payload.extend_from_slice(&width.to_be_bytes());
            payload.extend_from_slice(&2u32.to_be_bytes());
            payload.extend_from_slice(&x.to_be_bytes());
            payload.extend_from_slice(&0i32.to_be_bytes());
            payload.extend_from_slice(&[4, 1]); // Replace; refinement template 1.
            payload.extend_from_slice(&[0x55; 64]);
            let mut segment = number.to_be_bytes().to_vec();
            segment.extend_from_slice(&[kind, (references.len() as u8) << 5]);
            segment.extend_from_slice(references);
            segment.push(1);
            segment.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            segment.extend_from_slice(&payload);
            segment
        }
        let mut page = page_only(9, 2, true);
        page.truncate(30);
        // A wider region with an out-of-page origin gets a white SubImage.
        // Its stored page position is ignored when it is referenced later.
        page.extend_from_slice(&refinement(2, 40, 12, -1, &[]));
        let mut bitmap = CfxDibitmap::create(9, 2, FxdibFormat::OneBppRgb, 0).unwrap();
        assert!(decode_jbig2_into(&page, &mut bitmap, None));
        let prefix_len = page.len();
        // C++ repeatedly checks the first reference, even if a later one is
        // missing. Preserve that source behavior rather than searching them.
        page.extend_from_slice(&refinement(100, 42, 9, 0, &[2, 99]));
        assert!(decode_jbig2_into(&page, &mut bitmap, None));
        page.truncate(prefix_len);
        page.extend_from_slice(&refinement(100, 42, 9, 0, &[1, 2]));
        assert!(!decode_jbig2_into(&page, &mut bitmap, None));
        page.truncate(prefix_len);
        page.extend_from_slice(&refinement(100, 42, 9, 0, &[99]));
        assert!(!decode_jbig2_into(&page, &mut bitmap, None));
    }

    #[test]
    fn jbig2_globals_are_cached_before_a_later_page_header_error() {
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let mut context = hayro_jbig2::PdfiumDocumentContext::default();
        let mut bitmap = CfxDibitmap::create(9, 2, FxdibFormat::OneBppRgb, 0).unwrap();
        assert!(!decode_jbig2_with_context(
            &[0xff; 11],
            &mut bitmap,
            Some(globals),
            &mut context,
            77
        ));
        assert_eq!(
            context.symbol_dictionary_keys().collect::<Vec<_>>(),
            [(77, 11)]
        );
    }

    #[test]
    fn jbig2_generic_arithmetic_and_mmr_use_distinct_native_dimension_checks() {
        for (width, mmr, encoded, succeeds) in [
            (0u32, false, 255, false),
            (65536, false, 255, false),
            (65536, true, 128, true),
            (9, true, 0, true), // Partial EOFB/invalid data still gives a row.
        ] {
            let mut payload = Vec::new();
            payload.extend_from_slice(&width.to_be_bytes());
            payload.extend_from_slice(&1u32.to_be_bytes());
            payload.extend_from_slice(&[0; 8]);
            payload.extend_from_slice(&[4, if mmr { 1 } else { 6 }]);
            if !mmr {
                payload.extend_from_slice(&[2, 255]);
            }
            payload.push(encoded);
            let mut page = page_only(9, 1, true);
            page.truncate(30);
            page.extend_from_slice(&2u32.to_be_bytes());
            page.extend_from_slice(&[38, 0, 1]);
            page.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            page.extend_from_slice(&payload);
            let mut bitmap = CfxDibitmap::create(9, 1, FxdibFormat::OneBppRgb, 0).unwrap();
            assert_eq!(
                decode_jbig2_into(&page, &mut bitmap, None),
                succeeds,
                "width {width}, mmr {mmr}"
            );
            if succeeds {
                assert_eq!(bitmap.buffer()[0], 255);
            }
        }
    }

    fn unknown_generic(mmr: bool, height: u32, encoded: &[u8], footer: Option<u32>) -> Vec<u8> {
        let mut bytes = 2u32.to_be_bytes().to_vec();
        bytes.extend_from_slice(&[38, 0, 1]);
        bytes.extend_from_slice(&u32::MAX.to_be_bytes());
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&[4, if mmr { 1 } else { 6 }]);
        if !mmr {
            bytes.extend_from_slice(&[2, 255]);
        }
        bytes.extend_from_slice(encoded);
        if let Some(footer) = footer {
            bytes.extend_from_slice(&footer.to_be_bytes());
        }
        bytes
    }

    #[test]
    fn jbig2_unknown_generic_uses_decode_offsets_and_ignores_footer_row_counts() {
        for (mmr, encoded) in [(false, &[255, 172][..]), (true, &[128][..])] {
            let mut page = page_only(1, 2, false);
            page.truncate(30);
            page.extend_from_slice(&unknown_generic(
                mmr,
                if mmr { 1 } else { 2 },
                encoded,
                Some(u32::MAX),
            ));
            let mut final_page = page_only(1, 2, true);
            final_page[..4].copy_from_slice(&3u32.to_be_bytes());
            page.extend_from_slice(&final_page);
            page.extend_from_slice(&[255; 11]); // Native EndOfPage stops before this header.
            let mut bitmap = CfxDibitmap::create(1, 2, FxdibFormat::OneBppRgb, 0).unwrap();
            assert!(decode_jbig2_into(&page, &mut bitmap, None));
            assert!(bitmap.buffer().iter().all(|&value| value == 0));
        }
        let mut page = page_only(1, 2, false);
        page.truncate(30);
        // Native arithmetic can finish at its FF sentinel with no marker or
        // footer; align/addOffset clamp to the stream end instead of scanning.
        page.extend_from_slice(&unknown_generic(false, 2, &[255], None));
        let mut bitmap = CfxDibitmap::create(1, 2, FxdibFormat::OneBppRgb, 0).unwrap();
        assert!(decode_jbig2_into(&page, &mut bitmap, None));
    }

    #[test]
    fn jbig2_unknown_global_continuation_keeps_cache_offsets_and_local_stream() {
        let page = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let dictionary = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let image = hayro_jbig2::Image::new_embedded(page, Some(dictionary)).unwrap();
        let mut expected = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        let mut actual = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        assert!(decode_jbig2_into(page, &mut expected, Some(dictionary)));
        let mut globals = page_only(1, 2, false);
        globals.truncate(30);
        globals.extend_from_slice(&unknown_generic(false, 2, &[255, 172], Some(0)));
        let offset = globals.len() as u32 + 11;
        globals.extend_from_slice(dictionary);
        let mut context = hayro_jbig2::PdfiumDocumentContext::default();
        assert!(decode_jbig2_with_context(
            page,
            &mut actual,
            Some(&globals),
            &mut context,
            88
        ));
        assert_eq!(actual.buffer(), expected.buffer());
        assert_eq!(
            context.symbol_dictionary_keys().collect::<Vec<_>>(),
            [(88, offset)]
        );
        // An EndOfPage reached during deferred globals framing must discard
        // its tail and continue the separately retained local stream.
        globals.extend_from_slice(&page_only(1, 1, false)[30..]);
        globals.extend_from_slice(&[255; 11]);
        assert!(decode_jbig2_with_context(
            page,
            &mut actual,
            Some(&globals),
            &mut context,
            88
        ));
        assert_eq!(actual.buffer(), expected.buffer());
    }

    #[test]
    fn jbig2_unknown_page_text_table_and_refinement_resume_after_native_offsets() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let image = hayro_jbig2::Image::new_embedded(data, Some(globals)).unwrap();
        let mut expected = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        let mut actual = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        assert!(decode_jbig2_into(data, &mut expected, Some(globals)));
        let text_end = 42 + u32::from_be_bytes(data[38..42].try_into().unwrap()) as usize;
        for (field, end) in [(7usize, 30usize), (38usize, text_end)] {
            let mut page = data[..end].to_vec();
            page[field..field + 4].copy_from_slice(&u32::MAX.to_be_bytes());
            page.extend_from_slice(&[0; 4]);
            page.extend_from_slice(&data[end..]);
            assert!(
                decode_jbig2_into(&page, &mut actual, Some(globals)),
                "length field {field}"
            );
            assert_eq!(actual.buffer(), expected.buffer());
        }
        let mut refinement = Vec::new();
        refinement.extend_from_slice(&1u32.to_be_bytes());
        refinement.extend_from_slice(&1u32.to_be_bytes());
        refinement.extend_from_slice(&[0; 8]);
        refinement.extend_from_slice(&[4, 1, 255, 172]);
        let mut table = vec![0];
        table.extend_from_slice(&0i32.to_be_bytes());
        table.extend_from_slice(&1i32.to_be_bytes());
        table.push(0x80);
        for (kind, payload) in [(42, refinement), (53, table)] {
            let mut page = page_only(1, 1, false);
            page.truncate(30);
            page.extend_from_slice(&2u32.to_be_bytes());
            page.extend_from_slice(&[kind, 0, 1]);
            page.extend_from_slice(&u32::MAX.to_be_bytes());
            page.extend_from_slice(&payload);
            page.extend_from_slice(&[0; 4]);
            let mut fill = page_only(1, 1, true);
            fill[..4].copy_from_slice(&3u32.to_be_bytes());
            page.extend_from_slice(&fill);
            let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::OneBppRgb, 0).unwrap();
            assert!(
                decode_jbig2_into(&page, &mut bitmap, None),
                "segment {kind}"
            );
            assert!(bitmap.buffer().iter().all(|&value| value == 0));
        }
    }

    #[test]
    fn jbig2_unknown_symbol_cache_hits_do_not_consume_encoded_payload() {
        let mut globals = 0u32.to_be_bytes().to_vec();
        globals.extend_from_slice(&[0, 0, 0]);
        globals.extend_from_slice(&u32::MAX.to_be_bytes());
        globals.extend_from_slice(&[0, 0, 3, 255, 253, 255, 2, 254, 254, 254]);
        globals.extend_from_slice(&[0; 8]); // Zero exported and new symbols.
        globals.extend_from_slice(&[255, 172]);
        globals.extend_from_slice(&[0; 4]);
        globals.extend_from_slice(&1u32.to_be_bytes());
        globals.extend_from_slice(&[49, 0, 0, 0, 0, 0, 0]);
        let page = page_only(1, 1, false);
        let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        let mut context = hayro_jbig2::PdfiumDocumentContext::default();
        assert!(decode_jbig2_with_context(
            &page,
            &mut bitmap,
            Some(&globals),
            &mut context,
            99
        ));
        assert_eq!(
            context.symbol_dictionary_keys().collect::<Vec<_>>(),
            [(99, 11)]
        );
        // Source cache-hit flow skips no arithmetic bytes, so the generic
        // four-byte continuation now begins inside this fixture's terminator.
        // Its next header is malformed; the cached dictionary still survives.
        assert!(!decode_jbig2_with_context(
            &page,
            &mut bitmap,
            Some(&globals),
            &mut context,
            99
        ));
        assert_eq!(
            context.symbol_dictionary_keys().collect::<Vec<_>>(),
            [(99, 11)]
        );
    }

    #[test]
    fn jbig2_text_arithmetic_stops_at_native_instance_count() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        assert_eq!(data[60] & 1, 0);
        assert!(u32::from_be_bytes(data[61..65].try_into().unwrap()) > 1);
        let image = hayro_jbig2::Image::new_embedded(data, Some(globals)).unwrap();
        let mut bitmap = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        let mut page = data.to_vec();
        page[61..65].copy_from_slice(&1u32.to_be_bytes());
        // Native reads IADS once more, then stops before another symbol ID.
        // Remaining encoded instances are skipped by the declared segment length.
        assert!(decode_jbig2_into(&page, &mut bitmap, Some(globals)));
    }

    #[test]
    fn jbig2_huffman_text_refinement_checks_actual_marker_inclusive_size() {
        fn pack(bits: &[u8]) -> Vec<u8> {
            let mut out = vec![0; bits.len().div_ceil(8)];
            for (index, &bit) in bits.iter().enumerate() {
                out[index / 8] |= bit << (7 - index % 8);
            }
            out
        }
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        assert_eq!(u32::from_be_bytes(globals[21..25].try_into().unwrap()), 5);
        for (size, width_delta, succeeds) in [(2u8, 0u32, true), (3, 0, false), (2, 65536, true)] {
            let mut payload = Vec::new();
            payload.extend_from_slice(&64u32.to_be_bytes());
            payload.extend_from_slice(&64u32.to_be_bytes());
            payload.extend_from_slice(&[0; 8]);
            payload.push(4);
            payload.extend_from_slice(&0x8013u16.to_be_bytes()); // Huffman/refinement, top left, template 1.
            payload.extend_from_slice(&(if width_delta == 0 { 0u16 } else { 0x40 }).to_be_bytes()); // RDW table N/O.
            payload.extend_from_slice(&1u32.to_be_bytes());
            let mut codes = Vec::new();
            for index in 0..35 {
                codes.extend_from_slice(if index == 3 { &[0, 0, 0, 1] } else { &[0; 4] });
            }
            codes.extend_from_slice(&[0; 5]); // Five symbol-ID codes, each three bits long.
            payload.extend_from_slice(&pack(&codes));
            let mut bits = vec![0; 14]; // DT 1 twice, FS 0 (nine bits), symbol ID 0 (three bits).
            bits.push(1); // Refine.
            if width_delta == 0 {
                bits.push(0);
            } else {
                bits.extend_from_slice(&[1; 7]); // Table O upper range.
                let offset = width_delta - 25;
                for bit in (0..32).rev() {
                    bits.push(((offset >> bit) & 1) as u8);
                }
            }
            bits.extend_from_slice(&[0; 3]); // RDH/RDX/RDY zero.
            bits.extend_from_slice(&[0, 0, 0, (size >> 1) & 1, size & 1]); // Table A size.
            payload.extend_from_slice(&pack(&bits));
            payload.extend_from_slice(&[255, 172]);
            if size == 3 {
                payload.push(0);
            } // Bounded slicing alone would accept this extra byte.
            payload.push(0x40); // Table H end-of-strip OOB (01), padded to a byte.
            let mut page = page_only(64, 64, false);
            page.truncate(30);
            page.extend_from_slice(&2u32.to_be_bytes());
            page.extend_from_slice(&[6, 32, 0, 1]); // Text refers to global dictionary zero.
            page.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            page.extend_from_slice(&payload);
            let mut bitmap = CfxDibitmap::create(64, 64, FxdibFormat::OneBppRgb, 0).unwrap();
            assert_eq!(
                decode_jbig2_into(&page, &mut bitmap, Some(globals)),
                succeeds,
                "size {size}, width delta {width_delta}"
            );
            if width_delta != 0 {
                assert!(bitmap.buffer().iter().all(|&byte| byte == 255));
            }
        }
    }

    #[test]
    fn jbig2_declared_export_count_is_an_upper_bound_not_padding() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let image = hayro_jbig2::Image::new_embedded(data, Some(globals)).unwrap();
        let mut expected = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        let mut actual = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        assert!(decode_jbig2_into(data, &mut expected, Some(globals)));
        let mut globals = globals.to_vec();
        let count = u32::from_be_bytes(globals[21..25].try_into().unwrap());
        globals[21..25].copy_from_slice(&(count + 1).to_be_bytes());
        assert!(decode_jbig2_into(data, &mut actual, Some(&globals)));
        assert_eq!(actual.buffer(), expected.buffer());
        globals[21..25].copy_from_slice(&(count - 1).to_be_bytes());
        assert!(!decode_jbig2_into(data, &mut actual, Some(&globals)));
    }

    #[test]
    fn jbig2_native_region_allocation_checks_total_stride_before_allocating() {
        for kind in [6, 38, 42] {
            let mut payload = Vec::new();
            payload.extend_from_slice(&65535u32.to_be_bytes());
            payload.extend_from_slice(&65535u32.to_be_bytes());
            payload.extend_from_slice(&[0; 8]);
            payload.push(4);
            match kind {
                6 => {
                    payload.extend_from_slice(&[0; 6]);
                } // Arithmetic flags, zero instances.
                38 => payload.extend_from_slice(&[6, 2, 255]), // Arithmetic template 3.
                42 => payload.push(1),                         // Refinement template 1.
                _ => unreachable!(),
            }
            payload.extend_from_slice(&[255, 172]);
            let mut page = page_only(1, 1, false);
            page.truncate(30);
            page.extend_from_slice(&2u32.to_be_bytes());
            page.extend_from_slice(&[kind, 0, 1]);
            page.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            page.extend_from_slice(&payload);
            let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::OneBppRgb, 0).unwrap();
            // Both dimensions pass the ordinary 65535 bound, but their DWORD
            // stride product exceeds CJBig2_Image's native allocation limit.
            assert!(
                !decode_jbig2_into(&page, &mut bitmap, None),
                "segment {kind}"
            );
        }
    }

    #[test]
    fn jbig2_null_symbols_are_distinct_from_owned_empty_symbols_and_survive_cache_copy() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let original_globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let image = hayro_jbig2::Image::new_embedded(data, Some(original_globals)).unwrap();
        let mut page = data.to_vec();
        page[61..65].copy_from_slice(&1u32.to_be_bytes());
        for (flags, coded, succeeds) in [
            (1u16, &[0x3f, 0, 0, 0x40][..], true), // Raw zero-width crop: owned empty image.
            (1u16, &[0x3f, 8, 0, 0x40][..], false), // Empty MMR output: null slot.
            (0x1003u16, &[0x40, 0xfc, 1][..], false), // Refinement aggregation count zero: null slot.
        ] {
            let mut payload = flags.to_be_bytes().to_vec();
            payload.extend_from_slice(&1u32.to_be_bytes()); // Export one symbol.
            payload.extend_from_slice(&1u32.to_be_bytes()); // Decode one symbol.
            payload.extend_from_slice(coded);
            let mut globals = 0u32.to_be_bytes().to_vec();
            globals.extend_from_slice(&[0, 0, 0]);
            globals.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            globals.extend_from_slice(&payload);
            let mut bitmap = CfxDibitmap::create(
                image.width() as i32,
                image.height() as i32,
                FxdibFormat::OneBppRgb,
                0,
            )
            .unwrap();
            let mut cache = hayro_jbig2::PdfiumDocumentContext::default();
            assert_eq!(
                decode_jbig2_with_context(&page, &mut bitmap, Some(&globals), &mut cache, 77),
                succeeds,
                "flags {flags}, coded {coded:?}"
            );
            assert_eq!(
                cache.symbol_dictionary_keys().collect::<Vec<_>>(),
                [(77, 11)]
            );
            assert_eq!(
                decode_jbig2_with_context(&page, &mut bitmap, Some(&globals), &mut cache, 77),
                succeeds
            ); // Native DeepCopy preserves null versus empty.
            assert!(decode_jbig2_with_context(
                &page_only(1, 1, false),
                &mut bitmap,
                Some(&globals),
                &mut cache,
                77
            ));
        }
    }

    #[test]
    fn jbig2_progressive_generic_pattern_and_halftone_keep_partial_images() {
        for kind in [36, 38] {
            let mut payload = Vec::new();
            payload.extend_from_slice(&1u32.to_be_bytes());
            payload.extend_from_slice(&2u32.to_be_bytes());
            payload.extend_from_slice(&[0; 8]);
            payload.extend_from_slice(&[4, 6, 2, 255, 0]);
            let mut page = page_only(1, 2, true);
            page.truncate(30);
            page.extend_from_slice(&2u32.to_be_bytes());
            page.extend_from_slice(&[kind, 0, 1]);
            page.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            page.extend_from_slice(&payload);
            let mut bitmap = CfxDibitmap::create(1, 2, FxdibFormat::OneBppRgb, 0).unwrap();
            assert!(decode_jbig2_into(&page, &mut bitmap, None));
            // No complete rows: an immediate replace leaves the black page
            // unchanged; an intermediate image is retained without composition.
            assert!(bitmap.buffer().iter().all(|&byte| byte == 0));
        }
        let globals = [0, 0, 0, 2, 16, 0, 0, 0, 0, 0, 7, 0, 1, 1, 0, 0, 0, 0];
        let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        assert!(decode_jbig2_into(
            &page_only(1, 1, true),
            &mut bitmap,
            Some(&globals)
        ));
        let mut page = page_only(1, 1, true);
        page.truncate(30);
        let mut half = Vec::new();
        half.extend_from_slice(&1u32.to_be_bytes());
        half.extend_from_slice(&1u32.to_be_bytes());
        half.extend_from_slice(&[0; 8]);
        half.extend_from_slice(&[4, 0x40]);
        half.extend_from_slice(&1u32.to_be_bytes());
        half.extend_from_slice(&1u32.to_be_bytes());
        half.extend_from_slice(&[0; 12]); // Grid origin/vector; no arithmetic data.
        page.extend_from_slice(&3u32.to_be_bytes());
        page.extend_from_slice(&[22, 32, 2, 1]);
        page.extend_from_slice(&(half.len() as u32).to_be_bytes());
        page.extend_from_slice(&half);
        assert!(decode_jbig2_into(&page, &mut bitmap, Some(&globals)));
        assert_eq!(bitmap.buffer(), &[0x80, 0, 0, 0]);
    }

    #[test]
    fn jbig2_symbol_collective_width_uses_native_signed_narrowing() {
        fn bits(out: &mut Vec<u8>, value: u32, length: u32) {
            for shift in (0..length).rev() {
                out.push(((value >> shift) & 1) as u8);
            }
        }
        for large_class in [false, true] {
            // AGGINST is unused without REFAGG, even when its custom-table
            // selector is set. No referred table is needed for either case.
            let mut payload = (if large_class { 0x0085u16 } else { 0x0081u16 })
                .to_be_bytes()
                .to_vec();
            payload.extend_from_slice(&0u32.to_be_bytes());
            payload.extend_from_slice(&(if large_class { 32769u32 } else { 0 }).to_be_bytes());
            if large_class {
                let mut coded = Vec::new();
                bits(&mut coded, 0x7e, 7); // Table E: height delta -255 + 255 = 0.
                bits(&mut coded, 255, 8);
                bits(&mut coded, 0x3e, 6); // Table B: first width 75 + 65460 = 65535.
                bits(&mut coded, 65460, 32);
                for _ in 1..32769 {
                    bits(&mut coded, 0, 1); // Remaining width deltas zero.
                }
                bits(&mut coded, 0x3f, 6); // End height class.
                bits(&mut coded, 0, 5); // Raw collective bitmap, no rows.
                assert_eq!(coded.len() % 8, 0);
                bits(&mut coded, 6, 3); // Export false run 272 + 32497 = 32769.
                bits(&mut coded, 32497, 16);
                let mut bytes = vec![0; coded.len().div_ceil(8)];
                for (index, bit) in coded.into_iter().enumerate() {
                    bytes[index / 8] |= bit << (7 - index % 8);
                }
                payload.extend_from_slice(&bytes);
            }
            let mut globals = 0u32.to_be_bytes().to_vec();
            globals.extend_from_slice(&[0, 0, 0]);
            globals.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            globals.extend_from_slice(&payload);
            let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::OneBppRgb, 0).unwrap();
            // 32769 * 65535 narrows to a negative int in the native width
            // check. The zero-height collective image is an owned empty image.
            assert!(decode_jbig2_into(
                &page_only(1, 1, false),
                &mut bitmap,
                Some(&globals),
            ));
            assert_eq!(bitmap.buffer(), &[255; 4]);
        }
    }

    #[test]
    fn jbig2_text_checked_values_follow_native_deferred_and_unsigned_updates() {
        fn bits(out: &mut Vec<u8>, value: u32, length: u32) {
            for shift in (0..length).rev() {
                out.push(((value >> shift) & 1) as u8);
            }
        }
        fn packed(bits: &[u8]) -> Vec<u8> {
            let mut out = vec![0; bits.len().div_ceil(8)];
            for (index, &bit) in bits.iter().enumerate() {
                out[index / 8] |= bit << (7 - index % 8);
            }
            out
        }
        let stock_globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let mut empty_globals = 0u32.to_be_bytes().to_vec();
        empty_globals.extend_from_slice(&[0, 0, 0]);
        empty_globals.extend_from_slice(&14u32.to_be_bytes());
        empty_globals.extend_from_slice(&[0, 1]);
        empty_globals.extend_from_slice(&1u32.to_be_bytes());
        empty_globals.extend_from_slice(&1u32.to_be_bytes());
        empty_globals.extend_from_slice(&[0x3f, 0, 0, 0x40]);
        for case in 0..3 {
            let (flags, symbols, instances, globals) = match case {
                0 => (0x8013u16, 5u32, 1u32, Some(stock_globals.as_slice())),
                1 => (0x0011u16, 1u32, 1u32, Some(empty_globals.as_slice())),
                _ => (0x000du16, 0u32, 0u32, None),
            };
            let mut payload = Vec::new();
            payload.extend_from_slice(&64u32.to_be_bytes());
            payload.extend_from_slice(&64u32.to_be_bytes());
            payload.extend_from_slice(&[0; 8]);
            payload.push(4);
            payload.extend_from_slice(&flags.to_be_bytes());
            payload.extend_from_slice(&[0; 2]);
            payload.extend_from_slice(&instances.to_be_bytes());
            let code_length = if symbols == 5 { 3 } else { 1 };
            let mut codes = Vec::new();
            for index in 0..35 {
                bits(
                    &mut codes,
                    u32::from(symbols != 0 && index == code_length),
                    4,
                );
            }
            for _ in 0..symbols {
                bits(&mut codes, 0, 1);
            }
            payload.extend_from_slice(&packed(&codes));
            let mut coded = Vec::new();
            if case == 2 {
                // Native STRIPT overflows at INT_MAX * 8, but zero instances
                // never read its invalid value and the region still succeeds.
                bits(&mut coded, 0x7f, 7); // Table K upper range, base 141.
                bits(&mut coded, i32::MAX as u32 - 141, 32);
            } else {
                bits(&mut coded, 0, 2); // Initial/delta T = 1 from table K.
                if case == 0 {
                    bits(&mut coded, 0x3f, 6); // Table F upper range, base 2048.
                    bits(&mut coded, i32::MAX as u32 - 2048, 32);
                } else {
                    bits(&mut coded, 0x3e, 6); // Table F lower range, base -2049.
                    bits(&mut coded, 2147481599, 32); // First S = INT_MIN.
                }
                bits(&mut coded, 0, code_length); // Symbol zero.
                if case == 0 {
                    bits(&mut coded, 1, 1); // Refinement; width delta two guarantees width > 1.
                    bits(&mut coded, 7, 3); // Table N value two.
                    bits(&mut coded, 0, 3); // RDH/RDX/RDY zero.
                    bits(&mut coded, 2, 5); // Marker-inclusive size two.
                    payload.extend_from_slice(&packed(&coded));
                    coded.clear();
                    payload.extend_from_slice(&[255, 172]);
                }
                bits(&mut coded, 1, 2); // Table H OOB discards the strip's cursor.
            }
            payload.extend_from_slice(&packed(&coded));
            let mut page = page_only(64, 64, false);
            page.truncate(30);
            page.extend_from_slice(&2u32.to_be_bytes());
            if symbols == 0 {
                page.extend_from_slice(&[6, 0, 1]);
            } else {
                page.extend_from_slice(&[6, 32, 0, 1]);
            }
            page.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            page.extend_from_slice(&payload);
            let mut bitmap = CfxDibitmap::create(64, 64, FxdibFormat::OneBppRgb, 0).unwrap();
            assert!(
                decode_jbig2_into(&page, &mut bitmap, globals),
                "case {case}"
            );
            assert!(bitmap.buffer().iter().all(|&byte| byte == 255));
        }
    }

    #[test]
    fn jbig2_image_composition_preserves_native_coordinate_boundary() {
        for x in [1048576i32, 1048577, i32::MAX, i32::MIN] {
            let mut page = page_only(1, 1, false);
            page.truncate(30);
            let mut payload = Vec::new();
            payload.extend_from_slice(&1u32.to_be_bytes());
            payload.extend_from_slice(&1u32.to_be_bytes());
            payload.extend_from_slice(&x.to_be_bytes());
            payload.extend_from_slice(&0i32.to_be_bytes());
            payload.extend_from_slice(&[4, 1, 0x26, 0xa8]); // Replace, MMR, one black pixel.
            page.extend_from_slice(&2u32.to_be_bytes());
            page.extend_from_slice(&[38, 0, 1]);
            page.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            page.extend_from_slice(&payload);
            let mut bitmap = CfxDibitmap::create(1048586, 1, FxdibFormat::OneBppRgb, 0).unwrap();
            assert!(decode_jbig2_into(&page, &mut bitmap, None));
            for (index, &value) in bitmap.buffer().iter().enumerate() {
                let expected = if x == 1048576 && index == 1048576 / 8 {
                    0x7f
                } else {
                    255
                };
                assert_eq!(value, expected, "x {x}, byte {index}");
            }
        }
    }

    #[test]
    fn jbig2_mmr_halftone_skips_plane_markers_and_clamps_pattern_indices() {
        fn segment(number: u32, kind: u8, refs: &[u8], payload: &[u8]) -> Vec<u8> {
            let mut bytes = number.to_be_bytes().to_vec();
            bytes.extend_from_slice(&[kind, (refs.len() as u8) << 5]);
            bytes.extend_from_slice(refs);
            bytes.push(1);
            bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
            bytes.extend_from_slice(payload);
            bytes
        }
        let mut page = page_only(1, 1, false);
        page.truncate(30);
        // Three one-pixel patterns: white, white, black. MMR horizontal mode
        // gives a white run of two and a black run of one.
        page.extend_from_slice(&segment(2, 16, &[], &[1, 1, 1, 0, 0, 0, 2, 0x2e, 0x80]));
        let mut halftone = Vec::new();
        halftone.extend_from_slice(&1u32.to_be_bytes());
        halftone.extend_from_slice(&1u32.to_be_bytes());
        halftone.extend_from_slice(&[0; 8]);
        halftone.extend_from_slice(&[4, 0x41]); // Replace externally and internally; MMR.
        halftone.extend_from_slice(&1u32.to_be_bytes());
        halftone.extend_from_slice(&1u32.to_be_bytes());
        halftone.extend_from_slice(&[0; 12]);
        // Upper plane is black, lower encoded plane white; Gray decoding
        // gives binary 3. Native clamps it to the last of three patterns.
        halftone.extend_from_slice(&[0x26, 0xa8, 0, 0x10, 1, 0x80, 0, 0x10, 1]);
        let prefix = page.len();
        page.extend_from_slice(&segment(3, 22, &[2], &halftone));
        let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        assert!(decode_jbig2_into(&page, &mut bitmap, None));
        assert_eq!(bitmap.buffer(), &[0x7f, 255, 255, 255]);
        let mut unknown_page = page_only(1, 1, false);
        unknown_page.truncate(30);
        let mut patterns = segment(2, 16, &[], &[1, 1, 1, 0, 0, 0, 2, 0x2e, 0x80]);
        patterns[7..11].copy_from_slice(&u32::MAX.to_be_bytes());
        unknown_page.extend_from_slice(&patterns);
        unknown_page.extend_from_slice(&[0; 4]);
        let mut region = segment(3, 22, &[2], &halftone);
        region[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        unknown_page.extend_from_slice(&region);
        unknown_page.extend_from_slice(&[0; 4]);
        unknown_page.extend_from_slice(&page_only(1, 1, false)[30..]);
        assert!(decode_jbig2_into(&unknown_page, &mut bitmap, None));
        assert_eq!(bitmap.buffer(), &[0x7f, 255, 255, 255]);
        page.truncate(prefix);
        page.extend_from_slice(&segment(3, 22, &[2, 1], &halftone));
        assert!(!decode_jbig2_into(&page, &mut bitmap, None));
        let mut oversized_grid = halftone;
        oversized_grid[18..22].copy_from_slice(&65535u32.to_be_bytes());
        oversized_grid[22..26].copy_from_slice(&65535u32.to_be_bytes());
        page.truncate(prefix);
        page.extend_from_slice(&segment(3, 22, &[2], &oversized_grid));
        assert!(!decode_jbig2_into(&page, &mut bitmap, None));
    }

    #[test]
    fn jbig2_arithmetic_completion_follows_each_native_pixel_kernel() {
        for (kind, flags, adaptive, unchecked_pixels) in [
            (38, 6, &[2, 255][..], false),
            (38, 6, &[0, 1][..], false),
            (42, 1, &[][..], true),                  // Optimized template 1.
            (42, 0, &[255; 4][..], true),            // Optimized template 0.
            (42, 0, &[0, 255, 255, 255][..], false), // Unoptimized template 0.
            (42, 3, &[][..], false),                 // TPGRON still checks each prediction bit.
            (42, 2, &[255; 4][..], false),
        ] {
            for (encoded, succeeds) in [(&[][..], false), (&[0][..], false), (&[255][..], true)] {
                let mut payload = Vec::new();
                payload.extend_from_slice(&9u32.to_be_bytes());
                payload.extend_from_slice(&2u32.to_be_bytes());
                payload.extend_from_slice(&[0; 8]);
                payload.extend_from_slice(&[4, flags]);
                payload.extend_from_slice(adaptive);
                payload.extend_from_slice(encoded);
                let mut page = page_only(9, 2, false);
                page.truncate(30);
                page.extend_from_slice(&2u32.to_be_bytes());
                page.extend_from_slice(&[kind, 0, 1]);
                page.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                page.extend_from_slice(&payload);
                let mut bitmap = CfxDibitmap::create(9, 2, FxdibFormat::OneBppRgb, 0).unwrap();
                assert_eq!(
                    decode_jbig2_into(&page, &mut bitmap, None),
                    kind == 38 || unchecked_pixels || succeeds,
                    "segment {kind}, flags {flags}, encoded {encoded:?}"
                );
            }
        }
    }

    #[test]
    fn jbig2_declared_lengths_position_headers_without_bounding_field_reads() {
        let mut page = page_only(9, 2, true);
        page.truncate(30);
        // PageInfo's required fields still read from the stream, and the
        // oversized next-header offset is clamped to the stream end.
        page[7..11].copy_from_slice(&4096u32.to_be_bytes());
        let mut bitmap = CfxDibitmap::create(9, 2, FxdibFormat::OneBppRgb, 0).unwrap();
        assert!(decode_jbig2_into(&page, &mut bitmap, None));
        assert!(bitmap.buffer().iter().all(|&byte| byte == 0));
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        for length in [0, u32::MAX - 1] {
            let mut globals = globals.to_vec();
            globals[7..11].copy_from_slice(&length.to_be_bytes());
            let mut context = hayro_jbig2::PdfiumDocumentContext::default();
            // A declared zero length or overflowing next offset fails later;
            // the decoded global dictionary was already inserted in the cache.
            assert!(!decode_jbig2_with_context(
                &page,
                &mut bitmap,
                Some(&globals),
                &mut context,
                19
            ));
            assert_eq!(
                context.symbol_dictionary_keys().collect::<Vec<_>>(),
                [(19, 11)]
            );
        }
    }

    #[test]
    fn jbig2_global_lookup_preserves_source_order_with_unsorted_numbers() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_page.bin"
        ));
        let globals = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/jbig2_globals.bin"
        ));
        let image = hayro_jbig2::Image::new_embedded(data, Some(globals)).unwrap();
        let mut expected = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        let mut actual = CfxDibitmap::create(
            image.width() as i32,
            image.height() as i32,
            FxdibFormat::OneBppRgb,
            0,
        )
        .unwrap();
        assert!(decode_jbig2_into(data, &mut expected, Some(globals)));
        let mut first = globals.to_vec();
        first[..4].copy_from_slice(&10u32.to_be_bytes());
        let mut second = globals.to_vec();
        second[..4].copy_from_slice(&2u32.to_be_bytes());
        first.extend_from_slice(&second);
        let mut page = data.to_vec();
        page[30..34].copy_from_slice(&11u32.to_be_bytes());
        page[36] = 10;
        assert!(decode_jbig2_into(&page, &mut actual, Some(&first)));
        assert!(actual
            .buffer()
            .iter()
            .zip(expected.buffer())
            .all(|(a, b)| a == b));
    }
}

#[cfg(test)]
mod adapter_boundary_tests {
    use super::*;
    use hayro_jbig2::Decoder;

    fn fax_params(columns: i32, rows: i32, black: bool) -> CpdfDictionary {
        use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
        let mut params = CpdfDictionary::new();
        for (key, value) in [
            (b"K".as_slice(), -1),
            (b"Columns".as_slice(), columns),
            (b"Rows".as_slice(), rows),
            (b"BlackIs1".as_slice(), i32::from(black)),
        ] {
            params.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        params
    }

    #[test]
    fn fax_bitmap_uses_decoder_columns_and_requested_bitmap_height() {
        // T.6 V(0) is the single bit1. With a white reference line each
        // repeated1 ends an independently known all-white scanline.
        for columns in [0, 8, 16, 33] {
            for black in [false, true] {
                let params = fax_params(columns, 0, black);
                let result = decode_fax(&[0xff], 8, 3, Some(&params)).unwrap();
                assert_eq!(result.rows, 3);
                assert_eq!(
                    result.bitmap.width(),
                    if columns == 0 { 8 } else { columns }
                );
                assert_eq!(result.bitmap.height(), 3);
                assert!(result
                    .bitmap
                    .buffer()
                    .iter()
                    .all(|&byte| byte == if black { 0 } else { 255 }));
            }
        }
    }

    #[test]
    fn fax_bitmap_reports_partial_rows_and_preserves_unwritten_storage() {
        let mut params = fax_params(8, 1, false);
        // Rows selects the decoder's advertised height, not an EOF boundary:
        // CPDF_DIB still requests its own row count while encoded data lasts.
        let result = decode_fax(&[0xff], 8, 3, Some(&params)).unwrap();
        assert_eq!(result.rows, 3);
        assert!(result.bitmap.buffer().iter().all(|&byte| byte == 255));
        // One V(0) bit followed by seven zero padding bits. Byte alignment
        // consumes that padding, so the next row begins at genuine input EOF.
        params.set_new_for(
            b"EncodedByteAlign",
            crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject::number_from_signed(1),
        );
        let result = decode_fax(&[0x80], 8, 3, Some(&params)).unwrap();
        assert_eq!(result.rows, 1);
        assert_eq!(result.bitmap.scanline(0).unwrap(), &[255; 4]);
        assert_eq!(result.bitmap.scanline(1).unwrap(), &[0; 4]);
        assert_eq!(result.bitmap.scanline(2).unwrap(), &[0; 4]);
        let params = fax_params(8, 65536, false);
        let result = decode_fax(&[0xff], 8, 3, Some(&params)).unwrap();
        assert_eq!(result.rows, 3); // Oversized Rows is ignored by CreateFaxDecoder.
        assert!(result.bitmap.buffer().iter().all(|&byte| byte == 255));
    }

    #[test]
    fn fax_bitmap_checks_source_row_capacity_and_invalid_geometry() {
        let params = fax_params(8, 0, false);
        assert!(decode_fax(&[0xff], 9, 1, Some(&params)).is_none());
        for (width, height) in [(-1, 1), (8, -1), (0, 1), (8, 0)] {
            let params = fax_params(0, 0, false);
            assert!(decode_fax(&[0xff], width, height, Some(&params)).is_none());
        }
        // No DecodeParms means the PDF default of1728 columns, not width8.
        let result = decode_fax(&[0x35, 0xc0], 8, 1, None).unwrap();
        assert_eq!(result.bitmap.width(), 1728);
        assert_eq!(result.bitmap.height(), 1);
        assert_eq!(result.rows, 1);
        assert_eq!(result.bitmap.scanline(0).unwrap()[0], 0x3f);
    }

    #[test]
    fn packed_callbacks_reject_partial_rows_width_mismatch_and_short_words() {
        for invalid in 0..3 {
            let mut bitmap = CfxDibitmap::create(9, 1, FxdibFormat::OneBppRgb, 0).unwrap();
            let mut sink = PackedSink::new(&mut bitmap, true);
            match invalid {
                0 => {
                    sink.push_pixel(true);
                    assert!(sink.push_packed_row(&[0], 9));
                }
                1 => {
                    assert!(sink.push_packed_row(&[0], 8));
                }
                _ => {
                    assert!(sink.push_packed_row(&[], 9));
                }
            }
            assert!(sink.invalid);
            assert_eq!(sink.y, 0);
            assert_eq!(sink.x, usize::from(invalid == 0));
            assert_eq!(
                sink.bitmap.scanline(0).unwrap()[0],
                if invalid == 0 { 0x7f } else { 0xff }
            );
        }
    }

    #[test]
    fn byte_chunks_and_partial_runs_preserve_pixel_order_and_padding() {
        let mut bitmap = CfxDibitmap::create(19, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        let mut sink = PackedSink::new(&mut bitmap, true);
        sink.push_pixel_chunk(true, 1); // Eight black pixels.
        sink.push(false, 3); // Three more black pixels, crossing a byte boundary.
        sink.push(true, 8); // Eight white pixels complete the row.
        assert_eq!(sink.bitmap.scanline(0).unwrap(), &[0, 0x1f, 0xff, 0xff]);
        sink.next_line();
        assert!(!sink.invalid);
        assert_eq!((sink.x, sink.y), (0, 1));
        let before = sink.bitmap.buffer().to_vec();
        assert!(sink.push_packed_row(&[0], 19));
        assert!(sink.invalid);
        assert_eq!(sink.bitmap.buffer(), before);
    }

    #[test]
    fn incomplete_and_overlong_runs_fail_without_writing_past_the_canvas() {
        let mut bitmap = CfxDibitmap::create(9, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        let mut sink = PackedSink::new(&mut bitmap, true);
        sink.push(false, 10);
        assert!(sink.invalid);
        assert_eq!(sink.x, 0);
        assert_eq!(sink.bitmap.buffer(), &[255; 4]);
        // Failure is sticky even if a later callback completes a legal row.
        sink.push(false, 9);
        sink.next_line();
        assert!(sink.invalid);
        assert_eq!(sink.bitmap.buffer(), &[0, 0x7f, 255, 255]);
        let mut bitmap = CfxDibitmap::create(9, 1, FxdibFormat::OneBppRgb, 0).unwrap();
        let mut sink = PackedSink::new(&mut bitmap, true);
        sink.push(false, 8);
        sink.next_line();
        assert!(sink.invalid);
        assert_eq!(sink.bitmap.buffer(), &[0, 255, 255, 255]);
    }
}
