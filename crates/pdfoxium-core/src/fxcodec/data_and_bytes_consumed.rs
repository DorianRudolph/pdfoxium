// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxcodec/data_and_bytes_consumed.h` and `.cpp`.

pub struct DataAndBytesConsumed {
    pub data: Vec<u8>,
    pub bytes_consumed: u32,
}

impl DataAndBytesConsumed {
    pub fn new(data: Vec<u8>, bytes_consumed: u32) -> Self {
        Self {
            data,
            bytes_consumed,
        }
    }
}
