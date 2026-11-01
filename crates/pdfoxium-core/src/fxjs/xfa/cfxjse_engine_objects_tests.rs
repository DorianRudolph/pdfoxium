// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Actual CXFA_Object/SOM results, native node-list filtering and Boa identity.
use super::super::cfxjse_resolveprocessor::{PseudoObject, ResolvedObject};
use super::*;
use crate::xfa::fxfa::{
    fxfa_basic::XfaPacketType,
    test_support::{new_document, HostCall},
};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn fixture() -> Rc<CfxJseEngine> {
    let document = CxfaDocument::new();
    let root = document
        .create_node(XfaPacketType::Xdp, XfaElement::Xfa)
        .unwrap();
    assert!(document.set_root(Some(root)));
    CfxJseEngine::new(document).unwrap()
}
fn assert_script(runtime: &CfxJseEngine, source: &str) {
    assert_eq!(runtime.eval(source).unwrap().as_boolean(), Some(true));
}

#[test]
fn pseudo_som_results_share_canonical_bindings_and_node_lists_filter_them() {
    let runtime = fixture();
    assert_script(
        &runtime,
        r#"
        ['host','event','dataWindow','log','signature','layout'].every(function(name) {
            return xfa.resolveNode(name) === xfa[name] &&
                xfa.resolveNode('$'+name) === xfa[name] &&
                xfa.resolveNode(name+'[0]') === xfa[name] &&
                xfa.resolveNode(name+'[*]') === xfa[name] &&
                xfa.resolveNode(name+'[1]') === null &&
                xfa.resolveNodes(name).length === 0 &&
                xfa.resolveNode(name+'.className') === null &&
                xfa[name+'.className'] === xfa[name].className;
        }) && xfa['host.version'] === '11' &&
        xfa.resolveNode('host.version.missing') === null
    "#,
    );
    let flags = XfaResolveFlags::CHILDREN | XfaResolveFlags::ATTRIBUTES;
    let result = runtime
        .resolve_object_values(None, &wide("$host"), flags)
        .unwrap()
        .unwrap();
    assert!(
        matches!(&result.objects[0], ResolvedObject::Pseudo(object) if object.kind()==PseudoModel::Host)
    );
    // Parser/data-merge callers keep a deliberate node-only projection.
    assert!(runtime
        .resolve_objects(None, &wide("$host"), flags)
        .unwrap()
        .is_none());
    assert_eq!(
        runtime
            .run_script(CxfaScriptType::Formcalc, &wide("$host.version"), None)
            .unwrap()
            .as_string()
            .unwrap()
            .to_std_string_escaped(),
        "11"
    );
}

#[test]
fn pseudo_predicates_run_real_javascript_and_formcalc_with_scoped_object_this() {
    let runtime = fixture();
    assert_script(
        &runtime,
        r#"
        xfa.resolveNode('$host.(this === xfa.host && xfa.this === this && xfa.resolveNode("this") === this && xfa.resolveNode("$event") === xfa.event)') === xfa.host &&
        xfa.resolveNode('$host.[1]') === xfa.host &&
        xfa.resolveNode('$host.[0]') === null &&
        xfa.resolveNode('$host.(throw 17)') === null && xfa.this === null
    "#,
    );
    let object = ResolvedObject::Pseudo(PseudoObject::new(runtime.document(), PseudoModel::Host));
    assert_eq!(
        runtime
            .run_script_object(
                CxfaScriptType::Javascript,
                &wide("this === xfa.host && xfa.this === this && xfa.resolveNode('this') === this"),
                Some(&object)
            )
            .unwrap()
            .as_boolean(),
        Some(true)
    );
    assert!(runtime
        .run_script_object(CxfaScriptType::Javascript, &wide("throw 19"), Some(&object))
        .is_err());
    assert!(runtime.current_this_node().is_none());
    assert!(runtime.current_this_object().is_none());
    assert_script(&runtime, "xfa.this === null");
}

#[test]
fn actual_host_som_callbacks_reenter_and_retained_objects_reject_expired_owners() {
    let (document, host) = new_document(
        r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform/></template></xdp:xdp>"#,
    );
    let runtime = document.script_context();
    let weak = Rc::downgrade(&runtime);
    *host.on_callback.borrow_mut() = Some(Rc::new(move |call| {
        if matches!(call,HostCall::Title(value)if value==&wide("outer")) {
            assert_script(
                &weak.upgrade().unwrap(),
                "xfa.resolveNode('host').title='inner';xfa.resolveNode('$host')===xfa.host",
            );
        }
    }));
    assert_script(
        &runtime,
        r#"
        xfa.resolveNode('host').title = {toString(){if(xfa.resolveNode('host')!==xfa.host)throw 'identity';return 'outer'}};
        let marker={};let caught=false;
        try { xfa.resolveNode('host').title={toString(){throw marker}}; } catch(e) {caught=e===marker;}
        caught && xfa['host.title']==='inner' && xfa.host.title==='inner'
    "#,
    );
    assert_eq!(host.title.borrow().as_slice(), wide("inner"));
    let typed = ResolvedObject::Pseudo(PseudoObject::new(runtime.document(), PseudoModel::Host));
    let foreign = fixture();
    assert!(matches!(
        foreign.resolver().check_object_reference(&typed),
        Err(ResolveError::InvalidReference)
    ));
    let javascript = runtime.javascript().clone();
    let retained = runtime.eval("xfa.resolveNode('host')").unwrap();
    let engine_weak = Rc::downgrade(&runtime);
    let doc_weak = Rc::downgrade(runtime.document());
    document.pre_finalize();
    drop(document);
    drop(runtime);
    boa_gc::force_collect();
    assert!(engine_weak.upgrade().is_none());
    assert!(doc_weak.upgrade().is_none());
    assert!(typed.as_node().is_none());
    assert!(javascript
        .with_context(|context| retained
            .as_object()
            .unwrap()
            .get(JsString::from("version"), context))
        .is_err());
}
