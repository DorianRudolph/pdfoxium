// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived model/Boa checks; these do not claim XFA page rendering.
use super::cfxjse_engine::CfxJseEngine;
use crate::{
    fxcrt::{cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser},
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaElement, XfaPacketType},
        parser::{cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder},
    },
};
use std::{cell::RefCell, rc::Rc};
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn runtime() -> (Rc<CxfaDocument>, Rc<CfxJseEngine>) {
    let mut input = br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record"/></template><form xmlns="http://www.xfa.org/schema/xfa-form/3.3/"><subform name="record"><field name="amount"><ui><textEdit/></ui><value><text>original</text></value></field><draw name="label"><value><text>caption</text></value></draw><desc/></subform></form></xdp:xdp>"#.to_vec();
    input.push(0);
    let xml = Rc::new(RefCell::new(
        CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&input)))
            .parse()
            .unwrap(),
    ));
    let doc = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(doc.clone());
    assert!(builder.build_document(xml, XfaPacketType::Xdp));
    assert!(doc.set_root(builder.get_root_node()));
    let engine = CfxJseEngine::new(doc.clone()).unwrap();
    (doc, engine)
}
fn check(engine: &CfxJseEngine, source: &str) {
    assert_eq!(
        engine.eval(source).unwrap().as_boolean(),
        Some(true),
        "{source}"
    );
}
#[test]
fn colors_border_width_and_messages_modify_actual_model_xml() {
    let (_doc, engine) = runtime();
    let event = Rc::new(RefCell::new(
        crate::xfa::fxfa::cxfa_eventparam::CxfaEventParam::new(
            crate::xfa::fxfa::cxfa_eventparam::XfaEventType::Change,
        ),
    ));
    engine.with_event(None,event.clone(),||check(&engine,"xfa.event===xfa.event && (xfa.event.change='changed',xfa.event.cancelAction=true,xfa.event.change==='changed')"));
    assert_eq!(event.borrow().change, wide("changed"));
    assert!(event.borrow().cancel_action);
    check(
        &engine,
        r#"
        var record=xfa.form.resolveNode('record'), field=record.resolveNode('amount');
        field.fontColor === '0,0,0' && field.fillColor === '255,255,255' && field.borderWidth === '0.5pt'
    "#,
    );
    check(
        &engine,
        r#"
        field.fontColor='12,34,56'; field.fillColor='240,230,220';
        field.borderWidth='2mm'; field.borderColor='1,2,3';
        field.fontColor==='12,34,56' && field.fillColor==='240,230,220' && field.borderWidth==='2mm' && field.borderColor==='1,2,3' &&
        field.getElement('font').getElement('fill').getElement('color').getAttribute('value')==='12,34,56'
    "#,
    );
    check(
        &engine,
        r#"
        field.validationMessage='script failed'; field.mandatoryMessage='required'; field.mandatory='error';
        field.validationMessage==='script failed' && field.mandatoryMessage==='required' && field.mandatory==='error'
    "#,
    );
    check(
        &engine,
        r#"
        field.mandatory='invalid-null-test';
        var xml=field.saveXML();
        field.mandatory==='disabled' && xml.includes('12,34,56') && xml.includes('script failed') && xml.includes('required')
    "#,
    );
    let saved = engine
        .eval("field.saveXML()")
        .unwrap()
        .as_string()
        .unwrap()
        .to_std_string_escaped();
    let saved = saved
        .split_once("?>")
        .map_or(saved.as_str(), |(_, rest)| rest);
    let mut bytes = format!(
        "<form xmlns=\"http://www.xfa.org/schema/xfa-form/3.3/\"><subform>{saved}</subform></form>"
    )
    .into_bytes();
    bytes.push(0);
    let xml = Rc::new(RefCell::new(
        CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
            .parse()
            .unwrap(),
    ));
    let reloaded = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(reloaded);
    assert!(builder.build_document(xml, XfaPacketType::Form));
    let restored = builder
        .get_root_node()
        .unwrap()
        .first_child()
        .unwrap()
        .first_child()
        .unwrap();
    let validate = restored.get_property(0, XfaElement::Validate).0.unwrap();
    assert_eq!(
        validate.validate_get_message_text(&wide("scriptTest")),
        wide("script failed")
    );
    assert_eq!(
        validate.validate_get_message_text(&wide("nullTest")),
        wide("required")
    );
    check(
        &engine,
        r#"
        var marker={}, caught=false;
        try{field.fontColor={toString(){throw marker}}}catch(e){caught=e===marker}
        caught && field.fontColor==='12,34,56'
    "#,
    );
}
#[test]
fn inherited_container_methods_leaf_classes_and_metadata_follow_source() {
    let (_doc, engine) = runtime();
    check(
        &engine,
        r#"
        var record=xfa.form.resolveNode('record'), field=record.resolveNode('amount'), label=record.resolveNode('label');
        var desc=record.resolveNode('#desc');
        var list=field.getDeltas(1,2,3);
        list.length===0 && field.getDelta()===undefined && desc.metadata()==='' && desc.metadata({})==='' &&
        label.rawValue==='caption'
    "#,
    );
    check(
        &engine,
        r#"
        label.rawValue=3; var unchanged=label.rawValue==='caption';
        label.rawValue='new label';
        var boolean=xfa.form.createNode('boolean');
        boolean.value=1;
        unchanged && label.rawValue==='new label' && boolean.value===true &&
        boolean.getElement===undefined && typeof field.getElement==='function'
    "#,
    );
}
#[test]
fn font_metrics_color_defaults_and_validate_text_fallbacks() {
    let (doc, _engine) = runtime();
    let form = doc
        .get_root()
        .unwrap()
        .get_first_child_by_class(XfaElement::Form)
        .unwrap();
    let field = form
        .get_first_child_by_name(&wide("record"))
        .unwrap()
        .get_first_child_by_name(&wide("amount"))
        .unwrap();
    let font = field.get_or_create_property(0, XfaElement::Font).unwrap();
    assert_eq!(font.font_get_typeface(), wide("Courier"));
    assert_eq!(font.font_get_size(), 10.0);
    assert_eq!(font.font_get_horizontal_scale(), 100.0);
    assert_eq!(font.font_get_vertical_scale(), 100.0);
    assert_eq!(font.font_get_color(), 0xff000000);
    font.set_cdata(XfaAttribute::FontHorizontalScale, &wide("80%"));
    font.set_cdata(XfaAttribute::FontVerticalScale, &wide("-1%"));
    font.set_cdata(XfaAttribute::LetterSpacing, &wide("0.2em"));
    // Enum attributes use the typed source setter, not CData storage.
    font.set_attribute_by_enum(XfaAttribute::Weight, &wide("bold"), false);
    font.set_attribute_by_enum(XfaAttribute::Posture, &wide("italic"), false);
    assert_eq!(font.font_get_horizontal_scale(), 80.0);
    assert_eq!(font.font_get_vertical_scale(), 100.0);
    assert_eq!(font.font_get_letter_spacing(), 2.0);
    assert!(font.font_is_bold() && font.font_is_italic());
    let validate = field
        .get_or_create_property(0, XfaElement::Validate)
        .unwrap();
    let message = validate
        .get_or_create_property(0, XfaElement::Message)
        .unwrap();
    let text = message.create_same_packet_node(XfaElement::Text).unwrap();
    assert!(message.insert_child_before_and_notify(&text, None));
    text.set_content(&wide("fallback"), &wide("fallback"), false);
    assert_eq!(
        validate.validate_get_message_text(&wide("scriptTest")),
        wide("fallback")
    );
    assert!(validate.validate_set_message_text(&wide("nullTest"), &wide("replacement")));
    assert_eq!(message.count_children(XfaElement::Text, false), 1);
    assert_eq!(
        validate.validate_get_message_text(&wide("scriptTest")),
        wide("replacement")
    );
    assert_eq!(
        crate::xfa::fxfa::parser::cxfa_presentation::string_to_argb(&wide(" 12, 34, 56")),
        0xff0c2238
    );
}

#[test]
fn native_constructor_lineage_controls_method_installation() {
    let (_doc, engine) = runtime();
    check(
        &engine,
        r#"
      ['boolean','decimal','exData','float','integer','text','time'].every(function(name){
        var node=xfa.form.createNode(name);
        return node.getElement===undefined && node.resolveNode===undefined && node.getDelta===undefined;
      }) && ['area','contentArea','pageArea','pageSet','variables','subformSet','field','draw','subform'].every(function(name){
        var node=xfa.form.createNode(name);
        return typeof node.getElement==='function' && typeof node.getDelta==='function' && node.getDeltas().length===0;
      }) && xfa.form.createNode('exclGroup').getDelta===undefined
    "#,
    );
}
