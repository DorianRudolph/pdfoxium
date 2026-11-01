// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! `cxfa_node_unittest.cpp`: native tree cases and Delta regression. Native
//! invalid-parent death cases return false in Rust; null children cannot be
//! represented by these reference-taking APIs. Additional connected tests
//! cover schema defaults, local CJX storage, XML mapping and notify reentry.
use super::super::{cxfa_document::XfaDocumentNotify, node_schema::packet_mask};
use super::*;
use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
use std::cell::{Cell, RefCell};

const TEST_SCHEMA: NodeSchema = NodeSchema {
    element: XfaElement::Node,
    object_type: XfaObjectType::Node,
    valid_packets: packet_mask(XfaPacketType::Template) | packet_mask(XfaPacketType::Form),
    properties: &[],
    attributes: &[],
};
struct Fixture {
    doc: Rc<CxfaDocument>,
    _owner: TreeNodeOwner<CxfaNodeData>,
    node: CxfaNode,
}
impl Fixture {
    fn new() -> Self {
        let doc = CxfaDocument::new();
        let (owner, node) = CxfaNode::allocate_with_schema(&doc, XfaPacketType::Form, &TEST_SCHEMA);
        Self {
            doc,
            _owner: owner,
            node,
        }
    }
    fn child(&self) -> CxfaNode {
        self.doc
            .create_node(XfaPacketType::Form, XfaElement::Ui)
            .unwrap()
    }
    fn append(&self) -> CxfaNode {
        let child = self.child();
        assert!(self.node.insert_child_and_notify(-1, &child));
        child
    }
}
fn assert_detached(node: &CxfaNode) {
    assert_eq!(node.parent(), None);
    assert_eq!(node.next_sibling(), None);
    assert_eq!(node.prev_sibling(), None);
}
fn assert_order(parent: &CxfaNode, children: &[CxfaNode]) {
    assert_eq!(parent.first_child(), children.first().cloned());
    assert_eq!(parent.last_child(), children.last().cloned());
    for (index, node) in children.iter().enumerate() {
        assert_eq!(node.parent(), Some(parent.clone()));
        assert_eq!(
            node.prev_sibling(),
            index.checked_sub(1).and_then(|i| children.get(i)).cloned()
        );
        assert_eq!(node.next_sibling(), children.get(index + 1).cloned());
    }
}
#[test]
fn insert_first_child() {
    let f = Fixture::new();
    assert_eq!(f.node.first_child(), None);
    assert_eq!(f.node.last_child(), None);
    let child = f.child();
    assert!(f.node.insert_child_and_notify(-1, &child));
    assert_order(&f.node, &[child]);
}
#[test]
fn insert_child_by_negative_index() {
    let f = Fixture::new();
    let child0 = f.append();
    let child = f.child();
    assert!(f.node.insert_child_and_notify(-1, &child));
    assert_order(&f.node, &[child0, child]);
}
#[test]
fn insert_child_by_index() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.append();
    let child2 = f.append();
    let child3 = f.append();
    let child = f.child();
    assert!(f.node.insert_child_and_notify(2, &child));
    assert_order(&f.node, &[child0, child1, child, child2, child3]);
}
#[test]
fn insert_child_index_past_end() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.append();
    let child = f.child();
    assert!(f.node.insert_child_and_notify(20, &child));
    assert_order(&f.node, &[child0, child1, child]);
}
#[test]
fn insert_first_child_before_nullptr() {
    let f = Fixture::new();
    assert_eq!(f.node.first_child(), None);
    assert_eq!(f.node.last_child(), None);
    let child = f.child();
    assert!(f.node.insert_child_before_and_notify(&child, None));
    assert_order(&f.node, &[child]);
}
#[test]
fn insert_before_with_null_before() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.append();
    let child = f.child();
    assert!(f.node.insert_child_before_and_notify(&child, None));
    assert_order(&f.node, &[child0, child1, child]);
}
#[test]
fn insert_before_first_child() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.append();
    let child = f.child();
    assert!(f.node.insert_child_before_and_notify(&child, Some(&child0)));
    assert_order(&f.node, &[child, child0, child1]);
}
#[test]
fn insert_before() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.append();
    let child2 = f.append();
    let child3 = f.append();
    let child = f.child();
    assert!(f.node.insert_child_before_and_notify(&child, Some(&child2)));
    assert_order(&f.node, &[child0, child1, child, child2, child3]);
}
#[test]
fn remove_only_child() {
    let f = Fixture::new();
    let child0 = f.append();
    assert_order(&f.node, &[child0.clone()]);
    assert!(f.node.remove_child_and_notify(&child0, false));
    assert_order(&f.node, &[]);
    assert_detached(&child0);
}
#[test]
fn remove_first_child() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.append();
    let child2 = f.append();
    assert_order(&f.node, &[child0.clone(), child1.clone(), child2.clone()]);
    assert!(f.node.remove_child_and_notify(&child0, false));
    assert_order(&f.node, &[child1, child2]);
    assert_detached(&child0);
}
#[test]
fn remove_last_child() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.append();
    let child2 = f.append();
    assert_order(&f.node, &[child0.clone(), child1.clone(), child2.clone()]);
    assert!(f.node.remove_child_and_notify(&child2, false));
    assert_order(&f.node, &[child0, child1]);
    assert_detached(&child2);
}
#[test]
fn remove_child() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.append();
    let child2 = f.append();
    assert_order(&f.node, &[child0.clone(), child1.clone(), child2.clone()]);
    assert!(f.node.remove_child_and_notify(&child1, false));
    assert_order(&f.node, &[child0, child2]);
    assert_detached(&child1);
}
#[test]
fn insert_child_with_parent() {
    let f = Fixture::new();
    let child0 = f.child();
    let child1 = f.child();
    assert!(child0.insert_child_and_notify(-1, &child1));
    assert!(!f.node.insert_child_and_notify(0, &child1));
    assert_eq!(child1.parent(), Some(child0));
}
#[test]
fn insert_before_with_before_in_another_parent() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.child();
    assert!(child0.insert_child_and_notify(-1, &child1));
    let child = f.child();
    assert!(!f.node.insert_child_before_and_notify(&child, Some(&child1)));
    assert_detached(&child);
}
#[test]
fn insert_before_with_node_in_another_parent() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.child();
    assert!(child0.insert_child_and_notify(-1, &child1));
    assert!(!f.node.insert_child_before_and_notify(&child1, None));
    assert_eq!(child1.parent(), Some(child0));
}
#[test]
fn remove_child_another_parent() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.child();
    assert!(child0.insert_child_and_notify(-1, &child1));
    assert!(!f.node.remove_child_and_notify(&child1, false));
    assert_eq!(child1.parent(), Some(child0));
}
#[test]
fn ancestor_of() {
    let f = Fixture::new();
    let child0 = f.append();
    let child1 = f.append();
    let grandchild = f.child();
    assert!(child0.insert_child_and_notify(-1, &grandchild));
    assert!(f.node.is_ancestor_of(&child0));
    assert!(f.node.is_ancestor_of(&child1));
    assert!(!child1.is_ancestor_of(&child0));
    assert!(child0.is_ancestor_of(&grandchild));
    assert!(f.node.is_ancestor_of(&grandchild));
    assert!(!child1.is_ancestor_of(&grandchild));
    assert!(f.node.is_ancestor_of(&f.node));
    assert!(!grandchild.insert_child_and_notify(-1, &f.node));
}
#[test]
fn delta_object_is_node() {
    let f = Fixture::new();
    let delta = f
        .doc
        .create_node(XfaPacketType::Form, XfaElement::Delta)
        .unwrap();
    assert!(delta.is_node());
    assert_eq!(delta.get_object_type(), XfaObjectType::Object);
    assert!(delta.set_attribute_by_enum(XfaAttribute::Name, &wide("delta"), false));
    assert_eq!(delta.get_cdata(XfaAttribute::Name), wide("delta"));
}

#[test]
fn schema_property_defaults_and_cdata_absence_remain_distinct() {
    let doc = CxfaDocument::new();
    let field = doc
        .create_node(XfaPacketType::Template, XfaElement::Field)
        .unwrap();
    assert!(field.has_property(XfaElement::Ui));
    assert_eq!(field.property_occurrence_count(XfaElement::Ui), 1);
    assert!(field.get_or_create_property(-1, XfaElement::Ui).is_none());
    assert!(field.get_or_create_property(1, XfaElement::Ui).is_none());
    let ui = field.get_or_create_property(0, XfaElement::Ui).unwrap();
    assert!(ui.is_initialized());
    assert_eq!(
        field.get_or_create_property(0, XfaElement::Ui),
        Some(ui.clone())
    );
    assert!(ui.has_property_flag(XfaElement::TextEdit, XFA_PROPERTY_ONE_OF));
    let text_edit = ui.get_or_create_property(0, XfaElement::TextEdit).unwrap();
    assert!(ui
        .get_or_create_property(0, XfaElement::NumericEdit)
        .is_none());
    assert_eq!(
        ui.get_property(0, XfaElement::TextEdit),
        (Some(text_edit), 1)
    );
    assert!(field.has_attribute(XfaAttribute::Name));
    assert!(!field.has_local_attribute(XfaAttribute::Name));
    assert_eq!(field.try_cdata(XfaAttribute::Name, false), None);
    assert_eq!(field.try_cdata(XfaAttribute::Name, true), Some(Vec::new()));
    assert_eq!(field.get_default_integer(XfaAttribute::Name), None);
    assert!(!field.has_attribute(XfaAttribute::Unknown));
    assert_eq!(field.get_attribute(usize::MAX), XfaAttribute::Unknown);
    assert!(doc
        .create_node(XfaPacketType::Datasets, XfaElement::Field)
        .is_none());
    assert!(doc
        .create_node(XfaPacketType::Form, XfaElement::Node)
        .is_none());
    field.set_cdata(XfaAttribute::Name, &wide("field"));
    assert!(field.has_local_attribute(XfaAttribute::Name));
    assert_eq!(field.get_name_hash(), hash_name(&wide("field")));
    assert_eq!(field.get_class_name(), "field");
    assert_eq!(field.get_class_hash_code(), hash_name(&wide("field")));
}

#[test]
fn typed_storage_template_lookup_cycles_and_dataset_chain_boundary() {
    let doc = CxfaDocument::new();
    let template = doc
        .create_node(XfaPacketType::Template, XfaElement::Field)
        .unwrap();
    let form = doc
        .create_node(XfaPacketType::Form, XfaElement::Field)
        .unwrap();
    form.set_template_node(Some(&template));
    template.set_cdata(XfaAttribute::Name, &wide("inherited"));
    template.set_integer(XfaAttribute::MaxChars, 17, false);
    template.set_boolean(XfaAttribute::Relevant, true, false);
    template.set_enum(XfaAttribute::Presence, XfaAttributeValue::Hidden, false);
    template.set_measure(
        XfaAttribute::H,
        CxfaMeasurement::new(2., XfaUnit::In),
        false,
    );
    assert_eq!(form.get_cdata(XfaAttribute::Name), wide("inherited"));
    assert_eq!(form.get_integer(XfaAttribute::MaxChars), 17);
    assert!(form.get_boolean(XfaAttribute::Relevant));
    assert_eq!(
        form.get_enum(XfaAttribute::Presence),
        XfaAttributeValue::Hidden
    );
    assert_eq!(form.get_measure_in_unit(XfaAttribute::H, XfaUnit::Pt), 144.);
    assert!(!form.presence_requires_space());
    assert!(form.set_attribute_by_enum(XfaAttribute::H, &wide("25.4mm"), false));
    assert!((form.get_measure_in_unit(XfaAttribute::H, XfaUnit::Pt) - 72.).abs() < 0.0001);
    template.set_template_node(Some(&form));
    assert_eq!(form.try_cdata(XfaAttribute::Id, false), None);
    assert_eq!(form.try_integer(XfaAttribute::ColSpan, false), None);
    form.set_attribute_by_string(&wide("custom"), &wide("value"));
    assert_eq!(form.get_attribute_by_string(&wide("custom")), wide("value"));
    form.remove_attribute(&wide("custom"));
    assert_eq!(
        form.get_attribute_by_string(&wide("custom")),
        Vec::<u32>::new()
    );
    let data = doc
        .create_node(XfaPacketType::Datasets, XfaElement::DataValue)
        .unwrap();
    data.set_template_node(Some(&template));
    assert_eq!(data.try_cdata(XfaAttribute::Name, false), None);
    let different_class = doc
        .create_node(XfaPacketType::Form, XfaElement::Ui)
        .unwrap();
    different_class.set_template_node(Some(&template));
    assert_eq!(different_class.try_cdata(XfaAttribute::Name, false), None);
    assert_eq!(
        different_class.try_integer(XfaAttribute::MaxChars, false),
        None
    );
    assert_eq!(different_class.try_measure(XfaAttribute::H, false), None);
    template.set_attribute_by_string(&wide("custom"), &wide("inherited custom"));
    assert_eq!(
        different_class.get_attribute_by_string(&wide("custom")),
        wide("inherited custom")
    );
}

#[test]
fn children_properties_name_lookup_and_removed_flags_follow_the_live_tree() {
    let doc = CxfaDocument::new();
    let root = doc
        .create_node(XfaPacketType::Form, XfaElement::Subform)
        .unwrap();
    let ui = doc
        .create_node(XfaPacketType::Form, XfaElement::Ui)
        .unwrap();
    let field = doc
        .create_node(XfaPacketType::Form, XfaElement::Field)
        .unwrap();
    let next = doc
        .create_node(XfaPacketType::Form, XfaElement::Field)
        .unwrap();
    field.set_cdata(XfaAttribute::Name, &wide("entry"));
    next.set_cdata(XfaAttribute::Name, &wide("entry"));
    assert!(root.insert_child_and_notify(-1, &ui));
    assert!(root.insert_child_and_notify(-1, &field));
    assert!(root.insert_child_and_notify(-1, &next));
    assert_eq!(root.count_children(XfaElement::Unknown, false), 3);
    assert_eq!(root.count_children(XfaElement::Field, true), 2);
    assert_eq!(
        root.get_child(1, XfaElement::Field, true),
        Some(next.clone())
    );
    assert_eq!(
        root.get_first_child_by_name(&wide("entry")),
        Some(field.clone())
    );
    assert_eq!(
        field.get_next_same_name_sibling(hash_name(&wide("entry"))),
        Some(next.clone())
    );
    assert_eq!(
        field.get_next_same_class_sibling(XfaElement::Field),
        Some(next.clone())
    );
    assert_eq!(root.get_first_container_child(), Some(field.clone()));
    assert_eq!(field.get_container_parent(), Some(root.clone()));
    assert!(root.remove_child_and_notify(&field, false));
    assert!(field.has_removed_children());
    assert!(root.insert_child_and_notify(-1, &field));
    assert!(!field.has_removed_children());
    root.set_node_and_descendants_unused();
    assert!(root.is_unused_node());
    assert!(ui.is_unused_node());
    assert!(field.is_unused_node());
    assert!(next.is_unused_node());
}

#[test]
fn xml_mapping_sync_content_metadata_detach_and_namespace() {
    let doc = CxfaDocument::new();
    let xml = Rc::new(RefCell::new(CfxXmlDocument::new()));
    doc.set_xml_document(xml.clone());
    let group = doc
        .create_node(XfaPacketType::Datasets, XfaElement::DataGroup)
        .unwrap();
    group.set_cdata(XfaAttribute::Name, &wide("group"));
    let group_xml = group.create_xml_mapping_node().unwrap();
    xml.borrow().root().append_last_child(&group_xml);
    group_xml.set_attribute(&wide("xmlns:p"), &wide("urn:test"));
    let child = doc
        .create_node(XfaPacketType::Datasets, XfaElement::DataValue)
        .unwrap();
    child.set_cdata(XfaAttribute::Name, &wide("child"));
    let child_xml = child.create_xml_mapping_node().unwrap();
    child.set_cdata(XfaAttribute::Value, &wide("value"));
    assert_eq!(child_xml.text_data(), wide("value"));
    assert!(group.insert_child_and_notify(-1, &child));
    assert_eq!(child_xml.parent(), Some(group_xml.clone()));
    child.set_content(&wide("new"), &wide("serialized"), false);
    assert_eq!(child.get_content(), wide("new"));
    assert_eq!(child_xml.text_data(), wide("serialized"));
    assert!(group.remove_child_and_notify(&child, false));
    assert_eq!(child_xml.parent(), None);
    let metadata = doc
        .create_node(XfaPacketType::Datasets, XfaElement::DataValue)
        .unwrap();
    metadata.set_cdata(XfaAttribute::Name, &wide("meta"));
    metadata.set_cdata(XfaAttribute::QualifiedName, &wide("p:meta"));
    metadata.set_enum(XfaAttribute::Contains, XfaAttributeValue::MetaData, false);
    metadata.set_cdata(XfaAttribute::Value, &wide("owned"));
    assert!(group.insert_child_and_notify(-1, &metadata));
    metadata.set_xml_mapping_node(group_xml.clone());
    metadata.set_cdata(XfaAttribute::Value, &wide("changed"));
    assert_eq!(
        group_xml.get_attribute(&wide("p:meta")).as_ref(),
        wide("changed")
    );
    assert_eq!(metadata.try_namespace(), Some(wide("urn:test")));
    assert!(group.remove_child_and_notify(&metadata, false));
    assert!(!group_xml.has_attribute(&wide("p:meta")));
    let detached = metadata.get_xml_mapping_node().unwrap();
    assert_ne!(detached, group_xml);
    assert_eq!(detached.text_data(), wide("changed"));
    assert!(!metadata.is_attribute_in_xml());
    let text = doc
        .create_node(XfaPacketType::Form, XfaElement::Text)
        .unwrap();
    assert!(text.set_content(&wide("hello"), &wide("hello"), false));
    assert_eq!(
        text.first_child().unwrap().get_element_type(),
        XfaElement::Sharptext
    );
    assert_eq!(text.get_content(), wide("hello"));
}

struct ReentrantNotify {
    added: Cell<usize>,
    removed: Cell<usize>,
    attributes: Cell<usize>,
    ready: Cell<usize>,
    remove_added: Cell<bool>,
}
impl XfaDocumentNotify for ReentrantNotify {
    fn on_child_added(&self, parent: &CxfaNode, child: &CxfaNode) {
        self.added.set(self.added.get() + 1);
        assert_eq!(child.parent(), Some(parent.clone()));
        if self.remove_added.replace(false) {
            assert!(parent.remove_child_and_notify(child, true));
        }
    }
    fn on_child_removed(&self, parent: &CxfaNode, child: &CxfaNode) {
        self.removed.set(self.removed.get() + 1);
        assert_eq!(child.parent(), None);
        assert!(parent.count_children(XfaElement::Unknown, false) < usize::MAX);
    }
    fn on_attribute_changed(&self, node: &CxfaNode, attribute: XfaAttribute) {
        self.attributes.set(self.attributes.get() + 1);
        assert!(node.has_local_attribute(attribute));
        if attribute == XfaAttribute::Name && node.get_cdata(attribute) == wide("outer") {
            node.set_cdata_and_notify(attribute, &wide("inner"), true);
        }
    }
    fn on_node_ready(&self, node: &CxfaNode) {
        self.ready.set(self.ready.get() + 1);
        assert!(!node.is_initialized());
        node.set_initialized_flag_and_notify();
        node.set_cdata(XfaAttribute::Name, &wide("ready"));
    }
}
#[test]
fn notify_callbacks_reenter_after_tree_and_attribute_borrows_are_released() {
    let notify = Rc::new(ReentrantNotify {
        added: Cell::new(0),
        removed: Cell::new(0),
        attributes: Cell::new(0),
        ready: Cell::new(0),
        remove_added: Cell::new(true),
    });
    let doc = CxfaDocument::with_notify(Some(notify.clone()));
    let xml = Rc::new(RefCell::new(CfxXmlDocument::new()));
    doc.set_xml_document(xml);
    let root = doc
        .create_node(XfaPacketType::Datasets, XfaElement::DataGroup)
        .unwrap();
    root.set_cdata(XfaAttribute::Name, &wide("root"));
    let root_xml = root.create_xml_mapping_node().unwrap();
    let child = doc
        .create_node(XfaPacketType::Datasets, XfaElement::DataValue)
        .unwrap();
    child.set_cdata(XfaAttribute::Name, &wide("child"));
    let child_xml = child.create_xml_mapping_node().unwrap();
    assert!(root.insert_child_and_notify(-1, &child));
    assert_detached(&child);
    assert_eq!(child_xml.parent(), None);
    assert_eq!(root_xml.first_child(), None);
    assert_eq!(notify.added.get(), 1);
    assert_eq!(notify.removed.get(), 1);
    child.set_cdata_and_notify(XfaAttribute::Name, &wide("before"), true);
    assert_eq!(notify.attributes.get(), 0);
    child.set_initialized_flag_and_notify();
    assert!(child.is_initialized());
    assert_eq!(notify.ready.get(), 1);
    child.set_initialized_flag_and_notify();
    assert_eq!(notify.ready.get(), 1);
    child.set_cdata_and_notify(XfaAttribute::Name, &wide("outer"), true);
    assert_eq!(child.get_cdata(XfaAttribute::Name), wide("inner"));
    assert_eq!(child.get_name_hash(), hash_name(&wide("inner")));
    assert_eq!(notify.attributes.get(), 2);
    let other = CxfaDocument::new();
    let foreign = other
        .create_node(XfaPacketType::Datasets, XfaElement::DataValue)
        .unwrap();
    assert!(!root.insert_child_and_notify(-1, &foreign));
}
