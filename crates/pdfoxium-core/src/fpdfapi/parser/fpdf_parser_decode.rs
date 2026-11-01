// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Byte-level functions and filter pipelines from
//! `core/fpdfapi/parser/fpdf_parser_decode.{h,cpp}`.

use super::cpdf_array::CpdfArray;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_object::ObjectRef;
use super::cpdf_scalar_object::ObjectType;
use crate::fxcodec::{brotli, flate};

use super::fpdf_parser_utility::pdf_char_is_line_ending;
use crate::fxcodec::data_and_bytes_consumed::DataAndBytesConsumed;
use crate::fxcodec::image_predictors::INVALID_OFFSET;

const MAX_STREAM_SIZE: u32 = 20 * 1024 * 1024;

/// Native CreateFaxDecoder parameter defaults and oversized Rows fallback.
pub fn create_fax_decoder<'a>(
    data: &'a [u8],
    width: i32,
    height: i32,
    params: Option<&CpdfDictionary>,
) -> Option<crate::fxcodec::fax::FaxScanlineDecoder<'a>> {
    let k = params.map_or(0, |dict| dict.get_integer_for(b"K", 0));
    let end_of_line = params.is_some_and(|dict| dict.get_integer_for(b"EndOfLine", 0) != 0);
    let byte_align = params.is_some_and(|dict| dict.get_integer_for(b"EncodedByteAlign", 0) != 0);
    let black = params.is_some_and(|dict| dict.get_integer_for(b"BlackIs1", 0) != 0);
    let columns = params.map_or(1728, |dict| dict.get_integer_for(b"Columns", 1728));
    let rows = params.map_or(0, |dict| dict.get_integer_for(b"Rows", 0));
    let rows = if rows > u16::MAX as i32 { 0 } else { rows };
    crate::fxcodec::fax::FaxScanlineDecoder::create(
        data,
        width,
        height,
        k,
        end_of_line,
        byte_align,
        black,
        columns,
        rows,
    )
}

/// Native CreateFlateDecoder uses the same checked DecodeParms as bulk filters.
#[allow(clippy::too_many_arguments)]
pub fn create_flate_decoder<'a>(
    data: &'a [u8],
    width: i32,
    height: i32,
    components: i32,
    bpc: i32,
    params: Option<&CpdfDictionary>,
) -> Option<crate::fxcodec::flate::FlateScanlineDecoder<'a>> {
    let params = get_and_check_decode_params(params)?;
    Some(crate::fxcodec::flate::FlateScanlineDecoder::new(
        data,
        width,
        height,
        components,
        bpc,
        params.predictor,
        params.colors,
        params.bits_per_component,
        params.columns,
    ))
}

#[derive(Clone, Copy, Debug)]
pub struct DecodeParams {
    pub predictor: i32,
    pub colors: i32,
    pub bits_per_component: i32,
    pub columns: i32,
    pub early_change: bool,
}

impl Default for DecodeParams {
    fn default() -> Self {
        Self {
            predictor: 0,
            colors: 1,
            bits_per_component: 8,
            columns: 1,
            early_change: true,
        }
    }
}

pub type DecoderArray = Vec<(Vec<u8>, Option<ObjectRef>)>;

pub struct PdfDataDecodeResult {
    pub data: Vec<u8>,
    /// An empty decoded stream is distinct from an untouched source whose
    /// image filter is deferred (or whose only filter is already-applied Crypt).
    pub data_was_decoded: bool,
    pub image_encoding: Vec<u8>,
    pub image_params: Option<ObjectRef>,
}

pub fn validate_decoder_pipeline(decoders: &CpdfArray) -> bool {
    let count = decoders.size();
    for index in 0..count {
        let Some(object) = decoders.get_direct_object_at(index) else {
            return false;
        };
        if object.borrow().object_type() != ObjectType::Name {
            return false;
        }
    }
    if count <= 1 {
        return true;
    }
    const VALID: [&[u8]; 10] = [
        b"FlateDecode",
        b"Fl",
        b"LZWDecode",
        b"LZW",
        b"ASCII85Decode",
        b"A85",
        b"ASCIIHexDecode",
        b"AHx",
        b"RunLengthDecode",
        b"RL",
    ];
    (0..count - 1).all(|index| VALID.contains(&decoders.get_byte_string_at(index).as_slice()))
}

pub fn get_and_check_decode_params(params: Option<&CpdfDictionary>) -> Option<DecodeParams> {
    let Some(params) = params else {
        return Some(DecodeParams::default());
    };
    let result = DecodeParams {
        predictor: params.get_integer_for(b"Predictor", 0),
        early_change: params.get_integer_for(b"EarlyChange", 1) != 0,
        colors: params.get_integer_for(b"Colors", 1),
        bits_per_component: params.get_integer_for(b"BitsPerComponent", 8),
        columns: params.get_integer_for(b"Columns", 1),
    };
    if result.colors < 0 || result.bits_per_component < 0 || result.columns < 0 {
        return None;
    }
    let product = result
        .columns
        .checked_mul(result.colors)?
        .checked_mul(result.bits_per_component)?;
    (product <= i32::MAX - 7).then_some(result)
}

pub fn flate_or_lzw_decode(
    use_lzw: bool,
    src: &[u8],
    params: Option<&CpdfDictionary>,
    estimated_size: u32,
) -> DataAndBytesConsumed {
    let Some(params) = get_and_check_decode_params(params) else {
        return DataAndBytesConsumed::new(Vec::new(), INVALID_OFFSET);
    };
    flate::flate_or_lzw_decode(
        use_lzw,
        src,
        params.early_change,
        params.predictor,
        params.colors,
        params.bits_per_component,
        params.columns,
        estimated_size,
    )
}

pub fn brotli_decode(
    src: &[u8],
    params: Option<&CpdfDictionary>,
    estimated_size: u32,
) -> DataAndBytesConsumed {
    let Some(params) = get_and_check_decode_params(params) else {
        return DataAndBytesConsumed::new(Vec::new(), INVALID_OFFSET);
    };
    brotli::decode(
        src,
        params.predictor,
        params.colors,
        params.bits_per_component,
        params.columns,
        estimated_size,
    )
}

pub fn get_decoder_array(dict: &CpdfDictionary) -> Option<DecoderArray> {
    let Some(filter) = dict.get_direct_object_for(b"Filter") else {
        return Some(Vec::new());
    };
    let filter_borrowed = filter.borrow();
    match filter_borrowed.object_type() {
        ObjectType::Array => {
            let decoders = filter_borrowed.as_any().downcast_ref::<CpdfArray>()?;
            if !validate_decoder_pipeline(decoders) {
                return None;
            }
            let params = dict.get_direct_object_for(b"DecodeParms");
            let params_borrowed = params.as_ref().map(|value| value.borrow());
            let params_array = params_borrowed
                .as_ref()
                .and_then(|value| value.as_any().downcast_ref::<CpdfArray>());
            Some(
                (0..decoders.size())
                    .map(|index| {
                        (
                            decoders.get_byte_string_at(index),
                            params_array.and_then(|array| {
                                array.get_typed_direct_at(index, ObjectType::Dictionary)
                            }),
                        )
                    })
                    .collect(),
            )
        }
        ObjectType::Name => {
            let params = dict.get_direct_object_for(b"DecodeParms");
            let params =
                params.filter(|value| value.borrow().object_type() == ObjectType::Dictionary);
            Some(vec![(filter_borrowed.get_string(), params)])
        }
        _ => None,
    }
}

pub fn pdf_data_decode(
    src: &[u8],
    estimated_size: u32,
    image_acc: bool,
    decoders: &DecoderArray,
) -> Option<PdfDataDecodeResult> {
    let mut current: Option<Vec<u8>> = None;
    for (index, (name, param_object)) in decoders.iter().enumerate() {
        let span = current.as_deref().unwrap_or(src);
        let size = if index + 1 == decoders.len() {
            estimated_size
        } else {
            0
        };
        let param_borrowed = param_object.as_ref().map(|object| object.borrow());
        let params = param_borrowed
            .as_ref()
            .and_then(|object| object.as_any().downcast_ref::<CpdfDictionary>());
        if crate::fxcrt::bytestring::eq_cstr(name, b"Crypt") {
            continue;
        }
        let decoded = match crate::fxcrt::bytestring::c_str(name) {
            b"FlateDecode" | b"Fl" => {
                if image_acc && index + 1 == decoders.len() {
                    return Some(PdfDataDecodeResult {
                        data_was_decoded: current.is_some(),
                        data: current.unwrap_or_default(),
                        image_encoding: b"FlateDecode".to_vec(),
                        image_params: param_object.clone(),
                    });
                }
                flate_or_lzw_decode(false, span, params, size)
            }
            b"LZWDecode" | b"LZW" => flate_or_lzw_decode(true, span, params, size),
            b"ASCII85Decode" | b"A85" => a85_decode(span),
            b"ASCIIHexDecode" | b"AHx" => hex_decode(span),
            b"RunLengthDecode" | b"RL" => {
                if image_acc && index + 1 == decoders.len() {
                    return Some(PdfDataDecodeResult {
                        data_was_decoded: current.is_some(),
                        data: current.unwrap_or_default(),
                        image_encoding: b"RunLengthDecode".to_vec(),
                        image_params: param_object.clone(),
                    });
                }
                run_length_decode(span)
            }
            b"BrotliDecode" if brotli::get_brotli_enabled() => brotli_decode(span, params, size),
            _ => {
                let canonical = match crate::fxcrt::bytestring::c_str(name) {
                    b"DCT" => b"DCTDecode".as_slice(),
                    b"CCF" => b"CCITTFaxDecode".as_slice(),
                    _ => name,
                };
                return Some(PdfDataDecodeResult {
                    data_was_decoded: current.is_some(),
                    data: current.unwrap_or_default(),
                    image_encoding: canonical.to_vec(),
                    image_params: param_object.clone(),
                });
            }
        };
        if decoded.bytes_consumed == INVALID_OFFSET {
            return None;
        }
        current = Some(decoded.data);
    }
    Some(PdfDataDecodeResult {
        data_was_decoded: current.is_some(),
        data: current.unwrap_or_default(),
        image_encoding: Vec::new(),
        image_params: None,
    })
}

// Indexed by 8-bit character code; copied from kPDFDocEncoding.
pub const PDF_DOC_ENCODING: [u16; 256] = [
    0x0000, 0x0001, 0x0002, 0x0003, 0x0004, 0x0005, 0x0006, 0x0007, 0x0008, 0x0009, 0x000a, 0x000b,
    0x000c, 0x000d, 0x000e, 0x000f, 0x0010, 0x0011, 0x0012, 0x0013, 0x0014, 0x0015, 0x0016, 0x0017,
    0x02d8, 0x02c7, 0x02c6, 0x02d9, 0x02dd, 0x02db, 0x02da, 0x02dc, 0x0020, 0x0021, 0x0022, 0x0023,
    0x0024, 0x0025, 0x0026, 0x0027, 0x0028, 0x0029, 0x002a, 0x002b, 0x002c, 0x002d, 0x002e, 0x002f,
    0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037, 0x0038, 0x0039, 0x003a, 0x003b,
    0x003c, 0x003d, 0x003e, 0x003f, 0x0040, 0x0041, 0x0042, 0x0043, 0x0044, 0x0045, 0x0046, 0x0047,
    0x0048, 0x0049, 0x004a, 0x004b, 0x004c, 0x004d, 0x004e, 0x004f, 0x0050, 0x0051, 0x0052, 0x0053,
    0x0054, 0x0055, 0x0056, 0x0057, 0x0058, 0x0059, 0x005a, 0x005b, 0x005c, 0x005d, 0x005e, 0x005f,
    0x0060, 0x0061, 0x0062, 0x0063, 0x0064, 0x0065, 0x0066, 0x0067, 0x0068, 0x0069, 0x006a, 0x006b,
    0x006c, 0x006d, 0x006e, 0x006f, 0x0070, 0x0071, 0x0072, 0x0073, 0x0074, 0x0075, 0x0076, 0x0077,
    0x0078, 0x0079, 0x007a, 0x007b, 0x007c, 0x007d, 0x007e, 0x0000, 0x2022, 0x2020, 0x2021, 0x2026,
    0x2014, 0x2013, 0x0192, 0x2044, 0x2039, 0x203a, 0x2212, 0x2030, 0x201e, 0x201c, 0x201d, 0x2018,
    0x2019, 0x201a, 0x2122, 0xfb01, 0xfb02, 0x0141, 0x0152, 0x0160, 0x0178, 0x017d, 0x0131, 0x0142,
    0x0153, 0x0161, 0x017e, 0x0000, 0x20ac, 0x00a1, 0x00a2, 0x00a3, 0x00a4, 0x00a5, 0x00a6, 0x00a7,
    0x00a8, 0x00a9, 0x00aa, 0x00ab, 0x00ac, 0x0000, 0x00ae, 0x00af, 0x00b0, 0x00b1, 0x00b2, 0x00b3,
    0x00b4, 0x00b5, 0x00b6, 0x00b7, 0x00b8, 0x00b9, 0x00ba, 0x00bb, 0x00bc, 0x00bd, 0x00be, 0x00bf,
    0x00c0, 0x00c1, 0x00c2, 0x00c3, 0x00c4, 0x00c5, 0x00c6, 0x00c7, 0x00c8, 0x00c9, 0x00ca, 0x00cb,
    0x00cc, 0x00cd, 0x00ce, 0x00cf, 0x00d0, 0x00d1, 0x00d2, 0x00d3, 0x00d4, 0x00d5, 0x00d6, 0x00d7,
    0x00d8, 0x00d9, 0x00da, 0x00db, 0x00dc, 0x00dd, 0x00de, 0x00df, 0x00e0, 0x00e1, 0x00e2, 0x00e3,
    0x00e4, 0x00e5, 0x00e6, 0x00e7, 0x00e8, 0x00e9, 0x00ea, 0x00eb, 0x00ec, 0x00ed, 0x00ee, 0x00ef,
    0x00f0, 0x00f1, 0x00f2, 0x00f3, 0x00f4, 0x00f5, 0x00f6, 0x00f7, 0x00f8, 0x00f9, 0x00fa, 0x00fb,
    0x00fc, 0x00fd, 0x00fe, 0x00ff,
];

#[inline]
fn hex_char_to_int(ch: u8) -> u8 {
    match ch {
        b'0'..=b'9' => ch - b'0',
        b'a'..=b'f' => ch - b'a' + 10,
        b'A'..=b'F' => ch - b'A' + 10,
        _ => 0,
    }
}

#[inline]
fn is_hex_digit(ch: u8) -> bool {
    ch.is_ascii_hexdigit()
}

pub fn a85_decode(src: &[u8]) -> DataAndBytesConsumed {
    if src.is_empty() {
        return DataAndBytesConsumed::new(Vec::new(), 0);
    }
    let mut zcount = 0u32;
    let mut pos = 0usize;
    while pos < src.len() {
        let ch = src[pos];
        if ch == b'z' {
            zcount += 1;
        } else if !(b'!'..=b'u').contains(&ch)
            && !pdf_char_is_line_ending(ch)
            && ch != b' '
            && ch != b'\t'
        {
            break;
        }
        pos += 1;
    }
    if pos == 0 {
        return DataAndBytesConsumed::new(Vec::new(), 0);
    }
    let space_for_non_zeroes = ((pos as u32 - zcount) / 5) * 4 + 4;
    let Some(size) = zcount
        .checked_mul(4)
        .and_then(|size| size.checked_add(space_for_non_zeroes))
    else {
        return DataAndBytesConsumed::new(Vec::new(), INVALID_OFFSET);
    };
    let mut dest = vec![0u8; size as usize];
    let mut dest_pos = 0usize;
    let mut state = 0usize;
    let mut res = 0u32;
    pos = 0;
    while pos < src.len() {
        let ch = src[pos];
        pos += 1;
        if pdf_char_is_line_ending(ch) || ch == b' ' || ch == b'\t' {
            continue;
        }
        if ch == b'z' {
            dest[dest_pos..dest_pos + 4].fill(0);
            dest_pos += 4;
            state = 0;
            res = 0;
            continue;
        }
        if !(b'!'..=b'u').contains(&ch) {
            break;
        }
        res = res.wrapping_mul(85).wrapping_add(u32::from(ch - 33));
        if state < 4 {
            state += 1;
            continue;
        }
        dest[dest_pos..dest_pos + 4].copy_from_slice(&res.to_be_bytes());
        dest_pos += 4;
        state = 0;
        res = 0;
    }
    if state != 0 {
        for _ in state..5 {
            res = res.wrapping_mul(85).wrapping_add(84);
        }
        dest[dest_pos..dest_pos + state - 1].copy_from_slice(&res.to_be_bytes()[..state - 1]);
        dest_pos += state - 1;
    }
    if pos < src.len() && src[pos] == b'>' {
        pos += 1;
    }
    dest.truncate(dest_pos);
    DataAndBytesConsumed::new(
        dest,
        u32::try_from(pos).expect("source offset exceeds uint32"),
    )
}

pub fn hex_decode(src: &[u8]) -> DataAndBytesConsumed {
    if src.is_empty() {
        return DataAndBytesConsumed::new(Vec::new(), 0);
    }
    let i = src.iter().position(|&ch| ch == b'>').unwrap_or(src.len());
    let mut dest = vec![0u8; i / 2 + 1];
    let mut dest_pos = 0;
    let mut is_first = true;
    let mut i = 0;
    while i < src.len() {
        let ch = src[i];
        i += 1;
        if pdf_char_is_line_ending(ch) || ch == b' ' || ch == b'\t' {
            continue;
        }
        if ch == b'>' {
            break;
        }
        if !is_hex_digit(ch) {
            continue;
        }
        let digit = hex_char_to_int(ch);
        if is_first {
            dest[dest_pos] = digit * 16;
        } else {
            dest[dest_pos] += digit;
            dest_pos += 1;
        }
        is_first = !is_first;
    }
    if !is_first {
        dest_pos += 1;
    }
    dest.truncate(dest_pos);
    DataAndBytesConsumed::new(
        dest,
        u32::try_from(i).expect("source offset exceeds uint32"),
    )
}

pub fn run_length_decode(src: &[u8]) -> DataAndBytesConsumed {
    let mut dest_size = 0u32;
    let mut i = 0usize;
    while i < src.len() {
        if src[i] == 128 {
            break;
        }
        let old = dest_size;
        if src[i] < 128 {
            dest_size = dest_size.wrapping_add(u32::from(src[i]) + 1);
            if dest_size < old {
                return DataAndBytesConsumed::new(Vec::new(), INVALID_OFFSET);
            }
            i += src[i] as usize + 2;
        } else {
            dest_size = dest_size.wrapping_add(257 - u32::from(src[i]));
            if dest_size < old {
                return DataAndBytesConsumed::new(Vec::new(), INVALID_OFFSET);
            }
            i += 2;
        }
    }
    if dest_size >= MAX_STREAM_SIZE {
        return DataAndBytesConsumed::new(Vec::new(), INVALID_OFFSET);
    }
    let mut dest = vec![0u8; dest_size as usize];
    let mut dest_count = 0usize;
    i = 0;
    while i < src.len() {
        if src[i] == 128 {
            break;
        }
        if src[i] < 128 {
            let requested = src[i] as usize + 1;
            let copy_len = requested.min(src.len() - i - 1);
            dest[dest_count..dest_count + copy_len].copy_from_slice(&src[i + 1..i + 1 + copy_len]);
            dest_count += requested;
            i += requested + 1;
        } else {
            let fill = src.get(i + 1).copied().unwrap_or(0);
            let count = 257 - src[i] as usize;
            dest[dest_count..dest_count + count].fill(fill);
            dest_count += count;
            i += 2;
        }
    }
    let consumed = u32::try_from((i + 1).min(src.len())).expect("source offset exceeds uint32");
    DataAndBytesConsumed::new(dest, consumed)
}

pub fn pdf_encode_string(src: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(src.len() + 2);
    result.push(b'(');
    for &ch in src {
        if ch == b'\n' {
            result.extend_from_slice(b"\\n");
            continue;
        }
        if ch == b'\r' {
            result.extend_from_slice(b"\\r");
            continue;
        }
        if matches!(ch, b')' | b'\\' | b'(') {
            result.push(b'\\');
        }
        result.push(ch);
    }
    result.push(b')');
    result
}

pub fn pdf_hex_encode_string(src: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut result = Vec::with_capacity(src.len() * 2 + 2);
    result.push(b'<');
    for &ch in src {
        result.push(HEX[(ch >> 4) as usize]);
        result.push(HEX[(ch & 0xf) as usize]);
    }
    result.push(b'>');
    result
}

fn strip_language_codes(text: &mut Vec<u32>) {
    let mut dest_pos = 0;
    let mut i = 0;
    while i < text.len() {
        if text[i] == 0x1b {
            i += 1;
            while i < text.len() && text[i] != 0x1b {
                i += 1;
            }
            i += 1;
            continue;
        }
        text[dest_pos] = text[i];
        dest_pos += 1;
        i += 1;
    }
    text.truncate(dest_pos);
}

/// PDFium's `WideString` code units on its 32-bit `wchar_t` desktop targets.
/// A `Vec<u32>` retains unpaired surrogates that Rust `char` cannot represent.
pub fn pdf_decode_text(span: &[u8]) -> Vec<u32> {
    if span.starts_with(&[0xfe, 0xff]) || span.starts_with(&[0xff, 0xfe]) {
        let mut result = crate::fxcrt::fx_string::utf16_decode(&span[2..], span[0] == 0xfe);
        strip_language_codes(&mut result);
        return result;
    }
    if span.starts_with(&[0xef, 0xbb, 0xbf]) {
        let mut result = crate::fxcrt::fx_string::utf8_decode(&span[3..]);
        strip_language_codes(&mut result);
        return result;
    }
    span.iter()
        .map(|&byte| u32::from(PDF_DOC_ENCODING[byte as usize]))
        .collect()
}

pub fn pdf_encode_text(text: &[u32]) -> Vec<u8> {
    let mut result = Vec::with_capacity(text.len());
    for &point in text {
        if let Some(code) = PDF_DOC_ENCODING
            .iter()
            .position(|&mapped| u32::from(mapped) == point)
        {
            result.push(code as u8);
        } else {
            break;
        }
    }
    if result.len() == text.len() {
        return result;
    }
    if text.len() > i32::MAX as usize / 2 - 1 {
        return Vec::new();
    }
    result.clear();
    result.reserve(text.len() * 2 + 2);
    result.extend_from_slice(&[0xfe, 0xff]);
    for unit in crate::fxcrt::fx_string::utf16_encode(text) {
        result.extend_from_slice(&unit.to_be_bytes());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        a85_decode, get_decoder_array, hex_decode, pdf_data_decode, pdf_decode_text,
        pdf_encode_string, pdf_encode_text, pdf_hex_encode_string, run_length_decode,
    };
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_name::CpdfName;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fxcodec::basic::basicmodule::BasicModule;
    use crate::fxcodec::flate;

    #[test]
    fn ascii85_and_hex_stop_at_pdfium_offsets() {
        let result = a85_decode(b"FCfN8~>FCfN8");
        assert_eq!(result.data, b"test");
        assert_eq!(result.bytes_consumed, 7);
        let result = a85_decode(b"12A");
        assert_eq!(result.data, b"2k");
        assert_eq!(result.bytes_consumed, 3);
        let result = hex_decode(b"12tk  \tAc>zzz");
        assert_eq!(result.data, [0x12, 0xac]);
        assert_eq!(result.bytes_consumed, 10);
        let result = hex_decode(b"12A>zzz");
        assert_eq!(result.data, [0x12, 0xa0]);
    }

    #[test]
    fn run_length_decode_handles_truncated_literal_and_encoder_output() {
        let source = b"aaabbbccccdef";
        let encoded = BasicModule::run_length_encode(source);
        let result = run_length_decode(&encoded);
        assert_eq!(result.data, source);
        assert_eq!(result.bytes_consumed as usize, encoded.len());
        let result = run_length_decode(&[2, b'a']);
        assert_eq!(result.data, [b'a', 0, 0]);
        assert_eq!(result.bytes_consumed, 2);
    }

    #[test]
    fn literal_and_hex_string_encoding_escape_exact_bytes() {
        assert_eq!(pdf_encode_string(b"a(b)\\\n\r"), b"(a\\(b\\)\\\\\\n\\r)");
        assert_eq!(pdf_hex_encode_string(&[0, 0x8f, 0xff]), b"<008FFF>");
    }

    #[test]
    fn pdf_text_boms_language_markers_and_unpaired_surrogates() {
        let ascii: Vec<_> = "the quick\tfox".chars().map(u32::from).collect();
        assert_eq!(pdf_decode_text(b"the quick\tfox"), ascii);
        assert_eq!(
            pdf_decode_text(&[0xef, 0xbb, 0xbf, 0x1b, b'j', b'a', 0x1b, b' ', 0xe5, 0x8d, 0xb0]),
            vec![0x20, 0x5370]
        );
        assert_eq!(
            pdf_decode_text(&[0xfe, 0xff, 0xd8, 0x00, 0xd8, 0x3c, 0xdf, 0xa8]),
            vec![0xd800, 0x1f3a8]
        );
        assert_eq!(
            pdf_encode_text(&[0x0330, 0x0331]),
            vec![0xfe, 0xff, 0x03, 0x30, 0x03, 0x31]
        );
        assert_eq!(
            pdf_encode_text(&[0x1f3a8]),
            vec![0xfe, 0xff, 0xd8, 0x3c, 0xdf, 0xa8]
        );
        for byte in 0u8..=255 {
            let encoded = pdf_encode_text(&pdf_decode_text(&[byte]));
            if matches!(byte, 0x7f | 0x9f | 0xad) {
                assert_eq!(encoded, [0]);
            } else {
                assert_eq!(encoded, [byte], "{byte}");
            }
        }
    }

    #[test]
    fn filter_pipeline_validates_order_and_decodes_in_sequence() {
        let mut filters = CpdfArray::new();
        filters.append(retain_object(CpdfName::new(None, b"ASCII85Decode")));
        filters.append(retain_object(CpdfName::new(None, b"FlateDecode")));
        let mut dict = CpdfDictionary::new();
        dict.set_for(b"Filter", Some(retain_object(filters)));
        let decoders = get_decoder_array(&dict).unwrap();
        let encoded = BasicModule::a85_encode(&flate::encode(b"filter chain"));
        let result = pdf_data_decode(&encoded, 0, false, &decoders).unwrap();
        assert_eq!(result.data, b"filter chain");
        let mut bad = CpdfArray::new();
        bad.append(retain_object(CpdfName::new(None, b"DCTDecode")));
        bad.append(retain_object(CpdfName::new(None, b"FlateDecode")));
        dict.set_for(b"Filter", Some(retain_object(bad)));
        assert!(get_decoder_array(&dict).is_none());
    }
}

#[cfg(test)]
#[path = "fpdf_parser_decode_unittest.rs"]
mod upstream_tests;
