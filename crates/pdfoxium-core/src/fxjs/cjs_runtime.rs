// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! SDK registrations, event/duplicate scopes, execution and shared-XFA global
//! services from `cjs_runtime.{h,cpp}` and `ijs_runtime.{h,cpp}`. Actual Boa
//! contexts/realms and weak host owners replace native isolate/template handles.
use super::cfxjs_engine::CfxJsEngine;
use super::cjs_app::CjsApp;
use super::cjs_event_context::{
    wide_from_utf16, CjsEventContext, FieldActionChannels, FieldReference,
};
use super::cjs_global::CjsGlobal;
use super::ijs_event_context::{EventRuntime, FieldEvent, JsError};
use crate::fpdfdoc::cpdf_aaction::AActionType;
use crate::fpdfdoc::cpdf_formfield::CpdfFormField;
use crate::fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
use crate::fpdfsdk::formfiller::cffl_fieldaction::CfflFieldAction;
use crate::fxcrt::fx_string::utf16_encode;
use boa_engine::{realm::Realm, Context, JsObject, JsResult};
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashSet;
use std::rc::{Rc, Weak};

struct RuntimeLink(Weak<CjsRuntime>);
pub struct CjsRuntime {
    self_weak: Weak<Self>,
    environment: Weak<CpdfSdkFormFillEnvironment>,
    engine: OnceCell<Rc<CfxJsEngine>>,
    realm: OnceCell<Realm>,
    app: OnceCell<Rc<CjsApp>>,
    global: OnceCell<Rc<CjsGlobal>>,
    events: RefCell<Vec<Rc<CjsEventContext>>>,
    field_events: RefCell<HashSet<FieldEvent>>,
    blocking: Cell<bool>,
}
/// The SDK and XFA share Boa's VM, while script entry retains its own realm.
/// This guard also encloses exception coercion, which can call either host.
struct RuntimeRealmScope<'a> {
    context: &'a mut Context,
    previous: Option<Realm>,
}
impl<'a> RuntimeRealmScope<'a> {
    fn new(context: &'a mut Context, realm: Realm) -> Self {
        let previous = context.enter_realm(realm);
        Self {
            context,
            previous: Some(previous),
        }
    }
    fn context(&mut self) -> &mut Context {
        self.context
    }
}
impl Drop for RuntimeRealmScope<'_> {
    fn drop(&mut self) {
        self.context
            .enter_realm(self.previous.take().expect("SDK realm restores once"));
    }
}
struct EventScope<'a>(&'a CjsRuntime);
impl Drop for EventScope<'_> {
    fn drop(&mut self) {
        self.0.events.borrow_mut().pop();
    }
}
impl CjsRuntime {
    pub fn new(environment: Weak<CpdfSdkFormFillEnvironment>) -> Result<Rc<Self>, JsError> {
        let runtime = Rc::new_cyclic(|this| Self {
            self_weak: this.clone(),
            environment,
            engine: OnceCell::new(),
            realm: OnceCell::new(),
            app: OnceCell::new(),
            global: OnceCell::new(),
            events: RefCell::new(Vec::new()),
            field_events: RefCell::new(HashSet::new()),
            blocking: Cell::new(false),
        });
        runtime
            .app
            .set(CjsApp::new(Rc::downgrade(&runtime)))
            .ok()
            .expect("new runtime app");
        let mut context = super::boa_adapters::new_context()
            .map_err(|error| JsError::text(&error.to_string()))?;
        if let Some(limit) = runtime
            .environment()
            .and_then(|env| env.host())
            .and_then(|host| host.javascript_loop_limit())
        {
            context.runtime_limits_mut().set_loop_iteration_limit(limit);
        }
        runtime
            .realm
            .set(context.realm().clone())
            .ok()
            .expect("new SDK realm");
        context.insert_data(RuntimeLink(Rc::downgrade(&runtime)));
        let registered = super::cjs_global::register_global(&mut context)
            .and_then(|global| {
                runtime
                    .global
                    .set(global)
                    .ok()
                    .expect("new runtime globals");
                Ok(())
            })
            .and_then(|_| super::cjs_constants::register_constants(&mut context))
            .and_then(|_| super::cjs_app::register_app(&mut context))
            .and_then(|_| super::cjs_color::register_color(&mut context))
            .and_then(|_| super::cjs_console::register_console(&mut context))
            .and_then(|_| super::cjs_event::register_event(&mut context))
            .and_then(|_| super::cjs_document::register_document(&mut context))
            .and_then(|_| super::cjs_util::register_util(&mut context))
            .and_then(|_| super::cjs_publicmethods::register_public_methods(&mut context));
        if let Err(error) = registered {
            return Err(Self::describe_error(error, &mut context));
        }
        runtime
            .engine
            .set(CfxJsEngine::new(context))
            .ok()
            .expect("new runtime engine");
        Ok(runtime)
    }
    pub fn from_context(context: &Context) -> Option<Rc<Self>> {
        context.get_data::<RuntimeLink>()?.0.upgrade()
    }
    pub fn engine(&self) -> &Rc<CfxJsEngine> {
        self.engine.get().expect("initialized engine")
    }
    pub fn app_state(&self) -> &Rc<CjsApp> {
        self.app.get().expect("initialized runtime app")
    }
    pub fn shutdown(&self) {
        self.app_state().shutdown();
    }
    pub fn environment(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
        self.environment.upgrade().filter(|env| env.is_alive())
    }
    /// Native IJS_Runtime::ScopedEventContext for XFA script entry. The event
    /// remains Unknown/uninitialized, shielding any suspended SDK event without
    /// changing Boa's current realm or pretending to execute a PDF event.
    #[cfg(feature = "xfa")]
    pub(crate) fn with_event_context<R>(&self, operation: impl FnOnce() -> R) -> R {
        self.events.borrow_mut().push(self.new_event_context());
        let _event = EventScope(self);
        operation()
    }
    /// XFA's named-property fallback enters the actual SDK realm on the shared
    /// VM. Native ScopedEventContext supplies a temporary, uninitialized event;
    /// no script is evaluated and no outer-script global commit is triggered.
    #[cfg(feature = "xfa")]
    fn with_global_context<R>(
        &self,
        context: &mut Context,
        operation: impl FnOnce(&mut Context, JsObject) -> JsResult<R>,
    ) -> JsResult<R> {
        if self.environment().is_none() {
            return Err(boa_engine::JsNativeError::reference()
                .with_message("Object no longer exists.")
                .into());
        }
        self.events.borrow_mut().push(self.new_event_context());
        let _event = EventScope(self);
        let mut realm = RuntimeRealmScope::new(
            context,
            self.realm.get().expect("initialized SDK realm").clone(),
        );
        let context = realm.context();
        let global = context.global_object();
        operation(context, global)
    }
    #[cfg(feature = "xfa")]
    pub(crate) fn get_value_by_name_in_context(
        &self,
        name: boa_engine::JsString,
        context: &mut Context,
    ) -> JsResult<boa_engine::JsValue> {
        self.with_global_context(context, |context, global| global.get(name, context))
    }
    #[cfg(feature = "xfa")]
    pub(crate) fn has_value_by_name_in_context(
        &self,
        name: boa_engine::JsString,
        context: &mut Context,
    ) -> JsResult<bool> {
        self.with_global_context(context, |context, global| {
            global.has_property(name, context)
        })
    }
    #[cfg(feature = "xfa")]
    pub(crate) fn set_value_by_name_in_context(
        &self,
        name: boa_engine::JsString,
        value: boa_engine::JsValue,
        context: &mut Context,
    ) -> JsResult<bool> {
        if name.is_empty() {
            return Ok(false);
        }
        self.with_global_context(context, |context, global| {
            global.set(name, value, false, context)
        })
    }
    pub fn new_event_context(&self) -> Rc<CjsEventContext> {
        let runtime: Rc<dyn EventRuntime> = self.self_weak.upgrade().expect("live runtime");
        CjsEventContext::new(Rc::downgrade(&runtime))
    }
    pub fn current_event_context(&self) -> Option<Rc<CjsEventContext>> {
        self.events.borrow().last().cloned()
    }
    pub fn field_object(context: &mut Context, reference: FieldReference) -> JsResult<JsObject> {
        super::cjs_field::field_object(context, reference)
    }
    pub fn is_blocking(&self) -> bool {
        self.blocking.get()
    }
    pub fn with_block<R>(&self, operation: impl FnOnce() -> R) -> R {
        struct Restore<'a>(&'a Cell<bool>, bool);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _restore = Restore(&self.blocking, self.blocking.replace(true));
        operation()
    }
    fn describe_error(error: boa_engine::JsError, context: &mut Context) -> JsError {
        // Keep opaque thrown values within Boa. At this outer SDK error boundary,
        // use the engine's string conversion; unavailable source coordinates are
        // represented by zero instead of fabricated V8 locations.
        let message = error
            .into_opaque(context)
            .and_then(|value| value.to_string(context))
            .map(|value| value.to_vec())
            .unwrap_or_else(|_| "JavaScript exception".encode_utf16().collect());
        JsError::new(0, 0, message)
    }
    pub fn run_document_script(&self, name: &[u32], script: &[u16]) -> Result<(), JsError> {
        let event = self.new_event_context();
        event.on_doc_open(&utf16_encode(name));
        event.run_script(script)
    }
    pub fn run_document_page_script(
        &self,
        kind: AActionType,
        script: &[u16],
    ) -> Result<(), JsError> {
        let event = self.new_event_context();
        match kind {
            AActionType::OpenPage | AActionType::PageOpen => event.on_page_open(),
            AActionType::ClosePage | AActionType::PageClose => event.on_page_close(),
            AActionType::PageVisible => event.on_page_in_view(),
            AActionType::PageInvisible => event.on_page_out_view(),
            AActionType::CloseDocument => event.on_doc_will_close(),
            AActionType::SaveDocument => event.on_doc_will_save(),
            AActionType::DocumentSaved => event.on_doc_did_save(),
            AActionType::PrintDocument => event.on_doc_will_print(),
            AActionType::DocumentPrinted => event.on_doc_did_print(),
            AActionType::DocumentOpen => event.on_doc_open(&[]),
            _ => return Err(JsError::text("Invalid document event.")),
        }
        event.run_script(script)
    }
    pub fn run_field_script(
        &self,
        field: &CpdfFormField,
        kind: AActionType,
        data: &mut CfflFieldAction,
        script: &[u16],
    ) -> Result<(), JsError> {
        let channels = FieldActionChannels::from_action(data);
        let event = self.new_event_context();
        match kind {
            AActionType::CursorEnter => {
                event.on_field_mouse_enter(data.b_modifier, data.b_shift, field)
            }
            AActionType::CursorExit => {
                event.on_field_mouse_exit(data.b_modifier, data.b_shift, field)
            }
            AActionType::ButtonDown => {
                event.on_field_mouse_down(data.b_modifier, data.b_shift, field)
            }
            AActionType::ButtonUp => event.on_field_mouse_up(data.b_modifier, data.b_shift, field),
            AActionType::GetFocus => {
                event.on_field_focus(data.b_modifier, data.b_shift, field, channels.value.clone())
            }
            AActionType::LoseFocus => {
                event.on_field_blur(data.b_modifier, data.b_shift, field, channels.value.clone())
            }
            AActionType::KeyStroke => event.on_field_keystroke(field, &channels),
            AActionType::Validate => event.on_field_validate(field, &channels),
            _ => return Err(JsError::text("Invalid field event.")),
        }
        let result = event.run_script(script);
        // Native event setters mutate references immediately, including before
        // a later exception. Return those changes before reporting the failure.
        channels.write_back(data);
        result
    }
    pub fn run_calculate_script(
        &self,
        source: Option<&CpdfFormField>,
        target: &CpdfFormField,
        value: &mut Vec<u32>,
        rc: &mut bool,
        script: &[u16],
    ) -> Result<(), JsError> {
        let channel = Rc::new(RefCell::new(utf16_encode(value)));
        let accepted = Rc::new(Cell::new(*rc));
        let event = self.new_event_context();
        event.on_field_calculate(source, target, channel.clone(), accepted.clone());
        let result = event.run_script(script);
        *value = wide_from_utf16(&channel.borrow());
        *rc = accepted.get();
        result
    }
    pub fn run_format_script(
        &self,
        field: &CpdfFormField,
        value: &mut Vec<u32>,
        script: &[u16],
    ) -> Result<(), JsError> {
        let channel = Rc::new(RefCell::new(utf16_encode(value)));
        let event = self.new_event_context();
        event.on_field_format(field, channel.clone());
        let result = event.run_script(script);
        *value = wide_from_utf16(&channel.borrow());
        result
    }
}
impl EventRuntime for CjsRuntime {
    fn execute_script(&self, script: &[u16], event: Rc<CjsEventContext>) -> Result<(), JsError> {
        if self.environment().is_none() {
            return Err(JsError::text("Object no longer exists."));
        }
        self.events.borrow_mut().push(event);
        let _scope = EventScope(self);
        self.engine()
            .with_context(|context| {
                let mut realm = RuntimeRealmScope::new(
                    context,
                    self.realm.get().expect("initialized SDK realm").clone(),
                );
                let context = realm.context();
                // Error conversion can execute user toString. It remains inside the
                // active context/event scope and preserves its reentry capability.
                let result = context
                    .eval(boa_engine::Source::from_utf16(script))
                    .map(|_| ());
                // Publish a complete outer script's globals without holding a
                // runtime/store borrow across snapshot getters or nested script.
                let outermost = self.events.borrow().len() == 1;
                let result = if outermost && self.environment().is_some() {
                    let committed = super::cjs_global::commit_global(context);
                    result.and(committed)
                } else {
                    result
                };
                Ok(result.map_err(|error| Self::describe_error(error, context)))
            })
            .map_err(|_| JsError::text("JavaScript context is unavailable."))?
    }
    fn add_event(&self, event: FieldEvent) -> bool {
        self.field_events.borrow_mut().insert(event)
    }
    fn remove_event(&self, event: &FieldEvent) {
        self.field_events.borrow_mut().remove(event);
    }
    fn environment(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
        self.environment()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
    use boa_engine::JsValue;

    fn fixture() -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<CpdfSdkFormFillEnvironment>,
        Rc<CjsRuntime>,
    ) {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let environment = CpdfSdkFormFillEnvironment::new(&document, None);
        let runtime = environment.get_or_create_runtime().unwrap();
        (document, environment, runtime)
    }
    fn run(runtime: &CjsRuntime, script: &str) -> Result<(), JsError> {
        runtime.run_document_script(&[], &script.encode_utf16().collect::<Vec<_>>())
    }
    #[test]
    fn outer_scripts_publish_globals_to_new_document_runtimes() {
        let (_first_doc, _first_env, first) = fixture();
        run(
            &first,
            r#"
            global.shared=17; global.setPersistent('shared',true);
            global.details={checked:true, nested:{text:'kept'}};
            global.cyclic={answer:42}; global.cyclic.self=global.cyclic;
            if(global.cyclic.self !== global.cyclic) throw 'lost live identity';
        "#,
        )
        .unwrap();
        let (_second_doc, _second_env, second) = fixture();
        run(
            &second,
            r#"
            if(global.shared !== 17 || global.details.checked !== true ||
               global.details.nested.text !== 'kept' || global.cyclic.answer !== 42)
                throw 'lost shared globals';
        "#,
        )
        .unwrap();
        run(&first, "delete global.shared").unwrap();
        let (_third_doc, _third_env, third) = fixture();
        run(
            &third,
            "if('shared' in global) throw 'deleted global restored'",
        )
        .unwrap();
    }
    #[test]
    fn global_snapshot_getters_reenter_with_outer_error_priority() {
        fn reenter(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            let runtime = CjsRuntime::from_context(context).unwrap();
            CfxJsEngine::with_host_callback(context, || {
                runtime
                    .run_document_page_script(
                        AActionType::PageOpen,
                        &"globalThis.reentered=(globalThis.reentered||0)+1"
                            .encode_utf16()
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
            });
            Ok(1.into())
        }
        let (_document, _environment, runtime) = fixture();
        runtime
            .engine()
            .define_global_method(
                &"__reenterGlobal".encode_utf16().collect::<Vec<_>>(),
                0,
                reenter,
            )
            .unwrap();
        run(
            &runtime,
            "global.object={get value(){return __reenterGlobal()}}",
        )
        .unwrap();
        assert_eq!(
            runtime
                .engine()
                .eval(&"reentered".encode_utf16().collect::<Vec<_>>())
                .unwrap(),
            1.into()
        );
        let error = run(&runtime, "delete global.object; global.bad={get value(){throw 'snapshot failure'}}; throw 'script failure'").unwrap_err();
        assert_eq!(String::from_utf16_lossy(&error.message), "script failure");
        let error = run(&runtime, "0").unwrap_err();
        assert_eq!(String::from_utf16_lossy(&error.message), "snapshot failure");
        run(&runtime, "delete global.bad").unwrap();
        assert!(runtime.current_event_context().is_none());
    }
}

#[cfg(all(test, feature = "xfa"))]
#[path = "cjs_runtime_global_tests.rs"]
mod global_tests;
