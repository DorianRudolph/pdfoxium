// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2023 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxge/fx_fontencoding.h`.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FontEncoding {
    AdobeCustom,
    AdobeExpert,
    AdobeStandard,
    AppleRoman,
    Big5,
    GB2312,
    Johab,
    Latin1,
    None,
    OldLatin2,
    Sjis,
    Symbol,
    Unicode,
    Wansung,
}
