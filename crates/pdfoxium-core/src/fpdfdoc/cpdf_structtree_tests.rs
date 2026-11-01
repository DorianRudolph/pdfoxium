// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived tests: the pinned structure model has no native unit file.
use super::*;
use crate::fpdfapi::parser::{
    cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef},
    cpdf_name::CpdfName,
    cpdf_object::retain_object,
    cpdf_scalar_object::CpdfScalarObject,
    cpdf_string::CpdfString,
};

fn dict() -> ObjectRef {
    retain_object(CpdfDictionary::new())
}
fn array(values: impl IntoIterator<Item = ObjectRef>) -> ObjectRef {
    let mut result = CpdfArray::new();
    for value in values {
        result.append(value);
    }
    retain_object(result)
}
fn number(value: i32) -> ObjectRef {
    retain_object(CpdfScalarObject::number_from_signed(value))
}
fn name(value: &[u8]) -> ObjectRef {
    retain_object(CpdfName::new(None, value))
}
fn string(value: &str) -> ObjectRef {
    retain_object(CpdfString::from_bytes(None, value.as_bytes()))
}
fn set(object: &ObjectRef, key: &[u8], value: ObjectRef) {
    object
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(key, Some(value));
}
fn reference(holder: &HolderRef, object: &ObjectRef) -> ObjectRef {
    retain_object(CpdfReference::new(holder, object.borrow().obj_num()))
}
fn indirect(holder: &HolderRef, object: ObjectRef) -> ObjectRef {
    holder.borrow_mut().add_indirect_object(object.clone());
    object
}
fn catalog(root: Option<&ObjectRef>) -> ObjectRef {
    let catalog = dict();
    let marked = dict();
    set(&marked, b"Marked", number(1));
    set(&catalog, b"MarkInfo", marked);
    if let Some(root) = root {
        set(&catalog, b"StructTreeRoot", root.clone());
    }
    catalog
}
fn indirect_catalog(root: &ObjectRef, holder: &HolderRef) -> ObjectRef {
    let catalog = catalog(None);
    set(&catalog, b"StructTreeRoot", reference(holder, root));
    catalog
}
fn parent_array(root: &ObjectRef, parents: ObjectRef) {
    let tree = dict();
    set(&tree, b"Nums", array([number(0), parents]));
    set(root, b"ParentTree", tree);
}

#[test]
fn tagged_gate_and_empty_top_slots_preserve_native_load_results() {
    let page = dict();
    assert!(CpdfStructTree::load_page_from_root(dict(), page.clone()).is_none());
    let tree = CpdfStructTree::load_page_from_root(catalog(None), page.clone()).unwrap();
    assert_eq!(tree.count_top_elements(), 0);
    assert!(tree.get_top_element(0).is_none());
    let root = dict();
    set(&root, b"K", dict());
    let tree = CpdfStructTree::load_page_from_root(catalog(Some(&root)), page.clone()).unwrap();
    assert_eq!(tree.count_top_elements(), 1);
    assert!(tree.get_top_element(0).is_none());
    set(&root, b"K", array([dict(), number(1), name(b"invalid")]));
    let tree = CpdfStructTree::load_page_from_root(catalog(Some(&root)), page).unwrap();
    assert_eq!(tree.count_top_elements(), 3);
    assert!((0..4).all(|index| tree.get_top_element(index).is_none()));
}

#[test]
fn parent_number_tree_role_map_children_text_and_identity_are_connected() {
    let holder = CpdfIndirectObjectHolder::new();
    let page = indirect(&holder, dict());
    set(&page, b"StructParents", number(0));
    let foreign_page = indirect(&holder, dict());
    let root = indirect(&holder, dict());
    set(&root, b"Type", name(b"StructTreeRoot"));
    let top = indirect(&holder, dict());
    set(&top, b"Type", name(b"StructElem"));
    set(&top, b"S", name(b"Document"));
    set(&top, b"P", reference(&holder, &root));
    set(&top, b"Pg", reference(&holder, &page));
    let child = indirect(&holder, dict());
    set(&child, b"S", name(b"Custom"));
    set(&child, b"Type", name(b"StructElem"));
    set(&child, b"P", reference(&holder, &top));
    for (key, value) in [
        (b"Alt".as_slice(), "alternative"),
        (b"ActualText", "actual"),
        (b"E", "expansion"),
        (b"T", "title"),
        (b"ID", "id"),
        (b"Lang", "en-US"),
    ] {
        set(&child, key, string(value));
    }
    let attributes = dict();
    set(&attributes, b"O", name(b"Layout"));
    set(&child, b"A", attributes.clone());
    let role_map = dict();
    set(&role_map, b"Custom", name(b"Span"));
    set(&root, b"RoleMap", role_map);
    let mcr = dict();
    set(&mcr, b"Type", name(b"MCR"));
    set(&mcr, b"MCID", number(7));
    set(
        &mcr,
        b"Stm",
        retain_object(CpdfReference::without_holder(80)),
    );
    let foreign = dict();
    set(&foreign, b"Type", name(b"MCR"));
    set(&foreign, b"MCID", number(9));
    set(&foreign, b"Pg", reference(&holder, &foreign_page));
    let object = dict();
    set(&object, b"Type", name(b"OBJR"));
    set(
        &object,
        b"Obj",
        retain_object(CpdfReference::without_holder(81)),
    );
    set(
        &top,
        b"K",
        array([
            number(3),
            mcr,
            foreign,
            object,
            reference(&holder, &child),
            name(b"invalid"),
        ]),
    );
    set(
        &root,
        b"K",
        array([reference(&holder, &top), reference(&holder, &top)]),
    );
    parent_array(
        &root,
        array([reference(&holder, &child), reference(&holder, &child)]),
    );
    let tree = CpdfStructTree::load_page_from_root(indirect_catalog(&root, &holder), page.clone())
        .unwrap();
    assert_eq!(tree.get_page_obj_num(), page.borrow().obj_num());
    assert_eq!(tree.count_top_elements(), 2);
    let top = tree.get_top_element(0).unwrap();
    assert!(Rc::ptr_eq(&top, &tree.get_top_element(1).unwrap()));
    assert!(top.get_parent().is_none());
    assert_eq!(top.count_kids(), 6);
    assert_eq!(
        (0..7)
            .map(|index| top.get_kid_content_id(index))
            .collect::<Vec<_>>(),
        [3, 7, -1, -1, -1, -1, -1]
    );
    assert!((0..4).all(|index| top.get_kid_if_element(index).is_none()));
    let child = top.get_kid_if_element(4).unwrap();
    assert!(Rc::ptr_eq(&top, &child.get_parent().unwrap()));
    assert_eq!(child.get_type(), b"Span");
    assert_eq!(child.get_raw_type(), b"Custom");
    assert_eq!(child.get_obj_type(), b"StructElem");
    for (actual, expected) in [
        (child.get_alt_text(), "alternative"),
        (child.get_actual_text(), "actual"),
        (child.get_expansion(), "expansion"),
        (child.get_title(), "title"),
    ] {
        assert_eq!(actual, expected.chars().map(u32::from).collect::<Vec<_>>());
    }
    assert_eq!(child.get_id(), Some("id".chars().map(u32::from).collect()));
    assert_eq!(
        child.get_lang(),
        Some("en-US".chars().map(u32::from).collect())
    );
    assert!(Rc::ptr_eq(&child.get_a().unwrap(), &attributes));
    assert!(child.get_k().is_none());
    assert_eq!(tree.get_role_map_name_for(b"UnknownRole"), b"UnknownRole");
    let weak_top = Rc::downgrade(&top);
    drop(top);
    drop(tree);
    assert!(weak_top.upgrade().is_none());
    assert!(child.get_parent().is_none());
    assert_eq!(child.get_type(), b"Span");
}

#[test]
fn direct_top_dictionary_and_optional_string_types_follow_source() {
    let page = dict();
    set(&page, b"StructParents", number(0));
    let root = dict();
    let top = dict();
    set(&top, b"S", name(b"P"));
    set(&root, b"K", top.clone());
    parent_array(&root, array([top.clone()]));
    set(&top, b"ID", string(""));
    set(&top, b"Lang", name(b"en"));
    set(&top, b"K", number(-3));
    let tree = CpdfStructTree::load_page_from_root(catalog(Some(&root)), page.clone()).unwrap();
    let element = tree.get_top_element(0).unwrap();
    assert_eq!(element.get_id(), Some(Vec::new()));
    assert_eq!(element.get_lang(), None);
    assert_eq!(element.get_kid_content_id(0), -3);
    assert_eq!(element.count_kids(), 1);
    // A different inline dictionary must not match merely because both have
    // native object number zero.
    parent_array(&root, array([dict()]));
    assert!(
        CpdfStructTree::load_page_from_root(catalog(Some(&root)), page)
            .unwrap()
            .get_top_element(0)
            .is_none()
    );
}

#[test]
fn cyclic_parent_dictionary_graphs_never_create_element_ownership_cycles() {
    let holder = CpdfIndirectObjectHolder::new();
    let page = dict();
    set(&page, b"StructParents", number(0));
    let root = dict();
    let first = indirect(&holder, dict());
    let second = indirect(&holder, dict());
    set(&first, b"P", reference(&holder, &second));
    set(&second, b"P", reference(&holder, &first));
    set(&first, b"K", reference(&holder, &second));
    set(&second, b"K", reference(&holder, &first));
    set(&root, b"K", array([reference(&holder, &first)]));
    parent_array(&root, array([reference(&holder, &first)]));
    let tree = CpdfStructTree::load_page_from_root(catalog(Some(&root)), page).unwrap();
    assert_eq!(tree.count_top_elements(), 1);
    assert!(tree.get_top_element(0).is_none());
    // Only each holder's stored dictionary and these local variables remain.
    assert_eq!(Rc::strong_count(&first), 2);
    assert_eq!(Rc::strong_count(&second), 2);
}

#[test]
fn native_parent_depth_limit_is_preserved() {
    for depth in [32, 33] {
        let holder = CpdfIndirectObjectHolder::new();
        let page = dict();
        set(&page, b"StructParents", number(0));
        let root = indirect(&holder, dict());
        set(&root, b"Type", name(b"StructTreeRoot"));
        let nodes: Vec<_> = (0..=depth).map(|_| indirect(&holder, dict())).collect();
        for index in 0..nodes.len() {
            set(
                &nodes[index],
                b"P",
                reference(&holder, if index == 0 { &root } else { &nodes[index - 1] }),
            );
            if let Some(next) = nodes.get(index + 1) {
                set(&nodes[index], b"K", reference(&holder, next));
            }
        }
        set(&root, b"K", reference(&holder, &nodes[0]));
        parent_array(&root, array([reference(&holder, nodes.last().unwrap())]));
        let tree =
            CpdfStructTree::load_page_from_root(indirect_catalog(&root, &holder), page).unwrap();
        assert_eq!(tree.get_top_element(0).is_some(), depth == 32);
    }
}
