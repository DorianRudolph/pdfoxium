// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/parser/cpdf_cross_ref_avail.{h,cpp}`. `DocAvailStatus`
//! is shared with the eventual `CPDF_DataAvail` translation.

use std::collections::{BTreeSet, VecDeque};

use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_read_validator::ScopedSession;
use super::cpdf_scalar_object::ObjectType;
use super::cpdf_stream::CpdfStream;
use super::cpdf_syntax_parser::{CpdfSyntaxParser, ParseType};
use crate::fxcrt::fx_types::FxFileSize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum DocAvailStatus {
    DataError = -1,
    DataNotAvailable = 0,
    DataAvailable = 1,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum State {
    CrossRefCheck,
    CrossRefTableItemCheck,
    CrossRefTableTrailerCheck,
    Done,
}

pub struct CpdfCrossRefAvail<'a> {
    parser: &'a mut CpdfSyntaxParser,
    last_crossref_offset: FxFileSize,
    status: DocAvailStatus,
    state: State,
    offset: FxFileSize,
    cross_refs_for_check: VecDeque<FxFileSize>,
    registered_crossrefs: BTreeSet<FxFileSize>,
}

/// Owned scanner continuation for DataAvail. Moving this state in/out of a
/// borrowed parser avoids a self-reference without repeating work or cloning
/// its queues/visited set between availability polls.
pub struct CpdfCrossRefAvailState {
    last_crossref_offset: FxFileSize,
    status: DocAvailStatus,
    state: State,
    offset: FxFileSize,
    cross_refs_for_check: VecDeque<FxFileSize>,
    registered_crossrefs: BTreeSet<FxFileSize>,
}
impl CpdfCrossRefAvailState {
    pub fn last_crossref_offset(&self) -> FxFileSize {
        self.last_crossref_offset
    }
}

impl<'a> CpdfCrossRefAvail<'a> {
    pub fn from_state(parser: &'a mut CpdfSyntaxParser, state: CpdfCrossRefAvailState) -> Self {
        Self {
            parser,
            last_crossref_offset: state.last_crossref_offset,
            status: state.status,
            state: state.state,
            offset: state.offset,
            cross_refs_for_check: state.cross_refs_for_check,
            registered_crossrefs: state.registered_crossrefs,
        }
    }
    pub fn into_state(self) -> CpdfCrossRefAvailState {
        CpdfCrossRefAvailState {
            last_crossref_offset: self.last_crossref_offset,
            status: self.status,
            state: self.state,
            offset: self.offset,
            cross_refs_for_check: self.cross_refs_for_check,
            registered_crossrefs: self.registered_crossrefs,
        }
    }
    pub fn new(parser: &'a mut CpdfSyntaxParser, last_crossref_offset: FxFileSize) -> Self {
        let mut result = Self {
            parser,
            last_crossref_offset,
            status: DocAvailStatus::DataNotAvailable,
            state: State::CrossRefCheck,
            offset: 0,
            cross_refs_for_check: VecDeque::new(),
            registered_crossrefs: BTreeSet::new(),
        };
        result.add_cross_ref_for_check(last_crossref_offset);
        result
    }

    pub fn last_crossref_offset(&self) -> FxFileSize {
        self.last_crossref_offset
    }

    pub fn check_avail(&mut self) -> DocAvailStatus {
        if self.status == DocAvailStatus::DataAvailable {
            return self.status;
        }
        let validator = self.parser.get_validator();
        let _read_session = ScopedSession::new(validator.clone());
        loop {
            let result = match self.state {
                State::CrossRefCheck => self.check_cross_ref(),
                State::CrossRefTableItemCheck => self.check_cross_ref_table_item(),
                State::CrossRefTableTrailerCheck => self.check_cross_ref_table_trailer(),
                State::Done => false,
            };
            if !result {
                break;
            }
            debug_assert!(!validator.has_read_problems());
        }
        self.status
    }

    fn check_read_problems(&mut self) -> bool {
        let validator = self.parser.get_validator();
        if validator.read_error() {
            self.status = DocAvailStatus::DataError;
            return true;
        }
        validator.has_unavailable_data()
    }

    fn check_cross_ref(&mut self) -> bool {
        let Some(&offset) = self.cross_refs_for_check.front() else {
            self.state = State::Done;
            self.status = DocAvailStatus::DataAvailable;
            return true;
        };
        self.parser.set_pos(offset);
        let first_word = self.parser.peek_next_word();
        if self.check_read_problems() {
            return false;
        }
        let result = if first_word == b"xref" {
            self.check_cross_ref_table()
        } else {
            self.check_cross_ref_stream()
        };
        if result {
            self.cross_refs_for_check.pop_front();
        }
        result
    }

    fn check_cross_ref_table(&mut self) -> bool {
        let keyword = self.parser.get_keyword();
        if self.check_read_problems() {
            return false;
        }
        if keyword != b"xref" {
            self.status = DocAvailStatus::DataError;
            return false;
        }
        self.state = State::CrossRefTableItemCheck;
        self.offset = self.parser.get_pos();
        true
    }

    fn check_cross_ref_table_item(&mut self) -> bool {
        self.parser.set_pos(self.offset);
        let keyword = self.parser.get_keyword();
        if self.check_read_problems() {
            return false;
        }
        if keyword.is_empty() {
            self.status = DocAvailStatus::DataError;
            return false;
        }
        if keyword == b"trailer" {
            self.state = State::CrossRefTableTrailerCheck;
        }
        self.offset = self.parser.get_pos();
        true
    }

    fn check_cross_ref_table_trailer(&mut self) -> bool {
        self.parser.set_pos(self.offset);
        let trailer = self.parser.get_object_body(None);
        if self.check_read_problems() {
            return false;
        }
        let Some(trailer) = trailer else {
            self.status = DocAvailStatus::DataError;
            return false;
        };
        let borrowed = trailer.borrow();
        let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
            self.status = DocAvailStatus::DataError;
            return false;
        };
        if Self::has_encryption_reference(dict) {
            self.status = DocAvailStatus::DataError;
            return false;
        }
        let prev = dict.get_direct_integer_for(b"Prev");
        if prev > 0 {
            self.add_cross_ref_for_check(i64::from(prev));
        }
        let xref_stream = dict.get_direct_integer_for(b"XRefStm");
        if xref_stream > 0 {
            self.add_cross_ref_for_check(i64::from(xref_stream));
        }
        self.state = State::CrossRefCheck;
        true
    }

    fn check_cross_ref_stream(&mut self) -> bool {
        let cross_ref = self.parser.get_indirect_object(None, ParseType::Loose);
        if self.check_read_problems() {
            return false;
        }
        let dict = cross_ref.and_then(|object| {
            let borrowed = object.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfStream>()
                .map(CpdfStream::get_dict)
        });
        let Some(dict) = dict else {
            self.status = DocAvailStatus::DataError;
            return false;
        };
        let borrowed = dict.borrow();
        let dict = borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("PDF stream has a dictionary");
        if Self::has_encryption_reference(dict) {
            self.status = DocAvailStatus::DataError;
            return false;
        }
        if dict.get_name_for(b"Type") == b"XRef" {
            let prev = dict.get_integer_for(b"Prev", 0);
            if prev > 0 {
                self.add_cross_ref_for_check(i64::from(prev));
            }
        }
        self.state = State::CrossRefCheck;
        true
    }

    fn has_encryption_reference(dict: &CpdfDictionary) -> bool {
        dict.get_object_for(b"Encrypt")
            .is_some_and(|object| object.borrow().object_type() == ObjectType::Reference)
    }

    fn add_cross_ref_for_check(&mut self, offset: FxFileSize) {
        if self.registered_crossrefs.insert(offset) {
            self.cross_refs_for_check.push_back(offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{CpdfCrossRefAvail, DocAvailStatus};
    use crate::fpdfapi::parser::cpdf_syntax_parser::CpdfSyntaxParser;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    #[test]
    fn walks_previous_cross_reference_table_and_rejects_truncated_one() {
        let mut data = b"%PDF-1.7\nxref\n0 1\n0000000000 65535 f\ntrailer << /Size 1 >>\n".to_vec();
        let second_offset = data.len() as i64;
        data.extend_from_slice(b"xref\n0 1\n0000000000 65535 f\ntrailer << /Prev 9 /Size 1 >>\n");
        let file = Rc::new(CfxReadOnlyContainerStream::new(data));
        let mut syntax = CpdfSyntaxParser::from_file(file);
        let mut avail = CpdfCrossRefAvail::new(&mut syntax, second_offset);
        assert_eq!(avail.check_avail(), DocAvailStatus::DataAvailable);
        assert_eq!(avail.check_avail(), DocAvailStatus::DataAvailable);

        let file = Rc::new(CfxReadOnlyContainerStream::new(b"xref\n0 1".to_vec()));
        let mut syntax = CpdfSyntaxParser::from_file(file);
        assert_eq!(
            CpdfCrossRefAvail::new(&mut syntax, 0).check_avail(),
            DocAvailStatus::DataError
        );
    }
}

#[cfg(test)]
#[path = "cpdf_cross_ref_avail_unittest.rs"]
mod upstream_tests;
