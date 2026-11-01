// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/observed_ptr_unittest.cpp over real Rc/Weak observers.
//! Weak count is the native active-observer count; upgrade supplies the
//! nullable view after actual owner destruction. No observer retains its owner.
#![allow(non_snake_case)]
use std::cell::RefCell;
use std::rc::{Rc, Weak};
#[derive(Default)]
struct PseudoObservable {
    other_: RefCell<Weak<Self>>,
}
fn active(value: &Rc<PseudoObservable>) -> usize {
    Rc::weak_count(value)
}
fn identity(value: &Weak<PseudoObservable>) -> Option<*const PseudoObservable> {
    value.upgrade().as_ref().map(Rc::as_ptr)
}

#[test]
fn Null() {
    let ptr = Weak::<PseudoObservable>::new();
    assert!(ptr.upgrade().is_none());
}

#[test]
fn LivesLonger() {
    let mut ptr = Weak::<PseudoObservable>::new();
    std::hint::black_box(&ptr);
    {
        let pObs = Rc::new(PseudoObservable::default());
        ptr = Rc::downgrade(&pObs);
        assert!(ptr.upgrade().is_some());
        assert_eq!(1usize, active(&pObs));
    }
    assert!(ptr.upgrade().is_none());
}

#[test]
fn LivesShorter() {
    let obs = Rc::new(PseudoObservable::default());
    {
        let ptr = Rc::downgrade(&obs);
        assert!(ptr.upgrade().is_some());
        assert_eq!(1usize, active(&obs));
    }
    assert_eq!(0usize, active(&obs));
}

#[test]
fn CopyConstruct() {
    let obs = Rc::new(PseudoObservable::default());
    {
        let ptr = Rc::downgrade(&obs);
        assert!(ptr.upgrade().is_some());
        assert_eq!(1usize, active(&obs));
        {
            let ptr2 = ptr.clone();
            assert!(ptr2.upgrade().is_some());
            assert_eq!(2usize, active(&obs));
        }
        assert_eq!(1usize, active(&obs));
    }
    assert_eq!(0usize, active(&obs));
}

#[test]
fn CopyAssign() {
    let obs = Rc::new(PseudoObservable::default());
    {
        let ptr = Rc::downgrade(&obs);
        assert!(ptr.upgrade().is_some());
        assert_eq!(1usize, active(&obs));
        {
            let ptr2;
            ptr2 = ptr.clone();
            assert!(ptr2.upgrade().is_some());
            assert_eq!(2usize, active(&obs));
        }
        assert_eq!(1usize, active(&obs));
    }
    assert_eq!(0usize, active(&obs));
}

#[test]
fn Vector() {
    let obs = Rc::new(PseudoObservable::default());
    {
        let mut vec1 = Vec::<Weak<PseudoObservable>>::new();
        let mut vec2;
        vec1.push(Rc::downgrade(&obs));
        vec1.push(Rc::downgrade(&obs));
        assert!(vec1[0].upgrade().is_some());
        assert!(vec1[1].upgrade().is_some());
        assert_eq!(2usize, active(&obs));
        vec2 = vec1.clone();
        assert!(vec2[0].upgrade().is_some());
        assert!(vec2[1].upgrade().is_some());
        assert_eq!(4usize, active(&obs));
        vec1.clear();
        assert_eq!(2usize, active(&obs));
        vec2.resize_with(10000, Weak::new);
        assert_eq!(2usize, active(&obs));
        vec2.resize_with(0, Weak::new);
        assert_eq!(0usize, active(&obs));
    }
    assert_eq!(0usize, active(&obs));
}

#[test]
fn VectorAutoClear() {
    let mut vec1 = Vec::<Weak<PseudoObservable>>::new();
    {
        let obs = Rc::new(PseudoObservable::default());
        vec1.push(Rc::downgrade(&obs));
        vec1.push(Rc::downgrade(&obs));
        assert!(vec1[0].upgrade().is_some());
        assert!(vec1[1].upgrade().is_some());
        assert_eq!(2usize, active(&obs));
    }
    assert!(vec1[0].upgrade().is_none());
    assert!(vec1[1].upgrade().is_none());
}

#[test]
fn ResetNull() {
    let obs = Rc::new(PseudoObservable::default());
    let mut ptr = Rc::downgrade(&obs);
    std::hint::black_box(&ptr);
    assert_eq!(1usize, active(&obs));
    ptr = Weak::new();
    std::hint::black_box(&ptr);
    assert_eq!(0usize, active(&obs));
}

#[test]
fn Reset() {
    let obs1 = Rc::new(PseudoObservable::default());
    let obs2 = Rc::new(PseudoObservable::default());
    let mut ptr = Rc::downgrade(&obs1);
    std::hint::black_box(&ptr);
    assert_eq!(1usize, active(&obs1));
    assert_eq!(0usize, active(&obs2));
    ptr = Rc::downgrade(&obs2);
    std::hint::black_box(&ptr);
    assert_eq!(0usize, active(&obs1));
    assert_eq!(1usize, active(&obs2));
}

#[test]
fn Equals() {
    let obj1 = Rc::new(PseudoObservable::default());
    let obj2 = Rc::new(PseudoObservable::default());
    let null_ptr1 = Weak::<PseudoObservable>::new();
    let obj1_ptr1 = Rc::downgrade(&obj1);
    let obj2_ptr1 = Rc::downgrade(&obj2);
    assert!(Some(Rc::as_ptr(&obj1)) == identity(&obj1_ptr1));
    assert!(identity(&obj1_ptr1) == Some(Rc::as_ptr(&obj1)));
    assert!(Some(Rc::as_ptr(&obj2)) == identity(&obj2_ptr1));
    assert!(identity(&obj2_ptr1) == Some(Rc::as_ptr(&obj2)));
    {
        let null_ptr2 = Weak::<PseudoObservable>::new();
        assert!(identity(&null_ptr1) == identity(&null_ptr2));

        let obj1_ptr2 = Rc::downgrade(&obj1);
        assert!(identity(&obj1_ptr1) == identity(&obj1_ptr2));

        let obj2_ptr2 = Rc::downgrade(&obj2);
        assert!(identity(&obj2_ptr1) == identity(&obj2_ptr2));
    }
    assert!(!(identity(&null_ptr1) == identity(&obj1_ptr1)));
    assert!(!(identity(&null_ptr1) == identity(&obj2_ptr1)));
    assert!(!(identity(&obj1_ptr1) == identity(&obj2_ptr1)));
}

#[test]
fn NotEquals() {
    let obj1 = Rc::new(PseudoObservable::default());
    let obj2 = Rc::new(PseudoObservable::default());
    let null_ptr1 = Weak::<PseudoObservable>::new();
    let obj1_ptr1 = Rc::downgrade(&obj1);
    let obj2_ptr1 = Rc::downgrade(&obj2);
    assert!(!(Some(Rc::as_ptr(&obj1)) != identity(&obj1_ptr1)));
    assert!(!(identity(&obj1_ptr1) != Some(Rc::as_ptr(&obj1))));
    assert!(!(Some(Rc::as_ptr(&obj2)) != identity(&obj2_ptr1)));
    assert!(!(identity(&obj2_ptr1) != Some(Rc::as_ptr(&obj2))));
    {
        let null_ptr2 = Weak::<PseudoObservable>::new();
        let obj1_ptr2 = Rc::downgrade(&obj1);
        let obj2_ptr2 = Rc::downgrade(&obj2);
        assert!(!(identity(&null_ptr1) != identity(&null_ptr2)));
        assert!(!(identity(&obj1_ptr1) != identity(&obj1_ptr2)));
        assert!(!(identity(&obj2_ptr1) != identity(&obj2_ptr2)));
    }
    assert!(identity(&null_ptr1) != identity(&obj1_ptr1));
    assert!(identity(&null_ptr1) != identity(&obj2_ptr1));
    assert!(identity(&obj1_ptr1) != identity(&obj2_ptr1));
}

#[test]
fn Bool() {
    let obj1 = Rc::new(PseudoObservable::default());
    let null_ptr = Weak::<PseudoObservable>::new();
    let obj1_ptr = Rc::downgrade(&obj1);
    let null_bool = null_ptr.upgrade().is_some();
    let obj1_bool = obj1_ptr.upgrade().is_some();
    assert!(!(null_bool));
    assert!(obj1_bool);
}

#[test]
fn SelfObservable() {
    let thing = Rc::new(PseudoObservable::default());
    *thing.other_.borrow_mut() = Rc::downgrade(&thing);
    assert_eq!(Some(Rc::as_ptr(&thing)), identity(&thing.other_.borrow()));
    // Weak self reference permits cleanup without retaining the owner.
}
#[test]
fn PairwiseObservable() {
    let thing1 = Rc::new(PseudoObservable::default());
    {
        let thing2 = Rc::new(PseudoObservable::default());
        *thing1.other_.borrow_mut() = Rc::downgrade(&thing2);
        *thing2.other_.borrow_mut() = Rc::downgrade(&thing1);
        assert_eq!(Some(Rc::as_ptr(&thing2)), identity(&thing1.other_.borrow()));
        assert_eq!(Some(Rc::as_ptr(&thing1)), identity(&thing2.other_.borrow()));
    }
    assert!(thing1.other_.borrow().upgrade().is_none());
}
