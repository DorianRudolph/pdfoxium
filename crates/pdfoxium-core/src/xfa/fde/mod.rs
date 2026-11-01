//! Portable FDE text editing and output use FGAS layout and the AGG device.
pub mod cfde_data;
pub mod cfde_texteditengine;
#[cfg(test)]
mod cfde_texteditengine_tests;
pub mod cfde_textout;
pub mod cfde_wordbreak_data;
#[cfg(test)]
mod textedit_render_tests;

#[cfg(test)]
mod cfde_texteditengine_callback_tests;
