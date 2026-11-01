#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
mod actions;
mod backend;
mod collections;
mod fonts;
mod object_store;
mod paths;
mod read_models;
pub use actions::*;
pub use backend::annotations::PdfFormFieldType;
pub use backend::metadata::{PdfDocumentMetadataTag, PdfDocumentMetadataTagType};
pub use backend::objects::PdfFontBuiltin;
pub use backend::render::{PdfPageOrientation, PdfPageRenderRotation};
pub use backend::{
    bitmap::*, color::*, error::*, matrix::*, points::*, quad_points::*, rect::*, render_config::*,
    size::*,
};
pub use collections::*;
pub use fonts::*;
pub use paths::*;
pub use read_models::*;
pub mod prelude {
    pub use crate::*;
}

mod document;
pub use document::*;

mod pdfium;
pub use pdfium::*;

mod boundaries;
pub use boundaries::*;

mod objects;
pub use objects::*;

mod path_objects;
pub use path_objects::*;

mod text_objects;
pub use text_objects::*;

mod object_collections;
pub use object_collections::*;

mod groups;
pub use groups::*;

mod text;
pub use text::*;

mod images;
pub use images::*;

mod clips;
pub use clips::*;

mod fields;
pub use fields::*;

mod annotations;
pub use annotations::*;

mod links;
pub use links::*;

#[cfg(all(test, feature = "image_api"))]
mod test_support {
    pub fn test_bind_to_pdfium() -> crate::Pdfium {
        crate::Pdfium::default()
    }
}
#[cfg(all(test, feature = "image_api"))]
mod upstream_tests;

#[cfg(any(target_arch = "wasm32", feature = "browser-interface-check"))]
mod browser;

#[cfg(all(
    test,
    any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64")
    )
))]
mod native_tests;
#[cfg(test)]
mod safe_api_tests;

#[cfg(test)]
mod safety_tests;
