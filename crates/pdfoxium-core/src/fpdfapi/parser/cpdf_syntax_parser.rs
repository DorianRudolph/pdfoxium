// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_syntax_parser.{h,cpp}`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::cpdf_array::CpdfArray;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_indirect_object_holder::HolderRef;
use super::cpdf_name::CpdfName;
use super::cpdf_object::{retain_object, ObjectRef};
use super::cpdf_read_validator::{CpdfReadValidator, ScopedSession};
use super::cpdf_reference::CpdfReference;
use super::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use super::cpdf_stream::CpdfStream;
use super::cpdf_stream::FILE_BUF_SIZE;
use super::cpdf_string::CpdfString;
use super::fpdf_parser_utility::{
    pdf_char_is_delimiter, pdf_char_is_line_ending, pdf_char_is_numeric, pdf_char_is_other,
    pdf_char_is_whitespace, pdf_name_decode,
};
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};
use crate::fxcrt::fx_types::FxFileSize;

pub const PARSER_MAX_RECURSION_DEPTH: u32 = 64;

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ParseType {
    Strict,
    Loose,
}

pub struct WordResult {
    pub word: Vec<u8>,
    pub is_number: bool,
}

#[derive(Clone, Copy)]
enum ReadStatus {
    Normal,
    Backslash,
    Octal,
    FinishOctal,
    CarriageReturn,
}

#[derive(Clone, Copy)]
enum EofState {
    Initial,
    NonPercent,
    Percent,
    E,
    O,
    F,
    Invalid,
}

pub struct CpdfSyntaxParser {
    file_access: Rc<CpdfReadValidator>,
    header_offset: FxFileSize,
    file_len: FxFileSize,
    pos: FxFileSize,
    file_buf: Vec<u8>,
    buf_offset: FxFileSize,
    word_size: usize,
    read_buffer_size: u32,
    word_buffer: [u8; 257],
    trailer_ends: Option<Weak<RefCell<Vec<u32>>>>,
    recursion_depth: u32,
}

impl CpdfSyntaxParser {
    pub fn from_file(file: Rc<dyn SeekableReadStream>) -> Self {
        Self::new(Rc::new(CpdfReadValidator::new(file, None)), 0)
    }

    pub fn create_for_testing(file: Rc<dyn SeekableReadStream>, header_offset: FxFileSize) -> Self {
        Self::new(Rc::new(CpdfReadValidator::new(file, None)), header_offset)
    }

    pub fn new(file_access: Rc<CpdfReadValidator>, header_offset: FxFileSize) -> Self {
        let file_len = file_access.get_size();
        assert!(header_offset <= file_len);
        Self {
            file_access,
            header_offset,
            file_len,
            pos: 0,
            file_buf: Vec::new(),
            buf_offset: 0,
            word_size: 0,
            read_buffer_size: FILE_BUF_SIZE as u32,
            word_buffer: [0; 257],
            trailer_ends: None,
            recursion_depth: 0,
        }
    }

    pub fn get_validator(&self) -> Rc<CpdfReadValidator> {
        Rc::clone(&self.file_access)
    }
    pub fn set_read_buffer_size(&mut self, size: u32) {
        self.read_buffer_size = size;
    }
    pub fn get_pos(&self) -> FxFileSize {
        self.pos
    }
    pub fn set_pos(&mut self, pos: FxFileSize) {
        assert!(pos >= 0);
        self.pos = pos.min(self.file_len);
    }
    pub fn get_document_size(&self) -> FxFileSize {
        self.file_len - self.header_offset
    }
    pub fn set_trailer_ends(&mut self, ends: Option<&Rc<RefCell<Vec<u32>>>>) {
        self.trailer_ends = ends.map(Rc::downgrade);
    }

    fn is_position_read(&self, pos: FxFileSize) -> bool {
        self.buf_offset <= pos
            && pos
                < self
                    .buf_offset
                    .saturating_add(self.file_buf.len() as FxFileSize)
    }

    fn read_block_at(&mut self, read_pos: FxFileSize) -> bool {
        if read_pos < 0 || read_pos >= self.file_len {
            return false;
        }
        let read_size = self.read_buffer_size as FxFileSize;
        let read_size = if read_pos
            .checked_add(read_size)
            .is_none_or(|end| end > self.file_len)
        {
            self.file_len - read_pos
        } else {
            read_size
        };
        let Ok(read_size) = usize::try_from(read_size) else {
            return false;
        };
        self.file_buf.resize(read_size, 0);
        if !self
            .file_access
            .read_block_at_offset(&mut self.file_buf, read_pos)
        {
            self.file_buf.clear();
            return false;
        }
        self.buf_offset = read_pos;
        true
    }

    pub fn get_next_char(&mut self) -> Option<u8> {
        let absolute = self.pos.checked_add(self.header_offset)?;
        if absolute < 0 || absolute >= self.file_len {
            return None;
        }
        if !self.is_position_read(absolute) && !self.read_block_at(absolute) {
            return None;
        }
        let index = usize::try_from(absolute - self.buf_offset).ok()?;
        let ch = *self.file_buf.get(index)?;
        self.pos += 1;
        Some(ch)
    }

    pub fn get_char_at(&mut self, pos: FxFileSize) -> Option<u8> {
        let saved = self.pos;
        self.pos = pos;
        let result = self.get_next_char();
        self.pos = saved;
        result
    }

    pub fn get_char_at_backward(&mut self, pos: FxFileSize) -> Option<u8> {
        let absolute = pos.checked_add(self.header_offset)?;
        if absolute < 0 || absolute >= self.file_len {
            return None;
        }
        if !self.is_position_read(absolute) {
            let block_start = if absolute >= FILE_BUF_SIZE as FxFileSize {
                absolute - FILE_BUF_SIZE as FxFileSize + 1
            } else {
                0
            };
            if !self.read_block_at(block_start) || !self.is_position_read(absolute) {
                return None;
            }
        }
        self.file_buf
            .get(usize::try_from(absolute - self.buf_offset).ok()?)
            .copied()
    }

    pub fn read_block(&mut self, buffer: &mut [u8]) -> bool {
        let Some(absolute) = self.pos.checked_add(self.header_offset) else {
            return false;
        };
        if !self.file_access.read_block_at_offset(buffer, absolute) {
            return false;
        }
        self.pos += buffer.len() as FxFileSize;
        true
    }

    /// Complete the creator's split ReadBlock operation after the retained
    /// validator has read without a document borrow. Increment the current
    /// cursor, as native ReadBlock does even if a callback changed it.
    pub(crate) fn advance_position_after_save_read(&mut self, size: usize) {
        self.pos = self
            .pos
            .checked_add(FxFileSize::try_from(size).expect("read size"))
            .expect("syntax read position overflow");
    }

    fn get_next_word_internal(&mut self) -> bool {
        self.word_size = 0;
        let mut is_number = true;
        self.to_next_word();
        let Some(mut ch) = self.get_next_char() else {
            return is_number;
        };
        if pdf_char_is_delimiter(ch) {
            is_number = false;
            self.word_buffer[self.word_size] = ch;
            self.word_size += 1;
            if ch == b'/' {
                loop {
                    let Some(next) = self.get_next_char() else {
                        return is_number;
                    };
                    if !pdf_char_is_other(next) && !pdf_char_is_numeric(next) {
                        self.pos -= 1;
                        return is_number;
                    }
                    if self.word_size < self.word_buffer.len() - 1 {
                        self.word_buffer[self.word_size] = next;
                        self.word_size += 1;
                    }
                }
            }
            if ch == b'<' || ch == b'>' {
                let Some(next) = self.get_next_char() else {
                    return is_number;
                };
                if next == ch {
                    self.word_buffer[self.word_size] = next;
                    self.word_size += 1;
                } else {
                    self.pos -= 1;
                }
            }
            return is_number;
        }
        loop {
            if self.word_size < self.word_buffer.len() - 1 {
                self.word_buffer[self.word_size] = ch;
                self.word_size += 1;
            }
            if !pdf_char_is_numeric(ch) {
                is_number = false;
            }
            let Some(next) = self.get_next_char() else {
                return is_number;
            };
            ch = next;
            if pdf_char_is_delimiter(ch) || pdf_char_is_whitespace(ch) {
                self.pos -= 1;
                return is_number;
            }
        }
    }

    pub fn get_next_word(&mut self) -> WordResult {
        let _session = ScopedSession::new(self.get_validator());
        let is_number = self.get_next_word_internal();
        let word = if self.file_access.has_read_problems() {
            Vec::new()
        } else {
            self.word_buffer[..self.word_size].to_vec()
        };
        WordResult { word, is_number }
    }

    pub fn peek_next_word(&mut self) -> Vec<u8> {
        let saved = self.pos;
        let word = self.get_next_word().word;
        self.pos = saved;
        word
    }

    pub fn get_keyword(&mut self) -> Vec<u8> {
        self.get_next_word().word
    }

    pub fn to_next_line(&mut self) {
        while let Some(ch) = self.get_next_char() {
            if ch == b'\n' {
                break;
            }
            if ch == b'\r' {
                if let Some(next) = self.get_next_char() {
                    if next != b'\n' {
                        self.pos -= 1;
                    }
                }
                break;
            }
        }
    }

    pub fn to_next_word(&mut self) {
        if self.trailer_ends.as_ref().and_then(Weak::upgrade).is_some() {
            self.recording_to_next_word();
            return;
        }
        let Some(mut ch) = self.get_next_char() else {
            return;
        };
        loop {
            while pdf_char_is_whitespace(ch) {
                let Some(next) = self.get_next_char() else {
                    return;
                };
                ch = next;
            }
            if ch != b'%' {
                break;
            }
            loop {
                let Some(next) = self.get_next_char() else {
                    return;
                };
                ch = next;
                if pdf_char_is_line_ending(ch) {
                    break;
                }
            }
        }
        self.pos -= 1;
    }

    pub fn recording_to_next_word(&mut self) {
        let Some(ends) = self.trailer_ends.as_ref().and_then(Weak::upgrade) else {
            return;
        };
        let mut state = EofState::Initial;
        loop {
            let Some(mut ch) = self.get_next_char() else {
                return;
            };
            state = match state {
                EofState::Initial => {
                    if pdf_char_is_whitespace(ch) {
                        EofState::Initial
                    } else if ch == b'%' {
                        EofState::Percent
                    } else {
                        EofState::NonPercent
                    }
                }
                EofState::NonPercent => EofState::NonPercent,
                EofState::Percent => {
                    if ch == b'E' {
                        EofState::E
                    } else if ch == b'%' {
                        EofState::Percent
                    } else {
                        EofState::Invalid
                    }
                }
                EofState::E => {
                    if ch == b'O' {
                        EofState::O
                    } else {
                        EofState::Invalid
                    }
                }
                EofState::O => {
                    if ch == b'F' {
                        EofState::F
                    } else {
                        EofState::Invalid
                    }
                }
                EofState::F => {
                    if ch == b'\r' {
                        if let Some(next) = self.get_next_char() {
                            if next != b'\n' {
                                self.pos -= 1;
                            } else {
                                ch = next;
                            }
                        }
                    }
                    if ch == b'\r' || ch == b'\n' {
                        ends.borrow_mut().push(self.pos as u32);
                    }
                    EofState::Invalid
                }
                EofState::Invalid => EofState::Invalid,
            };
            if pdf_char_is_line_ending(ch) {
                state = EofState::Initial;
            }
            if matches!(state, EofState::NonPercent) {
                break;
            }
        }
        self.pos -= 1;
    }

    pub fn read_string(&mut self) -> Vec<u8> {
        let Some(mut ch) = self.get_next_char() else {
            return Vec::new();
        };
        let mut result = Vec::new();
        let mut par_level = 0;
        let mut status = ReadStatus::Normal;
        let mut escape_code = 0u32;
        loop {
            match status {
                ReadStatus::Normal => {
                    if ch == b')' {
                        if par_level == 0 {
                            return result;
                        }
                        par_level -= 1;
                    } else if ch == b'(' {
                        par_level += 1;
                    }
                    if ch == b'\\' {
                        status = ReadStatus::Backslash;
                    } else {
                        result.push(ch);
                    }
                }
                ReadStatus::Backslash => {
                    if (b'0'..=b'7').contains(&ch) {
                        escape_code = u32::from(ch - b'0');
                        status = ReadStatus::Octal;
                    } else if ch == b'\r' {
                        status = ReadStatus::CarriageReturn;
                    } else {
                        match ch {
                            b'n' => result.push(b'\n'),
                            b'r' => result.push(b'\r'),
                            b't' => result.push(b'\t'),
                            b'b' => result.push(8),
                            b'f' => result.push(12),
                            b'\n' => {}
                            _ => result.push(ch),
                        }
                        status = ReadStatus::Normal;
                    }
                }
                ReadStatus::Octal => {
                    if (b'0'..=b'7').contains(&ch) {
                        escape_code = escape_code * 8 + u32::from(ch - b'0');
                        status = ReadStatus::FinishOctal;
                    } else {
                        result.push(escape_code as u8);
                        status = ReadStatus::Normal;
                        continue;
                    }
                }
                ReadStatus::FinishOctal => {
                    status = ReadStatus::Normal;
                    if (b'0'..=b'7').contains(&ch) {
                        escape_code = escape_code * 8 + u32::from(ch - b'0');
                        result.push(escape_code as u8);
                    } else {
                        result.push(escape_code as u8);
                        continue;
                    }
                }
                ReadStatus::CarriageReturn => {
                    status = ReadStatus::Normal;
                    if ch != b'\n' {
                        continue;
                    }
                }
            }
            let Some(next) = self.get_next_char() else {
                break;
            };
            ch = next;
        }
        let _ = self.get_next_char();
        result
    }

    pub fn read_hex_string(&mut self) -> Vec<u8> {
        let Some(mut ch) = self.get_next_char() else {
            return Vec::new();
        };
        let mut result = Vec::new();
        let mut first = true;
        let mut code = 0;
        loop {
            if ch == b'>' {
                break;
            }
            if ch.is_ascii_hexdigit() {
                let value = ch.to_ascii_uppercase();
                let value = if value <= b'9' {
                    value - b'0'
                } else {
                    value - b'A' + 10
                };
                if first {
                    code = value * 16;
                } else {
                    code += value;
                    result.push(code);
                }
                first = !first;
            }
            let Some(next) = self.get_next_char() else {
                break;
            };
            ch = next;
        }
        if !first {
            result.push(code);
        }
        result
    }

    pub fn get_direct_num(&mut self) -> u32 {
        if !self.get_next_word_internal() {
            return 0;
        }
        crate::fxcrt::fx_system::atoui(&self.word_buffer[..self.word_size])
    }

    pub fn get_object_body(&mut self, holder: Option<&HolderRef>) -> Option<ObjectRef> {
        let _session = ScopedSession::new(self.get_validator());
        let result = self.get_object_body_internal(holder, ParseType::Loose);
        if self.file_access.has_read_problems() {
            None
        } else {
            result
        }
    }

    pub fn get_indirect_object(
        &mut self,
        holder: Option<&HolderRef>,
        parse_type: ParseType,
    ) -> Option<ObjectRef> {
        let _session = ScopedSession::new(self.get_validator());
        let saved = self.get_pos();
        let obj_num_word = self.get_next_word();
        if !obj_num_word.is_number || obj_num_word.word.is_empty() {
            self.set_pos(saved);
            return None;
        }
        let obj_num = crate::fxcrt::fx_system::atoui(&obj_num_word.word);
        let gen_num_word = self.get_next_word();
        if !gen_num_word.is_number || gen_num_word.word.is_empty() {
            self.set_pos(saved);
            return None;
        }
        let gen_num = crate::fxcrt::fx_system::atoui(&gen_num_word.word);
        if self.get_keyword() != b"obj" {
            self.set_pos(saved);
            return None;
        }
        let object = self.get_object_body_internal(holder, parse_type);
        if let Some(object) = &object {
            let mut borrowed = object.borrow_mut();
            borrowed.set_obj_num(obj_num);
            borrowed.set_gen_num(gen_num);
        }
        if self.file_access.has_read_problems() {
            None
        } else {
            object
        }
    }

    fn get_object_body_internal(
        &mut self,
        holder: Option<&HolderRef>,
        parse_type: ParseType,
    ) -> Option<ObjectRef> {
        self.recursion_depth += 1;
        let result = if self.recursion_depth > PARSER_MAX_RECURSION_DEPTH {
            None
        } else {
            self.parse_object_body(holder, parse_type)
        };
        self.recursion_depth -= 1;
        result
    }

    fn parse_object_body(
        &mut self,
        holder: Option<&HolderRef>,
        parse_type: ParseType,
    ) -> Option<ObjectRef> {
        let saved_object_pos = self.pos;
        let word_result = self.get_next_word();
        let word = word_result.word;
        if word.is_empty() {
            return None;
        }
        if word_result.is_number {
            let saved = self.pos;
            let next = self.get_next_word();
            if !next.is_number {
                self.pos = saved;
                return Some(retain_object(CpdfScalarObject::number_from_string(&word)));
            }
            let next2 = self.get_next_word();
            if next2.word != b"R" {
                self.pos = saved;
                return Some(retain_object(CpdfScalarObject::number_from_string(&word)));
            }
            let ref_num = crate::fxcrt::fx_system::atoui(&word);
            if ref_num == u32::MAX {
                return None;
            }
            return Some(retain_object(match holder {
                Some(holder) => CpdfReference::new(holder, ref_num),
                None => CpdfReference::without_holder(ref_num),
            }));
        }
        if word == b"true" || word == b"false" {
            return Some(retain_object(CpdfScalarObject::boolean(word == b"true")));
        }
        if word == b"null" {
            return Some(retain_object(CpdfScalarObject::null()));
        }
        if word == b"(" {
            return Some(retain_object(CpdfString::from_bytes(
                None,
                &self.read_string(),
            )));
        }
        if word == b"<" {
            return Some(retain_object(CpdfString::from_hex_bytes(
                None,
                &self.read_hex_string(),
            )));
        }
        if word == b"[" {
            let mut array = CpdfArray::new();
            while let Some(object) = self.get_object_body_internal(holder, ParseType::Loose) {
                if object.borrow().object_type() != ObjectType::Stream {
                    array.append(object);
                }
            }
            return (parse_type == ParseType::Loose || self.word_buffer[0] == b']')
                .then(|| retain_object(array));
        }
        if word[0] == b'/' {
            let name = pdf_name_decode(&self.word_buffer[1..self.word_size]);
            return Some(retain_object(CpdfName::new(None, &name)));
        }
        if word == b"<<" {
            let dict_obj = retain_object(CpdfDictionary::new());
            loop {
                let inner_word = self.get_next_word().word;
                if inner_word.is_empty() {
                    return None;
                }
                let saved_pos = self.pos - inner_word.len() as FxFileSize;
                if inner_word == b">>" {
                    break;
                }
                if inner_word == b"endobj" {
                    self.pos = saved_pos;
                    break;
                }
                if inner_word[0] != b'/' {
                    continue;
                }
                let key = pdf_name_decode(&inner_word);
                if key.is_empty() && parse_type == ParseType::Loose {
                    continue;
                }
                let value = self.get_object_body_internal(holder, ParseType::Loose);
                let Some(value) = value else {
                    if parse_type == ParseType::Loose {
                        continue;
                    }
                    self.to_next_line();
                    return None;
                };
                if key.len() > 1 && value.borrow().object_type() != ObjectType::Stream {
                    dict_obj
                        .borrow_mut()
                        .as_any_mut()
                        .downcast_mut::<CpdfDictionary>()
                        .unwrap()
                        .set_for(&key[1..], Some(value));
                }
            }
            let saved = self.pos;
            if self.get_next_word().word != b"stream" {
                self.pos = saved;
                return Some(dict_obj);
            }
            return self.read_stream(dict_obj);
        }
        if word == b">>" {
            self.pos = saved_object_pos;
        }
        None
    }

    fn read_eol_markers(&mut self, pos: FxFileSize) -> u32 {
        let first = self.get_char_at(pos).unwrap_or(0);
        let second = self.get_char_at(pos + 1).unwrap_or(0);
        if first == b'\r' && second == b'\n' {
            2
        } else if first == b'\r' || first == b'\n' {
            1
        } else {
            0
        }
    }

    fn is_whole_word(
        &mut self,
        start: FxFileSize,
        limit: FxFileSize,
        tag: &[u8],
        check_keyword: bool,
    ) -> bool {
        let check_left = !pdf_char_is_delimiter(tag[0]) && !pdf_char_is_whitespace(tag[0]);
        let check_right = !pdf_char_is_delimiter(tag[tag.len() - 1])
            && !pdf_char_is_whitespace(tag[tag.len() - 1]);
        if check_right && start + tag.len() as FxFileSize <= limit {
            if let Some(ch) = self.get_char_at(start + tag.len() as FxFileSize) {
                if pdf_char_is_numeric(ch)
                    || pdf_char_is_other(ch)
                    || (check_keyword && pdf_char_is_delimiter(ch))
                {
                    return false;
                }
            }
        }
        if check_left && start > 0 {
            if let Some(ch) = self.get_char_at(start - 1) {
                if pdf_char_is_numeric(ch)
                    || pdf_char_is_other(ch)
                    || (check_keyword && pdf_char_is_delimiter(ch))
                {
                    return false;
                }
            }
        }
        true
    }

    pub fn backwards_search_to_word(&mut self, word: &[u8], limit: FxFileSize) -> bool {
        let tag_len = word.len() as i32;
        if tag_len == 0 {
            return false;
        }
        let start = self.pos;
        let mut pos = self.pos;
        let mut offset = tag_len - 1;
        loop {
            if limit != 0 && pos <= start - limit {
                return false;
            }
            let Some(byte) = self.get_char_at_backward(pos) else {
                return false;
            };
            if byte == word[offset as usize] {
                offset -= 1;
                if offset >= 0 {
                    pos -= 1;
                    continue;
                }
                if self.is_whole_word(pos, limit, word, false) {
                    self.pos = pos;
                    return true;
                }
            }
            offset = if byte == word[(tag_len - 1) as usize] {
                (tag_len - 2).max(0)
            } else {
                tag_len - 1
            };
            pos -= 1;
            if pos < 0 {
                return false;
            }
        }
    }

    pub fn find_tag(&mut self, tag: &[u8]) -> FxFileSize {
        assert!(!tag.is_empty());
        let start = self.pos;
        loop {
            let match_start = self.get_pos();
            let mut found = true;
            for &expected in tag {
                let Some(ch) = self.get_next_char() else {
                    return -1;
                };
                if ch != expected {
                    found = false;
                    break;
                }
            }
            if found {
                return match_start - start;
            }
            self.set_pos(match_start + 1);
        }
    }

    fn find_word_pos(&mut self, word: &[u8]) -> FxFileSize {
        let saved = self.pos;
        let mut offset = self.find_tag(word);
        while offset >= 0 {
            let candidate = self.get_pos() - word.len() as FxFileSize;
            if self.is_whole_word(candidate, self.file_len, word, true) {
                self.pos = saved;
                return candidate;
            }
            offset = self.find_tag(word);
        }
        self.pos = saved;
        -1
    }

    fn find_stream_end_pos(&mut self) -> FxFileSize {
        let mut end_stream = self.find_word_pos(b"endstream");
        let end_obj = self.find_word_pos(b"endobj");
        if end_stream < 0 && end_obj < 0 {
            return -1;
        }
        if end_stream < 0 {
            end_stream = end_obj;
        } else if end_obj >= 0 && end_stream > end_obj {
            end_stream = end_obj;
        }
        if self.read_eol_markers(end_stream - 2) == 2 {
            end_stream -= 2;
        } else if self.read_eol_markers(end_stream - 1) == 1 {
            end_stream -= 1;
        }
        if end_stream < self.get_pos() {
            -1
        } else {
            end_stream
        }
    }

    fn read_stream(&mut self, dict_obj: ObjectRef) -> Option<ObjectRef> {
        let mut len = {
            let borrowed = dict_obj.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            dict.get_typed_direct_for(b"Length", ObjectType::Number)
                .map_or(-1, |number| i64::from(number.borrow().get_integer()))
        };
        self.to_next_line();
        let stream_start = self.get_pos();
        if len > 0
            && self
                .pos
                .checked_add(len)
                .is_none_or(|end| end >= self.file_len)
        {
            len = -1;
        }
        if len > 0 {
            let absolute = self.header_offset.checked_add(self.pos)?;
            if !self
                .file_access
                .check_data_range_and_request_if_unavailable(absolute, usize::try_from(len).ok()?)
            {
                return None;
            }
            self.set_pos(self.pos + len);
        }
        if len >= 0 {
            let _session = ScopedSession::new(self.get_validator());
            self.pos += i64::from(self.read_eol_markers(self.get_pos()));
            self.word_buffer[..b"endstream".len() + 1].fill(0);
            self.get_next_word_internal();
            if self.file_access.has_read_problems() {
                return None;
            }
            if &self.word_buffer[..b"endstream".len()] != b"endstream" {
                len = -1;
                self.set_pos(stream_start);
            }
        }
        if len < 0 {
            let stream_end = self.find_stream_end_pos();
            if stream_end < 0 {
                return None;
            }
            len = stream_end - stream_start;
            if len > 0 {
                self.set_pos(stream_start);
                let absolute = self.header_offset.checked_add(self.pos)?;
                if !self
                    .file_access
                    .check_data_range_and_request_if_unavailable(
                        absolute,
                        usize::try_from(len).ok()?,
                    )
                {
                    return None;
                }
                self.set_pos(self.pos + len);
            }
        }
        let stream = if len > 0 {
            let mut data = vec![0u8; usize::try_from(len).ok()?];
            let absolute = self.header_offset.checked_add(stream_start)?;
            assert!(self.file_access.read_block_at_offset(&mut data, absolute));
            let file: Rc<dyn SeekableReadStream> = Rc::new(CfxReadOnlyContainerStream::new(data));
            CpdfStream::from_file(file, dict_obj)
        } else {
            CpdfStream::from_dict(dict_obj)
        };
        let end_stream_offset = self.get_pos();
        self.word_buffer[..b"endobj".len() + 1].fill(0);
        self.get_next_word_internal();
        while let Some(ch) = self.get_next_char() {
            if !pdf_char_is_whitespace(ch) || pdf_char_is_line_ending(ch) {
                break;
            }
        }
        self.set_pos((self.get_pos() - 1).max(0));
        let markers = self.read_eol_markers(self.get_pos());
        if self.word_size == b"endobj".len()
            && markers != 0
            && &self.word_buffer[..b"endobj".len()] == b"endobj"
        {
            self.set_pos(end_stream_offset);
        }
        Some(retain_object(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::{CpdfSyntaxParser, ParseType};
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
    use std::rc::Rc;

    #[test]
    fn words_and_escaped_strings_cross_small_buffer_boundaries() {
        let file = Rc::new(CfxReadOnlySpanStream::new(
            b"xx% comment\r\n/Name (a\\n(b)\\053z) <41 2>",
        ));
        let mut parser = CpdfSyntaxParser::create_for_testing(file, 2);
        parser.set_read_buffer_size(3);
        assert_eq!(parser.get_next_word().word, b"/Name");
        assert_eq!(parser.get_next_word().word, b"(");
        assert_eq!(parser.read_string(), b"a\n(b)+z");
        assert_eq!(parser.get_next_word().word, b"<");
        assert_eq!(parser.read_hex_string(), [0x41, 0x20]);
    }

    #[test]
    fn stream_end_search_prefers_earlier_endobj_and_removes_eol() {
        let file = Rc::new(CfxReadOnlySpanStream::new(b"abc\r\nendobj later endstream"));
        let mut parser = CpdfSyntaxParser::from_file(file);
        parser.set_read_buffer_size(4);
        assert_eq!(parser.find_stream_end_pos(), 3);
        assert_eq!(parser.get_pos(), 0);
    }

    #[test]
    fn direct_numbers_saturate_and_wrap_sign_like_pdfium() {
        let file = Rc::new(CfxReadOnlySpanStream::new(b"-1 4294967296 9x9"));
        let mut parser = CpdfSyntaxParser::from_file(file);
        assert_eq!(parser.get_direct_num(), u32::MAX);
        assert_eq!(parser.get_direct_num(), u32::MAX);
        assert_eq!(parser.get_direct_num(), 0);
    }

    #[test]
    fn parses_indirect_dictionary_and_file_backed_stream() {
        let file = Rc::new(CfxReadOnlySpanStream::new(
            b"4 2 obj << /Length 3 /Type /X >>\nstream\nabc\nendstream\nendobj",
        ));
        let mut parser = CpdfSyntaxParser::from_file(file);
        let object = parser.get_indirect_object(None, ParseType::Strict).unwrap();
        let borrowed = object.borrow();
        assert_eq!(borrowed.object_type(), ObjectType::Stream);
        assert_eq!(borrowed.obj_num(), 4);
        assert_eq!(borrowed.gen_num(), 2);
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
        assert!(stream.is_file_based());
        assert_eq!(stream.read_all_raw_data(), b"abc");
        let dict = stream.get_dict();
        assert_eq!(
            dict.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_name_for(b"Type"),
            b"X"
        );
    }

    #[test]
    fn reference_arrays_resolve_and_wrong_stream_length_falls_back_to_markers() {
        let holder = CpdfIndirectObjectHolder::new();
        holder
            .borrow_mut()
            .add_indirect_object(retain_object(CpdfScalarObject::number_from_signed(23)));
        let file = Rc::new(CfxReadOnlySpanStream::new(b"[1 0 R]"));
        let mut parser = CpdfSyntaxParser::from_file(file);
        let array = parser.get_object_body(Some(&holder)).unwrap();
        let borrowed = array.borrow();
        let array = borrowed
            .as_any()
            .downcast_ref::<crate::fpdfapi::parser::cpdf_array::CpdfArray>()
            .unwrap();
        assert_eq!(array.get_integer_at(0), 23);

        let file = Rc::new(CfxReadOnlySpanStream::new(
            b"<< /Length 10 >>\nstream\nabc\nendstream\nendobj",
        ));
        let mut parser = CpdfSyntaxParser::from_file(file);
        let object = parser.get_object_body(None).unwrap();
        let borrowed = object.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
        assert_eq!(stream.read_all_raw_data(), b"abc");
    }
}

#[cfg(test)]
#[path = "cpdf_syntax_parser_unittest.rs"]
mod upstream_tests;
