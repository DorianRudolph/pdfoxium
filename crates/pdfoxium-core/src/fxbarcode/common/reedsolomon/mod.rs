//! Direct source GF(256), polynomial and cached parity encoder ports.
pub mod bc_reed_solomon;
pub mod bc_reed_solomon_gf256;
pub mod bc_reed_solomon_gf256_poly;
pub use bc_reed_solomon::CbcReedSolomonEncoder;
pub use bc_reed_solomon_gf256::{CbcReedSolomonGf256, DATA_MATRIX_FIELD, QR_CODE_FIELD};
pub use bc_reed_solomon_gf256_poly::CbcReedSolomonGf256Poly;
