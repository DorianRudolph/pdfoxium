// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
//! Translation of `upstream/pdfium/constants/stream_dict_common.h`.
#![allow(non_upper_case_globals)]

// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
// ISO 32000-1:2008 spec, table 5.
// Entries common to all stream dictionaries.
//
// TODO(https://crbug.com/42270045): Examine all usages of "Length",
// "Filter", and "F".
pub const kLength: &str = "Length";
pub const kFilter: &str = "Filter";
pub const kDecodeParms: &str = "DecodeParms";
pub const kF: &str = "F";
pub const kDL: &str = "DL";
