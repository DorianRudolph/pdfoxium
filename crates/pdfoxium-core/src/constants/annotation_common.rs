// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
//! Translation of `upstream/pdfium/constants/annotation_common.h`.
#![allow(non_upper_case_globals)]

// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
// ISO 32000-1:2008 spec, table 164.
// Entries common to all annotation dictionaries.
pub const kType: &str = "Type";
pub const kSubtype: &str = "Subtype";
pub const kRect: &str = "Rect";
pub const kContents: &str = "Contents";
pub const kP: &str = "P";
pub const kNM: &str = "NM";
pub const kM: &str = "M";
pub const kF: &str = "F";
pub const kAP: &str = "AP";
pub const kAS: &str = "AS";
pub const kBorder: &str = "Border";
pub const kC: &str = "C";
pub const kStructParent: &str = "StructParent";
pub const kOC: &str = "OC";
// Entries for polygon and polyline annotations.
pub const kVertices: &str = "Vertices";
// Entries for ink annotations
pub const kInkList: &str = "InkList";
// Entries for line annotations
pub const kL: &str = "L";
