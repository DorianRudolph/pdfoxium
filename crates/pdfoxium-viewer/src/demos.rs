// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Small original samples embedded in the executable; no working-directory dependency.
pub struct Demo {
    pub id: &'static str,
    pub widget: &'static str,
    pub title: &'static str,
    pub filename: &'static str,
    pub bytes: &'static [u8],
}
macro_rules! demo {
    ($id:literal, $title:literal, $file:literal) => {
        Demo {
            id: $id,
            widget: concat!("demo.", $id),
            title: $title,
            filename: $file,
            bytes: include_bytes!(concat!("../assets/demos/", $file)),
        }
    };
}
pub const DEMOS: &[Demo] = &[
    demo!("layers", "A map with layers", "layers-demo.pdf"),
    demo!(
        "editing",
        "Annotations and page editing",
        "editing-demo.pdf"
    ),
    demo!(
        "acroform",
        "AcroForm fields and JavaScript",
        "acroform-demo.pdf"
    ),
    demo!("xfa", "Interactive XFA form", "xfa-demo.pdf"),
    demo!(
        "dynamic-xfa",
        "Dynamic XFA — add and remove rows",
        "dynamic-xfa-demo.pdf"
    ),
    demo!(
        "animation",
        "Animated travelling wave",
        "animation-demo.pdf"
    ),
];
pub fn find(id: &str) -> Option<usize> {
    DEMOS.iter().position(|demo| demo.id == id)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samples_have_unique_names_and_parse_without_external_files() {
        let mut ids = std::collections::HashSet::new();
        for demo in DEMOS {
            assert!(ids.insert(demo.id));
            let doc = pdfoxium::Document::from_bytes(demo.bytes).unwrap();
            assert!(doc.page_count().unwrap() > 0);
        }
    }
}
