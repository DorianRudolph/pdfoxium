// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Single-threaded AcroForm and XFA interaction over PDFium's actual form filler.
use crate::{
    Bitmap, Color, Document, Error, FormDialog, FormDialogResponse, PixelFormat, Point, Rect,
    RenderConfig, Result, WidgetFieldType,
};
use pdfoxium_core::{
    fpdfapi::{page::cpdf_page::CpdfPage, parser::cpdf_document::CpdfDocument},
    fpdfdoc::cpdf_aaction::{AActionType, CpdfAAction},
    fpdfsdk::{
        cpdfsdk_annot::CpdfSdkAnnot,
        cpdfsdk_formfillenvironment::{CpdfSdkFormFillEnvironment, FormFillHost},
        cpdfsdk_pageview::{CpdfSdkPageView, SdkPage},
        pwl::{ipwl_fillernotify::CursorStyle, pwl_draw::AggPwlDraw},
    },
    fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF, FxRect},
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmapData, fx_dib::FxdibFormat},
    },
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::{Rc, Weak},
};
use web_time::{Duration, Instant};
#[derive(Clone, Copy, Debug, Default)]
pub struct FormOptions {
    pub highlight: Option<Color>,
    /// Execute document and field scripts using the Rust JavaScript runtime.
    /// Requires the `js` feature; off by default for existing embeddings.
    pub javascript: bool,
    /// Optional maximum iterations of an individual JavaScript loop. This is
    /// not a wall-clock timeout or a total script instruction budget.
    pub javascript_loop_limit: Option<u64>,
}
#[derive(Clone, Copy, Debug)]
pub enum FormDocumentAction {
    WillClose,
    WillSave,
    DidSave,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FormModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
    pub left_button: bool,
}
impl FormModifiers {
    fn flags(self) -> u32 {
        u32::from(self.shift)
            | u32::from(self.control) << 1
            | u32::from(self.alt) << 2
            | u32::from(self.meta) << 3
            | u32::from(self.left_button) << 6
    }
}
/// Keyboard keys use PDFium's platform-independent virtual-key values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum FormKey {
    Backspace = 8,
    Tab = 9,
    Enter = 13,
    Escape = 27,
    Space = 32,
    PageUp = 33,
    PageDown = 34,
    End = 35,
    Home = 36,
    Left = 37,
    Up = 38,
    Right = 39,
    Down = 40,
    Delete = 46,
}
#[derive(Clone, Debug)]
pub enum FormInput {
    MouseMove(Point, FormModifiers),
    MouseDown(Point, FormModifiers),
    MouseUp(Point, FormModifiers),
    DoubleClick(Point, FormModifiers),
    /// Wheel vector in device directions; PDFium choices use its vertical sign.
    MouseWheel(Point, Point, FormModifiers),
    KeyDown(FormKey, FormModifiers),
    Text(String, FormModifiers),
    SelectAll,
    /// Select the Unicode word at the focused text caret.
    SelectWord,
    ReplaceSelection(String),
    Undo,
    Redo,
    Blur,
}
#[derive(Clone, Debug)]
pub enum FormEvent {
    ScriptError {
        message: String,
        line: i32,
        column: i32,
    },
    UnsupportedAction(String),
    Invalidate {
        page: usize,
        rect: Rect,
    },
    Selection {
        page: usize,
        rect: Rect,
    },
    Cursor(i32),
    FocusChanged,
    TextFocus {
        focused: bool,
        text: String,
    },
    Changed,
    NavigatePage(usize),
    Uri(Vec<u8>),
    Named(Vec<u8>),
    PagesChanged,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FormField {
    pub page: usize,
    pub index: usize,
    pub rect: Rect,
    /// Current interaction bounds, including an open choice popup.
    pub view_rect: Rect,
    pub name: String,
    pub value: String,
    pub kind: WidgetFieldType,
    pub flags: u32,
    pub checked: bool,
    pub visible: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormState {
    pub focus: Option<FormField>,
    pub text: String,
    pub selected_text: String,
    pub dirty: bool,
}
#[derive(Clone, Debug)]
pub struct FormInputResult {
    pub handled: bool,
    pub events: Vec<FormEvent>,
    pub state: FormState,
}
struct Timer {
    deadline: Instant,
    interval: Duration,
    callback: Rc<dyn Fn(i32)>,
}
type DialogHandler = Rc<dyn Fn(FormDialog) -> FormDialogResponse>;
struct Host {
    #[cfg(feature = "xfa")]
    self_weak: Weak<Host>,
    #[cfg(feature = "xfa")]
    xfa: RefCell<Weak<pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_context::CpdfXfaContext>>,
    javascript: bool,
    loop_limit: Option<u64>,
    dialog: RefCell<Option<DialogHandler>>,
    document: Weak<RefCell<CpdfDocument>>,
    pages: RefCell<BTreeMap<usize, Rc<SdkPage>>>,
    events: RefCell<VecDeque<FormEvent>>,
    timers: RefCell<BTreeMap<i32, Timer>>,
    next_timer: Cell<i32>,
    current: Cell<usize>,
    closing: Cell<bool>,
    changed: Cell<bool>,
}
fn text(value: &[u32]) -> String {
    value
        .iter()
        .map(|c| char::from_u32(*c).unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}
fn rect(r: CfxFloatRect) -> Rect {
    Rect {
        left: r.left,
        bottom: r.bottom,
        right: r.right,
        top: r.top,
    }
}
impl Host {
    fn emit(&self, event: FormEvent) {
        if !self.closing.get() {
            self.events.borrow_mut().push_back(event);
        }
    }
    fn page(&self, index: usize) -> Option<Rc<SdkPage>> {
        #[cfg(feature = "xfa")]
        if self
            .xfa
            .borrow()
            .upgrade()
            .filter(|c| c.contains_extension_form())
            .is_some_and(|c| index >= c.get_page_count().max(0) as usize)
        {
            return None;
        }
        if let Some(page) = self.pages.borrow().get(&index).filter(|p| p.is_alive()) {
            return Some(page.clone());
        }
        #[cfg(feature = "xfa")]
        if let Some(context) = self
            .xfa
            .borrow()
            .upgrade()
            .filter(|c| c.contains_extension_form())
        {
            let xfa = context.get_or_create_xfa_page(i32::try_from(index).ok()?)?;
            let page = context.sdk_page(&xfa)?;
            self.pages.borrow_mut().insert(index, page.clone());
            return Some(page);
        }
        let document = self.document.upgrade()?;
        let geometry = CpdfPage::from_document(&mut document.borrow_mut(), index)?;
        let page = SdkPage::new(&document, &geometry);
        self.pages.borrow_mut().insert(index, page.clone());
        Some(page)
    }
}
#[cfg(feature = "xfa")]
impl pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_docenvironment::XfaSdkHost for Host {
    fn app_name(&self) -> Vec<u32> {
        "pdfoxium".chars().map(u32::from).collect()
    }
    fn page_view_rect(&self, page: &SdkPage) -> Option<CfxFloatRect> {
        // XFA popup placement consumes top-down viewport coordinates, unlike
        // the PDF bounding box's bottom/top order.
        Some(CfxFloatRect::new(0., page.height(), page.width(), 0.))
    }
    fn current_page_index(&self) -> i32 {
        self.current.get() as i32
    }
    fn set_current_page(&self, index: i32) {
        if let Ok(index) = usize::try_from(index) {
            self.current.set(index);
            self.emit(FormEvent::NavigatePage(index));
        }
    }
    fn page_event(&self, _: i32, _: pdfoxium_core::xfa::fxfa::cxfa_ffdoc::PageViewEvent) {
        self.emit(FormEvent::PagesChanged);
    }
    fn goto_url(&self, url: &[u32]) {
        self.emit(FormEvent::Uri(text(url).into_bytes()));
    }
    fn display_caret(&self, page: &SdkPage, _: bool, r: CfxFloatRect) {
        if let Ok(page) = usize::try_from(page.index()) {
            self.emit(FormEvent::Invalidate {
                page,
                rect: rect(r),
            });
        }
    }
}
impl FormFillHost for Host {
    #[cfg(feature = "xfa")]
    fn xfa_host(
        &self,
    ) -> Option<Rc<dyn pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_docenvironment::XfaSdkHost>> {
        self.self_weak.upgrade().map(|h| {
            h as Rc<dyn pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_docenvironment::XfaSdkHost>
        })
    }
    fn javascript_enabled(&self) -> bool {
        self.javascript
    }
    fn javascript_loop_limit(&self) -> Option<u64> {
        self.loop_limit
    }
    fn get_platform(&self) -> Vec<u32> {
        let platform = if cfg!(target_os = "macos") {
            "MAC"
        } else if cfg!(windows) {
            "WIN"
        } else {
            "UNIX"
        };
        platform.chars().map(u32::from).collect()
    }
    fn js_field_browse(&self) -> Vec<u32> {
        self.emit(FormEvent::UnsupportedAction(
            "JavaScript file selection is not available".into(),
        ));
        Vec::new()
    }
    fn submit_form(&self, _: &[u8], _: &[u32]) -> bool {
        self.emit(FormEvent::UnsupportedAction(
            "Form submission is not available".into(),
        ));
        false
    }
    fn js_doc_print(&self, _: bool, _: i32, _: i32, _: bool, _: bool, _: bool, _: bool, _: bool) {
        self.emit(FormEvent::UnsupportedAction(
            "JavaScript printing is not available".into(),
        ));
    }
    fn js_doc_mail_form(
        &self,
        _: &[u8],
        _: bool,
        _: &[u32],
        _: &[u32],
        _: &[u32],
        _: &[u32],
        _: &[u32],
    ) {
        self.emit(FormEvent::UnsupportedAction(
            "JavaScript email is not available".into(),
        ));
    }
    fn js_app_alert(&self, message: &[u32], title: &[u32], buttons: i32, icon: i32) -> i32 {
        if self.closing.get() {
            return 2;
        }
        if !(0..=3).contains(&buttons) {
            self.emit(FormEvent::UnsupportedAction(format!(
                "unsupported JavaScript alert button type {buttons}"
            )));
            return -1;
        }
        let dialog = FormDialog::Alert {
            message: text(message),
            title: text(title),
            buttons,
            icon,
        };
        let handler = self.dialog.borrow().clone();
        let Some(handler) = handler else {
            self.emit(FormEvent::UnsupportedAction(
                "JavaScript alert has no dialog handler".into(),
            ));
            return -1;
        };
        match handler(dialog) {
            FormDialogResponse::Ok => 1,
            FormDialogResponse::Cancel => 2,
            FormDialogResponse::No => 3,
            FormDialogResponse::Yes => 4,
            FormDialogResponse::Text(_) => -1,
        }
    }
    fn js_app_response(
        &self,
        question: &[u32],
        title: &[u32],
        default: &[u32],
        label: &[u32],
        password: bool,
        output: &mut [u8],
    ) -> i32 {
        if self.closing.get() {
            return 0;
        }
        let dialog = FormDialog::Response {
            question: text(question),
            title: text(title),
            default: text(default),
            label: text(label),
            password,
        };
        let handler = self.dialog.borrow().clone();
        let Some(handler) = handler else {
            self.emit(FormEvent::UnsupportedAction(
                "JavaScript prompt has no dialog handler".into(),
            ));
            return -1;
        };
        match handler(dialog) {
            FormDialogResponse::Text(value) => {
                let bytes: Vec<_> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
                if bytes.len() > output.len() {
                    return i32::try_from(bytes.len()).unwrap_or(i32::MAX);
                }
                output[..bytes.len()].copy_from_slice(&bytes);
                bytes.len() as i32
            }
            // PDFium's response callback has no distinct cancellation encoding.
            FormDialogResponse::Cancel => 0,
            _ => -1,
        }
    }
    fn get_page(&self, index: i32) -> Option<Rc<SdkPage>> {
        usize::try_from(index).ok().and_then(|i| self.page(i))
    }
    fn current_page(&self) -> Option<Rc<SdkPage>> {
        self.page(self.current.get())
    }
    fn invalidate(&self, page: &SdkPage, r: FxRect) -> bool {
        if let Ok(page) = usize::try_from(page.index()) {
            self.emit(FormEvent::Invalidate {
                page,
                rect: Rect {
                    left: r.left as f32,
                    bottom: r.top.min(r.bottom) as f32,
                    right: r.right as f32,
                    top: r.top.max(r.bottom) as f32,
                },
            });
        }
        true
    }
    fn output_selected_rect(&self, page: &SdkPage, r: CfxFloatRect) -> bool {
        if let Ok(page) = usize::try_from(page.index()) {
            self.emit(FormEvent::Selection {
                page,
                rect: rect(r),
            });
        }
        true
    }
    fn is_selection_implemented(&self) -> bool {
        false
    }
    fn set_cursor(&self, cursor: CursorStyle) -> bool {
        self.emit(FormEvent::Cursor(cursor as i32));
        true
    }
    fn on_change(&self) -> bool {
        self.changed.set(true);
        self.emit(FormEvent::Changed);
        true
    }
    fn on_focus_change(&self, _: &Rc<CpdfSdkAnnot>, _: i32) -> bool {
        self.emit(FormEvent::FocusChanged);
        true
    }
    fn set_text_field_focus(&self, value: &[u32], focused: bool) -> bool {
        self.emit(FormEvent::TextFocus {
            focused,
            text: text(value),
        });
        true
    }
    fn set_timer(&self, interval: i32, callback: Rc<dyn Fn(i32)>) -> i32 {
        if self.closing.get() || interval <= 0 {
            return 0;
        }
        let Some(id) = self.next_timer.get().checked_add(1) else {
            return 0;
        };
        self.next_timer.set(id);
        let interval = Duration::from_millis(interval as u64);
        self.timers.borrow_mut().insert(
            id,
            Timer {
                deadline: Instant::now() + interval,
                interval,
                callback,
            },
        );
        id
    }
    fn kill_timer(&self, id: i32) {
        self.timers.borrow_mut().remove(&id);
    }
    fn do_uri_action(&self, uri: &[u8], _: u32) -> bool {
        self.emit(FormEvent::Uri(uri.to_vec()));
        true
    }
    fn execute_named_action(&self, name: &[u8]) -> bool {
        self.emit(FormEvent::Named(name.to_vec()));
        true
    }
    fn js_doc_goto_page(&self, index: i32) -> bool {
        if let Ok(index) = usize::try_from(index) {
            self.current.set(index);
            self.emit(FormEvent::NavigatePage(index));
            true
        } else {
            false
        }
    }
    fn do_go_to_action(&self, index: i32, _: i32, _: &[f32]) -> bool {
        self.js_doc_goto_page(index)
    }
}
pub(crate) struct FormInner {
    #[cfg(feature = "xfa")]
    xfa: Option<Rc<pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_context::CpdfXfaContext>>,
    initialized: Cell<bool>,
    change_generation: Cell<u64>,
    document: Document,
    env: Rc<CpdfSdkFormFillEnvironment>,
    host: Rc<Host>,
}
impl Drop for FormInner {
    fn drop(&mut self) {
        self.host.closing.set(true);
        // Commit before shutdown marks the environment as being destroyed.
        // Page leases and core guards retain us until their readers finish.
        self.env.kill_focus_annot(0);
        self.env.shutdown();
        // Keep wrapper revisions/dirty state accurate on implicit focus loss.
        let notified = self.host.changed.replace(false);
        if notified || self.env.change_generation() != self.change_generation.get() {
            let _ = self.document.record_form_change();
        }
        self.document.set_form_dirty(self.env.change_mark());
        for page in self.host.pages.borrow().values() {
            page.invalidate();
        }
        self.host.timers.borrow_mut().clear();
    }
}
/// A retained interactive form session. Clones share focus, field state and timers.
/// JavaScript is opt-in and required for interactive XFA.
#[derive(Clone)]
pub struct FormSession(pub(crate) Rc<FormInner>);
impl Document {
    pub fn forms(&self, options: FormOptions) -> Result<FormSession> {
        if options.javascript && !cfg!(feature = "js") {
            return Err(Error::Unsupported("JavaScript requires the js feature"));
        }
        if self.active_forms().is_some() {
            return Err(Error::Busy);
        }
        let xfa = matches!(
            self.form_type()?,
            crate::FormType::XfaFull | crate::FormType::XfaForeground
        );
        if xfa && (!cfg!(feature = "xfa") || !options.javascript) {
            return Err(Error::Unsupported(
                "interactive XFA requires the xfa feature and JavaScript",
            ));
        }
        let session = self.with_form_core(|document| {
            let host = Rc::new_cyclic(|_weak| Host {
                #[cfg(feature = "xfa")]
                self_weak: _weak.clone(),
                #[cfg(feature = "xfa")]
                xfa: RefCell::new(Weak::new()),
                javascript: options.javascript,
                loop_limit: options.javascript_loop_limit,
                dialog: RefCell::new(None),
                document: Rc::downgrade(document),
                pages: Default::default(),
                events: Default::default(),
                timers: Default::default(),
                next_timer: Cell::new(0),
                current: Cell::new(0),
                closing: Cell::new(false),
                changed: Cell::new(false),
            });
            let interface: Rc<dyn FormFillHost> = host.clone();
            let env = CpdfSdkFormFillEnvironment::new(document, Some(&interface));
            #[cfg(feature = "xfa")]
            let xfa = if xfa {
                let context =
                    pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_context::CpdfXfaContext::new(document);
                *host.xfa.borrow_mut() = Rc::downgrade(&context);
                context.set_form_fill_env(Some(&env));
                Some(context)
            } else {
                None
            };
            if self.is_dirty() {
                env.set_change_mark();
            }
            if let Some(c) = options.highlight {
                let form = env.interactive_form();
                form.set_highlight_alpha(c.alpha);
                form.set_all_highlight_colors(
                    // PDFium highlight colors use COLORREF (0x00BBGGRR).
                    u32::from(c.red) | (u32::from(c.green) << 8) | (u32::from(c.blue) << 16),
                );
            }
            Ok(FormSession(Rc::new(FormInner {
                #[cfg(feature = "xfa")]
                xfa,
                initialized: Cell::new(false),
                change_generation: Cell::new(env.change_generation()),
                document: self.clone(),
                env,
                host,
            })))
        })?;
        self.register_forms(&session);
        Ok(session)
    }
}
impl FormSession {
    /// Run named document scripts and OpenAction once, after installing a host
    /// dialog handler. Input events then share the same persistent JS realm.
    pub fn initialize(&self) -> Result<()> {
        self.0.document.with_form_core(|_| {
            if self.0.initialized.replace(true) {
                return Ok(());
            }
            #[cfg(feature = "xfa")]
            if let Some(context) = &self.0.xfa {
                if context.load_xfa_doc().is_err() {
                    self.0.initialized.set(false);
                    return Err(Error::Format);
                }
            }
            let _event = self.0.env.begin_input_event();
            let _ = self.0.env.proc_javascript_action();
            self.publish_script_errors();
            let _ = self.0.env.proc_open_action();
            self.finish_action()
        })
    }
    pub fn document_action(&self, action: FormDocumentAction) -> Result<()> {
        self.0.document.with_form_core(|document| {
            let _event = self.0.env.begin_input_event();
            let kind = match action {
                FormDocumentAction::WillClose => AActionType::CloseDocument,
                FormDocumentAction::WillSave => AActionType::SaveDocument,
                FormDocumentAction::DidSave => AActionType::DocumentSaved,
            };
            let root = document.borrow().root();
            let aa = root.and_then(|root| {
                root.borrow()
                    .as_any()
                    .downcast_ref::<pdfoxium_core::fpdfapi::parser::cpdf_dictionary::CpdfDictionary>(
                    )?
                    .get_dict_for(b"AA")
            });
            let aa = CpdfAAction::new(aa);
            if aa.action_exist(kind) {
                let _ = self.0.env.do_action_document(&aa.action(kind), kind);
            }
            self.finish_action()
        })
    }
    /// Enter/leave a visible page. Dispatch page /O or /C and annotation
    /// /PO, /PV or /PI, /PC, including hidden controller annotations.
    /// These actions are separate from parsed-page loading/eviction.
    pub fn page_action(&self, page: usize, open: bool) -> Result<()> {
        // Full-XFA pages have no PDF dictionary or PDF /AA entry, matching
        // FORM_DoPageAAction's null PDF-page branch. XFA lifecycle is driven by
        // its document view; still validate the SDK page here.
        if self.is_full_xfa() {
            return self.0.document.with_form_core(|_| {
                self.view(page)?;
                self.finish_action()
            });
        }

        self.0.document.with_form_core(|_| {
            let _event = self.0.env.begin_input_event();
            let view = self.view(page)?;
            let dict = view
                .page()
                .and_then(|page| page.pdf_dict())
                .ok_or(Error::Stale)?;
            let aa = CpdfAAction::new(
                dict.borrow()
                    .as_any()
                    .downcast_ref::<pdfoxium_core::fpdfapi::parser::cpdf_dictionary::CpdfDictionary>(
                    )
                    .and_then(|dict| dict.get_dict_for(b"AA")),
            );
            let kind = if open {
                AActionType::OpenPage
            } else {
                AActionType::ClosePage
            };
            if open && aa.action_exist(kind) {
                let _ = self.0.env.do_action_page(&aa.action(kind), kind);
            }
            // Snapshot owners, not an annotation-list borrow: actions can add
            // or remove annotations. Hidden Screen annotations are commonly
            // animation controllers and still receive page lifecycle events.
            let annotations = view.annot_list();
            let events = if open {
                [AActionType::PageOpen, AActionType::PageVisible]
            } else {
                [AActionType::PageInvisible, AActionType::PageClose]
            };
            for event in events {
                for annot in &annotations {
                    if !annot.is_alive() { continue; }
                    let Some(dict) = annot.pdf_dict() else { continue; };
                    let actions = CpdfAAction::new(dict.borrow().as_any()
                        .downcast_ref::<pdfoxium_core::fpdfapi::parser::cpdf_dictionary::CpdfDictionary>()
                        .and_then(|dict| dict.get_dict_for(b"AA")));
                    if actions.action_exist(event) {
                        let _ = self.0.env.do_action_page(&actions.action(event), event);
                    }
                }
            }
            if !open && aa.action_exist(kind) {
                let _ = self.0.env.do_action_page(&aa.action(kind), kind);
            }
            self.finish_action()
        })
    }
    fn finish_action(&self) -> Result<()> {
        self.publish_script_errors();
        self.record_changes()?;
        Ok(())
    }
    fn record_changes(&self) -> Result<()> {
        let generation = self.0.env.change_generation();
        let notified = self.0.host.changed.replace(false);
        let changed = generation != self.0.change_generation.replace(generation);
        if notified || changed {
            self.0.document.record_form_change()?;
        }
        // Preserve explicit script writes to this.dirty without pretending
        // that clearing the flag changed page content or its revision.
        self.0.document.set_form_dirty(self.0.env.change_mark());
        Ok(())
    }
    /// The callback runs on this session's owning thread, without borrowing the
    /// handler cell. The core operation remains active: conflicting document
    /// or session calls return Busy. Avoid capturing an owning session here.
    pub fn set_dialog_handler(&self, handler: impl Fn(FormDialog) -> FormDialogResponse + 'static) {
        *self.0.host.dialog.borrow_mut() = Some(Rc::new(handler));
    }
    pub fn javascript_enabled(&self) -> bool {
        self.0.host.javascript
    }
    /// Add static markup or a note while retaining this session's JS context.
    /// Commits focused input and retires the affected SDK page view first.
    pub fn add_annotation(
        &self,
        page: usize,
        spec: crate::AnnotationSpec,
    ) -> Result<crate::AnnotId> {
        self.prepare_annotation_edit(page)?;
        self.0
            .document
            .page(page)?
            .add_annotation_with_forms(spec, true)
    }
    /// Restyle static editor annotations without replacing the live form/JS session.
    pub fn set_annotation_style(
        &self,
        page: usize,
        id: crate::AnnotId,
        style: crate::AnnotationStyle,
    ) -> Result<()> {
        style.validate_for(self.0.document.page(page)?.annotation(id)?.kind)?;
        self.check_static_annotation(page, id)?;
        self.prepare_annotation_edit(page)?;
        self.0
            .document
            .page(page)?
            .set_annotation_style_with_forms(id, style, true)
    }
    /// Remove a non-widget annotation. Widget/interactive annotation editing is
    /// deliberately excluded from this live editing path.
    pub fn remove_annotation(&self, page: usize, id: crate::AnnotId) -> Result<()> {
        self.check_static_annotation(page, id)?;
        self.prepare_annotation_edit(page)?;
        self.0
            .document
            .page(page)?
            .remove_annotation_with_forms(id, true)
    }
    pub fn set_annotation_contents(
        &self,
        page: usize,
        id: crate::AnnotId,
        text: &str,
    ) -> Result<()> {
        self.check_static_annotation(page, id)?;
        self.prepare_annotation_edit(page)?;
        self.0
            .document
            .page(page)?
            .set_annotation_contents_with_forms(id, text, true)
    }
    /// Translate a text note, preserving its dimensions and existing appearance.
    pub fn move_note(&self, page: usize, id: crate::AnnotId, offset: Point) -> Result<()> {
        self.translate_annotation(page, id, offset, true)
    }
    /// Translate a note, free-text box, square or circle in native PDF units.
    /// Keeps its dimensions and existing appearance, including custom artwork;
    /// moving free text does not reflow or regenerate it.
    pub fn move_annotation(&self, page: usize, id: crate::AnnotId, offset: Point) -> Result<()> {
        self.translate_annotation(page, id, offset, false)
    }
    fn translate_annotation(
        &self,
        page: usize,
        id: crate::AnnotId,
        offset: Point,
        note_only: bool,
    ) -> Result<()> {
        if !offset.x.is_finite() || !offset.y.is_finite() {
            return Err(Error::InvalidArgument("annotation offset must be finite"));
        }
        let handle = self.0.document.page(page)?;
        let kind = handle.annotation(id)?.kind;
        let supported = kind == crate::AnnotationKind::Text
            || (!note_only
                && matches!(
                    kind,
                    crate::AnnotationKind::FreeText
                        | crate::AnnotationKind::Square
                        | crate::AnnotationKind::Circle
                ));
        if !supported {
            return Err(Error::Unsupported(if note_only {
                "only text notes can be moved by move_note"
            } else {
                "movement supports notes, free-text boxes, squares and circles"
            }));
        }
        if offset.x == 0. && offset.y == 0. {
            return Ok(());
        }
        self.prepare_annotation_edit(page)?;
        let current = handle.annotation(id)?;
        if current.kind != kind {
            return Err(Error::Stale);
        }
        let r = current.rect;
        handle.set_annotation_bounds_with_forms(
            id,
            crate::Rect {
                left: r.left + offset.x,
                right: r.right + offset.x,
                bottom: r.bottom + offset.y,
                top: r.top + offset.y,
            },
            true,
        )
    }
    /// Resize a free-text, square or circle annotation in native PDF page units.
    /// Rebuilds its built-in appearance, including free-text layout, rather than
    /// stretching its previous appearance. Text-note icons and markup are excluded.
    pub fn resize_annotation(
        &self,
        page: usize,
        id: crate::AnnotId,
        rect: crate::Rect,
    ) -> Result<()> {
        let valid = [
            rect.left,
            rect.bottom,
            rect.right,
            rect.top,
            rect.width(),
            rect.height(),
        ]
        .iter()
        .all(|x| x.is_finite())
            && rect.width() > 0.
            && rect.height() > 0.;
        if !valid {
            return Err(Error::InvalidArgument(
                "annotation rectangle must be finite and positive",
            ));
        }
        let handle = self.0.document.page(page)?;
        let check = |kind| {
            if matches!(
                kind,
                crate::AnnotationKind::FreeText
                    | crate::AnnotationKind::Square
                    | crate::AnnotationKind::Circle
            ) {
                Ok(())
            } else {
                Err(Error::Unsupported(
                    "only free-text, square and circle annotations can be resized",
                ))
            }
        };
        let before = handle.annotation(id)?;
        check(before.kind)?;
        if before.rect == rect {
            return Ok(());
        }
        self.prepare_annotation_edit(page)?;
        check(handle.annotation(id)?.kind)?;
        handle.set_annotation_rect_with_forms(id, rect, true)
    }
    fn check_static_annotation(&self, page: usize, id: crate::AnnotId) -> Result<()> {
        use crate::AnnotationKind as K;
        match self.0.document.page(page)?.annotation(id)?.kind {
            K::Text
            | K::FreeText
            | K::Highlight
            | K::Underline
            | K::Squiggly
            | K::StrikeOut
            | K::Square
            | K::Circle
            | K::Ink => Ok(()),
            _ => Err(Error::Unsupported(
                "live edits support static markup and notes only",
            )),
        }
    }
    fn prepare_annotation_edit(&self, page: usize) -> Result<()> {
        if self.is_xfa() {
            return Err(Error::Unsupported(
                "PDF annotation editing on XFA documents",
            ));
        }
        self.0.document.page(page)?;
        self.0.document.with_form_core(|_| {
            if self.0.env.get_focus_annot().is_some() && !self.0.env.kill_focus_annot(0) {
                return Err(Error::Edit);
            }
            let sdk_page = self.0.host.pages.borrow().get(&page).cloned();
            if let Some(page) = sdk_page {
                self.0.env.remove_page_view(&page);
            }
            self.finish_action()
        })
    }
    /// Update the embedding viewer's current page without opening/closing it.
    pub fn set_current_page(&self, page: usize) -> Result<()> {
        let count = self.0.document.page_count()?;
        if page >= count {
            return Err(Error::PageOutOfRange { index: page, count });
        }
        self.0.host.current.set(page);
        Ok(())
    }
    fn publish_script_errors(&self) {
        #[cfg(feature = "js")]
        if let Some(error) = self.0.env.take_last_js_error() {
            self.0.host.emit(FormEvent::ScriptError {
                message: String::from_utf16_lossy(&error.message),
                line: error.line,
                column: error.column,
            });
        }
        if let Some(error) = self.0.env.take_action_error() {
            if error != pdfoxium_core::fpdfsdk::cpdfsdk_formfillenvironment::ActionError::JavaScriptException {
                self.0.host.emit(FormEvent::UnsupportedAction(format!("form action failed: {error:?}")));
            }
        }
    }
    fn view(&self, page: usize) -> Result<Rc<CpdfSdkPageView>> {
        let page = self.0.host.page(page).ok_or(Error::PageOutOfRange {
            index: page,
            count: self.0.env.get_page_count() as usize,
        })?;
        let view = self
            .0
            .env
            .get_or_create_page_view(&page)
            .ok_or(Error::Format)?;
        view.set_valid(true);
        Ok(view)
    }
    fn field(&self, annot: &Rc<CpdfSdkAnnot>, index: usize) -> Option<FormField> {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = annot.xfa_widget() {
            use pdfoxium_core::xfa::fxfa::fxfa_basic::XfaAttribute;
            let widget = xfa.ff_widget().as_ff_widget()?;
            let kind = match widget.get_form_field_type() as i32 {
                9 => WidgetFieldType::CheckBox,
                10 => WidgetFieldType::ComboBox,
                12 => WidgetFieldType::ListBox,
                13 => WidgetFieldType::PushButton,
                14 => WidgetFieldType::Signature,
                15 => WidgetFieldType::Text,
                _ => return None,
            };
            let node = widget.widget().node();
            let value = text(&node.get_raw_value());
            return Some(FormField {
                page: annot.page_view()?.page_index().try_into().ok()?,
                index,
                rect: rect(annot.rect()),
                view_rect: rect(annot.view_bbox()),
                name: text(&node.get_cdata(XfaAttribute::Name)),
                checked: matches!(
                    kind,
                    WidgetFieldType::CheckBox | WidgetFieldType::RadioButton
                ) && !value.is_empty()
                    && value != "0"
                    && value != "Off",
                value,
                kind,
                flags: if node.is_open_access() { 0 } else { 1 },
                visible: xfa.is_alive(),
            });
        }
        let widget = annot.widget()?;
        let field = widget.field()?;
        Some(FormField {
            page: annot.page_view()?.page_index().try_into().ok()?,
            index,
            rect: rect(annot.rect()),
            view_rect: rect(annot.view_bbox()),
            name: text(&field.full_name()),
            value: text(&field.value()),
            kind: match field.field_type() as i32 {
                1 => WidgetFieldType::PushButton,
                2 => WidgetFieldType::CheckBox,
                3 => WidgetFieldType::RadioButton,
                4 => WidgetFieldType::ComboBox,
                5 => WidgetFieldType::ListBox,
                6 => WidgetFieldType::Text,
                7 => WidgetFieldType::Signature,
                _ => WidgetFieldType::Unknown,
            },
            flags: field.field_flags(),
            checked: widget.control().is_checked(),
            visible: annot.optional_content_visible()
                && annot
                    .pdf_dict()
                    .is_some_and(|d| pdfoxium_core::fpdfsdk::fpdf_annot::flags(&d) & 35 == 0),
        })
    }
    fn state_inner(&self) -> FormState {
        let annot = self.0.env.get_focus_annot();
        let focus = annot.as_ref().and_then(|a| {
            let view = a.page_view()?;
            let index = view.annot_list().iter().position(|v| Rc::ptr_eq(v, a))?;
            self.field(a, index)
        });
        let view = annot.as_ref().and_then(|a| a.page_view());
        let value = view
            .as_ref()
            .map(|v| text(&v.focused_form_text()))
            .unwrap_or_default();
        let editing = annot
            .as_ref()
            .and_then(|a| a.widget())
            .and_then(|w| self.0.env.interactive_form_filler().get_form_field(&w))
            .is_some_and(|f| f.base().is_valid());
        let editing = editing || self.is_xfa() && annot.is_some();
        let pending = editing
            && focus.as_ref().is_some_and(|f| {
                matches!(f.kind, WidgetFieldType::ComboBox | WidgetFieldType::Text)
                    && value != f.value
            });
        FormState {
            focus,
            text: value,
            selected_text: view.map(|v| text(&v.selected_text())).unwrap_or_default(),
            dirty: self.0.document.is_dirty() || pending,
        }
    }
    fn select_word(&self, view: &Rc<CpdfSdkPageView>) -> bool {
        use pdfoxium_core::fpdfsdk::pwl::{cpwl_combo_box::CpwlComboBox, cpwl_edit::CpwlEdit};
        let Some(widget) = self.0.env.get_focus_annot().and_then(|a| a.widget()) else {
            return false;
        };
        let Some(filler) = self.0.env.interactive_form_filler().get_form_field(&widget) else {
            return false;
        };
        let Some(window) = filler.base().pwl_window(view) else {
            return false;
        };
        let combo_edit = window
            .as_any()
            .downcast_ref::<CpwlComboBox>()
            .and_then(|c| c.edit());
        let Some(edit) = window
            .as_any()
            .downcast_ref::<CpwlEdit>()
            .or(combo_edit.as_deref())
        else {
            return false;
        };
        let (start, end) = word_selection(&edit.text(), edit.selection().1);
        if start == end {
            return false;
        }
        edit.set_selection(start, end);
        true
    }
    pub fn state(&self) -> Result<FormState> {
        self.0.document.with_form_core(|_| {
            self.record_changes()?;
            Ok(self.state_inner())
        })
    }
    pub fn fields(&self, page: usize) -> Result<Vec<FormField>> {
        self.0.document.with_form_core(|_| {
            let view = self.view(page)?;
            Ok(view
                .annot_list()
                .iter()
                .enumerate()
                .filter_map(|(i, a)| self.field(a, i))
                .collect())
        })
    }
    pub fn take_events(&self) -> Vec<FormEvent> {
        self.publish_script_errors();
        self.0.host.events.borrow_mut().drain(..).collect()
    }
    pub fn input(&self, page: usize, input: FormInput) -> Result<FormInputResult> {
        if let FormInput::MouseMove(p, _)
        | FormInput::MouseDown(p, _)
        | FormInput::MouseUp(p, _)
        | FormInput::DoubleClick(p, _)
        | FormInput::MouseWheel(p, _, _) = &input
        {
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err(Error::InvalidArgument(
                    "form input coordinates must be finite",
                ));
            }
        }
        if let FormInput::MouseWheel(_, delta, _) = &input {
            if !delta.x.is_finite() || !delta.y.is_finite() {
                return Err(Error::InvalidArgument("form wheel delta must be finite"));
            }
        }
        self.0.document.with_form_core(|_| {
            self.0.host.current.set(page);
            let view = self.view(page)?;
            let p = |p: Point| CfxPointF::new(p.x, p.y);
            let _event = self.0.env.begin_input_event();
            let handled = match input {
                FormInput::MouseMove(point, m) => view.on_mouse_move(m.flags(), p(point)),
                FormInput::MouseDown(point, m) => view.on_l_button_down(m.flags(), p(point)),
                FormInput::MouseUp(point, m) => view.on_l_button_up(m.flags(), p(point)),
                FormInput::DoubleClick(point, m) => view.on_l_button_dbl_clk(m.flags(), p(point)),
                FormInput::MouseWheel(point, delta, m) => {
                    view.on_mouse_wheel(m.flags(), p(point), p(delta))
                }
                FormInput::KeyDown(key, m) => view.on_key_down(key as i32, m.flags()),
                FormInput::Text(value, m) => {
                    let mut handled = false;
                    for ch in value.chars() {
                        handled |= view.on_char(ch as u32, m.flags());
                    }
                    handled
                }
                FormInput::SelectAll => view.select_all_text(),
                FormInput::SelectWord => self.select_word(&view),
                FormInput::ReplaceSelection(value) => {
                    view.replace_selection(&value.chars().map(u32::from).collect::<Vec<_>>());
                    true
                }
                FormInput::Undo => view.undo(),
                FormInput::Redo => view.redo(),
                FormInput::Blur => self.0.env.kill_focus_annot(0),
            };
            self.record_changes()?;
            self.publish_script_errors();
            Ok(FormInputResult {
                handled,
                state: self.state_inner(),
                events: self.take_events(),
            })
        })
    }
    pub fn commit(&self) -> Result<()> {
        self.0.document.with_form_core(|_| {
            let focused = self.0.env.get_focus_annot().is_some();
            let accepted = self.0.env.kill_focus_annot(0);
            if focused && !accepted {
                return Err(Error::Edit);
            }
            self.record_changes()?;
            Ok(())
        })
    }
    pub(crate) fn saved(&self) {
        self.0.env.clear_change_mark();
        self.0.change_generation.set(self.0.env.change_generation());
    }
    pub fn next_timer_deadline(&self) -> Option<Instant> {
        self.0
            .host
            .timers
            .borrow()
            .values()
            .map(|t| t.deadline)
            .min()
    }
    pub fn run_due_timers(&self, now: Instant) -> Result<Vec<FormEvent>> {
        self.0.document.with_form_core(|_| {
            let callbacks: Vec<_> = {
                let mut timers = self.0.host.timers.borrow_mut();
                timers
                    .iter_mut()
                    .filter_map(|(&id, t)| {
                        if t.deadline <= now {
                            t.deadline = now + t.interval;
                            Some((id, t.callback.clone()))
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            for (id, callback) in callbacks {
                let present = self.0.host.timers.borrow().contains_key(&id);
                if present {
                    callback(id);
                }
            }
            self.record_changes()?;
            Ok(self.take_events())
        })
    }
    pub fn is_xfa(&self) -> bool {
        #[cfg(feature = "xfa")]
        {
            self.0.xfa.is_some()
        }
        #[cfg(not(feature = "xfa"))]
        {
            false
        }
    }
    pub fn is_full_xfa(&self) -> bool {
        #[cfg(feature = "xfa")]
        {
            self.0
                .xfa
                .as_ref()
                .is_some_and(|c| c.contains_extension_full_form())
        }
        #[cfg(not(feature = "xfa"))]
        {
            false
        }
    }
    pub(crate) fn xfa_page_count(&self) -> Option<usize> {
        #[cfg(feature = "xfa")]
        {
            self.0
                .xfa
                .as_ref()
                .filter(|c| c.contains_extension_form())
                .map(|c| c.get_page_count().max(0) as usize)
        }
        #[cfg(not(feature = "xfa"))]
        {
            None
        }
    }
    pub fn page_sizes(&self) -> Result<Vec<crate::Size>> {
        self.0.document.with_form_core(|core| {
            let count = self
                .xfa_page_count()
                .unwrap_or_else(|| core.borrow().page_count());
            (0..count)
                .map(|i| {
                    let page = self.0.host.page(i).ok_or(Error::Format)?;
                    Ok(crate::Size {
                        width: page.width(),
                        height: page.height(),
                    })
                })
                .collect()
        })
    }
    pub fn page_matrix(&self, page: usize, config: &RenderConfig) -> Result<crate::Matrix> {
        self.0.document.with_form_core(|_| {
            let page = self.0.host.page(page).ok_or(Error::Format)?;
            let (_, _, display) = config.geometry(page.width(), page.height())?;
            let m = page.display_matrix_for_rect(display, config.rotation as i32);
            Ok(crate::Matrix {
                a: m.a,
                b: m.b,
                c: m.c,
                d: m.d,
                e: m.e,
                f: m.f,
            })
        })
    }
    pub(crate) fn xfa_pre_save(&self) {
        #[cfg(feature = "xfa")]
        if let Some(context) = &self.0.xfa {
            context.send_pre_save_to_xfa_doc();
        }
    }
    pub(crate) fn xfa_post_save(&self, success: bool) {
        #[cfg(feature = "xfa")]
        if let Some(context) = &self.0.xfa {
            let dirty = self.0.env.change_mark();
            context.send_post_save_to_xfa_doc();
            // Native clears XFA's mark even after writer failure. The safe API
            // must retain unsaved changes when no usable copy was produced.
            if !success && dirty {
                self.0.env.set_change_mark();
            }
        }
        #[cfg(not(feature = "xfa"))]
        let _ = success;
    }

    pub(crate) fn sync_xfa_data(&self) -> Result<()> {
        #[cfg(feature = "xfa")]
        if let Some(context) = &self.0.xfa {
            if !pdfoxium_core::fpdfsdk::fpdf_xfa_save::save_xfa_document_data(Some(
                context.as_ref(),
            )) {
                return Err(Error::Save);
            }
        }
        Ok(())
    }
    pub fn render(&self, page: usize, config: &RenderConfig) -> Result<Bitmap> {
        self.0.document.with_form_core(|_| Ok(()))?;
        let mut bitmap = if self.is_full_xfa() {
            let sdk = self.0.host.page(page).ok_or(Error::Format)?;
            config.empty_bitmap(sdk.width(), sdk.height())?
        } else {
            self.0.document.page(page)?.render(config)?
        };
        self.draw(page, &mut bitmap, config)?;
        Ok(bitmap)
    }
    pub(crate) fn draw(
        &self,
        page: usize,
        bitmap: &mut Bitmap,
        config: &RenderConfig,
    ) -> Result<()> {
        self.0.document.with_form_core(|_| {
            let view = self.view(page)?;
            let sdk = view.page().ok_or(Error::Stale)?;
            let (w, h, display) = config.geometry(sdk.width(), sdk.height())?;
            if (w, h) != (bitmap.width(), bitmap.height()) {
                return Err(Error::InvalidArgument(
                    "form bitmap dimensions do not match render configuration",
                ));
            }
            let matrix = sdk.display_matrix_for_rect(display, config.rotation as i32);
            let rgba = bitmap.format() == PixelFormat::Rgba8;
            let mut dib = CfxDibitmapData::from_buffer(
                w as i32,
                h as i32,
                FxdibFormat::Bgra,
                bitmap.stride as u32,
                &mut bitmap.data,
            )
            .ok_or(Error::Allocation)?;
            let mut driver = CfxAggDeviceDriver::new(&mut dib, rgba, None);
            let clip = FxRect::new(0, 0, w as i32, h as i32);
            driver.set_clip_rect(clip);
            let mut device = AggPwlDraw::new(&mut driver);
            // Normal annotations were already drawn by the base page renderer.
            view.page_view_on_draw(&mut device, matrix, false, clip);
            Ok(())
        })
    }
}

// PWL indices count character slots plus one per paragraph break. Its extracted
// text uses CRLF; collapse that pair before translating Unicode word boundaries.
fn word_selection(value: &[u32], caret: i32) -> (i32, i32) {
    use unicode_segmentation::UnicodeSegmentation;
    let mut previous_cr = false;
    let normalized: String = value
        .iter()
        .filter_map(|&v| {
            let skip = v == 10 && previous_cr;
            previous_cr = v == 13;
            (!skip).then(|| char::from_u32(v).unwrap_or(char::REPLACEMENT_CHARACTER))
        })
        .collect();
    if normalized.is_empty() || caret < 0 {
        return (0, 0);
    }
    let byte = normalized
        .char_indices()
        .nth(caret as usize)
        .map(|(b, _)| b)
        .unwrap_or_else(|| normalized.char_indices().last().unwrap().0);
    for (start, word) in normalized.split_word_bound_indices() {
        if (start..start + word.len()).contains(&byte) {
            let start = normalized[..start].chars().count();
            return match (
                i32::try_from(start),
                i32::try_from(start + word.chars().count()),
            ) {
                (Ok(start), Ok(end)) => (start, end),
                _ => (0, 0),
            };
        }
    }
    (0, 0)
}
#[cfg(test)]
mod tests {
    use super::word_selection;
    #[test]
    fn word_boundaries_translate_unicode_and_paragraph_slots() {
        let text: Vec<_> = "café test\r\nsecond word".chars().map(u32::from).collect();
        assert_eq!(word_selection(&text, 2), (0, 4));
        assert_eq!(word_selection(&text, 6), (5, 9));
        assert_eq!(word_selection(&text, 12), (10, 16));
        assert_eq!(word_selection(&text, 99), (17, 21));
        assert_eq!(word_selection(&[], 0), (0, 0));
    }
}
