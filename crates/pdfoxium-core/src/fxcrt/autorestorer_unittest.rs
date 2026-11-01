// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/autorestorer_unittest.cpp using scoped safe cleanup/restoration, as selected
//! source translations do. Cell/RefCell preserve reads during the scope;
//! abandonment retains the saved value until scope exit, matching native.
#![allow(non_snake_case)]
use super::ownership_test_support::ScopeExit;
use std::cell::Cell;

use super::ownership_test_support::{PseudoRetainable, RetainPtr};
use std::cell::RefCell;

#[test]
fn AutoRestorer() {
    let x = Cell::new(5);
    {
        let old_value = x.get();
        let _restorer = ScopeExit::new(|| x.set(old_value));
        x.set(6);
        assert_eq!(6, x.get());
    }
    assert_eq!(5, x.get());
}

#[test]
fn AutoRestorerNoOp() {
    let x = Cell::new(5);
    {
        let old_value = x.get();
        let _restorer = ScopeExit::new(|| x.set(old_value));
        assert_eq!(5, x.get());
    }
    assert_eq!(5, x.get());
}

#[test]
fn AutoRestorerAbandon() {
    let x = Cell::new(5);
    {
        let old_value = x.get();
        let restorer = ScopeExit::new(|| x.set(old_value));
        x.set(6);
        assert_eq!(6, x.get());
        restorer.abandon();
    }
    assert_eq!(6, x.get());
}

#[test]
fn AutoRestorerUnownedPtr() {
    let x = 5;
    let y = 6;
    let ptr = Cell::new(&x);
    {
        let old_value = ptr.get();
        let _restorer = ScopeExit::new(|| ptr.set(old_value));
        ptr.set(&y);
        assert!(std::ptr::eq(&y, ptr.get()));
    }
    assert!(std::ptr::eq(&x, ptr.get()));
}

#[test]
fn AutoRestorerUnownedNoOp() {
    let x = 5;
    let ptr = Cell::new(&x);
    {
        let old_value = ptr.get();
        let _restorer = ScopeExit::new(|| ptr.set(old_value));
        assert!(std::ptr::eq(&x, ptr.get()));
    }
    assert!(std::ptr::eq(&x, ptr.get()));
}

#[test]
fn AutoRestorerUnownedPtrAbandon() {
    let x = 5;
    let y = 6;
    let ptr = Cell::new(&x);
    {
        let old_value = ptr.get();
        let restorer = ScopeExit::new(|| ptr.set(old_value));
        ptr.set(&y);
        assert!(std::ptr::eq(&y, ptr.get()));
        restorer.abandon();
    }
    assert!(std::ptr::eq(&y, ptr.get()));
}

#[test]
fn AutoRestorerRetainPtr() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    let ptr = RefCell::new(RetainPtr::new(&obj1));
    assert_eq!(1, obj1.retain_count());
    assert_eq!(0, obj1.release_count());
    {
        let old_value = ptr.borrow().clone();
        let _restorer = ScopeExit::new(|| ptr.borrow_mut().assign(&old_value));
        // |obj1| is kept alive by restorer in case it need to be restored.
        assert_eq!(2, obj1.retain_count());
        assert_eq!(0, obj1.release_count());
        ptr.borrow_mut().Reset(Some(&obj2));
        assert_eq!(Some(&obj2 as *const _), ptr.borrow().Get());

        // |obj1| released by |ptr| assignment.
        assert!(obj1.alive());
        assert_eq!(2, obj1.retain_count());
        assert_eq!(1, obj1.release_count());

        // |obj2| now kept alive by |ptr|.
        assert!(obj1.alive());
        assert_eq!(1, obj2.retain_count());
        assert_eq!(0, obj2.release_count());
    }
    assert_eq!(Some(&obj1 as *const _), ptr.borrow().Get());

    // |obj1| now kept alive again by |ptr|.
    assert!(obj1.alive());
    assert_eq!(3, obj1.retain_count());
    assert_eq!(2, obj1.release_count());

    // |obj2| dead.
    assert!(!(obj2.alive()));
    assert_eq!(1, obj2.retain_count());
    assert_eq!(1, obj2.release_count());
}

#[test]
fn AutoRestorerRetainPtrNoOp() {
    let obj1 = PseudoRetainable::default();
    let ptr = RefCell::new(RetainPtr::new(&obj1));
    assert_eq!(1, obj1.retain_count());
    assert_eq!(0, obj1.release_count());
    {
        let old_value = ptr.borrow().clone();
        let _restorer = ScopeExit::new(|| ptr.borrow_mut().assign(&old_value));
        assert_eq!(2, obj1.retain_count());
        assert_eq!(0, obj1.release_count());
    }
    assert_eq!(Some(&obj1 as *const _), ptr.borrow().Get());

    // Self-reassignement avoided ref churn.
    assert!(obj1.alive());
    assert_eq!(2, obj1.retain_count());
    assert_eq!(1, obj1.release_count());
}

#[test]
fn AutoRestorerRetainPtrAbandon() {
    let obj1 = PseudoRetainable::default();
    let obj2 = PseudoRetainable::default();
    let ptr = RefCell::new(RetainPtr::new(&obj1));
    assert_eq!(1, obj1.retain_count());
    assert_eq!(0, obj1.release_count());
    {
        let old_value = ptr.borrow().clone();
        let restorer = ScopeExit::new(|| ptr.borrow_mut().assign(&old_value));
        // |obj1| is kept alive by restorer in case it need to be restored.
        assert_eq!(2, obj1.retain_count());
        assert_eq!(0, obj1.release_count());
        ptr.borrow_mut().Reset(Some(&obj2));
        assert_eq!(Some(&obj2 as *const _), ptr.borrow().Get());

        // |obj1| released by |ptr| assignment.
        assert_eq!(2, obj1.retain_count());
        assert_eq!(1, obj1.release_count());

        // |obj2| now kept alive by |ptr|.
        assert_eq!(1, obj2.retain_count());
        assert_eq!(0, obj2.release_count());

        restorer.abandon();
    }
    assert_eq!(Some(&obj2 as *const _), ptr.borrow().Get());

    // |obj1| now dead as a result of abandonment.
    assert!(!(obj1.alive()));
    assert_eq!(2, obj1.retain_count());
    assert_eq!(2, obj1.release_count());

    // |obj2| kept alive by |ptr|.
    assert!(obj2.alive());
    assert_eq!(1, obj2.retain_count());
    assert_eq!(0, obj2.release_count());
}
