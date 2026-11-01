// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use crate::{engine, Error, Result};
use pdfoxium_core::{
    fpdfapi::{
        edit::cpdf_creator::CpdfCreator,
        page::{cpdf_page::CpdfPage, cpdf_pagemodule::initialize_page_module},
        parser::{
            cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument, cpdf_parser::ParserError,
        },
    },
    fxcrt::{
        cfx_read_only_container_stream::CfxReadOnlyContainerStream,
        fx_stream::{SeekableReadStream, StreamWithSize, WriteStream},
    },
    fxge::cfx_gemodule::initialize_ge_module,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt,
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DOCUMENT: AtomicU64 = AtomicU64::new(1);
type ReadError = Rc<RefCell<Option<io::Error>>>;

/// Opaque identity scoped to one open document, stable across unloading.
/// Reopening a file creates a different identity namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct PageId {
    document: u64,
    slot: usize,
}

/// PDF header version, with the catalog's valid `/Version` override applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}

/// Save flags without contradictory incremental/full settings.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaveOptions {
    /// Write a complete output containing original bytes plus an incremental update.
    /// The writer still starts empty; never append to the source file yourself.
    pub incremental: bool,
    pub remove_security: bool,
    pub subset_new_fonts: bool,
    /// Explicit header version; the current engine supports PDF 1.0 through 1.7.
    pub version: Option<Version>,
}

/// A cheap, single-threaded handle to an open PDF. Clones share live state.
#[derive(Clone)]
pub struct Document(Rc<DocumentInner>);

struct DocumentInner {
    // Dependent page state is destroyed before the document and input source.
    pages: RefCell<PageRegistry>,
    revision: Cell<u64>,
    modified: Cell<bool>,
    form_dirty: Cell<bool>,
    core: Rc<RefCell<CpdfDocument>>,
    read_error: ReadError,
    forms: RefCell<std::rc::Weak<crate::forms::FormInner>>,
    identity: u64,
    optional_content_source: Option<std::sync::Arc<[u8]>>,
    active: Cell<bool>,
    poisoned: Cell<bool>,
}

/// A cheap page handle that keeps its document alive. Clones alias the same
/// parsed page and cache. Use `unload()` to release that shared residency.
#[derive(Clone)]
pub struct Page {
    state: Rc<PageState>,
    document: Document,
}

struct PageState {
    loaded: RefCell<Option<Rc<CpdfPage>>>,
    activity: Cell<Activity>,
    id: PageId,
    index: Cell<usize>,
    deleted: Cell<bool>,
    dirty: Cell<bool>,
    objects: RefCell<crate::edit::ObjectRegistry>,
    annotations: RefCell<crate::annot::AnnotRegistry>,
}

#[derive(Default)]
struct PageRegistry {
    by_index: HashMap<usize, Rc<PageState>>,
    by_id: HashMap<usize, Rc<PageState>>,
    next_id: usize,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Activity {
    Idle,
    Loading,
    Rendering,
    Unloading,
    Editing,
}

/// Reservation only: no RefCell borrow is held between public calls.
pub(crate) struct PageLease {
    page: Page,
    // Defer form teardown until every retained page reader is released.
    _forms: Option<crate::FormSession>,
}

/// Reject user callbacks reentering an active core operation. Dropped before
/// progressive pause callbacks, which may operate on other idle pages.
pub(crate) struct CoreCallGuard {
    document: Document,
    _forms: Option<crate::FormSession>,
}

impl Drop for CoreCallGuard {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.document.0.poisoned.set(true);
        }
        self.document.0.active.set(false);
    }
}

impl Drop for PageLease {
    fn drop(&mut self) {
        self.page.state.activity.set(Activity::Idle);
    }
}

impl PageLease {
    pub(crate) fn document(&self) -> &Document {
        &self.page.document
    }
    pub(crate) fn pdf_page(&self) -> Result<Rc<CpdfPage>> {
        self.page
            .state
            .loaded
            .try_borrow()
            .map_err(|_| Error::Busy)?
            .clone()
            .ok_or(Error::Internal("leased page is not loaded"))
    }

    pub(crate) fn core_document(&self) -> &Rc<RefCell<CpdfDocument>> {
        &self.page.document.0.core
    }

    pub(crate) fn enter_core(&self) -> Result<CoreCallGuard> {
        self.page.document.enter_core()
    }

    pub(crate) fn check_io(&self) -> Result<()> {
        self.page.document.check_io()
    }

    pub(crate) fn page_id(&self) -> PageId {
        self.page.id()
    }
}

impl Document {
    pub(crate) fn same_document(&self, other: &Document) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    /// Open a file, retaining it for lazy reads until the last owner is dropped.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_reader(File::open(path)?)
    }

    pub fn open_with_password(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        Self::from_reader_with_password(File::open(path)?, password)
    }

    /// Take ownership of PDF bytes without copying their contents. Accepts a
    /// `Vec<u8>`, `Arc<[u8]>`, boxed slice, or a static byte slice. To load a
    /// borrowed non-static slice, explicitly copy it with `to_vec()`.
    pub fn from_bytes(data: impl AsRef<[u8]> + 'static) -> Result<Self> {
        Self::from_bytes_with_password(data, "")
    }

    pub fn from_bytes_with_password(
        data: impl AsRef<[u8]> + 'static,
        password: &str,
    ) -> Result<Self> {
        i64::try_from(data.as_ref().len())
            .map_err(|_| Error::InvalidArgument("PDF input exceeds the supported length"))?;
        let source = (&data as &dyn std::any::Any)
            .downcast_ref::<std::sync::Arc<[u8]>>()
            .cloned();
        let mut document = Self::load(
            Rc::new(CfxReadOnlyContainerStream::new(data)),
            password,
            Rc::new(RefCell::new(None)),
        )?;
        Rc::get_mut(&mut document.0)
            .expect("new document has one owner")
            .optional_content_source = source;
        Ok(document)
    }

    /// Own a seekable source. Reads are lazy; the source must remain unchanged
    /// for the lifetime of the document and all its pages or jobs.
    pub fn from_reader(reader: impl Read + Seek + 'static) -> Result<Self> {
        Self::from_reader_with_password(reader, "")
    }

    pub fn from_reader_with_password(
        mut reader: impl Read + Seek + 'static,
        password: &str,
    ) -> Result<Self> {
        let length = reader.seek(SeekFrom::End(0))?;
        let length = i64::try_from(length)
            .map_err(|_| Error::InvalidArgument("PDF input exceeds the supported length"))?;
        reader.seek(SeekFrom::Start(0))?;
        let read_error = Rc::new(RefCell::new(None));
        let stream = ReaderStream {
            reader: RefCell::new(reader),
            length,
            error: read_error.clone(),
        };
        Self::load(Rc::new(stream), password, read_error)
    }

    fn load(
        stream: Rc<dyn SeekableReadStream>,
        password: &str,
        read_error: ReadError,
    ) -> Result<Self> {
        initialize_ge_module(None);
        initialize_page_module();
        let mut core = CpdfDocument::new();
        let status = core.load_doc(stream, password.as_bytes());
        if let Some(error) = read_error.borrow_mut().take() {
            return Err(Error::Io(error));
        }
        match status {
            ParserError::Success => {}
            ParserError::FileError => {
                return Err(Error::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "could not read the PDF input",
                )));
            }
            ParserError::FormatError => return Err(Error::Format),
            ParserError::PasswordError => return Err(Error::Password),
            ParserError::HandlerError => return Err(Error::Security),
        }
        Self::from_core(Rc::new(RefCell::new(core)), read_error)
    }

    fn from_core(core: Rc<RefCell<CpdfDocument>>, read_error: ReadError) -> Result<Self> {
        // Keep the Rust 1.91 MSRV; newer toolchains rename this to try_update.
        #[allow(deprecated)]
        let identity = NEXT_DOCUMENT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| Error::Internal("document identity space exhausted"))?;
        Ok(Self(Rc::new(DocumentInner {
            pages: RefCell::new(PageRegistry::default()),
            revision: Cell::new(0),
            modified: Cell::new(false),
            form_dirty: Cell::new(false),
            core,
            read_error,
            identity,
            optional_content_source: None,
            forms: RefCell::new(std::rc::Weak::new()),
            active: Cell::new(false),
            poisoned: Cell::new(false),
        })))
    }

    fn check_available(&self) -> Result<()> {
        if self.0.poisoned.get() {
            Err(Error::Poisoned)
        } else if self.0.active.get() {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }

    fn enter_core(&self) -> Result<CoreCallGuard> {
        self.check_available()?;
        self.0.active.set(true);
        Ok(CoreCallGuard {
            document: self.clone(),
            _forms: self.active_forms(),
        })
    }

    fn check_io(&self) -> Result<()> {
        match self.0.read_error.borrow_mut().take() {
            Some(error) => Err(Error::Io(error)),
            None => Ok(()),
        }
    }

    pub fn page_count(&self) -> Result<usize> {
        let _call = self.enter_core()?;
        if let Some(count) = self.active_forms().and_then(|f| f.xfa_page_count()) {
            return Ok(count);
        }
        Ok(self
            .0
            .core
            .try_borrow()
            .map_err(|_| Error::Busy)?
            .page_count())
    }

    pub fn version(&self) -> Result<Version> {
        let _call = self.enter_core()?;
        let core = self.0.core.try_borrow().map_err(|_| Error::Busy)?;
        let mut version = core.parser().file_version();
        if core.parser().document_size().is_none() {
            version = 17;
        } // New documents save as PDF 1.7.
        if let Some(root) = core.root() {
            let root = root.borrow();
            if let Some(root) = root.as_any().downcast_ref::<CpdfDictionary>() {
                let name = root.get_name_for(b"Version");
                if name.len() == 3
                    && name[0].is_ascii_digit()
                    && name[1] == b'.'
                    && name[2].is_ascii_digit()
                {
                    version = i32::from(name[0] - b'0') * 10 + i32::from(name[2] - b'0');
                }
            }
        }
        self.check_io()?;
        Ok(Version {
            major: (version / 10) as u8,
            minor: (version % 10) as u8,
        })
    }

    /// Load a zero-based page. Repeated lookups share its parsed state and
    /// caches. Residency lasts until explicit unload or document teardown.
    pub fn page(&self, index: usize) -> Result<Page> {
        let page = self.page_handle(index)?;
        // A loaded page can be looked up while paused without granting access
        // to the core. Its operation methods still check their own leases.
        if page
            .state
            .loaded
            .try_borrow()
            .map_err(|_| Error::Busy)?
            .is_none()
        {
            let _lease = page.acquire(Activity::Loading)?;
            page.load()?;
        }
        Ok(page)
    }

    /// Return a canonical handle without parsing its content.
    pub(crate) fn page_handle(&self, index: usize) -> Result<Page> {
        let count = self.page_count()?;
        if index >= count {
            return Err(Error::PageOutOfRange { index, count });
        }
        let state = {
            let mut registry = self.0.pages.try_borrow_mut().map_err(|_| Error::Busy)?;
            if let Some(state) = registry.by_index.get(&index) {
                state.clone()
            } else {
                let slot = registry.next_id;
                registry.next_id = slot
                    .checked_add(1)
                    .ok_or(Error::Internal("page identity space exhausted"))?;
                let state = Rc::new(PageState {
                    loaded: RefCell::new(None),
                    activity: Cell::new(Activity::Idle),
                    id: PageId {
                        document: self.0.identity,
                        slot,
                    },
                    index: Cell::new(index),
                    deleted: Cell::new(false),
                    dirty: Cell::new(false),
                    objects: RefCell::new(Default::default()),
                    annotations: RefCell::new(Default::default()),
                });
                registry.by_id.insert(slot, state.clone());
                registry.by_index.insert(index, state.clone());
                state
            }
        };
        Ok(Page {
            state,
            document: self.clone(),
        })
    }

    pub(crate) fn active_forms(&self) -> Option<crate::FormSession> {
        self.0.forms.borrow().upgrade().map(crate::FormSession)
    }
    pub(crate) fn register_forms(&self, form: &crate::FormSession) {
        *self.0.forms.borrow_mut() = Rc::downgrade(&form.0);
    }
    pub(crate) fn record_form_change(&self) -> Result<()> {
        let revision = self.next_revision()?;
        self.0.revision.set(revision);
        Ok(())
    }
    pub(crate) fn set_form_dirty(&self, dirty: bool) {
        self.0.form_dirty.set(dirty);
    }
    pub(crate) fn with_form_core<T>(
        &self,
        operation: impl FnOnce(&Rc<RefCell<CpdfDocument>>) -> Result<T>,
    ) -> Result<T> {
        let _guard = self.enter_core()?;
        if self
            .0
            .pages
            .borrow()
            .by_index
            .values()
            .any(|p| p.activity.get() != Activity::Idle)
        {
            return Err(Error::Busy);
        }
        let result = operation(&self.0.core);
        self.check_io()?;
        result
    }
    pub(crate) fn with_core<T>(
        &self,
        f: impl FnOnce(&Rc<RefCell<CpdfDocument>>) -> Result<T>,
    ) -> Result<T> {
        let _guard = self.enter_core()?;
        let result = f(&self.0.core);
        self.check_io()?;
        result
    }

    pub(crate) fn optional_content_identity(&self) -> u64 {
        self.0.identity
    }
    pub(crate) fn optional_content_generation(&self) -> Result<u64> {
        Ok(self
            .0
            .core
            .try_borrow()
            .map_err(|_| Error::Busy)?
            .optional_content_generation())
    }
    pub(crate) fn optional_content_source(&self) -> Option<std::sync::Arc<[u8]>> {
        self.0.optional_content_source.clone()
    }
    /// Revision of the document, incremented after editing attempts that may
    /// change content or page order. Failed operations can retain partial changes.
    pub fn revision(&self) -> u64 {
        self.0.revision.get()
    }

    /// Create an empty PDF, without adding a timestamp automatically.
    pub fn create() -> Result<Self> {
        initialize_ge_module(None);
        initialize_page_module();
        let core = pdfoxium_core::fpdfsdk::fpdf_editpage::create_new_document(false, || None);
        Self::from_core(core, Rc::new(RefCell::new(None)))
    }

    /// Import zero-based source indices at `index`, preserving order and duplicates.
    /// Existing page handles and IDs keep referring to the same pages; their
    /// indices move. Both documents must be idle, and self-import is rejected.
    ///
    /// Invalid arguments are rejected before mutation. Like the underlying
    /// PDFium importer, a failure on malformed input can leave inserted pages
    /// and copied resources; this is not an editing transaction. Existing handles
    /// remain consistent even then. Build a fresh document for atomic merge/split
    /// workflows and save it only after all imports succeed.
    pub fn import_pages(&self, source: &Document, indices: &[usize], index: usize) -> Result<()> {
        if Rc::ptr_eq(&self.0, &source.0) {
            return Err(Error::InvalidArgument(
                "source and destination must be different documents",
            ));
        }
        if self.active_forms().is_some() || source.active_forms().is_some() {
            return Err(Error::Busy);
        }
        let _dest_call = self.enter_core()?;
        let _src_call = source.enter_core()?;
        for doc in [self, source] {
            if doc
                .0
                .pages
                .borrow()
                .by_index
                .values()
                .any(|p| p.activity.get() != Activity::Idle)
            {
                return Err(Error::Busy);
            }
        }
        let before = self.0.core.borrow().page_count();
        let source_count = source.0.core.borrow().page_count();
        if index > before {
            return Err(Error::InvalidArgument("insertion index exceeds page count"));
        }
        before
            .checked_add(indices.len())
            .filter(|&n| n <= i32::MAX as usize)
            .ok_or(Error::InvalidArgument("too many pages"))?;
        let mut selected = Vec::with_capacity(indices.len());
        for &page in indices {
            if page >= source_count {
                return Err(Error::PageOutOfRange {
                    index: page,
                    count: source_count,
                });
            }
            selected.push(
                i32::try_from(page).map_err(|_| Error::InvalidArgument("page index too large"))?,
            );
        }
        if selected.is_empty() {
            return Ok(());
        }
        let revision = self
            .revision()
            .checked_add(1)
            .ok_or(Error::Internal("revision exhausted"))?;
        self.flush_all()?;
        source.flush_all()?;
        let success = pdfoxium_core::fpdfsdk::fpdf_ppo::import_pages_by_index(
            self.0.core.clone(),
            source.0.core.clone(),
            Some(&selected),
            index as i32,
        );
        let after = self.0.core.borrow().page_count();
        let inserted = after
            .checked_sub(before)
            .ok_or(Error::Internal("import removed pages"))?;
        if inserted != 0 {
            let mut registry = self.0.pages.borrow_mut();
            let old = std::mem::take(&mut registry.by_index);
            for (position, state) in old {
                let position = if position >= index {
                    position + inserted
                } else {
                    position
                };
                state.index.set(position);
                registry.by_index.insert(position, state);
            }
            self.0.revision.set(revision);
            self.0.modified.set(true);
        }
        // Always drain both reader errors before choosing the returned error.
        let dest_io = self.check_io();
        let src_io = source.check_io();
        dest_io?;
        src_io?;
        if success {
            Ok(())
        } else {
            Err(Error::Import)
        }
    }

    pub fn page_by_id(&self, id: PageId) -> Result<Page> {
        self.check_available()?;
        if id.document != self.0.identity {
            return Err(Error::Stale);
        }
        self.page(self.page_index(id)?)
    }

    /// Resolve an existing identity without loading or parsing its page.
    pub fn page_index(&self, id: PageId) -> Result<usize> {
        self.check_available()?;
        if id.document != self.0.identity {
            return Err(Error::Stale);
        }
        self.0
            .pages
            .borrow()
            .by_id
            .get(&id.slot)
            .map(|state| state.index.get())
            .ok_or(Error::Stale)
    }

    /// Lazy, owning iterator; it does not borrow the `Document` handle.
    pub fn pages(&self) -> Result<Pages> {
        Ok(Pages {
            document: self.clone(),
            next: 0,
            count: self.page_count()?,
            revision: self.revision(),
        })
    }

    /// Whether a mutation has occurred since opening or the last successful save.
    pub fn is_dirty(&self) -> bool {
        self.0.modified.get() || self.0.form_dirty.get()
    }

    fn next_revision(&self) -> Result<u64> {
        self.revision()
            .checked_add(1)
            .ok_or(Error::Internal("revision exhausted"))
    }
    fn record_edit(&self, revision: u64) {
        self.0.revision.set(revision);
        self.0.modified.set(true);
    }
    fn flush_state(&self, state: &PageState) -> Result<()> {
        if !state.dirty.get() || state.deleted.get() {
            return Ok(());
        }
        let mut loaded = state.loaded.try_borrow_mut().map_err(|_| Error::Busy)?;
        let page = loaded.as_mut().and_then(Rc::get_mut).ok_or(Error::Busy)?;
        if !CpdfPage::is_valid_page_dict(&page.dict()) {
            return Err(Error::Edit);
        }
        let holder = page.mutable_holder();
        if holder.iter().any(|object| object.base().content_stream() == pdfoxium_core::fpdfapi::page::cpdf_pageobject::CpdfPageObject::NO_CONTENT_STREAM) {
            // A new object must not inherit an old stream's persistent clip.
            // Serialize the parsed page's explicit graphics state in paint order.
            holder.prepare_full_rewrite();
        }
        // Keep retry markers if a lazy source read fails during generation.
        let dirty_streams = holder.take_dirty_streams();
        for &stream in &dirty_streams {
            holder.mark_stream_dirty(stream);
        }
        {
            let mut document = self.0.core.try_borrow_mut().map_err(|_| Error::Busy)?;
            pdfoxium_core::fpdfapi::edit::cpdf_pagecontentgenerator::CpdfPageContentGenerator::new(
                holder,
                Some(&mut document),
            )
            .generate_content();
        }
        if let Err(error) = self.check_io() {
            for stream in dirty_streams {
                holder.mark_stream_dirty(stream);
            }
            for index in 0..holder.page_object_count() {
                holder
                    .page_object_by_index_mut(index)
                    .unwrap()
                    .base_mut()
                    .set_dirty(true);
            }
            return Err(error);
        }
        state.dirty.set(false);
        Ok(())
    }
    fn flush_all(&self) -> Result<()> {
        let states: Vec<_> = self.0.pages.borrow().by_index.values().cloned().collect();
        for state in states {
            self.flush_state(&state)?;
        }
        Ok(())
    }
    pub(crate) fn with_structure_edit<T>(
        &self,
        edit: impl FnOnce(&Rc<RefCell<CpdfDocument>>) -> Result<T>,
    ) -> Result<T> {
        if self.active_forms().is_some() {
            return Err(Error::Busy);
        }
        let _guard = self.enter_core()?;
        if self
            .0
            .pages
            .borrow()
            .by_index
            .values()
            .any(|p| p.activity.get() != Activity::Idle)
        {
            return Err(Error::Busy);
        }
        let revision = self.next_revision()?;
        self.flush_all()?;
        let result = edit(&self.0.core);
        self.record_edit(revision);
        self.check_io()?;
        result
    }
    pub(crate) fn any_loaded_page(
        &self,
        mut predicate: impl FnMut(&CpdfPage) -> bool,
    ) -> Result<bool> {
        let _guard = self.enter_core()?;
        let states: Vec<_> = self.0.pages.borrow().by_index.values().cloned().collect();
        for state in states {
            let loaded = state.loaded.try_borrow().map_err(|_| Error::Busy)?;
            if loaded.as_ref().is_some_and(|page| predicate(page)) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn with_all_pages_edit(
        &self,
        mut edit: impl FnMut(&mut CpdfPage, &Rc<RefCell<CpdfDocument>>) -> Result<()>,
    ) -> Result<()> {
        if self.active_forms().is_some() {
            return Err(Error::Busy);
        }
        let _guard = self.enter_core()?;
        let states: Vec<_> = self.0.pages.borrow().by_index.values().cloned().collect();
        // Validate every participant before mutating a shared mark.
        for state in &states {
            if state.activity.get() != Activity::Idle {
                return Err(Error::Busy);
            }
            let loaded = state.loaded.try_borrow().map_err(|_| Error::Busy)?;
            if loaded
                .as_ref()
                .is_some_and(|page| Rc::strong_count(page) != 1)
            {
                return Err(Error::Busy);
            }
        }
        let revision = self.next_revision()?;
        self.record_edit(revision);
        let mut result = Ok(());
        for state in states {
            let mut loaded = state.loaded.borrow_mut();
            if let Some(page) = loaded.as_mut().and_then(Rc::get_mut) {
                state.dirty.set(true);
                result = edit(page, &self.0.core);
                if result.is_err() {
                    break;
                }
            }
        }
        self.check_io()?;
        result
    }

    pub(crate) fn with_import_source<T>(
        &self,
        source: &Document,
        edit: impl FnOnce(&Rc<RefCell<CpdfDocument>>, &Rc<RefCell<CpdfDocument>>) -> Result<T>,
    ) -> Result<T> {
        if self.active_forms().is_some() || source.active_forms().is_some() {
            return Err(Error::Busy);
        }
        let _destination = self.enter_core()?;
        let _source = if Rc::ptr_eq(&self.0, &source.0) {
            None
        } else {
            Some(source.enter_core()?)
        };
        for document in [self, source] {
            if document
                .0
                .pages
                .borrow()
                .by_index
                .values()
                .any(|p| p.activity.get() != Activity::Idle)
            {
                return Err(Error::Busy);
            }
            document.flush_all()?;
        }
        let revision = self.next_revision()?;
        let result = edit(&self.0.core, &source.0.core);
        self.record_edit(revision);
        let a = self.check_io();
        let b = source.check_io();
        a?;
        b?;
        result
    }

    /// Reconcile tracked handles after a structural operation, including failure.
    pub(crate) fn remap_pages(&self, positions: &[Option<usize>]) -> Result<()> {
        let mut registry = self.0.pages.try_borrow_mut().map_err(|_| Error::Busy)?;
        let old = std::mem::take(&mut registry.by_index);
        for (index, state) in old {
            if let Some(Some(position)) = positions.get(index) {
                state.index.set(*position);
                registry.by_index.insert(*position, state);
            } else {
                state.deleted.set(true);
                state.dirty.set(false);
                state.objects.borrow_mut().reset();
                state.annotations.borrow_mut().reset();
                state.loaded.borrow_mut().take();
                registry.by_id.remove(&state.id.slot);
            }
        }
        Ok(())
    }

    /// Write a full PDF copy, preserving its security settings and version.
    /// The writer is borrowed only for this call and is not flushed by us.
    /// Do not truncate or overwrite this document's lazily read source file;
    /// save to a separate file and replace the source after dropping its owners.
    pub fn save_to(&self, out: &mut impl Write) -> Result<()> {
        self.save_with_options(out, &SaveOptions::default())
    }

    /// Save with explicit version, security removal, font subsetting or incremental output.
    /// As with `save_to`, write to a separate output while the source remains readable.
    pub fn save_with_options(&self, out: &mut impl Write, options: &SaveOptions) -> Result<()> {
        self.write_document(out, options, true)
    }
    /// Serialize the current document without marking its changes as saved.
    /// Commits focused form input, but does not dispatch document save actions.
    /// Useful for an isolated editing copy; the original remains dirty on failure.
    pub fn snapshot(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.write_document(&mut bytes, &SaveOptions::default(), false)?;
        Ok(bytes)
    }
    fn write_document(
        &self,
        out: &mut impl Write,
        options: &SaveOptions,
        mark_saved: bool,
    ) -> Result<()> {
        let form = self.active_forms();
        if let Some(form) = &form {
            form.commit()?;
            if mark_saved {
                form.xfa_pre_save();
            }
        }
        let result = (|| {
            if let Some(form) = &form {
                form.sync_xfa_data()?;
            }
            self.write_document_core(out, options, mark_saved)
        })();
        if mark_saved {
            if let Some(form) = &form {
                form.xfa_post_save(result.is_ok());
            }
        }
        result
    }
    fn write_document_core(
        &self,
        out: &mut impl Write,
        options: &SaveOptions,
        mark_saved: bool,
    ) -> Result<()> {
        let version = match options.version {
            None => 0,
            Some(Version { major: 1, minor }) if minor <= 7 => 10 + i32::from(minor),
            Some(_) => {
                return Err(Error::InvalidArgument(
                    "save version must be PDF 1.0 through 1.7",
                ))
            }
        };
        let flags = if options.incremental {
            CpdfCreator::INCREMENTAL
        } else {
            CpdfCreator::NONE
        } | if options.remove_security {
            CpdfCreator::REMOVE_SECURITY
        } else {
            0
        } | if options.subset_new_fonts {
            CpdfCreator::SUBSET_NEW_FONTS
        } else {
            0
        };
        let _call = self.enter_core()?;
        if self
            .0
            .pages
            .borrow()
            .by_index
            .values()
            .any(|page| page.activity.get() != Activity::Idle)
        {
            return Err(Error::Busy);
        }
        self.flush_all()?;
        let mut sink = WriterStream {
            writer: out,
            error: None,
        };
        let success = CpdfCreator::new(self.0.core.clone(), &mut sink).create(flags, version);
        // Both sources can fail in one save: the creator may flush buffered
        // output after a read failure. Consume this operation's read
        // error even when the writer error takes precedence in the result.
        let read_result = self.check_io();
        if let Some(error) = sink.error {
            return Err(Error::Io(error));
        }
        read_result?;
        if success && mark_saved {
            self.0.modified.set(false);
            self.0.form_dirty.set(false);
            if let Some(form) = self.active_forms() {
                form.saved();
            }
            Ok(())
        } else if success {
            Ok(())
        } else {
            Err(Error::Save)
        }
    }
}

impl Page {
    pub(crate) fn ensure_live(&self) -> Result<()> {
        if self.state.deleted.get() {
            Err(Error::Stale)
        } else {
            Ok(())
        }
    }
    pub(crate) fn with_object_access<T>(
        &self,
        mutating: bool,
        edit: impl FnOnce(
            &mut CpdfPage,
            &Rc<RefCell<CpdfDocument>>,
            &mut crate::edit::ObjectRegistry,
        ) -> Result<T>,
    ) -> Result<T> {
        self.with_object_access_inner(mutating, false, edit)
    }
    fn with_object_access_inner<T>(
        &self,
        mutating: bool,
        allow_forms: bool,
        edit: impl FnOnce(
            &mut CpdfPage,
            &Rc<RefCell<CpdfDocument>>,
            &mut crate::edit::ObjectRegistry,
        ) -> Result<T>,
    ) -> Result<T> {
        if mutating && !allow_forms && self.document.active_forms().is_some() {
            return Err(Error::Busy);
        }
        let _lease = self.acquire(Activity::Editing)?;
        self.load()?;
        let _guard = self.document.enter_core()?;
        let revision = self.document.next_revision()?;
        let mut loaded = self
            .state
            .loaded
            .try_borrow_mut()
            .map_err(|_| Error::Busy)?;
        let page = loaded.as_mut().and_then(Rc::get_mut).ok_or(Error::Busy)?;
        let result = edit(
            page,
            &self.document.0.core,
            &mut self.state.objects.borrow_mut(),
        );
        if mutating {
            self.state.dirty.set(true);
            self.document.record_edit(revision);
        }
        self.document.check_io()?;
        result
    }
    pub(crate) fn with_edit<T>(
        &self,
        edit: impl FnOnce(&mut CpdfPage, &Rc<RefCell<CpdfDocument>>) -> Result<T>,
    ) -> Result<T> {
        self.with_object_access(true, |page, doc, _| edit(page, doc))
    }
    pub(crate) fn with_annotations<T>(
        &self,
        mutating: bool,
        edit: impl FnOnce(
            &mut CpdfPage,
            &Rc<RefCell<CpdfDocument>>,
            &mut crate::annot::AnnotRegistry,
        ) -> Result<T>,
    ) -> Result<T> {
        self.with_annotation_access(mutating, false, edit)
    }
    // Only FormSession's narrow non-widget edit methods pass allow_forms=true,
    // after committing input and retiring the affected SDK annotation view.
    // Page/object/structure edits retain their ordinary session exclusion.
    pub(crate) fn with_annotation_access<T>(
        &self,
        mutating: bool,
        allow_forms: bool,
        edit: impl FnOnce(
            &mut CpdfPage,
            &Rc<RefCell<CpdfDocument>>,
            &mut crate::annot::AnnotRegistry,
        ) -> Result<T>,
    ) -> Result<T> {
        self.with_object_access_inner(mutating, allow_forms, |page, doc, _| {
            edit(page, doc, &mut self.state.annotations.borrow_mut())
        })
    }
    pub(crate) fn with_reload_edit<T>(
        &self,
        edit: impl FnOnce(&mut CpdfPage, &Rc<RefCell<CpdfDocument>>) -> Result<T>,
    ) -> Result<T> {
        if self.document.active_forms().is_some() {
            return Err(Error::Busy);
        }
        let _lease = self.acquire(Activity::Editing)?;
        self.load()?;
        let _guard = self.document.enter_core()?;
        self.document.flush_state(&self.state)?;
        let revision = self.document.next_revision()?;
        let mut loaded = self
            .state
            .loaded
            .try_borrow_mut()
            .map_err(|_| Error::Busy)?;
        let page = loaded.as_mut().and_then(Rc::get_mut).ok_or(Error::Busy)?;
        let result = edit(page, &self.document.0.core);
        self.document.record_edit(revision);
        // Flatten can mutate serialized dictionaries even on failure. Existing
        // content was flushed first; reload from those dictionaries on retry.
        self.state.annotations.borrow_mut().prune(page);
        self.state.objects.borrow_mut().reset();
        loaded.take();
        self.document.check_io()?;
        result
    }
    pub(crate) fn with_two_edits<T>(
        &self,
        other: &Page,
        edit: impl FnOnce(
            &mut CpdfPage,
            &mut CpdfPage,
            &Rc<RefCell<CpdfDocument>>,
            &mut crate::edit::ObjectRegistry,
            &mut crate::edit::ObjectRegistry,
        ) -> Result<T>,
    ) -> Result<T> {
        if !Rc::ptr_eq(&self.document.0, &other.document.0) {
            return Err(Error::InvalidArgument(
                "objects can only move within one document",
            ));
        }
        if self.id() == other.id() {
            return Err(Error::InvalidArgument(
                "source and destination pages are identical",
            ));
        }
        if self.document.active_forms().is_some() {
            return Err(Error::Busy);
        }
        let _source = self.acquire(Activity::Editing)?;
        let _destination = other.acquire(Activity::Editing)?;
        self.load()?;
        other.load()?;
        let _guard = self.document.enter_core()?;
        let revision = self.document.next_revision()?;
        let mut source = self
            .state
            .loaded
            .try_borrow_mut()
            .map_err(|_| Error::Busy)?;
        let mut destination = other
            .state
            .loaded
            .try_borrow_mut()
            .map_err(|_| Error::Busy)?;
        let source = source.as_mut().and_then(Rc::get_mut).ok_or(Error::Busy)?;
        let destination = destination
            .as_mut()
            .and_then(Rc::get_mut)
            .ok_or(Error::Busy)?;
        let result = edit(
            source,
            destination,
            &self.document.0.core,
            &mut self.state.objects.borrow_mut(),
            &mut other.state.objects.borrow_mut(),
        );
        self.state.dirty.set(true);
        other.state.dirty.set(true);
        self.document.record_edit(revision);
        self.document.check_io()?;
        result
    }

    pub(crate) fn document(&self) -> &Document {
        &self.document
    }
    pub(crate) fn is_loaded(&self) -> Result<bool> {
        self.ensure_live()?;
        self.document.check_available()?;
        Ok(self
            .state
            .loaded
            .try_borrow()
            .map_err(|_| Error::Busy)?
            .is_some())
    }
    /// Identity and index queries do not enter the engine.
    pub fn id(&self) -> PageId {
        self.state.id
    }

    /// Current zero-based index, updated by page insertion and movement.
    /// After deletion this is the last index; use `Document::page_index` to check liveness.
    pub fn index(&self) -> usize {
        self.state.index.get()
    }

    fn acquire(&self, activity: Activity) -> Result<PageLease> {
        self.ensure_live()?;
        self.document.check_available()?;
        if self.state.activity.get() != Activity::Idle {
            return Err(Error::Busy);
        }
        self.state.activity.set(activity);
        Ok(PageLease {
            page: self.clone(),
            _forms: self.document.active_forms(),
        })
    }

    fn load(&self) -> Result<()> {
        if self
            .document
            .active_forms()
            .is_some_and(|f| f.is_full_xfa())
        {
            return Err(Error::Unsupported(
                "full-XFA pages use FormSession geometry and rendering",
            ));
        }
        let _call = self.document.enter_core()?;
        if self
            .state
            .loaded
            .try_borrow()
            .map_err(|_| Error::Busy)?
            .is_some()
        {
            return Ok(());
        }
        let page = engine::load_pdf_page(&self.document.0.core, self.state.index.get());
        self.document.check_io()?;
        let page = page.ok_or(Error::Format)?;
        *self
            .state
            .loaded
            .try_borrow_mut()
            .map_err(|_| Error::Busy)? = Some(page);
        Ok(())
    }

    pub(crate) fn acquire_render(&self) -> Result<PageLease> {
        let lease = self.acquire(Activity::Rendering)?;
        self.load()?;
        Ok(lease)
    }

    pub fn width(&self) -> Result<f32> {
        let lease = self.acquire(Activity::Loading)?;
        self.load()?;
        Ok(lease.pdf_page()?.width())
    }

    pub fn height(&self) -> Result<f32> {
        let lease = self.acquire(Activity::Loading)?;
        self.load()?;
        Ok(lease.pdf_page()?.height())
    }

    /// Regenerate pending object edits into content streams without unloading.
    /// Save and unload call this automatically; no output file is written.
    pub fn generate_content(&self) -> Result<()> {
        let _lease = self.acquire(Activity::Editing)?;
        self.load()?;
        let _guard = self.document.enter_core()?;
        self.document.flush_state(&self.state)
    }
    pub fn has_transparency(&self) -> Result<bool> {
        self.with_object_access(false, |page, _, _| {
            Ok(page.holder().background_alpha_needed())
        })
    }

    /// Drop parsed content and page-local image caches for every alias.
    /// Page handles and IDs remain valid; page-object IDs and annotation-object
    /// handles become stale. Later access reloads the page. Returns
    /// `Busy` if an active render needs the loaded content. Shared document
    /// resources remain cached. Dirty content is regenerated before release;
    /// a regeneration error retains the loaded content for retry.
    pub fn unload(&self) -> Result<()> {
        let _lease = self.acquire(Activity::Unloading)?;
        let _call = self.document.enter_core()?;
        self.document.flush_state(&self.state)?;
        self.state.objects.borrow_mut().reset();
        self.state
            .annotations
            .borrow_mut()
            .invalidate_object_handles();
        let loaded = self
            .state
            .loaded
            .try_borrow_mut()
            .map_err(|_| Error::Busy)?
            .take();
        // Drop resources after the RefCell borrow has ended.
        drop(loaded);
        Ok(())
    }
}

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Document")
            .field("identity", &self.0.identity)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Page")
            .field("id", &self.id())
            .field("index", &self.index())
            .finish_non_exhaustive()
    }
}

/// Owning iterator over zero-based pages. Unload pages as you finish with them
/// to bound parsed-content and image-cache residency in large documents.
pub struct Pages {
    document: Document,
    next: usize,
    count: usize,
    revision: u64,
}

impl Iterator for Pages {
    type Item = Result<Page>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        if self.document.revision() != self.revision {
            self.next = self.count;
            return Some(Err(Error::Stale));
        }
        let index = self.next;
        self.next += 1;
        Some(self.document.page(index))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = if self.next < self.count && self.document.revision() != self.revision {
            1 // One stale-sequence error, then fused termination.
        } else {
            self.count - self.next
        };
        (remaining, Some(remaining))
    }
}
impl ExactSizeIterator for Pages {}
impl std::iter::FusedIterator for Pages {}

struct ReaderStream<R> {
    reader: RefCell<R>,
    length: i64,
    error: ReadError,
}
impl<R> StreamWithSize for ReaderStream<R> {
    fn get_size(&self) -> i64 {
        self.length
    }
}
impl<R: Read + Seek> SeekableReadStream for ReaderStream<R> {
    fn read_block_at_offset(&self, buffer: &mut [u8], offset: i64) -> bool {
        let Ok(length) = i64::try_from(buffer.len()) else {
            return false;
        };
        if offset < 0
            || offset
                .checked_add(length)
                .is_none_or(|end| end > self.length)
        {
            return false;
        }
        let result = (|| {
            let mut reader = self
                .reader
                .try_borrow_mut()
                .map_err(|_| io::Error::other("recursive PDF reader access"))?;
            reader.seek(SeekFrom::Start(offset as u64))?;
            reader.read_exact(buffer)
        })();
        if let Err(error) = result {
            let mut slot = self.error.borrow_mut();
            if slot.is_none() {
                *slot = Some(error);
            }
            false
        } else {
            true
        }
    }
}

struct WriterStream<'a, W> {
    writer: &'a mut W,
    error: Option<io::Error>,
}
impl<W: Write> WriteStream for WriterStream<'_, W> {
    fn write_block(&mut self, data: &[u8]) -> bool {
        if self.error.is_some() {
            return false;
        }
        if let Err(error) = self.writer.write_all(data) {
            self.error = Some(error);
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RenderConfig;
    use std::io::Cursor;

    const PDF: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/render_corpus/pdfs/rectangles_multi_pages.pdf"
    ));

    struct TrackedReader {
        bytes: Cursor<Vec<u8>>,
        dropped: Rc<Cell<bool>>,
    }
    impl Read for TrackedReader {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.bytes.read(out)
        }
    }
    impl Seek for TrackedReader {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.bytes.seek(pos)
        }
    }
    impl Drop for TrackedReader {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }

    #[test]
    fn pages_retain_input_and_final_drop_releases_document_without_cycles() {
        let dropped = Rc::new(Cell::new(false));
        let document = Document::from_reader(TrackedReader {
            bytes: Cursor::new(PDF.to_vec()),
            dropped: dropped.clone(),
        })
        .unwrap();
        let page = document.page(0).unwrap();
        let alias = page.clone();
        drop(document);
        assert!(!dropped.get());
        assert!(page.width().unwrap() > 0.0);
        drop(page);
        assert!(!dropped.get());
        alias.unload().unwrap();
        assert!(alias.height().unwrap() > 0.0);
        drop(alias);
        assert!(dropped.get());
    }

    #[test]
    fn unload_releases_actual_page_and_preserves_alias_identity_and_pixels() {
        let document = Document::from_bytes(PDF).unwrap();
        let page = document.page(0).unwrap();
        let alias = document.page_by_id(page.id()).unwrap();
        let weak_page = Rc::downgrade(page.state.loaded.borrow().as_ref().unwrap());
        assert!(Rc::ptr_eq(&page.state, &alias.state));
        let before = page.render(&RenderConfig::default()).unwrap();
        page.unload().unwrap();
        assert!(
            weak_page.upgrade().is_none(),
            "unload must release parsed page and cache"
        );
        assert!(alias.state.loaded.borrow().is_none());
        let after = alias.render(&RenderConfig::default()).unwrap();
        assert_eq!(before.pixels(), after.pixels());
        assert_eq!(page.id(), alias.id());
        assert_eq!(page.id(), document.page(0).unwrap().id());
        page.unload().unwrap();
        page.unload().unwrap();
    }

    #[test]
    fn ids_are_document_scoped_and_iteration_owns_the_document() {
        let document = Document::from_bytes(PDF).unwrap();
        let count = document.page_count().unwrap();
        let page = document.page(0).unwrap();
        let other = Document::from_bytes(PDF).unwrap();
        assert!(matches!(other.page_by_id(page.id()), Err(Error::Stale)));
        assert!(matches!(
            document.page(count),
            Err(Error::PageOutOfRange { .. })
        ));
        assert!(matches!(
            document.page(usize::MAX),
            Err(Error::PageOutOfRange { .. })
        ));
        let mut pages = document.pages().unwrap();
        drop(document);
        drop(page);
        assert_eq!(pages.len(), count);
        for index in 0..count {
            let page = pages.next().unwrap().unwrap();
            assert_eq!(page.index(), index);
            page.unload().unwrap();
        }
        assert!(pages.next().is_none());
        assert!(pages.next().is_none());
    }

    #[test]
    fn full_save_uses_borrowed_writer_and_reopens_with_identical_rendering() {
        let document = Document::from_bytes(PDF).unwrap();
        let page = document.page(0).unwrap();
        let original = page.render(&RenderConfig::default()).unwrap();
        let mut saved = Vec::new();
        document.save_to(&mut saved).unwrap();
        assert!(!saved.is_empty());
        let reopened = Document::from_bytes(saved).unwrap();
        assert_eq!(
            document.page_count().unwrap(),
            reopened.page_count().unwrap()
        );
        assert_eq!(document.version().unwrap(), reopened.version().unwrap());
        let rendered = reopened
            .page(0)
            .unwrap()
            .render(&RenderConfig::default())
            .unwrap();
        assert_eq!(original.pixels(), rendered.pixels());
    }

    #[test]
    fn writer_errors_and_callback_reentry_are_reported_without_borrow_panics() {
        struct FailingWriter(Document);
        impl Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                assert!(matches!(self.0.page(0), Err(Error::Busy)));
                assert!(matches!(self.0.page_count(), Err(Error::Busy)));
                Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "test writer failed",
                ))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let document = Document::from_bytes(PDF).unwrap();
        let error = document
            .save_to(&mut FailingWriter(document.clone()))
            .unwrap_err();
        assert!(matches!(error, Error::Io(ref e) if e.kind() == io::ErrorKind::BrokenPipe));
        assert!(document.page(0).is_ok());
    }

    #[test]
    fn simultaneous_save_read_and_write_errors_do_not_leak_into_next_operation() {
        struct FailingReader {
            bytes: Cursor<&'static [u8]>,
            fail: Rc<Cell<bool>>,
            failures: Rc<Cell<usize>>,
        }
        impl Read for FailingReader {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                if self.fail.get() {
                    self.failures.set(self.failures.get() + 1);
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "input failed",
                    ))
                } else {
                    self.bytes.read(out)
                }
            }
        }
        impl Seek for FailingReader {
            fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
                self.bytes.seek(pos)
            }
        }
        struct FailingWriter {
            failures: usize,
        }
        impl Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                self.failures += 1;
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "output failed"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        // Large enough that opening leaves source data for lazy reads during
        // saving. Do not depend on the creator's flush boundaries or counts.
        const LAZY_PDF: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/many_rectangles.pdf"
        ));
        let fail_reads = Rc::new(Cell::new(false));
        let read_failures = Rc::new(Cell::new(0));
        let document = Document::from_reader(FailingReader {
            bytes: Cursor::new(LAZY_PDF),
            fail: fail_reads.clone(),
            failures: read_failures.clone(),
        })
        .unwrap();
        let version = document.version().unwrap();
        fail_reads.set(true);
        let mut writer = FailingWriter { failures: 0 };
        let error = document.save_to(&mut writer).unwrap_err();
        assert!(matches!(error, Error::Io(ref e) if e.kind() == io::ErrorKind::BrokenPipe));
        assert!(
            read_failures.get() > 0,
            "save must exercise the failing reader"
        );
        assert!(writer.failures > 0, "save must exercise the failing writer");

        fail_reads.set(false);
        assert_eq!(document.version().unwrap(), version);
    }

    #[test]
    fn panic_in_user_writer_poisons_document_and_releases_call_guard() {
        struct PanickingWriter;
        impl Write for PanickingWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                panic!("writer panic")
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let document = Document::from_bytes(PDF).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            document.save_to(&mut PanickingWriter)
        }));
        assert!(result.is_err());
        assert!(!document.0.active.get());
        assert!(matches!(document.page_count(), Err(Error::Poisoned)));
    }

    #[test]
    fn read_errors_and_invalid_pdf_are_distinct() {
        struct FailingReader;
        impl Read for FailingReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "test read failed",
                ))
            }
        }
        impl Seek for FailingReader {
            fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
                Ok(4096)
            }
        }
        assert!(
            matches!(Document::from_reader(FailingReader), Err(Error::Io(e)) if e.kind() == io::ErrorKind::PermissionDenied)
        );
        assert!(matches!(
            Document::from_bytes(b"not a pdf".as_slice()),
            Err(Error::Format)
        ));
        let absent = std::env::temp_dir().join(format!(
            "pdfoxium-missing-{}-{}.pdf",
            std::process::id(),
            NEXT_DOCUMENT.load(Ordering::Relaxed)
        ));
        assert!(
            matches!(Document::open(absent), Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound)
        );
    }

    #[test]
    fn password_loading_reports_failure_and_accepts_utf8_owner_password() {
        const ENCRYPTED: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/render_corpus/pdfs/encrypted_r6.pdf"
        ));
        assert!(matches!(
            Document::from_bytes_with_password(ENCRYPTED, "incorrect"),
            Err(Error::Password)
        ));
        let document = Document::from_bytes_with_password(ENCRYPTED, "âge").unwrap();
        assert_eq!(document.page_count().unwrap(), 1);
        assert!(document.page(0).unwrap().width().unwrap() > 0.0);
    }
}
