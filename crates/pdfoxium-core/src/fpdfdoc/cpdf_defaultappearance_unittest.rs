// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfdoc/cpdf_defaultappearance_unittest.cpp`.

#![allow(non_snake_case, non_upper_case_globals)]

use super::find_tag_param_from_start;
use crate::fpdfapi::parser::cpdf_simple_parser::CpdfSimpleParser;

#[test]
fn FindTagParamFromStart() {
    let test_data: &[(&[u8], &[u8], usize, bool, u32)] = &[
        (b"", b"Tj", 1, false, 0),
        (b"", b"", 1, false, 0),
        (b"  T j", b"", 1, false, 5),
        (b"Tj", b"Tj", 1, false, 2),
        (b"(Tj", b"Tj", 1, false, 3),
        (b"\r12\t34  56 78Tj", b"Tj", 1, false, 15),
        (b"\r\0abd Tj", b"Tj", 1, true, 0),
        (b"12 4 Tj 3 46 Tj", b"Tj", 1, true, 2),
        (b"er^ 2 (34) (5667) Tj", b"Tj", 2, true, 5),
        (b"<344> (232)\t343.4\n12 45 Tj", b"Tj", 3, true, 11),
        (b"1 2 3 4 5 6 7 8 cm", b"cm", 6, true, 3),
    ];
    for &(input, token, num_params, result, result_position) in test_data {
        let mut parser = CpdfSimpleParser::new(input);
        assert_eq!(
            result,
            find_tag_param_from_start(&mut parser, token, num_params),
            "for case {input:?}"
        );
        assert_eq!(
            result_position,
            parser.current_position(),
            "for case {input:?}"
        );
    }
}
