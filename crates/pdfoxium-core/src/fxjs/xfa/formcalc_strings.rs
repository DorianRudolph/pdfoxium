// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! String functions and codec/English wording helpers from
//! `fxjs/xfa/cfxjse_formcalc_context.{h,cpp}`. UTF-8 byte offsets stay byte offsets;
//! wide/codepoint operations retain their separate source conversion paths.
use super::formcalc_values::{
    exception, get_simple_value, param_count, value_is_null, value_to_float, value_to_integer,
    value_to_utf8_string,
};
use crate::fxcrt::fx_string::{utf8_decode, utf8_encode};
use boa_engine::{native_function::NativeFunctionPointer, Context, JsResult, JsString, JsValue};
#[path = "formcalc_string_tables.rs"]
mod tables;
use tables::{HTML_DECODE, HTML_ENCODE};
const MAX_CHAR_COUNT: usize = 15_654_908;
pub const METHODS: &[(&str, NativeFunctionPointer)] = &[
    ("At", at),
    ("Concat", concat),
    ("Decode", decode),
    ("Encode", encode),
    ("Left", left),
    ("Len", len),
    ("Lower", lower),
    ("Ltrim", ltrim),
    ("Replace", replace),
    ("Right", right),
    ("Rtrim", rtrim),
    ("Space", space),
    ("Str", str_number),
    ("Stuff", stuff),
    ("Substr", substr),
    ("Upper", upper),
    ("WordNum", word_num),
];
fn bytes(text: &[u8]) -> JsValue {
    // NewStringHelper(ByteStringView) interprets the bytes as UTF-8, including
    // replacement when a source byte slice cuts through a multibyte character.
    JsString::from(String::from_utf8_lossy(text).as_ref()).into()
}
fn wide(text: &[u32]) -> JsValue {
    bytes(&utf8_encode(text))
}
fn arity(args: &[JsValue], min: usize, max: usize, name: &str) -> JsResult<()> {
    if args.len() < min || args.len() > max {
        Err(param_count(name))
    } else {
        Ok(())
    }
}
fn simple(args: &[JsValue], count: usize, context: &mut Context) -> JsResult<Vec<JsValue>> {
    (0..count)
        .map(|index| get_simple_value(args, index, context))
        .collect()
}
fn any_null(values: &[JsValue], context: &mut Context) -> JsResult<bool> {
    for value in values {
        if value_is_null(value, context)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn at(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arity(args, 2, 2, "At")?;
    let values = simple(args, 2, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let needle = value_to_utf8_string(&values[1], context)?;
    if needle.is_empty() {
        return Ok(1.into());
    }
    let text = value_to_utf8_string(&values[0], context)?;
    Ok(text
        .windows(needle.len())
        .position(|part| part == needle)
        .map_or(0., |i| (i + 1) as f64)
        .into())
}
fn concat(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arity(args, 1, usize::MAX, "Concat")?;
    let mut result = Vec::new();
    let mut all_null = true;
    for index in 0..args.len() {
        let value = get_simple_value(args, index, context)?;
        if value_is_null(&value, context)? {
            continue;
        }
        all_null = false;
        result.extend(value_to_utf8_string(&value, context)?);
    }
    Ok(if all_null {
        JsValue::null()
    } else {
        bytes(&result)
    })
}
fn codec(args: &[JsValue], context: &mut Context, encoding: bool) -> JsResult<JsValue> {
    arity(args, 1, 2, if encoding { "Encode" } else { "Decode" })?;
    let values = simple(args, args.len(), context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let text = value_to_utf8_string(&values[0], context)?;
    let kind = if values.len() == 2 {
        value_to_utf8_string(&values[1], context)?
    } else {
        Vec::new()
    };
    let result = if encoding {
        encode_text(&text, &kind)
    } else {
        decode_text(&utf8_decode(&text), &kind)
    };
    Ok(wide(&result))
}
fn decode(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    codec(args, context, false)
}
fn encode(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    codec(args, context, true)
}
fn edge(args: &[JsValue], context: &mut Context, from_right: bool) -> JsResult<JsValue> {
    arity(args, 2, 2, if from_right { "Right" } else { "Left" })?;
    let values = simple(args, 2, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let text = value_to_utf8_string(&values[0], context)?;
    let count = (value_to_integer(&values[1], context)?.max(0) as usize).min(text.len());
    Ok(bytes(if from_right {
        &text[text.len() - count..]
    } else {
        &text[..count]
    }))
}
fn left(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    edge(args, context, false)
}
fn right(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    edge(args, context, true)
}
fn len(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arity(args, 1, 1, "Len")?;
    let value = get_simple_value(args, 0, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    Ok((value_to_utf8_string(&value, context)?.len() as f64).into())
}
fn change_case(args: &[JsValue], context: &mut Context, to_upper: bool) -> JsResult<JsValue> {
    arity(args, 1, 2, if to_upper { "Upper" } else { "Lower" })?;
    let value = get_simple_value(args, 0, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    let mut text = utf8_decode(&value_to_utf8_string(&value, context)?);
    for c in &mut text {
        if to_upper {
            if (0x61..=0x7a).contains(c) || (0xe0..=0xfe).contains(c) {
                *c -= 32;
            } else if [0x101, 0x103, 0x105].contains(c) {
                *c -= 1;
            }
        } else if (0x41..=0x5a).contains(c) || (0xc0..=0xde).contains(c) {
            *c += 32;
        } else if [0x100, 0x102, 0x104].contains(c) {
            *c += 1;
        }
    }
    Ok(wide(&text))
}
fn lower(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    change_case(args, context, false)
}
fn upper(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    change_case(args, context, true)
}
fn whitespace(c: u8) -> bool {
    matches!(c, 0x20 | 0x09 | 0x0b | 0x0c | 0x0a | 0x0d)
}
fn trim(args: &[JsValue], context: &mut Context, back: bool) -> JsResult<JsValue> {
    arity(args, 1, 1, if back { "Rtrim" } else { "Ltrim" })?;
    let value = get_simple_value(args, 0, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    let text = value_to_utf8_string(&value, context)?;
    let range = if back {
        0..text
            .iter()
            .rposition(|&c| !whitespace(c))
            .map_or(0, |i| i + 1)
    } else {
        text.iter()
            .position(|&c| !whitespace(c))
            .unwrap_or(text.len())..text.len()
    };
    Ok(bytes(&text[range]))
}
fn ltrim(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    trim(args, context, false)
}
fn rtrim(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    trim(args, context, true)
}
fn replace(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arity(args, 2, 3, "Replace")?;
    let values = simple(args, 2, context)?;
    let (mut text, mut needle) = (Vec::new(), Vec::new());
    if !any_null(&values, context)? {
        text = value_to_utf8_string(&values[0], context)?;
        needle = value_to_utf8_string(&values[1], context)?;
    }
    let replacement = if args.len() == 3 {
        let value = get_simple_value(args, 2, context)?;
        value_to_utf8_string(&value, context)?
    } else {
        Vec::new()
    };
    if needle.is_empty() {
        return Ok(bytes(&text));
    }
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < text.len() {
        if text[offset..].starts_with(&needle) {
            result.extend_from_slice(&replacement);
            offset += needle.len();
        } else {
            result.push(text[offset]);
            offset += 1;
        }
    }
    Ok(bytes(&result))
}
fn space(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arity(args, 1, 1, "Space")?;
    let value = get_simple_value(args, 0, context)?;
    if value.is_null() {
        return Ok(JsValue::null());
    }
    let count = value_to_integer(&value, context)?.max(0) as usize;
    if count > MAX_CHAR_COUNT {
        return Err(exception("String too long."));
    }
    Ok(bytes(&vec![b' '; count]))
}
fn str_number(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arity(args, 1, 3, "Str")?;
    let value = get_simple_value(args, 0, context)?;
    if value.is_null() {
        return Ok(JsValue::null());
    }
    let number = value_to_float(&value, context)?;
    let width = if args.len() > 1 {
        let value = get_simple_value(args, 1, context)?;
        value_to_float(&value, context)? as i32
    } else {
        10
    };
    if width > MAX_CHAR_COUNT as i32 {
        return Err(exception("String too long."));
    }
    let precision = if args.len() > 2 {
        let value = get_simple_value(args, 2, context)?;
        (value_to_float(&value, context)? as i32).clamp(0, 15) as usize
    } else {
        0
    };
    let text = format!(
        "{:.*}",
        if precision == 0 { 6 } else { precision },
        f64::from(number)
    );
    let decimal = text.find('.').unwrap_or(text.len());
    if decimal as i64 > i64::from(width) || (precision + decimal) as i64 >= i64::from(width) {
        return Ok(bytes(&vec![b'*'; width.max(0) as usize]));
    }
    if decimal == text.len() {
        let mut result = vec![b' '; (width as usize).saturating_sub(text.len())];
        result.extend(text.as_bytes());
        return Ok(bytes(&result));
    }
    let padding = (i64::from(width) - decimal as i64 - precision as i64 - i64::from(precision != 0))
        .max(0) as usize;
    let mut result = vec![b' '; padding];
    result.extend_from_slice(&text.as_bytes()[..decimal]);
    if precision > 0 {
        result.push(b'.');
        let fraction = &text.as_bytes()[decimal + 1..];
        result.extend_from_slice(&fraction[..fraction.len().min(precision)]);
        result.resize(padding + decimal + 1 + precision, b'0');
    }
    Ok(bytes(&result))
}
fn stuff(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arity(args, 3, 4, "Stuff")?;
    let values = simple(args, 3, context)?;
    if values.iter().any(JsValue::is_null) {
        return Ok(JsValue::null());
    }
    let text = value_to_utf8_string(&values[0], context)?;
    let (mut start, mut delete) = (0, 0);
    if !text.is_empty() {
        start = (value_to_float(&values[1], context)? as i32)
            .clamp(1, text.len().min(i32::MAX as usize) as i32) as usize
            - 1;
        delete = (value_to_float(&values[2], context)? as i32).max(0) as usize;
        delete = delete.min(text.len() - start);
    }
    let insert = if args.len() == 4 {
        let value = get_simple_value(args, 3, context)?;
        value_to_utf8_string(&value, context)?
    } else {
        Vec::new()
    };
    let mut result = Vec::with_capacity(text.len() - delete + insert.len());
    result.extend_from_slice(&text[..start]);
    result.extend(insert);
    result.extend_from_slice(&text[start + delete..]);
    Ok(bytes(&result))
}
fn substr(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arity(args, 3, 3, "Substr")?;
    let values = simple(args, 3, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let text = value_to_utf8_string(&values[0], context)?;
    if text.is_empty() {
        return Ok(bytes(&[]));
    }
    let start = (value_to_integer(&values[1], context)?.max(1) - 1) as usize;
    if start >= text.len() {
        return Ok(bytes(&[]));
    }
    let count = (value_to_integer(&values[2], context)?.max(0) as usize).min(text.len() - start);
    Ok(bytes(&text[start..start + count]))
}
fn word_num(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arity(args, 1, 3, "WordNum")?;
    let value = get_simple_value(args, 0, context)?;
    if value.is_null() {
        return Ok(JsValue::null());
    }
    let number = value_to_float(&value, context)?;
    let mut style = 0;
    if args.len() > 1 {
        let value = get_simple_value(args, 1, context)?;
        if value.is_null() {
            return Ok(JsValue::null());
        }
        style = value_to_float(&value, context)? as i32;
    }
    if args.len() > 2 {
        let value = get_simple_value(args, 2, context)?;
        if value.is_null() {
            return Ok(JsValue::null());
        }
        let _ = value_to_utf8_string(&value, context)?;
    }
    if number.is_nan() || number < 0. || number > 922337203685477550.0_f32 {
        return Ok(bytes(b"*"));
    }
    Ok(bytes(
        word_us(format!("{:.2}", f64::from(number)).as_bytes(), style).as_bytes(),
    ))
}

fn hex(c: u32) -> Option<u32> {
    match c {
        0x30..=0x39 => Some(c - 0x30),
        0x61..=0x66 => Some(c - 0x61 + 10),
        0x41..=0x46 => Some(c - 0x41 + 10),
        _ => None,
    }
}
fn decode_text(text: &[u32], kind: &[u8]) -> Vec<u32> {
    if kind.eq_ignore_ascii_case(b"html") {
        decode_ml(text, true)
    } else if kind.eq_ignore_ascii_case(b"xml") {
        decode_ml(text, false)
    } else {
        decode_url(text)
    }
}
fn decode_url(text: &[u32]) -> Vec<u32> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let c = text[index];
        index += 1;
        if c != u32::from(b'%') {
            result.push(c);
            continue;
        }
        let Some(pair) = text.get(index..index + 2) else {
            return Vec::new();
        };
        let (Some(high), Some(low)) = (hex(pair[0]), hex(pair[1])) else {
            return Vec::new();
        };
        result.push(high * 16 + low);
        index += 2;
    }
    result
}
fn decode_ml(text: &[u32], html: bool) -> Vec<u32> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let c = text[index];
        index += 1;
        if c != u32::from(b'&') {
            result.push(c);
            continue;
        }
        let Some(&c) = text.get(index) else { break };
        if c == u32::from(b'#') {
            index += 1;
            let Some(&c) = text.get(index) else { break };
            if c != u32::from(b'x') && c != u32::from(b'X') {
                return Vec::new();
            }
            index += 1;
            if index == text.len() {
                break;
            }
            let mut code = 0u32;
            while index < text.len() && text[index] != u32::from(b';') {
                let Some(value) = hex(text[index]) else {
                    return Vec::new();
                };
                code = code.wrapping_mul(16).wrapping_add(value);
                index += 1;
            }
            result.push(code);
        } else {
            let mut name = Vec::with_capacity(8);
            while index < text.len() && text[index] != u32::from(b';') {
                if name.len() < 8 {
                    name.push(text[index]);
                }
                index += 1;
            }
            if let Some(nul) = name.iter().position(|&c| c == 0) {
                name.truncate(nul);
            }
            if html {
                if name.iter().all(|&c| c < 128) {
                    let name: Vec<u8> = name.into_iter().map(|c| c as u8).collect();
                    if let Ok(found) = HTML_DECODE
                        .binary_search_by(|(candidate, _)| candidate.as_bytes().cmp(&name))
                    {
                        result.push(HTML_DECODE[found].1);
                    }
                }
            } else {
                let code = match name.as_slice() {
                    [113, 117, 111, 116] => Some(34),
                    [97, 109, 112] => Some(38),
                    [97, 112, 111, 115] => Some(39),
                    [108, 116] => Some(60),
                    [103, 116] => Some(62),
                    _ => None,
                };
                if let Some(code) = code {
                    result.push(code);
                }
            }
        }
        if index < text.len() {
            index += 1;
        }
    }
    result
}
fn append_ascii(result: &mut Vec<u32>, text: &str) {
    result.extend(text.bytes().map(u32::from));
}
fn encode_text(text: &[u8], kind: &[u8]) -> Vec<u32> {
    let text = utf8_decode(text);
    let html = kind.eq_ignore_ascii_case(b"html");
    let xml = kind.eq_ignore_ascii_case(b"xml");
    let mut result = Vec::new();
    for c in text {
        if !html && !xml {
            if c <= 0x1f
                || (0x7f..=0xff).contains(&c)
                || b" <>\"#%{}|\\^~[]`;/?:@=&"
                    .iter()
                    .any(|&x| u32::from(x) == c)
            {
                append_ascii(&mut result, &format!("%{c:02x}"));
            } else if (0x20..=0x7e).contains(&c) {
                result.push(c);
            } else {
                let mut code = format!("{c:x}");
                if code.len() % 2 != 0 {
                    code.insert(0, '0');
                }
                for pair in code.as_bytes().chunks_exact(2) {
                    result.extend([u32::from(b'%'), u32::from(pair[0]), u32::from(pair[1])]);
                }
            }
            continue;
        }
        let name = if html {
            HTML_ENCODE
                .binary_search_by_key(&c, |(code, _)| *code)
                .ok()
                .map(|i| HTML_ENCODE[i].1)
        } else {
            match c {
                34 => Some("quot"),
                38 => Some("amp"),
                39 => Some("apos"),
                60 => Some("lt"),
                62 => Some("gt"),
                _ => None,
            }
        };
        if let Some(name) = name {
            result.push(u32::from(b'&'));
            append_ascii(&mut result, name);
            result.push(u32::from(b';'));
        } else if (32..=126).contains(&c) {
            result.push(c);
        } else if c < 256 {
            append_ascii(&mut result, &format!("&#x{c:02x};"));
        } else if c < 65536 {
            append_ascii(&mut result, &format!("&#x{c:04x};"));
        }
        // The pinned HTML/XML encoder has no non-BMP path and omits it.
    }
    result
}
const UNITS: [&str; 10] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
];
const CAP_UNITS: [&str; 10] = [
    "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine",
];
const TENS: [&str; 10] = [
    "Ten",
    "Eleven",
    "Twelve",
    "Thirteen",
    "Fourteen",
    "Fifteen",
    "Sixteen",
    "Seventeen",
    "Eighteen",
    "Nineteen",
];
const LAST_TENS: [&str; 8] = [
    "Twenty", "Thirty", "Forty", "Fifty", "Sixty", "Seventy", "Eighty", "Ninety",
];
const COMMON: [&str; 5] = [
    " Hundred ",
    " Thousand ",
    " Million ",
    " Billion ",
    "Trillion",
];
fn group_us(data: &[u8], result: &mut String) {
    let mut offset = 0;
    if data.len() == 3 {
        if data[0] != b'0' {
            result.push_str(CAP_UNITS[(data[0] - b'0') as usize]);
            result.push_str(COMMON[0]);
        }
        offset = 1;
    }
    if data.len() - offset == 1 {
        result.push_str(CAP_UNITS[(data[offset] - b'0') as usize]);
    } else {
        let tens = data[offset];
        let unit = (data[offset + 1] - b'0') as usize;
        if tens == b'0' {
            result.push_str(CAP_UNITS[unit]);
        } else if tens == b'1' {
            result.push_str(TENS[unit]);
        } else {
            result.push_str(LAST_TENS[(tens - b'2') as usize]);
            result.push('-');
            result.push_str(UNITS[unit]);
        }
    }
}
fn trillion_us(data: &[u8]) -> String {
    if data.is_empty() || !data.iter().all(u8::is_ascii_digit) {
        return String::new();
    }
    let mut result = String::new();
    let first = if data.len() % 3 == 0 {
        3
    } else {
        data.len() % 3
    };
    let mut common = (data.len() - 1) / 3;
    group_us(&data[..first], &mut result);
    if data.len() > 3 {
        result.push_str(COMMON[common]);
        common -= 1;
    }
    let mut index = first;
    while index < data.len() {
        group_us(&data[index..index + 3], &mut result);
        if index < data.len() - 3 {
            result.push_str(COMMON[common]);
            common -= 1;
        }
        index += 3;
    }
    result
}
fn word_parts(data: &[u8]) -> String {
    let mut result = String::new();
    let mut index = 0;
    while index < data.len() {
        let remaining = data.len() - index;
        let count = if remaining % 12 == 0 {
            12
        } else {
            remaining % 12
        };
        result.push_str(&trillion_us(&data[index..index + count]));
        index += count;
        if index < data.len() {
            result.push_str(" Trillion ");
        }
    }
    result
}
fn word_us(data: &[u8], style: i32) -> String {
    if !(0..=2).contains(&style) {
        return String::new();
    }
    let decimal = data.iter().position(|&c| c == b'.').unwrap_or(data.len());
    let mut result = word_parts(&data[..decimal]);
    if style > 0 {
        result.push_str(" Dollars");
    }
    if style > 1 && decimal < data.len() {
        result.push_str(" And ");
        result.push_str(&word_parts(&data[decimal + 1..]));
        result.push_str(" Cents");
    }
    result
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::super::cfxjse_formcalc_context::CfxJseFormcalcContext;
    use super::*;
    fn execute(runtime: &CfxJseFormcalcContext, source: &str) -> JsResult<JsValue> {
        runtime.execute(&source.chars().map(u32::from).collect::<Vec<_>>())
    }
    fn check_string(runtime: &CfxJseFormcalcContext, source: &str, expected: &str) {
        let result = execute(runtime, source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
        assert_eq!(
            result.as_string().expect(source).to_vec(),
            expected.encode_utf16().collect::<Vec<_>>(),
            "{source}"
        );
    }
    fn check_integer(runtime: &CfxJseFormcalcContext, source: &str, expected: f64) {
        assert_eq!(
            execute(runtime, source).unwrap().as_number(),
            Some(expected),
            "{source}"
        );
    }
    fn check_error(runtime: &CfxJseFormcalcContext, source: &str) {
        assert!(execute(runtime, source).is_err(), "{source}");
    }
    fn check_null(runtime: &CfxJseFormcalcContext, source: &str) {
        assert!(execute(runtime, source).unwrap().is_null(), "{source}");
    }
    // Complete selected cfxjse_formcalc_context_embeddertest.cpp assertions.
    // Pure string functions use the real compiler/runtime without an XFA PDF.
    #[test]
    fn At() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_integer(&runtime, r#"At("ABCDEFGH", "AB")"#, 1.0);
        check_integer(&runtime, r#"At("ABCDEFGH", "F")"#, 6.0);
        check_integer(&runtime, r#"At(23412931298471, 29)"#, 5.0);
    }
    #[test]
    fn Concat() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Concat("ABC", "DEF")"#, r#"ABCDEF"#);
        check_string(
            &runtime,
            r#"Concat("Tony", Space(1), "Blue")"#,
            r#"Tony Blue"#,
        );
        check_string(
            &runtime,
            r#"Concat("You owe ", WordNum(1154.67, 2), ".")"#,
            r#"You owe One Thousand One Hundred Fifty-four Dollars And Sixty-seven Cents."#,
        );
    }
    #[test]
    fn Decode() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Decode("", "html")"#, r#""#);
        check_string(&runtime, r#"Decode("abc&Acirc;xyz", "html")"#, r#"abcÂxyz"#);
        check_string(
            &runtime,
            r#"Decode("abc&NoneSuchButVeryLongIndeed;", "html")"#,
            r#"abc"#,
        );
        check_string(
            &runtime,
            r#"Decode("&#x0041;&AElig;&Aacute;", "html")"#,
            r#"AÆÁ"#,
        );
        check_string(&runtime, r#"Decode("xyz&#", "html")"#, r#"xyz"#);
        check_string(&runtime, r#"Decode("|&zzzzzz;|", "html")"#, r#"||"#);
        check_string(&runtime, r#"Decode("", "xml")"#, r#""#);
        check_string(
            &runtime,
            r#"Decode("~!@#$%%^&amp;*()_+|`", "xml")"#,
            r#"~!@#$%%^&*()_+|`"#,
        );
        check_string(
            &runtime,
            r#"Decode("abc&nonesuchbutverylongindeed;", "xml")"#,
            r#"abc"#,
        );
        check_string(
            &runtime,
            r#"Decode("&quot;&#x45;&lt;&gt;[].&apos;", "xml")"#,
            r#""E<>[].'"#,
        );
        check_string(&runtime, r#"Decode("xyz&#", "xml")"#, r#"xyz"#);
        check_string(&runtime, r#"Decode("|&zzzzzz;|", "xml")"#, r#"||"#);
        check_string(&runtime, r#"Decode("", "url")"#, r#""#);
        check_string(
            &runtime,
            r#"Decode("~%26^&*()_+|`{", "url")"#,
            r#"~&^&*()_+|`{"#,
        );
        check_string(
            &runtime,
            r#"Decode("~%26^&*()_+|`{", "mbogo")"#,
            r#"~&^&*()_+|`{"#,
        );
        check_string(&runtime, r#"Decode("~%26^&*()_+|`{")"#, r#"~&^&*()_+|`{"#);
        check_string(&runtime, r#"Decode("~%~~")"#, r#""#);
        check_string(&runtime, r#"Decode("?%f~")"#, r#""#);
        check_string(&runtime, r#"Decode("?%~")"#, r#""#);
        check_string(&runtime, r#"Decode("?%")"#, r#""#);
    }
    #[test]
    fn Encode() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(
            &runtime,
            r#"Encode("X/~&^*<=>?|")"#,
            r#"X%2f%7e%26%5e*%3c%3d%3e%3f%7c"#,
        );
        check_string(
            &runtime,
            r#"Encode("X/~&^*<=>?|", "mbogo")"#,
            r#"X%2f%7e%26%5e*%3c%3d%3e%3f%7c"#,
        );
        check_string(
            &runtime,
            r#"Encode("X/~&^*<=>?|", "url")"#,
            r#"X%2f%7e%26%5e*%3c%3d%3e%3f%7c"#,
        );
        check_string(
            &runtime,
            r#"Encode("X/~&^*<=>?|", "xml")"#,
            r#"X/~&amp;^*&lt;=&gt;?|"#,
        );
        check_string(
            &runtime,
            r#"Encode("X/~&^*<=>?|", "html")"#,
            r#"X/~&amp;^*&lt;=&gt;?|"#,
        );
        check_string(
            &runtime,
            r#"Encode("\u0022\u00f5\ufed0", "url")"#,
            r#"%22%f5%fe%d0"#,
        );
        check_string(
            &runtime,
            r#"Encode("\u0022\u00f4\ufed0", "xml")"#,
            r#"&quot;&#xf4;&#xfed0;"#,
        );
        check_string(
            &runtime,
            r#"Encode("\u0022\u00f5\ufed0", "html")"#,
            r#"&quot;&otilde;&#xfed0;"#,
        );
        check_string(&runtime, r#"Encode("\uD83D\uDCA9", "url")"#, r#"%01%f4%a9"#);
        check_string(&runtime, r#"Encode("\uD83D\uDCA9", "xml")"#, r#""#);
        check_string(&runtime, r#"Encode("\uD83D\uDCA9", "html")"#, r#""#);
    }
    #[test]
    fn Left() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Left("ABCDEFGH", 3)"#, r#"ABC"#);
        check_string(&runtime, r#"Left("Tony Blue", 5)"#, r#"Tony "#);
    }
    #[test]
    fn Len() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_integer(&runtime, r#"Len("ABCDEFGH")"#, 8.0);
        check_integer(&runtime, r#"Len(4)"#, 1.0);
        check_integer(&runtime, r#"Len(Str(4.532, 6, 4))"#, 6.0);
    }
    #[test]
    fn Lower() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Lower("ABC")"#, r#"abc"#);
        check_string(&runtime, r#"Lower("21 Main St.")"#, r#"21 main st."#);
        check_string(&runtime, r#"Lower(15)"#, r#"15"#);
    }
    #[test]
    fn Ltrim() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Ltrim("   ABCD")"#, r#"ABCD"#);
        check_string(
            &runtime,
            r#"Ltrim(Rtrim("    Tony Blue    "))"#,
            r#"Tony Blue"#,
        );
    }
    #[test]
    fn Replace() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(
            &runtime,
            r#"Replace("Tony Blue", "Tony", "Chris")"#,
            r#"Chris Blue"#,
        );
        check_string(&runtime, r#"Replace("ABCDEFGH", "D")"#, r#"ABCEFGH"#);
        check_string(&runtime, r#"Replace("ABCDEFGH", "d")"#, r#"ABCDEFGH"#);
    }
    #[test]
    fn Right() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Right("ABCDEFGH", 3)"#, r#"FGH"#);
        check_string(&runtime, r#"Right("Tony Blue", 5)"#, r#" Blue"#);
    }
    #[test]
    fn Rtrim() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Rtrim("ABCD   ")"#, r#"ABCD"#);
        check_string(&runtime, "Rtrim(\"Tony Blue      \t\")", r#"Tony Blue"#);
    }
    #[test]
    fn Space() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Space(5)"#, r#"     "#);
        check_string(
            &runtime,
            r#"Concat("Tony", Space(1), "Blue")"#,
            r#"Tony Blue"#,
        );
        check_error(&runtime, r#"Space(15654909)"#);
        check_error(&runtime, r#"Space(99999999)"#);
        check_error(&runtime, r#"Space()"#);
        check_error(&runtime, r#"Space(1, 2)"#);
        check_null(&runtime, r#"Space( $)"#);
    }
    #[test]
    fn Str() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Str(2.456)"#, r#"         2"#);
        check_string(&runtime, r#"Str(4.532, 6, 4)"#, r#"4.5320"#);
        check_string(&runtime, r#"Str(234.458, 4)"#, r#" 234"#);
        check_string(&runtime, r#"Str(31.2345, 4, 2)"#, r#"****"#);
        check_string(&runtime, r#"Str(-765, 19, 14)"#, r#"-765.00000000000000"#);
        check_string(&runtime, r#"Str(-765, 20, 15)"#, r#"-765.000000000000000"#);
        check_string(&runtime, r#"Str(-765, 21, 16)"#, r#" -765.000000000000000"#);
        check_error(&runtime, r#"Str()"#);
        check_error(&runtime, r#"Str(1, 2, 3, 4)"#);
        check_error(&runtime, r#"Str(42, 15654909)"#);
        check_null(&runtime, r#"Str( $)"#);
    }
    #[test]
    fn Stuff() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_error(&runtime, r#"Stuff(1, 2)"#);
        check_error(&runtime, r#"Stuff(1, 2, 3, 4, 5)"#);
        check_null(&runtime, r#"Stuff(null, 0, 4)"#);
        check_null(&runtime, r#"Stuff("ABCDEFG", null, 4)"#);
        check_null(&runtime, r#"Stuff("ABCDEFG", 0, null)"#);
        check_string(&runtime, r#"Stuff("", 0, 0, "clams")"#, r#"clams"#);
        check_string(&runtime, r#"Stuff("TonyBlue", 5, 0, " ")"#, r#"Tony Blue"#);
        check_string(&runtime, r#"Stuff("A", 1, 0)"#, r#"A"#);
        check_string(&runtime, r#"Stuff("A", 1, 1)"#, r#""#);
        check_string(&runtime, r#"Stuff("ABCDEFGH", 4, 2)"#, r#"ABCFGH"#);
        check_string(&runtime, r#"Stuff("ABCDEFGH", 7, 2)"#, r#"ABCDEF"#);
        check_string(&runtime, r#"Stuff("ABCDEFGH", -400, 400)"#, r#""#);
        check_string(
            &runtime,
            r#"Stuff("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345678900abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345678900abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345678900abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345678900", 133, 2147483520)"#,
            r#"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345678900abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ012345678900abcd"#,
        );
    }
    #[test]
    fn Substr() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_error(&runtime, r#"Substr()"#);
        check_error(&runtime, r#"Substr(1)"#);
        check_error(&runtime, r#"Substr(1, 2)"#);
        check_error(&runtime, r#"Substr(1, 2, 3, 4)"#);
        check_null(&runtime, r#"Substr(null, 0, 4)"#);
        check_null(&runtime, r#"Substr("ABCDEFG", null, 4)"#);
        check_null(&runtime, r#"Substr("ABCDEFG", 0, null)"#);
        check_null(&runtime, r#"Substr(null, null, 4)"#);
        check_null(&runtime, r#"Substr(null, 0, null)"#);
        check_null(&runtime, r#"Substr("ABCDEFG", null, null)"#);
        check_null(&runtime, r#"Substr(null, null, null)"#);
        check_string(&runtime, r#"Substr("ABCDEFG", -1, 4)"#, r#"ABCD"#);
        check_string(&runtime, r#"Substr("ABCDEFG", 0, 4)"#, r#"ABCD"#);
        check_string(&runtime, r#"Substr("ABCDEFG", 3, 4)"#, r#"CDEF"#);
        check_string(&runtime, r#"Substr("ABCDEFG", 4, 4)"#, r#"DEFG"#);
        check_string(&runtime, r#"Substr("ABCDEFG", 5, 4)"#, r#"EFG"#);
        check_string(&runtime, r#"Substr("ABCDEFG", 6, 4)"#, r#"FG"#);
        check_string(&runtime, r#"Substr("ABCDEFG", 7, 4)"#, r#"G"#);
        check_string(&runtime, r#"Substr("ABCDEFG", 8, 4)"#, r#""#);
        check_string(&runtime, r#"Substr("ABCDEFG", 5, -1)"#, r#""#);
        check_string(&runtime, r#"Substr("ABCDEFG", 5, 0)"#, r#""#);
        check_string(&runtime, r#"Substr("ABCDEFG", 5, 1)"#, r#"E"#);
        check_string(&runtime, r#"Substr("abcdefghi", 5, 3)"#, r#"efg"#);
        check_string(&runtime, r#"Substr(3214, 2, 1)"#, r#"2"#);
        check_string(&runtime, r#"Substr("21 Waterloo St.", 4, 5)"#, r#"Water"#);
    }
    #[test]
    fn Upper() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_string(&runtime, r#"Upper("abc")"#, r#"ABC"#);
        check_string(&runtime, r#"Upper("21 Main St.")"#, r#"21 MAIN ST."#);
        check_string(&runtime, r#"Upper(15)"#, r#"15"#);
    }
    #[test]
    fn WordNum() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        check_error(&runtime, r#"WordNum()"#);
        check_error(&runtime, r#"WordNum(1, 2, 3, 4)"#);
        check_string(
            &runtime,
            r#"WordNum(123.45)"#,
            r#"One Hundred Twenty-three"#,
        );
        check_string(
            &runtime,
            r#"WordNum(123.45, 0)"#,
            r#"One Hundred Twenty-three"#,
        );
        check_string(
            &runtime,
            r#"WordNum(123.45, 1)"#,
            r#"One Hundred Twenty-three Dollars"#,
        );
        check_string(
            &runtime,
            r#"WordNum(123.45, 2)"#,
            r#"One Hundred Twenty-three Dollars And Forty-five Cents"#,
        );
        check_string(&runtime, r#"WordNum(123.45, -1)"#, r#""#);
        check_string(&runtime, r#"WordNum(123.45, 3)"#, r#""#);
        check_string(
            &runtime,
            r#"WordNum(123.45, 0, "zh_CN")"#,
            r#"One Hundred Twenty-three"#,
        );
        check_string(&runtime, r#"WordNum(0, 0)"#, r#"Zero"#);
        check_string(&runtime, r#"WordNum(0, 1)"#, r#"Zero Dollars"#);
        check_string(
            &runtime,
            r#"WordNum(0, 2)"#,
            r#"Zero Dollars And Zero Cents"#,
        );
        check_string(&runtime, r#"WordNum(0.12, 0)"#, r#"Zero"#);
        check_string(&runtime, r#"WordNum(0.12, 1)"#, r#"Zero Dollars"#);
        check_string(
            &runtime,
            r#"WordNum(0.12, 2)"#,
            r#"Zero Dollars And Twelve Cents"#,
        );
        check_string(&runtime, r#"WordNum(-1, 0)"#, r#"*"#);
        check_string(&runtime, r#"WordNum(-1, 1)"#, r#"*"#);
        check_string(&runtime, r#"WordNum(-1, 2)"#, r#"*"#);
        check_string(
            &runtime,
            r#"WordNum(1.234e+6)"#,
            r#"One Million Two Hundred Thirty-four Thousand Zero"#,
        );
        check_string(
            &runtime,
            r#"WordNum(1.234e+9)"#,
            r#"One Billion Two Hundred Thirty-four Million Zero Thousand Zero"#,
        );
        check_string(
            &runtime,
            r#"WordNum(1.234e+12)"#,
            r#"One Trillion Two Hundred Thirty-four Billion Zero Million Nineteen Thousand Four Hundred Fifty-six"#,
        );
        check_string(
            &runtime,
            r#"WordNum(1.234e+15)"#,
            r#"One Thousand Two Hundred Thirty-three Trillion Nine Hundred Ninety-nine Billion Nine Hundred Thirty-eight Million Seven Hundred Fifteen Thousand Six Hundred Forty-eight"#,
        );
        check_string(&runtime, r#"WordNum(1.234e+18)"#, r#"*"#);
        check_string(&runtime, r#"WordNum(1.234e+21)"#, r#"*"#);
        check_string(&runtime, r#"WordNum(1.234e+24)"#, r#"*"#);
        check_string(&runtime, r#"WordNum(1.234e+30)"#, r#"*"#);
    }
    #[test]
    fn Bug854623() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        let input = [
            0x4c, 0x6f, 0x77, 0x65, 0x72, 0x28, 0x22, 0xc3, 0x85, 0xc3, 0x85, 0xc3, 0x85, 0x22,
            0x29,
        ];
        assert!(execute(&runtime, std::str::from_utf8(&input).unwrap()).is_ok());
        check_string(&runtime, "Lower(\"ÅÅÅ\")", "ååå");
    }
    #[test]
    fn complete_entity_tables_and_codec_failure_boundaries() {
        assert!(HTML_DECODE.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert!(HTML_ENCODE.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for &(name, code) in HTML_DECODE {
            let encoded = format!("&{name};");
            assert_eq!(
                decode_text(&encoded.chars().map(u32::from).collect::<Vec<_>>(), b"HtMl"),
                [code],
                "{name}"
            );
        }
        for &(code, name) in HTML_ENCODE {
            assert_eq!(
                encode_text(&utf8_encode(&[code]), b"HTML"),
                format!("&{name};")
                    .chars()
                    .map(u32::from)
                    .collect::<Vec<_>>()
            );
        }
        for malformed in [
            "prefix%",
            "prefix%a",
            "prefix%ax",
            "prefix&#65;",
            "prefix&#xq;",
        ] {
            let kind = if malformed.contains('%') {
                b"url".as_slice()
            } else {
                b"xml".as_slice()
            };
            assert!(
                decode_text(&malformed.chars().map(u32::from).collect::<Vec<_>>(), kind).is_empty()
            );
        }
        assert_eq!(
            decode_text(&"&amp".chars().map(u32::from).collect::<Vec<_>>(), b"xml"),
            [38]
        );
        assert_eq!(
            decode_text(&"x&#".chars().map(u32::from).collect::<Vec<_>>(), b"xml"),
            [120]
        );
        assert_eq!(
            encode_text(&utf8_encode(&[0x123, 0x1f4a9]), b"url"),
            "%01%23%01%f4%a9".chars().map(u32::from).collect::<Vec<_>>()
        );
    }
    #[test]
    fn all_string_arities_nulls_byte_offsets_and_source_ignored_options() {
        let mut context = Context::default();
        for &(name, callback) in METHODS {
            context
                .register_global_builtin_callable(
                    JsString::from(name),
                    0,
                    boa_engine::NativeFunction::from_fn_ptr(callback),
                )
                .unwrap();
        }
        let source = r#"
            function check(v){if(!v)throw Error('string boundary assertion')}
            for(let name of ['At','Concat','Decode','Encode','Left','Len','Lower','Ltrim','Replace','Right','Rtrim','Space','Str','Stuff','Substr','Upper','WordNum']){
                let failed=false;try{globalThis[name]()}catch(e){failed=String(e).includes('Incorrect number of parameters')}check(failed);
            }
            check(At(null,'a')===null&&Concat(null,null)===null&&Concat(null,'')==='');
            check(Decode(null)===null&&Encode('a',null)===null&&Left(null,1)===null&&Len(null)===null);
            check(Lower(null)===null&&Ltrim(null)===null&&Right('a',null)===null&&Rtrim(null)===null);
            check(Space(null)===null&&Str(null)===null&&Stuff('a',null,0)===null&&Substr(null,1,1)===null);
            check(Upper(null)===null&&WordNum(null)===null&&WordNum(1,null)===null&&WordNum(1,0,null)===null);
            check(Replace(null,'x')===''&&Replace('abc',null,'x')==='');
            check(Len('é')===2&&At('éX','X')===3&&Left('é',1)==='\ufffd'&&Right('é',1)==='\ufffd');
            let seen='';
            const a=[0,'text',{get text(){seen+='a';return 'hay'}}];
            const b=[0,'text',{get text(){seen+='b';return ''}}];
            check(At(a,b)===1&&seen==='ab');
            seen='';const ignored=[0,'text',{get text(){seen+='x';return null}}];
            check(Lower('ABC',ignored)==='abc'&&Upper('abc',ignored)==='ABC'&&seen==='');
            check(Replace('aaa','aa','b')==='ba'&&Space(-1)===''&&Substr('abc',99,1)==='');
        "#;
        assert_eq!(
            context
                .eval(boa_engine::Source::from_bytes(source))
                .unwrap(),
            JsValue::undefined()
        );
    }
}
