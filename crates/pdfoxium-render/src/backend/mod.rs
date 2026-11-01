pub(crate) use crate::{create_transform_getters, create_transform_setters};
pub(crate) mod annotations;
pub(crate) mod bitmap;
pub(crate) mod bookmarks;
pub(crate) mod color;
pub(crate) mod document;
pub(crate) mod error;
pub(crate) mod matrix;
pub(crate) mod metadata;
pub(crate) mod objects;
pub(crate) mod points;
pub(crate) mod quad_points;
pub(crate) mod rect;
pub(crate) mod render;
pub(crate) mod render_config;
pub(crate) mod size;
pub(crate) mod text;
pub(crate) mod transform;

pub(crate) use bookmarks::*;
pub(crate) use color::*;
pub(crate) use document::*;
pub(crate) use error::*;
pub(crate) use metadata::*;
pub(crate) use objects::*;

pub(crate) mod prelude {
    pub(crate) use crate::backend::{
        annotations::*, bitmap::*, color::*, document::*, error::*, matrix::*, points::*, rect::*,
        render::*, render_config::*, size::*,
    };
}
