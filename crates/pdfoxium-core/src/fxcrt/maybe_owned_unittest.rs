// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/maybe_owned_unittest.cpp over selected owned/borrowed variants.
//! Box/borrow variants retain native null-ownership tags after explicit moves.
//! Release rebinds the borrowed view after the Box is transferred, making its
//! owner lifetime explicit without constructing a self-referential Rust value.
#![allow(non_snake_case)]
use std::cell::Cell;

struct PseudoDeletable<'a> {
    id: i32,
    count_location: &'a Cell<i32>,
}
impl<'a> PseudoDeletable<'a> {
    fn new(id: i32, count_location: &'a Cell<i32>) -> Self {
        Self { id, count_location }
    }
}
impl Drop for PseudoDeletable<'_> {
    fn drop(&mut self) {
        self.count_location.set(self.count_location.get() + 1);
    }
}
enum MaybeOwned<'a, T> {
    Owned(Option<Box<T>>),
    Unowned(Option<&'a T>),
}
impl<T> Default for MaybeOwned<'_, T> {
    fn default() -> Self {
        Self::Unowned(None)
    }
}
impl<'a, T> MaybeOwned<'a, T> {
    fn borrowed(value: &'a T) -> Self {
        Self::Unowned(Some(value))
    }
    fn owned(value: T) -> Self {
        Self::Owned(Some(Box::new(value)))
    }
    fn IsOwned(&self) -> bool {
        matches!(self, Self::Owned(_))
    }
    fn Get(&self) -> Option<&T> {
        match self {
            Self::Owned(b) => b.as_deref(),
            Self::Unowned(b) => *b,
        }
    }
    fn is_some(&self) -> bool {
        self.Get().is_some()
    }
    fn take(&mut self) -> Self {
        let empty = if self.IsOwned() {
            Self::Owned(None)
        } else {
            Self::Unowned(None)
        };
        std::mem::replace(self, empty)
    }
    fn identity(&self) -> Option<*const T> {
        self.Get().map(std::ptr::from_ref)
    }
}
impl<T> PartialEq for MaybeOwned<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.identity() == other.identity()
    }
}
impl<T> PartialEq<&T> for MaybeOwned<'_, T> {
    fn eq(&self, other: &&T) -> bool {
        self.identity() == Some(std::ptr::from_ref(*other))
    }
}
impl MaybeOwned<'_, PseudoDeletable<'_>> {
    fn GetID(&self) -> i32 {
        self.Get().unwrap().id
    }
}

#[test]
fn Null() {
    let ptr1: MaybeOwned<'_, PseudoDeletable<'_>> = MaybeOwned::default();
    assert!(!(ptr1.IsOwned()));
    assert!(!ptr1.is_some());
    assert!(ptr1.Get().is_none());

    let ptr2: MaybeOwned<'_, PseudoDeletable<'_>> = MaybeOwned::default();
    assert!(ptr1 == ptr2);
    assert!(!(ptr1 != ptr2));
}

#[test]
fn NotOwned() {
    let delete_count = Cell::new(0);
    let thing1 = PseudoDeletable::new(100, &delete_count);
    {
        let ptr = MaybeOwned::borrowed(&thing1);
        assert!(!(ptr.IsOwned()));
        assert!(std::ptr::eq(ptr.Get().unwrap(), &thing1));
        assert_eq!(100, ptr.GetID());
        assert!(ptr == &thing1);
        assert!(!(ptr != &thing1));

        let empty: MaybeOwned<'_, PseudoDeletable<'_>> = MaybeOwned::default();
        assert!(!(ptr == empty));
        assert!(ptr != empty);
    }
    assert_eq!(0, delete_count.get());

    delete_count.set(0);
    let thing2 = PseudoDeletable::new(200, &delete_count);
    {
        let mut ptr = MaybeOwned::borrowed(&thing1);
        std::hint::black_box(&ptr);
        ptr = MaybeOwned::borrowed(&thing2);
        assert!(!(ptr.IsOwned()));
        assert!(std::ptr::eq(ptr.Get().unwrap(), &thing2));
        assert_eq!(200, ptr.GetID());
    }
    assert_eq!(0, delete_count.get());

    delete_count.set(0);
    let owned_delete_count = Cell::new(0);
    {
        let mut ptr = MaybeOwned::borrowed(&thing1);
        assert_eq!(100, ptr.GetID());
        ptr = MaybeOwned::owned(PseudoDeletable::new(300, &owned_delete_count));
        assert!(ptr.IsOwned());
        assert_eq!(300, ptr.GetID());
    }
    assert_eq!(0, delete_count.get());
    assert_eq!(1, owned_delete_count.get());
}

#[test]
fn UnownedPtr() {
    let delete_count = Cell::new(0);
    let thing1 = PseudoDeletable::new(100, &delete_count);
    let thing2 = PseudoDeletable::new(200, &delete_count);
    let unowned1 = &thing1;
    let unowned2 = &thing2;
    {
        let mut ptr1 = MaybeOwned::borrowed(unowned1);
        let mut ptr2 = MaybeOwned::borrowed(unowned2);
        std::hint::black_box((&ptr1, &ptr2));
        ptr2 = MaybeOwned::borrowed(unowned1);
        ptr1 = MaybeOwned::borrowed(unowned2);
        std::hint::black_box((&ptr1, &ptr2));
    }
    assert_eq!(0, delete_count.get());
}

#[test]
fn Owned() {
    let delete_count = Cell::new(0);
    {
        let ptr = MaybeOwned::owned(PseudoDeletable::new(100, &delete_count));
        assert!(ptr.IsOwned());
        assert_eq!(100, ptr.GetID());

        let empty: MaybeOwned<'_, PseudoDeletable<'_>> = MaybeOwned::default();
        assert!(!(ptr == empty));
        assert!(ptr != empty);
    }
    assert_eq!(1, delete_count.get());

    delete_count.set(0);
    {
        let mut ptr = MaybeOwned::owned(PseudoDeletable::new(200, &delete_count));
        std::hint::black_box(&ptr);
        ptr = MaybeOwned::owned(PseudoDeletable::new(300, &delete_count));
        assert!(ptr.IsOwned());
        assert_eq!(300, ptr.GetID());
        assert_eq!(1, delete_count.get());
    }
    assert_eq!(2, delete_count.get());

    delete_count.set(0);
    let unowned_delete_count = Cell::new(0);
    let thing2 = PseudoDeletable::new(400, &unowned_delete_count);
    {
        let mut ptr = MaybeOwned::owned(PseudoDeletable::new(500, &delete_count));
        std::hint::black_box(&ptr);
        ptr = MaybeOwned::borrowed(&thing2);
        assert!(!(ptr.IsOwned()));
        assert_eq!(400, ptr.GetID());
        assert_eq!(1, delete_count.get());
        assert_eq!(0, unowned_delete_count.get());
    }
    assert_eq!(1, delete_count.get());
    assert_eq!(0, unowned_delete_count.get());
}

#[test]
fn Move() {
    let delete_count = Cell::new(0);
    let thing1 = PseudoDeletable::new(100, &delete_count);
    {
        let mut ptr1 = MaybeOwned::borrowed(&thing1);
        let mut ptr2 = MaybeOwned::owned(PseudoDeletable::new(200, &delete_count));
        assert!(!(ptr1.IsOwned()));
        assert!(ptr2.IsOwned());

        let mut ptr3 = ptr1.take();
        let mut ptr4 = ptr2.take();
        assert!(!(ptr1.IsOwned())); // Unowned and null.
        assert!(ptr1.Get().is_none());
        assert!(ptr2.IsOwned()); // Owned but null.
        assert!(ptr2.Get().is_none());
        assert!(!(ptr3.IsOwned()));
        assert!(ptr4.IsOwned());
        assert_eq!(0, delete_count.get());
        assert_eq!(100, ptr3.GetID());
        assert_eq!(200, ptr4.GetID());

        let ptr5: MaybeOwned<'_, PseudoDeletable<'_>>;
        let ptr6: MaybeOwned<'_, PseudoDeletable<'_>>;
        ptr5 = ptr3.take();
        ptr6 = ptr4.take();
        assert!(!(ptr3.IsOwned())); // Unowned and null.
        assert!(ptr3.Get().is_none());
        assert!(ptr4.IsOwned()); // Owned but null.
        assert!(ptr4.Get().is_none());
        assert!(!(ptr5.IsOwned()));
        assert!(ptr6.IsOwned());
        assert_eq!(0, delete_count.get());
        assert_eq!(100, ptr5.GetID());
        assert_eq!(200, ptr6.GetID());
    }
    assert_eq!(1, delete_count.get());
}

#[test]
fn Release() {
    let delete_count = Cell::new(0);
    {
        let stolen;
        {
            let mut ptr = MaybeOwned::owned(PseudoDeletable::new(100, &delete_count));
            assert!(ptr.IsOwned());
            let MaybeOwned::Owned(value) = &mut ptr else {
                unreachable!()
            };
            stolen = value.take().unwrap();
            ptr = MaybeOwned::borrowed(stolen.as_ref());
            assert!(!ptr.IsOwned());
            assert!(std::ptr::eq(ptr.Get().unwrap(), stolen.as_ref()));
            assert_eq!(0, delete_count.get());
        }
        assert_eq!(0, delete_count.get());
    }
    assert_eq!(1, delete_count.get());
}
#[test]
fn MoveElisionThwarted() {
    struct Thing {
        _x: i32,
    }
    struct Owner {
        _thing: std::rc::Rc<Thing>,
    }
    struct Manager {
        transient: Option<std::rc::Weak<Thing>>,
        _owner: Box<Owner>,
        _thing: std::rc::Weak<Thing>,
    }
    impl Manager {
        fn new() -> Self {
            let owned = std::rc::Rc::new(Thing { _x: 42 });
            let mut transient = Some(std::rc::Rc::downgrade(&owned));
            let owner = Box::new(Owner { _thing: owned });
            // Native rvalue Get consumes the transient borrowed variant.
            let thing = transient.take().unwrap();
            Self {
                transient,
                _owner: owner,
                _thing: thing,
            }
        }
        fn has_transient(&self) -> bool {
            self.transient.is_some()
        }
    }
    let manager = Manager::new();
    assert!(!manager.has_transient());
}
