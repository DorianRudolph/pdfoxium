// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct translation of `core/fpdfdoc/cpdf_metadata.{h,cpp}`.

use std::rc::Rc;

use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfRetainedStreamAcc;
use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
use crate::fxcrt::fx_string::string_to_int_wide;
use crate::fxcrt::xml::cfx_xmlnode::{XmlNodeRef, XmlNodeType};
use crate::fxcrt::xml::cfx_xmlparser::CfxXmlParser;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum UnsupportedFeature {
    DocumentXfaForm = 1,
    DocumentPortableCollection = 2,
    DocumentAttachment = 3,
    DocumentSecurity = 4,
    DocumentSharedReview = 5,
    DocumentSharedFormAcrobat = 6,
    DocumentSharedFormFilesystem = 7,
    DocumentSharedFormEmail = 8,
    Annotation3d = 11,
    AnnotationMovie = 12,
    AnnotationSound = 13,
    AnnotationScreenMedia = 14,
    AnnotationScreenRichMedia = 15,
    AnnotationAttachment = 16,
    AnnotationSignature = 17,
}

const MAX_METADATA_DEPTH: i32 = 128;

fn wide_ascii(text: &[u8]) -> Vec<u32> {
    text.iter().map(|&byte| u32::from(byte)).collect()
}

fn equals_ascii(text: &[u32], ascii: &[u8]) -> bool {
    text.iter()
        .copied()
        .eq(ascii.iter().map(|&byte| u32::from(byte)))
}

fn check_for_shared_form_internal(
    depth: i32,
    element: &XmlNodeRef,
    unsupported: &mut Vec<UnsupportedFeature>,
) -> bool {
    if depth >= MAX_METADATA_DEPTH {
        return false;
    }
    let namespace = element.get_attribute(&wide_ascii(b"xmlns:adhocwf"));
    if equals_ascii(&namespace, b"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/") {
        let mut child = element.first_child();
        while let Some(child_node) = child {
            child = child_node.next_sibling();
            if child_node.node_type() != XmlNodeType::Element {
                continue;
            }
            if !equals_ascii(&child_node.name(), b"adhocwf:workflowType") {
                continue;
            }
            match string_to_int_wide(&child_node.text_data()) {
                0 => unsupported.push(UnsupportedFeature::DocumentSharedFormEmail),
                1 => unsupported.push(UnsupportedFeature::DocumentSharedFormAcrobat),
                2 => unsupported.push(UnsupportedFeature::DocumentSharedFormFilesystem),
                _ => (),
            }
            // We only care about the first one we find.
            break;
        }
    }
    let mut child = element.first_child();
    while let Some(child_node) = child {
        child = child_node.next_sibling();
        if child_node.node_type() == XmlNodeType::Element
            && !check_for_shared_form_internal(depth + 1, &child_node, unsupported)
        {
            return false;
        }
    }
    true
}

pub struct CpdfMetadata {
    stream: ObjectRef,
}

impl CpdfMetadata {
    pub fn new(stream: ObjectRef) -> Self {
        debug_assert!(stream.borrow().as_any().is::<CpdfStream>());
        Self { stream }
    }

    pub fn check_for_shared_form(&self) -> Vec<UnsupportedFeature> {
        let acc = CpdfRetainedStreamAcc::load_filtered(Rc::clone(&self.stream))
            .expect("metadata requires a PDF stream");
        let span = acc.get_span();
        let stream = Rc::new(CfxReadOnlySpanStream::new(&span));
        let Some(doc) = CfxXmlParser::new(stream).parse() else {
            return Vec::new();
        };
        let mut unsupported = Vec::new();
        // Upstream ignores the return: a depth failure keeps earlier results.
        check_for_shared_form_internal(0, &doc.root(), &mut unsupported);
        unsupported
    }
}

#[cfg(test)]
#[path = "cpdf_metadata_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_name::CpdfName;
    use crate::fpdfapi::parser::cpdf_object::retain_object;

    fn check(data: &[u8]) -> Vec<UnsupportedFeature> {
        CpdfMetadata::new(retain_object(CpdfStream::from_bytes(data))).check_for_shared_form()
    }

    #[test]
    fn filtered_metadata_and_cdata_use_the_real_stream_and_xml_pipeline() {
        let xml = b"<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\"><adhocwf:workflowType><![CDATA[1]]></adhocwf:workflowType></node>";
        let encoded = crate::fxcodec::flate::encode(xml);
        let stream = CpdfStream::from_bytes(&encoded);
        stream
            .get_dict()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Filter", CpdfName::new(None, b"FlateDecode"));
        let stream = retain_object(stream);
        let owner = Rc::downgrade(&stream);
        let metadata = CpdfMetadata::new(stream);
        assert_eq!(
            metadata.check_for_shared_form(),
            [UnsupportedFeature::DocumentSharedFormAcrobat]
        );
        assert!(owner.upgrade().is_some());
        drop(metadata);
        assert!(owner.upgrade().is_none());
    }

    #[test]
    fn only_the_first_workflow_counts_and_the_namespace_attribute_is_local() {
        assert!(check(b"<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\"><adhocwf:workflowType>3</adhocwf:workflowType><adhocwf:workflowType>0</adhocwf:workflowType></node>").is_empty());
        assert!(check(b"<parent xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\"><node><adhocwf:workflowType>0</adhocwf:workflowType></node></parent>").is_empty());
        assert_eq!(check(b"<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\"><adhocwf:workflowType /></node>"), [UnsupportedFeature::DocumentSharedFormEmail]);
        assert!(check(b"").is_empty());
        assert!(check(b"<node></other>").is_empty());
    }

    #[test]
    fn a_depth_failure_preserves_earlier_findings_and_stops_later_siblings() {
        let first = "<first xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\"><adhocwf:workflowType>0</adhocwf:workflowType></first>";
        let last = "<last xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\"><adhocwf:workflowType>2</adhocwf:workflowType></last>";
        let xml = format!(
            "<root>{first}{}{last}</root>",
            "<node>".repeat(130) + &"</node>".repeat(130)
        );
        assert_eq!(
            check(xml.as_bytes()),
            [UnsupportedFeature::DocumentSharedFormEmail]
        );
    }
}
