// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cpdfxfa_context.{h,cpp}`. The PDF extension owns the FF graph; document,
//! environment, page and app-provider backedges are weak. The XFA realm uses
//! the SDK's existing JavaScript VM and host clock.
use super::{cpdfxfa_docenvironment::CpdfXfaDocEnvironment, cpdfxfa_page::CpdfXfaPage};
use crate::fpdfsdk::cpdfsdk_pageview::SdkPage;
use crate::{
    fpdfapi::parser::{
        cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument,
        cpdf_object::ObjectRef, cpdf_scalar_object::ObjectType,
        cpdf_seekablemultistream::CpdfSeekableMultiStream,
    },
    fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment,
    fxcrt::{
        cfx_timer::TimerHandler,
        fx_stream::{SeekableReadStream, WriteStream},
        fx_string::utf16_decode,
        xml::{cfx_xmldocument::CfxXmlDocument, cfx_xmlparser::CfxXmlParser},
    },
    xfa::fxfa::{
        cxfa_eventparam::{CxfaEventParam, XfaEventType},
        cxfa_ffapp::{CxfaFfApp, XfaAppProvider},
        cxfa_ffdoc::CxfaFfDoc,
        cxfa_ffdocview::CxfaFfDocView,
        cxfa_readynodeiterator::CxfaReadyNodeIterator,
        fxfa::FormType,
        fxfa_basic::XfaHashCode,
        layout::cxfa_viewlayoutitem::CxfaViewLayoutItem,
    },
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadStatus {
    Preload,
    Loading,
    Loaded,
    Closing,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XfaLoadError {
    Detached,
    JavaScriptUnavailable,
    MissingXfa,
    InvalidXml,
    OpenDocument,
    Theme,
    Layout(String),
    Reentrant,
    EnvironmentChanged,
}
pub struct CpdfXfaContext {
    pdf: Weak<RefCell<CpdfDocument>>,
    env: RefCell<Weak<CpdfSdkFormFillEnvironment>>,
    xml: RefCell<Option<Rc<RefCell<CfxXmlDocument>>>>,
    app: RefCell<Option<Rc<CxfaFfApp>>>,
    doc: RefCell<Option<Rc<CxfaFfDoc>>>,
    view: RefCell<Option<Rc<CxfaFfDocView>>>,
    pages: RefCell<Vec<Option<Rc<CpdfXfaPage>>>>,
    sdk_pages: RefCell<HashMap<usize, Rc<SdkPage>>>,
    form_type: Cell<FormType>,
    status: Cell<LoadStatus>,
    page_count: Cell<i32>,
    generation: Cell<u64>,
}
impl CpdfXfaContext {
    pub fn new(pdf: &Rc<RefCell<CpdfDocument>>) -> Rc<Self> {
        Rc::new(Self {
            pdf: Rc::downgrade(pdf),
            env: RefCell::new(Weak::new()),
            xml: RefCell::new(None),
            app: RefCell::new(None),
            doc: RefCell::new(None),
            view: RefCell::new(None),
            pages: RefCell::new(Vec::new()),
            sdk_pages: RefCell::new(HashMap::new()),
            form_type: Cell::new(FormType::kNone),
            status: Cell::new(LoadStatus::Preload),
            page_count: Cell::new(0),
            generation: Cell::new(0),
        })
    }
    pub fn pdf_document(&self) -> Option<Rc<RefCell<CpdfDocument>>> {
        self.pdf.upgrade()
    }
    pub fn form_fill_env(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
        self.env.borrow().upgrade().filter(|env| env.is_alive())
    }
    pub fn xfa_doc(&self) -> Option<Rc<CxfaFfDoc>> {
        self.doc.borrow().clone()
    }
    pub fn doc_view(&self) -> Option<Rc<CxfaFfDocView>> {
        self.view.borrow().clone()
    }
    pub fn xml_document(&self) -> Option<Rc<RefCell<CfxXmlDocument>>> {
        self.xml.borrow().clone()
    }
    pub fn form_type(&self) -> FormType {
        self.form_type.get()
    }
    pub fn load_status(&self) -> LoadStatus {
        self.status.get()
    }
    pub fn contains_extension_form(&self) -> bool {
        matches!(
            self.form_type(),
            FormType::kXFAFull | FormType::kXFAForeground
        )
    }
    pub fn contains_extension_full_form(&self) -> bool {
        self.form_type() == FormType::kXFAFull
    }
    pub fn contains_extension_foreground_form(&self) -> bool {
        self.form_type() == FormType::kXFAForeground
    }
    fn clear_graph(&self) {
        let doc = self.doc.borrow_mut().take();
        let sdk_pages = std::mem::take(&mut *self.sdk_pages.borrow_mut());
        let view = self.view.borrow_mut().take();
        let app = self.app.borrow_mut().take();
        self.pages.borrow_mut().clear();
        self.xml.borrow_mut().take();
        // Explicit prefinalization releases the realm and callback roots while
        // the outgoing SDK runtime and environment can still be reached.
        if let Some(doc) = doc {
            doc.pre_finalize();
        }
        for page in sdk_pages.into_values() {
            page.invalidate();
        }
        drop(view);
        drop(app);
    }
    pub fn set_form_fill_env(self: &Rc<Self>, env: Option<&Rc<CpdfSdkFormFillEnvironment>>) {
        // SDK shutdown can reenter while FF docClose scripts are running. A
        // closing extension cannot be revived by that outgoing environment.
        if self.status.get() == LoadStatus::Closing {
            if env.is_none() {
                *self.env.borrow_mut() = Weak::new();
            }
            return;
        }
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.clear_graph();
        if self.generation.get() != generation {
            return;
        }
        let previous = self.env.borrow().upgrade();
        if let Some(previous) = previous {
            if previous
                .xfa_context()
                .is_some_and(|ctx| Rc::ptr_eq(&ctx, self))
            {
                previous.set_xfa_context(None);
            }
        }
        *self.env.borrow_mut() = env.map_or_else(Weak::new, Rc::downgrade);
        if let Some(env) = env {
            env.set_xfa_context(Some(self));
        }
        self.form_type.set(FormType::kNone);
        self.status.set(LoadStatus::Preload);
    }
    pub fn close(&self) {
        if self.status.replace(LoadStatus::Closing) == LoadStatus::Closing {
            return;
        }
        self.generation.set(self.generation.get().wrapping_add(1));
        self.clear_graph();
        let env = self.env.borrow_mut().upgrade();
        *self.env.borrow_mut() = Weak::new();
        if let Some(env) = env {
            env.on_detach_from_document();
        }
    }
    pub fn load_xfa_doc(self: &Rc<Self>) -> Result<(), XfaLoadError> {
        if self.status.get() == LoadStatus::Loading {
            return Err(XfaLoadError::Reentrant);
        }
        if self.status.get() == LoadStatus::Closing {
            return Err(XfaLoadError::Detached);
        }
        if self.form_fill_env().is_none() {
            return Err(XfaLoadError::Detached);
        }
        self.status.set(LoadStatus::Loading);
        let generation = self.generation.get();
        self.clear_graph();
        self.check_generation(generation)?;
        let result = self.load_impl(generation);
        if result.is_err() && self.generation.get() == generation {
            self.clear_graph();
            if self.generation.get() == generation {
                self.form_type.set(FormType::kNone);
                self.status.set(LoadStatus::Preload);
            }
        }
        result
    }
    fn check_generation(&self, generation: u64) -> Result<(), XfaLoadError> {
        if self.generation.get() != generation || self.form_fill_env().is_none() {
            Err(XfaLoadError::EnvironmentChanged)
        } else {
            Ok(())
        }
    }
    fn load_impl(self: &Rc<Self>, generation: u64) -> Result<(), XfaLoadError> {
        let env = self.form_fill_env().ok_or(XfaLoadError::Detached)?;
        // Native XFA requires an actual CJS runtime; a disabled JS platform
        // must not silently acquire an independent or enabled XFA engine.
        if !env.is_javascript_enabled() {
            return Err(XfaLoadError::JavaScriptUnavailable);
        }
        let runtime = env
            .get_or_create_runtime()
            .map_err(|_| XfaLoadError::JavaScriptUnavailable)?;
        self.check_generation(generation)?;
        let pdf = self.pdf_document().ok_or(XfaLoadError::Detached)?;
        let stream = create_xfa_multistream(&pdf.borrow()).ok_or(XfaLoadError::MissingXfa)?;
        let xml = Rc::new(RefCell::new(
            CfxXmlParser::new(stream)
                .parse()
                .ok_or(XfaLoadError::InvalidXml)?,
        ));
        let provider = Rc::new(AppProvider {
            context: Rc::downgrade(self),
        });
        let app = CxfaFfApp::new(provider);
        let environment = Rc::new(CpdfXfaDocEnvironment::new(Rc::downgrade(self)));
        let doc = CxfaFfDoc::new_with_runtime(
            app.clone(),
            environment,
            &pdf.borrow(),
            runtime.engine().clone(),
        )
        .map_err(|_| XfaLoadError::JavaScriptUnavailable)?;
        *self.xml.borrow_mut() = Some(xml.clone());
        *self.app.borrow_mut() = Some(app.clone());
        *self.doc.borrow_mut() = Some(doc.clone());
        if !doc.open_doc(xml) {
            return Err(XfaLoadError::OpenDocument);
        }
        self.check_generation(generation)?;
        if !app.load_fwl_theme(&doc) {
            return Err(XfaLoadError::Theme);
        }
        self.form_type.set(doc.form_type());
        let view = doc.create_doc_view();
        *self.view.borrow_mut() = Some(view.clone());
        let started = view
            .start_layout()
            .map_err(|e| XfaLoadError::Layout(format!("{e:?}")))?;
        if started < 0 {
            return Err(XfaLoadError::Layout(
                "StartLayout rejected the document".into(),
            ));
        }
        self.check_generation(generation)?;
        view.do_layout()
            .map_err(|e| XfaLoadError::Layout(format!("{e:?}")))?;
        self.check_generation(generation)?;
        view.stop_layout()
            .map_err(|e| XfaLoadError::Layout(format!("{e:?}")))?;
        self.check_generation(generation)?;
        self.status.set(LoadStatus::Loaded);
        Ok(())
    }
    pub fn get_page_count(&self) -> i32 {
        if self.contains_extension_full_form() {
            self.doc_view().map_or(0, |v| v.count_page_views())
        } else {
            self.pdf_document().map_or(0, |doc| {
                i32::try_from(doc.borrow().page_count()).unwrap_or(i32::MAX)
            })
        }
    }
    pub fn original_page_count(&self) -> i32 {
        self.page_count.get()
    }
    pub fn set_original_page_count(&self, count: i32) {
        let count = count.max(0);
        self.page_count.set(count);
        self.pages.borrow_mut().resize_with(count as usize, || None);
    }
    pub fn get_or_create_xfa_page(self: &Rc<Self>, index: i32) -> Option<Rc<CpdfXfaPage>> {
        if index < 0 || index >= self.get_page_count() {
            return None;
        }
        if let Some(page) = self.get_xfa_page(index) {
            return Some(page);
        }
        self.set_original_page_count(self.get_page_count());
        let page = CpdfXfaPage::new(Rc::downgrade(self), index);
        if !page.load_page() {
            return None;
        }
        self.pages
            .borrow_mut()
            .get_mut(index as usize)?
            .replace(page.clone());
        Some(page)
    }
    pub fn get_xfa_page(&self, index: i32) -> Option<Rc<CpdfXfaPage>> {
        self.pages
            .borrow()
            .get(usize::try_from(index).ok()?)?
            .clone()
    }
    pub fn get_xfa_page_for_ffview(&self, view: &CxfaViewLayoutItem) -> Option<Rc<CpdfXfaPage>> {
        if self.xfa_doc().is_none() || !self.contains_extension_full_form() {
            return None;
        }
        self.pages
            .borrow()
            .iter()
            .flatten()
            .find(|page| page.get_xfa_page_view().as_ref() == Some(view))
            .cloned()
    }
    pub fn sdk_page(&self, page: &Rc<CpdfXfaPage>) -> Option<Rc<SdkPage>> {
        let key = Rc::as_ptr(page) as usize;
        if let Some(cached) = self
            .sdk_pages
            .borrow()
            .get(&key)
            .filter(|page| page.is_alive())
        {
            return Some(cached.clone());
        }
        let sdk = SdkPage::new_xfa(&self.pdf_document()?, page.clone());
        self.sdk_pages.borrow_mut().insert(key, sdk.clone());
        Some(sdk)
    }
    pub fn delete_page(&self, index: i32) -> u32 {
        let number = self
            .pdf_document()
            .map_or(0, |pdf| pdf.borrow_mut().delete_page(index));
        if number == 0 {
            return 0;
        }
        self.page_count.set(self.page_count.get().saturating_sub(1));
        let removed = {
            let mut pages = self.pages.borrow_mut();
            if let Ok(index) = usize::try_from(index) {
                if index < pages.len() {
                    let removed = pages.remove(index);
                    for (i, page) in pages.iter().enumerate().skip(index) {
                        if let Some(page) = page {
                            page.set_xfa_page_view_index(i as i32);
                        }
                    }
                    removed
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(page) = removed {
            let sdk = self
                .sdk_pages
                .borrow_mut()
                .remove(&(Rc::as_ptr(&page) as usize));
            if let Some(sdk) = sdk {
                sdk.invalidate();
            }
        }
        number
    }
    pub fn pages_inserted(&self, index: i32, count: usize) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let mut pages = self.pages.borrow_mut();
        // Include appending at the cache boundary, and reindex retained pages.
        if index > pages.len()
            || count > i32::MAX as usize
            || pages
                .len()
                .checked_add(count)
                .is_none_or(|n| n > i32::MAX as usize)
        {
            return;
        }
        pages.splice(index..index, std::iter::repeat_with(|| None).take(count));
        for (i, page) in pages.iter().enumerate().skip(index + count) {
            if let Some(page) = page {
                page.set_xfa_page_view_index(i as i32);
            }
        }
        self.page_count.set(pages.len() as i32);
    }
    pub fn clear_change_mark(&self) {
        if let Some(env) = self.form_fill_env() {
            env.clear_change_mark();
        }
    }
    pub fn save_datasets_package(&self, stream: &mut dyn WriteStream) -> bool {
        self.save_package(stream, XfaHashCode::Datasets)
    }
    pub fn save_form_package(&self, stream: &mut dyn WriteStream) -> bool {
        self.save_package(stream, XfaHashCode::Form)
    }
    fn save_package(&self, stream: &mut dyn WriteStream, code: XfaHashCode) -> bool {
        let Some(doc) = self.xfa_doc().filter(|_| self.doc_view().is_some()) else {
            return false;
        };
        let node = doc.model_document().get_xfa_node(code as u32);
        doc.save_package(node.as_ref(), stream)
    }
    fn send_save_event(&self, kind: XfaEventType) {
        if !self.contains_extension_form() {
            return;
        }
        let Some(view) = self.doc_view() else {
            return;
        };
        let mut iterator = CxfaReadyNodeIterator::new(view.root_subform());
        while let Some(current) = iterator.move_to_next() {
            if current.is_alive() && current.is_widget_ready() {
                let mut event = CxfaEventParam::new(kind);
                event.targeted = false;
                view.process_event(&current, &mut event);
            }
            if self
                .doc_view()
                .as_ref()
                .is_none_or(|active| !Rc::ptr_eq(active, &view))
            {
                return;
            }
        }
        let _ = view.update_doc_view();
    }
    pub fn send_pre_save_to_xfa_doc(&self) {
        self.send_save_event(XfaEventType::PreSave);
    }
    pub fn send_post_save_to_xfa_doc(&self) {
        // Native does not clear ordinary PDF changes merely because form-fill
        // setup created an extension which has no loaded XFA document/view.
        if !self.contains_extension_form() || self.doc_view().is_none() {
            return;
        }
        self.send_save_event(XfaEventType::PostSave);
        self.clear_change_mark();
    }
}
impl Drop for CpdfXfaContext {
    fn drop(&mut self) {
        self.close();
    }
}
pub(crate) fn xfa_object(pdf: &CpdfDocument) -> Option<ObjectRef> {
    let root = pdf.root()?;
    let acro = root
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_dict_for(b"AcroForm")?;
    let object = acro
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_direct_object_for(b"XFA")?;
    Some(object)
}
pub fn create_xfa_multistream(pdf: &CpdfDocument) -> Option<Rc<CpdfSeekableMultiStream>> {
    let object = xfa_object(pdf)?;
    let streams = {
        let object_ref = object.borrow();
        match object_ref.object_type() {
            ObjectType::Stream => vec![object.clone()],
            ObjectType::Array => {
                let array = object_ref.as_any().downcast_ref::<CpdfArray>()?;
                (0..array.size() / 2)
                    .filter_map(|i| array.get_stream_at(i * 2 + 1))
                    .collect()
            }
            _ => return None,
        }
    };
    (!streams.is_empty()).then(|| Rc::new(CpdfSeekableMultiStream::new(streams)))
}
struct AppProvider {
    context: Weak<CpdfXfaContext>,
}
impl AppProvider {
    fn env(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
        self.context.upgrade()?.form_fill_env()
    }
}
impl XfaAppProvider for AppProvider {
    fn get_language(&self) -> Vec<u32> {
        self.env().map_or_else(Vec::new, |env| env.get_language())
    }
    fn get_platform(&self) -> Vec<u32> {
        self.env().map_or_else(Vec::new, |env| env.get_platform())
    }
    fn get_app_name(&self) -> Vec<u32> {
        self.env()
            .and_then(|env| env.host()?.xfa_host())
            .map_or_else(Vec::new, |host| host.app_name())
    }
    fn get_app_title(&self) -> Vec<u32> {
        "PDFium".bytes().map(u32::from).collect()
    }
    fn beep(&self, kind: u32) {
        if let Some(host) = self.env().and_then(|env| env.host()) {
            host.js_app_beep(kind as i32);
        }
    }
    fn message_box(&self, message: &[u32], title: &[u32], icon: u32, buttons: u32) -> i32 {
        let Some(context) = self
            .context
            .upgrade()
            .filter(|ctx| ctx.load_status() == LoadStatus::Loaded)
        else {
            return -1;
        };
        let Some(host) = context.form_fill_env().and_then(|env| env.host()) else {
            return -1;
        };
        host.js_app_alert(
            message,
            title,
            if buttons <= 3 { buttons as i32 } else { 0 },
            if icon <= 4 { icon as i32 } else { 0 },
        )
    }
    fn response(&self, question: &[u32], title: &[u32], default: &[u32], masked: bool) -> Vec<u32> {
        let Some(host) = self.env().and_then(|env| env.host()) else {
            return Vec::new();
        };
        let mut buffer = [0u8; 2048];
        let length = host.js_app_response(question, title, default, &[], masked, &mut buffer);
        if length <= 0 {
            Vec::new()
        } else {
            utf16_decode(&buffer[..(length as usize).min(buffer.len())], false)
        }
    }
    fn download_url(&self, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        self.env()?.host()?.xfa_host()?.download_url(url)
    }
    fn post_request_url(
        &self,
        url: &[u32],
        data: &[u32],
        content_type: &[u32],
        encoding: &[u32],
        header: &[u32],
    ) -> Option<Vec<u32>> {
        self.env()?
            .host()?
            .xfa_host()?
            .post_request_url(url, data, content_type, encoding, header)
    }
    fn put_request_url(&self, url: &[u32], data: &[u32], encoding: &[u32]) -> bool {
        self.env()
            .and_then(|env| env.host()?.xfa_host())
            .is_some_and(|host| host.put_request_url(url, data, encoding))
    }
    fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        self.env()?.timer_handler()
    }
}

#[cfg(test)]
#[path = "cpdfxfa_tests.rs"]
mod tests;
