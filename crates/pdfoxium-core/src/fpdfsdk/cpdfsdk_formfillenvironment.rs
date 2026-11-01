// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Ordinary host/lifecycle/action portions of
//! `fpdfsdk/cpdfsdk_formfillenvironment.{h,cpp}`. Host callbacks contain no raw
//! ABI pointers. Ordinary JavaScript events use the real Rust runtime;
//! exceptions remain explicit action errors. XFA integration remains open.

use super::cpdfsdk_annot::{CpdfSdkAnnot, SdkInputEvent};
use super::cpdfsdk_interactiveform::CpdfSdkInteractiveForm;
use super::cpdfsdk_pageview::{CpdfSdkPageView, SdkPage};
use super::cpdfsdk_widget::CpdfSdkWidget;
use super::formfiller::cffl_fieldaction::CfflFieldAction;
use super::formfiller::cffl_interactiveformfiller::CfflInteractiveFormFiller;
#[cfg(feature = "xfa")]
use super::fpdfxfa::{cpdfxfa_context::CpdfXfaContext, cpdfxfa_docenvironment::XfaSdkHost};
use super::pwl::ipwl_fillernotify::CursorStyle;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::get_direct_object;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfdoc::cpdf_aaction::{AActionType, CpdfAAction};
use crate::fpdfdoc::cpdf_action::{ActionType, CpdfAction};
use crate::fpdfdoc::cpdf_annot::Subtype;
use crate::fpdfdoc::cpdf_dest::CpdfDest;
use crate::fpdfdoc::cpdf_formfield::{CpdfFormField, FormFieldType};
use crate::fpdfdoc::cpdf_nametree::CpdfNameTree;
use crate::fxcrt::cfx_timer::TimerHandler;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF, FxRect};
#[cfg(feature = "js")]
use crate::fxcrt::fx_string::utf16_encode;
#[cfg(feature = "js")]
use crate::fxjs::cjs_runtime::CjsRuntime;
#[cfg(feature = "js")]
use crate::fxjs::ijs_event_context::JsError;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionError {
    Detached,
    JavaScriptException,
    UnsupportedAction,
    HostUnavailable,
    CyclicAction,
    ActionDepthExceeded,
}
pub type ActionResult = Result<bool, ActionError>;

/// Optional FPDF_FORMFILLINFO services. False/None means that the embedder did
/// not provide that service, matching a missing function pointer. The FFI
/// adapter owns encodings, buffer lengths, version gates and temporary handles.
pub trait FormFillHost {
    #[cfg(feature = "xfa")]
    fn xfa_host(&self) -> Option<Rc<dyn XfaSdkHost>> {
        None
    }
    /// Native embedders opt into JavaScript through m_pJsPlatform. Ordinary
    /// Rust hosts have the runtime enabled unless they explicitly disable it.
    fn javascript_enabled(&self) -> bool {
        true
    }
    /// Optional Boa per-loop iteration bound for embedders that need bounded
    /// script loops. None preserves the native host's unrestricted behavior.
    fn javascript_loop_limit(&self) -> Option<u64> {
        None
    }
    fn invalidate(&self, _page: &SdkPage, _rect: FxRect) -> bool {
        false
    }
    fn output_selected_rect(&self, _page: &SdkPage, _rect: CfxFloatRect) -> bool {
        false
    }
    fn is_selection_implemented(&self) -> bool {
        false
    }
    fn set_cursor(&self, _cursor: CursorStyle) -> bool {
        false
    }
    fn set_timer(&self, _interval: i32, _callback: Rc<dyn Fn(i32)>) -> i32 {
        0
    }
    fn kill_timer(&self, _id: i32) {}
    fn on_change(&self) -> bool {
        false
    }
    fn set_text_field_focus(&self, _text: &[u32], _focused: bool) -> bool {
        false
    }
    fn on_focus_change(&self, _annot: &Rc<CpdfSdkAnnot>, _page_index: i32) -> bool {
        false
    }
    fn get_page(&self, _index: i32) -> Option<Rc<SdkPage>> {
        None
    }
    fn current_page(&self) -> Option<Rc<SdkPage>> {
        None
    }
    fn js_app_alert(&self, _message: &[u32], _title: &[u32], _kind: i32, _icon: i32) -> i32 {
        -1
    }
    fn js_app_beep(&self, _kind: i32) {}
    fn js_app_response(
        &self,
        _question: &[u32],
        _title: &[u32],
        _default: &[u32],
        _label: &[u32],
        _password: bool,
        _response: &mut [u8],
    ) -> i32 {
        -1
    }
    fn js_field_browse(&self) -> Vec<u32> {
        Vec::new()
    }
    fn js_doc_mail_form(
        &self,
        _data: &[u8],
        _ui: bool,
        _to: &[u32],
        _subject: &[u32],
        _cc: &[u32],
        _bcc: &[u32],
        _message: &[u32],
    ) {
    }
    fn js_doc_print(
        &self,
        _ui: bool,
        _start: i32,
        _end: i32,
        _silent: bool,
        _shrink_to_fit: bool,
        _as_image: bool,
        _reverse: bool,
        _annotations: bool,
    ) {
    }
    fn js_doc_goto_page(&self, _index: i32) -> bool {
        false
    }
    fn get_platform(&self) -> Vec<u32> {
        Vec::new()
    }
    fn get_language(&self) -> Vec<u32> {
        Vec::new()
    }
    fn execute_named_action(&self, _name: &[u8]) -> bool {
        false
    }
    fn do_uri_action(&self, _uri: &[u8], _modifiers: u32) -> bool {
        false
    }
    fn do_go_to_action(&self, _page_index: i32, _zoom_mode: i32, _positions: &[f32]) -> bool {
        false
    }
    fn file_path(&self) -> Option<Vec<u32>> {
        None
    }
    fn submit_form(&self, _bytes: &[u8], _destination: &[u32]) -> bool {
        false
    }
    fn release(&self) {}
}

struct PassiveCallback<'a> {
    flag: &'a Cell<bool>,
    previous: bool,
}
impl<'a> PassiveCallback<'a> {
    fn new(flag: &'a Cell<bool>) -> Self {
        Self {
            flag,
            previous: flag.replace(true),
        }
    }
}
impl Drop for PassiveCallback<'_> {
    fn drop(&mut self) {
        self.flag.set(self.previous);
    }
}

/// Nesting preserves an enclosing event's error through reentrant callbacks.
pub struct SdkInputEventGuard<'a> {
    depth: &'a Cell<usize>,
}
impl Drop for SdkInputEventGuard<'_> {
    fn drop(&mut self) {
        self.depth.set(self.depth.get() - 1);
    }
}

pub struct CpdfSdkFormFillEnvironment {
    self_weak: Weak<Self>,
    document: RefCell<Weak<RefCell<CpdfDocument>>>,
    host: Option<Weak<dyn FormFillHost>>,
    page_map: RefCell<BTreeMap<u64, Rc<CpdfSdkPageView>>>,
    interactive_form: Rc<CpdfSdkInteractiveForm>,
    interactive_form_filler: Rc<CfflInteractiveFormFiller>,
    focus_annot: RefCell<Option<Weak<CpdfSdkAnnot>>>,
    #[cfg(feature = "xfa")]
    xfa_context: RefCell<Weak<CpdfXfaContext>>,
    focusable_annot_types: RefCell<Vec<i32>>,
    change_mark: Cell<bool>,
    change_generation: Cell<u64>,
    being_destroyed: Cell<bool>,
    detaching: Cell<bool>,
    detached: Cell<bool>,
    in_passive_ffi_callback: Cell<bool>,
    released: Cell<bool>,
    active_timers: RefCell<BTreeSet<i32>>,
    action_error: Cell<Option<ActionError>>,
    #[cfg(feature = "js")]
    runtime: RefCell<Option<Rc<CjsRuntime>>>,
    #[cfg(feature = "js")]
    last_js_error: RefCell<Option<JsError>>,
    input_event_depth: Cell<usize>,
}

impl CpdfSdkFormFillEnvironment {
    pub fn new(
        document: &Rc<RefCell<CpdfDocument>>,
        host: Option<&Rc<dyn FormFillHost>>,
    ) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            self_weak: this.clone(),
            document: RefCell::new(Rc::downgrade(document)),
            host: host.map(Rc::downgrade),
            page_map: RefCell::new(BTreeMap::new()),
            interactive_form: CpdfSdkInteractiveForm::new(this.clone(), document),
            interactive_form_filler: CfflInteractiveFormFiller::new(this.clone()),
            focus_annot: RefCell::new(None),
            #[cfg(feature = "xfa")]
            xfa_context: RefCell::new(Weak::new()),
            focusable_annot_types: RefCell::new(vec![Subtype::Widget as i32]),
            change_mark: Cell::new(false),
            change_generation: Cell::new(0),
            being_destroyed: Cell::new(false),
            detaching: Cell::new(false),
            detached: Cell::new(false),
            in_passive_ffi_callback: Cell::new(false),
            released: Cell::new(false),
            active_timers: RefCell::new(BTreeSet::new()),
            action_error: Cell::new(None),
            #[cfg(feature = "js")]
            runtime: RefCell::new(None),
            #[cfg(feature = "js")]
            last_js_error: RefCell::new(None),
            input_event_depth: Cell::new(0),
        })
    }
    #[cfg(feature = "xfa")]
    pub fn xfa_context(&self) -> Option<Rc<CpdfXfaContext>> {
        self.xfa_context.borrow().upgrade()
    }
    #[cfg(feature = "xfa")]
    pub fn set_xfa_context(&self, context: Option<&Rc<CpdfXfaContext>>) {
        *self.xfa_context.borrow_mut() = context.map_or_else(Weak::new, Rc::downgrade);
    }
    pub fn page_views(&self) -> Vec<Rc<CpdfSdkPageView>> {
        self.views()
    }
    pub fn document(&self) -> Option<Rc<RefCell<CpdfDocument>>> {
        self.document.borrow().upgrade()
    }
    /// Snapshot a runtime already owned by this live document without starting
    /// JavaScript merely to read incidental metadata.
    #[cfg(feature = "js")]
    pub fn existing_runtime(&self) -> Option<Rc<CjsRuntime>> {
        self.is_alive()
            .then(|| self.runtime.borrow().clone())
            .flatten()
    }
    /// Lazy document runtime, matching GetIJSRuntime. End slot borrows before
    /// constructing the engine or invoking a script/host callback.
    #[cfg(feature = "js")]
    pub fn get_or_create_runtime(&self) -> Result<Rc<CjsRuntime>, ActionError> {
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        let existing = self.runtime.borrow().clone();
        if let Some(runtime) = existing {
            return Ok(runtime);
        }
        let created =
            CjsRuntime::new(self.self_weak.clone()).map_err(|error| self.record_js_error(error))?;
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        let selected = {
            let mut slot = self.runtime.borrow_mut();
            slot.get_or_insert_with(|| created.clone()).clone()
        };
        Ok(selected)
    }
    /// First concrete exception for the current outer input operation.
    #[cfg(feature = "js")]
    pub fn last_js_error(&self) -> Option<JsError> {
        self.last_js_error.borrow().clone()
    }
    #[cfg(feature = "js")]
    pub fn take_last_js_error(&self) -> Option<JsError> {
        self.last_js_error.borrow_mut().take()
    }
    #[cfg(feature = "js")]
    pub fn record_js_error(&self, error: JsError) -> ActionError {
        if self.last_js_error.borrow().is_none() {
            *self.last_js_error.borrow_mut() = Some(error);
        }
        self.record_action_error(ActionError::JavaScriptException);
        ActionError::JavaScriptException
    }
    pub fn is_alive(&self) -> bool {
        !self.detached.get() && self.document().is_some()
    }
    pub fn is_javascript_enabled(&self) -> bool {
        // The host may reference embedder state freed during Release().
        cfg!(feature = "js")
            && self.is_alive()
            && self.host().is_none_or(|host| host.javascript_enabled())
    }
    pub fn is_being_destroyed(&self) -> bool {
        self.being_destroyed.get()
    }
    pub fn host(&self) -> Option<Rc<dyn FormFillHost>> {
        self.host.as_ref().and_then(Weak::upgrade)
    }
    pub fn in_passive_ffi_callback(&self) -> bool {
        self.in_passive_ffi_callback.get()
    }
    pub fn interactive_form(&self) -> Rc<CpdfSdkInteractiveForm> {
        self.interactive_form.clone()
    }
    pub fn get_interactive_form(&self) -> Rc<CpdfSdkInteractiveForm> {
        self.interactive_form()
    }
    pub fn get_interactive_form_filler(&self) -> Rc<CfflInteractiveFormFiller> {
        self.interactive_form_filler.clone()
    }
    pub fn interactive_form_filler(&self) -> Rc<CfflInteractiveFormFiller> {
        self.get_interactive_form_filler()
    }
    pub fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        Some(self.self_weak.upgrade()? as Rc<dyn TimerHandler>)
    }
    pub fn get_timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        self.timer_handler()
    }
    pub fn change_mark(&self) -> bool {
        self.change_mark.get()
    }
    pub fn get_change_mark(&self) -> bool {
        self.change_mark()
    }
    pub fn set_change_mark(&self) {
        self.change_mark.set(true);
        self.change_generation
            .set(self.change_generation.get().wrapping_add(1));
    }
    /// Monotonic edit notification independent of the persistent dirty flag.
    /// Script bindings can set that flag without calling FFI_OnChange.
    pub fn change_generation(&self) -> u64 {
        self.change_generation.get()
    }
    pub fn clear_change_mark(&self) {
        self.change_mark.set(false);
    }
    /// Reset only when a new outer input operation starts. Keep its first error
    /// available to the caller after all nested callbacks have returned.
    pub fn begin_input_event(&self) -> SdkInputEventGuard<'_> {
        let depth = self.input_event_depth.get();
        if depth == 0 {
            self.clear_action_error();
            #[cfg(feature = "js")]
            self.last_js_error.borrow_mut().take();
        }
        self.input_event_depth
            .set(depth.checked_add(1).expect("input event nesting exhausted"));
        SdkInputEventGuard {
            depth: &self.input_event_depth,
        }
    }
    pub fn run_input_event<R>(&self, operation: impl FnOnce() -> R) -> R {
        let _event = self.begin_input_event();
        operation()
    }
    pub fn record_action_error(&self, error: ActionError) {
        if self.action_error.get().is_none() {
            self.action_error.set(Some(error));
        }
    }
    pub fn action_error(&self) -> Option<ActionError> {
        self.action_error.get()
    }
    pub fn take_action_error(&self) -> Option<ActionError> {
        self.action_error.take()
    }
    pub fn clear_action_error(&self) {
        self.action_error.set(None);
    }
    pub fn focusable_annot_subtypes(&self) -> Vec<Subtype> {
        self.focusable_annot_types
            .borrow()
            .iter()
            .filter_map(|&value| Subtype::from_i32(value))
            .collect()
    }
    pub fn set_focusable_annot_subtypes(&self, types: &[Subtype]) {
        self.set_focusable_annot_subtype_values(
            &types.iter().map(|&kind| kind as i32).collect::<Vec<_>>(),
        );
    }
    /// Preserve unrecognized public values for ABI roundtripping without
    /// constructing invalid enum variants. Such values cannot match an annot.
    pub fn focusable_annot_subtype_values(&self) -> Vec<i32> {
        self.focusable_annot_types.borrow().clone()
    }
    pub fn set_focusable_annot_subtype_values(&self, types: &[i32]) {
        *self.focusable_annot_types.borrow_mut() = types.to_vec();
    }
    pub fn get_page_count(&self) -> i32 {
        #[cfg(feature = "xfa")]
        if let Some(context) = self
            .xfa_context()
            .filter(|ctx| ctx.contains_extension_full_form())
        {
            return context.get_page_count();
        }
        self.document()
            .map_or(0, |d| d.borrow().page_count().min(i32::MAX as usize) as i32)
    }
    pub fn has_permissions(&self, flags: u32) -> bool {
        self.document()
            .is_some_and(|d| d.borrow().user_permissions(true) & flags != 0)
    }
    fn views(&self) -> Vec<Rc<CpdfSdkPageView>> {
        self.page_map.borrow().values().cloned().collect()
    }
    pub fn get_page_view(&self, page: &SdkPage) -> Option<Rc<CpdfSdkPageView>> {
        self.page_map.borrow().get(&page.id()).cloned()
    }
    pub fn get_or_create_page_view(&self, page: &Rc<SdkPage>) -> Option<Rc<CpdfSdkPageView>> {
        if !self.is_alive() || self.being_destroyed.get() || !page.is_alive() {
            return None;
        }
        if let Some(existing) = self.get_page_view(page) {
            return Some(existing);
        }
        let this = self.self_weak.upgrade()?;
        let view = CpdfSdkPageView::new(&this, page);
        // Insert before loading annotations: field/widget discovery can reenter.
        self.page_map.borrow_mut().insert(page.id(), view.clone());
        view.load_fx_annots();
        (self.is_alive() && view.is_alive()).then_some(view)
    }
    pub fn get_page_view_at_index(&self, index: i32) -> Option<Rc<CpdfSdkPageView>> {
        if !self.is_alive() {
            return None;
        }
        let page = self.host().and_then(|host| host.get_page(index));
        if let Some(page) = page {
            return self.get_page_view(&page);
        }
        self.views()
            .into_iter()
            .find(|view| view.page_index() == index)
    }
    pub fn remove_page_view(&self, page: &SdkPage) {
        let Some(view) = self.get_page_view(page) else {
            return;
        };
        if view.is_locked() || view.is_being_destroyed() {
            return;
        }
        view.set_being_destroyed();
        if self
            .get_focus_annot()
            .as_ref()
            .is_some_and(|annot| view.is_valid_sdk_annot(annot))
        {
            self.kill_focus_annot(0);
        }
        let removed = self.page_map.borrow_mut().remove(&page.id());
        if let Some(view) = removed {
            view.destroy();
        }
    }
    /// A closed embedder page cannot defer teardown behind the loading lock.
    pub fn on_page_closed(&self, page: &SdkPage) {
        let removed = self.page_map.borrow_mut().remove(&page.id());
        if let Some(view) = removed {
            view.destroy();
        }
        // Logical page invalidation makes an outstanding focus handle dead.
        let dead_focus = self
            .focus_annot
            .borrow()
            .as_ref()
            .is_some_and(|annot| annot.upgrade().is_none_or(|annot| !annot.is_alive()));
        if dead_focus {
            self.focus_annot.borrow_mut().take();
        }
    }
    pub fn get_focus_annot(&self) -> Option<Rc<CpdfSdkAnnot>> {
        self.focus_annot
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .filter(|annot| annot.is_alive())
    }
    pub fn set_focus_annot(&self, annot: &Rc<CpdfSdkAnnot>) -> bool {
        let _event = self.begin_input_event();
        if !self.is_alive() || self.being_destroyed.get() {
            return false;
        }
        if self
            .get_focus_annot()
            .is_some_and(|current| Rc::ptr_eq(&current, annot))
        {
            return true;
        }
        if self.get_focus_annot().is_some() && !self.kill_focus_annot(0) {
            return false;
        }
        if !annot.is_alive()
            || !annot.page_view().is_some_and(|view| view.is_valid())
            || self.get_focus_annot().is_some()
        {
            return false;
        }
        #[cfg(feature = "xfa")]
        if annot.xfa_widget().is_some_and(|xfa| xfa.on_changed_focus()) {
            return false;
        }
        if !annot.is_alive()
            || !annot.on_set_focus(0)
            || !self.is_alive()
            || !annot.is_alive()
            || self.get_focus_annot().is_some()
        {
            return false;
        }
        *self.focus_annot.borrow_mut() = Some(Rc::downgrade(annot));
        self.send_on_focus_change(annot);
        annot.is_alive() && self.is_alive()
    }
    pub fn kill_focus_annot(&self, flags: u32) -> bool {
        let _event = self.begin_input_event();
        let Some(annot) = self.get_focus_annot() else {
            return false;
        };
        self.focus_annot.borrow_mut().take();
        if !annot.on_kill_focus(flags) {
            // A callback may have focused another annotation. Do not overwrite it.
            if annot.is_alive() && self.get_focus_annot().is_none() {
                *self.focus_annot.borrow_mut() = Some(Rc::downgrade(&annot));
            }
            return false;
        }
        if !annot.is_alive() {
            return false;
        }
        if annot.widget().is_some_and(|widget| {
            matches!(
                widget.field_type(),
                FormFieldType::TextField | FormFieldType::ComboBox
            )
        }) {
            self.on_set_field_input_focus_internal(&[], false);
        }
        self.get_focus_annot().is_none()
    }
    pub fn clear_all_focused_annots(&self) {
        for view in self.views() {
            if self
                .get_focus_annot()
                .as_ref()
                .is_some_and(|annot| view.is_valid_sdk_annot(annot))
            {
                self.kill_focus_annot(0);
            }
        }
    }
    pub fn on_detach_from_document(&self) {
        if self.detached.get() || self.detaching.replace(true) {
            return;
        }
        // A callback may close the public form handle while this cleanup is
        // active. Retain its adapter until deferred Release has completed.
        let host = self.host();
        #[cfg(feature = "xfa")]
        let xfa = self.xfa_context.borrow_mut().upgrade();
        #[cfg(feature = "xfa")]
        {
            *self.xfa_context.borrow_mut() = Weak::new();
        }
        #[cfg(feature = "xfa")]
        if let Some(xfa) = xfa {
            if let Some(this) = self.self_weak.upgrade() {
                if xfa
                    .form_fill_env()
                    .is_some_and(|env| Rc::ptr_eq(&env, &this))
                {
                    xfa.set_form_fill_env(None);
                }
            }
        }
        self.clear_all_focused_annots();
        let views = std::mem::take(&mut *self.page_map.borrow_mut());
        for view in views.into_values() {
            view.destroy();
        }
        self.interactive_form_filler.shutdown();
        self.focus_annot.borrow_mut().take();
        #[cfg(feature = "js")]
        let runtime = self.runtime.borrow_mut().take();
        self.document.borrow_mut().clone_from(&Weak::new());
        self.detached.set(true);
        #[cfg(feature = "js")]
        if let Some(runtime) = runtime.as_ref() {
            runtime.shutdown();
        }
        // Cancel remaining host IDs before Release, including non-JS timers.
        self.stop_host_timers();
        #[cfg(feature = "js")]
        drop(runtime);
        self.detaching.set(false);
        if self.being_destroyed.get() {
            self.shutdown();
        }
        drop(host);
    }
    pub fn shutdown(&self) {
        if self.released.get() {
            return;
        }
        self.being_destroyed.set(true);
        // The outer detach owns callback cleanup. Reentrant close requests
        // must not release its callback table before cleanup returns.
        if self.detaching.get() {
            return;
        }
        let host = self.host();
        self.on_detach_from_document();
        if self.released.get() {
            return;
        }
        self.stop_host_timers();
        // Mark before calling Release, which can reenter the C API.
        if self.released.replace(true) {
            return;
        }
        if let Some(host) = host {
            host.release();
        }
    }
    fn stop_host_timers(&self) {
        let ids = std::mem::take(&mut *self.active_timers.borrow_mut());
        if let Some(host) = self.host() {
            for id in ids {
                host.kill_timer(id);
            }
        }
    }
    pub fn dispatch_annot_event(&self, annot: &Rc<CpdfSdkAnnot>, event: SdkInputEvent) -> bool {
        let _event = self.begin_input_event();
        if !self.is_alive() || self.being_destroyed.get() || !annot.is_alive() {
            return false;
        }
        self.interactive_form_filler
            .dispatch_annot_event(annot, event)
            && self.action_error.get().is_none()
    }
    pub fn update_all_views(&self, annot: &Rc<CpdfSdkAnnot>) {
        for view in self.views() {
            if self.is_alive() {
                view.update_view(annot);
            }
        }
    }
    pub fn invalidate_page(&self, page: &SdkPage, rect: FxRect) {
        if !self.is_alive() || !page.is_alive() {
            return;
        }
        if let Some(host) = self.host() {
            let _passive = PassiveCallback::new(&self.in_passive_ffi_callback);
            host.invalidate(page, rect);
        }
    }
    pub fn invalidate_rect(&self, widget: &CpdfSdkWidget, rect: CfxFloatRect) {
        let Some(view) = widget.page_view() else {
            return;
        };
        let Some(page) = view.page() else {
            return;
        };
        let inverse = view.current_matrix().get_inverse();
        let a = inverse.transform(CfxPointF::new(rect.left, rect.top));
        let b = inverse.transform(CfxPointF::new(rect.right, rect.bottom));
        let mut pdf = CfxFloatRect::new(a.x, b.y, b.x, a.y);
        pdf.normalize();
        self.invalidate_page(&page, pdf.get_outer_rect());
    }
    pub fn invalidate_widget(&self, widget: &CpdfSdkWidget) {
        if let Some(page) = widget.page_view().and_then(|view| view.page()) {
            self.invalidate_page(&page, widget.view_bbox().get_outer_rect());
        }
    }
    /// The formfield caller supplies its PWLtoFFL-transformed rectangle.
    pub fn output_selected_rect(&self, widget: &CpdfSdkWidget, rect: CfxFloatRect) {
        let Some(view) = widget.page_view() else {
            return;
        };
        let Some(page) = view.page() else {
            return;
        };
        if let Some(host) = self.host() {
            let _passive = PassiveCallback::new(&self.in_passive_ffi_callback);
            host.output_selected_rect(&page, rect);
        }
    }
    pub fn is_selection_implemented(&self) -> bool {
        self.host()
            .is_some_and(|host| host.is_selection_implemented())
    }
    pub fn set_cursor(&self, cursor: CursorStyle) {
        if let Some(host) = self.host() {
            let _passive = PassiveCallback::new(&self.in_passive_ffi_callback);
            host.set_cursor(cursor);
        }
    }
    pub fn on_change(&self) {
        if let Some(host) = self.host() {
            let _passive = PassiveCallback::new(&self.in_passive_ffi_callback);
            host.on_change();
        }
    }
    pub fn on_set_field_input_focus(&self, text: &[u32]) {
        self.on_set_field_input_focus_internal(text, true);
    }
    fn on_set_field_input_focus_internal(&self, text: &[u32], focused: bool) {
        if let Some(host) = self.host() {
            let _passive = PassiveCallback::new(&self.in_passive_ffi_callback);
            host.set_text_field_focus(text, focused);
        }
    }
    fn send_on_focus_change(&self, annot: &Rc<CpdfSdkAnnot>) {
        let Some(view) = annot.page_view().filter(|view| view.is_valid()) else {
            return;
        };
        if view.page().is_none() {
            return;
        }
        let index = view.page_index();
        if let Some(host) = self.host() {
            let _passive = PassiveCallback::new(&self.in_passive_ffi_callback);
            host.on_focus_change(annot, index);
        }
    }
    pub fn on_calculate(&self, annot: &Rc<CpdfSdkAnnot>) {
        if let Some(field) = annot.widget().and_then(|widget| widget.field()) {
            if let Err(error) = self.interactive_form.on_calculate(Some(&field)) {
                self.record_action_error(error);
            }
        }
    }
    pub fn on_format(&self, annot: &Rc<CpdfSdkAnnot>) {
        let Some(field) = annot.widget().and_then(|widget| widget.field()) else {
            return;
        };
        match self.interactive_form.on_format(&field) {
            Ok(Some(value)) if annot.is_alive() => {
                self.interactive_form
                    .reset_field_appearance(&field, Some(&value));
                self.interactive_form.update_field(&field);
            }
            Err(error) => self.record_action_error(error),
            _ => {}
        }
    }
    pub fn file_path(&self) -> Vec<u32> {
        if !self.is_alive() {
            return Vec::new();
        }
        self.host()
            .and_then(|host| host.file_path())
            .unwrap_or_default()
    }
    pub fn get_file_path(&self) -> Vec<u32> {
        self.file_path()
    }
    pub fn current_page_index(&self) -> Option<i32> {
        if !self.is_alive() {
            return None;
        }
        let page = self.host()?.current_page()?;
        (self.is_alive() && page.is_alive()).then(|| page.index())
    }
    pub fn js_app_alert(&self, message: &[u32], title: &[u32], kind: i32, icon: i32) -> i32 {
        if !self.is_alive() {
            return -1;
        }
        self.host()
            .map_or(-1, |host| host.js_app_alert(message, title, kind, icon))
    }
    pub fn js_app_beep(&self, kind: i32) {
        if self.is_alive() {
            if let Some(host) = self.host() {
                host.js_app_beep(kind);
            }
        }
    }
    pub fn js_app_response(
        &self,
        question: &[u32],
        title: &[u32],
        default: &[u32],
        label: &[u32],
        password: bool,
        response: &mut [u8],
    ) -> i32 {
        if !self.is_alive() {
            return -1;
        }
        self.host().map_or(-1, |host| {
            host.js_app_response(question, title, default, label, password, response)
        })
    }
    pub fn js_field_browse(&self) -> Vec<u32> {
        if !self.is_alive() {
            return Vec::new();
        }
        let result = self
            .host()
            .map_or_else(Vec::new, |host| host.js_field_browse());
        if self.is_alive() {
            result
        } else {
            Vec::new()
        }
    }
    pub fn js_doc_mail_form(
        &self,
        data: &[u8],
        ui: bool,
        to: &[u32],
        subject: &[u32],
        cc: &[u32],
        bcc: &[u32],
        message: &[u32],
    ) {
        if self.is_alive() {
            if let Some(host) = self.host() {
                host.js_doc_mail_form(data, ui, to, subject, cc, bcc, message);
            }
        }
    }
    pub fn js_doc_print(
        &self,
        ui: bool,
        start: i32,
        end: i32,
        silent: bool,
        shrink_to_fit: bool,
        as_image: bool,
        reverse: bool,
        annotations: bool,
    ) {
        if self.is_alive() {
            if let Some(host) = self.host() {
                host.js_doc_print(
                    ui,
                    start,
                    end,
                    silent,
                    shrink_to_fit,
                    as_image,
                    reverse,
                    annotations,
                );
            }
        }
    }
    pub fn js_doc_goto_page(&self, index: i32) -> ActionResult {
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        self.host()
            .filter(|host| host.js_doc_goto_page(index))
            .map(|_| true)
            .ok_or(ActionError::HostUnavailable)
    }
    pub fn get_platform(&self) -> Vec<u32> {
        if !self.is_alive() {
            return Vec::new();
        }
        self.host()
            .map_or_else(Vec::new, |host| host.get_platform())
    }
    pub fn get_language(&self) -> Vec<u32> {
        if !self.is_alive() {
            return Vec::new();
        }
        self.host()
            .map_or_else(Vec::new, |host| host.get_language())
    }
    pub fn submit_form(&self, bytes: &[u8], destination: &[u32]) -> ActionResult {
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        self.host()
            .filter(|host| host.submit_form(bytes, destination))
            .map(|_| true)
            .ok_or(ActionError::HostUnavailable)
    }
    pub fn execute_named_action(&self, name: &[u8]) -> ActionResult {
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        self.host()
            .filter(|host| host.execute_named_action(name))
            .map(|_| true)
            .ok_or(ActionError::HostUnavailable)
    }
    pub fn do_uri_action(&self, uri: &[u8], modifiers: u32) -> ActionResult {
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        self.host()
            .filter(|host| host.do_uri_action(uri, modifiers))
            .map(|_| true)
            .ok_or(ActionError::HostUnavailable)
    }
    pub fn do_go_to_action(&self, index: i32, mode: i32, positions: &[f32]) -> ActionResult {
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        self.host()
            .filter(|host| host.do_go_to_action(index, mode, positions))
            .map(|_| true)
            .ok_or(ActionError::HostUnavailable)
    }
    pub fn do_action_destination(&self, dest: &CpdfDest) -> ActionResult {
        let doc = self.document().ok_or(ActionError::Detached)?;
        let index = dest.dest_page_index(&mut doc.borrow_mut());
        let mode = dest.zoom_mode();
        let positions = dest.scroll_position_array();
        self.do_go_to_action(index, mode, &positions)
    }
    pub fn do_action_go_to(&self, action: &CpdfAction) -> ActionResult {
        let doc = self.document().ok_or(ActionError::Detached)?;
        let destination = action.dest(&doc.borrow());
        self.do_action_destination(&destination)
    }
    pub fn do_action_uri(&self, action: &CpdfAction, modifiers: u32) -> ActionResult {
        let doc = self.document().ok_or(ActionError::Detached)?;
        let uri = action.uri(&doc.borrow());
        self.do_uri_action(&uri, modifiers)
    }
    pub fn do_action_hide(&self, action: &CpdfAction) -> ActionResult {
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        let changed = self.interactive_form.do_action_hide(action);
        if changed {
            self.set_change_mark();
        }
        Ok(changed)
    }
    pub fn do_action_submit_form(&self, action: &CpdfAction) -> ActionResult {
        self.interactive_form.do_action_submit_form(action)
    }
    pub fn do_action_reset_form(&self, action: &CpdfAction) -> ActionResult {
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        self.interactive_form.do_action_reset_form(action);
        Ok(true)
    }
    pub fn do_action_link(
        &self,
        action: &CpdfAction,
        kind: AActionType,
        modifiers: u32,
    ) -> ActionResult {
        if !CpdfAAction::is_user_input(kind) {
            return Ok(false);
        }
        match action.action_type() {
            ActionType::SetOcgState => self.do_action_optional_content(action),
            ActionType::GoTo => self.do_action_go_to(action),
            ActionType::Uri => self.do_action_uri(action, modifiers),
            _ => Ok(false),
        }
    }
    fn do_action_optional_content(&self, action: &CpdfAction) -> ActionResult {
        let dictionary = action.dict().ok_or(ActionError::UnsupportedAction)?;
        let action = crate::fpdfapi::page::optional_content::parse_action(&dictionary)
            .ok_or(ActionError::UnsupportedAction)?;
        let document = self.document().ok_or(ActionError::Detached)?;
        let Some(state) = document.borrow().optional_content() else {
            return Ok(false);
        };
        let changed = state
            .borrow_mut()
            .apply_action(&action)
            .map_err(|_| ActionError::UnsupportedAction)?;
        if changed {
            for view in self.page_views() {
                if let Some(page) = view.page() {
                    if let Some(pdf) = page.geometry_page() {
                        self.invalidate_page(
                            &page,
                            CfxFloatRect::new(0., 0., pdf.width(), pdf.height()).get_outer_rect(),
                        );
                    }
                }
            }
        }
        Ok(changed)
    }
    pub fn do_action_no_js(&self, action: &CpdfAction, kind: AActionType) -> ActionResult {
        if !action.has_dict() {
            return Ok(false);
        }
        match action.action_type() {
            ActionType::SetOcgState => self.do_action_optional_content(action),
            ActionType::GoTo => self.do_action_go_to(action),
            ActionType::Uri if CpdfAAction::is_user_input(kind) => self.do_action_uri(action, 0),
            ActionType::Hide => self.do_action_hide(action),
            ActionType::Named => self.execute_named_action(&action.named_action()),
            ActionType::SubmitForm if CpdfAAction::is_user_input(kind) => {
                self.do_action_submit_form(action)
            }
            ActionType::ResetForm => self.do_action_reset_form(action),
            ActionType::Uri | ActionType::SubmitForm => Ok(false),
            ActionType::JavaScript => Err(ActionError::UnsupportedAction),
            _ => Err(ActionError::UnsupportedAction),
        }
    }
    fn execute_action_tree(
        &self,
        action: &CpdfAction,
        kind: AActionType,
        field: Option<&CpdfFormField>,
        mut data: Option<&mut CfflFieldAction>,
        visited: &mut HashSet<usize>,
        depth: usize,
    ) -> ActionResult {
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        let Some(dict) = action.dict() else {
            return Ok(false);
        };
        if depth >= 256 {
            return Err(ActionError::ActionDepthExceeded);
        }
        let id = Rc::as_ptr(&dict).cast::<()>() as usize;
        if !visited.insert(id) {
            return Err(ActionError::CyclicAction);
        }
        if action.action_type() == ActionType::JavaScript {
            #[cfg(feature = "js")]
            {
                let script = action.javascript();
                if !script.is_empty() && self.is_javascript_enabled() {
                    let runtime = self.get_or_create_runtime()?;
                    let script = utf16_encode(&script);
                    let result = if let Some(field) = field {
                        let Some(data) = data.as_deref_mut() else {
                            return Err(ActionError::UnsupportedAction);
                        };
                        runtime.run_field_script(field, kind, data, &script)
                    } else if kind == AActionType::DocumentOpen {
                        runtime.run_document_script(&[], &script)
                    } else {
                        runtime.run_document_page_script(kind, &script)
                    };
                    result.map_err(|error| self.record_js_error(error))?;
                    if !self.is_alive() {
                        return Err(ActionError::Detached);
                    }
                    if let Some(field) = field {
                        if self
                            .interactive_form
                            .core_form()
                            .get_field_by_dict(&field.field_dict())
                            .is_none()
                        {
                            return Ok(false);
                        }
                    }
                }
            }
        } else {
            self.do_action_no_js(action, kind)?;
        }
        for index in 0..action.sub_actions_count() {
            if !self.execute_action_tree(
                &action.sub_action(index),
                kind,
                field,
                data.as_deref_mut(),
                visited,
                depth + 1,
            )? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn do_action_doc_open(&self, action: &CpdfAction) -> ActionResult {
        self.execute_action_tree(
            action,
            AActionType::DocumentOpen,
            None,
            None,
            &mut HashSet::new(),
            0,
        )
    }
    pub fn do_action_page(&self, action: &CpdfAction, kind: AActionType) -> ActionResult {
        self.execute_action_tree(action, kind, None, None, &mut HashSet::new(), 0)
    }
    pub fn do_action_document(&self, action: &CpdfAction, kind: AActionType) -> ActionResult {
        self.do_action_page(action, kind)
    }
    pub fn do_action_field(
        &self,
        action: &CpdfAction,
        kind: AActionType,
        field: &CpdfFormField,
        data: &mut CfflFieldAction,
    ) -> ActionResult {
        let result = self.execute_action_tree(
            action,
            kind,
            Some(field),
            Some(&mut *data),
            &mut HashSet::new(),
            0,
        );
        if let Err(error) = result {
            data.b_rc = false;
            self.record_action_error(error);
        }
        result
    }
    pub fn do_action_field_javascript(
        &self,
        action: &CpdfAction,
        kind: AActionType,
        field: &CpdfFormField,
        data: &mut CfflFieldAction,
    ) -> ActionResult {
        #[cfg(not(feature = "js"))]
        let _ = (kind, field, &data);
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        if action.action_type() != ActionType::JavaScript || !self.is_javascript_enabled() {
            return Ok(false);
        }
        let script = action.javascript();
        if script.is_empty() {
            return Ok(false);
        }
        #[cfg(feature = "js")]
        {
            let result = self.get_or_create_runtime()?.run_field_script(
                field,
                kind,
                data,
                &utf16_encode(&script),
            );
            if let Err(error) = result {
                data.b_rc = false;
                return Err(self.record_js_error(error));
            }
            if !self.is_alive() {
                return Err(ActionError::Detached);
            }
        }
        Ok(true)
    }
    pub fn do_action_javascript(&self, action: &CpdfAction, name: &[u32]) -> ActionResult {
        #[cfg(not(feature = "js"))]
        let _ = name;
        if !self.is_alive() {
            return Err(ActionError::Detached);
        }
        if action.action_type() != ActionType::JavaScript || !self.is_javascript_enabled() {
            return Ok(false);
        }
        let script = action.javascript();
        if script.is_empty() {
            return Ok(false);
        }
        #[cfg(feature = "js")]
        {
            self.get_or_create_runtime()?
                .run_document_script(name, &utf16_encode(&script))
                .map_err(|error| self.record_js_error(error))?;
            if !self.is_alive() {
                return Err(ActionError::Detached);
            }
        }
        Ok(true)
    }
    pub fn proc_javascript_action(&self) -> Result<(), ActionError> {
        let document = self.document().ok_or(ActionError::Detached)?;
        let root = document.borrow().root();
        let Some(tree) = root.and_then(|root| CpdfNameTree::create_from_root(&root, b"JavaScript"))
        else {
            return Ok(());
        };
        for index in 0..tree.count() {
            if let Some((dict, name)) = tree.lookup_value_and_name(index) {
                // Each named script is an independent document action. A script
                // exception must not suppress subsequent entries in the name tree.
                if let Err(error) = self.do_action_javascript(&CpdfAction::new(Some(dict)), &name) {
                    if error != ActionError::JavaScriptException {
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }
    pub fn proc_open_action(&self) -> ActionResult {
        let document = self.document().ok_or(ActionError::Detached)?;
        let Some(root) = document.borrow().root() else {
            return Ok(false);
        };
        let raw = root
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .and_then(|dict| dict.get_object_for(b"OpenAction"));
        let Some(action) = raw.and_then(|object| get_direct_object(&object)) else {
            return Ok(false);
        };
        let kind = action.borrow().object_type();
        match kind {
            ObjectType::Array => Ok(true),
            ObjectType::Dictionary => self.do_action_doc_open(&CpdfAction::new(Some(action))),
            _ => Ok(false),
        }
    }
}
impl TimerHandler for CpdfSdkFormFillEnvironment {
    fn set_timer(&self, interval: i32, callback: Rc<dyn Fn(i32)>) -> i32 {
        if !self.is_alive() || self.being_destroyed.get() {
            return 0;
        }
        let Some(host) = self.host() else {
            return 0;
        };
        let id = host.set_timer(interval, callback);
        if id != 0 {
            if self.is_alive() && !self.being_destroyed.get() {
                self.active_timers.borrow_mut().insert(id);
            } else {
                host.kill_timer(id);
                return 0;
            }
        }
        id
    }
    fn kill_timer(&self, id: i32) {
        self.active_timers.borrow_mut().remove(&id);
        if let Some(host) = self.host() {
            host.kill_timer(id);
        }
    }
}
impl Drop for CpdfSdkFormFillEnvironment {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_page::CpdfPage;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_object::ObjectRef;
    use crate::fpdfapi::parser::cpdf_parser::ParserError;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    #[derive(Default)]
    struct Host {
        environment: RefCell<Weak<CpdfSdkFormFillEnvironment>>,
        named: RefCell<Vec<Vec<u8>>>,
        uri: RefCell<Vec<(Vec<u8>, u32)>>,
        destinations: RefCell<Vec<(i32, i32, Vec<f32>)>>,
        submits: RefCell<Vec<(Vec<u8>, Vec<u32>)>>,
        timers: RefCell<Vec<i32>>,
        released: Cell<usize>,
        passive_seen: Cell<bool>,
        javascript_disabled: Cell<bool>,
        javascript_checks: Cell<usize>,
    }
    impl FormFillHost for Host {
        fn javascript_enabled(&self) -> bool {
            self.javascript_checks.set(self.javascript_checks.get() + 1);
            !self.javascript_disabled.get()
        }
        fn execute_named_action(&self, name: &[u8]) -> bool {
            let env = self.environment.borrow().upgrade();
            if let Some(env) = env {
                env.set_change_mark();
                assert!(env.document().is_some());
            }
            self.named.borrow_mut().push(name.to_vec());
            true
        }
        fn do_uri_action(&self, uri: &[u8], modifiers: u32) -> bool {
            self.uri.borrow_mut().push((uri.to_vec(), modifiers));
            true
        }
        fn do_go_to_action(&self, page: i32, mode: i32, positions: &[f32]) -> bool {
            self.destinations
                .borrow_mut()
                .push((page, mode, positions.to_vec()));
            true
        }
        fn submit_form(&self, bytes: &[u8], destination: &[u32]) -> bool {
            self.submits
                .borrow_mut()
                .push((bytes.to_vec(), destination.to_vec()));
            true
        }
        fn set_cursor(&self, _: CursorStyle) -> bool {
            let env = self.environment.borrow().upgrade();
            if let Some(env) = env {
                self.passive_seen.set(env.in_passive_ffi_callback());
                env.clear_change_mark();
            }
            true
        }
        fn set_timer(&self, interval: i32, _callback: Rc<dyn Fn(i32)>) -> i32 {
            assert_eq!(interval, 500);
            42
        }
        fn kill_timer(&self, id: i32) {
            self.timers.borrow_mut().push(id);
        }
        fn release(&self) {
            self.released.set(self.released.get() + 1);
        }
    }
    fn fixture() -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<Host>,
        Rc<CpdfSdkFormFillEnvironment>,
    ) {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let host = Rc::new(Host::default());
        let iface: Rc<dyn FormFillHost> = host.clone();
        let env = CpdfSdkFormFillEnvironment::new(&document, Some(&iface));
        *host.environment.borrow_mut() = Rc::downgrade(&env);
        (document, host, env)
    }
    fn action(kind: &[u8]) -> (ObjectRef, CpdfAction) {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"S", kind);
        let dict = retain_object(dict);
        let action = CpdfAction::new(Some(dict.clone()));
        (dict, action)
    }
    fn with_dict<R>(dict: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
        let mut dict = dict.borrow_mut();
        f(dict.as_any_mut().downcast_mut::<CpdfDictionary>().unwrap())
    }
    #[test]
    #[cfg(feature = "js")]
    fn absent_actions_real_scripts_and_exceptions_are_distinct() {
        let (_doc, _host, env) = fixture();
        assert_eq!(env.do_action_doc_open(&CpdfAction::default()), Ok(false));
        assert_eq!(env.proc_open_action(), Ok(false));
        assert_eq!(env.proc_javascript_action(), Ok(()));
        let (dict, script) = action(b"JavaScript");
        with_dict(&dict, |d| {
            d.set_new_for(
                b"JS",
                CpdfString::from_bytes(None, b"globalThis.saved = 'changed';"),
            );
        });
        assert_eq!(env.do_action_doc_open(&script), Ok(true));
        with_dict(&dict, |d| {
            d.set_new_for(
                b"JS",
                CpdfString::from_bytes(
                    None,
                    b"if (globalThis.saved !== 'changed') throw new Error('lost runtime state');",
                ),
            );
        });
        assert_eq!(env.do_action_javascript(&script, &[]), Ok(true));
        #[cfg(feature = "js")]
        assert!(env.last_js_error().is_none());
        with_dict(&dict, |d| {
            d.set_new_for(
                b"JS",
                CpdfString::from_bytes(None, b"throw new Error('document failure');"),
            );
        });
        assert_eq!(
            env.run_input_event(|| env.do_action_doc_open(&script)),
            Err(ActionError::JavaScriptException)
        );
        assert!(
            String::from_utf16_lossy(&env.last_js_error().unwrap().message)
                .contains("document failure")
        );
        assert_eq!(env.action_error(), Some(ActionError::JavaScriptException));
        assert!(env.take_last_js_error().is_some());
        #[cfg(feature = "js")]
        assert!(env.last_js_error().is_none());
        with_dict(&dict, |d| {
            d.set_new_for(b"JS", CpdfString::from_bytes(None, b""));
        });
        assert_eq!(
            env.run_input_event(|| env.do_action_javascript(&script, &[])),
            Ok(false)
        );
        assert_eq!(env.action_error(), None);
        env.record_action_error(ActionError::JavaScriptException);
        env.record_action_error(ActionError::HostUnavailable);
        assert_eq!(
            env.take_action_error(),
            Some(ActionError::JavaScriptException)
        );
        assert_eq!(env.action_error(), None);
    }
    #[test]
    fn disabled_javascript_skips_scripts_but_preserves_next_actions_and_teardown() {
        let (_doc, host, env) = fixture();
        host.javascript_disabled.set(true);
        let (dict, script) = action(b"JavaScript");
        let (next, _) = action(b"Named");
        with_dict(&next, |d| {
            d.set_new_name_for(b"N", b"AfterSkippedScript");
        });
        with_dict(&dict, |d| {
            d.set_new_for(
                b"JS",
                CpdfString::from_bytes(None, b"throw new Error('disabled');"),
            );
            d.set_for(b"Next", Some(next));
        });
        assert_eq!(env.do_action_doc_open(&script), Ok(true));
        assert_eq!(*host.named.borrow(), [b"AfterSkippedScript".to_vec()]);
        assert_eq!(env.do_action_javascript(&script, &[]), Ok(false));
        #[cfg(feature = "js")]
        assert!(env.runtime.borrow().is_none());
        #[cfg(feature = "js")]
        assert!(env.last_js_error().is_none());
        // The source-facing gate does not prohibit explicitly creating a runtime.
        #[cfg(feature = "js")]
        assert!(env.get_or_create_runtime().is_ok());
        env.shutdown();
        let checks = host.javascript_checks.get();
        assert!(!env.is_javascript_enabled());
        assert_eq!(host.javascript_checks.get(), checks);
        #[cfg(feature = "js")]
        assert!(env.runtime.borrow().is_none());
    }
    #[test]
    #[cfg(not(feature = "js"))]
    fn compiled_out_javascript_skips_scripts_with_an_enabled_host_and_keeps_next_actions() {
        let (_doc, host, env) = fixture();
        assert!(!host.javascript_disabled.get());
        assert!(!env.is_javascript_enabled());
        assert_eq!(host.javascript_checks.get(), 0);
        let (dict, script) = action(b"JavaScript");
        let (next, _) = action(b"Named");
        with_dict(&dict, |dict| {
            dict.set_new_for(
                b"JS",
                CpdfString::from_bytes(None, b"throw Error('must not execute');"),
            );
            dict.set_for(b"Next", Some(next.clone()));
        });
        with_dict(&next, |dict| {
            dict.set_new_name_for(b"N", b"AfterCompiledOutScript")
        });
        assert_eq!(env.do_action_doc_open(&script), Ok(true));
        assert_eq!(*host.named.borrow(), [b"AfterCompiledOutScript".to_vec()]);
        assert_eq!(env.do_action_javascript(&script, &[]), Ok(false));
        assert_eq!(env.action_error(), None);
        assert_eq!(host.javascript_checks.get(), 0);
    }
    #[test]
    fn action_traversal_preserves_order_detects_cycles_and_releases_borrows() {
        let (doc, host, env) = fixture();
        let (first, a) = action(b"Named");
        let (second, _) = action(b"Named");
        with_dict(&first, |d| {
            d.set_new_name_for(b"N", b"First");
        });
        with_dict(&second, |d| {
            d.set_new_name_for(b"N", b"Second");
        });
        let holder = doc.borrow().holder();
        let n1 = holder.borrow_mut().add_indirect_object(first.clone());
        let n2 = holder.borrow_mut().add_indirect_object(second.clone());
        with_dict(&first, |d| {
            d.set_new_for(b"Next", CpdfReference::new(&holder, n2));
        });
        assert_eq!(env.do_action_doc_open(&a), Ok(true));
        assert_eq!(
            *host.named.borrow(),
            [b"First".to_vec(), b"Second".to_vec()]
        );
        assert!(env.change_mark());
        host.named.borrow_mut().clear();
        with_dict(&second, |d| {
            d.set_new_for(b"Next", CpdfReference::new(&holder, n1));
        });
        assert_eq!(env.do_action_doc_open(&a), Err(ActionError::CyclicAction));
        assert_eq!(
            *host.named.borrow(),
            [b"First".to_vec(), b"Second".to_vec()]
        );
        env.set_cursor(CursorStyle::Hand);
        assert!(host.passive_seen.get());
        assert!(!env.in_passive_ffi_callback());
    }
    #[test]
    fn non_script_actions_dispatch_host_data_and_respect_user_input() {
        let (_doc, host, env) = fixture();
        let (uri, action) = action(b"URI");
        with_dict(&uri, |d| {
            d.set_new_string_for(b"URI", b"https://example.test/");
        });
        assert_eq!(
            env.do_action_link(&action, AActionType::Validate, 7),
            Ok(false)
        );
        assert_eq!(
            env.do_action_link(&action, AActionType::ButtonUp, 7),
            Ok(true)
        );
        assert_eq!(*host.uri.borrow(), [(b"https://example.test/".to_vec(), 7)]);
        let mut array = CpdfArray::new();
        array.append_new(CpdfScalarObject::number_from_signed(3));
        array.append_new_name(b"XYZ");
        for n in [4, 5, 2] {
            array.append_new(CpdfScalarObject::number_from_signed(n));
        }
        assert_eq!(
            env.do_action_destination(&CpdfDest::new(Some(retain_object(array)))),
            Ok(true)
        );
        assert_eq!(*host.destinations.borrow(), [(3, 1, vec![4., 5., 2.])]);
        assert_eq!(env.submit_form(b"data", &[65, 66]), Ok(true));
        assert_eq!(*host.submits.borrow(), [(b"data".to_vec(), vec![65, 66])]);
    }
    #[test]
    fn shutdown_cancels_host_timers_releases_once_and_detaches_weak_document() {
        let (doc, host, env) = fixture();
        assert_eq!(env.set_timer(500, Rc::new(|_| {})), 42);
        env.shutdown();
        assert!(!env.is_alive());
        assert!(env.document().is_none());
        assert_eq!(*host.timers.borrow(), [42]);
        assert_eq!(host.released.get(), 1);
        assert_eq!(
            env.execute_named_action(b"NextPage"),
            Err(ActionError::Detached)
        );
        assert_eq!(env.set_timer(500, Rc::new(|_| {})), 0);
        env.shutdown();
        drop(env);
        assert_eq!(host.released.get(), 1);
        assert_eq!(*host.timers.borrow(), [42]);
        assert!(Rc::strong_count(&doc) >= 1);
    }

    #[test]
    fn reentrant_shutdown_from_focus_loss_defers_release_until_detach_cleanup_finishes() {
        #[derive(Default)]
        struct ClosingHost {
            environment: RefCell<Weak<CpdfSdkFormFillEnvironment>>,
            releases: Cell<usize>,
            after_release: Cell<usize>,
            next_timer: Cell<i32>,
            cleanup_timer: Cell<i32>,
            events: RefCell<Vec<&'static str>>,
        }
        impl ClosingHost {
            fn record(&self, event: &'static str) {
                if self.releases.get() != 0 {
                    self.after_release.set(self.after_release.get() + 1);
                }
                self.events.borrow_mut().push(event);
            }
        }
        impl FormFillHost for ClosingHost {
            fn javascript_enabled(&self) -> bool {
                self.record("javascript-enabled");
                true
            }
            fn invalidate(&self, _: &SdkPage, _: FxRect) -> bool {
                self.record("invalidate");
                true
            }
            fn set_cursor(&self, _: CursorStyle) -> bool {
                self.record("cursor");
                true
            }
            fn is_selection_implemented(&self) -> bool {
                self.record("selection-implemented");
                false
            }
            fn on_focus_change(&self, _: &Rc<CpdfSdkAnnot>, _: i32) -> bool {
                self.record("focus-change");
                true
            }
            fn set_text_field_focus(&self, _: &[u32], focused: bool) -> bool {
                self.record(if focused { "focus-in" } else { "focus-out" });
                if !focused {
                    let environment = self.environment.borrow().upgrade().unwrap();
                    assert!(environment.detaching.get());
                    assert_eq!(self.releases.get(), 0);
                    environment.shutdown();
                    assert_eq!(self.releases.get(), 0, "Release must wait for outer detach");
                }
                true
            }
            fn set_timer(&self, _: i32, _: Rc<dyn Fn(i32)>) -> i32 {
                self.record("set-timer");
                let id = self.next_timer.get() + 1;
                self.next_timer.set(id);
                id
            }
            fn kill_timer(&self, id: i32) {
                self.record(if id == self.cleanup_timer.get() {
                    "cleanup-timer"
                } else {
                    "kill-timer"
                });
            }
            fn release(&self) {
                self.record("release");
                let environment = self.environment.borrow().upgrade().unwrap();
                assert!(!environment.detaching.get());
                assert!(!environment.is_alive());
                self.releases.set(self.releases.get() + 1);
                environment.shutdown();
            }
        }
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        assert_eq!(
            document.borrow_mut().load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(
                    include_bytes!("../../../../upstream/pdfium/testing/resources/text_form.pdf")
                        .to_vec()
                )),
                b""
            ),
            ParserError::Success
        );
        let host = Rc::new(ClosingHost::default());
        let callbacks: Rc<dyn FormFillHost> = host.clone();
        let environment = CpdfSdkFormFillEnvironment::new(&document, Some(&callbacks));
        *host.environment.borrow_mut() = Rc::downgrade(&environment);
        let page = CpdfPage::from_document(&mut document.borrow_mut(), 0).unwrap();
        let page = SdkPage::new(&document, &page);
        let view = environment.get_or_create_page_view(&page).unwrap();
        view.set_valid(true);
        let annotation = view.annot_list()[0].clone();
        assert!(environment.set_focus_annot(&annotation));
        let cleanup_timer = environment.set_timer(100, Rc::new(|_| {}));
        assert!(cleanup_timer > 0);
        host.cleanup_timer.set(cleanup_timer);

        environment.on_detach_from_document();
        assert!(!environment.is_alive());
        assert!(!view.is_alive());
        assert!(!annotation.is_alive());
        assert_eq!(host.releases.get(), 1);
        assert_eq!(host.after_release.get(), 0);
        let events = host.events.borrow();
        let focus_out = events
            .iter()
            .position(|&event| event == "focus-out")
            .unwrap();
        let kill_timer = events
            .iter()
            .position(|&event| event == "cleanup-timer")
            .unwrap();
        assert!(focus_out < kill_timer);
        assert_eq!(events.last(), Some(&"release"));
        drop(events);
        environment.shutdown();
        environment.on_detach_from_document();
        drop(environment);
        assert_eq!(host.releases.get(), 1);
        assert_eq!(host.after_release.get(), 0);
    }
}
