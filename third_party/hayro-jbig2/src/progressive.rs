// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by the BSD-style license in
// ../LICENSE-PDFIUM. Original code copyright 2014 Foxit Software Inc.
//! Owned continuation state for jbig2_context/decoder and progressive GRD.
//! Source slices are borrowed only during advance; no encoded data is copied,
//! no arithmetic work is replayed, and no embedding callback runs in the codec.
use super::*;
use crate::arithmetic_decoder::{ArithmeticDecoder, ArithmeticDecoderState};
use crate::decode::{AdaptiveTemplatePixel, RegionSegmentInfo, Template};
use crate::segment::{Segment, SegmentHeader, parse_segment_header};

/// An actual completed codec boundary. The caller may consult its pause owner
/// after releasing all document/cache/source borrows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PdfiumDecodeBoundary {
    PageReady,
    Segment,
    GenericRows { first: u32, end: u32 },
    Finished { default_pixel: bool },
}
struct PendingSegment {
    header: SegmentHeader,
    data_offset: usize,
}
struct GenericState {
    info: RegionSegmentInfo,
    template: Template,
    tpgdon: bool,
    at: [AdaptiveTemplatePixel; 4],
    data_offset: usize,
    bitmap: Bitmap,
    arithmetic: ArithmeticDecoderState,
    progress: generic::ArithmeticProgress,
    contexts: Vec<ArithmeticDecoderContext>,
}

/// Source CJBig2_Context::pause_step_/segment_/stream_/page_/GRD state. The
/// document symbol cache is supplied for each call rather than held borrowed.
pub struct PdfiumProgressiveDecoder {
    width: u32,
    height: u32,
    context: DecoderContext,
    global_bitmap: Bitmap,
    global: bool,
    local_ready: bool,
    position: usize,
    current: Option<PendingSegment>,
    generic: Option<GenericState>,
    in_page: bool,
    default_pixel: bool,
    finished: bool,
    failure: Option<DecodeError>,
}
impl PdfiumProgressiveDecoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            bail!(FormatError::EmptyPage);
        }
        let mut context = DecoderContext::default();
        context.page_state.reset(PageInformation::default());
        context.page_state.pdfium = true;
        Ok(Self {
            width,
            height,
            context,
            global_bitmap: Bitmap::default(),
            global: true,
            local_ready: false,
            position: 0,
            current: None,
            generic: None,
            in_page: false,
            default_pixel: false,
            finished: false,
            failure: None,
        })
    }
    /// Whether native sequential parsing has a page on which to pause.
    pub fn can_pause(&self) -> bool {
        self.local_ready || !self.global_bitmap.data.is_empty()
    }
    pub fn completed_generic_rows(&self) -> Option<u32> {
        self.generic.as_ref().map(|state| state.progress.row)
    }
    /// Emit only on final success. Source Jbig2Decoder inverts the external
    /// canvas once, after the context reaches DecodeFinished.
    pub fn emit<D: Decoder>(&self, decoder: &mut D) -> Result<()> {
        if !self.finished || self.failure.is_some() {
            bail!(ParseError::UnexpectedEof);
        }
        emit_bitmap(&self.context.page_bitmap, decoder);
        Ok(())
    }
    pub fn advance(
        &mut self,
        data: &[u8],
        globals: Option<&[u8]>,
        document: &mut PdfiumDocumentContext,
        global_key: u64,
        row_budget: u32,
    ) -> Result<PdfiumDecodeBoundary> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.finished {
            return Ok(PdfiumDecodeBoundary::Finished {
                default_pixel: self.default_pixel,
            });
        }
        let result = self.advance_inner(data, globals, document, global_key, row_budget.max(1));
        if let Err(error) = result {
            self.failure = Some(error);
        }
        result
    }
    fn advance_inner(
        &mut self,
        data: &[u8],
        globals: Option<&[u8]>,
        document: &mut PdfiumDocumentContext,
        global_key: u64,
        row_budget: u32,
    ) -> Result<PdfiumDecodeBoundary> {
        if data.is_empty()
            || data.len() > 256 * 1024 * 1024
            || globals.is_some_and(|source| source.len() > 256 * 1024 * 1024)
        {
            bail!(ParseError::UnexpectedEof);
        }
        let source = if self.global {
            globals.unwrap_or(&[])
        } else {
            data
        };
        if self.current.is_none() {
            // Native DecodeSequential ignores the tail shorter than a header.
            if source.len().saturating_sub(self.position) < 11 {
                if self.global {
                    self.global = false;
                    self.position = 0;
                    self.context
                        .page_bitmap
                        .reinitialize_canvas(self.width, self.height, false)?;
                    self.local_ready = true;
                    self.context.page_state.page_info = PageInformation::default();
                    self.context.page_state.page_pristine = true;
                    self.in_page = false;
                    return Ok(PdfiumDecodeBoundary::PageReady);
                }
                self.finished = true;
                return Ok(PdfiumDecodeBoundary::Finished {
                    default_pixel: self.default_pixel,
                });
            }
            let mut reader = Reader::new_pdfium(&source[self.position..]);
            let header = parse_segment_header(&mut reader)?;
            self.current = Some(PendingSegment {
                header,
                data_offset: self.position + reader.byte_pos(),
            });
        }
        let current = self.current.take().unwrap();
        let seg = Segment {
            header: current.header,
            stream_len: source.len(),
            data: source
                .get(current.data_offset..)
                .ok_or(ParseError::UnexpectedEof)?,
            global_offset: self.global.then_some(current.data_offset as u32),
        };
        let kind = seg.header.segment_type;
        if !self.in_page
            && matches!(
                kind,
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
        if matches!(
            kind,
            SegmentType::SymbolDictionary
                | SegmentType::ImmediateTextRegion
                | SegmentType::ImmediateLosslessTextRegion
                | SegmentType::IntermediateTextRegion
        ) && seg.header.referred_to_segments.iter().any(|number| {
            !self
                .context
                .page_state
                .processed_segments
                .iter()
                .any(|(known, _)| known == number)
        }) {
            bail!(SegmentError::InvalidReference);
        }
        let mut reader = Reader::new_pdfium(seg.data);
        let page_bitmap = if self.global {
            &mut self.global_bitmap
        } else {
            &mut self.context.page_bitmap
        };
        let mut generic_end = None;
        if kind.is_generic_region() {
            if self.generic.is_none() {
                let header = generic::parse(&mut reader, seg.header.data_length.is_none())?;
                if !header.mmr {
                    let bitmap = Bitmap::new_region(
                        header.region_info.width,
                        header.region_info.height,
                        header.region_info.x_location,
                        header.region_info.y_location,
                        false,
                        true,
                    )?;
                    let arithmetic = ArithmeticDecoder::new_with_pdfium(header.data, true).save();
                    self.generic = Some(GenericState {
                        info: header.region_info,
                        template: header.template,
                        tpgdon: header.tpgdon,
                        at: header.adaptive_template_pixels,
                        data_offset: current.data_offset + reader.byte_pos(),
                        bitmap,
                        arithmetic,
                        progress: generic::ArithmeticProgress::default(),
                        contexts: alloc::vec![ArithmeticDecoderContext::default(); 1 << header.template.context_bits()],
                    });
                }
            }
            if let Some(state) = &mut self.generic {
                let bytes = source
                    .get(state.data_offset..)
                    .ok_or(ParseError::UnexpectedEof)?;
                let mut arithmetic = ArithmeticDecoder::resume(bytes, state.arithmetic);
                let first = state.progress.row;
                let mut remaining = row_budget;
                let result = generic::decode_bitmap_arithmetic_coding_progressive(
                    &mut state.bitmap,
                    &mut arithmetic,
                    &mut state.contexts,
                    state.template,
                    state.tpgdon,
                    &state.at,
                    &mut state.progress,
                    &mut || {
                        remaining -= 1;
                        remaining == 0
                    },
                );
                state.arithmetic = arithmetic.save();
                // Native retains the allocated image and its completed replace
                // rectangle after arithmetic EOF; it does not replay that row.
                if let Err(error) = result {
                    if !matches!(error, DecodeError::Parse(ParseError::UnexpectedEof)) {
                        return Err(error);
                    }
                    state.progress.paused = false;
                }
                if kind != SegmentType::IntermediateGenericRegion {
                    page_bitmap.combine_row_range(
                        &state.bitmap,
                        state.info.x_location as i32,
                        state.info.y_location as i32,
                        state.info.combination_operator,
                        first,
                        state.progress.row,
                    );
                }
                if state.progress.paused {
                    let end = state.progress.row;
                    self.current = Some(PendingSegment {
                        header: seg.header,
                        data_offset: current.data_offset,
                    });
                    return Ok(PdfiumDecodeBoundary::GenericRows { first, end });
                }
                generic_end = Some(
                    state.data_offset - current.data_offset
                        + arithmetic
                            .byte_position()
                            .saturating_add(2)
                            .min(bytes.len()),
                );
                let state = self.generic.take().unwrap();
                if kind == SegmentType::IntermediateGenericRegion {
                    self.context
                        .page_state
                        .store_region(seg.header.segment_number, state.bitmap);
                } else {
                    self.context.page_state.page_pristine = false;
                }
            }
        }
        let effect = if let Some(end) = generic_end {
            SegmentEffect::Data(Some(end))
        } else {
            // Generic MMR and every other segment retain the shared synchronous
            // source algorithms; their native outer continuation is per segment.
            reader = Reader::new_pdfium(seg.data);
            decode_segment(
                &seg,
                &mut reader,
                &mut self.context.page_state,
                page_bitmap,
                &mut self.context.scratch_buffers,
                Some((self.width, self.height)),
                self.global,
                Some(document),
                global_key,
                &mut self.in_page,
                &mut self.default_pixel,
            )?
        };
        let end = match effect {
            SegmentEffect::Data(end) => end,
            SegmentEffect::EndGlobal => {
                self.position = source.len();
                return Ok(PdfiumDecodeBoundary::Segment);
            }
            SegmentEffect::EndPage => {
                self.finished = true;
                return Ok(PdfiumDecodeBoundary::Finished {
                    default_pixel: self.default_pixel,
                });
            }
        };
        self.position = if let Some(length) = seg.header.data_length {
            let offset = u32::try_from(current.data_offset).map_err(|_| OverflowError::Index)?;
            offset.checked_add(length).ok_or(OverflowError::Index)? as usize
        } else {
            current.data_offset + end.unwrap_or(0).saturating_add(4).min(seg.data.len())
        }
        .min(source.len());
        self.context
            .page_state
            .processed_segments
            .push((seg.header.segment_number, kind));
        Ok(PdfiumDecodeBoundary::Segment)
    }
}
#[cfg(test)]
#[path = "progressive_tests.rs"]
mod tests;
