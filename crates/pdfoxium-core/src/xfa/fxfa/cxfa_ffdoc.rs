// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_ffdoc.{h,cpp}`. Actual embedding callbacks, parsed model, Boa context,
//! fonts, images and document view share one owner with weak back-references.
use super::{
    cxfa_ffapp::CxfaFfApp,
    cxfa_ffdocview::CxfaFfDocView,
    cxfa_ffimage::{load_image_from_buffer, ImageResource, XfaImageOwner},
    cxfa_ffnotify::CxfaFfNotify,
    cxfa_ffwidget::FfWidget,
    fxfa::{FormType, XfaPrintOpt},
    fxfa_basic::{XfaElement, XfaHashCode, XfaPacketType},
    layout::{
        cxfa_layoutitem::LayoutItemWidget, cxfa_layoutprocessor::CxfaLayoutProcessor,
        cxfa_viewlayoutitem::CxfaViewLayoutItem,
    },
    parser::{
        cxfa_dataexporter::CxfaDataExporter, cxfa_document::CxfaDocument,
        cxfa_document_builder::CxfaDocumentBuilder, cxfa_document_datamerge::DataMergeLayoutHost,
        cxfa_node::CxfaNode,
    },
};
use crate::{
    fpdfapi::parser::{
        cpdf_document::CpdfDocument, cpdf_object::PdfObject, cpdf_stream_acc::CpdfRetainedStreamAcc,
    },
    fpdfdoc::cpdf_nametree::CpdfNameTree,
    fxcodec::fx_codec_def::FxcodecImageType,
    fxcrt::{
        cfx_read_only_span_stream::CfxReadOnlySpanStream,
        fx_coordinates::{CfxPointF, CfxRectF},
        fx_stream::{SeekableReadStream, WriteStream},
        xml::cfx_xmldocument::CfxXmlDocument,
    },
    fxjs::{cfxjs_engine::CfxJsEngine, xfa::cfxjse_engine::CfxJseEngine},
    xfa::fgas::font::{cfgas_gefont::CfgasGeFont, cfgas_pdffontmgr::CfgasPdfFontMgr},
};
use boa_engine::{js_string, property::Attribute, JsResult};
use std::{
    cell::{Cell, OnceCell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum PageViewEvent {
    PostAdded = 1,
    PostRemoved = 3,
    StopLayout = 4,
}
/// Native document-environment capabilities. The library supplies no default
/// UI, network request, page selection or printing implementation.
pub trait XfaDocumentEnvironment {
    fn set_change_mark(&self, doc: &CxfaFfDoc);
    fn invalidate_rect(&self, page: Option<&CxfaViewLayoutItem>, rect: CfxRectF);
    fn display_caret(&self, widget: &dyn FfWidget, visible: bool, anchor: Option<CfxRectF>);
    fn get_popup_pos(
        &self,
        widget: &dyn FfWidget,
        min: f32,
        max: f32,
        anchor: CfxRectF,
        popup: &mut CfxRectF,
    ) -> bool;
    fn popup_menu(&self, widget: &dyn FfWidget, point: CfxPointF) -> bool;
    fn on_page_view_event(&self, page: Option<&CxfaViewLayoutItem>, event: PageViewEvent);
    fn widget_post_add(&self, widget: &dyn FfWidget);
    fn widget_pre_remove(&self, widget: &dyn FfWidget);
    fn count_pages(&self, doc: &CxfaFfDoc) -> i32;
    fn get_current_page(&self, doc: &CxfaFfDoc) -> i32;
    fn set_current_page(&self, doc: &CxfaFfDoc, page: i32);
    fn is_calculations_enabled(&self, doc: &CxfaFfDoc) -> bool;
    fn set_calculations_enabled(&self, doc: &CxfaFfDoc, enabled: bool);
    fn get_title(&self, doc: &CxfaFfDoc) -> Vec<u32>;
    fn set_title(&self, doc: &CxfaFfDoc, title: &[u32]);
    fn export_data(&self, doc: &CxfaFfDoc, path: &[u32], xdp: bool);
    fn goto_url(&self, doc: &CxfaFfDoc, url: &[u32]);
    fn is_validations_enabled(&self, doc: &CxfaFfDoc) -> bool;
    fn set_validations_enabled(&self, doc: &CxfaFfDoc, enabled: bool);
    fn set_focus_widget(&self, doc: &CxfaFfDoc, widget: Option<Rc<dyn LayoutItemWidget>>);
    fn print(&self, doc: &CxfaFfDoc, start: i32, end: i32, options: XfaPrintOpt);
    fn get_highlight_color(&self, doc: &CxfaFfDoc) -> u32;
    fn xml_document(&self) -> Option<Rc<RefCell<CfxXmlDocument>>>;
    fn open_linked_file(&self, doc: &CxfaFfDoc, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>>;
}
pub struct CxfaFfDoc {
    app: Rc<CxfaFfApp>,
    environment: Rc<dyn XfaDocumentEnvironment>,
    catalog: Option<Weak<RefCell<dyn PdfObject>>>,
    document: Rc<CxfaDocument>,
    notify: Rc<CxfaFfNotify>,
    layout: Rc<CxfaLayoutProcessor>,
    script: RefCell<Option<Rc<CfxJseEngine>>>,
    doc_view: OnceCell<Rc<CxfaFfDocView>>,
    pdf_fonts: Rc<CfgasPdfFontMgr>,
    images: RefCell<HashMap<Vec<u32>, Option<ImageResource>>>,
    form_type: Cell<FormType>,
    opened: Cell<bool>,
    closed: Cell<bool>,
    closing: Cell<bool>,
}
impl DataMergeLayoutHost for CxfaLayoutProcessor {
    fn set_force_relayout(&self) {
        CxfaLayoutProcessor::set_force_relayout(self);
    }
}
impl CxfaFfDoc {
    pub fn new(
        app: Rc<CxfaFfApp>,
        environment: Rc<dyn XfaDocumentEnvironment>,
        pdf: &CpdfDocument,
    ) -> JsResult<Rc<Self>> {
        Self::create(app, environment, pdf, None)
    }
    /// Use the SDK's actual Boa VM and host clock with a separate XFA realm.
    pub fn new_with_runtime(
        app: Rc<CxfaFfApp>,
        environment: Rc<dyn XfaDocumentEnvironment>,
        pdf: &CpdfDocument,
        javascript: Rc<CfxJsEngine>,
    ) -> JsResult<Rc<Self>> {
        Self::create(app, environment, pdf, Some(javascript))
    }
    fn create(
        app: Rc<CxfaFfApp>,
        environment: Rc<dyn XfaDocumentEnvironment>,
        pdf: &CpdfDocument,
        javascript: Option<Rc<CfxJsEngine>>,
    ) -> JsResult<Rc<Self>> {
        let catalog = pdf.root().map(|root| Rc::downgrade(&root));
        let pdf_fonts = Rc::new(CfgasPdfFontMgr::new(pdf));
        let doc = Rc::new_cyclic(|weak| {
            let notify = CxfaFfNotify::new(weak.clone());
            let document = CxfaDocument::with_notify(Some(notify.clone()));
            let layout = CxfaLayoutProcessor::new(&document);
            Self {
                app,
                environment,
                catalog,
                document,
                notify,
                layout,
                script: RefCell::new(None),
                doc_view: OnceCell::new(),
                pdf_fonts,
                images: RefCell::new(HashMap::new()),
                form_type: Cell::new(FormType::kXFAForeground),
                opened: Cell::new(false),
                closed: Cell::new(false),
                closing: Cell::new(false),
            }
        });
        let runtime = match javascript {
            Some(javascript) => CfxJseEngine::with_runtime(doc.document.clone(), javascript)?,
            None => CfxJseEngine::new(doc.document.clone())?,
        };
        runtime.set_data_merge_layout_host(Some(doc.layout.clone()));
        doc.layout.store().set_notify(Some(doc.notify.clone()));
        doc.layout.set_content_host(Some(doc.notify.clone()));
        doc.layout
            .get_layout_page_mgr()
            .set_host(Some(doc.notify.clone()));
        runtime.set_layout_host(Some(doc.notify.clone()));
        runtime.set_instance_manager_host(Some(doc.notify.clone()));
        let host = Rc::new(super::cxfa_ffhost::CxfaFfHost::new(Rc::downgrade(&doc)));
        runtime.set_host(Some(host.clone()));
        runtime.set_network_host(Some(host));
        *doc.script.borrow_mut() = Some(runtime);
        Ok(doc)
    }
    pub fn app(&self) -> Rc<CxfaFfApp> {
        self.app.clone()
    }
    pub fn environment(&self) -> Rc<dyn XfaDocumentEnvironment> {
        self.environment.clone()
    }
    pub fn model_document(&self) -> Rc<CxfaDocument> {
        self.document.clone()
    }
    pub fn notify(&self) -> Rc<CxfaFfNotify> {
        self.notify.clone()
    }
    pub fn layout_processor(&self) -> Rc<CxfaLayoutProcessor> {
        self.layout.clone()
    }
    pub fn script_context(&self) -> Rc<CfxJseEngine> {
        self.script
            .borrow()
            .as_ref()
            .expect("script requested after FF document close")
            .clone()
    }
    pub fn try_script_context(&self) -> Option<Rc<CfxJseEngine>> {
        self.script.borrow().clone()
    }
    pub fn form_type(&self) -> FormType {
        self.form_type.get()
    }
    pub fn get_doc_view(&self) -> Option<Rc<CxfaFfDocView>> {
        self.doc_view.get().cloned()
    }
    pub fn get_doc_view_for_layout(
        &self,
        layout: &Rc<CxfaLayoutProcessor>,
    ) -> Option<Rc<CxfaFfDocView>> {
        Rc::ptr_eq(&self.layout, layout)
            .then(|| self.get_doc_view())
            .flatten()
    }
    pub fn create_doc_view(self: &Rc<Self>) -> Rc<CxfaFfDocView> {
        if let Some(view) = self.doc_view.get() {
            return view.clone();
        }
        let view = CxfaFfDocView::new(Rc::downgrade(self));
        let _ = self.doc_view.set(view.clone());
        let runtime = self.script_context();
        runtime.set_widget_script_host(Some(view.clone()));
        runtime.set_event_host(Some(view.clone()));
        view
    }
    pub fn open_doc(&self, xml: Rc<RefCell<CfxXmlDocument>>) -> bool {
        if self.closed.get() {
            return false;
        }
        let mut builder = CxfaDocumentBuilder::new(self.document.clone());
        if !builder.build_document(xml, XfaPacketType::Xdp) {
            return false;
        }
        let Some(root) = builder.get_root_node() else {
            return false;
        };
        if !self.document.set_root(Some(root.clone())) {
            return false;
        }
        let runtime = self.script_context();
        let installed = runtime.with_script_context(|context| {
            let binding = runtime.get_or_create_binding(&root, context)?;
            context.register_global_property(js_string!("xfa"), binding, Attribute::all())?;
            Ok(())
        });
        if installed.is_err() {
            return false;
        }
        let language = self.app.provider().get_language();
        let javascript = Rc::downgrade(runtime.javascript());
        self.document.configure_locales(
            &language,
            Rc::new(move || {
                let javascript = javascript
                    .upgrade()
                    .expect("locale requested after its Boa context expired");
                javascript
                    .with_context(|context| {
                        let now = context.clock().system_time_millis() as f64;
                        Ok(
                            ((crate::fxjs::js_date::local_time(context, now) - now) / 60_000.0)
                                as i32,
                        )
                    })
                    .expect(
                        "XFA locale timezone callback requires the existing Boa host context lease",
                    )
            }),
        );
        self.opened.set(true);
        self.form_type.set(FormType::kXFAForeground);
        let dynamic = self
            .document
            .get_xfa_node(XfaHashCode::Config as u32)
            .and_then(|config| config.get_first_child_by_class(XfaElement::Acrobat))
            .and_then(|node| node.get_first_child_by_class(XfaElement::Acrobat7))
            .and_then(|node| node.get_first_child_by_class(XfaElement::DynamicRender));
        if dynamic.is_some_and(|node| {
            node.get_content()
                .iter()
                .copied()
                .eq("required".bytes().map(u32::from))
        }) {
            self.form_type.set(FormType::kXFAFull);
        }
        true
    }
    /// Explicit source prefinalization runs while weak document links can still
    /// be upgraded. Embedders call this before removing their owning handle.
    pub fn pre_finalize(self: &Rc<Self>) {
        if self.closed.get() || self.closing.replace(true) {
            return;
        }
        if let Some(view) = self.get_doc_view() {
            view.run_doc_close();
            view.clear_widget_layout_data();
        }
        self.closed.set(true);
        let runtime = self.script.borrow_mut().take();
        if let Some(runtime) = runtime {
            runtime.set_host(None);
            runtime.set_layout_host(None);
            runtime.set_network_host(None);
            runtime.set_widget_script_host(None);
            runtime.set_event_host(None);
            runtime.set_instance_manager_host(None);
            runtime.set_data_merge_layout_host(None);
            drop(runtime);
        }
        self.document.clear_layout_data();
        self.closing.set(false);
    }
    pub fn get_pdf_font(
        &self,
        family: &[u32],
        styles: u32,
        strict: bool,
    ) -> Option<Rc<CfgasGeFont>> {
        if !self.opened.get() {
            return None;
        }
        self.pdf_fonts.get_font(family, styles, strict)
    }
    pub fn get_font(&self, family: &[u32], styles: u32) -> Option<Rc<CfgasGeFont>> {
        if !self.opened.get() {
            return None;
        }
        self.app
            .font_mgr()
            .get_font(&self.pdf_fonts, family, styles)
    }
    pub fn pdf_font_mgr(&self) -> Rc<CfgasPdfFontMgr> {
        self.pdf_fonts.clone()
    }
    pub fn get_pdf_named_image(&self, name: &[u32]) -> Option<ImageResource> {
        if let Some(cached) = self.images.borrow().get(name) {
            return cached.clone();
        }
        let root = self.catalog.as_ref()?.upgrade()?;
        let tree = CpdfNameTree::create_from_root(&root, b"XFAImages")?;
        let count = tree.count();
        if count == 0 {
            return None;
        }
        let mut object = tree.lookup_value(name);
        if object.is_none() {
            for index in 0..count {
                if let Some((candidate, key)) = tree.lookup_value_and_name(index) {
                    if key == name {
                        object = Some(candidate);
                        break;
                    }
                }
            }
        }
        let stream = object?;
        let acc = CpdfRetainedStreamAcc::load_filtered(stream)?;
        let bytes = acc.share_data();
        let file = Rc::new(CfxReadOnlySpanStream::new(bytes.as_slice()));
        let mut image = load_image_from_buffer(file, FxcodecImageType::Unknown);
        if let Some(image) = &mut image {
            image.named = true;
        }
        // Full names avoid the native cache's accidental hash-collision aliases.
        self.images
            .borrow_mut()
            .insert(name.to_vec(), image.clone());
        image
    }
    pub fn save_package(&self, node: Option<&CxfaNode>, stream: &mut dyn WriteStream) -> bool {
        let root = node.cloned().or_else(|| self.document.get_root());
        root.is_some_and(|node| CxfaDataExporter::new().export(stream, &node))
    }
    pub fn set_change_mark(&self) {
        self.environment.set_change_mark(self);
    }
    pub fn invalidate_rect(&self, page: Option<&CxfaViewLayoutItem>, rect: CfxRectF) {
        self.environment.invalidate_rect(page, rect);
    }
    pub fn display_caret(&self, widget: &dyn FfWidget, visible: bool, anchor: Option<CfxRectF>) {
        self.environment.display_caret(widget, visible, anchor);
    }
    pub fn get_popup_pos(
        &self,
        widget: &dyn FfWidget,
        min: f32,
        max: f32,
        anchor: CfxRectF,
        popup: &mut CfxRectF,
    ) -> bool {
        self.environment
            .get_popup_pos(widget, min, max, anchor, popup)
    }
    pub fn popup_menu(&self, widget: &dyn FfWidget, point: CfxPointF) -> bool {
        self.environment.popup_menu(widget, point)
    }
    pub fn on_page_view_event(&self, page: Option<&CxfaViewLayoutItem>, event: PageViewEvent) {
        self.environment.on_page_view_event(page, event);
    }
    pub fn widget_post_add(&self, widget: &dyn FfWidget) {
        self.environment.widget_post_add(widget);
    }
    pub fn widget_pre_remove(&self, widget: &dyn FfWidget) {
        self.environment.widget_pre_remove(widget);
    }
    pub fn count_pages(&self) -> i32 {
        self.environment.count_pages(self)
    }
    pub fn get_current_page(&self) -> i32 {
        self.environment.get_current_page(self)
    }
    pub fn set_current_page(&self, page: i32) {
        self.environment.set_current_page(self, page);
    }
    pub fn is_calculations_enabled(&self) -> bool {
        self.environment.is_calculations_enabled(self)
    }
    pub fn set_calculations_enabled(&self, enabled: bool) {
        self.environment.set_calculations_enabled(self, enabled);
    }
    pub fn get_title(&self) -> Vec<u32> {
        self.environment.get_title(self)
    }
    pub fn set_title(&self, title: &[u32]) {
        self.environment.set_title(self, title);
    }
    pub fn export_data(&self, path: &[u32], xdp: bool) {
        self.environment.export_data(self, path, xdp);
    }
    pub fn goto_url(&self, url: &[u32]) {
        self.environment.goto_url(self, url);
    }
    pub fn is_validations_enabled(&self) -> bool {
        self.environment.is_validations_enabled(self)
    }
    pub fn set_validations_enabled(&self, enabled: bool) {
        self.environment.set_validations_enabled(self, enabled);
    }
    pub fn set_focus_widget(&self, widget: Option<Rc<dyn LayoutItemWidget>>) {
        self.environment.set_focus_widget(self, widget);
    }
    pub fn print(&self, start: i32, end: i32, options: XfaPrintOpt) {
        self.environment.print(self, start, end, options);
    }
    pub fn get_highlight_color(&self) -> u32 {
        self.environment.get_highlight_color(self)
    }
    pub fn get_ijs_runtime(&self) -> Rc<CfxJsEngine> {
        self.script_context().javascript().clone()
    }
    pub fn get_xml_document(&self) -> Option<Rc<RefCell<CfxXmlDocument>>> {
        self.environment.xml_document()
    }
    pub fn open_linked_file(&self, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        self.environment.open_linked_file(self, url)
    }
}
impl XfaImageOwner for CxfaFfDoc {
    fn get_pdf_named_image(&self, name: &[u32]) -> Option<ImageResource> {
        CxfaFfDoc::get_pdf_named_image(self, name)
    }
    fn open_linked_file(&self, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        CxfaFfDoc::open_linked_file(self, url)
    }
}
#[cfg(test)]
#[path = "cxfa_ffdoc_tests.rs"]
mod tests;
