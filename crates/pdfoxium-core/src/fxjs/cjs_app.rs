// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_app.{h,cpp}` for PDF and loaded XFA documents. All source methods, including
//! its deliberate unsupported errors and reader-UI no-ops, are represented.
use super::cfxjs_engine::CfxJsEngine;
use super::cjs_event_context::wide_from_utf16;
use super::cjs_field::binding_error;
use super::cjs_runtime::CjsRuntime;
use super::cjs_timerobj::{clear_timer, timer_object};
use super::global_timer::{GlobalTimer, TimerType};
use super::js_define::{expand_keyword_params, is_expanded_param_known};
use super::js_resources::{js_get_string_from_id, JsMessage};
use boa_engine::{
    js_string,
    native_function::NativeFunctionPointer,
    object::{builtins::JsArray, FunctionObjectBuilder, ObjectInitializer},
    property::Attribute,
    Context, JsData, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, Trace};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub struct CjsApp {
    runtime: Weak<CjsRuntime>,
    calculate: Cell<bool>,
    runtime_highlight: Cell<bool>,
    timers: RefCell<Vec<Rc<GlobalTimer>>>,
    stopped: Cell<bool>,
}
impl CjsApp {
    pub fn new(runtime: Weak<CjsRuntime>) -> Rc<Self> {
        Rc::new(Self {
            runtime,
            calculate: Cell::new(true),
            runtime_highlight: Cell::new(false),
            timers: RefCell::new(Vec::new()),
            stopped: Cell::new(false),
        })
    }
    pub fn shutdown(&self) {
        if self.stopped.replace(true) {
            return;
        }
        let timers = std::mem::take(&mut *self.timers.borrow_mut());
        for timer in timers {
            timer.cancel();
        }
    }
    pub fn cancel_timer(&self, timer: &Rc<GlobalTimer>) {
        // Remove ownership before calling an embedder that can reenter.
        let removed = {
            let mut timers = self.timers.borrow_mut();
            timers
                .iter()
                .position(|candidate| Rc::ptr_eq(candidate, timer))
                .map(|index| timers.remove(index))
        };
        if let Some(timer) = removed {
            timer.cancel();
        }
    }
    fn add_timer(&self, timer: Rc<GlobalTimer>) {
        if self.stopped.get()
            || self
                .runtime
                .upgrade()
                .and_then(|runtime| runtime.environment())
                .is_none()
        {
            timer.cancel();
        } else {
            self.timers.borrow_mut().push(timer);
        }
    }
    pub fn sys_path_to_pdf_path(path: &[u32]) -> Vec<u32> {
        std::iter::once(u32::from(b'/'))
            .chain(
                path.iter()
                    .copied()
                    .filter(|&c| c != u32::from(b':'))
                    .map(|c| {
                        if c == u32::from(b'\\') {
                            u32::from(b'/')
                        } else {
                            c
                        }
                    }),
            )
            .collect()
    }
}
impl Drop for CjsApp {
    fn drop(&mut self) {
        self.shutdown();
    }
}
#[derive(Trace, Finalize, JsData)]
struct AppBinding;
fn error(name: &str, message: JsMessage) -> boa_engine::JsError {
    binding_error("app", name, message)
}
fn runtime(context: &Context, name: &str) -> JsResult<Rc<CjsRuntime>> {
    CjsRuntime::from_context(context).ok_or_else(|| error(name, JsMessage::BadObjectError))
}
fn value_wide(value: &JsValue, context: &mut Context) -> JsResult<Vec<u32>> {
    Ok(wide_from_utf16(&value.to_string(context)?.to_vec()))
}
fn optional_wide(value: Option<&JsValue>, context: &mut Context) -> JsResult<Vec<u32>> {
    if is_expanded_param_known(value) {
        value_wide(value.expect("known value"), context)
    } else {
        Ok(Vec::new())
    }
}
fn text(value: &[u32]) -> JsValue {
    JsString::from(crate::fxcrt::fx_string::utf16_encode(value).as_slice()).into()
}

// JSMethod checks the receiver, whereas JSPropGetter/Setter check the
// property holder (which can be on a receiver's prototype chain).
fn bound_callback(callback: NativeFunctionPointer, holder: bool) -> NativeFunction {
    NativeFunction::from_copy_closure(move |this, args, context| {
        let mut object = this.as_object();
        while let Some(candidate) = object {
            if candidate.downcast_ref::<AppBinding>().is_some() {
                return callback(this, args, context);
            }
            if !holder {
                break;
            }
            object = candidate.prototype();
        }
        Ok(JsValue::undefined())
    })
}

pub fn register_app(context: &mut Context) -> JsResult<()> {
    type Callback = NativeFunctionPointer;
    let properties: [(&str, Callback, Callback); 12] = [
        ("activeDocs", get_active_docs, set_active_docs),
        ("calculate", get_calculate, set_calculate),
        ("formsVersion", get_forms_version, set_forms_version),
        ("fs", get_fs, set_fs),
        ("fullscreen", get_fullscreen, set_fullscreen),
        ("language", get_language, set_language),
        ("media", get_media, set_media),
        ("platform", get_platform, set_platform),
        (
            "runtimeHighlight",
            get_runtime_highlight,
            set_runtime_highlight,
        ),
        ("viewerType", get_viewer_type, set_viewer_type),
        (
            "viewerVariation",
            get_viewer_variation,
            set_viewer_variation,
        ),
        ("viewerVersion", get_viewer_version, set_viewer_version),
    ];
    let properties: Vec<_> = properties
        .into_iter()
        .map(|(name, get, set)| {
            (
                JsString::from(name),
                bound_callback(get, true).to_js_function(context.realm()),
                bound_callback(set, true).to_js_function(context.realm()),
            )
        })
        .collect();
    let methods: [(&str, Callback); 21] = [
        ("alert", alert),
        ("beep", beep),
        ("browseForDoc", browse_for_doc),
        ("clearInterval", clear_interval),
        ("clearTimeOut", clear_time_out),
        ("execDialog", exec_dialog),
        ("execMenuItem", exec_menu_item),
        ("findComponent", find_component),
        ("goBack", go_back),
        ("goForward", go_forward),
        ("launchURL", launch_url),
        ("mailMsg", mail_msg),
        ("newFDF", new_fdf),
        ("newDoc", new_doc),
        ("openDoc", open_doc),
        ("openFDF", open_fdf),
        ("popUpMenuEx", pop_up_menu_ex),
        ("popUpMenu", pop_up_menu),
        ("response", response),
        ("setInterval", set_interval),
        ("setTimeOut", set_time_out),
    ];
    let methods: Vec<_> = methods
        .into_iter()
        .map(|(name, callback)| {
            let name = JsString::from(name);
            let function =
                FunctionObjectBuilder::new(context.realm(), bound_callback(callback, false))
                    .name(name.clone())
                    .length(0)
                    .constructor(false)
                    .build();
            (name, function)
        })
        .collect();
    let mut object = ObjectInitializer::with_native_data(AppBinding, context);
    for (name, get, set) in properties {
        object.accessor(name, Some(get), Some(set), Attribute::all());
    }
    for (name, function) in methods {
        object.property(
            name,
            function,
            Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
        );
    }
    let object = object.build();
    context.register_global_property(js_string!("app"), object, Attribute::all())
}
macro_rules! unsupported {
    ($($function:ident => $name:literal),* $(,)?) => {$ (
        fn $function(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> { Err(error($name, JsMessage::NotSupportedError)) }
    )*};
}
unsupported! {
    set_active_docs => "activeDocs", set_forms_version => "formsVersion", get_fs => "fs", set_fs => "fs",
    get_fullscreen => "fullscreen", set_fullscreen => "fullscreen", set_language => "language",
    get_media => "media", set_media => "media", set_platform => "platform", set_viewer_type => "viewerType",
    set_viewer_variation => "viewerVariation", set_viewer_version => "viewerVersion",
    exec_menu_item => "execMenuItem", new_doc => "newDoc", open_doc => "openDoc",
    pop_up_menu_ex => "popUpMenuEx", pop_up_menu => "popUpMenu",
}
macro_rules! source_noops {
    ($($function:ident),* $(,)?) => {$ (
        fn $function(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> { Ok(JsValue::undefined()) }
    )*};
}
// These methods deliberately succeed without action in upstream PDFium.
source_noops! { browse_for_doc, exec_dialog, find_component, go_back, go_forward, launch_url, new_fdf, open_fdf }
fn get_active_docs(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(JsArray::from_iter([JsValue::from(context.global_object())], context).into())
}
fn get_calculate(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(runtime(context, "calculate")?
        .app_state()
        .calculate
        .get()
        .into())
}
fn set_calculate(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let value = args.first().unwrap_or(&JsValue::undefined()).to_boolean();
    let runtime = runtime(context, "calculate")?;
    runtime.app_state().calculate.set(value);
    if let Some(environment) = runtime.environment() {
        environment.interactive_form().enable_calculate(value);
    }
    Ok(JsValue::undefined())
}
fn get_runtime_highlight(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    Ok(runtime(context, "runtimeHighlight")?
        .app_state()
        .runtime_highlight
        .get()
        .into())
}
fn set_runtime_highlight(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    runtime(context, "runtimeHighlight")?
        .app_state()
        .runtime_highlight
        .set(args.first().unwrap_or(&JsValue::undefined()).to_boolean());
    Ok(JsValue::undefined())
}
fn get_forms_version(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(7.into())
}
fn get_viewer_type(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(js_string!("pdfium").into())
}
fn get_viewer_variation(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok(js_string!("Full").into())
}
fn get_viewer_version(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(context, "viewerVersion")?;
    #[cfg(feature = "xfa")]
    let version = if runtime
        .environment()
        .and_then(|env| env.xfa_context())
        .is_some_and(|ctx| ctx.contains_extension_form())
    {
        11
    } else {
        8
    };
    #[cfg(not(feature = "xfa"))]
    let version = {
        let _ = runtime;
        8
    };
    Ok(version.into())
}
fn get_platform(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(context, "platform")?;
    let value = CfxJsEngine::with_host_callback(context, || {
        runtime
            .environment()
            .map(|environment| environment.get_platform())
            .unwrap_or_default()
    });
    if value.is_empty() {
        Ok(js_string!("WIN").into())
    } else {
        Ok(text(&value))
    }
}
fn get_language(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(context, "language")?;
    let value = CfxJsEngine::with_host_callback(context, || {
        runtime
            .environment()
            .map(|environment| environment.get_language())
            .unwrap_or_default()
    });
    if value.is_empty() {
        Ok(js_string!("ENU").into())
    } else {
        Ok(text(&value))
    }
}
fn alert(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let params = expand_keyword_params(context, args, &["cMsg", "nIcon", "nType", "cTitle"])?;
    if !is_expanded_param_known(params[0].as_ref()) {
        return Err(error("alert", JsMessage::ParamError));
    }
    let runtime = runtime(context, "alert")?;
    let Some(environment) = runtime.environment() else {
        return Ok(0.into());
    };
    let value = params[0].as_ref().expect("message parameter");
    let message = if let Some(array) = value.as_object().filter(|object| object.is_array()) {
        let array = JsArray::from_object(array)?;
        let mut result = vec![u32::from(b'[')];
        let mut index = 0;
        // The source reads length again after each user conversion.
        while index < array.length(context)? {
            if index > 0 {
                result.extend([u32::from(b','), u32::from(b' ')]);
            }
            let value = array.get(index, context)?;
            result.extend(value_wide(&value, context)?);
            index += 1;
        }
        result.push(u32::from(b']'));
        result
    } else {
        value_wide(value, context)?
    };
    let icon = if is_expanded_param_known(params[1].as_ref()) {
        params[1].as_ref().unwrap().to_i32(context)?
    } else {
        0
    };
    let kind = if is_expanded_param_known(params[2].as_ref()) {
        params[2].as_ref().unwrap().to_i32(context)?
    } else {
        0
    };
    let title = if is_expanded_param_known(params[3].as_ref()) {
        value_wide(params[3].as_ref().unwrap(), context)?
    } else {
        wide_from_utf16(&js_get_string_from_id(JsMessage::Alert))
    };
    let result = CfxJsEngine::with_host_callback(context, || {
        runtime.with_block(|| {
            environment.kill_focus_annot(0);
            if environment.is_alive() {
                environment.js_app_alert(&message, &title, kind, icon)
            } else {
                -1
            }
        })
    });
    Ok(result.into())
}
fn beep(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(error("beep", JsMessage::ParamError));
    }
    let kind = if is_expanded_param_known(args.first()) {
        args[0].to_i32(context)?
    } else {
        0
    };
    let runtime = runtime(context, "beep")?;
    CfxJsEngine::with_host_callback(context, || {
        if let Some(environment) = runtime.environment() {
            environment.js_app_beep(kind);
        }
    });
    Ok(JsValue::undefined())
}
fn response(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let params = expand_keyword_params(
        context,
        args,
        &["cQuestion", "cTitle", "cDefault", "bPassword", "cLabel"],
    )?;
    if !is_expanded_param_known(params[0].as_ref()) {
        return Err(error("response", JsMessage::ParamError));
    }
    let question = value_wide(params[0].as_ref().unwrap(), context)?;
    let title = if is_expanded_param_known(params[1].as_ref()) {
        value_wide(params[1].as_ref().unwrap(), context)?
    } else {
        "PDF".chars().map(u32::from).collect()
    };
    let default = optional_wide(params[2].as_ref(), context)?;
    let password =
        is_expanded_param_known(params[3].as_ref()) && params[3].as_ref().unwrap().to_boolean();
    let label = optional_wide(params[4].as_ref(), context)?;
    let runtime = runtime(context, "response")?;
    let mut buffer = [0u8; 2048];
    let length = CfxJsEngine::with_host_callback(context, || {
        runtime.environment().map_or(-1, |environment| {
            environment.js_app_response(&question, &title, &default, &label, password, &mut buffer)
        })
    });
    if !(0..=2048).contains(&length) {
        return Err(error("response", JsMessage::ParamTooLongError));
    }
    let words: Vec<u16> = buffer[..length as usize]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Ok(JsString::from(words.as_slice()).into())
}
fn mail_msg(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let params = expand_keyword_params(
        context,
        args,
        &["bUI", "cTo", "cCc", "cBcc", "cSubject", "cMsg"],
    )?;
    if !is_expanded_param_known(params[0].as_ref()) {
        return Err(error("mailMsg", JsMessage::ParamError));
    }
    let ui = params[0].as_ref().unwrap().to_boolean();
    if !ui && !is_expanded_param_known(params[1].as_ref()) {
        return Err(error("mailMsg", JsMessage::ParamError));
    }
    let to = optional_wide(params[1].as_ref(), context)?;
    let cc = optional_wide(params[2].as_ref(), context)?;
    let bcc = optional_wide(params[3].as_ref(), context)?;
    let subject = optional_wide(params[4].as_ref(), context)?;
    let message = optional_wide(params[5].as_ref(), context)?;
    let runtime = runtime(context, "mailMsg")?;
    CfxJsEngine::with_host_callback(context, || {
        runtime.with_block(|| {
            if let Some(environment) = runtime.environment() {
                environment.js_doc_mail_form(&[], ui, &to, &subject, &cc, &bcc, &message);
            }
        })
    });
    Ok(JsValue::undefined())
}
fn set_timer(
    args: &[JsValue],
    context: &mut Context,
    kind: TimerType,
    name: &str,
) -> JsResult<JsValue> {
    if args.is_empty() || args.len() > 2 {
        return Err(error(name, JsMessage::ParamError));
    }
    let script = args[0].to_string(context)?.to_vec();
    if script.is_empty() {
        return Err(error(name, JsMessage::InvalidInputError));
    }
    let interval = if args.len() > 1 {
        args[1].to_i32(context)? as u32
    } else {
        1000
    };
    let runtime = runtime(context, name)?;
    if runtime.environment().is_none() {
        return Err(error(name, JsMessage::BadObjectError));
    }
    let app = runtime.app_state().clone();
    let timeout = if kind == TimerType::OneShot {
        interval
    } else {
        0
    };
    let timer = CfxJsEngine::with_host_callback(context, || {
        let timer = GlobalTimer::new(&app, &runtime, kind, script, interval, timeout);
        app.add_timer(timer.clone());
        timer
    });
    Ok(timer_object(context, &timer).into())
}
fn set_interval(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    set_timer(args, context, TimerType::Repeating, "setInterval")
}
fn set_time_out(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    set_timer(args, context, TimerType::OneShot, "setTimeOut")
}
fn clear_timer_common(args: &[JsValue], context: &mut Context, name: &str) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(error(name, JsMessage::ParamError));
    }
    CfxJsEngine::with_host_callback(context, || clear_timer(&args[0]));
    Ok(JsValue::undefined())
}
fn clear_interval(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    clear_timer_common(args, context, "clearInterval")
}
fn clear_time_out(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    clear_timer_common(args, context, "clearTimeOut")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::{CpdfSdkFormFillEnvironment, FormFillHost};
    use std::collections::BTreeMap;

    type TimerCallback = Rc<dyn Fn(i32)>;
    #[derive(Default)]
    struct Host {
        next_id: Cell<i32>,
        timers: RefCell<BTreeMap<i32, (i32, TimerCallback)>>,
        killed: RefCell<Vec<i32>>,
        alerts: RefCell<Vec<(Vec<u32>, Vec<u32>, i32, i32)>>,
        responses: RefCell<Vec<(Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, bool)>>,
        response_length: Cell<Option<i32>>,
        beeps: RefCell<Vec<i32>>,
        mail: RefCell<Vec<(bool, Vec<Vec<u32>>)>>,
        fire_on_alert: Cell<bool>,
        fire_on_beep: Cell<bool>,
        fire_on_response: Cell<bool>,
        detach_on_alert: Cell<bool>,
        environment: RefCell<Weak<CpdfSdkFormFillEnvironment>>,
    }
    impl Host {
        fn fire(&self, id: i32) {
            let callback = self
                .timers
                .borrow()
                .get(&id)
                .map(|(_, callback)| callback.clone());
            if let Some(callback) = callback {
                callback(id);
            }
        }
        fn fire_all(&self) {
            let callbacks: Vec<_> = self
                .timers
                .borrow()
                .iter()
                .map(|(&id, (_, callback))| (id, callback.clone()))
                .collect();
            for (id, callback) in callbacks {
                callback(id);
            }
        }
    }
    impl FormFillHost for Host {
        fn set_timer(&self, interval: i32, callback: TimerCallback) -> i32 {
            let id = self.next_id.get() + 1;
            self.next_id.set(id);
            self.timers.borrow_mut().insert(id, (interval, callback));
            id
        }
        fn kill_timer(&self, id: i32) {
            self.timers.borrow_mut().remove(&id);
            self.killed.borrow_mut().push(id);
        }
        fn js_app_alert(&self, message: &[u32], title: &[u32], kind: i32, icon: i32) -> i32 {
            self.alerts
                .borrow_mut()
                .push((message.to_vec(), title.to_vec(), kind, icon));
            if self.fire_on_alert.get() {
                self.fire_all();
            }
            if self.detach_on_alert.get() {
                let environment = self.environment.borrow().upgrade();
                if let Some(environment) = environment {
                    environment.shutdown();
                }
            }
            4
        }
        fn js_app_beep(&self, kind: i32) {
            self.beeps.borrow_mut().push(kind);
            if self.fire_on_beep.get() {
                self.fire_all();
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
            self.responses.borrow_mut().push((
                question.to_vec(),
                title.to_vec(),
                default.to_vec(),
                label.to_vec(),
                password,
            ));
            assert_eq!(response.len(), 2048);
            if self.fire_on_response.get() {
                self.fire_all();
            }
            let words = [0x0041u16, 0xd800, 0, 0xd834, 0xdd1e];
            for (pair, word) in response.chunks_exact_mut(2).zip(words) {
                pair.copy_from_slice(&word.to_le_bytes());
            }
            self.response_length.get().unwrap_or(10)
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
            assert!(data.is_empty());
            self.mail.borrow_mut().push((
                ui,
                [to, subject, cc, bcc, message]
                    .map(<[u32]>::to_vec)
                    .to_vec(),
            ));
        }
        fn get_platform(&self) -> Vec<u32> {
            wide("Rust host")
        }
        fn get_language(&self) -> Vec<u32> {
            wide("DEU")
        }
    }
    struct Session {
        _document: Rc<RefCell<CpdfDocument>>,
        environment: Rc<CpdfSdkFormFillEnvironment>,
        runtime: Rc<CjsRuntime>,
        host: Rc<Host>,
    }
    impl Session {
        fn new() -> Self {
            let document = Rc::new(RefCell::new(CpdfDocument::new()));
            let host = Rc::new(Host::default());
            let callbacks: Rc<dyn FormFillHost> = host.clone();
            let environment = CpdfSdkFormFillEnvironment::new(&document, Some(&callbacks));
            *host.environment.borrow_mut() = Rc::downgrade(&environment);
            let runtime = environment.get_or_create_runtime().unwrap();
            Self {
                _document: document,
                environment,
                runtime,
                host,
            }
        }
        fn eval(&self, source: &str) -> JsValue {
            self.runtime
                .engine()
                .eval(&source.encode_utf16().collect::<Vec<_>>())
                .unwrap()
        }
        fn number(&self, source: &str) -> f64 {
            self.eval(source).as_number().unwrap()
        }
    }
    impl Drop for Session {
        fn drop(&mut self) {
            self.environment.shutdown();
        }
    }
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }

    #[test]
    fn app_properties_supported_errors_and_source_noops() {
        let session = Session::new();
        assert_eq!(session.eval("JSON.stringify([app.formsVersion,app.viewerType,app.viewerVariation,app.viewerVersion,app.platform,app.language,app.calculate,app.runtimeHighlight,app.activeDocs[0]===globalThis]);").as_string().unwrap(), js_string!("[7,\"pdfium\",\"Full\",8,\"Rust host\",\"DEU\",true,false,true]"));
        session.eval("app.calculate=0; app.runtimeHighlight='yes';");
        assert!(!session
            .environment
            .interactive_form()
            .is_calculate_enabled());
        assert_eq!(
            session
                .eval("!app.calculate && app.runtimeHighlight;")
                .as_boolean(),
            Some(true)
        );
        assert_eq!(session.eval("['fs','fullscreen','media'].every(k=>{try{app[k];return false}catch(e){return typeof e==='string'&&e.includes('Operation not supported')}});").as_boolean(), Some(true));
        assert_eq!(session.eval("['activeDocs','formsVersion','fs','fullscreen','language','media','platform','viewerType','viewerVariation','viewerVersion'].every(k=>{try{app[k]=12;return false}catch(e){return e.includes('Operation not supported')}});").as_boolean(), Some(true));
        assert_eq!(session.eval("['execMenuItem','newDoc','openDoc','popUpMenuEx','popUpMenu'].every(k=>{try{app[k]();return false}catch(e){return e.includes('Operation not supported')}});").as_boolean(), Some(true));
        assert_eq!(session.eval("let poison={toString(){throw 9}}; ['browseForDoc','execDialog','findComponent','goBack','goForward','launchURL','newFDF','openFDF'].every(k=>app[k](poison)===undefined);").as_boolean(), Some(true));
        assert_eq!(session.eval("app.alert.call({},'ignored')===undefined && Object.create(app).formsVersion===7;").as_boolean(), Some(true));
        assert!(session.host.alerts.borrow().is_empty());
        assert_eq!(
            CjsApp::sys_path_to_pdf_path(&wide("C:\\path\\file.pdf")),
            wide("/C/path/file.pdf")
        );
        assert_eq!(
            CjsApp::sys_path_to_pdf_path(&wide("/tmp/色.pdf")),
            wide("//tmp/色.pdf")
        );
    }

    #[test]
    fn host_alert_response_beep_and_mail_keep_keyword_coercion_and_utf16() {
        let session = Session::new();
        assert_eq!(
            session.number("app.alert({cMsg:[1,'two',null],nIcon:2,nType:3,cTitle:'Title'});"),
            4.0
        );
        assert_eq!(
            *session.host.alerts.borrow(),
            [(wide("[1, two, null]"), wide("Title"), 3, 2)]
        );
        session.eval("app.alert('plain'); app.beep(undefined); app.beep(4); app.mailMsg({bUI:false,cTo:'to',cCc:'cc',cBcc:'bcc',cSubject:'subject',cMsg:'message'});");
        assert_eq!(
            session.host.alerts.borrow()[1],
            (wide("plain"), wide("Alert"), 0, 0)
        );
        assert_eq!(*session.host.beeps.borrow(), [0, 4]);
        assert_eq!(
            *session.host.mail.borrow(),
            [(
                false,
                vec![
                    wide("to"),
                    wide("subject"),
                    wide("cc"),
                    wide("bcc"),
                    wide("message")
                ]
            )]
        );
        assert_eq!(session.eval("app.response({cQuestion:'Question',cTitle:'Title',cDefault:'Default',bPassword:true,cLabel:'Label'});").as_string().unwrap().to_vec(), vec![0x41, 0xd800, 0, 0xd834, 0xdd1e]);
        assert_eq!(
            *session.host.responses.borrow(),
            [(
                wide("Question"),
                wide("Title"),
                wide("Default"),
                wide("Label"),
                true
            )]
        );
        session.eval("app.response('Q');");
        assert_eq!(
            session.host.responses.borrow()[1],
            (wide("Q"), wide("PDF"), vec![], vec![], false)
        );
        session.host.response_length.set(Some(2049));
        assert_eq!(
            session
                .eval("try{app.response('Q')}catch(e){e.includes('too long')}")
                .as_boolean(),
            Some(true)
        );
        session.host.response_length.set(Some(-1));
        assert_eq!(
            session
                .eval("try{app.response('Q')}catch(e){e.includes('too long')}")
                .as_boolean(),
            Some(true)
        );
        assert_eq!(session.eval("[()=>app.alert(),()=>app.response(),()=>app.beep(),()=>app.mailMsg(false)].every(f=>{try{f();return false}catch(e){return e.includes('Incorrect number')}});").as_boolean(), Some(true));
        assert_eq!(
            session
                .eval("try{app.alert({cMsg:{toString(){throw 19}}})}catch(e){e===19}")
                .as_boolean(),
            Some(true)
        );
    }

    #[test]
    fn timers_are_host_driven_weak_handles_reentrant_and_cancellable() {
        let session = Session::new();
        session.eval("var ticks=0,external=false; var repeating=app.setInterval('ticks++; external=event.name===\"Exec\"&&event.type===\"External\";app.beep(1);',25); var once=app.setTimeOut('ticks+=10;',50);");
        assert_eq!(
            session
                .host
                .timers
                .borrow()
                .iter()
                .map(|(&id, (interval, _))| (id, *interval))
                .collect::<Vec<_>>(),
            [(1, 25), (2, 50)]
        );
        // While the first timer is processing, beep reenters the same host
        // timer once. processing_ prevents recursion. The nested one-shot
        // encounters the existing ExternalExec event, so the source duplicate-
        // event guard rejects its script; Trigger still cancels that one-shot.
        session.host.fire_on_beep.set(true);
        session.host.fire(1);
        session.host.fire_on_beep.set(false);
        assert_eq!(session.number("ticks;"), 1.0);
        assert_eq!(session.eval("external;").as_boolean(), Some(true));
        assert_eq!(*session.host.killed.borrow(), [2]);
        session.host.fire(1);
        assert_eq!(session.number("ticks;"), 2.0);
        session.eval("app.clearTimeOut(repeating); app.clearInterval(once); app.clearInterval({}); app.clearTimeOut(7);");
        assert_eq!(*session.host.killed.borrow(), [2, 1]);
        session.eval("var zero=app.setTimeOut('ticks+=100;',0); var negative=app.setInterval('ticks+=2;',-1); app.setTimeOut('ticks+=3;');");
        assert_eq!(
            session
                .host
                .timers
                .borrow()
                .iter()
                .map(|(&id, (interval, _))| (id, *interval))
                .collect::<Vec<_>>(),
            [(3, 0), (4, -1), (5, 1000)]
        );
        boa_gc::force_collect();
        session.host.fire(3);
        assert_eq!(
            session.number("ticks;"),
            2.0,
            "zero timeout is cancelled without executing its script"
        );
        session.host.fire(4);
        session.host.fire(5);
        assert_eq!(
            session.number("ticks;"),
            7.0,
            "collecting a timer handle does not cancel it"
        );
        assert_eq!(session.eval("[()=>app.setInterval(),()=>app.setTimeOut('x',1,2),()=>app.clearInterval(),()=>app.clearTimeOut(1,2)].every(f=>{try{f();return false}catch(e){return e.includes('Incorrect number')}}) && (()=>{try{app.setInterval('');return false}catch(e){return e.includes('invalid')}})();").as_boolean(), Some(true));
        let stale = session.host.timers.borrow().get(&4).unwrap().1.clone();
        session.environment.shutdown();
        assert!(session.host.timers.borrow().is_empty());
        assert_eq!(
            session
                .host
                .killed
                .borrow()
                .iter()
                .filter(|&&id| id == 4)
                .count(),
            1
        );
        stale(4); // The embedder can retain an obsolete callback safely.
    }

    #[test]
    fn modal_blocking_and_environment_detachment_do_not_strand_runtime_or_timers() {
        let session = Session::new();
        session.eval("var ticks=0; var interval=app.setInterval('ticks++;',5); app.setTimeOut('ticks+=10;',5);");
        session.host.fire_on_alert.set(true);
        session.eval("app.alert('blocked');");
        assert_eq!(session.number("ticks;"), 0.0);
        assert_eq!(
            *session.host.killed.borrow(),
            [2],
            "blocked one-shot is still cancelled by source Trigger"
        );
        session.host.fire_on_response.set(true);
        session.eval("app.response('not blocked');");
        assert_eq!(
            session.number("ticks;"),
            1.0,
            "response has no source BeginBlock"
        );
        session.host.detach_on_alert.set(true);
        session.eval("app.alert('detach');");
        assert!(!session.environment.is_alive());
        assert!(session.host.timers.borrow().is_empty());
        assert!(!session.runtime.is_blocking());
        assert_eq!(
            session
                .host
                .killed
                .borrow()
                .iter()
                .filter(|&&id| id == 1)
                .count(),
            1
        );
    }
}
