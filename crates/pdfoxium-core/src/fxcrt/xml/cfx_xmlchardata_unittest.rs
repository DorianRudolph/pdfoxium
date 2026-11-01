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
    let data = fixture.create_chardata(&wide("My Data"));
    assert_eq!(XmlNodeType::CharData, data.node_type());
}

#[test]
fn GetText() {
    let mut fixture = CfxXmlDocument::new();
    let data = fixture.create_chardata(&wide("My Data"));
    assert_eq!(wide("My Data").as_slice(), data.text().as_ref());
}

#[test]
fn Clone() {
    let mut fixture = CfxXmlDocument::new();
    let mut doc = CfxXmlDocument::new();
    let data = fixture.create_chardata(&wide("My Data"));
    let clone = data.clone_into(&mut doc);
    assert!(clone.is_alive());
    assert_ne!(data.clone(), clone);
    assert_eq!(XmlNodeType::CharData, clone.node_type());
    assert_eq!(wide("My Data").as_slice(), clone.text().as_ref());
}

#[test]
fn Save() {
    let mut fixture = CfxXmlDocument::new();
    let mut stream = StringWriteStream::default();
    let data = fixture.create_chardata(&wide("My Data"));
    data.save(&mut stream);
    assert_eq!("<![CDATA[My Data]]>", stream.text());
}
