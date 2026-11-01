// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fixed_size_data_vector_unittest.cpp`.
//! Selected storage is Vec/slices. Uninit is initialized safely before use;
//! mem::take preserves the native moved-from state and allocation identity.
#![allow(non_snake_case)]

fn try_zeroed(count: usize) -> Vec<i32> {
    let mut data = Vec::new();
    if data.try_reserve_exact(count).is_err() {
        return data;
    }
    data.resize(count, 0);
    data
}

#[test]
fn NoData() {
    let vec: Vec<i32> = Vec::new();
    assert_eq!(0, vec.len());
    assert!(vec.is_empty());
    assert!(vec.as_slice().is_empty());
}

#[test]
fn UninitData() {
    let mut vec = vec![0i32; 4];
    assert!(!vec.is_empty());
    assert_eq!(4, vec.len());
    assert_eq!(4, vec.as_slice().len());

    let kData: [i32; 4] = [1, 2, 3, 4];
    vec.as_mut_slice().copy_from_slice(&kData);
    assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);
}

#[test]
fn TryUninitData() {
    let mut vec = try_zeroed(4);
    assert!(!vec.is_empty());
    assert_eq!(4, vec.len());
    assert_eq!(4, vec.as_slice().len());

    let kData: [i32; 4] = [1, 2, 3, 4];
    vec.as_mut_slice().copy_from_slice(&kData);
    assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);
}

#[test]
fn ZeroedData() {
    let mut vec = vec![0i32; 4];
    assert!(!vec.is_empty());
    assert_eq!(4, vec.len());
    assert_eq!(4, vec.as_slice().len());
    assert_eq!(vec.as_slice(), &[0, 0, 0, 0]);

    let kData: [i32; 4] = [1, 2, 3, 4];
    vec.as_mut_slice().copy_from_slice(&kData);
    assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);
}

#[test]
fn TryZeroedData() {
    let mut vec = try_zeroed(4);
    assert!(!vec.is_empty());
    assert_eq!(4, vec.len());
    assert_eq!(4, vec.as_slice().len());
    assert_eq!(vec.as_slice(), &[0, 0, 0, 0]);

    let kData: [i32; 4] = [1, 2, 3, 4];
    vec.as_mut_slice().copy_from_slice(&kData);
    assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);
}

#[test]
fn TryAllocFailures() {
    let kCloseToMaxByteAlloc = usize::MAX - 100;
    let mut vec = try_zeroed(kCloseToMaxByteAlloc);
    assert!(vec.is_empty());
    assert_eq!(0, vec.len());
    assert_eq!(0, vec.as_slice().len());

    vec = try_zeroed(kCloseToMaxByteAlloc);
    assert!(vec.is_empty());
    assert_eq!(0, vec.len());
    assert_eq!(0, vec.as_slice().len());
}

#[test]
fn MoveConstruct() {
    let kData: [i32; 4] = [1, 2, 3, 4];
    let mut vec = vec![0i32; 4];
    assert_eq!(4, vec.as_slice().len());
    vec.as_mut_slice().copy_from_slice(&kData);
    let original_data_ptr = vec.as_ptr();

    let mut vec2 = std::mem::take(&mut vec);
    assert!(!vec2.is_empty());
    assert_eq!(4, vec2.len());
    assert_eq!(4, vec2.as_slice().len());
    assert_eq!(vec2.as_slice(), &[1, 2, 3, 4]);
    assert_eq!(vec2.as_ptr(), original_data_ptr);

    assert_eq!(0, vec.len());
    assert!(vec.is_empty());
    assert!(vec.as_slice().is_empty());

    drop(std::mem::replace(&mut vec, std::mem::take(&mut vec2)));
    assert!(!vec.is_empty());
    assert_eq!(4, vec.len());
    assert_eq!(4, vec.as_slice().len());
    assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);
    assert_eq!(vec.as_ptr(), original_data_ptr);

    assert_eq!(0, vec2.len());
    assert!(vec2.is_empty());
    assert!(vec2.as_slice().is_empty());
}

#[test]
fn MoveAssign() {
    let mut vec = Vec::<i32>::new();
    let mut vec2 = vec![0i32; 4];
    let kData: [i32; 4] = [1, 2, 3, 4];
    assert_eq!(4, vec2.as_slice().len());
    vec2.as_mut_slice().copy_from_slice(&kData);

    drop(std::mem::replace(&mut vec, std::mem::take(&mut vec2)));
    assert!(vec2.is_empty());
    assert_eq!(4, vec.as_slice().len());
    assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);
}

#[test]
fn TruncatedFrom() {
    let kData: [i32; 4] = [1, 2, 3, 4];
    let mut vec1 = vec![0i32; 4];
    vec1.as_mut_slice().copy_from_slice(&kData);

    let mut vec2 = std::mem::take(&mut vec1);
    vec2.truncate(3);
    assert_eq!(0, vec1.as_slice().len());
    // Rust empty slices use a non-null dangling pointer; zero capacity proves
    // the source no longer owns the moved allocation.
    assert_eq!(0usize, vec1.capacity());
    assert_eq!(vec2.as_slice(), &[1, 2, 3]);
}

#[test]
fn Subspan() {
    let mut vec = vec![0u32; 4];
    for (i, value) in vec.iter_mut().enumerate() {
        *value = i as u32;
    }

    let empty = &vec[2..2];
    assert!(empty.is_empty());

    let first = &vec[0..1];
    assert_eq!(first.len(), 1);
    assert_eq!(first[0], 0);

    let mids = &vec[1..3];
    assert_eq!(mids.len(), 2);
    assert_eq!(mids[0], 1);
    assert_eq!(mids[1], 2);

    let rest = &vec[3..];
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0], 3);
}

#[test]
fn First() {
    let mut vec = vec![0u32; 4];
    for (i, value) in vec.iter_mut().enumerate() {
        *value = i as u32;
    }

    let empty = &vec[..0];
    assert!(empty.is_empty());

    let some = &vec[..2];
    assert_eq!(some.len(), 2);
    assert_eq!(some[0], 0);
    assert_eq!(some[1], 1);
}

#[test]
fn Last() {
    let mut vec = vec![0u32; 4];
    for (i, value) in vec.iter_mut().enumerate() {
        *value = i as u32;
    }

    let empty = &vec[..0];
    assert!(empty.is_empty());

    let some = &vec[..2];
    assert_eq!(some.len(), 2);
    assert_eq!(some[0], 0);
    assert_eq!(some[1], 1);
}
