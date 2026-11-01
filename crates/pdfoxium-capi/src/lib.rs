// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! PDFium-compatible C ABI boundary.
//!
//! `build.rs` emits ABI-correct, aborting placeholders for all remaining public
//! functions. They exist only so clients that eagerly load every PDFium symbol
//! can bind to this library. Calling one is a hard failure, never a fake result.

use pdfoxium_core::fpdfapi::edit::cpdf_npagetooneexporter::{
    CpdfNPageToOneExporter, XObjectContext,
};
use pdfoxium_core::fpdfapi::page::cpdf_pageobject::PageObject;
use pdfoxium_core::fpdfapi::page::cpdf_pageobjectholder::PageObjectId;
use pdfoxium_core::fpdfsdk::fpdf_editpage;
use std::cell::{Cell, RefCell};
use std::ffi::{c_char, c_int, c_ulong, c_void, CStr};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use pdfoxium_core::fpdfapi::edit::cpdf_creator::CpdfCreator;
use pdfoxium_core::fpdfapi::edit::cpdf_pagecontentgenerator::CpdfPageContentGenerator;
use pdfoxium_core::fpdfapi::page::cpdf_occontext::{CpdfOcContext, UsageType};
use pdfoxium_core::fpdfapi::page::cpdf_page::CpdfPage;
use pdfoxium_core::fpdfapi::page::cpdf_pagemodule::{destroy_page_module, initialize_page_module};
use pdfoxium_core::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use pdfoxium_core::fpdfapi::parser::cpdf_document::CpdfDocument;
use pdfoxium_core::fpdfapi::parser::cpdf_parser::ParserError;
use pdfoxium_core::fpdfapi::render::cpdf_renderoptions::{CpdfRenderOptions, RenderColorMode};
use pdfoxium_core::fpdfapi::render::cpdf_renderstatus::render_page_objects_with_clip;
use pdfoxium_core::fxcodec::brotli::set_brotli_enabled;
use pdfoxium_core::fxcrt::cfx_fileaccess_stream::CfxFileAccessStream;
use pdfoxium_core::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
use pdfoxium_core::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF, FxRect};
use pdfoxium_core::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize, WriteStream};
use pdfoxium_core::fxcrt::fx_system::{get_last_error, set_last_error};
use pdfoxium_core::fxge::cfx_gemodule::{destroy_ge_module, initialize_ge_module};
use pdfoxium_core::fxge::dib::cfx_dibitmap::{CfxDibitmap, CfxDibitmapData};
use pdfoxium_core::fxge::dib::fx_dib::FxdibFormat;
use pdfoxium_core::fxge::dib::fx_dib::{argb_b, argb_g, argb_r, rgb_to_gray};

mod annot;
mod annot_form;
mod attachment;
mod catalog_signature_thumbnail;
mod doc;
mod editimg;
mod editmark;
mod editpage;
mod editpath;
mod editpattern;
mod editstate;
mod edittext;
mod flatten;
mod formfill;
mod javascript;
mod structtree;
mod text;
mod transformpage;
pub use annot::*;
pub use annot_form::*;
pub use attachment::*;
pub use catalog_signature_thumbnail::*;
pub use doc::*;
pub use editimg::*;
pub use editmark::*;
pub use editpage::*;
pub use editpath::*;
pub use editpattern::*;
pub use editstate::*;
pub use edittext::*;
pub use flatten::*;
pub use formfill::*;
pub use javascript::*;
pub use structtree::*;
pub use text::*;
pub use transformpage::*;

thread_local! {
    static LIBRARY_INITIALIZED: Cell<bool> = const { Cell::new(false) };
}

struct DocumentHandle {
    #[cfg(feature = "xfa")]
    xfa_context: Option<Rc<pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_context::CpdfXfaContext>>,
    document: Rc<RefCell<CpdfDocument>>,
    attachment_handles: AttachmentHandleCache,
    borrowed_objects: doc::BorrowedObjectCache,
    link_list: Rc<RefCell<pdfoxium_core::fpdfdoc::cpdf_linklist::CpdfLinkList>>,
}
type AttachmentHandleCache =
    Rc<RefCell<std::collections::HashMap<usize, Box<attachment::AttachmentHandle>>>>;
impl DocumentHandle {
    fn new(document: Rc<RefCell<CpdfDocument>>) -> Self {
        Self {
            document,
            #[cfg(feature = "xfa")]
            xfa_context: None,
            attachment_handles: Default::default(),
            borrowed_objects: Default::default(),
            link_list: Default::default(),
        }
    }
}

struct PageHandle {
    page: Option<Rc<CpdfPage>>,
    #[cfg(feature = "xfa")]
    xfa_page: Option<Rc<pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_page::CpdfXfaPage>>,
    progressive: Rc<progressive::ProgressiveSlot>,
    sdk_page: Rc<pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage>,
    object_handles: std::collections::HashMap<PageObjectId, Box<PageObjectHandle>>,
    // Pages retain their document's indirect objects and resource caches.
    _document: Rc<RefCell<CpdfDocument>>,
    attachment_handles: AttachmentHandleCache,
    borrowed_objects: doc::BorrowedObjectCache,
    link_list: Rc<RefCell<pdfoxium_core::fpdfdoc::cpdf_linklist::CpdfLinkList>>,
}

impl PageHandle {
    fn pdf_page(&self) -> Option<&CpdfPage> {
        self.page.as_deref()
    }
    fn retained_pdf_page(&self) -> Option<Rc<CpdfPage>> {
        self.page.clone()
    }

    fn page_mut(&mut self) -> Option<PdfPageMutation<'_>> {
        let page = self.page.as_mut()?;
        #[cfg(feature = "xfa")]
        let xfa = self.xfa_page.clone();
        #[cfg(feature = "xfa")]
        if let Some(xfa) = &xfa {
            xfa.set_live_pdf_page(None);
        }
        if Rc::get_mut(page).is_none() {
            #[cfg(feature = "xfa")]
            if let Some(xfa) = &xfa {
                xfa.set_live_pdf_page(Some(page));
            }
            return None;
        }
        Some(PdfPageMutation {
            page,
            #[cfg(feature = "xfa")]
            xfa,
        })
    }
}
/// Exclusive lease over the actual parsed page. Strong readers (including a
/// progressive job) still reject mutation. Suspend only our weak cache during
/// the lease; no page object tree is copied and identity is restored afterward.
struct PdfPageMutation<'a> {
    page: &'a mut Rc<CpdfPage>,
    #[cfg(feature = "xfa")]
    xfa: Option<Rc<pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_page::CpdfXfaPage>>,
}
impl std::ops::Deref for PdfPageMutation<'_> {
    type Target = CpdfPage;
    fn deref(&self) -> &Self::Target {
        self.page
    }
}
impl std::ops::DerefMut for PdfPageMutation<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Rc::get_mut(self.page).expect("exclusive page mutation lease")
    }
}
impl Drop for PdfPageMutation<'_> {
    fn drop(&mut self) {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = &self.xfa {
            xfa.set_live_pdf_page(Some(self.page));
        }
    }
}
impl Drop for PageHandle {
    fn drop(&mut self) {
        self.progressive.cancel();
        self.sdk_page.invalidate();
    }
}
/// Ordinary PDF-only entry points reject full-XFA handles before touching a
/// dictionary or page-object holder. This is CPDFPageFromFPDFPage's boundary.
unsafe fn pdf_handle<'a>(page: *mut c_void) -> Option<&'a PageHandle> {
    unsafe { page.cast::<PageHandle>().as_ref() }.filter(|handle| handle.pdf_page().is_some())
}
unsafe fn pdf_handle_mut<'a>(page: *mut c_void) -> Option<&'a mut PageHandle> {
    unsafe { page.cast::<PageHandle>().as_mut() }.filter(|handle| handle.pdf_page().is_some())
}
/// Snapshot the safe page identity before entering callbacks that may close it.
unsafe fn page_sdk_bridge(
    page: *mut c_void,
) -> Option<Rc<pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage>> {
    if page.is_null() {
        return None;
    }
    Some(unsafe { &*page.cast::<PageHandle>() }.sdk_page.clone())
}

// Thin C handles represent Rust trait objects without exposing fat pointers.
// Attached objects are resolved by allocation identity through their live page;
// this keeps the engine safe and preserves handles when holder storage moves.
struct PageObjectHandle {
    location: ObjectLocation,
    segment_handles: std::collections::HashMap<usize, Box<editpath::PathSegmentHandle>>,
    mark_handles: std::collections::HashMap<usize, Box<editmark::MarkHandle>>,
    font_handle: Option<Box<edittext::FontHandle>>,
    child_handles: std::collections::HashMap<PageObjectId, Box<PageObjectHandle>>,
    clip_handle: Option<Box<transformpage::ClipPathHandle>>,
}
impl PageObjectHandle {
    fn new(location: ObjectLocation) -> Self {
        Self {
            location,
            segment_handles: Default::default(),
            mark_handles: Default::default(),
            font_handle: None,
            child_handles: Default::default(),
            clip_handle: None,
        }
    }
}
fn new_owned_page_object(object: Box<dyn PageObject>) -> *mut c_void {
    Box::into_raw(Box::new(PageObjectHandle::new(ObjectLocation::Owned(
        object,
    ))))
    .cast()
}
enum ObjectLocation {
    Owned(Box<dyn PageObject>),
    Attached {
        page: NonNull<PageHandle>,
        index: usize,
        id: PageObjectId,
    },
    InForm {
        form: NonNull<PageObjectHandle>,
        index: usize,
        id: PageObjectId,
    },
    InAnnotation {
        annot: NonNull<annot::AnnotHandle>,
        index: usize,
        id: PageObjectId,
    },
    Transferring,
}
unsafe fn with_page_object<R>(
    handle: *mut c_void,
    f: impl FnOnce(&mut dyn PageObject) -> R,
) -> Option<R> {
    let mut f = Some(f);
    let mut result = None;
    unsafe {
        visit_page_object(handle, &mut |object| {
            result = Some(f.take().expect("one object visit")(object));
        });
    }
    result
}

// Erase the callback type across recursive form traversal. Each public call
// visits at most one object; engine references cannot escape the callback.
unsafe fn visit_page_object(
    handle: *mut c_void,
    f: &mut dyn FnMut(&mut dyn PageObject),
) -> Option<()> {
    if handle.is_null() {
        return None;
    }
    // Borrow only the location/engine field, never the parent's handle maps.
    let location = unsafe { &mut (*handle.cast::<PageObjectHandle>()).location };
    match location {
        ObjectLocation::Owned(object) => {
            f(object.as_mut());
            Some(())
        }
        ObjectLocation::Attached { page, index, id } => {
            let mut engine_page = unsafe { &mut *page.as_ptr() }.page_mut()?;
            let holder = engine_page.mutable_holder();
            if holder.page_object_id_by_index(*index) != Some(*id) {
                *index = (0..holder.page_object_count())
                    .find(|&i| holder.page_object_id_by_index(i) == Some(*id))?;
            }
            f(holder.page_object_by_index_mut(*index)?);
            Some(())
        }
        ObjectLocation::InForm { form, index, id } => {
            let mut result = None;
            unsafe {
                visit_page_object(form.as_ptr().cast(), &mut |object| {
                    result = (|| {
                        let form = object.as_any_mut().downcast_mut::<
                        pdfoxium_core::fpdfapi::page::cpdf_formobject::CpdfFormObject,
                    >()?;
                        let holder = form.form_mut().holder_mut();
                        if holder.page_object_id_by_index(*index) != Some(*id) {
                            *index = (0..holder.page_object_count())
                                .find(|&i| holder.page_object_id_by_index(i) == Some(*id))?;
                        }
                        f(holder.page_object_by_index_mut(*index)?);
                        Some(())
                    })();
                });
            }
            result
        }
        ObjectLocation::InAnnotation { annot, index, id } => {
            let context = (unsafe { &mut (*annot.as_ptr()).context }).as_mut()?;
            let holder = context.form_mut()?.holder_mut();
            if holder.page_object_id_by_index(*index) != Some(*id) {
                *index = (0..holder.page_object_count())
                    .find(|&i| holder.page_object_id_by_index(i) == Some(*id))?;
            }
            f(holder.page_object_by_index_mut(*index)?);
            Some(())
        }
        ObjectLocation::Transferring => unreachable!("synchronous object transfer"),
    }
}

/// Shared getters may also borrow the owning page and its resources.
unsafe fn with_page_object_ref<R>(
    handle: *mut c_void,
    f: impl FnOnce(&dyn PageObject) -> R,
) -> Option<R> {
    let mut f = Some(f);
    let mut result = None;
    unsafe {
        visit_page_object_ref(handle, &mut |object| {
            result = Some(f.take().expect("one object visit")(object));
        });
    }
    result
}

unsafe fn visit_page_object_ref(
    handle: *mut c_void,
    f: &mut dyn FnMut(&dyn PageObject),
) -> Option<()> {
    if handle.is_null() {
        return None;
    }
    let location = unsafe { &(*handle.cast::<PageObjectHandle>()).location };
    match location {
        ObjectLocation::Owned(object) => {
            f(object.as_ref());
            Some(())
        }
        ObjectLocation::Attached { page, index, id } => {
            let holder = unsafe { page.as_ref() }.pdf_page()?.holder();
            let index = if holder.page_object_id_by_index(*index) == Some(*id) {
                *index
            } else {
                (0..holder.page_object_count())
                    .find(|&i| holder.page_object_id_by_index(i) == Some(*id))?
            };
            f(holder.page_object_by_index(index)?);
            Some(())
        }
        ObjectLocation::InForm { form, index, id } => {
            let mut result = None;
            unsafe {
                visit_page_object_ref(form.as_ptr().cast(), &mut |object| {
                    result = (|| {
                        let form = object.as_any().downcast_ref::<
                        pdfoxium_core::fpdfapi::page::cpdf_formobject::CpdfFormObject,
                    >()?;
                        let holder = form.form().holder();
                        let index = if holder.page_object_id_by_index(*index) == Some(*id) {
                            *index
                        } else {
                            (0..holder.page_object_count())
                                .find(|&i| holder.page_object_id_by_index(i) == Some(*id))?
                        };
                        f(holder.page_object_by_index(index)?);
                        Some(())
                    })();
                });
            }
            result
        }
        ObjectLocation::InAnnotation { annot, index, id } => {
            let context = (unsafe { &(*annot.as_ptr()).context }).as_ref()?;
            let holder = context.form()?.holder();
            let index = if holder.page_object_id_by_index(*index) == Some(*id) {
                *index
            } else {
                (0..holder.page_object_count())
                    .find(|&i| holder.page_object_id_by_index(i) == Some(*id))?
            };
            f(holder.page_object_by_index(index)?);
            Some(())
        }
        ObjectLocation::Transferring => unreachable!("synchronous object transfer"),
    }
}

struct BitmapHandle {
    storage: Rc<RefCell<BitmapStorage>>,
}

enum BitmapStorage {
    Owned(CfxDibitmap),
    External {
        width: c_int,
        height: c_int,
        format: FxdibFormat,
        pitch: u32,
        buffer: NonNull<u8>,
    },
}

impl BitmapHandle {
    fn dimensions(&self) -> (c_int, c_int) {
        match &*self.storage.borrow() {
            BitmapStorage::Owned(bitmap) => (bitmap.width(), bitmap.height()),
            BitmapStorage::External { width, height, .. } => (*width, *height),
        }
    }

    fn format(&self) -> FxdibFormat {
        match &*self.storage.borrow() {
            BitmapStorage::Owned(bitmap) => bitmap.format(),
            BitmapStorage::External { format, .. } => *format,
        }
    }

    fn pitch(&self) -> u32 {
        match &*self.storage.borrow() {
            BitmapStorage::Owned(bitmap) => bitmap.pitch(),
            BitmapStorage::External { pitch, .. } => *pitch,
        }
    }

    fn buffer_ptr(&mut self) -> *mut u8 {
        match &mut *self.storage.borrow_mut() {
            BitmapStorage::Owned(bitmap) => bitmap.buffer_mut().as_mut_ptr(),
            BitmapStorage::External { buffer, .. } => buffer.as_ptr(),
        }
    }

    /// # Safety
    /// The caller of `FPDFBitmap_CreateEx` keeps an external buffer writable
    /// and exclusive for the bitmap's lifetime.
    unsafe fn buffer_mut(&mut self) -> &mut [u8] {
        let length = self.pitch() as usize * self.dimensions().1 as usize;
        let pointer = self.buffer_ptr();
        // SAFETY: this API requires exclusive buffer access for the whole call;
        // no callback runs while a returned slice is alive.
        unsafe { std::slice::from_raw_parts_mut(pointer, length) }
    }
}

fn fill_bitmap_rect(
    buffer: &mut [u8],
    dimensions: (c_int, c_int),
    pitch: u32,
    format: FxdibFormat,
    rect: (c_int, c_int, c_int, c_int),
    color: u32,
) -> bool {
    let (left, top, width, height) = rect;
    let Some(right) = left.checked_add(width) else {
        return false;
    };
    let Some(bottom) = top.checked_add(height) else {
        return false;
    };
    let x0 = left.max(0).min(dimensions.0);
    let y0 = top.max(0).min(dimensions.1);
    let x1 = right.max(0).min(dimensions.0);
    let y1 = bottom.max(0).min(dimensions.1);
    if x1 <= x0 || y1 <= y0 {
        return true;
    }
    let pitch = pitch as usize;
    let x0 = x0 as usize;
    let x1 = x1 as usize;
    let (bytes, bpp) = match format {
        FxdibFormat::EightBppRgb => {
            let gray = rgb_to_gray(
                argb_r(color).into(),
                argb_g(color).into(),
                argb_b(color).into(),
            ) as u8;
            ([gray, 0, 0, 0], 1)
        }
        FxdibFormat::Bgr => ([argb_b(color), argb_g(color), argb_r(color), 0], 3),
        FxdibFormat::Bgrx | FxdibFormat::Bgra => {
            let alpha = if format == FxdibFormat::Bgrx {
                255
            } else {
                (color >> 24) as u8
            };
            ([argb_b(color), argb_g(color), argb_r(color), alpha], 4)
        }
        _ => return false,
    };
    // Same-byte colors (notably opaque white and transparent black) can
    // use memset. Fill contiguous rows together only when no padding or
    // pixels outside the rectangle would be touched.
    let uniform = bytes[..bpp].iter().all(|&byte| byte == bytes[0]);
    if uniform && x0 == 0 && x1 * bpp == pitch {
        if let Some(dest) = buffer.get_mut(y0 as usize * pitch..y1 as usize * pitch) {
            dest.fill(bytes[0]);
            return true;
        }
    }
    for row in y0 as usize..y1 as usize {
        let start = row * pitch + x0 * bpp;
        let end = row * pitch + x1 * bpp;
        let Some(dest) = buffer.get_mut(start..end) else {
            return false;
        };
        if uniform {
            dest.fill(bytes[0]);
        } else if bpp == 4 {
            for pixel in dest.chunks_exact_mut(4) {
                pixel.copy_from_slice(&bytes);
            }
        } else {
            debug_assert_eq!(bpp, 3);
            for pixel in dest.chunks_exact_mut(3) {
                pixel.copy_from_slice(&bytes[..3]);
            }
        }
    }
    true
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct FS_RECTF {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct FS_MATRIX {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct FS_SIZEF {
    pub width: f32,
    pub height: f32,
}

/// Layout of PDFium's `FPDF_FILEACCESS` callback descriptor.
#[repr(C)]
#[allow(non_snake_case, non_camel_case_types)]
pub struct FPDF_FILEACCESS {
    pub m_FileLen: c_ulong,
    pub m_GetBlock: Option<extern "C" fn(*mut c_void, c_ulong, *mut u8, c_ulong) -> c_int>,
    pub m_Param: *mut c_void,
}

/// Layout of `public/fpdf_save.h::FPDF_FILEWRITE`.
#[repr(C)]
#[allow(non_snake_case, non_camel_case_types)]
pub struct FPDF_FILEWRITE {
    pub version: c_int,
    pub WriteBlock:
        Option<unsafe extern "C" fn(*mut FPDF_FILEWRITE, *const c_void, c_ulong) -> c_int>,
}

/// `fpdfsdk/cpdfsdk_filewriteadapter.{h,cpp}`. The raw callback and its
/// non-owning descriptor stay within this FFI boundary.
struct CpdfSdkFileWriteAdapter {
    file_write: *mut FPDF_FILEWRITE,
}

impl CpdfSdkFileWriteAdapter {
    /// # Safety
    /// The descriptor and callback must remain valid through the last write,
    /// including the creator's final buffered write during destruction.
    unsafe fn new(file_write: *mut FPDF_FILEWRITE) -> Self {
        debug_assert!(!file_write.is_null());
        Self { file_write }
    }
}

impl WriteStream for CpdfSdkFileWriteAdapter {
    fn write_block(&mut self, buffer: &[u8]) -> bool {
        let size = c_ulong::try_from(buffer.len())
            .expect("CPDFSDK_FileWriteAdapter block exceeds unsigned long");
        // SAFETY: construction requires a live descriptor/callback. Read the
        // function pointer without holding a descriptor reference across user
        // code, which may re-enter document queries or update its descriptor.
        let write_block = unsafe { (*self.file_write).WriteBlock }
            .expect("FPDF_FILEWRITE requires a WriteBlock callback");
        unsafe { write_block(self.file_write, buffer.as_ptr().cast(), size) != 0 }
    }
}

struct CustomAccessStream {
    access: FPDF_FILEACCESS,
}

// CFX_ReadOnlySpanStream's non-owning source at the C ABI boundary. The
// LoadMemDocument caller retains readable, unchanged input until close.
struct MemoryReadStream {
    data: *const u8,
    size: usize,
}

impl StreamWithSize for MemoryReadStream {
    fn get_size(&self) -> i64 {
        self.size as i64
    }
}

impl SeekableReadStream for MemoryReadStream {
    fn read_block_at_offset(&self, buffer: &mut [u8], offset: i64) -> bool {
        let span = if self.size == 0 {
            &[]
        } else {
            // SAFETY: Only LoadMemDocument constructs this private stream,
            // after validating its length and pointer. The C caller keeps
            // the allocation readable and unchanged through document close.
            unsafe { std::slice::from_raw_parts(self.data, self.size) }
        };
        CfxReadOnlySpanStream::new(span).read_block_at_offset(buffer, offset)
    }
}

impl StreamWithSize for CustomAccessStream {
    fn get_size(&self) -> i64 {
        self.access.m_FileLen as i64
    }
}

impl SeekableReadStream for CustomAccessStream {
    fn read_block_at_offset(&self, buffer: &mut [u8], offset: i64) -> bool {
        if buffer.is_empty() || offset < 0 {
            return false;
        }
        let Ok(len) = c_ulong::try_from(buffer.len()) else {
            return false;
        };
        let Ok(offset) = c_ulong::try_from(offset) else {
            return false;
        };
        if offset
            .checked_add(len)
            .is_none_or(|end| end > self.access.m_FileLen)
        {
            return false;
        }
        self.access.m_GetBlock.is_some_and(|get_block| {
            get_block(self.access.m_Param, offset, buffer.as_mut_ptr(), len) != 0
        })
    }
}

fn set_parse_error(error: ParserError) {
    let code = match error {
        ParserError::Success => 0,
        ParserError::FileError => 2,
        ParserError::FormatError => 3,
        ParserError::PasswordError => 4,
        ParserError::HandlerError => 5,
    };
    set_last_error(code);
}

unsafe fn password_bytes(password: *const c_char) -> Vec<u8> {
    if password.is_null() {
        Vec::new()
    } else {
        // SAFETY: The PDFium C API requires a NUL-terminated password.
        unsafe { CStr::from_ptr(password) }.to_bytes().to_vec()
    }
}

fn load_document_stream(stream: Rc<dyn SeekableReadStream>, password: &[u8]) -> *mut c_void {
    let mut document = CpdfDocument::new();
    let error = document.load_doc(stream, password);
    if error != ParserError::Success {
        set_parse_error(error);
        return std::ptr::null_mut();
    }
    let document = Rc::new(RefCell::new(document));
    ext::report_unsupported_features(&document);
    Box::into_raw(Box::new(DocumentHandle::new(document))).cast()
}

/// # Safety
/// `file_access` must point to a valid descriptor. Its callback and parameter
/// must remain valid until the document and all its pages have closed.
/// `password` must be a NUL-terminated string when non-null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_LoadCustomDocument(
    file_access: *const FPDF_FILEACCESS,
    password: *const c_char,
) -> *mut c_void {
    if file_access.is_null() {
        return std::ptr::null_mut();
    }
    let access = unsafe { &*file_access };
    let stream = CustomAccessStream {
        access: FPDF_FILEACCESS {
            m_FileLen: access.m_FileLen,
            m_GetBlock: access.m_GetBlock,
            m_Param: access.m_Param,
        },
    };
    load_document_stream(Rc::new(stream), &unsafe { password_bytes(password) })
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_InitLibrary() {
    FPDF_InitLibraryWithConfig(std::ptr::null());
}

#[repr(C)]
#[allow(non_snake_case)]
struct FpdfLibraryConfigV6 {
    version: c_int,
    m_pUserFontPaths: *const *const c_char,
    m_pIsolate: *mut c_void,
    m_v8EmbedderSlot: u32,
    m_pPlatform: *mut c_void,
    m_RendererType: c_int,
    m_FontLibraryType: c_int,
    m_BrotliEnabled: c_int,
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_InitLibraryWithConfig(config: *const c_void) {
    if LIBRARY_INITIALIZED.with(Cell::get) {
        return;
    }
    let mut user_font_paths = None;
    if !config.is_null() {
        // The public struct begins with `version`; later fields are only
        // available when the caller supplies that version of the struct.
        let version = unsafe { config.cast::<c_int>().read() };
        // Address individual fields through a raw pointer: a version 1 caller
        // need only provide the initial fields, not the entire version 6 type.
        let config = config.cast::<FpdfLibraryConfigV6>();
        if version >= 4 {
            let renderer = unsafe { std::ptr::addr_of!((*config).m_RendererType).read() };
            assert_eq!(renderer, 0, "this build requires the AGG renderer");
        }
        if version >= 5 {
            let backend = unsafe { std::ptr::addr_of!((*config).m_FontLibraryType).read() };
            assert_eq!(backend, 1, "this build requires Fontations");
        }
        if version >= 6 {
            let enabled = unsafe { std::ptr::addr_of!((*config).m_BrotliEnabled).read() };
            set_brotli_enabled(enabled != 0);
        }
        let paths = unsafe { std::ptr::addr_of!((*config).m_pUserFontPaths).read() };
        if !paths.is_null() {
            let mut owned_paths = Vec::new();
            let mut index = 0;
            loop {
                // Public ABI: the path array is terminated by a null pointer,
                // and every preceding entry is a NUL-terminated C string.
                let path = unsafe { paths.add(index).read() };
                if path.is_null() {
                    break;
                }
                let bytes = unsafe { CStr::from_ptr(path) }.to_bytes();
                #[cfg(unix)]
                let path = {
                    use std::os::unix::ffi::OsStrExt;
                    std::path::PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
                };
                #[cfg(not(unix))]
                let path = std::path::PathBuf::from(String::from_utf8_lossy(bytes).as_ref());
                owned_paths.push(path);
                index += 1;
            }
            user_font_paths = Some(owned_paths);
        }
    }
    initialize_ge_module(user_font_paths);
    initialize_page_module();
    LIBRARY_INITIALIZED.with(|initialized| initialized.set(true));
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_DestroyLibrary() {
    if !LIBRARY_INITIALIZED.with(Cell::get) {
        return;
    }
    destroy_page_module();
    destroy_ge_module();
    LIBRARY_INITIALIZED.with(|initialized| initialized.set(false));
}

/// Set the calling thread's new color-plane realization threshold. Zero keeps
/// color planes lazy; UINT64_MAX is unlimited. Existing entries, masks and
/// decoder storage are unaffected. Requires an initialized library instance.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn PDFOXIUM_SetImageMemoryLimits(cached_image_bytes: u64) -> c_int {
    use pdfoxium_core::fpdfapi::page::image_memory::{set_image_memory_limits, ImageMemoryLimits};
    if !LIBRARY_INITIALIZED.with(Cell::get) {
        return 0;
    }
    let limit = if cached_image_bytes == u64::MAX {
        usize::MAX
    } else if let Ok(value) = usize::try_from(cached_image_bytes) {
        value
    } else {
        return 0;
    };
    set_image_memory_limits(ImageMemoryLimits {
        cached_image_bytes: limit,
    });
    1
}

/// # Safety
/// The output must point to a writable uint64_t. A null output or uninitialized
/// library fails without writing. UINT64_MAX denotes unlimited.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn PDFOXIUM_GetImageMemoryLimits(cached_image_bytes: *mut u64) -> c_int {
    if !LIBRARY_INITIALIZED.with(Cell::get) || cached_image_bytes.is_null() {
        return 0;
    }
    let limit =
        pdfoxium_core::fpdfapi::page::image_memory::image_memory_limits().cached_image_bytes;
    unsafe {
        cached_image_bytes.write(if limit == usize::MAX {
            u64::MAX
        } else {
            limit as u64
        });
    }
    1
}

/// Configure the initialized calling thread's library instance. Zero disables
/// retention; UINT64_MAX means unlimited. No PDFium struct or flag is changed.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn PDFOXIUM_SetFontMemoryLimits(
    glyph_cache_bytes: u64,
    glyph_scratch_bytes: u64,
) -> c_int {
    use pdfoxium_core::fxge::font_memory::{set_font_memory_limits, FontMemoryLimits};
    if !LIBRARY_INITIALIZED.with(Cell::get) {
        return 0;
    }
    let convert = |value| {
        if value == u64::MAX {
            Some(usize::MAX)
        } else {
            usize::try_from(value).ok()
        }
    };
    let (Some(glyph_cache_bytes), Some(glyph_scratch_bytes)) =
        (convert(glyph_cache_bytes), convert(glyph_scratch_bytes))
    else {
        return 0;
    };
    set_font_memory_limits(FontMemoryLimits {
        glyph_cache_bytes,
        glyph_scratch_bytes,
    });
    1
}

/// # Safety
/// Both outputs must point to writable uint64_t values. Null outputs fail
/// without writing either value. Requires an initialized library instance.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn PDFOXIUM_GetFontMemoryLimits(
    glyph_cache_bytes: *mut u64,
    glyph_scratch_bytes: *mut u64,
) -> c_int {
    if !LIBRARY_INITIALIZED.with(Cell::get)
        || glyph_cache_bytes.is_null()
        || glyph_scratch_bytes.is_null()
    {
        return 0;
    }
    let limits = pdfoxium_core::fxge::font_memory::font_memory_limits();
    let convert = |value| {
        if value == usize::MAX {
            u64::MAX
        } else {
            value as u64
        }
    };
    unsafe {
        glyph_cache_bytes.write(convert(limits.glyph_cache_bytes));
        glyph_scratch_bytes.write(convert(limits.glyph_scratch_bytes));
    }
    1
}

/// # Safety
/// Both outputs must point to writable uint64_t values. These are accounted
/// retained bytes, excluding active scratch, externally retained results, and RSS.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn PDFOXIUM_GetFontMemoryUsage(
    glyph_cache_bytes: *mut u64,
    glyph_scratch_bytes: *mut u64,
) -> c_int {
    if !LIBRARY_INITIALIZED.with(Cell::get)
        || glyph_cache_bytes.is_null()
        || glyph_scratch_bytes.is_null()
    {
        return 0;
    }
    let usage = pdfoxium_core::fxge::font_memory::font_memory_usage();
    unsafe {
        glyph_cache_bytes.write(usage.glyph_cache_bytes as u64);
        glyph_scratch_bytes.write(usage.glyph_scratch_bytes as u64);
    }
    1
}

/// Flush idle retention, keeping configuration. Checked-out scratch is
/// discarded on return. Does not call malloc_trim or touch page image caches.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn PDFOXIUM_TrimFontMemory() -> c_int {
    if !LIBRARY_INITIALIZED.with(Cell::get) {
        return 0;
    }
    pdfoxium_core::fxge::font_memory::trim_font_memory();
    1
}

/// # Safety
/// `data_buf` must point to `size` readable bytes that remain valid and
/// unchanged until the document and all its pages have closed. `password`
/// must be a NUL-terminated string when non-null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_LoadMemDocument64(
    data_buf: *const c_void,
    size: usize,
    password: *const c_char,
) -> *mut c_void {
    if size > isize::MAX as usize || (size != 0 && data_buf.is_null()) {
        set_parse_error(ParserError::FileError);
        return std::ptr::null_mut();
    }
    let stream = MemoryReadStream {
        data: data_buf.cast(),
        size,
    };
    load_document_stream(Rc::new(stream), &unsafe { password_bytes(password) })
}

/// # Safety
/// The pointers must satisfy `FPDF_LoadMemDocument64`'s requirements.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_LoadMemDocument(
    data_buf: *const c_void,
    size: c_int,
    password: *const c_char,
) -> *mut c_void {
    if size < 0 {
        return std::ptr::null_mut();
    }
    unsafe { FPDF_LoadMemDocument64(data_buf, size as usize, password) }
}

/// # Safety
/// `file_path` and `password` must be NUL-terminated strings when non-null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_LoadDocument(
    file_path: *const c_char,
    password: *const c_char,
) -> *mut c_void {
    if file_path.is_null() {
        set_parse_error(ParserError::FileError);
        return std::ptr::null_mut();
    }
    let path = {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            std::path::PathBuf::from(std::ffi::OsStr::from_bytes(
                unsafe { CStr::from_ptr(file_path) }.to_bytes(),
            ))
        }
        #[cfg(not(unix))]
        {
            let Ok(path) = unsafe { CStr::from_ptr(file_path) }.to_str() else {
                set_parse_error(ParserError::FileError);
                return std::ptr::null_mut();
            };
            std::path::PathBuf::from(path)
        }
    };
    let Some(stream) = CfxFileAccessStream::create_from_path(&path) else {
        set_parse_error(ParserError::FileError);
        return std::ptr::null_mut();
    };
    load_document_stream(Rc::new(stream), &unsafe { password_bytes(password) })
}

/// # Safety
/// `document` must be a handle returned by a successful load call, or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_CloseDocument(document: *mut c_void) {
    if !document.is_null() {
        let owner = unsafe { &*document.cast::<DocumentHandle>() }
            .document
            .clone();
        formfill::detach_document_forms(&owner);
        // SAFETY: The caller transfers its live document handle back to us.
        drop(unsafe { Box::from_raw(document.cast::<DocumentHandle>()) });
    }
}

// JS builds retain Boa clock/timezone behavior; other builds record system UTC.
static SANDBOX_POLICY: AtomicU32 = AtomicU32::new(u32::MAX);
fn machine_time_enabled() -> bool {
    SANDBOX_POLICY.load(Ordering::Relaxed) & 1 != 0
}
fn document_date_fields(
    document: Option<&Rc<RefCell<CpdfDocument>>>,
) -> Option<(fpdf_editpage::PdfDateFields, fpdf_editpage::PdfDateFields)> {
    #[cfg(feature = "js")]
    {
        let runtime = document.and_then(formfill::document_runtime);
        if let Some(runtime) = runtime {
            runtime
                .engine()
                .with_context(|context| {
                    Ok(pdfoxium_core::fxjs::js_date::pdf_date_fields(Some(context)))
                })
                .ok()
        } else {
            Some(pdfoxium_core::fxjs::js_date::pdf_date_fields(None))
        }
    }
    #[cfg(not(feature = "js"))]
    {
        use pdfoxium_core::fxcrt::fx_date_helpers as date;
        use std::time::{SystemTime, UNIX_EPOCH};
        let _ = document;
        // Metadata remains available without an interpreter. Use a real UTC
        // instant and an explicit zero offset rather than inventing local time.
        let utc = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_secs_f64() * 1000.0,
            Err(error) => -error.duration().as_secs_f64() * 1000.0,
        };
        let fields = fpdf_editpage::PdfDateFields {
            year: date::get_year_from_time(utc),
            month: (date::get_month_from_time(utc) + 1) as u32,
            day: date::get_day_from_time(utc) as u32,
            hour: date::get_hour_from_time(utc) as u32,
            minute: date::get_min_from_time(utc) as u32,
            second: date::get_sec_from_time(utc) as u32,
        };
        Some((fields, fields))
    }
}
fn creation_date() -> Option<Vec<u8>> {
    let (local, utc) = document_date_fields(None)?;
    Some(fpdf_editpage::format_pdf_date(local, Some(utc)))
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_SetSandBoxPolicy(policy: c_ulong, enable: c_int) {
    if policy == 0 {
        if enable != 0 {
            SANDBOX_POLICY.fetch_or(1, Ordering::Relaxed);
        } else {
            SANDBOX_POLICY.fetch_and(!1, Ordering::Relaxed);
        }
    }
}
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_CreateNewDocument() -> *mut c_void {
    let document = fpdf_editpage::create_new_document(machine_time_enabled(), creation_date);
    Box::into_raw(Box::new(DocumentHandle::new(document))).cast()
}
/// # Safety
/// Document is null or a live document handle.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetFormType(document: *mut c_void) -> c_int {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    let root = document.document.borrow().root();
    pdfoxium_core::fpdfsdk::fpdf_view::form_type(root.as_ref())
}

/// # Safety
/// Source must be a live document or null, kept live through input callbacks.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_ImportNPagesToOne(
    src: *mut c_void,
    width: f32,
    height: f32,
    x: usize,
    y: usize,
) -> *mut c_void {
    if src.is_null()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.
        || height <= 0.
        || x == 0
        || y == 0
    {
        return std::ptr::null_mut();
    }
    let source = Rc::clone(&unsafe { &*src.cast::<DocumentHandle>() }.document);
    let document = fpdf_editpage::create_new_document(machine_time_enabled(), creation_date);
    if !pdfoxium_core::fpdfsdk::fpdf_ppo::import_n_pages_to_one(
        Rc::clone(&document),
        source,
        width,
        height,
        x,
        y,
    ) {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(DocumentHandle::new(document))).cast()
}
/// # Safety
/// Documents must be live or null; the destination outlives the returned
/// context. Source stays live through synchronous import callbacks.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_NewXObjectFromPage(
    dest: *mut c_void,
    src: *mut c_void,
    index: c_int,
) -> *mut c_void {
    let Some((dest, src)) = (unsafe { import_document_handles(dest, src) }) else {
        return std::ptr::null_mut();
    };
    let Some(context) =
        CpdfNPageToOneExporter::new(dest, src).create_xobject_context_from_page(index)
    else {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(context)).cast()
}
/// # Safety
/// Context must be live or null, and must close before its destination.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_CloseXObject(context: *mut c_void) {
    if !context.is_null() {
        drop(unsafe { Box::from_raw(context.cast::<XObjectContext>()) });
    }
}
/// # Safety
/// Context must be live or null, with a live destination through parsing callbacks.
/// Caller owns the returned page object until inserting or destroying it.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_NewFormObjectFromXObject(context: *mut c_void) -> *mut c_void {
    if context.is_null() {
        return std::ptr::null_mut();
    }
    let context = unsafe { &*context.cast::<XObjectContext>() };
    let object = pdfoxium_core::fpdfsdk::fpdf_ppo::new_form_object_from_xobject(context);
    new_owned_page_object(Box::new(object))
}
/// # Safety
/// Document must be live or null and exclusively accessible during mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_New(
    document: *mut c_void,
    index: c_int,
    width: f64,
    height: f64,
) -> *mut c_void {
    if document.is_null() {
        return std::ptr::null_mut();
    }
    let handle = unsafe { &*document.cast::<DocumentHandle>() };
    let attachment_handles = Rc::clone(&handle.attachment_handles);
    let borrowed_objects = Rc::clone(&handle.borrowed_objects);
    let link_list = Rc::clone(&handle.link_list);
    let document = Rc::clone(&handle.document);
    #[cfg(feature = "xfa")]
    let extension = handle.xfa_context.clone();
    let index = index.clamp(0, document.borrow().page_count() as c_int);
    let Some(page) = fpdf_editpage::new_page(&document, index, width, height) else {
        return std::ptr::null_mut();
    };
    // Native FPDFPage_New wraps a real newly created PDF page whenever the
    // document has an XFA extension, even before the XFA model is loaded.
    #[cfg(feature = "xfa")]
    let (page, xfa_page, sdk_page) = if let Some(extension) = extension {
        let xfa = pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_page::CpdfXfaPage::new_with_pdf_page(
            Rc::downgrade(&extension),
            index,
            page,
        );
        let page = xfa.take_pdf_page().expect("new PDF backing page");
        let sdk =
            pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage::new_xfa(&document, xfa.clone());
        (page, Some(xfa), sdk)
    } else {
        let sdk = pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage::new(&document, &page);
        (Rc::new(page), None, sdk)
    };
    #[cfg(not(feature = "xfa"))]
    let (page, sdk_page) = {
        let sdk = pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage::new(&document, &page);
        (Rc::new(page), sdk)
    };
    Box::into_raw(Box::new(PageHandle {
        sdk_page,
        page: Some(page),
        #[cfg(feature = "xfa")]
        xfa_page,
        progressive: Default::default(),
        object_handles: Default::default(),
        _document: document,
        attachment_handles,
        borrowed_objects,
        link_list,
    }))
    .cast()
}
/// # Safety
/// Page is live or null. Object is null or a live caller-owned object from this
/// library. Ownership transfers even when the page is null or invalid.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_InsertObject(page: *mut c_void, object: *mut c_void) -> c_int {
    if object.is_null() {
        return 0;
    }
    let mut handle = unsafe { Box::from_raw(object.cast::<PageObjectHandle>()) };
    let ObjectLocation::Owned(mut object) =
        std::mem::replace(&mut handle.location, ObjectLocation::Transferring)
    else {
        panic!("insert requires caller-owned object");
    };
    if page.is_null() {
        return 0;
    }
    let page_pointer = NonNull::new(page.cast::<PageHandle>()).unwrap();
    let page = unsafe { &mut *page_pointer.as_ptr() };
    if !page
        .pdf_page()
        .is_some_and(|page| CpdfPage::is_valid_page_dict(&page.dict()))
    {
        return 0;
    }
    object.base_mut().set_dirty(true);
    let Some(mut engine_page) = page.page_mut() else {
        return 0;
    };
    let holder = engine_page.mutable_holder();
    let index = holder.page_object_count();
    holder.append_page_object(object);
    fpdf_editpage::calc_bounding_box(holder.page_object_by_index_mut(index).unwrap());
    let id = holder.page_object_id_by_index(index).unwrap();
    handle.location = ObjectLocation::Attached {
        page: page_pointer,
        index,
        id,
    };
    drop(engine_page);
    page.object_handles.insert(id, handle);
    1
}
/// # Safety
/// Object is null or a live caller-owned object, never an object owned by a page.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_Destroy(object: *mut c_void) {
    if !object.is_null() {
        drop(unsafe { Box::from_raw(object.cast::<PageObjectHandle>()) });
    }
}
/// # Safety
/// Object is live or null; any owning page remains live for this call.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_GetType(object: *mut c_void) -> c_int {
    unsafe { with_page_object(object, |o| o.object_type() as c_int) }.unwrap_or(0)
}
/// # Safety
/// Object is live or null. For a live object, all four output pointers must be
/// writable. The owning page must remain live and exclusively accessible.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_GetBounds(
    object: *mut c_void,
    left: *mut f32,
    bottom: *mut f32,
    right: *mut f32,
    top: *mut f32,
) -> c_int {
    let rect = unsafe { with_page_object(object, |o| o.base().rect()) };
    let Some(rect) = rect else {
        return 0;
    };
    unsafe {
        *left = rect.left;
        *bottom = rect.bottom;
        *right = rect.right;
        *top = rect.top;
    }
    1
}
/// # Safety
/// Object is live or null. Matrix is null or readable. The owning page must
/// remain live and exclusively accessible through this synchronous mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_SetMatrix(
    object: *mut c_void,
    matrix: *const FS_MATRIX,
) -> c_int {
    if object.is_null() || matrix.is_null() {
        return 0;
    }
    let m = unsafe { &*matrix };
    let matrix = pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix::new(m.a, m.b, m.c, m.d, m.e, m.f);
    unsafe { with_page_object(object, |o| fpdf_editpage::set_object_matrix(o, matrix)) }
        .unwrap_or(false) as c_int
}
/// # Safety
/// Page must be live or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_CountObjects(page: *mut c_void) -> c_int {
    if page.is_null() {
        return -1;
    }
    let Some(page) = unsafe { &*page.cast::<PageHandle>() }.pdf_page() else {
        return -1;
    };
    if !CpdfPage::is_valid_page_dict(&page.dict()) {
        return -1;
    }
    c_int::try_from(page.holder().page_object_count()).expect("native checked object count")
}
/// # Safety
/// Page must be live or null. The returned borrowed handle is owned by this page
/// and must not be destroyed or inserted elsewhere. It expires when the page closes.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_GetObject(page: *mut c_void, index: c_int) -> *mut c_void {
    let Some(pointer) = NonNull::new(page.cast::<PageHandle>()) else {
        return std::ptr::null_mut();
    };
    let page = unsafe { &mut *pointer.as_ptr() };
    if !page
        .pdf_page()
        .is_some_and(|page| CpdfPage::is_valid_page_dict(&page.dict()))
    {
        return std::ptr::null_mut();
    }
    let index = index as usize;
    let Some(id) = page
        .pdf_page()
        .unwrap()
        .holder()
        .page_object_id_by_index(index)
    else {
        return std::ptr::null_mut();
    };
    if let Some(handle) = page.object_handles.get_mut(&id) {
        return (&mut **handle as *mut PageObjectHandle).cast();
    }
    let mut handle = Box::new(PageObjectHandle::new(ObjectLocation::Attached {
        page: pointer,
        index,
        id,
    }));
    let result = (&mut *handle as *mut PageObjectHandle).cast();
    page.object_handles.insert(id, handle);
    result
}
/// # Safety
/// Object is live or null; any owning page stays live through this call.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFFormObj_CountObjects(object: *mut c_void) -> c_int {
    unsafe {
        with_page_object(object, |o| {
            o.as_any()
                .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_formobject::CpdfFormObject>()
                .map(|o| {
                    c_int::try_from(o.form().holder().page_object_count())
                        .expect("native checked form count")
                })
                .unwrap_or(-1)
        })
    }
    .unwrap_or(-1)
}

// `fpdfsdk/fpdf_ppo.cpp` entrypoints and extension page-cache notifications.
unsafe fn import_document_handles(
    dest: *mut c_void,
    src: *mut c_void,
) -> Option<(Rc<RefCell<CpdfDocument>>, Rc<RefCell<CpdfDocument>>)> {
    if dest.is_null() || src.is_null() {
        return None;
    }
    // SAFETY: callers supply live document handles; only their retained owners
    // are copied, so no handle/document borrow spans import input callbacks.
    let dest = Rc::clone(&unsafe { &*dest.cast::<DocumentHandle>() }.document);
    let src = Rc::clone(&unsafe { &*src.cast::<DocumentHandle>() }.document);
    Some((dest, src))
}

/// # Safety
/// Document handles must be live or null. For a non-null indices pointer and
/// positive length, the caller supplies that many readable aligned ints. Both
/// documents and the index data must remain live through synchronous callbacks.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_ImportPagesByIndex(
    dest_doc: *mut c_void,
    src_doc: *mut c_void,
    page_indices: *const c_int,
    length: c_ulong,
    index: c_int,
) -> c_int {
    let Some((dest, src)) = (unsafe { import_document_handles(dest_doc, src_doc) }) else {
        return 0;
    };
    let length = usize::try_from(length).expect("native checked page-index length");
    let indices = if page_indices.is_null() {
        None
    } else {
        if length == 0 {
            return 0;
        }
        // SAFETY: positive-length input is required to be readable and aligned.
        Some(unsafe { std::slice::from_raw_parts(page_indices, length) })
    };
    #[cfg(feature = "xfa")]
    let extension = unsafe { &*dest_doc.cast::<DocumentHandle>() }
        .xfa_context
        .clone();
    #[cfg(feature = "xfa")]
    let before = dest.borrow().page_count();
    let result =
        pdfoxium_core::fpdfsdk::fpdf_ppo::import_pages_by_index(dest.clone(), src, indices, index);
    #[cfg(feature = "xfa")]
    if result {
        let count = dest.borrow().page_count().saturating_sub(before);
        if let Some(extension) = extension {
            extension.pages_inserted(index, count);
        }
    }
    result as c_int
}

/// # Safety
/// Documents must be live or null, and pagerange must be null or a readable
/// NUL-terminated string. Inputs remain live through synchronous callbacks.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_ImportPages(
    dest_doc: *mut c_void,
    src_doc: *mut c_void,
    pagerange: *const c_char,
    index: c_int,
) -> c_int {
    let Some((dest, src)) = (unsafe { import_document_handles(dest_doc, src_doc) }) else {
        return 0;
    };
    let range = if pagerange.is_null() {
        b"".as_slice()
    } else {
        unsafe { CStr::from_ptr(pagerange) }.to_bytes()
    };
    #[cfg(feature = "xfa")]
    let extension = unsafe { &*dest_doc.cast::<DocumentHandle>() }
        .xfa_context
        .clone();
    #[cfg(feature = "xfa")]
    let before = dest.borrow().page_count();
    let result = pdfoxium_core::fpdfsdk::fpdf_ppo::import_pages(dest.clone(), src, range, index);
    #[cfg(feature = "xfa")]
    if result {
        let count = dest.borrow().page_count().saturating_sub(before);
        if let Some(extension) = extension {
            extension.pages_inserted(index, count);
        }
    }
    result as c_int
}

/// # Safety
/// Document handles must be live or null for this synchronous call.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_CopyViewerPreferences(
    dest_doc: *mut c_void,
    src_doc: *mut c_void,
) -> c_int {
    let Some((dest, src)) = (unsafe { import_document_handles(dest_doc, src_doc) }) else {
        return 0;
    };
    pdfoxium_core::fpdfsdk::fpdf_ppo::copy_viewer_preferences(&dest, &src) as c_int
}

// `fpdfsdk/fpdf_save.cpp::DoDocSave`, including the XFA save-event/package ordering.
unsafe fn do_doc_save(
    document: *mut c_void,
    file_write: *mut FPDF_FILEWRITE,
    flags: c_ulong,
    version: Option<c_int>,
) -> bool {
    if document.is_null() {
        return false;
    }
    // Retain both owners before callbacks so no handle or document borrow spans
    // pre-save scripts, package export, writer callbacks, or post-save scripts.
    let handle = unsafe { &*document.cast::<DocumentHandle>() };
    let document = handle.document.clone();
    #[cfg(feature = "xfa")]
    let extension = handle.xfa_context.clone();
    #[cfg(feature = "xfa")]
    if let Some(extension) = &extension {
        extension.send_pre_save_to_xfa_doc();
        // Native still saves the PDF if the optional XFA package update fails.
        let _ = pdfoxium_core::fpdfsdk::fpdf_xfa_save::save_xfa_document_data(Some(extension));
    }
    let result = {
        let mut archive = unsafe { CpdfSdkFileWriteAdapter::new(file_write) };
        let mut creator = CpdfCreator::new(document, &mut archive);
        // Native explicitly narrows FPDF_DWORD (unsigned long) to uint32_t.
        creator.create(flags as u32, version.unwrap_or(0))
    };
    #[cfg(feature = "xfa")]
    if let Some(extension) = extension {
        extension.send_post_save_to_xfa_doc();
    }
    result
}

/// `fpdfsdk/fpdf_save.cpp::FPDF_SaveAsCopy`.
///
/// # Safety
/// `document` must be a live document handle or null. For a live document,
/// `file_write` must point to a writable descriptor with a valid callback that
/// remains live for this call. The callback may query the document, but must
/// not close it or mutate it while saving.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_SaveAsCopy(
    document: *mut c_void,
    file_write: *mut FPDF_FILEWRITE,
    flags: c_ulong,
) -> c_int {
    unsafe { do_doc_save(document, file_write, flags, None) as c_int }
}

/// `fpdfsdk/fpdf_save.cpp::FPDF_SaveWithVersion`.
///
/// # Safety
/// The document and writer must meet `FPDF_SaveAsCopy`'s lifetime and callback
/// requirements.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_SaveWithVersion(
    document: *mut c_void,
    file_write: *mut FPDF_FILEWRITE,
    flags: c_ulong,
    file_version: c_int,
) -> c_int {
    unsafe { do_doc_save(document, file_write, flags, Some(file_version)) as c_int }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_GetLastError() -> c_ulong {
    c_ulong::from(get_last_error())
}

/// # Safety
/// `document` must be a live document handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetPageCount(document: *mut c_void) -> c_int {
    if document.is_null() {
        return 0;
    }
    let handle = unsafe { &*document.cast::<DocumentHandle>() };
    #[cfg(feature = "xfa")]
    if let Some(context) = &handle.xfa_context {
        return context.get_page_count();
    }
    handle.document.borrow().page_count() as c_int
}

/// # Safety
/// `document` must be a live document handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_DocumentHasValidCrossReferenceTable(document: *mut c_void) -> c_int {
    if document.is_null() {
        return 0;
    }
    unsafe { &*document.cast::<DocumentHandle>() }
        .document
        .borrow()
        .has_valid_cross_reference_table() as c_int
}

/// # Safety
/// `document` must be a live document handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetDocPermissions(document: *mut c_void) -> c_ulong {
    if document.is_null() {
        return 0;
    }
    unsafe { &*document.cast::<DocumentHandle>() }
        .document
        .borrow()
        .user_permissions(true) as c_ulong
}

/// # Safety
/// `document` must be a live document handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetDocUserPermissions(document: *mut c_void) -> c_ulong {
    if document.is_null() {
        return 0;
    }
    unsafe { &*document.cast::<DocumentHandle>() }
        .document
        .borrow()
        .user_permissions(false) as c_ulong
}

/// # Safety
/// `document` must be a live document handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetSecurityHandlerRevision(document: *mut c_void) -> c_int {
    if document.is_null() {
        return -1;
    }
    let handle = unsafe { &*document.cast::<DocumentHandle>() };
    let document = handle.document.borrow();
    let Some(encrypt) = document.parser().encrypt_dict() else {
        return -1;
    };
    let borrowed = encrypt.borrow();
    borrowed
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .map_or(-1, |dict| dict.get_integer_for(b"R", 0))
}

/// # Safety
/// `document` must be a live document handle or null. `file_version` must be
/// null or point to a writable C `int`.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetFileVersion(
    document: *mut c_void,
    file_version: *mut c_int,
) -> c_int {
    if file_version.is_null() {
        return 0;
    }

    // SAFETY: The caller supplies a valid writable pointer per the C ABI.
    unsafe { *file_version = 0 };
    if document.is_null() {
        return 0;
    }

    let handle = unsafe { &*document.cast::<DocumentHandle>() };
    let document = handle.document.borrow();
    // Native API-created documents have no parser. Rust retains parser storage
    // for their indirect objects, but without a source file there is no loaded
    // file version to report, including after saving that document.
    if document.parser().document_size().is_none() {
        return 0;
    }
    let mut version = document.parser().file_version();
    if let Some(root) = document.root() {
        let borrowed = root.borrow();
        if let Some(root) = borrowed.as_any().downcast_ref::<CpdfDictionary>() {
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
    unsafe { *file_version = version };
    1
}

/// # Safety
/// `document` must be a live document handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_LoadPage(document: *mut c_void, page_index: c_int) -> *mut c_void {
    #[cfg(feature = "render-profile")]
    let _profile = pdfoxium_core::render_profile::Session::new("page load other");
    if document.is_null() || page_index < 0 {
        return std::ptr::null_mut();
    }
    let handle = unsafe { &*document.cast::<DocumentHandle>() };
    #[cfg(feature = "xfa")]
    if let Some(context) = handle.xfa_context.clone() {
        let document = handle.document.clone();
        let attachment_handles = handle.attachment_handles.clone();
        let borrowed_objects = handle.borrowed_objects.clone();
        let link_list = handle.link_list.clone();
        let Some(loaded) = pdfoxium::engine::load_xfa_page(&document, &context, page_index) else {
            return std::ptr::null_mut();
        };
        return Box::into_raw(Box::new(PageHandle {
            page: loaded.pdf,
            #[cfg(feature = "xfa")]
            xfa_page: Some(loaded.xfa),
            progressive: Default::default(),
            sdk_page: loaded.sdk,
            object_handles: Default::default(),
            _document: document,
            attachment_handles,
            borrowed_objects,
            link_list,
        }))
        .cast();
    }
    let Some(page) = pdfoxium::engine::load_pdf_page(&handle.document, page_index as usize) else {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(PageHandle {
        sdk_page: pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage::new(&handle.document, &page),
        object_handles: Default::default(),
        page: Some(page),
        #[cfg(feature = "xfa")]
        xfa_page: None,
        progressive: Default::default(),
        _document: Rc::clone(&handle.document),
        attachment_handles: Rc::clone(&handle.attachment_handles),
        borrowed_objects: Rc::clone(&handle.borrowed_objects),
        link_list: Rc::clone(&handle.link_list),
    }))
    .cast()
}

/// # Safety
/// `page` must be a live page handle returned by `FPDF_LoadPage`, or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_ClosePage(page: *mut c_void) {
    if !page.is_null() {
        drop(unsafe { Box::from_raw(page.cast::<PageHandle>()) });
    }
}

/// `fpdfsdk/fpdf_editpage.cpp::FPDFPage_GenerateContent`.
///
/// # Safety
/// `page` must be a live page handle or null. The caller must provide exclusive
/// access to the page and its owning document for the duration of this call.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_GenerateContent(page: *mut c_void) -> c_int {
    if page.is_null() {
        return 0;
    }
    let handle = unsafe { &mut *page.cast::<PageHandle>() };
    // Upstream's IsPageObject checks the current dictionary, since it may have
    // changed after the page was loaded.
    if !handle
        .pdf_page()
        .is_some_and(|page| CpdfPage::is_valid_page_dict(&page.dict()))
    {
        return 0;
    }

    let document_owner = handle._document.clone();
    let Some(mut engine_page) = handle.page_mut() else {
        return 0;
    };
    let mut document = document_owner.borrow_mut();
    CpdfPageContentGenerator::new(engine_page.mutable_holder(), Some(&mut document))
        .generate_content();
    1
}

/// # Safety
/// `page` must be a live page handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetPageWidthF(page: *mut c_void) -> f32 {
    if page.is_null() {
        return 0.0;
    }
    unsafe { &*page.cast::<PageHandle>() }.sdk_page.width()
}

/// # Safety
/// `page` must be a live page handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetPageWidth(page: *mut c_void) -> f64 {
    f64::from(unsafe { FPDF_GetPageWidthF(page) })
}

/// # Safety
/// `page` must be a live page handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetPageHeightF(page: *mut c_void) -> f32 {
    if page.is_null() {
        return 0.0;
    }
    unsafe { &*page.cast::<PageHandle>() }.sdk_page.height()
}

/// # Safety
/// `page` must be a live page handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetPageHeight(page: *mut c_void) -> f64 {
    f64::from(unsafe { FPDF_GetPageHeightF(page) })
}

/// # Safety
/// `page` must be a live page handle or null; `rect` must be null or writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetPageBoundingBox(page: *mut c_void, rect: *mut FS_RECTF) -> c_int {
    if page.is_null() || rect.is_null() {
        return 0;
    }
    let Some(pdf) = unsafe { &*page.cast::<PageHandle>() }.pdf_page() else {
        return 0;
    };
    let bbox = pdf.bbox();
    unsafe {
        *rect = FS_RECTF {
            left: bbox.left,
            top: bbox.top,
            right: bbox.right,
            bottom: bbox.bottom,
        };
    }
    1
}

/// # Safety
/// `document` must be a live document handle or null; `size` must be null or writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetPageSizeByIndexF(
    document: *mut c_void,
    page_index: c_int,
    size: *mut FS_SIZEF,
) -> c_int {
    if document.is_null() || page_index < 0 || size.is_null() {
        return 0;
    }
    let handle = unsafe { &*document.cast::<DocumentHandle>() };
    #[cfg(feature = "xfa")]
    if let Some(context) = &handle.xfa_context {
        let Some(page) = context.get_or_create_xfa_page(page_index) else {
            return 0;
        };
        unsafe {
            *size = FS_SIZEF {
                width: page.width(),
                height: page.height(),
            };
        }
        return 1;
    }
    let mut document = handle.document.borrow_mut();
    let Some(page) = CpdfPage::from_document(&mut document, page_index as usize) else {
        return 0;
    };
    unsafe {
        *size = FS_SIZEF {
            width: page.width(),
            height: page.height(),
        };
    }
    1
}

/// # Safety
/// `document` must be a live document handle or null; output pointers must be
/// null or writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_GetPageSizeByIndex(
    document: *mut c_void,
    page_index: c_int,
    width: *mut f64,
    height: *mut f64,
) -> c_int {
    if width.is_null() || height.is_null() {
        return 0;
    }
    let mut size = FS_SIZEF {
        width: 0.0,
        height: 0.0,
    };
    if unsafe { FPDF_GetPageSizeByIndexF(document, page_index, &mut size) } == 0 {
        return 0;
    }
    unsafe {
        *width = f64::from(size.width);
        *height = f64::from(size.height);
    }
    1
}

/// # Safety
/// `page` must be a live page handle or null. Output pointers must be
/// writable when non-null.
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "C" fn FPDF_DeviceToPage(
    page: *mut c_void,
    start_x: c_int,
    start_y: c_int,
    size_x: c_int,
    size_y: c_int,
    rotate: c_int,
    device_x: c_int,
    device_y: c_int,
    page_x: *mut f64,
    page_y: *mut f64,
) -> c_int {
    if page.is_null() || page_x.is_null() || page_y.is_null() {
        return 0;
    }
    let (Some(end_x), Some(end_y)) = (start_x.checked_add(size_x), start_y.checked_add(size_y))
    else {
        return 0;
    };
    let rect = FxRect::new(start_x, start_y, end_x, end_y);
    let page = unsafe { &*page.cast::<PageHandle>() };
    let point = page
        .sdk_page
        .display_matrix_for_rect(rect, rotate)
        .get_inverse()
        .transform(CfxPointF::new(device_x as f32, device_y as f32));
    unsafe {
        *page_x = f64::from(point.x);
        *page_y = f64::from(point.y);
    }
    1
}

/// # Safety
/// `page` must be a live page handle or null. Output pointers must be
/// writable when non-null.
#[no_mangle]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub unsafe extern "C" fn FPDF_PageToDevice(
    page: *mut c_void,
    start_x: c_int,
    start_y: c_int,
    size_x: c_int,
    size_y: c_int,
    rotate: c_int,
    page_x: f64,
    page_y: f64,
    device_x: *mut c_int,
    device_y: *mut c_int,
) -> c_int {
    if page.is_null() || device_x.is_null() || device_y.is_null() {
        return 0;
    }
    let (Some(end_x), Some(end_y)) = (start_x.checked_add(size_x), start_y.checked_add(size_y))
    else {
        return 0;
    };
    let rect = FxRect::new(start_x, start_y, end_x, end_y);
    let page = unsafe { &*page.cast::<PageHandle>() };
    let point = page
        .sdk_page
        .display_matrix_for_rect(rect, rotate)
        .transform(CfxPointF::new(page_x as f32, page_y as f32));
    unsafe {
        *device_x = point.x.round() as c_int;
        *device_y = point.y.round() as c_int;
    }
    1
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDFBitmap_Create(width: c_int, height: c_int, alpha: c_int) -> *mut c_void {
    let format = if alpha == 0 {
        FxdibFormat::Bgrx
    } else {
        FxdibFormat::Bgra
    };
    let Some(bitmap) = CfxDibitmap::create(width, height, format, 0) else {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(BitmapHandle {
        storage: Rc::new(RefCell::new(BitmapStorage::Owned(bitmap))),
    }))
    .cast()
}

/// # Safety
/// When `first_scan` is non-null, it must point to a writable buffer large
/// enough for the calculated bitmap size and stay valid until destruction.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFBitmap_CreateEx(
    width: c_int,
    height: c_int,
    format: c_int,
    first_scan: *mut c_void,
    stride: c_int,
) -> *mut c_void {
    let format = match format {
        1 => FxdibFormat::EightBppRgb,
        2 => FxdibFormat::Bgr,
        3 => FxdibFormat::Bgrx,
        4 => FxdibFormat::Bgra,
        _ => return std::ptr::null_mut(),
    };
    let pitch = stride as u32;
    let storage = if let Some(buffer) = NonNull::new(first_scan.cast::<u8>()) {
        let Some((pitch, _size)) =
            CfxDibitmap::calculate_pitch_and_size(width, height, format, pitch)
        else {
            return std::ptr::null_mut();
        };
        BitmapStorage::External {
            width,
            height,
            format,
            pitch,
            buffer,
        }
    } else {
        let Some(bitmap) = CfxDibitmap::create(width, height, format, pitch) else {
            return std::ptr::null_mut();
        };
        BitmapStorage::Owned(bitmap)
    };
    Box::into_raw(Box::new(BitmapHandle {
        storage: Rc::new(RefCell::new(storage)),
    }))
    .cast()
}

/// # Safety
/// `bitmap` must be a live bitmap handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFBitmap_GetFormat(bitmap: *mut c_void) -> c_int {
    if bitmap.is_null() {
        return 0;
    }
    match unsafe { &*bitmap.cast::<BitmapHandle>() }.format() {
        FxdibFormat::EightBppRgb | FxdibFormat::EightBppMask => 1,
        FxdibFormat::Bgr => 2,
        FxdibFormat::Bgrx => 3,
        FxdibFormat::Bgra => 4,
        _ => 0,
    }
}

/// # Safety
/// `bitmap` must be a live bitmap handle or null. The returned pointer stays
/// valid until the bitmap is destroyed.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFBitmap_GetBuffer(bitmap: *mut c_void) -> *mut c_void {
    if bitmap.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { &mut *bitmap.cast::<BitmapHandle>() }
        .buffer_ptr()
        .cast()
}

/// # Safety
/// `bitmap` must be a live bitmap handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFBitmap_GetWidth(bitmap: *mut c_void) -> c_int {
    if bitmap.is_null() {
        return 0;
    }
    unsafe { &*bitmap.cast::<BitmapHandle>() }.dimensions().0
}

/// # Safety
/// `bitmap` must be a live bitmap handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFBitmap_GetHeight(bitmap: *mut c_void) -> c_int {
    if bitmap.is_null() {
        return 0;
    }
    unsafe { &*bitmap.cast::<BitmapHandle>() }.dimensions().1
}

/// # Safety
/// `bitmap` must be a live bitmap handle or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFBitmap_GetStride(bitmap: *mut c_void) -> c_int {
    if bitmap.is_null() {
        return 0;
    }
    unsafe { &*bitmap.cast::<BitmapHandle>() }.pitch() as c_int
}

/// # Safety
/// `bitmap` must be a live bitmap handle or null. A caller supplied buffer
/// must remain writable and exclusively available during the call.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFBitmap_FillRect(
    bitmap: *mut c_void,
    left: c_int,
    top: c_int,
    width: c_int,
    height: c_int,
    color: c_ulong,
) -> c_int {
    if bitmap.is_null() {
        return 0;
    }
    let bitmap = unsafe { &mut *bitmap.cast::<BitmapHandle>() };
    let dimensions = bitmap.dimensions();
    let pitch = bitmap.pitch();
    let format = bitmap.format();
    fill_bitmap_rect(
        unsafe { bitmap.buffer_mut() },
        dimensions,
        pitch,
        format,
        (left, top, width, height),
        color as u32,
    ) as c_int
}

/// # Safety
/// `bitmap` must be a live bitmap handle returned by a bitmap create call, or null.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFBitmap_Destroy(bitmap: *mut c_void) {
    if !bitmap.is_null() {
        drop(unsafe { Box::from_raw(bitmap.cast::<BitmapHandle>()) });
    }
}

/// # Safety
/// `bitmap` and `page` must be live handles. A caller supplied bitmap buffer
/// must remain writable and exclusively available during the call.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_RenderPageBitmap(
    bitmap: *mut c_void,
    page: *mut c_void,
    start_x: c_int,
    start_y: c_int,
    size_x: c_int,
    size_y: c_int,
    rotate: c_int,
    flags: c_int,
) {
    #[cfg(feature = "render-profile")]
    let _profile = pdfoxium_core::render_profile::Session::new("page render other");
    if bitmap.is_null() || page.is_null() {
        return;
    }
    let (Some(right), Some(bottom)) = (start_x.checked_add(size_x), start_y.checked_add(size_y))
    else {
        return;
    };
    let page = unsafe { &*page.cast::<PageHandle>() };
    let Some(pdf) = page.pdf_page() else {
        return;
    };
    let matrix = pdf.display_matrix_for_rect(FxRect::new(start_x, start_y, right, bottom), rotate);
    let bitmap = unsafe { &mut *bitmap.cast::<BitmapHandle>() };
    unsafe {
        render_bitmap_with_matrix(
            bitmap,
            page,
            matrix,
            Some(FxRect::new(start_x, start_y, right, bottom)),
            flags,
        );
    }
}

/// # Safety
/// `bitmap` and `page` must be live handles, and any caller supplied bitmap
/// buffer must remain writable for the duration of the call.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_RenderPageBitmapWithMatrix(
    bitmap: *mut c_void,
    page: *mut c_void,
    matrix: *const FS_MATRIX,
    clipping: *const FS_RECTF,
    flags: c_int,
) {
    if bitmap.is_null() || page.is_null() {
        return;
    }
    let page = unsafe { &*page.cast::<PageHandle>() };
    let Some(pdf) = page.pdf_page() else {
        return;
    };
    let bitmap = unsafe { &mut *bitmap.cast::<BitmapHandle>() };
    let transform = if matrix.is_null() {
        pdf.display_matrix()
    } else {
        let matrix = unsafe { &*matrix };
        pdf.display_matrix()
            * pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix::new(
                matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
            )
    };
    let clip = if clipping.is_null() {
        Some(FxRect::default())
    } else {
        let clip = unsafe { &*clipping };
        Some(CfxFloatRect::new(clip.left, clip.bottom, clip.right, clip.top).to_fx_rect())
    };
    unsafe {
        render_bitmap_with_matrix(bitmap, page, transform, clip, flags);
    }
}

unsafe fn render_bitmap_with_matrix(
    bitmap: &mut BitmapHandle,
    page: &PageHandle,
    matrix: pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix,
    clip: Option<FxRect>,
    flags: c_int,
) {
    let Some(pdf) = page.pdf_page() else {
        return;
    };
    let options = pdfoxium::engine::render_options(&page._document.borrow(), flags, None);
    let rgb_byte_order = flags & 0x10 != 0;
    let mut storage = bitmap.storage.borrow_mut();
    match &mut *storage {
        BitmapStorage::Owned(bitmap) => {
            render_page_objects_with_clip(pdf, bitmap, matrix, &options, rgb_byte_order, clip);
        }
        BitmapStorage::External {
            width,
            height,
            format,
            pitch,
            buffer,
        } => {
            let size = *pitch as usize * *height as usize;
            // The registered external buffer remains exclusive for this FFI
            // call. The safe view never outlives this slice or takes ownership
            // of the caller's allocation.
            let external = unsafe { std::slice::from_raw_parts_mut(buffer.as_ptr(), size) };
            let Some(mut view) =
                CfxDibitmapData::from_buffer(*width, *height, *format, *pitch, external)
            else {
                return;
            };
            render_page_objects_with_clip(pdf, &mut view, matrix, &options, rgb_byte_order, clip);
            if let Some(owned) = view.into_owned_if_allocated() {
                *storage = BitmapStorage::Owned(owned);
            }
        }
    }
}

// Generated placeholders are contained in this FFI boundary. The generator
// derives exact signatures from the pinned upstream public headers.
#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code,
    unused_variables,
    unused_imports
)]
mod unimplemented_abi {
    include!(concat!(env!("OUT_DIR"), "/pdfium_stubs.rs"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdfoxium_core::fpdfapi::parser::cpdf_array::CpdfArray;
    use pdfoxium_core::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use pdfoxium_core::fpdfapi::parser::cpdf_object::retain_object;
    use pdfoxium_core::fpdfapi::parser::cpdf_reference::CpdfReference;
    use pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use std::ffi::CString;

    #[test]
    fn library_config_v1_paths_empty_paths_and_repeated_initialization() {
        use pdfoxium_core::fxge::cfx_fontmgr::with_font_manager;

        #[repr(C)]
        struct ConfigV1 {
            version: c_int,
            paths: *const *const c_char,
        }
        let path = CString::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem"
        ))
        .unwrap();
        let paths = [path.as_ptr(), std::ptr::null()];
        let config = ConfigV1 {
            version: 1,
            paths: paths.as_ptr(),
        };
        FPDF_InitLibraryWithConfig(std::ptr::from_ref(&config).cast());
        assert_eq!(
            with_font_manager(|manager| manager
                .builtin_mapper_mut()
                .match_installed_fonts(b"ahem")),
            b"Ahem"
        );
        FPDF_DestroyLibrary();
        FPDF_DestroyLibrary();

        let empty = [std::ptr::null()];
        let config = ConfigV1 {
            version: 1,
            paths: empty.as_ptr(),
        };
        FPDF_InitLibraryWithConfig(std::ptr::from_ref(&config).cast());
        // Reinitialization must not replace an explicit empty path list.
        let replacement = ConfigV1 {
            version: 1,
            paths: paths.as_ptr(),
        };
        FPDF_InitLibraryWithConfig(std::ptr::from_ref(&replacement).cast());
        assert!(with_font_manager(|manager| manager
            .builtin_mapper_mut()
            .match_installed_fonts(b"ahem"))
        .is_empty());
        FPDF_DestroyLibrary();
    }

    const PDF: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
    ));

    const RECTANGLES_PDF: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/rectangles.pdf"
    ));

    #[test]
    fn safe_api_render_matches_c_abi_with_rotation_annotations_and_padded_targets() {
        use pdfoxium::{Document, PixelFormat, PixelsMut, RenderConfig, Rotation};

        let annotated = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/annotation_highlight_square_with_ap.pdf"
        ));
        FPDF_InitLibrary();
        for bytes in [RECTANGLES_PDF, annotated.as_slice()] {
            let document = Document::from_bytes(bytes.to_vec()).unwrap();
            let page = document.page(0).unwrap();
            // SAFETY: the source bytes, page and external pixel storage remain
            // live through all calls, and each C handle is closed below.
            unsafe {
                let c_document =
                    FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null());
                assert!(!c_document.is_null());
                let c_page = FPDF_LoadPage(c_document, 0);
                assert!(!c_page.is_null());
                for (rotation, c_rotation) in [
                    (Rotation::None, 0),
                    (Rotation::Cw90, 1),
                    (Rotation::Cw180, 2),
                    (Rotation::Cw270, 3),
                ] {
                    for annotations in [false, true] {
                        for grayscale in [false, true] {
                            for format in [PixelFormat::Bgra8, PixelFormat::Rgba8] {
                                let config = RenderConfig::dpi(18.0)
                                    .rotation(rotation)
                                    .annotations(annotations)
                                    .grayscale(grayscale)
                                    .format(format);
                                let expected = page.render(&config).unwrap();
                                let width = expected.width();
                                let height = expected.height();
                                let stride = width as usize * 4 + 12;
                                let mut c_pixels = vec![0x5a; stride * height as usize];
                                let mut rust_pixels = c_pixels.clone();
                                let bitmap = FPDFBitmap_CreateEx(
                                    width as i32,
                                    height as i32,
                                    4,
                                    c_pixels.as_mut_ptr().cast(),
                                    stride as i32,
                                );
                                assert!(!bitmap.is_null());
                                FPDFBitmap_FillRect(
                                    bitmap,
                                    0,
                                    0,
                                    width as i32,
                                    height as i32,
                                    0xffffffff,
                                );
                                let flags = i32::from(annotations)
                                    | if grayscale { 0x08 } else { 0 }
                                    | if format == PixelFormat::Rgba8 {
                                        0x10
                                    } else {
                                        0
                                    };
                                FPDF_RenderPageBitmap(
                                    bitmap,
                                    c_page,
                                    0,
                                    0,
                                    width as i32,
                                    height as i32,
                                    c_rotation,
                                    flags,
                                );
                                page.render_into(
                                    &mut PixelsMut::new(
                                        &mut rust_pixels,
                                        width,
                                        height,
                                        stride,
                                        format,
                                    )
                                    .unwrap(),
                                    &config,
                                )
                                .unwrap();
                                assert_eq!(rust_pixels, c_pixels);
                                for (padded, packed) in c_pixels
                                    .chunks_exact(stride)
                                    .zip(expected.pixels().chunks_exact(expected.stride()))
                                {
                                    assert_eq!(&padded[..width as usize * 4], packed);
                                    assert!(padded[width as usize * 4..]
                                        .iter()
                                        .all(|&b| b == 0x5a));
                                }
                                assert!(expected
                                    .pixels()
                                    .chunks_exact(4)
                                    .any(|pixel| pixel != [255; 4]));
                                FPDFBitmap_Destroy(bitmap);
                            }
                        }
                    }
                }
                FPDF_ClosePage(c_page);
                FPDF_CloseDocument(c_document);
            }
            // The annotation switch must change visible pixels, not merely
            // produce equivalent output through two broken render paths.
            if bytes.as_ptr() == annotated.as_ptr() {
                let without = page
                    .render(&RenderConfig::dpi(18.0).annotations(false))
                    .unwrap();
                let with = page
                    .render(&RenderConfig::dpi(18.0).annotations(true))
                    .unwrap();
                assert_ne!(without.pixels(), with.pixels());
            }
        }
        FPDF_DestroyLibrary();
    }

    #[test]
    fn page_import_entrypoints_preserve_null_empty_negative_and_all_pages_contracts() {
        unsafe {
            let load = || {
                FPDF_LoadMemDocument64(SAVE_PDF.as_ptr().cast(), SAVE_PDF.len(), std::ptr::null())
            };
            let src = load();
            let dest = load();
            assert!(!src.is_null() && !dest.is_null());
            assert_eq!(
                FPDF_ImportPagesByIndex(std::ptr::null_mut(), src, std::ptr::null(), 0, 0),
                0
            );
            assert_eq!(
                FPDF_ImportPages(dest, std::ptr::null_mut(), std::ptr::null(), 0),
                0
            );
            assert_eq!(FPDF_CopyViewerPreferences(std::ptr::null_mut(), src), 0);
            assert_eq!(FPDF_CopyViewerPreferences(dest, std::ptr::null_mut()), 0);
            // Native rejects non-null/zero length before reading the pointer.
            assert_eq!(
                FPDF_ImportPagesByIndex(dest, src, std::ptr::dangling(), 0, 0),
                0
            );
            assert_eq!(FPDF_ImportPagesByIndex(dest, src, [-1].as_ptr(), 1, 0), 0);
            assert_eq!(FPDF_GetPageCount(dest), 1);
            assert_eq!(FPDF_ImportPagesByIndex(dest, src, [0, 0].as_ptr(), 2, 1), 1);
            assert_eq!(FPDF_GetPageCount(dest), 3);
            // Null imports all pages, irrespective of the unused length.
            assert_eq!(
                FPDF_ImportPagesByIndex(dest, src, std::ptr::null(), c_ulong::MAX, 0),
                1
            );
            assert_eq!(FPDF_GetPageCount(dest), 4);
            assert_eq!(FPDF_ImportPages(dest, src, std::ptr::null(), 0), 1);
            assert_eq!(FPDF_GetPageCount(dest), 5);
            assert_eq!(FPDF_CopyViewerPreferences(dest, src), 0);
            let mut writer = SaveWriter::new();
            assert_eq!(FPDF_SaveAsCopy(dest, &mut writer.interface, 0), 1);
            FPDF_CloseDocument(src);
            FPDF_CloseDocument(dest);
            let reopened = FPDF_LoadMemDocument64(
                writer.bytes.as_ptr().cast(),
                writer.bytes.len(),
                std::ptr::null(),
            );
            assert!(!reopened.is_null());
            assert_eq!(FPDF_GetPageCount(reopened), 5);
            FPDF_CloseDocument(reopened);
        }
    }

    #[test]
    fn page_import_bad_indices_leave_native_partial_mutations_and_allow_same_document() {
        unsafe {
            let load = || {
                FPDF_LoadMemDocument64(SAVE_PDF.as_ptr().cast(), SAVE_PDF.len(), std::ptr::null())
            };
            let src = load();
            let dest = load();
            assert_eq!(
                FPDF_ImportPagesByIndex(dest, src, [0, 99].as_ptr(), 2, 0),
                0
            );
            assert_eq!(FPDF_GetPageCount(dest), 3);
            assert_eq!(FPDF_ImportPages(dest, src, c"1,".as_ptr(), 0), 0);
            assert_eq!(FPDF_GetPageCount(dest), 3);
            assert_eq!(FPDF_ImportPages(dest, src, c"1".as_ptr(), -1), 0);
            assert_eq!(FPDF_GetPageCount(dest), 3);
            assert_eq!(FPDF_ImportPagesByIndex(src, src, [0].as_ptr(), 1, 1), 1);
            assert_eq!(FPDF_GetPageCount(src), 2);
            FPDF_CloseDocument(src);
            FPDF_CloseDocument(dest);
        }
    }

    #[test]
    fn nup_and_xobject_bad_parameters_preserve_upstream_contracts() {
        unsafe {
            let src = FPDF_LoadMemDocument64(
                RECTANGLES_PDF.as_ptr().cast(),
                RECTANGLES_PDF.len(),
                std::ptr::null(),
            );
            assert!(!src.is_null());
            let multi = include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../upstream/pdfium/testing/resources/rectangles_multi_pages.pdf"
            ));
            let nup_src =
                FPDF_LoadMemDocument64(multi.as_ptr().cast(), multi.len(), std::ptr::null());
            let dest = FPDF_CreateNewDocument();
            for (w, h, x, y) in [
                (612., 792., 0, 3),
                (612., 792., 2, 0),
                (0., 792., 2, 1),
                (612., 0., 7, 1),
                (612., 792., usize::MAX, 2),
                (f32::NAN, 792., 2, 1),
                (612., f32::NAN, 2, 1),
                (f32::INFINITY, 792., 2, 1),
                (612., f32::NEG_INFINITY, 2, 1),
            ] {
                assert!(FPDF_ImportNPagesToOne(nup_src, w, h, x, y).is_null());
            }
            assert!(FPDF_ImportNPagesToOne(std::ptr::null_mut(), 612., 792., 2, 1).is_null());
            assert!(FPDF_ImportNPagesToOne(dest, 612., 792., 2, 1).is_null());
            for (a, b) in [
                (std::ptr::null_mut(), std::ptr::null_mut()),
                (src, std::ptr::null_mut()),
                (std::ptr::null_mut(), src),
            ] {
                for i in [-1, 0, 1] {
                    assert!(FPDF_NewXObjectFromPage(a, b, i).is_null());
                }
            }
            for i in [-1, 1] {
                assert!(FPDF_NewXObjectFromPage(dest, src, i).is_null());
            }
            FPDF_CloseXObject(std::ptr::null_mut());
            assert!(FPDF_NewFormObjectFromXObject(std::ptr::null_mut()).is_null());
            assert_eq!(
                FPDFPage_InsertObject(std::ptr::null_mut(), std::ptr::null_mut()),
                0
            );
            assert_eq!(FPDFPageObj_GetType(std::ptr::null_mut()), 0);
            assert_eq!(
                FPDFPageObj_SetMatrix(std::ptr::null_mut(), std::ptr::dangling()),
                0
            );
            assert_eq!(FPDFPage_CountObjects(std::ptr::null_mut()), -1);
            assert_eq!(FPDFFormObj_CountObjects(std::ptr::null_mut()), -1);
            assert!(FPDFPage_GetObject(std::ptr::null_mut(), 0).is_null());
            FPDFPageObj_Destroy(std::ptr::null_mut());
            FPDF_CloseDocument(dest);
            FPDF_CloseDocument(src);
            FPDF_CloseDocument(nup_src);
        }
    }
    #[test]
    fn xobject_ownership_aliases_and_shared_streams_survive_source_context_and_save_reload() {
        unsafe {
            let src = FPDF_LoadMemDocument64(
                RECTANGLES_PDF.as_ptr().cast(),
                RECTANGLES_PDF.len(),
                std::ptr::null(),
            );
            let dest = FPDF_CreateNewDocument();
            let xobject = FPDF_NewXObjectFromPage(dest, src, 0);
            assert!(!xobject.is_null());
            let weak_dest = Rc::downgrade(&(*dest.cast::<DocumentHandle>()).document);
            FPDF_CloseDocument(src);
            let mut streams = Vec::new();
            for i in 0..2 {
                let page = FPDFPage_New(dest, if i == 0 { -10 } else { i32::MAX }, 612., 792.);
                assert!(!page.is_null());
                assert_eq!(FPDFPage_CountObjects(page), 0);
                let object = FPDF_NewFormObjectFromXObject(xobject);
                assert_eq!(FPDFPageObj_GetType(object), 5);
                assert_eq!(FPDFFormObj_CountObjects(object), 8);
                streams.push(with_page_object(object,|o| Rc::clone(o.as_any().downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_formobject::CpdfFormObject>().unwrap().form().stream())).unwrap());
                assert_eq!(FPDFPage_InsertObject(page, object), 1);
                assert_eq!(FPDFPage_GetObject(page, 0), object);
                assert_eq!(FPDFPage_GetObject(page, 0), object);
                assert!(FPDFPage_GetObject(page, -1).is_null());
                assert!(FPDFPage_GetObject(page, 1).is_null());
                assert_eq!(FPDFPage_CountObjects(page), 1);
                let (mut l, mut b, mut r, mut t) = (0., 0., 0., 0.);
                assert_eq!(
                    FPDFPageObj_GetBounds(object, &mut l, &mut b, &mut r, &mut t),
                    1
                );
                assert_eq!((l, b, r, t), (-1., -1., 201., 301.));
                assert_eq!(FPDFPageObj_SetMatrix(object, std::ptr::null()), 0);
                let m = FS_MATRIX {
                    a: 0.5,
                    b: 0.,
                    c: 0.,
                    d: 0.5,
                    e: 10.,
                    f: 20.,
                };
                assert_eq!(FPDFPageObj_SetMatrix(object, &m), 1);
                assert_eq!(
                    FPDFPageObj_GetBounds(object, &mut l, &mut b, &mut r, &mut t),
                    1
                );
                assert_eq!((l, b, r, t), (9.5, 19.5, 110.5, 170.5));
                assert_eq!(FPDFPage_GenerateContent(page), 1);
                FPDF_ClosePage(page);
            }
            assert!(Rc::ptr_eq(&streams[0], &streams[1]));
            // Forms already inserted remain valid after closing the template.
            let object = FPDF_NewFormObjectFromXObject(xobject);
            FPDFPageObj_Destroy(object);
            let rejected = FPDF_NewFormObjectFromXObject(xobject);
            assert_eq!(FPDFPage_InsertObject(std::ptr::null_mut(), rejected), 0);
            FPDF_CloseXObject(xobject);
            let mut writer = SaveWriter::new();
            assert_eq!(FPDF_SaveAsCopy(dest, &mut writer.interface, 0), 1);
            FPDF_CloseDocument(dest);
            assert!(weak_dest.upgrade().is_none());
            let reopened = FPDF_LoadMemDocument64(
                writer.bytes.as_ptr().cast(),
                writer.bytes.len(),
                std::ptr::null(),
            );
            assert_eq!(FPDF_GetPageCount(reopened), 2);
            let mut reloaded_streams = Vec::new();
            for i in 0..2 {
                let page = FPDF_LoadPage(reopened, i);
                assert_eq!(FPDFPage_CountObjects(page), 1);
                let object = FPDFPage_GetObject(page, 0);
                assert_eq!(FPDFPageObj_GetType(object), 5);
                assert_eq!(FPDFFormObj_CountObjects(object), 8);
                reloaded_streams.push(with_page_object(object, |o| Rc::clone(o.as_any().downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_formobject::CpdfFormObject>().unwrap().form().stream())).unwrap());
                FPDF_ClosePage(page);
            }
            assert!(Rc::ptr_eq(&reloaded_streams[0], &reloaded_streams[1]));
            FPDF_CloseDocument(reopened);
        }
    }
    #[test]
    fn document_creation_honors_global_machine_time_policy_and_page_clamping() {
        unsafe {
            FPDF_SetSandBoxPolicy(0, 0);
            FPDF_SetSandBoxPolicy(99, 1);
            let document = FPDF_CreateNewDocument();
            let info = (*document.cast::<DocumentHandle>())
                .document
                .borrow_mut()
                .info()
                .unwrap();
            assert!(!info
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .key_exist(b"CreationDate"));
            FPDF_SetSandBoxPolicy(0, 1);
            let enabled = FPDF_CreateNewDocument();
            let info = (*enabled.cast::<DocumentHandle>())
                .document
                .borrow_mut()
                .info()
                .unwrap();
            let date = info
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_byte_string_for(b"CreationDate", b"");
            assert!(date.starts_with(b"D:"));
            assert_eq!(date.len(), 23);
            assert!(matches!(date[16], b'+' | b'-'));
            assert_eq!(date[19], b'\'');
            assert_eq!(date[22], b'\'');
            assert!(date[17..19]
                .iter()
                .chain(&date[20..22])
                .all(u8::is_ascii_digit));
            assert!(FPDFPage_New(std::ptr::null_mut(), 0, 10., 20.).is_null());
            for index in [-1, i32::MAX, 1] {
                let page = FPDFPage_New(document, index, 123.5, 234.25);
                assert!(!page.is_null());
                assert_eq!(FPDF_GetPageWidthF(page), 123.5);
                FPDF_ClosePage(page);
            }
            assert_eq!(FPDF_GetPageCount(document), 3);
            FPDF_CloseDocument(document);
            FPDF_CloseDocument(enabled);
        }
    }

    const SAVE_PDF: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/hello_world.pdf"
    ));

    #[repr(C)]
    struct SaveWriter {
        interface: FPDF_FILEWRITE,
        bytes: Vec<u8>,
        calls: usize,
        fail_after: Option<usize>,
        query_document: *mut c_void,
        page_counts: Vec<c_int>,
    }

    impl SaveWriter {
        fn new() -> Self {
            Self {
                interface: FPDF_FILEWRITE {
                    version: 1,
                    WriteBlock: Some(Self::write_block),
                },
                bytes: Vec::new(),
                calls: 0,
                fail_after: None,
                query_document: std::ptr::null_mut(),
                page_counts: Vec::new(),
            }
        }

        unsafe extern "C" fn write_block(
            interface: *mut FPDF_FILEWRITE,
            data: *const c_void,
            size: c_ulong,
        ) -> c_int {
            let writer = unsafe { &mut *interface.cast::<Self>() };
            writer.calls += 1;
            if !writer.query_document.is_null() {
                writer
                    .page_counts
                    .push(unsafe { FPDF_GetPageCount(writer.query_document) });
            }
            if writer.fail_after.is_some_and(|count| writer.calls > count) {
                return 0;
            }
            if size != 0 {
                let bytes = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) };
                writer.bytes.extend_from_slice(bytes);
            }
            // Native treats any nonzero callback result as success.
            7
        }
    }

    #[test]
    fn save_adapter_calls_empty_blocks_and_ignores_descriptor_version() {
        let mut writer = SaveWriter::new();
        writer.interface.version = 77;
        let mut adapter =
            unsafe { CpdfSdkFileWriteAdapter::new(std::ptr::addr_of_mut!(writer.interface)) };
        assert!(adapter.write_block(&[]));
        assert_eq!(writer.calls, 1);
        assert!(writer.bytes.is_empty());
        assert!(adapter.write_block(b"saved"));
        assert_eq!(writer.bytes, b"saved");
        writer.fail_after = Some(2);
        assert!(!adapter.write_block(b"failed"));
        assert_eq!(writer.calls, 3);
        assert_eq!(writer.bytes, b"saved");
    }

    #[test]
    fn save_entry_points_preserve_versions_flag_normalization_and_null_contract() {
        unsafe {
            assert_eq!(
                FPDF_SaveAsCopy(std::ptr::null_mut(), std::ptr::null_mut(), 0),
                0
            );
            assert_eq!(
                FPDF_SaveWithVersion(std::ptr::null_mut(), std::ptr::null_mut(), 0, 14),
                0
            );
            let document =
                FPDF_LoadMemDocument64(SAVE_PDF.as_ptr().cast(), SAVE_PDF.len(), std::ptr::null());
            assert!(!document.is_null());
            for flags in [0, 2, 3, 4, 999_999] {
                for version in [10, 14, 17, -1, 0, 18] {
                    let mut writer = SaveWriter::new();
                    assert_eq!(
                        FPDF_SaveWithVersion(
                            document,
                            std::ptr::addr_of_mut!(writer.interface),
                            flags,
                            version,
                        ),
                        1,
                        "flags={flags}, version={version}"
                    );
                    let expected_version = if (10..=17).contains(&version) {
                        version
                    } else {
                        17
                    };
                    let header = format!("%PDF-1.{}\r\n", expected_version % 10);
                    assert!(writer.bytes.starts_with(header.as_bytes()));
                    let reloaded = FPDF_LoadMemDocument64(
                        writer.bytes.as_ptr().cast(),
                        writer.bytes.len(),
                        std::ptr::null(),
                    );
                    assert!(!reloaded.is_null());
                    assert_eq!(FPDF_GetPageCount(reloaded), 1);
                    let mut actual_version = -1;
                    assert_eq!(FPDF_GetFileVersion(reloaded, &mut actual_version), 1);
                    assert_eq!(actual_version, expected_version);
                    FPDF_CloseDocument(reloaded);
                }
            }

            // On platforms with 64-bit unsigned long, the public flags are
            // explicitly narrowed before the creator sees them.
            if c_ulong::BITS > 32 {
                for (low_flags, incremental) in [(0, false), (1, true)] {
                    let mut writer = SaveWriter::new();
                    let flags = ((1u64 << 32) | low_flags) as c_ulong;
                    assert_eq!(
                        FPDF_SaveAsCopy(document, std::ptr::addr_of_mut!(writer.interface), flags,),
                        1
                    );
                    if incremental {
                        assert!(writer.bytes.starts_with(SAVE_PDF));
                    } else {
                        assert!(writer.bytes.starts_with(b"%PDF-1.7\r\n"));
                    }
                }
            }
            FPDF_CloseDocument(document);
        }
    }

    #[test]
    fn save_callback_failure_preserves_checked_flush_and_ignored_final_flush() {
        unsafe {
            let document =
                FPDF_LoadMemDocument64(SAVE_PDF.as_ptr().cast(), SAVE_PDF.len(), std::ptr::null());
            assert!(!document.is_null());
            let mut writer = SaveWriter::new();
            writer.fail_after = Some(0);
            assert_eq!(
                FPDF_SaveAsCopy(document, std::ptr::addr_of_mut!(writer.interface), 0),
                1
            );
            // The small document stays buffered until FileBufferArchive's
            // destructor, whose final Flush() result native deliberately ignores.
            assert_eq!(writer.calls, 1);
            assert!(writer.bytes.is_empty());

            {
                let document = (*document.cast::<DocumentHandle>()).document.borrow();
                document
                    .holder()
                    .borrow_mut()
                    .new_indirect_string(&vec![b'X'; 70_000]);
            }
            let mut writer = SaveWriter::new();
            writer.fail_after = Some(0);
            assert_eq!(
                FPDF_SaveAsCopy(document, std::ptr::addr_of_mut!(writer.interface), 0),
                0
            );
            // This object forces an ordinary checked 32 KiB buffered flush.
            assert_eq!(writer.calls, 1);
            assert!(writer.bytes.is_empty());
            FPDF_CloseDocument(document);
        }
    }

    #[test]
    fn save_writer_can_reenter_document_queries_during_full_and_final_flushes() {
        unsafe {
            let document =
                FPDF_LoadMemDocument64(SAVE_PDF.as_ptr().cast(), SAVE_PDF.len(), std::ptr::null());
            assert!(!document.is_null());
            {
                let doc = (*document.cast::<DocumentHandle>()).document.borrow();
                let holder = doc.holder();
                let large = holder.borrow_mut().new_indirect_string(&vec![b'R'; 70_000]);
                let number = large.borrow().obj_num();
                doc.root()
                    .unwrap()
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<CpdfDictionary>()
                    .unwrap()
                    .set_new_for(b"CallbackSaveData", CpdfReference::new(&holder, number));
            }
            let mut writer = SaveWriter::new();
            writer.query_document = document;
            assert_eq!(
                FPDF_SaveAsCopy(document, std::ptr::addr_of_mut!(writer.interface), 0),
                1
            );
            assert!(writer.calls >= 3);
            assert_eq!(writer.page_counts, vec![1; writer.calls]);
            let reloaded = FPDF_LoadMemDocument64(
                writer.bytes.as_ptr().cast(),
                writer.bytes.len(),
                std::ptr::null(),
            );
            assert!(!reloaded.is_null());
            assert_eq!(FPDF_GetPageCount(reloaded), 1);
            FPDF_CloseDocument(reloaded);
            FPDF_CloseDocument(document);
        }
    }

    #[test]
    fn save_reload_preserves_generated_path_edits_in_full_and_incremental_output() {
        unsafe {
            let document = FPDF_LoadMemDocument64(
                RECTANGLES_PDF.as_ptr().cast(),
                RECTANGLES_PDF.len(),
                std::ptr::null(),
            );
            assert!(!document.is_null());
            let page = FPDF_LoadPage(document, 0);
            assert!(!page.is_null());
            (*page.cast::<PageHandle>())
                .page_mut()
                .unwrap()
                .mutable_holder()
                .page_object_by_index_mut(0)
                .unwrap()
                .base_mut()
                .set_is_active(false);
            assert_eq!(FPDFPage_GenerateContent(page), 1);
            for flags in [0, 1] {
                let mut writer = SaveWriter::new();
                assert_eq!(
                    FPDF_SaveWithVersion(
                        document,
                        std::ptr::addr_of_mut!(writer.interface),
                        flags,
                        14,
                    ),
                    1
                );
                if flags == 1 {
                    // Incremental creation retains the source bytes and source
                    // header even when an output version was requested.
                    assert!(writer.bytes.starts_with(RECTANGLES_PDF));
                } else {
                    assert!(writer.bytes.starts_with(b"%PDF-1.4\r\n"));
                }
                let reloaded = FPDF_LoadMemDocument64(
                    writer.bytes.as_ptr().cast(),
                    writer.bytes.len(),
                    std::ptr::null(),
                );
                assert!(!reloaded.is_null());
                let saved_page = FPDF_LoadPage(reloaded, 0);
                assert!(!saved_page.is_null());
                assert_eq!(
                    (*saved_page.cast::<PageHandle>())
                        .page
                        .as_ref()
                        .unwrap()
                        .holder()
                        .page_object_count(),
                    7
                );
                FPDF_ClosePage(saved_page);
                FPDF_CloseDocument(reloaded);
            }
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
        }
    }

    fn generation_contents(
        page: &CpdfPage,
        direct: bool,
    ) -> Option<pdfoxium_core::fpdfapi::parser::cpdf_object::ObjectRef> {
        let object = page.dict();
        let borrowed = object.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        if direct {
            dict.get_direct_object_for(b"Contents")
        } else {
            dict.get_object_for(b"Contents")
        }
    }

    #[test]
    fn generate_content_null_and_clean_page_preserve_contents_identity() {
        unsafe {
            assert_eq!(FPDFPage_GenerateContent(std::ptr::null_mut()), 0);
            let document = FPDF_LoadMemDocument64(
                RECTANGLES_PDF.as_ptr().cast(),
                RECTANGLES_PDF.len(),
                std::ptr::null(),
            );
            assert!(!document.is_null());
            let page = FPDF_LoadPage(document, 0);
            assert!(!page.is_null());
            let contents =
                generation_contents(&(*page.cast::<PageHandle>()).page.as_ref().unwrap(), false)
                    .unwrap();
            let stream =
                generation_contents(&(*page.cast::<PageHandle>()).page.as_ref().unwrap(), true)
                    .unwrap();
            assert_eq!(FPDFPage_GenerateContent(page), 1);
            let after =
                generation_contents(&(*page.cast::<PageHandle>()).page.as_ref().unwrap(), false)
                    .unwrap();
            assert!(Rc::ptr_eq(&contents, &after));
            let after_stream =
                generation_contents(&(*page.cast::<PageHandle>()).page.as_ref().unwrap(), true)
                    .unwrap();
            assert!(Rc::ptr_eq(&stream, &after_stream));
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
        }
    }

    #[test]
    fn generate_content_rewrites_deactivated_paths_after_document_handle_closes() {
        use pdfoxium_core::fpdfapi::parser::cpdf_stream_acc::CpdfRetainedStreamAcc;

        unsafe {
            let document = FPDF_LoadMemDocument64(
                RECTANGLES_PDF.as_ptr().cast(),
                RECTANGLES_PDF.len(),
                std::ptr::null(),
            );
            assert!(!document.is_null());
            let page = FPDF_LoadPage(document, 0);
            assert!(!page.is_null());
            let document_lifetime = Rc::downgrade(&(*page.cast::<PageHandle>())._document);
            let source =
                generation_contents(&(*page.cast::<PageHandle>()).page.as_ref().unwrap(), true)
                    .unwrap();
            let before = CpdfRetainedStreamAcc::load_filtered(source)
                .unwrap()
                .get_span()
                .to_vec();
            FPDF_CloseDocument(document);
            assert!(document_lifetime.upgrade().is_some());
            {
                let handle = &mut *page.cast::<PageHandle>();
                assert_eq!(
                    handle.page.as_ref().unwrap().holder().page_object_count(),
                    8
                );
                let mut engine_page = handle.page_mut().unwrap();
                let object = engine_page
                    .mutable_holder()
                    .page_object_by_index_mut(0)
                    .unwrap();
                object.base_mut().set_is_active(false);
            }

            assert_eq!(FPDFPage_GenerateContent(page), 1);
            {
                let handle = &*page.cast::<PageHandle>();
                assert_eq!(
                    handle
                        .page
                        .as_ref()
                        .unwrap()
                        .holder()
                        .active_page_object_count(),
                    7
                );
                let stream = generation_contents(&handle.page.as_ref().unwrap(), true).unwrap();
                let after = CpdfRetainedStreamAcc::load_filtered(stream)
                    .unwrap()
                    .get_span()
                    .to_vec();
                assert_ne!(before, after);
                let removed_rect = b"\n0 290 10 10 re";
                assert!(before
                    .windows(removed_rect.len())
                    .any(|bytes| bytes == removed_rect));
                assert!(!after
                    .windows(removed_rect.len())
                    .any(|bytes| bytes == removed_rect));
                // Parse the actual serialized Contents again, independently
                // from the retained in-memory page objects used to generate it.
                let mut reparsed =
                    CpdfPage::from_document(&mut handle._document.borrow_mut(), 0).unwrap();
                reparsed.parse_path_content();
                assert_eq!(reparsed.holder().page_object_count(), 7);
            }
            FPDF_ClosePage(page);
            assert!(document_lifetime.upgrade().is_none());
        }
    }

    #[test]
    fn generate_content_rejects_invalidated_page_dict_without_consuming_edits() {
        use pdfoxium_core::fpdfapi::parser::cpdf_stream_acc::CpdfRetainedStreamAcc;

        unsafe {
            let document = FPDF_LoadMemDocument64(
                RECTANGLES_PDF.as_ptr().cast(),
                RECTANGLES_PDF.len(),
                std::ptr::null(),
            );
            assert!(!document.is_null());
            let page = FPDF_LoadPage(document, 0);
            assert!(!page.is_null());
            let (contents, before) = {
                let handle = &mut *page.cast::<PageHandle>();
                handle
                    .page_mut()
                    .unwrap()
                    .mutable_holder()
                    .page_object_by_index_mut(0)
                    .unwrap()
                    .base_mut()
                    .set_is_active(false);
                let contents = generation_contents(&handle.page.as_ref().unwrap(), false).unwrap();
                let stream = generation_contents(&handle.page.as_ref().unwrap(), true).unwrap();
                let bytes = CpdfRetainedStreamAcc::load_filtered(stream)
                    .unwrap()
                    .get_span()
                    .to_vec();
                handle
                    .page
                    .as_ref()
                    .unwrap()
                    .dict()
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<CpdfDictionary>()
                    .unwrap()
                    .set_new_name_for(b"Type", b"Pages");
                (contents, bytes)
            };

            assert_eq!(FPDFPage_GenerateContent(page), 0);
            {
                let handle = &*page.cast::<PageHandle>();
                let after_contents =
                    generation_contents(&handle.page.as_ref().unwrap(), false).unwrap();
                assert!(Rc::ptr_eq(&contents, &after_contents));
                let stream = generation_contents(&handle.page.as_ref().unwrap(), true).unwrap();
                let after = CpdfRetainedStreamAcc::load_filtered(stream).unwrap();
                assert_eq!(&*after.get_span(), before.as_slice());
                assert!(handle
                    .page
                    .as_ref()
                    .unwrap()
                    .holder()
                    .page_object_by_index(0)
                    .unwrap()
                    .base()
                    .is_dirty());
            }
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
        }
    }

    #[test]
    fn image_memory_extensions_lifecycle_and_thread_isolation() {
        unsafe {
            FPDF_DestroyLibrary();
            let mut limit = 123;
            assert_eq!(PDFOXIUM_SetImageMemoryLimits(0), 0);
            assert_eq!(PDFOXIUM_GetImageMemoryLimits(&mut limit), 0);
            assert_eq!(limit, 123);
            FPDF_InitLibrary();
            assert_eq!(PDFOXIUM_GetImageMemoryLimits(std::ptr::null_mut()), 0);
            assert_eq!(PDFOXIUM_GetImageMemoryLimits(&mut limit), 1);
            assert_eq!(limit, 60_000_000);
            for value in [0, 123_456, u64::MAX] {
                assert_eq!(PDFOXIUM_SetImageMemoryLimits(value), 1);
                assert_eq!(PDFOXIUM_GetImageMemoryLimits(&mut limit), 1);
                assert_eq!(limit, value);
            }
            std::thread::spawn(|| {
                assert_eq!(PDFOXIUM_SetImageMemoryLimits(0), 0);
                FPDF_InitLibrary();
                let mut other = 0;
                assert_eq!(PDFOXIUM_GetImageMemoryLimits(&mut other), 1);
                assert_eq!(other, 60_000_000);
                assert_eq!(PDFOXIUM_SetImageMemoryLimits(0), 1);
                FPDF_DestroyLibrary();
            })
            .join()
            .unwrap();
            assert_eq!(PDFOXIUM_GetImageMemoryLimits(&mut limit), 1);
            assert_eq!(limit, u64::MAX);
            FPDF_DestroyLibrary();
            FPDF_InitLibrary();
            assert_eq!(PDFOXIUM_GetImageMemoryLimits(&mut limit), 1);
            assert_eq!(limit, 60_000_000);
            FPDF_DestroyLibrary();
        }
    }

    #[test]
    fn font_memory_extensions_lifecycle_runtime_limits_trim_and_pixels() {
        unsafe {
            FPDF_DestroyLibrary();
            let (mut cache, mut scratch) = (17, 19);
            assert_eq!(PDFOXIUM_SetFontMemoryLimits(0, 0), 0);
            assert_eq!(PDFOXIUM_TrimFontMemory(), 0);
            assert_eq!(PDFOXIUM_GetFontMemoryLimits(&mut cache, &mut scratch), 0);
            assert_eq!(PDFOXIUM_GetFontMemoryUsage(&mut cache, &mut scratch), 0);
            assert_eq!((cache, scratch), (17, 19));
            FPDF_InitLibrary();
            assert_eq!(PDFOXIUM_GetFontMemoryLimits(&mut cache, &mut scratch), 1);
            assert_eq!((cache, scratch), (64 * 1024 * 1024, 4 * 1024 * 1024));
            cache = 17;
            assert_eq!(
                PDFOXIUM_GetFontMemoryLimits(&mut cache, std::ptr::null_mut()),
                0
            );
            assert_eq!(
                PDFOXIUM_GetFontMemoryUsage(&mut cache, std::ptr::null_mut()),
                0
            );
            assert_eq!(cache, 17);
            let document = FPDF_LoadMemDocument64(PDF.as_ptr().cast(), PDF.len(), std::ptr::null());
            assert!(!document.is_null());
            let page = FPDF_LoadPage(document, 0);
            assert!(!page.is_null());
            let render = |size: i32, flags| {
                let bitmap = FPDFBitmap_Create(size, size, 1);
                FPDFBitmap_FillRect(bitmap, 0, 0, size, size, 0xffffffff);
                FPDF_RenderPageBitmap(bitmap, page, 0, 0, size, size, 0, flags);
                let pixels = std::slice::from_raw_parts(
                    FPDFBitmap_GetBuffer(bitmap).cast::<u8>(),
                    (size * size * 4) as usize,
                )
                .to_vec();
                FPDFBitmap_Destroy(bitmap);
                pixels
            };
            let original = render(600, 0);
            assert_eq!(PDFOXIUM_GetFontMemoryUsage(&mut cache, &mut scratch), 1);
            assert!(cache > 0 && scratch > 0);
            assert_eq!(PDFOXIUM_SetFontMemoryLimits(0, 0), 1);
            assert_eq!(PDFOXIUM_GetFontMemoryUsage(&mut cache, &mut scratch), 1);
            assert_eq!((cache, scratch), (0, 0));
            assert_eq!(original, render(600, 0));
            // Existing PDFium flag retains its independent image-cache policy.
            assert_eq!(original, render(600, 0x200));
            assert_eq!(PDFOXIUM_GetFontMemoryUsage(&mut cache, &mut scratch), 1);
            assert_eq!((cache, scratch), (0, 0));
            assert_eq!(PDFOXIUM_SetFontMemoryLimits(1024 * 1024, 128 * 1024), 1);
            for size in [200, 600, 300, 600, 200] {
                render(size, 0);
                assert_eq!(PDFOXIUM_GetFontMemoryUsage(&mut cache, &mut scratch), 1);
                assert!(cache <= 1024 * 1024 && scratch <= 128 * 1024);
            }
            assert_eq!(PDFOXIUM_TrimFontMemory(), 1);
            assert_eq!(PDFOXIUM_GetFontMemoryUsage(&mut cache, &mut scratch), 1);
            assert_eq!((cache, scratch), (0, 0));
            assert_eq!(PDFOXIUM_GetFontMemoryLimits(&mut cache, &mut scratch), 1);
            assert_eq!((cache, scratch), (1024 * 1024, 128 * 1024));
            assert_eq!(original, render(600, 0));
            assert_eq!(PDFOXIUM_SetFontMemoryLimits(u64::MAX, u64::MAX), 1);
            assert_eq!(PDFOXIUM_GetFontMemoryLimits(&mut cache, &mut scratch), 1);
            assert_eq!((cache, scratch), (u64::MAX, u64::MAX));
            std::thread::spawn(|| {
                assert_eq!(PDFOXIUM_SetFontMemoryLimits(0, 0), 0);
                FPDF_InitLibrary();
                let (mut a, mut b) = (0, 0);
                assert_eq!(PDFOXIUM_GetFontMemoryLimits(&mut a, &mut b), 1);
                assert_eq!((a, b), (64 * 1024 * 1024, 4 * 1024 * 1024));
                assert_eq!(PDFOXIUM_SetFontMemoryLimits(0, 0), 1);
                FPDF_DestroyLibrary();
            })
            .join()
            .unwrap();
            assert_eq!(PDFOXIUM_GetFontMemoryLimits(&mut cache, &mut scratch), 1);
            assert_eq!((cache, scratch), (u64::MAX, u64::MAX));
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
            FPDF_DestroyLibrary();
            FPDF_InitLibrary();
            assert_eq!(PDFOXIUM_GetFontMemoryLimits(&mut cache, &mut scratch), 1);
            assert_eq!((cache, scratch), (64 * 1024 * 1024, 4 * 1024 * 1024));
            FPDF_DestroyLibrary();
        }
    }

    #[test]
    fn bitmap_render_honors_optional_content_visibility() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/bug_40162073.pdf"
        ));
        let document =
            unsafe { FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null()) };
        assert!(!document.is_null());
        let page = unsafe { FPDF_LoadPage(document, 0) };
        assert!(!page.is_null());
        let bitmap = FPDFBitmap_Create(200, 200, 1);
        unsafe { FPDF_RenderPageBitmap(bitmap, page, 0, 0, 200, 200, 0, 0) };
        let pixels = unsafe {
            std::slice::from_raw_parts(FPDFBitmap_GetBuffer(bitmap).cast::<u8>(), 200 * 200 * 4)
        };
        let at = |y: usize| &pixels[(y * 200 + 100) * 4..(y * 200 + 100) * 4 + 4];
        assert_eq!(at(50), &[0, 0, 0, 0]);
        assert_eq!(at(100), &[0, 255, 0, 255]);
        assert_eq!(at(150), &[255, 0, 0, 255]);
        unsafe {
            FPDFBitmap_Destroy(bitmap);
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
        }
    }

    #[test]
    fn page_render_draws_parsed_path_to_owned_and_external_bitmaps() {
        let objects = CpdfIndirectObjectHolder::new();
        let stream = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"1 0 0 rg 10 10 20 20 re f");
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Type", b"Page");
        let mut media_box = CpdfArray::new();
        for edge in [0, 0, 100, 100] {
            media_box.append_new(CpdfScalarObject::number_from_signed(edge));
        }
        dict.set_new_for(b"MediaBox", media_box);
        dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        let mut appearance_dict = CpdfDictionary::new();
        appearance_dict.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
        let appearance = retain_object(
            pdfoxium_core::fpdfapi::parser::cpdf_stream::CpdfStream::from_data_and_dict(
                b"0 0 1 rg 0 0 10 10 re f".to_vec(),
                retain_object(appearance_dict),
            ),
        );
        objects.borrow_mut().add_indirect_object(appearance.clone());
        let mut ap = CpdfDictionary::new();
        ap.set_new_for(
            b"N",
            CpdfReference::new(&objects, appearance.borrow().obj_num()),
        );
        let mut annot = CpdfDictionary::new();
        annot.set_rect_for(b"Rect", CfxFloatRect::new(60.0, 60.0, 80.0, 80.0));
        annot.set_new_for(b"AP", ap);
        let mut annots = CpdfArray::new();
        annots.append_new(annot);
        dict.set_new_for(b"Annots", annots);
        let mut page = CpdfPage::new(retain_object(dict)).unwrap();
        page.parse_path_content();
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        let handle = Box::into_raw(Box::new(PageHandle {
            sdk_page: pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage::new(&document, &page),
            object_handles: Default::default(),
            page: Some(Rc::new(page)),
            #[cfg(feature = "xfa")]
            xfa_page: None,
            progressive: Default::default(),
            _document: document,
            attachment_handles: Default::default(),
            borrowed_objects: Default::default(),
            link_list: Default::default(),
        }))
        .cast();
        let owned = FPDFBitmap_Create(30, 30, 1);
        unsafe { FPDF_RenderPageBitmap(owned, handle, 4, 5, 20, 20, 0, 0) };
        {
            let pixels = unsafe {
                std::slice::from_raw_parts(FPDFBitmap_GetBuffer(owned).cast::<u8>(), 30 * 30 * 4)
            };
            assert_eq!(
                &pixels[(20 * 30 + 7) * 4..(20 * 30 + 7) * 4 + 4],
                &[0, 0, 255, 255]
            );
            assert_eq!(
                &pixels[(20 * 30 + 3) * 4..(20 * 30 + 3) * 4 + 4],
                &[0, 0, 0, 0]
            );
            assert_eq!(&pixels[(11 * 30 + 18) * 4..(11 * 30 + 18) * 4 + 4], &[0; 4]);
        }
        unsafe { FPDF_RenderPageBitmap(owned, handle, 4, 5, 20, 20, 0, 0x01) };
        let pixels = unsafe {
            std::slice::from_raw_parts(FPDFBitmap_GetBuffer(owned).cast::<u8>(), 30 * 30 * 4)
        };
        assert_eq!(
            &pixels[(11 * 30 + 18) * 4..(11 * 30 + 18) * 4 + 4],
            &[255, 0, 0, 255]
        );
        unsafe { FPDFBitmap_Destroy(owned) };
        let mut external = vec![0u8; 30 * 30 * 4];
        let bitmap = unsafe { FPDFBitmap_CreateEx(30, 30, 4, external.as_mut_ptr().cast(), 120) };
        unsafe { FPDF_RenderPageBitmap(bitmap, handle, 4, 5, 20, 20, 0, 0) };
        assert_eq!(
            &external[(20 * 30 + 7) * 4..(20 * 30 + 7) * 4 + 4],
            &[0, 0, 255, 255]
        );
        let matrix = FS_MATRIX {
            a: 0.2,
            b: 0.0,
            c: 0.0,
            d: 0.2,
            e: 4.0,
            f: 5.0,
        };
        let clipping = FS_RECTF {
            left: 4.0,
            top: 25.0,
            right: 24.0,
            bottom: 5.0,
        };
        let with_matrix = FPDFBitmap_Create(30, 30, 1);
        unsafe { FPDF_RenderPageBitmapWithMatrix(with_matrix, handle, &matrix, &clipping, 0) };
        let pixels = unsafe {
            std::slice::from_raw_parts(FPDFBitmap_GetBuffer(with_matrix).cast::<u8>(), 30 * 30 * 4)
        };
        assert_eq!(pixels, external);
        unsafe { FPDFBitmap_Destroy(with_matrix) };
        let fractional_clip = FS_RECTF {
            left: 4.9,
            top: 25.9,
            right: 7.9,
            bottom: 5.9,
        };
        let clipped = FPDFBitmap_Create(30, 30, 1);
        unsafe { FPDF_RenderPageBitmapWithMatrix(clipped, handle, &matrix, &fractional_clip, 0) };
        let pixels = unsafe {
            std::slice::from_raw_parts(FPDFBitmap_GetBuffer(clipped).cast::<u8>(), 30 * 30 * 4)
        };
        assert_eq!(
            &pixels[(20 * 30 + 6) * 4..(20 * 30 + 6) * 4 + 4],
            &[0, 0, 255, 255]
        );
        assert_eq!(&pixels[(20 * 30 + 7) * 4..(20 * 30 + 7) * 4 + 4], &[0; 4]);
        unsafe { FPDFBitmap_Destroy(clipped) };
        let empty_clip = FPDFBitmap_Create(30, 30, 1);
        unsafe {
            FPDF_RenderPageBitmapWithMatrix(empty_clip, handle, &matrix, std::ptr::null(), 0)
        };
        let pixels = unsafe {
            std::slice::from_raw_parts(FPDFBitmap_GetBuffer(empty_clip).cast::<u8>(), 30 * 30 * 4)
        };
        assert!(pixels.iter().all(|&byte| byte == 0));
        unsafe { FPDFBitmap_Destroy(empty_clip) };
        unsafe {
            FPDFBitmap_Destroy(bitmap);
            FPDF_ClosePage(handle);
        }
    }

    #[test]
    fn renders_existing_two_page_fixture_through_c_abi() {
        let document =
            unsafe { FPDF_LoadMemDocument64(PDF.as_ptr().cast(), PDF.len(), std::ptr::null()) };
        let page = unsafe { FPDF_LoadPage(document, 0) };
        assert!(!page.is_null());
        let bitmap = FPDFBitmap_Create(200, 200, 1);
        unsafe { FPDF_RenderPageBitmap(bitmap, page, 0, 0, 200, 200, 0, 0) };
        let pixels = unsafe {
            std::slice::from_raw_parts(FPDFBitmap_GetBuffer(bitmap).cast::<u8>(), 200 * 200 * 4)
        };
        let drawn_x: Vec<_> = pixels
            .chunks_exact(4)
            .enumerate()
            .filter(|(_, pixel)| pixel[3] != 0)
            .map(|(index, _)| index % 200)
            .collect();
        assert!(drawn_x.iter().max().unwrap() - drawn_x.iter().min().unwrap() > 50);
        unsafe {
            FPDFBitmap_Destroy(bitmap);
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
        }
    }

    #[test]
    fn renders_existing_jbig2_fixture_through_c_abi() {
        let pdf = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/bug_631912.pdf"
        ));
        let document =
            unsafe { FPDF_LoadMemDocument64(pdf.as_ptr().cast(), pdf.len(), std::ptr::null()) };
        assert!(!document.is_null());
        let page = unsafe { FPDF_LoadPage(document, 0) };
        assert!(!page.is_null());
        let bitmap = FPDFBitmap_Create(72, 45, 1);
        unsafe { FPDF_RenderPageBitmap(bitmap, page, 0, 0, 72, 45, 0, 0) };
        let pixels = unsafe {
            std::slice::from_raw_parts(FPDFBitmap_GetBuffer(bitmap).cast::<u8>(), 72 * 45 * 4)
        };
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
        unsafe {
            FPDFBitmap_Destroy(bitmap);
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
        }
    }

    #[test]
    fn loads_and_closes_memory_document_through_c_abi() {
        FPDF_InitLibrary();
        let document =
            unsafe { FPDF_LoadMemDocument64(PDF.as_ptr().cast(), PDF.len(), std::ptr::null()) };
        assert!(!document.is_null());
        assert_eq!(unsafe { FPDF_GetPageCount(document) }, 2);
        let mut version = 0;
        assert_eq!(unsafe { FPDF_GetFileVersion(document, &mut version) }, 1);
        assert_eq!(version, 17);
        unsafe { FPDF_CloseDocument(document) };
        let document = unsafe {
            FPDF_LoadMemDocument(PDF.as_ptr().cast(), PDF.len() as c_int, std::ptr::null())
        };
        assert!(!document.is_null());
        unsafe { FPDF_CloseDocument(document) };
        FPDF_DestroyLibrary();
    }

    #[test]
    fn file_version_requires_loaded_input_even_after_created_document_is_saved() {
        // SAFETY: every handle remains live until closed below; the writer and
        // reloaded document's byte storage outlive all synchronous API calls.
        unsafe {
            let document = FPDF_CreateNewDocument();
            assert!(!document.is_null());
            let mut version = 123;
            assert_eq!(FPDF_GetFileVersion(document, &mut version), 0);
            assert_eq!(version, 0);
            assert_eq!(FPDF_GetFileVersion(document, std::ptr::null_mut()), 0);

            let page = FPDFPage_New(document, 0, 60.0, 40.0);
            assert!(!page.is_null());
            assert_eq!(FPDFPage_GenerateContent(page), 1);
            FPDF_ClosePage(page);
            let mut writer = SaveWriter::new();
            assert_eq!(
                FPDF_SaveWithVersion(document, std::ptr::addr_of_mut!(writer.interface), 2, 14),
                1
            );
            version = 456;
            assert_eq!(FPDF_GetFileVersion(document, &mut version), 0);
            assert_eq!(version, 0);

            let reloaded = FPDF_LoadMemDocument64(
                writer.bytes.as_ptr().cast(),
                writer.bytes.len(),
                std::ptr::null(),
            );
            assert!(!reloaded.is_null());
            assert_eq!(FPDF_GetFileVersion(reloaded, &mut version), 1);
            assert_eq!(version, 14);
            FPDF_CloseDocument(reloaded);
            FPDF_CloseDocument(document);
        }
    }

    #[test]
    fn file_load_and_parse_errors_match_pdfium_codes() {
        let path = CString::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/hello_world_2_pages.pdf"
        ))
        .unwrap();
        let document = unsafe { FPDF_LoadDocument(path.as_ptr(), std::ptr::null()) };
        assert!(!document.is_null());
        assert_eq!(unsafe { FPDF_GetPageCount(document) }, 2);
        unsafe { FPDF_CloseDocument(document) };
        let invalid = b"not a pdf";
        assert!(unsafe {
            FPDF_LoadMemDocument64(invalid.as_ptr().cast(), invalid.len(), std::ptr::null())
        }
        .is_null());
        assert_eq!(FPDF_GetLastError(), 3);
        std::thread::spawn(|| {
            assert!(unsafe { FPDF_LoadDocument(std::ptr::null(), std::ptr::null()) }.is_null());
        })
        .join()
        .unwrap();
        assert_eq!(FPDF_GetLastError(), 2);
        let document = unsafe { FPDF_LoadDocument(path.as_ptr(), std::ptr::null()) };
        assert!(!document.is_null());
        assert_eq!(FPDF_GetLastError(), 2);
        unsafe { FPDF_CloseDocument(document) };
        assert!(unsafe { FPDF_LoadMemDocument(std::ptr::null(), -1, std::ptr::null()) }.is_null());
        assert!(unsafe { FPDF_LoadCustomDocument(std::ptr::null(), std::ptr::null()) }.is_null());
        assert_eq!(FPDF_GetLastError(), 2);
        assert_eq!(unsafe { FPDF_GetPageCount(std::ptr::null_mut()) }, 0);
        let mut version = 123;
        assert_eq!(
            unsafe { FPDF_GetFileVersion(std::ptr::null_mut(), &mut version) },
            0
        );
        assert_eq!(version, 0);
    }

    #[test]
    fn page_handles_expose_geometry_and_keep_document_alive() {
        let document =
            unsafe { FPDF_LoadMemDocument64(PDF.as_ptr().cast(), PDF.len(), std::ptr::null()) };
        assert!(!document.is_null());
        assert!(unsafe { FPDF_LoadPage(document, -1) }.is_null());
        assert!(unsafe { FPDF_LoadPage(document, 2) }.is_null());
        let page = unsafe { FPDF_LoadPage(document, 0) };
        assert!(!page.is_null());
        assert_eq!(unsafe { FPDF_GetPageWidthF(page) }, 200.0);
        assert_eq!(unsafe { FPDF_GetPageHeight(page) }, 200.0);
        let mut bbox = FS_RECTF {
            left: -1.0,
            top: -1.0,
            right: -1.0,
            bottom: -1.0,
        };
        assert_eq!(unsafe { FPDF_GetPageBoundingBox(page, &mut bbox) }, 1);
        assert_eq!(
            (bbox.left, bbox.top, bbox.right, bbox.bottom),
            (0.0, 200.0, 200.0, 0.0)
        );
        let mut size = FS_SIZEF {
            width: 0.0,
            height: 0.0,
        };
        assert_eq!(
            unsafe { FPDF_GetPageSizeByIndexF(document, 1, &mut size) },
            1
        );
        assert_eq!((size.width, size.height), (200.0, 200.0));
        let mut width = 0.0;
        let mut height = 0.0;
        assert_eq!(
            unsafe { FPDF_GetPageSizeByIndex(document, 0, &mut width, &mut height) },
            1
        );
        assert_eq!((width, height), (200.0, 200.0));
        unsafe { FPDF_CloseDocument(document) };
        assert_eq!(unsafe { FPDF_GetPageWidth(page) }, 200.0);
        unsafe { FPDF_ClosePage(page) };
        unsafe { FPDF_ClosePage(std::ptr::null_mut()) };
    }

    #[test]
    fn custom_access_reads_document_after_descriptor_scope() {
        extern "C" fn read_block(
            param: *mut c_void,
            position: c_ulong,
            buffer: *mut u8,
            size: c_ulong,
        ) -> c_int {
            let bytes = unsafe { &*(param.cast::<Vec<u8>>()) };
            let offset = position as usize;
            let len = size as usize;
            let Some(block) = bytes.get(offset..offset + len) else {
                return 0;
            };
            unsafe { std::ptr::copy_nonoverlapping(block.as_ptr(), buffer, len) };
            1
        }

        let mut bytes = PDF.to_vec();
        let document = {
            let access = FPDF_FILEACCESS {
                m_FileLen: bytes.len() as c_ulong,
                m_GetBlock: Some(read_block),
                m_Param: (&mut bytes as *mut Vec<u8>).cast(),
            };
            unsafe { FPDF_LoadCustomDocument(&access, std::ptr::null()) }
        };
        assert!(!document.is_null());
        assert_eq!(unsafe { FPDF_GetPageCount(document) }, 2);
        let page = unsafe { FPDF_LoadPage(document, 1) };
        assert!(!page.is_null());
        unsafe { FPDF_ClosePage(page) };
        unsafe { FPDF_CloseDocument(document) };
        assert!(unsafe { FPDF_LoadCustomDocument(std::ptr::null(), std::ptr::null()) }.is_null());
    }

    #[test]
    fn document_security_queries_handle_plain_and_encrypted_files() {
        assert_eq!(
            unsafe { FPDF_DocumentHasValidCrossReferenceTable(std::ptr::null_mut()) },
            0
        );
        assert_eq!(unsafe { FPDF_GetDocPermissions(std::ptr::null_mut()) }, 0);
        assert_eq!(
            unsafe { FPDF_GetDocUserPermissions(std::ptr::null_mut()) },
            0
        );
        assert_eq!(
            unsafe { FPDF_GetSecurityHandlerRevision(std::ptr::null_mut()) },
            -1
        );

        let plain =
            unsafe { FPDF_LoadMemDocument64(PDF.as_ptr().cast(), PDF.len(), std::ptr::null()) };
        assert!(!plain.is_null());
        assert_eq!(
            unsafe { FPDF_DocumentHasValidCrossReferenceTable(plain) },
            1
        );
        assert_eq!(
            unsafe { FPDF_GetDocPermissions(plain) },
            u32::MAX as c_ulong
        );
        assert_eq!(
            unsafe { FPDF_GetDocUserPermissions(plain) },
            u32::MAX as c_ulong
        );
        assert_eq!(unsafe { FPDF_GetSecurityHandlerRevision(plain) }, -1);
        unsafe { FPDF_CloseDocument(plain) };

        let encrypted = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/encrypted_hello_world_r6.pdf"
        ));
        let password = CString::new("hôtel").unwrap();
        let document = unsafe {
            FPDF_LoadMemDocument64(
                encrypted.as_ptr().cast(),
                encrypted.len(),
                password.as_ptr(),
            )
        };
        assert!(!document.is_null());
        assert_eq!(
            unsafe { FPDF_DocumentHasValidCrossReferenceTable(document) },
            1
        );
        assert_eq!(unsafe { FPDF_GetDocUserPermissions(document) }, 0xffff_fffc);
        assert_eq!(unsafe { FPDF_GetSecurityHandlerRevision(document) }, 6);
        unsafe { FPDF_CloseDocument(document) };
    }

    #[test]
    fn bitmap_allocation_exposes_stable_bgra_buffer() {
        assert!(FPDFBitmap_Create(0, 1, 1).is_null());
        let bitmap = FPDFBitmap_Create(3, 2, 1);
        assert!(!bitmap.is_null());
        assert_eq!(unsafe { FPDFBitmap_GetFormat(bitmap) }, 4);
        assert_eq!(unsafe { FPDFBitmap_GetWidth(bitmap) }, 3);
        assert_eq!(unsafe { FPDFBitmap_GetHeight(bitmap) }, 2);
        assert_eq!(unsafe { FPDFBitmap_GetStride(bitmap) }, 12);
        let buffer = unsafe { FPDFBitmap_GetBuffer(bitmap) };
        assert!(!buffer.is_null());
        unsafe { *buffer.cast::<u8>().add(12) = 0x5a };
        assert_eq!(
            unsafe { *FPDFBitmap_GetBuffer(bitmap).cast::<u8>().add(12) },
            0x5a
        );
        unsafe { FPDFBitmap_Destroy(bitmap) };
        let opaque = FPDFBitmap_Create(1, 1, 0);
        assert_eq!(unsafe { FPDFBitmap_GetFormat(opaque) }, 3);
        unsafe { FPDFBitmap_Destroy(opaque) };
        assert_eq!(unsafe { FPDFBitmap_GetFormat(std::ptr::null_mut()) }, 0);
        unsafe { FPDFBitmap_Destroy(std::ptr::null_mut()) };
    }

    #[test]
    fn bitmap_create_ex_accepts_owned_and_caller_buffers() {
        let owned = unsafe { FPDFBitmap_CreateEx(3, 2, 2, std::ptr::null_mut(), 16) };
        assert!(!owned.is_null());
        assert_eq!(unsafe { FPDFBitmap_GetFormat(owned) }, 2);
        assert_eq!(unsafe { FPDFBitmap_GetStride(owned) }, 16);
        unsafe { FPDFBitmap_Destroy(owned) };

        let mut external = [0u8; 32];
        let buffer = external.as_mut_ptr();
        let bitmap = unsafe { FPDFBitmap_CreateEx(3, 2, 4, buffer.cast(), 16) };
        assert!(!bitmap.is_null());
        assert_eq!(unsafe { FPDFBitmap_GetBuffer(bitmap) }, buffer.cast());
        external[17] = 0x5a;
        assert_eq!(
            unsafe { *FPDFBitmap_GetBuffer(bitmap).cast::<u8>().add(17) },
            0x5a
        );
        unsafe { FPDFBitmap_Destroy(bitmap) };
        assert_eq!(external[17], 0x5a);
        assert!(unsafe { FPDFBitmap_CreateEx(3, 2, 4, buffer.cast(), 8) }.is_null());
        assert!(unsafe { FPDFBitmap_CreateEx(3, 2, 5, buffer.cast(), 16) }.is_null());
    }

    #[test]
    fn bitmap_fill_rect_clips_and_obeys_pixel_formats() {
        let bgra = FPDFBitmap_Create(2, 2, 1);
        assert_eq!(
            unsafe { FPDFBitmap_FillRect(bgra, -1, 0, 2, 1, 0x7f_12_34_56) },
            1
        );
        let pixels =
            unsafe { std::slice::from_raw_parts(FPDFBitmap_GetBuffer(bgra).cast::<u8>(), 16) };
        assert_eq!(&pixels[..8], &[0x56, 0x34, 0x12, 0x7f, 0, 0, 0, 0]);
        assert_eq!(
            unsafe { FPDFBitmap_FillRect(bgra, i32::MAX, 0, 1, 1, 0) },
            0
        );
        unsafe { FPDFBitmap_Destroy(bgra) };

        let bgrx = FPDFBitmap_Create(1, 1, 0);
        assert_eq!(
            unsafe { FPDFBitmap_FillRect(bgrx, 0, 0, 1, 1, 0x12_34_56_78) },
            1
        );
        let pixel =
            unsafe { std::slice::from_raw_parts(FPDFBitmap_GetBuffer(bgrx).cast::<u8>(), 4) };
        assert_eq!(pixel, &[0x78, 0x56, 0x34, 0xff]);
        unsafe { FPDFBitmap_Destroy(bgrx) };

        let gray = unsafe { FPDFBitmap_CreateEx(2, 1, 1, std::ptr::null_mut(), 0) };
        assert_eq!(
            unsafe { FPDFBitmap_FillRect(gray, 0, 0, 2, 1, 0x00_ff_00_00) },
            1
        );
        let pixels =
            unsafe { std::slice::from_raw_parts(FPDFBitmap_GetBuffer(gray).cast::<u8>(), 4) };
        assert_eq!(&pixels[..2], &[76, 76]);
        unsafe { FPDFBitmap_Destroy(gray) };

        let mut external = [0xaau8; 8];
        let bgr = unsafe { FPDFBitmap_CreateEx(2, 1, 2, external.as_mut_ptr().cast(), 8) };
        assert_eq!(
            unsafe { FPDFBitmap_FillRect(bgr, 1, 0, 1, 1, 0x00_01_02_03) },
            1
        );
        assert_eq!(external, [0xaa, 0xaa, 0xaa, 3, 2, 1, 0xaa, 0xaa]);
        unsafe { FPDFBitmap_Destroy(bgr) };
        assert_eq!(
            unsafe { FPDFBitmap_FillRect(std::ptr::null_mut(), 0, 0, 1, 1, 0) },
            0
        );
    }

    #[test]
    fn bitmap_fill_uniform_and_pattern_rows_preserve_guards_padding_and_alpha() {
        for (format, bpp) in [
            (FxdibFormat::EightBppRgb, 1),
            (FxdibFormat::Bgr, 3),
            (FxdibFormat::Bgrx, 4),
            (FxdibFormat::Bgra, 4),
        ] {
            for padding in [0, 3] {
                let pitch = 3 * bpp + padding;
                for (color, gray, bgr, alpha) in [
                    (0xffff_ffff, 255, [255, 255, 255], 255),
                    (0x0000_0000, 0, [0, 0, 0], 0),
                    (0x7f7f_7f7f, 127, [127, 127, 127], 127),
                    (0x1234_5678, 79, [120, 86, 52], 18),
                ] {
                    for rect in [
                        (0, 0, 3, 3),
                        (0, 1, 3, 1),
                        (-1, -1, 3, 3),
                        (1, 0, 1, 3),
                        (0, 0, 0, 1),
                    ] {
                        let mut storage = vec![0xaa; 1 + 3 * pitch + 1];
                        let mut expected = storage.clone();
                        let (left, top, width, height) = rect;
                        for y in 0..3 {
                            for x in 0..3 {
                                if x < left || x >= left + width || y < top || y >= top + height {
                                    continue;
                                }
                                let start = 1 + y as usize * pitch + x as usize * bpp;
                                if bpp == 1 {
                                    expected[start] = gray;
                                } else {
                                    expected[start..start + 3].copy_from_slice(&bgr);
                                    if bpp == 4 {
                                        expected[start + 3] = if format == FxdibFormat::Bgrx {
                                            255
                                        } else {
                                            alpha
                                        };
                                    }
                                }
                            }
                        }
                        assert!(fill_bitmap_rect(
                            &mut storage[1..1 + 3 * pitch],
                            (3, 3),
                            pitch as u32,
                            format,
                            rect,
                            color
                        ));
                        assert_eq!(
                            storage, expected,
                            "{format:?} padding={padding} color={color:#x} rect={rect:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn page_device_coordinates_round_trip() {
        let document =
            unsafe { FPDF_LoadMemDocument64(PDF.as_ptr().cast(), PDF.len(), std::ptr::null()) };
        let page = unsafe { FPDF_LoadPage(document, 0) };
        assert!(!page.is_null());
        let mut device_x = 0;
        let mut device_y = 0;
        assert_eq!(
            unsafe {
                FPDF_PageToDevice(
                    page,
                    0,
                    0,
                    400,
                    400,
                    0,
                    50.0,
                    75.0,
                    &mut device_x,
                    &mut device_y,
                )
            },
            1
        );
        assert_eq!((device_x, device_y), (100, 250));
        let mut page_x = 0.0;
        let mut page_y = 0.0;
        assert_eq!(
            unsafe {
                FPDF_DeviceToPage(
                    page,
                    0,
                    0,
                    400,
                    400,
                    0,
                    device_x,
                    device_y,
                    &mut page_x,
                    &mut page_y,
                )
            },
            1
        );
        assert_eq!((page_x, page_y), (50.0, 75.0));
        assert_eq!(
            unsafe {
                FPDF_PageToDevice(
                    page,
                    i32::MAX,
                    0,
                    1,
                    400,
                    0,
                    0.0,
                    0.0,
                    &mut device_x,
                    &mut device_y,
                )
            },
            0
        );
        unsafe { FPDF_ClosePage(page) };
        unsafe { FPDF_CloseDocument(document) };
    }
}

mod dataavail;
mod ext;
pub use dataavail::*;
pub use ext::*;

mod progressive;
pub use progressive::*;

mod sysfontinfo;
pub use sysfontinfo::*;

mod view_queries;
pub use view_queries::*;

#[cfg(all(test, not(feature = "xfa")))]
mod feature_tests;
