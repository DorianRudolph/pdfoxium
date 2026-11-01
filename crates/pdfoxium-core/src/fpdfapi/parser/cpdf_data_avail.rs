// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct port of `core/fpdfapi/parser/cpdf_data_avail.{h,cpp}`.
//! Availability state owns no document. Cross-reference and page work survive
//! incomplete reads; only native error/rebuild branches request the whole file.

use super::cpdf_array::CpdfArray;
pub use super::cpdf_cross_ref_avail::DocAvailStatus;
use super::cpdf_cross_ref_avail::{CpdfCrossRefAvail, CpdfCrossRefAvailState};
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_document::CpdfDocument;
use super::cpdf_hint_tables::CpdfHintTables;
use super::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
use super::cpdf_linearized_header::CpdfLinearizedHeader;
use super::cpdf_object::{get_dict_object, get_direct_object, ObjectRef, INVALID_OBJ_NUM};
use super::cpdf_page_object_avail::CpdfPageObjectAvail;
use super::cpdf_parser::{parse_start_xref, CpdfParser, ParserError};
use super::cpdf_read_validator::{CpdfReadValidator, ScopedSession};
pub use super::cpdf_read_validator::{DownloadHints, FileAvail};
use super::cpdf_reference::CpdfReference;
use super::cpdf_scalar_object::ObjectType;
use super::fpdf_parser_utility::get_header_offset;
use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};
use DocAvailStatus::*;

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocLinearizationStatus {
    LinearizationUnknown = -1,
    NotLinearized = 0,
    Linearized = 1,
}
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocFormStatus {
    FormError = -1,
    FormNotAvailable = 0,
    FormAvailable = 1,
    FormNotExist = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InternalStatus {
    Header,
    FirstPage,
    HintTable,
    LoadAllCrossRef,
    Root,
    Info,
    PageTree,
    Page,
    PageLaterLoad,
    Done,
    Error,
    LoadAllFile,
}
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum PageNodeType {
    #[default]
    Unknown,
    Page,
    Pages,
    Array,
}
#[derive(Default)]
struct PageNode {
    kind: PageNodeType,
    number: u32,
    children: Vec<usize>,
}
struct HintsScope(Rc<CpdfReadValidator>);
impl HintsScope {
    fn new(
        validator: Rc<CpdfReadValidator>,
        hints: Option<&Rc<RefCell<dyn DownloadHints>>>,
    ) -> Self {
        validator.set_download_hints(hints);
        Self(validator)
    }
}
impl Drop for HintsScope {
    fn drop(&mut self) {
        self.0.set_download_hints(None);
    }
}
fn raw(dict: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    let dict = dict.borrow();
    dict.as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)
}
fn integer(dict: &ObjectRef, key: &[u8]) -> i32 {
    let Some(mut object) = raw(dict, key) else {
        return 0;
    };
    let mut seen = BTreeSet::new();
    loop {
        if reference(&object).is_none() {
            return object.borrow().get_integer();
        }
        let identity = Rc::as_ptr(&object) as *const () as usize;
        if !seen.insert(identity) {
            return 0;
        }
        let Some(direct) = get_direct_object(&object) else {
            return 0;
        };
        object = direct;
    }
}

fn name(dict: &ObjectRef, key: &[u8]) -> Vec<u8> {
    let dict = dict.borrow();
    dict.as_any()
        .downcast_ref::<CpdfDictionary>()
        .map_or_else(Vec::new, |d| d.get_name_for(key))
}
fn reference(object: &ObjectRef) -> Option<u32> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfReference>()
        .map(CpdfReference::ref_obj_num)
}
fn reference_children(object: &ObjectRef) -> Option<Vec<u32>> {
    if let Some(number) = reference(object) {
        return Some(vec![number]);
    }
    let object = object.borrow();
    let array = object.as_any().downcast_ref::<CpdfArray>()?;
    Some(
        (0..array.size())
            .filter_map(|i| array.get_object_at(i).and_then(|v| reference(&v)))
            .collect(),
    )
}

pub struct CpdfDataAvail {
    file_read: Rc<CpdfReadValidator>,
    file_len: i64,
    parser: CpdfParser,
    root: Option<ObjectRef>,
    linearized: Option<CpdfLinearizedHeader>,
    doc_avail: bool,
    status: InternalStatus,
    cross_ref_avail: Option<CpdfCrossRefAvailState>,
    document: Weak<RefCell<CpdfDocument>>,
    page_obj_list: Vec<u32>,
    seen_page_obj_list: BTreeSet<u32>,
    pages_obj_num: u32,
    linearized_data_ok: bool,
    main_xref_load_tried: bool,
    main_xref_loaded_ok: bool,
    pages_tree_load: bool,
    pages_load: bool,
    form_avail: Option<CpdfPageObjectAvail>,
    pages_array: Vec<ObjectRef>,
    total_load_page_tree: bool,
    cur_page_dict_load_ok: bool,
    header_avail: bool,
    page_nodes: Vec<PageNode>,
    page_map_check_state: BTreeSet<u32>,
    pages_load_state: BTreeSet<u32>,
    hint_tables: Option<CpdfHintTables>,
    pages_obj_avail: BTreeMap<u32, CpdfPageObjectAvail>,
    pages_resources_avail: BTreeMap<usize, (ObjectRef, CpdfPageObjectAvail)>,
}
impl CpdfDataAvail {
    pub fn new(file_avail: Option<&Rc<dyn FileAvail>>, file: Rc<dyn SeekableReadStream>) -> Self {
        Self::from_validator(Rc::new(CpdfReadValidator::new(file, file_avail)))
    }
    pub fn from_validator(file_read: Rc<CpdfReadValidator>) -> Self {
        let file_len = file_read.get_size();
        Self {
            file_read,
            file_len,
            parser: CpdfParser::new(),
            root: None,
            linearized: None,
            doc_avail: false,
            status: InternalStatus::Header,
            cross_ref_avail: None,
            document: Weak::new(),
            page_obj_list: Vec::new(),
            seen_page_obj_list: BTreeSet::new(),
            pages_obj_num: 0,
            linearized_data_ok: false,
            main_xref_load_tried: false,
            main_xref_loaded_ok: false,
            pages_tree_load: false,
            pages_load: false,
            form_avail: None,
            pages_array: Vec::new(),
            total_load_page_tree: false,
            cur_page_dict_load_ok: false,
            header_avail: false,
            page_nodes: vec![PageNode::default()],
            page_map_check_state: BTreeSet::new(),
            pages_load_state: BTreeSet::new(),
            hint_tables: None,
            pages_obj_avail: BTreeMap::new(),
            pages_resources_avail: BTreeMap::new(),
        }
    }
    pub fn validator(&self) -> Rc<CpdfReadValidator> {
        self.file_read.clone()
    }
    pub fn hint_tables(&self) -> Option<&CpdfHintTables> {
        self.hint_tables.as_ref()
    }
    pub fn get_page_count(&self) -> usize {
        self.linearized.as_ref().map_or_else(
            || {
                self.document
                    .upgrade()
                    .map_or(0, |d| d.borrow().page_count())
            },
            |h| h.page_count() as usize,
        )
    }
    fn live_document(&mut self) -> Option<Rc<RefCell<CpdfDocument>>> {
        let document = self.document.upgrade();
        if document.is_none() {
            self.form_avail = None;
            self.pages_array.clear();
            self.pages_obj_avail.clear();
            self.pages_resources_avail.clear();
        }
        document
    }
    pub fn is_doc_avail(
        &mut self,
        hints: Option<&Rc<RefCell<dyn DownloadHints>>>,
    ) -> DocAvailStatus {
        if self.file_len == 0 {
            return DataError;
        }
        let _hints = HintsScope::new(self.validator(), hints);
        self.seen_page_obj_list.clear();
        while !self.doc_avail {
            if !self.check_doc_status() {
                self.seen_page_obj_list.clear();
                return DataNotAvailable;
            }
        }
        self.seen_page_obj_list.clear();
        DataAvailable
    }
    fn check_doc_status(&mut self) -> bool {
        use InternalStatus::*;
        match self.status {
            Header => match self.check_header_and_linearized() {
                DataAvailable => {
                    self.status = if self.linearized.is_some() {
                        FirstPage
                    } else {
                        LoadAllCrossRef
                    };
                    true
                }
                DataNotAvailable => false,
                DataError => {
                    self.status = Error;
                    true
                }
            },
            FirstPage => self.check_first_page(),
            HintTable => self.check_hint_tables(),
            LoadAllCrossRef => self.check_and_load_all_xref(),
            LoadAllFile | Error => self.load_all_file(),
            Root => self.check_root(),
            Info => self.check_info(),
            PageTree => {
                if self.total_load_page_tree {
                    self.check_pages()
                } else {
                    self.load_doc_pages()
                }
            }
            Page => {
                if self.total_load_page_tree {
                    self.check_page_tree()
                } else {
                    self.status = PageLaterLoad;
                    true
                }
            }
            PageLaterLoad => {
                self.status = Page;
                self.doc_avail = true;
                true
            }
            Done => {
                self.doc_avail = true;
                true
            }
        }
    }
    fn load_all_file(&mut self) -> bool {
        if self.file_read.check_whole_file_and_request_if_unavailable() {
            self.status = InternalStatus::Done;
            true
        } else {
            false
        }
    }
    fn check_header_and_linearized(&mut self) -> DocAvailStatus {
        if self.header_avail {
            return DataAvailable;
        }
        let _session = ScopedSession::new(self.validator());
        let header = get_header_offset(self.file_read.as_ref());
        if self.file_read.has_read_problems() {
            return DataNotAvailable;
        }
        let Some(header) = header else {
            return DataError;
        };
        self.parser.init_availability(self.validator(), header);
        self.linearized =
            CpdfLinearizedHeader::parse(self.parser.syntax_for_availability().unwrap());
        if self.file_read.has_read_problems() {
            return DataNotAvailable;
        }
        self.header_avail = true;
        DataAvailable
    }
    pub fn is_linearized_pdf(&mut self) -> DocLinearizationStatus {
        match self.check_header_and_linearized() {
            DataAvailable => {
                if self.linearized.is_some() {
                    DocLinearizationStatus::Linearized
                } else {
                    DocLinearizationStatus::NotLinearized
                }
            }
            DataNotAvailable => DocLinearizationStatus::LinearizationUnknown,
            DataError => DocLinearizationStatus::NotLinearized,
        }
    }
    fn check_first_page(&mut self) -> bool {
        let header = self.linearized.as_ref().unwrap();
        if header.first_page_end_offset() == 0
            || header.file_size() == 0
            || header.main_xref_table_first_entry_offset() == 0
        {
            self.status = InternalStatus::Error;
            return false;
        }
        let end = header
            .first_page_end_offset()
            .saturating_add(512)
            .min(self.file_len);
        let start = self.file_len.min(1024);
        let size = end.saturating_sub(1024).max(0) as usize;
        if !self
            .file_read
            .check_data_range_and_request_if_unavailable(start, size)
        {
            return false;
        }
        self.status = InternalStatus::HintTable;
        true
    }
    fn check_hint_tables(&mut self) -> bool {
        let _session = ScopedSession::new(self.validator());
        self.hint_tables = CpdfHintTables::parse(
            self.parser.syntax_for_availability().unwrap(),
            self.linearized.as_ref().unwrap(),
        );
        if self.file_read.read_error() {
            self.status = InternalStatus::Error;
            return true;
        }
        if self.file_read.has_unavailable_data() {
            return false;
        }
        self.status = InternalStatus::Done;
        true
    }
    fn check_and_load_all_xref(&mut self) -> bool {
        if self.cross_ref_avail.is_none() {
            let _session = ScopedSession::new(self.validator());
            let offset = parse_start_xref(self.parser.syntax_for_availability().unwrap());
            if self.file_read.has_read_problems() {
                return false;
            }
            if offset <= 0 {
                self.status = InternalStatus::Error;
                return false;
            }
            self.cross_ref_avail = Some(
                CpdfCrossRefAvail::new(self.parser.syntax_for_availability().unwrap(), offset)
                    .into_state(),
            );
        }
        let state = self.cross_ref_avail.take().unwrap();
        let mut scanner =
            CpdfCrossRefAvail::from_state(self.parser.syntax_for_availability().unwrap(), state);
        let result = scanner.check_avail();
        let state = scanner.into_state();
        let offset = state.last_crossref_offset();
        self.cross_ref_avail = Some(state);
        match result {
            DataNotAvailable => return false,
            DataError => {
                self.status = InternalStatus::Error;
                return false;
            }
            DataAvailable => {}
        }
        if !self.parser.load_all_cross_refs_for_availability(offset) {
            self.status = InternalStatus::LoadAllFile;
            return false;
        }
        self.status = InternalStatus::Root;
        true
    }
    fn get_object(&self, number: u32) -> (Option<ObjectRef>, bool) {
        let _session = ScopedSession::new(self.validator());
        let reader = if let Some(document) = self.document.upgrade() {
            document.borrow().parser().availability_object_reader()
        } else {
            self.parser.availability_object_reader()
        };
        let object = reader(number);
        let exists = object.is_some();
        if self.file_read.has_read_problems() {
            (None, exists)
        } else {
            (object, exists)
        }
    }
    fn check_root(&mut self) -> bool {
        let number = self.parser.root_obj_num();
        if number == INVALID_OBJ_NUM {
            self.status = InternalStatus::Error;
            return true;
        }
        let _session = ScopedSession::new(self.validator());
        self.root = self
            .parser
            .parse_indirect_object(number)
            .and_then(|o| get_dict_object(&o));
        if self.file_read.has_read_problems() {
            return false;
        }
        let Some(number) = self
            .root
            .as_ref()
            .and_then(|r| raw(r, b"Pages"))
            .and_then(|r| reference(&r))
        else {
            self.status = InternalStatus::Error;
            return false;
        };
        self.pages_obj_num = number;
        self.status = InternalStatus::Info;
        true
    }
    fn check_info(&mut self) -> bool {
        let number = self.parser.info_obj_num();
        if number != INVALID_OBJ_NUM {
            let _session = ScopedSession::new(self.validator());
            self.parser.parse_indirect_object(number);
            if self.file_read.has_read_problems() {
                return false;
            }
        }
        self.status = InternalStatus::PageTree;
        true
    }
    fn prepare_page_item(&mut self) -> bool {
        let root = self.document.upgrade().and_then(|d| d.borrow().root());
        let Some(number) = root
            .and_then(|r| raw(&r, b"Pages"))
            .and_then(|r| reference(&r))
        else {
            self.status = InternalStatus::Error;
            return false;
        };
        self.pages_obj_num = number;
        self.status = InternalStatus::PageTree;
        true
    }
    fn get_page_kids(&mut self, pages: &ObjectRef) -> bool {
        let Some(kids) = raw(pages, b"Kids") else {
            return true;
        };
        let Some(numbers) = reference_children(&kids) else {
            self.status = InternalStatus::Error;
            return false;
        };
        for number in numbers {
            if self.seen_page_obj_list.insert(number) {
                self.page_obj_list.push(number);
            }
        }
        true
    }
    fn check_pages(&mut self) -> bool {
        let (pages, exists) = self.get_object(self.pages_obj_num);
        if !exists {
            self.status = InternalStatus::LoadAllFile;
            return true;
        }
        let Some(pages) = pages else {
            return false;
        };
        if !self.get_page_kids(&pages) {
            self.status = InternalStatus::Error;
            return false;
        }
        self.status = InternalStatus::Page;
        true
    }
    fn check_page_tree(&mut self) -> bool {
        let mut unavailable = Vec::new();
        for number in std::mem::take(&mut self.page_obj_list) {
            let (object, exists) = self.get_object(number);
            let Some(object) = object else {
                if exists {
                    unavailable.push(number);
                }
                continue;
            };
            if object.borrow().object_type() == ObjectType::Array {
                if let Some(numbers) = reference_children(&object) {
                    unavailable.extend(numbers);
                }
            } else if name(&object, b"Type") == b"Pages" {
                self.pages_array.push(object);
            }
        }
        if !unavailable.is_empty() {
            self.page_obj_list = unavailable;
            return false;
        }
        for pages in std::mem::take(&mut self.pages_array) {
            if !self.get_page_kids(&pages) {
                self.status = InternalStatus::Error;
                return false;
            }
        }
        if self.page_obj_list.is_empty() {
            self.status = InternalStatus::Done;
        }
        true
    }
    fn populate_page_node(&mut self, index: usize) -> bool {
        let number = self.page_nodes[index].number;
        let (object, exists) = self.get_object(number);
        if !exists {
            self.status = InternalStatus::Error;
            return false;
        }
        let Some(object) = object else {
            return false;
        };
        let array = object.borrow().object_type() == ObjectType::Array;
        if self.page_nodes[index].kind == PageNodeType::Unknown && array {
            self.page_nodes[index].kind = PageNodeType::Array;
            return true;
        }
        let (kind, numbers) = if array {
            (
                PageNodeType::Pages,
                reference_children(&object).unwrap_or_default(),
            )
        } else {
            match name(&object, b"Type").as_slice() {
                b"Page" => (PageNodeType::Page, Vec::new()),
                b"Pages" => (
                    PageNodeType::Pages,
                    raw(&object, b"Kids")
                        .and_then(|k| reference_children(&k))
                        .unwrap_or_default(),
                ),
                _ => {
                    self.status = InternalStatus::Error;
                    return false;
                }
            }
        };
        self.page_nodes[index].kind = kind;
        // Repeated probes replace the same node, never append duplicate children.
        if self.page_nodes[index].children.is_empty() {
            for number in numbers {
                let child = self.page_nodes.len();
                self.page_nodes.push(PageNode {
                    number,
                    ..PageNode::default()
                });
                self.page_nodes[index].children.push(child);
            }
        }
        true
    }
    fn load_doc_pages(&mut self) -> bool {
        self.page_nodes[0].number = self.pages_obj_num;
        if !self.populate_page_node(0) {
            return false;
        }
        let (pages, exists) = self.get_object(self.pages_obj_num);
        if !exists {
            self.status = InternalStatus::Error;
            return false;
        }
        let Some(pages) = pages else {
            return false;
        };
        if raw(&pages, b"Kids").is_none() || integer(&pages, b"Count") > 0 {
            self.status = InternalStatus::Page;
            true
        } else {
            self.total_load_page_tree = true;
            // Stop this page-number scan. Its caller must re-enter through
            // the whole-tree loading path selected by the new flag.
            false
        }
    }
    fn load_doc_page(&mut self, page: u32) -> bool {
        let Some(document) = self.document.upgrade() else {
            return false;
        };
        if page as usize >= document.borrow().page_count()
            || document.borrow().is_page_loaded(page as usize)
        {
            self.status = InternalStatus::Done;
            return true;
        }
        if self.page_nodes[0].kind == PageNodeType::Page {
            self.status = if page == 0 {
                InternalStatus::Done
            } else {
                InternalStatus::Error
            };
            return true;
        }
        // Explicit stack preserves the depth limit without a native recursion stack.
        // The source's local child-count/global-page comparison rejects valid nested
        // page trees; count actual leaf pages instead.
        let mut stack = vec![(0usize, 0usize, 0usize)];
        let mut count = 0u32;
        let mut active = BTreeSet::new();
        active.insert(self.pages_obj_num);
        while let Some((node, child, depth)) = stack.pop() {
            if depth >= 1024 {
                self.status = InternalStatus::Error;
                return false;
            }
            if child >= self.page_nodes[node].children.len() {
                active.remove(&self.page_nodes[node].number);
                continue;
            }
            stack.push((node, child + 1, depth));
            let index = self.page_nodes[node].children[child];
            if self.page_nodes[index].kind == PageNodeType::Unknown
                && !self.populate_page_node(index)
            {
                return false;
            }
            if self.page_nodes[index].kind == PageNodeType::Array && !self.populate_page_node(index)
            {
                return false;
            }
            match self.page_nodes[index].kind {
                PageNodeType::Page => {
                    if count == page {
                        document
                            .borrow_mut()
                            .set_page_obj_num(page as usize, self.page_nodes[index].number);
                        self.status = InternalStatus::Done;
                        return true;
                    }
                    count += 1;
                }
                PageNodeType::Pages => {
                    if !active.insert(self.page_nodes[index].number) {
                        self.status = InternalStatus::Error;
                        return false;
                    }
                    stack.push((index, 0, depth + 1));
                }
                _ => return false,
            }
        }
        self.status = InternalStatus::Error;
        false
    }
    fn check_page_number(&mut self, page: u32) -> bool {
        loop {
            match self.status {
                InternalStatus::PageTree => {
                    if !self.load_doc_pages() {
                        return false;
                    }
                }
                InternalStatus::Page => {
                    if !self.load_doc_page(page) {
                        return false;
                    }
                }
                InternalStatus::Error => return self.load_all_file(),
                _ => {
                    self.pages_tree_load = true;
                    self.pages_load = true;
                    self.cur_page_dict_load_ok = true;
                    self.status = InternalStatus::Page;
                    return true;
                }
            }
        }
    }
    fn load_pages(&mut self) -> bool {
        while !self.pages_tree_load {
            let ready = match self.status {
                InternalStatus::PageTree => self.check_pages(),
                InternalStatus::Page => self.check_page_tree(),
                InternalStatus::Error => self.load_all_file(),
                _ => {
                    self.pages_tree_load = true;
                    self.pages_load = true;
                    true
                }
            };
            if !ready {
                return false;
            }
        }
        if self.pages_load {
            return true;
        }
        if let Some(document) = self.document.upgrade() {
            document.borrow_mut().load_pages();
        }
        false
    }
    fn check_linearized_data(&mut self) -> DocAvailStatus {
        if self.linearized_data_ok {
            return DataAvailable;
        }
        let Some(document) = self.document.upgrade() else {
            return DataError;
        };
        if self
            .linearized
            .as_ref()
            .is_none_or(|h| h.main_xref_table_first_entry_offset() == 0)
        {
            return DataError;
        }
        let trailer = document.borrow().parser().trailer();
        let Some(trailer) = trailer else {
            return DataError;
        };
        if !self.main_xref_load_tried {
            let offset = i64::from(integer(&trailer, b"Prev"));
            if offset < 0 {
                return DataError;
            }
            if offset == 0 {
                return DataAvailable;
            }
            let Some(size) = self
                .file_len
                .checked_sub(offset)
                .and_then(|n| usize::try_from(n).ok())
            else {
                return DataError;
            };
            if !self
                .file_read
                .check_data_range_and_request_if_unavailable(offset, size)
            {
                return DataNotAvailable;
            }
            let loader = document
                .borrow()
                .parser()
                .availability_main_xref_loader(offset);
            let (result, xref_stream) = loader();
            document
                .borrow_mut()
                .parser_mut()
                .finish_availability_main_xref(result, xref_stream);
            self.main_xref_load_tried = true;
            if result != ParserError::Success {
                return DataError;
            }
            if !self.prepare_page_item() {
                return DataNotAvailable;
            }
            self.main_xref_loaded_ok = true;
            self.linearized_data_ok = true;
        }
        if self.linearized_data_ok {
            DataAvailable
        } else {
            DataNotAvailable
        }
    }
    pub fn is_page_avail(
        &mut self,
        page: u32,
        hints: Option<&Rc<RefCell<dyn DownloadHints>>>,
    ) -> DocAvailStatus {
        let Some(document) = self.live_document() else {
            return DataError;
        };
        if page as usize >= document.borrow().page_count() {
            return DataAvailable;
        }
        if self.page_map_check_state.insert(page) {
            self.cur_page_dict_load_ok = false;
        }
        if self.pages_load_state.contains(&page) {
            return DataAvailable;
        }
        let _hints = HintsScope::new(self.validator(), hints);
        if let Some(header) = self.linearized.as_ref() {
            if page == header.first_page_no() {
                let Some(dict) = Self::known_page_dictionary(&document, page) else {
                    return DataError;
                };
                return self.check_page_objects(page, dict, &document);
            }
            let status = self.check_linearized_data();
            if status != DataAvailable {
                return status;
            }
            if let Some(hints) = self.hint_tables.as_ref() {
                let status = hints.check_page(page);
                if status != DataAvailable {
                    return status;
                }
                if self.get_page_dictionary(page).is_some() {
                    self.pages_load_state.insert(page);
                    return DataAvailable;
                }
            }
            if !self.main_xref_loaded_ok {
                if !self.load_all_file() {
                    return DataNotAvailable;
                }
                let rebuild = document.borrow().parser().availability_rebuilder();
                let success = rebuild();
                document
                    .borrow_mut()
                    .parser_mut()
                    .finish_availability_rebuild(success);
                self.page_map_check_state.remove(&page);
                return DataAvailable;
            }
            if self.total_load_page_tree {
                if !self.load_pages() {
                    return DataNotAvailable;
                }
            } else if !self.cur_page_dict_load_ok && !self.check_page_number(page) {
                return DataNotAvailable;
            }
        } else if !self.total_load_page_tree
            && !self.cur_page_dict_load_ok
            && !self.check_page_number(page)
        {
            return DataNotAvailable;
        }
        if self.check_acro_form() == DocFormStatus::FormNotAvailable {
            return DataNotAvailable;
        }
        // Resolve through the live document, as GetMutablePageDictionary does
        // in the source. A whole-file fallback may leave the availability
        // scanner's page_nodes unpopulated even though the document's parser
        // has rebuilt/loaded its complete cross-reference table.
        let dict = {
            let _session = ScopedSession::new(self.validator());
            let dict = document.borrow_mut().page_dictionary(page as usize);
            if self.file_read.read_error() {
                return DataError;
            }
            if self.file_read.has_unavailable_data() {
                return DataNotAvailable;
            }
            dict
        };
        let Some(dict) = dict else {
            return DataError;
        };
        let status = self.check_page_objects(page, dict.clone(), &document);
        if status != DataAvailable {
            return status;
        }
        let status = self.check_resources(dict, &document);
        if status != DataAvailable {
            return status;
        }
        self.cur_page_dict_load_ok = false;
        self.page_map_check_state.remove(&page);
        self.pages_load_state.insert(page);
        DataAvailable
    }
    fn check_page_objects(
        &mut self,
        page: u32,
        dict: ObjectRef,
        document: &Rc<RefCell<CpdfDocument>>,
    ) -> DocAvailStatus {
        let holder = document.borrow().holder();
        let validator = self.validator();
        self.pages_obj_avail
            .entry(page)
            .or_insert_with(|| CpdfPageObjectAvail::from_root(validator, &holder, dict))
            .check_avail()
    }
    fn check_resources(
        &mut self,
        mut page: ObjectRef,
        document: &Rc<RefCell<CpdfDocument>>,
    ) -> DocAvailStatus {
        let _session = ScopedSession::new(self.validator());
        let mut resources = None;
        for _ in 0..=64 {
            if let Some(value) = raw(&page, b"Resources") {
                resources = Some(value);
                break;
            }
            let Some(parent) = raw(&page, b"Parent").and_then(|p| get_dict_object(&p)) else {
                break;
            };
            page = parent;
        }
        if self.file_read.has_read_problems() {
            return DataNotAvailable;
        }
        let Some(resources) = resources else {
            return DataAvailable;
        };
        let key = Rc::as_ptr(&resources) as *const () as usize;
        let holder = document.borrow().holder();
        let validator = self.validator();
        self.pages_resources_avail
            .entry(key)
            .or_insert_with(|| {
                let retained = resources.clone();
                (
                    retained,
                    CpdfPageObjectAvail::from_root(validator, &holder, resources),
                )
            })
            .1
            .check_avail()
    }
    fn known_page_dictionary(
        document: &Rc<RefCell<CpdfDocument>>,
        index: u32,
    ) -> Option<ObjectRef> {
        let (number, holder) = {
            let doc = document.borrow();
            (
                doc.page_obj_num_for_availability(index as usize)?,
                doc.holder(),
            )
        };
        let object = CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, number)?;
        let is_dict = object.borrow().object_type() == ObjectType::Dictionary;
        is_dict.then_some(object)
    }
    pub fn get_page_dictionary(&mut self, index: u32) -> Option<ObjectRef> {
        let document = self.live_document()?;
        if index as usize >= self.get_page_count() {
            return None;
        }
        if let Some(page) = Self::known_page_dictionary(&document, index) {
            return Some(page);
        }
        if self.linearized.is_none() || self.hint_tables.is_none() {
            if self.page_nodes[0].number == 0 {
                if !self.prepare_page_item() || !self.load_doc_pages() {
                    return None;
                }
            }
            if !self.load_doc_page(index) {
                return None;
            }
            return Self::known_page_dictionary(&document, index);
        }
        let header = self.linearized.as_ref()?;
        if index == header.first_page_no() {
            return None;
        }
        let (offset, _, number) = self.hint_tables.as_ref()?.get_page_pos(index)?;
        if number == 0 {
            return None;
        }
        document
            .borrow_mut()
            .set_page_obj_num(index as usize, number);
        let holder = document.borrow().holder();
        if holder.borrow().get_indirect_object(number).is_none() {
            let read = document.borrow().parser().availability_object_at_reader();
            let object = read(offset, number);
            if let Some(object) = object {
                holder
                    .borrow_mut()
                    .replace_indirect_object_if_higher_generation(number, Some(object));
            }
        }
        let page = Self::known_page_dictionary(&document, index)?;
        if CpdfPageObjectAvail::from_root(self.validator(), &holder, page.clone()).check_avail()
            != DataAvailable
        {
            return None;
        }
        Some(page)
    }
    pub fn is_form_avail(
        &mut self,
        hints: Option<&Rc<RefCell<dyn DownloadHints>>>,
    ) -> DocFormStatus {
        let _hints = HintsScope::new(self.validator(), hints);
        self.check_acro_form()
    }
    fn check_acro_form(&mut self) -> DocFormStatus {
        use DocFormStatus::*;
        let Some(document) = self.live_document() else {
            return FormAvailable;
        };
        if self.linearized.is_some() {
            match self.check_linearized_data() {
                DataError => return FormError,
                DataNotAvailable => return FormNotAvailable,
                DataAvailable => {}
            }
        }
        if self.form_avail.is_none() {
            let Some(root) = document.borrow().root() else {
                return FormAvailable;
            };
            let Some(form) = raw(&root, b"AcroForm") else {
                return FormNotExist;
            };
            let holder = document.borrow().holder();
            self.form_avail = Some(CpdfPageObjectAvail::from_root(
                self.validator(),
                &holder,
                form,
            ));
        }
        match self.form_avail.as_mut().unwrap().check_avail() {
            DataError => FormError,
            DataNotAvailable => FormNotAvailable,
            DataAvailable => FormAvailable,
        }
    }
    pub fn parse_document(
        &mut self,
        password: &[u8],
    ) -> Result<Rc<RefCell<CpdfDocument>>, ParserError> {
        // Native OnObservableDestroyed clears per-document scans. Perform the
        // same weak-owner cleanup before replacing a document as well as on
        // page/form queries, so an immediate reparse cannot retain old scans.
        if self.live_document().is_some() {
            return Err(ParserError::HandlerError);
        }
        let mut document = CpdfDocument::new();
        let _session = ScopedSession::new(self.validator());
        let error = document.load_linearized_doc(self.validator(), password);
        if self.file_read.has_read_problems() {
            return Err(ParserError::HandlerError);
        }
        if error != ParserError::Success {
            return Err(error);
        }
        let document = Rc::new(RefCell::new(document));
        self.document = Rc::downgrade(&document);
        Ok(document)
    }
}

#[cfg(test)]
#[path = "cpdf_data_avail_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cpdf_data_avail_resource_tests.rs"]
mod resource_tests;
