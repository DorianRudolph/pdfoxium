// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/font/cpdf_tounicodemap_unittest.cpp`.
//! Wide strings keep wchar32 values, including individual surrogate units.
#![allow(non_snake_case, non_upper_case_globals)]

use super::CpdfToUnicodeMap;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::test_support::assert_wide_cstr_eq;

#[test]
fn StringToCode() {
    assert_eq!(CpdfToUnicodeMap::string_to_code(b"<0001>"), Some(1));
    assert_eq!(CpdfToUnicodeMap::string_to_code(b"<c2>"), Some(194));
    assert_eq!(CpdfToUnicodeMap::string_to_code(b"<A2>"), Some(162));
    assert_eq!(CpdfToUnicodeMap::string_to_code(b"<Af2>"), Some(2802));
    assert_eq!(
        CpdfToUnicodeMap::string_to_code(b"<FFFFFFFF>"),
        Some(4294967295)
    );

    // Whitespaces within the string are ignored.
    assert_eq!(CpdfToUnicodeMap::string_to_code(b"<00\x0a0\x0d1>"), Some(1));
    assert_eq!(CpdfToUnicodeMap::string_to_code(b"<c 2>"), Some(194));
    assert_eq!(CpdfToUnicodeMap::string_to_code(b"<A2\x0d\x0a>"), Some(162));

    // Integer overflow
    assert!(CpdfToUnicodeMap::string_to_code(b"<100000000>").is_none());
    assert!(CpdfToUnicodeMap::string_to_code(b"<1abcdFFFF>").is_none());

    // Invalid string
    assert!(CpdfToUnicodeMap::string_to_code(b"").is_none());
    assert!(CpdfToUnicodeMap::string_to_code(b"<>").is_none());
    assert!(CpdfToUnicodeMap::string_to_code(b"12").is_none());
    assert!(CpdfToUnicodeMap::string_to_code(b"<12").is_none());
    assert!(CpdfToUnicodeMap::string_to_code(b"12>").is_none());
    assert!(CpdfToUnicodeMap::string_to_code(b"<1-7>").is_none());
    assert!(CpdfToUnicodeMap::string_to_code(b"00AB").is_none());
    assert!(CpdfToUnicodeMap::string_to_code(b"<00NN>").is_none());
}

#[test]
fn StringToWideString() {
    assert_wide_cstr_eq(&[], &CpdfToUnicodeMap::string_to_wide_string(b""));
    assert_wide_cstr_eq(&[], &CpdfToUnicodeMap::string_to_wide_string(b"1234"));
    assert_wide_cstr_eq(&[], &CpdfToUnicodeMap::string_to_wide_string(b"<c2"));
    assert_wide_cstr_eq(&[], &CpdfToUnicodeMap::string_to_wide_string(b"<c2D2"));
    assert_wide_cstr_eq(&[], &CpdfToUnicodeMap::string_to_wide_string(b"c2ab>"));

    let mut res = vec![0xc2ab];
    assert_eq!(res, CpdfToUnicodeMap::string_to_wide_string(b"<c2ab>"));
    assert_eq!(res, CpdfToUnicodeMap::string_to_wide_string(b"<c2abab>"));

    res.extend_from_slice(&[0xfaab]);
    assert_eq!(res, CpdfToUnicodeMap::string_to_wide_string(b"<c2abFaAb>"));
    assert_eq!(
        res,
        CpdfToUnicodeMap::string_to_wide_string(b"<c2abFaAb12>")
    );
    assert_eq!(res, CpdfToUnicodeMap::string_to_wide_string(b"<c2ab FaAb>"));
    assert_eq!(
        res,
        CpdfToUnicodeMap::string_to_wide_string(b"<c2ab FaAb12>")
    );
    assert_eq!(
        res,
        CpdfToUnicodeMap::string_to_wide_string(b"<c2ab FaAb 12>")
    );
    assert_eq!(
        res,
        CpdfToUnicodeMap::string_to_wide_string(b"< c 2 a b  F a A b  1 2 >")
    );
}

#[test]
fn HandleBeginBFCharRejectsInvalidCidValues() {
    let kInputs: &[&[u8]] = &[
        b"1 beginbfchar<00NN><0041>endbfchar",
        b"1 beginbfchar<100000000><0041>endbfchar",
    ];

    for input in kInputs {
        let stream = CpdfStream::from_bytes(input);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[], &map.lookup(0));
        assert_eq!(0, map.reverse_lookup(0x0041));
        assert_eq!(0, map.unicode_count_by_charcode(0));
    }
}

#[test]
fn HandleBeginBFCharBadCount() {
    {
        const kInput1: &[u8] = b"1 beginbfchar<1><0041><2><0042>endbfchar\x00";
        let stream = CpdfStream::from_bytes(kInput1);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_eq!(0, map.reverse_lookup(0x0041));
        assert_eq!(0, map.reverse_lookup(0x0042));
        assert_eq!(0, map.unicode_count_by_charcode(1));
        assert_eq!(0, map.unicode_count_by_charcode(2));
    }
    {
        const kInput2: &[u8] = b"3 beginbfchar<1><0041><2><0042>endbfchar\x00";
        let stream = CpdfStream::from_bytes(kInput2);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_eq!(0, map.reverse_lookup(0x0041));
        assert_eq!(0, map.reverse_lookup(0x0042));
        assert_eq!(0, map.unicode_count_by_charcode(1));
        assert_eq!(0, map.unicode_count_by_charcode(2));
    }
}

#[test]
fn HandleBeginBFCharTolerateOutOfSpecCount() {
    // Tolerate more than 100 entries.
    const kTooManyEntriesInput: &[u8] = concat!(
        "112 beginbfchar",
        "<0000><0008>",
        "<0001><0009>",
        "<0002><000A>",
        "<0003><000B>",
        "<0004><000C>",
        "<0005><000D>",
        "<0006><000E>",
        "<0007><000F>",
        "<0008><0000>",
        "<0009><0001>",
        "<000A><0002>",
        "<000B><0003>",
        "<000C><0004>",
        "<000D><0005>",
        "<000E><0006>",
        "<000F><0007>",
        "<0010><0018>",
        "<0011><0019>",
        "<0012><001A>",
        "<0013><001B>",
        "<0014><001C>",
        "<0015><001D>",
        "<0016><001E>",
        "<0017><001F>",
        "<0018><0010>",
        "<0019><0011>",
        "<001A><0012>",
        "<001B><0013>",
        "<001C><0014>",
        "<001D><0015>",
        "<001E><0016>",
        "<001F><0017>",
        "<0020><0028>",
        "<0021><0029>",
        "<0022><002A>",
        "<0023><002B>",
        "<0024><002C>",
        "<0025><002D>",
        "<0026><002E>",
        "<0027><002F>",
        "<0028><0020>",
        "<0029><0021>",
        "<002A><0022>",
        "<002B><0023>",
        "<002C><0024>",
        "<002D><0025>",
        "<002E><0026>",
        "<002F><0027>",
        "<0030><0038>",
        "<0031><0039>",
        "<0032><003A>",
        "<0033><003B>",
        "<0034><003C>",
        "<0035><003D>",
        "<0036><003E>",
        "<0037><003F>",
        "<0038><0030>",
        "<0039><0031>",
        "<003A><0032>",
        "<003B><0033>",
        "<003C><0034>",
        "<003D><0035>",
        "<003E><0036>",
        "<003F><0037>",
        "<0040><0048>",
        "<0041><0049>",
        "<0042><004A>",
        "<0043><004B>",
        "<0044><004C>",
        "<0045><004D>",
        "<0046><004E>",
        "<0047><004F>",
        "<0048><0040>",
        "<0049><0041>",
        "<004A><0042>",
        "<004B><0043>",
        "<004C><0044>",
        "<004D><0045>",
        "<004E><0046>",
        "<004F><0047>",
        "<0050><0058>",
        "<0051><0059>",
        "<0052><005A>",
        "<0053><005B>",
        "<0054><005C>",
        "<0055><005D>",
        "<0056><005E>",
        "<0057><005F>",
        "<0058><0050>",
        "<0059><0051>",
        "<005A><0052>",
        "<005B><0053>",
        "<005C><0054>",
        "<005D><0055>",
        "<005E><0056>",
        "<005F><0057>",
        "<0060><0068>",
        "<0061><0069>",
        "<0062><006A>",
        "<0063><006B>",
        "<0064><006C>",
        "<0065><006D>",
        "<0066><006E>",
        "<0067><006F>",
        "<0068><0060>",
        "<0069><0061>",
        "<006A><0062>",
        "<006B><0063>",
        "<006C><0064>",
        "<006D><0065>",
        "<006E><0066>",
        "<006F><0067>",
        "endbfchar",
        "\0",
    )
    .as_bytes();
    let stream = CpdfStream::from_bytes(kTooManyEntriesInput);
    let map = CpdfToUnicodeMap::from_stream(&stream);
    assert_eq!(9, map.reverse_lookup(0x0001));
    assert_eq!(111, map.reverse_lookup(0x0067));
    assert_eq!(1, map.unicode_count_by_charcode(1));
    assert_eq!(1, map.unicode_count_by_charcode(111));
}

#[test]
fn HandleBeginBFRangeRejectsInvalidCidValues() {
    // Adapt the upstream test's first three expectations: FFFFFFFF is a
    // valid four-byte Type0 source code, rather than an invalid16-bit CID.
    // The remaining reversed/mismatched ranges still fail safely.
    {
        const kInput1: &[u8] = b"1 beginbfrange<FFFFFFFF><FFFFFFFF>[<0041>]endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput1);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[0x41], &map.lookup(0xffffffff));
    }
    {
        const kInput2: &[u8] = b"1 beginbfrange<FFFFFFFF><FFFFFFFF><0042>endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput2);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[0x42], &map.lookup(0xffffffff));
    }
    {
        const kInput3: &[u8] = b"1 beginbfrange<FFFFFFFF><FFFFFFFF><00410042>endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput3);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[0x41, 0x42], &map.lookup(0xffffffff));
    }
    {
        const kInput4: &[u8] = b"1 beginbfrange<0001><10000>[<0041>]endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput4);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[], &map.lookup(0xffffffff));
        assert_wide_cstr_eq(&[], &map.lookup(0x0001));
        assert_wide_cstr_eq(&[], &map.lookup(0xffff));
        assert_wide_cstr_eq(&[], &map.lookup(0x10000));
    }
    {
        const kInput5: &[u8] = b"1 beginbfrange<10000><10001>[<0041>]endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput5);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[], &map.lookup(0x10000));
        assert_wide_cstr_eq(&[], &map.lookup(0x10001));
    }
    {
        const kInput6: &[u8] = b"1 beginbfrange<0006><0004>[<0041>]endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput6);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[], &map.lookup(0x0004));
        assert_wide_cstr_eq(&[], &map.lookup(0x0005));
        assert_wide_cstr_eq(&[], &map.lookup(0x0006));
    }
}

#[test]
fn four_byte_source_codes_keep_scalar_array_and_multi_string_mappings() {
    let map = CpdfToUnicodeMap::from_data(
        b"2 beginbfchar <00824100> <0041> <FFFFFFFC> <0058> endbfchar \
          3 beginbfrange \
          <00824101> <00824102> <0042> \
          <90410000> <90410001> [<0044> <00660069>] \
          <FFFFFFFD> <FFFFFFFF> <00780031> endbfrange",
    );
    for (source, destination) in [
        (0x824100, vec![0x41]),
        (0x824101, vec![0x42]),
        (0x824102, vec![0x43]),
        (0x90410000, vec![0x44]),
        (0x90410001, vec![0x66, 0x69]),
        (0xfffffffc, vec![0x58]),
        (0xfffffffd, vec![0x78, 0x31]),
        (0xfffffffe, vec![0x78, 0x32]),
        (0xffffffff, vec![0x78, 0x33]),
    ] {
        assert_eq!(map.lookup(source), destination);
    }
    assert_eq!(map.reverse_lookup(0x41), 0x824100);
    assert_eq!(map.reverse_lookup(0x43), 0x824102);
    assert_eq!(map.reverse_lookup(0x58), 0xfffffffc);

    // The maximum source value exercises both the scalar offset expression
    // and multi-string iteration without overflowing before/after the range.
    for input in [
        b"1 beginbfchar <FFFFFFFF> <0041> endbfchar".as_slice(),
        b"1 beginbfrange <FFFFFFFF> <FFFFFFFF> <0041> endbfrange".as_slice(),
        b"1 beginbfrange <FFFFFFFF> <FFFFFFFF> [<0041>] endbfrange".as_slice(),
    ] {
        let maximum = CpdfToUnicodeMap::from_data(input);
        assert_eq!(maximum.lookup(u32::MAX), vec![0x41]);
        assert_eq!(maximum.reverse_lookup(0x41), u32::MAX);
    }
    let maximum =
        CpdfToUnicodeMap::from_data(b"1 beginbfrange <FFFFFFFF> <FFFFFFFF> <00660069> endbfrange");
    assert_eq!(maximum.lookup(u32::MAX), vec![0x66, 0x69]);
}

#[test]
fn HandleBeginBFRangeRejectsMismatchedBracket() {
    const kInput: &[u8] = b"1 beginbfrange<3><3>[<0041>}endbfrange\x00";
    let stream = CpdfStream::from_bytes(kInput);
    let map = CpdfToUnicodeMap::from_stream(&stream);
    assert_eq!(0, map.reverse_lookup(0x0041));
    assert_eq!(0, map.unicode_count_by_charcode(3));
}

#[test]
fn HandleBeginBFRangeBadCount() {
    {
        const kInput1: &[u8] = b"1 beginbfrange<1><2><0040><4><5><0050>endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput1);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        for unicode in 0x0039..0x0053 {
            assert_eq!(0, map.reverse_lookup(unicode));
        }
        for charcode in 0..7 {
            assert_eq!(0, map.unicode_count_by_charcode(charcode));
        }
    }
    {
        const kInput2: &[u8] = b"3 beginbfrange<1><2><0040><4><5><0050>endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput2);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        for unicode in 0x0039..0x0053 {
            assert_eq!(0, map.reverse_lookup(unicode));
        }
        for charcode in 0..7 {
            assert_eq!(0, map.unicode_count_by_charcode(charcode));
        }
    }
}

#[test]
fn HandleBeginBFRangeGoodCount() {
    const kInput: &[u8] = b"2 beginbfrange<1><2><0040><4><5><0050>endbfrange\x00";
    let stream = CpdfStream::from_bytes(kInput);
    let map = CpdfToUnicodeMap::from_stream(&stream);
    assert_eq!(0, map.reverse_lookup(0x0039));
    assert_eq!(1, map.reverse_lookup(0x0040));
    assert_eq!(2, map.reverse_lookup(0x0041));
    assert_eq!(0, map.reverse_lookup(0x0042));
    assert_eq!(0, map.reverse_lookup(0x0049));
    assert_eq!(4, map.reverse_lookup(0x0050));
    assert_eq!(5, map.reverse_lookup(0x0051));
    assert_eq!(0, map.reverse_lookup(0x0052));
    assert_eq!(0, map.unicode_count_by_charcode(0));
    assert_eq!(1, map.unicode_count_by_charcode(1));
    assert_eq!(1, map.unicode_count_by_charcode(2));
    assert_eq!(0, map.unicode_count_by_charcode(3));
    assert_eq!(1, map.unicode_count_by_charcode(4));
    assert_eq!(1, map.unicode_count_by_charcode(5));
    assert_eq!(0, map.unicode_count_by_charcode(6));
}

#[test]
fn HandleBeginBFRangeDestLargeValue() {
    const kInput: &[u8] = b"1 beginbfrange<0010><00ff><fff0>endbfrange\x00";
    let stream = CpdfStream::from_bytes(kInput);
    let map = CpdfToUnicodeMap::from_stream(&stream);
    assert_wide_cstr_eq(&[0xfff0], &map.lookup(0x10));
    assert_wide_cstr_eq(&[0xfff1], &map.lookup(0x11));
    // TODO(thestig): Should this return L"\xffff"?
    assert_wide_cstr_eq(&[], &map.lookup(0x1f));
    assert_wide_cstr_eq(&[], &map.lookup(0x20));
    assert_eq!(16, map.reverse_lookup(0xfff0));
    assert_eq!(17, map.reverse_lookup(0xfff1));
    assert_eq!(31, map.reverse_lookup(0xffff));
    // Selected configuration: WCHAR_T_IS_32_BIT.
    // TODO(crbug.com/374947848): Should be able to make this call if wchar_t is
    // 16-bit.
    // TODO(thestig): Should this reverse lookup work when the corresponding
    // Lookup() call does not work?
    assert_eq!(32, map.reverse_lookup(0x10000));
}

#[test]
fn InsertIntoMaps() {
    {
        // Both the CIDs and the unicodes are different.
        const kInput1: &[u8] = b"2 beginbfchar<1><0041><2><0042>endbfchar\x00";
        let stream = CpdfStream::from_bytes(kInput1);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_eq!(1, map.reverse_lookup(0x0041));
        assert_eq!(2, map.reverse_lookup(0x0042));
        assert_eq!(1, map.unicode_count_by_charcode(1));
        assert_eq!(1, map.unicode_count_by_charcode(2));
    }
    {
        // The same CID with different unicodes.
        const kInput2: &[u8] = b"2 beginbfrange<0><0><0041><0><0><0042>endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput2);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_eq!(0, map.reverse_lookup(0x0041));
        assert_eq!(0, map.reverse_lookup(0x0042));
        assert_eq!(2, map.unicode_count_by_charcode(0));
    }
    {
        // Duplicate mappings of CID 0 to unicode "A". There should be only 1 entry
        // in `reverse_map_`.
        const kInput3: &[u8] =
            b"1 beginbfrange<0><0>[<0041>]endbfrange\x0a1 beginbfchar<0><0041>endbfchar\x00";
        let stream = CpdfStream::from_bytes(kInput3);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_eq!(0, map.reverse_lookup(0x0041));
        assert_eq!(1, map.unicode_count_by_charcode(0));
    }
}

#[test]
fn SucceedingMapsSupersedePreceding() {
    // Adobe TN #5014: "Code mappings (unlike codespace ranges) may overlap, but
    // succeeding maps superceded preceding maps." Subset fonts commonly rely on
    // this, emitting a catch-all bfrange and then overriding individual codes.
    {
        // bfchar entries override an earlier catch-all bfrange, including when the
        // unicode is numerically above the charcode.
        const kInput1: &[u8] = b"1 beginbfrange<00><FF><0000>endbfrange\x0a3 beginbfchar<41><00F3><42><00BA><43><20AC>endbfchar\x00";
        let stream = CpdfStream::from_bytes(kInput1);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[0xf3], &map.lookup(0x41));
        assert_wide_cstr_eq(&[0xba], &map.lookup(0x42));
        assert_wide_cstr_eq(&[0x20ac], &map.lookup(0x43));
        // A charcode no later entry mentions keeps the range's mapping.
        assert_wide_cstr_eq(&[0x44], &map.lookup(0x44));
    }
    {
        // A later bfrange overrides an earlier one the same way.
        const kInput2: &[u8] =
            b"1 beginbfrange<00><FF><0000>endbfrange\x0a1 beginbfrange<70><71><00E9>endbfrange\x00";
        let stream = CpdfStream::from_bytes(kInput2);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[0xe9], &map.lookup(0x70));
        assert_wide_cstr_eq(&[0xea], &map.lookup(0x71));
        assert_wide_cstr_eq(&[0x72], &map.lookup(0x72));
    }
    {
        // A mapping to a lower unicode also wins when it comes later.
        const kInput3: &[u8] =
            b"1 beginbfchar<41><00F3>endbfchar\x0a1 beginbfchar<41><0041>endbfchar\x00";
        let stream = CpdfStream::from_bytes(kInput3);
        let map = CpdfToUnicodeMap::from_stream(&stream);
        assert_wide_cstr_eq(&[0x41], &map.lookup(0x41));
    }
}

#[test]
fn NonBmpUnicodeLookup() {
    const kInput: &[u8] = b"1 beginbfchar<01><d841de76>endbfchar\x00";
    let stream = CpdfStream::from_bytes(kInput);
    let map = CpdfToUnicodeMap::from_stream(&stream);
    assert_wide_cstr_eq(&[0xd841, 0xde76], &map.lookup(0x01));
    // Selected configuration: WCHAR_T_IS_32_BIT.
    // TODO(crbug.com/374947848): Should work if wchar_t is 16-bit.
    // TODO(crbug.com/374947848): Should return 1.
    // A valid UTF-16 surrogate pair reverse-maps as one Unicode scalar.
    // Native only indexes its internal multi-character indicator here.
    assert_eq!(1, map.reverse_lookup(0x20676));
}
