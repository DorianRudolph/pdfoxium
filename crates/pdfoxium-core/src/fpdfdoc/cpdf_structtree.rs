// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct page-specific structure loading from `cpdf_structtree.{cpp,h}`.
use super::{cpdf_numbertree::CpdfNumberTree, cpdf_structelement::CpdfStructElement};
use crate::fpdfapi::parser::{
    cpdf_array::CpdfArray,
    cpdf_dictionary::CpdfDictionary,
    cpdf_document::CpdfDocument,
    cpdf_object::{get_dict_object, get_direct_object, ObjectRef},
    cpdf_reference::CpdfReference,
    cpdf_scalar_object::ObjectType,
};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

pub struct CpdfStructTree {
    tree_root: Option<ObjectRef>,
    role_map: Option<ObjectRef>,
    page: ObjectRef,
    kids: Vec<Option<Rc<CpdfStructElement>>>,
}
impl CpdfStructTree {
    pub fn load_page(document: &CpdfDocument, page: ObjectRef) -> Option<Rc<Self>> {
        Self::load_page_from_root(document.root()?, page)
    }
    /// The same load path with a retained catalog, useful to document adapters
    /// which already own their parsed root. References keep their native holder.
    pub fn load_page_from_root(catalog: ObjectRef, page: ObjectRef) -> Option<Rc<Self>> {
        let mark_info = dictionary_for(&catalog, b"MarkInfo")?;
        if integer_for(&mark_info, b"Marked", 0) == 0 {
            return None;
        }
        let tree_root = dictionary_for(&catalog, b"StructTreeRoot");
        let role_map = tree_root
            .as_ref()
            .and_then(|root| dictionary_for(root, b"RoleMap"));
        let mut tree = Self {
            tree_root,
            role_map,
            page,
            kids: Vec::new(),
        };
        tree.load_page_tree();
        Some(Rc::new(tree))
    }
    pub fn count_top_elements(&self) -> usize {
        self.kids.len()
    }
    pub fn get_top_element(&self, index: usize) -> Option<Rc<CpdfStructElement>> {
        self.kids.get(index).and_then(Clone::clone)
    }
    pub fn get_page_obj_num(&self) -> u32 {
        self.page.borrow().obj_num()
    }
    pub fn get_role_map_name_for(&self, kind: &[u8]) -> Vec<u8> {
        if let Some(map) = &self.role_map {
            let mapped = name_for(map, kind);
            if !mapped.is_empty() {
                return mapped;
            }
        }
        kind.to_vec()
    }
    fn load_page_tree(&mut self) {
        let Some(root) = self.tree_root.clone() else {
            return;
        };
        let Some(kids) = direct_for(&root, b"K") else {
            return;
        };
        let count = match kids.borrow().object_type() {
            ObjectType::Dictionary => 1,
            ObjectType::Array => array_size(&kids),
            _ => return,
        };
        self.kids.resize_with(count, || None);
        let Some(parent_tree) = dictionary_for(&root, b"ParentTree") else {
            return;
        };
        let parents_id = integer_for(&self.page, b"StructParents", -1);
        if parents_id < 0 {
            return;
        }
        let Some(parents) = CpdfNumberTree::new(parent_tree).lookup_value(parents_id) else {
            return;
        };
        if parents.borrow().object_type() != ObjectType::Array {
            return;
        }
        let mut map = HashMap::new();
        for index in 0..array_size(&parents) {
            if let Some(parent) =
                array_direct(&parents, index).and_then(|obj| get_dict_object(&obj))
            {
                self.add_page_node(parent, &mut map, &mut HashSet::new(), 0);
            }
        }
    }
    fn add_page_node(
        &mut self,
        dictionary: ObjectRef,
        map: &mut HashMap<usize, Rc<CpdfStructElement>>,
        active: &mut HashSet<usize>,
        level: usize,
    ) -> Option<Rc<CpdfStructElement>> {
        if level > 32 {
            return None;
        }
        let key = identity(&dictionary);
        // Native caches before ascending parents. Reject a malformed parent
        // cycle before reusing that cache entry, so child ownership stays acyclic.
        if active.contains(&key) {
            return None;
        }
        if let Some(element) = map.get(&key) {
            return Some(element.clone());
        }
        let element = CpdfStructElement::new(self, dictionary.clone());
        map.insert(key, element.clone());
        active.insert(key);
        let parent = dictionary_for(&dictionary, b"P");
        if parent
            .as_ref()
            .is_none_or(|parent| name_for(parent, b"Type") == b"StructTreeRoot")
        {
            if !self.add_top_level_node(&dictionary, &element) {
                map.remove(&key);
            }
        } else if let Some(parent) =
            parent.and_then(|parent| self.add_page_node(parent, map, active, level + 1))
        {
            if parent.update_kid_if_element(&dictionary, &element) {
                element.set_parent(Some(&parent));
            } else {
                map.remove(&key);
            }
        }
        active.remove(&key);
        Some(element)
    }
    fn add_top_level_node(
        &mut self,
        dictionary: &ObjectRef,
        element: &Rc<CpdfStructElement>,
    ) -> bool {
        let Some(kids) = self
            .tree_root
            .as_ref()
            .and_then(|root| direct_for(root, b"K"))
        else {
            return false;
        };
        if kids.borrow().object_type() == ObjectType::Dictionary {
            // Inline dictionaries share object number zero; compare their real
            // identity rather than treating unrelated inline dictionaries alike.
            let number = dictionary.borrow().obj_num();
            if !Rc::ptr_eq(&kids, dictionary) && (number == 0 || kids.borrow().obj_num() != number)
            {
                return false;
            }
            self.kids[0] = Some(element.clone());
            return true;
        }
        if kids.borrow().object_type() != ObjectType::Array {
            return true;
        }
        let number = dictionary.borrow().obj_num();
        let mut saved = false;
        for index in 0..array_size(&kids) {
            if array_raw(&kids, index).and_then(|kid| reference_number(&kid)) == Some(number) {
                self.kids[index] = Some(element.clone());
                saved = true;
            }
        }
        saved
    }
}

pub(crate) fn identity(object: &ObjectRef) -> usize {
    Rc::as_ptr(object) as *const () as usize
}
pub(crate) fn raw_for(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)
}
pub(crate) fn direct_for(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    // Release the dictionary borrow before lazy reference parsing/host I/O.
    get_direct_object(&raw_for(object, key)?)
}
pub(crate) fn dictionary_for(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    get_dict_object(&raw_for(object, key)?)
}
pub(crate) fn integer_for(object: &ObjectRef, key: &[u8], default: i32) -> i32 {
    direct_for(object, key).map_or(default, |value| value.borrow().get_integer())
}
pub(crate) fn name_for(object: &ObjectRef, key: &[u8]) -> Vec<u8> {
    let Some(value) = raw_for(object, key) else {
        return Vec::new();
    };
    let value = value.borrow();
    if value.object_type() == ObjectType::Name {
        value.get_string()
    } else {
        Vec::new()
    }
}
pub(crate) fn array_size(object: &ObjectRef) -> usize {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .map_or(0, CpdfArray::size)
}
pub(crate) fn array_raw(object: &ObjectRef, index: usize) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()?
        .get_object_at(index)
}
pub(crate) fn array_direct(object: &ObjectRef, index: usize) -> Option<ObjectRef> {
    get_direct_object(&array_raw(object, index)?)
}
pub(crate) fn reference_number(object: &ObjectRef) -> Option<u32> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfReference>()
        .map(CpdfReference::ref_obj_num)
}

#[cfg(test)]
#[path = "cpdf_structtree_tests.rs"]
mod tests;
