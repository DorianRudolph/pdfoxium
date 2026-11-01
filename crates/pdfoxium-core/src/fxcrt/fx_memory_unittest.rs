// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/fx_memory_unittest.cpp selected allocation/overflow contracts.
//! Typed raw allocations become owned zeroed bytes; safe initialization also
//! satisfies native uninitialized-allocation cases, which never read the data.
//! Failure leaves existing Vec storage untouched. Native aborts become panics.
#![allow(non_snake_case, non_upper_case_globals)]

use super::{checked_allocation_size, try_alloc_zeroed};
use std::panic::{catch_unwind, AssertUnwindSafe};

const kMaxByteAlloc: usize = usize::MAX;
const kMaxIntAlloc: usize = kMaxByteAlloc / size_of::<i32>();
const kOverflowIntAlloc: usize = kMaxIntAlloc + 100;
const kWidth: usize = 640;
const kOverflowIntAlloc2D: usize = kMaxIntAlloc / kWidth + 10;
const kCloseToMaxIntAlloc: usize = kMaxIntAlloc - 100;
const kCloseToMaxByteAlloc: usize = kMaxByteAlloc - 100;
const _: () = assert!(usize::MAX % 2 == 1);

fn alloc_int(count: usize) -> Option<Vec<u8>> {
    try_alloc_zeroed(checked_allocation_size(count, size_of::<i32>())?)
}
fn alloc_int_2d(w: usize, h: usize) -> Option<Vec<u8>> {
    alloc_int(w.checked_mul(h)?)
}
fn realloc_int(ptr: &mut Vec<u8>, count: usize) -> Option<()> {
    let size = checked_allocation_size(count, size_of::<i32>())?;
    if size > ptr.len() {
        ptr.try_reserve_exact(size - ptr.len()).ok()?;
    }
    ptr.resize(size, 0);
    Some(())
}
#[test]
fn FXAllocOOM() {
    assert!(catch_unwind(|| alloc_int(kCloseToMaxIntAlloc).expect("FX_Alloc")).is_err());
    let mut ptr = alloc_int(1).unwrap();
    assert!(!ptr.is_empty());
    assert!(catch_unwind(AssertUnwindSafe(|| realloc_int(
        &mut ptr,
        kCloseToMaxIntAlloc
    )
    .expect("FX_Realloc")))
    .is_err());
    drop(ptr);
}
#[test]
fn FXAllocOverflow() {
    assert!(catch_unwind(|| alloc_int(kOverflowIntAlloc).expect("FX_Alloc")).is_err());
    let mut ptr = alloc_int(1).unwrap();
    assert!(!ptr.is_empty());
    assert!(catch_unwind(AssertUnwindSafe(|| realloc_int(
        &mut ptr,
        kOverflowIntAlloc
    )
    .expect("FX_Realloc")))
    .is_err());
    drop(ptr);
}
#[test]
fn FXAllocOverflow2D() {
    assert!(
        catch_unwind(|| alloc_int_2d(kWidth, kOverflowIntAlloc2D).expect("FX_Alloc2D")).is_err()
    );
}
#[test]
fn FXTryAllocOOM() {
    assert!(alloc_int(kCloseToMaxIntAlloc).is_none());
    let mut ptr = alloc_int(1).unwrap();
    assert!(!ptr.is_empty());
    assert!(realloc_int(&mut ptr, kCloseToMaxIntAlloc).is_none());
    drop(ptr);
}
#[test]
fn FXTryAllocUninit() {
    let ptr = alloc_int(4).unwrap();
    assert!(!ptr.is_empty());
    drop(ptr);
    let ptr = alloc_int_2d(4, 4).unwrap();
    assert!(!ptr.is_empty());
    drop(ptr);
}
#[test]
fn FXTryAllocUninitOOM() {
    assert!(alloc_int(kCloseToMaxIntAlloc).is_none());
    assert!(alloc_int_2d(kWidth, kOverflowIntAlloc2D).is_none());
}
#[test]
fn FXTryAllocOverflow() {
    assert!(alloc_int(kOverflowIntAlloc).is_none());
    let mut ptr = alloc_int(1).unwrap();
    assert!(!ptr.is_empty());
    ptr[..4].copy_from_slice(&1492i32.to_ne_bytes());
    assert!(realloc_int(&mut ptr, kOverflowIntAlloc).is_none());
    assert_eq!(1492, i32::from_ne_bytes(ptr[..4].try_into().unwrap()));
    drop(ptr);
}
#[test]
fn FXMEMDefaultOOM() {
    assert!(try_alloc_zeroed(kCloseToMaxByteAlloc).is_none());
    let mut ptr = try_alloc_zeroed(1).unwrap();
    assert!(!ptr.is_empty());
    // DefaultRealloc shares the same size predicate; no live buffer is replaced
    // unless reservation succeeds, as in typed reallocation above.
    let result = (|| {
        let size = checked_allocation_size(kCloseToMaxByteAlloc, 1)?;
        ptr.try_reserve_exact(size.saturating_sub(ptr.len())).ok()?;
        ptr.resize(size, 0);
        Some(())
    })();
    assert!(result.is_none());
    drop(ptr);
}
#[test]
fn AllocZeroesMemory() {
    let ptr = try_alloc_zeroed(32).unwrap();
    assert!(!ptr.is_empty());
    for i in 0..32 {
        assert_eq!(0, ptr[i]);
    }
    drop(ptr);
}
#[test]
fn FXAlign() {
    let s0 = 0usize;
    let s1 = 1usize;
    let s2 = 2usize;
    let sbig = usize::MAX - 2;
    assert_eq!(0usize, (s0 + (2 - 1)) & !(2 - 1));
    assert_eq!(2usize, (s1 + (2 - 1)) & !(2 - 1));
    assert_eq!(2usize, (s2 + (2 - 1)) & !(2 - 1));
    assert_eq!(usize::MAX - 1, (sbig + (2 - 1)) & !(2 - 1));
    let i0 = 0i32;
    let i511 = 511i32;
    let i512 = 512i32;
    let ineg = -513i32;
    assert_eq!(0, (i0 + (512 - 1)) & !(512 - 1));
    assert_eq!(512, (i511 + (512 - 1)) & !(512 - 1));
    assert_eq!(512, (i512 + (512 - 1)) & !(512 - 1));
    assert_eq!(-512, (ineg + (512 - 1)) & !(512 - 1));
}
