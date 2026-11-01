// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Barcode types from `fxbarcode/BC_Library.{h,cpp}`. Immutable Rust tables
//! replace the source's explicit initialization/finalization of global objects.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum BcTextLoc {
    #[default]
    None = 0,
    Above,
    Below,
    AboveEmbed,
    BelowEmbed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(i8)]
pub enum BcType {
    #[default]
    Unknown = -1,
    Code39 = 0,
    Codabar,
    Code128,
    Code128B,
    Code128C,
    Ean8,
    Upca,
    Ean13,
    QrCode,
    Pdf417,
    DataMatrix,
}
