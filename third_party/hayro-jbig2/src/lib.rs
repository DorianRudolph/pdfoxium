// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
/*!
A memory-safe, pure-Rust JBIG2 decoder.

`hayro-jbig2` decodes JBIG2 images as specified in ITU-T T.88 (also known as
ISO/IEC 14492). JBIG2 is a bi-level image compression standard commonly used
in PDF documents for compressing scanned text documents.

The crate is `no_std` compatible but requires an allocator to be available.

# Safety
This crate forbids unsafe code via a crate-level attribute.
*/

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![allow(missing_docs)]

extern crate alloc;

// PDFium external-canvas extensions are translated from
// core/fxcodec/jbig2/jbig2_context.cpp.
// Copyright 2014 The PDFium Authors. Original code copyright 2014 Foxit
// Software Inc. http://www.foxitsoftware.com
// These extensions use the BSD-style license in ../LICENSE-PDFIUM.

use alloc::vec::Vec;

use crate::arithmetic_decoder::ArithmeticDecoderContext;

/// A reusable context for decoding JBIG2 images.
#[derive(Default)]
pub struct DecoderContext {
    pub(crate) page_state: PageState,
    pub(crate) scratch_buffers: ScratchBuffers,
    pub(crate) page_bitmap: Bitmap,
}

#[derive(Default)]
pub(crate) struct ScratchBuffers {
    pub(crate) contexts: Vec<ArithmeticDecoderContext>,
}

/// A decoder for JBIG2 images.
pub trait Decoder {
    /// Accept a complete row of packed, most-significant-bit-first pixels.
    /// Each `u32` contains 32 pixels, with one meaning black. Only the first
    /// `width` bits are visible; remaining bits are padding. Return true if
    /// consumed, or false to use the pixel callbacks instead. `next_line`
    /// follows either output path. The words are borrowed without conversion.
    fn push_packed_row(&mut self, _words: &[u32], _width: u32) -> bool {
        false
    }

    /// Push a single pixel to the output.
    fn push_pixel(&mut self, black: bool);
    /// Push multiple chunks of 8 pixels of the same color.
    ///
    /// The `chunk_count` parameter indicates how many 8-pixel chunks to push.
    /// For example, if this method is called with `white = true` and
    /// `chunk_count = 10`, 80 white pixels are pushed (10 × 8 = 80).
    ///
    /// You can assume that this method is only called if the number of already
    /// pushed pixels is a multiple of 8 (i.e. byte-aligned).
    fn push_pixel_chunk(&mut self, black: bool, chunk_count: u32);
    /// Called when a row has been completed.
    fn next_line(&mut self);
}

#[cfg(feature = "image")]
pub mod integration;

mod arithmetic_decoder;
mod bitmap;
mod decode;
mod error;
mod file;
mod gray_scale;
mod huffman_table;
mod integer_decoder;
mod lazy;
mod page_info;
mod pdfium;
mod progressive;
pub use pdfium::PdfiumDocumentContext;
pub use progressive::{PdfiumDecodeBoundary, PdfiumProgressiveDecoder};
mod reader;
mod segment;
mod simd;
mod symbol_id_decoder;

use error::bail;
pub use error::{
    DecodeError, FormatError, HuffmanError, OverflowError, ParseError, RegionError, Result,
    SegmentError, SymbolError, TemplateError,
};

use crate::file::parse_segments_sequential;
use bitmap::Bitmap;
use decode::CombinationOperator;
use decode::generic;
use decode::generic_refinement;
use decode::halftone;
use decode::pattern;
use decode::pattern::PatternDictionary;
use decode::symbol;
use decode::symbol::SymbolDictionary;
use decode::text;
use file::parse_file;
use huffman_table::{HuffmanTable, StandardHuffmanTables};
use page_info::{PageInformation, parse_page_information};
use reader::Reader;
use segment::{SegmentType, parse_segment};

/// A JBIG2 image.
pub struct Image<'a> {
    /// The parsed segments.
    segments: Vec<segment::Segment<'a>>,
    /// The width of the image in pixels.
    width: u32,
    /// The height of the image in pixels.
    height: u32,
    /// The height determined from `EndOfStripe` segments, if applicable.
    height_from_stripes: Option<u32>,
    canvas: Option<(u32, u32)>,
    parse_error: Option<DecodeError>,
}

impl<'a> Image<'a> {
    /// Parse a JBIG2 file from the given data.
    ///
    /// The file is expected to use the sequential or random-access organization,
    /// as defined in Annex D.1 and D.2.
    pub fn new(data: &'a [u8]) -> Result<Self> {
        let file = parse_file(data)?;
        Self::from_segments(file.segments, None)
    }

    /// Parse an embedded JBIG2 image with optional global segments.
    ///
    /// The file is expected to use the embedded organization defined in
    /// Annex D.3.
    pub fn new_embedded(data: &'a [u8], globals: Option<&'a [u8]>) -> Result<Self> {
        Self::from_embedded(data, globals, None)
    }

    /// Use PDFium's external-canvas mode. Stream PageInfo dimensions do not
    /// replace the supplied PDF dimensions, and striped pages do not expand it.
    pub fn new_embedded_with_canvas(
        data: &'a [u8],
        globals: Option<&'a [u8]>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            bail!(FormatError::EmptyPage);
        }
        let mut segments = Vec::new();
        let mut parse_error = None;
        if let Some(globals) = globals.filter(|globals| !globals.is_empty()) {
            if globals.len() > 256 * 1024 * 1024 {
                parse_error = Some(ParseError::UnexpectedEof.into());
            } else {
                let mut reader = Reader::new_pdfium(globals);
                parse_error = parse_segments_sequential(&mut reader, &mut segments).err();
                for segment in &mut segments {
                    let offset = segment.data.as_ptr() as usize - globals.as_ptr() as usize;
                    segment.global_offset =
                        Some(u32::try_from(offset).map_err(|_| OverflowError::Index)?);
                }
            }
        }
        if parse_error.is_none() {
            if data.is_empty() || data.len() > 256 * 1024 * 1024 {
                parse_error = Some(ParseError::UnexpectedEof.into());
            } else {
                parse_error =
                    parse_segments_sequential(&mut Reader::new_pdfium(data), &mut segments).err();
            }
        }
        Ok(Self {
            segments,
            width,
            height,
            height_from_stripes: None,
            canvas: Some((width, height)),
            parse_error,
        })
    }

    fn from_embedded(
        data: &'a [u8],
        globals: Option<&'a [u8]>,
        canvas: Option<(u32, u32)>,
    ) -> Result<Self> {
        let mut segments = Vec::new();
        if let Some(globals_data) = globals {
            let mut reader = if canvas.is_some() {
                Reader::new_pdfium(globals_data)
            } else {
                Reader::new(globals_data)
            };
            parse_segments_sequential(&mut reader, &mut segments)?;
            for segment in &mut segments {
                let offset = segment.data.as_ptr() as usize - globals_data.as_ptr() as usize;
                segment.global_offset =
                    Some(u32::try_from(offset).map_err(|_| OverflowError::Index)?);
            }
        };

        let mut reader = if canvas.is_some() {
            Reader::new_pdfium(data)
        } else {
            Reader::new(data)
        };
        parse_segments_sequential(&mut reader, &mut segments)?;

        if canvas.is_none() {
            segments.sort_by_key(|seg| seg.header.segment_number);
        }

        Self::from_segments(segments, canvas)
    }

    fn from_segments(
        segments: Vec<segment::Segment<'a>>,
        canvas: Option<(u32, u32)>,
    ) -> Result<Self> {
        // Pre-scan for stripe height from EndOfStripe segments.
        let height_from_stripes = segments
            .iter()
            .filter(|seg| seg.header.segment_type == SegmentType::EndOfStripe)
            .filter_map(|seg| u32::from_be_bytes(seg.data.try_into().ok()?).checked_add(1))
            .max();

        // Find and parse page information to extract dimensions.
        let page_info_seg = segments
            .iter()
            .find(|s| s.header.segment_type == SegmentType::PageInformation)
            .ok_or(FormatError::MissingPageInfo)?;

        let mut reader = Reader::new(page_info_seg.data);
        let page_info = parse_page_information(&mut reader)?;

        // "A page's bitmap height may be declared in its page information segment
        // to be unknown (by specifying a height of 0xFFFFFFFF). In this case, the
        // page must be striped." (7.4.8.2)
        let (width, height) = if let Some(canvas) = canvas {
            canvas
        } else {
            let height = if page_info.height == 0xFFFF_FFFF {
                height_from_stripes.ok_or(FormatError::UnknownPageHeight)?
            } else {
                page_info.height
            };
            (page_info.width, height)
        };
        if width == 0 || height == 0 {
            bail!(FormatError::EmptyPage);
        }

        Ok(Self {
            segments,
            width,
            height,
            height_from_stripes,
            canvas,
            parse_error: None,
        })
    }

    /// The width of the image in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The height of the image in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Decode the image data through the given [`Decoder`].
    pub fn decode<D: Decoder>(&self, decoder: &mut D) -> Result<()> {
        let mut ctx = DecoderContext::default();

        self.decode_with(decoder, &mut ctx)
    }

    /// Decode with PDFium's per-document global symbol cache and return the
    /// final target page default pixel for native row padding.
    pub fn decode_with_document_context<D: Decoder>(
        &self,
        decoder: &mut D,
        document: &mut PdfiumDocumentContext,
        global_key: u64,
    ) -> Result<bool> {
        let mut ctx = DecoderContext::default();
        let default_pixel = decode_segments(
            &self.segments,
            self.height_from_stripes,
            &mut ctx,
            self.canvas,
            Some(document),
            global_key,
            self.parse_error,
        )?;
        emit_bitmap(&ctx.page_bitmap, decoder);
        Ok(default_pixel)
    }

    /// Decode using a reusable allocation context.
    pub fn decode_with<D: Decoder>(&self, decoder: &mut D, ctx: &mut DecoderContext) -> Result<()> {
        decode_segments(
            &self.segments,
            self.height_from_stripes,
            ctx,
            self.canvas,
            None,
            0,
            self.parse_error,
        )?;
        emit_bitmap(&ctx.page_bitmap, decoder);

        Ok(())
    }
}

fn emit_bitmap<D: Decoder>(bitmap: &Bitmap, decoder: &mut D) {
    let width = bitmap.width;
    let bytes_per_row = width.div_ceil(8) as usize;

    for row in bitmap.data.chunks_exact(bitmap.stride as usize) {
        if decoder.push_packed_row(row, width) {
            decoder.next_line();
            continue;
        }
        let mut x = 0_u32;
        let mut chunk_byte: Option<u8> = None;
        let mut chunk_count = 0_u32;

        let bytes = row.iter().flat_map(|w| w.to_be_bytes()).take(bytes_per_row);

        for byte in bytes {
            let remaining = width - x;

            if remaining >= 8 && (byte == 0x00 || byte == 0xFF) {
                // Continue the previous chunk.
                if chunk_byte == Some(byte) {
                    chunk_count += 1;
                    x += 8;
                    continue;
                }

                // Flush previous chunk if any, then start new one.
                if let Some(b) = chunk_byte {
                    decoder.push_pixel_chunk(b == 0xFF, chunk_count);
                }

                chunk_byte = Some(byte);
                chunk_count = 1;
                x += 8;

                continue;
            }

            // Can't continue/start chunk, flush any existing chunk first.
            if let Some(b) = chunk_byte.take() {
                decoder.push_pixel_chunk(b == 0xFF, chunk_count);
                chunk_count = 0;
            }

            // Emit individual pixels.
            let count = remaining.min(8);
            for i in 0..count {
                decoder.push_pixel((byte >> (7 - i)) & 1 != 0);
            }
            x += count;
        }

        // Flush any remaining chunk at end of row.
        if let Some(b) = chunk_byte {
            decoder.push_pixel_chunk(b == 0xFF, chunk_count);
        }

        decoder.next_line();
    }
}

#[cfg(test)]
mod packed_output_tests {
    use super::*;

    #[derive(Default)]
    struct PixelOutput {
        pixels: Vec<bool>,
        ends: Vec<usize>,
    }
    impl Decoder for PixelOutput {
        fn push_pixel(&mut self, black: bool) {
            self.pixels.push(black);
        }
        fn push_pixel_chunk(&mut self, black: bool, chunks: u32) {
            self.pixels
                .resize(self.pixels.len() + chunks as usize * 8, black);
        }
        fn next_line(&mut self) {
            self.ends.push(self.pixels.len());
        }
    }

    #[derive(Default)]
    struct PackedOutput {
        output: PixelOutput,
        rows: usize,
    }
    impl Decoder for PackedOutput {
        fn push_packed_row(&mut self, words: &[u32], width: u32) -> bool {
            self.rows += 1;
            for x in 0..width as usize {
                self.output
                    .pixels
                    .push(words[x / 32] & (1 << (31 - x % 32)) != 0);
            }
            true
        }
        fn push_pixel(&mut self, _: bool) {
            panic!("packed output must bypass pixel callbacks");
        }
        fn push_pixel_chunk(&mut self, _: bool, _: u32) {
            panic!("packed output must bypass run callbacks");
        }
        fn next_line(&mut self) {
            self.output.next_line();
        }
    }

    #[test]
    fn packed_rows_and_default_pixel_fallback_emit_the_same_visible_bits() {
        for width in 1..=97 {
            let mut bitmap = Bitmap::new_pdfium(width, 3);
            for (i, word) in bitmap.data.iter_mut().enumerate() {
                *word = [0xffff_ffff, 0, 0x0123_4567, 0xa5c3_7e81][i % 4];
            }
            let mut pixels = PixelOutput::default();
            let mut packed = PackedOutput::default();
            emit_bitmap(&bitmap, &mut pixels);
            emit_bitmap(&bitmap, &mut packed);
            assert_eq!(packed.rows, 3);
            assert_eq!(packed.output.pixels, pixels.pixels);
            assert_eq!(packed.output.ends, pixels.ends);
            assert_eq!(
                pixels.ends,
                [width as usize, width as usize * 2, width as usize * 3]
            );
        }
    }
}

struct NativeSegmentCursor<'a> {
    reader: Reader<'a>,
    stream_len: usize,
    global: bool,
}

fn decode_segments(
    segments: &[segment::Segment<'_>],
    height_from_stripes: Option<u32>,
    decoder_ctx: &mut DecoderContext,
    canvas: Option<(u32, u32)>,
    mut document: Option<&mut PdfiumDocumentContext>,
    global_key: u64,
    parse_error: Option<DecodeError>,
) -> Result<bool> {
    // Find and parse page information segment first.
    if canvas.is_some() {
        decoder_ctx.page_state.reset(PageInformation::default());
        decoder_ctx.page_state.pdfium = true;
    } else if let Some(page_info) = segments
        .iter()
        .find(|s| s.header.segment_type == SegmentType::PageInformation)
    {
        let mut reader = Reader::new(page_info.data);
        init_page(
            &mut reader,
            height_from_stripes,
            &mut decoder_ctx.page_state,
            &mut decoder_ctx.page_bitmap,
            canvas,
        )?;
    } else {
        bail!(FormatError::MissingPageInfo);
    }

    let page_state = &mut decoder_ctx.page_state;
    let scratch_buffers = &mut decoder_ctx.scratch_buffers;

    let mut global_bitmap = Bitmap::default();
    let mut canvas_ready = canvas.is_none();
    let mut was_global = true;
    let mut in_page = false;
    let mut target_default_pixel = page_state.page_info.flags.default_pixel != 0;
    // Known-length prefixes are already framed. Parsed unknown-length segments
    // resume native sequential framing only after their decoder supplies an offset.
    let mut source_segments = segments.iter();
    let mut cursor: Option<NativeSegmentCursor<'_>> = None;
    loop {
        let mut parsed;
        let seg = if let Some(active) = cursor.as_mut() {
            if active.reader.tail().map_or(0, |tail| tail.len()) < 11 {
                cursor = None;
                continue;
            }
            parsed = parse_segment(&mut active.reader)?;
            parsed.stream_len = active.stream_len;
            parsed.global_offset = active
                .global
                .then(|| (active.stream_len - parsed.data.len()) as u32);
            &parsed
        } else {
            let Some(seg) = source_segments.next() else {
                break;
            };
            seg
        };
        let global = canvas.is_some() && seg.global_offset.is_some();
        if canvas.is_some() && was_global && !global {
            let (width, height) = canvas.unwrap();
            decoder_ctx
                .page_bitmap
                .reinitialize_canvas(width, height, false)?;
            canvas_ready = true;
            page_state.page_info = PageInformation::default();
            page_state.page_pristine = true;
            in_page = false;
        }
        was_global = global;
        let page_bitmap = if global {
            &mut global_bitmap
        } else {
            &mut decoder_ctx.page_bitmap
        };
        if canvas.is_some()
            && !in_page
            && matches!(
                seg.header.segment_type,
                SegmentType::ImmediateGenericRegion
                    | SegmentType::ImmediateLosslessGenericRegion
                    | SegmentType::IntermediateGenericRegion
                    | SegmentType::ImmediateTextRegion
                    | SegmentType::ImmediateLosslessTextRegion
                    | SegmentType::IntermediateTextRegion
                    | SegmentType::ImmediateHalftoneRegion
                    | SegmentType::ImmediateLosslessHalftoneRegion
                    | SegmentType::IntermediateHalftoneRegion
                    | SegmentType::IntermediateGenericRefinementRegion
                    | SegmentType::ImmediateGenericRefinementRegion
                    | SegmentType::ImmediateLosslessGenericRefinementRegion
            )
        {
            bail!(FormatError::MissingPageInfo);
        }
        if canvas.is_some()
            && matches!(
                seg.header.segment_type,
                SegmentType::SymbolDictionary
                    | SegmentType::ImmediateTextRegion
                    | SegmentType::ImmediateLosslessTextRegion
                    | SegmentType::IntermediateTextRegion
            )
            && seg.header.referred_to_segments.iter().any(|number| {
                !page_state
                    .processed_segments
                    .iter()
                    .any(|(known, _)| known == number)
            })
        {
            bail!(SegmentError::InvalidReference);
        }
        let mut reader = if canvas.is_some() {
            Reader::new_pdfium(seg.data)
        } else {
            Reader::new(seg.data)
        };

        let native_data_end = match decode_segment(
            seg,
            &mut reader,
            page_state,
            page_bitmap,
            scratch_buffers,
            canvas,
            global,
            document.as_deref_mut(),
            global_key,
            &mut in_page,
            &mut target_default_pixel,
        )? {
            SegmentEffect::Data(end) => end,
            SegmentEffect::EndGlobal => {
                cursor = None;
                continue;
            }
            SegmentEffect::EndPage => break,
        };
        if canvas.is_some() {
            if let Some(length) = seg.header.data_length {
                let data_offset = seg.stream_len - seg.data.len();
                (data_offset as u32)
                    .checked_add(length)
                    .ok_or(OverflowError::Index)?;
            } else if let Some(end) = native_data_end {
                let mut reader = Reader::new_pdfium(seg.data);
                reader
                    .skip_bytes(end.saturating_add(4).min(seg.data.len()))
                    .ok_or(ParseError::UnexpectedEof)?;
                cursor = Some(NativeSegmentCursor {
                    reader,
                    stream_len: seg.stream_len,
                    global,
                });
            }
            page_state
                .processed_segments
                .push((seg.header.segment_number, seg.header.segment_type));
        }
    }

    // A deferred framing error follows all successful prefix/cache effects.
    // In particular, a global failure occurs before creating the local canvas.
    if let Some(error) = parse_error {
        return Err(error);
    }
    if !canvas_ready {
        let (width, height) = canvas.unwrap();
        decoder_ctx
            .page_bitmap
            .reinitialize_canvas(width, height, false)?;
    }
    Ok(target_default_pixel)
}

/// Page-level decoding state for a JBIG2 page.
#[derive(Default)]
pub(crate) struct PageState {
    pdfium: bool,
    processed_segments: Vec<(u32, SegmentType)>,
    /// The parsed page information.
    pub(crate) page_info: PageInformation,
    /// Whether the page bitmap is still in its initial state (not yet painted to).
    pub(crate) page_pristine: bool,
    /// Decoded intermediate regions, stored as (`segment_number`, region) pairs.
    pub(crate) referred_segments: Vec<(u32, Bitmap)>,
    /// Decoded pattern dictionaries, stored as (`segment_number`, dictionary) pairs.
    pub(crate) pattern_dictionaries: Vec<(u32, PatternDictionary)>,
    /// Decoded symbol dictionaries, stored as (`segment_number`, dictionary) pairs.
    pub(crate) symbol_dictionaries: Vec<(u32, SymbolDictionary)>,
    /// Decoded Huffman tables from table segments, stored as (`segment_number`, table) pairs.
    /// "Tables – see 7.4.13." (type 53)
    pub(crate) huffman_tables: Vec<(u32, HuffmanTable)>,
    /// Standard Huffman tables (`TABLE_A` through `TABLE_O`).
    pub(crate) standard_tables: StandardHuffmanTables,
}

impl PageState {
    fn reset(&mut self, page_info: PageInformation) {
        self.page_info = page_info;
        self.pdfium = false;
        self.processed_segments.clear();
        self.page_pristine = true;
        self.referred_segments.clear();
        self.pattern_dictionaries.clear();
        self.symbol_dictionaries.clear();
        self.huffman_tables.clear();
        // Standard tables are lazily built and reused across images.
    }

    /// Check if an immediate region can be decoded directly into the page bitmap.
    fn can_decode_directly(
        &self,
        page_bitmap: &Bitmap,
        region_info: &decode::RegionSegmentInfo,
        region_default_pixel: bool,
    ) -> bool {
        if !self.page_pristine {
            return false;
        }

        let covers_page = region_info.x_location == 0
            && region_info.y_location == 0
            && region_info.width == page_bitmap.width
            && region_info.height == page_bitmap.height;

        if !covers_page {
            return false;
        }

        let page_default_is_zero = self.page_info.flags.default_pixel == 0;

        if region_default_pixel == page_default_is_zero {
            return false;
        }

        let op = region_info.combination_operator;
        match op {
            CombinationOperator::Replace => true,
            CombinationOperator::Or | CombinationOperator::Xor => page_default_is_zero,
            CombinationOperator::And | CombinationOperator::Xnor => !page_default_is_zero,
        }
    }

    /// Store a decoded region for later reference.
    fn store_region(&mut self, segment_number: u32, region: Bitmap) {
        self.referred_segments.push((segment_number, region));
    }

    /// Look up a referred segment by number.
    fn get_referred_segment(&self, segment_number: u32) -> Option<&Bitmap> {
        if self.pdfium {
            let kind = self
                .processed_segments
                .iter()
                .find(|(number, _)| *number == segment_number)?
                .1;
            if !matches!(
                kind,
                SegmentType::IntermediateTextRegion
                    | SegmentType::IntermediateHalftoneRegion
                    | SegmentType::IntermediateGenericRegion
                    | SegmentType::IntermediateGenericRefinementRegion
            ) {
                return None;
            }
            return self
                .referred_segments
                .iter()
                .find(|(number, _)| *number == segment_number)
                .map(|entry| &entry.1);
        }

        self.referred_segments
            .binary_search_by_key(&segment_number, |(num, _)| *num)
            .ok()
            .map(|idx| &self.referred_segments[idx].1)
    }

    /// Store a decoded pattern dictionary for later reference.
    fn store_pattern_dictionary(&mut self, segment_number: u32, dictionary: PatternDictionary) {
        self.pattern_dictionaries.push((segment_number, dictionary));
    }

    /// Look up a pattern dictionary by segment number.
    fn get_pattern_dictionary(&self, segment_number: u32) -> Option<&PatternDictionary> {
        if self.pdfium {
            let kind = self
                .processed_segments
                .iter()
                .find(|(number, _)| *number == segment_number)?
                .1;
            if !matches!(kind, SegmentType::PatternDictionary) {
                return None;
            }
            return self
                .pattern_dictionaries
                .iter()
                .find(|(number, _)| *number == segment_number)
                .map(|entry| &entry.1);
        }

        self.pattern_dictionaries
            .binary_search_by_key(&segment_number, |(num, _)| *num)
            .ok()
            .map(|idx| &self.pattern_dictionaries[idx].1)
    }

    /// Store a decoded symbol dictionary for later reference.
    fn store_symbol_dictionary(&mut self, segment_number: u32, dictionary: SymbolDictionary) {
        self.symbol_dictionaries.push((segment_number, dictionary));
    }

    /// Look up a symbol dictionary by segment number.
    fn get_symbol_dictionary(&self, segment_number: u32) -> Option<&SymbolDictionary> {
        if self.pdfium {
            let kind = self
                .processed_segments
                .iter()
                .find(|(number, _)| *number == segment_number)?
                .1;
            if !matches!(kind, SegmentType::SymbolDictionary) {
                return None;
            }
            return self
                .symbol_dictionaries
                .iter()
                .find(|(number, _)| *number == segment_number)
                .map(|entry| &entry.1);
        }

        self.symbol_dictionaries
            .binary_search_by_key(&segment_number, |(num, _)| *num)
            .ok()
            .map(|idx| &self.symbol_dictionaries[idx].1)
    }

    /// Store a decoded Huffman table for later reference.
    fn store_huffman_table(&mut self, segment_number: u32, table: HuffmanTable) {
        self.huffman_tables.push((segment_number, table));
    }

    /// Look up a Huffman table by segment number.
    fn get_huffman_table(&self, segment_number: u32) -> Option<&HuffmanTable> {
        if self.pdfium {
            let kind = self
                .processed_segments
                .iter()
                .find(|(number, _)| *number == segment_number)?
                .1;
            if !matches!(kind, SegmentType::Tables) {
                return None;
            }
            return self
                .huffman_tables
                .iter()
                .find(|(number, _)| *number == segment_number)
                .map(|entry| &entry.1);
        }

        self.huffman_tables
            .binary_search_by_key(&segment_number, |(num, _)| *num)
            .ok()
            .map(|idx| &self.huffman_tables[idx].1)
    }
}

/// Parse page information and initialize the page bitmap.
fn init_page(
    reader: &mut Reader<'_>,
    height_from_stripes: Option<u32>,
    page: &mut PageState,
    bitmap: &mut Bitmap,
    canvas: Option<(u32, u32)>,
) -> Result<()> {
    let page_info = parse_page_information(reader)?;

    // "A page's bitmap height may be declared in its page information segment
    // to be unknown (by specifying a height of 0xFFFFFFFF). In this case, the
    // page must be striped." (7.4.8.2)
    let height = if let Some((_, height)) = canvas {
        height
    } else if page_info.height == 0xFFFF_FFFF {
        height_from_stripes.ok_or(FormatError::UnknownPageHeight)?
    } else {
        page_info.height
    };

    // "Bit 2: Page default pixel value. This bit contains the initial value
    // for every pixel in the page, before any region segments are decoded
    // or drawn." (7.4.8.5)
    if let Some((width, _)) = canvas {
        bitmap.reinitialize_canvas(width, height, page_info.flags.default_pixel != 0)?;
    } else {
        bitmap.reinitialize(page_info.width, height, page_info.flags.default_pixel != 0)?;
    }

    page.reset(page_info);

    Ok(())
}

enum SegmentEffect {
    Data(Option<usize>),
    EndGlobal,
    EndPage,
}

/// Shared segment dispatcher. The retained sequential decoder and uninterrupted
/// decoder use the same algorithms, reference checks and symbol-cache effects.
#[allow(clippy::too_many_arguments)]
fn decode_segment(
    seg: &segment::Segment<'_>,
    mut reader: &mut Reader<'_>,
    page_state: &mut PageState,
    page_bitmap: &mut Bitmap,
    scratch_buffers: &mut ScratchBuffers,
    canvas: Option<(u32, u32)>,
    global: bool,
    mut document: Option<&mut PdfiumDocumentContext>,
    global_key: u64,
    in_page: &mut bool,
    target_default_pixel: &mut bool,
) -> Result<SegmentEffect> {
    let mut native_data_end = None;
    match seg.header.segment_type {
        SegmentType::PageInformation => {
            if canvas.is_some() {
                let info = parse_page_information(&mut reader)?;
                native_data_end = Some(reader.byte_pos());
                let background = info.flags.default_pixel != 0;
                if global {
                    let height = if info.height == u32::MAX {
                        u32::from(info._striping._max_stripe_size)
                    } else {
                        info.height
                    };
                    // Native replaces the owned image, then requires
                    // has_data before retaining the PageInfo record.
                    *page_bitmap = Bitmap::new_pdfium(info.width, height);
                    if page_bitmap.data.is_empty() {
                        bail!(FormatError::EmptyPage);
                    }
                    if background {
                        page_bitmap.data.fill(!0);
                    }
                    if info._striping._is_striped || info.height == u32::MAX {
                        page_bitmap.expand_background = Some(background);
                    }
                } else {
                    page_bitmap.data.fill(if background { !0 } else { 0 });
                    *target_default_pixel = background;
                }
                page_state.page_info = info;
                page_state.page_pristine = true;
                *in_page = true;
            }
        }
        SegmentType::ImmediateGenericRegion | SegmentType::ImmediateLosslessGenericRegion => {
            let had_unknown_length = seg.header.data_length.is_none();
            let header = generic::parse(&mut reader, had_unknown_length)?;

            let (consumed, _) = if (canvas.is_none() || header.mmr)
                && page_state.can_decode_directly(page_bitmap, &header.region_info, false)
            {
                generic::decode_into(&header, page_bitmap, scratch_buffers)?
            } else {
                let (region, consumed, rows) = generic::decode(&header, scratch_buffers)?;
                page_bitmap.combine_completed_rows(
                    &region.bitmap,
                    region.bitmap.x_location as i32,
                    region.bitmap.y_location as i32,
                    region.combination_operator,
                    rows,
                );
                (consumed, rows)
            };
            native_data_end = Some(reader.byte_pos() + consumed);
            page_state.page_pristine = false;
        }
        SegmentType::IntermediateGenericRegion => {
            let header = generic::parse(&mut reader, seg.header.data_length.is_none())?;
            let (region, consumed, _) = generic::decode(&header, scratch_buffers)?;
            native_data_end = Some(reader.byte_pos() + consumed);
            page_state.store_region(seg.header.segment_number, region.bitmap);
        }
        SegmentType::PatternDictionary => {
            let header = pattern::parse(&mut reader)?;
            let (dictionary, consumed) = pattern::decode(&header, scratch_buffers)?;
            native_data_end = Some(reader.byte_pos() + consumed);
            page_state.store_pattern_dictionary(seg.header.segment_number, dictionary);
        }
        SegmentType::SymbolDictionary => {
            // "1) Concatenate all the input symbol dictionaries to form SDINSYMS."
            // (6.5.5, step 1)
            // Collect references to avoid cloning; symbols are only cloned if re-exported.
            let input_symbols: Vec<&Bitmap> = seg
                .header
                .referred_to_segments
                .iter()
                .filter_map(|&num| page_state.get_symbol_dictionary(num))
                .flat_map(|dict| dict.exported_symbols.iter())
                .collect();

            // Collect Huffman tables from referred table segments.
            let referred_tables: Vec<HuffmanTable> = seg
                .header
                .referred_to_segments
                .iter()
                .filter_map(|&num| page_state.get_huffman_table(num))
                .cloned()
                .collect();

            // Get retained contexts from the last referred symbol dictionary (7.4.2.2 step 3).
            let last_dictionary = if canvas.is_some() {
                seg.header
                    .referred_to_segments
                    .iter()
                    .rev()
                    .find_map(|&number| page_state.get_symbol_dictionary(number))
            } else {
                seg.header
                    .referred_to_segments
                    .last()
                    .and_then(|&number| page_state.get_symbol_dictionary(number))
            };
            let header = symbol::parse(&mut reader)?;
            let (dictionary, consumed) = if canvas.is_some() {
                let initial = symbol::initial_contexts(&header, last_dictionary)?;
                let key = seg.global_offset.map(|offset| (global_key, offset));
                let cached = key.and_then(|key| {
                    document
                        .as_deref_mut()
                        .and_then(|document| document.get(key))
                });
                if let Some(mut dictionary) = cached {
                    if header.flags.bitmap_context_retained {
                        dictionary.retained_contexts = Some(initial);
                    }
                    // A native cache hit does not read payload or markers.
                    (dictionary, 0)
                } else {
                    let (dictionary, consumed) = symbol::decode_with_initial_contexts(
                        &header,
                        &input_symbols,
                        &referred_tables,
                        &page_state.standard_tables,
                        initial,
                    )?;
                    if let (Some(key), Some(document)) = (key, document.as_deref_mut()) {
                        document.insert(key, &dictionary);
                    }
                    (dictionary, consumed)
                }
            } else {
                (
                    symbol::decode(
                        &header,
                        &input_symbols,
                        &referred_tables,
                        &page_state.standard_tables,
                        last_dictionary
                            .and_then(|dictionary| dictionary.retained_contexts.as_ref()),
                    )?,
                    0,
                )
            };
            native_data_end = Some(reader.byte_pos() + consumed);
            page_state.store_symbol_dictionary(seg.header.segment_number, dictionary);
        }
        SegmentType::ImmediateTextRegion | SegmentType::ImmediateLosslessTextRegion => {
            // Collect symbols from referred symbol dictionaries (SBSYMS).
            let symbols: Vec<&Bitmap> = seg
                .header
                .referred_to_segments
                .iter()
                .filter_map(|&num| page_state.get_symbol_dictionary(num))
                .flat_map(|dict| dict.exported_symbols.iter())
                .collect();

            // Collect Huffman tables from referred table segments.
            // "These user-supplied Huffman decoding tables may be supplied either
            // as a Tables segment..." (7.4.3.1.6)
            let referred_tables: Vec<HuffmanTable> = seg
                .header
                .referred_to_segments
                .iter()
                .filter_map(|&num| page_state.get_huffman_table(num))
                .cloned()
                .collect();

            let mut header = text::parse(&mut reader, symbols.len() as u32)?;
            if canvas.is_some() {
                header.segment_data_len = seg.stream_len;
            }

            let consumed = if page_state.can_decode_directly(
                page_bitmap,
                &header.region_info,
                header.flags.default_pixel,
            ) {
                text::decode_into(
                    &header,
                    &symbols,
                    &referred_tables,
                    &page_state.standard_tables,
                    page_bitmap,
                    scratch_buffers,
                )?
            } else {
                let (region, consumed) = text::decode(
                    &header,
                    &symbols,
                    &referred_tables,
                    &page_state.standard_tables,
                    scratch_buffers,
                )?;
                page_bitmap.combine(
                    &region.bitmap,
                    region.bitmap.x_location as i32,
                    region.bitmap.y_location as i32,
                    region.combination_operator,
                );
                consumed
            };
            native_data_end = Some(reader.byte_pos() + consumed);
            page_state.page_pristine = false;
        }
        SegmentType::IntermediateTextRegion => {
            // Collect symbols from referred symbol dictionaries (SBSYMS).
            let symbols: Vec<&Bitmap> = seg
                .header
                .referred_to_segments
                .iter()
                .filter_map(|&num| page_state.get_symbol_dictionary(num))
                .flat_map(|dict| dict.exported_symbols.iter())
                .collect();

            // Collect Huffman tables from referred table segments.
            let referred_tables: Vec<HuffmanTable> = seg
                .header
                .referred_to_segments
                .iter()
                .filter_map(|&num| page_state.get_huffman_table(num))
                .cloned()
                .collect();

            let mut header = text::parse(&mut reader, symbols.len() as u32)?;
            if canvas.is_some() {
                header.segment_data_len = seg.stream_len;
            }
            let (region, consumed) = text::decode(
                &header,
                &symbols,
                &referred_tables,
                &page_state.standard_tables,
                scratch_buffers,
            )?;
            native_data_end = Some(reader.byte_pos() + consumed);
            page_state.store_region(seg.header.segment_number, region.bitmap);
        }
        SegmentType::ImmediateHalftoneRegion | SegmentType::ImmediateLosslessHalftoneRegion => {
            if canvas.is_some() && seg.header.referred_to_segments.len() != 1 {
                bail!(SegmentError::InvalidReference);
            }
            let pattern_dict = seg
                .header
                .referred_to_segments
                .first()
                .and_then(|&num| page_state.get_pattern_dictionary(num))
                .ok_or(SegmentError::MissingPatternDictionary)?;

            let header = halftone::parse(&mut reader)?;

            let consumed = if !header.pdfium
                && page_state.can_decode_directly(
                    page_bitmap,
                    &header.region_info,
                    header.flags.initial_pixel_color,
                ) {
                halftone::decode_into(&header, pattern_dict, page_bitmap, scratch_buffers)?
            } else {
                let (region, consumed) = halftone::decode(&header, pattern_dict, scratch_buffers)?;
                page_bitmap.combine(
                    &region.bitmap,
                    region.bitmap.x_location as i32,
                    region.bitmap.y_location as i32,
                    region.combination_operator,
                );
                consumed
            };
            native_data_end = Some(reader.byte_pos() + consumed);
            page_state.page_pristine = false;
        }
        SegmentType::IntermediateHalftoneRegion => {
            if canvas.is_some() && seg.header.referred_to_segments.len() != 1 {
                bail!(SegmentError::InvalidReference);
            }
            let pattern_dict = seg
                .header
                .referred_to_segments
                .first()
                .and_then(|&num| page_state.get_pattern_dictionary(num))
                .ok_or(SegmentError::MissingPatternDictionary)?;

            let header = halftone::parse(&mut reader)?;
            let (region, consumed) = halftone::decode(&header, pattern_dict, scratch_buffers)?;
            native_data_end = Some(reader.byte_pos() + consumed);
            page_state.store_region(seg.header.segment_number, region.bitmap);
        }
        SegmentType::IntermediateGenericRefinementRegion
        | SegmentType::ImmediateGenericRefinementRegion
        | SegmentType::ImmediateLosslessGenericRefinementRegion => {
            let intermediate =
                seg.header.segment_type == SegmentType::IntermediateGenericRefinementRegion;
            let header = generic_refinement::parse(&mut reader)?;
            let referred_segment = seg
                .header
                .referred_to_segments
                .first()
                .and_then(|&number| page_state.get_referred_segment(number));
            if canvas.is_some()
                && !seg.header.referred_to_segments.is_empty()
                && referred_segment.is_none()
            {
                // Native ParseGenericRefinementRegion checks the first
                // reference on every loop iteration. Preserve that order;
                // a wrong-type first reference never falls back to the page.
                bail!(SegmentError::InvalidReference);
            }
            let page_subimage = if canvas.is_some() && seg.header.referred_to_segments.is_empty() {
                Some(page_bitmap.pdfium_subimage(
                    header.region_info.x_location as i32,
                    header.region_info.y_location as i32,
                    header.region_info.width,
                    header.region_info.height,
                )?)
            } else {
                None
            };
            if !intermediate
                && canvas.is_none()
                && let Some(reference) = referred_segment
                && page_state.can_decode_directly(page_bitmap, &header.region_info, false)
            {
                generic_refinement::decode_into(&header, reference, page_bitmap, scratch_buffers)?;
            } else {
                let reference = page_subimage
                    .as_ref()
                    .or(referred_segment)
                    .unwrap_or(page_bitmap);
                let (region, consumed) =
                    generic_refinement::decode(&header, reference, scratch_buffers)?;
                native_data_end = Some(reader.byte_pos() + consumed);
                if intermediate {
                    page_state.store_region(seg.header.segment_number, region.bitmap);
                } else {
                    page_bitmap.combine(
                        &region.bitmap,
                        region.bitmap.x_location as i32,
                        region.bitmap.y_location as i32,
                        region.combination_operator,
                    );
                }
            }
            if !intermediate {
                page_state.page_pristine = false;
            }
        }
        SegmentType::Tables => {
            // "Tables – see 7.4.13." (type 53)
            // "This segment contains data which defines one or more user-supplied
            // Huffman coding tables." (7.4.13)
            let table = HuffmanTable::read_custom(&mut reader)?;
            native_data_end = Some(reader.byte_pos() + usize::from(reader.bit_pos() != 0));
            page_state.store_huffman_table(seg.header.segment_number, table);
        }
        SegmentType::EndOfPage | SegmentType::EndOfFile => {
            if global {
                *in_page = false;
                return Ok(SegmentEffect::EndGlobal);
            }
            return Ok(SegmentEffect::EndPage);
        }
        // Other segment types not yet implemented.
        _ => {}
    }
    Ok(SegmentEffect::Data(native_data_end))
}
