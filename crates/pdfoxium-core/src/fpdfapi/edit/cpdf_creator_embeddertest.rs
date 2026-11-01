// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/edit/cpdf_creator_embeddertest.cpp`: the first two cases are
//! complete engine-level ports. SaveLinearizedInfo preserves the partial-input
//! parser, second-page/main-xref, full-download and save assertions, but remains
//! partial native coverage: CPDF_DataAvail's dispatcher and hint tables are not
//! yet ported. Its lower-level availability checks use real read validators.
#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use super::CpdfCreator;
use crate::fpdfapi::page::cpdf_occontext::{CpdfOcContext, UsageType};
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_cross_ref_avail::DocAvailStatus;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_linearized_header::CpdfLinearizedHeader;
use crate::fpdfapi::parser::cpdf_page_object_avail::CpdfPageObjectAvail;
use crate::fpdfapi::parser::cpdf_parser::ParserError;
use crate::fpdfapi::parser::cpdf_read_validator::{CpdfReadValidator, DownloadHints, FileAvail};
use crate::fpdfapi::parser::cpdf_syntax_parser::CpdfSyntaxParser;
use crate::fpdfapi::render::cpdf_renderoptions::CpdfRenderOptions;
use crate::fpdfapi::render::cpdf_renderstatus::render_page_objects;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize, WriteStream};
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::FxdibFormat;

fn fixture(filename: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../upstream/pdfium/testing/resources")
        .join(filename);
    std::fs::read(path).unwrap()
}

fn open_document(filename: &str) -> Rc<RefCell<CpdfDocument>> {
    let mut doc = CpdfDocument::new();
    assert_eq!(
        doc.load_doc(
            Rc::new(CfxReadOnlyContainerStream::new(fixture(filename))),
            b""
        ),
        ParserError::Success
    );
    Rc::new(RefCell::new(doc))
}

#[derive(Default)]
struct SavedData(Vec<u8>);
impl WriteStream for SavedData {
    fn write_block(&mut self, bytes: &[u8]) -> bool {
        self.0.extend_from_slice(bytes);
        true
    }
}

fn save(doc: &Rc<RefCell<CpdfDocument>>) -> Vec<u8> {
    let mut saved = SavedData::default();
    {
        let mut creator = CpdfCreator::new(Rc::clone(doc), &mut saved);
        // FPDF_SaveAsCopy(document,writer,0) uses the default file version.
        assert!(creator.create(0, 0));
    }
    saved.0
}

#[test]
fn SavedDocsAreEqualAfterParse() {
    let doc = open_document("annotation_stamp_with_ap.pdf");
    let saved_doc_1 = save(&doc);
    {
        // Preserve the native ASSERT_GE(1,page_count), including its order.
        assert!(1 >= doc.borrow().page_count());
        let mut page = CpdfPage::from_document(&mut doc.borrow_mut(), 0).unwrap();
        page.parse_path_content();
        let width = page.width() as i32;
        let height = page.height() as i32;
        let alpha = page.holder().background_alpha_needed();
        let mut bitmap = CfxDibitmap::create(
            width,
            height,
            if alpha {
                FxdibFormat::Bgra
            } else {
                FxdibFormat::Bgrx
            },
            0,
        )
        .unwrap();
        bitmap.clear(if alpha { 0 } else { 0xffffffff });
        let mut options = CpdfRenderOptions::new();
        // RenderLoadedPageWithFlags(page,FPDF_ANNOT): actual annotation AP
        // loading/rendering must happen between the two saves.
        options.set_draw_annots(true);
        options.set_oc_context(Some(Rc::new(CpdfOcContext::new(
            &doc.borrow(),
            UsageType::View,
        ))));
        let matrix = page.display_matrix_for_rect(FxRect::new(0, 0, width, height), 0);
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            matrix,
            &options,
            false
        ));
        assert_eq!(bitmap.width(), 595);
        assert_eq!(bitmap.height(), 842);
    }
    let saved_doc_2 = save(&doc);
    assert_eq!(saved_doc_1.len(), saved_doc_2.len());
}

#[test]
fn Bug873() {
    let doc = open_document("embedded_attachments.pdf");
    let saved_data = save(&doc);
    // The second ID is random; retain the exact first-ID and trailer checks.
    let before_second_id =
        b"trailer\r\n<</Info 9 0 R /Root 11 0 R /Size 36/ID[<D889EB6B9ADF88E5EDA7DC08FE85978B><";
    let trailer_start = saved_data
        .windows(before_second_id.len())
        .position(|part| part == before_second_id)
        .unwrap();
    let trailer_continuation = trailer_start + before_second_id.len() + 32;
    assert!(saved_data[trailer_continuation..].starts_with(b">]>>\r\n"));
}

// Direct test-only translation of testing/FakeFileAccess's range availability,
// requesting, downloading and read checks. The source bytes stay in one buffer.
struct FakeFileAccess {
    bytes: Vec<u8>,
    available: RefCell<Vec<(usize, usize)>>,
    requested: RefCell<Vec<(usize, usize)>>,
}
impl FakeFileAccess {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            available: RefCell::new(Vec::new()),
            requested: RefCell::new(Vec::new()),
        }
    }
    fn set_requested_data_available(&self) {
        let mut available = self.available.borrow_mut();
        available.append(&mut self.requested.borrow_mut());
        available.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for range in available.drain(..) {
            if let Some(previous) = merged.last_mut().filter(|previous| previous.1 >= range.0) {
                previous.1 = previous.1.max(range.1);
            } else {
                merged.push(range);
            }
        }
        *available = merged;
    }
    fn set_whole_file_available(&self) {
        *self.available.borrow_mut() = vec![(0, self.bytes.len())];
        self.requested.borrow_mut().clear();
    }
    fn download_requested(&self) {
        assert!(
            !self.requested.borrow().is_empty(),
            "unavailable operation must request input"
        );
        self.set_requested_data_available();
    }
}
impl StreamWithSize for FakeFileAccess {
    fn get_size(&self) -> i64 {
        self.bytes.len() as i64
    }
}
impl FileAvail for FakeFileAccess {
    fn is_data_avail(&self, offset: i64, size: usize) -> bool {
        usize::try_from(offset)
            .ok()
            .and_then(|start| start.checked_add(size).map(|end| (start, end)))
            .is_some_and(|(start, end)| {
                self.available
                    .borrow()
                    .iter()
                    .any(|range| range.0 <= start && end <= range.1)
            })
    }
}
impl SeekableReadStream for FakeFileAccess {
    fn read_block_at_offset(&self, buffer: &mut [u8], offset: i64) -> bool {
        if buffer.is_empty() || !self.is_data_avail(offset, buffer.len()) {
            return false;
        }
        let Some(source) = usize::try_from(offset)
            .ok()
            .and_then(|start| self.bytes.get(start..start.checked_add(buffer.len())?))
        else {
            return false;
        };
        buffer.copy_from_slice(source);
        true
    }
}
struct FakeDownloadHints(Rc<FakeFileAccess>);
impl DownloadHints for FakeDownloadHints {
    fn add_segment(&mut self, offset: i64, size: usize) {
        let start = usize::try_from(offset).unwrap();
        self.0
            .requested
            .borrow_mut()
            .push((start, start.checked_add(size).unwrap()));
    }
}

#[test]
fn SaveLinearizedInfo() {
    // Partial native coverage: perform the same real parser/main-xref and page
    // availability operations directly. The missing DataAvail dispatcher and
    // hint-table state machine cannot be represented as a completed case yet.
    let fake = Rc::new(FakeFileAccess::new(fixture("linearized.pdf")));
    let available: Rc<dyn FileAvail> = fake.clone();
    let validator = Rc::new(CpdfReadValidator::new(fake.clone(), Some(&available)));
    let hints: Rc<RefCell<dyn DownloadHints>> =
        Rc::new(RefCell::new(FakeDownloadHints(Rc::clone(&fake))));
    validator.set_download_hints(Some(&hints));

    // CheckHeaderAndLinearized and CheckFirstPage request only their ranges.
    while !validator.check_data_range_and_request_if_unavailable(0, 1024) {
        fake.download_requested();
    }
    let mut syntax = CpdfSyntaxParser::new(Rc::clone(&validator), 0);
    let header = CpdfLinearizedHeader::parse(&mut syntax).unwrap();
    let first_page_end = (header.first_page_end_offset() + 512).min(fake.get_size());
    while !validator
        .check_data_range_and_request_if_unavailable(1024, (first_page_end - 1024).max(0) as usize)
    {
        fake.download_requested();
    }
    validator.reset_errors();
    let mut doc = CpdfDocument::new();
    assert_eq!(
        doc.load_linearized_doc(Rc::clone(&validator), b""),
        ParserError::Success
    );
    assert!(doc.root().is_some());
    assert!(!validator.is_whole_file_available());

    // Loading page 1 invokes CheckLinearizedData's main-xref range and merge.
    let trailer = doc.parser().trailer().unwrap();
    let main_xref_offset = trailer
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .get_integer_for(b"Prev", 0) as i64;
    assert!(main_xref_offset > 0);
    while !validator.check_data_range_and_request_if_unavailable(
        main_xref_offset,
        (fake.get_size() - main_xref_offset) as usize,
    ) {
        fake.download_requested();
    }
    validator.reset_errors();
    assert_eq!(
        doc.parser_mut().load_linearized_main_xref_table(),
        ParserError::Success
    );
    let second_page = loop {
        validator.reset_errors();
        if let Some(page) = doc.page_dictionary(1) {
            break page;
        }
        fake.download_requested();
    };
    let mut page_avail =
        CpdfPageObjectAvail::from_root(Rc::clone(&validator), &doc.holder(), second_page);
    while page_avail.check_avail() != DocAvailStatus::DataAvailable {
        fake.download_requested();
    }
    let resources = loop {
        validator.reset_errors();
        let page = CpdfPage::from_document(&mut doc, 1).unwrap();
        let resources = page.holder().resources().cloned();
        if !validator.has_unavailable_data() {
            break resources;
        }
        fake.download_requested();
    };
    if let Some(resources) = resources {
        let mut resource_avail =
            CpdfPageObjectAvail::from_root(Rc::clone(&validator), &doc.holder(), resources);
        while resource_avail.check_avail() != DocAvailStatus::DataAvailable {
            fake.download_requested();
        }
    }
    fake.set_whole_file_available();
    let saved_doc = save(&Rc::new(RefCell::new(doc)));
    assert!(saved_doc
        .windows(b"/Info".len())
        .any(|part| part == b"/Info"));
}
