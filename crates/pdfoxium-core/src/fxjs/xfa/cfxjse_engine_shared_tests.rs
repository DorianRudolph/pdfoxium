// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Shared native SDK isolate / separate XFA realms, using the actual Boa VM.
use super::*;
use crate::{
    fpdfapi::parser::cpdf_document::CpdfDocument,
    fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment,
    fxjs::cjs_runtime::CjsRuntime,
    xfa::fxfa::fxfa_basic::{XfaAttribute, XfaPacketType},
};
use boa_engine::{
    context::{time::FixedClock, HostHooks},
    NativeFunction,
};
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn sdk_eval(engine: &CfxJsEngine, text: &str) -> JsResult<JsValue> {
    engine.eval(&text.encode_utf16().collect::<Vec<_>>())
}
#[derive(Debug)]
struct Hooks;
impl HostHooks for Hooks {
    fn local_timezone_offset_seconds(&self, _: i64) -> i32 {
        5400
    }
}
#[test]
fn separate_xfa_realms_share_the_sdk_vm_clock_and_host_metadata() {
    let clock = Rc::new(FixedClock::from_millis(123_456_000));
    let mut context = Context::builder()
        .clock(clock.clone())
        .host_hooks(Rc::new(Hooks))
        .build()
        .unwrap();
    context.insert_data(String::from("sdk metadata"));
    let javascript = CfxJsEngine::new(context);
    sdk_eval(
        &javascript,
        "globalThis.realmName='sdk';globalThis.sdkOnly=17",
    )
    .unwrap();
    let first = CfxJseEngine::with_runtime(CxfaDocument::new(), javascript.clone()).unwrap();
    let second = CfxJseEngine::with_runtime(CxfaDocument::new(), javascript.clone()).unwrap();
    assert!(Rc::ptr_eq(first.javascript(), &javascript));
    assert!(Rc::ptr_eq(second.javascript(), &javascript));
    first
        .eval("globalThis.realmName='first';globalThis.xfaOnly=23")
        .unwrap();
    second.eval("globalThis.realmName='second'").unwrap();
    assert!(sdk_eval(&javascript,"realmName==='sdk' && sdkOnly===17 && typeof xfaOnly==='undefined' && typeof pfm_rt==='undefined'").unwrap().to_boolean());
    assert!(first
        .eval("realmName==='first' && xfaOnly===23 && typeof sdkOnly==='undefined'")
        .unwrap()
        .to_boolean());
    assert!(second
        .eval("realmName==='second' && typeof xfaOnly==='undefined'")
        .unwrap()
        .to_boolean());
    for runtime in [&first, &second] {
        assert_eq!(
            runtime.eval("Date.now()").unwrap().as_number(),
            Some(123_456_000.0)
        );
        assert_eq!(
            runtime
                .eval("new Date(0).getTimezoneOffset()")
                .unwrap()
                .as_number(),
            Some(-90.0)
        );
        runtime
            .with_script_context(|context| {
                assert!(Rc::ptr_eq(
                    &CfxJseEngine::from_context(context).unwrap(),
                    runtime
                ));
                assert_eq!(context.get_data::<String>().unwrap(), "sdk metadata");
                Ok(())
            })
            .unwrap();
    }
    clock.forward(2000);
    assert_eq!(
        first.eval("Date.now()").unwrap().as_number(),
        sdk_eval(&javascript, "Date.now()").unwrap().as_number()
    );
    javascript
        .with_context(|context| {
            assert!(CfxJseEngine::from_context(context).is_none());
            Ok(())
        })
        .unwrap();
    assert!(first.eval("throw {marker:42}").is_err());
    assert!(sdk_eval(&javascript, "realmName==='sdk'")
        .unwrap()
        .to_boolean());
    let weak = Rc::downgrade(&first);
    drop(first);
    assert!(weak.upgrade().is_none());
    assert!(second.eval("realmName==='second'").unwrap().to_boolean());
}
#[derive(Clone)]
struct ReentryRuntime(Weak<CfxJseEngine>);
fn call_xfa(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    assert!(CfxJseEngine::from_context(context).is_none());
    let runtime = context
        .get_data::<ReentryRuntime>()
        .unwrap()
        .0
        .upgrade()
        .unwrap();
    CfxJsEngine::with_host_callback(context, || {
        runtime.eval("if(realmName!=='xfa')throw 'wrong XFA realm';globalThis.xfaCalls++;42")
    })
}
fn call_sdk(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    assert!(CfxJseEngine::from_context(context).is_some());
    let runtime = CjsRuntime::from_context(context).unwrap();
    let fail = args.first().is_some_and(JsValue::to_boolean);
    CfxJsEngine::with_host_callback(context, || {
        let source = if fail {
            "throw {toString(){if(realmName!=='sdk')throw 'wrong SDK exception realm';globalThis.sdkErrorText=true;return 'sdk error'}}"
        } else {
            "if(realmName!=='sdk')throw 'wrong SDK realm';globalThis.sdkCalls++;if(callXfa()!==42)throw 'wrong XFA completion'"
        };
        let result = runtime.run_document_script(&[], &source.encode_utf16().collect::<Vec<_>>());
        assert_eq!(result.is_err(), fail);
    });
    Ok(JsValue::undefined())
}
#[test]
fn actual_sdk_events_reenter_xfa_on_the_same_vm_and_restore_both_realms_after_errors() {
    let pdf = Rc::new(RefCell::new(CpdfDocument::new()));
    pdf.borrow_mut().create_new_doc();
    let environment = CpdfSdkFormFillEnvironment::new(&pdf, None);
    let sdk = environment.get_or_create_runtime().unwrap();
    sdk.run_document_script(
        &[],
        &"globalThis.realmName='sdk';globalThis.sdkCalls=0"
            .encode_utf16()
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let xfa = CfxJseEngine::with_runtime(CxfaDocument::new(), sdk.engine().clone()).unwrap();
    sdk.engine()
        .with_context(|context| {
            context.insert_data(ReentryRuntime(Rc::downgrade(&xfa)));
            Ok(())
        })
        .unwrap();
    sdk.engine()
        .define_global_method(&"callXfa".encode_utf16().collect::<Vec<_>>(), 0, call_xfa)
        .unwrap();
    xfa.with_script_context(|context| {
        context.register_global_callable(
            js_string!("callSdk"),
            1,
            NativeFunction::from_fn_ptr(call_sdk),
        )?;
        Ok(())
    })
    .unwrap();
    assert!(xfa.eval("Object.defineProperties(globalThis,{realmName:{value:'xfa',writable:true,configurable:true},xfaCalls:{value:0,writable:true,configurable:true}});callSdk(false);callSdk(true);callSdk(false);realmName==='xfa' && xfaCalls===2 && sdkCalls===2 && !Object.hasOwn(globalThis,'sdkCalls')").unwrap().to_boolean());
    assert!(sdk_eval(
        sdk.engine(),
        "realmName==='sdk' && sdkCalls===2 && sdkErrorText===true && typeof xfaCalls==='undefined'"
    )
    .unwrap()
    .to_boolean());
    assert!(sdk.current_event_context().is_none());
}
#[test]
fn variables_realms_keep_their_own_runtime_link_on_a_shared_vm() {
    let javascript = CfxJsEngine::new(Context::default());
    let document = CxfaDocument::new();
    let subform = document
        .create_node(XfaPacketType::Form, XfaElement::Subform)
        .unwrap();
    let variables = subform
        .get_or_create_property(0, XfaElement::Variables)
        .unwrap();
    let script = document
        .create_node(XfaPacketType::Form, XfaElement::Script)
        .unwrap();
    script.set_cdata(XfaAttribute::Name, &wide("values"));
    script.set_content(
        &wide("var saved=7;function next(){return ++saved;}"),
        &wide("var saved=7;function next(){return ++saved;}"),
        false,
    );
    variables.insert_child_and_notify(-1, &script);
    let runtime = CfxJseEngine::with_runtime(document, javascript.clone()).unwrap();
    assert!(runtime.run_variables_script(&script).unwrap());
    assert_eq!(
        runtime
            .query_variable_value(&script, &wide("saved"))
            .unwrap()
            .unwrap()
            .as_number(),
        Some(7.)
    );
    let function = runtime
        .query_variable_value(&script, &wide("next"))
        .unwrap()
        .unwrap();
    let result = runtime
        .with_script_context(|context| {
            function
                .as_callable()
                .unwrap()
                .call(&JsValue::undefined(), &[], context)
        })
        .unwrap();
    assert_eq!(result.as_number(), Some(8.));
    assert!(sdk_eval(&javascript, "typeof saved==='undefined'")
        .unwrap()
        .to_boolean());
}
