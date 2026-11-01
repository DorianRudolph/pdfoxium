//! Portable primitives from `upstream/pdfium/core/fxcrt`.

pub mod binary_buffer;
pub mod byteorder;
pub mod bytestring;
pub mod bytestring_pool;
pub mod cfx_bidi_resolver;
pub mod cfx_bitstream;
pub mod cfx_fileaccess_stream;
pub mod cfx_memorystream;
pub mod cfx_read_only_container_stream;
pub mod cfx_read_only_span_stream;
pub mod cfx_seekablestreamproxy;
pub mod cfx_timer;
pub mod css;
pub mod fileaccess_iface;
pub mod fx_2d_size;
pub mod fx_ceil_div;
pub mod fx_ceil_log2;
pub mod fx_codepage;
pub mod fx_coordinates;
// Calendar arithmetic is also needed for PDF metadata when scripting is off.
// Keep one source implementation and its source-derived tests.
#[path = "../fxjs/fx_date_helpers.rs"]
pub mod fx_date_helpers;
pub mod fx_extension;
pub mod fx_memory;
pub mod fx_number;
pub mod fx_random;
pub mod fx_stream;
pub mod fx_string;
pub mod fx_string_wrappers;
pub mod fx_system;
pub mod fx_types;
pub mod fx_unicode;
pub mod pauseindicator_iface;
pub mod shared_copy_on_write;
pub mod stream_write_utils;
pub mod tree_node;
pub mod utf16;
pub mod xml;

#[cfg(test)]
#[path = "code_point_view_unittest.rs"]
mod code_point_view_upstream_tests;

#[cfg(test)]
#[path = "fx_safe_types_unittest.rs"]
mod fx_safe_types_upstream_tests;

#[cfg(test)]
#[path = "fx_memory_wrappers_unittest.rs"]
mod fx_memory_wrappers_upstream_tests;

#[cfg(test)]
#[path = "data_vector_unittest.rs"]
mod data_vector_upstream_tests;

#[cfg(test)]
#[path = "containers/to_vector_unittest.rs"]
mod to_vector_upstream_tests;

#[cfg(test)]
#[path = "fixed_size_data_vector_unittest.rs"]
mod fixed_size_data_vector_upstream_tests;

#[cfg(test)]
#[path = "stl_util_unittest.rs"]
mod stl_util_upstream_tests;

#[cfg(test)]
#[path = "fx_extension_unittest.rs"]
mod fx_extension_upstream_tests;

#[cfg(test)]
#[path = "fx_memcpy_wrappers_unittest.rs"]
mod fx_memcpy_wrappers_upstream_tests;

#[cfg(test)]
#[path = "span_util_unittest.rs"]
mod span_util_upstream_tests;

#[cfg(test)]
#[path = "pdfium_span_unittest.rs"]
mod pdfium_span_upstream_tests;

#[cfg(test)]
#[path = "zip_unittest.rs"]
mod zip_upstream_tests;

#[cfg(test)]
#[path = "span_io_unittest.rs"]
mod span_io_upstream_tests;

#[cfg(test)]
#[path = "to_underlying_unittest.rs"]
mod to_underlying_upstream_tests;

#[cfg(test)]
#[path = "mask_unittest.rs"]
mod mask_upstream_tests;

#[cfg(test)]
mod ownership_test_support;

#[cfg(test)]
#[path = "retain_ptr_unittest.rs"]
mod retain_ptr_upstream_tests;

#[cfg(test)]
#[path = "autonuller_unittest.rs"]
mod autonuller_upstream_tests;

#[cfg(test)]
#[path = "autorestorer_unittest.rs"]
mod autorestorer_upstream_tests;

#[cfg(test)]
#[path = "maybe_owned_unittest.rs"]
mod maybe_owned_upstream_tests;

#[cfg(test)]
#[path = "maybe_owned_data_vector_unittest.rs"]
mod maybe_owned_data_vector_upstream_tests;

#[cfg(test)]
#[path = "observed_ptr_unittest.rs"]
mod observed_ptr_upstream_tests;

#[cfg(test)]
#[path = "weak_ptr_unittest.rs"]
mod weak_ptr_upstream_tests;

#[cfg(test)]
#[path = "unowned_ptr_unittest.rs"]
mod unowned_ptr_upstream_tests;

#[cfg(test)]
#[path = "bytestring_unittest.rs"]
mod bytestring_upstream_tests;

#[cfg(test)]
#[path = "widestring_unittest.rs"]
mod widestring_upstream_tests;

#[cfg(test)]
mod string_test_support;

pub mod fx_bidi;
#[cfg(test)]
mod fx_bidi_unittest;

pub mod widetext_buffer;

pub mod widestring;
