// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/cfx_standardfont_unittest.cpp`.

#![allow(non_snake_case, non_upper_case_globals)]

use super::*;

#[test]
fn IsStandardFontName() {
    assert!(is_standard_font_name(b"Courier"));
    assert!(is_standard_font_name(b"Courier-Bold"));
    assert!(is_standard_font_name(b"Courier-BoldOblique"));
    assert!(is_standard_font_name(b"Courier-Oblique"));
    assert!(is_standard_font_name(b"Helvetica"));
    assert!(is_standard_font_name(b"Helvetica-Bold"));
    assert!(is_standard_font_name(b"Helvetica-BoldOblique"));
    assert!(is_standard_font_name(b"Helvetica-Oblique"));
    assert!(is_standard_font_name(b"Times-Roman"));
    assert!(is_standard_font_name(b"Times-Bold"));
    assert!(is_standard_font_name(b"Times-BoldItalic"));
    assert!(is_standard_font_name(b"Times-Italic"));
    assert!(is_standard_font_name(b"Symbol"));
    assert!(is_standard_font_name(b"ZapfDingbats"));

    assert!(!(is_standard_font_name(b"Courie")));
    assert!(!(is_standard_font_name(b"Courier-")));
    assert!(!(is_standard_font_name(b"Helvetica+Bold")));
    assert!(!(is_standard_font_name(b"YapfDingbats")));
}

#[test]
fn GetStandardFontIndex() {
    // Canonical names
    assert_eq!(
        Some(StandardFontIndex::Courier),
        get_standard_font_index(b"Courier")
    );
    assert_eq!(
        Some(StandardFontIndex::TimesBold),
        get_standard_font_index(b"Times-Bold")
    );
    assert_eq!(
        Some(StandardFontIndex::Dingbats),
        get_standard_font_index(b"ZapfDingbats")
    );

    // Alternates (aliases)
    assert_eq!(
        Some(StandardFontIndex::Helvetica),
        get_standard_font_index(b"Arial")
    );
    assert_eq!(
        Some(StandardFontIndex::Courier),
        get_standard_font_index(b"CourierNew")
    );
    assert_eq!(
        Some(StandardFontIndex::TimesBold),
        get_standard_font_index(b"TimesNewRoman,Bold")
    );
    assert_eq!(
        Some(StandardFontIndex::Helvetica),
        get_standard_font_index(b"arial")
    );

    // Non-standard
    assert!(!(get_standard_font_index(b"ComicSans").is_some()));
}

#[test]
fn GetCanonicalFontName() {
    assert_eq!(
        "Courier",
        get_canonical_font_name(StandardFontIndex::Courier)
    );
    assert_eq!(
        "Helvetica-Bold",
        get_canonical_font_name(StandardFontIndex::HelveticaBold)
    );
    assert_eq!(
        "ZapfDingbats",
        get_canonical_font_name(StandardFontIndex::Dingbats)
    );
}

#[test]
fn IsSymbolicFont() {
    assert!(is_symbolic_font(StandardFontIndex::Symbol));
    assert!(is_symbolic_font(StandardFontIndex::Dingbats));

    assert!(!(is_symbolic_font(StandardFontIndex::Courier)));
    assert!(!(is_symbolic_font(StandardFontIndex::Helvetica)));
    assert!(!(is_symbolic_font(StandardFontIndex::Times)));
}

#[test]
fn IsFixedFont() {
    assert!(is_fixed_font(StandardFontIndex::Courier));
    assert!(is_fixed_font(StandardFontIndex::CourierBold));
    assert!(is_fixed_font(StandardFontIndex::CourierBoldOblique));
    assert!(is_fixed_font(StandardFontIndex::CourierOblique));

    assert!(!(is_fixed_font(StandardFontIndex::Helvetica)));
    assert!(!(is_fixed_font(StandardFontIndex::Times)));
    assert!(!(is_fixed_font(StandardFontIndex::Symbol)));
}
