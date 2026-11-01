// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/dib/cfx_cmyk_to_srgb_unittest.cpp`.

#![allow(non_snake_case, non_upper_case_globals)]

use super::adobe_cmyk_to_standard_rgb_f;

#[test]
fn CMYKRounding() {
    // Keep the entire native known-dangerous float range. to_bits/from_bits
    // replace the native union's representation access without aliasing.
    const kStartValue: f32 = 0.001;
    const kEndValue: f32 = 0.003;
    for bits in kStartValue.to_bits()..kEndValue.to_bits() {
        let f = f32::from_bits(bits);
        let _rgb = adobe_cmyk_to_standard_rgb_f(f, f, f, f);
    }
    let _rgb = adobe_cmyk_to_standard_rgb_f(0.0, 0.25, 0.5, 1.0);
}
