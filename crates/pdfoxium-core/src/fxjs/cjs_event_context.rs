// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Event setup, aliased data, naming and RunScript lifetime from
//! `fxjs/cjs_event_context.{h,cpp}`. Native mutable string/int/bool pointers
//! become explicit shared channels; no engine or document borrow spans script
//! execution. SourceField/TargetField return binding descriptions: construction
//! of the actual JavaScript Field objects belongs to the runtime host bindings.
use super::ijs_event_context::{EventRuntime, FieldEvent, JsError};
use super::js_resources::{js_get_string_from_id, JsMessage};
use crate::fpdfdoc::cpdf_formfield::CpdfFormField;
use crate::fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
use crate::fpdfsdk::formfiller::cffl_fieldaction::CfflFieldAction;
use crate::fxcrt::fx_string::utf16_encode;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

pub type SharedString = Rc<RefCell<Vec<u16>>>;
pub type SharedBool = Rc<Cell<bool>>;
pub type SharedInt = Rc<Cell<i32>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Kind {
    Unknown,
    DocOpen,
    DocWillPrint,
    DocDidPrint,
    DocWillSave,
    DocDidSave,
    DocWillClose,
    PageOpen,
    PageClose,
    PageInView,
    PageOutView,
    FieldMouseDown,
    FieldMouseUp,
    FieldMouseEnter,
    FieldMouseExit,
    FieldFocus,
    FieldBlur,
    FieldKeystroke,
    FieldValidate,
    FieldCalculate,
    FieldFormat,
    ExternalExec,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::DocDidPrint => "DidPrint",
            Self::DocDidSave => "DidSave",
            Self::DocOpen | Self::PageOpen => "Open",
            Self::DocWillClose => "WillClose",
            Self::DocWillPrint => "WillPrint",
            Self::DocWillSave => "WillSave",
            Self::ExternalExec => "Exec",
            Self::FieldFocus => "Focus",
            Self::FieldBlur => "Blur",
            Self::FieldMouseDown => "Mouse Down",
            Self::FieldMouseUp => "Mouse Up",
            Self::FieldMouseEnter => "Mouse Enter",
            Self::FieldMouseExit => "Mouse Exit",
            Self::FieldCalculate => "Calculate",
            Self::FieldFormat => "Format",
            Self::FieldKeystroke => "Keystroke",
            Self::FieldValidate => "Validate",
            Self::PageClose => "Close",
            Self::PageInView => "InView",
            Self::PageOutView => "OutView",
            Self::Unknown => "",
        }
    }
    pub fn event_type(self) -> &'static str {
        match self {
            Self::DocOpen
            | Self::DocWillPrint
            | Self::DocDidPrint
            | Self::DocWillSave
            | Self::DocDidSave
            | Self::DocWillClose => "Doc",
            Self::ExternalExec => "External",
            Self::FieldMouseDown
            | Self::FieldMouseUp
            | Self::FieldMouseEnter
            | Self::FieldMouseExit
            | Self::FieldFocus
            | Self::FieldBlur
            | Self::FieldKeystroke
            | Self::FieldValidate
            | Self::FieldCalculate
            | Self::FieldFormat => "Field",
            Self::PageOpen | Self::PageClose | Self::PageInView | Self::PageOutView => "Page",
            Self::Unknown => "",
        }
    }
    pub fn is_user_gesture(self) -> bool {
        matches!(
            self,
            Self::FieldMouseDown | Self::FieldMouseUp | Self::FieldKeystroke
        )
    }
}

/// Script-visible state from one native CFFL_FieldAction. Readonly inputs are
/// owned values; writable event properties share their channels with the caller.
/// UTF-16 conversion preserves embedded NULs and isolated surrogate code units.
pub struct FieldActionChannels {
    pub modifier: bool,
    pub shift: bool,
    pub key_down: bool,
    pub will_commit: bool,
    pub field_full: bool,
    pub change: SharedString,
    pub change_ex: Rc<[u16]>,
    pub value: SharedString,
    pub rc: SharedBool,
    pub sel_start: SharedInt,
    pub sel_end: SharedInt,
}
impl FieldActionChannels {
    pub fn from_action(action: &CfflFieldAction) -> Self {
        Self {
            modifier: action.b_modifier,
            shift: action.b_shift,
            key_down: action.b_key_down,
            will_commit: action.b_will_commit,
            field_full: action.b_field_full,
            change: Rc::new(RefCell::new(utf16_encode(&action.s_change))),
            change_ex: utf16_encode(&action.s_change_ex).into(),
            value: Rc::new(RefCell::new(utf16_encode(&action.s_value))),
            rc: Rc::new(Cell::new(action.b_rc)),
            sel_start: Rc::new(Cell::new(action.n_sel_start)),
            sel_end: Rc::new(Cell::new(action.n_sel_end)),
        }
    }
    pub fn write_back(&self, action: &mut CfflFieldAction) {
        action.s_change = wide_from_utf16(&self.change.borrow());
        action.s_value = wide_from_utf16(&self.value.borrow());
        action.b_rc = self.rc.get();
        action.n_sel_start = self.sel_start.get();
        action.n_sel_end = self.sel_end.get();
    }
}
/// Preserve raw UTF-16 surrogates, as WideString does; String::from_utf16_lossy
/// would change script-visible values before they reached the PDF model.
pub fn wide_from_utf16(text: &[u16]) -> Vec<u32> {
    let mut result = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let high = u32::from(text[index]);
        if (0xd800..=0xdbff).contains(&high) {
            if let Some(&low) = text.get(index + 1) {
                let low = u32::from(low);
                if (0xdc00..=0xdfff).contains(&low) {
                    result.push(0x10000 + ((high - 0xd800) << 10) + low - 0xdc00);
                    index += 2;
                    continue;
                }
            }
        }
        result.push(high);
        index += 1;
    }
    result
}

struct EventData {
    kind: Kind,
    source_name: Rc<[u16]>,
    target_name: Rc<[u16]>,
    change: SharedString,
    change_ex: Rc<[u16]>,
    value: Option<SharedString>,
    commit_key: i32,
    key_down: bool,
    modifier: bool,
    shift: bool,
    sel_start: SharedInt,
    sel_end: SharedInt,
    will_commit: bool,
    field_full: bool,
    rc: SharedBool,
}
impl EventData {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            source_name: Rc::from([]),
            target_name: Rc::from([]),
            change: Rc::new(RefCell::new(Vec::new())),
            change_ex: Rc::from([]),
            value: None,
            commit_key: -1,
            key_down: false,
            modifier: false,
            shift: false,
            sel_start: Rc::new(Cell::new(0)),
            sel_end: Rc::new(Cell::new(0)),
            will_commit: false,
            field_full: false,
            rc: Rc::new(Cell::new(false)),
        }
    }
}

/// Description for the runtime to bind a real CJS_Field/CJS_Document pair.
/// This is not itself a stand-in JavaScript object.
#[derive(Clone)]
pub struct FieldReference {
    pub environment: Weak<CpdfSdkFormFillEnvironment>,
    pub name: Rc<[u16]>,
}

pub struct CjsEventContext {
    runtime: Weak<dyn EventRuntime>,
    environment: Weak<CpdfSdkFormFillEnvironment>,
    data: RefCell<EventData>,
    busy: Cell<bool>,
    valid: Cell<bool>,
}
struct BusyGuard<'a>(&'a Cell<bool>, bool);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}
struct RegisteredEvent<'a> {
    runtime: Rc<dyn EventRuntime>,
    context: &'a CjsEventContext,
    key: FieldEvent,
}
impl Drop for RegisteredEvent<'_> {
    fn drop(&mut self) {
        self.runtime.remove_event(&self.key);
        self.context.valid.set(false);
    }
}
impl CjsEventContext {
    pub fn new(runtime: Weak<dyn EventRuntime>) -> Rc<Self> {
        let environment = runtime
            .upgrade()
            .and_then(|runtime| runtime.environment())
            .map_or_else(Weak::new, |env| Rc::downgrade(&env));
        Rc::new(Self {
            runtime,
            environment,
            data: RefCell::new(EventData::new(Kind::Unknown)),
            busy: Cell::new(false),
            valid: Cell::new(false),
        })
    }
    pub fn run_script(self: &Rc<Self>, script: &[u16]) -> Result<(), JsError> {
        if self.busy.get() {
            return Err(JsError::new(
                1,
                1,
                js_get_string_from_id(JsMessage::BusyError),
            ));
        }
        let _busy = BusyGuard(&self.busy, self.busy.replace(true));
        if !self.valid.get() {
            return Err(JsError::text("Object no longer exists."));
        }
        let runtime = self
            .runtime
            .upgrade()
            .ok_or_else(|| JsError::text("Object no longer exists."))?;
        let key = FieldEvent {
            target_name: self.target_name(),
            kind: self.event_kind(),
        };
        if !runtime.add_event(key.clone()) {
            return Err(JsError::new(
                1,
                1,
                js_get_string_from_id(JsMessage::DuplicateEventError),
            ));
        }
        let _registered = RegisteredEvent {
            runtime: runtime.clone(),
            context: self,
            key,
        };
        if script.is_empty() {
            Ok(())
        } else {
            runtime.execute_script(script, self.clone())
        }
    }
    pub fn environment(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
        self.environment.upgrade().filter(|env| env.is_alive())
    }
    fn initialize(&self, kind: Kind) {
        *self.data.borrow_mut() = EventData::new(kind);
        self.valid.set(true);
    }
    pub fn destroy(&self) {
        self.valid.set(false);
    }
    pub fn event_kind(&self) -> Kind {
        self.data.borrow().kind
    }
    pub fn is_valid(&self) -> bool {
        self.valid.get()
    }
    pub fn is_busy(&self) -> bool {
        self.busy.get()
    }
    pub fn is_user_gesture(&self) -> bool {
        self.event_kind().is_user_gesture()
    }
    pub fn name(&self) -> &'static str {
        self.event_kind().name()
    }
    pub fn event_type(&self) -> &'static str {
        self.event_kind().event_type()
    }
    pub fn change(&self) -> SharedString {
        self.data.borrow().change.clone()
    }
    pub fn change_ex(&self) -> Rc<[u16]> {
        self.data.borrow().change_ex.clone()
    }
    pub fn source_name(&self) -> Rc<[u16]> {
        self.data.borrow().source_name.clone()
    }
    pub fn target_name(&self) -> Rc<[u16]> {
        self.data.borrow().target_name.clone()
    }
    pub fn commit_key(&self) -> i32 {
        self.data.borrow().commit_key
    }
    pub fn field_full(&self) -> bool {
        self.data.borrow().field_full
    }
    pub fn key_down(&self) -> bool {
        self.data.borrow().key_down
    }
    pub fn modifier(&self) -> bool {
        self.data.borrow().modifier
    }
    pub fn shift(&self) -> bool {
        self.data.borrow().shift
    }
    pub fn will_commit(&self) -> bool {
        self.data.borrow().will_commit
    }
    pub fn rc(&self) -> SharedBool {
        self.data.borrow().rc.clone()
    }
    pub fn sel_start(&self) -> i32 {
        self.data.borrow().sel_start.get()
    }
    pub fn sel_end(&self) -> i32 {
        self.data.borrow().sel_end.get()
    }
    pub fn set_sel_start(&self, value: i32) {
        self.data.borrow().sel_start.set(value);
    }
    pub fn set_sel_end(&self, value: i32) {
        self.data.borrow().sel_end.set(value);
    }
    pub fn value(&self) -> Option<SharedString> {
        self.data.borrow().value.clone()
    }
    pub fn has_value(&self) -> bool {
        self.data.borrow().value.is_some()
    }
    pub fn source_field(&self) -> Result<FieldReference, JsError> {
        self.field_reference(self.source_name())
    }
    pub fn target_field(&self) -> Result<FieldReference, JsError> {
        self.field_reference(self.target_name())
    }
    fn field_reference(&self, name: Rc<[u16]>) -> Result<FieldReference, JsError> {
        self.environment()
            .ok_or_else(|| JsError::text("Object no longer exists."))?;
        Ok(FieldReference {
            environment: self.environment.clone(),
            name,
        })
    }

    pub fn on_doc_open(&self, target_name: &[u16]) {
        self.initialize(Kind::DocOpen);
        self.data.borrow_mut().target_name = Rc::from(target_name);
    }
    pub fn on_doc_will_print(&self) {
        self.initialize(Kind::DocWillPrint);
    }
    pub fn on_doc_did_print(&self) {
        self.initialize(Kind::DocDidPrint);
    }
    pub fn on_doc_will_save(&self) {
        self.initialize(Kind::DocWillSave);
    }
    pub fn on_doc_did_save(&self) {
        self.initialize(Kind::DocDidSave);
    }
    pub fn on_doc_will_close(&self) {
        self.initialize(Kind::DocWillClose);
    }
    pub fn on_page_open(&self) {
        self.initialize(Kind::PageOpen);
    }
    pub fn on_page_close(&self) {
        self.initialize(Kind::PageClose);
    }
    pub fn on_page_in_view(&self) {
        self.initialize(Kind::PageInView);
    }
    pub fn on_page_out_view(&self) {
        self.initialize(Kind::PageOutView);
    }
    pub fn on_external_exec(&self) {
        self.initialize(Kind::ExternalExec);
    }
    fn mouse(&self, kind: Kind, modifier: bool, shift: bool, target: &CpdfFormField) {
        self.initialize(kind);
        let mut data = self.data.borrow_mut();
        data.modifier = modifier;
        data.shift = shift;
        data.target_name = utf16_encode(&target.full_name()).into();
    }
    pub fn on_field_mouse_enter(&self, modifier: bool, shift: bool, target: &CpdfFormField) {
        self.mouse(Kind::FieldMouseEnter, modifier, shift, target);
    }
    pub fn on_field_mouse_exit(&self, modifier: bool, shift: bool, target: &CpdfFormField) {
        self.mouse(Kind::FieldMouseExit, modifier, shift, target);
    }
    pub fn on_field_mouse_down(&self, modifier: bool, shift: bool, target: &CpdfFormField) {
        self.mouse(Kind::FieldMouseDown, modifier, shift, target);
    }
    pub fn on_field_mouse_up(&self, modifier: bool, shift: bool, target: &CpdfFormField) {
        self.mouse(Kind::FieldMouseUp, modifier, shift, target);
    }
    fn focus(
        &self,
        kind: Kind,
        modifier: bool,
        shift: bool,
        target: &CpdfFormField,
        value: SharedString,
    ) {
        self.mouse(kind, modifier, shift, target);
        self.data.borrow_mut().value = Some(value);
    }
    pub fn on_field_focus(
        &self,
        modifier: bool,
        shift: bool,
        target: &CpdfFormField,
        value: SharedString,
    ) {
        self.focus(Kind::FieldFocus, modifier, shift, target, value);
    }
    pub fn on_field_blur(
        &self,
        modifier: bool,
        shift: bool,
        target: &CpdfFormField,
        value: SharedString,
    ) {
        self.focus(Kind::FieldBlur, modifier, shift, target, value);
    }
    pub fn on_field_calculate(
        &self,
        source: Option<&CpdfFormField>,
        target: &CpdfFormField,
        value: SharedString,
        rc: SharedBool,
    ) {
        self.initialize(Kind::FieldCalculate);
        let mut data = self.data.borrow_mut();
        data.source_name = source.map_or_else(
            || Rc::from([]),
            |field| utf16_encode(&field.full_name()).into(),
        );
        data.target_name = utf16_encode(&target.full_name()).into();
        data.value = Some(value);
        data.rc = rc;
    }
    pub fn on_field_format(&self, target: &CpdfFormField, value: SharedString) {
        self.initialize(Kind::FieldFormat);
        let mut data = self.data.borrow_mut();
        data.commit_key = 0;
        data.target_name = utf16_encode(&target.full_name()).into();
        data.value = Some(value);
        data.will_commit = true;
    }
    fn field_action(&self, kind: Kind, target: &CpdfFormField, channels: &FieldActionChannels) {
        self.initialize(kind);
        let mut data = self.data.borrow_mut();
        data.target_name = utf16_encode(&target.full_name()).into();
        data.change = channels.change.clone();
        data.change_ex = channels.change_ex.clone();
        data.value = Some(channels.value.clone());
        data.rc = channels.rc.clone();
        data.key_down = channels.key_down;
        data.modifier = channels.modifier;
        data.shift = channels.shift;
        if kind == Kind::FieldKeystroke {
            data.commit_key = 0;
            data.sel_end = channels.sel_end.clone();
            data.sel_start = channels.sel_start.clone();
            data.will_commit = channels.will_commit;
            data.field_full = channels.field_full;
        }
    }
    pub fn on_field_keystroke(&self, target: &CpdfFormField, channels: &FieldActionChannels) {
        self.field_action(Kind::FieldKeystroke, target, channels);
    }
    pub fn on_field_validate(&self, target: &CpdfFormField, channels: &FieldActionChannels) {
        self.field_action(Kind::FieldValidate, target, channels);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{cpdf_document::CpdfDocument, cpdf_parser::ParserError};
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
    use std::collections::HashSet;
    type Callback = Rc<dyn Fn(Rc<CjsEventContext>) -> Result<(), JsError>>;
    struct RecordingRuntime {
        env: Option<Rc<CpdfSdkFormFillEnvironment>>,
        events: RefCell<HashSet<FieldEvent>>,
        calls: Cell<usize>,
        callback: RefCell<Option<Callback>>,
    }
    impl RecordingRuntime {
        fn new(env: Option<Rc<CpdfSdkFormFillEnvironment>>) -> Rc<Self> {
            Rc::new(Self {
                env,
                events: RefCell::new(HashSet::new()),
                calls: Cell::new(0),
                callback: RefCell::new(None),
            })
        }
        fn event(self: &Rc<Self>) -> Rc<CjsEventContext> {
            let runtime: Rc<dyn EventRuntime> = self.clone();
            CjsEventContext::new(Rc::downgrade(&runtime))
        }
    }
    impl EventRuntime for RecordingRuntime {
        fn execute_script(&self, _: &[u16], event: Rc<CjsEventContext>) -> Result<(), JsError> {
            self.calls.set(self.calls.get() + 1);
            let callback = self.callback.borrow().clone();
            callback.map_or(Ok(()), |callback| callback(event))
        }
        fn add_event(&self, event: FieldEvent) -> bool {
            self.events.borrow_mut().insert(event)
        }
        fn remove_event(&self, event: &FieldEvent) {
            self.events.borrow_mut().remove(event);
        }
        fn environment(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
            self.env.clone()
        }
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }
    fn fixture() -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<CpdfSdkFormFillEnvironment>,
        Rc<CpdfFormField>,
    ) {
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
        let env = CpdfSdkFormFillEnvironment::new(&document, None);
        let field = env
            .interactive_form()
            .core_form()
            .get_field(0, &[])
            .unwrap();
        (document, env, field)
    }
    #[test]
    fn complete_kind_name_type_and_user_gesture_mapping_and_reset() {
        use Kind::*;
        let runtime = RecordingRuntime::new(None);
        let event = runtime.event();
        for (kind, name, ty, gesture) in [
            (Unknown, "", "", false),
            (DocOpen, "Open", "Doc", false),
            (DocWillPrint, "WillPrint", "Doc", false),
            (DocDidPrint, "DidPrint", "Doc", false),
            (DocWillSave, "WillSave", "Doc", false),
            (DocDidSave, "DidSave", "Doc", false),
            (DocWillClose, "WillClose", "Doc", false),
            (PageOpen, "Open", "Page", false),
            (PageClose, "Close", "Page", false),
            (PageInView, "InView", "Page", false),
            (PageOutView, "OutView", "Page", false),
            (FieldMouseDown, "Mouse Down", "Field", true),
            (FieldMouseUp, "Mouse Up", "Field", true),
            (FieldMouseEnter, "Mouse Enter", "Field", false),
            (FieldMouseExit, "Mouse Exit", "Field", false),
            (FieldFocus, "Focus", "Field", false),
            (FieldBlur, "Blur", "Field", false),
            (FieldKeystroke, "Keystroke", "Field", true),
            (FieldValidate, "Validate", "Field", false),
            (FieldCalculate, "Calculate", "Field", false),
            (FieldFormat, "Format", "Field", false),
            (ExternalExec, "Exec", "External", false),
        ] {
            event.initialize(kind);
            assert!(event.is_valid());
            assert_eq!(
                (event.name(), event.event_type(), event.is_user_gesture()),
                (name, ty, gesture)
            );
        }
        event.on_doc_open(&wide("document.js"));
        assert_eq!(&*event.target_name(), wide("document.js"));
        event.change().borrow_mut().push(65);
        event.rc().set(true);
        event.set_sel_start(-1);
        event.on_page_close();
        assert_eq!(event.commit_key(), -1);
        assert!(event.change().borrow().is_empty());
        assert!(event.change_ex().is_empty());
        assert!(!event.rc().get() && !event.has_value() && !event.will_commit());
        assert_eq!((event.sel_start(), event.sel_end()), (0, 0));
        assert!(event.source_name().is_empty() && event.target_name().is_empty());
    }
    #[test]
    fn native_action_channels_alias_and_roundtrip_utf16_without_loss() {
        let (_document, env, field) = fixture();
        let runtime = RecordingRuntime::new(Some(env));
        let event = runtime.event();
        let mut action = CfflFieldAction {
            b_modifier: true,
            b_shift: true,
            b_key_down: true,
            b_will_commit: true,
            b_field_full: true,
            n_sel_start: 2,
            n_sel_end: 4,
            s_change: vec![65],
            s_change_ex: vec![66],
            s_value: vec![0xd800, 0, 0x1d11e],
            ..Default::default()
        };
        let channels = FieldActionChannels::from_action(&action);
        event.on_field_keystroke(&field, &channels);
        assert_eq!(
            (event.commit_key(), event.sel_start(), event.sel_end()),
            (0, 2, 4)
        );
        assert!(
            event.modifier()
                && event.shift()
                && event.key_down()
                && event.field_full()
                && event.will_commit()
        );
        assert_eq!(
            &*event.value().unwrap().borrow(),
            &[0xd800, 0, 0xd834, 0xdd1e]
        );
        *event.change().borrow_mut() = wide("replacement");
        *event.value().unwrap().borrow_mut() = vec![0xdc00, 0, 0xd834, 0xdd1e];
        event.rc().set(false);
        event.set_sel_start(-5);
        event.set_sel_end(9);
        channels.write_back(&mut action);
        assert_eq!(action.s_value, vec![0xdc00, 0, 0x1d11e]);
        assert_eq!(
            action.s_change,
            "replacement".chars().map(u32::from).collect::<Vec<_>>()
        );
        assert_eq!(
            (action.n_sel_start, action.n_sel_end, action.b_rc),
            (-5, 9, false)
        );
        event.on_field_validate(&field, &channels);
        assert_eq!(
            (event.commit_key(), event.sel_start(), event.sel_end()),
            (-1, 0, 0)
        );
        assert!(!event.will_commit() && !event.field_full());
        event.set_sel_start(100);
        assert_eq!(channels.sel_start.get(), -5); // Validate has native dummy selection storage.
        event.on_field_format(&field, channels.value.clone());
        assert_eq!(event.commit_key(), 0);
        assert!(event.will_commit());
        event.on_field_focus(true, false, &field, channels.value.clone());
        assert!(event.modifier() && !event.shift() && event.has_value());
        event.on_field_blur(false, true, &field, channels.value.clone());
        assert!(!event.modifier() && event.shift() && event.has_value());
        event.on_field_calculate(
            Some(&field),
            &field,
            channels.value.clone(),
            channels.rc.clone(),
        );
        assert_eq!(event.source_name(), event.target_name());
        event.on_field_calculate(None, &field, channels.value, channels.rc);
        assert!(event.source_name().is_empty());
    }
    #[test]
    fn script_execution_guards_reentry_duplicate_keys_and_errors() {
        let runtime = RecordingRuntime::new(None);
        let event = runtime.event();
        event.on_external_exec();
        let weak_runtime = Rc::downgrade(&runtime);
        *runtime.callback.borrow_mut() = Some(Rc::new(move |event| {
            assert!(event.is_valid() && event.is_busy());
            assert_eq!(
                event.run_script(&wide("nested")).unwrap_err(),
                JsError::text("System is busy.")
            );
            let runtime = weak_runtime.upgrade().unwrap();
            let duplicate = runtime.event();
            duplicate.on_external_exec();
            assert_eq!(
                duplicate.run_script(&wide("duplicate")).unwrap_err(),
                JsError::text("Duplicate formfield event found.")
            );
            assert!(duplicate.is_valid() && !duplicate.is_busy());
            assert_eq!(runtime.events.borrow().len(), 1);
            Err(JsError::new(8, 9, vec![0xd800, 0]))
        }));
        assert_eq!(
            event.run_script(&wide("outer")).unwrap_err(),
            JsError::new(8, 9, vec![0xd800, 0])
        );
        assert!(!event.is_valid() && !event.is_busy());
        assert!(runtime.events.borrow().is_empty());
        assert_eq!(runtime.calls.get(), 1);
        event.on_doc_open(&wide("empty"));
        assert!(event.run_script(&[]).is_ok());
        assert!(!event.is_valid());
        assert_eq!(runtime.calls.get(), 1);
        assert!(runtime.events.borrow().is_empty());
        assert_eq!(
            event.run_script(&wide("invalid")).unwrap_err(),
            JsError::text("Object no longer exists.")
        );
    }
    #[test]
    fn binding_references_observe_environment_and_document_lifetime() {
        let (document, env, field) = fixture();
        let runtime = RecordingRuntime::new(Some(env.clone()));
        let event = runtime.event();
        event.on_field_mouse_down(false, false, &field);
        let reference = event.target_field().unwrap();
        assert_eq!(&*reference.name, utf16_encode(&field.full_name()));
        assert!(Rc::ptr_eq(&reference.environment.upgrade().unwrap(), &env));
        drop(document);
        assert!(event.environment().is_none());
        assert!(event.target_field().is_err());
    }
}
