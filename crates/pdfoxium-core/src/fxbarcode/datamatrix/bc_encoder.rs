// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Source encoder interface, represented by stateless mode dispatch.
use super::bc_encoder_context::CbcEncoderContext;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Encoding {
    Ascii = 0,
    C40 = 1,
    Text = 2,
    X12 = 3,
    Edifact = 4,
    Base256 = 5,
}
pub(crate) fn encode(mode: Encoding, context: &mut CbcEncoderContext) -> bool {
    match mode {
        Encoding::Ascii => super::bc_ascii_encoder::encode(context),
        Encoding::C40 => super::bc_c40_encoder::encode(context, Encoding::C40),
        Encoding::Text => super::bc_text_encoder::encode(context),
        Encoding::X12 => super::bc_x12_encoder::encode(context),
        Encoding::Edifact => super::bc_edifact_encoder::encode(context),
        Encoding::Base256 => super::bc_base256_encoder::encode(context),
    }
}
