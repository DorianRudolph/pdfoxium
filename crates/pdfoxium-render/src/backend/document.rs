// Copyright 2026 The PDFium Authors. BSD-3-Clause; see upstream/pdfium/LICENSE.
use crate::backend::prelude::*;
use pdfoxium_core::{
    fpdfapi::{
        page::{cpdf_page::CpdfPage, cpdf_pagemodule::initialize_page_module},
        parser::{cpdf_document::CpdfDocument, cpdf_parser::ParserError},
    },
    fpdfsdk::{fpdf_editpage, fpdf_ppo},
    fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream,
    fxge::cfx_gemodule::initialize_ge_module,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::{Read, Seek},
    ops::RangeInclusive,
    path::Path,
    rc::Rc,
};

#[derive(Default)]
pub struct Pdfium;
impl Pdfium {
    pub fn create_new_pdf(&self) -> Result<PdfDocument, PdfiumError> {
        initialize();
        Ok(PdfDocument::from_core(fpdf_editpage::create_new_document(
            false,
            || None,
        )))
    }
    pub fn load_pdf_from_file(
        &self,
        path: impl AsRef<Path>,
        password: Option<&str>,
    ) -> Result<PdfDocument, PdfiumError> {
        self.load_pdf_from_byte_vec(std::fs::read(path)?, password)
    }
    pub fn load_pdf_from_byte_slice(
        &self,
        bytes: &[u8],
        password: Option<&str>,
    ) -> Result<PdfDocument, PdfiumError> {
        self.load_pdf_from_byte_vec(bytes.to_vec(), password)
    }
    pub fn load_pdf_from_byte_vec(
        &self,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> Result<PdfDocument, PdfiumError> {
        initialize();
        let mut document = CpdfDocument::new();
        match document.load_doc(
            Rc::new(CfxReadOnlyContainerStream::new(bytes)),
            password.unwrap_or_default().as_bytes(),
        ) {
            ParserError::Success => Ok(PdfDocument::from_core(Rc::new(RefCell::new(document)))),
            ParserError::PasswordError => Err(PdfiumError::PdfiumLibraryInternalError(
                crate::PdfiumInternalError::PasswordError,
            )),
            ParserError::HandlerError => Err(PdfiumError::PdfiumLibraryInternalError(
                crate::PdfiumInternalError::SecurityError,
            )),
            ParserError::FileError => Err(PdfiumError::PdfiumLibraryInternalError(
                crate::PdfiumInternalError::FileError,
            )),
            ParserError::FormatError => Err(PdfiumError::PdfiumLibraryInternalError(
                crate::PdfiumInternalError::FormatError,
            )),
        }
    }
    pub fn load_pdf_from_reader(
        &self,
        mut reader: impl Read + Seek,
        password: Option<&str>,
    ) -> Result<PdfDocument, PdfiumError> {
        reader.rewind()?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        self.load_pdf_from_byte_vec(bytes, password)
    }
}
fn initialize() {
    initialize_ge_module(None);
    initialize_page_module();
}

pub(crate) struct DocumentState {
    // Parsed pages must be destroyed before their document and backing input.
    pages: RefCell<BTreeMap<usize, Rc<PageState>>>,
    pub core: Rc<RefCell<CpdfDocument>>,
    pub fonts: RefCell<Vec<pdfoxium_core::fpdfapi::font::cpdf_font::CpdfFont>>,
    pub identity: u64,
    font_indices: RefCell<std::collections::HashMap<usize, usize>>,
    pub(crate) annotations: RefCell<
        std::collections::HashMap<usize, std::rc::Weak<crate::annotations::AnnotationState>>,
    >,
    pub(crate) objects: RefCell<
        std::collections::HashMap<
            (
                pdfoxium_core::fpdfapi::page::cpdf_pageobjectholder::PageObjectId,
                u64,
                u64,
            ),
            std::rc::Weak<RefCell<crate::object_store::Location>>,
        >,
    >,
}
pub(crate) struct PageState {
    pub core: RefCell<Rc<CpdfPage>>,
    pub text_cache: RefCell<Option<Rc<pdfoxium_core::fpdftext::cpdf_textpage::CpdfTextPage>>>,
    pub(crate) index: Cell<Option<usize>>,
    pub(crate) epoch: Cell<u64>,
    pub(crate) strategy: Cell<crate::document::PdfPageContentRegenerationStrategy>,
}
pub struct PdfDocument {
    pub(crate) inner: Rc<DocumentState>,
}
impl PdfDocument {
    pub(crate) fn from_core(core: Rc<RefCell<CpdfDocument>>) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let identity = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self {
            inner: Rc::new(DocumentState {
                pages: RefCell::new(BTreeMap::new()),
                core,
                fonts: RefCell::new(Vec::new()),
                identity,
                font_indices: RefCell::new(std::collections::HashMap::new()),
                annotations: RefCell::new(std::collections::HashMap::new()),
                objects: RefCell::new(std::collections::HashMap::new()),
            }),
        }
    }
    pub fn pages(&self) -> PdfPages {
        PdfPages {
            inner: self.inner.clone(),
        }
    }
}

pub type PdfPageIndex = i32;
pub struct PdfPages {
    inner: Rc<DocumentState>,
}
impl PdfPages {
    pub fn len(&self) -> PdfPageIndex {
        self.inner
            .core
            .borrow()
            .page_count()
            .try_into()
            .expect("pdfium-render page index capacity exceeded")
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: PdfPageIndex) -> Result<PdfPage, PdfiumError> {
        let index = index as usize;
        if index >= self.inner.core.borrow().page_count() {
            return Err(PdfiumError::PageIndexOutOfBounds);
        }
        if let Some(state) = self.inner.pages.borrow().get(&index) {
            return Ok(PdfPage {
                state: state.clone(),
                document: self.inner.clone(),
            });
        }
        let mut page = CpdfPage::from_document(&mut self.inner.core.borrow_mut(), index)
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        // Parsing resolves shared resources, so no document borrow may be held here.
        page.parse_path_content();
        Ok(self.register(index, page))
    }
    fn register(&self, index: usize, page: CpdfPage) -> PdfPage {
        let state = Rc::new(PageState {
            core: RefCell::new(Rc::new(page)),
            text_cache: RefCell::new(None),
            index: Cell::new(Some(index)),
            epoch: Cell::new(0),
            strategy: Cell::new(
                crate::document::PdfPageContentRegenerationStrategy::AutomaticOnEveryChange,
            ),
        });
        self.inner.pages.borrow_mut().insert(index, state.clone());
        PdfPage {
            state,
            document: self.inner.clone(),
        }
    }

    pub fn page_size(&self, index: PdfPageIndex) -> Result<PdfRect, PdfiumError> {
        Ok(self.get(index)?.page_size())
    }
    pub fn page_sizes(&self) -> Result<Vec<PdfRect>, PdfiumError> {
        (0..self.len()).map(|i| self.page_size(i)).collect()
    }
    fn shift(&self, index: usize, count: usize) {
        let mut pages = self.inner.pages.borrow_mut();
        let tail = pages.split_off(&index);
        for (i, state) in tail {
            state.index.set(Some(i + count));
            pages.insert(i + count, state);
        }
    }
    pub fn create_page_at_index(
        &mut self,
        size: PdfPagePaperSize,
        index: PdfPageIndex,
    ) -> Result<PdfPage, PdfiumError> {
        if index < 0 || index > self.len() {
            return Err(PdfiumError::PageIndexOutOfBounds);
        }
        let (w, h) = (size.width().value, size.height().value);
        if !w.is_finite() || !h.is_finite() || w <= 0. || h <= 0. {
            return Err(PdfiumError::ImageSizeOutOfBounds);
        }
        let page = fpdf_editpage::new_page(&self.inner.core, i32::from(index), w as f64, h as f64)
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        self.shift(index as usize, 1);
        Ok(self.register(index as usize, page))
    }
    pub fn create_page_at_start(&mut self, size: PdfPagePaperSize) -> Result<PdfPage, PdfiumError> {
        self.create_page_at_index(size, 0)
    }
    pub fn create_page_at_end(&mut self, size: PdfPagePaperSize) -> Result<PdfPage, PdfiumError> {
        self.create_page_at_index(size, self.len())
    }
    pub fn delete_page_at_index(&mut self, index: PdfPageIndex) -> Result<(), PdfiumError> {
        if index < 0 || index >= self.len() {
            return Err(PdfiumError::PageIndexOutOfBounds);
        }
        let number = self.inner.core.borrow_mut().delete_page(i32::from(index));
        if number == 0 {
            return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
        }
        let mut pages = self.inner.pages.borrow_mut();
        if let Some(state) = pages.remove(&(index as usize)) {
            state.index.set(None);
        }
        let tail = pages.split_off(&(index as usize));
        for (i, state) in tail {
            state.index.set(Some(i - 1));
            pages.insert(i - 1, state);
        }
        self.inner.core.borrow_mut().set_page_to_null_object(number);
        Ok(())
    }
    pub fn copy_page_range_from_document(
        &mut self,
        source: &PdfDocument,
        range: RangeInclusive<PdfPageIndex>,
        destination: PdfPageIndex,
    ) -> Result<(), PdfiumError> {
        if range.is_empty()
            || *range.start() < 0
            || *range.end() >= source.pages().len()
            || destination < 0
            || destination > self.len()
        {
            return Err(PdfiumError::PageIndexOutOfBounds);
        }
        self.copy_pages_from_document(source, &range.collect::<Vec<_>>(), destination)
    }
    pub fn copy_pages_from_document(
        &mut self,
        source: &PdfDocument,
        indices: &[PdfPageIndex],
        destination: PdfPageIndex,
    ) -> Result<(), PdfiumError> {
        if Rc::ptr_eq(&self.inner, &source.inner) {
            return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
        }
        if destination < 0
            || destination > self.len()
            || indices.iter().any(|i| *i < 0 || *i >= source.pages().len())
        {
            return Err(PdfiumError::PageIndexOutOfBounds);
        }
        let before = self.inner.core.borrow().page_count();
        let selected: Vec<i32> = indices.iter().copied().map(i32::from).collect();
        let ok = fpdf_ppo::import_pages_by_index(
            self.inner.core.clone(),
            source.inner.core.clone(),
            Some(&selected),
            i32::from(destination),
        );
        let inserted = self.inner.core.borrow().page_count() - before;
        self.shift(destination as usize, inserted);
        if ok {
            Ok(())
        } else {
            Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
        }
    }
    pub fn append(&mut self, source: &PdfDocument) -> Result<(), PdfiumError> {
        self.copy_pages_from_document(
            source,
            &(0..source.pages().len()).collect::<Vec<_>>(),
            self.len(),
        )
    }
}

pub struct PdfPage {
    pub(crate) state: Rc<PageState>,
    pub(crate) document: Rc<DocumentState>,
}
impl PdfPage {
    pub(crate) fn check_live(&self) -> Result<(), PdfiumError> {
        if self.state.index.get().is_some() {
            Ok(())
        } else {
            Err(PdfiumError::OwnershipNotAttachedToPage)
        }
    }
    pub fn width(&self) -> PdfPoints {
        PdfPoints::new(self.state.core.borrow().width())
    }
    pub fn height(&self) -> PdfPoints {
        PdfPoints::new(self.state.core.borrow().height())
    }
    pub fn page_size(&self) -> PdfRect {
        PdfRect::new(
            PdfPoints::ZERO,
            PdfPoints::ZERO,
            self.height(),
            self.width(),
        )
    }
    pub fn is_landscape(&self) -> bool {
        self.width() > self.height()
    }
    pub fn is_portrait(&self) -> bool {
        !self.is_landscape()
    }

    pub fn render(
        &self,
        width: Pixels,
        height: Pixels,
        rotation: Option<PdfPageRenderRotation>,
    ) -> Result<PdfBitmap<'static>, PdfiumError> {
        self.render_with_config(
            &PdfRenderConfig::new()
                .set_target_size(width, height)
                .rotate(rotation.unwrap_or_default(), false),
        )
    }
    pub fn render_with_config(
        &self,
        config: &PdfRenderConfig,
    ) -> Result<PdfBitmap<'static>, PdfiumError> {
        self.check_live()?;
        let settings = config.apply_to_page(self);
        let mut bitmap = PdfBitmap::empty(settings.width, settings.height, settings.format)?;
        self.render_into_bitmap_with_config(&mut bitmap, config)?;
        Ok(bitmap)
    }
}

impl PdfPage {
    pub(crate) fn index(&self) -> i32 {
        self.state
            .index
            .get()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1)
    }
    pub(crate) fn reload_core(&self) -> Result<(), PdfiumError> {
        self.check_live()?;
        let mut page =
            CpdfPage::from_document(&mut self.document.core.borrow_mut(), self.index() as usize)
                .ok_or(PdfiumError::PdfiumLibraryInternalError(
                    crate::PdfiumInternalError::FormatError,
                ))?;
        page.parse_path_content();
        *self.state.core.borrow_mut() = Rc::new(page);
        self.state.text_cache.borrow_mut().take();
        self.state.epoch.set(
            self.state
                .epoch
                .get()
                .checked_add(1)
                .expect("page residency exhausted"),
        );
        Ok(())
    }
}

impl Clone for PdfPage {
    fn clone(&self) -> Self {
        self.alias()
    }
}

impl DocumentState {
    pub(crate) fn register_font(
        &self,
        font: &pdfoxium_core::fpdfapi::font::cpdf_font::CpdfFont,
    ) -> usize {
        let key = font
            .font_dict()
            .map(|dict| Rc::as_ptr(&dict).cast::<()>() as usize);
        let mut indices = self.font_indices.borrow_mut();
        if let Some(index) = key.and_then(|key| indices.get(&key).copied()) {
            return index;
        }
        let mut fonts = self.fonts.borrow_mut();
        let index = fonts.len();
        fonts.push(font.clone());
        if let Some(key) = key {
            indices.insert(key, index);
        }
        index
    }
}
