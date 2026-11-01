// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxcrt/fx_ceil_log2.h`.

#[inline]
pub const fn ceil_log2(value: u32) -> u8 {
    if value <= 1 {
        0
    } else {
        (u32::BITS - (value - 1).leading_zeros()) as u8
    }
}
