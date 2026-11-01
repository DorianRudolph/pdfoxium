// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/fx_random_unittest.cpp`.
#![allow(non_snake_case)]
use std::collections::BTreeSet;

#[test]
fn Fill3600times() {
    // Prove this doesn't spin wait for a second each time.
    // Global seeds are sequential and won't collide until 2^32 calls;
    // different seeds should not produce identical sequences in this range.
    let mut seen = BTreeSet::new();
    let mut current = [0u32; 16];
    for _ in 0..3600 {
        super::fill(&mut current);
        assert!(seen.insert(current));
    }
}
