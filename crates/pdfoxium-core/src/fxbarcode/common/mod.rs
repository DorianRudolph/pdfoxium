//! Portable barcode matrix storage and Reed-Solomon field arithmetic.
pub mod bc_common_bit_matrix;
pub mod bc_common_byte_matrix;
pub mod reedsolomon;
pub use bc_common_bit_matrix::CbcCommonBitMatrix;
pub use bc_common_byte_matrix::CbcCommonByteMatrix;
