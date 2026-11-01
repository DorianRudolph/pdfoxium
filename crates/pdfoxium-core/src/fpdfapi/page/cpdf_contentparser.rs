// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Content stream preparation and clip cleanup from
//! `core/fpdfapi/page/cpdf_contentparser.{h,cpp}`. Operator execution and
//! pause-aware operator execution uses a separately borrowed parser and holder.

use super::cpdf_pageobject::PageObjectType;
use super::cpdf_pageobjectholder::CpdfPageObjectHolder;
use super::cpdf_streamcontentparser::CpdfStreamContentParser;
use crate::fpdfapi::font::cpdf_type3char::CpdfType3Char;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::{CpdfRetainedStreamAcc, RetainedStreamSpan};
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use crate::fxcrt::pauseindicator_iface::PauseIndicatorIface;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentStage {
    GetContent,
    PrepareContent,
    Parse,
    CheckClip,
    Complete,
}

pub struct CpdfContentParser {
    stage: ContentStage,
    single_stream: Option<CpdfRetainedStreamAcc>,
    streams: Vec<Option<ObjectRef>>,
    stream_array: Vec<Option<CpdfRetainedStreamAcc>>,
    data: Option<Vec<u8>>,
    stream_segment_offsets: Vec<u32>,
    current_offset: u32,
}

impl CpdfContentParser {
    pub fn from_stream(stream: &ObjectRef) -> Self {
        let mut parser = Self::empty();
        parser.single_stream = CpdfRetainedStreamAcc::load_filtered(stream.clone());
        parser.stage = ContentStage::Parse;
        parser
    }
    pub fn from_page_dict(page_dict: &CpdfDictionary) -> Self {
        let Some(contents) = page_dict.get_direct_object_for(b"Contents") else {
            return Self::empty();
        };
        let borrowed = contents.borrow();
        if borrowed.as_any().is::<CpdfStream>() {
            drop(borrowed);
            let mut parser = Self::from_stream(&contents);
            parser.stage = ContentStage::PrepareContent;
            return parser;
        }
        let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
            return Self::empty();
        };
        Self::from_streams(
            (0..array.size())
                .map(|index| array.get_direct_object_at(index))
                .collect(),
        )
    }
    pub fn from_streams(streams: Vec<Option<ObjectRef>>) -> Self {
        let mut parser = Self::empty();
        if streams.is_empty() || u32::try_from(streams.len()).is_err() {
            return parser;
        }
        parser.stream_array = (0..streams.len()).map(|_| None).collect();
        parser.streams = streams;
        parser.stage = ContentStage::GetContent;
        parser
    }
    fn empty() -> Self {
        Self {
            stage: ContentStage::Complete,
            single_stream: None,
            streams: Vec::new(),
            stream_array: Vec::new(),
            data: None,
            stream_segment_offsets: Vec::new(),
            current_offset: 0,
        }
    }
    pub fn stage(&self) -> ContentStage {
        self.stage
    }
    pub fn data(&self) -> RetainedStreamSpan<'_> {
        match &self.data {
            Some(data) => RetainedStreamSpan::Data(data),
            None => self.single_stream.as_ref().map_or(
                RetainedStreamSpan::Data(&[]),
                CpdfRetainedStreamAcc::get_span,
            ),
        }
    }
    pub fn stream_segment_offsets(&self) -> &[u32] {
        &self.stream_segment_offsets
    }
    fn get_content(&mut self) -> ContentStage {
        let index = self.current_offset as usize;
        self.stream_array[index] = self.streams[index]
            .take()
            .and_then(CpdfRetainedStreamAcc::load_filtered);
        self.current_offset += 1;
        if self.current_offset as usize == self.streams.len() {
            ContentStage::PrepareContent
        } else {
            ContentStage::GetContent
        }
    }
    fn prepare_content(&mut self) -> ContentStage {
        self.current_offset = 0;
        if self.stream_array.is_empty() {
            return ContentStage::Parse;
        }
        let mut size = 0_u32;
        for stream in &self.stream_array {
            self.stream_segment_offsets.push(size);
            let length = stream.as_ref().map_or(0, |stream| stream.get_span().len());
            let Some(next) = u32::try_from(length)
                .ok()
                .and_then(|length| size.checked_add(length))
                .and_then(|value| value.checked_add(1))
            else {
                return ContentStage::Complete;
            };
            size = next;
        }
        let mut data = Vec::new();
        if data.try_reserve_exact(size as usize).is_err() {
            self.data = None;
            return ContentStage::Complete;
        }
        for stream in &self.stream_array {
            if let Some(stream) = stream {
                data.extend_from_slice(&stream.get_span());
            }
            data.push(b' ');
        }
        self.stream_array.clear();
        self.streams.clear();
        self.data = Some(data);
        ContentStage::Parse
    }
    fn parse(
        &mut self,
        holder: &mut CpdfPageObjectHolder,
        parser: &mut CpdfStreamContentParser,
    ) -> ContentStage {
        if self.current_offset as usize >= self.data().len() {
            return ContentStage::CheckClip;
        }
        if self.stream_segment_offsets.is_empty() {
            self.stream_segment_offsets.push(0);
        }
        let consumed = parser.parse(
            &self.data(),
            self.current_offset as usize,
            100,
            &self.stream_segment_offsets,
            holder,
        );
        // A selected parser always consumes input here. Avoid looping forever
        // if a malformed stream violates that invariant.
        if consumed == 0 {
            return ContentStage::CheckClip;
        }
        let Some(offset) = u32::try_from(consumed)
            .ok()
            .and_then(|value| self.current_offset.checked_add(value))
        else {
            return ContentStage::Complete;
        };
        self.current_offset = offset;
        ContentStage::Parse
    }
    /// True means more work remains. The holder/parser are separate borrows
    /// instead of the source helper's back-pointer to its owning holder.
    pub fn continue_parse(
        &mut self,
        holder: &mut CpdfPageObjectHolder,
        parser: &mut CpdfStreamContentParser,
        mut type3_char: Option<&mut CpdfType3Char>,
        mut pause: Option<&mut dyn PauseIndicatorIface>,
    ) -> bool {
        while self.stage == ContentStage::GetContent {
            self.stage = self.get_content();
            if pause
                .as_mut()
                .is_some_and(|pause| pause.need_to_pause_now())
            {
                return true;
            }
        }
        if self.stage == ContentStage::PrepareContent {
            self.stage = self.prepare_content();
        }
        while self.stage == ContentStage::Parse {
            self.stage = self.parse(holder, parser);
            if pause
                .as_mut()
                .is_some_and(|pause| pause.need_to_pause_now())
            {
                return true;
            }
        }
        if self.stage == ContentStage::CheckClip {
            if let Some(glyph) = type3_char.as_mut() {
                glyph.initialize_from_stream_data(parser.is_colored(), parser.type3_data());
            }
            Self::check_clip(holder);
            self.stage = ContentStage::Complete;
        }
        debug_assert_eq!(self.stage, ContentStage::Complete);
        false
    }
    pub fn parse_all(
        &mut self,
        holder: &mut CpdfPageObjectHolder,
        parser: &mut CpdfStreamContentParser,
    ) {
        self.parse_all_for_type3(holder, parser, None);
    }
    pub fn parse_all_for_type3(
        &mut self,
        holder: &mut CpdfPageObjectHolder,
        parser: &mut CpdfStreamContentParser,
        type3_char: Option<&mut CpdfType3Char>,
    ) {
        holder.start_parse();
        assert!(!self.continue_parse(holder, parser, type3_char, None));
        holder.finish_parse(parser.take_all_ctms());
    }
    pub fn check_clip(holder: &mut CpdfPageObjectHolder) {
        for index in 0..holder.page_object_count() {
            let Some(object) = holder.page_object_by_index_mut(index) else {
                continue;
            };
            if !object.base().is_active() || object.object_type() == PageObjectType::Shading {
                continue;
            }
            let clip = object.base().clip_path();
            if !clip.has_ref() || clip.path_count() != 1 || clip.text_count() > 0 {
                continue;
            }
            let path = clip.path(0);
            if !path.is_rect() {
                continue;
            }
            let point0 = path.get_point(0);
            let point2 = path.get_point(2);
            let rect = CfxFloatRect::new(point0.x, point0.y, point2.x, point2.y);
            if rect.contains_rect(object.base().rect()) {
                object.base_mut().mutable_clip_path().set_null();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_pageobject::PageObject;
    use crate::fpdfapi::page::cpdf_path::CpdfPath;
    use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fxcrt::fx_coordinates::CfxMatrix;
    use crate::fxge::cfx_fillrenderoptions::FillType;

    #[test]
    fn joins_streams_with_separator_and_offsets() {
        let first = retain_object(CpdfStream::from_bytes(b"q"));
        let second = retain_object(CpdfStream::from_bytes(b"Q"));
        let mut parser = CpdfContentParser::from_streams(vec![Some(first), Some(second)]);
        assert_eq!(parser.stage(), ContentStage::GetContent);
        assert_eq!(parser.get_content(), ContentStage::GetContent);
        assert_eq!(parser.get_content(), ContentStage::PrepareContent);
        assert_eq!(parser.prepare_content(), ContentStage::Parse);
        assert_eq!(&*parser.data(), b"q Q ");
        assert_eq!(parser.stream_segment_offsets(), [0, 2]);
    }

    #[test]
    fn removes_redundant_single_rect_clip() {
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        let mut object = CpdfPathObject::new(0);
        object
            .base_mut()
            .set_rect(CfxFloatRect::new(2.0, 2.0, 3.0, 3.0));
        let mut clip = CpdfPath::new();
        clip.append_rect(0.0, 0.0, 10.0, 10.0);
        object
            .base_mut()
            .mutable_clip_path()
            .append_path(clip, FillType::Winding);
        holder.append_page_object(Box::new(object));
        CpdfContentParser::check_clip(&mut holder);
        assert!(!holder
            .page_object_by_index(0)
            .unwrap()
            .base()
            .clip_path()
            .has_ref());

        let mut object = CpdfPathObject::new(0);
        object
            .base_mut()
            .set_rect(CfxFloatRect::new(2.0, 2.0, 3.0, 3.0));
        let mut path = CpdfPath::new();
        path.append_rect(0.0, 0.0, 10.0, 10.0);
        let clip = object.base_mut().mutable_clip_path();
        clip.append_path(path, FillType::Winding);
        clip.append_texts(&mut vec![Box::new(
            crate::fpdfapi::page::cpdf_textobject::CpdfTextObject::new(0),
        )]);
        holder.append_page_object(Box::new(object));
        CpdfContentParser::check_clip(&mut holder);
        assert!(holder
            .page_object_by_index(1)
            .unwrap()
            .base()
            .clip_path()
            .has_ref());
    }

    #[test]
    fn retained_content_preserves_storage_and_native_pause_boundaries() {
        struct Pause;
        impl PauseIndicatorIface for Pause {
            fn need_to_pause_now(&mut self) -> bool {
                true
            }
        }
        let stream = retain_object(CpdfStream::from_bytes(b"0 0 1 1 re f"));
        let pointer = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_in_memory_raw_data()
            .as_ptr();
        let mut content = CpdfContentParser::from_stream(&stream);
        assert_eq!(content.data().as_ptr(), pointer);
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        holder.start_parse();
        assert!(content.continue_parse(&mut holder, &mut parser, None, Some(&mut Pause)));
        assert_eq!(content.stage(), ContentStage::Parse);
        assert_eq!(holder.page_object_count(), 1);
        assert!(content.continue_parse(&mut holder, &mut parser, None, Some(&mut Pause)));
        assert_eq!(content.stage(), ContentStage::CheckClip);
        assert!(!content.continue_parse(&mut holder, &mut parser, None, None));
        assert_eq!(content.stage(), ContentStage::Complete);
    }

    #[test]
    fn prepares_and_parses_path_content_into_holder() {
        let stream = retain_object(CpdfStream::from_bytes(b"0 0 10 20 re f"));
        let mut content = CpdfContentParser::from_streams(vec![Some(stream)]);
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        content.parse_all(&mut holder, &mut parser);
        assert_eq!(holder.page_object_count(), 1);
        assert_eq!(
            holder.parse_state(),
            crate::fpdfapi::page::cpdf_pageobjectholder::ParseState::Parsed
        );
        assert_eq!(
            holder.page_object_by_index(0).unwrap().base().rect(),
            CfxFloatRect::new(0.0, 0.0, 10.0, 20.0)
        );
    }
}
