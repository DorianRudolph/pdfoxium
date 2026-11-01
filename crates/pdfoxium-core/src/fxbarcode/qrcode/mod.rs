// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

pub mod bc_qr_code_writer;
pub mod bc_qr_coder;
pub mod bc_qr_coder_bit_vector;
pub mod bc_qr_coder_ec_block_data;
pub mod bc_qr_coder_encoder;
pub mod bc_qr_coder_error_correction_level;
pub mod bc_qr_coder_mask_util;
pub mod bc_qr_coder_matrix_util;
pub mod bc_qr_coder_mode;
pub mod bc_qr_coder_version;

pub use bc_qr_code_writer::CbcQrCodeWriter;

#[cfg(test)]
mod bc_qr_code_writer_unittest;
