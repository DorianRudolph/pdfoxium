// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete cases from the same-named pinned C++ source.
#![allow(non_snake_case)]
use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
use crate::fxcrt::xml::cfx_xmlnode::XmlNodeType;
use crate::fxcrt::xml::test_support::wide;

#[test]
fn Root() {
    let doc = CfxXmlDocument::new();
    assert!(doc.root().is_alive());
}

#[test]
fn CreateNode() {
    let mut doc = CfxXmlDocument::new();
    let node = doc.create_element(&wide("elem"));
    assert_eq!(XmlNodeType::Element, node.node_type());
    assert_eq!(wide("elem").as_slice(), node.name().as_ref());
}
