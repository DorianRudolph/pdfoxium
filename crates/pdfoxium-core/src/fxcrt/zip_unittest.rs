// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/zip_unittest.cpp. Bounds-checked subslices preserve native Zip's
//! first-range length rather than Rust zip's minimum-range rule. Death checks
//! become bounds panics; no native invalid references are constructed.
#![allow(non_snake_case)]

struct NoCopyNoMove {
    value: i32,
}

#[test]
fn EmptyZip() {
    let nothing: &[i32] = &[];
    let stuff = [1, 2, 3];
    let mut zip_nothing_nothing = nothing.iter().zip(nothing[..nothing.len()].iter());
    assert!(zip_nothing_nothing.next().is_none());
    let mut zip_nothing_stuff = nothing.iter().zip(stuff[..nothing.len()].iter());
    assert!(zip_nothing_stuff.next().is_none());
    let mut zip_nothing_nothing_nothing = nothing
        .iter()
        .zip(nothing[..nothing.len()].iter())
        .zip(nothing[..nothing.len()].iter());
    assert!(zip_nothing_nothing_nothing.next().is_none());
    let mut zip_nothing_nothing_stuff = nothing
        .iter()
        .zip(nothing[..nothing.len()].iter())
        .zip(stuff[..nothing.len()].iter());
    assert!(zip_nothing_nothing_stuff.next().is_none());
    let mut zip_nothing_stuff_stuff = nothing
        .iter()
        .zip(stuff[..nothing.len()].iter())
        .zip(stuff[..nothing.len()].iter());
    assert!(zip_nothing_stuff_stuff.next().is_none());
}
#[test]
fn ActualZip() {
    let stuff = [1, 2, 3];
    let expected = [1, 2, 3, 0];
    {
        let mut output = [0; 4];
        for (input, out) in stuff.iter().zip(output[..stuff.len()].iter_mut()) {
            *out = *input;
        }
        assert_eq!(output, expected);
    }
    {
        // Ordering affects the size determination only.
        let mut output = [0; 4];
        let sub_output = &mut output[..3];
        let size = sub_output.len();
        for (out, input) in sub_output.iter_mut().zip(stuff[..size].iter()) {
            *out = *input;
        }
        assert_eq!(output, expected);
    }
}
#[test]
fn ActualZip3() {
    let stuff1 = [1, 2, 3];
    let stuff2 = [4, 5, 6];
    let expected = [5, 7, 9, 0];
    {
        let mut output = [0; 4];
        for ((in1, in2), out) in stuff1
            .iter()
            .zip(stuff2[..stuff1.len()].iter())
            .zip(output[..stuff1.len()].iter_mut())
        {
            *out = *in1 + *in2;
        }
        assert_eq!(output, expected);
    }
    {
        let mut output = [0; 4];
        for ((in1, out), in2) in stuff1
            .iter()
            .zip(output[..stuff1.len()].iter_mut())
            .zip(stuff2[..stuff1.len()].iter())
        {
            *out = *in1 + *in2;
        }
        assert_eq!(output, expected);
    }
    {
        let mut output = [0; 4];
        let sub_output = &mut output[..3];
        let size = sub_output.len();
        for ((out, in1), in2) in sub_output
            .iter_mut()
            .zip(stuff1[..size].iter())
            .zip(stuff2[..size].iter())
        {
            *out = *in1 + *in2;
        }
        assert_eq!(output, expected);
    }
}
#[test]
fn NoCopy() {
    let stuff1 = [
        NoCopyNoMove { value: 1 },
        NoCopyNoMove { value: 2 },
        NoCopyNoMove { value: 3 },
    ];
    let expected = [
        NoCopyNoMove { value: 1 },
        NoCopyNoMove { value: 2 },
        NoCopyNoMove { value: 3 },
    ];
    for (in1, exp) in stuff1.iter().zip(expected[..stuff1.len()].iter()) {
        assert_eq!(exp.value, in1.value);
    }
}
#[test]
fn NoCopy3() {
    let stuff1 = [
        NoCopyNoMove { value: 1 },
        NoCopyNoMove { value: 2 },
        NoCopyNoMove { value: 3 },
    ];
    let stuff2 = [
        NoCopyNoMove { value: 4 },
        NoCopyNoMove { value: 5 },
        NoCopyNoMove { value: 6 },
    ];
    let expected = [
        NoCopyNoMove { value: 5 },
        NoCopyNoMove { value: 7 },
        NoCopyNoMove { value: 9 },
    ];
    for ((in1, in2), exp) in stuff1
        .iter()
        .zip(stuff2[..stuff1.len()].iter())
        .zip(expected[..stuff1.len()].iter())
    {
        assert_eq!(exp.value, in1.value + in2.value);
    }
}
#[test]
#[should_panic]
fn BadArgumentsZip() {
    let nothing: &[i32] = &[];
    let stuff = [1, 2, 3];
    let _ = std::hint::black_box(stuff.iter().zip(nothing[..stuff.len()].iter()));
}
#[test]
#[should_panic]
fn BadArgumentsZip3() {
    let nothing: &[i32] = &[];
    let stuff = [1, 2, 3];
    let _ = std::hint::black_box(
        stuff
            .iter()
            .zip(stuff[..stuff.len()].iter())
            .zip(nothing[..stuff.len()].iter()),
    );
}
