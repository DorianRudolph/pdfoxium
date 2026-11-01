// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fx_memory_wrappers_unittest.cpp`.
//! The selected allocator aliases map to typed Box/Vec/String ownership;
//! native compile-time partition exception declarations need no Rust machinery.
#![allow(non_snake_case)]

struct OnlyNumbers {
    x: i32,
    y: i32,
}

#[test]
fn FxFreeDeleter() {
    let empty: Option<Box<[i32]>> = None;
    let thing = Some(vec![0i32; 1].into_boxed_slice());
    let several = Some(vec![0i32; 100].into_boxed_slice());
    assert!(empty.is_none());
    assert!(thing.is_some());
    assert!(several.is_some());
}

#[test]
fn FxAllocAllocator() {
    // Let ASAN sanity check some simple operations.
    let mut vec = Vec::<i32>::new();
    vec.push(42);
    vec.reserve(100);
    vec.resize(20, 0);
    vec[11] = 42;
    let mut vec2 = vec.clone();
    vec = std::mem::take(&mut vec2);
    vec2.resize(0, 0);
    vec2.push(42);
    // Same destruction order as the native local variables.
    drop(vec2);
    drop(vec);
}

#[test]
fn FxStringAllocator() {
    // Let ASAN sanity check some simple operations.
    let mut str = String::new();
    str.push('B');
    str.push_str("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    str.push_str("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    str.push_str("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    super::fx_string_wrappers::write_default_float(&mut str, 42.0f32).unwrap();
}

#[test]
fn FxAllocAllocatorStructException() {
    let mut vec = Vec::<OnlyNumbers>::new();
    vec.push(OnlyNumbers { x: 42, y: 73 });
    assert_eq!(vec.last().unwrap().x, 42);
    assert_eq!(vec.last().unwrap().y, 73);
}
