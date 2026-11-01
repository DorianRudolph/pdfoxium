// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_streamcontentparser_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfStreamContentParser;

#[test]
fn PDFFindKeyAbbreviation() {
    assert_eq!(
        CpdfStreamContentParser::find_key_abbreviation(b"BPC"),
        b"BitsPerComponent"
    );
    assert_eq!(
        CpdfStreamContentParser::find_key_abbreviation(b"W"),
        b"Width"
    );
    assert_eq!(CpdfStreamContentParser::find_key_abbreviation(b""), b"");
    assert_eq!(
        CpdfStreamContentParser::find_key_abbreviation(b"NoInList"),
        b""
    );
    assert_eq!(CpdfStreamContentParser::find_key_abbreviation(b"WW"), b"");
}

#[test]
fn PDFFindValueAbbreviation() {
    assert_eq!(
        CpdfStreamContentParser::find_value_abbreviation(b"G"),
        b"DeviceGray"
    );
    assert_eq!(
        CpdfStreamContentParser::find_value_abbreviation(b"DCT"),
        b"DCTDecode"
    );
    assert_eq!(CpdfStreamContentParser::find_value_abbreviation(b""), b"");
    assert_eq!(
        CpdfStreamContentParser::find_value_abbreviation(b"NoInList"),
        b""
    );
    assert_eq!(CpdfStreamContentParser::find_value_abbreviation(b"II"), b"");
}
