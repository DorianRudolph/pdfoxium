// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/weak_ptr_unittest.cpp. Native WeakPtr shares an owning Handle
//! until explicit DeleteObject; this differs from std::rc::Weak. Real Rc
//! nullable handle slots reproduce that contract, including distinct cleared
//! and null handles and reentrant cyclic release. Stack fixtures retain only
//! diagnostic state and clear their next member at native fixture teardown.
#![allow(non_snake_case)]
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Default)]
struct State {
    delete_count: Cell<i32>,
    next: RefCell<WeakTestPtr>,
}
#[derive(Default)]
struct PseudoDeletable {
    state: Rc<State>,
}
impl PseudoDeletable {
    fn delete_count(&self) -> i32 {
        self.state.delete_count.get()
    }
}
impl Drop for PseudoDeletable {
    fn drop(&mut self) {
        let next = std::mem::take(&mut *self.state.next.borrow_mut());
        drop(next);
    }
}
struct UniqueTestPtr {
    state: Rc<State>,
    identity: *const PseudoDeletable,
}
impl UniqueTestPtr {
    fn new(thing: &PseudoDeletable) -> Self {
        Self {
            state: Rc::clone(&thing.state),
            identity: thing,
        }
    }
}
impl Drop for UniqueTestPtr {
    fn drop(&mut self) {
        self.state
            .delete_count
            .set(self.state.delete_count.get() + 1);
        // Clear the member before invoking its callback/destruction.
        let next = std::mem::take(&mut *self.state.next.borrow_mut());
        drop(next);
    }
}
#[derive(Default, Clone)]
struct WeakTestPtr {
    handle: Option<Rc<RefCell<Option<UniqueTestPtr>>>>,
}
impl WeakTestPtr {
    fn new(value: UniqueTestPtr) -> Self {
        Self {
            handle: Some(Rc::new(RefCell::new(Some(value)))),
        }
    }
    fn Get(&self) -> Option<*const PseudoDeletable> {
        self.handle
            .as_ref()
            .and_then(|h| h.borrow().as_ref().map(|v| v.identity))
    }
    fn is_some(&self) -> bool {
        self.Get().is_some()
    }
    fn Reset(&mut self, value: Option<UniqueTestPtr>) {
        *self = value.map_or_else(Self::default, Self::new);
    }
    fn DeleteObject(&mut self) {
        if let Some(handle) = &self.handle {
            let old = handle.borrow_mut().take();
            drop(old); // Release callback can reset other handles; borrow ended.
            self.handle = None;
        }
    }
    fn SetNext(&self, next: &Self) {
        let state = self
            .handle
            .as_ref()
            .unwrap()
            .borrow()
            .as_ref()
            .map(|v| Rc::clone(&v.state))
            .unwrap();
        *state.next.borrow_mut() = next.clone();
    }
}
impl PartialEq for WeakTestPtr {
    fn eq(&self, rhs: &Self) -> bool {
        match (&self.handle, &rhs.handle) {
            (None, None) => true,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

#[test]
fn Null() {
    let ptr1 = WeakTestPtr::default();
    assert!(!ptr1.is_some());

    let ptr2 = WeakTestPtr::default();
    assert!(ptr1 == ptr2);
    assert!(!(ptr1 != ptr2));

    let ptr3 = ptr1.clone();
    assert!(ptr1 == ptr3);
    assert!(!(ptr1 != ptr3));

    let ptr4 = ptr1.clone();
    assert!(ptr1 == ptr4);
    assert!(!(ptr1 != ptr4));
}

#[test]
fn NonNull() {
    let thing = PseudoDeletable::default();
    assert_eq!(0, thing.delete_count());
    {
        let unique = UniqueTestPtr::new(&thing);
        let ptr1 = WeakTestPtr::new(unique);
        assert!(ptr1.is_some());
        assert_eq!(Some(&thing as *const _), ptr1.Get());

        let ptr2 = WeakTestPtr::default();
        assert!(!(ptr1 == ptr2));
        assert!(ptr1 != ptr2);
        {
            let ptr3 = ptr1.clone();
            assert!(ptr1 == ptr3);
            assert!(!(ptr1 != ptr3));
            assert_eq!(Some(&thing as *const _), ptr3.Get());
            {
                let ptr4 = ptr1.clone();
                assert!(ptr1 == ptr4);
                assert!(!(ptr1 != ptr4));
                assert_eq!(Some(&thing as *const _), ptr4.Get());
            }
        }
        assert_eq!(0, thing.delete_count());
    }
    assert_eq!(1, thing.delete_count());
}

#[test]
fn ResetNull() {
    let thing = PseudoDeletable::default();
    {
        let unique = UniqueTestPtr::new(&thing);
        let mut ptr1 = WeakTestPtr::new(unique);
        let ptr2 = ptr1.clone();
        ptr1.Reset(None);
        assert!(!ptr1.is_some());
        assert!(ptr1.Get().is_none());
        assert!(ptr2.is_some());
        assert_eq!(Some(&thing as *const _), ptr2.Get());
        assert!(!(ptr1 == ptr2));
        assert!(ptr1 != ptr2);
        assert_eq!(0, thing.delete_count());
    }
    assert_eq!(1, thing.delete_count());
}

#[test]
fn ResetNonNull() {
    let thing1 = PseudoDeletable::default();
    let thing2 = PseudoDeletable::default();
    {
        let unique1 = UniqueTestPtr::new(&thing1);
        let ptr1 = WeakTestPtr::new(unique1);
        let mut ptr2 = ptr1.clone();
        let unique2 = UniqueTestPtr::new(&thing2);
        ptr2.Reset(Some(unique2));
        assert!(ptr1.is_some());
        assert_eq!(Some(&thing1 as *const _), ptr1.Get());
        assert!(ptr2.is_some());
        assert_eq!(Some(&thing2 as *const _), ptr2.Get());
        assert!(!(ptr1 == ptr2));
        assert!(ptr1 != ptr2);
        assert_eq!(0, thing1.delete_count());
        assert_eq!(0, thing2.delete_count());
    }
    assert_eq!(1, thing1.delete_count());
    assert_eq!(1, thing2.delete_count());
}

#[test]
fn DeleteObject() {
    let thing = PseudoDeletable::default();
    {
        let unique = UniqueTestPtr::new(&thing);
        let mut ptr1 = WeakTestPtr::new(unique);
        let ptr2 = ptr1.clone();
        ptr1.DeleteObject();
        assert!(!ptr1.is_some());
        assert!(ptr1.Get().is_none());
        assert!(!ptr2.is_some());
        assert!(ptr2.Get().is_none());
        assert!(!(ptr1 == ptr2));
        assert!(ptr1 != ptr2);
        assert_eq!(1, thing.delete_count());
    }
    assert_eq!(1, thing.delete_count());
}

#[test]
fn Cyclic() {
    let thing1 = PseudoDeletable::default();
    let thing2 = PseudoDeletable::default();
    {
        let unique1 = UniqueTestPtr::new(&thing1);
        let unique2 = UniqueTestPtr::new(&thing2);
        let ptr1 = WeakTestPtr::new(unique1);
        let ptr2 = WeakTestPtr::new(unique2);
        ptr1.SetNext(&ptr2);
        ptr2.SetNext(&ptr1);
    }
    // Leaks without explicit clear.
    assert_eq!(0, thing1.delete_count());
    assert_eq!(0, thing2.delete_count());
}

#[test]
fn CyclicDeleteObject() {
    let thing1 = PseudoDeletable::default();
    let thing2 = PseudoDeletable::default();
    {
        let unique1 = UniqueTestPtr::new(&thing1);
        let unique2 = UniqueTestPtr::new(&thing2);
        let mut ptr1 = WeakTestPtr::new(unique1);
        let ptr2 = WeakTestPtr::new(unique2);
        ptr1.SetNext(&ptr2);
        ptr2.SetNext(&ptr1);
        ptr1.DeleteObject();
        assert_eq!(1, thing1.delete_count());
        assert_eq!(0, thing2.delete_count());
    }
    assert_eq!(1, thing1.delete_count());
    assert_eq!(1, thing2.delete_count());
}
