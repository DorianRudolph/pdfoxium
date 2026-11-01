// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/parser/cpdf_object_walker.{h,cpp}`. Frames retain direct
//! child handles so iteration can outlive a `RefCell` borrow of each parent.

use super::cpdf_array::CpdfArray;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_object::ObjectRef;
use super::cpdf_stream::CpdfStream;

struct Frame {
    object: ObjectRef,
    children: Vec<(Vec<u8>, ObjectRef)>,
    index: usize,
    started: bool,
}

impl Drop for Frame {
    fn drop(&mut self) {
        let borrowed = self.object.borrow();
        if let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() {
            dict.unlock_for_walker();
        } else if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
            array.unlock_for_walker();
        }
    }
}

impl Frame {
    fn new(object: ObjectRef) -> Option<Self> {
        let borrowed = object.borrow();
        let children = if let Some(stream) = borrowed.as_any().downcast_ref::<CpdfStream>() {
            vec![(Vec::new(), stream.get_dict())]
        } else if let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() {
            dict.lock_for_walker();
            dict.with_locked_entries(|entries| {
                entries
                    .iter()
                    .map(|(key, value)| (key.to_vec(), value.clone()))
                    .collect()
            })
        } else if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
            array.lock_for_walker();
            array.with_locked_objects(|objects| {
                objects
                    .iter()
                    .map(|value| (Vec::new(), value.clone()))
                    .collect()
            })
        } else {
            return None;
        };
        drop(borrowed);
        Some(Self {
            object,
            children,
            index: 0,
            started: false,
        })
    }
}

pub struct CpdfObjectWalker {
    next_object: Option<ObjectRef>,
    parent_object: Option<ObjectRef>,
    dict_key: Vec<u8>,
    current_depth: usize,
    stack: Vec<Frame>,
}

impl CpdfObjectWalker {
    pub fn new(root: ObjectRef) -> Self {
        Self {
            next_object: Some(root),
            parent_object: None,
            dict_key: Vec::new(),
            current_depth: 0,
            stack: Vec::new(),
        }
    }

    pub fn get_next(&mut self) -> Option<ObjectRef> {
        loop {
            if let Some(object) = self.next_object.take() {
                if let Some(frame) = Frame::new(object.clone()) {
                    self.stack.push(frame);
                }
                return Some(object);
            }
            let depth = self.stack.len();
            let Some(frame) = self.stack.last_mut() else {
                self.dict_key.clear();
                self.current_depth = 0;
                return None;
            };
            frame.started = true;
            if let Some((key, child)) = frame.children.get(frame.index) {
                self.next_object = Some(child.clone());
                self.parent_object = Some(frame.object.clone());
                self.dict_key = key.clone();
                self.current_depth = depth;
                frame.index += 1;
            } else {
                self.stack.pop();
            }
        }
    }

    pub fn skip_walk_into_current_object(&mut self) {
        if self.stack.last().is_some_and(|frame| !frame.started) {
            self.stack.pop();
        }
    }

    pub fn current_depth(&self) -> usize {
        self.current_depth
    }
    pub fn parent(&self) -> Option<ObjectRef> {
        self.parent_object.clone()
    }
    pub fn dictionary_key(&self) -> &[u8] {
        &self.dict_key
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfObjectWalker;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn walks_depth_first_and_skips_subtree() {
        let mut array = CpdfArray::new();
        array.append_new(CpdfScalarObject::number_from_signed(7));
        let mut dict = CpdfDictionary::new();
        dict.set_for(b"A", Some(retain_object(array)));
        dict.set_new_for(b"B", CpdfScalarObject::number_from_signed(8));
        let mut walker = CpdfObjectWalker::new(retain_object(dict));
        let root = walker.get_next().unwrap();
        assert!(root
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .is_locked());
        assert_eq!(walker.current_depth(), 0);
        let child = walker.get_next().unwrap();
        assert_eq!(walker.dictionary_key(), b"A");
        assert_eq!(walker.current_depth(), 1);
        assert!(walker
            .parent()
            .is_some_and(|parent| std::rc::Rc::ptr_eq(&parent, &root)));
        walker.skip_walk_into_current_object();
        let next = walker.get_next().unwrap();
        assert_eq!(next.borrow().get_integer(), 8);
        assert_eq!(walker.dictionary_key(), b"B");
        assert!(!std::rc::Rc::ptr_eq(&child, &next));
        assert!(walker.get_next().is_none());
        assert!(!root
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .is_locked());
    }
}

#[cfg(test)]
#[path = "cpdf_object_walker_unittest.rs"]
mod upstream_tests;
