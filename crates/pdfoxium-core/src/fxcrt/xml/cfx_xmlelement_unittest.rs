// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete cases from the same-named pinned C++ source.
#![allow(non_snake_case)]
use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
use crate::fxcrt::xml::cfx_xmlnode::XmlNodeType;
use crate::fxcrt::xml::test_support::{wide, StringWriteStream};

#[test]
fn GetType() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_element(&wide("node"));
    assert_eq!(XmlNodeType::Element, node.node_type());
}

#[test]
fn GetName() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_element(&wide("node"));
    assert_eq!(wide("node").as_slice(), node.name().as_ref());
}

#[test]
fn GetLocalTagName() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node1"));
    assert_eq!(wide("node1").as_slice(), node1.local_tag_name().as_ref());
    let node2 = fixture.create_element(&wide("test:node2"));
    assert_eq!(wide("node2").as_slice(), node2.local_tag_name().as_ref());
}

#[test]
fn GetNamespacePrefix() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node1"));
    assert_eq!(wide("").as_slice(), node1.namespace_prefix().as_ref());
    let node2 = fixture.create_element(&wide("test:node2"));
    assert_eq!(wide("test").as_slice(), node2.namespace_prefix().as_ref());
}

#[test]
fn GetNamespaceURI() {
    let mut fixture = CfxXmlDocument::new();
    let node1 = fixture.create_element(&wide("node1"));
    assert_eq!(wide("").as_slice(), node1.namespace_uri().as_ref());
    node1.set_attribute(&wide("xmlns"), &wide("https://example.org/ns1"));
    assert_eq!(
        wide("https://example.org/ns1").as_slice(),
        node1.namespace_uri().as_ref()
    );
    let node2 = fixture.create_element(&wide("test:node2"));
    assert_eq!(wide("").as_slice(), node2.namespace_uri().as_ref());
    node2.set_attribute(&wide("xmlns"), &wide("https://example.org/ns2"));
    assert_eq!(wide("").as_slice(), node2.namespace_uri().as_ref());
    node2.set_attribute(&wide("xmlns:test"), &wide("https://example.org/ns2"));
    assert_eq!(
        wide("https://example.org/ns2").as_slice(),
        node2.namespace_uri().as_ref()
    );
}

#[test]
fn Attributes() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_element(&wide("test:node"));
    node.set_attribute(&wide("first"), &wide("one"));
    node.set_attribute(&wide("second"), &wide("two"));
    assert!(node.has_attribute(&wide("first")));
    assert_eq!(
        wide("one").as_slice(),
        node.get_attribute(&wide("first")).as_ref()
    );
    assert!(node.has_attribute(&wide("second")));
    assert_eq!(
        wide("two").as_slice(),
        node.get_attribute(&wide("second")).as_ref()
    );
    assert_eq!(2usize, node.attribute_count());
    node.remove_attribute(&wide("first"));
    assert!(!(node.has_attribute(&wide("first"))));
    assert_eq!(1usize, node.attribute_count());
}

#[test]
fn Clone() {
    let mut fixture = CfxXmlDocument::new();
    let mut doc = CfxXmlDocument::new();
    let node = fixture.create_element(&wide("test:node"));
    node.set_attribute(&wide("first"), &wide("one"));
    node.set_attribute(&wide("second"), &wide("two"));
    node.set_attribute(&wide("xmlns:test"), &wide("https://example.org/test"));
    let text_child1 = fixture.create_text(&wide("Text Child"));
    node.append_last_child(&text_child1);
    let node_child1 = fixture.create_element(&wide("Node child"));
    node.append_last_child(&node_child1);
    let clone = node.clone_into(&mut doc);
    assert!(clone.is_alive());
    assert_eq!(XmlNodeType::Element, clone.node_type());
    let inst = clone.clone();
    assert_eq!(wide("test:node").as_slice(), inst.name().as_ref());
    assert_eq!(wide("node").as_slice(), inst.local_tag_name().as_ref());
    assert_eq!(wide("test").as_slice(), inst.namespace_prefix().as_ref());
    assert_eq!(
        wide("https://example.org/test").as_slice(),
        inst.namespace_uri().as_ref()
    );
    assert!(inst.has_attribute(&wide("first")));
    assert_eq!(
        wide("one").as_slice(),
        inst.get_attribute(&wide("first")).as_ref()
    );
    assert!(inst.has_attribute(&wide("second")));
    assert_eq!(
        wide("two").as_slice(),
        inst.get_attribute(&wide("second")).as_ref()
    );
    // Only clone the Text node, so expect only one child.
    assert!(inst.first_child().is_some());
    assert!(inst.first_child().unwrap().next_sibling().is_none());
    assert_eq!(XmlNodeType::Text, inst.first_child().unwrap().node_type());
    let text = inst.first_child().unwrap();
    assert_eq!(wide("Text Child").as_slice(), text.text().as_ref());
}

#[test]
fn Save() {
    let mut fixture = CfxXmlDocument::new();
    let mut stream = StringWriteStream::default();
    let node = fixture.create_element(&wide("root"));
    node.save(&mut stream);
    assert_eq!("<root />", stream.text());
}

#[test]
fn SaveWithAttributes() {
    let mut fixture = CfxXmlDocument::new();
    let mut stream = StringWriteStream::default();
    let node = fixture.create_element(&wide("root"));
    node.set_attribute(&wide("first"), &wide("one"));
    node.set_attribute(&wide("second"), &wide("two"));
    node.save(&mut stream);
    assert_eq!("<root first=\"one\" second=\"two\" />", stream.text());
}

#[test]
fn SaveWithChildren() {
    let mut fixture = CfxXmlDocument::new();
    let mut stream = StringWriteStream::default();
    let node = fixture.create_element(&wide("node"));
    let text_child1 = fixture.create_text(&wide("Text Child 1"));
    node.append_last_child(&text_child1);
    let node_child1 = fixture.create_element(&wide("node-child"));
    node.append_last_child(&node_child1);
    let text_child2 = fixture.create_text(&wide("Text Child 2"));
    node_child1.append_last_child(&text_child2);
    let char_data1 = fixture.create_chardata(&wide("Char Data"));
    node.append_last_child(&char_data1);
    node.save(&mut stream);
    assert_eq!(
        concat!(
            "<node>",
            "Text Child 1",
            "<node-child>Text Child 2</node-child>",
            "<![CDATA[Char Data]]>",
            "</node>"
        ),
        stream.text()
    );
}

#[test]
fn SaveWithNamespace() {
    let mut fixture = CfxXmlDocument::new();
    let mut stream = StringWriteStream::default();
    let node = fixture.create_element(&wide("test:root"));
    node.set_attribute(&wide("xmlns:test"), &wide("https://example.org/ns1"));
    node.save(&mut stream);
    assert_eq!(
        "<test:root xmlns:test=\"https://example.org/ns1\" />",
        stream.text()
    );
}

#[test]
fn GetFirstChildNamed() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_element(&wide("node"));
    let node_child1 = fixture.create_element(&wide("node-child"));
    node.append_last_child(&node_child1);
    let found = node.first_child_named(&wide("node-child"));
    assert!(found.is_some());
    assert_eq!(Some(node_child1.clone()), found);
}

#[test]
fn GetFirstChildNamedMissing() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_element(&wide("node"));
    let node_child1 = fixture.create_element(&wide("node-child"));
    node.append_last_child(&node_child1);
    let found = node.first_child_named(&wide("node-sibling"));
    assert!(found.is_none());
}

#[test]
fn GetNthChildNamed() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_element(&wide("node"));
    let node_child1 = fixture.create_element(&wide("node-child"));
    let node_child2 = fixture.create_element(&wide("node-child"));
    let node_child3 = fixture.create_element(&wide("node-child"));
    node.append_last_child(&node_child1);
    node.append_last_child(&node_child2);
    node.append_last_child(&node_child3);
    let found = node.nth_child_named(&wide("node-child"), 2);
    assert!(found.is_some());
    assert_eq!(Some(node_child3.clone()), found);
}

#[test]
fn GetNthChildNamedMissingChild() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_element(&wide("node"));
    let node_child1 = fixture.create_element(&wide("node-child"));
    let node_child2 = fixture.create_element(&wide("node-child"));
    let node_child3 = fixture.create_element(&wide("node-child"));
    node.append_last_child(&node_child1);
    node.append_last_child(&node_child2);
    node.append_last_child(&node_child3);
    let found = node.nth_child_named(&wide("node-child"), 5);
    assert!(found.is_none());
}

#[test]
fn GetTextData() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_element(&wide("node"));
    let text_child1 = fixture.create_text(&wide("Text Child 1"));
    node.append_last_child(&text_child1);
    let node_child1 = fixture.create_element(&wide("Node child"));
    node.append_last_child(&node_child1);
    let text_child2 = fixture.create_text(&wide("Text Child 2"));
    node_child1.append_last_child(&text_child2);
    let char_data1 = fixture.create_chardata(&wide("Char Data"));
    node.append_last_child(&char_data1);
    assert_eq!(
        wide("Text Child 1Char Data").as_slice(),
        node.text_data().as_slice()
    );
}

#[test]
fn save_reload_preserves_mixed_text_cdata_and_instruction_spacing() {
    use crate::fxcrt::{
        cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
    };
    use std::rc::Rc;
    let parse = |input: &[u8]| {
        CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(input)))
            .parse()
            .unwrap()
    };
    let document = parse(
        b"<node> first <b>bold</b> tail <![CDATA[cdata]]><?acrobat Display:1 ?> end </node>\0",
    );
    let node = document.root().first_child().unwrap();
    let before = node.text_data();
    let mut stream = StringWriteStream::default();
    node.save(&mut stream);
    let output = stream.text();
    assert!(!output.contains('\n'));
    assert!(output.contains("> first <b>bold</b> tail "));
    let reloaded = parse(output.as_bytes());
    let result = reloaded.root().first_child().unwrap();
    assert_eq!(result.text_data(), before);
    assert_eq!(
        result.first_child_named(&wide("b")).unwrap().text_data(),
        wide("bold")
    );
    let mut before_child = node.first_child();
    let mut after_child = result.first_child();
    while let Some(before) = before_child {
        let after = after_child.unwrap();
        assert_eq!(after.node_type(), before.node_type());
        if before.node_type() == XmlNodeType::Instruction {
            assert_eq!(
                after.target_data().iter().collect::<Vec<_>>(),
                before.target_data().iter().collect::<Vec<_>>()
            );
        }
        before_child = before.next_sibling();
        after_child = after.next_sibling();
    }
    assert!(after_child.is_none());
}
