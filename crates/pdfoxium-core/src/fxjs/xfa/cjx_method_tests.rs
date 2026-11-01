// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! JSE_METHOD DynamicTypeIs checks apply before argument coercion. Base-class
//! methods remain usable on valid derived receivers; property holders retain
//! their independent JSE_PROP behavior.
use super::*;
use crate::{
    fxjs::xfa::cfxjse_engine::CfxJseEngine,
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaPacketType},
        parser::cxfa_document::CxfaDocument,
    },
};
use boa_engine::property::Attribute;
use std::rc::Rc;
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn fixture() -> Rc<CfxJseEngine> {
    let document = CxfaDocument::new();
    let root = document
        .create_node(XfaPacketType::Xdp, XfaElement::Xfa)
        .unwrap();
    document.set_root(Some(root.clone()));
    let form = document
        .create_node(XfaPacketType::Form, XfaElement::Form)
        .unwrap();
    root.insert_child_and_notify(-1, &form);
    let runtime = CfxJseEngine::new(document.clone()).unwrap();
    runtime
        .with_script_context(|context| {
            for (name, element) in [
                ("field", XfaElement::Field),
                ("field2", XfaElement::Field),
                ("draw", XfaElement::Draw),
                ("group", XfaElement::ExclGroup),
                ("subform", XfaElement::Subform),
                ("delta", XfaElement::Delta),
                ("boolean", XfaElement::Boolean),
                ("desc", XfaElement::Desc),
            ] {
                let node = document.create_node(XfaPacketType::Form, element).unwrap();
                if node.has_attribute(XfaAttribute::Name) {
                    node.set_cdata(XfaAttribute::Name, &wide(name));
                }
                form.insert_child_and_notify(-1, &node);
                let binding = runtime.get_or_create_binding(&node, context)?;
                context.register_global_property(
                    JsString::from(name),
                    binding,
                    Attribute::all(),
                )?;
            }
            let binding = runtime.get_or_create_binding(&form, context)?;
            context.register_global_property(JsString::from("form"), binding, Attribute::all())?;
            Ok(())
        })
        .unwrap();
    runtime
}
#[test]
fn borrowed_specialized_methods_reject_wrong_classes_before_coercion() {
    let runtime = fixture();
    assert!(runtime.eval("let touched=false;const arg={toString(){touched=true;throw 'coerced'}};let rejected=0;for(const invoke of [()=>field.addItem.call(draw,arg),()=>field.execEvent.call(group,arg),()=>subform.execEvent.call(field,arg),()=>form.createNode.call(field,arg),()=>delta.restore.call(field),()=>desc.metadata.call(draw,arg)]){try{invoke()}catch(e){rejected++}}rejected===6 && !touched").unwrap().to_boolean());
}
#[test]
fn borrowed_base_methods_accept_real_subclasses_and_reject_object_only_nodes() {
    let runtime = fixture();
    assert!(runtime.eval("field.getAttribute.call(draw,'name')==='draw' && field.getAttribute.call(subform,'name')==='subform' && field.resolveNode.call(draw,'$')===draw && field.getDelta.call(draw)===undefined && field.getDeltas.call(subform).length===0").unwrap().to_boolean());
    assert!(runtime.eval("let rejected=0;for(const invoke of [()=>field.getAttribute.call(boolean,'name'),()=>field.resolveNode.call(delta,'$'),()=>field.getDelta.call(group),()=>field.getAttribute.call({},'name')]){try{invoke()}catch(e){rejected++}}rejected===4").unwrap().to_boolean());
}
#[test]
fn valid_borrowed_method_preserves_nested_calls_and_original_thrown_value() {
    let runtime = fixture();
    assert!(runtime.eval("const original={};let nested=false,caught=false;try{field.getAttribute.call(draw,{toString(){nested=field.getAttribute.call(subform,'name')==='subform';throw original}})}catch(e){caught=e===original}nested && caught && field.getAttribute.call(draw,'name')==='draw'").unwrap().to_boolean());
    // Ordinary inherited property access resolves its native holder. Calling
    // an extracted accessor directly supplies a different function receiver.
    // The empty Model callback must ignore the value without coercion.
    assert!(runtime.eval("let coerced=false;const inherited=Object.create(form);inherited.context={toString(){coerced=true;throw 1}};!coerced && inherited.context===undefined").unwrap().to_boolean());
}

#[test]
fn borrowed_pseudo_methods_use_their_actual_native_class_and_keep_node_guards() {
    let runtime = fixture();
    runtime
        .with_script_context(|context| {
            let root = runtime.document().get_root().unwrap();
            let binding = runtime.get_or_create_binding(&root, context)?;
            context.register_global_property(JsString::from("xfa"), binding, Attribute::all())?;
            Ok(())
        })
        .unwrap();
    assert!(runtime.eval("let untouched=true;const bad={toString(){untouched=false;throw 1}};xfa.log.message(bad);xfa.dataWindow.gotoRecord(bad);let failed=0;for(const call of [()=>xfa.log.message.call(xfa.dataWindow,bad),()=>xfa.dataWindow.gotoRecord.call(xfa.log,bad),()=>xfa.signature.enumerate.call(xfa.layout),()=>xfa.layout.absPageCountInBatch.call(field),()=>field.addItem.call(xfa.log,bad)]){try{call()}catch(e){failed++}}failed===5 && untouched && xfa.signature.enumerate()===undefined && xfa.layout.absPageCountInBatch()===0").unwrap().to_boolean());
}
