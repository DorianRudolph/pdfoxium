// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_linearized_header.{h,cpp}`.

use super::cpdf_array::CpdfArray;
use super::cpdf_cross_ref_table::MAX_OBJECT_NUMBER;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_scalar_object::CpdfScalarObject;
use super::cpdf_syntax_parser::{CpdfSyntaxParser, ParseType};
use crate::fxcrt::fx_types::FxFileSize;

const LINEARIZED_HEADER_OFFSET: FxFileSize = 9;

pub struct CpdfLinearizedHeader {
    file_size: FxFileSize,
    first_page_no: u32,
    main_xref_table_first_entry_offset: FxFileSize,
    page_count: u32,
    first_page_end_offset: FxFileSize,
    first_page_obj_num: u32,
    last_xref_offset: FxFileSize,
    hint_start: FxFileSize,
    hint_length: u32,
}

impl CpdfLinearizedHeader {
    pub fn parse(parser: &mut CpdfSyntaxParser) -> Option<Self> {
        parser.set_pos(LINEARIZED_HEADER_OFFSET);
        let object = parser.get_indirect_object(None, ParseType::Loose)?;
        let borrowed = object.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        if !dict.key_exist(b"Linearized")
            || !Self::valid_number(dict, b"L", 1, false, true)
            || !Self::valid_number(dict, b"P", 0, true, false)
            || !Self::valid_number(dict, b"T", 1, false, true)
            || !Self::valid_number(dict, b"N", 1, true, true)
            || !Self::valid_number(dict, b"E", 1, false, true)
            || !Self::valid_number(dict, b"O", 1, true, true)
        {
            return None;
        }
        if parser.get_next_word().word != b"endobj" {
            return None;
        }
        let result = Self::from_dict(dict, parser.get_pos());
        let document_size = parser.get_document_size();
        (result.file_size == document_size
            && result.first_page_no < i32::MAX as u32
            && result.first_page_no < result.page_count
            && result.main_xref_table_first_entry_offset < document_size
            && result.first_page_end_offset < document_size
            && result.first_page_obj_num <= MAX_OBJECT_NUMBER
            && result.last_xref_offset < document_size
            && result.hint_start < document_size)
            .then_some(result)
    }

    // CPDF_LinearizedHeader's protected dictionary constructor.
    pub(super) fn from_dict(dict: &CpdfDictionary, last_xref_offset: FxFileSize) -> Self {
        let mut result = Self {
            file_size: i64::from(dict.get_integer_for(b"L", 0)),
            first_page_no: dict.get_integer_for(b"P", 0) as u32,
            main_xref_table_first_entry_offset: i64::from(dict.get_integer_for(b"T", 0)),
            page_count: dict.get_integer_for(b"N", 0) as u32,
            first_page_end_offset: i64::from(dict.get_integer_for(b"E", 0)),
            first_page_obj_num: dict.get_integer_for(b"O", 0) as u32,
            last_xref_offset,
            hint_start: 0,
            hint_length: 0,
        };
        if let Some(hints) = dict.get_array_for(b"H") {
            let hints = hints.borrow();
            let hints = hints.as_any().downcast_ref::<CpdfArray>().unwrap();
            if hints.size() == 2 || hints.size() == 4 {
                result.hint_start = i64::from(hints.get_integer_at(0).max(0));
                if let Ok(length) = u32::try_from(hints.get_integer_at(1)) {
                    result.hint_length = length;
                }
            }
        }
        result
    }

    fn valid_number(
        dict: &CpdfDictionary,
        key: &[u8],
        minimum: i32,
        unsigned: bool,
        must_exist: bool,
    ) -> bool {
        if !dict.key_exist(key) {
            return !must_exist;
        }
        let Some(number) = dict.get_number_for(key) else {
            return false;
        };
        let borrowed = number.borrow();
        let Some(number) = borrowed.as_any().downcast_ref::<CpdfScalarObject>() else {
            return false;
        };
        number.is_integer()
            && number.get_integer() >= minimum
            && (!unsigned || number.get_integer() >= 0)
    }

    pub fn file_size(&self) -> FxFileSize {
        self.file_size
    }
    pub fn first_page_no(&self) -> u32 {
        self.first_page_no
    }
    pub fn main_xref_table_first_entry_offset(&self) -> FxFileSize {
        self.main_xref_table_first_entry_offset
    }
    pub fn page_count(&self) -> u32 {
        self.page_count
    }
    pub fn first_page_end_offset(&self) -> FxFileSize {
        self.first_page_end_offset
    }
    pub fn first_page_obj_num(&self) -> u32 {
        self.first_page_obj_num
    }
    pub fn last_xref_offset(&self) -> FxFileSize {
        self.last_xref_offset
    }
    pub fn has_hint_table(&self) -> bool {
        self.page_count > 1 && self.hint_start > 0 && self.hint_length > 0
    }
    pub fn hint_start(&self) -> FxFileSize {
        self.hint_start
    }
    pub fn hint_length(&self) -> u32 {
        self.hint_length
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::CpdfLinearizedHeader;
    use crate::fpdfapi::parser::cpdf_syntax_parser::CpdfSyntaxParser;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    fn parse_with_page_count(page_count: u32) -> Option<CpdfLinearizedHeader> {
        let mut data = format!(
            "%PDF-1.7\n1 0 obj << /Linearized 1 /L 0000 /P 0 /T 10 /N {page_count} /E 20 /O 1 /H [30 5] >> endobj\ntrailer"
        )
        .into_bytes();
        let length = format!("{:04}", data.len());
        let offset = data.windows(4).position(|bytes| bytes == b"0000").unwrap();
        data[offset..offset + 4].copy_from_slice(length.as_bytes());
        let file = Rc::new(CfxReadOnlyContainerStream::new(data));
        CpdfLinearizedHeader::parse(&mut CpdfSyntaxParser::from_file(file))
    }

    #[test]
    fn parses_valid_header_and_rejects_invalid_page_count() {
        let header = parse_with_page_count(2).unwrap();
        assert_eq!(header.first_page_no(), 0);
        assert_eq!(header.first_page_obj_num(), 1);
        assert!(header.has_hint_table());
        assert_eq!(header.hint_length(), 5);
        assert!(parse_with_page_count(0).is_none());
    }
}
