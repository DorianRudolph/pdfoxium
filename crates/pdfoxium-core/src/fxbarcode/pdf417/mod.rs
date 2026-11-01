// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
//! PDF417 source ports.
pub mod bc_pdf417;
pub mod bc_pdf417_barcode_matrix;
pub mod bc_pdf417_barcode_row;
pub mod bc_pdf417_error_correction;
pub mod bc_pdf417_high_level_encoder;
pub mod bc_pdf417_writer;
mod tables;
#[cfg(test)]
mod tests;
