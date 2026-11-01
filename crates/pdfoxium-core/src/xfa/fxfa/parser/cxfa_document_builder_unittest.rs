// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Complete `cxfa_document_builder_unittest.cpp` cases and packet/data paths.
#![allow(non_snake_case)]
use super::*;
use crate::fxcrt::{
    cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
};

fn parse(input: &str) -> Option<Rc<RefCell<CfxXmlDocument>>> {
    let mut bytes = input.as_bytes().to_vec();
    bytes.push(0);
    let stream = Rc::new(CfxReadOnlySpanStream::new(&bytes));
    let parsed = CfxXmlParser::new(stream)
        .parse()
        .map(|xml| Rc::new(RefCell::new(xml)));
    parsed
}
fn parse_and_build(doc: &Rc<CxfaDocument>, input: &str) -> Option<CxfaNode> {
    let xml = parse(input)?;
    let mut builder = CxfaDocumentBuilder::new(doc.clone());
    if !builder.build_document(xml, XfaPacketType::Config) {
        return None;
    }
    builder.get_root_node()
}
#[test]
fn EmptyInput() {
    assert!(parse_and_build(&CxfaDocument::new(), "").is_none());
}
#[test]
fn BadInput() {
    assert!(parse_and_build(&CxfaDocument::new(), "<<<>bar?>>>>>>>").is_none());
}
#[test]
fn XMLInstructionsScriptOff() {
    let doc = CxfaDocument::new();
    assert!(!doc.is_scripting());
    let root=parse_and_build(&doc,"<config>\n<?originalXFAVersion http://www.xfa.org/schema/xfa-template/2.7 v2.7-scripting:0 ?>\n</config>");
    assert!(root.is_some());
    assert!(!doc.is_scripting());
}
#[test]
fn XMLInstructionsScriptOn() {
    let doc = CxfaDocument::new();
    assert!(!doc.is_scripting());
    let root=parse_and_build(&doc,"<config>\n<?originalXFAVersion http://www.xfa.org/schema/xfa-template/2.7 v2.7-scripting:1 ?>\n</config>");
    assert!(root.is_some());
    assert!(doc.is_scripting());
}
#[test]
fn XMLInstructionsStrictScope() {
    let doc = CxfaDocument::new();
    assert!(!doc.is_strict_scoping());
    let root = parse_and_build(
        &doc,
        "<config><?acrobat JavaScript strictScoping ?>\n</config>",
    );
    assert!(root.is_some());
    assert!(doc.is_strict_scoping());
}
#[test]
fn XMLInstructionsStrictScopeBad() {
    let doc = CxfaDocument::new();
    assert!(!doc.is_strict_scoping());
    let root = parse_and_build(
        &doc,
        "<config><?acrobat JavaScript otherScoping ?>\n</config>",
    );
    assert!(root.is_some());
    assert!(!doc.is_strict_scoping());
}
#[test]
fn MultipleXMLInstructions() {
    let doc = CxfaDocument::new();
    assert!(!doc.is_scripting());
    assert!(!doc.is_strict_scoping());
    let root=parse_and_build(&doc,"<config><?originalXFAVersion http://www.xfa.org/schema/xfa-template/2.7 v2.7-scripting:1 ?>\n<?acrobat JavaScript strictScoping ?>\n</config>");
    assert!(root.is_some());
    assert!(doc.is_scripting());
    assert!(doc.is_strict_scoping());
}
fn build_packet(input: &str, packet: XfaPacketType) -> (Rc<CxfaDocument>, Option<CxfaNode>) {
    let doc = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(doc.clone());
    let result = builder.build_document(parse(input).unwrap(), packet);
    assert_eq!(result, builder.get_root_node().is_some());
    (doc, builder.get_root_node())
}
#[test]
fn xdp_packet_order_namespaces_and_duplicate_rejection() {
    let input="<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/' uuid='id' timeStamp='time'><form xmlns='http://www.xfa.org/schema/xfa-form/2.8/'/><config/><datasets xmlns='http://www.xfa.org/schema/xfa-data/1.0/'/><template xmlns='http://www.xfa.org/schema/xfa-template/2.8/'/><custom xmlns='urn:custom'/></xdp:xdp>";
    let (doc, root) = build_packet(input, XfaPacketType::Xdp);
    let root = root.unwrap();
    assert_eq!(root.get_element_type(), XfaElement::Xfa);
    assert_eq!(root.get_cdata(XfaAttribute::Uuid), wide("id"));
    assert_eq!(root.get_cdata(XfaAttribute::TimeStamp), wide("time"));
    let mut elements = Vec::new();
    let mut current = root.first_child();
    while let Some(node) = current {
        elements.push(node.get_element_type());
        current = node.next_sibling();
    }
    assert_eq!(
        elements,
        [
            XfaElement::Config,
            XfaElement::Template,
            XfaElement::Packet,
            XfaElement::DataModel,
            XfaElement::Form
        ]
    );
    assert!(root.get_xml_mapping_node().unwrap().is_alive());
    assert_eq!(doc.get_cur_version_mode(), 208);
    let template = "<template xmlns='http://www.xfa.org/schema/xfa-template/2.8/'/>";
    for duplicate in ["<config/><config/>".to_owned(),format!("{template}{template}"),"<datasets xmlns='http://www.xfa.org/schema/xfa-data/1.0/'/><datasets xmlns='http://www.xfa.org/schema/xfa-data/1.0/'/>".to_owned(),"<form xmlns='http://www.xfa.org/schema/xfa-form/2.8/'/><form xmlns='http://www.xfa.org/schema/xfa-form/2.8/'/>".to_owned()] {
        let xdp=format!("<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'>{template}{duplicate}</xdp:xdp>");
        assert!(build_packet(&xdp,XfaPacketType::Xdp).1.is_none(),"{duplicate}");
    }
    assert!(build_packet(
        "<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><config/></xdp:xdp>",
        XfaPacketType::Xdp
    )
    .1
    .is_none());
    assert!(build_packet("<xdp xmlns='urn:wrong'/>", XfaPacketType::Xdp)
        .1
        .is_none());
}
#[test]
fn datasets_metadata_nil_compound_and_rich_content() {
    let input="<xfa:datasets xmlns:xfa='http://www.xfa.org/schema/xfa-data/1.0/' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'><xfa:data><person code='7'><name>Alice</name><empty xsi:nil='true'>ignored</empty><value xfa:dataNode='dataValue'>before<part>middle</part>after</value><rich><body xmlns='http://www.w3.org/1999/xhtml'>one<br/>two</body></rich></person></xfa:data></xfa:datasets>";
    let (_doc, root) = build_packet(input, XfaPacketType::Datasets);
    let root = root.unwrap();
    let data = root.get_first_child_by_name(&wide("data")).unwrap();
    let person = data.get_first_child_by_name(&wide("person")).unwrap();
    let code = person.get_first_child_by_name(&wide("code")).unwrap();
    assert_eq!(
        code.get_enum(XfaAttribute::Contains),
        XfaAttributeValue::MetaData
    );
    assert_eq!(code.get_cdata(XfaAttribute::QualifiedName), wide("code"));
    assert_eq!(code.get_cdata(XfaAttribute::Value), wide("7"));
    assert_eq!(
        person
            .get_first_child_by_name(&wide("name"))
            .unwrap()
            .get_cdata(XfaAttribute::Value),
        wide("Alice")
    );
    let empty = person.get_first_child_by_name(&wide("empty")).unwrap();
    assert!(empty.get_cdata(XfaAttribute::Value).is_empty());
    assert!(!empty
        .get_xml_mapping_node()
        .unwrap()
        .has_attribute(&wide("xsi:nil")));
    let value = person.get_first_child_by_name(&wide("value")).unwrap();
    assert_eq!(
        value.get_cdata(XfaAttribute::Value),
        wide("beforemiddleafter")
    );
    assert_eq!(
        value.first_child().unwrap().get_cdata(XfaAttribute::Value),
        wide("before")
    );
    assert_eq!(
        value.last_child().unwrap().get_cdata(XfaAttribute::Value),
        wide("after")
    );
    assert_eq!(
        person
            .get_first_child_by_name(&wide("rich"))
            .unwrap()
            .get_cdata(XfaAttribute::Value),
        wide("one\ntwo")
    );
}
#[test]
fn standalone_data_wrapping_and_other_packets() {
    let (doc, root) = build_packet(
        "<record><field>value</field></record>",
        XfaPacketType::Datasets,
    );
    let root = root.unwrap();
    assert_eq!(root.get_element_type(), XfaElement::DataGroup);
    assert_eq!(root.get_cdata(XfaAttribute::Name), wide("data"));
    assert_eq!(
        root.first_child().unwrap().get_cdata(XfaAttribute::Name),
        wide("record")
    );
    assert!(doc
        .xml_document()
        .unwrap()
        .borrow()
        .root()
        .first_child()
        .is_some()); // injected declaration remains
    for (input, packet, element) in [
        (
            "<localeSet xmlns='http://www.xfa.org/schema/xfa-locale-set/2.7/'/>",
            XfaPacketType::LocaleSet,
            XfaElement::LocaleSet,
        ),
        (
            "<connectionSet xmlns='http://www.xfa.org/schema/xfa-connection-set/2.8/'/>",
            XfaPacketType::ConnectionSet,
            XfaElement::ConnectionSet,
        ),
        (
            "<sourceSet xmlns='http://www.xfa.org/schema/xfa-source-set/2.8/'/>",
            XfaPacketType::SourceSet,
            XfaElement::SourceSet,
        ),
        (
            "<xdc xmlns='http://www.xfa.org/schema/xdc/1.0/'/>",
            XfaPacketType::Xdc,
            XfaElement::Xdc,
        ),
        ("<custom/>", XfaPacketType::User, XfaElement::Packet),
    ] {
        assert_eq!(
            build_packet(input, packet).1.unwrap().get_element_type(),
            element,
            "{input}"
        );
    }
}

#[test]
fn normal_content_one_of_nil_and_form_restore_attributes() {
    let config = "<config><locale>en_US</locale><validate>rule</validate></config>";
    let (_doc, root) = build_packet(config, XfaPacketType::Config);
    let root = root.unwrap();
    assert_eq!(
        root.get_first_child_by_class(XfaElement::Locale)
            .unwrap()
            .get_cdata(XfaAttribute::Value),
        wide("en_US")
    );
    let validate = root.get_first_child_by_class(XfaElement::Validate).unwrap();
    assert_eq!(
        validate
            .first_child()
            .unwrap()
            .get_cdata(XfaAttribute::Value),
        wide("rule")
    );
    let template="<template xmlns='http://www.xfa.org/schema/xfa-template/2.8/'><subform name='form'><field name='field'><value><text>first</text><integer>2</integer></value></field><draw name='rich'><value><exData contentType='text/html'><body xmlns='http://www.w3.org/1999/xhtml'>a<br/>b</body></exData></value></draw><draw name='xml'><value><exData contentType='text/xml'><root><p>one</p><p>two</p></root></exData></value></draw><draw name='nil'><value><text nil='true'>ignored</text></value></draw></subform></template>";
    let (_doc, root) = build_packet(template, XfaPacketType::Template);
    let subform = root.unwrap().first_child().unwrap();
    let field = subform.get_first_child_by_name(&wide("field")).unwrap();
    let value = field.get_first_child_by_class(XfaElement::Value).unwrap();
    assert_eq!(
        value.first_child().unwrap().get_element_type(),
        XfaElement::Text
    );
    let text = value.first_child().unwrap();
    // CXFA_Text is a ContentNode. ParseContentNode stores its value in
    // the #text child; the node's own Value attribute is not that content.
    let raw = text.first_child().unwrap();
    assert_eq!(raw.get_element_type(), XfaElement::Sharptext);
    assert_eq!(raw.get_cdata(XfaAttribute::Value), wide("first"));
    assert_eq!(text.get_content(), wide("first"));
    assert!(value.first_child().unwrap().next_sibling().is_none());
    for (name, expected, raw) in [
        ("rich", "a\nb", XfaElement::SharpxHTML),
        ("xml", "one\ntwo\n", XfaElement::Sharpxml),
    ] {
        let draw = subform.get_first_child_by_name(&wide(name)).unwrap();
        let ex_data = draw.first_child().unwrap().first_child().unwrap();
        let child = ex_data.first_child().unwrap();
        assert_eq!(child.get_element_type(), raw);
        assert_eq!(child.get_cdata(XfaAttribute::Value), wide(expected));
    }
    let nil = subform.get_first_child_by_name(&wide("nil")).unwrap();
    let nil_text = nil.first_child().unwrap().first_child().unwrap();
    assert_eq!(nil_text.get_element_type(), XfaElement::Text);
    assert!(nil_text.first_child().is_none());
    assert!(nil_text.get_content().is_empty());
    for (restore, expected) in [("manual", false), ("auto", true)] {
        let xdp=format!("<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><template><subform restoreState='{restore}'/></template><form><subform name='saved' h='10pt'/></form></xdp:xdp>");
        let (_doc, root) = build_packet(&xdp, XfaPacketType::Xdp);
        let form = root
            .unwrap()
            .get_first_child_by_class(XfaElement::Form)
            .unwrap();
        let subform = form.first_child().unwrap();
        assert_eq!(subform.get_cdata(XfaAttribute::Name), wide("saved"));
        // CData serializes a typed measurement using the node's attribute helper.
        assert_eq!(subform.has_local_attribute(XfaAttribute::H), expected);
    }
}
#[test]
fn construct_existing_nodes_and_depth_guards() {
    let doc = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(doc.clone());
    let xml = builder
        .build(parse("<value><part>hello<br/>world</part></value>").unwrap())
        .unwrap();
    let node = doc
        .create_node(XfaPacketType::Datasets, XfaElement::DataValue)
        .unwrap();
    builder.construct_xfa_node(&node, &xml);
    assert_eq!(builder.get_root_node(), Some(node.clone()));
    let part = node.first_child().unwrap();
    assert_eq!(part.get_cdata(XfaAttribute::Name), wide("part"));
    assert_eq!(part.get_cdata(XfaAttribute::Value), wide("hello\nworld"));
    let config = doc
        .create_node(XfaPacketType::Config, XfaElement::Config)
        .unwrap();
    builder.execute_recursion_depth = MAX_EXECUTE_RECURSION + 1;
    assert!(builder
        .normal_loader(&config, &xml, XfaPacketType::Config, true)
        .is_none());
    builder.execute_recursion_depth = 0;
    assert!(builder
        .parse_data_group(
            &node,
            &xml,
            XfaPacketType::Datasets,
            MAX_EXECUTE_RECURSION + 1
        )
        .is_none());
    assert!(builder
        .parse_data_value(
            &node,
            &xml,
            XfaPacketType::Datasets,
            MAX_EXECUTE_RECURSION + 1
        )
        .is_none());
}
