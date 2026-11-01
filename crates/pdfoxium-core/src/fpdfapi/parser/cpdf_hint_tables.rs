// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct port of `core/fpdfapi/parser/cpdf_hint_tables.{h,cpp}`.
//! Header scalars are copied rather than retaining a pointer into the availability
//! object's header. Bit counts and offsets are checked before advancing.

use super::cpdf_cross_ref_avail::DocAvailStatus;
use super::cpdf_cross_ref_table::MAX_OBJECT_NUMBER;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_document::PAGE_MAX_NUM;
use super::cpdf_linearized_header::CpdfLinearizedHeader;
use super::cpdf_read_validator::CpdfReadValidator;
use super::cpdf_scalar_object::ObjectType;
use super::cpdf_stream::CpdfStream;
use super::cpdf_stream_acc::CpdfStreamAcc;
use super::cpdf_syntax_parser::{CpdfSyntaxParser, ParseType};
use crate::fxcrt::cfx_bitstream::CfxBitStream;
use std::rc::Rc;

#[derive(Default, Debug, PartialEq, Eq)]
pub struct PageInfo {
    objects_count: u32,
    offset: i64,
    length: u32,
    start_obj_num: u32,
    identifiers: Vec<u32>,
}
impl PageInfo {
    pub fn set_objects_count(&mut self, value: u32) {
        self.objects_count = value;
    }
    pub fn set_page_offset(&mut self, value: i64) {
        self.offset = value;
    }
    pub fn set_page_length(&mut self, value: u32) {
        self.length = value;
    }
    pub fn set_start_obj_num(&mut self, value: u32) {
        self.start_obj_num = value;
    }
    pub fn add_identifier(&mut self, value: u32) {
        self.identifiers.push(value);
    }
    pub fn objects_count(&self) -> u32 {
        self.objects_count
    }
    pub fn page_offset(&self) -> i64 {
        self.offset
    }
    pub fn page_length(&self) -> u32 {
        self.length
    }
    pub fn start_obj_num(&self) -> u32 {
        self.start_obj_num
    }
    pub fn identifiers(&self) -> &[u32] {
        &self.identifiers
    }
}
#[derive(Default, Debug, PartialEq, Eq)]
pub struct SharedObjGroupInfo {
    pub offset: i64,
    pub length: u32,
    pub objects_count: u32,
    pub start_obj_num: u32,
}
struct Header {
    page_count: u32,
    first_page: u32,
    first_obj: u32,
    first_page_end: i64,
    hint_start: i64,
    hint_length: u32,
    has_hints: bool,
}
pub struct CpdfHintTables {
    validator: Rc<CpdfReadValidator>,
    header: Header,
    first_page_shared_objs: u32,
    first_page_obj_offset: i64,
    page_infos: Vec<PageInfo>,
    shared_obj_group_infos: Vec<SharedObjGroupInfo>,
}
fn can_read(stream: &CfxBitStream<'_>, width: u32, count: u32) -> bool {
    width
        .checked_mul(count)
        .is_some_and(|bits| stream.bits_remaining() >= bits as usize)
}
fn valid_width(bits: u32) -> bool {
    (1..=32).contains(&bits)
}
impl CpdfHintTables {
    pub fn new(validator: Rc<CpdfReadValidator>, header: &CpdfLinearizedHeader) -> Self {
        Self {
            validator,
            header: Header {
                page_count: header.page_count(),
                first_page: header.first_page_no(),
                first_obj: header.first_page_obj_num(),
                first_page_end: header.first_page_end_offset(),
                hint_start: header.hint_start(),
                hint_length: header.hint_length(),
                has_hints: header.has_hint_table(),
            },
            first_page_shared_objs: 0,
            first_page_obj_offset: 0,
            page_infos: Vec::new(),
            shared_obj_group_infos: Vec::new(),
        }
    }
    pub fn parse(parser: &mut CpdfSyntaxParser, header: &CpdfLinearizedHeader) -> Option<Self> {
        if header.page_count() <= 1 || !header.has_hint_table() {
            return None;
        }
        let validator = parser.get_validator();
        if !validator.check_data_range_and_request_if_unavailable(
            header.hint_start(),
            header.hint_length() as usize,
        ) {
            return None;
        }
        parser.set_pos(header.hint_start());
        let object = parser.get_indirect_object(None, ParseType::Loose)?;
        let borrowed = object.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>()?;
        let mut result = Self::new(validator, header);
        result.load_hint_stream(stream).then_some(result)
    }
    pub fn page_infos(&self) -> &[PageInfo] {
        &self.page_infos
    }
    pub fn shared_group_infos(&self) -> &[SharedObjGroupInfo] {
        &self.shared_obj_group_infos
    }
    pub fn first_page_obj_offset(&self) -> i64 {
        self.first_page_obj_offset
    }
    pub fn get_page_pos(&self, index: u32) -> Option<(i64, i64, u32)> {
        let page = self.page_infos.get(index as usize)?;
        Some((page.offset, i64::from(page.length), page.start_obj_num))
    }
    pub fn check_page(&self, index: u32) -> DocAvailStatus {
        use DocAvailStatus::*;
        if index == self.header.first_page {
            return DataAvailable;
        }
        let Some(page) = self.page_infos.get(index as usize) else {
            return DataError;
        };
        if page.length == 0 {
            return DataError;
        }
        if !self
            .validator
            .check_data_range_and_request_if_unavailable(page.offset, page.length as usize)
        {
            return DataNotAvailable;
        }
        for &id in &page.identifiers {
            let Some(group) = self.shared_obj_group_infos.get(id as usize) else {
                continue;
            };
            if group.offset == 0 || group.length == 0 {
                return DataError;
            }
            if !self
                .validator
                .check_data_range_and_request_if_unavailable(group.offset, group.length as usize)
            {
                return DataNotAvailable;
            }
        }
        DataAvailable
    }
    pub fn load_hint_stream(&mut self, stream: &CpdfStream) -> bool {
        if !self.header.has_hints {
            return false;
        }
        let dict = stream.get_dict();
        let offset = {
            let dict = dict.borrow();
            let Some(dict) = dict.as_any().downcast_ref::<CpdfDictionary>() else {
                return false;
            };
            let Some(offset) = dict.get_object_for(b"S") else {
                return false;
            };
            let offset = offset.borrow();
            if offset.object_type() != ObjectType::Number {
                return false;
            }
            offset.get_integer()
        };
        if offset <= 0 {
            return false;
        }
        let mut acc = CpdfStreamAcc::new(stream);
        acc.load_all_data_filtered();
        let data = acc.get_span();
        if data.len() < 60 || data.len() < offset as usize {
            return false;
        }
        let mut bits = CfxBitStream::new(data);
        self.read_page_hint_table(&mut bits)
            && self.read_shared_obj_hint_table(&mut bits, offset as u32)
    }
    fn hints_offset_to_file_offset(&self, offset: u32) -> i64 {
        let offset = i64::from(offset);
        if offset >= self.header.hint_start {
            offset
                .checked_add(i64::from(self.header.hint_length))
                .unwrap_or(0)
        } else {
            offset
        }
    }
    fn read_page_hint_table(&mut self, bits: &mut CfxBitStream<'_>) -> bool {
        let n = self.header.page_count;
        if n < 1
            || n as usize >= PAGE_MAX_NUM
            || self.header.first_page >= n
            || bits.bits_remaining() < 288
        {
            return false;
        }
        let least_objects = bits.get_bits(32);
        if least_objects == 0 || least_objects > MAX_OBJECT_NUMBER {
            return false;
        }
        self.first_page_obj_offset = self.hints_offset_to_file_offset(bits.get_bits(32));
        if self.first_page_obj_offset == 0 {
            return false;
        }
        let object_bits = bits.get_bits(16);
        if !valid_width(object_bits) {
            return false;
        }
        let least_length = bits.get_bits(32);
        if least_length == 0 {
            return false;
        }
        let length_bits = bits.get_bits(16);
        if !valid_width(length_bits) {
            return false;
        }
        bits.skip_bits(96);
        let shared_bits = bits.get_bits(16);
        if !valid_width(shared_bits) {
            return false;
        }
        let id_bits = bits.get_bits(16);
        if !valid_width(id_bits) {
            return false;
        }
        let numerator_bits = bits.get_bits(16);
        if numerator_bits > 32 {
            return false;
        }
        bits.skip_bits(16);
        if !can_read(bits, object_bits, n) {
            return false;
        }
        self.page_infos = (0..n).map(|_| PageInfo::default()).collect();
        self.page_infos[self.header.first_page as usize].start_obj_num = self.header.first_obj;
        let mut next_object = 1u32;
        for (index, page) in self.page_infos.iter_mut().enumerate() {
            let Some(count) = least_objects.checked_add(bits.get_bits(object_bits)) else {
                return false;
            };
            page.objects_count = count;
            if index == self.header.first_page as usize {
                continue;
            }
            page.start_obj_num = next_object;
            let Some(next) = next_object
                .checked_add(count)
                .filter(|&v| v <= MAX_OBJECT_NUMBER)
            else {
                return false;
            };
            next_object = next;
        }
        bits.byte_align();
        if !can_read(bits, length_bits, n) {
            return false;
        }
        for page in &mut self.page_infos {
            let Some(length) = least_length.checked_add(bits.get_bits(length_bits)) else {
                return false;
            };
            page.length = length;
        }
        self.page_infos[self.header.first_page as usize].offset = self.first_page_obj_offset;
        let mut previous_end = self.header.first_page_end;
        for (index, page) in self.page_infos.iter_mut().enumerate() {
            if index == self.header.first_page as usize {
                continue;
            }
            page.offset = previous_end;
            let Some(end) = previous_end.checked_add(i64::from(page.length)) else {
                return false;
            };
            previous_end = end;
        }
        bits.byte_align();
        if !can_read(bits, shared_bits, n) {
            return false;
        }
        let counts: Vec<_> = (0..n).map(|_| bits.get_bits(shared_bits)).collect();
        bits.byte_align();
        for (page, &count) in self.page_infos.iter_mut().zip(&counts) {
            if !can_read(bits, id_bits, count) {
                return false;
            }
            page.identifiers = (0..count).map(|_| bits.get_bits(id_bits)).collect();
        }
        bits.byte_align();
        if numerator_bits > 0 {
            for count in counts {
                if !can_read(bits, numerator_bits, count) {
                    return false;
                }
                bits.skip_bits((numerator_bits * count) as usize);
            }
            bits.byte_align();
        }
        if !can_read(bits, length_bits, n) {
            return false;
        }
        bits.skip_bits((length_bits * n) as usize);
        bits.byte_align();
        true
    }
    fn read_shared_obj_hint_table(&mut self, bits: &mut CfxBitStream<'_>, offset: u32) -> bool {
        let Some(offset) = offset.checked_mul(8) else {
            return false;
        };
        if bits.is_eof() || bits.pos() > offset as usize {
            return false;
        }
        bits.skip_bits(offset as usize - bits.pos());
        if bits.bits_remaining() < 192 {
            return false;
        }
        let first_object = bits.get_bits(32);
        if first_object == 0 || first_object > MAX_OBJECT_NUMBER {
            return false;
        }
        let first_location = self.hints_offset_to_file_offset(bits.get_bits(32));
        if first_location == 0 {
            return false;
        }
        self.first_page_shared_objs = bits.get_bits(32);
        let total = bits.get_bits(32);
        let count_bits = bits.get_bits(16);
        if count_bits > 32 {
            return false;
        }
        let least_length = bits.get_bits(32);
        let length_bits = bits.get_bits(16);
        if !valid_width(length_bits)
            || self.first_page_shared_objs > MAX_OBJECT_NUMBER
            || total > MAX_OBJECT_NUMBER
            || !can_read(bits, length_bits, total)
        {
            return false;
        }
        if total > 0
            && total - 1 > self.first_page_shared_objs
            && first_object
                .checked_add(total - 1 - self.first_page_shared_objs)
                .is_none()
        {
            return false;
        }
        self.shared_obj_group_infos = (0..total).map(|_| SharedObjGroupInfo::default()).collect();
        let mut previous_end = self.first_page_obj_offset;
        for (index, group) in self.shared_obj_group_infos.iter_mut().enumerate() {
            if index == self.first_page_shared_objs as usize {
                previous_end = first_location;
            }
            let Some(length) = least_length.checked_add(bits.get_bits(length_bits)) else {
                return false;
            };
            group.length = length;
            group.offset = previous_end;
            let Some(end) = previous_end.checked_add(i64::from(length)) else {
                return false;
            };
            previous_end = end;
        }
        bits.byte_align();
        // Unlike the native unchecked GetBits fallback, reject truncated flags/counts.
        if !can_read(bits, 1, total) {
            return false;
        }
        let signatures: u32 = (0..total).map(|_| bits.get_bits(1)).sum();
        bits.byte_align();
        if !can_read(bits, 128, signatures) {
            return false;
        }
        bits.skip_bits((signatures * 128) as usize);
        bits.byte_align();
        if !can_read(bits, count_bits, total) {
            return false;
        }
        let mut current = self.header.first_obj;
        for (index, group) in self.shared_obj_group_infos.iter_mut().enumerate() {
            if index == self.first_page_shared_objs as usize {
                current = first_object;
            }
            let delta = if count_bits == 0 {
                0
            } else {
                bits.get_bits(count_bits)
            };
            let Some(count) = delta.checked_add(1) else {
                return false;
            };
            group.start_obj_num = current;
            group.objects_count = count;
            let Some(next) = current.checked_add(count) else {
                return false;
            };
            current = next;
        }
        bits.byte_align();
        true
    }
}

#[cfg(test)]
#[path = "cpdf_hint_tables_unittest.rs"]
mod upstream_tests;
