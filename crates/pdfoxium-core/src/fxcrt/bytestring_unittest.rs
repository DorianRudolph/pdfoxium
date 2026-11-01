// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of core/fxcrt/bytestring_unittest.cpp over selected bytes.
#![allow(non_snake_case)]
fn format_integer(value: i32) -> Vec<u8> {
    let mut buf = [0u8; 32];
    super::fx_system::itoa(value, &mut buf, 10).to_vec()
}
#[test]
fn FormatInteger() {
    assert_eq!(b"0".as_slice(), format_integer(0));
    assert_eq!(b"123456".as_slice(), format_integer(123456));
    assert_eq!(b"-123456".as_slice(), format_integer(-123456));
    assert_eq!(b"2147483647".as_slice(), format_integer(i32::MAX));
    assert_eq!(b"-2147483648".as_slice(), format_integer(i32::MIN));
}

#[test]
fn FXHashCodeAscii() {
    use super::bytestring::{hash_code, hash_code_lowered};
    assert_eq!(0u32, hash_code(b""));
    assert_eq!(65u32, hash_code(b"A"));
    assert_eq!(97u32, hash_code_lowered(b"A"));
    assert_eq!(31 * 65u32 + 66u32, hash_code(b"AB"));
    assert_eq!(31u32 * 65u32 + 255u32, hash_code(b"A\xff"));
    assert_eq!(31u32 * 97u32 + 255u32, hash_code_lowered(b"A\xff"));
}

use super::string_test_support::View;
type ByteView<'a> = View<'a, u8>;

#[test]
fn ByteStringView_ConstexprCtors() {
    const NULL_STRING: &[u8] = &[];
    const _: () = assert!(NULL_STRING.len() == 0);
    const _: () = assert!(NULL_STRING.is_empty());
    const COPIED_NULL_STRING: &[u8] = NULL_STRING;
    const _: () = assert!(COPIED_NULL_STRING.len() == 0);
    const _: () = assert!(COPIED_NULL_STRING.is_empty());
}

#[test]
fn ByteStringView_Null() {
    let null_string = ByteView::default();
    assert!(!(null_string.has_data()));
    assert_eq!(0usize, null_string.GetLength());
    assert!(null_string.IsEmpty());

    let another_null_string = ByteView::default();
    assert_eq!(null_string, another_null_string);

    let copied_null_string = null_string;
    assert!(!(copied_null_string.has_data()));
    assert_eq!(0usize, copied_null_string.GetLength());
    assert!(copied_null_string.IsEmpty());
    assert_eq!(null_string, copied_null_string);

    let span_null_string = ByteView::from_span(&[]);
    assert!(!(span_null_string.has_data()));
    assert_eq!(0usize, span_null_string.GetLength());
    assert!(span_null_string.IsEmpty());
    assert_eq!(null_string, span_null_string);

    let empty_string = ByteView::literal(&[]); // Pointer to NUL, not NULL pointer.
    assert!(empty_string.has_data());
    assert_eq!(0usize, empty_string.GetLength());
    assert!(empty_string.IsEmpty());
    assert_eq!(null_string, empty_string);

    let mut assigned_null_string = ByteView::literal(b"initially not nullptr");
    std::hint::black_box(assigned_null_string);
    assigned_null_string = null_string;
    assert!(!(assigned_null_string.has_data()));
    assert_eq!(0usize, assigned_null_string.GetLength());
    assert!(assigned_null_string.IsEmpty());
    assert_eq!(null_string, assigned_null_string);

    let mut assigned_nullptr_string = ByteView::literal(b"initially not nullptr");
    std::hint::black_box(assigned_nullptr_string);
    assigned_nullptr_string = ByteView::default();
    assert!(!(assigned_nullptr_string.has_data()));
    assert_eq!(0usize, assigned_nullptr_string.GetLength());
    assert!(assigned_nullptr_string.IsEmpty());
    assert_eq!(null_string, assigned_nullptr_string);

    let mut assigned_span_null_string = ByteView::literal(b"initially not null span");
    std::hint::black_box(assigned_span_null_string);
    assigned_span_null_string = ByteView::from_span(&[]);
    assert!(!(assigned_span_null_string.has_data()));
    assert_eq!(0usize, assigned_span_null_string.GetLength());
    assert!(assigned_span_null_string.IsEmpty());
    assert_eq!(null_string, assigned_span_null_string);

    let non_null_string = ByteView::literal(b"a");
    assert_ne!(null_string, non_null_string);
}

#[test]
fn ByteStringView_NotNull() {
    let string3 = ByteView::literal(b"abc");
    let string6 = ByteView::literal(b"abcdef");
    // SAFETY: known fixed-length string.
    let alternate_string3 = ByteView::explicit(b"abcdef", 3);
    let abcd = b"abcd";
    let span_string4 = ByteView::from_span(&abcd[..4]);
    // SAFETY: known fixed-length string.
    let embedded_nul_string7 = ByteView::explicit(b"abc\0def", 7);
    // SAFETY: known fixed-length string.
    let illegal_string7 = ByteView::explicit(b"abcdef\0", 7);

    assert_eq!(3usize, string3.GetLength());
    assert_eq!(6usize, string6.GetLength());
    assert_eq!(3usize, alternate_string3.GetLength());
    assert_eq!(4usize, span_string4.GetLength());
    assert_eq!(7usize, embedded_nul_string7.GetLength());
    assert_eq!(7usize, illegal_string7.GetLength());

    assert_ne!(string3, string6);
    assert_eq!(string3, alternate_string3);
    assert_ne!(string3, embedded_nul_string7);
    assert_ne!(string3, illegal_string7);
    assert_ne!(string6, alternate_string3);
    assert_ne!(string6, embedded_nul_string7);
    assert_ne!(string6, illegal_string7);
    assert_ne!(alternate_string3, embedded_nul_string7);
    assert_ne!(alternate_string3, illegal_string7);
    assert_ne!(embedded_nul_string7, illegal_string7);

    let copied_string3 = string3;
    let copied_alternate_string3 = alternate_string3;
    let copied_embedded_nul_string7 = embedded_nul_string7;

    assert_eq!(string3, copied_string3);
    assert_eq!(alternate_string3, copied_alternate_string3);
    assert_eq!(embedded_nul_string7, copied_embedded_nul_string7);

    let mut assigned_string3 = ByteView::literal(b"intially something else");
    std::hint::black_box(assigned_string3);
    let mut assigned_alternate_string3 = ByteView::literal(b"initally something else");
    std::hint::black_box(assigned_alternate_string3);
    let mut assigned_ptr_string3 = ByteView::literal(b"initially something else");
    std::hint::black_box(assigned_ptr_string3);
    let mut assigned_embedded_nul_string7 = ByteView::literal(b"initially something else");
    std::hint::black_box(assigned_embedded_nul_string7);

    assigned_string3 = string3;
    assigned_alternate_string3 = alternate_string3;
    assigned_ptr_string3 = ByteView::literal(b"abc");
    assigned_embedded_nul_string7 = embedded_nul_string7;
    assert_eq!(string3, assigned_string3);
    assert_eq!(alternate_string3, assigned_alternate_string3);
    assert_eq!(alternate_string3, assigned_ptr_string3);
    assert_eq!(embedded_nul_string7, assigned_embedded_nul_string7);
}

#[test]
fn ByteStringView_FromChar() {
    let null_string = ByteView::default();
    let lower_a_string = ByteView::literal(b"a");

    // Must have lvalues that outlive the corresponding ByteStringC.
    let nul = b'\0';
    let lower_a = b'a';
    let nul_string_from_char = ByteView::from_ref(&nul);
    let lower_a_string_from_char = ByteView::from_ref(&lower_a);

    // Pointer to nul, not nullptr ptr, hence length 1 ...
    assert_eq!(1usize, nul_string_from_char.GetLength());
    assert_ne!(null_string, nul_string_from_char);

    assert_eq!(1usize, lower_a_string_from_char.GetLength());
    assert_eq!(lower_a_string, lower_a_string_from_char);
    assert_ne!(nul_string_from_char, lower_a_string_from_char);

    let longer_string = ByteView::literal(b"ab");
    assert_ne!(longer_string, lower_a_string_from_char);
}

#[test]
fn ByteStringView_FromVector() {
    let null_vec = Vec::<u8>::new();
    let null_string = ByteView::from_span(&null_vec);
    assert_eq!(0usize, null_string.GetLength());

    let lower_a_vec = vec![b'a'; 10];
    let lower_a_string = ByteView::from_span(&lower_a_vec);
    assert_eq!(10usize, lower_a_string.GetLength());
    assert_eq!(ByteView::literal(b"aaaaaaaaaa"), lower_a_string);

    let mut cleared_vec = Vec::<u8>::new();
    cleared_vec.push(42);
    cleared_vec.pop();
    let cleared_string = ByteView::from_span(&cleared_vec);
    assert_eq!(0usize, cleared_string.GetLength());
    assert!(!(cleared_string.has_data()));
}

#[test]
fn ByteStringView_GetID() {
    let null_string = ByteView::default();
    assert_eq!(0u32, null_string.GetID());

    let empty_string = ByteView::literal(&[]);
    assert_eq!(0u32, empty_string.GetID());

    let short_string = ByteView::literal(b"ab");
    assert_eq!(u32::from_be_bytes([97, 98, 0, 0]), short_string.GetID());

    let longer_string = ByteView::literal(b"abcdef");
    assert_eq!(
        u32::from_be_bytes([b'a', b'b', b'c', b'd']),
        longer_string.GetID()
    );
}

#[test]
fn ByteStringView_Find() {
    let null_string = ByteView::default();
    assert!(!(null_string.Find(b'a').is_some()));
    assert!(!(null_string.Find(b'\0').is_some()));

    let empty_string = ByteView::literal(&[]);
    assert!(!(empty_string.Find(b'a').is_some()));
    assert!(!(empty_string.Find(b'\0').is_some()));

    let single_string = ByteView::literal(b"a");
    let mut result = single_string.Find(b'a');
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    assert!(!(single_string.Find(b'b').is_some()));
    assert!(!(single_string.Find(b'\0').is_some()));

    let longer_string = ByteView::literal(b"abccc");
    result = longer_string.Find(b'a');
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    result = longer_string.Find(b'c');
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
    assert!(!(longer_string.Find(b'd').is_some()));
    assert!(!(longer_string.Find(b'\0').is_some()));

    let hibyte_string = ByteView::literal(b"ab\x8cdef");
    result = hibyte_string.Find(b'\x8c');
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
}

#[test]
fn ByteStringView_OneArgSubstr() {
    let null_string = ByteView::default();
    assert_eq!(null_string, null_string.SubstrFrom(0));
    assert_eq!(null_string, null_string.SubstrFrom(1));

    let empty_string = ByteView::literal(&[]);
    assert_eq!(ByteView::literal(&[]), empty_string.SubstrFrom(0));
    assert_eq!(ByteView::literal(&[]), empty_string.SubstrFrom(1));

    let single_character = ByteView::literal(b"a");
    assert_eq!(single_character, single_character.SubstrFrom(0));
    assert_eq!(ByteView::literal(&[]), single_character.SubstrFrom(1));

    let longer_string = ByteView::literal(b"abcdef");
    assert_eq!(longer_string, longer_string.SubstrFrom(0));
    assert_eq!(ByteView::literal(&[]), longer_string.SubstrFrom(187));

    let trailing_substring = ByteView::literal(b"ef");
    assert_eq!(trailing_substring, longer_string.SubstrFrom(4));
}

#[test]
fn ByteStringView_TwoArgSubstr() {
    let null_string = ByteView::default();
    assert_eq!(null_string, null_string.Substr(0, 1));
    assert_eq!(null_string, null_string.Substr(1, 1));

    let empty_string = ByteView::literal(&[]);
    assert_eq!(ByteView::literal(&[]), empty_string.Substr(0, 1));
    assert_eq!(ByteView::literal(&[]), empty_string.Substr(1, 1));

    let single_character = ByteView::literal(b"a");
    assert_eq!(ByteView::literal(&[]), single_character.Substr(0, 0));
    assert_eq!(single_character, single_character.Substr(0, 1));
    assert_eq!(ByteView::literal(&[]), single_character.Substr(1, 0));
    assert_eq!(ByteView::literal(&[]), single_character.Substr(1, 1));

    let longer_string = ByteView::literal(b"abcdef");
    assert_eq!(longer_string, longer_string.Substr(0, 6));
    assert_eq!(ByteView::literal(&[]), longer_string.Substr(0, 187));

    let leading_substring = ByteView::literal(b"ab");
    assert_eq!(leading_substring, longer_string.Substr(0, 2));

    let middle_substring = ByteView::literal(b"bcde");
    assert_eq!(middle_substring, longer_string.Substr(1, 4));

    let trailing_substring = ByteView::literal(b"ef");
    assert_eq!(trailing_substring, longer_string.Substr(4, 2));
    assert_eq!(ByteView::literal(&[]), longer_string.Substr(4, 3));
}

#[test]
fn ByteStringView_TrimmedRight() {
    let fred = ByteView::literal(b"FRED");
    assert_eq!(ByteView::literal(b"FRED"), fred.TrimmedRight(b'E'));
    assert_eq!(ByteView::literal(b"FRE"), fred.TrimmedRight(b'D'));
    let fredd = ByteView::literal(b"FREDD");
    assert_eq!(ByteView::literal(b"FRE"), fredd.TrimmedRight(b'D'));
}

#[test]
fn ByteStringView_ElementAccess() {
    // ByteStringView includes the NUL terminator for non-empty strings.
    let abc = ByteView::literal(b"abc");
    assert_eq!(b'a', abc[0]);
    assert_eq!(b'b', abc[1]);
    assert_eq!(b'c', abc[2]);
    assert!(std::panic::catch_unwind(|| { std::hint::black_box(abc[3]) }).is_err());
}

#[test]
fn ByteStringView_OperatorLT() {
    let empty = ByteView::default();
    let a = ByteView::literal(b"a");
    let abc = ByteView::literal(b"abc");
    let def = ByteView::literal(b"def");
    let c_null = ByteView::default();
    let c_empty = ByteView::literal(&[]);
    let c_a = ByteView::literal(b"a");
    let c_abc = ByteView::literal(b"abc");
    let c_def = ByteView::literal(b"def");

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
fn ByteStringView_OperatorEQ() {
    let byte_string_c = ByteView::literal(b"hello");
    assert!(byte_string_c == byte_string_c);

    let byte_string_c_same1 = ByteView::literal(b"hello");
    assert!(byte_string_c == byte_string_c_same1);
    assert!(byte_string_c_same1 == byte_string_c);

    let byte_string_c_same2 = byte_string_c;
    assert!(byte_string_c == byte_string_c_same2);
    assert!(byte_string_c_same2 == byte_string_c);

    let byte_string_c1 = ByteView::literal(b"he");
    let byte_string_c2 = ByteView::literal(b"hellp");
    let byte_string_c3 = ByteView::literal(b"hellod");
    assert!(!(byte_string_c == byte_string_c1));
    assert!(!(byte_string_c == byte_string_c2));
    assert!(!(byte_string_c == byte_string_c3));
    assert!(!(byte_string_c1 == byte_string_c));
    assert!(!(byte_string_c2 == byte_string_c));
    assert!(!(byte_string_c3 == byte_string_c));

    let byte_string_same1 = (b"hello").to_vec();
    assert!(byte_string_c == byte_string_same1);
    assert!(byte_string_same1 == byte_string_c);

    let byte_string1 = (b"he").to_vec();
    let byte_string2 = (b"hellp").to_vec();
    let byte_string3 = (b"hellod").to_vec();
    assert!(!(byte_string_c == byte_string1));
    assert!(!(byte_string_c == byte_string2));
    assert!(!(byte_string_c == byte_string3));
    assert!(!(byte_string1 == byte_string_c));
    assert!(!(byte_string2 == byte_string_c));
    assert!(!(byte_string3 == byte_string_c));

    let c_string_same1 = ByteView::literal(b"hello");
    assert!(byte_string_c == c_string_same1);
    assert!(c_string_same1 == byte_string_c);

    let c_string1 = ByteView::literal(b"he");
    let c_string2 = ByteView::literal(b"hellp");
    let c_string3 = ByteView::literal(b"hellod");
    assert!(!(byte_string_c == c_string1));
    assert!(!(byte_string_c == c_string2));
    assert!(!(byte_string_c == c_string3));

    assert!(!(c_string1 == byte_string_c));
    assert!(!(c_string2 == byte_string_c));
    assert!(!(c_string3 == byte_string_c));

    let kHello = b"hello";
    let span5 = &kHello[..5];
    let raw_span = byte_string_c.span();
    assert!(raw_span == span5);
}

#[test]
fn ByteStringView_OperatorNE() {
    let byte_string_c = ByteView::literal(b"hello");
    assert!(!(byte_string_c != byte_string_c));

    let byte_string_c_same1 = ByteView::literal(b"hello");
    assert!(!(byte_string_c != byte_string_c_same1));
    assert!(!(byte_string_c_same1 != byte_string_c));

    let byte_string_c_same2 = byte_string_c;
    assert!(!(byte_string_c != byte_string_c_same2));
    assert!(!(byte_string_c_same2 != byte_string_c));

    let byte_string_c1 = ByteView::literal(b"he");
    let byte_string_c2 = ByteView::literal(b"hellp");
    let byte_string_c3 = ByteView::literal(b"hellod");
    assert!(byte_string_c != byte_string_c1);
    assert!(byte_string_c != byte_string_c2);
    assert!(byte_string_c != byte_string_c3);
    assert!(byte_string_c1 != byte_string_c);
    assert!(byte_string_c2 != byte_string_c);
    assert!(byte_string_c3 != byte_string_c);

    let byte_string_same1 = (b"hello").to_vec();
    assert!(!(byte_string_c != byte_string_same1));
    assert!(!(byte_string_same1 != byte_string_c));

    let byte_string1 = (b"he").to_vec();
    let byte_string2 = (b"hellp").to_vec();
    let byte_string3 = (b"hellod").to_vec();
    assert!(byte_string_c != byte_string1);
    assert!(byte_string_c != byte_string2);
    assert!(byte_string_c != byte_string3);
    assert!(byte_string1 != byte_string_c);
    assert!(byte_string2 != byte_string_c);
    assert!(byte_string3 != byte_string_c);

    let c_string_same1 = ByteView::literal(b"hello");
    assert!(!(byte_string_c != c_string_same1));
    assert!(!(c_string_same1 != byte_string_c));

    let c_string1 = ByteView::literal(b"he");
    let c_string2 = ByteView::literal(b"hellp");
    let c_string3 = ByteView::literal(b"hellod");
    assert!(byte_string_c != c_string1);
    assert!(byte_string_c != c_string2);
    assert!(byte_string_c != c_string3);

    assert!(c_string1 != byte_string_c);
    assert!(c_string2 != byte_string_c);
    assert!(c_string3 != byte_string_c);
}

#[test]
fn ByteStringView_NullIterator() {
    let null_str = ByteView::default();
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
fn ByteStringView_EmptyIterator() {
    let empty_str = ByteView::literal(&[]);
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
fn ByteStringView_OneCharIterator() {
    let one_str = ByteView::literal(b"a");
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in one_str.span() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(any_present);
    assert_eq!((97i32) as i32, sum);
}

#[test]
fn ByteStringView_MultiCharIterator() {
    let one_str = ByteView::literal(b"abc");
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in one_str.span() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(any_present);
    assert_eq!((97i32 + 98i32 + 99i32) as i32, sum);
}

#[test]
fn ByteStringView_EmptyReverseIterator() {
    let empty = ByteView::default();
    let iter = 0usize;
    assert!(iter == empty.GetLength());
    assert!(!(iter != empty.GetLength()));
    assert!(!(iter < empty.GetLength()));
}

#[test]
fn ByteStringView_OneCharReverseIterator() {
    let one_str = ByteView::literal(b"a");
    let mut iter = 0usize;
    assert!(!(iter == one_str.GetLength()));
    assert!(iter != one_str.GetLength());
    assert!(iter < one_str.GetLength());

    let ch = {
        let value = one_str[one_str.GetLength() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!(b'a', ch);
    assert!(iter == one_str.GetLength());
    assert!(!(iter != one_str.GetLength()));
    assert!(!(iter < one_str.GetLength()));
}

#[test]
fn ByteStringView_MultiCharReverseIterator() {
    let multi_str = ByteView::literal(b"abcd");
    let mut iter = 0usize;
    assert!(!(iter == multi_str.GetLength()));

    let mut ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!(b'd', ch);
    assert_eq!(b'c', multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        iter += 1;
        multi_str[multi_str.GetLength() - 1 - iter]
    };
    assert_eq!(b'b', ch);
    assert_eq!(b'b', multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!(b'b', ch);
    assert_eq!(b'a', multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!(b'a', ch);
    assert!(iter == multi_str.GetLength());

    ch = {
        iter -= 1;
        multi_str[multi_str.GetLength() - 1 - iter]
    };
    assert_eq!(b'a', ch);
    assert_eq!(b'a', multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter -= 1;
        value
    };
    assert_eq!(b'a', ch);
    assert_eq!(b'b', multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        let value = multi_str[multi_str.GetLength() - 1 - iter];
        iter -= 1;
        value
    };
    assert_eq!(b'b', ch);
    assert_eq!(b'c', multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(!(iter == multi_str.GetLength()));

    ch = {
        iter -= 1;
        multi_str[multi_str.GetLength() - 1 - iter]
    };
    assert_eq!(b'd', ch);
    assert_eq!(b'd', multi_str[multi_str.GetLength() - 1 - iter]);
    assert!(iter == 0usize);
}

#[test]
fn ByteStringView_AnyAllNoneOf() {
    let str = ByteView::literal(b"aaaaaaaaaaaaaaaaab");
    assert!(!(str.span().iter().all(|&c| c == b'a')));

    assert!(!(!str.span().iter().any(|&c| c == b'a')));

    assert!(str.span().iter().any(|&c| c == b'a'));

    assert!(str.span().contains(&b'a'));
    assert!(str.span().contains(&b'b'));
    assert!(!(str.span().contains(&b'z')));
}

// All 31 file-scope native static assertions; the four constructor assertions
// inside the native GTest case are retained in StringView_ConstexprCtors.
const fn const_view_is_ascii(text: &[u8]) -> bool {
    let mut i = 0;
    while i < text.len() {
        if text[i] > 127 {
            return false;
        }
        i += 1;
    }
    true
}
const fn const_view_eq_ascii(text: &[u8], other: &[u8], no_case: bool) -> bool {
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
const fn const_view_lt(a: &[u8], b: &[u8]) -> bool {
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
    let kConstexprView: &[u8] = b"hello";
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
    let kLiteral: &[u8] = b"world";
    let kConstexprViewFromVar = kLiteral;
    assert!(kConstexprViewFromVar.len() == 5);
    assert!(kConstexprViewFromVar[0] == 119);
    let kConstexprViewWithNul: &[u8] = b"with\0nul";
    assert!(kConstexprViewWithNul.len() == 8);
    assert!(kConstexprViewWithNul[4] == 0);
    assert!(kConstexprViewWithNul[5] == 110);
    let kSubstrView = const_substr(kConstexprView, 1, 3);
    assert!(kSubstrView.len() == 3);
    assert!(kSubstrView[0] == 101);
    assert!(kSubstrView[2] == 108);
    let kEmptyView: &[u8] = &[];
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
    let kViewA: &[u8] = b"abc";
    let kViewB: &[u8] = b"def";
    assert!(const_view_lt(kViewA, kViewB));
    assert!(!const_view_lt(kViewB, kViewA));
    assert!(const_view_is_ascii(kConstexprView));
    let kNonASCII: &[u8] = b"hel\x80lo";
    assert!(!const_view_is_ascii(kNonASCII));
    let kASCIIWithNul: &[u8] = b"hel\0lo";
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
    let mut str = Vec::<u8>::new();
    stream.extend_from_slice(&str);
    assert_eq!(b"", stream.as_slice());

    // Basic case, non-empty string
    str = ByteView::literal(b"def").span().to_vec();
    stream.clear();
    stream.extend_from_slice(b"abc");
    stream.extend_from_slice(&str);
    stream.extend_from_slice(b"ghi");
    assert_eq!(b"abcdefghi", stream.as_slice());

    // Changing the ByteString does not change the stream it was written to.
    str = ByteView::literal(b"123").span().to_vec();
    assert_eq!(b"abcdefghi", stream.as_slice());

    // Writing it again to the stream will use the latest value.
    stream.clear();
    stream.extend_from_slice(b"abc");
    stream.extend_from_slice(&str);
    stream.extend_from_slice(b"ghi");
    assert_eq!(b"abc123ghi", stream.as_slice());

    let stringWithNulls = [b'x', b'y', b'\0', b'z'];

    // Writing a ByteString with nulls and no specified length treats it as
    // a C-style null-terminated string.
    str = ByteView::literal(&stringWithNulls).span().to_vec();
    assert_eq!(2usize, str.len());
    stream.clear();
    stream.extend_from_slice(&str);
    assert_eq!(2usize, stream.len());

    // Writing a ByteString with nulls but specifying its length treats it as
    // a C++-style string.
    // SAFETY: required for testing, manual length calculation based on above.
    str = ByteView::explicit(&stringWithNulls, 4).span().to_vec();
    assert_eq!(4usize, str.len());
    stream.clear();
    stream.extend_from_slice(&str);
    assert_eq!(4usize, stream.len());

    // << operators can be chained.
    let str1 = ByteView::literal(b"abc").span().to_vec();
    let str2 = ByteView::literal(b"def").span().to_vec();
    stream.clear();
    stream.extend_from_slice(&str1);
    stream.extend_from_slice(&str2);
    assert_eq!(b"abcdef", stream.as_slice());
}

#[test]
fn ByteStringView_OStreamOverload() {
    // Basic case, empty string
    {
        let mut stream = Vec::<u8>::new();
        let str = ByteView::default();
        stream.extend_from_slice(str.span());
        assert_eq!(b"", stream.as_slice());
    }

    // Basic case, non-empty string
    {
        let mut stream = Vec::<u8>::new();
        let str = ByteView::literal(b"def");
        stream.extend_from_slice(b"abc");
        stream.extend_from_slice(str.span());
        stream.extend_from_slice(b"ghi");
        assert_eq!(b"abcdefghi", stream.as_slice());
    }

    // Changing the ByteStringView does not change the stream it was written to.
    {
        let mut stream = Vec::<u8>::new();
        let mut str = ByteView::literal(b"abc");
        stream.extend_from_slice(str.span());
        str = ByteView::literal(b"123");
        std::hint::black_box(str);
        assert_eq!(b"abc", stream.as_slice());
    }

    // Writing it again to the stream will use the latest value.
    {
        let mut stream = Vec::<u8>::new();
        let mut str = ByteView::literal(b"abc");
        stream.extend_from_slice(str.span());
        stream.clear();
        str = ByteView::literal(b"123");
        stream.extend_from_slice(str.span());
        assert_eq!(b"123", stream.as_slice());
    }

    // Writing a ByteStringView with nulls and no specified length treats it as
    // a C-style null-terminated string.
    {
        let mut stream = Vec::<u8>::new();
        let stringWithNulls = [b'x', b'y', b'\0', b'z'];
        let mut str = ByteView::literal(&stringWithNulls);
        assert_eq!(2usize, str.GetLength());
        stream.extend_from_slice(str.span());
        assert_eq!(2usize, stream.len());
        str = ByteView::literal(b"");
        std::hint::black_box(str);
    }

    // Writing a ByteStringView with nulls but specifying its length treats it as
    // a C++-style string.
    {
        let mut stream = Vec::<u8>::new();
        let stringWithNulls = [b'x', b'y', b'\0', b'z'];
        // SAFETY: known array above.
        let mut str = ByteView::explicit(&stringWithNulls, 4);
        assert_eq!(4usize, str.GetLength());
        stream.extend_from_slice(str.span());
        assert_eq!(4usize, stream.len());
        str = ByteView::literal(b"");
        std::hint::black_box(str);
    }

    // << operators can be chained.
    {
        let mut stream = Vec::<u8>::new();
        let str1 = ByteView::literal(b"abc");
        let str2 = ByteView::literal(b"def");
        stream.extend_from_slice(str1.span());
        stream.extend_from_slice(str2.span());
        assert_eq!(b"abcdef", stream.as_slice());
    }
}

#[test]
fn OperatorLT() {
    let empty = Vec::<u8>::new();
    let a = (b"a").to_vec();
    let abc = (b"abc").to_vec();
    let def = (b"def").to_vec();
    let v_empty = ByteView::default();
    let v_a = ByteView::literal(b"a");
    let v_abc = ByteView::literal(b"abc");
    let v_def = ByteView::literal(b"def");
    let c_null = ByteView::default();
    let c_empty = ByteView::literal(&[]);
    let c_a = ByteView::literal(b"a");
    let c_abc = ByteView::literal(b"abc");
    let c_def = ByteView::literal(b"def");

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

    assert!(v_empty < a);
    assert!(v_empty < c_a);

    let mut str_set = std::collections::BTreeSet::<Vec<u8>>::new();
    let inserted = str_set.insert((b"hello").to_vec());
    assert!(inserted);
    assert!(str_set.contains(&(b"hello").to_vec()));
    assert!(str_set.contains(ByteView::literal(b"hello").span()));
    assert!(str_set.contains(&(b"hello")[..]));
    assert!(!(str_set.contains(&(b"goodbye").to_vec())));
    assert!(!(str_set.contains(ByteView::literal(b"goodbye").span())));
    assert!(!(str_set.contains(&(b"goodbye")[..])));
}

#[test]
fn OperatorEQ() {
    let null_string = Vec::<u8>::new();
    assert!(null_string == null_string);

    let empty_string: Vec<u8> = (&[]).to_vec();
    assert!(empty_string == empty_string);
    assert!(empty_string == null_string);
    assert!(null_string == empty_string);

    let mut deleted_string = (b"hello").to_vec();
    super::string_test_support::delete(&mut deleted_string, 0, 5);
    assert!(deleted_string == deleted_string);
    assert!(deleted_string == null_string);
    assert!(deleted_string == empty_string);
    assert!(null_string == deleted_string);
    assert!(empty_string == deleted_string);

    let byte_string = (b"hello").to_vec();
    assert!(byte_string == byte_string);
    assert!(!(byte_string == null_string));
    assert!(!(byte_string == empty_string));
    assert!(!(byte_string == deleted_string));
    assert!(!(null_string == byte_string));
    assert!(!(empty_string == byte_string));
    assert!(!(deleted_string == byte_string));

    let byte_string_same1 = (b"hello").to_vec();
    assert!(byte_string == byte_string_same1);
    assert!(byte_string_same1 == byte_string);

    let byte_string_same2 = byte_string.clone();
    assert!(byte_string == byte_string_same2);
    assert!(byte_string_same2 == byte_string);

    let byte_string1 = (b"he").to_vec();
    let byte_string2 = (b"hellp").to_vec();
    let byte_string3 = (b"hellod").to_vec();
    assert!(!(byte_string == byte_string1));
    assert!(!(byte_string == byte_string2));
    assert!(!(byte_string == byte_string3));
    assert!(!(byte_string1 == byte_string));
    assert!(!(byte_string2 == byte_string));
    assert!(!(byte_string3 == byte_string));

    let null_string_c = ByteView::default();
    let empty_string_c = ByteView::literal(&[]);
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

    let byte_string_c_same1 = ByteView::literal(b"hello");
    assert!(byte_string == byte_string_c_same1);
    assert!(byte_string_c_same1 == byte_string);

    let byte_string_c1 = ByteView::literal(b"he");
    let byte_string_c2 = ByteView::literal(b"hellp");
    let byte_string_c3 = ByteView::literal(b"hellod");
    assert!(!(byte_string == byte_string_c1));
    assert!(!(byte_string == byte_string_c2));
    assert!(!(byte_string == byte_string_c3));
    assert!(!(byte_string_c1 == byte_string));
    assert!(!(byte_string_c2 == byte_string));
    assert!(!(byte_string_c3 == byte_string));

    let c_null_string = ByteView::default();
    let c_empty_string = ByteView::literal(&[]);
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

    let c_string_same1 = ByteView::literal(b"hello");
    assert!(byte_string == c_string_same1);
    assert!(c_string_same1 == byte_string);

    let c_string1 = ByteView::literal(b"he");
    let c_string2 = ByteView::literal(b"hellp");
    let c_string3 = ByteView::literal(b"hellod");
    assert!(!(byte_string == c_string1));
    assert!(!(byte_string == c_string2));
    assert!(!(byte_string == c_string3));
    assert!(!(c_string1 == byte_string));
    assert!(!(c_string2 == byte_string));
    assert!(!(c_string3 == byte_string));
}

#[test]
fn OperatorNE() {
    let null_string = Vec::<u8>::new();
    assert!(!(null_string != null_string));

    let empty_string: Vec<u8> = (&[]).to_vec();
    assert!(!(empty_string != empty_string));
    assert!(!(empty_string != null_string));
    assert!(!(null_string != empty_string));

    let mut deleted_string = (b"hello").to_vec();
    super::string_test_support::delete(&mut deleted_string, 0, 5);
    assert!(!(deleted_string != deleted_string));
    assert!(!(deleted_string != null_string));
    assert!(!(deleted_string != empty_string));
    assert!(!(deleted_string != deleted_string));
    assert!(!(null_string != deleted_string));
    assert!(!(empty_string != deleted_string));
    assert!(!(deleted_string != deleted_string));

    let byte_string = (b"hello").to_vec();
    assert!(!(byte_string != byte_string));
    assert!(byte_string != null_string);
    assert!(byte_string != empty_string);
    assert!(byte_string != deleted_string);
    assert!(null_string != byte_string);
    assert!(empty_string != byte_string);
    assert!(deleted_string != byte_string);

    let byte_string_same1 = (b"hello").to_vec();
    assert!(!(byte_string != byte_string_same1));
    assert!(!(byte_string_same1 != byte_string));

    let byte_string_same2 = byte_string.clone();
    assert!(!(byte_string != byte_string_same2));
    assert!(!(byte_string_same2 != byte_string));

    let byte_string1 = (b"he").to_vec();
    let byte_string2 = (b"hellp").to_vec();
    let byte_string3 = (b"hellod").to_vec();
    assert!(byte_string != byte_string1);
    assert!(byte_string != byte_string2);
    assert!(byte_string != byte_string3);
    assert!(byte_string1 != byte_string);
    assert!(byte_string2 != byte_string);
    assert!(byte_string3 != byte_string);

    let null_string_c = ByteView::default();
    let empty_string_c = ByteView::literal(&[]);
    assert!(!(null_string != null_string_c));
    assert!(!(null_string != empty_string_c));
    assert!(!(empty_string != null_string_c));
    assert!(!(empty_string != empty_string_c));
    assert!(!(null_string_c != null_string));
    assert!(!(empty_string_c != null_string));
    assert!(!(null_string_c != empty_string));
    assert!(!(empty_string_c != empty_string));

    let byte_string_c_same1 = ByteView::literal(b"hello");
    assert!(!(byte_string != byte_string_c_same1));
    assert!(!(byte_string_c_same1 != byte_string));

    let byte_string_c1 = ByteView::literal(b"he");
    let byte_string_c2 = ByteView::literal(b"hellp");
    let byte_string_c3 = ByteView::literal(b"hellod");
    assert!(byte_string != byte_string_c1);
    assert!(byte_string != byte_string_c2);
    assert!(byte_string != byte_string_c3);
    assert!(byte_string_c1 != byte_string);
    assert!(byte_string_c2 != byte_string);
    assert!(byte_string_c3 != byte_string);

    let c_null_string = ByteView::default();
    let c_empty_string = ByteView::literal(&[]);
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

    let c_string_same1 = ByteView::literal(b"hello");
    assert!(!(byte_string != c_string_same1));
    assert!(!(c_string_same1 != byte_string));

    let c_string1 = ByteView::literal(b"he");
    let c_string2 = ByteView::literal(b"hellp");
    let c_string3 = ByteView::literal(b"hellod");
    assert!(byte_string != c_string1);
    assert!(byte_string != c_string2);
    assert!(byte_string != c_string3);
    assert!(c_string1 != byte_string);
    assert!(c_string2 != byte_string);
    assert!(c_string3 != byte_string);
}

#[test]
fn EmptyReverseIterator() {
    let empty = Vec::<u8>::new();
    let iter = 0usize;
    assert!(iter == empty.len());
    assert!(!(iter != empty.len()));
    assert!(!(iter < empty.len()));
}

#[test]
fn OneCharReverseIterator() {
    let one_str = (b"a").to_vec();
    let mut iter = 0usize;
    assert!(!(iter == one_str.len()));
    assert!(iter != one_str.len());
    assert!(iter < one_str.len());

    let ch = {
        let value = one_str[one_str.len() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!(b'a', ch);
    assert!(iter == one_str.len());
    assert!(!(iter != one_str.len()));
    assert!(!(iter < one_str.len()));
}

#[test]
fn MultiCharReverseIterator() {
    let multi_str = (b"abcd").to_vec();
    let mut iter = 0usize;
    assert_ne!(iter, multi_str.len());
    assert_eq!(4, multi_str.len() - iter);
    assert_eq!(0, iter - 0usize);

    let mut ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!(b'd', ch);
    assert_eq!(b'c', multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(3, multi_str.len() - iter);
    assert_eq!(1, iter - 0usize);

    ch = {
        iter += 1;
        multi_str[multi_str.len() - 1 - iter]
    };
    assert_eq!(b'b', ch);
    assert_eq!(b'b', multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(2, multi_str.len() - iter);
    assert_eq!(2, iter - 0usize);

    ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!(b'b', ch);
    assert_eq!(b'a', multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(1, multi_str.len() - iter);
    assert_eq!(3, iter - 0usize);

    ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter += 1;
        value
    };
    assert_eq!(b'a', ch);
    assert_eq!(iter, multi_str.len());
    assert_eq!(0, multi_str.len() - iter);
    assert_eq!(4, iter - 0usize);

    ch = {
        iter -= 1;
        multi_str[multi_str.len() - 1 - iter]
    };
    assert_eq!(b'a', ch);
    assert_eq!(b'a', multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(1, multi_str.len() - iter);
    assert_eq!(3, iter - 0usize);

    ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter -= 1;
        value
    };
    assert_eq!(b'a', ch);
    assert_eq!(b'b', multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(2, multi_str.len() - iter);
    assert_eq!(2, iter - 0usize);

    ch = {
        let value = multi_str[multi_str.len() - 1 - iter];
        iter -= 1;
        value
    };
    assert_eq!(b'b', ch);
    assert_eq!(b'c', multi_str[multi_str.len() - 1 - iter]);
    assert_ne!(iter, multi_str.len());
    assert_eq!(3, multi_str.len() - iter);
    assert_eq!(1, iter - 0usize);

    ch = {
        iter -= 1;
        multi_str[multi_str.len() - 1 - iter]
    };
    assert_eq!(b'd', ch);
    assert_eq!(b'd', multi_str[multi_str.len() - 1 - iter]);
    assert_eq!(iter, 0usize);
    assert_eq!(4, multi_str.len() - iter);
    assert_eq!(0, iter - 0usize);
}

#[test]
fn NullIterator() {
    let null_str = Vec::<u8>::new();
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
    let empty_str: Vec<u8> = (&[]).to_vec();
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
    let one_str = (b"a").to_vec();
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in one_str.iter() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(any_present);
    assert_eq!((97i32) as i32, sum);
}

#[test]
fn MultiCharIterator() {
    let one_str = (b"abc").to_vec();
    let mut sum = 0i32;
    let mut any_present = false;
    for &c in one_str.iter() {
        any_present = true;
        sum += c as i32; // Avoid unused arg warnings.
    }
    assert!(any_present);
    assert_eq!((97i32 + 98i32 + 99i32) as i32, sum);
}

#[test]
fn StdBegin() {
    let one_str = (b"abc").to_vec();
    let vec = one_str.iter().copied().collect::<Vec<_>>();
    assert_eq!(3usize, vec.len());
    assert_eq!(b'a', vec[0]);
    assert_eq!(b'b', vec[1]);
    assert_eq!(b'c', vec[2]);
}

#[test]
fn AnyAllNoneOf() {
    let str = (b"aaaaaaaaaaaaaaaaab").to_vec();
    assert!(!(str.iter().all(|&c| c == b'a')));

    assert!(!(!str.iter().any(|&c| c == b'a')));

    assert!(str.iter().any(|&c| c == b'a'));

    assert!(str.contains(&b'a'));
    assert!(str.contains(&b'b'));
    assert!(!(str.contains(&b'z')));
}

#[test]
fn Empty() {
    let empty_str = Vec::<u8>::new();
    assert!(empty_str.is_empty());
    assert_eq!(0usize, empty_str.len());
    let cstr = b"\0";
    assert!(!cstr.is_empty()); // Native non-null empty C string's terminator.
    assert_eq!(0usize, super::bytestring::c_str(cstr).len());
    let rstr = (empty_str.capacity() != 0).then(|| empty_str.as_ptr());
    assert!(rstr.is_none());
    let cspan = empty_str.as_slice();
    assert!(cspan.is_empty());
    let cspan_data = (empty_str.capacity() != 0).then(|| cspan.as_ptr());
    assert!(cspan_data.is_none());
    let rspan = empty_str.as_slice();
    assert!(rspan.is_empty());
    let rspan_data = (empty_str.capacity() != 0).then(|| rspan.as_ptr());
    assert!(rspan_data.is_none());
}
#[test]
fn InitializerList() {
    let mut many_str = [
        b"clams".as_slice(),
        b" and ".as_slice(),
        b"oysters".as_slice(),
    ]
    .concat();
    assert_eq!(ByteView::literal(b"clams and oysters"), many_str);
    many_str = [
        b"fish".as_slice(),
        b" and ".as_slice(),
        b"chips".as_slice(),
        b" and ".as_slice(),
        b"soda".as_slice(),
    ]
    .concat();
    assert_eq!(ByteView::literal(b"fish and chips and soda"), many_str);
}

use super::string_test_support::VecStringOps;

#[test]
fn Concat() {
    let mut fred = Vec::<u8>::new();
    fred.Concat(b"FRED", 4);
    assert_eq!(ByteView::literal(b"FRED"), fred);

    fred.Concat(b"DY", 2);
    assert_eq!(ByteView::literal(b"FREDDY"), fred);

    fred.Delete(3, 3);
    assert_eq!(ByteView::literal(b"FRE"), fred);

    fred.Concat(b"D", 1);
    assert_eq!(ByteView::literal(b"FRED"), fred);

    let copy = fred.clone();
    fred.Concat(b"DY", 2);
    assert_eq!(ByteView::literal(b"FREDDY"), fred);
    assert_eq!(ByteView::literal(b"FRED"), copy);
}

#[test]
fn Remove() {
    let mut freed = (b"FREED").to_vec();
    freed.Remove(b'E');
    assert_eq!(ByteView::literal(b"FRD"), freed);
    freed.Remove(b'F');
    assert_eq!(ByteView::literal(b"RD"), freed);
    freed.Remove(b'D');
    assert_eq!(ByteView::literal(b"R"), freed);
    freed.Remove(b'X');
    assert_eq!(ByteView::literal(b"R"), freed);
    freed.Remove(b'R');
    assert_eq!(ByteView::literal(&[]), freed);

    let mut empty = Vec::<u8>::new();
    empty.Remove(b'X');
    assert_eq!(ByteView::literal(&[]), empty);
}

#[test]
fn Replace() {
    let mut empty = Vec::<u8>::new();
    empty.Replace(&[], b"CLAMS");
    empty.Replace(b"xx", b"CLAMS");
    assert_eq!(ByteView::literal(&[]), empty);

    let mut fred = (b"FRED").to_vec();
    fred.Replace(&[], &[]);
    assert_eq!(ByteView::literal(b"FRED"), fred);
    fred.Replace(&[], b"CLAMS");
    assert_eq!(ByteView::literal(b"FRED"), fred);
    fred.Replace(b"FR", b"BL");
    assert_eq!(ByteView::literal(b"BLED"), fred);
    fred.Replace(b"D", b"DDY");
    assert_eq!(ByteView::literal(b"BLEDDY"), fred);
    fred.Replace(b"LEDD", &[]);
    assert_eq!(ByteView::literal(b"BY"), fred);
    fred.Replace(b"X", b"CLAMS");
    assert_eq!(ByteView::literal(b"BY"), fred);
    fred.Replace(b"BY", b"HI");
    assert_eq!(ByteView::literal(b"HI"), fred);
    fred.Replace(b"I", b"IHIHI");
    assert_eq!(ByteView::literal(b"HIHIHI"), fred);
    fred.Replace(b"HI", b"HO");
    assert_eq!(ByteView::literal(b"HOHOHO"), fred);
    fred.Replace(b"HO", &[]);
    assert_eq!(ByteView::literal(&[]), fred);

    let mut five_xs = (b"xxxxx").to_vec();
    five_xs.Replace(b"xx", b"xxx");
    assert_eq!(ByteView::literal(b"xxxxxxx"), five_xs);

    let mut five_ys = (b"yyyyy").to_vec();
    five_ys.Replace(b"yy", b"y");
    assert_eq!(ByteView::literal(b"yyy"), five_ys);
}

#[test]
fn Insert() {
    let mut fred = (b"FRED").to_vec();
    assert_eq!(5usize, fred.Insert(0, b'S'));
    assert_eq!(ByteView::literal(b"SFRED"), fred);
    assert_eq!(6usize, fred.Insert(1, b'T'));
    assert_eq!(ByteView::literal(b"STFRED"), fred);
    assert_eq!(7usize, fred.Insert(4, b'U'));
    assert_eq!(ByteView::literal(b"STFRUED"), fred);
    assert_eq!(8usize, fred.Insert(7, b'V'));
    assert_eq!(ByteView::literal(b"STFRUEDV"), fred);
    assert_eq!(8usize, fred.Insert(12, b'P'));
    assert_eq!(ByteView::literal(b"STFRUEDV"), fred);
    {
        let mut empty = Vec::<u8>::new();
        assert_eq!(1usize, empty.Insert(0, b'X'));
        assert_eq!(ByteView::literal(b"X"), empty);
    }
    {
        let mut empty = Vec::<u8>::new();
        assert_eq!(0usize, empty.Insert(5, b'X'));
        assert_ne!(ByteView::literal(b"X"), empty);
    }
}

#[test]
fn InsertAtFrontAndInsertAtBack() {
    {
        let mut empty = Vec::<u8>::new();
        assert_eq!(1usize, empty.InsertAtFront(b'D'));
        assert_eq!(ByteView::literal(b"D"), empty);
        assert_eq!(2usize, empty.InsertAtFront(b'E'));
        assert_eq!(ByteView::literal(b"ED"), empty);
        assert_eq!(3usize, empty.InsertAtFront(b'R'));
        assert_eq!(ByteView::literal(b"RED"), empty);
        assert_eq!(4usize, empty.InsertAtFront(b'F'));
        assert_eq!(ByteView::literal(b"FRED"), empty);
    }
    {
        let mut empty = Vec::<u8>::new();
        assert_eq!(1usize, empty.InsertAtBack(b'F'));
        assert_eq!(ByteView::literal(b"F"), empty);
        assert_eq!(2usize, empty.InsertAtBack(b'R'));
        assert_eq!(ByteView::literal(b"FR"), empty);
        assert_eq!(3usize, empty.InsertAtBack(b'E'));
        assert_eq!(ByteView::literal(b"FRE"), empty);
        assert_eq!(4usize, empty.InsertAtBack(b'D'));
        assert_eq!(ByteView::literal(b"FRED"), empty);
    }
    {
        let mut empty = Vec::<u8>::new();
        assert_eq!(1usize, empty.InsertAtBack(b'E'));
        assert_eq!(ByteView::literal(b"E"), empty);
        assert_eq!(2usize, empty.InsertAtFront(b'R'));
        assert_eq!(ByteView::literal(b"RE"), empty);
        assert_eq!(3usize, empty.InsertAtBack(b'D'));
        assert_eq!(ByteView::literal(b"RED"), empty);
        assert_eq!(4usize, empty.InsertAtFront(b'F'));
        assert_eq!(ByteView::literal(b"FRED"), empty);
    }
}

#[test]
fn Delete() {
    let mut fred = (b"FRED").to_vec();
    assert_eq!(4usize, fred.Delete(0, 0));
    assert_eq!(ByteView::literal(b"FRED"), fred);
    assert_eq!(2usize, fred.Delete(0, 2));
    assert_eq!(ByteView::literal(b"ED"), fred);
    assert_eq!(1usize, fred.Delete(1, 1));
    assert_eq!(ByteView::literal(b"E"), fred);
    assert_eq!(0usize, fred.Delete(0, 1));
    assert_eq!(ByteView::literal(&[]), fred);
    assert_eq!(0usize, fred.Delete(0, 1));
    assert_eq!(ByteView::literal(&[]), fred);

    let mut empty = Vec::<u8>::new();
    assert_eq!(0usize, empty.Delete(0, 1));
    assert_eq!(ByteView::literal(&[]), empty);
    assert_eq!(0usize, empty.Delete(1, 1));
    assert_eq!(ByteView::literal(&[]), empty);
}

#[test]
fn OneArgSubstr() {
    let fred = (b"FRED").to_vec();
    assert_eq!(ByteView::literal(b"FRED"), fred.SubstrFrom(0));
    assert_eq!(ByteView::literal(b"RED"), fred.SubstrFrom(1));
    assert_eq!(ByteView::literal(b"ED"), fred.SubstrFrom(2));
    assert_eq!(ByteView::literal(b"D"), fred.SubstrFrom(3));
    assert_eq!(ByteView::literal(&[]), fred.SubstrFrom(4));

    let empty = Vec::<u8>::new();
    assert_eq!(ByteView::literal(&[]), empty.SubstrFrom(0));
    assert_eq!(ByteView::literal(&[]), empty.SubstrFrom(1));
}

#[test]
fn TwoArgSubstr() {
    let fred = (b"FRED").to_vec();
    assert_eq!(ByteView::literal(&[]), fred.Substr(0, 0));
    assert_eq!(ByteView::literal(&[]), fred.Substr(3, 0));
    assert_eq!(ByteView::literal(b"FRED"), fred.Substr(0, 4));
    assert_eq!(ByteView::literal(b"RED"), fred.Substr(1, 3));
    assert_eq!(ByteView::literal(b"ED"), fred.Substr(2, 2));
    assert_eq!(ByteView::literal(b"D"), fred.Substr(3, 1));
    assert_eq!(ByteView::literal(b"F"), fred.Substr(0, 1));
    assert_eq!(ByteView::literal(b"R"), fred.Substr(1, 1));
    assert_eq!(ByteView::literal(b"E"), fred.Substr(2, 1));
    assert_eq!(ByteView::literal(b"D"), fred.Substr(3, 1));
    assert_eq!(ByteView::literal(b"FR"), fred.Substr(0, 2));
    assert_eq!(ByteView::literal(b"FRED"), fred.Substr(0, 4));
    assert_eq!(ByteView::literal(&[]), fred.Substr(0, 10));

    assert_eq!(ByteView::literal(b"RED"), fred.Substr(1, 3));
    assert_eq!(ByteView::literal(&[]), fred.Substr(4, 1));

    let empty = Vec::<u8>::new();
    assert_eq!(ByteView::literal(&[]), empty.Substr(0, 0));
}

#[test]
fn First() {
    let fred = (b"FRED").to_vec();
    assert_eq!(ByteView::literal(&[]), fred.First(0));
    assert_eq!(ByteView::literal(b"F"), fred.First(1));
    assert_eq!(ByteView::literal(b"FR"), fred.First(2));
    assert_eq!(ByteView::literal(b"FRE"), fred.First(3));
    assert_eq!(ByteView::literal(b"FRED"), fred.First(4));

    assert_eq!(ByteView::literal(&[]), fred.First(5));

    let empty = Vec::<u8>::new();
    assert_eq!(ByteView::literal(&[]), empty.First(0));
    assert_eq!(ByteView::literal(&[]), empty.First(1));
}

#[test]
fn Last() {
    let fred = (b"FRED").to_vec();
    assert_eq!(ByteView::literal(&[]), fred.Last(0));
    assert_eq!(ByteView::literal(b"D"), fred.Last(1));
    assert_eq!(ByteView::literal(b"ED"), fred.Last(2));
    assert_eq!(ByteView::literal(b"RED"), fred.Last(3));
    assert_eq!(ByteView::literal(b"FRED"), fred.Last(4));

    assert_eq!(ByteView::literal(&[]), fred.Last(5));

    let empty = Vec::<u8>::new();
    assert_eq!(ByteView::literal(&[]), empty.Last(0));
    assert_eq!(ByteView::literal(&[]), empty.Last(1));
}

#[test]
fn Find() {
    let null_string = Vec::<u8>::new();
    assert!(!(null_string.Find(b"a").is_some()));
    assert!(!(null_string.Find(b"\0").is_some()));

    let empty_string: Vec<u8> = (&[]).to_vec();
    assert!(!(empty_string.Find(b"a").is_some()));
    assert!(!(empty_string.Find(b"\0").is_some()));

    let single_string = (b"a").to_vec();
    let mut result = single_string.Find(b"a");
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    assert!(!(single_string.Find(b"b").is_some()));
    assert!(!(single_string.Find(b"\0").is_some()));

    let longer_string = (b"abccc").to_vec();
    result = longer_string.Find(b"a");
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    result = longer_string.Find(b"c");
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
    result = longer_string.FindFrom(b"c", 3);
    assert!(result.is_some());
    assert_eq!(3usize, result.unwrap());
    assert!(!(longer_string.Find(b"d").is_some()));
    assert!(!(longer_string.Find(b"\0").is_some()));

    result = longer_string.Find(b"ab");
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    result = longer_string.Find(b"ccc");
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
    result = longer_string.FindFrom(b"cc", 3);
    assert!(result.is_some());
    assert_eq!(3usize, result.unwrap());
    assert!(!(longer_string.Find(b"d").is_some()));

    let hibyte_string = (b"ab\x8cdef").to_vec();
    result = hibyte_string.Find(b"\x8c");
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
}

#[test]
fn ReverseFind() {
    let null_string = Vec::<u8>::new();
    assert!(!(null_string.ReverseFind(b'a').is_some()));
    assert!(!(null_string.ReverseFind(b'\0').is_some()));

    let empty_string: Vec<u8> = (&[]).to_vec();
    assert!(!(empty_string.ReverseFind(b'a').is_some()));
    assert!(!(empty_string.ReverseFind(b'\0').is_some()));

    let single_string = (b"a").to_vec();
    let mut result = single_string.ReverseFind(b'a');
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    assert!(!(single_string.ReverseFind(b'b').is_some()));
    assert!(!(single_string.ReverseFind(b'\0').is_some()));

    let longer_string = (b"abccc").to_vec();
    result = longer_string.ReverseFind(b'a');
    assert!(result.is_some());
    assert_eq!(0usize, result.unwrap());
    result = longer_string.ReverseFind(b'c');
    assert!(result.is_some());
    assert_eq!(4usize, result.unwrap());
    assert!(!(longer_string.ReverseFind(b'\0').is_some()));

    let hibyte_string = (b"ab\x8cdef").to_vec();
    result = hibyte_string.ReverseFind(b'\x8c');
    assert!(result.is_some());
    assert_eq!(2usize, result.unwrap());
}

#[test]
fn UpperLower() {
    let mut fred = (b"F-Re.42D").to_vec();
    fred.MakeLower();
    assert_eq!(ByteView::literal(b"f-re.42d"), fred);
    fred.MakeUpper();
    assert_eq!(ByteView::literal(b"F-RE.42D"), fred);

    let mut empty = Vec::<u8>::new();
    empty.MakeLower();
    assert_eq!(ByteView::literal(&[]), empty);
    empty.MakeUpper();
    assert_eq!(ByteView::literal(&[]), empty);

    let mut empty_with_buffer = (b"x").to_vec();
    empty_with_buffer.Delete(0, 1);

    let mut additional_empty_with_buffer_ref = empty_with_buffer.clone();
    additional_empty_with_buffer_ref.MakeLower();
    assert_eq!(ByteView::literal(&[]), additional_empty_with_buffer_ref);

    additional_empty_with_buffer_ref = empty_with_buffer.clone();
    additional_empty_with_buffer_ref.MakeUpper();
    assert_eq!(ByteView::literal(&[]), additional_empty_with_buffer_ref);
}

#[test]
fn Trim() {
    let mut fred = (b"  FRED  ").to_vec();
    fred.TrimWhitespace();
    assert_eq!(ByteView::literal(b"FRED"), fred);
    fred.Trim(b"E");
    assert_eq!(ByteView::literal(b"FRED"), fred);
    fred.Trim(b"F");
    assert_eq!(ByteView::literal(b"RED"), fred);
    fred.Trim(b"ERP");
    assert_eq!(ByteView::literal(b"D"), fred);

    let mut blank = (b"   ").to_vec();
    blank.Trim(b"ERP");
    assert_eq!(ByteView::literal(b"   "), blank);
    blank.Trim(b"E");
    assert_eq!(ByteView::literal(b"   "), blank);
    blank.TrimWhitespace();
    assert_eq!(ByteView::literal(&[]), blank);

    let mut empty = Vec::<u8>::new();
    empty.Trim(b"ERP");
    assert_eq!(ByteView::literal(&[]), empty);
    empty.Trim(b"E");
    assert_eq!(ByteView::literal(&[]), empty);
    empty.TrimWhitespace();
    assert_eq!(ByteView::literal(&[]), empty);

    let mut abc = (b"  ABCCBA  ").to_vec();
    abc.Trim(b"A");
    assert_eq!(ByteView::literal(b"  ABCCBA  "), abc);
    abc.Trim(b" A");
    assert_eq!(ByteView::literal(b"BCCB"), abc);
}

#[test]
fn TrimFront() {
    let mut fred = (b"  FRED  ").to_vec();
    fred.TrimWhitespaceFront();
    assert_eq!(ByteView::literal(b"FRED  "), fred);
    fred.TrimFront(b"E");
    assert_eq!(ByteView::literal(b"FRED  "), fred);
    fred.TrimFront(b"F");
    assert_eq!(ByteView::literal(b"RED  "), fred);
    fred.TrimFront(b"ERP");
    assert_eq!(ByteView::literal(b"D  "), fred);

    let mut blank = (b"   ").to_vec();
    blank.TrimFront(b"ERP");
    assert_eq!(ByteView::literal(b"   "), blank);
    blank.TrimFront(b"E");
    assert_eq!(ByteView::literal(b"   "), blank);
    blank.TrimWhitespaceFront();
    assert_eq!(ByteView::literal(&[]), blank);

    let mut empty = Vec::<u8>::new();
    empty.TrimFront(b"ERP");
    assert_eq!(ByteView::literal(&[]), empty);
    empty.TrimFront(b"E");
    assert_eq!(ByteView::literal(&[]), empty);
    empty.TrimWhitespaceFront();
    assert_eq!(ByteView::literal(&[]), empty);
}

#[test]
fn TrimBack() {
    let mut fred = (b"  FRED  ").to_vec();
    fred.TrimWhitespaceBack();
    assert_eq!(ByteView::literal(b"  FRED"), fred);
    fred.TrimBack(b"E");
    assert_eq!(ByteView::literal(b"  FRED"), fred);
    fred.TrimBack(b"D");
    assert_eq!(ByteView::literal(b"  FRE"), fred);
    fred.TrimBack(b"ERP");
    assert_eq!(ByteView::literal(b"  F"), fred);

    let mut blank = (b"   ").to_vec();
    blank.TrimBack(b"ERP");
    assert_eq!(ByteView::literal(b"   "), blank);
    blank.TrimBack(b"E");
    assert_eq!(ByteView::literal(b"   "), blank);
    blank.TrimWhitespaceBack();
    assert_eq!(ByteView::literal(&[]), blank);

    let mut empty = Vec::<u8>::new();
    empty.TrimBack(b"ERP");
    assert_eq!(ByteView::literal(&[]), empty);
    empty.TrimBack(b"E");
    assert_eq!(ByteView::literal(&[]), empty);
    empty.TrimWhitespaceBack();
    assert_eq!(ByteView::literal(&[]), empty);
}

#[test]
fn GetBuffer() {
    let mut str1 = Vec::<u8>::new();
    {
        let buffer = str1.GetBuffer(12);
        // SAFETY: required for test.
        // NOLINTNEXTLINE(runtime/printf)
        buffer[..6].copy_from_slice(&[99, 108, 97, 109, 115, 0]);
    }
    str1.ReleaseBuffer(str1.GetStringLength());
    assert_eq!(ByteView::literal(b"clams"), str1);

    let mut str2 = (b"cl").to_vec();
    {
        let buffer = str2.GetBuffer(12);
        // SAFETY: required for test.
        // NOLINTNEXTLINE(runtime/printf)
        buffer[2..6].copy_from_slice(&[97, 109, 115, 0]);
    }
    str2.ReleaseBuffer(str2.GetStringLength());
    assert_eq!(ByteView::literal(b"clams"), str2);
}

#[test]
fn EqualNoCase() {
    let str = (b"aaa").to_vec();
    assert!(str.eq_ignore_ascii_case(b"aaa"));
    assert!(str.eq_ignore_ascii_case(b"AAA"));
    assert!(str.eq_ignore_ascii_case(b"aaA"));
    assert!(str.eq_ignore_ascii_case(b"Aaa"));
    assert!(!(str.eq_ignore_ascii_case(b"aab")));
    assert!(!(str.eq_ignore_ascii_case(b"aaaa")));
    assert!(!(str.eq_ignore_ascii_case(b"BBBB")));
    assert!(!(str.eq_ignore_ascii_case(b"a")));
    assert!(!(str.eq_ignore_ascii_case(&[])));
}

use super::string_test_support::{plus, SharedText};
type SharedBytes = SharedText<u8>;

#[test]
fn ElementAccess() {
    let empty = SharedBytes::default();
    let empty_span = empty.span();
    let empty_span_with_terminator = empty.span_with_terminator();
    assert_eq!(0usize, empty_span.len());
    assert_eq!(1usize, empty_span_with_terminator.len());
    assert_eq!(b'\0', empty_span_with_terminator[0]);

    let abc = SharedBytes::new(b"abc");
    assert_eq!(b'a', abc[0]);
    assert_eq!(b'b', abc[1]);
    assert_eq!(b'c', abc[2]);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::hint::black_box(abc[3])
    }))
    .is_err());

    let abc_span = abc.span();
    assert_eq!(3usize, abc_span.len());
    assert_eq!(
        0,
        abc_span.iter().copied().cmp(b"abc".iter().copied()) as i32
    );

    let abc_span_with_terminator = abc.span_with_terminator();
    assert_eq!(4usize, abc_span_with_terminator.len());
    assert_eq!(
        0,
        abc_span_with_terminator
            .iter()
            .copied()
            .cmp(b"abc\0".iter().copied()) as i32
    );

    let abc_raw_span = abc.unsigned_span();
    assert_eq!(3usize, abc_raw_span.len());
    assert_eq!(
        0,
        abc_raw_span.iter().copied().cmp(b"abc".iter().copied()) as i32
    );

    let abc_raw_span_with_terminator = abc.unsigned_span_with_terminator();
    assert_eq!(4usize, abc_raw_span_with_terminator.len());
    assert_eq!(
        0,
        abc_raw_span_with_terminator
            .iter()
            .copied()
            .cmp(b"abc\0".iter().copied()) as i32
    );

    let mut mutable_abc = abc.clone();
    assert_eq!(abc.c_str(), mutable_abc.c_str());
    assert_eq!(b'a', mutable_abc[0]);
    assert_eq!(b'b', mutable_abc[1]);
    assert_eq!(b'c', mutable_abc[2]);
    assert_eq!(abc.c_str(), mutable_abc.c_str());
    assert_eq!(ByteView::literal(b"abc"), abc);

    let c_str = abc.c_str();
    mutable_abc.SetAt(0, b'd');
    assert_eq!(c_str, abc.c_str());
    assert_ne!(c_str, mutable_abc.c_str());
    assert_eq!(ByteView::literal(b"abc"), abc);
    assert_eq!(ByteView::literal(b"dbc"), mutable_abc);

    mutable_abc.SetAt(1, b'e');
    assert_eq!(ByteView::literal(b"abc"), abc);
    assert_eq!(ByteView::literal(b"dec"), mutable_abc);

    mutable_abc.SetAt(2, b'f');
    assert_eq!(ByteView::literal(b"abc"), abc);
    assert_eq!(ByteView::literal(b"def"), mutable_abc);

    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::hint::black_box(mutable_abc.SetAt(3, b'g'))
    }))
    .is_err());
}

#[test]
fn Construct() {
    {
        // Copy-construct.
        let string1 = SharedBytes::new(b"abc");
        let string2 = string1.clone();
        assert_eq!(ByteView::literal(b"abc"), string1);
        assert_eq!(ByteView::literal(b"abc"), string2);
        assert_eq!(2, string1.ReferenceCountForTesting());
        assert_eq!(2, string2.ReferenceCountForTesting());
    }
    {
        // Move-construct.
        let mut string1 = SharedBytes::new(b"abc");
        let string2 = std::mem::take(&mut string1);
        assert!(string1.IsEmpty());
        assert_eq!(ByteView::literal(b"abc"), string2);
        assert_eq!(0, string1.ReferenceCountForTesting());
        assert_eq!(1, string2.ReferenceCountForTesting());
    }
}

#[test]
fn Assign() {
    {
        // Copy-assign.
        let mut string1 = SharedBytes::default();
        assert_eq!(0, string1.ReferenceCountForTesting());
        {
            let string2 = SharedBytes::new(b"abc");
            assert_eq!(1, string2.ReferenceCountForTesting());

            string1 = string2.clone();
            assert_eq!(2, string1.ReferenceCountForTesting());
            assert_eq!(2, string2.ReferenceCountForTesting());
        }
        assert_eq!(1, string1.ReferenceCountForTesting());
    }
    {
        // Move-assign.
        let mut string1 = SharedBytes::default();
        assert_eq!(0, string1.ReferenceCountForTesting());
        {
            let mut string2 = SharedBytes::new(b"abc");
            assert_eq!(1, string2.ReferenceCountForTesting());

            string1 = std::mem::take(&mut string2);
            assert_eq!(ByteView::literal(b"abc"), string1);
            assert!(string2.IsEmpty());
            assert_eq!(1, string1.ReferenceCountForTesting());
            assert_eq!(0, string2.ReferenceCountForTesting());
        }
        assert_eq!(1, string1.ReferenceCountForTesting());
    }
    {
        // From char*.
        let mut string1 = SharedBytes::new(b"abc");
        assert_eq!(ByteView::literal(b"abc"), string1);
        string1.clear();
        assert!(string1.IsEmpty());
        string1.assign_literal(b"def");
        assert_eq!(ByteView::literal(b"def"), string1);
        string1.assign_literal(&[]);
        assert!(string1.IsEmpty());
    }
    {
        // From ByteStringView.
        let mut string1 = SharedBytes::from_span(ByteView::literal(b"abc").span());
        assert_eq!(ByteView::literal(b"abc"), string1);
        string1.assign_span(ByteView::literal(&[]).span());
        assert!(string1.IsEmpty());
        string1.assign_span(ByteView::literal(b"def").span());
        assert_eq!(ByteView::literal(b"def"), string1);
    }
}

#[test]
fn OperatorPlus() {
    assert_eq!(
        ByteView::literal(b"I like dogs"),
        plus(b"I like ", SharedBytes::new(b"dogs").span())
    );
    assert_eq!(
        ByteView::literal(b"Dogs like me"),
        plus(SharedBytes::new(b"Dogs").span(), b" like me")
    );
    assert_eq!(
        ByteView::literal(b"Oh no, error number 42"),
        plus(b"Oh no, error number ", &format_integer(42))
    );

    {
        // Make sure operator+= and Concat() increases string memory allocation
        // geometrically.
        let mut allocations = 0i32;
        let mut str = SharedBytes::new(b"ABCDEFGHIJKLMN");
        let mut buffer = str.c_str();
        for _i in 0..10000 {
            str.Concat(b"!", b"!".len());
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
    let mut freed = SharedBytes::new(b"FREED");
    let old_buffer = freed.c_str();

    // No change with single reference - no copy.
    freed.Remove(b'Q');
    assert_eq!(ByteView::literal(b"FREED"), freed);
    assert_eq!(old_buffer, freed.c_str());

    // Change with single reference - no copy.
    freed.Remove(b'E');
    assert_eq!(ByteView::literal(b"FRD"), freed);
    assert_eq!(old_buffer, freed.c_str());

    // No change with multiple references - no copy.
    let shared = freed.clone();
    freed.Remove(b'Q');
    assert_eq!(ByteView::literal(b"FRD"), freed);
    assert_eq!(old_buffer, freed.c_str());
    assert_eq!(old_buffer, shared.c_str());

    // Change with multiple references -- must copy.
    freed.Remove(b'D');
    assert_eq!(ByteView::literal(b"FR"), freed);
    assert_ne!(old_buffer, freed.c_str());
    assert_eq!(ByteView::literal(b"FRD"), shared);
    assert_eq!(old_buffer, shared.c_str());
}

#[test]
fn TrimFrontCopies() {
    {
        // With a single reference, no copy takes place.
        let mut fred = SharedBytes::new(b"  FRED  ");
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceFront();
        assert_eq!(ByteView::literal(b"FRED  "), fred);
        assert_eq!(old_buffer, fred.c_str());
    }
    {
        // With multiple references, we must copy.
        let mut fred = SharedBytes::new(b"  FRED  ");
        let other_fred = fred.clone();
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceFront();
        assert_eq!(ByteView::literal(b"FRED  "), fred);
        assert_eq!(ByteView::literal(b"  FRED  "), other_fred);
        assert_ne!(old_buffer, fred.c_str());
    }
    {
        // With multiple references, but no modifications, no copy.
        let mut fred = SharedBytes::new(b"FRED");
        let other_fred = fred.clone();
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceFront();
        assert_eq!(ByteView::literal(b"FRED"), fred);
        assert_eq!(ByteView::literal(b"FRED"), other_fred);
        assert_eq!(old_buffer, fred.c_str());
    }
}

#[test]
fn TrimBackCopies() {
    {
        // With a single reference, no copy takes place.
        let mut fred = SharedBytes::new(b"  FRED  ");
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceBack();
        assert_eq!(ByteView::literal(b"  FRED"), fred);
        assert_eq!(old_buffer, fred.c_str());
    }
    {
        // With multiple references, we must copy.
        let mut fred = SharedBytes::new(b"  FRED  ");
        let other_fred = fred.clone();
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceBack();
        assert_eq!(ByteView::literal(b"  FRED"), fred);
        assert_eq!(ByteView::literal(b"  FRED  "), other_fred);
        assert_ne!(old_buffer, fred.c_str());
    }
    {
        // With multiple references, but no modifications, no copy.
        let mut fred = SharedBytes::new(b"FRED");
        let other_fred = fred.clone();
        let old_buffer = fred.c_str();
        fred.TrimWhitespaceBack();
        assert_eq!(ByteView::literal(b"FRED"), fred);
        assert_eq!(ByteView::literal(b"FRED"), other_fred);
        assert_eq!(old_buffer, fred.c_str());
    }
}

#[test]
fn Reserve() {
    {
        let mut str = SharedBytes::default();
        str.Reserve(6);
        let old_buffer = str.c_str();
        str.Concat(b"ABCDEF", b"ABCDEF".len());
        assert_eq!(old_buffer, str.c_str());
        str.Concat(
            b"Blah Blah Blah Blah Blah Blah",
            b"Blah Blah Blah Blah Blah Blah".len(),
        );
        assert_ne!(old_buffer, str.c_str());
    }
    {
        let mut str = SharedBytes::new(b"A");
        str.Reserve(6);
        let old_buffer = str.c_str();
        str.Concat(b"BCDEF", b"BCDEF".len());
        assert_eq!(old_buffer, str.c_str());
        str.Concat(
            b"Blah Blah Blah Blah Blah Blah",
            b"Blah Blah Blah Blah Blah Blah".len(),
        );
        assert_ne!(old_buffer, str.c_str());
    }
}

#[test]
fn ReleaseBuffer() {
    {
        let mut str = SharedBytes::default();
        str.Reserve(12);
        str.Concat(b"clams", b"clams".len());
        let old_buffer = str.c_str();
        str.ReleaseBuffer(4);
        assert_eq!(old_buffer, str.c_str());
        assert_eq!(ByteView::literal(b"clam"), str);
    }
    {
        let mut str = SharedBytes::new(b"c");
        str.Reserve(12);
        str.Concat(b"lams", b"lams".len());
        let old_buffer = str.c_str();
        str.ReleaseBuffer(4);
        assert_eq!(old_buffer, str.c_str());
        assert_eq!(ByteView::literal(b"clam"), str);
    }
    {
        let mut str = SharedBytes::default();
        str.Reserve(200);
        str.Concat(b"clams", b"clams".len());
        let old_buffer = str.c_str();
        str.ReleaseBuffer(4);
        assert_ne!(old_buffer, str.c_str());
        assert_eq!(ByteView::literal(b"clam"), str);
    }
    {
        let mut str = SharedBytes::new(b"c");
        str.Reserve(200);
        str.Concat(b"lams", b"lams".len());
        let old_buffer = str.c_str();
        str.ReleaseBuffer(4);
        assert_ne!(old_buffer, str.c_str());
        assert_eq!(ByteView::literal(b"clam"), str);
    }
}

// The selected byte-string formatting surface uses Rust formatting at its
// actual callers, so exercise that formatter directly rather than a second
// test-only printf interpreter.
#[test]
fn FormatWidth() {
    assert_eq!(b"    1", format!("{:5}", 1).as_bytes());
    assert_eq!(b"1", format!("{}", 1).as_bytes());
    assert_eq!(b"    1", format!("{:width$}", 1, width = 5).as_bytes());
    assert_eq!(b"1", format!("{:<1}", 1).as_bytes());
    assert_eq!(b"1", format!("{:0width$}", 1, width = 0).as_bytes());
}
#[test]
fn FormatPrecision() {
    assert_eq!(b"1.12", format!("{:.2}", 1.12345).as_bytes());
    assert_eq!(
        b"1.123",
        format!("{:.precision$}", 1.12345, precision = 3).as_bytes()
    );
    assert_eq!(b"1.123450", format!("{:.6}", 1.12345).as_bytes());
    assert_eq!(b"1.123450", format!("{:<1.6}", 1.12345).as_bytes());
    assert_eq!(
        b"1.123450",
        format!("{:0width$.6}", 1.12345, width = 0).as_bytes()
    );
}

#[test]
fn FXHashCodeWide() {
    use super::bytestring::{hash_code_as_if_w, hash_code_lowered_as_if_w};
    assert_eq!(0u32, hash_code_as_if_w(b""));
    assert_eq!(65u32, hash_code_as_if_w(b"A"));
    assert_eq!(97u32, hash_code_lowered_as_if_w(b"A"));
    assert_eq!(1313u32 * 65u32 + 66u32, hash_code_as_if_w(b"AB"));
    assert_eq!(1313u32 * 65u32 + 255u32, hash_code_as_if_w(b"A\xff"));
    assert_eq!(
        1313u32 * 97u32 + 255u32,
        hash_code_lowered_as_if_w(b"A\xff")
    );
}
