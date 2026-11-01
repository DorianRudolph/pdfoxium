// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/edit/cpdf_pagecontentmanager.{h,cpp}`.
//! Scoped borrows replace the unowned page/document pointers; destruction still
//! executes deferred removals. Stream arguments retain the existing Rust
//! `(bytes, position)` representation of an `ostringstream` and its `tellp()`.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::fpdfapi::page::cpdf_pageobjectholder::CpdfPageObjectHolder;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::object_tree_traversal_util::get_objects_with_multiple_references;

enum Contents {
    Stream(Option<ObjectRef>),
    Array(ObjectRef),
}

pub struct CpdfPageContentManager<'page, 'doc> {
    page_obj_holder: &'page mut CpdfPageObjectHolder,
    document: &'doc CpdfDocument,
    objects_with_multi_refs: BTreeSet<u32>,
    contents: Contents,
    streams_to_remove: BTreeSet<usize>,
}

fn array<R>(object: &ObjectRef, f: impl FnOnce(&CpdfArray) -> R) -> R {
    let object = object.borrow();
    f(object.as_any().downcast_ref::<CpdfArray>().unwrap())
}

fn array_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfArray) -> R) -> R {
    let mut object = object.borrow_mut();
    f(object.as_any_mut().downcast_mut::<CpdfArray>().unwrap())
}

fn reference_target(object: &ObjectRef) -> Option<ObjectRef> {
    let object = object.borrow();
    object
        .as_any()
        .downcast_ref::<CpdfReference>()?
        .get_direct()
}

impl<'page, 'doc> CpdfPageContentManager<'page, 'doc> {
    pub fn new(
        page_obj_holder: &'page mut CpdfPageObjectHolder,
        document: &'doc CpdfDocument,
    ) -> Self {
        let mut manager = Self {
            page_obj_holder,
            document,
            objects_with_multi_refs: get_objects_with_multiple_references(document),
            contents: Contents::Stream(None),
            streams_to_remove: BTreeSet::new(),
        };
        let contents_object = {
            let dictionary = manager.page_obj_holder.dict().borrow();
            dictionary
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_object_for(b"Contents")
        };
        let Some(contents_object) = contents_object else {
            return manager;
        };
        if contents_object.borrow().object_type() == ObjectType::Array {
            assert!(contents_object.borrow().is_inline());
            manager.contents = Contents::Array(contents_object);
            return manager;
        }
        if let Some(indirect_object) = reference_target(&contents_object) {
            let object_type = indirect_object.borrow().object_type();
            if object_type == ObjectType::Array {
                let number = indirect_object.borrow().obj_num();
                if manager.objects_with_multi_refs.contains(&number) {
                    // This clone is upstream's copy-on-write operation for a
                    // shared content array, not a borrow-checker workaround.
                    let cloned_array = indirect_object.borrow().clone_object();
                    // Cloning this array creates a second reference to every
                    // child stream. The traversal snapshot only visited the
                    // original array once, so those streams may not yet be
                    // marked shared. Include them before replacing any data.
                    array(&cloned_array, |contents| {
                        for index in 0..contents.size() {
                            if let Some(stream) = contents.get_stream_at(index) {
                                manager
                                    .objects_with_multi_refs
                                    .insert(stream.borrow().obj_num());
                            }
                        }
                    });
                    manager.set_page_contents(Some(Rc::clone(&cloned_array)));
                    manager.contents = Contents::Array(cloned_array);
                } else {
                    manager.contents = Contents::Array(indirect_object);
                }
            } else if object_type == ObjectType::Stream {
                manager.contents = Contents::Stream(Some(indirect_object));
            }
        }
        manager
    }

    pub(crate) fn page_object_holder_mut(&mut self) -> &mut CpdfPageObjectHolder {
        self.page_obj_holder
    }

    pub fn has_stream_at_index(&self, stream_index: usize) -> bool {
        self.get_stream_by_index(stream_index).is_some()
    }

    fn get_stream_by_index(&self, stream_index: usize) -> Option<ObjectRef> {
        if let Some(contents_stream) = self.get_contents_stream() {
            return (stream_index == 0).then_some(contents_stream);
        }
        let contents_array = self.get_contents_array()?;
        let stream_reference = array(&contents_array, |array| array.get_object_at(stream_index))?;
        let stream = reference_target(&stream_reference)?;
        let is_stream = stream.borrow().object_type() == ObjectType::Stream;
        is_stream.then_some(stream)
    }

    fn new_stream(&self, buffer: &[u8], position: i64) -> ObjectRef {
        let data = if position <= 0 {
            &[]
        } else {
            &buffer[..usize::try_from(position).expect("stringstream position")]
        };
        self.document
            .holder()
            .borrow_mut()
            .new_indirect_stream_from_bytes(data)
    }

    fn set_page_contents(&self, contents: Option<ObjectRef>) {
        self.page_obj_holder
            .dict()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"Contents", contents);
    }

    fn set_page_contents_reference(&self, number: u32) {
        let document_objects = self.document.holder();
        self.page_obj_holder
            .dict()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Contents", CpdfReference::new(&document_objects, number));
    }

    /// Add a content stream and return its array index, or zero when it is the
    /// page's sole stream. `position` is the native output stream's `tellp()`.
    pub fn add_stream(&mut self, buffer: &[u8], position: i64) -> usize {
        let new_stream = self.new_stream(buffer, position);
        let document_objects = self.document.holder();
        let new_stream_number = new_stream.borrow().obj_num();
        if let Some(contents_stream) = self.get_contents_stream() {
            // Convert the previous sole stream into an indirect two-entry array.
            let new_contents_array = document_objects.borrow_mut().new_indirect_array();
            array_mut(&new_contents_array, |array| {
                array.append_new(CpdfReference::new(
                    &document_objects,
                    contents_stream.borrow().obj_num(),
                ));
                array.append_new(CpdfReference::new(&document_objects, new_stream_number));
            });
            self.set_page_contents_reference(new_contents_array.borrow().obj_num());
            self.contents = Contents::Array(new_contents_array);
            return 1;
        }
        if let Some(contents_array) = self.get_contents_array() {
            return array_mut(&contents_array, |array| {
                array.append_new(CpdfReference::new(&document_objects, new_stream_number));
                array.size() - 1
            });
        }
        self.set_page_contents_reference(new_stream_number);
        self.contents = Contents::Stream(Some(new_stream));
        0
    }

    /// Replace a stream's data, or defer removing it when `position <= 0`.
    pub fn update_stream(&mut self, stream_index: usize, buffer: &[u8], position: i64) {
        if position <= 0 {
            self.streams_to_remove.insert(stream_index);
            return;
        }
        let existing_stream = self
            .get_stream_by_index(stream_index)
            .expect("UpdateStream requires an existing content stream");
        let existing_number = existing_stream.borrow().obj_num();
        if !self.objects_with_multi_refs.contains(&existing_number) {
            existing_stream
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfStream>()
                .unwrap()
                .set_data_from_stringstream_and_remove_filter(buffer, position);
            return;
        }
        if self.get_contents_stream().is_some() {
            let new_stream = self.new_stream(buffer, position);
            self.set_page_contents_reference(new_stream.borrow().obj_num());
            // Preserve the source's cached contents_: subsequent operations
            // still observe the old sole stream within this manager's lifetime.
        }
        let Some(contents_array) = self.get_contents_array() else {
            return;
        };
        let Some(stream_reference) =
            array(&contents_array, |array| array.get_object_at(stream_index))
        else {
            return;
        };
        if stream_reference.borrow().object_type() != ObjectType::Reference {
            return;
        }
        let new_stream = self.new_stream(buffer, position);
        stream_reference
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfReference>()
            .unwrap()
            .set_ref(&self.document.holder(), new_stream.borrow().obj_num());
    }

    fn execute_scheduled_removals(&mut self) {
        // GenerateContent cleans the holder's dirty streams before creating us.
        // Active status is intentionally irrelevant to index remapping.
        debug_assert!(!self.page_obj_holder.has_dirty_streams());
        if self.streams_to_remove.is_empty() {
            return;
        }
        if self.get_contents_stream().is_some() {
            if self.streams_to_remove.contains(&0) {
                self.page_obj_holder
                    .dict()
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<CpdfDictionary>()
                    .unwrap()
                    .remove_for(b"Contents");
            }
            return;
        }
        let Some(contents_array) = self.get_contents_array() else {
            return;
        };
        let mut streams_left: Vec<usize> = (0..array(&contents_array, CpdfArray::size)).collect();
        for &stream_index in self.streams_to_remove.iter().rev() {
            array_mut(&contents_array, |array| array.remove_at(stream_index));
            // Native vector::erase requires a valid scheduled stream index.
            streams_left.remove(stream_index);
        }
        let mut stream_index_mapping: BTreeMap<usize, usize> = streams_left
            .into_iter()
            .enumerate()
            .map(|(new_index, old_index)| (old_index, new_index))
            .collect();
        for index in 0..self.page_obj_holder.page_object_count() {
            let object = self
                .page_obj_holder
                .page_object_by_index_mut(index)
                .unwrap();
            let old_stream_index = object.base().content_stream();
            // C++ map<size_t, size_t>::operator[] default-inserts zero, including
            // for removed stream indexes and a converted kNoContentStream.
            let new_index = *stream_index_mapping
                .entry(old_stream_index as usize)
                .or_default();
            object
                .base_mut()
                .set_content_stream(i32::try_from(new_index).expect("content stream index"));
        }
        // Keep an array even when only one stream remains, exactly upstream.
    }

    fn get_contents_stream(&self) -> Option<ObjectRef> {
        match &self.contents {
            Contents::Stream(stream) => stream.clone(),
            Contents::Array(_) => None,
        }
    }

    fn get_contents_array(&self) -> Option<ObjectRef> {
        match &self.contents {
            Contents::Array(array) => Some(Rc::clone(array)),
            Contents::Stream(_) => None,
        }
    }
}

impl Drop for CpdfPageContentManager<'_, '_> {
    fn drop(&mut self) {
        self.execute_scheduled_removals();
    }
}

#[cfg(test)]
#[path = "cpdf_pagecontentmanager_tests.rs"]
mod tests;
