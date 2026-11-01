// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/parser/cpdf_object_avail.{h,cpp}`. The exclusion predicate
//! represents PDFium's virtual override used by page availability checks.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::{Rc, Weak};

use super::cpdf_cross_ref_avail::DocAvailStatus;
use super::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use super::cpdf_object::{retain_object, ObjectRef};
use super::cpdf_object_walker::CpdfObjectWalker;
use super::cpdf_read_validator::{CpdfReadValidator, ScopedSession};
use super::cpdf_reference::CpdfReference;

pub struct CpdfObjectAvail {
    validator: Rc<CpdfReadValidator>,
    holder: Weak<RefCell<CpdfIndirectObjectHolder>>,
    root: Option<ObjectRef>,
    parsed_objnums: BTreeSet<u32>,
    non_parsed_objects: Vec<u32>,
    exclude_object: Option<Rc<dyn Fn(&ObjectRef) -> bool>>,
}

impl CpdfObjectAvail {
    pub fn from_root(
        validator: Rc<CpdfReadValidator>,
        holder: &HolderRef,
        root: ObjectRef,
    ) -> Self {
        let mut parsed_objnums = BTreeSet::new();
        if !root.borrow().is_inline() {
            parsed_objnums.insert(root.borrow().obj_num());
        }
        Self {
            validator,
            holder: Rc::downgrade(holder),
            root: Some(root),
            parsed_objnums,
            non_parsed_objects: Vec::new(),
            exclude_object: None,
        }
    }

    pub fn from_obj_num(
        validator: Rc<CpdfReadValidator>,
        holder: &HolderRef,
        obj_num: u32,
    ) -> Self {
        Self {
            validator,
            holder: Rc::downgrade(holder),
            root: Some(retain_object(CpdfReference::new(holder, obj_num))),
            parsed_objnums: BTreeSet::new(),
            non_parsed_objects: Vec::new(),
            exclude_object: None,
        }
    }

    pub fn set_exclude_object(&mut self, exclude: Option<Rc<dyn Fn(&ObjectRef) -> bool>>) {
        self.exclude_object = exclude;
    }

    pub fn check_avail(&mut self) -> DocAvailStatus {
        if !self.load_root_object() {
            return DocAvailStatus::DataNotAvailable;
        }
        if self.check_objects() {
            self.root = None;
            self.parsed_objnums.clear();
            DocAvailStatus::DataAvailable
        } else {
            DocAvailStatus::DataNotAvailable
        }
    }

    fn load_root_object(&mut self) -> bool {
        if !self.non_parsed_objects.is_empty() {
            return true;
        }
        loop {
            let ref_num = self.root.as_ref().and_then(|root| {
                let borrowed = root.borrow();
                borrowed
                    .as_any()
                    .downcast_ref::<CpdfReference>()
                    .map(CpdfReference::ref_obj_num)
            });
            let Some(ref_num) = ref_num else { break };
            if self.parsed_objnums.contains(&ref_num) {
                self.root = None;
                return true;
            }
            let _session = ScopedSession::new(self.validator.clone());
            let Some(holder) = self.holder.upgrade() else {
                return false;
            };
            let direct = CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, ref_num);
            if self.validator.has_read_problems() {
                return false;
            }
            self.parsed_objnums.insert(ref_num);
            self.root = direct;
        }
        let mut refs = Vec::new();
        if self.append_object_sub_refs(self.root.clone(), &mut refs) {
            self.non_parsed_objects = refs;
            true
        } else {
            false
        }
    }

    fn check_objects(&mut self) -> bool {
        let mut checked_objects = BTreeSet::new();
        let mut objects_to_check = std::mem::take(&mut self.non_parsed_objects);
        while let Some(obj_num) = objects_to_check.pop() {
            if self.parsed_objnums.contains(&obj_num) || !checked_objects.insert(obj_num) {
                continue;
            }
            let _session = ScopedSession::new(self.validator.clone());
            let Some(holder) = self.holder.upgrade() else {
                self.non_parsed_objects.push(obj_num);
                continue;
            };
            let direct = CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, obj_num);
            if direct
                .as_ref()
                .zip(self.root.as_ref())
                .is_some_and(|(a, b)| Rc::ptr_eq(a, b))
            {
                continue;
            }
            if self.validator.has_read_problems()
                || !self.append_object_sub_refs(direct, &mut objects_to_check)
            {
                self.non_parsed_objects.push(obj_num);
                continue;
            }
            self.parsed_objnums.insert(obj_num);
        }
        self.non_parsed_objects.is_empty()
    }

    fn append_object_sub_refs(&self, object: Option<ObjectRef>, refs: &mut Vec<u32>) -> bool {
        let Some(object) = object else { return true };
        let mut walker = CpdfObjectWalker::new(object);
        while let Some(obj) = walker.get_next() {
            let _session = ScopedSession::new(self.validator.clone());
            let is_root = self
                .root
                .as_ref()
                .is_some_and(|root| Rc::ptr_eq(root, &obj));
            let skip = (walker.parent().is_some() && is_root)
                || walker.dictionary_key() == b"Parent"
                || (!is_root
                    && self
                        .exclude_object
                        .as_ref()
                        .is_some_and(|exclude| exclude(&obj)));
            if self.validator.has_read_problems() {
                return false;
            }
            if skip {
                walker.skip_walk_into_current_object();
                continue;
            }
            let borrowed = obj.borrow();
            if let Some(reference) = borrowed.as_any().downcast_ref::<CpdfReference>() {
                refs.push(reference.ref_obj_num());
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{CpdfObjectAvail, DocAvailStatus};
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_read_validator::CpdfReadValidator;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    #[test]
    fn checks_child_reference_and_skips_parent_cycle() {
        let holder = CpdfIndirectObjectHolder::new();
        let leaf = retain_object(CpdfScalarObject::number_from_signed(12));
        let leaf_num = holder.borrow_mut().add_indirect_object(leaf);
        let mut root = CpdfDictionary::new();
        root.set_for(
            b"Child",
            Some(retain_object(CpdfReference::new(&holder, leaf_num))),
        );
        root.set_for(
            b"Parent",
            Some(retain_object(CpdfReference::new(&holder, 999))),
        );
        let file = Rc::new(CfxReadOnlyContainerStream::new(Vec::new()));
        let validator = Rc::new(CpdfReadValidator::new(file, None));
        let mut avail = CpdfObjectAvail::from_root(validator, &holder, retain_object(root));
        assert_eq!(avail.check_avail(), DocAvailStatus::DataAvailable);
        assert_eq!(avail.check_avail(), DocAvailStatus::DataAvailable);
    }
}

#[cfg(test)]
#[path = "cpdf_object_avail_unittest.rs"]
mod upstream_tests;
