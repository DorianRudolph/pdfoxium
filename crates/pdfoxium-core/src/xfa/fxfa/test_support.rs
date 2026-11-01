// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Shared actual FFDoc/App fixture; recording embedding services are test-only.
use super::{
    cxfa_ffapp::{CxfaFfApp, XfaAppProvider},
    cxfa_ffdoc::{CxfaFfDoc, PageViewEvent, XfaDocumentEnvironment},
    cxfa_ffwidget::FfWidget,
    fxfa::XfaPrintOpt,
    layout::{cxfa_layoutitem::LayoutItemWidget, cxfa_viewlayoutitem::CxfaViewLayoutItem},
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fpdfapi::parser::cpdf_document::CpdfDocument,
    fxcrt::{
        cfx_read_only_span_stream::CfxReadOnlySpanStream,
        cfx_timer::TimerHandler,
        fx_coordinates::{CfxPointF, CfxRectF},
        fx_stream::SeekableReadStream,
        xml::{cfx_xmldocument::CfxXmlDocument, cfx_xmlparser::CfxXmlParser},
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[derive(Clone, Debug, PartialEq)]
pub enum HostCall {
    ChangeMark,
    Invalidate(Option<CxfaViewLayoutItem>, CfxRectF),
    Caret(CxfaNode, bool, Option<CfxRectF>),
    Popup(CxfaNode, f32, f32, CfxRectF),
    Menu(CxfaNode, CfxPointF),
    PageEvent(Option<CxfaViewLayoutItem>, PageViewEvent),
    WidgetAdded(CxfaNode),
    WidgetRemoved(CxfaNode),
    SetPage(i32),
    Calculations(bool),
    Validations(bool),
    Title(Vec<u32>),
    Export(Vec<u32>, bool),
    Goto(Vec<u32>),
    Focus(Option<CxfaNode>),
    Print(i32, i32, XfaPrintOpt),
    Beep(u32),
    Message(Vec<u32>, Vec<u32>, u32, u32),
    Response(Vec<u32>, Vec<u32>, Vec<u32>, bool),
    Download(Vec<u32>),
    Post(Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>),
    Put(Vec<u32>, Vec<u32>, Vec<u32>),
    OpenLinked(Vec<u32>),
}
pub type CallbackHook = Rc<dyn Fn(&HostCall)>;
pub struct RecordingHost {
    pub pdf: RefCell<CpdfDocument>,
    pub xml: Rc<RefCell<CfxXmlDocument>>,
    pub calls: RefCell<Vec<HostCall>>,
    pub on_callback: RefCell<Option<CallbackHook>>,
    pub calculations: Cell<bool>,
    pub validations: Cell<bool>,
    pub current_page: Cell<i32>,
    pub page_count: Cell<i32>,
    pub title: RefCell<Vec<u32>>,
    pub response_value: RefCell<Vec<u32>>,
    pub message_result: Cell<i32>,
    pub highlight: Cell<u32>,
    pub popup_result: Cell<bool>,
    pub focus: RefCell<Option<CxfaNode>>,
    pub files: RefCell<Vec<(Vec<u32>, Rc<dyn SeekableReadStream>)>>,
}
impl RecordingHost {
    pub fn record(&self, call: HostCall) {
        self.calls.borrow_mut().push(call.clone());
        let callback = self.on_callback.borrow().clone();
        if let Some(callback) = callback {
            callback(&call);
        }
    }
    pub fn clear_calls(&self) {
        self.calls.borrow_mut().clear();
    }
}
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
pub fn new_document(xml: impl AsRef<[u8]>) -> (Rc<CxfaFfDoc>, Rc<RecordingHost>) {
    let mut bytes = xml.as_ref().to_vec();
    bytes.push(0);
    let xml = Rc::new(RefCell::new(
        CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
            .parse()
            .expect("valid XFA fixture XML"),
    ));
    let mut pdf = CpdfDocument::new();
    pdf.create_new_doc();
    let host = Rc::new(RecordingHost {
        pdf: RefCell::new(pdf),
        xml: xml.clone(),
        calls: RefCell::new(Vec::new()),
        on_callback: RefCell::new(None),
        calculations: Cell::new(true),
        validations: Cell::new(true),
        current_page: Cell::new(0),
        page_count: Cell::new(1),
        title: RefCell::new(wide("Test document")),
        response_value: RefCell::new(wide("Test response")),
        message_result: Cell::new(1),
        highlight: Cell::new(0x4080a0ff),
        popup_result: Cell::new(true),
        focus: RefCell::new(None),
        files: RefCell::new(Vec::new()),
    });
    let app = CxfaFfApp::new(host.clone());
    let doc = {
        let pdf = host.pdf.borrow();
        CxfaFfDoc::new(app, host.clone(), &pdf).expect("actual Boa context")
    };
    assert!(doc.open_doc(xml));
    assert!(doc.app().load_fwl_theme(&doc));
    host.clear_calls();
    (doc, host)
}
impl XfaAppProvider for RecordingHost {
    fn get_language(&self) -> Vec<u32> {
        wide("en_US")
    }
    fn get_platform(&self) -> Vec<u32> {
        wide("UNIX")
    }
    fn get_app_name(&self) -> Vec<u32> {
        wide("PDFOxium test")
    }
    fn get_app_title(&self) -> Vec<u32> {
        wide("PDFOxium test title")
    }
    fn beep(&self, kind: u32) {
        self.record(HostCall::Beep(kind));
    }
    fn message_box(&self, message: &[u32], title: &[u32], icon: u32, buttons: u32) -> i32 {
        self.record(HostCall::Message(
            message.to_vec(),
            title.to_vec(),
            icon,
            buttons,
        ));
        self.message_result.get()
    }
    fn response(&self, question: &[u32], title: &[u32], answer: &[u32], masked: bool) -> Vec<u32> {
        self.record(HostCall::Response(
            question.to_vec(),
            title.to_vec(),
            answer.to_vec(),
            masked,
        ));
        self.response_value.borrow().clone()
    }
    fn download_url(&self, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        self.record(HostCall::Download(url.to_vec()));
        self.files
            .borrow()
            .iter()
            .find(|(name, _)| name == url)
            .map(|(_, file)| file.clone())
    }
    fn post_request_url(
        &self,
        url: &[u32],
        data: &[u32],
        kind: &[u32],
        encoding: &[u32],
        header: &[u32],
    ) -> Option<Vec<u32>> {
        self.record(HostCall::Post(
            url.to_vec(),
            data.to_vec(),
            kind.to_vec(),
            encoding.to_vec(),
            header.to_vec(),
        ));
        Some(self.response_value.borrow().clone())
    }
    fn put_request_url(&self, url: &[u32], data: &[u32], encoding: &[u32]) -> bool {
        self.record(HostCall::Put(
            url.to_vec(),
            data.to_vec(),
            encoding.to_vec(),
        ));
        true
    }
    fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        None
    }
}
impl XfaDocumentEnvironment for RecordingHost {
    fn set_change_mark(&self, _: &CxfaFfDoc) {
        self.record(HostCall::ChangeMark);
    }
    fn invalidate_rect(&self, page: Option<&CxfaViewLayoutItem>, rect: CfxRectF) {
        self.record(HostCall::Invalidate(page.cloned(), rect));
    }
    fn display_caret(&self, widget: &dyn FfWidget, visible: bool, anchor: Option<CfxRectF>) {
        self.record(HostCall::Caret(
            widget.widget().node().clone(),
            visible,
            anchor,
        ));
    }
    fn get_popup_pos(
        &self,
        widget: &dyn FfWidget,
        min: f32,
        max: f32,
        anchor: CfxRectF,
        popup: &mut CfxRectF,
    ) -> bool {
        self.record(HostCall::Popup(
            widget.widget().node().clone(),
            min,
            max,
            anchor,
        ));
        *popup = CfxRectF::new(anchor.left, anchor.bottom(), anchor.width, max);
        self.popup_result.get()
    }
    fn popup_menu(&self, widget: &dyn FfWidget, point: CfxPointF) -> bool {
        self.record(HostCall::Menu(widget.widget().node().clone(), point));
        true
    }
    fn on_page_view_event(&self, page: Option<&CxfaViewLayoutItem>, event: PageViewEvent) {
        self.record(HostCall::PageEvent(page.cloned(), event));
    }
    fn widget_post_add(&self, widget: &dyn FfWidget) {
        self.record(HostCall::WidgetAdded(widget.widget().node().clone()));
    }
    fn widget_pre_remove(&self, widget: &dyn FfWidget) {
        self.record(HostCall::WidgetRemoved(widget.widget().node().clone()));
    }
    fn count_pages(&self, _: &CxfaFfDoc) -> i32 {
        self.page_count.get()
    }
    fn get_current_page(&self, _: &CxfaFfDoc) -> i32 {
        self.current_page.get()
    }
    fn set_current_page(&self, _: &CxfaFfDoc, page: i32) {
        self.current_page.set(page);
        self.record(HostCall::SetPage(page));
    }
    fn is_calculations_enabled(&self, _: &CxfaFfDoc) -> bool {
        self.calculations.get()
    }
    fn set_calculations_enabled(&self, _: &CxfaFfDoc, enabled: bool) {
        self.calculations.set(enabled);
        self.record(HostCall::Calculations(enabled));
    }
    fn get_title(&self, _: &CxfaFfDoc) -> Vec<u32> {
        self.title.borrow().clone()
    }
    fn set_title(&self, _: &CxfaFfDoc, title: &[u32]) {
        *self.title.borrow_mut() = title.to_vec();
        self.record(HostCall::Title(title.to_vec()));
    }
    fn export_data(&self, _: &CxfaFfDoc, path: &[u32], xdp: bool) {
        self.record(HostCall::Export(path.to_vec(), xdp));
    }
    fn goto_url(&self, _: &CxfaFfDoc, url: &[u32]) {
        self.record(HostCall::Goto(url.to_vec()));
    }
    fn is_validations_enabled(&self, _: &CxfaFfDoc) -> bool {
        self.validations.get()
    }
    fn set_validations_enabled(&self, _: &CxfaFfDoc, enabled: bool) {
        self.validations.set(enabled);
        self.record(HostCall::Validations(enabled));
    }
    fn set_focus_widget(&self, _: &CxfaFfDoc, widget: Option<Rc<dyn LayoutItemWidget>>) {
        let node = widget.and_then(|widget| {
            widget
                .as_ff_widget()
                .map(|widget| widget.widget().node().clone())
        });
        *self.focus.borrow_mut() = node.clone();
        self.record(HostCall::Focus(node));
    }
    fn print(&self, _: &CxfaFfDoc, start: i32, end: i32, options: XfaPrintOpt) {
        self.record(HostCall::Print(start, end, options));
    }
    fn get_highlight_color(&self, _: &CxfaFfDoc) -> u32 {
        self.highlight.get()
    }
    fn xml_document(&self) -> Option<Rc<RefCell<CfxXmlDocument>>> {
        Some(self.xml.clone())
    }
    fn open_linked_file(&self, _: &CxfaFfDoc, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        self.record(HostCall::OpenLinked(url.to_vec()));
        self.files
            .borrow()
            .iter()
            .find(|(name, _)| name == url)
            .map(|(_, file)| file.clone())
    }
}
