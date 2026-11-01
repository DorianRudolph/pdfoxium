// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Header search, PDF name encoding, character classification, and object
//! helpers from `core/fpdfapi/parser/fpdf_parser_utility.{h,cpp}`.

use super::cpdf_array::CpdfArray;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_object::get_direct_object;
use super::cpdf_object::ObjectRef;
use super::cpdf_reference::CpdfReference;
use super::cpdf_scalar_object::ObjectType;
use super::cpdf_stream::CpdfStream;
use super::cpdf_stream_acc::CpdfStreamAcc;
use super::cpdf_string::CpdfString;

use crate::fxcrt::fx_stream::SeekableReadStream;
use crate::fxcrt::fx_types::FxFileSize;

#[inline]
pub const fn pdf_char_is_whitespace(c: u8) -> bool {
    matches!(c, 0 | b'\t' | b'\n' | 12 | b'\r' | b' ' | 0x80 | 0xff)
}

#[inline]
pub const fn pdf_char_is_numeric(c: u8) -> bool {
    c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.')
}

#[inline]
pub const fn pdf_char_is_delimiter(c: u8) -> bool {
    matches!(
        c,
        b'%' | b'(' | b')' | b'/' | b'<' | b'>' | b'[' | b']' | b'{' | b'}'
    )
}

#[inline]
pub const fn pdf_char_is_other(c: u8) -> bool {
    !pdf_char_is_whitespace(c) && !pdf_char_is_numeric(c) && !pdf_char_is_delimiter(c)
}

#[inline]
pub const fn pdf_char_is_line_ending(c: u8) -> bool {
    matches!(c, b'\r' | b'\n')
}

/// Returns the offset of `%PDF` within the first 1025 possible positions.
pub fn get_header_offset(file: &dyn SeekableReadStream) -> Option<FxFileSize> {
    let mut buf = [0u8; 4];
    for offset in 0..=1024 {
        if !file.read_block_at_offset(&mut buf, offset) {
            return None;
        }
        if &buf == b"%PDF" {
            return Some(offset);
        }
    }
    None
}

#[inline]
pub(crate) const fn hex_char_to_int(ch: u8) -> u8 {
    match ch {
        b'0'..=b'9' => ch - b'0',
        b'a'..=b'f' => ch - b'a' + 10,
        b'A'..=b'F' => ch - b'A' + 10,
        _ => 0,
    }
}

pub fn pdf_name_decode(orig: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(orig.len());
    let mut i = 0;
    while i < orig.len() {
        if orig[i] == b'#' && i + 2 < orig.len() {
            result.push(hex_char_to_int(orig[i + 1]) * 16 + hex_char_to_int(orig[i + 2]));
            i += 3;
        } else {
            result.push(orig[i]);
            i += 1;
        }
    }
    result
}

pub fn pdf_name_encode(orig: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut result = Vec::with_capacity(orig.len());
    for &ch in orig {
        if ch >= 0x80 || pdf_char_is_whitespace(ch) || ch == b'#' || pdf_char_is_delimiter(ch) {
            result.push(b'#');
            result.push(HEX[(ch >> 4) as usize]);
            result.push(HEX[(ch & 0x0f) as usize]);
        } else {
            result.push(ch);
        }
    }
    result
}

pub fn read_array_elements_to_vector(array: &CpdfArray, count: usize) -> Vec<f32> {
    assert!(array.size() >= count);
    (0..count).map(|index| array.get_float_at(index)).collect()
}

pub fn validate_dict_type(dict: Option<&CpdfDictionary>, expected: &[u8]) -> bool {
    assert!(!expected.is_empty());
    dict.is_some_and(|dict| dict.get_name_for(b"Type") == expected)
}

pub fn validate_dict_all_resources_of_type(dict: Option<&CpdfDictionary>, expected: &[u8]) -> bool {
    let Some(dict) = dict else {
        return false;
    };
    dict.with_locked_entries(|entries| {
        entries.values().all(|value| {
            let Some(direct) = get_direct_object(value) else {
                return false;
            };
            let borrowed = direct.borrow();
            if borrowed.object_type() != ObjectType::Dictionary {
                return false;
            }
            validate_dict_type(borrowed.as_any().downcast_ref::<CpdfDictionary>(), expected)
        })
    })
}

pub fn validate_font_resource_dict(dict: Option<&CpdfDictionary>) -> bool {
    validate_dict_all_resources_of_type(dict, b"Font")
}

pub fn validate_dict_optional_type(dict: Option<&CpdfDictionary>, expected: &[u8]) -> bool {
    assert!(!expected.is_empty());
    dict.is_some_and(|dict| !dict.key_exist(b"Type") || dict.get_name_for(b"Type") == expected)
}

/// Byte-preserving equivalent of PDFium's `operator<<(ostream, CPDF_Object*)`.
pub fn object_to_debug_bytes(object: Option<&ObjectRef>) -> Vec<u8> {
    let mut output = Vec::new();
    write_object_debug(object, &mut output);
    output
}

fn write_object_debug(object: Option<&ObjectRef>, output: &mut Vec<u8>) {
    let Some(object) = object else {
        output.extend_from_slice(b" null");
        return;
    };
    let borrowed = object.borrow();
    match borrowed.object_type() {
        ObjectType::Null => output.extend_from_slice(b" null"),
        ObjectType::Boolean | ObjectType::Number => {
            output.push(b' ');
            output.extend_from_slice(&borrowed.get_string());
        }
        ObjectType::String => {
            let string = borrowed.as_any().downcast_ref::<CpdfString>().unwrap();
            output.extend_from_slice(&string.encode_string());
        }
        ObjectType::Name => {
            output.push(b'/');
            output.extend_from_slice(&pdf_name_encode(&borrowed.get_string()));
        }
        ObjectType::Reference => {
            let reference = borrowed.as_any().downcast_ref::<CpdfReference>().unwrap();
            output.push(b' ');
            output.extend_from_slice(reference.ref_obj_num().to_string().as_bytes());
            output.extend_from_slice(b" 0 R ");
        }
        ObjectType::Array => {
            let array = borrowed.as_any().downcast_ref::<CpdfArray>().unwrap();
            output.push(b'[');
            for index in 0..array.size() {
                let child = array.get_object_at(index).unwrap();
                if child.borrow().is_inline() {
                    write_object_debug(Some(&child), output);
                } else {
                    output.push(b' ');
                    output.extend_from_slice(child.borrow().obj_num().to_string().as_bytes());
                    output.extend_from_slice(b" 0 R");
                }
            }
            output.push(b']');
        }
        ObjectType::Dictionary => {
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            output.extend_from_slice(b"<<");
            dict.with_locked_entries(|entries| {
                for (key, child) in entries {
                    output.push(b'/');
                    output.extend_from_slice(&pdf_name_encode(key));
                    if child.borrow().is_inline() {
                        write_object_debug(Some(child), output);
                    } else {
                        output.push(b' ');
                        output.extend_from_slice(child.borrow().obj_num().to_string().as_bytes());
                        output.extend_from_slice(b" 0 R ");
                    }
                }
            });
            output.extend_from_slice(b">>");
        }
        ObjectType::Stream => {
            let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
            let dict = stream.get_dict();
            write_object_debug(Some(&dict), output);
            output.extend_from_slice(b"stream\r\n");
            let mut access = CpdfStreamAcc::new(stream);
            access.load_all_data_raw();
            output.extend_from_slice(access.get_span());
            output.extend_from_slice(b"\r\nendstream");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        get_header_offset, object_to_debug_bytes, pdf_name_decode, pdf_name_encode,
        validate_dict_optional_type, validate_font_resource_dict,
    };
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_name::CpdfName;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;

    #[test]
    fn name_escapes_follow_pdfium_even_for_invalid_hex() {
        assert_eq!(pdf_name_decode(b"#41#GG#4"), b"A\0#4");
        assert_eq!(
            pdf_name_encode(b"!@#$%^&*()<>[]\xc2"),
            b"!@#23$#25^&*#28#29#3C#3E#5B#5D#C2"
        );
    }

    #[test]
    fn header_scan_stops_on_failed_read() {
        assert_eq!(
            get_header_offset(&CfxReadOnlySpanStream::new(b"xx%PDF-1.7")),
            Some(2)
        );
        assert_eq!(get_header_offset(&CfxReadOnlySpanStream::new(b"%PD")), None);
    }

    #[test]
    fn resource_type_checks_require_name_objects() {
        let mut fonts = CpdfDictionary::new();
        let mut font = CpdfDictionary::new();
        assert!(validate_dict_optional_type(Some(&font), b"Font"));
        font.set_for(
            b"Type",
            Some(retain_object(CpdfString::from_bytes(None, b"Font"))),
        );
        assert!(!validate_dict_optional_type(Some(&font), b"Font"));
        font.set_for(b"Type", Some(retain_object(CpdfName::new(None, b"Font"))));
        fonts.set_for(b"F1", Some(retain_object(font)));
        assert!(validate_font_resource_dict(Some(&fonts)));
        assert!(!validate_font_resource_dict(None));
    }

    #[test]
    fn diagnostic_writer_includes_raw_stream_data() {
        let stream = retain_object(CpdfStream::from_bytes(b"raw"));
        let bytes = object_to_debug_bytes(Some(&stream));
        assert_eq!(bytes, b"<</Length 3>>stream\r\nraw\r\nendstream");
        assert_eq!(object_to_debug_bytes(None), b" null");
    }
}

#[cfg(test)]
#[path = "fpdf_parser_utility_unittest.rs"]
mod upstream_tests;
