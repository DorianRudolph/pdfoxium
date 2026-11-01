// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Ordinary form-fill ABI from `fpdfsdk/fpdf_formfill.cpp`.
//! The embedder retains its callback table through Release. As with PDFium,
//! bitmap storage remains live and exclusive throughout FPDF_FFLDraw.
use super::unimplemented_abi::{FPDF_FORMFILLINFO, FS_POINTF, IPDF_JSPLATFORM};
use super::*;
use pdfoxium_core::fpdfdoc::cpdf_formfield::{int_to_form_field_type, FormFieldType};
use pdfoxium_core::fpdfsdk::cpdfsdk_formfillenvironment::{
    CpdfSdkFormFillEnvironment, FormFillHost,
};
use pdfoxium_core::fpdfsdk::cpdfsdk_pageview::{CpdfSdkPageView, SdkPage};
use pdfoxium_core::fpdfsdk::pwl::ipwl_fillernotify::CursorStyle;
use pdfoxium_core::fpdfsdk::pwl::pwl_draw::AggPwlDraw;
use pdfoxium_core::fxcrt::fx_string::utf16_encode;
use pdfoxium_core::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use std::collections::HashMap;
use std::rc::Weak;

#[cfg(feature = "xfa")]
#[path = "formfill_xfa.rs"]
mod xfa_host;
#[cfg(feature = "xfa")]
pub use xfa_host::{FPDF_BStr_Clear, FPDF_BStr_Init, FPDF_BStr_Set, FPDF_LoadXFA};
#[cfg(not(feature = "xfa"))]
#[no_mangle]
pub unsafe extern "C" fn FPDF_LoadXFA(_document: *mut c_void) -> c_int {
    // fpdf_view.cpp::FPDF_LoadXFA's !PDF_ENABLE_XFA branch.
    0
}
type Timer = Rc<dyn Fn(i32)>;
thread_local! {
    static TIMERS: RefCell<HashMap<i32, Timer>> = RefCell::new(HashMap::new());
    static FORM_HANDLES:RefCell<HashMap<usize,usize>>=RefCell::new(HashMap::new());
    static ENVIRONMENTS: RefCell<Vec<Weak<CpdfSdkFormFillEnvironment>>> = const { RefCell::new(Vec::new()) };
}
unsafe extern "C" fn timer_callback(id: i32) {
    let callback = TIMERS.with(|timers| timers.borrow().get(&id).cloned());
    if let Some(callback) = callback {
        callback(id);
    }
}
struct FormHandle {
    env: Rc<CpdfSdkFormFillEnvironment>,
    host: Rc<FfiHost>,
}
struct FfiHost {
    #[cfg(feature = "xfa")]
    self_weak: Weak<FfiHost>,
    info: *mut FPDF_FORMFILLINFO,
    document: *mut c_void,
    pages: RefCell<HashMap<u64, (usize, Weak<SdkPage>)>>,
    released: Cell<bool>,
}
impl FfiHost {
    fn js_platform(&self) -> Option<NonNull<IPDF_JSPLATFORM>> {
        if self.released.get() {
            return None;
        }
        NonNull::new(unsafe { (*self.info).m_pJsPlatform })
    }
    fn platform_text(&self, language: bool) -> Vec<u32> {
        if self.released.get() || unsafe { (*self.info).version } < 2 {
            return Vec::new();
        }
        let callback = unsafe {
            if language {
                (*self.info).FFI_GetLanguage
            } else {
                (*self.info).FFI_GetPlatform
            }
        };
        let Some(callback) = callback else {
            return Vec::new();
        };
        let required = unsafe { callback(self.info, std::ptr::null_mut(), 0) };
        if required <= 0 || self.released.get() {
            return Vec::new();
        }
        let mut bytes = vec![0; required as usize];
        let actual = unsafe { callback(self.info, bytes.as_mut_ptr().cast(), required) };
        if actual <= 0 || actual > required || self.released.get() {
            return Vec::new();
        }
        pdfoxium_core::fxcrt::fx_string::utf16_decode(&bytes[..actual as usize], false)
    }
    fn remember_page(&self, raw: *mut c_void, page: &Rc<SdkPage>) {
        self.pages
            .borrow_mut()
            .insert(page.id(), (raw as usize, Rc::downgrade(page)));
    }
    fn page_handle(&self, page: &SdkPage) -> Option<*mut c_void> {
        let (raw, weak) = self.pages.borrow().get(&page.id()).cloned()?;
        weak.upgrade()
            .filter(|page| page.is_alive())
            .map(|_| raw as *mut c_void)
    }
}
impl FormFillHost for FfiHost {
    #[cfg(feature = "xfa")]
    fn xfa_host(
        &self,
    ) -> Option<Rc<dyn pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_docenvironment::XfaSdkHost>> {
        self.self_weak.upgrade().map(|host| {
            host as Rc<dyn pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_docenvironment::XfaSdkHost>
        })
    }
    fn javascript_enabled(&self) -> bool {
        // Cargo may unify core/js through another dependency. The C API's own
        // feature remains the authority for enabling this embedder's runtime.
        if !cfg!(feature = "js") || self.released.get() {
            return false;
        }
        // Preserve the public embedder's JS-platform opt-in. The Rust engine
        // supplies execution; external V8 isolate pointers are never consumed.
        !unsafe { (*self.info).m_pJsPlatform }.is_null()
    }
    fn invalidate(&self, page: &SdkPage, rect: FxRect) -> bool {
        if self.released.get() {
            return false;
        }
        let Some(page) = self.page_handle(page) else {
            return false;
        };
        let Some(callback) = (unsafe { (*self.info).FFI_Invalidate }) else {
            return false;
        };
        unsafe {
            callback(
                self.info,
                page.cast(),
                rect.left as f64,
                rect.top as f64,
                rect.right as f64,
                rect.bottom as f64,
            );
        }
        true
    }
    fn output_selected_rect(&self, page: &SdkPage, rect: CfxFloatRect) -> bool {
        if self.released.get() {
            return false;
        }
        let Some(page) = self.page_handle(page) else {
            return false;
        };
        let Some(callback) = (unsafe { (*self.info).FFI_OutputSelectedRect }) else {
            return false;
        };
        unsafe {
            callback(
                self.info,
                page.cast(),
                rect.left as f64,
                rect.top as f64,
                rect.right as f64,
                rect.bottom as f64,
            );
        }
        true
    }
    fn is_selection_implemented(&self) -> bool {
        if self.released.get() {
            return false;
        }
        unsafe { (*self.info).FFI_OutputSelectedRect.is_some() }
    }
    fn set_cursor(&self, cursor: CursorStyle) -> bool {
        if self.released.get() {
            return false;
        }
        let Some(callback) = (unsafe { (*self.info).FFI_SetCursor }) else {
            return false;
        };
        unsafe {
            callback(self.info, cursor as i32);
        }
        true
    }
    fn set_timer(&self, elapse: i32, callback: Timer) -> i32 {
        if self.released.get() {
            return 0;
        }
        let Some(set_timer) = (unsafe { (*self.info).FFI_SetTimer }) else {
            return 0;
        };
        let id = unsafe { set_timer(self.info, elapse, Some(timer_callback)) };
        if self.released.get() {
            return 0;
        }
        if id != 0 {
            TIMERS.with(|timers| timers.borrow_mut().insert(id, callback));
        }
        id
    }
    fn kill_timer(&self, id: i32) {
        TIMERS.with(|timers| timers.borrow_mut().remove(&id));
        if self.released.get() {
            return;
        }
        let callback = unsafe { (*self.info).FFI_KillTimer };
        if let Some(callback) = callback {
            unsafe {
                callback(self.info, id);
            }
        }
    }
    fn on_change(&self) -> bool {
        if self.released.get() {
            return false;
        }
        let Some(callback) = (unsafe { (*self.info).FFI_OnChange }) else {
            return false;
        };
        unsafe {
            callback(self.info);
        }
        true
    }
    fn set_text_field_focus(&self, text: &[u32], focused: bool) -> bool {
        if self.released.get() {
            return false;
        }
        let Some(callback) = (unsafe { (*self.info).FFI_SetTextFieldFocus }) else {
            return false;
        };
        let mut value = utf16_encode(text);
        let length = value.len();
        value.push(0);
        unsafe {
            callback(self.info, value.as_ptr(), length as _, focused as _);
        }
        true
    }
    fn on_focus_change(
        &self,
        annot: &Rc<pdfoxium_core::fpdfsdk::cpdfsdk_annot::CpdfSdkAnnot>,
        page_index: i32,
    ) -> bool {
        if self.released.get() {
            return false;
        }
        if unsafe { (*self.info).version } < 2 {
            return false;
        }
        let Some(callback) = (unsafe { (*self.info).FFI_OnFocusChange }) else {
            return false;
        };
        let Some(page) = annot.page() else {
            return false;
        };
        let Some(raw) = self
            .page_handle(&page)
            .and_then(|raw| NonNull::new(raw.cast::<PageHandle>()))
        else {
            return false;
        };
        // The callback annotation is borrowed for this call only, as specified
        // by FFI_OnFocusChange; the embedder does not close or retain it.
        let mut context = Box::new(annot::AnnotHandle {
            context: annot
                .pdf_dict()
                .map(pdfoxium_core::fpdfapi::page::cpdf_annotcontext::CpdfAnnotContext::new),
            #[cfg(feature = "xfa")]
            xfa: annot.xfa_widget().as_ref().map(Rc::downgrade),
            page: raw,
            object_handles: Default::default(),
        });
        unsafe {
            callback(
                self.info,
                (&mut *context as *mut annot::AnnotHandle).cast(),
                page_index,
            );
        }
        true
    }
    fn get_page(&self, index: i32) -> Option<Rc<SdkPage>> {
        if self.released.get() {
            return None;
        }
        let callback = unsafe { (*self.info).FFI_GetPage }?;
        let raw = unsafe { callback(self.info, self.document.cast(), index) }.cast();
        if self.released.get() {
            return None;
        }
        let page = unsafe { page_sdk_bridge(raw) }?;
        self.remember_page(raw, &page);
        Some(page)
    }
    fn current_page(&self) -> Option<Rc<SdkPage>> {
        if self.released.get() {
            return None;
        }
        let callback = unsafe { (*self.info).FFI_GetCurrentPage }?;
        let raw = unsafe { callback(self.info, self.document.cast()) }.cast();
        if self.released.get() {
            return None;
        }
        let page = unsafe { page_sdk_bridge(raw) }?;
        self.remember_page(raw, &page);
        Some(page)
    }
    fn js_app_alert(&self, message: &[u32], title: &[u32], kind: i32, icon: i32) -> i32 {
        let Some(platform) = self.js_platform() else {
            return -1;
        };
        let Some(callback) = (unsafe { platform.as_ref().app_alert }) else {
            return -1;
        };
        let message = terminated_wide(message);
        let title = terminated_wide(title);
        unsafe {
            callback(
                platform.as_ptr(),
                message.as_ptr(),
                title.as_ptr(),
                kind,
                icon,
            )
        }
    }
    fn js_app_beep(&self, kind: i32) {
        let Some(platform) = self.js_platform() else {
            return;
        };
        if let Some(callback) = unsafe { platform.as_ref().app_beep } {
            unsafe {
                callback(platform.as_ptr(), kind);
            }
        }
    }
    fn js_app_response(
        &self,
        question: &[u32],
        title: &[u32],
        default: &[u32],
        label: &[u32],
        password: bool,
        response: &mut [u8],
    ) -> i32 {
        let Some(platform) = self.js_platform() else {
            return -1;
        };
        let Some(callback) = (unsafe { platform.as_ref().app_response }) else {
            return -1;
        };
        let Ok(length) = i32::try_from(response.len()) else {
            return -1;
        };
        let question = terminated_wide(question);
        let title = terminated_wide(title);
        let default = terminated_wide(default);
        let label = terminated_wide(label);
        unsafe {
            callback(
                platform.as_ptr(),
                question.as_ptr(),
                title.as_ptr(),
                default.as_ptr(),
                label.as_ptr(),
                password as _,
                response.as_mut_ptr().cast(),
                length,
            )
        }
    }
    fn js_field_browse(&self) -> Vec<u32> {
        let Some(platform) = self.js_platform() else {
            return Vec::new();
        };
        let Some(callback) = (unsafe { platform.as_ref().Field_browse }) else {
            return Vec::new();
        };
        // Read the function pointer before invoking the embedder. Release can
        // invalidate the callback table during either call.
        let required = unsafe { callback(platform.as_ptr(), std::ptr::null_mut(), 0) };
        if required <= 0 || self.released.get() {
            return Vec::new();
        }
        let mut bytes = Vec::new();
        if bytes.try_reserve_exact(required as usize).is_err() {
            return Vec::new();
        }
        bytes.resize(required as usize, 0);
        let actual = unsafe { callback(platform.as_ptr(), bytes.as_mut_ptr().cast(), required) };
        if actual <= 0 || actual > required || self.released.get() {
            return Vec::new();
        }
        // Native JS_fieldBrowse consumes local-ANSI bytes except the terminator.
        bytes.truncate(actual as usize - 1);
        let mut wide = vec![0; bytes.len()];
        pdfoxium_core::fxcrt::fx_codepage::multi_byte_to_wide_char(
            pdfoxium_core::fxcrt::fx_codepage::FxCodePage::DefAnsi,
            &bytes,
            &mut wide,
        );
        wide
    }
    fn js_doc_mail_form(
        &self,
        data: &[u8],
        ui: bool,
        to: &[u32],
        subject: &[u32],
        cc: &[u32],
        bcc: &[u32],
        message: &[u32],
    ) {
        let Some(platform) = self.js_platform() else {
            return;
        };
        let Some(callback) = (unsafe { platform.as_ref().Doc_mail }) else {
            return;
        };
        let Ok(length) = i32::try_from(data.len()) else {
            return;
        };
        let to = terminated_wide(to);
        let subject = terminated_wide(subject);
        let cc = terminated_wide(cc);
        let bcc = terminated_wide(bcc);
        let message = terminated_wide(message);
        // mailData is an input buffer in fpdf_formfill.h; the C ABI spells its
        // pointer as non-const, as in the source adapter's const_cast.
        unsafe {
            callback(
                platform.as_ptr(),
                data.as_ptr().cast_mut().cast(),
                length,
                ui as _,
                to.as_ptr(),
                subject.as_ptr(),
                cc.as_ptr(),
                bcc.as_ptr(),
                message.as_ptr(),
            );
        }
    }
    fn js_doc_print(
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
        let Some(platform) = self.js_platform() else {
            return;
        };
        if let Some(callback) = unsafe { platform.as_ref().Doc_print } {
            unsafe {
                callback(
                    platform.as_ptr(),
                    ui as _,
                    start,
                    end,
                    silent as _,
                    shrink_to_fit as _,
                    as_image as _,
                    reverse as _,
                    annotations as _,
                );
            }
        }
    }
    fn js_doc_goto_page(&self, index: i32) -> bool {
        let Some(platform) = self.js_platform() else {
            return false;
        };
        let Some(callback) = (unsafe { platform.as_ref().Doc_gotoPage }) else {
            return false;
        };
        unsafe {
            callback(platform.as_ptr(), index);
        }
        true
    }
    fn get_platform(&self) -> Vec<u32> {
        self.platform_text(false)
    }
    fn get_language(&self) -> Vec<u32> {
        self.platform_text(true)
    }
    fn execute_named_action(&self, name: &[u8]) -> bool {
        if self.released.get() {
            return false;
        }
        let Some(callback) = (unsafe { (*self.info).FFI_ExecuteNamedAction }) else {
            return false;
        };
        let text = terminated_bytes(name);
        unsafe {
            callback(self.info, text.as_ptr().cast());
        }
        true
    }
    fn do_uri_action(&self, uri: &[u8], flags: u32) -> bool {
        if self.released.get() {
            return false;
        }
        let text = terminated_bytes(uri);
        if unsafe { (*self.info).version } >= 2 {
            if let Some(callback) = unsafe { (*self.info).FFI_DoURIActionWithKeyboardModifier } {
                unsafe {
                    callback(self.info, text.as_ptr().cast(), flags as i32);
                }
                return true;
            }
        }
        let Some(callback) = (unsafe { (*self.info).FFI_DoURIAction }) else {
            return false;
        };
        unsafe {
            callback(self.info, text.as_ptr().cast());
        }
        true
    }
    fn do_go_to_action(&self, page: i32, zoom: i32, positions: &[f32]) -> bool {
        if self.released.get() {
            return false;
        }
        let Some(callback) = (unsafe { (*self.info).FFI_DoGoToAction }) else {
            return false;
        };
        let mut positions = positions.to_vec();
        unsafe {
            callback(
                self.info,
                page,
                zoom,
                positions.as_mut_ptr(),
                positions.len() as i32,
            );
        }
        true
    }
    fn file_path(&self) -> Option<Vec<u32>> {
        let platform = self.js_platform()?;
        let callback = unsafe { platform.as_ref().Doc_getFilePath }?;
        let size = unsafe { callback(platform.as_ptr(), std::ptr::null_mut(), 0) };
        if size <= 0 || self.released.get() {
            return None;
        }
        let mut bytes = vec![0u8; size as usize];
        let actual = unsafe { callback(platform.as_ptr(), bytes.as_mut_ptr().cast(), size) };
        if actual <= 0 || actual > size || self.released.get() {
            return None;
        }
        // Source consumes local-ANSI bytes and excludes the final terminator.
        bytes.truncate(actual as usize - 1);
        let mut wide = vec![0; bytes.len()];
        pdfoxium_core::fxcrt::fx_codepage::multi_byte_to_wide_char(
            pdfoxium_core::fxcrt::fx_codepage::FxCodePage::DefAnsi,
            &bytes,
            &mut wide,
        );
        Some(wide)
    }
    fn submit_form(&self, data: &[u8], url: &[u32]) -> bool {
        if self.released.get() {
            return false;
        }
        let platform = unsafe { (*self.info).m_pJsPlatform };
        if platform.is_null() {
            return false;
        }
        let Some(callback) = (unsafe { (*platform).Doc_submitForm }) else {
            return false;
        };
        let mut data = data.to_vec();
        let mut url = utf16_encode(url);
        url.push(0);
        unsafe {
            callback(
                platform,
                data.as_mut_ptr().cast(),
                data.len() as i32,
                url.as_ptr(),
            );
        }
        true
    }
    fn release(&self) {
        if self.released.replace(true) {
            return;
        }
        let callback = unsafe { (*self.info).Release };
        if let Some(callback) = callback {
            unsafe {
                callback(self.info);
            }
        }
    }
}
fn terminated_bytes(text: &[u8]) -> Vec<u8> {
    text.iter().copied().chain(Some(0)).collect()
}
fn terminated_wide(text: &[u32]) -> Vec<u16> {
    let mut value = utf16_encode(text);
    value.push(0);
    value
}

#[cfg(all(test, feature = "js"))]
#[path = "formfill_js_tests.rs"]
mod javascript_host_tests;
unsafe fn snapshot(handle: *mut c_void) -> Option<(Rc<CpdfSdkFormFillEnvironment>, Rc<FfiHost>)> {
    let handle = unsafe { handle.cast::<FormHandle>().as_ref() }?;
    Some((handle.env.clone(), handle.host.clone()))
}
/// Read-only environment lookup. Operations which may call the embedder use
/// `with_form_environment` to keep the host alive across reentrant teardown.
pub(super) unsafe fn form_environment(
    handle: *mut c_void,
) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
    let (env, _host) = unsafe { snapshot(handle) }?;
    env.is_alive().then_some(env)
}
/// Retain both owners while calling out through the environment. No raw handle
/// or handle reference is used again once the callback starts.
pub(super) unsafe fn with_form_environment<R>(
    handle: *mut c_void,
    callback: impl FnOnce(Rc<CpdfSdkFormFillEnvironment>) -> R,
) -> Option<R> {
    let (env, _host) = unsafe { snapshot(handle) }?;
    env.is_alive().then(|| callback(env))
}
unsafe fn page_view(
    handle: *mut c_void,
    page: *mut c_void,
) -> Option<(
    Rc<CpdfSdkFormFillEnvironment>,
    Rc<FfiHost>,
    Rc<CpdfSdkPageView>,
)> {
    let (env, host) = unsafe { snapshot(handle) }?;
    if !env.is_alive() || env.in_passive_ffi_callback() {
        return None;
    }
    let page_bridge = unsafe { page_sdk_bridge(page) }?;
    let document = env.document()?;
    if !page_bridge
        .document()
        .is_some_and(|d| Rc::ptr_eq(&d, &document))
    {
        return None;
    }
    host.remember_page(page, &page_bridge);
    let view = env.get_or_create_page_view(&page_bridge)?;
    Some((env, host, view))
}
unsafe fn event(
    handle: *mut c_void,
    page: *mut c_void,
    callback: impl FnOnce(&CpdfSdkPageView) -> bool,
) -> c_int {
    let Some((entry_env, _entry_host)) = (unsafe { snapshot(handle) }) else {
        return 0;
    };
    let _event = entry_env.begin_input_event();
    let Some((env, _host, view)) = (unsafe { page_view(handle, page) }) else {
        return 0;
    };
    let result = callback(&view);
    (result && env.is_alive() && env.action_error().is_none()) as c_int
}
#[cfg(feature = "js")]
pub(super) fn document_runtime(
    document: &Rc<RefCell<CpdfDocument>>,
) -> Option<Rc<pdfoxium_core::fxjs::cjs_runtime::CjsRuntime>> {
    let environments: Vec<_> =
        ENVIRONMENTS.with(|entries| entries.borrow().iter().filter_map(Weak::upgrade).collect());
    environments.into_iter().find_map(|environment| {
        environment
            .document()
            .filter(|owner| Rc::ptr_eq(owner, document))?;
        environment.existing_runtime()
    })
}
pub(super) fn detach_document_forms(document: &Rc<RefCell<CpdfDocument>>) {
    let envs: Vec<_> = ENVIRONMENTS.with(|envs| {
        let mut envs = envs.borrow_mut();
        envs.retain(|env| env.strong_count() > 0);
        envs.iter().filter_map(Weak::upgrade).collect()
    });
    for env in envs {
        if env.document().is_some_and(|d| Rc::ptr_eq(&d, document)) {
            env.on_detach_from_document();
        }
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFDOC_InitFormFillEnvironment(
    document: *mut c_void,
    info: *mut FPDF_FORMFILLINFO,
) -> *mut c_void {
    if document.is_null() || info.is_null() {
        return std::ptr::null_mut();
    }
    let version = unsafe { (*info).version };
    // CheckFormfillVersion permits v1 without XFA, and requires v2 with XFA.
    if !(1..=2).contains(&version) || (cfg!(feature = "xfa") && version != 2) {
        return std::ptr::null_mut();
    }
    let handle = unsafe { &mut *document.cast::<DocumentHandle>() };
    let document_rc = handle.document.clone();
    #[cfg(feature = "xfa")]
    let xfa_disabled = unsafe { (*info).version >= 2 && (*info).xfa_disabled != 0 };
    #[cfg(feature = "xfa")]
    let context = if xfa_disabled {
        None
    } else {
        Some(
            handle
                .xfa_context
                .get_or_insert_with(|| {
                    pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_context::CpdfXfaContext::new(
                        &document_rc,
                    )
                })
                .clone(),
        )
    };
    #[cfg(feature = "xfa")]
    if let Some(existing) = context.as_ref().and_then(|ctx| ctx.form_fill_env()) {
        if let Some(raw) = FORM_HANDLES.with(|handles| {
            handles
                .borrow()
                .get(&(Rc::as_ptr(&existing) as usize))
                .copied()
        }) {
            return raw as *mut c_void;
        }
    }
    let host = Rc::new_cyclic(|_weak| FfiHost {
        #[cfg(feature = "xfa")]
        self_weak: _weak.clone(),
        info,
        document,
        pages: Default::default(),
        released: Cell::new(false),
    });
    let callbacks: Rc<dyn FormFillHost> = host.clone();
    let env = CpdfSdkFormFillEnvironment::new(&document_rc, Some(&callbacks));
    ENVIRONMENTS.with(|envs| envs.borrow_mut().push(Rc::downgrade(&env)));
    #[cfg(feature = "xfa")]
    if let Some(context) = &context {
        context.set_form_fill_env(Some(&env));
    }
    let env_key = Rc::as_ptr(&env) as usize;
    let result = Box::into_raw(Box::new(FormHandle { env, host })).cast();
    FORM_HANDLES.with(|handles| handles.borrow_mut().insert(env_key, result as usize));
    #[cfg(feature = "xfa")]
    super::ext::report_unsupported_xfa(&document_rc, context.is_some());
    #[cfg(not(feature = "xfa"))]
    super::ext::report_unsupported_xfa(&document_rc, false);
    result
}
#[no_mangle]
pub unsafe extern "C" fn FPDFDOC_ExitFormFillEnvironment(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let handle = unsafe { Box::from_raw(handle.cast::<FormHandle>()) };
    FORM_HANDLES.with(|handles| {
        handles
            .borrow_mut()
            .remove(&(Rc::as_ptr(&handle.env) as usize))
    });
    handle.env.shutdown();
}
#[no_mangle]
pub unsafe extern "C" fn FORM_OnAfterLoadPage(page: *mut c_void, handle: *mut c_void) {
    if let Some((_env, _host, view)) = unsafe { page_view(handle, page) } {
        view.set_valid(true);
    }
}
#[no_mangle]
pub unsafe extern "C" fn FORM_OnBeforeClosePage(page: *mut c_void, handle: *mut c_void) {
    let Some((env, _host)) = (unsafe { snapshot(handle) }) else {
        return;
    };
    if env.in_passive_ffi_callback() {
        return;
    }
    let Some(page) = (unsafe { page_sdk_bridge(page) }) else {
        return;
    };
    if let Some(view) = env.get_page_view(&page) {
        view.set_valid(false);
        env.remove_page_view(&page);
    }
}
macro_rules! point_event {
    ($name:ident, $method:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(
            handle: *mut c_void,
            page: *mut c_void,
            flags: c_int,
            x: f64,
            y: f64,
        ) -> c_int {
            unsafe {
                event(handle, page, |view| {
                    view.$method(flags as u32, CfxPointF::new(x as f32, y as f32))
                })
            }
        }
    };
}
point_event!(FORM_OnMouseMove, on_mouse_move);
point_event!(FORM_OnFocus, on_focus);
point_event!(FORM_OnLButtonDown, on_l_button_down);
point_event!(FORM_OnLButtonUp, on_l_button_up);
point_event!(FORM_OnLButtonDoubleClick, on_l_button_dbl_clk);
point_event!(FORM_OnRButtonDown, on_r_button_down);
point_event!(FORM_OnRButtonUp, on_r_button_up);
#[no_mangle]
pub unsafe extern "C" fn FORM_OnMouseWheel(
    handle: *mut c_void,
    page: *mut c_void,
    flags: c_int,
    point: *const FS_POINTF,
    dx: c_int,
    dy: c_int,
) -> c_int {
    let Some(point) = (unsafe { point.as_ref() }) else {
        return 0;
    };
    let point = CfxPointF::new(point.x, point.y);
    unsafe {
        event(handle, page, |view| {
            view.on_mouse_wheel(flags as u32, point, CfxPointF::new(dx as f32, dy as f32))
        })
    }
}
#[no_mangle]
pub unsafe extern "C" fn FORM_OnKeyDown(
    handle: *mut c_void,
    page: *mut c_void,
    key: c_int,
    flags: c_int,
) -> c_int {
    unsafe { event(handle, page, |view| view.on_key_down(key, flags as u32)) }
}
#[no_mangle]
pub unsafe extern "C" fn FORM_OnKeyUp(
    _handle: *mut c_void,
    _page: *mut c_void,
    _key: c_int,
    _flags: c_int,
) -> c_int {
    // The ordinary upstream entry point does not consume key-up events.
    0
}
#[no_mangle]
pub unsafe extern "C" fn FORM_OnChar(
    handle: *mut c_void,
    page: *mut c_void,
    word: c_int,
    flags: c_int,
) -> c_int {
    unsafe { event(handle, page, |view| view.on_char(word as u32, flags as u32)) }
}
macro_rules! simple_event {
    ($name:ident,$method:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(handle: *mut c_void, page: *mut c_void) -> c_int {
            unsafe { event(handle, page, |view| view.$method()) }
        }
    };
}
simple_event!(FORM_SelectAllText, select_all_text);
simple_event!(FORM_CanUndo, can_undo);
simple_event!(FORM_CanRedo, can_redo);
simple_event!(FORM_Undo, undo);
simple_event!(FORM_Redo, redo);
#[no_mangle]
pub unsafe extern "C" fn FORM_ForceToKillFocus(handle: *mut c_void) -> c_int {
    let Some((env, _host)) = (unsafe { snapshot(handle) }) else {
        return 0;
    };
    if env.in_passive_ffi_callback() {
        return 0;
    }
    let _event = env.begin_input_event();
    let result = env.kill_focus_annot(0);
    (result && env.action_error().is_none()) as c_int
}
unsafe fn copy_text(text: &[u32], buffer: *mut c_void, length: c_ulong) -> c_ulong {
    let values: Vec<u16> = utf16_encode(text).into_iter().chain(Some(0)).collect();
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    if !buffer.is_null() && length as usize >= bytes.len() {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), bytes.len());
        }
    }
    bytes.len() as c_ulong
}
macro_rules! get_text {
    ($name:ident,$method:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(
            handle: *mut c_void,
            page: *mut c_void,
            buffer: *mut c_void,
            length: c_ulong,
        ) -> c_ulong {
            let Some((_env, _host, view)) = (unsafe { page_view(handle, page) }) else {
                return 0;
            };
            unsafe { copy_text(&view.$method(), buffer, length) }
        }
    };
}
get_text!(FORM_GetFocusedText, focused_form_text);
get_text!(FORM_GetSelectedText, selected_text);
unsafe fn wide_string(text: *const u16) -> Vec<u32> {
    if text.is_null() {
        return Vec::new();
    }
    let mut length = 0;
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    char::decode_utf16(
        unsafe { std::slice::from_raw_parts(text, length) }
            .iter()
            .copied(),
    )
    .map(|c| c.map_or(0xfffd, u32::from))
    .collect()
}
macro_rules! replace_text {
    ($name:ident,$method:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(handle: *mut c_void, page: *mut c_void, text: *const u16) {
            let text = unsafe { wide_string(text) };
            if let Some((env, _host, view)) = unsafe { page_view(handle, page) } {
                let _event = env.begin_input_event();
                view.$method(&text);
            }
        }
    };
}
replace_text!(FORM_ReplaceSelection, replace_selection);
replace_text!(FORM_ReplaceAndKeepSelection, replace_and_keep_selection);
#[no_mangle]
pub unsafe extern "C" fn FORM_SetIndexSelected(
    handle: *mut c_void,
    page: *mut c_void,
    index: c_int,
    selected: c_int,
) -> c_int {
    unsafe {
        event(handle, page, |view| {
            view.set_index_selected(index, selected != 0)
        })
    }
}
#[no_mangle]
pub unsafe extern "C" fn FORM_IsIndexSelected(
    handle: *mut c_void,
    page: *mut c_void,
    index: c_int,
) -> c_int {
    unsafe { event(handle, page, |view| view.is_index_selected(index)) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_SetFormFieldHighlightColor(
    handle: *mut c_void,
    kind: c_int,
    color: c_ulong,
) {
    let Some((env, _host)) = (unsafe { snapshot(handle) }) else {
        return;
    };
    let Some(kind) = int_to_form_field_type(kind) else {
        return;
    };
    if kind == FormFieldType::Unknown {
        env.interactive_form()
            .set_all_highlight_colors(color as u32);
    } else {
        env.interactive_form()
            .set_highlight_color(color as u32, kind);
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_SetFormFieldHighlightAlpha(handle: *mut c_void, alpha: u8) {
    if let Some((env, _host)) = unsafe { snapshot(handle) } {
        env.interactive_form().set_highlight_alpha(alpha);
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_RemoveFormFieldHighlight(handle: *mut c_void) {
    if let Some((env, _host)) = unsafe { snapshot(handle) } {
        env.interactive_form().remove_all_highlights();
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_FFLDraw(
    handle: *mut c_void,
    bitmap: *mut c_void,
    page: *mut c_void,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    rotate: c_int,
    flags: c_int,
) {
    if bitmap.is_null() {
        return;
    }
    let Some(right) = x.checked_add(width) else {
        return;
    };
    let Some(bottom) = y.checked_add(height) else {
        return;
    };
    let Some((env, _host, view)) = (unsafe { page_view(handle, page) }) else {
        return;
    };
    let Some(page) = view.page() else {
        return;
    };
    let rect = FxRect::new(x, y, right, bottom);
    let matrix = page.display_matrix_for_rect(rect, rotate);
    let mut options = CpdfRenderOptions::new();
    options.options_mut().clear_type = flags & 2 != 0;
    if flags & 8 != 0 {
        options.set_color_mode(RenderColorMode::Gray);
    }
    options.set_draw_annots(flags & 1 != 0);
    if let Some(document) = env.document() {
        options.set_oc_context(Some(Rc::new(CpdfOcContext::new(
            &document.borrow(),
            UsageType::View,
        ))));
    }
    let draw = |bitmap: &mut CfxDibitmapData<'_>| {
        if bitmap.format() == FxdibFormat::OneBppMask || bitmap.format() == FxdibFormat::OneBppRgb {
            return;
        }
        let mut driver = CfxAggDeviceDriver::new(bitmap, flags & 0x10 != 0, None);
        driver.set_clip_rect(rect);
        let mut device = AggPwlDraw::with_options(&mut driver, options.clone());
        view.page_view_on_draw(&mut device, matrix, flags & 1 != 0, rect);
    };
    let bitmap = unsafe { &mut *bitmap.cast::<BitmapHandle>() };
    match &mut *bitmap.storage.borrow_mut() {
        BitmapStorage::Owned(bitmap) => draw(bitmap),
        BitmapStorage::External {
            width,
            height,
            format,
            pitch,
            buffer,
        } => {
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(buffer.as_ptr(), *pitch as usize * *height as usize)
            };
            if let Some(mut bitmap) =
                CfxDibitmapData::from_buffer(*width, *height, *format, *pitch, bytes)
            {
                draw(&mut bitmap);
            }
        }
    }
}

// Void action entrypoints retain an explicit error on the environment when a
// document requests JavaScript. The following ordinary input event starts a new
// error scope, so an unavailable script cannot poison unrelated later input.
#[no_mangle]
pub unsafe extern "C" fn FORM_DoDocumentJSAction(handle: *mut c_void) {
    let Some((env, _host)) = (unsafe { snapshot(handle) }) else {
        return;
    };
    if !env.is_alive() || env.in_passive_ffi_callback() {
        return;
    }
    let _event = env.begin_input_event();
    if let Err(error) = env.proc_javascript_action() {
        env.record_action_error(error);
    }
}
#[no_mangle]
pub unsafe extern "C" fn FORM_DoDocumentOpenAction(handle: *mut c_void) {
    let Some((env, _host)) = (unsafe { snapshot(handle) }) else {
        return;
    };
    if !env.is_alive() || env.in_passive_ffi_callback() {
        return;
    }
    let _event = env.begin_input_event();
    if let Err(error) = env.proc_open_action() {
        env.record_action_error(error);
    }
}
fn additional_action(
    dict: &pdfoxium_core::fpdfapi::parser::cpdf_object::ObjectRef,
) -> pdfoxium_core::fpdfdoc::cpdf_aaction::CpdfAAction {
    let action = dict
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .and_then(|d| d.get_dict_for(b"AA"));
    pdfoxium_core::fpdfdoc::cpdf_aaction::CpdfAAction::new(action)
}
#[no_mangle]
pub unsafe extern "C" fn FORM_DoDocumentAAction(handle: *mut c_void, kind: c_int) {
    use pdfoxium_core::fpdfdoc::cpdf_aaction::AActionType;
    let Some((env, _host)) = (unsafe { snapshot(handle) }) else {
        return;
    };
    if !env.is_alive() || env.in_passive_ffi_callback() {
        return;
    }
    let _event = env.begin_input_event();
    let kind = match kind {
        0x10 => AActionType::CloseDocument,
        0x11 => AActionType::SaveDocument,
        0x12 => AActionType::DocumentSaved,
        0x13 => AActionType::PrintDocument,
        0x14 => AActionType::DocumentPrinted,
        _ => return,
    };
    let Some(root) = env.document().and_then(|d| d.borrow().root()) else {
        return;
    };
    let action = additional_action(&root);
    if action.action_exist(kind) {
        if let Err(error) = env.do_action_document(&action.action(kind), kind) {
            env.record_action_error(error);
        }
    }
}
#[no_mangle]
pub unsafe extern "C" fn FORM_DoPageAAction(page: *mut c_void, handle: *mut c_void, kind: c_int) {
    use pdfoxium_core::fpdfdoc::cpdf_aaction::AActionType;
    let Some((env, _host)) = (unsafe { snapshot(handle) }) else {
        return;
    };
    if !env.is_alive() || env.in_passive_ffi_callback() {
        return;
    }
    let _event = env.begin_input_event();
    let Some(page) = (unsafe { page_sdk_bridge(page) }) else {
        return;
    };
    if env.get_page_view(&page).is_none() {
        return;
    }
    let kind = if kind == 0 {
        AActionType::OpenPage
    } else {
        AActionType::ClosePage
    };
    // Native CPDFPageFromFPDFPage rejects layout-only XFA pages before
    // inspecting PDF additional actions; no PDF dictionary is fabricated.
    let Some(dictionary) = page.pdf_dict() else {
        return;
    };
    let action = additional_action(&dictionary);
    if action.action_exist(kind) {
        if let Err(error) = env.do_action_page(&action.action(kind), kind) {
            env.record_action_error(error);
        }
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_HasFormFieldAtPoint(
    handle: *mut c_void,
    page: *mut c_void,
    x: f64,
    y: f64,
) -> c_int {
    let Some((env, _host, view)) = (unsafe { page_view(handle, page) }) else {
        return -1;
    };
    #[cfg(feature = "xfa")]
    if let Some(xfa) = view.page().and_then(|page| page.xfa_page()).filter(|page| {
        page.context()
            .is_some_and(|ctx| ctx.contains_extension_full_form())
    }) {
        return xfa.has_form_field_at_point(CfxPointF::new(x as f32, y as f32));
    }
    let Some(page) = view.page().and_then(|p| p.geometry_page()) else {
        return -1;
    };
    env.interactive_form()
        .core_form()
        .get_control_at_point(&page, CfxPointF::new(x as f32, y as f32))
        .and_then(|(control, _)| control.field())
        .map_or(-1, |field| field.field_type() as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_FormFieldZOrderAtPoint(
    handle: *mut c_void,
    page: *mut c_void,
    x: f64,
    y: f64,
) -> c_int {
    let Some((env, _host, view)) = (unsafe { page_view(handle, page) }) else {
        return -1;
    };
    let Some(page) = view.page().and_then(|p| p.geometry_page()) else {
        return -1;
    };
    env.interactive_form()
        .core_form()
        .get_control_at_point(&page, CfxPointF::new(x as f32, y as f32))
        .map_or(-1, |(_, index)| index)
}

#[no_mangle]
pub unsafe extern "C" fn FORM_GetFocusedAnnot(
    handle: *mut c_void,
    page_index: *mut c_int,
    annotation: *mut *mut c_void,
) -> c_int {
    if page_index.is_null() || annotation.is_null() {
        return 0;
    }
    let Some((env, host)) = (unsafe { snapshot(handle) }) else {
        return 0;
    };
    unsafe {
        *page_index = -1;
        *annotation = std::ptr::null_mut();
    }
    let Some(annot) = env.get_focus_annot() else {
        return 1;
    };
    let Some(view) = annot.page_view().filter(|view| view.is_valid()) else {
        return 1;
    };
    let Some(page) = annot.page() else {
        return 1;
    };
    let Some(raw) = host
        .page_handle(&page)
        .and_then(|raw| NonNull::new(raw.cast::<PageHandle>()))
    else {
        return 1;
    };
    let context = Box::new(annot::AnnotHandle {
        context: annot
            .pdf_dict()
            .map(pdfoxium_core::fpdfapi::page::cpdf_annotcontext::CpdfAnnotContext::new),
        #[cfg(feature = "xfa")]
        xfa: annot.xfa_widget().as_ref().map(Rc::downgrade),
        page: raw,
        object_handles: Default::default(),
    });
    unsafe {
        *page_index = view.page_index();
        *annotation = Box::into_raw(context).cast();
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FORM_SetFocusedAnnot(
    handle: *mut c_void,
    annotation: *mut c_void,
) -> c_int {
    if annotation.is_null() {
        return 0;
    }
    let (page, dict) = {
        let context = unsafe { &*annotation.cast::<annot::AnnotHandle>() };
        (context.page.as_ptr().cast(), context.pdf_dict())
    };
    #[cfg(feature = "xfa")]
    let xfa = unsafe { &*annotation.cast::<annot::AnnotHandle>() }.xfa_widget();
    let Some((env, _host, view)) = (unsafe { page_view(handle, page) }) else {
        return 0;
    };
    if !view.is_valid() {
        return 0;
    }
    #[cfg(not(feature = "xfa"))]
    let annot = dict.and_then(|dict| view.annot_by_dict(&dict));
    #[cfg(feature = "xfa")]
    let annot = xfa
        .and_then(|xfa| view.get_annot_for_ff_widget(xfa.ff_widget().as_ff_widget().unwrap()))
        .or_else(|| dict.and_then(|dict| view.annot_by_dict(&dict)));
    let Some(annot) = annot else {
        return 0;
    };
    let _event = env.begin_input_event();
    let result = env.set_focus_annot(&annot);
    (result && env.action_error().is_none()) as c_int
}

/// `fpdfsdk/fpdf_annot.cpp::FPDFAnnot_SetFocusableSubtypes`.
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetFocusableSubtypes(
    handle: *mut c_void,
    subtypes: *const c_int,
    count: usize,
) -> c_int {
    let Some(env) = (unsafe { form_environment(handle) }) else {
        return 0;
    };
    if (count > 0 && subtypes.is_null()) || count > isize::MAX as usize / size_of::<c_int>() {
        return 0;
    }
    // The source accepts arbitrary enum integers and preserves order/duplicates.
    // Unknown values are stored safely as integers; they never match an annot.
    let subtypes = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(subtypes, count) }
    };
    env.set_focusable_annot_subtype_values(subtypes);
    1
}

/// `fpdfsdk/fpdf_annot.cpp::FPDFAnnot_GetFocusableSubtypesCount`.
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFocusableSubtypesCount(handle: *mut c_void) -> c_int {
    unsafe { form_environment(handle) }.map_or(-1, |env| {
        c_int::try_from(env.focusable_annot_subtype_values().len()).unwrap_or(-1)
    })
}

/// `fpdfsdk/fpdf_annot.cpp::FPDFAnnot_GetFocusableSubtypes`.
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFocusableSubtypes(
    handle: *mut c_void,
    subtypes: *mut c_int,
    count: usize,
) -> c_int {
    let Some(env) = (unsafe { form_environment(handle) }) else {
        return 0;
    };
    if subtypes.is_null() {
        return 0;
    }
    let values = env.focusable_annot_subtype_values();
    if count < values.len() {
        return 0;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(values.as_ptr(), subtypes, values.len());
    }
    1
}

unsafe fn direction_widget(
    handle: *mut c_void,
    annotation: *mut c_void,
) -> Option<(
    Rc<CpdfSdkFormFillEnvironment>,
    Rc<FfiHost>,
    Rc<pdfoxium_core::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget>,
)> {
    // End the annotation-handle borrow before discovering widgets or calling
    // the host: either the annotation or its page can close during callbacks.
    let (page, dict) = {
        let context = unsafe { annotation.cast::<annot::AnnotHandle>().as_ref() }?;
        (context.page.as_ptr().cast(), context.pdf_dict()?)
    };
    let (env, host, view) = unsafe { page_view(handle, page) }?;
    if !view.is_valid() {
        return None;
    }
    Some((env, host, view.widget_by_dict(&dict)?))
}

/// `fpdfsdk/fpdf_formfill.cpp::FORM_GetTextDirection`.
#[no_mangle]
pub unsafe extern "C" fn FORM_GetTextDirection(
    handle: *mut c_void,
    annotation: *mut c_void,
) -> c_int {
    use pdfoxium_core::fxcrt::cfx_bidi_resolver::ParagraphDirection;
    let Some((_env, _host, widget)) = (unsafe { direction_widget(handle, annotation) }) else {
        return 0;
    };
    match widget.text_direction() {
        ParagraphDirection::Auto => 1,
        ParagraphDirection::LeftToRight => 2,
        ParagraphDirection::RightToLeft => 3,
    }
}

/// `fpdfsdk/fpdf_formfill.cpp::FORM_SetTextDirection`.
#[no_mangle]
pub unsafe extern "C" fn FORM_SetTextDirection(
    handle: *mut c_void,
    annotation: *mut c_void,
    direction: c_int,
) -> c_int {
    use pdfoxium_core::fxcrt::cfx_bidi_resolver::ParagraphDirection;
    let Some((entry_env, _entry_host)) = (unsafe { snapshot(handle) }) else {
        return 0;
    };
    let _event = entry_env.begin_input_event();
    let Some((env, _host, widget)) = (unsafe { direction_widget(handle, annotation) }) else {
        return 0;
    };
    let direction = match direction {
        1 => ParagraphDirection::Auto,
        2 => ParagraphDirection::LeftToRight,
        3 => ParagraphDirection::RightToLeft,
        _ => return 0,
    };
    env.interactive_form_filler()
        .update_form_field_text_direction(&widget, direction);
    (env.is_alive() && widget.is_alive() && env.action_error().is_none()) as c_int
}

#[cfg(test)]
mod tests {
    use super::*;
    #[repr(C)]
    struct Host {
        info: FPDF_FORMFILLINFO,
        handle: Cell<*mut c_void>,
        releases: Cell<usize>,
    }
    unsafe extern "C" fn release(info: *mut FPDF_FORMFILLINFO) {
        let host = unsafe { &*info.cast::<Host>() };
        host.releases.set(host.releases.get() + 1);
    }
    unsafe extern "C" fn close_during_change(info: *mut FPDF_FORMFILLINFO) {
        let host = unsafe { &*info.cast::<Host>() };
        let handle = host.handle.replace(std::ptr::null_mut());
        unsafe {
            FPDFDOC_ExitFormFillEnvironment(handle);
        }
    }
    struct FormFixture {
        document: *mut c_void,
        page: *mut c_void,
        form: *mut c_void,
        _info: Box<FPDF_FORMFILLINFO>,
    }
    impl FormFixture {
        fn open(bytes: &'static [u8], after_load: bool) -> Self {
            FPDF_InitLibrary();
            let document = unsafe {
                FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null())
            };
            assert!(!document.is_null());
            let page = unsafe { FPDF_LoadPage(document, 0) };
            assert!(!page.is_null());
            let mut info: Box<FPDF_FORMFILLINFO> = Box::new(unsafe { std::mem::zeroed() });
            info.version = 2;
            let form = unsafe { FPDFDOC_InitFormFillEnvironment(document, &mut *info) };
            assert!(!form.is_null());
            if after_load {
                unsafe {
                    FORM_OnAfterLoadPage(page, form);
                }
            }
            Self {
                document,
                page,
                form,
                _info: info,
            }
        }
        fn assert_focusability(&self, focusable: &[c_int]) {
            let expected = [2, 2, 2, 2, 9, 9, 16, 9, 20];
            assert_eq!(
                unsafe { FPDFPage_GetAnnotCount(self.page) },
                expected.len() as c_int
            );
            for (index, subtype) in expected.into_iter().enumerate() {
                let annotation = unsafe { FPDFPage_GetAnnot(self.page, index as c_int) };
                assert!(!annotation.is_null());
                assert_eq!(unsafe { FPDFAnnot_GetSubtype(annotation) }, subtype);
                assert_eq!(
                    unsafe { FORM_SetFocusedAnnot(self.form, annotation) } != 0,
                    focusable.contains(&subtype)
                );
                unsafe {
                    FORM_ForceToKillFocus(self.form);
                    FPDFPage_CloseAnnot(annotation);
                }
            }
        }
        fn assert_subtypes(&self, expected: &[c_int]) {
            assert_eq!(
                unsafe { FPDFAnnot_GetFocusableSubtypesCount(self.form) },
                expected.len() as c_int
            );
            // Even an empty Vec supplies a non-null, aligned output pointer.
            let mut actual = vec![0; expected.len()];
            assert_ne!(
                unsafe {
                    FPDFAnnot_GetFocusableSubtypes(self.form, actual.as_mut_ptr(), actual.len())
                },
                0
            );
            assert_eq!(actual, expected);
        }
        fn render_form(&self) -> Vec<u8> {
            let bitmap = FPDFBitmap_Create(300, 300, 1);
            assert!(!bitmap.is_null());
            unsafe {
                FPDFBitmap_FillRect(bitmap, 0, 0, 300, 300, 0xffffffff);
                FPDF_FFLDraw(self.form, bitmap, self.page, 0, 0, 300, 300, 0, 0);
                let result = std::slice::from_raw_parts(
                    FPDFBitmap_GetBuffer(bitmap).cast::<u8>(),
                    300 * 300 * 4,
                )
                .to_vec();
                FPDFBitmap_Destroy(bitmap);
                result
            }
        }
    }
    impl Drop for FormFixture {
        fn drop(&mut self) {
            unsafe {
                FORM_OnBeforeClosePage(self.page, self.form);
                FPDF_ClosePage(self.page);
                FPDFDOC_ExitFormFillEnvironment(self.form);
                FPDF_CloseDocument(self.document);
            }
            FPDF_DestroyLibrary();
        }
    }

    /// Complete behavioral assertions from
    /// FPDFAnnotEmbedderTest.FocusableAnnotSubtypes, plus integer preservation.
    #[test]
    fn focusable_subtype_api_preserves_configuration_and_controls_focus() {
        let fixture = FormFixture::open(
            include_bytes!("../../../upstream/pdfium/testing/resources/annots.pdf"),
            true,
        );
        fixture.assert_subtypes(&[20]);
        fixture.assert_focusability(&[20]);
        assert_ne!(
            unsafe { FPDFAnnot_SetFocusableSubtypes(fixture.form, std::ptr::null(), 0) },
            0
        );
        fixture.assert_subtypes(&[]);
        assert_ne!(
            unsafe { FPDFAnnot_SetFocusableSubtypes(fixture.form, [20].as_ptr(), 1) },
            0
        );
        fixture.assert_subtypes(&[20]);
        assert_ne!(
            unsafe { FPDFAnnot_SetFocusableSubtypes(fixture.form, [20].as_ptr(), 0) },
            0
        );
        fixture.assert_subtypes(&[]);
        fixture.assert_focusability(&[]);
        assert_ne!(
            unsafe { FPDFAnnot_SetFocusableSubtypes(fixture.form, [2].as_ptr(), 1) },
            0
        );
        fixture.assert_subtypes(&[2]);
        fixture.assert_focusability(&[2]);
        assert_eq!(
            unsafe { FPDFAnnot_SetFocusableSubtypes(std::ptr::null_mut(), [20].as_ptr(), 1) },
            0
        );
        assert_eq!(
            unsafe { FPDFAnnot_SetFocusableSubtypes(fixture.form, std::ptr::null(), 1) },
            0
        );
        assert_eq!(
            unsafe { FPDFAnnot_GetFocusableSubtypesCount(std::ptr::null_mut()) },
            -1
        );
        let mut actual = [77];
        assert_eq!(
            unsafe {
                FPDFAnnot_GetFocusableSubtypes(
                    std::ptr::null_mut(),
                    actual.as_mut_ptr(),
                    actual.len(),
                )
            },
            0
        );
        assert_eq!(
            unsafe { FPDFAnnot_GetFocusableSubtypes(fixture.form, std::ptr::null_mut(), 1) },
            0
        );
        assert_eq!(
            unsafe { FPDFAnnot_GetFocusableSubtypes(fixture.form, actual.as_mut_ptr(), 0) },
            0
        );
        assert_eq!(actual, [77]);
        let subtypes = [2, 9, 2, -7, c_int::MAX];
        assert_ne!(
            unsafe {
                FPDFAnnot_SetFocusableSubtypes(fixture.form, subtypes.as_ptr(), subtypes.len())
            },
            0
        );
        fixture.assert_subtypes(&subtypes);
    }

    /// Complete observable assertions from
    /// FPDFFormFillEmbedderTest.SetTextDirectionUpdatesRenderingOfFocusedTextField.
    #[test]
    fn text_direction_updates_focused_bidi_rendering_and_checks_arguments() {
        let fixture = FormFixture::open(
            include_bytes!("../../../upstream/pdfium/testing/resources/text_form.pdf"),
            false,
        );
        let annotation = unsafe { FPDFPage_GetAnnot(fixture.page, 0) };
        assert!(!annotation.is_null());
        assert_eq!(
            unsafe { FORM_GetTextDirection(fixture.form, annotation) },
            0
        );
        assert_eq!(
            unsafe { FORM_SetTextDirection(fixture.form, annotation, 3) },
            0
        );
        unsafe {
            FORM_OnAfterLoadPage(fixture.page, fixture.form);
        }
        assert_eq!(
            unsafe { FORM_GetTextDirection(std::ptr::null_mut(), annotation) },
            0
        );
        assert_eq!(
            unsafe { FORM_GetTextDirection(fixture.form, std::ptr::null_mut()) },
            0
        );
        assert_eq!(
            unsafe { FORM_SetTextDirection(fixture.form, annotation, 0) },
            0
        );
        assert_eq!(
            unsafe { FORM_SetTextDirection(fixture.form, annotation, -1) },
            0
        );
        assert_eq!(
            unsafe { FORM_SetTextDirection(fixture.form, std::ptr::null_mut(), 2) },
            0
        );
        assert_ne!(
            unsafe { FORM_OnFocus(fixture.form, fixture.page, 0, 120.0, 120.0) },
            0
        );
        for character in [
            u32::from('A'),
            u32::from(' '),
            0x05e9,
            u32::from(' '),
            0x05de,
            u32::from(' '),
            u32::from('B'),
        ] {
            assert_ne!(
                unsafe { FORM_OnChar(fixture.form, fixture.page, character as c_int, 0) },
                0
            );
        }
        assert_eq!(
            unsafe { FORM_GetTextDirection(fixture.form, annotation) },
            1
        );
        let auto = fixture.render_form();
        assert_ne!(
            unsafe { FORM_SetTextDirection(fixture.form, annotation, 3) },
            0
        );
        assert_eq!(
            unsafe { FORM_GetTextDirection(fixture.form, annotation) },
            3
        );
        let rtl = fixture.render_form();
        assert_ne!(
            unsafe { FORM_SetTextDirection(fixture.form, annotation, 2) },
            0
        );
        assert_eq!(
            unsafe { FORM_GetTextDirection(fixture.form, annotation) },
            2
        );
        let ltr = fixture.render_form();
        assert_ne!(auto, rtl);
        assert_ne!(rtl, ltr);
        assert_eq!(auto, ltr);
        assert_ne!(unsafe { FORM_ForceToKillFocus(fixture.form) }, 0);
        unsafe {
            FPDFPage_CloseAnnot(annotation);
        }
    }

    #[test]
    fn callback_can_exit_environment_while_the_entrypoint_retains_its_snapshot() {
        let document = FPDF_CreateNewDocument();
        let mut host = Host {
            info: unsafe { std::mem::zeroed() },
            handle: Cell::new(std::ptr::null_mut()),
            releases: Cell::new(0),
        };
        host.info.version = 2;
        host.info.Release = Some(release);
        host.info.FFI_OnChange = Some(close_during_change);
        let handle = unsafe { FPDFDOC_InitFormFillEnvironment(document, &mut host.info) };
        assert!(!handle.is_null());
        host.handle.set(handle);
        let (env, callbacks) = unsafe { snapshot(handle) }.unwrap();
        env.on_change();
        assert!(!env.is_alive());
        assert_eq!(host.releases.get(), 1);
        drop(env);
        drop(callbacks);
        assert_eq!(host.releases.get(), 1);
        unsafe {
            FPDF_CloseDocument(document);
        }
    }
}

#[cfg(all(test, feature = "xfa"))]
#[path = "formfill_xfa_tests.rs"]
mod xfa_integration_tests;
