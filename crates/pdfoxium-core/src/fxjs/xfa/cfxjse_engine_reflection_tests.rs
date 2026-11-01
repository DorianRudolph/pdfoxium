// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Live model reflection, nondeletion and Boa Proxy invariant regressions.
use super::*;
use crate::xfa::fxfa::fxfa_basic::{XfaAttributeValue, XfaPacketType};
use boa_engine::property::Attribute;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn fixture() -> (Rc<CfxJseEngine>, CxfaNode) {
    let document = CxfaDocument::new();
    let xfa = document
        .create_node(XfaPacketType::Xdp, XfaElement::Xfa)
        .unwrap();
    document.set_root(Some(xfa.clone()));
    let form = document
        .create_node(XfaPacketType::Form, XfaElement::Form)
        .unwrap();
    xfa.insert_child_and_notify(-1, &form);
    let subform = document
        .create_node(XfaPacketType::Form, XfaElement::Subform)
        .unwrap();
    form.insert_child_and_notify(-1, &subform);
    let field = document
        .create_node(XfaPacketType::Form, XfaElement::Field)
        .unwrap();
    field.set_cdata(XfaAttribute::Name, &wide("original"));
    subform.insert_child_and_notify(-1, &field);
    let runtime = CfxJseEngine::new(document).unwrap();
    runtime
        .with_script_context(|context| {
            let object = runtime.get_or_create_binding(&field, context)?;
            context.register_global_property(js_string!("field"), object, Attribute::all())?;
            Ok(())
        })
        .unwrap();
    (runtime, field)
}
#[test]
fn declared_properties_reflect_real_accessors_and_remain_live_and_nondeletable() {
    let (runtime, field) = fixture();
    assert!(runtime.eval("globalThis.descriptor=Object.getOwnPropertyDescriptor(field,'name');typeof descriptor.get==='function' && typeof descriptor.set==='function' && descriptor.enumerable && !descriptor.configurable && !('value' in descriptor) && 'name' in field && !Reflect.deleteProperty(field,'name') && !Reflect.defineProperty(field,'name',{value:'fake'}) && Object.keys(field).includes('name')").unwrap().to_boolean());
    field.set_cdata(XfaAttribute::Name, &wide("external"));
    assert!(runtime.eval("descriptor.get.call(field)==='external' && field.name==='external' && Object.getOwnPropertyDescriptor(field,'name').get===descriptor.get").unwrap().to_boolean());
    assert!(runtime
        .eval("descriptor.set.call(field,'script');field.name==='script' && !(delete field.name)")
        .unwrap()
        .to_boolean());
    assert_eq!(field.get_cdata(XfaAttribute::Name), wide("script"));
    assert!(runtime.eval("(function(){'use strict';try{delete field.name}catch(e){return e instanceof TypeError}})()").unwrap().to_boolean());
    assert!(runtime.eval("Object.getOwnPropertyDescriptor(field,'notDeclared')===undefined && delete field.notDeclared").unwrap().to_boolean());
}

#[test]
fn generated_host_accessors_do_not_hide_named_children_or_explicit_js_properties() {
    let (runtime, field) = fixture();
    let subject = field.parent().unwrap();
    // Real parsed packet roots carry their packet name; get_model_node uses
    // that name hash rather than merely walking to the nearest Form ancestor.
    subject
        .parent()
        .unwrap()
        .set_cdata(XfaAttribute::Name, &wide("form"));
    let model = runtime
        .document()
        .create_node(XfaPacketType::Form, XfaElement::Subform)
        .unwrap();
    model.set_cdata(XfaAttribute::Name, &wide("model"));
    assert!(subject.insert_child_and_notify(-1, &model));
    for name in ["ordinary", "getAttribute"] {
        let child = runtime
            .document()
            .create_node(XfaPacketType::Form, XfaElement::Field)
            .unwrap();
        child.set_cdata(XfaAttribute::Name, &wide(name));
        assert!(subject.insert_child_and_notify(-1, &child));
    }
    runtime
        .with_script_context(|context| {
            for (name, node) in [("subject", &subject), ("modelChild", &model)] {
                let object = runtime.get_or_create_binding(node, context)?;
                context.register_global_property(JsString::from(name), object, Attribute::all())?;
            }
            Ok(())
        })
        .unwrap();
    assert!(runtime
        .eval(
            r#"
        if (subject.model !== modelChild || Object.create(subject).model !== modelChild)
            throw 'generated model accessor hides child';
        if (Object.getOwnPropertyDescriptor(subject,'model').get.call(subject).className !== 'form')
            throw 'explicit model attribute lost its callback';
        if (typeof subject.getAttribute !== 'function' ||
            subject.resolveNode('getAttribute').name !== 'getAttribute')
            throw 'declared method precedence changed';
        if (Reflect.defineProperty(subject,'model',{value:17}) ||
            Reflect.deleteProperty(subject,'model') || subject.model !== modelChild)
            throw 'declared nonconfigurable accessor protection changed';
        Object.defineProperty(subject,'ordinary',{value:17,writable:true,configurable:true});
        if (subject.ordinary !== 17 || subject.resolveNode('ordinary').name !== 'ordinary')
            throw 'explicit own property must shadow the child';
        if (!delete subject.ordinary || subject.ordinary.name !== 'ordinary')
            throw 'deleting an explicit property must restore SOM lookup';
        Object.setPrototypeOf(subject,{ordinary:99});
        if (subject.ordinary !== 99 || Object.create(subject).ordinary !== 99 ||
            subject.resolveNode('ordinary').name !== 'ordinary')
            throw 'ordinary JavaScript prototype precedence changed';
        subject.ordinary=101;
        if (subject.ordinary !== 101 ||
            Object.getOwnPropertyDescriptor(subject,'ordinary').value !== 101)
            throw 'ordinary JavaScript own property changed';
        true
    "#
        )
        .unwrap()
        .to_boolean());
}
#[test]
fn reflective_setters_reenter_and_preserve_original_exceptions_without_snapshot_values() {
    let (runtime, field) = fixture();
    assert!(runtime.eval("let d=Object.getOwnPropertyDescriptor(field,'name');let marker={};let caught=false;try{d.set.call(field,{toString(){field.name='nested';if(d.get.call(field)!=='nested')throw 'not live';throw marker}})}catch(e){caught=e===marker}caught && field.name==='nested'").unwrap().to_boolean());
    assert_eq!(field.get_cdata(XfaAttribute::Name), wide("nested"));
    assert!(runtime.eval("let thrown={};Reflect.defineProperty(field,'custom',{get(){throw thrown},configurable:true});let getter=Object.getOwnPropertyDescriptor(field,'custom').get;try{getter.call(field)}catch(e){e===thrown}").unwrap().to_boolean());
    assert!(runtime.eval("Reflect.getOwnPropertyDescriptor=()=>{throw 'poison'};Reflect.defineProperty=()=>{throw 'poison'};Reflect.deleteProperty=()=>{throw 'poison'};Object.getOwnPropertyDescriptor(field,'presence').configurable===false && !(delete field.presence)").unwrap().to_boolean());
}
#[test]
fn preventing_extensions_materializes_declared_metadata_without_invoking_getters() {
    let (runtime, field) = fixture();
    assert!(runtime.eval("Object.preventExtensions(field);!Object.isExtensible(field) && Object.getOwnPropertyDescriptor(field,'presence').configurable===false && Reflect.ownKeys(field).includes('rawValue') && !Reflect.defineProperty(field,'newProperty',{value:1})").unwrap().to_boolean());
    assert!(runtime.eval("field.presence='hidden';field.presence==='hidden' && !Reflect.deleteProperty(field,'presence')").unwrap().to_boolean());
    assert_eq!(
        field.get_enum(XfaAttribute::Presence),
        XfaAttributeValue::Hidden
    );
}
#[test]
fn retained_reflective_accessors_reject_expired_runtime_after_collection() {
    let (runtime, _) = fixture();
    let javascript = runtime.javascript().clone();
    let getter = runtime
        .eval("Object.getOwnPropertyDescriptor(field,'name').get")
        .unwrap();
    let weak = Rc::downgrade(&runtime);
    drop(runtime);
    boa_gc::force_collect();
    assert!(weak.upgrade().is_none());
    assert!(javascript
        .with_context(|context| getter.as_callable().unwrap().call(
            &JsValue::undefined(),
            &[],
            context
        ))
        .is_err());
}

#[test]
fn primitive_object_nodes_do_not_reenter_absent_tree_and_node_property_callbacks() {
    let (runtime, _) = fixture();
    for element in [
        XfaElement::Boolean,
        XfaElement::Decimal,
        XfaElement::ExData,
        XfaElement::Float,
        XfaElement::Integer,
        XfaElement::Text,
        XfaElement::Time,
    ] {
        let primitive = runtime
            .document()
            .create_node(XfaPacketType::Form, element)
            .unwrap();
        runtime
            .with_script_context(|context| {
                let object = runtime.get_or_create_binding(&primitive, context)?;
                context.register_global_property(
                    js_string!("primitive"),
                    object,
                    Attribute::all(),
                )?;
                Ok(())
            })
            .unwrap();
        assert!(
            runtime
                .eval(
                    r#"
            primitive.parent===undefined && primitive.model===undefined &&
            primitive.nodes===undefined && primitive.isContainer===undefined &&
            typeof primitive.clone==='undefined' && typeof primitive.getAttribute==='undefined'
        "#
                )
                .unwrap()
                .to_boolean(),
            "{element:?}"
        );
        assert!(runtime.eval("primitive.parent=17;primitive.model=23;primitive.parent===undefined && primitive.model===undefined")
            .unwrap().to_boolean(), "{element:?}");
    }
}
