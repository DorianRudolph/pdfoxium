// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of core/fxcrt/mask_unittest.cpp over selected Rust primitives.
#![allow(non_snake_case, non_upper_case_globals, non_camel_case_types)]

// Native Mask<Privilege> is represented by its selected integer bit flags.
#[repr(u8)]
#[derive(Clone, Copy)]
enum Privilege {
    kPriv1 = 1 << 0,
    kPriv2 = 1 << 1,
    kPriv4 = 1 << 2,
    kPriv8 = 1 << 3,
    kPriv16 = 1 << 4,
    kPriv32 = 1 << 5,
    kPriv64 = 1 << 6,
    kPriv128 = 1 << 7,
}
const kAllMask: u8 = Privilege::kPriv1 as u8
    | Privilege::kPriv2 as u8
    | Privilege::kPriv4 as u8
    | Privilege::kPriv8 as u8
    | Privilege::kPriv16 as u8
    | Privilege::kPriv32 as u8
    | Privilege::kPriv64 as u8
    | Privilege::kPriv128 as u8;
const _: () = assert!(size_of::<u8>() == size_of::<Privilege>());

#[test]
fn Empty() {
    let privs: u8 = 0;
    assert_eq!(0u8, privs);
    assert!((privs & (Privilege::kPriv1 as u8)) == 0);
    assert!((privs & (Privilege::kPriv4 as u8)) == 0);
    assert!((privs & (Privilege::kPriv8 as u8)) == 0);
    assert!((privs & kAllMask) == 0);
}

#[test]
fn FromOne() {
    let privs: u8 = Privilege::kPriv1 as u8;
    assert_eq!(1u8, privs);
    assert!(privs & (Privilege::kPriv1 as u8) != 0);
    assert!((privs & (Privilege::kPriv4 as u8)) == 0);
    assert!((privs & (Privilege::kPriv8 as u8)) == 0);
    assert!(privs & kAllMask != 0);
}

#[test]
fn FromTwo() {
    // Not adjacent bits, just because.
    let privs: u8 = (Privilege::kPriv1 as u8) | (Privilege::kPriv8 as u8);
    assert_eq!(9u8, privs);
    assert!(privs & (Privilege::kPriv1 as u8) != 0);
    assert!((privs & (Privilege::kPriv4 as u8)) == 0);
    assert!(privs & (Privilege::kPriv8 as u8) != 0);
    assert!(privs & kAllMask != 0);
}

#[test]
fn FromThree() {
    let privs: u8 =
        (Privilege::kPriv1 as u8) | (Privilege::kPriv2 as u8) | (Privilege::kPriv4 as u8);
    assert_eq!(7u8, privs);
}

#[test]
fn FromFour() {
    let privs: u8 = (Privilege::kPriv1 as u8)
        | (Privilege::kPriv2 as u8)
        | (Privilege::kPriv4 as u8)
        | (Privilege::kPriv8 as u8);
    assert_eq!(15u8, privs);
}

#[test]
fn FromFive() {
    let privs: u8 = (Privilege::kPriv1 as u8)
        | (Privilege::kPriv2 as u8)
        | (Privilege::kPriv4 as u8)
        | (Privilege::kPriv8 as u8)
        | (Privilege::kPriv16 as u8);
    assert_eq!(31u8, privs);
}

#[test]
fn FromSix() {
    let privs: u8 = (Privilege::kPriv1 as u8)
        | (Privilege::kPriv2 as u8)
        | (Privilege::kPriv4 as u8)
        | (Privilege::kPriv8 as u8)
        | (Privilege::kPriv16 as u8)
        | (Privilege::kPriv32 as u8);
    assert_eq!(63u8, privs);
}

#[test]
fn FromSeven() {
    let privs: u8 = (Privilege::kPriv1 as u8)
        | (Privilege::kPriv2 as u8)
        | (Privilege::kPriv4 as u8)
        | (Privilege::kPriv8 as u8)
        | (Privilege::kPriv16 as u8)
        | (Privilege::kPriv32 as u8)
        | (Privilege::kPriv64 as u8);
    assert_eq!(127u8, privs);
}

#[test]
fn FromEight() {
    let privs: u8 = (Privilege::kPriv1 as u8)
        | (Privilege::kPriv2 as u8)
        | (Privilege::kPriv4 as u8)
        | (Privilege::kPriv8 as u8)
        | (Privilege::kPriv16 as u8)
        | (Privilege::kPriv32 as u8)
        | (Privilege::kPriv64 as u8)
        | (Privilege::kPriv128 as u8);
    assert_eq!(255u8, privs);
}

#[test]
fn FromUnderlying() {
    let privs: u8 = 5;
    assert_eq!(5u8, privs);
    assert!(privs & (Privilege::kPriv1 as u8) != 0);
    assert!(privs & (Privilege::kPriv4 as u8) != 0);
    assert!((privs & (Privilege::kPriv8 as u8)) == 0);
}

#[test]
fn AssignAndEQ() {
    let source: u8 = (Privilege::kPriv1 as u8) | (Privilege::kPriv8 as u8);
    let other: u8 = Privilege::kPriv1 as u8;
    let dest: u8;
    dest = source;
    assert_eq!(9u8, dest);
    assert_eq!(source, dest);
    assert_ne!(other, dest);
}

#[test]
fn OrAndAnd() {
    let source: u8 = (Privilege::kPriv1 as u8) | (Privilege::kPriv8 as u8);
    let or_result: u8 = source | ((Privilege::kPriv1 as u8) | (Privilege::kPriv4 as u8));
    let and_result: u8 = source & ((Privilege::kPriv1 as u8) | (Privilege::kPriv4 as u8));
    assert_eq!(13u8, or_result);
    assert_eq!(1u8, and_result);
}

#[test]
fn OrEqualsAndAndEquals() {
    let mut source_or: u8 = (Privilege::kPriv1 as u8) | (Privilege::kPriv8 as u8);
    let mut source_and: u8 = (Privilege::kPriv1 as u8) | (Privilege::kPriv8 as u8);
    source_or |= (Privilege::kPriv1 as u8) | (Privilege::kPriv4 as u8);
    source_and &= (Privilege::kPriv1 as u8) | (Privilege::kPriv4 as u8);
    assert_eq!(13u8, source_or);
    assert_eq!(1u8, source_and);
}

#[test]
fn Clear() {
    let mut source: u8 = kAllMask;
    std::hint::black_box(source);
    source = 0;
    assert_eq!(0u8, source);
}

#[test]
fn ClearSpecificBits() {
    let mut source: u8 = kAllMask;
    source &= !((Privilege::kPriv1 as u8) | (Privilege::kPriv4 as u8));
    assert_eq!(250u8, source);
}

#[test]
fn TestAll() {
    let source: u8 =
        (Privilege::kPriv1 as u8) | (Privilege::kPriv8 as u8) | (Privilege::kPriv64 as u8);
    let passes: u8 = (Privilege::kPriv1 as u8) | (Privilege::kPriv64 as u8);
    let fails: u8 = (Privilege::kPriv1 as u8) | (Privilege::kPriv32 as u8);
    assert!((source & passes) == passes);
    assert!(!((source & fails) == fails));
}
