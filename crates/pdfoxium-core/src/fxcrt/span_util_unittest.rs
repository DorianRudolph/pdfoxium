// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/span_util_unittest.cpp over selected safe slice operations.
//! Remaining mutable spans are observed by pointer/length before reading the
//! destination, then their borrows end. Word conversion uses native-endian
//! chunks, not an aligned raw-pointer cast. Empty slice pointers are non-null.
#![allow(non_snake_case, non_upper_case_globals)]

fn spancpy<'a>(dst: &'a mut [u8], src: &[u8]) -> &'a mut [u8] {
    let (copy, remain) = dst.split_at_mut(src.len());
    copy.copy_from_slice(src);
    remain
}
fn try_spancpy(dst: &mut [u8], src: &[u8]) -> bool {
    let Some(copy) = dst.get_mut(..src.len()) else {
        return false;
    };
    copy.copy_from_slice(src);
    true
}

#[test]
fn Spancpy_FitsEntirely() {
    let src = vec![b'A'; 4];
    let mut dst = vec![b'B'; 4];
    let remain = spancpy(&mut dst, &src);
    let empty = remain.is_empty();
    assert_eq!(dst[0], b'A');
    assert_eq!(dst[1], b'A');
    assert_eq!(dst[2], b'A');
    assert_eq!(dst[3], b'A');
    assert!(empty);
}
#[test]
fn Spancpy_FitsWithin() {
    let src = vec![b'A'; 2];
    let mut dst = vec![b'B'; 4];
    // Also show that a const src argument is acceptable.
    let remain = spancpy(&mut dst[1..], &src);
    let (size, data) = (remain.len(), remain.as_ptr());
    assert_eq!(dst[0], b'B');
    assert_eq!(dst[1], b'A');
    assert_eq!(dst[2], b'A');
    assert_eq!(dst[3], b'B');
    assert_eq!(size, 1usize);
    assert!(std::ptr::eq(data, &dst[3]));
}
#[test]
fn Spancpy_EmptyCopyWithin() {
    let src = vec![b'A'; 2];
    let mut dst = vec![b'B'; 4];
    let remain = spancpy(&mut dst[1..], &src[2..]);
    let (size, data) = (remain.len(), remain.as_ptr());
    assert_eq!(dst[0], b'B');
    assert_eq!(dst[1], b'B');
    assert_eq!(dst[2], b'B');
    assert_eq!(dst[3], b'B');
    assert_eq!(size, 3usize);
    assert!(std::ptr::eq(data, &dst[1]));
}
#[test]
fn Spancpy_EmptyCopyToEmpty() {
    let src = vec![b'A'; 2];
    let mut dst = vec![b'B'; 4];
    let remain = spancpy(&mut dst[4..], &src[2..]);
    let empty = remain.is_empty();
    assert_eq!(dst[0], b'B');
    assert_eq!(dst[1], b'B');
    assert_eq!(dst[2], b'B');
    assert_eq!(dst[3], b'B');
    assert!(empty);
}
#[test]
fn Spancpy_TryFitsEntirely() {
    let src = vec![b'A'; 4];
    let mut dst = vec![b'B'; 4];
    assert!(try_spancpy(&mut dst, &src));
    assert_eq!(dst[0], b'A');
    assert_eq!(dst[1], b'A');
    assert_eq!(dst[2], b'A');
    assert_eq!(dst[3], b'A');
}
#[test]
fn Spancpy_TryDoesNotFit() {
    let src = vec![b'A'; 4];
    let mut dst = vec![b'B'; 3];
    assert!(!try_spancpy(&mut dst, &src));
    assert_eq!(dst[0], b'B');
    assert_eq!(dst[1], b'B');
    assert_eq!(dst[2], b'B');
}
#[test]
fn Spanmove_FitsWithin() {
    let src = vec![b'A'; 2];
    let mut dst = vec![b'B'; 4];
    // Disjoint source/destination use the same safe copy operation.
    let remain = spancpy(&mut dst[1..], &src);
    let (size, data) = (remain.len(), remain.as_ptr());
    assert_eq!(dst[0], b'B');
    assert_eq!(dst[1], b'A');
    assert_eq!(dst[2], b'A');
    assert_eq!(dst[3], b'B');
    assert_eq!(size, 1usize);
    assert!(std::ptr::eq(data, &dst[3]));
}
#[test]
fn Spanmove_TryFitsEntirely() {
    let src = vec![b'A'; 4];
    let mut dst = vec![b'B'; 4];
    assert!(try_spancpy(&mut dst, &src));
    assert_eq!(dst[0], b'A');
    assert_eq!(dst[1], b'A');
    assert_eq!(dst[2], b'A');
    assert_eq!(dst[3], b'A');
}
#[test]
fn Spanmove_TrySelfIntersect() {
    {
        let mut vec = vec![b'A', b'B', b'C', b'D'];
        // Bounds validation and copying have no overlapping mutable borrows.
        let success = vec.get(..3).is_some() && vec.get(1..).is_some();
        if success {
            vec.copy_within(1..4, 0);
        }
        assert!(success);
        assert_eq!(vec[0], b'B');
        assert_eq!(vec[1], b'C');
        assert_eq!(vec[2], b'D');
        assert_eq!(vec[3], b'D');
    }
    {
        let mut vec = vec![b'A', b'B', b'C', b'D'];
        let success = vec.get(1..).is_some() && vec.get(..3).is_some();
        if success {
            vec.copy_within(0..3, 1);
        }
        assert!(success);
        assert_eq!(vec[0], b'A');
        assert_eq!(vec[1], b'A');
        assert_eq!(vec[2], b'B');
        assert_eq!(vec[3], b'C');
    }
}
#[test]
fn Spanmove_TryDoesNotFit() {
    let src = vec![b'A'; 4];
    let mut dst = vec![b'B'; 3];
    assert!(!try_spancpy(&mut dst, &src));
    assert_eq!(dst[0], b'B');
    assert_eq!(dst[1], b'B');
    assert_eq!(dst[2], b'B');
}
#[test]
fn AssignOverOnePastEnd() {
    let src = vec![b'A'; 2];
    let mut span = &src[..];
    span = &span[2..];
    // Keep the one-past-end view observably evaluated before reassignment.
    std::hint::black_box(span);
    span = &src;
    assert_eq!(span.len(), 2usize);
}
#[test]
fn ReinterpretSpan_Empty() {
    let empty: &[u8] = &[];
    let converted = empty
        .chunks_exact(4)
        .map(|s| u32::from_ne_bytes(s.try_into().unwrap()));
    // No native null pointer is constructed: an empty iterator has no element.
    assert_eq!(converted.clone().next(), None);
    assert_eq!(converted.len(), 0usize);
}
#[test]
fn ReinterpretSpan_LegalConversions() {
    let aaaabbbb = [0x61u8, 0x61, 0x61, 0x61, 0x62, 0x62, 0x62, 0x62];
    let original = &aaaabbbb[..];
    let mut converted = original
        .chunks_exact(4)
        .map(|s| u32::from_ne_bytes(s.try_into().unwrap()));
    assert!(converted.clone().next().is_some());
    assert_eq!(converted.len(), 2usize);
    assert_eq!(converted.next().unwrap(), 0x61616161u32);
    assert_eq!(converted.next().unwrap(), 0x62626262u32);
}
#[test]
fn FlattenMultiDimension() {
    #[derive(Clone, Copy, Default)]
    struct Pt {
        x: f32,
        y: f32,
    }
    type Line = [Pt; 4];
    type Plane = [Line; 4];
    type Volume = [Plane; 4];
    let mut box_: Volume = [[[Pt::default(); 4]; 4]; 4];
    assert_eq!(64usize, box_.iter().flatten().flatten().count());
    let flat = box_.iter_mut().flatten().flatten();
    let mut ctr = 0.0f32;
    for pt in flat {
        pt.x = ctr;
        ctr += 1.0;
        pt.y = ctr;
        ctr += 1.0;
    }
    assert_eq!(box_[3][3][3].x, 126.0f32);
    assert_eq!(box_[3][3][3].y, 127.0f32);
}
fn spanpos(haystack: &[u32], needle: &[u32]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|part| part == needle)
}
#[test]
fn Spanpos_Empty() {
    let kEmpty: &[u32] = &[];
    let kHaystack = [0, 1, 2, 3, 4, 5];
    let kNeedle = [1, 2];
    assert_eq!(spanpos(kEmpty, kEmpty), None);
    assert_eq!(spanpos(&kHaystack, kEmpty), None);
    assert_eq!(spanpos(kEmpty, &kNeedle), None);
}
#[test]
fn Spanpos_NotEmpty() {
    let kHaystack = [0, 1, 2, 3, 4, 5];
    let kStartMatch = [0, 1];
    let kEndMatch = [4, 5];
    let kNotFound = [256, 512]; // test byte-shifted {1,2}.
    let kTooLong = [0, 1, 2, 3, 4, 5, 6];
    assert_eq!(spanpos(&kHaystack, &kStartMatch), Some(0usize));
    assert_eq!(spanpos(&kHaystack, &kEndMatch), Some(4usize));
    assert_eq!(spanpos(&kHaystack, &kNotFound), None);
    assert_eq!(spanpos(&kHaystack, &kTooLong), None);
}
