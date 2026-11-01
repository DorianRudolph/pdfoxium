// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Decoder progress values from `core/fxcodec/fx_codec_def.h` for the
//! portable engine, including XFA image type selection.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum FxcodecStatus {
    Error = -1,
    FrameReady = 0,
    FrameToBeContinued = 1,
    DecodeReady = 2,
    DecodeToBeContinued = 3,
    DecodeFinished = 4,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(i32)]
pub enum FxcodecImageType {
    #[default]
    Unknown = 0,
    Jpg = 1,
    Bmp = 2,
    Png = 3,
    Gif = 4,
    Tiff = 5,
}
