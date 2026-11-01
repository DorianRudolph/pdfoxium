// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/shared_copy_on_write_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::SharedCopyOnWrite;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Default)]
struct Observer {
    construction_counts: RefCell<BTreeMap<String, i32>>,
    destruction_counts: RefCell<BTreeMap<String, i32>>,
}
impl Observer {
    fn on_construct(&self, name: &str) {
        *self
            .construction_counts
            .borrow_mut()
            .entry(name.into())
            .or_default() += 1;
    }
    fn on_destruct(&self, name: &str) {
        *self
            .destruction_counts
            .borrow_mut()
            .entry(name.into())
            .or_default() += 1;
    }
    fn get_construction_count(&self, name: &str) -> i32 {
        *self
            .construction_counts
            .borrow_mut()
            .entry(name.into())
            .or_default()
    }
    fn get_destruction_count(&self, name: &str) -> i32 {
        *self
            .destruction_counts
            .borrow_mut()
            .entry(name.into())
            .or_default()
    }
}

struct Object<'a> {
    name: String,
    observer: &'a Observer,
}
impl<'a> Object<'a> {
    fn new(observer: &'a Observer, name: &str) -> Self {
        observer.on_construct(name);
        Self {
            name: name.into(),
            observer,
        }
    }
}
impl Clone for Object<'_> {
    fn clone(&self) -> Self {
        Self::new(self.observer, &self.name)
    }
}
impl Drop for Object<'_> {
    fn drop(&mut self) {
        self.observer.on_destruct(&self.name);
    }
}

#[test]
fn Null() {
    let _observer = Observer::default();
    {
        let ptr = SharedCopyOnWrite::<Object<'_>>::default();
        assert!(ptr.get_object().is_none());
    }
}

#[test]
fn Copy() {
    let observer = Observer::default();
    {
        let mut ptr1 = SharedCopyOnWrite::default();
        ptr1.emplace(Object::new(&observer, "one"));
        {
            let _ptr2 = ptr1.clone();
            assert_eq!(observer.get_construction_count("one"), 1);
            assert_eq!(observer.get_destruction_count("one"), 0);
        }
        {
            let _ptr3 = ptr1.clone();
            assert_eq!(observer.get_construction_count("one"), 1);
            assert_eq!(observer.get_destruction_count("one"), 0);
        }
        assert_eq!(observer.get_construction_count("one"), 1);
        assert_eq!(observer.get_destruction_count("one"), 0);
    }
    assert_eq!(observer.get_destruction_count("one"), 1);
}

#[test]
fn AssignOverOld() {
    let observer = Observer::default();
    {
        let mut ptr1 = SharedCopyOnWrite::default();
        ptr1.emplace(Object::new(&observer, "one"));
        ptr1.emplace(Object::new(&observer, "two"));
        assert_eq!(observer.get_construction_count("one"), 1);
        assert_eq!(observer.get_construction_count("two"), 1);
        assert_eq!(observer.get_destruction_count("one"), 1);
        assert_eq!(observer.get_destruction_count("two"), 0);
    }
    assert_eq!(observer.get_destruction_count("two"), 1);
}

#[test]
fn AssignOverRetained() {
    let observer = Observer::default();
    {
        let mut ptr1 = SharedCopyOnWrite::default();
        ptr1.emplace(Object::new(&observer, "one"));
        let _ptr2 = ptr1.clone();
        ptr1.emplace(Object::new(&observer, "two"));
        assert_eq!(observer.get_construction_count("one"), 1);
        assert_eq!(observer.get_construction_count("two"), 1);
        assert_eq!(observer.get_destruction_count("one"), 0);
        assert_eq!(observer.get_destruction_count("two"), 0);
    }
    assert_eq!(observer.get_destruction_count("one"), 1);
    assert_eq!(observer.get_destruction_count("two"), 1);
}

#[test]
fn GetModify() {
    let observer = Observer::default();
    {
        let mut ptr = SharedCopyOnWrite::default();
        // &mut Object guarantees the native EXPECT_TRUE(non-null) contract.
        let _object = ptr.get_private_copy(|| Object::new(&observer, "one"));
        assert_eq!(observer.get_construction_count("one"), 1);
        assert_eq!(observer.get_destruction_count("one"), 0);
        let _object = ptr.get_private_copy(|| Object::new(&observer, "one"));
        assert_eq!(observer.get_construction_count("one"), 1);
        assert_eq!(observer.get_destruction_count("one"), 0);
        {
            let _other = ptr.clone();
            let _object = ptr.get_private_copy(|| Object::new(&observer, "one"));
            assert_eq!(observer.get_construction_count("one"), 2);
            assert_eq!(observer.get_destruction_count("one"), 0);
        }
        assert_eq!(observer.get_construction_count("one"), 2);
        assert_eq!(observer.get_destruction_count("one"), 1);
    }
    assert_eq!(observer.get_destruction_count("one"), 2);
}
