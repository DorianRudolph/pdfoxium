// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Number-tree exact/floor lookup from `core/fpdfdoc/cpdf_numbertree.{h,cpp}`.
use crate::fpdfapi::parser::{
    cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_object::ObjectRef,
};
use std::{collections::HashSet, rc::Rc};
pub struct KeyValue {
    pub key: i32,
    pub value: Option<ObjectRef>,
}
pub struct CpdfNumberTree {
    root: ObjectRef,
}
impl CpdfNumberTree {
    pub fn new(root: ObjectRef) -> Self {
        Self { root }
    }
    pub fn lookup_value(&self, num: i32) -> Option<ObjectRef> {
        find_number_node(&self.root, num, &mut HashSet::new(), 0)
    }
    pub fn get_lower_bound(&self, num: i32) -> Option<KeyValue> {
        find_lower_bound(&self.root, num, &mut HashSet::new(), 0)
    }
}
fn array(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_array_for(key)
}
fn array_size(object: &ObjectRef) -> usize {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .map_or(0, CpdfArray::size)
}
fn integer(object: &ObjectRef, index: usize) -> i32 {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .map_or(0, |a| a.get_integer_at(index))
}
fn direct(object: &ObjectRef, index: usize) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()?
        .get_direct_object_at(index)
}
fn dict_at(object: &ObjectRef, index: usize) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()?
        .get_dict_at(index)
}
fn enter(node: &ObjectRef, seen: &mut HashSet<usize>, depth: usize) -> bool {
    // Malformed cycles/deep trees must not overflow the host stack. Legitimate
    // tree lookups retain the native limits pruning and traversal order.
    depth < 128 && seen.insert(Rc::as_ptr(node) as *const () as usize)
}
fn find_number_node(
    node: &ObjectRef,
    num: i32,
    seen: &mut HashSet<usize>,
    depth: usize,
) -> Option<ObjectRef> {
    if !enter(node, seen, depth) {
        return None;
    }
    if let Some(limits) = array(node, b"Limits") {
        if num < integer(&limits, 0) || num > integer(&limits, 1) {
            return None;
        }
    }
    if let Some(numbers) = array(node, b"Nums") {
        for i in 0..array_size(&numbers) / 2 {
            let key = integer(&numbers, i * 2);
            if num == key {
                return direct(&numbers, i * 2 + 1);
            }
            if key > num {
                break;
            }
        }
        return None;
    }
    let kids = array(node, b"Kids")?;
    for i in 0..array_size(&kids) {
        if let Some(kid) = dict_at(&kids, i) {
            if let Some(value) = find_number_node(&kid, num, seen, depth + 1) {
                return Some(value);
            }
        }
    }
    None
}
fn find_lower_bound(
    node: &ObjectRef,
    num: i32,
    seen: &mut HashSet<usize>,
    depth: usize,
) -> Option<KeyValue> {
    if !enter(node, seen, depth) {
        return None;
    }
    if let Some(limits) = array(node, b"Limits") {
        if num < integer(&limits, 0) {
            return None;
        }
        let max = integer(&limits, 1);
        if num >= max {
            return Some(KeyValue {
                key: max,
                value: find_number_node(node, max, &mut HashSet::new(), 0),
            });
        }
    }
    if let Some(numbers) = array(node, b"Nums") {
        for i in (0..array_size(&numbers) / 2).rev() {
            let key = integer(&numbers, i * 2);
            if num >= key {
                return Some(KeyValue {
                    key,
                    value: direct(&numbers, i * 2 + 1),
                });
            }
        }
        return None;
    }
    let kids = array(node, b"Kids")?;
    for i in (0..array_size(&kids)).rev() {
        if let Some(kid) = dict_at(&kids, i) {
            if let Some(result) = find_lower_bound(&kid, num, seen, depth + 1) {
                return Some(result);
            }
        }
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{
        cpdf_object::retain_object, cpdf_scalar_object::CpdfScalarObject,
    };
    #[test]
    fn number_tree_exact_and_floor_lookup_preserve_limits_and_pair_order() {
        let mut nums = CpdfArray::new();
        for n in [2, 20, 7, 70, 11, 110] {
            nums.append_new(CpdfScalarObject::number_from_signed(n));
        }
        let mut node = CpdfDictionary::new();
        node.set_new_for(b"Nums", nums);
        let tree = CpdfNumberTree::new(retain_object(node));
        for n in -1..15 {
            let expected = [2, 7, 11].into_iter().find(|&k| k == n).map(|k| k * 10);
            assert_eq!(
                tree.lookup_value(n).map(|v| v.borrow().get_integer()),
                expected
            );
            let expected = [2, 7, 11].into_iter().filter(|&k| k <= n).next_back();
            assert_eq!(
                tree.get_lower_bound(n)
                    .map(|v| (v.key, v.value.unwrap().borrow().get_integer())),
                expected.map(|k| (k, k * 10))
            );
        }
    }
    #[test]
    fn malformed_number_tree_cycles_are_bounded() {
        let node = retain_object(CpdfDictionary::new());
        let mut kids = CpdfArray::new();
        kids.append(node.clone());
        node.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Kids", kids);
        let tree = CpdfNumberTree::new(node.clone());
        assert!(tree.lookup_value(3).is_none());
        assert!(tree.get_lower_bound(3).is_none());
        node.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .remove_for(b"Kids");
    }
}
