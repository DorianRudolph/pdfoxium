// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
//! Translation of `upstream/pdfium/constants/access_permissions.h`.
#![allow(non_upper_case_globals)]

// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
// ISO 32000-1:2008 spec, table 22.
// User access permissions.
pub const kModifyContent: u32 = 1 << 3;
pub const kModifyAnnotation: u32 = 1 << 5;
pub const kFillForm: u32 = 1 << 8;
pub const kExtractForAccessibility: u32 = 1 << 9;
