// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Empty property callbacks in `cjx_encrypt.cpp`, `cjx_extras.cpp`,
//! `cjx_handler.cpp`, `cjx_delta.cpp`, and `cjx_source.cpp`. Return undefined
//! on reads and ignore writes without conversion, exactly as the source does.
pub(crate) fn is_noop_property(callback: &str) -> bool {
    matches!(
        callback,
        "CJX_Encrypt::format"
            | "CJX_Extras::type"
            | "CJX_Handler::version"
            | "CJX_Delta::currentValue"
            | "CJX_Delta::savedValue"
            | "CJX_Delta::target"
            | "CJX_Source::db"
    )
}
