// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Owned value trees from `fxjs/cfx_keyvalue.{h,cpp}`. A tagged Rust enum
//! replaces the source's discriminator and inactive scalar/object fields.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DataType {
    Number = 0,
    Boolean = 1,
    String = 2,
    Object = 3,
    Null = 4,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum CfxValue {
    Number(f64),
    Boolean(bool),
    /// ByteString storage. The existing FX UTF-8 boundary also retains lone
    /// surrogate code points, so the script binding can round-trip UTF-16.
    String(Vec<u8>),
    Object(Vec<CfxKeyValue>),
    #[default]
    Null,
}
impl CfxValue {
    pub fn data_type(&self) -> DataType {
        match self {
            Self::Number(_) => DataType::Number,
            Self::Boolean(_) => DataType::Boolean,
            Self::String(_) => DataType::String,
            Self::Object(_) => DataType::Object,
            Self::Null => DataType::Null,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CfxKeyValue {
    pub key: Vec<u8>,
    pub value: CfxValue,
}
