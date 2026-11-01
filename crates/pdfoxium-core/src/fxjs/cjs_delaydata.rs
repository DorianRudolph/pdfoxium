// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_delaydata.{h,cpp}`. A typed payload represents FIELD_PROP and its
//! corresponding native variant together, preventing mismatched queue entries.
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use std::rc::Rc;

pub struct CjsDelayData {
    pub field_name: Rc<[u16]>,
    pub control_index: i32,
    pub value: CjsDelayValue,
}

pub enum CjsDelayValue {
    BorderStyle(Vec<u8>),
    CurrentValueIndices(Vec<u32>),
    Display(i32),
    Hidden(bool),
    LineWidth(i32),
    Rect(CfxFloatRect),
    Value(Vec<Vec<u32>>),
}
