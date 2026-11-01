// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived context/variables tests executing published Boa realms.
use super::*;
use crate::xfa::fxfa::{
    cxfa_eventparam::XfaEventType,
    fxfa_basic::{XfaAttribute, XfaPacketType},
};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn node(
    document: &Rc<CxfaDocument>,
    packet: XfaPacketType,
    element: XfaElement,
    name: &str,
) -> CxfaNode {
    let node = document.create_node(packet, element).unwrap();
    node.set_cdata(XfaAttribute::Name, &wide(name));
    node
}
fn model() -> (Rc<CfxJseEngine>, CxfaNode, CxfaNode) {
    let document = CxfaDocument::new();
    let xfa = node(&document, XfaPacketType::Xdp, XfaElement::Xfa, "xfa");
    let form = node(&document, XfaPacketType::Form, XfaElement::Form, "form");
    assert!(xfa.insert_child_and_notify(-1, &form));
    assert!(document.set_root(Some(xfa)));
    let subform = node(
        &document,
        XfaPacketType::Form,
        XfaElement::Subform,
        "record",
    );
    assert!(form.insert_child_and_notify(-1, &subform));
    let field = node(&document, XfaPacketType::Form, XfaElement::Field, "amount");
    assert!(subform.insert_child_and_notify(-1, &field));
    let engine = CfxJseEngine::new(document).unwrap();
    (engine, subform, field)
}
fn variables(engine: &CfxJseEngine, parent: &CxfaNode, name: &str, source: &str) -> CxfaNode {
    let variables = parent
        .get_or_create_property(0, XfaElement::Variables)
        .unwrap();
    let script = node(
        engine.document(),
        XfaPacketType::Form,
        XfaElement::Script,
        name,
    );
    assert!(script.set_content(&wide(source), &wide(source), false));
    assert!(variables.insert_child_and_notify(-1, &script));
    script
}
fn run(engine: &CfxJseEngine, node: &CxfaNode, source: &str) -> JsResult<JsValue> {
    engine.run_script(CxfaScriptType::Javascript, &wide(source), Some(node))
}

#[test]
fn bare_global_names_resolve_live_som_and_attributes_without_rewriting_lexical_scope() {
    let (engine, subform, field) = model();
    assert_eq!(
        run(&engine, &field, "rawValue='bare'; rawValue;")
            .unwrap()
            .as_string()
            .unwrap()
            .to_std_string_escaped(),
        "bare"
    );
    assert_eq!(
        run(
            &engine,
            &subform,
            "amount === this.amount && amount.rawValue==='bare';"
        )
        .unwrap()
        .as_boolean(),
        Some(true)
    );
    let sibling = node(
        engine.document(),
        XfaPacketType::Form,
        XfaElement::Field,
        "mirror",
    );
    assert!(subform.insert_child_and_notify(-1, &sibling));
    assert_eq!(
        run(&engine, &field, "mirror === this.parent.mirror;")
            .unwrap()
            .as_boolean(),
        Some(true)
    );
    assert_eq!(
        run(
            &engine,
            &field,
            "let rawValue=7; {let rawValue=8;} rawValue;"
        )
        .unwrap()
        .as_number(),
        Some(7.)
    );
    assert_eq!(field.get_raw_value(), wide("bare"));
    assert_eq!(
        run(
            &engine,
            &field,
            "'use strict'; rawValue='strict'; this.rawValue;"
        )
        .unwrap()
        .as_string()
        .unwrap()
        .to_std_string_escaped(),
        "strict"
    );
    assert_eq!(
        run(&engine, &subform, "freshGlobal=19; freshGlobal;")
            .unwrap()
            .as_number(),
        Some(19.)
    );
    assert_eq!(engine.eval("freshGlobal;").unwrap().as_number(), Some(19.));
    assert_eq!(
        run(&engine, &field, "typeof missingName;")
            .unwrap()
            .as_string()
            .unwrap()
            .to_std_string_escaped(),
        "undefined"
    );
    assert_eq!(engine.eval("typeof Date==='function' && typeof Number==='function' && new Date(0).getUTCFullYear()===1970;").unwrap().as_boolean(),Some(true));
    sibling.set_cdata(XfaAttribute::Name, &wide("changed"));
    assert_eq!(
        run(
            &engine,
            &field,
            "typeof mirror==='undefined' && changed===this.parent.changed;"
        )
        .unwrap()
        .as_boolean(),
        Some(true)
    );
}

#[test]
fn variables_use_persistent_isolated_realms_and_bind_exported_functions_to_their_global() {
    let (engine, subform, field) = model();
    let first=variables(&engine,&subform,"first","var counter=1; let privateValue=10; function increment(n){ this.counter += n; return counter + privateValue; } function change(v){amount.rawValue=v; return amount===this.amount;}");
    let second = variables(
        &engine,
        &subform,
        "second",
        "var counter=100; function increment(n){counter+=n;return counter;}",
    );
    assert!(engine
        .query_variable_value(&first, &wide("counter"))
        .unwrap()
        .is_none());
    assert!(engine.run_variables_script(&first).unwrap());
    assert!(engine.run_variables_script(&second).unwrap());
    assert_eq!(
        engine
            .query_variable_value(&first, &wide("counter"))
            .unwrap()
            .unwrap()
            .as_number(),
        Some(1.)
    );
    assert_eq!(
        engine
            .query_variable_value(&second, &wide("counter"))
            .unwrap()
            .unwrap()
            .as_number(),
        Some(100.)
    );
    assert!(engine
        .query_variable_value(&first, &wide("privateValue"))
        .unwrap()
        .is_none());
    let function = engine
        .query_variable_value(&first, &wide("increment"))
        .unwrap()
        .unwrap();
    let result = engine
        .javascript
        .with_context(|context| {
            function
                .as_callable()
                .unwrap()
                .call(&JsValue::null(), &[2.into()], context)
        })
        .unwrap();
    assert_eq!(result.as_number(), Some(13.));
    assert_eq!(
        engine
            .query_variable_value(&first, &wide("counter"))
            .unwrap()
            .unwrap()
            .as_number(),
        Some(3.)
    );
    assert!(engine.run_variables_script(&first).unwrap());
    assert_eq!(
        engine
            .query_variable_value(&first, &wide("counter"))
            .unwrap()
            .unwrap()
            .as_number(),
        Some(3.)
    );
    assert_eq!(
        engine
            .eval("typeof counter;")
            .unwrap()
            .as_string()
            .unwrap()
            .to_std_string_escaped(),
        "undefined"
    );
    let change = engine
        .query_variable_value(&first, &wide("change"))
        .unwrap()
        .unwrap();
    assert_eq!(
        engine
            .javascript
            .with_context(|context| change.as_callable().unwrap().call(
                &JsValue::undefined(),
                &[js_string!("from variables").into()],
                context
            ))
            .unwrap()
            .as_boolean(),
        Some(true)
    );
    assert_eq!(field.get_raw_value(), wide("from variables"));
    assert!(engine
        .update_variable_value(&first, &wide("counter"), 23.into())
        .unwrap());
    assert_eq!(
        engine
            .query_variable_value(&first, &wide("counter"))
            .unwrap()
            .unwrap()
            .as_number(),
        Some(23.)
    );
    assert!(engine.current_this_node().is_none());
    assert_eq!(engine.variable_contexts.borrow().len(), 2);
}

#[test]
fn obtaining_script_binding_initializes_once_and_proxies_expose_variable_values() {
    let (engine, subform, _) = model();
    let script = variables(
        &engine,
        &subform,
        "tools",
        "var answer=41; function plus(){return this.answer+1;}",
    );
    engine
        .javascript
        .with_context(|context| {
            let first = engine.get_or_create_binding(&script, context)?;
            let second = engine.get_or_create_binding(&script, context)?;
            assert_eq!(first, second);
            context.register_global_property(
                js_string!("tools"),
                first,
                boa_engine::property::Attribute::all(),
            )
        })
        .unwrap();
    assert_eq!(
        engine
            .eval("tools.answer===41 && tools.plus()===42;")
            .unwrap()
            .as_boolean(),
        Some(true)
    );
    engine.eval("tools.answer=51;").unwrap();
    assert_eq!(engine.eval("tools.plus();").unwrap().as_number(), Some(52.));
    assert_eq!(
        engine
            .query_variable_value(&script, &wide("answer"))
            .unwrap()
            .unwrap()
            .as_number(),
        Some(51.)
    );
}

#[test]
fn variables_failures_keep_opaque_values_partial_state_and_restore_the_main_realm() {
    let (engine, subform, _) = model();
    let script = variables(
        &engine,
        &subform,
        "broken",
        "var before=7; throw {marker:'variables',parts:[1,2,3]};",
    );
    let error = engine.run_variables_script(&script).unwrap_err();
    engine
        .javascript
        .with_context(|context| {
            let object = error.clone().into_opaque(context)?.as_object().unwrap();
            assert_eq!(
                object
                    .get(js_string!("marker"), context)?
                    .as_string()
                    .unwrap()
                    .to_std_string_escaped(),
                "variables"
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(
        engine
            .query_variable_value(&script, &wide("before"))
            .unwrap()
            .unwrap()
            .as_number(),
        Some(7.)
    );
    assert!(engine.run_variables_script(&script).unwrap());
    assert!(engine.current_this_node().is_none());
    assert_eq!(engine.eval("40+2;").unwrap().as_number(), Some(42.));
    assert_eq!(
        engine
            .eval("typeof before;")
            .unwrap()
            .as_string()
            .unwrap()
            .to_std_string_escaped(),
        "undefined"
    );
}

#[test]
fn scoped_event_run_at_and_up_array_services_restore_even_when_operations_unwind() {
    let (engine, subform, field) = model();
    let outer = Rc::new(RefCell::new(CxfaEventParam::new(XfaEventType::Change)));
    let inner = Rc::new(RefCell::new(CxfaEventParam::new(XfaEventType::Click)));
    assert!(engine.current_event().is_none());
    engine.with_event(Some(&field), outer.clone(), || {
        assert!(Rc::ptr_eq(&engine.current_event().unwrap(), &outer));
        assert_eq!(engine.event_target(), Some(field.clone()));
        engine.with_event(Some(&subform), inner.clone(), || {
            assert!(Rc::ptr_eq(&engine.current_event().unwrap(), &inner));
            assert_eq!(engine.event_target(), Some(subform.clone()));
        });
        assert!(Rc::ptr_eq(&engine.current_event().unwrap(), &outer));
    });
    assert!(engine.current_event().is_none());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine.with_event(Some(&field), outer, || panic!("event host unwinds"))
    }));
    assert!(result.is_err());
    assert!(engine.current_event().is_none());
    assert!(engine.event_target().is_none());
    assert!(engine.is_run_at_client());
    engine.set_run_at_type(XfaAttributeValue::Server);
    assert!(!engine.is_run_at_client());
    engine.set_run_at_type(XfaAttributeValue::Client);
    assert!(engine.is_run_at_client());
    assert!(engine.last_object_from_up_array().is_none());
    engine.add_object_to_up_array(&field);
    engine.add_object_to_up_array(&subform);
    assert_eq!(engine.last_object_from_up_array(), Some(subform));
    assert!(engine.event_host().is_none());
    assert!(engine.widget_script_host().is_none());
    assert!(engine.instance_manager_host().is_none());
}

#[test]
fn reflected_root_attributes_do_not_shadow_bare_predicate_names() {
    let (engine, subform, first) = model();
    let second = node(
        engine.document(),
        XfaPacketType::Form,
        XfaElement::Field,
        "amount",
    );
    assert!(subform.insert_child_and_notify(-1, &second));
    // Reflection materializes an own accessor on the root object. That is
    // an attribute, not a CJX method, and must retain current-this scoping.
    assert_eq!(
        run(
            &engine,
            &first,
            "Object.getOwnPropertyDescriptor(xfa,'index'); index===this.index && name==='amount';"
        )
        .unwrap()
        .as_boolean(),
        Some(true)
    );
    assert_eq!(run(&engine, &subform,
        "this.resolveNodes('amount.(index == 1)').length===1 && this.resolveNode('amount.(index == 1)')===this.resolveNode('amount[1]') && this.name==='record';")
        .unwrap().as_boolean(), Some(true));
    assert_eq!(
        run(&engine, &second, "index===1 && this.index===1;")
            .unwrap()
            .as_boolean(),
        Some(true)
    );
    assert_eq!(
        run(
            &engine,
            &second,
            r#"
            Object.getOwnPropertyDescriptor(xfa,'index');
            Object.defineProperty(Object.prototype,'value',{
                get:function(){throw 'inherited descriptor value must not be read';},
                configurable:true
            });
            try { index===1 && this.index===1 && name==='amount'; }
            finally { delete Object.prototype.value; }
        "#
        )
        .unwrap()
        .as_boolean(),
        Some(true)
    );
}
