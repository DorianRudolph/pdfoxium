// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
//! Translation of `upstream/pdfium/constants/annotation_flags.h`.
#![allow(non_upper_case_globals)]

// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
// ISO 32000-1:2008 spec, table 165.
pub const kInvisible: u32 = 1 << 0;
pub const kHidden: u32 = 1 << 1;
pub const kPrint: u32 = 1 << 2;
pub const kNoZoom: u32 = 1 << 3;
pub const kNoRotate: u32 = 1 << 4;
pub const kNoView: u32 = 1 << 5;
pub const kReadOnly: u32 = 1 << 6;
pub const kLocked: u32 = 1 << 7;
pub const kToggleNoView: u32 = 1 << 8;
pub const kLockedContents: u32 = 1 << 9;
