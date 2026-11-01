// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/bytestring_pool_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::ByteStringPool;
use std::rc::Rc;

#[test]
fn Basic() {
    let mut pool = ByteStringPool::new();
    let null1: Rc<[u8]> = Rc::from(&b""[..]);
    let null2: Rc<[u8]> = Rc::from(&b""[..]);
    let goats1: Rc<[u8]> = Rc::from(&b"goats"[..]);
    let goats2: Rc<[u8]> = Rc::from(&b"goats"[..]);
    assert!(null1.is_empty());
    assert!(null2.is_empty());
    assert_ne!(goats1.as_ptr(), goats2.as_ptr());

    let interned_null1 = pool.intern_shared(&null1);
    let interned_null2 = pool.intern_shared(&null2);
    let interned_goats1 = pool.intern_shared(&goats1);
    let interned_goats2 = pool.intern_shared(&goats2);
    assert_eq!(null1, interned_null1);
    assert_eq!(null2, interned_null2);
    assert_eq!(goats1, interned_goats1);
    assert_eq!(goats2, interned_goats2);
    assert!(interned_null1.is_empty());
    assert!(interned_null2.is_empty());
    assert_eq!(goats1.as_ptr(), interned_goats1.as_ptr());
    assert_eq!(goats1.as_ptr(), interned_goats2.as_ptr());
}
