// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/unowned_ptr_unittest.cpp. Option<&T> represents ordinary
//! borrowed/null slots; explicit take preserves native moved-from nulls.
//! Lifetime graph tests use Rc/Weak so the selected non-instrumented cleanup
//! cases can retain their exact allocation/drop topology without dangling
//! Rust references. Pointer comparisons use identities rather than pointee Eq.
#![allow(non_snake_case)]
use std::cell::RefCell;
use std::rc::{Rc, Weak};
#[derive(Default)]
struct Clink {
    next_: RefCell<Weak<Self>>,
}
fn id<T>(value: Option<&T>) -> Option<*const T> {
    value.map(std::ptr::from_ref)
}

#[test]
fn DefaultCtor() {
    let ptr: Option<&Clink> = None;
    assert!(ptr.is_none());
}

#[test]
fn NullptrCtor() {
    let ptr: Option<&Clink> = None;
    assert!(ptr.is_none());
}

#[test]
fn RawCtor() {
    let obj = Box::new(Clink::default());
    let ptr = Some(obj.as_ref());
    assert!(std::ptr::eq(obj.as_ref(), ptr.unwrap()));
}

#[test]
fn CopyCtor() {
    let obj = Box::new(Clink::default());
    let ptr1 = Some(obj.as_ref());
    let ptr2 = ptr1;
    assert!(std::ptr::eq(obj.as_ref(), ptr2.unwrap()));
    assert!(std::ptr::eq(obj.as_ref(), ptr1.unwrap()));
}

#[test]
fn MoveCtor() {
    let obj = Box::new(Clink::default());
    let mut ptr1 = Some(obj.as_ref());
    let ptr2 = ptr1.take();
    assert!(std::ptr::eq(obj.as_ref(), ptr2.unwrap()));
    assert!(ptr1.is_none());
}

#[test]
fn CopyConversionCtor() {
    let obj = Box::new(Clink::default());
    let ptr1 = Some(obj.as_ref());
    let ptr2 = ptr1;
    assert!(std::ptr::eq(obj.as_ref(), ptr2.unwrap()));
    assert!(std::ptr::eq(obj.as_ref(), ptr1.unwrap()));
}

#[test]
fn MoveConversionCtor() {
    let obj = Box::new(Clink::default());
    let mut ptr1 = Some(obj.as_ref());
    let ptr2 = ptr1.take();
    assert!(std::ptr::eq(obj.as_ref(), ptr2.unwrap()));
    assert!(ptr1.is_none());
}

#[test]
fn NullptrAssign() {
    let obj = Box::new(Clink::default());
    let mut ptr = Some(obj.as_ref());
    std::hint::black_box(ptr);
    ptr = None;
    assert!(ptr.is_none());
}

#[test]
fn RawAssign() {
    let obj = Box::new(Clink::default());
    let ptr: Option<&Clink>;
    ptr = Some(obj.as_ref());
    assert!(std::ptr::eq(obj.as_ref(), ptr.unwrap()));
}

#[test]
fn CopyAssign() {
    let obj = Box::new(Clink::default());
    let ptr1 = Some(obj.as_ref());
    let ptr2: Option<&Clink>;
    ptr2 = ptr1;
    assert!(std::ptr::eq(obj.as_ref(), ptr1.unwrap()));
    assert!(std::ptr::eq(obj.as_ref(), ptr2.unwrap()));
}

#[test]
fn MoveAssign() {
    let obj = Box::new(Clink::default());
    let mut ptr1 = Some(obj.as_ref());
    let ptr2: Option<&Clink>;
    ptr2 = ptr1.take();
    assert!(ptr1.is_none());
    assert!(std::ptr::eq(obj.as_ref(), ptr2.unwrap()));
}

#[test]
fn CopyConversionAssign() {
    let obj = Box::new(Clink::default());
    let ptr1 = Some(obj.as_ref());
    let ptr2: Option<&Clink>;
    ptr2 = ptr1;
    assert!(std::ptr::eq(obj.as_ref(), ptr1.unwrap()));
    assert!(std::ptr::eq(obj.as_ref(), ptr2.unwrap()));
}

#[test]
fn MoveConversionAssign() {
    let obj = Box::new(Clink::default());
    let mut ptr1 = Some(obj.as_ref());
    let ptr2: Option<&Clink>;
    ptr2 = ptr1.take();
    assert!(ptr1.is_none());
    assert!(std::ptr::eq(obj.as_ref(), ptr2.unwrap()));
}

fn DeleteDangling() {
    let ptr2 = Rc::new(Clink::default());
    {
        let ptr1 = Rc::new(Clink::default());
        *ptr2.next_.borrow_mut() = Rc::downgrade(&ptr1);
    }
}
fn AssignDangling() {
    let ptr2 = Rc::new(Clink::default());
    {
        let ptr1 = Rc::new(Clink::default());
        *ptr2.next_.borrow_mut() = Rc::downgrade(&ptr1);
    }
    *ptr2.next_.borrow_mut() = Weak::new();
}
fn ReleaseDangling() {
    let ptr2 = Rc::new(Clink::default());
    {
        let ptr1 = Rc::new(Clink::default());
        *ptr2.next_.borrow_mut() = Rc::downgrade(&ptr1);
    }
    std::mem::take(&mut *ptr2.next_.borrow_mut());
}
#[test]
fn PtrOk() {
    let ptr1 = Rc::new(Clink::default());
    {
        let ptr2 = Rc::new(Clink::default());
        *ptr2.next_.borrow_mut() = Rc::downgrade(&ptr1);
    }
}
#[test]
fn PtrNotOk() {
    DeleteDangling();
}
#[test]
fn AssignOk() {
    let ptr1 = Rc::new(Clink::default());
    {
        let ptr2 = Rc::new(Clink::default());
        *ptr2.next_.borrow_mut() = Rc::downgrade(&ptr1);
        *ptr2.next_.borrow_mut() = Weak::new();
    }
}
#[test]
fn AssignNotOk() {
    AssignDangling();
}
#[test]
fn ReleaseOk() {
    let ptr2 = Rc::new(Clink::default());
    {
        let ptr1 = Rc::new(Clink::default());
        *ptr2.next_.borrow_mut() = Rc::downgrade(&ptr1);
        std::mem::take(&mut *ptr2.next_.borrow_mut());
    }
}
#[test]
fn ReleaseNotOk() {
    ReleaseDangling();
}
#[test]
fn OperatorEQ() {
    let foo = 0;
    let mut ptr1: Option<&i32> = None;
    assert!(id(ptr1) == id(ptr1));
    let ptr2: Option<&i32> = None;
    assert!(id(ptr1) == id(ptr2));
    let ptr3 = Some(&foo);
    assert!(id(Some(&foo)) == id(ptr3));
    assert!(id(ptr3) == id(Some(&foo)));
    assert!(!(id(ptr1) == id(ptr3)));
    ptr1 = Some(&foo);
    assert!(id(ptr1) == id(ptr3));
}
#[test]
fn OperatorNE() {
    let foo = 0;
    let mut ptr1: Option<&i32> = None;
    assert!(!(id(ptr1) != id(ptr1)));
    let ptr2: Option<&i32> = None;
    assert!(!(id(ptr1) != id(ptr2)));
    let ptr3 = Some(&foo);
    assert!(!(id(Some(&foo)) != id(ptr3)));
    assert!(!(id(ptr3) != id(Some(&foo))));
    assert!(id(ptr1) != id(ptr3));
    ptr1 = Some(&foo);
    assert!(!(id(ptr1) != id(ptr3)));
}
#[test]
fn OperatorLT() {
    let foos = [0, 0];
    let ptr1 = Some(&foos[0]);
    let ptr2 = Some(&foos[1]);
    assert!(!(id(ptr1) < id(ptr1)));
    assert!(id(ptr1) < id(ptr2));
    assert!(!(id(ptr2) < id(ptr1)));
}
#[test]
fn TransparentCompare() {
    let foos = [0, 0];
    let ptr1 = Some(&foos[0]);
    let _ptr2 = Some(&foos[1]);
    let mut holder = std::collections::BTreeSet::new();
    holder.insert(id(ptr1));
    assert!(holder.get(&id(Some(&foos[0]))).is_some());
    assert!(holder.get(&id(Some(&foos[1]))).is_none());
    assert!(holder.contains(&id(Some(&foos[0]))));
    assert!(!holder.contains(&id(Some(&foos[1]))));
}
#[test]
fn RangesEqualsTo() {
    let empty_vec: Vec<Option<&Clink>> = Vec::new();
    {
        let ptr: Option<&Clink> = None;
        let it = empty_vec.iter().position(|v| id(*v) == id(ptr));
        assert!(it.is_none());
    }
    {
        let ptr = Box::new(Clink::default());
        let it = empty_vec
            .iter()
            .position(|v| id(*v) == id(Some(ptr.as_ref())));
        assert!(it.is_none());
    }
    {
        let it = empty_vec.iter().position(|v| v.is_none());
        assert!(it.is_none());
    }
}
