// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Connected CJX_Node contracts executed by the actual Boa callbacks.
use super::super::cfxjse_engine::CfxJseEngine;
use super::*;
use crate::{
    fxcrt::xml::cfx_xmlparser::CfxXmlParser,
    xfa::fxfa::parser::cxfa_document::{CxfaDocument, XfaDocumentNotify},
};
use boa_engine::{js_string, property::Attribute};
use std::rc::Weak;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn build(input: &[u8], packet: XfaPacketType) -> (Rc<CxfaDocument>, CxfaNode) {
    let mut input = input.to_vec();
    input.push(0);
    let stream = Rc::new(CfxReadOnlySpanStream::new(&input));
    let xml = Rc::new(RefCell::new(CfxXmlParser::new(stream).parse().unwrap()));
    let document = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    assert!(builder.build_document(xml, packet));
    let root = builder.get_root_node().unwrap();
    assert!(document.set_root(Some(root.clone())));
    (document, root)
}
fn named(parent: &CxfaNode, name: &str) -> CxfaNode {
    parent.get_first_child_by_name(&wide(name)).unwrap()
}
fn bind(engine: &Rc<CfxJseEngine>, node: &CxfaNode) {
    engine
        .javascript()
        .with_context(|context| {
            let object = engine.get_or_create_binding(node, context)?;
            context.register_global_property(js_string!("subject"), object, Attribute::all())?;
            Ok(())
        })
        .unwrap();
}
fn eval(engine: &CfxJseEngine, script: &str) -> JsValue {
    match engine
        .javascript()
        .eval(&script.encode_utf16().collect::<Vec<_>>())
    {
        Ok(value) => value,
        Err(error) => {
            let message = engine
                .javascript()
                .with_context(|context| error.clone().into_opaque(context)?.to_string(context))
                .map(|message| message.to_std_string_escaped())
                .unwrap_or_else(|_| format!("{error:?}"));
            panic!("Boa script failed: {message}\n{script}");
        }
    }
}
const TEMPLATE:&[u8]=br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><field name="original"><value><text>hello</text></value></field></subform></template></xdp:xdp>"#;
#[test]
fn node_methods_properties_coercions_and_identity() {
    let (document, root) = build(TEMPLATE, XfaPacketType::Xdp);
    let template = root.get_first_child_by_class(XfaElement::Template).unwrap();
    let subform = named(&template, "root");
    let field = named(&subform, "original");
    let engine = CfxJseEngine::new(document).unwrap();
    bind(&engine, &field);
    eval(
        &engine,
        r#"
        if(subject.getAttribute('name')!=='original')throw Error('name');
        const val=subject.getElement('value');
        if(val!==subject.getElement('value',0))throw Error('property identity');
        if(val.oneOfChild!==val.getElement('text'))throw Error('one of child');
        if(!subject.isContainer||subject.isNull||val.isContainer)throw Error('node kind');
        if(subject.ns!=='http://www.xfa.org/schema/xfa-template/3.3/')throw Error('namespace');
        if(subject.model!==subject.model||subject.model.getAttribute('name')!=='template')throw Error('model');
        if(subject.getElement('nonesuch')!==null||subject.getElement('value',-1)!==null)throw Error('missing property');
        if(!subject.isPropertySpecified('name')||subject.isPropertySpecified('nonesuch'))throw Error('specified');
        // Element-only parent queries must not dereference a missing attribute descriptor.
        if(subject.isPropertySpecified('form',true,0))throw Error('element-only name');
        const deep=subject.clone(true), shallow=subject.clone(false);
        if(deep===subject||deep.getAttribute('name')!=='original'||deep.isNull)throw Error('deep clone');
        if(!shallow.isNull||!shallow.isPropertySpecified('value',false))throw Error('shallow/property creation');
        let order=[];
        subject.setAttribute({toString(){order.push('value');return 'renamed'}},{toString(){order.push('name');return 'name'}});
        if(order.join(',')!=='value,name'||subject.getAttribute('name')!=='renamed')throw Error('argument order');
        subject.setAttribute('\ud800\u0000\udfff','custom');
        if(subject.getAttribute('custom')!=='\ud800\u0000\udfff')throw Error('UTF16');
        const marker={};let caught=false;try{subject.setAttribute({toString(){throw marker}},'name')}catch(e){caught=e===marker}if(!caught)throw Error('opaque exception');
        let coerced=false;try{subject.ns={toString(){coerced=true;return ''}}}catch(e){}if(coerced)throw Error('readonly coercion');
        if(subject.saveXML()!=='')throw Error('template saveXML');
    "#,
    );
    assert_eq!(field.get_cdata(XfaAttribute::Name), wide("renamed"));
}
#[test]
fn load_xml_append_overwrite_root_selection_and_main_owner_are_preserved() {
    let (document,datasets)=build(br#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><record><old>o</old></record></xfa:data></xfa:datasets>"#,XfaPacketType::Datasets);
    let record = named(&named(&datasets, "data"), "record");
    let owner = document.xml_document().unwrap();
    let mapping = record.get_xml_mapping_node().unwrap();
    let engine = CfxJseEngine::new(document.clone()).unwrap();
    bind(&engine, &record);
    eval(
        &engine,
        "subject.loadXML('<wrapper><new>n</new></wrapper>',true,false)",
    );
    assert_eq!(record.count_children(XfaElement::Unknown, false), 2);
    assert_eq!(named(&record, "old").get_content(), wide("o"));
    assert_eq!(named(&record, "new").get_content(), wide("n"));
    assert!(Rc::ptr_eq(&owner, &document.xml_document().unwrap()));
    assert_eq!(record.get_xml_mapping_node(), Some(mapping.clone()));
    eval(&engine,"subject.loadXML('<wrapper><replacement>fresh &amp; ready</replacement></wrapper>',true,true)");
    assert_eq!(record.count_children(XfaElement::Unknown, false), 1);
    assert_eq!(
        named(&record, "replacement").get_content(),
        wide("fresh & ready")
    );
    assert!(record.get_first_child_by_name(&wide("old")).is_none());
    let serialized = eval(&engine, "subject.saveXML('pretty')");
    let serialized = wide_from_utf16(&serialized.as_string().unwrap().to_vec());
    let serialized = String::from_utf8(utf8_encode(&serialized)).unwrap();
    assert!(serialized.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(serialized.contains("fresh &amp; ready"));
    assert!(!serialized.contains("<old"));
    eval(
        &engine,
        "subject.loadXML('<retained><leaf>x</leaf></retained>',false,false)",
    );
    assert_eq!(
        named(&named(&record, "retained"), "leaf").get_content(),
        wide("x")
    );
    assert_eq!(record.count_children(XfaElement::Unknown, false), 2);
    eval(&engine, "subject.loadXML('');subject.loadXML('<broken>')");
    assert_eq!(record.count_children(XfaElement::Unknown, false), 2);
    assert!(Rc::ptr_eq(&owner, &document.xml_document().unwrap()));
}
#[test]
fn rich_content_overwrite_updates_mapping_and_roundtrips_through_save_xml() {
    let (document,form)=build(br#"<form xmlns="http://www.xfa.org/schema/xfa-form/2.8/"><subform name="root"><field name="rich"><value><exData contentType="text/html"><body xmlns="http://www.w3.org/1999/xhtml"><p>old</p></body></exData></value></field></subform></form>"#,XfaPacketType::Form);
    let field = named(&named(&form, "root"), "rich");
    let exdata = field
        .get_child(0, XfaElement::Value, false)
        .unwrap()
        .first_child()
        .unwrap();
    let mapping = exdata.get_xml_mapping_node().unwrap();
    let engine = CfxJseEngine::new(document).unwrap();
    bind(&engine, &exdata);
    assert_eq!(
        eval(
            &engine,
            "subject.loadXML===undefined && subject.saveXML===undefined"
        )
        .as_boolean(),
        Some(true)
    );
    // ExData is Object-derived. Exercise the retained model operation directly,
    // then serialize through the actual Node-derived field binding.
    load_xml_model(
        &exdata,
        br#"<body xmlns="http://www.w3.org/1999/xhtml"><p>new <b>bold</b> tail</p></body>"#,
        true,
        true,
    )
    .unwrap();
    assert_ne!(exdata.get_xml_mapping_node(), Some(mapping));
    bind(&engine, &field);
    let actual = eval(&engine, "subject.saveXML()");
    let saved = String::from_utf16(&actual.as_string().unwrap().to_vec()).unwrap();
    assert!(saved.contains("new <b>bold</b> tail"), "{saved}");
    assert!(!saved.contains(">old<"));
}
#[test]
fn method_arity_value_errors_and_native_noop_contracts() {
    let (document, root) = build(TEMPLATE, XfaPacketType::Xdp);
    let template = root.get_first_child_by_class(XfaElement::Template).unwrap();
    let field = named(&named(&template, "root"), "original");
    let engine = CfxJseEngine::new(document).unwrap();
    bind(&engine, &field);
    eval(
        &engine,
        r#"
        for(const call of [()=>subject.clone(),()=>subject.getAttribute(),()=>subject.getElement(),()=>subject.isPropertySpecified(),()=>subject.loadXML(),()=>subject.setAttribute('x'),()=>subject.setElement(),()=>subject.assignNode(),()=>subject.applyXSL(),()=>subject.saveXML('pretty',1)]){let failed=false;try{call()}catch(e){failed=true}if(!failed)throw Error('arity')}
        let badPretty=false;try{subject.saveXML('other')}catch(e){badPretty=true}if(!badPretty)throw Error('save option');
        const uncoerced={toString(){throw Error('unexpected coercion')},valueOf(){throw Error('unexpected coercion')}};
        for(const call of [()=>subject.applyXSL(uncoerced),()=>subject.assignNode(uncoerced,uncoerced,uncoerced),()=>subject.setElement(uncoerced,uncoerced),()=>subject.saveFilteredXML(uncoerced,uncoerced)]){if(call()!==undefined)throw Error('native no-op result')}
        let badReceiver=false;try{subject.getAttribute.call({},'name')}catch(e){badReceiver=true}if(!badReceiver)throw Error('receiver');
    "#,
    );
    for name in ["nonesuch", "calculate", "indexChange"] {
        assert_eq!(
            engine
                .javascript()
                .with_context(|context| exec_single_event_by_name(
                    &engine,
                    &field,
                    &wide(name),
                    XfaElement::Field,
                    context
                ))
                .unwrap(),
            XfaEventError::kNotExist
        );
    }
}
struct Reenter {
    engine: RefCell<Weak<CfxJseEngine>>,
}
impl XfaDocumentNotify for Reenter {
    fn on_child_added(&self, _: &CxfaNode, _: &CxfaNode) {}
    fn on_child_removed(&self, _: &CxfaNode, _: &CxfaNode) {}
    fn on_node_ready(&self, _: &CxfaNode) {}
    fn on_attribute_changed(&self, _: &CxfaNode, attribute: XfaAttribute) {
        if attribute == XfaAttribute::Name {
            let engine = self.engine.borrow().upgrade();
            if let Some(engine) = engine {
                eval(
                    &engine,
                    "globalThis.notified=(globalThis.notified||0)+1;subject.getAttribute('name')",
                );
            }
        }
    }
}
#[test]
fn mutation_notifications_reenter_the_same_boa_context() {
    let (document, root) = build(TEMPLATE, XfaPacketType::Xdp);
    let template = root.get_first_child_by_class(XfaElement::Template).unwrap();
    let field = named(&named(&template, "root"), "original");
    field.set_initialized_flag();
    let notify = Rc::new(Reenter {
        engine: RefCell::new(Weak::new()),
    });
    document.set_notify(Some(notify.clone()));
    let engine = CfxJseEngine::new(document).unwrap();
    *notify.engine.borrow_mut() = Rc::downgrade(&engine);
    bind(&engine, &field);
    eval(&engine,"subject.setAttribute('updated','name');if(notified!==1||subject.getAttribute('name')!=='updated')throw Error('reentry')");
}
