//! Safe Rust translations of PDFium's portable engine modules.
//!
//! The dependencies in Cargo.toml are the codec and font adapters selected in
//! the project plan; they do not represent implemented functionality.

pub mod agg;
pub mod constants;
pub mod fdrm;
pub mod fpdfapi;
pub mod fpdfdoc;
pub mod fpdfsdk;
pub mod fpdftext;
#[cfg(feature = "xfa")]
pub mod fxbarcode;
pub mod fxcodec;
pub mod fxcrt;
pub mod fxge;
#[cfg(feature = "js")]
pub mod fxjs;
#[cfg(feature = "xfa")]
pub mod xfa;

#[cfg(feature = "render-profile")]
pub mod render_profile;

#[cfg(test)]
mod test_support;
