// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Data, XDP and changed-form export from `cxfa_dataexporter.{h,cpp}`.
use super::{
    cxfa_node::CxfaNode,
    xfa_utils::{deal_with_data_group_node, regenerate_form_file},
};
use crate::{
    fxcrt::{
        cfx_memorystream::CfxMemoryStream,
        fx_stream::WriteStream,
        xml::cfx_xmlnode::{XmlNodeRef, XmlNodeType},
    },
    xfa::fxfa::fxfa_basic::XfaPacketType,
};
fn wide(s: &str) -> Vec<u32> {
    s.bytes().map(u32::from).collect()
}

/// XML Save is void in the source. Retain the first failed write so the public
/// exporter cannot report success for a truncated package.
struct CheckedWriter<'a> {
    stream: &'a mut dyn WriteStream,
    ok: bool,
}
impl WriteStream for CheckedWriter<'_> {
    fn write_block(&mut self, data: &[u8]) -> bool {
        self.ok = self.ok && self.stream.write_block(data);
        self.ok
    }
}
fn save_xml_node(node: &XmlNodeRef, stream: &mut dyn WriteStream) -> bool {
    let mut checked = CheckedWriter { stream, ok: true };
    node.save(&mut checked);
    checked.ok
}
struct NamespaceRestore {
    node: XmlNodeRef,
    previous: Option<Vec<u32>>,
}
impl Drop for NamespaceRestore {
    fn drop(&mut self) {
        if !self.node.is_alive() {
            return;
        }
        let name = wide("xmlns:xfa");
        if let Some(value) = self.previous.as_ref() {
            self.node.set_attribute(&name, value);
        } else {
            self.node.remove_attribute(&name);
        }
    }
}
#[derive(Default)]
pub struct CxfaDataExporter;
impl CxfaDataExporter {
    pub fn new() -> Self {
        Self
    }
    pub fn export_to_vec(&self, node: &CxfaNode) -> Option<Vec<u8>> {
        let mut output = CfxMemoryStream::new();
        self.export(&mut output, node)
            .then(|| output.get_span().to_vec())
    }
    pub fn export(&self, stream: &mut dyn WriteStream, node: &CxfaNode) -> bool {
        let _store = node.retain_store();
        if !node.is_alive() {
            return false;
        }
        if node.is_model_node() {
            return match node.get_packet_type() {
                XfaPacketType::Xdp => {
                    if !stream.write_string(b"<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">") {
                        return false;
                    }
                    let mut child = node.first_child();
                    while let Some(current) = child {
                        if !self.export(stream, &current) {
                            return false;
                        }
                        child = current.next_sibling();
                    }
                    stream.write_string(b"</xdp:xdp>\n")
                }
                XfaPacketType::Datasets => {
                    let Some(xml) = node
                        .get_xml_mapping_node()
                        .filter(|xml| xml.node_type() == XmlNodeType::Element)
                    else {
                        return false;
                    };
                    deal_with_data_group_node(node.first_child().as_ref())
                        && save_xml_node(&xml, stream)
                }
                XfaPacketType::Form => regenerate_form_file(node, stream, false),
                _ => {
                    let Some(xml) = node
                        .get_xml_mapping_node()
                        .filter(|xml| xml.node_type() == XmlNodeType::Element)
                    else {
                        return false;
                    };
                    save_xml_node(&xml, stream)
                }
            };
        }
        let Some(parent) = node.parent() else {
            return false;
        };
        let mut export = node.clone();
        let mut child = parent.first_child();
        while let Some(current) = child {
            if current != *node {
                export = parent;
                break;
            }
            child = current.next_sibling();
        }
        let Some(xml) = export
            .get_xml_mapping_node()
            .filter(|xml| xml.node_type() == XmlNodeType::Element)
        else {
            return false;
        };
        if !deal_with_data_group_node(Some(&export)) {
            return false;
        }
        let attribute = wide("xmlns:xfa");
        let previous = xml
            .has_attribute(&attribute)
            .then(|| xml.get_attribute(&attribute).to_vec());
        // Restore a preexisting namespace too, including on a failed write.
        let _restore = NamespaceRestore {
            node: xml.clone(),
            previous,
        };
        xml.set_attribute(&attribute, &wide("http://www.xfa.org/schema/xfa-data/1.0/"));
        save_xml_node(&xml, stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream,
            xml::{cfx_xmldocument::CfxXmlDocument, cfx_xmlparser::CfxXmlParser},
        },
        xfa::fxfa::{
            fxfa_basic::{XfaAttribute, XfaElement},
            parser::{
                cxfa_document::CxfaDocument,
                cxfa_document_builder::CxfaDocumentBuilder,
                xfa_utils::{field_is_multi_list_box, get_locale_value},
            },
        },
    };
    use std::{cell::RefCell, rc::Rc};
    fn text(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }
    fn xml(bytes: &[u8]) -> CfxXmlDocument {
        let mut bytes = bytes.to_vec();
        bytes.push(0);
        let stream = Rc::new(CfxReadOnlySpanStream::new(&bytes));
        let document = CfxXmlParser::new(stream).parse().unwrap();
        document
    }
    fn build(bytes: &[u8], packet: XfaPacketType) -> (Rc<CxfaDocument>, CxfaNode) {
        let xml = Rc::new(RefCell::new(xml(bytes)));
        let document = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(document.clone());
        assert!(builder.build_document(xml, packet));
        let root = builder.get_root_node().unwrap();
        assert!(document.set_root(Some(root.clone())));
        (document, root)
    }
    fn named(parent: &CxfaNode, name: &str) -> CxfaNode {
        parent.get_first_child_by_name(&text(name)).unwrap()
    }
    fn xml_root(document: &CfxXmlDocument) -> XmlNodeRef {
        let mut child = document.root().first_child();
        while let Some(node) = child {
            if node.node_type() == XmlNodeType::Element {
                return node;
            }
            child = node.next_sibling();
        }
        panic!("missing root element")
    }
    #[test]
    fn dataset_edits_empty_group_markers_and_subtree_namespaces_roundtrip() {
        let (_document,datasets)=build(br#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><record><value>original</value><empty xfa:dataNode="dataGroup"/></record></xfa:data></xfa:datasets>"#,XfaPacketType::Datasets);
        let record = named(&named(&datasets, "data"), "record");
        let value = named(&record, "value");
        let edited = text("  edited & <value> 😀");
        assert!(value.set_content(&edited, &edited, false));
        let record_xml = record.get_xml_mapping_node().unwrap();
        record_xml.set_attribute(&wide("xmlns:xfa"), &text("urn:original"));
        let exporter = CxfaDataExporter::new();
        let subset = exporter.export_to_vec(&value).unwrap();
        assert_eq!(
            record_xml.get_attribute(&wide("xmlns:xfa")).as_ref(),
            text("urn:original")
        );
        let parsed = xml(&subset);
        let parsed_record = xml_root(&parsed);
        assert_eq!(parsed_record.local_tag_name().as_ref(), text("record"));
        assert_eq!(
            parsed_record
                .first_child_named(&text("value"))
                .unwrap()
                .text_data(),
            edited
        );
        assert_eq!(
            parsed_record
                .first_child_named(&text("empty"))
                .unwrap()
                .get_attribute(&wide("xfa:dataNode"))
                .as_ref(),
            wide("dataGroup")
        );
        record_xml.remove_attribute(&wide("xmlns:xfa"));
        let saved = exporter.export_to_vec(&datasets).unwrap();
        let (_reloaded, reloaded) = build(&saved, XfaPacketType::Datasets);
        assert_eq!(
            named(&named(&named(&reloaded, "data"), "record"), "value").get_content(),
            edited
        );
    }
    #[test]
    fn changed_form_content_escaping_password_binding_gate_and_locale_value() {
        let (document,form)=build(br#"<form xmlns="http://www.xfa.org/schema/xfa-form/2.8/"><subform name="record"><field name="visible"><value><text>initial</text></value></field><field name="secret"><ui><passwordEdit/></ui><value><text>secret</text></value></field></subform></form>"#,XfaPacketType::Form);
        let record = named(&form, "record");
        let visible = named(&record, "visible");
        let secret = named(&record, "secret");
        let data = document
            .create_node(XfaPacketType::Datasets, XfaElement::DataValue)
            .unwrap();
        visible.set_binding_node(Some(&data));
        secret.set_binding_node(Some(&data));
        let edited = text("  first & <second> 'third' \"fourth\" 😀");
        assert!(visible.set_content(&edited, &edited, false));
        let relevant = text("a&<>'\"");
        visible.set_cdata(XfaAttribute::Relevant, &relevant);
        document.configure_locales(&text("en_US"), Rc::new(|| 60));
        let locale_value = get_locale_value(&visible);
        assert_eq!(locale_value.get_value(), edited);
        assert!(locale_value.is_valid());
        let saved = CxfaDataExporter::new().export_to_vec(&form).unwrap();
        let (_reloaded, form) = build(&saved, XfaPacketType::Form);
        let record = named(&form, "record");
        let visible = named(&record, "visible");
        assert_eq!(visible.get_content(), edited);
        assert_eq!(visible.get_cdata(XfaAttribute::Relevant), relevant);
        assert!(named(&record, "secret").get_content().is_empty());
    }
    #[test]
    fn full_subtree_rich_text_and_multiselect_xml_content() {
        let (_document,form)=build(br#"<form xmlns="http://www.xfa.org/schema/xfa-form/2.8/"><subform name="record"><field name="rich"><value><exData contentType="text/html"><body xmlns="http://www.w3.org/1999/xhtml"><p>hello <b>world</b> tail</p></body></exData></value></field><field name="choices"><ui><choiceList open="multiSelect"/></ui><value><exData contentType="text/xml"><choices><value>red</value></choices></exData></value></field></subform></form>"#,XfaPacketType::Form);
        let record = named(&form, "record");
        let rich = named(&record, "rich");
        let mut output = CfxMemoryStream::new();
        assert!(regenerate_form_file(&rich, &mut output, true));
        let parsed = xml(output.get_span());
        let root = xml_root(&parsed);
        let body = root
            .first_child_named(&wide("value"))
            .unwrap()
            .first_child_named(&wide("exData"))
            .unwrap()
            .first_child_named(&wide("body"))
            .unwrap();
        assert_eq!(
            body.first_child_named(&wide("p")).unwrap().text_data(),
            text("hello  tail")
        );
        assert_eq!(
            body.first_child_named(&wide("p"))
                .unwrap()
                .first_child_named(&wide("b"))
                .unwrap()
                .text_data(),
            text("world")
        );
        let choices = named(&record, "choices");
        assert!(field_is_multi_list_box(Some(&choices)));
        let raw = choices
            .get_child(0, XfaElement::Value, false)
            .unwrap()
            .first_child()
            .unwrap()
            .first_child()
            .unwrap();
        raw.set_cdata(XfaAttribute::Value, &text("red\nblue & green"));
        let mut output = CfxMemoryStream::new();
        assert!(regenerate_form_file(&choices, &mut output, true));
        let parsed = xml(output.get_span());
        let root = xml_root(&parsed);
        let values = root
            .first_child_named(&wide("value"))
            .unwrap()
            .first_child_named(&wide("exData"))
            .unwrap()
            .first_child_named(&wide("choices"))
            .unwrap();
        let first = values.first_child_named(&wide("value")).unwrap();
        assert_eq!(first.text_data(), text("red"));
        let second = values.nth_child_named(&wide("value"), 1).unwrap();
        assert_eq!(second.text_data(), text("blue & green"));
        assert!(!field_is_multi_list_box(None));
    }
    struct FailWriter;
    impl WriteStream for FailWriter {
        fn write_block(&mut self, _: &[u8]) -> bool {
            false
        }
    }
    #[test]
    fn failed_output_and_missing_mapping_are_reported_and_namespace_restored() {
        let (document,datasets)=build(br#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><record><value>v</value></record></xfa:data></xfa:datasets>"#,XfaPacketType::Datasets);
        let record = named(&named(&datasets, "data"), "record");
        let value = named(&record, "value");
        let mapping = value.get_xml_mapping_node().unwrap();
        mapping.set_attribute(&wide("xmlns:xfa"), &text("urn:previous"));
        let exporter = CxfaDataExporter::new();
        assert!(!exporter.export(&mut FailWriter, &value));
        assert_eq!(
            mapping.get_attribute(&wide("xmlns:xfa")).as_ref(),
            text("urn:previous")
        );
        let missing = document
            .create_node(XfaPacketType::Datasets, XfaElement::DataModel)
            .unwrap();
        assert!(exporter.export_to_vec(&missing).is_none());
        assert!(!regenerate_form_file(&missing, &mut FailWriter, false));
    }
}
