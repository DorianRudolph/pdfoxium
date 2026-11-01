// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete `xfa/fxfa/parser/cxfa_document_unittest.cpp` declarations.
use super::CxfaDocument;

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

#[test]
fn parse_xfa_version() {
    for (input, expected) in [
        // Malformed namespace prefix, missing version and missing separator.
        ("", 0),
        ("http://www.xfa.org/schema/xfa-template", 0),
        ("http://www.xfa.org/schema/xfa-templatX/", 0),
        ("http://www.xfa.org/schema/xfa-template/", 0),
        ("http://www.xfa.org/schema/xfa-template/2", 0),
        // Versions outside the supported range.
        ("http://www.xfa.org/schema/xfa-template/-1.0", 0),
        ("http://www.xfa.org/schema/xfa-template/1.9", 0),
        ("http://www.xfa.org/schema/xfa-template/4.1", 0),
        // Missing digits retain wtoi's prefix parsing behavior.
        ("http://www.xfa.org/schema/xfa-template/.", 0),
        ("http://www.xfa.org/schema/xfa-template/.3", 0),
        ("http://www.xfa.org/schema/xfa-template/3.", 300),
        ("http://www.xfa.org/schema/xfa-template/clams.6", 0),
        ("http://www.xfa.org/schema/xfa-template/3.clams", 300),
        // Native minimum, maximum and normal decimal version.
        ("http://www.xfa.org/schema/xfa-template/2.0", 200),
        ("http://www.xfa.org/schema/xfa-template/4.0", 400),
        ("http://www.xfa.org/schema/xfa-template/3.6", 306),
        // Every native tolerant numeric-prefix case remains represented.
        ("http://www.xfa.org/schema/xfa-template/0003.00006", 306),
        (
            "http://www.xfa.org/schema/xfa-template/0003.00006.0000",
            306,
        ),
        ("http://www.xfa.org/schema/xfa-template/2.6clams", 206),
        ("http://www.xfa.org/schema/xfa-template/1.106", 206),
        ("http://www.xfa.org/schema/xfa-template/4.-94", 306),
        ("http://www.xfa.org/schema/xfa-template/3.17", 317),
    ] {
        assert_eq!(
            CxfaDocument::parse_xfa_version(&wide(input)),
            expected,
            "{input:?}"
        );
    }
}

#[test]
fn parse_use_href() {
    for (input, uri, id, som) in [
        ("", "", "", ""),
        ("url-part-only", "url-part-only", "", ""),
        ("url-part#frag", "url-part", "frag", ""),
        ("url-part#som(", "url-part", "som(", ""),
        ("url-part#som()", "url-part", "", ""),
        (
            "url-part#som(nested(foo.bar))",
            "url-part",
            "",
            "nested(foo.bar)",
        ),
    ] {
        // The input owns the storage for all three borrowed output views.
        let input = wide(input);
        let parts = CxfaDocument::parse_use_href(&input);
        assert_eq!(parts.uri, wide(uri));
        assert_eq!(parts.id, wide(id));
        assert_eq!(parts.som, wide(som));
    }
}

#[test]
fn parse_use() {
    for (input, id, som) in [
        ("", "", ""),
        ("clams", "", "clams"),
        ("#clams", "clams", ""),
    ] {
        let input = wide(input);
        let parts = CxfaDocument::parse_use(&input);
        assert_eq!(parts.id, wide(id));
        assert_eq!(parts.som, wide(som));
    }
}

#[test]
fn version_components_reject_narrowing_overflow() {
    // Source narrows parsed components to int8_t. Reject values that would
    // otherwise wrap into an apparently supported version (3.6 in each case).
    for input in ["259.6", "3.262", "3.-250"] {
        let namespace = format!("http://www.xfa.org/schema/xfa-template/{input}");
        assert_eq!(CxfaDocument::parse_xfa_version(&wide(&namespace)), 0);
    }
}
