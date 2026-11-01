// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of core/fxcrt/widestring_unittest.cpp (wchar_t = u32).
//! Owned values retain embedded NULs; native literal constructors terminate
//! there. Production UTF/codepage functions supply selected conversion logic.
#![allow(non_snake_case, non_upper_case_globals)]

use super::fx_codepage::{multi_byte_to_wide_char, wide_char_to_multi_byte, FxCodePage};
use super::fx_string::{utf16_decode, utf16_encode, utf8_decode, utf8_encode};
use super::utf16::{is_high_surrogate, is_low_surrogate};
use crate::test_support::assert_wide_cstr_eq;

fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}
fn wide_literal(s: &str) -> Vec<u32> {
    s.chars()
        .take_while(|&c| c != '\0')
        .map(u32::from)
        .collect()
}

#[test]
fn EncodeEntities() {
    assert_eq!(
        super::fx_string::encode_entities(&wide("Symbols &<>'\".")),
        wide("Symbols &amp;&lt;&gt;&apos;&quot;."),
    );
}

#[test]
fn FromUTF8() {
    assert_wide_cstr_eq(&wide(""), &utf8_decode(&[]));
    assert_wide_cstr_eq(
        &wide("x\u{0080}\u{00ff}\u{d7ff}\u{e000}\u{ff2c}\u{ffff}y"),
        &utf8_decode("x\u{0080}\u{00ff}\u{d7ff}\u{e000}\u{ff2c}\u{ffff}y".as_bytes()),
    );
}
#[test]
fn FromUTF8Supplementary() {
    assert_wide_cstr_eq(
        &wide("\u{10000}\u{1f3a8}\u{10ffff}"),
        &utf8_decode("\u{10000}🎨\u{10ffff}".as_bytes()),
    );
}
#[test]
fn FromUTF8ErrorRecovery() {
    assert_wide_cstr_eq(&wide("(A)"), &utf8_decode(b"(\xc2\x41)")); // Invalid continuation.
    assert_wide_cstr_eq(&wide("()"), &utf8_decode(b"(\xc2\xc2)")); // Invalid continuation.
    assert_wide_cstr_eq(&wide("()"), &utf8_decode(b"(\xc2\xff\x80)")); // Invalid continuation.
    assert_wide_cstr_eq(&wide("()"), &utf8_decode(b"(\x80\x80)")); // Invalid leading.
    assert_wide_cstr_eq(&wide("()"), &utf8_decode(b"(\xff\x80\x80)")); // Invalid leading.
    assert_wide_cstr_eq(&wide("()"), &utf8_decode(b"(\xf8\x80\x80\x80\x80)")); // Invalid leading.
    assert_wide_cstr_eq(&wide("()"), &utf8_decode(b"(\xf8\x88\x80\x80\x80)")); // Invalid leading.
    assert_wide_cstr_eq(&wide("()"), &utf8_decode(b"(\xf4\x90\x80\x80)")); // Above U+10FFFF.
}
#[test]
fn UTF8EncodeDecodeConsistency() {
    let mut wstr = Vec::with_capacity(0x10000);
    for w in 0..0x10000u32 {
        if is_high_surrogate(w) || is_low_surrogate(w) {
            continue;
        }
        wstr.push(w);
    }
    assert_eq!(0xf800usize, wstr.len());
    let bstr = utf8_encode(&wstr);
    let wstr2 = utf8_decode(&bstr);
    assert_eq!(wstr, wstr2); // Owned comparison includes leading NUL.
}
#[test]
fn UTF8EncodeDecodeConsistencyUnpairedHighSurrogates() {
    let mut wstr = Vec::with_capacity(0x400);
    for w in 0xd800u32..=0xdbff {
        wstr.push(w);
    }
    assert_eq!(0x400usize, wstr.len());
    let bstr = utf8_encode(&wstr);
    let wstr2 = utf8_decode(&bstr);
    assert_eq!(wstr, wstr2);
}
#[test]
fn UTF8EncodeDecodeConsistencyUnpairedLowSurrogates() {
    let mut wstr = Vec::with_capacity(0x400);
    for w in 0xdc00u32..=0xdfff {
        wstr.push(w);
    }
    assert_eq!(0x400usize, wstr.len());
    let bstr = utf8_encode(&wstr);
    let wstr2 = utf8_decode(&bstr);
    assert_eq!(wstr, wstr2);
}
#[test]
fn FromUTF16BE() {
    let utf16be_decode_cases: &[(&[u8], Vec<u32>)] = &[
        (b"", wide("")),
        (b"\0a\0b\0c", wide("abc")),
        (b"\0a\0b\0c\0\0\0d\0e\0f", wide("abc\0def")),
        (b" &", wide("…")),
        (b"\xd8\x3c\xdf\xa8", wide("🎨")),
    ];
    for (i, (input, output)) in utf16be_decode_cases.iter().enumerate() {
        assert_eq!(utf16_decode(input, true), *output, "for case number {i}");
    }
}
#[test]
fn FromUTF16LE() {
    let utf16le_decode_cases: &[(&[u8], Vec<u32>)] = &[
        (b"", wide("")),
        (b"a\0b\0c\0", wide("abc")),
        (b"a\0b\0c\0\0\0d\0e\0f\0", wide("abc\0def")),
        (b"& ", wide("…")),
        (b"\x3c\xd8\xa8\xdf", wide("🎨")),
    ];
    for (i, (input, output)) in utf16le_decode_cases.iter().enumerate() {
        assert_eq!(utf16_decode(input, false), *output, "for case number {i}");
    }
}
#[test]
fn ToUTF16LE() {
    let utf16le_encode_cases: &[(&str, &[u8])] = &[
        ("", b"\0\0"),
        ("abc", b"a\0b\0c\0\0\0"),
        ("abcdef", b"a\0b\0c\0d\0e\0f\0\0\0"),
        ("abc\0def", b"a\0b\0c\0\0\0"),
        ("\u{aabb}\u{ccdd}", b"\xbb\xaa\xdd\xcc\0\0"),
        ("\u{3132}\u{6162}", b"\x32\x31\x62\x61\0\0"),
        ("🎨", b"\x3c\xd8\xa8\xdf\0\0"),
    ];
    for (i, (ws, bs)) in utf16le_encode_cases.iter().enumerate() {
        let mut result: Vec<u8> = utf16_encode(&wide_literal(ws))
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect();
        result.extend_from_slice(&[0, 0]);
        assert_eq!(*bs, result, "for case number {i}");
    }
}
#[test]
fn IsASCII() {
    assert!(wide("xy\u{007f}z").iter().all(|&c| c < 0x80));
    assert!(!wide("xy\u{0080}z").iter().all(|&c| c < 0x80));
    assert!(!wide("xy\u{2041}z").iter().all(|&c| c < 0x80));
}
fn equals_ascii(text: &[u32], ascii: &[u8]) -> bool {
    text.iter()
        .copied()
        .eq(ascii.iter().copied().map(u32::from))
}
fn equals_ascii_no_case(text: &[u32], ascii: &[u8]) -> bool {
    let lower = |c: u32| {
        if c >= b'A' as u32 && c <= b'Z' as u32 {
            c + 32
        } else {
            c
        }
    };
    text.iter()
        .copied()
        .map(lower)
        .eq(ascii.iter().copied().map(u32::from).map(lower))
}
#[test]
fn EqualsASCII() {
    assert!(equals_ascii(&wide(""), b""));
    assert!(!equals_ascii(&wide("A"), b""));
    assert!(!equals_ascii(&wide(""), b"A"));
    assert!(!equals_ascii(&wide("A"), b"B"));
    assert!(equals_ascii(&wide("ABC"), b"ABC"));
    assert!(!equals_ascii(&wide("ABC"), b"AEC"));
    assert!(!equals_ascii(&wide("\u{00c1}"), b"\x41"));
    assert!(!equals_ascii(&wide("\u{0141}"), b"\x41"));
}
#[test]
fn EqualsASCIINoCase() {
    assert!(equals_ascii_no_case(&wide(""), b""));
    assert!(!equals_ascii_no_case(&wide("A"), b"b"));
    assert!(equals_ascii_no_case(&wide("AbC"), b"aBc"));
    assert!(!equals_ascii_no_case(&wide("ABc"), b"AeC"));
    assert!(!equals_ascii_no_case(&wide("\u{00c1}"), b"\x41"));
    assert!(!equals_ascii_no_case(&wide("\u{0141}"), b"\x41"));
}
#[test]
fn ToASCII() {
    let kResult = b"x\x02\x7f\x22\x0cy";
    let result: Vec<u8> = wide("x\u{0082}\u{00ff}\u{0122}\u{208c}y")
        .into_iter()
        .map(|wc| (wc & 0x7f) as u8)
        .collect();
    assert_eq!(kResult.as_slice(), result);
}
#[test]
fn ToLatin1() {
    let kResult = b"x\x82\xff\x22\x8cy";
    let result: Vec<u8> = wide("x\u{0082}\u{00ff}\u{0122}\u{208c}y")
        .into_iter()
        .map(|wc| (wc & 0xff) as u8)
        .collect();
    assert_eq!(kResult.as_slice(), result);
}
fn to_def_ansi(text: &[u32]) -> Vec<u8> {
    let dest_len = wide_char_to_multi_byte(FxCodePage::DefAnsi, text, &mut []);
    if dest_len == 0 {
        return Vec::new();
    }
    let mut bstr = vec![0; dest_len];
    wide_char_to_multi_byte(FxCodePage::DefAnsi, text, &mut bstr);
    bstr
}
fn from_def_ansi(text: &[u8]) -> Vec<u32> {
    let dest_len = multi_byte_to_wide_char(FxCodePage::DefAnsi, text, &mut []);
    if dest_len == 0 {
        return Vec::new();
    }
    let mut wstr = vec![0; dest_len];
    multi_byte_to_wide_char(FxCodePage::DefAnsi, text, &mut wstr);
    wstr
}
#[test]
fn ToDefANSI() {
    assert_eq!(b"", to_def_ansi(&[]).as_slice());
    let kResult = b"x\x80\xffy"; // Selected non-Windows branch.
    assert_eq!(
        kResult.as_slice(),
        to_def_ansi(&wide("x\u{0080}\u{00ff}\u{0100}\u{208c}y"))
    );
}
#[test]
fn FromASCII() {
    let convert = |input: &[u8]| {
        input
            .iter()
            .map(|&c| u32::from(c & 0x7f))
            .collect::<Vec<_>>()
    };
    assert_wide_cstr_eq(&wide(""), &convert(&[]));
    let kResult = wide("x\u{0002}\u{007f}y");
    assert_wide_cstr_eq(&kResult, &convert(b"x\x82\xffy"));
}
#[test]
fn FromLatin1() {
    let convert = |input: &[u8]| {
        input
            .iter()
            .map(|&c| u32::from(c & 0xff))
            .collect::<Vec<_>>()
    };
    assert_wide_cstr_eq(&wide(""), &convert(&[]));
    let kResult = wide("x\u{0082}\u{00ff}y");
    assert_wide_cstr_eq(&kResult, &convert(b"x\x82\xffy"));
}
#[test]
fn FromDefANSI() {
    assert_wide_cstr_eq(&wide(""), &from_def_ansi(&[]));
    let kResult = wide("x\u{0080}\u{00ff}y"); // Selected non-Windows branch.
    assert_wide_cstr_eq(&kResult, &from_def_ansi(b"x\x80\xffy"));
}
fn format_integer(value: i32) -> Vec<u32> {
    let mut buf = [0u8; 32];
    super::fx_system::itoa(value, &mut buf, 10)
        .iter()
        .copied()
        .map(u32::from)
        .collect()
}
#[test]
fn FormatInteger() {
    assert_wide_cstr_eq(&wide("0"), &format_integer(0));
    assert_wide_cstr_eq(&wide("123456"), &format_integer(123456));
    assert_wide_cstr_eq(&wide("-123456"), &format_integer(-123456));
    assert_wide_cstr_eq(&wide("2147483647"), &format_integer(i32::MAX));
    assert_wide_cstr_eq(&wide("-2147483648"), &format_integer(i32::MIN));
}

use super::string_test_support::{w, View};
type WideView<'a> = View<'a, u32>;

#[test]
fn WideStringView_ConstexprCtors() {
    const NULL_STRING: &[u32] = &[];
    const _: () = assert!(NULL_STRING.len() == 0);
    const _: () = assert!(NULL_STRING.is_empty());
    const COPIED_NULL_STRING: &[u32] = NULL_STRING;
    const _: () = assert!(COPIED_NULL_STRING.len() == 0);
    const _: () = assert!(COPIED_NULL_STRING.is_empty());
}

#[test]
fn WideStringView_FromVector() {
    let null_vec = Vec::<u32>::new();
    let null_string = WideView::from_span(&null_vec);
    assert_eq!(0usize, null_string.GetLength());

    let lower_a_vec = vec!['a' as u32; 10];
    let lower_a_string = WideView::from_span(&lower_a_vec);
    assert_eq!(10usize, lower_a_string.GetLength());
    assert_eq!(WideView::literal(w!("aaaaaaaaaa")), lower_a_string);

    let mut cleared_vec = Vec::<u32>::new();
    cleared_vec.push(42);
    cleared_vec.pop();
    let cleared_string = WideView::from_span(&cleared_vec);
    assert_eq!(0usize, cleared_string.GetLength());
    assert!(!(cleared_string.has_data()));
}

#[test]
fn WideStringView_ElementAccess() {
    let abc = WideView::literal(w!("abc"));
    assert_eq!('a' as u32, abc[0]);
    assert_eq!('b' as u32, abc[1]);
    assert_eq!('c' as u32, abc[2]);
    assert!(std::panic::catch_unwind(|| { std::hint::black_box(abc[4]) }).is_err());
}

#[test]
fn WideStringView_OperatorLT() {
    let empty = WideView::default();
    let a = WideView::literal(w!("a"));
    let abc = WideView::literal(w!("\u{110}qq")); // Comes InsertAtFront despite endianness.
    let def = WideView::literal(w!("\u{1001}qq")); // Comes InsertAtBack despite endianness.
    let c_null = WideView::default();
    let c_empty = WideView::literal(&[]);
    let c_a = WideView::literal(w!("a"));
    let c_abc = WideView::literal(w!("\u{110}qq"));
    let c_def = WideView::literal(w!("\u{1001}qq"));

    assert!(!(empty < empty));
    assert!(!(a < a));
    assert!(!(abc < abc));
    assert!(!(def < def));
    assert!(!(c_null < empty));
    assert!(!(c_empty < empty));
    assert!(!(c_a < a));
    assert!(!(c_abc < abc));
    assert!(!(c_def < def));
    assert!(!(empty < c_null));
    assert!(!(empty < c_empty));
    assert!(!(a < c_a));
    assert!(!(abc < c_abc));
    assert!(!(def < c_def));

    assert!(empty < a);
    assert!(!(a < empty));
    assert!(empty < c_a);
    assert!(!(a < c_null));
    assert!(!(a < c_empty));

    assert!(empty < abc);
    assert!(!(abc < empty));
    assert!(empty < c_abc);
    assert!(!(abc < c_null));
    assert!(!(abc < c_empty));

    assert!(empty < def);
    assert!(!(def < empty));
    assert!(empty < c_def);
    assert!(!(def < c_null));
    assert!(!(def < c_empty));

    assert!(a < abc);
    assert!(!(abc < a));
    assert!(a < c_abc);
    assert!(!(abc < c_a));

    assert!(a < def);
    assert!(!(def < a));
    assert!(a < c_def);
    assert!(!(def < c_a));

    assert!(abc < def);
    assert!(!(def < abc));
    assert!(abc < c_def);
    assert!(!(def < c_abc));
}

#[test]
fn WideStringView_OperatorEQ() {
    let wide_string_c = WideView::literal(w!("hello"));
    assert!(wide_string_c == wide_string_c);

    let wide_string_c_same1 = WideView::literal(w!("hello"));
    assert!(wide_string_c == wide_string_c_same1);
    assert!(wide_string_c_same1 == wide_string_c);

    let wide_string_c_same2 = wide_string_c;
    assert!(wide_string_c == wide_string_c_same2);
    assert!(wide_string_c_same2 == wide_string_c);

    let wide_string_c1 = WideView::literal(w!("he"));
    let wide_string_c2 = WideView::literal(w!("hellp"));
    let wide_string_c3 = WideView::literal(w!("hellod"));
    assert!(!(wide_string_c == wide_string_c1));
    assert!(!(wide_string_c == wide_string_c2));
    assert!(!(wide_string_c == wide_string_c3));
    assert!(!(wide_string_c1 == wide_string_c));
    assert!(!(wide_string_c2 == wide_string_c));
    assert!(!(wide_string_c3 == wide_string_c));

    let wide_string_same1 = (w!("hello")).to_vec();
    assert!(wide_string_c == wide_string_same1);
    assert!(wide_string_same1 == wide_string_c);

    let wide_string1 = (w!("he")).to_vec();
    let wide_string2 = (w!("hellp")).to_vec();
    let wide_string3 = (w!("hellod")).to_vec();
    assert!(!(wide_string_c == wide_string1));
    assert!(!(wide_string_c == wide_string2));
    assert!(!(wide_string_c == wide_string3));
    assert!(!(wide_string1 == wide_string_c));
    assert!(!(wide_string2 == wide_string_c));
    assert!(!(wide_string3 == wide_string_c));

    let c_string_same1 = WideView::literal(w!("hello"));
    assert!(wide_string_c == c_string_same1);
    assert!(c_string_same1 == wide_string_c);

    let c_string1 = WideView::literal(w!("he"));
    let c_string2 = WideView::literal(w!("hellp"));
    let c_string3 = WideView::literal(w!("hellod"));
    assert!(!(wide_string_c == c_string1));
    assert!(!(wide_string_c == c_string2));
    assert!(!(wide_string_c == c_string3));

    assert!(!(c_string1 == wide_string_c));
    assert!(!(c_string2 == wide_string_c));
    assert!(!(c_string3 == wide_string_c));
}

#[test]
fn WideStringView_OperatorNE() {
    let wide_string_c = WideView::literal(w!("hello"));
    assert!(!(wide_string_c != wide_string_c));

    let wide_string_c_same1 = WideView::literal(w!("hello"));
    assert!(!(wide_string_c != wide_string_c_same1));
    assert!(!(wide_string_c_same1 != wide_string_c));

    let wide_string_c_same2 = wide_string_c;
    assert!(!(wide_string_c != wide_string_c_same2));
    assert!(!(wide_string_c_same2 != wide_string_c));

    let wide_string_c1 = WideView::literal(w!("he"));
    let wide_string_c2 = WideView::literal(w!("hellp"));
    let wide_string_c3 = WideView::literal(w!("hellod"));
    assert!(wide_string_c != wide_string_c1);
    assert!(wide_string_c != wide_string_c2);
    assert!(wide_string_c != wide_string_c3);
    assert!(wide_string_c1 != wide_string_c);
    assert!(wide_string_c2 != wide_string_c);
    assert!(wide_string_c3 != wide_string_c);

    let wide_string_same1 = (w!("hello")).to_vec();
    assert!(!(wide_string_c != wide_string_same1));
    assert!(!(wide_string_same1 != wide_string_c));

    let wide_string1 = (w!("he")).to_vec();
    let wide_string2 = (w!("hellp")).to_vec();
    let wide_string3 = (w!("hellod")).to_vec();
    assert!(wide_string_c != wide_string1);
    assert!(wide_string_c != wide_string2);
    assert!(wide_string_c != wide_string3);
    assert!(wide_string1 != wide_string_c);
    assert!(wide_string2 != wide_string_c);
    assert!(wide_string3 != wide_string_c);

    let c_string_same1 = WideView::literal(w!("hello"));
    assert!(!(wide_string_c != c_string_same1));
    assert!(!(c_string_same1 != wide_string_c));

    let c_string1 = WideView::literal(w!("he"));
    let c_string2 = WideView::literal(w!("hellp"));
    let c_string3 = WideView::literal(w!("hellod"));
    assert!(wide_string_c != c_string1);
    assert!(wide_string_c != c_string2);
    assert!(wide_string_c != c_string3);

    assert!(c_string1 != wide_string_c);
    assert!(c_string2 != wide_string_c);
    assert!(c_string3 != wide_string_c);
}

#[test]
fn WideStringView_Find() {
    let null_string = WideView::default();
    assert!(!(null_string.Find('a' as u32).is_some()));
    assert!(!(null_string.Find('\0' as u32).is_some()));

    let empty_string = WideView::literal(&[]);
    assert!(!(empty_string.Find('a' as u32).is_some()));
    assert!(!(empty_string.Find('\0' as u32).is_some()));

    let single_string = WideView::literal(w!("a"));
    let mut result = single_string.Find('a' as u32);
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    assert!(!(single_string.Find('b' as u32).is_some()));
    assert!(!(single_string.Find('\0' as u32).is_some()));

    let longer_string = WideView::literal(w!("abccc"));
    result = longer_string.Find('a' as u32);
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    result = longer_string.Find('c' as u32);
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
    assert!(!(longer_string.Find('d' as u32).is_some()));
    assert!(!(longer_string.Find('\0' as u32).is_some()));

    let hibyte_string = WideView::literal(w!("ab\u{ff8c}def"));
    result = hibyte_string.Find('\u{ff8c}' as u32);
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
}

#[test]
fn WideStringView_NullIterator() {
    let null_str = WideView::default();
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in null_str.span() {
        sum += c as i32; // Avoid unused arg warnings.
        any_present = true;
    }
    assert!(!(any_present));
    assert_eq!((0) as i32, sum);
}

#[test]
fn WideStringView_EmptyIterator() {
    let empty_str = WideView::literal(&[]);
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in empty_str.span() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(!(any_present));
    assert_eq!((0) as i32, sum);
}

#[test]
fn WideStringView_OneCharIterator() {
    let one_str = WideView::literal(w!("a"));
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in one_str.span() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(any_present);
    assert_eq!((97i32 as i32) as i32, sum);
}

#[test]
fn WideStringView_MultiCharIterator() {
    let one_str = WideView::literal(w!("abc"));
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in one_str.span() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(any_present);
    assert_eq!((97i32 + 98i32 + 99i32 as i32) as i32, sum);
}

#[test]
fn WideStringView_EmptyReverseIterator() {
    let empty = WideView::default();
    let iter = 0usize;
    assert!(iter == empty.GetLength());
    assert!(!(iter != empty.GetLength()));
    assert!(!(iter < empty.GetLength()));
}

#[test]
fn WideStringView_OneCharReverseIterator() {
    let one_str = WideView::literal(w!("a"));
    let mut iter = 0usize;
    assert!(!(iter == one_str.GetLength()));
    assert!(iter != one_str.GetLength());
    assert!(iter < one_str.GetLength());

    let ch = {
        let value = one_str[one_str.GetLength() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!('a' as u32, ch);
    assert!(iter == one_str.GetLength());
    assert!(!(iter != one_str.GetLength()));
    assert!(!(iter < one_str.GetLength()));
}

#[test]
fn WideStringView_MultiCharReverseIterator() {
    let multi_str = WideView::literal(w!("abcd"));
    let mut iter = 0usize;
    assert!(!(iter == multi_str.GetLength()));

    let mut ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!('d' as u32, ch);
    assert_eq!('c' as u32, multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        iter += 1;
        multi_str[multi_str.GetLength() - 1 - iter]
    };
    assert_eq!('b' as u32, ch);
    assert_eq!('b' as u32, multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!('b' as u32, ch);
    assert_eq!('a' as u32, multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!('a' as u32, ch);
    assert!(iter == multi_str.GetLength());

    ch = {
        iter -= 1;
        multi_str[multi_str.GetLength() - 1 - iter]
    };
    assert_eq!('a' as u32, ch);
    assert_eq!('a' as u32, multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter -= 1;
        value
    };
    assert_eq!('a' as u32, ch);
    assert_eq!('b' as u32, multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter -= 1;
        value
    };
    assert_eq!('b' as u32, ch);
    assert_eq!('c' as u32, multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        iter -= 1;
        multi_str[multi_str.GetLength() - 1 - iter]
    };
    assert_eq!('d' as u32, ch);
    assert_eq!('d' as u32, multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(iter == 0usize);
}

#[test]
fn WideStringView_AnyAllNoneOf() {
    let str = WideView::literal(w!("aaaaaaaaaaaaaaaaab"));
    assert!(!(str.span().iter().all(|&c| c == ('a' as u32))));

    assert!(!(!str.span().iter().any(|&c| c == ('a' as u32))));

    assert!(str.span().iter().any(|&c| c == ('a' as u32)));

    assert!(str.span().contains(&('a' as u32)));
    assert!(str.span().contains(&('b' as u32)));
    assert!(!(str.span().contains(&('z' as u32))));
}

#[test]
fn WideStringView_TrimmedRight() {
    let fred = WideView::literal(w!("FRED"));
    assert_eq!(WideView::literal(w!("FRED")), fred.TrimmedRight('E' as u32));
    assert_eq!(WideView::literal(w!("FRE")), fred.TrimmedRight('D' as u32));
    let fredd = WideView::literal(w!("FREDD"));
    assert_eq!(WideView::literal(w!("FRE")), fredd.TrimmedRight('D' as u32));
}

// All 31 file-scope native static assertions; the four constructor assertions
// inside the native GTest case are retained in StringView_ConstexprCtors.
const fn const_view_is_ascii(text: &[u32]) -> bool {
    let mut i = 0;
    while i < text.len() {
        if text[i] > 127 {
            return false;
        }
        i += 1;
    }
    true
}
const fn const_view_eq_ascii(text: &[u32], other: &[u8], no_case: bool) -> bool {
    if text.len() != other.len() {
        return false;
    }
    let mut i = 0;
    while i < text.len() {
        if text[i] > 127 {
            return false;
        }
        let a = text[i] as u8;
        let b = other[i];
        if no_case {
            if a.to_ascii_lowercase() != b.to_ascii_lowercase() {
                return false;
            }
        } else if a != b {
            return false;
        }
        i += 1;
    }
    true
}
const fn const_view_lt(a: &[u32], b: &[u32]) -> bool {
    let mut i = 0;
    while i < a.len() && i < b.len() {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
        i += 1;
    }
    a.len() < b.len()
}
const _: () = {
    use super::string_test_support::const_substr;
    let kConstexprView: &[u32] = w!("hello");
    assert!(kConstexprView.len() == 5);
    assert!(kConstexprView[0] == 104);
    assert!(kConstexprView[4] == 111);
    assert!(
        (if kConstexprView.is_empty() {
            0
        } else {
            kConstexprView[0]
        }) == 104
    );
    assert!(
        (if kConstexprView.is_empty() {
            0
        } else {
            kConstexprView[kConstexprView.len() - 1]
        }) == 111
    );
    let kLiteral: &[u32] = w!("world");
    let kConstexprViewFromVar = kLiteral;
    assert!(kConstexprViewFromVar.len() == 5);
    assert!(kConstexprViewFromVar[0] == 119);
    let kConstexprViewWithNul: &[u32] = w!("with\0nul");
    assert!(kConstexprViewWithNul.len() == 8);
    assert!(kConstexprViewWithNul[4] == 0);
    assert!(kConstexprViewWithNul[5] == 110);
    let kSubstrView = const_substr(kConstexprView, 1, 3);
    assert!(kSubstrView.len() == 3);
    assert!(kSubstrView[0] == 101);
    assert!(kSubstrView[2] == 108);
    let kEmptyView: &[u32] = &[];
    assert!(
        (if kEmptyView.is_empty() {
            0
        } else {
            kEmptyView[0]
        }) == 0
    );
    assert!(
        (if kEmptyView.is_empty() {
            0
        } else {
            kEmptyView[kEmptyView.len() - 1]
        }) == 0
    );
    assert!(const_substr(kConstexprView, 6, kConstexprView.len().wrapping_sub(6)).is_empty());
    assert!(const_substr(kConstexprView, 1, 10).is_empty());
    assert!(const_substr(kConstexprView, 6, 1).is_empty());
    assert!(const_substr(kConstexprView, 0, 10).is_empty());
    assert!(const_substr(kConstexprView, kConstexprView.len().wrapping_sub(10), 10).is_empty());
    let kViewA: &[u32] = w!("abc");
    let kViewB: &[u32] = w!("def");
    assert!(const_view_lt(kViewA, kViewB));
    assert!(!const_view_lt(kViewB, kViewA));
    assert!(const_view_is_ascii(kConstexprView));
    let kNonASCII: &[u32] = w!("hel\u{80}lo");
    assert!(!const_view_is_ascii(kNonASCII));
    let kASCIIWithNul: &[u32] = w!("hel\0lo");
    assert!(const_view_is_ascii(kASCIIWithNul));
    assert!(const_view_eq_ascii(kConstexprView, b"hello", false));
    assert!(!const_view_eq_ascii(kConstexprView, b"world", false));
    assert!(const_view_eq_ascii(kASCIIWithNul, b"hel\0lo", false));
    assert!(const_view_eq_ascii(kConstexprView, b"HeLLo", true));
    assert!(!const_view_eq_ascii(kConstexprView, b"WoRLd", true));
    assert!(const_view_eq_ascii(kASCIIWithNul, b"HeL\0Lo", true));
};

#[test]
fn OStreamOverload() {
    let mut stream = Vec::<u8>::new();

    // Basic case, empty string
    let mut str = Vec::<u32>::new();
    stream.extend_from_slice(&utf8_encode(&str));
    assert_eq!(b"", stream.as_slice());

    // Basic case, wide character
    str = WideView::literal(w!("\u{20ac}")).span().to_vec();
    stream.extend_from_slice(&utf8_encode(&str));
    assert_eq!(b"\xe2\x82\xac", stream.as_slice());

    // Basic case, non-empty string
    str = WideView::literal(w!("def")).span().to_vec();
    stream.clear();
    stream.extend_from_slice(b"abc");
    stream.extend_from_slice(&utf8_encode(&str));
    stream.extend_from_slice(b"ghi");
    assert_eq!(b"abcdefghi", stream.as_slice());

    // Changing the WideString does not change the stream it was written to.
    str = WideView::literal(w!("123")).span().to_vec();
    assert_eq!(b"abcdefghi", stream.as_slice());

    // Writing it again to the stream will use the latest value.
    stream.clear();
    stream.extend_from_slice(b"abc");
    stream.extend_from_slice(&utf8_encode(&str));
    stream.extend_from_slice(b"ghi");
    assert_eq!(b"abc123ghi", stream.as_slice());

    let stringWithNulls = ['x' as u32, 'y' as u32, 0, 'z' as u32];

    // Writing a WideString with nulls and no specified length treats it as
    // a C-style null-terminated string.
    str = WideView::literal(&stringWithNulls).span().to_vec();
    assert_eq!(2usize, str.len());
    stream.clear();
    stream.extend_from_slice(&utf8_encode(&str));
    assert_eq!(2usize, stream.len());

    // Writing a WideString with nulls but specifying its length treats it as
    // a C++-style string.
    // SAFETY: known fixed-length string.
    str = WideView::explicit(&stringWithNulls, 4).span().to_vec();
    assert_eq!(4usize, str.len());
    stream.clear();
    stream.extend_from_slice(&utf8_encode(&str));
    assert_eq!(4usize, stream.len());

    // << operators can be chained.
    let str1 = WideView::literal(w!("abc")).span().to_vec();
    let str2 = WideView::literal(w!("def")).span().to_vec();
    stream.clear();
    stream.extend_from_slice(&utf8_encode(&str1));
    stream.extend_from_slice(&utf8_encode(&str2));
    assert_eq!(b"abcdef", stream.as_slice());
}

#[test]
fn WideOStreamOverload() {
    let mut stream = Vec::<u32>::new();

    // Basic case, empty string
    let mut str = Vec::<u32>::new();
    stream.extend_from_slice(&str);
    assert_eq!(w!(""), stream.as_slice());

    // Basic case, wide character
    str = WideView::literal(w!("\u{20ac}")).span().to_vec();
    stream.extend_from_slice(&str);
    assert_eq!(w!("\u{20ac}"), stream.as_slice());

    // Basic case, non-empty string
    str = WideView::literal(w!("def")).span().to_vec();
    stream.clear();
    stream.extend_from_slice(w!("abc"));
    stream.extend_from_slice(&str);
    stream.extend_from_slice(w!("ghi"));
    assert_eq!(w!("abcdefghi"), stream.as_slice());

    // Changing the WideString does not change the stream it was written to.
    str = WideView::literal(w!("123")).span().to_vec();
    assert_eq!(w!("abcdefghi"), stream.as_slice());

    // Writing it again to the stream will use the latest value.
    stream.clear();
    stream.extend_from_slice(w!("abc"));
    stream.extend_from_slice(&str);
    stream.extend_from_slice(w!("ghi"));
    assert_eq!(w!("abc123ghi"), stream.as_slice());

    let stringWithNulls = ['x' as u32, 'y' as u32, 0, 'z' as u32];

    // Writing a WideString with nulls and no specified length treats it as
    // a C-style null-terminated string.
    str = WideView::literal(&stringWithNulls).span().to_vec();
    assert_eq!(2usize, str.len());
    stream.clear();
    stream.extend_from_slice(&str);
    assert_eq!(2usize, stream.len());

    // Writing a WideString with nulls but specifying its length treats it as
    // a C++-style string.
    str = WideView::explicit(&stringWithNulls, 4).span().to_vec();
    assert_eq!(4usize, str.len());
    stream.clear();
    stream.extend_from_slice(&str);
    assert_eq!(4usize, stream.len());

    // << operators can be chained.
    let str1 = WideView::literal(w!("abc")).span().to_vec();
    let str2 = WideView::literal(w!("def")).span().to_vec();
    stream.clear();
    stream.extend_from_slice(&str1);
    stream.extend_from_slice(&str2);
    assert_eq!(w!("abcdef"), stream.as_slice());
}

#[test]
fn WideStringView_OStreamOverload() {
    // Basic case, empty string
    {
        let mut stream = Vec::<u8>::new();
        let str = WideView::default();
        stream.extend_from_slice(&utf8_encode(str.span()));
        assert_eq!(b"", stream.as_slice());
    }

    // Basic case, non-empty string
    {
        let mut stream = Vec::<u8>::new();
        let str = WideView::literal(w!("def"));
        stream.extend_from_slice(b"abc");
        stream.extend_from_slice(&utf8_encode(str.span()));
        stream.extend_from_slice(b"ghi");
        assert_eq!(b"abcdefghi", stream.as_slice());
    }

    // Basic case, wide character
    {
        let mut stream = Vec::<u8>::new();
        let str = WideView::literal(w!("\u{20ac}"));
        stream.extend_from_slice(&utf8_encode(str.span()));
        assert_eq!(b"\xe2\x82\xac", stream.as_slice());
    }

    // Changing the WideStringView does not change the stream it was written to.
    {
        let mut stream = Vec::<u8>::new();
        let mut str = WideView::literal(w!("abc"));
        stream.extend_from_slice(&utf8_encode(str.span()));
        str = WideView::literal(w!("123"));
        std::hint::black_box(str);
        assert_eq!(b"abc", stream.as_slice());
    }

    // Writing it again to the stream will use the latest value.
    {
        let mut stream = Vec::<u8>::new();
        let mut str = WideView::literal(w!("abc"));
        stream.extend_from_slice(&utf8_encode(str.span()));
        stream.clear();
        str = WideView::literal(w!("123"));
        stream.extend_from_slice(&utf8_encode(str.span()));
        assert_eq!(b"123", stream.as_slice());
    }

    // Writing a WideStringView with nulls and no specified length treats it as
    // a C-style null-terminated string.
    {
        let stringWithNulls = ['x' as u32, 'y' as u32, 0, 'z' as u32];
        let mut stream = Vec::<u8>::new();
        let mut str = WideView::literal(&stringWithNulls);
        assert_eq!(2usize, str.GetLength());
        stream.extend_from_slice(&utf8_encode(str.span()));
        assert_eq!(2usize, stream.len());
        str = WideView::literal(w!(""));
        std::hint::black_box(str);
    }

    // Writing a WideStringView with nulls but specifying its length treats it as
    // a C++-style string.
    {
        let stringWithNulls = ['x' as u32, 'y' as u32, 0, 'z' as u32];
        // SAFETY: known array above.
        let mut str = WideView::explicit(&stringWithNulls, 4);
        let mut stream = Vec::<u8>::new();
        assert_eq!(4usize, str.GetLength());
        stream.extend_from_slice(&utf8_encode(str.span()));
        assert_eq!(4usize, stream.len());
        str = WideView::literal(w!(""));
        std::hint::black_box(str);
    }

    // << operators can be chained.
    {
        let mut stream = Vec::<u8>::new();
        let str1 = WideView::literal(w!("abc"));
        let str2 = WideView::literal(w!("def"));
        stream.extend_from_slice(&utf8_encode(str1.span()));
        stream.extend_from_slice(&utf8_encode(str2.span()));
        assert_eq!(b"abcdef", stream.as_slice());
    }
}

#[test]
fn WideStringView_WideOStreamOverload() {
    // Basic case, empty string
    {
        let mut stream = Vec::<u32>::new();
        let str = WideView::default();
        stream.extend_from_slice(str.span());
        assert_eq!(w!(""), stream.as_slice());
    }

    // Basic case, non-empty string
    {
        let mut stream = Vec::<u32>::new();
        let str = WideView::literal(w!("def"));
        stream.extend(b"abc".iter().copied().map(u32::from));
        stream.extend_from_slice(str.span());
        stream.extend(b"ghi".iter().copied().map(u32::from));
        assert_eq!(w!("abcdefghi"), stream.as_slice());
    }

    // Basic case, wide character
    {
        let mut stream = Vec::<u32>::new();
        let str = WideView::literal(w!("\u{20ac}"));
        stream.extend_from_slice(str.span());
        assert_eq!(w!("\u{20ac}"), stream.as_slice());
    }

    // Changing the WideStringView does not change the stream it was written to.
    {
        let mut stream = Vec::<u32>::new();
        let mut str = WideView::literal(w!("abc"));
        stream.extend_from_slice(str.span());
        str = WideView::literal(w!("123"));
        std::hint::black_box(str);
        assert_eq!(w!("abc"), stream.as_slice());
    }

    // Writing it again to the stream will use the latest value.
    {
        let mut stream = Vec::<u32>::new();
        let mut str = WideView::literal(w!("abc"));
        stream.extend_from_slice(str.span());
        stream.clear();
        str = WideView::literal(w!("123"));
        stream.extend_from_slice(str.span());
        assert_eq!(w!("123"), stream.as_slice());
    }

    // Writing a WideStringView with nulls and no specified length treats it as
    // a C-style null-terminated string.
    {
        let stringWithNulls = ['x' as u32, 'y' as u32, 0, 'z' as u32];
        let mut stream = Vec::<u32>::new();
        let str = WideView::literal(&stringWithNulls);
        assert_eq!(2usize, str.GetLength());
        stream.extend_from_slice(str.span());
        assert_eq!(2usize, stream.len());
    }

    // Writing a WideStringView with nulls but specifying its length treats it as
    // a C++-style string.
    {
        let stringWithNulls = ['x' as u32, 'y' as u32, 0, 'z' as u32];
        // SAFETY: known array above.
        let str = WideView::explicit(&stringWithNulls, 4);
        let mut stream = Vec::<u32>::new();
        assert_eq!(4usize, str.GetLength());
        stream.extend_from_slice(str.span());
        assert_eq!(4usize, stream.len());
    }

    // << operators can be chained.
    {
        let mut stream = Vec::<u32>::new();
        let str1 = WideView::literal(w!("abc"));
        let str2 = WideView::literal(w!("def"));
        stream.extend_from_slice(str1.span());
        stream.extend_from_slice(str2.span());
        assert_eq!(w!("abcdef"), stream.as_slice());
    }
}

#[test]
fn OperatorLT() {
    let empty = Vec::<u32>::new();
    let a = (w!("a")).to_vec();
    let ab = (w!("ab")).to_vec();
    let abc = (w!("\u{110}qq")).to_vec(); // Comes before despite endianness.
    let def = (w!("\u{1001}qq")).to_vec(); // Comes after despite endianness.
    let v_empty = WideView::default();
    let v_a = WideView::literal(w!("a"));
    let v_ab = WideView::literal(w!("ab"));
    let v_abc = WideView::literal(w!("\u{110}qq"));
    let v_def = WideView::literal(w!("\u{1001}qq"));
    let c_null = WideView::default();
    let c_empty = WideView::literal(&[]);
    let c_a = WideView::literal(w!("a"));
    let c_ab = WideView::literal(w!("ab"));
    let c_abc = WideView::literal(w!("\u{110}qq"));
    let c_def = WideView::literal(w!("\u{1001}qq"));

    assert!(!(empty < empty));
    assert!(!(a < a));
    assert!(!(abc < abc));
    assert!(!(def < def));
    assert!(!(c_null < empty));
    assert!(!(c_empty < empty));
    assert!(!(c_a < a));
    assert!(!(c_abc < abc));
    assert!(!(c_def < def));
    assert!(!(empty < c_null));
    assert!(!(empty < c_empty));
    assert!(!(a < c_a));
    assert!(!(abc < c_abc));
    assert!(!(def < c_def));
    assert!(!(empty < v_empty));
    assert!(!(a < v_a));
    assert!(!(abc < v_abc));
    assert!(!(def < v_def));

    assert!(empty < a);
    assert!(!(a < empty));
    assert!(c_null < a);
    assert!(c_empty < a);
    assert!(!(c_a < empty));
    assert!(empty < c_a);
    assert!(!(a < c_null));
    assert!(!(a < c_empty));
    assert!(empty < v_a);
    assert!(!(a < v_empty));

    assert!(empty < abc);
    assert!(!(abc < empty));
    assert!(c_null < abc);
    assert!(c_empty < abc);
    assert!(!(c_abc < empty));
    assert!(empty < c_abc);
    assert!(!(abc < c_null));
    assert!(!(abc < c_empty));
    assert!(empty < v_abc);
    assert!(!(abc < v_empty));

    assert!(empty < def);
    assert!(!(def < empty));
    assert!(c_null < def);
    assert!(c_empty < def);
    assert!(!(c_def < empty));
    assert!(empty < c_def);
    assert!(!(def < c_null));
    assert!(!(def < c_empty));
    assert!(empty < v_def);
    assert!(!(def < v_empty));

    assert!(a < abc);
    assert!(!(abc < a));
    assert!(c_a < abc);
    assert!(!(c_abc < a));
    assert!(a < c_abc);
    assert!(!(abc < c_a));
    assert!(a < v_abc);
    assert!(!(abc < v_a));

    assert!(a < def);
    assert!(!(def < a));
    assert!(c_a < def);
    assert!(!(c_def < a));
    assert!(a < c_def);
    assert!(!(def < c_a));
    assert!(a < v_def);
    assert!(!(def < v_a));

    assert!(abc < def);
    assert!(!(def < abc));
    assert!(c_abc < def);
    assert!(!(c_def < abc));
    assert!(abc < c_def);
    assert!(!(def < c_abc));
    assert!(abc < v_def);
    assert!(!(def < v_abc));

    assert!(a < ab);
    assert!(a < c_ab);
    assert!(a < v_ab);
    assert!(c_a < ab);
    assert!(c_a < v_ab);
    assert!(v_a < c_ab);
    assert!(v_a < v_ab);
}

#[test]
fn OperatorEQ() {
    let null_string = Vec::<u32>::new();
    assert!(null_string == null_string);

    let empty_string: Vec<u32> = (&[]).to_vec();
    assert!(empty_string == empty_string);
    assert!(empty_string == null_string);
    assert!(null_string == empty_string);

    let mut deleted_string = (w!("hello")).to_vec();
    super::string_test_support::delete(&mut deleted_string, 0, 5);
    assert!(deleted_string == deleted_string);
    assert!(deleted_string == null_string);
    assert!(deleted_string == empty_string);
    assert!(null_string == deleted_string);
    assert!(null_string == empty_string);

    let wide_string = (w!("hello")).to_vec();
    assert!(wide_string == wide_string);
    assert!(!(wide_string == null_string));
    assert!(!(wide_string == empty_string));
    assert!(!(wide_string == deleted_string));
    assert!(!(null_string == wide_string));
    assert!(!(empty_string == wide_string));
    assert!(!(deleted_string == wide_string));

    let wide_string_same1 = (w!("hello")).to_vec();
    assert!(wide_string == wide_string_same1);
    assert!(wide_string_same1 == wide_string);

    let wide_string_same2 = wide_string.clone();
    assert!(wide_string == wide_string_same2);
    assert!(wide_string_same2 == wide_string);

    let wide_string1 = (w!("he")).to_vec();
    let wide_string2 = (w!("hellp")).to_vec();
    let wide_string3 = (w!("hellod")).to_vec();
    assert!(!(wide_string == wide_string1));
    assert!(!(wide_string == wide_string2));
    assert!(!(wide_string == wide_string3));
    assert!(!(wide_string1 == wide_string));
    assert!(!(wide_string2 == wide_string));
    assert!(!(wide_string3 == wide_string));

    let null_string_c = WideView::default();
    let empty_string_c = WideView::literal(&[]);
    assert!(null_string == null_string_c);
    assert!(null_string == empty_string_c);
    assert!(empty_string == null_string_c);
    assert!(empty_string == empty_string_c);
    assert!(deleted_string == null_string_c);
    assert!(deleted_string == empty_string_c);
    assert!(null_string_c == null_string);
    assert!(empty_string_c == null_string);
    assert!(null_string_c == empty_string);
    assert!(empty_string_c == empty_string);
    assert!(null_string_c == deleted_string);
    assert!(empty_string_c == deleted_string);

    let wide_string_c_same1 = WideView::literal(w!("hello"));
    assert!(wide_string == wide_string_c_same1);
    assert!(wide_string_c_same1 == wide_string);

    let wide_string_c1 = WideView::literal(w!("he"));
    let wide_string_c2 = WideView::literal(w!("hellp"));
    let wide_string_c3 = WideView::literal(w!("hellod"));
    assert!(!(wide_string == wide_string_c1));
    assert!(!(wide_string == wide_string_c2));
    assert!(!(wide_string == wide_string_c3));
    assert!(!(wide_string_c1 == wide_string));
    assert!(!(wide_string_c2 == wide_string));
    assert!(!(wide_string_c3 == wide_string));

    let c_null_string = WideView::default();
    let c_empty_string = WideView::literal(&[]);
    assert!(null_string == c_null_string);
    assert!(null_string == c_empty_string);
    assert!(empty_string == c_null_string);
    assert!(empty_string == c_empty_string);
    assert!(deleted_string == c_null_string);
    assert!(deleted_string == c_empty_string);
    assert!(c_null_string == null_string);
    assert!(c_empty_string == null_string);
    assert!(c_null_string == empty_string);
    assert!(c_empty_string == empty_string);
    assert!(c_null_string == deleted_string);
    assert!(c_empty_string == deleted_string);

    let c_string_same1 = WideView::literal(w!("hello"));
    assert!(wide_string == c_string_same1);
    assert!(c_string_same1 == wide_string);

    let c_string1 = WideView::literal(w!("he"));
    let c_string2 = WideView::literal(w!("hellp"));
    let c_string3 = WideView::literal(w!("hellod"));
    assert!(!(wide_string == c_string1));
    assert!(!(wide_string == c_string2));
    assert!(!(wide_string == c_string3));
    assert!(!(c_string1 == wide_string));
    assert!(!(c_string2 == wide_string));
    assert!(!(c_string3 == wide_string));
}

#[test]
fn OperatorNE() {
    let null_string = Vec::<u32>::new();
    assert!(!(null_string != null_string));

    let empty_string: Vec<u32> = (&[]).to_vec();
    assert!(!(empty_string != empty_string));
    assert!(!(empty_string != null_string));
    assert!(!(null_string != empty_string));

    let mut deleted_string = (w!("hello")).to_vec();
    super::string_test_support::delete(&mut deleted_string, 0, 5);
    assert!(!(deleted_string != deleted_string));
    assert!(!(deleted_string != null_string));
    assert!(!(deleted_string != empty_string));
    assert!(!(null_string != deleted_string));
    assert!(!(null_string != empty_string));

    let wide_string = (w!("hello")).to_vec();
    assert!(!(wide_string != wide_string));
    assert!(wide_string != null_string);
    assert!(wide_string != empty_string);
    assert!(wide_string != deleted_string);
    assert!(null_string != wide_string);
    assert!(empty_string != wide_string);
    assert!(deleted_string != wide_string);

    let wide_string_same1 = (w!("hello")).to_vec();
    assert!(!(wide_string != wide_string_same1));
    assert!(!(wide_string_same1 != wide_string));

    let wide_string_same2 = wide_string.clone();
    assert!(!(wide_string != wide_string_same2));
    assert!(!(wide_string_same2 != wide_string));

    let wide_string1 = (w!("he")).to_vec();
    let wide_string2 = (w!("hellp")).to_vec();
    let wide_string3 = (w!("hellod")).to_vec();
    assert!(wide_string != wide_string1);
    assert!(wide_string != wide_string2);
    assert!(wide_string != wide_string3);
    assert!(wide_string1 != wide_string);
    assert!(wide_string2 != wide_string);
    assert!(wide_string3 != wide_string);

    let null_string_c = WideView::default();
    let empty_string_c = WideView::literal(&[]);
    assert!(!(null_string != null_string_c));
    assert!(!(null_string != empty_string_c));
    assert!(!(empty_string != null_string_c));
    assert!(!(empty_string != empty_string_c));
    assert!(!(deleted_string != null_string_c));
    assert!(!(deleted_string != empty_string_c));
    assert!(!(null_string_c != null_string));
    assert!(!(empty_string_c != null_string));
    assert!(!(null_string_c != empty_string));
    assert!(!(empty_string_c != empty_string));

    let wide_string_c_same1 = WideView::literal(w!("hello"));
    assert!(!(wide_string != wide_string_c_same1));
    assert!(!(wide_string_c_same1 != wide_string));

    let wide_string_c1 = WideView::literal(w!("he"));
    let wide_string_c2 = WideView::literal(w!("hellp"));
    let wide_string_c3 = WideView::literal(w!("hellod"));
    assert!(wide_string != wide_string_c1);
    assert!(wide_string != wide_string_c2);
    assert!(wide_string != wide_string_c3);
    assert!(wide_string_c1 != wide_string);
    assert!(wide_string_c2 != wide_string);
    assert!(wide_string_c3 != wide_string);

    let c_null_string = WideView::default();
    let c_empty_string = WideView::literal(&[]);
    assert!(!(null_string != c_null_string));
    assert!(!(null_string != c_empty_string));
    assert!(!(empty_string != c_null_string));
    assert!(!(empty_string != c_empty_string));
    assert!(!(deleted_string != c_null_string));
    assert!(!(deleted_string != c_empty_string));
    assert!(!(c_null_string != null_string));
    assert!(!(c_empty_string != null_string));
    assert!(!(c_null_string != empty_string));
    assert!(!(c_empty_string != empty_string));
    assert!(!(c_null_string != deleted_string));
    assert!(!(c_empty_string != deleted_string));

    let c_string_same1 = WideView::literal(w!("hello"));
    assert!(!(wide_string != c_string_same1));
    assert!(!(c_string_same1 != wide_string));

    let c_string1 = WideView::literal(w!("he"));
    let c_string2 = WideView::literal(w!("hellp"));
    let c_string3 = WideView::literal(w!("hellod"));
    assert!(wide_string != c_string1);
    assert!(wide_string != c_string2);
    assert!(wide_string != c_string3);
    assert!(c_string1 != wide_string);
    assert!(c_string2 != wide_string);
    assert!(c_string3 != wide_string);
}

#[test]
fn EmptyReverseIterator() {
    let empty = Vec::<u32>::new();
    let iter = 0usize;
    assert!(iter == empty.len());
    assert!(!(iter != empty.len()));
    assert!(!(iter < empty.len()));
}

#[test]
fn OneCharReverseIterator() {
    let one_str = (w!("a")).to_vec();
    let mut iter = 0usize;
    assert!(!(iter == one_str.len()));
    assert!(iter != one_str.len());
    assert!(iter < one_str.len());

    let ch = {
        let value = one_str[one_str.len() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!('a' as u32, ch);
    assert!(iter == one_str.len());
    assert!(!(iter != one_str.len()));
    assert!(!(iter < one_str.len()));
}

#[test]
fn MultiCharReverseIterator() {
    let multi_str = (w!("abcd")).to_vec();
    let mut iter = 0usize;
    assert_ne!(iter, multi_str.len());
    assert_eq!(4, multi_str.len() - iter);
    assert_eq!(0, iter - 0usize);

    let mut ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!('d' as u32, ch);
    assert_eq!('c' as u32, multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(3, multi_str.len() - iter);
    assert_eq!(1, iter - 0usize);

    ch = {
        iter += 1;
        multi_str[multi_str.len() - 1 - iter]
    };
    assert_eq!('b' as u32, ch);
    assert_eq!('b' as u32, multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(2, multi_str.len() - iter);
    assert_eq!(2, iter - 0usize);

    ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!('b' as u32, ch);
    assert_eq!('a' as u32, multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(1, multi_str.len() - iter);
    assert_eq!(3, iter - 0usize);

    ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!('a' as u32, ch);
    assert_eq!(iter, multi_str.len());
    assert_eq!(0, multi_str.len() - iter);
    assert_eq!(4, iter - 0usize);

    ch = {
        iter -= 1;
        multi_str[multi_str.len() - 1 - iter]
    };
    assert_eq!('a' as u32, ch);
    assert_eq!('a' as u32, multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(1, multi_str.len() - iter);
    assert_eq!(3, iter - 0usize);

    ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter -= 1;
        value
    };
    assert_eq!('a' as u32, ch);
    assert_eq!('b' as u32, multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(2, multi_str.len() - iter);
    assert_eq!(2, iter - 0usize);

    ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter -= 1;
        value
    };
    assert_eq!('b' as u32, ch);
    assert_eq!('c' as u32, multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(3, multi_str.len() - iter);
    assert_eq!(1, iter - 0usize);

    ch = {
        iter -= 1;
        multi_str[multi_str.len() - 1 - iter]
    };
    assert_eq!('d' as u32, ch);
    assert_eq!('d' as u32, multi_str[multi_str.len() - 1 - iter]);
    assert_eq!(iter, 0usize);
    assert_eq!(4, multi_str.len() - iter);
    assert_eq!(0, iter - 0usize);
}

#[test]
fn NullIterator() {
    let null_str = Vec::<u32>::new();
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in null_str.iter() {
        sum += c as i32; // Avoid unused arg warnings.
        any_present = true;
    }
    assert!(!(any_present));
    assert_eq!((0) as i32, sum);
}

#[test]
fn EmptyIterator() {
    let empty_str: Vec<u32> = (&[]).to_vec();
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in empty_str.iter() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(!(any_present));
    assert_eq!((0) as i32, sum);
}

#[test]
fn OneCharIterator() {
    let one_str = (w!("a")).to_vec();
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in one_str.iter() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(any_present);
    assert_eq!((97i32 as i32) as i32, sum);
}

#[test]
fn MultiCharIterator() {
    let one_str = (w!("abc")).to_vec();
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in one_str.iter() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(any_present);
    assert_eq!((97i32 + 98i32 + 99i32 as i32) as i32, sum);
}

#[test]
fn StdBegin() {
    let one_str = (w!("abc")).to_vec();
    let vec = one_str.iter().copied().collect::<Vec<_>>();
    assert_eq!(3usize, vec.len());
    assert_eq!('a' as u32, vec[0]);
    assert_eq!('b' as u32, vec[1]);
    assert_eq!('c' as u32, vec[2]);
}

#[test]
fn AnyAllNoneOf() {
    let str = (w!("aaaaaaaaaaaaaaaaab")).to_vec();
    assert!(!(str.iter().all(|&c| c == ('a' as u32))));

    assert!(!(!str.iter().any(|&c| c == ('a' as u32))));

    assert!(str.iter().any(|&c| c == ('a' as u32)));

    assert!(str.contains(&('a' as u32)));
    assert!(str.contains(&('b' as u32)));
    assert!(!(str.contains(&('z' as u32))));
}

#[test]
fn Empty() {
    let empty_str = Vec::<u32>::new();
    assert!(empty_str.is_empty());
    assert_eq!(0usize, empty_str.len());
    let cstr = w!("\0");
    assert!(!cstr.is_empty()); // Native non-null empty wide string's terminator.
    assert_eq!(0usize, cstr.iter().position(|&c| c == 0).unwrap());
    let cspan = empty_str.as_slice();
    assert!(cspan.is_empty());
    let cspan_data = (empty_str.capacity() != 0).then(|| cspan.as_ptr());
    assert!(cspan_data.is_none());
}
#[test]
fn InitializerList() {
    let mut many_str = [
        w!("clams").as_slice(),
        w!(" and ").as_slice(),
        w!("oysters").as_slice(),
    ]
    .concat();
    assert_eq!(WideView::literal(w!("clams and oysters")), many_str);
    many_str = [
        w!("fish").as_slice(),
        w!(" and ").as_slice(),
        w!("chips").as_slice(),
        w!(" and ").as_slice(),
        w!("soda").as_slice(),
    ]
    .concat();
    assert_eq!(WideView::literal(w!("fish and chips and soda")), many_str);
}

use super::string_test_support::VecStringOps;

#[test]
fn ConcatInPlace() {
    let mut fred = Vec::<u32>::new();
    fred.Concat(w!("FRED"), 4);
    assert_eq!(WideView::literal(w!("FRED")), fred);

    fred.Concat(w!("DY"), 2);
    assert_eq!(WideView::literal(w!("FREDDY")), fred);

    fred.Delete(3, 3);
    assert_eq!(WideView::literal(w!("FRE")), fred);

    fred.Concat(w!("D"), 1);
    assert_eq!(WideView::literal(w!("FRED")), fred);

    let copy = fred.clone();
    fred.Concat(w!("DY"), 2);
    assert_eq!(WideView::literal(w!("FREDDY")), fred);
    assert_eq!(WideView::literal(w!("FRED")), copy);
}

#[test]
fn Remove() {
    let mut freed = (w!("FREED")).to_vec();
    freed.Remove('E' as u32);
    assert_eq!(WideView::literal(w!("FRD")), freed);
    freed.Remove('F' as u32);
    assert_eq!(WideView::literal(w!("RD")), freed);
    freed.Remove('D' as u32);
    assert_eq!(WideView::literal(w!("R")), freed);
    freed.Remove('X' as u32);
    assert_eq!(WideView::literal(w!("R")), freed);
    freed.Remove('R' as u32);
    assert_eq!(WideView::literal(&[]), freed);

    let mut empty = Vec::<u32>::new();
    empty.Remove('X' as u32);
    assert_eq!(WideView::literal(&[]), empty);
}

#[test]
fn Replace() {
    let mut empty = Vec::<u32>::new();
    empty.Replace(&[], w!("CLAMS"));
    empty.Replace(w!("xx"), w!("CLAMS"));
    assert_eq!(WideView::literal(&[]), empty);

    let mut fred = (w!("FRED")).to_vec();
    fred.Replace(&[], &[]);
    assert_eq!(WideView::literal(w!("FRED")), fred);
    fred.Replace(&[], w!("CLAMS"));
    assert_eq!(WideView::literal(w!("FRED")), fred);
    fred.Replace(w!("FR"), w!("BL"));
    assert_eq!(WideView::literal(w!("BLED")), fred);
    fred.Replace(w!("D"), w!("DDY"));
    assert_eq!(WideView::literal(w!("BLEDDY")), fred);
    fred.Replace(w!("LEDD"), &[]);
    assert_eq!(WideView::literal(w!("BY")), fred);
    fred.Replace(w!("X"), w!("CLAMS"));
    assert_eq!(WideView::literal(w!("BY")), fred);
    fred.Replace(w!("BY"), w!("HI"));
    assert_eq!(WideView::literal(w!("HI")), fred);
    fred.Replace(w!("I"), w!("IHIHI"));
    assert_eq!(WideView::literal(w!("HIHIHI")), fred);
    fred.Replace(w!("HI"), w!("HO"));
    assert_eq!(WideView::literal(w!("HOHOHO")), fred);
    fred.Replace(w!("HO"), &[]);
    assert_eq!(WideView::literal(&[]), fred);

    let mut five_xs = (w!("xxxxx")).to_vec();
    five_xs.Replace(w!("xx"), w!("xxx"));
    assert_eq!(WideView::literal(w!("xxxxxxx")), five_xs);

    let mut five_ys = (w!("yyyyy")).to_vec();
    five_ys.Replace(w!("yy"), w!("y"));
    assert_eq!(WideView::literal(w!("yyy")), five_ys);
}

#[test]
fn Insert() {
    let mut fred = (w!("FRED")).to_vec();
    assert_eq!(5usize, fred.Insert(0, 'S' as u32));
    assert_eq!(WideView::literal(w!("SFRED")), fred);
    assert_eq!(6usize, fred.Insert(1, 'T' as u32));
    assert_eq!(WideView::literal(w!("STFRED")), fred);
    assert_eq!(7usize, fred.Insert(4, 'U' as u32));
    assert_eq!(WideView::literal(w!("STFRUED")), fred);
    assert_eq!(8usize, fred.Insert(7, 'V' as u32));
    assert_eq!(WideView::literal(w!("STFRUEDV")), fred);
    assert_eq!(8usize, fred.Insert(12, 'P' as u32));
    assert_eq!(WideView::literal(w!("STFRUEDV")), fred);
    {
        let mut empty = Vec::<u32>::new();
        assert_eq!(1usize, empty.Insert(0, 'X' as u32));
        assert_eq!(WideView::literal(w!("X")), empty);
    }
    {
        let mut empty = Vec::<u32>::new();
        assert_eq!(0usize, empty.Insert(5, 'X' as u32));
        assert_ne!(WideView::literal(w!("X")), empty);
    }
}

#[test]
fn InsertAtFrontAndInsertAtBack() {
    {
        let mut empty = Vec::<u32>::new();
        assert_eq!(1usize, empty.InsertAtFront('D' as u32));
        assert_eq!(WideView::literal(w!("D")), empty);
        assert_eq!(2usize, empty.InsertAtFront('E' as u32));
        assert_eq!(WideView::literal(w!("ED")), empty);
        assert_eq!(3usize, empty.InsertAtFront('R' as u32));
        assert_eq!(WideView::literal(w!("RED")), empty);
        assert_eq!(4usize, empty.InsertAtFront('F' as u32));
        assert_eq!(WideView::literal(w!("FRED")), empty);
    }
    {
        let mut empty = Vec::<u32>::new();
        assert_eq!(1usize, empty.InsertAtBack('F' as u32));
        assert_eq!(WideView::literal(w!("F")), empty);
        assert_eq!(2usize, empty.InsertAtBack('R' as u32));
        assert_eq!(WideView::literal(w!("FR")), empty);
        assert_eq!(3usize, empty.InsertAtBack('E' as u32));
        assert_eq!(WideView::literal(w!("FRE")), empty);
        assert_eq!(4usize, empty.InsertAtBack('D' as u32));
        assert_eq!(WideView::literal(w!("FRED")), empty);
    }
    {
        let mut empty = Vec::<u32>::new();
        assert_eq!(1usize, empty.InsertAtBack('E' as u32));
        assert_eq!(WideView::literal(w!("E")), empty);
        assert_eq!(2usize, empty.InsertAtFront('R' as u32));
        assert_eq!(WideView::literal(w!("RE")), empty);
        assert_eq!(3usize, empty.InsertAtBack('D' as u32));
        assert_eq!(WideView::literal(w!("RED")), empty);
        assert_eq!(4usize, empty.InsertAtFront('F' as u32));
        assert_eq!(WideView::literal(w!("FRED")), empty);
    }
}

#[test]
fn Delete() {
    let mut fred = (w!("FRED")).to_vec();
    assert_eq!(4usize, fred.Delete(0, 0));
    assert_eq!(WideView::literal(w!("FRED")), fred);
    assert_eq!(2usize, fred.Delete(0, 2));
    assert_eq!(WideView::literal(w!("ED")), fred);
    assert_eq!(1usize, fred.Delete(1, 1));
    assert_eq!(WideView::literal(w!("E")), fred);
    assert_eq!(0usize, fred.Delete(0, 1));
    assert_eq!(WideView::literal(&[]), fred);
    assert_eq!(0usize, fred.Delete(0, 1));
    assert_eq!(WideView::literal(&[]), fred);

    let mut empty = Vec::<u32>::new();
    assert_eq!(0usize, empty.Delete(0, 1));
    assert_eq!(WideView::literal(&[]), empty);
    assert_eq!(0usize, empty.Delete(1, 1));
    assert_eq!(WideView::literal(&[]), empty);
}

#[test]
fn OneArgSubstr() {
    let fred = (w!("FRED")).to_vec();
    assert_eq!(WideView::literal(w!("FRED")), fred.SubstrFrom(0));
    assert_eq!(WideView::literal(w!("RED")), fred.SubstrFrom(1));
    assert_eq!(WideView::literal(w!("ED")), fred.SubstrFrom(2));
    assert_eq!(WideView::literal(w!("D")), fred.SubstrFrom(3));
    assert_eq!(WideView::literal(&[]), fred.SubstrFrom(4));

    let empty = Vec::<u32>::new();
    assert_eq!(WideView::literal(&[]), empty.SubstrFrom(0));
    assert_eq!(WideView::literal(&[]), empty.SubstrFrom(1));
}

#[test]
fn TwoArgSubstr() {
    let fred = (w!("FRED")).to_vec();
    assert_eq!(WideView::literal(&[]), fred.Substr(0, 0));
    assert_eq!(WideView::literal(&[]), fred.Substr(3, 0));
    assert_eq!(WideView::literal(w!("FRED")), fred.Substr(0, 4));
    assert_eq!(WideView::literal(w!("RED")), fred.Substr(1, 3));
    assert_eq!(WideView::literal(w!("ED")), fred.Substr(2, 2));
    assert_eq!(WideView::literal(w!("D")), fred.Substr(3, 1));
    assert_eq!(WideView::literal(w!("F")), fred.Substr(0, 1));
    assert_eq!(WideView::literal(w!("R")), fred.Substr(1, 1));
    assert_eq!(WideView::literal(w!("E")), fred.Substr(2, 1));
    assert_eq!(WideView::literal(w!("D")), fred.Substr(3, 1));
    assert_eq!(WideView::literal(w!("FR")), fred.Substr(0, 2));
    assert_eq!(WideView::literal(w!("FRED")), fred.Substr(0, 4));
    assert_eq!(WideView::literal(&[]), fred.Substr(0, 10));

    assert_eq!(WideView::literal(&[]), fred.Substr(1, 4));
    assert_eq!(WideView::literal(&[]), fred.Substr(4, 1));

    let empty = Vec::<u32>::new();
    assert_eq!(WideView::literal(&[]), empty.Substr(0, 0));
}

#[test]
fn First() {
    let fred = (w!("FRED")).to_vec();
    assert_eq!(WideView::literal(&[]), fred.First(0));
    assert_eq!(WideView::literal(w!("F")), fred.First(1));
    assert_eq!(WideView::literal(w!("FR")), fred.First(2));
    assert_eq!(WideView::literal(w!("FRE")), fred.First(3));
    assert_eq!(WideView::literal(w!("FRED")), fred.First(4));

    assert_eq!(WideView::literal(&[]), fred.First(5));

    let empty = Vec::<u32>::new();
    assert_eq!(WideView::literal(&[]), empty.First(0));
    assert_eq!(WideView::literal(&[]), empty.First(1));
}

#[test]
fn Last() {
    let fred = (w!("FRED")).to_vec();
    assert_eq!(WideView::literal(&[]), fred.Last(0));
    assert_eq!(WideView::literal(w!("D")), fred.Last(1));
    assert_eq!(WideView::literal(w!("ED")), fred.Last(2));
    assert_eq!(WideView::literal(w!("RED")), fred.Last(3));
    assert_eq!(WideView::literal(w!("FRED")), fred.Last(4));

    assert_eq!(WideView::literal(&[]), fred.Last(5));

    let empty = Vec::<u32>::new();
    assert_eq!(WideView::literal(&[]), empty.Last(0));
    assert_eq!(WideView::literal(&[]), empty.Last(1));
}

#[test]
fn Find() {
    let null_string = Vec::<u32>::new();
    assert!(!(null_string.Find(w!("a")).is_some()));
    assert!(!(null_string.Find(w!("\0")).is_some()));

    let empty_string: Vec<u32> = (&[]).to_vec();
    assert!(!(empty_string.Find(w!("a")).is_some()));
    assert!(!(empty_string.Find(w!("\0")).is_some()));

    let single_string = (w!("a")).to_vec();
    let mut result = single_string.Find(w!("a"));
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    assert!(!(single_string.Find(w!("b")).is_some()));
    assert!(!(single_string.Find(w!("\0")).is_some()));

    let longer_string = (w!("abccc")).to_vec();
    result = longer_string.Find(w!("a"));
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    result = longer_string.Find(w!("c"));
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
    result = longer_string.FindFrom(w!("c"), 3);
    assert!(result.is_some());
    assert_eq!(3usize, result.unwrap());
    assert!(!(longer_string.Find(w!("\0")).is_some()));

    result = longer_string.Find(w!("ab"));
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    result = longer_string.Find(w!("ccc"));
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
    result = longer_string.FindFrom(w!("cc"), 3);
    assert!(result.is_some());
    assert_eq!(3usize, result.unwrap());
    assert!(!(longer_string.Find(w!("d")).is_some()));

    let hibyte_string = (w!("ab\u{ff8c}def")).to_vec();
    result = hibyte_string.Find(w!("\u{ff8c}"));
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
}

#[test]
fn ReverseFind() {
    let null_string = Vec::<u32>::new();
    assert!(!(null_string.ReverseFind('a' as u32).is_some()));
    assert!(!(null_string.ReverseFind('\0' as u32).is_some()));

    let empty_string: Vec<u32> = (&[]).to_vec();
    assert!(!(empty_string.ReverseFind('a' as u32).is_some()));
    assert!(!(empty_string.ReverseFind('\0' as u32).is_some()));

    let single_string = (w!("a")).to_vec();
    let mut result = single_string.ReverseFind('a' as u32);
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    assert!(!(single_string.ReverseFind('b' as u32).is_some()));
    assert!(!(single_string.ReverseFind('\0' as u32).is_some()));

    let longer_string = (w!("abccc")).to_vec();
    result = longer_string.ReverseFind('a' as u32);
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    result = longer_string.ReverseFind('c' as u32);
    assert!(result.is_some());
    assert_eq!(4usize, result.unwrap());
    assert!(!(longer_string.ReverseFind('\0' as u32).is_some()));

    let hibyte_string = (w!("ab\u{ff8c}def")).to_vec();
    result = hibyte_string.ReverseFind('\u{ff8c}' as u32);
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
}

#[test]
fn UpperLower() {
    let mut fred = (w!("F-Re.42D")).to_vec();
    fred.MakeLower();
    assert_eq!(WideView::literal(w!("f-re.42d")), fred);
    fred.MakeUpper();
    assert_eq!(WideView::literal(w!("F-RE.42D")), fred);

    let mut empty = Vec::<u32>::new();
    empty.MakeLower();
    assert_eq!(WideView::literal(&[]), empty);
    empty.MakeUpper();
    assert_eq!(WideView::literal(&[]), empty);

    let mut empty_with_buffer = (w!("x")).to_vec();
    empty_with_buffer.Delete(0, 1);

    let mut additional_empty_with_buffer_ref = empty_with_buffer.clone();
    additional_empty_with_buffer_ref.MakeLower();
    assert_eq!(WideView::literal(&[]), additional_empty_with_buffer_ref);

    additional_empty_with_buffer_ref = empty_with_buffer.clone();
    additional_empty_with_buffer_ref.MakeUpper();
    assert_eq!(WideView::literal(&[]), additional_empty_with_buffer_ref);
}

#[test]
fn Trim() {
    let mut fred = (w!("  FRED  ")).to_vec();
    fred.TrimWhitespace();
    assert_eq!(WideView::literal(w!("FRED")), fred);
    fred.Trim(w!("E"));
    assert_eq!(WideView::literal(w!("FRED")), fred);
    fred.Trim(w!("F"));
    assert_eq!(WideView::literal(w!("RED")), fred);
    fred.Trim(w!("ERP"));
    assert_eq!(WideView::literal(w!("D")), fred);

    let mut blank = (w!("   ")).to_vec();
    blank.Trim(w!("ERP"));
    assert_eq!(WideView::literal(w!("   ")), blank);
    blank.Trim(w!("E"));
    assert_eq!(WideView::literal(w!("   ")), blank);
    blank.TrimWhitespace();
    assert_eq!(WideView::literal(&[]), blank);

    let mut empty = Vec::<u32>::new();
    empty.Trim(w!("ERP"));
    assert_eq!(WideView::literal(&[]), empty);
    empty.Trim(w!("E"));
    assert_eq!(WideView::literal(&[]), empty);
    empty.TrimWhitespace();
    assert_eq!(WideView::literal(&[]), empty);

    let mut abc = (w!("  ABCCBA  ")).to_vec();
    abc.Trim(w!("A"));
    assert_eq!(WideView::literal(w!("  ABCCBA  ")), abc);
    abc.Trim(w!(" A"));
    assert_eq!(WideView::literal(w!("BCCB")), abc);
}

#[test]
fn TrimFront() {
    let mut fred = (w!("  FRED  ")).to_vec();
    fred.TrimWhitespaceFront();
    assert_eq!(WideView::literal(w!("FRED  ")), fred);
    fred.TrimFront(w!("E"));
    assert_eq!(WideView::literal(w!("FRED  ")), fred);
    fred.TrimFront(w!("F"));
    assert_eq!(WideView::literal(w!("RED  ")), fred);
    fred.TrimFront(w!("ERP"));
    assert_eq!(WideView::literal(w!("D  ")), fred);

    let mut blank = (w!("   ")).to_vec();
    blank.TrimFront(w!("ERP"));
    assert_eq!(WideView::literal(w!("   ")), blank);
    blank.TrimFront(w!("E"));
    assert_eq!(WideView::literal(w!("   ")), blank);
    blank.TrimWhitespaceFront();
    assert_eq!(WideView::literal(&[]), blank);

    let mut empty = Vec::<u32>::new();
    empty.TrimFront(w!("ERP"));
    assert_eq!(WideView::literal(&[]), empty);
    empty.TrimFront(w!("E"));
    assert_eq!(WideView::literal(&[]), empty);
    empty.TrimWhitespaceFront();
    assert_eq!(WideView::literal(&[]), empty);
}

#[test]
fn TrimBack() {
    let mut fred = (w!("  FRED  ")).to_vec();
    fred.TrimWhitespaceBack();
    assert_eq!(WideView::literal(w!("  FRED")), fred);
    fred.TrimBack(w!("E"));
    assert_eq!(WideView::literal(w!("  FRED")), fred);
    fred.TrimBack(w!("D"));
    assert_eq!(WideView::literal(w!("  FRE")), fred);
    fred.TrimBack(w!("ERP"));
    assert_eq!(WideView::literal(w!("  F")), fred);

    let mut blank = (w!("   ")).to_vec();
    blank.TrimBack(w!("ERP"));
    assert_eq!(WideView::literal(w!("   ")), blank);
    blank.TrimBack(w!("E"));
    assert_eq!(WideView::literal(w!("   ")), blank);
    blank.TrimWhitespaceBack();
    assert_eq!(WideView::literal(&[]), blank);

    let mut empty = Vec::<u32>::new();
    empty.TrimBack(w!("ERP"));
    assert_eq!(WideView::literal(&[]), empty);
    empty.TrimBack(w!("E"));
    assert_eq!(WideView::literal(&[]), empty);
    empty.TrimWhitespaceBack();
    assert_eq!(WideView::literal(&[]), empty);
}

#[test]
fn GetBuffer() {
    let mut str1 = Vec::<u32>::new();
    {
        let buffer = str1.GetBuffer(12);
        // SAFETY: required for test.
        buffer[..6].copy_from_slice(&[99, 108, 97, 109, 115, 0]);
    }
    str1.ReleaseBuffer(str1.GetStringLength());
    assert_eq!(WideView::literal(w!("clams")), str1);

    let mut str2 = (w!("cl")).to_vec();
    {
        let buffer = str2.GetBuffer(12);
        buffer[2..6].copy_from_slice(&[97, 109, 115, 0]);
    }
    str2.ReleaseBuffer(str2.GetStringLength());
    assert_eq!(WideView::literal(w!("clams")), str2);
}

use super::string_test_support::{plus, SharedText};
type SharedWide = SharedText<u32>;

#[test]
fn ElementAccess() {
    let empty = SharedWide::default();
    let empty_span = empty.span();
    let empty_span_with_terminator = empty.span_with_terminator();
    assert_eq!(0usize, empty_span.len());
    assert_eq!(1usize, empty_span_with_terminator.len());
    assert_eq!('\0' as u32, empty_span_with_terminator[0]);

    let abc = SharedWide::new(w!("abc"));
    assert_eq!('a' as u32, abc[0]);
    assert_eq!('b' as u32, abc[1]);
    assert_eq!('c' as u32, abc[2]);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::hint::black_box(abc[4])
    }))
    .is_err());

    let abc_span = abc.span();
    assert_eq!(3usize, abc_span.len());
    assert_eq!(
        0,
        abc_span.iter().copied().cmp(w!("abc").iter().copied()) as i32
    );

    let abc_span_with_terminator = abc.span_with_terminator();
    assert_eq!(4usize, abc_span_with_terminator.len());
    assert_eq!(
        0,
        abc_span_with_terminator
            .iter()
            .copied()
            .cmp(w!("abc\0").iter().copied()) as i32
    );

    let mut mutable_abc = abc.clone();
    assert_eq!(abc.c_str(), mutable_abc.c_str());
    assert_eq!('a' as u32, mutable_abc[0]);
    assert_eq!('b' as u32, mutable_abc[1]);
    assert_eq!('c' as u32, mutable_abc[2]);
    assert_eq!(abc.c_str(), mutable_abc.c_str());
    assert_eq!(WideView::literal(w!("abc")), abc);

    let c_str = abc.c_str();
    mutable_abc.SetAt(0, 'd' as u32);
    assert_eq!(c_str, abc.c_str());
    assert_ne!(c_str, mutable_abc.c_str());
    assert_eq!(WideView::literal(w!("abc")), abc);
    assert_eq!(WideView::literal(w!("dbc")), mutable_abc);

    mutable_abc.SetAt(1, 'e' as u32);
    assert_eq!(WideView::literal(w!("abc")), abc);
    assert_eq!(WideView::literal(w!("dec")), mutable_abc);

    mutable_abc.SetAt(2, 'f' as u32);
    assert_eq!(WideView::literal(w!("abc")), abc);
    assert_eq!(WideView::literal(w!("def")), mutable_abc);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::hint::black_box(mutable_abc.SetAt(3, 'g' as u32))
    }))
    .is_err());
}

#[test]
fn Construct() {
    {
        // Copy-construct.
        let string1 = SharedWide::new(w!("abc"));
        let string2 = string1.clone();
        assert_eq!(WideView::literal(w!("abc")), string1);
        assert_eq!(WideView::literal(w!("abc")), string2);
        assert_eq!(2, string1.ReferenceCountForTesting());
        assert_eq!(2, string2.ReferenceCountForTesting());
    }
    {
        // Move-construct.
        let mut string1 = SharedWide::new(w!("abc"));
        let string2 = std::mem::take(&mut string1);
        assert!(string1.IsEmpty());
        assert_eq!(WideView::literal(w!("abc")), string2);
        assert_eq!(0, string1.ReferenceCountForTesting());
        assert_eq!(1, string2.ReferenceCountForTesting());
    }
}

#[test]
fn Assign() {
    {
        // Copy-assign.
        let mut string1 = SharedWide::default();
        assert_eq!(0, string1.ReferenceCountForTesting());
        {
            let string2 = SharedWide::new(w!("abc"));
            assert_eq!(1, string2.ReferenceCountForTesting());

            string1 = string2.clone();
            assert_eq!(2, string1.ReferenceCountForTesting());
            assert_eq!(2, string2.ReferenceCountForTesting());
        }
        assert_eq!(1, string1.ReferenceCountForTesting());
    }
    {
        // Move-assign.
        let mut string1 = SharedWide::default();
        assert_eq!(0, string1.ReferenceCountForTesting());
        {
            let mut string2 = SharedWide::new(w!("abc"));
            assert_eq!(1, string2.ReferenceCountForTesting());

            string1 = std::mem::take(&mut string2);
            assert_eq!(WideView::literal(w!("abc")), string1);
            assert!(string2.IsEmpty());
            assert_eq!(1, string1.ReferenceCountForTesting());
            assert_eq!(0, string2.ReferenceCountForTesting());
        }
        assert_eq!(1, string1.ReferenceCountForTesting());
    }
    {
        // From wchar_t*.
        let mut string1 = SharedWide::new(w!("abc"));
        assert_eq!(WideView::literal(w!("abc")), string1);
        string1.clear();
        assert!(string1.IsEmpty());
        string1.assign_literal(w!("def"));
        assert_eq!(WideView::literal(w!("def")), string1);
        string1.assign_literal(&[]);
        assert!(string1.IsEmpty());
    }
    {
        // From WideStringView.
        let mut string1 = SharedWide::from_span(WideView::literal(w!("abc")).span());
        assert_eq!(WideView::literal(w!("abc")), string1);
        string1.assign_span(WideView::literal(&[]).span());
        assert!(string1.IsEmpty());
        string1.assign_span(WideView::literal(w!("def")).span());
        assert_eq!(WideView::literal(w!("def")), string1);
    }
}

#[test]
fn OperatorPlus() {
    assert_eq!(
        WideView::literal(w!("I like dogs")),
        plus(w!("I like "), SharedWide::new(w!("dogs")).span())
    );
    assert_eq!(
        WideView::literal(w!("Dogs like me")),
        plus(SharedWide::new(w!("Dogs")).span(), w!(" like me"))
    );
    assert_eq!(
        WideView::literal(w!("Oh no, error number 42")),
        plus(w!("Oh no, error number "), &format_integer(42))
    );

    {
        // Make sure operator+= and Concat() increases string memory allocation
        // geometrically.
        let mut allocations = 0i32;
        let mut str = SharedWide::new(w!("ABCDEFGHIJKLMN"));
        let mut buffer = str.c_str();
        for _i in 0..10000 {
            str.Concat(w!("!"), w!("!").len());
            let new_buffer = str.c_str();
            if new_buffer != buffer {
                buffer = new_buffer;
                allocations += 1;
            }
        }
        assert!(allocations < 25);
        assert!(allocations > 10);
    }
}

#[test]
fn RemoveCopies() {
    let mut freed = SharedWide::new(w!("FREED"));
    let old_buffer = freed.c_str();

    // No change with single reference - no copy.
    freed.Remove('Q' as u32);
    assert_eq!(WideView::literal(w!("FREED")), freed);
    assert_eq!(old_buffer, freed.c_str());

    // Change with single reference - no copy.
    freed.Remove('E' as u32);
    assert_eq!(WideView::literal(w!("FRD")), freed);
    assert_eq!(old_buffer, freed.c_str());

    // No change with multiple references - no copy.
    let shared = freed.clone();
    freed.Remove('Q' as u32);
    assert_eq!(WideView::literal(w!("FRD")), freed);
    assert_eq!(old_buffer, freed.c_str());
    assert_eq!(old_buffer, shared.c_str());

    // Change with multiple references -- must copy.
    freed.Remove('D' as u32);
    assert_eq!(WideView::literal(w!("FR")), freed);
    assert_ne!(old_buffer, freed.c_str());
    assert_eq!(WideView::literal(w!("FRD")), shared);
    assert_eq!(old_buffer, shared.c_str());
}

#[test]
fn TrimFrontCopies() {
    {
        // With a single reference, no copy takes place.
        let mut fred = SharedWide::new(w!("  FRED  "));
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceFront();
        assert_eq!(WideView::literal(w!("FRED  ")), fred);
        assert_eq!(old_buffer, fred.c_str());
    }
    {
        // With multiple references, we must copy.
        let mut fred = SharedWide::new(w!("  FRED  "));
        let other_fred = fred.clone();
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceFront();
        assert_eq!(WideView::literal(w!("FRED  ")), fred);
        assert_eq!(WideView::literal(w!("  FRED  ")), other_fred);
        assert_ne!(old_buffer, fred.c_str());
    }
    {
        // With multiple references, but no modifications, no copy.
        let mut fred = SharedWide::new(w!("FRED"));
        let other_fred = fred.clone();
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceFront();
        assert_eq!(WideView::literal(w!("FRED")), fred);
        assert_eq!(WideView::literal(w!("FRED")), other_fred);
        assert_eq!(old_buffer, fred.c_str());
    }
}

#[test]
fn TrimBackCopies() {
    {
        // With a single reference, no copy takes place.
        let mut fred = SharedWide::new(w!("  FRED  "));
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceBack();
        assert_eq!(WideView::literal(w!("  FRED")), fred);
        assert_eq!(old_buffer, fred.c_str());
    }
    {
        // With multiple references, we must copy.
        let mut fred = SharedWide::new(w!("  FRED  "));
        let other_fred = fred.clone();
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceBack();
        assert_eq!(WideView::literal(w!("  FRED")), fred);
        assert_eq!(WideView::literal(w!("  FRED  ")), other_fred);
        assert_ne!(old_buffer, fred.c_str());
    }
    {
        // With multiple references, but no modifications, no copy.
        let mut fred = SharedWide::new(w!("FRED"));
        let other_fred = fred.clone();
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceBack();
        assert_eq!(WideView::literal(w!("FRED")), fred);
        assert_eq!(WideView::literal(w!("FRED")), other_fred);
        assert_eq!(old_buffer, fred.c_str());
    }
}

#[test]
fn Reserve() {
    {
        let mut str = SharedWide::default();
        str.Reserve(6);
        let old_buffer = str.c_str();
        str.Concat(w!("ABCDEF"), w!("ABCDEF").len());
        assert_eq!(old_buffer, str.c_str());
        str.Concat(
            w!("Blah Blah Blah Blah Blah Blah"),
            w!("Blah Blah Blah Blah Blah Blah").len(),
        );
        assert_ne!(old_buffer, str.c_str());
    }
    {
        let mut str = SharedWide::new(w!("A"));
        str.Reserve(6);
        let old_buffer = str.c_str();
        str.Concat(w!("BCDEF"), w!("BCDEF").len());
        assert_eq!(old_buffer, str.c_str());
        str.Concat(
            w!("Blah Blah Blah Blah Blah Blah"),
            w!("Blah Blah Blah Blah Blah Blah").len(),
        );
        assert_ne!(old_buffer, str.c_str());
    }
}

#[test]
fn ReleaseBuffer() {
    {
        let mut str = SharedWide::default();
        str.Reserve(12);
        str.Concat(w!("clams"), w!("clams").len());
        let old_buffer = str.c_str();
        str.ReleaseBuffer(4);
        assert_eq!(old_buffer, str.c_str());
        assert_eq!(WideView::literal(w!("clam")), str);
    }
    {
        let mut str = SharedWide::new(w!("c"));
        str.Reserve(12);
        str.Concat(w!("lams"), w!("lams").len());
        let old_buffer = str.c_str();
        str.ReleaseBuffer(4);
        assert_eq!(old_buffer, str.c_str());
        assert_eq!(WideView::literal(w!("clam")), str);
    }
    {
        let mut str = SharedWide::default();
        str.Reserve(200);
        str.Concat(w!("clams"), w!("clams").len());
        let old_buffer = str.c_str();
        str.ReleaseBuffer(4);
        assert_ne!(old_buffer, str.c_str());
        assert_eq!(WideView::literal(w!("clam")), str);
    }
    {
        let mut str = SharedWide::new(w!("c"));
        str.Reserve(200);
        str.Concat(w!("lams"), w!("lams").len());
        let old_buffer = str.c_str();
        str.ReleaseBuffer(4);
        assert_ne!(old_buffer, str.c_str());
        assert_eq!(WideView::literal(w!("clam")), str);
    }
}

#[cfg(feature = "js")]
fn assert_actual_printf_rejects(source: &str) {
    let mut context = boa_engine::Context::default();
    crate::fxjs::cjs_util::register_util(&mut context).unwrap();
    assert!(context
        .eval(boa_engine::Source::from_bytes(source))
        .is_err());
}
#[test]
fn FormatWidth() {
    assert_wide_cstr_eq(w!("    1"), &wide(&format!("{:5}", 1)));
    assert_wide_cstr_eq(w!("1"), &wide(&format!("{}", 1)));
    assert_wide_cstr_eq(w!("    1"), &wide(&format!("{:width$}", 1, width = 5)));
    assert_wide_cstr_eq(w!("1"), &wide(&format!("{:<1}", 1)));
    assert_wide_cstr_eq(w!("1"), &wide(&format!("{:0width$}", 1, width = 0)));
    // Native generic FormatV returns an empty string for this excessive field.
    // Its selected actual script caller reports a normal ValueError instead.
    #[cfg(feature = "js")]
    assert_actual_printf_rejects("util.printf('%1048576d', 1)");
}
#[test]
fn FormatPrecision() {
    assert_wide_cstr_eq(w!("1.12"), &wide(&format!("{:.2}", 1.12345)));
    assert_wide_cstr_eq(
        w!("1.123"),
        &wide(&format!("{:.precision$}", 1.12345, precision = 3)),
    );
    assert_wide_cstr_eq(w!("1.123450"), &wide(&format!("{:.6}", 1.12345)));
    assert_wide_cstr_eq(w!("1.123450"), &wide(&format!("{:<1.6}", 1.12345)));
    assert_wide_cstr_eq(
        w!("1.123450"),
        &wide(&format!("{:0width$.6}", 1.12345, width = 0)),
    );
    #[cfg(feature = "js")]
    assert_actual_printf_rejects("util.printf('%.1048576f', 1.2)");
}
#[test]
fn FormatString() {
    assert_wide_cstr_eq(w!(""), &wide(&format!("{}", "")));
    assert_wide_cstr_eq(w!(""), &wide(&format!("{}", String::new())));
    assert_wide_cstr_eq(w!("clams"), &wide(&format!("{}", "clams")));
    assert_wide_cstr_eq(w!("cla"), &wide(&format!("{:.3}", "clams")));
    assert_eq!(
        WideView::literal(w!("\u{043e}\u{043f}")),
        wide("\u{043e}\u{043f}")
    );
    // All three selected non-Apple Cyrillic formatting vectors.
    assert_wide_cstr_eq(w!("\u{043e}\u{043f}"), &wide(&format!("\u{043e}\u{043f}")));
    assert_wide_cstr_eq(
        w!("\u{043e}\u{043f}"),
        &wide(&format!("{}", "\u{043e}\u{043f}")),
    );
    assert_wide_cstr_eq(w!("\u{043e}"), &wide(&format!("{:.1}", "\u{043e}\u{043f}")));
}

#[test]
fn ToUCS2LE() {
    let cases: &[(&str, &[u8])] = &[
        ("", b"\0\0"),
        ("abc", b"a\0b\0c\0\0\0"),
        ("abcdef", b"a\0b\0c\0d\0e\0f\0\0\0"),
        ("abc\0def", b"a\0b\0c\0\0\0"),
        ("\u{aabb}\u{ccdd}", b"\xbb\xaa\xdd\xcc\0\0"),
        ("\u{3132}\u{6162}", b"\x32\x31\x62\x61\0\0"),
        ("🎨", b"\0\0"),
    ];
    for (index, (input, expected)) in cases.iter().enumerate() {
        assert_eq!(
            crate::fxcrt::widestring::to_ucs2le(&wide_literal(input)),
            *expected,
            "for case number {index}"
        );
    }
}

#[test]
fn FXHashCodeWide() {
    use super::bytestring::{hash_code_as_if_w, hash_code_lowered_as_if_w};
    use super::widestring::{hash_code, hash_code_lowered};
    assert_eq!(0u32, hash_code(&wide("")));
    assert_eq!(65u32, hash_code(&wide("A")));
    assert_eq!(97u32, hash_code_lowered(&wide("A")));
    assert_eq!(1313u32 * 65u32 + 66u32, hash_code(&wide("AB")));
    assert_eq!(hash_code_as_if_w(b"AB\xff"), hash_code(&wide("AB\u{ff}")));
    assert_eq!(
        hash_code_lowered_as_if_w(b"AB\xff"),
        hash_code_lowered(&wide("AB\u{ff}"))
    );
}
