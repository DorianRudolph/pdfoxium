// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Portable `FX_FILESIZE` representation from `core/fxcrt/fx_types.h`.

/// PDFium uses a signed 64-bit file offset on the supported desktop targets.
pub type FxFileSize = i64;
