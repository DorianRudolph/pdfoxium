// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
/*
 * Copyright 2011 ZXing authors
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! The owned engine dispatch from `cbc_codebase.{h,cpp}`. Concrete enum
//! variants replace the source's virtual writer hierarchy and checked casts.
use super::{
    bc_library::BcType, bc_writer::CbcWriter, cbc_datamatrix::CbcDataMatrix,
    cbc_onecode::CbcOneCode, cbc_pdf417i::CbcPdf417I, cbc_qrcode::CbcQrCode,
};
use crate::{
    fxcrt::fx_coordinates::CfxMatrix, fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
};

pub enum CbcCodeBase {
    One(CbcOneCode),
    Qr(CbcQrCode),
    DataMatrix(CbcDataMatrix),
    Pdf417(CbcPdf417I),
}
impl CbcCodeBase {
    pub fn new(kind: BcType) -> Option<Self> {
        match kind {
            BcType::Unknown => None,
            BcType::QrCode => Some(Self::Qr(CbcQrCode::new())),
            BcType::DataMatrix => Some(Self::DataMatrix(CbcDataMatrix::new())),
            BcType::Pdf417 => Some(Self::Pdf417(CbcPdf417I::new())),
            _ => CbcOneCode::new(kind).map(Self::One),
        }
    }
    pub fn get_type(&self) -> BcType {
        match self {
            Self::One(code) => code.get_type(),
            Self::Qr(_) => BcType::QrCode,
            Self::DataMatrix(_) => BcType::DataMatrix,
            Self::Pdf417(_) => BcType::Pdf417,
        }
    }
    pub fn writer_mut(&mut self) -> &mut CbcWriter {
        match self {
            Self::One(code) => code.writer_mut(),
            Self::Qr(code) => &mut code.writer.base.writer,
            Self::DataMatrix(code) => &mut code.writer.base.writer,
            Self::Pdf417(code) => &mut code.writer.base.writer,
        }
    }
    pub fn one_mut(&mut self) -> Option<&mut CbcOneCode> {
        match self {
            Self::One(code) => Some(code),
            _ => None,
        }
    }
    pub fn encode(&mut self, contents: &[u32]) -> bool {
        match self {
            Self::One(code) => code.encode(contents),
            Self::Qr(code) => code.encode(contents),
            Self::DataMatrix(code) => code.encode(contents),
            Self::Pdf417(code) => code.encode(contents),
        }
    }
    pub fn render_device(
        &mut self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
    ) -> bool {
        match self {
            Self::One(code) => code.render_device(device, matrix),
            Self::Qr(code) => code.render_device(device, matrix),
            Self::DataMatrix(code) => code.render_device(device, matrix),
            Self::Pdf417(code) => code.render_device(device, matrix),
        }
    }
}
