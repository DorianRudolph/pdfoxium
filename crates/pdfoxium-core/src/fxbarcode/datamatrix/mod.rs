// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
//! ECC200 DataMatrix encoding, directly ported from PDFium.

pub mod bc_ascii_encoder;
pub mod bc_base256_encoder;
pub mod bc_c40_encoder;
pub mod bc_data_matrix_symbol_info144;
pub mod bc_data_matrix_writer;
pub mod bc_default_placement;
pub mod bc_edifact_encoder;
pub mod bc_encoder;
pub mod bc_encoder_context;
pub mod bc_error_correction;
pub mod bc_high_level_encoder;
pub mod bc_symbol_info;
pub mod bc_text_encoder;
pub mod bc_x12_encoder;

#[cfg(test)]
mod bc_data_matrix_writer_tests;
