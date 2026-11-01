// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Name-tree lookup, indexed traversal and in-place mutation from
//! `core/fpdfdoc/cpdf_nametree.{h,cpp}`.
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfapi::parser::fpdf_parser_decode::pdf_decode_text;
use std::collections::HashSet;
use std::rc::Rc;

fn entry(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    let value = object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)?;
    get_direct_object(&value)
}
fn typed(object: &ObjectRef, key: &[u8], kind: ObjectType) -> Option<ObjectRef> {
    entry(object, key).filter(|value| value.borrow().object_type() == kind)
}
fn array_entry(array: &ObjectRef, index: usize) -> Option<ObjectRef> {
    let value = array
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()?
        .get_object_at(index)?;
    get_direct_object(&value)
}
fn size(array: &ObjectRef) -> usize {
    array
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .map_or(0, CpdfArray::size)
}
fn traversed(object: &ObjectRef, seen: &mut HashSet<u32>) -> bool {
    let number = object.borrow().obj_num();
    number != 0 && !seen.insert(number)
}
fn array_traversed(array: &ObjectRef, seen: &mut HashSet<u32>) -> bool {
    if traversed(array, seen) {
        return true;
    }
    let mut index = 0;
    while index < size(array) {
        let value = array
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .get_object_at(index)
            .unwrap();
        if traversed(&value, seen) {
            return true;
        }
        index += 1;
    }
    false
}
fn limits(array: &ObjectRef) -> (Vec<u32>, Vec<u32>) {
    while size(array) < 2 {
        array
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfString::from_bytes(None, b""));
    }
    let (left, right) = {
        let borrowed = array.borrow();
        let a = borrowed.as_any().downcast_ref::<CpdfArray>().unwrap();
        (a.get_object_at(0).unwrap(), a.get_object_at(1).unwrap())
    };
    let lo = left.borrow().get_unicode_text();
    let hi = right.borrow().get_unicode_text();
    if lo > hi {
        let mut borrowed = array.borrow_mut();
        let a = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        a.set_at(0, right);
        a.set_at(1, left);
        (hi, lo)
    } else {
        (lo, hi)
    }
}
#[derive(Default)]
struct NodeToInsert {
    names: Option<ObjectRef>,
    preceding_pair: Option<usize>,
}
fn trim_limits(array: &ObjectRef) {
    let mut borrowed = array.borrow_mut();
    let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
    while array.size() > 2 {
        array.remove_at(array.size() - 1);
    }
}
fn search(
    node: &ObjectRef,
    name: &[u32],
    level: usize,
    seen: &mut HashSet<u32>,
    mut insertion: Option<&mut NodeToInsert>,
) -> Option<ObjectRef> {
    if level > 32 {
        return None;
    }
    let names = typed(node, b"Names", ObjectType::Array).filter(|a| !array_traversed(a, seen));
    let node_limits =
        typed(node, b"Limits", ObjectType::Array).filter(|a| !array_traversed(a, seen));
    if let Some(node_limits) = node_limits {
        if insertion.is_some() {
            trim_limits(&node_limits);
        }
        let (lo, hi) = limits(&node_limits);
        if name < lo.as_slice() {
            return None;
        }
        if name > hi.as_slice() {
            let insert = insertion.as_deref_mut()?;
            if let Some(names) = &names {
                insert.names = Some(names.clone());
                insert.preceding_pair = (size(names) / 2).checked_sub(1);
                return None;
            }
        }
    }
    if let Some(names) = names {
        for i in 0..size(&names) / 2 {
            let key =
                array_entry(&names, i * 2).map_or_else(Vec::new, |v| v.borrow().get_unicode_text());
            let comparison = key.as_slice().cmp(name);
            if comparison == std::cmp::Ordering::Greater {
                break;
            }
            if let Some(insert) = insertion.as_deref_mut() {
                insert.names = Some(names.clone());
                insert.preceding_pair = Some(i);
            }
            if comparison == std::cmp::Ordering::Equal {
                return array_entry(&names, i * 2 + 1);
            }
        }
        return None;
    }
    let kids = typed(node, b"Kids", ObjectType::Array)?;
    if traversed(&kids, seen) {
        return None;
    }
    for i in 0..size(&kids) {
        let Some(child) =
            array_entry(&kids, i).filter(|v| v.borrow().object_type() == ObjectType::Dictionary)
        else {
            continue;
        };
        if traversed(&child, seen) {
            continue;
        }
        if let Some(value) = search(&child, name, level + 1, seen, insertion.as_deref_mut()) {
            return Some(value);
        }
    }
    None
}
struct IndexSearchResult {
    key: Vec<u32>,
    value: ObjectRef,
    container: ObjectRef,
    index: usize,
}
fn search_index(
    node: &ObjectRef,
    target: usize,
    level: usize,
    current: &mut usize,
) -> Option<IndexSearchResult> {
    if level > 32 {
        return None;
    }
    if let Some(names) = typed(node, b"Names", ObjectType::Array) {
        let count = size(&names) / 2;
        let end = current.checked_add(count)?;
        if target >= end {
            *current = end;
            return None;
        }
        let index = target.checked_sub(*current)?.checked_mul(2)?;
        let value = array_entry(&names, index + 1)?;
        let key =
            array_entry(&names, index).map_or_else(Vec::new, |v| v.borrow().get_unicode_text());
        return Some(IndexSearchResult {
            key,
            value,
            container: names,
            index,
        });
    }
    let kids = typed(node, b"Kids", ObjectType::Array)?;
    for i in 0..size(&kids) {
        let Some(child) =
            array_entry(&kids, i).filter(|v| v.borrow().object_type() == ObjectType::Dictionary)
        else {
            continue;
        };
        if let Some(found) = search_index(&child, target, level + 1, current) {
            return Some(found);
        }
    }
    None
}
fn count_names(node: &ObjectRef, level: usize, seen: &mut HashSet<usize>) -> usize {
    if level > 32 || !seen.insert(Rc::as_ptr(node).cast::<()>() as usize) {
        return 0;
    }
    if let Some(names) = typed(node, b"Names", ObjectType::Array) {
        return size(&names) / 2;
    }
    let Some(kids) = typed(node, b"Kids", ObjectType::Array) else {
        return 0;
    };
    let mut count = 0usize;
    for i in 0..size(&kids) {
        if let Some(child) =
            array_entry(&kids, i).filter(|v| v.borrow().object_type() == ObjectType::Dictionary)
        {
            count = count.saturating_add(count_names(&child, level + 1, seen));
        }
    }
    count
}
fn ancestor_limits(
    node: &ObjectRef,
    find: &ObjectRef,
    level: usize,
    result: &mut Vec<Option<ObjectRef>>,
) -> bool {
    if level > 32 {
        return false;
    }
    if typed(node, b"Names", ObjectType::Array).is_some_and(|names| Rc::ptr_eq(&names, find)) {
        result.push(typed(node, b"Limits", ObjectType::Array));
        return true;
    }
    let Some(kids) = typed(node, b"Kids", ObjectType::Array) else {
        return false;
    };
    for i in 0..size(&kids) {
        let Some(child) =
            array_entry(&kids, i).filter(|v| v.borrow().object_type() == ObjectType::Dictionary)
        else {
            continue;
        };
        if ancestor_limits(&child, find, level + 1, result) {
            result.push(typed(node, b"Limits", ObjectType::Array));
            return true;
        }
    }
    false
}
fn set_limits(array: &ObjectRef, left: &[u32], right: &[u32]) {
    let mut borrowed = array.borrow_mut();
    let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
    array.set_new_at(0, CpdfString::from_unicode(None, left));
    array.set_new_at(1, CpdfString::from_unicode(None, right));
}
fn unicode_at(array: &ObjectRef, index: usize) -> Vec<u32> {
    array_entry(array, index).map_or_else(Vec::new, |v| v.borrow().get_unicode_text())
}
fn update_after_deletion(node: &ObjectRef, find: &ObjectRef, name: &[u32], level: usize) -> bool {
    if level > 32 {
        return false;
    }
    let bounds = typed(node, b"Limits", ObjectType::Array);
    let (left, right) = bounds.as_ref().map_or_else(
        || (Vec::new(), Vec::new()),
        |bounds| {
            trim_limits(bounds);
            limits(bounds)
        },
    );
    if let Some(names) = typed(node, b"Names", ObjectType::Array) {
        if !Rc::ptr_eq(&names, find) {
            return false;
        }
        let Some(bounds) = bounds.filter(|_| size(&names) != 0) else {
            return true;
        };
        if left != name && right != name {
            return true;
        }
        let (mut new_left, mut new_right) = (right, left);
        for i in 0..size(&names) / 2 {
            let key = unicode_at(&names, i * 2);
            if key < new_left {
                new_left = key.clone();
            }
            if key > new_right {
                new_right = key;
            }
        }
        set_limits(&bounds, &new_left, &new_right);
        return true;
    }
    let Some(kids) = typed(node, b"Kids", ObjectType::Array) else {
        return false;
    };
    for i in 0..size(&kids) {
        let Some(child) =
            array_entry(&kids, i).filter(|v| v.borrow().object_type() == ObjectType::Dictionary)
        else {
            continue;
        };
        if !update_after_deletion(&child, find, name, level + 1) {
            continue;
        }
        let empty = typed(&child, b"Names", ObjectType::Array)
            .is_some_and(|names| size(&names) == 0)
            || typed(&child, b"Kids", ObjectType::Array).is_some_and(|kids| size(&kids) == 0);
        if empty {
            kids.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfArray>()
                .unwrap()
                .remove_at(i);
        }
        let Some(bounds) = bounds.filter(|_| size(&kids) != 0) else {
            return true;
        };
        if left != name && right != name {
            return true;
        }
        let (mut new_left, mut new_right) = (right, left);
        for j in 0..size(&kids) {
            let Some(child) = array_entry(&kids, j)
                .filter(|v| v.borrow().object_type() == ObjectType::Dictionary)
            else {
                continue;
            };
            let Some(child_limits) = typed(&child, b"Limits", ObjectType::Array) else {
                continue;
            };
            let child_left = unicode_at(&child_limits, 0);
            let child_right = unicode_at(&child_limits, 1);
            if child_left < new_left {
                new_left = child_left;
            }
            if child_right > new_right {
                new_right = child_right;
            }
        }
        set_limits(&bounds, &new_left, &new_right);
        return true;
    }
    false
}
fn dest_array(object: ObjectRef) -> Option<ObjectRef> {
    if object.borrow().object_type() == ObjectType::Array {
        Some(object)
    } else {
        typed(&object, b"D", ObjectType::Array)
    }
}
#[derive(Clone)]
pub struct CpdfNameTree {
    root: ObjectRef,
}
impl CpdfNameTree {
    pub fn create(document: &CpdfDocument, category: &[u8]) -> Option<Self> {
        Self::create_from_root(&document.root()?, category)
    }
    /// Snapshot the document's root before lookup so lazy input callbacks may
    /// re-enter its host handle without a surrounding document borrow.
    pub fn create_from_root(root: &ObjectRef, category: &[u8]) -> Option<Self> {
        let names = typed(root, b"Names", ObjectType::Dictionary)?;
        let root = typed(&names, category, ObjectType::Dictionary)?;
        Some(Self { root })
    }
    pub fn create_with_root_name_array(document: &CpdfDocument, category: &[u8]) -> Option<Self> {
        let root = document.root()?;
        let objects = document.holder();
        Self::create_with_root_name_array_from_parts(&root, &objects, category)
    }
    pub fn create_with_root_name_array_from_parts(
        root: &ObjectRef,
        objects: &HolderRef,
        category: &[u8],
    ) -> Option<Self> {
        let names = typed(root, b"Names", ObjectType::Dictionary).unwrap_or_else(|| {
            let names = objects.borrow_mut().new_indirect_dictionary();
            let number = names.borrow().obj_num();
            root.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(b"Names", CpdfReference::new(objects, number));
            names
        });
        let category = typed(&names, category, ObjectType::Dictionary).unwrap_or_else(|| {
            let value = objects.borrow_mut().new_indirect_dictionary();
            let number = value.borrow().obj_num();
            value
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(b"Names", CpdfArray::new());
            names
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(category, CpdfReference::new(objects, number));
            value
        });
        Some(Self { root: category })
    }
    pub fn root(&self) -> ObjectRef {
        self.root.clone()
    }
    pub fn count(&self) -> usize {
        count_names(&self.root, 0, &mut HashSet::new())
    }
    pub fn lookup_value_and_name(&self, index: usize) -> Option<(ObjectRef, Vec<u32>)> {
        let result = search_index(&self.root, index, 0, &mut 0)?;
        Some((result.value, result.key))
    }
    pub fn add_value_and_name(&self, object: ObjectRef, name: &[u32]) -> bool {
        let mut insert = NodeToInsert::default();
        // Removing the final nested leaf leaves an empty /Kids array. Restore
        // an empty leaf before insertion rather than searching for a first
        // entry that no longer exists. Do not collapse nonempty/deep subtrees.
        if typed(&self.root, b"Names", ObjectType::Array).is_none()
            && typed(&self.root, b"Kids", ObjectType::Array).is_some_and(|kids| size(&kids) == 0)
        {
            let mut borrowed = self.root.borrow_mut();
            let root = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            root.remove_for(b"Kids");
            root.remove_for(b"Limits");
            root.set_new_for(b"Names", CpdfArray::new());
        }
        if let Some(names) =
            typed(&self.root, b"Names", ObjectType::Array).filter(|names| size(names) == 0)
        {
            if typed(&self.root, b"Kids", ObjectType::Array).is_none() {
                insert.names = Some(names);
            }
        }
        if insert.names.is_none()
            && search(&self.root, name, 0, &mut HashSet::new(), Some(&mut insert)).is_some()
        {
            return false;
        }
        if insert.names.is_none() {
            let Some(found) = search_index(&self.root, 0, 0, &mut 0) else {
                return false;
            };
            insert.names = Some(found.container);
        }
        let names = insert.names.unwrap();
        let index = insert.preceding_pair.map_or(0, |i| (i + 1) * 2);
        {
            let mut borrowed = names.borrow_mut();
            let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
            array.insert_new_at(index, CpdfString::from_unicode(None, name));
            array.insert_at(index + 1, object);
        }
        let mut all_limits = Vec::new();
        ancestor_limits(&self.root, &names, 0, &mut all_limits);
        for bounds in all_limits.into_iter().flatten() {
            let left = unicode_at(&bounds, 0);
            let right = unicode_at(&bounds, 1);
            let mut borrowed = bounds.borrow_mut();
            let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
            if name < left.as_slice() {
                array.set_new_at(0, CpdfString::from_unicode(None, name));
            }
            if name > right.as_slice() {
                array.set_new_at(1, CpdfString::from_unicode(None, name));
            }
        }
        true
    }
    pub fn delete_value_and_name(&self, index: usize) -> bool {
        let Some(found) = search_index(&self.root, index, 0, &mut 0) else {
            return false;
        };
        {
            let mut borrowed = found.container.borrow_mut();
            let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
            array.remove_at(found.index + 1);
            array.remove_at(found.index);
        }
        update_after_deletion(&self.root, &found.container, &found.key, 0);
        true
    }
    pub fn create_for_testing(root: ObjectRef) -> Self {
        Self { root }
    }
    pub fn lookup_value(&self, name: &[u32]) -> Option<ObjectRef> {
        search(&self.root, name, 0, &mut HashSet::new(), None)
    }
    /// JavaScript already supplies Unicode. Preserve it for modern tree keys;
    /// decoding its UTF-8 bytes as PDFDocEncoding would select a different name.
    /// Legacy dictionary names remain byte keys, using the existing UTF-8 path.
    pub fn lookup_named_dest_unicode(document: &CpdfDocument, name: &[u32]) -> Option<ObjectRef> {
        if let Some(array) = Self::create(document, b"Dests")
            .and_then(|tree| tree.lookup_value(name))
            .and_then(dest_array)
        {
            return Some(array);
        }
        let old = typed(&document.root()?, b"Dests", ObjectType::Dictionary)?;
        dest_array(entry(&old, &crate::fxcrt::fx_string::utf8_encode(name))?)
    }
    pub fn lookup_named_dest(document: &CpdfDocument, name: &[u8]) -> Option<ObjectRef> {
        if let Some(array) = Self::create(document, b"Dests")
            .and_then(|tree| tree.lookup_value(&pdf_decode_text(name)))
            .and_then(dest_array)
        {
            return Some(array);
        }
        let old = typed(&document.root()?, b"Dests", ObjectType::Dictionary)?;
        dest_array(entry(&old, name)?)
    }
}
#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    #[test]
    fn inserting_after_deleting_the_last_nested_leaf_restores_an_empty_root() {
        let root = retain_object(CpdfDictionary::new());
        let kids = set_array(&root, b"Kids");
        let leaf = kids
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfDictionary::new());
        let names = set_array(&leaf, b"Names");
        append_name(&names, "old", 1);
        add_bounds(&root, "old", "old");
        add_bounds(&leaf, "old", "old");
        let tree = CpdfNameTree::create_for_testing(root.clone());
        assert!(tree.delete_value_and_name(0));
        assert_eq!(tree.count(), 0);
        assert_eq!(child_count(&root), 0);
        assert!(tree.add_value_and_name(
            retain_object(CpdfScalarObject::number_from_signed(2)),
            &wide("new")
        ));
        assert_eq!(tree.count(), 1);
        assert_eq!(
            tree.lookup_value(&wide("new"))
                .unwrap()
                .borrow()
                .get_integer(),
            2
        );
        assert!(typed(&root, b"Kids", ObjectType::Array).is_none());
        assert!(typed(&root, b"Limits", ObjectType::Array).is_none());
    }

    #[test]
    fn tree_lookup_repairs_reversed_limits_and_bounds_cycles() {
        let mut names = CpdfArray::new();
        names.append_new(CpdfString::from_bytes(None, b"key"));
        names.append_new(CpdfScalarObject::number_from_signed(7));
        let mut bounds = CpdfArray::new();
        bounds.append_new(CpdfString::from_bytes(None, b"z"));
        bounds.append_new(CpdfString::from_bytes(None, b"a"));
        let mut root = CpdfDictionary::new();
        root.set_new_for(b"Names", names);
        root.set_new_for(b"Limits", bounds);
        let root = retain_object(root);
        let tree = CpdfNameTree::create_for_testing(root.clone());
        assert_eq!(
            tree.lookup_value(&[107, 101, 121])
                .unwrap()
                .borrow()
                .get_integer(),
            7
        );
        assert!(tree.lookup_value(&[122, 122]).is_none());
        let bounds = typed(&root, b"Limits", ObjectType::Array).unwrap();
        assert_eq!(array_entry(&bounds, 0).unwrap().borrow().get_string(), b"a");
        let objects =
            crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder::new();
        let cycle = objects.borrow_mut().new_indirect_dictionary();
        let mut kids = CpdfArray::new();
        kids.append(crate::fpdfapi::parser::cpdf_object::make_reference(
            &cycle, &objects,
        ));
        cycle
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Kids", kids);
        assert!(CpdfNameTree::create_for_testing(cycle)
            .lookup_value(&[1])
            .is_none());
    }
    fn wide(name: &str) -> Vec<u32> {
        name.chars().map(u32::from).collect()
    }
    fn set_array(node: &ObjectRef, key: &[u8]) -> ObjectRef {
        node.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(key, CpdfArray::new())
    }
    fn append_name(names: &ObjectRef, name: &str, value: i32) {
        let mut borrowed = names.borrow_mut();
        let names = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        names.append_new(CpdfString::from_bytes(None, name.as_bytes()));
        names.append_new(CpdfScalarObject::number_from_signed(value));
    }
    fn add_bounds(node: &ObjectRef, left: &str, right: &str) {
        let bounds = set_array(node, b"Limits");
        let mut borrowed = bounds.borrow_mut();
        let bounds = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        bounds.append_new(CpdfString::from_bytes(None, left.as_bytes()));
        bounds.append_new(CpdfString::from_bytes(None, right.as_bytes()));
    }
    fn check_bounds(node: &ObjectRef, left: &str, right: &str) {
        let bounds = typed(node, b"Limits", ObjectType::Array).unwrap();
        assert!(size(&bounds) >= 2);
        for (i, key) in [left, right].into_iter().enumerate() {
            let string = array_entry(&bounds, i).unwrap();
            assert_eq!(string.borrow().object_type(), ObjectType::String);
            assert_eq!(string.borrow().get_string(), key.as_bytes());
        }
    }
    fn check_pair(names: &ObjectRef, index: usize, key: &str, value: i32) {
        assert_eq!(
            array_entry(names, index * 2).unwrap().borrow().get_string(),
            key.as_bytes()
        );
        assert_eq!(
            array_entry(names, index * 2 + 1)
                .unwrap()
                .borrow()
                .get_integer(),
            value
        );
    }
    fn child(node: &ObjectRef, index: usize) -> ObjectRef {
        array_entry(&typed(node, b"Kids", ObjectType::Array).unwrap(), index).unwrap()
    }
    fn child_count(node: &ObjectRef) -> usize {
        size(&typed(node, b"Kids", ObjectType::Array).unwrap())
    }
    fn fill_tree(root: &ObjectRef) {
        let root_kids = set_array(root, b"Kids");
        let k1 = root_kids
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfDictionary::new());
        add_bounds(&k1, "9.txt", "1.txt");
        let kids = set_array(&k1, b"Kids");
        let g2 = kids
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfDictionary::new());
        let g3 = kids
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfDictionary::new());
        add_bounds(&g2, "1.txt", "5.txt");
        let descendants = set_array(&g2, b"Kids");
        let g4 = descendants
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfDictionary::new());
        let g5 = descendants
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfDictionary::new());
        add_bounds(&g3, "9.txt", "9.txt");
        let names = set_array(&g3, b"Names");
        append_name(&names, "9.txt", 999);
        add_bounds(&g4, "2.txt", "1.txt");
        let names = set_array(&g4, b"Names");
        append_name(&names, "1.txt", 111);
        append_name(&names, "2.txt", 222);
        add_bounds(&g5, "3.txt", "5.txt");
        let names = set_array(&g5, b"Names");
        append_name(&names, "3.txt", 333);
        append_name(&names, "5.txt", 555);
    }
    #[test]
    fn GetUnicodeNameWithBOM() {
        let root = retain_object(CpdfDictionary::new());
        let names = set_array(&root, b"Names");
        {
            let mut borrowed = names.borrow_mut();
            let names = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
            names.append_new(CpdfString::from_hex_bytes(None, &[0xfe, 0xff, 0, 0x31]));
            names.append_new(CpdfScalarObject::number_from_signed(100));
        }
        let tree = CpdfNameTree::create_for_testing(root);
        assert_eq!(tree.lookup_value_and_name(0).unwrap().1, wide("1"));
        let number = tree.lookup_value(&wide("1")).unwrap();
        assert_eq!(number.borrow().object_type(), ObjectType::Number);
        assert_eq!(number.borrow().get_integer(), 100);
    }
    #[test]
    fn GetFromTreeWithLimitsArrayWith4Items() {
        let root = retain_object(CpdfDictionary::new());
        fill_tree(&root);
        let k1 = child(&root, 0);
        let g2 = child(&k1, 0);
        let g3 = child(&k1, 1);
        let bounds2 = typed(&g2, b"Limits", ObjectType::Array).unwrap();
        let bounds3 = typed(&g3, b"Limits", ObjectType::Array).unwrap();
        for bounds in [&bounds2, &bounds3] {
            assert_eq!(size(bounds), 2);
            let mut borrowed = bounds.borrow_mut();
            let a = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
            a.append_new(CpdfScalarObject::number_from_signed(5));
            a.append_new(CpdfScalarObject::number_from_signed(6));
            assert_eq!(a.size(), 4);
        }
        let tree = CpdfNameTree::create_for_testing(root);
        assert_eq!(
            tree.lookup_value(&wide("9.txt"))
                .unwrap()
                .borrow()
                .get_integer(),
            999
        );
        assert_eq!(size(&bounds3), 4);
        assert_eq!(
            tree.lookup_value(&wide("2.txt"))
                .unwrap()
                .borrow()
                .get_integer(),
            222
        );
        assert_eq!(size(&bounds2), 4);
        assert!(tree.add_value_and_name(
            retain_object(CpdfScalarObject::number_from_signed(150)),
            &wide("1a.txt")
        ));
        assert_eq!(size(&bounds2), 2);
        assert_eq!(size(&bounds3), 2);
    }
    #[test]
    fn AddIntoNames() {
        let root = retain_object(CpdfDictionary::new());
        let names = set_array(&root, b"Names");
        append_name(&names, "2.txt", 222);
        append_name(&names, "7.txt", 777);
        let tree = CpdfNameTree::create_for_testing(root);
        assert!(!tree.add_value_and_name(
            retain_object(CpdfScalarObject::number_from_signed(111)),
            &wide("2.txt")
        ));
        for (key, value) in [("1.txt", 111), ("5.txt", 555), ("9.txt", 999)] {
            assert!(tree.add_value_and_name(
                retain_object(CpdfScalarObject::number_from_signed(value)),
                &wide(key)
            ));
        }
        for (index, (key, value)) in [
            ("1.txt", 111),
            ("2.txt", 222),
            ("5.txt", 555),
            ("7.txt", 777),
            ("9.txt", 999),
        ]
        .into_iter()
        .enumerate()
        {
            check_pair(&names, index, key, value);
        }
    }
    #[test]
    fn AddIntoEmptyNames() {
        let root = retain_object(CpdfDictionary::new());
        let names = set_array(&root, b"Names");
        let tree = CpdfNameTree::create_for_testing(root);
        assert!(tree.add_value_and_name(
            retain_object(CpdfScalarObject::number_from_signed(111)),
            &wide("2.txt")
        ));
        assert!(!tree.add_value_and_name(
            retain_object(CpdfScalarObject::number_from_signed(111)),
            &wide("2.txt")
        ));
        for (key, value) in [("1.txt", 111), ("5.txt", 555), ("9.txt", 999)] {
            assert!(tree.add_value_and_name(
                retain_object(CpdfScalarObject::number_from_signed(value)),
                &wide(key)
            ));
        }
        for (index, (key, value)) in [
            ("1.txt", 111),
            ("2.txt", 111),
            ("5.txt", 555),
            ("9.txt", 999),
        ]
        .into_iter()
        .enumerate()
        {
            check_pair(&names, index, key, value);
        }
    }
    #[test]
    fn AddIntoKids() {
        let root = retain_object(CpdfDictionary::new());
        fill_tree(&root);
        let tree = CpdfNameTree::create_for_testing(root.clone());
        assert!(!tree.add_value_and_name(
            retain_object(CpdfScalarObject::number_from_signed(444)),
            &wide("9.txt")
        ));
        for (key, value) in [
            ("4.txt", 444),
            ("6.txt", 666),
            ("99.txt", 99),
            ("0.txt", -5),
        ] {
            assert!(tree.add_value_and_name(
                retain_object(CpdfScalarObject::number_from_signed(value)),
                &wide(key)
            ));
            assert_eq!(
                tree.lookup_value(&wide(key))
                    .unwrap()
                    .borrow()
                    .get_integer(),
                value
            );
        }
        let k1 = child(&root, 0);
        check_bounds(&k1, "0.txt", "99.txt");
        let g2 = child(&k1, 0);
        check_bounds(&g2, "0.txt", "6.txt");
        let g3 = child(&k1, 1);
        check_bounds(&g3, "9.txt", "99.txt");
        let names = typed(&g3, b"Names", ObjectType::Array).unwrap();
        check_pair(&names, 0, "9.txt", 999);
        check_pair(&names, 1, "99.txt", 99);
        let g4 = child(&g2, 0);
        check_bounds(&g4, "0.txt", "2.txt");
        let names = typed(&g4, b"Names", ObjectType::Array).unwrap();
        for (index, (key, value)) in [("0.txt", -5), ("1.txt", 111), ("2.txt", 222)]
            .into_iter()
            .enumerate()
        {
            check_pair(&names, index, key, value);
        }
        let g5 = child(&g2, 1);
        check_bounds(&g5, "3.txt", "6.txt");
        let names = typed(&g5, b"Names", ObjectType::Array).unwrap();
        for (index, (key, value)) in [
            ("3.txt", 333),
            ("4.txt", 444),
            ("5.txt", 555),
            ("6.txt", 666),
        ]
        .into_iter()
        .enumerate()
        {
            check_pair(&names, index, key, value);
        }
    }
    #[test]
    fn DeleteFromKids() {
        let root = retain_object(CpdfDictionary::new());
        fill_tree(&root);
        let tree = CpdfNameTree::create_for_testing(root.clone());
        let k1 = child(&root, 0);
        let g2 = child(&k1, 0);
        let g3 = child(&k1, 1);
        let g4 = child(&g2, 0);
        let g5 = child(&g2, 1);
        assert!(!tree.delete_value_and_name(5));
        for (name, value, index) in [
            ("9.txt", 999, 4),
            ("2.txt", 222, 1),
            ("1.txt", 111, 0),
            ("3.txt", 333, 0),
            ("5.txt", 555, 0),
        ] {
            assert_eq!(
                tree.lookup_value(&wide(name))
                    .unwrap()
                    .borrow()
                    .get_integer(),
                value
            );
            assert_eq!(tree.lookup_value_and_name(index).unwrap().1, wide(name));
            match name {
                "9.txt" => assert_eq!(child_count(&k1), 2),
                "2.txt" => assert_eq!(size(&typed(&g4, b"Names", ObjectType::Array).unwrap()), 4),
                "1.txt" => assert_eq!(child_count(&g2), 2),
                "3.txt" => assert_eq!(size(&typed(&g5, b"Names", ObjectType::Array).unwrap()), 4),
                "5.txt" => assert_eq!(child_count(&root), 1),
                _ => (),
            }
            assert!(tree.delete_value_and_name(index));
            match name {
                "9.txt" => {
                    assert_eq!(child_count(&k1), 1);
                    check_bounds(&k1, "1.txt", "5.txt");
                }
                "2.txt" => {
                    assert_eq!(size(&typed(&g4, b"Names", ObjectType::Array).unwrap()), 2);
                    check_bounds(&g4, "1.txt", "1.txt");
                    check_bounds(&g2, "1.txt", "5.txt");
                    check_bounds(&k1, "1.txt", "5.txt");
                }
                "1.txt" => {
                    assert_eq!(child_count(&g2), 1);
                    check_bounds(&g2, "3.txt", "5.txt");
                    check_bounds(&k1, "3.txt", "5.txt");
                }
                "3.txt" => {
                    assert_eq!(size(&typed(&g5, b"Names", ObjectType::Array).unwrap()), 2);
                    check_bounds(&g5, "5.txt", "5.txt");
                    check_bounds(&g2, "5.txt", "5.txt");
                    check_bounds(&k1, "5.txt", "5.txt");
                }
                _ => (),
            }
        }
        assert_eq!(tree.count(), 0);
        assert!(tree.lookup_value_and_name(0).is_none());
        assert!(!tree.delete_value_and_name(0));
        assert_eq!(child_count(&root), 0);
        assert_eq!(child_count(&k1), 0);
        assert_eq!(child_count(&g2), 0);
        assert_eq!(size(&typed(&g3, b"Names", ObjectType::Array).unwrap()), 0);
        assert_eq!(size(&typed(&g4, b"Names", ObjectType::Array).unwrap()), 0);
        assert_eq!(size(&typed(&g5, b"Names", ObjectType::Array).unwrap()), 0);
    }
}
