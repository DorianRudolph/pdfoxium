// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa_document_datamerger_imp.{h,cpp}`: form generation, matching,
//! template-container reuse and inherited binding scopes.
use super::{
    cxfa_document::CxfaDocument,
    cxfa_node::{CxfaNode, XfaNodeFlag},
};
use crate::xfa::fxfa::fxfa_basic::{XfaElement, XfaHashCode, XfaPacketType};
use std::rc::Rc;

pub fn data_merge_need_generate_form(
    template_child: &CxfaNode,
    use_instance_manager: bool,
) -> bool {
    let element = template_child.get_element_type();
    if element == XfaElement::Variables {
        return true;
    }
    if template_child.is_container_node() {
        return false;
    }
    !(element == XfaElement::Proto || (use_instance_manager && element == XfaElement::Occur))
}
pub fn data_merge_find_form_dom_instance(
    _document: &CxfaDocument,
    element: XfaElement,
    name_hash: u32,
    form_parent: &CxfaNode,
) -> Option<CxfaNode> {
    let mut child = form_parent.first_child();
    while let Some(node) = child {
        if node.get_element_type() == element
            && node.get_name_hash() == name_hash
            && node.is_unused_node()
        {
            return Some(node);
        }
        child = node.next_sibling();
    }
    None
}
pub fn node_merge_clone_or_merge_container(
    document: &Rc<CxfaDocument>,
    form_parent: &CxfaNode,
    template: &CxfaNode,
    recursive: bool,
    mut subforms: Option<&mut Vec<CxfaNode>>,
) -> Option<CxfaNode> {
    if template.get_packet_type() != XfaPacketType::Template {
        return None;
    }
    let has_subforms = subforms.is_some();
    let existing = if let Some(array) = subforms.as_mut() {
        if array.is_empty() {
            None
        } else {
            Some(array.remove(0))
        }
    } else {
        data_merge_find_form_dom_instance(
            document,
            template.get_element_type(),
            template.get_name_hash(),
            form_parent,
        )
    };
    if let Some(existing) = existing {
        if has_subforms {
            if !form_parent.insert_child_before_and_notify(&existing, None) {
                return None;
            }
        } else if existing.is_container_node() {
            if !form_parent.remove_child_and_notify(&existing, true)
                || !form_parent.insert_child_before_and_notify(&existing, None)
            {
                return None;
            }
        }
        existing.clear_flag(XfaNodeFlag::UnusedNode);
        existing.set_template_node(Some(template));
        if recursive && existing.get_element_type() != XfaElement::Items {
            let mut child = template.first_child();
            while let Some(node) = child {
                if data_merge_need_generate_form(&node, true) {
                    let _ = node_merge_clone_or_merge_container(
                        document, &existing, &node, recursive, None,
                    );
                }
                child = node.next_sibling();
            }
        }
        existing.set_initialized_flag_and_notify();
        return Some(existing);
    }
    let new_node = template.clone_template_to_form(false)?;
    if !form_parent.insert_child_before_and_notify(&new_node, None) {
        return None;
    }
    if !recursive {
        return Some(new_node);
    }
    let mut child = template.first_child();
    while let Some(node) = child {
        if node.get_packet_type() == XfaPacketType::Template
            && data_merge_need_generate_form(&node, true)
        {
            if let Some(new_child) = node.clone_template_to_form(true) {
                new_node.insert_child_before_and_notify(&new_child, None);
            }
        }
        child = node.next_sibling();
    }
    Some(new_node)
}
pub fn data_merge_find_data_scope(parent_form: Option<&CxfaNode>) -> Option<CxfaNode> {
    let parent_form = parent_form?;
    let mut bound = Some(parent_form.clone());
    while let Some(node) = bound {
        if !node.is_container_node() {
            break;
        }
        if let Some(scope) = node.get_bind_data() {
            return Some(scope);
        }
        bound = node.parent();
    }
    parent_form
        .document()?
        .get_xfa_node(XfaHashCode::Data as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfa::fxfa::fxfa_basic::XfaAttribute;
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn node(
        doc: &Rc<CxfaDocument>,
        packet: XfaPacketType,
        element: XfaElement,
        name: &str,
    ) -> CxfaNode {
        let node = doc.create_node(packet, element).unwrap();
        if !name.is_empty() {
            node.set_cdata(XfaAttribute::Name, &wide(name));
        }
        node
    }
    fn add(parent: &CxfaNode, child: &CxfaNode) {
        assert!(parent.insert_child_and_notify(-1, child));
    }
    #[test]
    fn generation_filters_preserve_variables_and_instance_manager_occur_rules() {
        let doc = CxfaDocument::new();
        for (element, ordinary, managed) in [
            (XfaElement::Variables, true, true),
            (XfaElement::Subform, false, false),
            (XfaElement::Field, false, false),
            (XfaElement::Proto, false, false),
            (XfaElement::Occur, true, false),
            (XfaElement::Bind, true, true),
            (XfaElement::Items, true, true),
        ] {
            let child = node(&doc, XfaPacketType::Template, element, "");
            assert_eq!(
                data_merge_need_generate_form(&child, false),
                ordinary,
                "{element:?}"
            );
            assert_eq!(
                data_merge_need_generate_form(&child, true),
                managed,
                "{element:?}"
            );
        }
    }
    #[test]
    fn existing_instances_require_type_hash_and_unused_flag_then_reuse_identity() {
        let doc = CxfaDocument::new();
        let parent = node(&doc, XfaPacketType::Form, XfaElement::Subform, "");
        let template = node(&doc, XfaPacketType::Template, XfaElement::Subform, "target");
        let wrong = node(&doc, XfaPacketType::Form, XfaElement::Field, "target");
        wrong.set_flag(XfaNodeFlag::UnusedNode);
        add(&parent, &wrong);
        let used = node(&doc, XfaPacketType::Form, XfaElement::Subform, "target");
        add(&parent, &used);
        let matching = node(&doc, XfaPacketType::Form, XfaElement::Subform, "target");
        matching.set_flag(XfaNodeFlag::UnusedNode);
        add(&parent, &matching);
        let tail = node(&doc, XfaPacketType::Form, XfaElement::Subform, "tail");
        add(&parent, &tail);
        assert_eq!(
            data_merge_find_form_dom_instance(
                &doc,
                XfaElement::Subform,
                template.get_name_hash(),
                &parent
            ),
            Some(matching.clone())
        );
        let reused =
            node_merge_clone_or_merge_container(&doc, &parent, &template, false, None).unwrap();
        assert_eq!(reused, matching);
        assert_eq!(parent.last_child(), Some(matching.clone()));
        assert_eq!(matching.prev_sibling(), Some(tail));
        assert!(!matching.is_unused_node());
        assert_eq!(matching.get_template_node_if_exists(), Some(template));
        assert!(matching.has_flag(XfaNodeFlag::Initialized));
    }
    #[test]
    fn recursive_new_clone_generates_properties_and_variables_but_not_containers_or_occur() {
        let doc = CxfaDocument::new();
        let parent = node(&doc, XfaPacketType::Form, XfaElement::Subform, "");
        let template = node(&doc, XfaPacketType::Template, XfaElement::Subform, "source");
        let occur = node(&doc, XfaPacketType::Template, XfaElement::Occur, "");
        let field = node(&doc, XfaPacketType::Template, XfaElement::Field, "field");
        let proto = node(&doc, XfaPacketType::Template, XfaElement::Proto, "");
        let variables = node(&doc, XfaPacketType::Template, XfaElement::Variables, "");
        let bind = node(&doc, XfaPacketType::Template, XfaElement::Bind, "");
        let picture = node(&doc, XfaPacketType::Template, XfaElement::Picture, "");
        picture.set_content(&wide("num{zz9}"), &wide("num{zz9}"), false);
        add(&bind, &picture);
        for child in [&occur, &field, &proto, &variables, &bind] {
            add(&template, child);
        }
        let generated =
            node_merge_clone_or_merge_container(&doc, &parent, &template, true, None).unwrap();
        assert_eq!(generated.get_packet_type(), XfaPacketType::Form);
        assert_eq!(generated.get_cdata(XfaAttribute::Name), wide("source"));
        assert_eq!(
            generated.get_template_node_if_exists(),
            Some(template.clone())
        );
        for element in [XfaElement::Occur, XfaElement::Field, XfaElement::Proto] {
            assert!(generated.get_child(0, element, false).is_none());
        }
        assert!(generated
            .get_child(0, XfaElement::Variables, false)
            .is_some());
        let bound = generated.get_child(0, XfaElement::Bind, false).unwrap();
        assert_eq!(bound.first_child().unwrap().get_content(), wide("num{zz9}"));
        let shallow =
            node_merge_clone_or_merge_container(&doc, &parent, &template, false, None).unwrap();
        assert!(shallow.first_child().is_none());
        assert!(
            node_merge_clone_or_merge_container(&doc, &parent, &generated, false, None).is_none()
        );
    }
    #[test]
    fn existing_recursive_merge_preserves_items_and_consumes_detached_subforms_in_order() {
        let doc = CxfaDocument::new();
        let parent = node(&doc, XfaPacketType::Form, XfaElement::Subform, "");
        let template = node(&doc, XfaPacketType::Template, XfaElement::Subform, "form");
        let template_bind = node(&doc, XfaPacketType::Template, XfaElement::Bind, "");
        add(&template, &template_bind);
        let existing = node(&doc, XfaPacketType::Form, XfaElement::Subform, "form");
        existing.set_flag(XfaNodeFlag::UnusedNode);
        add(&parent, &existing);
        assert_eq!(
            node_merge_clone_or_merge_container(&doc, &parent, &template, true, None),
            Some(existing.clone())
        );
        assert!(existing.get_child(0, XfaElement::Bind, false).is_some());
        let items_template = node(&doc, XfaPacketType::Template, XfaElement::Items, "");
        let new_item = node(&doc, XfaPacketType::Template, XfaElement::Text, "");
        add(&items_template, &new_item);
        let items = node(&doc, XfaPacketType::Form, XfaElement::Items, "");
        let old_item = node(&doc, XfaPacketType::Form, XfaElement::Text, "");
        add(&items, &old_item);
        items.set_flag(XfaNodeFlag::UnusedNode);
        add(&parent, &items);
        assert_eq!(
            node_merge_clone_or_merge_container(&doc, &parent, &items_template, true, None),
            Some(items.clone())
        );
        assert_eq!(items.first_child(), Some(old_item));
        assert_eq!(items.count_children(XfaElement::Unknown, false), 1);
        let detached1 = node(&doc, XfaPacketType::Form, XfaElement::Subform, "one");
        let detached2 = node(&doc, XfaPacketType::Form, XfaElement::Subform, "two");
        let mut array = vec![detached1.clone(), detached2.clone()];
        assert_eq!(
            node_merge_clone_or_merge_container(&doc, &parent, &template, false, Some(&mut array)),
            Some(detached1)
        );
        assert_eq!(array, [detached2.clone()]);
        assert_eq!(
            node_merge_clone_or_merge_container(&doc, &parent, &template, false, Some(&mut array)),
            Some(detached2)
        );
        assert!(array.is_empty());
    }
    #[test]
    fn data_scope_walks_container_bindings_then_falls_back_to_datasets_data() {
        use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
        use std::cell::RefCell;
        let doc = CxfaDocument::new();
        let parent = node(&doc, XfaPacketType::Form, XfaElement::Subform, "");
        let nested = node(&doc, XfaPacketType::Form, XfaElement::Subform, "");
        let field = node(&doc, XfaPacketType::Form, XfaElement::Field, "");
        add(&parent, &nested);
        add(&nested, &field);
        let scope = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataGroup,
            "bound",
        );
        parent.set_binding_node(Some(&scope));
        assert_eq!(data_merge_find_data_scope(Some(&field)), Some(scope));
        let nearer = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataGroup,
            "nearer",
        );
        nested.set_binding_node(Some(&nearer));
        assert_eq!(data_merge_find_data_scope(Some(&field)), Some(nearer));
        assert!(data_merge_find_data_scope(None).is_none());
        let xml = Rc::new(RefCell::new(CfxXmlDocument::new()));
        doc.set_xml_document(xml.clone());
        let root = node(&doc, XfaPacketType::Xdp, XfaElement::Xfa, "");
        let datasets = node(
            &doc,
            XfaPacketType::Datasets,
            XfaElement::DataModel,
            "datasets",
        );
        let data = node(&doc, XfaPacketType::Datasets, XfaElement::DataGroup, "data");
        datasets.set_xml_mapping_node(xml.borrow_mut().create_element(&wide("datasets")));
        data.set_xml_mapping_node(xml.borrow_mut().create_element(&wide("data")));
        add(&root, &datasets);
        add(&datasets, &data);
        assert!(doc.set_root(Some(root)));
        let non_container = node(&doc, XfaPacketType::Form, XfaElement::Value, "");
        add(&field, &non_container);
        assert_eq!(data_merge_find_data_scope(Some(&non_container)), Some(data));
    }
}
