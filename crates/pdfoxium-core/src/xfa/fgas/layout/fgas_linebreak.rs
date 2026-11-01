// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Exact 38-by-38 pair table from `xfa/fgas/layout/fgas_linebreak.{h,cpp}`.
use crate::fxcrt::fx_unicode::BreakProperty;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum LineBreakType {
    #[default]
    Unknown = 0x00,
    DirectBreak = 0x1a,
    IndirectBreak = 0x2b,
    CommonIndirectBreak = 0x3c,
    CommonProhibitedBreak = 0x4d,
    ProhibitedBreak = 0x5e,
    HangulSpaceBreak = 0x6f,
}
use LineBreakType::{
    CommonIndirectBreak as Cb, CommonProhibitedBreak as Cp, DirectBreak as Db, IndirectBreak as Ib,
    ProhibitedBreak as Pb, Unknown as Un,
};
const PAIRS: [[LineBreakType; 38]; 38] = [
    [
        Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Pb, Cp, Pb, Pb, Pb,
        Pb, Pb, Pb, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Pb, Pb, Pb, Pb, Ib, Ib, Db, Db, Db, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Pb, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Pb, Cb, Pb, Ib, Ib,
        Ib, Ib, Ib, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Ib, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Pb, Cb, Pb, Ib, Ib,
        Ib, Ib, Ib, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Db, Db, Db, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Db, Db, Db, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Ib, Db, Db, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Ib, Ib, Db, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Ib, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Ib, Ib, Ib, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Ib, Ib,
        Ib, Ib, Ib, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Ib, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Ib, Ib, Db, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Ib, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Ib, Ib, Ib, Ib, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Ib, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Ib, Ib, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Db, Ib, Pb, Pb, Pb, Db, Db, Ib, Db, Db, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Db, Ib, Pb, Pb, Pb, Db, Db, Db, Db, Db, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Ib, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Pb, Cb, Pb, Ib, Ib,
        Ib, Ib, Ib, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Db, Db, Db, Db, Ib, Ib, Db, Pb, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Db, Db, Db, Db, Db, Db, Db, Db, Db, Db, Db, Db, Db, Db, Db, Db, Db, Pb, Db, Db, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Db, Ib, Ib, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Ib, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Ib, Pb, Cb, Pb, Ib, Ib,
        Ib, Ib, Ib, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Ib, Ib, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Ib, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Ib, Ib,
        Ib, Ib, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Ib, Ib, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Ib, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Ib, Ib, Pb, Pb, Pb, Db, Ib, Db, Db, Db, Ib, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Db, Pb, Pb, Pb, Db, Pb, Db, Ib, Db, Db, Db, Pb, Pb, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Pb, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
        Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
    [
        Db, Pb, Ib, Db, Ib, Pb, Pb, Pb, Db, Db, Db, Db, Db, Db, Ib, Ib, Db, Db, Pb, Cb, Pb, Db, Db,
        Db, Db, Db, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un, Un,
    ],
];
pub fn get_line_break_type_from_pair(current: BreakProperty, next: BreakProperty) -> LineBreakType {
    PAIRS[current as usize][next as usize]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn punctuation_words_spaces_and_combining_marks() {
        use BreakProperty::*;
        for (current, next, expected) in [
            (OP, AL, Pb),
            (AL, AL, Ib),
            (SP, AL, Db),
            (AL, CM, Cb),
            (WJ, AL, Ib),
            (AL, OP, Ib),
            (ID, ID, Db),
            (NONE, AL, Un),
            (AL, TB, Un),
        ] {
            assert_eq!(
                get_line_break_type_from_pair(current, next),
                expected,
                "{current:?}/{next:?}"
            );
        }
    }
    #[test]
    fn complete_pinned_pair_table() {
        let source =
            include_str!("../../../../../../upstream/pdfium/xfa/fgas/layout/fgas_linebreak.cpp");
        let body = source
            .split_once("kLineBreakPairTable = {{")
            .unwrap()
            .1
            .split_once("}};")
            .unwrap()
            .0;
        let mut index = 0;
        for word in body
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|word| word.starts_with("kLb"))
        {
            let expected = match word {
                "kLbUn" => Un,
                "kLbDb" => Db,
                "kLbIb" => Ib,
                "kLbCb" => Cb,
                "kLbCp" => Cp,
                "kLbPb" => Pb,
                _ => panic!("{word}"),
            };
            assert_eq!(PAIRS[index / 38][index % 38], expected);
            index += 1;
        }
        assert_eq!(index, 38 * 38);
    }
}
