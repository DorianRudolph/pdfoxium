// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source event-property, scope and optional real-host contracts through Boa.
use super::*;
use crate::xfa::fxfa::{
    fxfa_basic::{XfaElement, XfaPacketType},
    parser::cxfa_document::{CxfaDocument, XfaDocumentNotify},
};
use boa_engine::{property::Attribute, Source};
use std::cell::Cell;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn fixture() -> Rc<CfxJseEngine> {
    let engine = CfxJseEngine::new(CxfaDocument::new()).unwrap();
    engine
        .javascript()
        .with_context(|context| {
            let object = event_object(&engine, context)?;
            context.register_global_property(js_string!("xfaEvent"), object, Attribute::all())?;
            Ok(())
        })
        .unwrap();
    engine
}
fn eval(engine: &CfxJseEngine, script: &str) -> JsValue {
    match engine.eval(script) {
        Ok(value) => value,
        Err(error) => {
            let message = engine
                .javascript()
                .with_context(|context| error.clone().into_opaque(context)?.to_string(context))
                .map(|value| value.to_std_string_escaped())
                .unwrap_or_else(|_| format!("{error:?}"));
            panic!("Boa event script failed: {message}\n{script}");
        }
    }
}
#[test]
fn missing_event_is_undefined_and_setters_do_not_coerce() {
    let engine = fixture();
    eval(
        &engine,
        r#"
        for(const name of ['cancelAction','change','commitKey','fullText','keyDown','modifier','newContentType','newText','prevContentType','prevText','reenter','selEnd','selStart','shift','soapFaultCode','soapFaultString','target'])if(xfaEvent[name]!==undefined)throw Error(name);
        let converted=false;const value={toString(){converted=true;throw Error('coerced')},valueOf(){converted=true;throw Error('coerced')}};
        xfaEvent.change=value;xfaEvent.selStart=value;xfaEvent.selEnd=value;xfaEvent.newText=value;
        if(converted)throw Error('missing event conversion');
        if(xfaEvent.emit(1,2)!==undefined||xfaEvent.reset(1,2)!==undefined)throw Error('missing event method');
        if(xfaEvent.className!=='eventPseudoModel')throw Error('class');
        let bad=false;try{xfaEvent.emit.call({})}catch(e){bad=true}if(!bad)throw Error('receiver');
    "#,
    );
}
#[test]
fn actual_event_reads_edits_clamps_and_preserves_utf16_and_opaque_errors() {
    let engine = fixture();
    let mut supplied = CxfaEventParam::new(XfaEventType::Change);
    supplied.prev_text = wide("α😀bc");
    supplied.change = wide("X");
    supplied.sel_start = 1;
    supplied.sel_end = 3;
    supplied.commit_key = 27;
    supplied.key_down = true;
    supplied.modifier = true;
    supplied.reenter = true;
    supplied.shift = true;
    supplied.full_text = wide("full text");
    supplied.new_content_type = wide("new/type");
    supplied.prev_content_type = wide("old/type");
    supplied.soap_fault_code = wide("code");
    supplied.soap_fault_string = wide("fault");
    let event = Rc::new(RefCell::new(supplied));
    engine.with_event(None,event.clone(),||eval(&engine,r#"
        if(xfaEvent.prevText!=='α😀bc'||xfaEvent.newText!=='αXc')throw Error('new text');
        if(xfaEvent.commitKey!==27||!xfaEvent.keyDown||!xfaEvent.modifier||!xfaEvent.reenter||!xfaEvent.shift)throw Error('keys');
        if(xfaEvent.fullText!=='full text'||xfaEvent.newContentType!=='new/type'||xfaEvent.prevContentType!=='old/type'||xfaEvent.soapFaultCode!=='code'||xfaEvent.soapFaultString!=='fault')throw Error('strings');
        if(xfaEvent.target!==undefined)throw Error('native target');
        let converted=false;const readonlyValue={toString(){converted=true;return 'changed'},valueOf(){converted=true;return 7}};
        for(const name of ['commitKey','fullText','keyDown','modifier','newContentType','newText','prevContentType','prevText','reenter','shift','soapFaultCode','soapFaultString','target'])xfaEvent[name]=readonlyValue;
        if(converted||xfaEvent.commitKey!==27||xfaEvent.prevText!=='α😀bc')throw Error('readonly');
        xfaEvent.cancelAction={valueOf(){throw Error('boolean coercion')}};
        if(!xfaEvent.cancelAction)throw Error('cancel');
        xfaEvent.change='Q\u0000\ud800';if(xfaEvent.newText!=='αQ\u0000\ud800c')throw Error('UTF16');
        xfaEvent.selStart=-100;if(xfaEvent.selStart!==0||xfaEvent.selEnd!==3)throw Error('start clamp');
        xfaEvent.selEnd=100;if(xfaEvent.selEnd!==4||xfaEvent.selStart!==0)throw Error('wide count');
        xfaEvent.selEnd=1;xfaEvent.selStart=3;if(xfaEvent.selStart!==3||xfaEvent.selEnd!==3)throw Error('range coupling');
        xfaEvent.selStart=4294967297;if(xfaEvent.selStart!==1)throw Error('ToInt32');
        const marker={};let caught=false;try{xfaEvent.change={toString(){throw marker}}}catch(e){caught=e===marker}if(!caught||xfaEvent.change!=='Q\u0000\ud800')throw Error('string exception');
        caught=false;try{xfaEvent.selStart={valueOf(){throw marker}}}catch(e){caught=e===marker}if(!caught||xfaEvent.selStart!==1)throw Error('integer exception');
        const inherited=Object.create(xfaEvent);inherited.change='inherited';if(xfaEvent.change!=='inherited')throw Error('property holder');
    "#));
    let result = event.borrow();
    assert!(result.cancel_action);
    assert_eq!(result.change, wide("inherited"));
    assert_eq!((result.sel_start, result.sel_end), (1, 3));
    assert_eq!(result.event_type, XfaEventType::Change);
    assert!(engine.current_event().is_none());
}
#[test]
fn reset_replaces_the_supplied_record_and_keeps_scope_target() {
    let engine = fixture();
    let target = engine
        .document()
        .create_node(XfaPacketType::Form, XfaElement::Field)
        .unwrap();
    let mut supplied = CxfaEventParam::new(XfaEventType::Validate);
    supplied.cancel_action = true;
    supplied.change = wide("change");
    supplied.prev_text = wide("previous");
    supplied.result = wide("result");
    supplied.targeted = false;
    let event = Rc::new(RefCell::new(supplied));
    engine.with_event(Some(&target),event.clone(),||{
        eval(&engine,"xfaEvent.reset('ignored');if(xfaEvent.newText!==''||xfaEvent.cancelAction||xfaEvent.commitKey!==0||xfaEvent.prevText!=='')throw Error('reset')");
        assert_eq!(engine.event_target(),Some(target.clone()));assert!(Rc::ptr_eq(&engine.current_event().unwrap(),&event));
    });
    assert_eq!(*event.borrow(), CxfaEventParam::new(XfaEventType::Unknown));
    assert!(engine.event_target().is_none());
}
struct NestedHost {
    engine: Weak<CfxJseEngine>,
    outer: Rc<RefCell<CxfaEventParam>>,
    inner: Rc<RefCell<CxfaEventParam>>,
    target: CxfaNode,
    calls: Cell<usize>,
    throwing: Cell<bool>,
}
impl XfaEventHost for NestedHost {
    fn handle_widget_event(
        &self,
        target: Option<&CxfaNode>,
        event: Rc<RefCell<CxfaEventParam>>,
    ) -> JsResult<()> {
        assert_eq!(target, Some(&self.target));
        assert!(Rc::ptr_eq(&event, &self.outer));
        self.calls.set(self.calls.get() + 1);
        let engine = self.engine.upgrade().unwrap();
        engine.with_event(Some(&self.target),self.inner.clone(),||{
            let script=if self.throwing.get(){"xfaEvent.change='nested before throw';throw nestedMarker"}else{"if(xfaEvent.prevText!=='inner')throw Error('inner record');xfaEvent.change='nested';xfaEvent.cancelAction=true"};
            engine.eval(script).map(|_|())
        })
    }
}
#[test]
fn emit_uses_the_real_optional_host_and_nested_scopes_restore_on_errors() {
    let engine = fixture();
    let target = engine
        .document()
        .create_node(XfaPacketType::Form, XfaElement::Field)
        .unwrap();
    let mut event = CxfaEventParam::new(XfaEventType::Change);
    event.prev_text = wide("outer");
    event.change = wide("before");
    let outer = Rc::new(RefCell::new(event));
    let mut event = CxfaEventParam::new(XfaEventType::Change);
    event.prev_text = wide("inner");
    let inner = Rc::new(RefCell::new(event));
    let host = Rc::new(NestedHost {
        engine: Rc::downgrade(&engine),
        outer: outer.clone(),
        inner: inner.clone(),
        target: target.clone(),
        calls: Cell::new(0),
        throwing: Cell::new(false),
    });
    engine.set_event_host(Some(host.clone()));
    engine.with_event(Some(&target),outer.clone(),||{
        eval(&engine,r#"xfaEvent.change={toString(){xfaEvent.emit();if(xfaEvent.prevText!=='outer')throw Error('restoration');return 'outer converted'}};if(xfaEvent.change!=='outer converted'||xfaEvent.cancelAction)throw Error('outer record')"#);
        assert_eq!(inner.borrow().change,wide("nested"));assert!(inner.borrow().cancel_action);assert!(Rc::ptr_eq(&engine.current_event().unwrap(),&outer));
        host.throwing.set(true);eval(&engine,r#"globalThis.nestedMarker={};let caught=false;try{xfaEvent.emit()}catch(e){caught=e===nestedMarker}if(!caught||xfaEvent.prevText!=='outer')throw Error('nested error restoration')"#);
        assert_eq!(inner.borrow().change,wide("nested before throw"));
    });
    assert_eq!(host.calls.get(), 2);
    assert!(engine.current_event().is_none());
    assert!(engine.event_target().is_none());
    // The scoped record is restored even when an exception escapes run_script.
    let failed = engine.with_event(None, outer.clone(), || {
        engine.eval("xfaEvent.change='committed';throw 7")
    });
    assert!(failed.is_err());
    assert_eq!(outer.borrow().change, wide("committed"));
    assert!(engine.current_event().is_none());
}
struct ModelObserver;
impl XfaDocumentNotify for ModelObserver {
    fn on_child_added(&self, _: &CxfaNode, _: &CxfaNode) {}
    fn on_child_removed(&self, _: &CxfaNode, _: &CxfaNode) {}
    fn on_attribute_changed(&self, _: &CxfaNode, _: crate::xfa::fxfa::fxfa_basic::XfaAttribute) {}
    fn on_node_ready(&self, _: &CxfaNode) {}
}
#[test]
fn absent_ff_host_is_a_native_noop_and_saved_bindings_do_not_retain_runtime() {
    let engine = fixture();
    engine.document().set_notify(Some(Rc::new(ModelObserver)));
    let event = Rc::new(RefCell::new(CxfaEventParam::new(XfaEventType::Click)));
    engine.with_event(None, event, || {
        eval(
            &engine,
            "xfaEvent.emit();xfaEvent.change='no FF host required'",
        )
    });
    let object = engine
        .javascript()
        .with_context(|context| event_object(&engine, context))
        .unwrap();
    let weak = Rc::downgrade(&engine);
    drop(engine);
    assert!(weak.upgrade().is_none());
    // The saved accessors belong to their original realm. Cross-realm errors
    // need not inherit from the new caller realm's ReferenceError.prototype.
    let mut context = Context::default();
    context
        .register_global_property(js_string!("savedEvent"), object, Attribute::all())
        .unwrap();
    assert_eq!(context.eval(Source::from_bytes("let failed=false;try{savedEvent.change}catch(e){failed=e.name==='ReferenceError'&&e.message.includes('expired')}failed")).unwrap().as_boolean(),Some(true));
}
