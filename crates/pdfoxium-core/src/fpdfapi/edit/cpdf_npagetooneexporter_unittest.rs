// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
#[test]
#[allow(non_snake_case)]
fn GenerateSubPageContentStream() {
    let settings = NupPageSettings {
        sub_page_start_point: CfxPointF::new(0.000001, 1000000000000.),
        scale: 0.5,
    };
    assert_eq!(
        CpdfNPageToOneExporter::generate_sub_page_content_stream_for_testing(b"foo", settings),
        b"q\n.5 0 0 .5 .000001 1000000000000 cm\n/foo Do Q\n"
    );
}
