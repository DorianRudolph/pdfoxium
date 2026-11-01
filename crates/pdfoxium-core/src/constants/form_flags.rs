//! Translation of `upstream/pdfium/constants/form_flags.h`.
#![allow(non_upper_case_globals)]

// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
// ISO 32000-1:2008 spec, table 221.
// Field flags common to all field types.
pub const kReadOnly: u32 = 1 << 0;
pub const kRequired: u32 = 1 << 1;
pub const kNoExport: u32 = 1 << 2;
// ISO 32000-1:2008 spec, table 226.
// Field flags specific to button fields.
pub const kButtonNoToggleToOff: u32 = 1 << 14;
pub const kButtonRadio: u32 = 1 << 15;
pub const kButtonPushbutton: u32 = 1 << 16;
pub const kButtonRadiosInUnison: u32 = 1 << 25;
// ISO 32000-1:2008 spec, table 228.
// Field flags specific to text fields.
pub const kTextMultiline: u32 = 1 << 12;
pub const kTextPassword: u32 = 1 << 13;
pub const kTextFileSelect: u32 = 1 << 20;
pub const kTextDoNotSpellCheck: u32 = 1 << 22;
pub const kTextDoNotScroll: u32 = 1 << 23;
pub const kTextComb: u32 = 1 << 24;
pub const kTextRichText: u32 = 1 << 25;
// ISO 32000-1:2008 spec, table 230.
// Field flags specific to choice fields.
pub const kChoiceCombo: u32 = 1 << 17;
pub const kChoiceEdit: u32 = 1 << 18;
pub const kChoiceSort: u32 = 1 << 19;
pub const kChoiceMultiSelect: u32 = 1 << 21;
pub const kChoiceDoNotSpellCheck: u32 = 1 << 22;
pub const kChoiceCommitOnSelChange: u32 = 1 << 26;
