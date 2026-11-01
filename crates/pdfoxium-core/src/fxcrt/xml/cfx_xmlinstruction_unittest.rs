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
    let node = fixture.create_instruction(&wide("acrobat"));
    assert_eq!(XmlNodeType::Instruction, node.node_type());
}

#[test]
fn AcrobatInstruction() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_instruction(&wide("acrobat"));
    assert!(node.is_acrobat());
    assert!(!(node.is_original_xfa_version()));
}

#[test]
fn OriginalXFAInstruction() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_instruction(&wide("originalXFAVersion"));
    assert!(node.is_original_xfa_version());
    assert!(!(node.is_acrobat()));
}

#[test]
fn TargetData() {
    let mut fixture = CfxXmlDocument::new();
    let node = fixture.create_instruction(&wide("acrobat"));
    assert_eq!(0usize, node.target_data().len());
    node.append_data(&wide("firstString"));
    node.append_data(&wide("secondString"));
    let data = node.target_data();
    assert_eq!(2usize, data.len());
    assert_eq!(
        wide("firstString").as_slice(),
        data.get(0).unwrap().as_ref()
    );
    assert_eq!(
        wide("secondString").as_slice(),
        data.get(1).unwrap().as_ref()
    );
}

#[test]
fn Clone() {
    let mut fixture = CfxXmlDocument::new();
    let mut doc = CfxXmlDocument::new();
    let node = fixture.create_instruction(&wide("acrobat"));
    node.append_data(&wide("firstString"));
    node.append_data(&wide("secondString"));
    let clone = node.clone_into(&mut doc);
    assert!(clone.is_alive());
    assert_eq!(XmlNodeType::Instruction, clone.node_type());
    let inst = clone.clone();
    assert!(inst.is_acrobat());
    let data = inst.target_data();
    assert_eq!(2usize, data.len());
    assert_eq!(
        wide("firstString").as_slice(),
        data.get(0).unwrap().as_ref()
    );
    assert_eq!(
        wide("secondString").as_slice(),
        data.get(1).unwrap().as_ref()
    );
}

#[test]
fn SaveXML() {
    let mut fixture = CfxXmlDocument::new();
    let mut stream = StringWriteStream::default();
    let node = fixture.create_instruction(&wide("xml"));
    node.save(&mut stream);
    assert_eq!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>", stream.text());
}

#[test]
fn SaveAcrobat() {
    let mut fixture = CfxXmlDocument::new();
    let mut stream = StringWriteStream::default();
    let node = fixture.create_instruction(&wide("acrobat"));
    node.append_data(&wide("http://www.xfa.org/schema/xfa-template/3.3/"));
    node.append_data(&wide("Display:1"));
    node.save(&mut stream);
    assert_eq!(
        "<?acrobat http://www.xfa.org/schema/xfa-template/3.3/ Display:1 ?>",
        stream.text()
    );
}

fn parse(input: &[u8]) -> CfxXmlDocument {
    use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
    use crate::fxcrt::xml::cfx_xmlparser::CfxXmlParser;
    use std::rc::Rc;
    // Native as_byte_span(input) includes the char array's terminating NUL.
    let mut bytes = input.to_vec();
    bytes.push(0);
    let doc = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
        .parse()
        .expect("parsed XML document");
    doc
}

#[test]
fn ParseAndReSave() {
    let input = concat!(
        "<?acrobat http://www.xfa.org/schema/xfa-template/3.3/ Display:1 ?>\n",
        "<node></node>",
    );
    let doc = parse(input.as_bytes());
    let root = doc.root();
    assert!(root.first_child().is_some());
    assert_eq!(
        XmlNodeType::Instruction,
        root.first_child().unwrap().node_type()
    );
    let node = root.first_child().unwrap();
    assert_eq!(XmlNodeType::Instruction, node.node_type());
    assert!(node.is_acrobat());
    let data = node.target_data();
    assert_eq!(2, data.len());
    assert_eq!(
        wide("http://www.xfa.org/schema/xfa-template/3.3/").as_slice(),
        data.get(0).unwrap().as_ref()
    );
    assert_eq!(wide("Display:1").as_slice(), data.get(1).unwrap().as_ref());
    let mut out_stream = StringWriteStream::default();
    node.save(&mut out_stream);
    assert_eq!(
        "<?acrobat http://www.xfa.org/schema/xfa-template/3.3/ Display:1 ?>",
        out_stream.text()
    );
}

#[test]
fn ParseAndReSaveInnerInstruction() {
    let input = concat!(
        "<node>\n",
        "<?acrobat http://www.xfa.org/schema/xfa-template/3.3/ Display:1 ?>\n",
        "</node>",
    );
    let doc = parse(input.as_bytes());
    let root = doc.root();
    assert!(root.first_child().is_some());
    assert_eq!(
        XmlNodeType::Element,
        root.first_child().unwrap().node_type()
    );
    let node = root.first_child().unwrap();
    assert_eq!(wide("node").as_slice(), node.name().as_ref());
    let mut instruction = None;
    let mut elem = node.first_child();
    while instruction.is_none() {
        let Some(current) = elem else {
            break;
        };
        if current.node_type() == XmlNodeType::Instruction {
            instruction = Some(current.clone());
        }
        elem = current.next_sibling();
    }
    assert!(instruction.is_some());
    let instruction = instruction.unwrap();
    assert!(instruction.is_acrobat());
    let data = instruction.target_data();
    assert_eq!(2, data.len());
    assert_eq!(
        wide("http://www.xfa.org/schema/xfa-template/3.3/").as_slice(),
        data.get(0).unwrap().as_ref()
    );
    assert_eq!(wide("Display:1").as_slice(), data.get(1).unwrap().as_ref());
    let mut out_stream = StringWriteStream::default();
    node.save(&mut out_stream);
    assert_eq!(
        concat!(
            "<node>\n",
            "<?acrobat http://www.xfa.org/schema/xfa-template/3.3/ Display:1 ?>\n",
            "</node>"
        ),
        out_stream.text()
    );
}
