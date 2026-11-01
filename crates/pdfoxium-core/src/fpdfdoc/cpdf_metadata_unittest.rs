// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete source cases from `core/fpdfdoc/cpdf_metadata_unittest.cpp`.
#![allow(non_snake_case)]

use super::{CpdfMetadata, UnsupportedFeature};
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

#[test]
fn CheckSharedFormEmailAtTopLevel() {
    let data = concat!(
        "<?xml charset=\"utf-8\"?>\n",
        "<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\">\n",
        "<adhocwf:workflowType>0</adhocwf:workflowType>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node>",
    );
    let stream = retain_object(CpdfStream::from_bytes(data.as_bytes()));
    let metadata = CpdfMetadata::new(stream);
    let results = metadata.check_for_shared_form();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], UnsupportedFeature::DocumentSharedFormEmail);
}

#[test]
fn CheckSharedFormAcrobatAtTopLevel() {
    let data = concat!(
        "<?xml charset=\"utf-8\"?>\n",
        "<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\">\n",
        "<adhocwf:workflowType>1</adhocwf:workflowType>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node>",
    );
    let stream = retain_object(CpdfStream::from_bytes(data.as_bytes()));
    let metadata = CpdfMetadata::new(stream);
    let results = metadata.check_for_shared_form();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], UnsupportedFeature::DocumentSharedFormAcrobat);
}

#[test]
fn CheckSharedFormFilesystemAtTopLevel() {
    let data = concat!(
        "<?xml charset=\"utf-8\"?>\n",
        "<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\">\n",
        "<adhocwf:workflowType>2</adhocwf:workflowType>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node>",
    );
    let stream = retain_object(CpdfStream::from_bytes(data.as_bytes()));
    let metadata = CpdfMetadata::new(stream);
    let results = metadata.check_for_shared_form();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], UnsupportedFeature::DocumentSharedFormFilesystem);
}

#[test]
fn CheckSharedFormWithoutWorkflow() {
    let data = concat!(
        "<?xml charset=\"utf-8\"?>\n",
        "<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\">\n",
        "<adhocwf:state>2</adhocwf:state>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node>",
    );
    let stream = retain_object(CpdfStream::from_bytes(data.as_bytes()));
    let metadata = CpdfMetadata::new(stream);
    let results = metadata.check_for_shared_form();
    assert_eq!(results.len(), 0);
}

#[test]
fn CheckSharedFormAsChild() {
    let data = concat!(
        "<?xml charset=\"utf-8\"?>\n",
        "<grandparent><parent>\n",
        "<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\">\n",
        "<adhocwf:workflowType>0</adhocwf:workflowType>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node>",
        "</parent></grandparent>",
    );
    let stream = retain_object(CpdfStream::from_bytes(data.as_bytes()));
    let metadata = CpdfMetadata::new(stream);
    let results = metadata.check_for_shared_form();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], UnsupportedFeature::DocumentSharedFormEmail);
}

#[test]
fn CheckSharedFormAsNoAdhoc() {
    let data = concat!("<?xml charset=\"utf-8\"?>\n", "<node></node>",);
    let stream = retain_object(CpdfStream::from_bytes(data.as_bytes()));
    let metadata = CpdfMetadata::new(stream);
    let results = metadata.check_for_shared_form();
    assert_eq!(results.len(), 0);
}

#[test]
fn CheckSharedFormExceedMaxDepth() {
    let data = concat!(
        "<?xml charset=\"utf-8\"?>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<node><node><node><node><node><node><node><node><node><node>\n",
        "<parent>\n",
        "<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\">\n",
        "<adhocwf:workflowType>0</adhocwf:workflowType>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node>",
        "</parent>",
    );
    let stream = retain_object(CpdfStream::from_bytes(data.as_bytes()));
    let metadata = CpdfMetadata::new(stream);
    let results = metadata.check_for_shared_form();
    assert_eq!(results.len(), 0);
}

#[test]
fn CheckSharedFormWrongNamespace() {
    let data = concat!(
        "<?xml charset=\"utf-8\"?>\n",
        "<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/2.0/\">\n",
        "<adhocwf:workflowType>1</adhocwf:workflowType>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node>",
    );
    let stream = retain_object(CpdfStream::from_bytes(data.as_bytes()));
    let metadata = CpdfMetadata::new(stream);
    let results = metadata.check_for_shared_form();
    assert_eq!(results.len(), 0);
}

#[test]
fn CheckSharedFormMultipleErrors() {
    let data = concat!(
        "<?xml charset=\"utf-8\"?>\n",
        "<grandparent>",
        "<parent>\n",
        "<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\">\n",
        "<adhocwf:workflowType>0</adhocwf:workflowType>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node>",
        "</parent>",
        "<node2 ",
        "xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\">\n",
        "<adhocwf:workflowType>2</adhocwf:workflowType>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node2>",
        "<node3 ",
        "xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\">\n",
        "<adhocwf:workflowType>1</adhocwf:workflowType>\n",
        "<adhocwf:version>1.1</adhocwf:version>\n",
        "</node3>",
        "</grandparent>",
    );
    let stream = retain_object(CpdfStream::from_bytes(data.as_bytes()));
    let metadata = CpdfMetadata::new(stream);
    let results = metadata.check_for_shared_form();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0], UnsupportedFeature::DocumentSharedFormEmail);
    assert_eq!(results[1], UnsupportedFeature::DocumentSharedFormFilesystem);
    assert_eq!(results[2], UnsupportedFeature::DocumentSharedFormAcrobat);
}
