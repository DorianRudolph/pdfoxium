// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_streamparser_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfStreamParser;

#[test]
fn ReadHexString() {
    // All source uint8_t data arrays include their terminating NUL.
    {
        let mut parser = CpdfStreamParser::new(b"12ab>\0");
        parser.set_pos(6);
        assert!(parser.read_hex_string().is_empty());
    }
    {
        let mut parser = CpdfStreamParser::new(b"1A2b>abcd\0");
        assert_eq!(parser.read_hex_string(), &[0x1a, 0x2b]);
        assert_eq!(parser.get_pos(), 5);
    }
    {
        let mut parser = CpdfStreamParser::new(b"1A2b\0");
        assert_eq!(parser.read_hex_string(), &[0x1a, 0x2b]);
        assert_eq!(parser.get_pos(), 5);
    }
    {
        let mut parser = CpdfStreamParser::new(b"1A2>asdf\0");
        assert_eq!(parser.read_hex_string(), &[0x1a, 0x20]);
        assert_eq!(parser.get_pos(), 4);
    }
    {
        let mut parser = CpdfStreamParser::new(b">\0");
        assert!(parser.read_hex_string().is_empty());
        assert_eq!(parser.get_pos(), 1);
    }
}
