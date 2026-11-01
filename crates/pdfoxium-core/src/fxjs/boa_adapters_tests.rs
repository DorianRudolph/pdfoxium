// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Native isolate-wrapper vectors and the selected corrected error boundary.
use super::*;
use crate::fxjs::{cfxjs_engine::CfxJsEngine, js_date};
use boa_engine::{context::time::FixedClock, object::builtins::JsArray, JsValue};
fn engine() -> Rc<CfxJsEngine> {
    CfxJsEngine::new(new_context().unwrap())
}
fn evaluate(engine: &CfxJsEngine, source: &str) -> JsValue {
    engine
        .eval(&source.encode_utf16().collect::<Vec<_>>())
        .unwrap()
}
fn assert_value(
    engine: &CfxJsEngine,
    value: JsValue,
    boolean: bool,
    integer: i32,
    number: f64,
    text: &str,
) {
    engine
        .with_context(|context| {
            assert_eq!(value.to_boolean(), boolean);
            assert_eq!(value.to_i32(context)?, integer);
            let actual = value.to_number(context)?;
            if number.is_nan() {
                assert!(actual.is_nan())
            } else {
                assert_eq!(actual, number)
            }
            let string = value.to_string(context)?;
            assert_eq!(string.to_std_string_escaped(), text);
            assert_eq!(string.to_vec(), text.encode_utf16().collect::<Vec<_>>());
            assert!(value.as_object().is_none());
            Ok(())
        })
        .unwrap();
}
#[test]
fn new_null() {
    let engine = engine();
    assert_value(&engine, JsValue::null(), false, 0, 0., "null");
}
#[test]
fn new_undefined() {
    let engine = engine();
    assert_value(
        &engine,
        JsValue::undefined(),
        false,
        0,
        f64::NAN,
        "undefined",
    );
}
#[test]
fn new_boolean() {
    let engine = engine();
    assert_value(&engine, true.into(), true, 1, 1., "true");
    assert_value(&engine, false.into(), false, 0, 0., "false");
}
#[test]
fn new_number() {
    let engine = engine();
    assert_value(&engine, 42.1.into(), true, 42, 42.1, "42.1");
}
#[test]
fn new_string() {
    let engine = engine();
    for text in [
        JsString::from("123"),
        JsString::from("123".encode_utf16().collect::<Vec<_>>().as_slice()),
    ] {
        assert_value(&engine, text.into(), true, 123, 123., "123");
    }
}
#[test]
fn new_date() {
    let engine = engine();
    engine
        .with_context(|context| {
            let date = js_date::new_date(context, 1_111_111_111.)?;
            assert!(date.to_boolean());
            assert_eq!(date.to_i32(context)?, 1_111_111_111);
            assert_eq!(date.to_number(context)?, 1_111_111_111.);
            assert!(!date.to_string(context)?.is_empty());
            let object = date.as_object().unwrap();
            assert!(!object.is_array());
            assert_eq!(js_date::date_value(&date, context)?, Some(1_111_111_111.));
            Ok(())
        })
        .unwrap();
}
#[test]
fn new_array() {
    let engine = engine();
    engine
        .with_context(|context| {
            let array = JsArray::new(context)?;
            assert_eq!(array.length(context)?, 0);
            assert!(array.get(2, context)?.is_undefined());
            assert_eq!(array.length(context)?, 0);
            array.set(3, 12, true, context)?;
            assert!(array.get(2, context)?.is_undefined());
            assert!(array.get(3, context)?.is_number());
            assert_eq!(array.length(context)?, 4);
            let value = JsValue::from(array.clone());
            assert!(value.to_boolean());
            assert_eq!(value.to_i32(context)?, 0);
            assert!(value.to_number(context)?.is_nan());
            assert_eq!(value.to_string(context)?, JsString::from(",,,12"));
            assert!(value.as_object().unwrap().is_array());
            Ok(())
        })
        .unwrap();
}
#[test]
fn new_object() {
    let engine = engine();
    engine
        .with_context(|context| {
            let object = JsObject::with_object_proto(context.intrinsics());
            assert!(object.own_property_keys(context)?.is_empty());
            assert!(object
                .get(boa_engine::js_string!("clams"), context)?
                .is_undefined());
            assert!(object.own_property_keys(context)?.is_empty());
            object.set(boa_engine::js_string!("clams"), 12, true, context)?;
            assert!(object
                .get(boa_engine::js_string!("clams"), context)?
                .is_number());
            let names = object.own_property_keys(context)?;
            assert_eq!(names.len(), 1);
            assert_eq!(
                names[0].clone(),
                boa_engine::property::PropertyKey::from(boa_engine::js_string!("clams"))
            );
            let value = JsValue::from(object);
            assert!(value.to_boolean());
            assert_eq!(value.to_i32(context)?, 0);
            assert!(value.to_number(context)?.is_nan());
            assert_eq!(
                value.to_string(context)?,
                boa_engine::js_string!("[object Object]")
            );
            assert!(!value.as_object().unwrap().is_array());
            Ok(())
        })
        .unwrap();
}
#[test]
fn throw_from_getter() {
    let engine = engine();
    let object = evaluate(
        &engine,
        "globalThis.getterSentinel=false;({get clams(){getterSentinel=true;throw 'clams'}})",
    )
    .as_object()
    .unwrap();
    let error = engine
        .get_object_property_reentrant(&object, &"clams".encode_utf16().collect::<Vec<_>>())
        .unwrap_err();
    engine
        .with_context(|context| {
            assert_eq!(
                error.into_opaque(context)?.as_string().unwrap(),
                boa_engine::js_string!("clams")
            );
            Ok(())
        })
        .unwrap();
    assert!(evaluate(&engine, "getterSentinel").to_boolean());
}
#[test]
fn throw_from_setter() {
    let engine = engine();
    let object = evaluate(
        &engine,
        "globalThis.setterSentinel=false;({set clams(value){setterSentinel=true;throw 'clams'}})",
    )
    .as_object()
    .unwrap();
    let error = engine
        .put_object_property_reentrant(
            &object,
            &"clams".encode_utf16().collect::<Vec<_>>(),
            boa_engine::js_string!("clams").into(),
        )
        .unwrap_err();
    engine
        .with_context(|context| {
            assert_eq!(
                error.into_opaque(context)?.as_string().unwrap(),
                boa_engine::js_string!("clams")
            );
            Ok(())
        })
        .unwrap();
    assert!(evaluate(&engine, "setterSentinel").to_boolean());
}
#[test]
fn compatibility_strings_preserve_separate_methods_and_reentrant_errors() {
    let engine = engine();
    engine.with_context(enable_compatible_mode).unwrap();
    assert!(evaluate(&engine,r#"String.substr('abcdef',2,3)==='cde' && String.toUpperCase('abc')==='ABC' && String.substr({toString(){return 'value'}},1)==='alue' && !Object.getOwnPropertyDescriptor(String,'substr').enumerable && Object.getOwnPropertyDescriptor(String,'substr').writable && !Object.getOwnPropertyDescriptor(String,'substr').configurable"#).to_boolean());
    assert!(evaluate(&engine,"let failed=0;for(const method of [String.substr,String.toUpperCase]){try{method()}catch(e){if(e instanceof TypeError)failed++}};failed===2").to_boolean());
    assert!(evaluate(
        &engine,
        "let marker={};try{String.toUpperCase({toString(){throw marker}})}catch(e){e===marker}"
    )
    .to_boolean());
    assert!(evaluate(
        &engine,
        "String.prototype.substr=function(){return 'changed'};String.substr('abc')==='changed'"
    )
    .to_boolean());
}
#[test]
fn compatibility_console_aliases_forward_actual_log_without_coercing_arguments() {
    let engine = engine();
    engine.with_context(enable_compatible_mode).unwrap();
    assert!(evaluate(&engine,"let marker={toString(){throw 7}};console.println(marker);let captured;console.log=function(...args){captured=args;return 9};console.println(marker,3);captured.length===2 && captured[0]===marker && captured[1]===3 && console.show(marker)===undefined").to_boolean());
    assert!(evaluate(&engine,"let marker2={};console.log=function(){throw marker2};try{console.println('text')}catch(e){e===marker2}").to_boolean());
}
#[test]
fn array_buffer_policy_rejects_before_allocation_and_keeps_boa_timezone() {
    let clock = Rc::new(FixedClock::from_millis(123_456_000));
    let mut native = Context::builder().clock(clock.clone()).build().unwrap();
    let mut selected = Context::builder()
        .clock(clock)
        .host_hooks(Rc::new(PdfHostHooks))
        .build()
        .unwrap();
    assert_eq!(
        selected.host_hooks().max_buffer_size(&mut selected),
        MAX_ARRAY_BUFFER_BYTES
    );
    assert_eq!(
        selected
            .eval(Source::from_bytes(
                "[Date.now(),new Date(0).getTimezoneOffset()].join(',')"
            ))
            .unwrap(),
        native
            .eval(Source::from_bytes(
                "[Date.now(),new Date(0).getTimezoneOffset()].join(',')"
            ))
            .unwrap()
    );
    assert!(selected
        .eval(Source::from_bytes(
            "let a=new Uint8Array(new ArrayBuffer(8));a.every(x=>x===0)"
        ))
        .unwrap()
        .to_boolean());
    assert!(selected.eval(Source::from_bytes("let failed=0;for(const source of [()=>new ArrayBuffer(268435457),()=>new ArrayBuffer(1,{maxByteLength:268435457})]){try{source()}catch(e){if(e instanceof RangeError)failed++}}failed===2")).unwrap().to_boolean());
}
#[test]
fn class_tags_are_readonly_non_enumerable_and_global_display_uses_descriptor_name() {
    let engine = engine();
    engine
        .with_context(|context| {
            let object = JsObject::with_object_proto(context.intrinsics());
            set_class_tag(&object, "XFAObject", context)?;
            context.register_global_property(
                boa_engine::js_string!("value"),
                object,
                boa_engine::property::Attribute::all(),
            )?;
            set_global_class(context, "Root")?;
            Ok(())
        })
        .unwrap();
    assert!(evaluate(&engine,"Object.prototype.toString.call(value)==='[object XFAObject]' && Object.prototype.toString.call(globalThis)==='[object global]' && globalThis.toString()==='[object Root]' && !Object.getOwnPropertyDescriptor(value,Symbol.toStringTag).writable && !Object.getOwnPropertyDescriptor(value,Symbol.toStringTag).enumerable").to_boolean());
}
