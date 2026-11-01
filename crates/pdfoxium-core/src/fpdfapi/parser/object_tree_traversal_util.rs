// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/parser/object_tree_traversal_util.{h,cpp}`.
//! Retained handles replace RetainPtr; object addresses are identity keys only.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

use super::cpdf_array::CpdfArray;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_document::CpdfDocument;
use super::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use super::cpdf_object::ObjectRef;
use super::cpdf_reference::CpdfReference;
use super::cpdf_stream::CpdfStream;

fn identity(object: &ObjectRef) -> usize {
    Rc::as_ptr(object) as *const () as usize
}

struct ReferenceEntry {
    ref_object_number: u32,
    referenced_object_number: u32,
}

/// Construct under a short document borrow, then traverse after releasing it.
/// Lazy indirect-object reads may call back into the document.
pub(crate) struct ObjectTreeTraverser {
    holder: HolderRef,
    // A retained object enters this queue at most once.
    object_queue: VecDeque<ObjectRef>,
    // Maps inline objects to the object number of their top-level ancestor.
    object_number_map: BTreeMap<usize, u32>,
    seen_objects: BTreeSet<usize>,
    referenced_objects: BTreeMap<u32, i32>,
}

impl ObjectTreeTraverser {
    pub(crate) fn new(document: &CpdfDocument) -> Self {
        let trailer = document.parser().trailer();
        let root = trailer
            .clone()
            .or_else(|| document.root())
            .expect("document has a catalog");
        let root_object_number = if trailer.is_some() {
            document.parser().trailer_object_number()
        } else {
            root.borrow().obj_num()
        };
        let root_identity = identity(&root);
        let mut traverser = Self {
            holder: document.holder(),
            object_queue: VecDeque::from([root]),
            object_number_map: BTreeMap::new(),
            seen_objects: BTreeSet::from([root_identity]),
            referenced_objects: BTreeMap::new(),
        };
        // Inlined trailers may have no object number.
        if root_object_number != 0 {
            traverser.referenced_objects.insert(root_object_number, 1);
            traverser
                .object_number_map
                .insert(root_identity, root_object_number);
        }
        traverser
    }

    pub(crate) fn get_objects_with_references(mut self) -> BTreeSet<u32> {
        self.traverse();
        self.referenced_objects.into_keys().collect()
    }

    fn traverse(&mut self) {
        let reference_entries = self.get_reference_entries();
        self.calculate_reference_counts(&reference_entries);
    }

    fn get_reference_entries(&mut self) -> Vec<ReferenceEntry> {
        let mut reference_entries = Vec::new();
        while let Some(current_object) = self.object_queue.pop_front() {
            let object = current_object.borrow();
            if let Some(array) = object.as_any().downcast_ref::<CpdfArray>() {
                array.with_locked_objects(|objects| {
                    for child in objects {
                        self.push_new_object(&current_object, Rc::clone(child));
                    }
                });
            } else if let Some(dict) = object.as_any().downcast_ref::<CpdfDictionary>() {
                dict.with_locked_entries(|entries| {
                    for child in entries.values() {
                        self.push_new_object(&current_object, Rc::clone(child));
                    }
                });
            } else if let Some(reference) = object.as_any().downcast_ref::<CpdfReference>() {
                let ref_object_number = self.get_object_number(&current_object);
                let referenced_object_number = reference.ref_obj_num();
                let holder = reference.indirect_object_holder();
                // GetDirect may invoke an input callback. The reference's
                // holder and number are evaluated before that call in native.
                drop(object);
                let referenced_object = if let Some(holder) = holder {
                    CpdfIndirectObjectHolder::get_or_parse_indirect_object(
                        &holder,
                        referenced_object_number,
                    )
                } else {
                    // Trailer references use already loaded indirect objects.
                    self.holder
                        .borrow()
                        .get_indirect_object(referenced_object_number)
                };
                // References, unlike container entries, may resolve to null.
                if let Some(referenced_object) = referenced_object {
                    assert_ne!(referenced_object_number, 0);
                    reference_entries.push(ReferenceEntry {
                        ref_object_number,
                        referenced_object_number,
                    });
                    self.push_new_object(&current_object, referenced_object);
                }
            } else if let Some(stream) = object.as_any().downcast_ref::<CpdfStream>() {
                let dict = stream.get_dict();
                let borrowed = dict.borrow();
                assert!(borrowed.is_inline());
                let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
                dict.with_locked_entries(|entries| {
                    for child in entries.values() {
                        self.push_new_object(&current_object, Rc::clone(child));
                    }
                });
            }
        }
        reference_entries
    }

    fn calculate_reference_counts(&mut self, reference_entries: &[ReferenceEntry]) {
        // PDF objects that referenced another PDF object, identified by their
        // nonzero object number. Preserve upstream's ordered cycle heuristic.
        let mut seen_ref_objects = BTreeSet::new();
        for entry in reference_entries {
            if entry.referenced_object_number == entry.ref_object_number {
                continue;
            }
            if seen_ref_objects.contains(&entry.ref_object_number)
                && seen_ref_objects.contains(&entry.referenced_object_number)
            {
                continue;
            }
            *self
                .referenced_objects
                .entry(entry.referenced_object_number)
                .or_default() += 1;
            if entry.ref_object_number != 0 {
                seen_ref_objects.insert(entry.ref_object_number);
            }
        }
    }

    fn push_new_object(&mut self, parent_object: &ObjectRef, child_object: ObjectRef) {
        let child_identity = identity(&child_object);
        if !self.seen_objects.insert(child_identity) {
            return;
        }
        let child_object_number = child_object.borrow().obj_num();
        if child_object_number != 0 {
            self.object_number_map
                .insert(child_identity, child_object_number);
        } else if let Some(&parent_number) = self.object_number_map.get(&identity(parent_object)) {
            // No ancestor number may be available for an inlined trailer.
            self.object_number_map.insert(child_identity, parent_number);
        }
        self.object_queue.push_back(child_object);
    }

    fn get_object_number(&self, object: &ObjectRef) -> u32 {
        self.object_number_map
            .get(&identity(object))
            .copied()
            .unwrap_or(0)
    }
}

/// Traverse from the trailer, or the catalog when no trailer exists, avoiding
/// cycles. Return the numbers of all referenced PDF objects.
pub fn get_objects_with_references(document: &CpdfDocument) -> BTreeSet<u32> {
    ObjectTreeTraverser::new(document).get_objects_with_references()
}

/// Return multiply referenced objects, ignoring references that create cycles
/// according to PDFium's traversal order and reference-count algorithm.
pub fn get_objects_with_multiple_references(document: &CpdfDocument) -> BTreeSet<u32> {
    let mut traverser = ObjectTreeTraverser::new(document);
    traverser.traverse();
    traverser
        .referenced_objects
        .into_iter()
        .filter_map(|(number, count)| (count > 1).then_some(number))
        .collect()
}

#[cfg(test)]
#[path = "object_tree_traversal_util_embeddertest.rs"]
mod upstream_tests;
