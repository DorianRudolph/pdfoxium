// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/dib/fx_dib_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::{alpha_merge, argb_to_bgra_struct};

#[test]
fn ArgbToBGRAStruct() {
    let white = argb_to_bgra_struct(0xffffffff);
    assert_eq!(0xff, white.blue);
    assert_eq!(0xff, white.green);
    assert_eq!(0xff, white.red);
    assert_eq!(0xff, white.alpha);

    let black = argb_to_bgra_struct(0xff000000);
    assert_eq!(0, black.blue);
    assert_eq!(0, black.green);
    assert_eq!(0, black.red);
    assert_eq!(0xff, black.alpha);

    let abeebead = argb_to_bgra_struct(0xabeebead);
    assert_eq!(0xad, abeebead.blue);
    assert_eq!(0xbe, abeebead.green);
    assert_eq!(0xee, abeebead.red);
    assert_eq!(0xab, abeebead.alpha);
}

#[test]
fn AlphaMerge() {
    assert_eq!(0, alpha_merge(0, 0, 0));
    assert_eq!(0, alpha_merge(0, 0, 127));
    assert_eq!(0, alpha_merge(0, 0, 255));
    assert_eq!(0, alpha_merge(0, 127, 0));
    assert_eq!(63, alpha_merge(0, 127, 127));
    assert_eq!(127, alpha_merge(0, 127, 255));
    assert_eq!(0, alpha_merge(0, 255, 0));
    assert_eq!(127, alpha_merge(0, 255, 127));
    assert_eq!(255, alpha_merge(0, 255, 255));
    assert_eq!(127, alpha_merge(127, 0, 0));
    assert_eq!(63, alpha_merge(127, 0, 127));
    assert_eq!(0, alpha_merge(127, 0, 255));
    assert_eq!(127, alpha_merge(127, 127, 0));
    assert_eq!(127, alpha_merge(127, 127, 127));
    assert_eq!(127, alpha_merge(127, 127, 255));
    assert_eq!(127, alpha_merge(127, 255, 0));
    assert_eq!(190, alpha_merge(127, 255, 127));
    assert_eq!(255, alpha_merge(127, 255, 255));
    assert_eq!(255, alpha_merge(255, 0, 0));
    assert_eq!(128, alpha_merge(255, 0, 127));
    assert_eq!(0, alpha_merge(255, 0, 255));
    assert_eq!(255, alpha_merge(255, 127, 0));
    assert_eq!(191, alpha_merge(255, 127, 127));
    assert_eq!(127, alpha_merge(255, 127, 255));
    assert_eq!(255, alpha_merge(255, 255, 0));
    assert_eq!(255, alpha_merge(255, 255, 127));
    assert_eq!(255, alpha_merge(255, 255, 255));
}
