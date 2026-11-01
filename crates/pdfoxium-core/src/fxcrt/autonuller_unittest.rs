// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/autonuller_unittest.cpp using scoped safe cleanup/restoration, as selected
//! source translations do. Cell/RefCell preserve reads during the scope;
//! abandonment retains the saved value until scope exit, matching native.
#![allow(non_snake_case)]
use super::ownership_test_support::ScopeExit;
use std::cell::Cell;

#[test]
fn AutoNuller() {
    let x = 5;
    let ptr = Cell::new(None);
    {
        let _nuller = ScopeExit::new(|| ptr.set(None));
        ptr.set(Some(&x));
        assert!(std::ptr::eq(&x, ptr.get().unwrap()));
    }
    assert!(ptr.get().is_none());
}

#[test]
fn AutoNullerAbandon() {
    let x = 5;
    let ptr = Cell::new(None);
    {
        let nuller = ScopeExit::new(|| ptr.set(None));
        ptr.set(Some(&x));
        assert!(std::ptr::eq(&x, ptr.get().unwrap()));
        nuller.abandon();
    }
    assert!(std::ptr::eq(&x, ptr.get().unwrap()));
}

#[test]
fn AutoNullerUnownedPtr() {
    let x = 5;
    let ptr = Cell::new(None);
    {
        let _nuller = ScopeExit::new(|| ptr.set(None));
        ptr.set(Some(&x));
        assert!(std::ptr::eq(&x, ptr.get().unwrap()));
    }
    assert!(ptr.get().is_none());
}

#[test]
fn AutoNullerUnownedPtrAbandon() {
    let x = 5;
    let ptr = Cell::new(None);
    {
        let nuller = ScopeExit::new(|| ptr.set(None));
        ptr.set(Some(&x));
        assert!(std::ptr::eq(&x, ptr.get().unwrap()));
        nuller.abandon();
    }
    assert!(std::ptr::eq(&x, ptr.get().unwrap()));
}
