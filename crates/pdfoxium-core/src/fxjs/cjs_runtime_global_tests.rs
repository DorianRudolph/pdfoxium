// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Native XFA Get/SetValueByNameInGlobalObject on the real shared SDK VM.
use super::*;
use crate::{
    fpdfapi::parser::cpdf_document::CpdfDocument,
    fxjs::xfa::cfxjse_engine::CfxJseEngine,
    xfa::fxfa::{
        fxfa_basic::{XfaElement, XfaPacketType},
        parser::cxfa_document::CxfaDocument,
    },
};
use boa_engine::{js_string, property::PropertyDescriptor, JsString, JsValue, NativeFunction};
fn fixture() -> (
    Rc<RefCell<CpdfDocument>>,
    Rc<CpdfSdkFormFillEnvironment>,
    Rc<CjsRuntime>,
    Rc<CfxJseEngine>,
) {
    let pdf = Rc::new(RefCell::new(CpdfDocument::new()));
    pdf.borrow_mut().create_new_doc();
    let environment = CpdfSdkFormFillEnvironment::new(&pdf, None);
    let sdk = environment.get_or_create_runtime().unwrap();
    let document = CxfaDocument::new();
    let root = document
        .create_node(XfaPacketType::Xdp, XfaElement::Xfa)
        .unwrap();
    assert!(document.set_root(Some(root)));
    let xfa = CfxJseEngine::with_runtime(document, sdk.engine().clone()).unwrap();
    (pdf, environment, sdk, xfa)
}
fn eval(sdk: &CjsRuntime, source: &str) -> JsResult<JsValue> {
    sdk.engine()
        .eval(&source.encode_utf16().collect::<Vec<_>>())
}
#[test]
fn named_xfa_fallback_reads_writes_sdk_globals_without_merging_realms() {
    let (_pdf, _environment, sdk, xfa) = fixture();
    eval(
        &sdk,
        "globalThis.sdkNumber=17;globalThis.sdkObject={value:23};globalThis.sdkRealm='sdk'",
    )
    .unwrap();
    assert!(xfa.eval("sdkNumber===17 && sdkObject.value===23 && typeof app==='object' && typeof util==='object' && !Object.hasOwn(globalThis,'sdkNumber')").unwrap().to_boolean());
    assert!(xfa.eval("sdkNumber+=2;sdkObject.value=29;Object.defineProperty(globalThis,'xfaOnly',{value:31,writable:true,configurable:true});sdkNumber===19 && sdkObject.value===29").unwrap().to_boolean());
    assert!(eval(&sdk,"sdkNumber===19 && sdkObject.value===29 && !Object.hasOwn(globalThis,'xfaOnly') && sdkRealm==='sdk'").unwrap().to_boolean());
    sdk.engine()
        .with_context(|context| {
            assert!(!sdk.set_value_by_name_in_context(JsString::from(""), 1.into(), context)?);
            assert!(sdk.current_event_context().is_none());
            assert!(CfxJseEngine::from_context(context).is_none());
            Ok(())
        })
        .unwrap();
}
#[derive(Clone)]
struct Reentry(Weak<CfxJseEngine>);
fn probe(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    assert!(CfxJseEngine::from_context(context).is_none());
    let sdk = CjsRuntime::from_context(context).unwrap();
    assert_eq!(
        sdk.current_event_context().unwrap().event_kind(),
        super::super::cjs_event_context::Kind::Unknown
    );
    let xfa = context.get_data::<Reentry>().unwrap().0.upgrade().unwrap();
    CfxJsEngine::with_host_callback(context, || {
        xfa.eval("globalThis.reentries=(globalThis.reentries||0)+1;42")
    })
}
#[test]
fn sdk_global_getters_setters_reenter_restore_scopes_and_keep_opaque_exceptions() {
    let (_pdf, _environment, sdk, xfa) = fixture();
    sdk.engine()
        .with_context(|context| {
            context.insert_data(Reentry(Rc::downgrade(&xfa)));
            let function = boa_engine::object::FunctionObjectBuilder::new(
                context.realm(),
                NativeFunction::from_fn_ptr(probe),
            )
            .build();
            context.global_object().define_property_or_throw(
                js_string!("sdkReentrant"),
                PropertyDescriptor::builder()
                    .get(function)
                    .configurable(true),
                context,
            )?;
            Ok(())
        })
        .unwrap();
    eval(&sdk,"globalThis.sdkMarker={};Object.defineProperty(globalThis,'sdkThrows',{get(){throw sdkMarker},set(value){throw sdkMarker},configurable:true})").unwrap();
    assert!(xfa
        .eval("sdkReentrant===42 && reentries===1")
        .unwrap()
        .to_boolean());
    assert!(xfa.eval("let read=false,write=false;try{sdkThrows}catch(e){read=e===sdkMarker}try{sdkThrows=1}catch(e){write=e===sdkMarker}read && write").unwrap().to_boolean());
    assert!(sdk.current_event_context().is_none());
    sdk.engine()
        .with_context(|context| {
            assert!(CfxJseEngine::from_context(context).is_none());
            Ok(())
        })
        .unwrap();
    assert!(xfa.eval("typeof pfm_rt==='object'").unwrap().to_boolean());
}
#[test]
fn sdk_global_fallback_rejects_detached_host_without_creating_replacement_runtime() {
    let (_pdf, environment, sdk, xfa) = fixture();
    eval(&sdk, "globalThis.sdkValue=7").unwrap();
    assert_eq!(xfa.eval("sdkValue").unwrap().as_number(), Some(7.0));
    environment.shutdown();
    assert!(xfa.eval("sdkValue").is_err());
    assert!(sdk.current_event_context().is_none());
    assert!(environment.existing_runtime().is_none());
}

#[test]
fn whole_xfa_execution_keeps_unknown_sdk_event_until_imported_function_finishes() {
    let (_pdf, _environment, sdk, xfa) = fixture();
    sdk.engine()
        .with_context(|context| {
            context.insert_data(Reentry(Rc::downgrade(&xfa)));
            Ok(())
        })
        .unwrap();
    sdk.engine()
        .define_global_method(
            &"sdkEventProbe".encode_utf16().collect::<Vec<_>>(),
            0,
            probe,
        )
        .unwrap();
    // The global lookup's own temporary event has already ended before the
    // imported function executes. RunScript must still shield the SDK scope.
    assert!(xfa
        .eval("sdkEventProbe()===42 && reentries===1")
        .unwrap()
        .to_boolean());
    assert!(xfa.eval("sdkEventProbe();throw {marker:1}").is_err());
    assert!(sdk.current_event_context().is_none());
    assert!(xfa
        .eval("sdkEventProbe()===42 && reentries===3")
        .unwrap()
        .to_boolean());
    assert!(sdk.current_event_context().is_none());
}
