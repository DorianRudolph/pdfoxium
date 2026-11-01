// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Token and direct-object reading from `core/fpdfapi/page/cpdf_streamparser.{h,cpp}`.
//! Color component lookup is supplied by document page data.

use std::cell::RefCell;
use std::rc::Weak;

use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_name::CpdfName;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfapi::parser::cpdf_syntax_parser::PARSER_MAX_RECURSION_DEPTH;
use crate::fpdfapi::parser::fpdf_parser_decode::{
    a85_decode, brotli_decode, flate_or_lzw_decode, hex_decode, run_length_decode,
};
use crate::fpdfapi::parser::fpdf_parser_utility::{
    pdf_char_is_delimiter, pdf_char_is_line_ending, pdf_char_is_numeric, pdf_char_is_other,
    pdf_char_is_whitespace, pdf_name_decode,
};
use crate::fxcodec::brotli::get_brotli_enabled;
use crate::fxcodec::jpeg::JpegScanlineDecoder;
use crate::fxcodec::scanlinedecoder::ScanlineDecoder;
use crate::fxcrt::bytestring_pool::ByteStringPool;
use crate::fxge::calculate_pitch::calculate_pitch8;

const MAX_WORD_LENGTH: usize = 255;
const MAX_STRING_LENGTH: usize = 32767;

fn decode_all_scanlines(mut decoder: impl ScanlineDecoder) -> Option<u32> {
    let width = decoder.get_width();
    let height = decoder.get_height();
    if width <= 0 || height <= 0 {
        return None;
    }
    let pitch = calculate_pitch8(
        decoder.get_bpc() as u32,
        decoder.count_comps() as u32,
        width,
    )?;
    if pitch.checked_mul(height as u32)? == 0 {
        return None;
    }
    for row in 0..height {
        if decoder.get_scanline(row).is_empty() {
            break;
        }
    }
    Some(decoder.get_src_offset())
}

fn fax_stream_length(
    data: &[u8],
    width: i32,
    height: i32,
    params: Option<&CpdfDictionary>,
) -> Option<u32> {
    decode_all_scanlines(
        crate::fpdfapi::parser::fpdf_parser_decode::create_fax_decoder(
            data, width, height, params,
        )?,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElementType {
    EndOfData,
    Number,
    Keyword,
    Name,
    Other,
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{CpdfStreamParser, ElementType};
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_parser::{CpdfParser, ParserError};
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    #[test]
    fn reads_pdfium_content_stream_operands_and_keywords() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
        ));
        let mut document = CpdfParser::new();
        assert_eq!(
            document.start_parse(
                Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
                b""
            ),
            ParserError::Success
        );
        let object = document.parse_indirect_object(7).unwrap();
        let borrowed = object.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
        let mut acc = CpdfStreamAcc::new(stream);
        acc.load_all_data_filtered();
        let mut parser = CpdfStreamParser::new(acc.get_span());
        assert_eq!(parser.parse_next_element(), ElementType::Keyword);
        assert_eq!(parser.get_word(), b"BT");
        assert_eq!(parser.parse_next_element(), ElementType::Number);
        assert_eq!(parser.get_word(), b"20");
        let mut saw_hello = false;
        let mut saw_text_operator = false;
        while parser.parse_next_element() != ElementType::EndOfData {
            if parser.get_word() == b"Tj" {
                saw_text_operator = true;
            }
            if let Some(object) = parser.get_object() {
                if object.borrow().get_string() == b"Hello, world!" {
                    saw_hello = true;
                }
            }
        }
        assert!(saw_hello);
        assert!(saw_text_operator);
    }

    #[test]
    fn reads_nested_content_objects_and_odd_hex_strings() {
        let mut parser = CpdfStreamParser::new(b"<< /A [1 (x) << /B true >>] /C <1A2> >>");
        let object = parser.read_next_object(false, false, 0).unwrap();
        let borrowed = object.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        let array = dict.get_array_for(b"A").unwrap();
        let borrowed = array.borrow();
        assert_eq!(
            borrowed
                .as_any()
                .downcast_ref::<CpdfArray>()
                .unwrap()
                .size(),
            3
        );
        assert_eq!(
            dict.get_string_for(b"C").unwrap().borrow().get_string(),
            [0x1a, 0x20]
        );
    }

    #[test]
    fn extracts_unfiltered_and_hex_inline_image_bytes() {
        for (input, filter, expected) in [
            (&b" \x01\x02 EI"[..], None, &b"\x01\x02"[..]),
            (&b" 1A2> EI"[..], Some(&b"ASCIIHexDecode"[..]), &b"1A2>"[..]),
        ] {
            let dict = retain_object(CpdfDictionary::new());
            {
                let mut borrowed = dict.borrow_mut();
                let dict = borrowed
                    .as_any_mut()
                    .downcast_mut::<CpdfDictionary>()
                    .unwrap();
                dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(16));
                dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
                if let Some(filter) = filter {
                    dict.set_new_name_for(b"Filter", filter);
                }
            }
            let mut parser = CpdfStreamParser::new(input);
            let stream = parser.read_inline_stream(dict, None).unwrap();
            let borrowed = stream.borrow();
            let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
            assert_eq!(stream.get_in_memory_raw_data(), expected);
            assert_eq!(parser.parse_next_element(), ElementType::Keyword);
            assert_eq!(parser.get_word(), b"EI");
        }
    }

    #[test]
    fn finds_jpeg_end_and_keeps_following_content_operator() {
        let jpeg = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/mona_lisa.jpg"
        ));
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(120));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(120));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        dict.set_new_name_for(b"Filter", b"DCTDecode");
        let mut content = vec![b' '];
        content.extend_from_slice(jpeg);
        content.extend_from_slice(b" EI q");
        let mut parser = CpdfStreamParser::new(&content);
        let stream = parser
            .read_inline_stream(retain_object(dict), Some(3))
            .unwrap();
        let borrowed = stream.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
        assert_eq!(stream.get_in_memory_raw_data(), jpeg);
        assert_eq!(parser.parse_next_element(), ElementType::Keyword);
        assert_eq!(parser.get_word(), b"EI");
        assert_eq!(parser.parse_next_element(), ElementType::Keyword);
        assert_eq!(parser.get_word(), b"q");
    }

    #[test]
    fn unresolved_inline_color_space_uses_three_components() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_name_for(b"ColorSpace", b"Unknown");
        let mut content = vec![b' '];
        content.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        content.extend_from_slice(b" EI");
        let mut parser = CpdfStreamParser::new(&content);
        let stream = parser
            .read_inline_stream(retain_object(dict), None)
            .unwrap();
        let borrowed = stream.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
        assert_eq!(stream.get_in_memory_raw_data(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(parser.parse_next_element(), ElementType::Keyword);
        assert_eq!(parser.get_word(), b"EI");
    }

    #[test]
    fn fax_inline_length_stops_after_decoded_row() {
        let mut params = CpdfDictionary::new();
        params.set_new_for(b"K", CpdfScalarObject::number_from_signed(-1));
        params.set_new_for(b"Columns", CpdfScalarObject::number_from_signed(8));
        params.set_new_for(b"Rows", CpdfScalarObject::number_from_signed(1));
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(8));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        dict.set_new_name_for(b"Filter", b"CCITTFaxDecode");
        dict.set_new_for(b"DecodeParms", params);
        let mut parser = CpdfStreamParser::new(b" \x80 EI q");
        let stream = parser
            .read_inline_stream(retain_object(dict), None)
            .unwrap();
        let borrowed = stream.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
        assert_eq!(stream.get_in_memory_raw_data(), &[0x80]);
        assert_eq!(parser.parse_next_element(), ElementType::Keyword);
        assert_eq!(parser.get_word(), b"EI");
        assert_eq!(parser.parse_next_element(), ElementType::Keyword);
        assert_eq!(parser.get_word(), b"q");
    }
}

pub struct CpdfStreamParser<'a> {
    pos: usize,
    word_size: usize,
    pool: Weak<RefCell<ByteStringPool>>,
    last_obj: Option<ObjectRef>,
    buf: &'a [u8],
    word_buffer: [u8; MAX_WORD_LENGTH + 1],
}

impl<'a> CpdfStreamParser<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self::with_pool(buf, Weak::new())
    }

    pub fn with_pool(buf: &'a [u8], pool: Weak<RefCell<ByteStringPool>>) -> Self {
        Self {
            pos: 0,
            word_size: 0,
            pool,
            last_obj: None,
            buf,
            word_buffer: [0; MAX_WORD_LENGTH + 1],
        }
    }

    pub fn get_word(&self) -> &[u8] {
        &self.word_buffer[..self.word_size]
    }
    pub(crate) fn byte_string_pool(&self) -> Weak<RefCell<ByteStringPool>> {
        self.pool.clone()
    }
    pub fn get_pos(&self) -> u32 {
        self.pos as u32
    }
    pub fn set_pos(&mut self, pos: u32) {
        self.pos = pos as usize;
    }
    pub fn get_object(&self) -> Option<ObjectRef> {
        self.last_obj.clone()
    }

    /// `color_components` is the color space component count resolved by the
    /// document page data. `None` matches PDFium's absent color space object.
    pub fn read_inline_stream(
        &mut self,
        dict: ObjectRef,
        color_components: Option<u32>,
    ) -> Option<ObjectRef> {
        let mut stream_span = self.buf.get(self.pos..)?;
        if stream_span.is_empty() {
            return None;
        }
        if pdf_char_is_whitespace(stream_span[0]) {
            self.pos += 1;
            stream_span = &stream_span[1..];
            if stream_span.is_empty() {
                return None;
            }
        }
        let (decoder, params, width, height, bpc, components) = {
            let borrowed = dict.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            let filter = dict.get_direct_object_for(b"Filter");
            let (decoder, params) = if let Some(filter) = filter {
                let borrowed = filter.borrow();
                if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                    let name = array.get_byte_string_at(0);
                    let params = dict.get_array_for(b"DecodeParms").and_then(|value| {
                        let borrowed = value.borrow();
                        borrowed
                            .as_any()
                            .downcast_ref::<CpdfArray>()?
                            .get_dict_at(0)
                    });
                    (name, params)
                } else {
                    (borrowed.get_string(), dict.get_dict_for(b"DecodeParms"))
                }
            } else {
                (Vec::new(), None)
            };
            let has_color_space = dict.get_direct_object_for(b"ColorSpace").is_some();
            (
                decoder,
                params,
                dict.get_integer_for(b"Width", 0),
                dict.get_integer_for(b"Height", 0),
                if has_color_space {
                    dict.get_integer_for(b"BitsPerComponent", 0) as u32
                } else {
                    1
                },
                if has_color_space {
                    color_components.unwrap_or(3)
                } else {
                    1
                },
            )
        };
        let pitch = calculate_pitch8(bpc, components, width)?;
        let original_size = pitch.checked_mul(height as u32)?;
        let mut actual_size;
        let data;
        if decoder.is_empty() {
            actual_size = (original_size as usize).min(stream_span.len());
            data = stream_span[..actual_size].to_vec();
        } else {
            let params_borrowed = params.as_ref().map(|value| value.borrow());
            let params = params_borrowed
                .as_ref()
                .and_then(|value| value.as_any().downcast_ref::<CpdfDictionary>());
            let consumed = match crate::fxcrt::bytestring::c_str(&decoder) {
                b"FlateDecode" => {
                    flate_or_lzw_decode(false, stream_span, params, original_size).bytes_consumed
                }
                b"LZWDecode" => flate_or_lzw_decode(true, stream_span, params, 0).bytes_consumed,
                b"ASCII85Decode" => a85_decode(stream_span).bytes_consumed,
                b"ASCIIHexDecode" => hex_decode(stream_span).bytes_consumed,
                b"RunLengthDecode" => run_length_decode(stream_span).bytes_consumed,
                b"DCTDecode" => decode_all_scanlines(JpegScanlineDecoder::create(
                    stream_span,
                    width as u32,
                    height as u32,
                    0,
                    params.is_none_or(|params| params.get_integer_for(b"ColorTransform", 1) != 0),
                    1,
                )?)?,
                b"CCITTFaxDecode" => fax_stream_length(stream_span, width, height, params)?,
                b"BrotliDecode" if get_brotli_enabled() => {
                    brotli_decode(stream_span, params, original_size).bytes_consumed
                }
                _ => return None,
            };
            if consumed > i32::MAX as u32 || consumed as usize > stream_span.len() {
                return None;
            }
            actual_size = consumed as usize;
            let saved_pos = self.pos;
            self.pos = self.pos.checked_add(actual_size)?;
            let scan = loop {
                let iteration_pos = self.pos;
                let token = self.parse_next_element();
                if token == ElementType::EndOfData {
                    break None;
                }
                if token == ElementType::Keyword && self.get_word() == b"EI" {
                    break Some(());
                }
                let Some(next_size) = actual_size.checked_add(self.pos - iteration_pos) else {
                    break None;
                };
                actual_size = next_size;
            };
            self.pos = saved_pos;
            scan?;
            if actual_size > stream_span.len() {
                return None;
            }
            data = stream_span[..actual_size].to_vec();
        }
        self.pos = self.pos.checked_add(actual_size)?;
        {
            let mut borrowed = dict.borrow_mut();
            borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()?
                .set_new_for(
                    b"Length",
                    CpdfScalarObject::number_from_signed(actual_size as i32),
                );
        }
        Some(retain_object(CpdfStream::from_data_and_dict(data, dict)))
    }

    fn in_bounds(&self) -> bool {
        self.pos < self.buf.len()
    }
    fn push_word(&mut self, ch: u8) {
        if self.word_size < MAX_WORD_LENGTH {
            self.word_buffer[self.word_size] = ch;
            self.word_size += 1;
        }
    }

    pub fn parse_next_element(&mut self) -> ElementType {
        self.last_obj = None;
        self.word_size = 0;
        if !self.in_bounds() {
            return ElementType::EndOfData;
        }
        let mut ch = self.buf[self.pos];
        self.pos += 1;
        loop {
            while pdf_char_is_whitespace(ch) {
                if !self.in_bounds() {
                    return ElementType::EndOfData;
                }
                ch = self.buf[self.pos];
                self.pos += 1;
            }
            if ch != b'%' {
                break;
            }
            loop {
                if !self.in_bounds() {
                    return ElementType::EndOfData;
                }
                ch = self.buf[self.pos];
                self.pos += 1;
                if pdf_char_is_line_ending(ch) {
                    break;
                }
            }
        }
        if pdf_char_is_delimiter(ch) && ch != b'/' {
            self.pos -= 1;
            self.last_obj = self.read_next_object(false, false, 0);
            return ElementType::Other;
        }
        let mut is_number = true;
        loop {
            self.push_word(ch);
            if !pdf_char_is_numeric(ch) {
                is_number = false;
            }
            if !self.in_bounds() {
                break;
            }
            ch = self.buf[self.pos];
            self.pos += 1;
            if pdf_char_is_delimiter(ch) || pdf_char_is_whitespace(ch) {
                self.pos -= 1;
                break;
            }
        }
        self.word_buffer[self.word_size] = 0;
        if is_number {
            return ElementType::Number;
        }
        if self.word_buffer[0] == b'/' {
            return ElementType::Name;
        }
        match self.get_word() {
            b"true" => {
                self.last_obj = Some(retain_object(CpdfScalarObject::boolean(true)));
                ElementType::Other
            }
            b"false" => {
                self.last_obj = Some(retain_object(CpdfScalarObject::boolean(false)));
                ElementType::Other
            }
            b"null" => {
                self.last_obj = Some(retain_object(CpdfScalarObject::null()));
                ElementType::Other
            }
            _ => ElementType::Keyword,
        }
    }

    pub fn read_next_object(
        &mut self,
        allow_nested_array: bool,
        in_array: bool,
        recursion_level: u32,
    ) -> Option<ObjectRef> {
        // Read a word even when the recursion limit is exceeded, as PDFium does.
        let is_number = self.get_next_word();
        if self.word_size == 0 || recursion_level > PARSER_MAX_RECURSION_DEPTH {
            return None;
        }
        if is_number {
            self.word_buffer[self.word_size] = 0;
            return Some(retain_object(CpdfScalarObject::number_from_string(
                self.get_word(),
            )));
        }
        let first = self.word_buffer[0];
        if first == b'<' && self.word_size > 1 {
            let mut dict = CpdfDictionary::with_pool(self.pool.clone());
            loop {
                self.get_next_word();
                if self.word_size == 2 && self.word_buffer[0] == b'>' {
                    break;
                }
                if self.word_size == 0 || self.word_buffer[0] != b'/' {
                    return None;
                }
                let key = pdf_name_decode(&self.get_word()[1..]);
                let object = self.read_next_object(true, in_array, recursion_level + 1)?;
                dict.set_for(&key, Some(object));
            }
            return Some(retain_object(dict));
        }
        if first == b'[' {
            if !allow_nested_array && in_array {
                return None;
            }
            let mut array = CpdfArray::new();
            loop {
                let object = self.read_next_object(allow_nested_array, true, recursion_level + 1);
                if let Some(object) = object {
                    array.append(object);
                    continue;
                }
                if self.word_size == 0 || self.word_buffer[0] == b']' {
                    break;
                }
            }
            return Some(retain_object(array));
        }
        self.read_leaf_object()
    }

    fn read_leaf_object(&mut self) -> Option<ObjectRef> {
        match self.word_buffer[0] {
            b'/' => {
                let name = pdf_name_decode(&self.get_word()[1..]);
                let value = self.pool.upgrade().map_or_else(
                    || CpdfName::new(None, &name),
                    |pool| CpdfName::new(Some(&mut pool.borrow_mut()), &name),
                );
                Some(retain_object(value))
            }
            b'(' => {
                let bytes = self.read_string();
                let value = self.pool.upgrade().map_or_else(
                    || CpdfString::from_bytes(None, &bytes),
                    |pool| CpdfString::from_bytes(Some(&mut pool.borrow_mut()), &bytes),
                );
                Some(retain_object(value))
            }
            b'<' => {
                let bytes = self.read_hex_string();
                let value = self.pool.upgrade().map_or_else(
                    || CpdfString::from_hex_bytes(None, &bytes),
                    |pool| CpdfString::from_hex_bytes(Some(&mut pool.borrow_mut()), &bytes),
                );
                Some(retain_object(value))
            }
            _ => match self.get_word() {
                b"false" => Some(retain_object(CpdfScalarObject::boolean(false))),
                b"true" => Some(retain_object(CpdfScalarObject::boolean(true))),
                b"null" => Some(retain_object(CpdfScalarObject::null())),
                _ => None,
            },
        }
    }

    fn get_next_word(&mut self) -> bool {
        self.word_size = 0;
        if !self.in_bounds() {
            return true;
        }
        let mut ch = self.buf[self.pos];
        self.pos += 1;
        loop {
            while pdf_char_is_whitespace(ch) {
                if !self.in_bounds() {
                    return true;
                }
                ch = self.buf[self.pos];
                self.pos += 1;
            }
            if ch != b'%' {
                break;
            }
            loop {
                if !self.in_bounds() {
                    return true;
                }
                ch = self.buf[self.pos];
                self.pos += 1;
                if pdf_char_is_line_ending(ch) {
                    break;
                }
            }
        }
        if pdf_char_is_delimiter(ch) {
            self.push_word(ch);
            if ch == b'/' {
                loop {
                    if !self.in_bounds() {
                        return false;
                    }
                    ch = self.buf[self.pos];
                    self.pos += 1;
                    if !pdf_char_is_other(ch) && !pdf_char_is_numeric(ch) {
                        self.pos -= 1;
                        return false;
                    }
                    self.push_word(ch);
                }
            } else if ch == b'<' || ch == b'>' {
                if !self.in_bounds() {
                    return false;
                }
                let next = self.buf[self.pos];
                self.pos += 1;
                if next == ch {
                    self.push_word(next);
                } else {
                    self.pos -= 1;
                }
            }
            return false;
        }
        let mut is_number = true;
        loop {
            self.push_word(ch);
            if !pdf_char_is_numeric(ch) {
                is_number = false;
            }
            if !self.in_bounds() {
                return is_number;
            }
            ch = self.buf[self.pos];
            self.pos += 1;
            if pdf_char_is_delimiter(ch) || pdf_char_is_whitespace(ch) {
                self.pos -= 1;
                break;
            }
        }
        is_number
    }

    fn read_string(&mut self) -> Vec<u8> {
        if !self.in_bounds() {
            return Vec::new();
        }
        let mut result = Vec::new();
        let mut par_level = 0;
        let mut status = 0;
        let mut escape_code = 0u16;
        let mut ch = self.buf[self.pos];
        self.pos += 1;
        loop {
            match status {
                0 => match ch {
                    b')' => {
                        if par_level == 0 {
                            result.truncate(MAX_STRING_LENGTH);
                            return result;
                        }
                        par_level -= 1;
                        result.push(b')');
                    }
                    b'(' => {
                        par_level += 1;
                        result.push(b'(');
                    }
                    b'\\' => status = 1,
                    _ => result.push(ch),
                },
                1 => {
                    if (b'0'..=b'7').contains(&ch) {
                        escape_code = u16::from(ch - b'0');
                        status = 2;
                    } else if ch == b'\r' {
                        status = 4;
                    } else {
                        match ch {
                            b'\n' => {}
                            b'n' => result.push(b'\n'),
                            b'r' => result.push(b'\r'),
                            b't' => result.push(b'\t'),
                            b'b' => result.push(8),
                            b'f' => result.push(12),
                            _ => result.push(ch),
                        }
                        status = 0;
                    }
                }
                2 | 3 => {
                    if (b'0'..=b'7').contains(&ch) {
                        escape_code = escape_code * 8 + u16::from(ch - b'0');
                        if status == 2 {
                            status = 3;
                        } else {
                            result.push(escape_code as u8);
                            status = 0;
                        }
                    } else {
                        result.push(escape_code as u8);
                        status = 0;
                        continue;
                    }
                }
                4 => {
                    status = 0;
                    if ch != b'\n' {
                        continue;
                    }
                }
                _ => unreachable!(),
            }
            if !self.in_bounds() {
                result.truncate(MAX_STRING_LENGTH);
                return result;
            }
            ch = self.buf[self.pos];
            self.pos += 1;
        }
    }

    fn read_hex_string(&mut self) -> Vec<u8> {
        if !self.in_bounds() {
            return Vec::new();
        }
        let mut result = Vec::new();
        let mut first = true;
        let mut code = 0u8;
        while self.in_bounds() {
            let ch = self.buf[self.pos];
            self.pos += 1;
            if ch == b'>' {
                break;
            }
            let value = match ch {
                b'0'..=b'9' => ch - b'0',
                b'a'..=b'f' => ch - b'a' + 10,
                b'A'..=b'F' => ch - b'A' + 10,
                _ => continue,
            };
            if first {
                code = value << 4;
            } else {
                result.push(code + value);
            }
            first = !first;
        }
        if !first {
            result.push(code);
        }
        result.truncate(MAX_STRING_LENGTH);
        result
    }
}

#[cfg(test)]
#[path = "cpdf_streamparser_unittest.rs"]
mod upstream_tests;
