// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Lifecycle from `core/fpdfapi/page/cpdf_pagemodule.{h,cpp}`.
//! Stock color spaces are thread-local, CMaps are bundled static data, and
//! parser operators are matched directly. There are no process singletons
//! to allocate or tear down in this Rust implementation.

use super::cpdf_colorspace::{destroy_stock_color_spaces, get_stock_cs_for_name};

pub fn initialize_page_module() {
    for name in [
        b"DeviceGray".as_slice(),
        b"DeviceRGB",
        b"DeviceCMYK",
        b"Pattern",
    ] {
        let _ = get_stock_cs_for_name(name);
    }
}

pub fn destroy_page_module() {
    destroy_stock_color_spaces();
    super::image_memory::set_image_memory_limits(Default::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn destroy_releases_cached_stock_spaces_for_reinitialization() {
        initialize_page_module();
        let first = get_stock_cs_for_name(b"DeviceRGB").unwrap();
        assert!(Rc::ptr_eq(
            &first,
            &get_stock_cs_for_name(b"DeviceRGB").unwrap()
        ));
        destroy_page_module();
        initialize_page_module();
        assert!(!Rc::ptr_eq(
            &first,
            &get_stock_cs_for_name(b"DeviceRGB").unwrap()
        ));
    }
}
