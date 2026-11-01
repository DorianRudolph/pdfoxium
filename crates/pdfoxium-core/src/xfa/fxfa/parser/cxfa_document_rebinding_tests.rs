// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Container-copy entry points and binding ownership across real model merges.
use super::CxfaDocument;
use crate::{
    fxcrt::{cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser},
    fxjs::xfa::cfxjse_resolveprocessor::CfxJseResolveProcessor,
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute as A, XfaElement as E, XfaHashCode, XfaPacketType as P},
        parser::{cxfa_document_builder::CxfaDocumentBuilder, cxfa_node::CxfaNode},
    },
};
use std::{cell::RefCell, rc::Rc};
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn fixture(template: &str, data: &str) -> Rc<CxfaDocument> {
    let input=format!("<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'><template xmlns='http://www.xfa.org/schema/xfa-template/3.3/'><subform name='record'>{template}</subform></template><xfa:datasets xmlns:xfa='http://www.xfa.org/schema/xfa-data/1.0/'><xfa:data><record>{data}</record></xfa:data></xfa:datasets></xdp:xdp>");
    let mut bytes = input.into_bytes();
    bytes.push(0);
    let xml = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
        .parse()
        .unwrap();
    let document = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    assert!(builder.build_document(Rc::new(RefCell::new(xml)), P::Xdp));
    document.set_root(builder.get_root_node());
    document.do_data_merge().unwrap();
    document
}
fn named(parent: &CxfaNode, name: &str) -> CxfaNode {
    parent.get_first_child_by_name(&wide(name)).unwrap()
}
fn form(document: &CxfaDocument) -> CxfaNode {
    document
        .get_xfa_node(XfaHashCode::Form as u32)
        .unwrap()
        .get_first_child_by_class(E::Subform)
        .unwrap()
}
fn record(document: &CxfaDocument) -> CxfaNode {
    document.get_xfa_node(XfaHashCode::Record as u32).unwrap()
}
fn template(document: &CxfaDocument) -> CxfaNode {
    document
        .get_xfa_node(XfaHashCode::Template as u32)
        .unwrap()
        .get_first_child_by_class(E::Subform)
        .unwrap()
}
fn parent(document: &Rc<CxfaDocument>) -> CxfaNode {
    let parent = document.create_node(P::Form, E::Subform).unwrap();
    parent.set_cdata(A::Name, &wide("copies"));
    form(document).insert_child_and_notify(-1, &parent);
    parent
}
const FIELD:&str="<field name='value'><bind match='dataRef' ref='$record.value'/><ui><textEdit/></ui><value><text>Default</text></value></field>";
#[test]
fn direct_copy_wrapper_binds_the_requested_record_node_and_keeps_template_identity() {
    let document = fixture(FIELD, "<value>Imported</value>");
    let source = named(&template(&document), "value");
    let parent = parent(&document);
    let data = record(&document);
    let copy = document
        .data_merge_copy_container(&source, &parent, Some(&data), true, true, false)
        .unwrap()
        .unwrap();
    assert_eq!(copy.parent(), Some(parent));
    assert_eq!(copy.get_template_node_if_exists(), Some(source));
    assert_eq!(copy.get_content(), wide("Imported"));
    assert_eq!(copy.get_bind_data(), Some(named(&data, "value")));
    assert_ne!(copy, named(&form(&document), "value"));
}
#[test]
fn unbound_copy_preserves_its_default_without_consuming_or_modifying_dataset_values() {
    let document = fixture(FIELD, "<value>Imported</value>");
    let data = named(&record(&document), "value");
    let bindings = data.get_bind_items_copy();
    let parent = parent(&document);
    let copy = document
        .data_merge_copy_container(
            &named(&template(&document), "value"),
            &parent,
            Some(&record(&document)),
            true,
            false,
            false,
        )
        .unwrap()
        .unwrap();
    assert!(copy.get_bind_data().is_none());
    assert_eq!(copy.get_content(), wide("Default"));
    assert!(copy.set_content(&wide("Local"), &wide("Local"), false));
    assert_eq!(data.get_content(), wide("Imported"));
    assert_eq!(data.get_bind_items_copy(), bindings);
    assert_eq!(
        named(&form(&document), "value").get_content(),
        wide("Imported")
    );
}
#[test]
fn binding_update_wrapper_attaches_a_previously_unbound_copy_to_its_ancestor_scope() {
    let document = fixture(FIELD, "<value>Imported</value>");
    let parent = parent(&document);
    let copy = document
        .data_merge_copy_container(
            &named(&template(&document), "value"),
            &parent,
            None,
            true,
            false,
            false,
        )
        .unwrap()
        .unwrap();
    assert!(copy.get_bind_data().is_none());
    assert_eq!(copy.get_content(), wide("Default"));
    document.data_merge_update_binding_relations(&copy).unwrap();
    let data = named(&record(&document), "value");
    assert_eq!(copy.get_bind_data(), Some(data.clone()));
    assert_eq!(copy.get_content(), wide("Imported"));
    assert!(data.get_bind_items_copy().contains(&copy));
}
#[test]
fn explicit_resolver_copy_and_binding_update_share_the_documents_real_scope() {
    let document = fixture(FIELD, "<value>Original</value>");
    let resolver = CfxJseResolveProcessor::new(document.clone());
    let parent = parent(&document);
    let source = named(&template(&document), "value");
    let copy = document
        .data_merge_copy_container_with_resolver(
            &resolver, &source, &parent, None, true, false, true,
        )
        .unwrap()
        .unwrap();
    document
        .data_merge_update_binding_relations_with_resolver(&resolver, &copy)
        .unwrap();
    let data = named(&record(&document), "value");
    assert_eq!(copy.get_bind_data(), Some(data.clone()));
    assert_eq!(copy.get_content(), wide("Original"));
    assert!(copy.set_content_with_sync(&wide("Updated"), &wide("Updated"), false, true));
    assert_eq!(data.get_content(), wide("Updated"));
}
#[test]
fn foreign_resolver_copy_and_binding_update_reject_before_changing_either_document() {
    let document = fixture(FIELD, "<value>Left</value>");
    let foreign = fixture(FIELD, "<value>Right</value>");
    let resolver = CfxJseResolveProcessor::new(foreign.clone());
    let parent = parent(&document);
    assert!(document
        .data_merge_copy_container_with_resolver(
            &resolver,
            &named(&template(&document), "value"),
            &parent,
            Some(&record(&document)),
            true,
            true,
            true
        )
        .is_err());
    assert!(parent.first_child().is_none());
    assert!(document
        .data_merge_update_binding_relations_with_resolver(
            &resolver,
            &named(&form(&document), "value")
        )
        .is_err());
    assert_eq!(
        named(&record(&document), "value").get_content(),
        wide("Left")
    );
    assert_eq!(
        named(&record(&foreign), "value").get_content(),
        wide("Right")
    );
}
#[test]
fn copied_nested_container_has_independent_children_and_no_unrequested_record_binding() {
    let document=fixture("<subform name='row'><bind match='none'/><occur min='0' initial='1' max='4'/><field name='local'><bind match='none'/><ui><textEdit/></ui><value><text>Default</text></value></field></subform>","<value>Data</value>");
    let parent = parent(&document);
    let source = named(&template(&document), "row");
    let copy = document
        .data_merge_copy_container(&source, &parent, None, true, false, false)
        .unwrap()
        .unwrap();
    let original = named(&form(&document), "row");
    let local = named(&copy, "local");
    assert_eq!(copy.get_template_node_if_exists(), Some(source));
    assert_ne!(local, named(&original, "local"));
    assert!(copy.get_bind_data().is_none());
    assert!(local.get_bind_data().is_none());
    assert!(local.set_content(&wide("Copied"), &wide("Copied"), false));
    assert_eq!(named(&original, "local").get_content(), wide("Default"));
}
#[test]
fn repeated_remerge_releases_reverse_binding_edges_of_every_retired_copy() {
    let document = fixture(FIELD, "<value>Persistent</value>");
    let data = named(&record(&document), "value");
    let mut old_forms = Vec::new();
    for generation in 0..4 {
        let old = named(&form(&document), "value");
        old_forms.push(old.clone());
        document.do_data_remerge_model().unwrap();
        let fresh = named(&form(&document), "value");
        assert_ne!(fresh, old);
        assert_eq!(fresh.get_bind_data(), Some(data.clone()));
        assert_eq!(fresh.get_content(), wide("Persistent"));
        assert_eq!(data.get_bind_items_copy(), vec![fresh]);
        for retired in &old_forms {
            assert!(retired.get_bind_data().is_none(), "generation {generation}");
        }
    }
}
#[test]
fn replacing_the_bound_record_rebinds_to_new_identity_and_isolates_retired_data() {
    let document = fixture(FIELD, "<value>Old</value>");
    let old_record = record(&document);
    let old_data = named(&old_record, "value");
    let data_parent = old_record.parent().unwrap();
    let replacement = old_record.clone_node(true).unwrap();
    let fresh_data = named(&replacement, "value");
    assert_ne!(fresh_data, old_data);
    assert!(fresh_data.set_content(&wide("New"), &wide("New"), false));
    data_parent.remove_child_and_notify(&old_record, true);
    data_parent.insert_child_and_notify(-1, &replacement);
    let retired = named(&form(&document), "value");
    document.do_data_remerge_model().unwrap();
    let current = named(&form(&document), "value");
    assert_eq!(record(&document), replacement);
    assert_eq!(current.get_bind_data(), Some(fresh_data.clone()));
    assert_eq!(current.get_content(), wide("New"));
    assert!(retired.get_bind_data().is_none());
    assert!(old_data.get_bind_items_copy().is_empty());
    assert!(old_data.set_content(&wide("Detached"), &wide("Detached"), false));
    assert_eq!(fresh_data.get_content(), wide("New"));
    assert_eq!(current.get_content(), wide("New"));
}
