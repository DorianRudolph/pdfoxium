// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Connected contracts from CXFA_Document::DoProtoMerge, CXFA_Node cloning and
//! binding, XFA_NodeMerge_CloneOrMergeContainer and CXFA_DataExporter::Export.
//! These exercise the parser model without claiming a PDF/XFA embedder test.
use super::{
    cxfa_dataexporter::CxfaDataExporter,
    cxfa_document::{CxfaDocument, XfaDocumentNotify},
    cxfa_document_builder::CxfaDocumentBuilder,
    cxfa_node::{hash_name, CxfaNode},
    xfa_document_datamerger_imp::node_merge_clone_or_merge_container,
};
use crate::{
    fxcrt::{cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser},
    xfa::fxfa::fxfa_basic::{XfaAttribute, XfaElement, XfaPacketType, XfaUnit},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

fn build(input: &[u8], packet: XfaPacketType) -> (Rc<CxfaDocument>, CxfaNode) {
    let mut bytes = input.to_vec();
    bytes.push(0);
    let stream = Rc::new(CfxReadOnlySpanStream::new(&bytes));
    let xml = Rc::new(RefCell::new(CfxXmlParser::new(stream).parse().unwrap()));
    let document = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    assert!(builder.build_document(xml, packet));
    let root = builder.get_root_node().unwrap();
    assert!(document.set_root(Some(root.clone())));
    // Builder and the caller's XML owner go away. The model retains its DOM.
    (document, root)
}

fn named(parent: &CxfaNode, name: &str) -> CxfaNode {
    parent.get_first_child_by_name(&wide(name)).unwrap()
}

struct ObserveMerge {
    additions: Cell<usize>,
    ready: Cell<usize>,
}
impl ObserveMerge {
    fn read_model(&self, node: &CxfaNode) {
        let _ = node.get_cdata(XfaAttribute::Name);
        let document = node.document().unwrap();
        let root = document.get_root().unwrap();
        assert!(document
            .get_node_by_id(Some(&root), &wide("prototype"))
            .is_some());
    }
}
impl XfaDocumentNotify for ObserveMerge {
    fn on_child_added(&self, parent: &CxfaNode, child: &CxfaNode) {
        assert_eq!(child.parent(), Some(parent.clone()));
        self.read_model(parent);
        self.read_model(child);
        self.additions.set(self.additions.get() + 1);
    }
    fn on_child_removed(&self, parent: &CxfaNode, child: &CxfaNode) {
        self.read_model(parent);
        self.read_model(child);
    }
    fn on_attribute_changed(&self, node: &CxfaNode, _attribute: XfaAttribute) {
        self.read_model(node);
    }
    fn on_node_ready(&self, node: &CxfaNode) {
        // Native OnNodeReady runs before the initialized flag is installed.
        assert!(!node.is_initialized());
        self.read_model(node);
        self.ready.set(self.ready.get() + 1);
    }
}

#[test]
fn parsed_prototypes_form_bindings_and_export_roundtrip() {
    let input = br##"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <template xmlns="http://www.xfa.org/schema/xfa-template/2.8/">
        <subform name="record">
          <field name="prototype" id="prototype" w="42pt">
            <ui><textEdit/></ui><value><text>prototype value</text></value>
          </field>
          <field name="amount" use="#prototype"/>
          <field name="override" usehref=".#prototype" w="19pt">
            <value><text>local value</text></value>
          </field>
          <field name="bySom" use="prototype"/>
          <field name="external" usehref="other.xdp#prototype"/>
        </subform>
      </template>
      <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
        <xfa:data><record><amount>original data</amount></record></xfa:data>
      </xfa:datasets>
    </xdp:xdp>"##;
    let (document, root) = build(input, XfaPacketType::Xdp);
    let template = root.get_first_child_by_class(XfaElement::Template).unwrap();
    let template_record = named(&template, "record");
    let prototype = named(&template_record, "prototype");
    let amount = named(&template_record, "amount");
    let local = named(&template_record, "override");
    let local_value = local.get_first_child_by_class(XfaElement::Value).unwrap();
    let mapping = template.get_xml_mapping_node().unwrap();
    assert!(mapping.is_alive());

    let observer = Rc::new(ObserveMerge {
        additions: Cell::new(0),
        ready: Cell::new(0),
    });
    document.set_notify(Some(observer.clone()));
    document.do_proto_merge().unwrap();
    document.set_notify(None);
    assert!(observer.additions.get() > 0);
    assert!(observer.ready.get() > 0);
    for node in [&amount, &local, &named(&template_record, "bySom")] {
        assert_eq!(node.get_template_node_if_exists(), Some(prototype.clone()));
    }
    assert_eq!(
        amount.get_measure_in_unit(XfaAttribute::W, XfaUnit::Pt),
        42.0
    );
    assert_eq!(amount.get_content(), wide("prototype value"));
    assert_eq!(
        local.get_measure_in_unit(XfaAttribute::W, XfaUnit::Pt),
        19.0
    );
    assert_eq!(local.get_content(), wide("local value"));
    assert_eq!(
        local.get_first_child_by_class(XfaElement::Value),
        Some(local_value.clone())
    );
    assert_eq!(
        named(&template_record, "bySom").get_content(),
        wide("prototype value")
    );
    assert!(named(&template_record, "external")
        .get_template_node_if_exists()
        .is_none());
    assert_eq!(amount.count_children(XfaElement::Value, false), 1);
    assert_eq!(
        local.get_first_child_by_class(XfaElement::Value),
        Some(local_value)
    );

    let form = document
        .create_node(XfaPacketType::Form, XfaElement::Form)
        .unwrap();
    form.set_cdata(XfaAttribute::Name, &wide("form"));
    assert!(root.insert_child_and_notify(-1, &form));
    let form_record = template_record.clone_template_to_form(true).unwrap();
    assert!(form.insert_child_and_notify(-1, &form_record));
    let form_amount = named(&form_record, "amount");
    let form_local = named(&form_record, "override");
    assert_ne!(form_amount, amount);
    assert_eq!(
        form_amount.get_template_node_if_exists(),
        Some(amount.clone())
    );
    assert_eq!(
        form_amount.get_xml_mapping_node(),
        amount.get_xml_mapping_node()
    );
    assert_eq!(form_amount.get_content(), wide("prototype value"));
    assert_eq!(form_local.get_content(), wide("local value"));
    assert_eq!(form_amount.get_packet_type(), XfaPacketType::Form);

    // Reusing an unused form instance must keep its identity and template link.
    form_amount.set_node_and_descendants_unused();
    let reused =
        node_merge_clone_or_merge_container(&document, &form_record, &amount, true, None).unwrap();
    assert_eq!(reused, form_amount);
    assert!(!reused.is_unused_node());
    assert_eq!(reused.get_template_node_if_exists(), Some(amount));

    let datasets = root
        .get_first_child_by_class(XfaElement::DataModel)
        .unwrap();
    let data_record = named(&named(&datasets, "data"), "record");
    let data_amount = named(&data_record, "amount");
    let data_mapping = data_amount.get_xml_mapping_node().unwrap();
    for field in [&form_amount, &form_local] {
        field.set_binding_node(Some(&data_amount));
        data_amount.add_bind_item(field);
        assert_eq!(field.get_bind_data(), Some(data_amount.clone()));
    }
    assert!(data_amount.binds_form_items());
    assert_eq!(
        data_amount.get_bind_items_copy(),
        [form_amount.clone(), form_local.clone()]
    );
    assert!(document
        .get_xfa_node(hash_name(&wide("datasets")))
        .is_some());
    let edited = wide("edited & <value>");
    assert!(form_amount.set_content(&edited, &edited, false));
    // Binding synchronization is a later document service; this batch connects
    // reciprocal identity and writes each model through its existing API.
    assert_eq!(data_amount.get_content(), wide("original data"));
    assert!(data_amount.set_content(&edited, &edited, false));
    assert_eq!(form_local.get_content(), wide("local value"));
    assert_eq!(prototype.get_content(), wide("prototype value"));

    let exporter = CxfaDataExporter::new();
    let form_bytes = exporter.export_to_vec(&form).unwrap();
    let (_reloaded_document, reloaded_form) = build(&form_bytes, XfaPacketType::Form);
    let reloaded_record = named(&reloaded_form, "record");
    assert_eq!(named(&reloaded_record, "amount").get_content(), edited);
    assert_eq!(
        named(&reloaded_record, "override").get_content(),
        wide("local value")
    );
    let data_bytes = exporter.export_to_vec(&datasets).unwrap();
    let (_reloaded_document, reloaded_datasets) = build(&data_bytes, XfaPacketType::Datasets);
    let reloaded_data_record = named(&named(&reloaded_datasets, "data"), "record");
    assert_eq!(named(&reloaded_data_record, "amount").get_content(), edited);
    assert_eq!(data_amount.get_xml_mapping_node(), Some(data_mapping));
    assert_eq!(template.get_xml_mapping_node(), Some(mapping));
    assert_eq!(
        data_amount.get_bind_items_copy(),
        [form_amount.clone(), form_local.clone()]
    );
    assert!(data_amount.remove_bind_item(&form_amount));
    form_amount.set_binding_node(None);
    assert!(form_amount.get_bind_data().is_none());
    assert_eq!(form_local.get_bind_data(), Some(data_amount));
    let weak_document = Rc::downgrade(&document);
    drop(document);
    assert!(weak_document.upgrade().is_none());
    assert!(!root.is_alive());
    assert!(!form_local.is_alive());
}
