// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_util.{h,cpp}`. Format validation, argument order and mask processing
//! follow PDFium. Safe Rust formatting replaces the platform printf/strftime
//! calls; UTF-16 strings are never round-tripped through lossy UTF-8.
use super::{
    fx_date_helpers as date,
    js_date::{date_value, local_time, new_date, new_local_date},
    js_resources::{js_format_error_string, js_get_string_from_id, JsMessage},
};
use boa_engine::{
    js_string,
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::Attribute,
    Context, JsData, JsError, JsNativeError, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, Trace};

#[derive(Debug, Trace, Finalize, JsData)]
struct UtilBinding;
fn error(method: &str, message: JsMessage) -> JsError {
    JsError::from_opaque(
        JsString::from(
            js_format_error_string("util", Some(method), &js_get_string_from_id(message))
                .as_slice(),
        )
        .into(),
    )
}
fn receiver(this: &JsValue) -> JsResult<()> {
    if this
        .as_object()
        .is_some_and(|object| object.is::<UtilBinding>())
    {
        Ok(())
    } else {
        Err(JsNativeError::typ()
            .with_message("Illegal invocation")
            .into())
    }
}
fn string(value: &JsValue, context: &mut Context) -> JsResult<Vec<u16>> {
    Ok(value.to_string(context)?.to_vec())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    Invalid,
    Int,
    Double,
    String,
}
/// PDFium's validation state machine; changes s to S in accepted string formats.
pub fn parse_data_type(format: &mut [u16]) -> DataType {
    #[derive(Clone, Copy)]
    enum State {
        Before,
        Flags,
        Width,
        Precision,
        Specifier,
        After,
    }
    let mut state = State::Before;
    let mut result = DataType::Invalid;
    let mut precision_digits = 0;
    let mut i = 0;
    while i < format.len() {
        let c = format[i];
        match state {
            State::Before => {
                if c == b'%' as u16 {
                    state = State::Flags;
                }
            }
            State::Flags => {
                if ![b'+', b'-', b'#', b' '].map(u16::from).contains(&c) {
                    state = State::Width;
                    continue;
                }
            }
            State::Width => {
                if c == b'*' as u16 {
                    return DataType::Invalid;
                }
                if digit(c) {
                } else if c == b'.' as u16 {
                    state = State::Precision;
                } else {
                    state = State::Specifier;
                    continue;
                }
            }
            State::Precision => {
                if c == b'*' as u16 {
                    return DataType::Invalid;
                }
                if digit(c) {
                    precision_digits += 1;
                } else {
                    state = State::Specifier;
                    continue;
                }
            }
            State::Specifier => {
                result = match c {
                    99 | 67 | 100 | 105 | 111 | 117 | 120 | 88 => DataType::Int,
                    101 | 69 | 102 | 103 | 71 => DataType::Double,
                    115 | 83 => {
                        format[i] = b'S' as u16;
                        DataType::String
                    }
                    _ => return DataType::Invalid,
                };
                state = State::After;
            }
            State::After => {
                if c == b'%' as u16 {
                    return DataType::Invalid;
                }
            }
        }
        i += 1;
    }
    if result == DataType::Int && precision_digits > 2 {
        DataType::Invalid
    } else {
        result
    }
}
fn digit(c: u16) -> bool {
    (b'0' as u16..=b'9' as u16).contains(&c)
}
fn translate_case(c: u32, mode: i8) -> u32 {
    // Use ordinary Unicode scalar case mapping instead of the native helper's
    // ASCII bit arithmetic on Unicode-classified letters. Lone UTF-16
    // surrogates remain code units; masks never perform lossy conversion.
    let Some(character) = char::from_u32(c) else {
        return c;
    };
    let translated = if mode < 0 {
        character.to_lowercase().next()
    } else if mode > 0 {
        character.to_uppercase().next()
    } else {
        Some(character)
    };
    translated.map(u32::from).unwrap_or(c)
}
/// Source CJS_Util::StringPrintx, shared by AFSpecial_Format.
pub fn string_printx(format: &[u16], source: &[u16]) -> Vec<u16> {
    // Match the source wchar32 boundary: valid UTF-16 pairs are one mask
    // character, while unpaired surrogates remain representable unchanged.
    let format = super::cjs_event_context::wide_from_utf16(format);
    let source = super::cjs_event_context::wide_from_utf16(source);
    let digit = |c| (u32::from(b'0')..=u32::from(b'9')).contains(&c);
    let mut result = Vec::with_capacity(format.len());
    let (mut input, mut output, mut mode, mut escaped) = (0, 0, 0, false);
    while output < format.len() {
        let c = format[output];
        if escaped {
            result.push(c);
            escaped = false;
            output += 1;
            continue;
        }
        match c {
            92 => {
                escaped = true;
                output += 1;
            }
            60 => {
                mode = -1;
                output += 1;
            }
            62 => {
                mode = 1;
                output += 1;
            }
            61 => {
                mode = 0;
                output += 1;
            }
            63 => {
                if input < source.len() {
                    result.push(translate_case(source[input], mode));
                    input += 1;
                }
                output += 1;
            }
            88 | 65 | 57 => {
                if input < source.len() {
                    let value = source[input];
                    let alpha = (u32::from(b'A')..=u32::from(b'Z')).contains(&value)
                        || (u32::from(b'a')..=u32::from(b'z')).contains(&value);
                    if (c == 88 && (alpha || digit(value)))
                        || (c == 65 && alpha)
                        || (c == 57 && digit(value))
                    {
                        result.push(translate_case(value, mode));
                        output += 1;
                    }
                    input += 1;
                } else {
                    output += 1;
                }
            }
            42 => {
                if input < source.len() {
                    result.push(translate_case(source[input], mode));
                    input += 1;
                } else {
                    output += 1;
                }
            }
            _ => {
                result.push(c);
                output += 1;
            }
        }
    }
    crate::fxcrt::fx_string::utf16_encode(&result)
}
fn printx(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    receiver(this)?;
    if args.len() < 2 {
        return Err(error("printx", JsMessage::ParamError));
    }
    let format = string(&args[0], context)?;
    let source = string(&args[1], context)?;
    Ok(JsString::from(string_printx(&format, &source).as_slice()).into())
}
fn byte_to_char(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    receiver(this)?;
    let value = args
        .first()
        .ok_or_else(|| error("byteToChar", JsMessage::ParamError))?
        .to_i32(context)?;
    if !(0..=255).contains(&value) {
        return Err(error("byteToChar", JsMessage::ValueError));
    }
    Ok(JsString::from(&[value as u16][..]).into())
}
fn scand(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    receiver(this)?;
    if args.len() < 2 {
        return Err(error("scand", JsMessage::ParamError));
    }
    let format = string(&args[0], context)?;
    let value = string(&args[1], context)?;
    if value.is_empty() {
        // Preserve the actual instant even when local time repeats during DST.
        return new_date(context, context.clock().system_time_millis() as f64);
    }
    let time = super::cjs_publicmethods::parse_date_using_format(context, &value, &format, None)?;
    if time.is_nan() {
        Ok(JsValue::undefined())
    } else {
        new_local_date(context, time)
    }
}

/// A single, validated conversion. Large host allocations return an ordinary
/// script failure instead of native printf's allocation/overflow accidents.
struct Conversion {
    end: usize,
    left: bool,
    plus: bool,
    space: bool,
    alternate: bool,
    zero: bool,
    width: usize,
    precision: Option<usize>,
    specifier: u16,
}
fn conversion(format: &[u16]) -> JsResult<Conversion> {
    let mut c = Conversion {
        end: 1,
        left: false,
        plus: false,
        space: false,
        alternate: false,
        zero: false,
        width: 0,
        precision: None,
        specifier: 0,
    };
    while let Some(&flag) = format.get(c.end) {
        match flag {
            45 => c.left = true,
            43 => c.plus = true,
            32 => c.space = true,
            35 => c.alternate = true,
            48 => c.zero = true,
            _ => break,
        }
        c.end += 1;
    }
    let number = |index: &mut usize| -> JsResult<usize> {
        let mut result = 0usize;
        while let Some(&v) = format.get(*index).filter(|&&v| digit(v)) {
            result = result
                .checked_mul(10)
                .and_then(|n| n.checked_add(usize::from(v - b'0' as u16)))
                .filter(|&n| n <= 1_000_000)
                .ok_or_else(|| error("printf", JsMessage::ValueError))?;
            *index += 1;
        }
        Ok(result)
    };
    c.width = number(&mut c.end)?;
    if format.get(c.end) == Some(&(b'.' as u16)) {
        c.end += 1;
        c.precision = Some(number(&mut c.end)?);
    }
    c.specifier = format[c.end];
    c.end += 1;
    Ok(c)
}
fn exponent(value: f64, precision: usize, upper: bool) -> String {
    let raw = format!("{value:.precision$e}");
    let Some((mantissa, power)) = raw.split_once('e') else {
        return raw;
    };
    let power = power.parse::<i32>().expect("Rust scientific exponent");
    format!("{mantissa}{}{power:+03}", if upper { 'E' } else { 'e' })
}
fn trim_fraction(value: &mut String) {
    let end = value.find(['e', 'E']).unwrap_or(value.len());
    if value[..end].contains('.') {
        let mut trim = end;
        while value.as_bytes()[trim - 1] == b'0' {
            trim -= 1;
        }
        if value.as_bytes()[trim - 1] == b'.' {
            trim -= 1;
        }
        value.replace_range(trim..end, "");
    }
}
fn format_double(value: f64, c: &Conversion) -> String {
    let p = c.precision.unwrap_or(6);
    let mut text = match c.specifier {
        101 | 69 => exponent(value, p, c.specifier == 69),
        102 => format!("{value:.p$}"),
        _ => {
            let p = p.max(1);
            let scientific = exponent(value, p - 1, c.specifier == 71);
            let power = scientific
                .split_once(['e', 'E'])
                .and_then(|(_, n)| n.parse::<i32>().ok())
                .unwrap_or(0);
            let mut output = if power < -4 || power >= p as i32 {
                scientific
            } else {
                let places = (p as i32 - power - 1).max(0) as usize;
                format!("{value:.places$}")
            };
            if !c.alternate {
                trim_fraction(&mut output);
            }
            output
        }
    };
    if c.alternate && value.is_finite() && !text.contains('.') {
        let at = text.find(['e', 'E']).unwrap_or(text.len());
        text.insert(at, '.');
    }
    if matches!(c.specifier, 69 | 71) {
        text.make_ascii_uppercase();
    }
    text
}
fn format_conversion(
    value: &JsValue,
    kind: DataType,
    c: &Conversion,
    context: &mut Context,
) -> JsResult<Vec<u16>> {
    let mut prefix = String::new();
    let mut output: Vec<u16> = match kind {
        DataType::String => {
            let mut value = string(value, context)?;
            if let Some(nul) = value.iter().position(|&u| u == 0) {
                value.truncate(nul);
            }
            if let Some(precision) = c.precision {
                value.truncate(precision);
            }
            value
        }
        DataType::Int => {
            let value = value.to_i32(context)?;
            if matches!(c.specifier, 99 | 67) {
                vec![value as u16]
            } else {
                let unsigned = value as u32;
                let mut digits = match c.specifier {
                    111 => format!("{unsigned:o}"),
                    120 => format!("{unsigned:x}"),
                    88 => format!("{unsigned:X}"),
                    117 => unsigned.to_string(),
                    _ => value.unsigned_abs().to_string(),
                };
                if c.precision == Some(0) && value == 0 {
                    digits.clear();
                }
                if let Some(precision) = c.precision {
                    if precision > digits.len() {
                        digits = "0".repeat(precision - digits.len()) + &digits;
                    }
                }
                if matches!(c.specifier, 100 | 105) {
                    if value < 0 {
                        prefix.push('-');
                    } else if c.plus {
                        prefix.push('+');
                    } else if c.space {
                        prefix.push(' ');
                    }
                } else if c.alternate {
                    match c.specifier {
                        111 => {
                            if !digits.starts_with('0') {
                                prefix.push('0');
                            }
                        }
                        120 if value != 0 => prefix.push_str("0x"),
                        88 if value != 0 => prefix.push_str("0X"),
                        _ => {}
                    }
                }
                digits.encode_utf16().collect()
            }
        }
        DataType::Double => {
            let value = value.to_number(context)?;
            if value.is_sign_negative() {
                prefix.push('-');
            } else if c.plus {
                prefix.push('+');
            } else if c.space {
                prefix.push(' ');
            }
            format_double(value.abs(), c).encode_utf16().collect()
        }
        DataType::Invalid => unreachable!(),
    };
    let padding = c.width.saturating_sub(prefix.len() + output.len());
    let zero = c.zero
        && !c.left
        && kind != DataType::String
        && !matches!(c.specifier, 99 | 67)
        && (kind == DataType::Double || c.precision.is_none());
    let mut result = Vec::with_capacity(prefix.len() + output.len() + padding);
    if !c.left && !zero {
        result.resize(padding, b' ' as u16);
    }
    result.extend(prefix.encode_utf16());
    if zero {
        result.resize(result.len() + padding, b'0' as u16);
    }
    result.append(&mut output);
    if c.left {
        result.resize(result.len() + padding, b' ' as u16);
    }
    Ok(result)
}
fn printf(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    receiver(this)?;
    let format = string(
        args.first()
            .ok_or_else(|| error("printf", JsMessage::ParamError))?,
        context,
    )?;
    let mut result = Vec::new();
    let mut offset = 0;
    let mut argument = 1;
    while offset < format.len() {
        if format[offset] != b'%' as u16 {
            result.push(format[offset]);
            offset += 1;
            continue;
        }
        let end = format[offset + 1..]
            .iter()
            .position(|&c| c == b'%' as u16)
            .map_or(format.len(), |p| offset + 1 + p);
        let mut segment = format[offset..end].to_vec();
        if let Some(value) = args.get(argument) {
            let kind = parse_data_type(&mut segment);
            if kind != DataType::Invalid {
                let c = conversion(&segment)?;
                result.extend(format_conversion(value, kind, &c, context)?);
                result.extend_from_slice(&segment[c.end..]);
            } else {
                result.extend(segment);
            }
        } else {
            result.extend(segment);
        }
        argument += 1;
        offset = end;
    }
    Ok(JsString::from(result.as_slice()).into())
}

fn replace_all(value: &mut Vec<u16>, from: &str, to: &str) {
    let from: Vec<_> = from.encode_utf16().collect();
    let to: Vec<_> = to.encode_utf16().collect();
    let mut offset = 0;
    while offset + from.len() <= value.len() {
        if value[offset..].starts_with(&from) {
            value.splice(offset..offset + from.len(), to.iter().copied());
            offset += to.len();
        } else {
            offset += 1;
        }
    }
}
fn printd(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    receiver(this)?;
    if args.len() < 2 {
        return Err(error("printd", JsMessage::ParamError));
    }
    let time = date_value(&args[1], context)?
        .ok_or_else(|| error("printd", JsMessage::SecondParamNotDateError))?;
    if !time.is_finite() || time.abs() > 8.64e15 {
        return Err(error("printd", JsMessage::SecondParamInvalidDateError));
    }
    let time = local_time(context, time);
    let year = date::get_year_from_time(time);
    let month = date::get_month_from_time(time) + 1;
    let day = date::get_day_from_time(time);
    let hour = date::get_hour_from_time(time);
    let minute = date::get_min_from_time(time);
    let second = date::get_sec_from_time(time);
    if args[0].is_number() {
        let value = match args[0].to_i32(context)? {
            0 => format!("D:{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}"),
            1 => format!("{year:04}.{month:02}.{day:02} {hour:02}:{minute:02}:{second:02}"),
            2 => format!("{year:04}/{month:02}/{day:02} {hour:02}:{minute:02}:{second:02}"),
            _ => return Err(error("printd", JsMessage::ValueError)),
        };
        return Ok(JsString::from(value).into());
    }
    if !args[0].is_string() {
        return Err(error("printd", JsMessage::TypeError));
    }
    if args.get(2).is_some_and(JsValue::to_boolean) {
        return Err(error("printd", JsMessage::NotSupportedError));
    }
    let mut format = string(&args[0], context)?;
    if let Some(nul) = format.iter().position(|&c| c == 0) {
        format.truncate(nul);
    }
    format.retain(|&c| c != b'%' as u16);
    // Keep source replacement order, including its single-character pass.
    for (from, to) in [
        ("mmmm", "%B"),
        ("mmm", "%b"),
        ("mm", "%m"),
        ("dddd", "%A"),
        ("ddd", "%a"),
        ("dd", "%d"),
        ("yyyy", "%Y"),
        ("yy", "%y"),
        ("HH", "%H"),
        ("hh", "%I"),
        ("MM", "%M"),
        ("ss", "%S"),
        ("TT", "%p"),
        ("tt", "%P"),
        ("h", "%l"),
    ] {
        replace_all(&mut format, from, to);
    }
    if year < 0 {
        return Err(error("printd", JsMessage::ValueError));
    }
    for (marker, value) in [
        (b'm', month),
        (b'd', day),
        (b'H', hour),
        (b'h', if hour > 12 { hour - 12 } else { hour }),
        (b'M', minute),
        (b's', second),
    ] {
        let replacement: Vec<_> = value.to_string().encode_utf16().collect();
        let mut offset = 0;
        while offset < format.len() {
            if format[offset] == u16::from(marker)
                && (offset == 0 || format[offset - 1] != b'%' as u16)
            {
                format.splice(offset..offset + 1, replacement.iter().copied());
                offset += replacement.len();
            } else {
                offset += 1;
            }
        }
    }
    let weekday = date::get_day_of_week_from_time(time) as usize;
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < format.len() {
        if format[offset] == b'%' as u16 && offset + 1 < format.len() {
            let directive = format[offset + 1];
            let value = match directive {
                66 => date::FULL_MONTHS[(month - 1) as usize].to_owned(),
                98 => date::MONTHS[(month - 1) as usize].to_owned(),
                65 => [
                    "Sunday",
                    "Monday",
                    "Tuesday",
                    "Wednesday",
                    "Thursday",
                    "Friday",
                    "Saturday",
                ][weekday]
                    .to_owned(),
                97 => ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][weekday].to_owned(),
                109 => format!("{month:02}"),
                100 => format!("{day:02}"),
                89 => format!("{year:04}"),
                121 => format!("{:02}", year % 100),
                72 => format!("{hour:02}"),
                73 => format!("{:02}", if hour % 12 == 0 { 12 } else { hour % 12 }),
                108 => format!("{:2}", if hour % 12 == 0 { 12 } else { hour % 12 }),
                77 => format!("{minute:02}"),
                83 => format!("{second:02}"),
                112 => if hour < 12 { "AM" } else { "PM" }.to_owned(),
                80 => if hour < 12 { "am" } else { "pm" }.to_owned(),
                _ => {
                    result.push(format[offset]);
                    offset += 1;
                    continue;
                }
            };
            result.extend(value.encode_utf16());
            offset += 2;
        } else {
            result.push(format[offset]);
            offset += 1;
        }
    }
    // Grow safely rather than retaining the native fixed 64-wchar buffer limit.
    Ok(JsString::from(result.as_slice()).into())
}

pub fn register_util(context: &mut Context) -> JsResult<()> {
    type Callback = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;
    let methods: [(&str, Callback); 5] = [
        ("printd", printd),
        ("printf", printf),
        ("printx", printx),
        ("scand", scand),
        ("byteToChar", byte_to_char),
    ];
    let functions: Vec<_> = methods
        .into_iter()
        .map(|(name, callback)| {
            let function =
                FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(callback))
                    .name(JsString::from(name))
                    .length(0)
                    .constructor(false)
                    .build();
            (JsString::from(name), function)
        })
        .collect();
    let mut object = ObjectInitializer::with_native_data(UtilBinding, context);
    for (name, function) in functions {
        object.property(
            name,
            function,
            Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
        );
    }
    let object = object.build();
    context.register_global_property(js_string!("util"), object, Attribute::all())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().collect()
    }
    fn context() -> Context {
        let mut context = Context::default();
        register_util(&mut context).unwrap();
        context
    }
    fn eval(context: &mut Context, source: &str) -> JsValue {
        context.eval(Source::from_bytes(source)).unwrap()
    }
    /// Every active input/assertion in CJSUtilTest.ParseDataType.
    #[test]
    fn parse_data_type_source_vectors() {
        let cases = [
            ("", DataType::Invalid),
            ("d", DataType::Invalid),
            ("%d", DataType::Int),
            ("%x", DataType::Int),
            ("%f", DataType::Double),
            ("%s", DataType::String),
            ("%+d", DataType::Int),
            ("%+x", DataType::Int),
            ("%+f", DataType::Double),
            ("% d", DataType::Int),
            ("% x", DataType::Int),
            ("% f", DataType::Double),
            ("%0d", DataType::Int),
            ("%0x", DataType::Int),
            ("%0f", DataType::Double),
            ("%#d", DataType::Int),
            ("%#x", DataType::Int),
            ("%#f", DataType::Double),
            ("%5d", DataType::Int),
            ("%05d", DataType::Int),
            ("%5x", DataType::Int),
            ("%05x", DataType::Int),
            ("%5f", DataType::Double),
            ("%05f", DataType::Double),
            ("%5s", DataType::String),
            ("%.5f", DataType::Double),
            ("%.14f", DataType::Double),
            ("%.1d", DataType::Int),
            ("%.10d", DataType::Int),
            ("%.100d", DataType::Invalid),
            ("%ad", DataType::Invalid),
            ("%bx", DataType::Invalid),
            ("%hx", DataType::Invalid),
            ("%js", DataType::Invalid),
            ("%@d", DataType::Invalid),
            ("%~x", DataType::Invalid),
            ("%[f", DataType::Invalid),
            ("%\0s", DataType::Invalid),
            ("%\nd", DataType::Invalid),
            ("%\rx", DataType::Invalid),
            ("%+6d", DataType::Int),
            ("% 7x", DataType::Int),
            ("%#9.3f", DataType::Double),
            ("%10s", DataType::String),
        ];
        for (format, expected) in cases {
            assert_eq!(parse_data_type(&mut wide(format)), expected, "{format:?}");
        }
        let mut format = wide("%s");
        assert_eq!(parse_data_type(&mut format), DataType::String);
        assert_eq!(format, wide("%S"));
        for format in ["%*d", "%.*f", "%d%d", "%%f"] {
            assert_eq!(parse_data_type(&mut wide(format)), DataType::Invalid);
        }
    }
    #[test]
    fn masks_filter_escape_and_change_case_without_losing_utf16() {
        // Source util_printx.in covers all mask instructions and scan behavior.
        for (format, source, expected) in [
            ("", "123", ""),
            ("??", "f27", "f2"),
            ("XXX", "-1Afp3.d33F$", "1Af"),
            ("AAA", "-1Afp3.d33F$", "Afp"),
            ("999", "-1Afp3.d33F$", "133"),
            ("[*]X", "-1Afp3.", "[-1Afp3.]"),
            ("<*", "-1Afp3.d33F$", "-1afp3.d33f$"),
            (">??????=*", "-1Afp3.d33F$", "-1AFP3.d33F$"),
            (r"cl\9ms", "-1Afp3.", "cl9ms"),
            (r"\>[\**]", "abc", ">[*abc]"),
        ] {
            assert_eq!(
                string_printx(&wide(format), &wide(source)),
                wide(expected),
                "{format}"
            );
        }
        assert_eq!(
            string_printx(&wide("?*"), &[0xd800, 0, 0xdc00]),
            vec![0xd800, 0, 0xdc00]
        );
    }
    #[test]
    fn printx_consumes_surrogate_pairs_as_one_mask_character() {
        assert_eq!(string_printx(&wide("?"), &wide("😀")), wide("😀"));
        assert_eq!(
            string_printx(&wide("? ?"), &[0xd83d, 0xde00, 0xd800]),
            vec![0xd83d, 0xde00, 32, 0xd800]
        );
        assert_eq!(
            string_printx(&wide("? ?"), &[0xd800, 0xd83d, 0xde00]),
            vec![0xd800, 32, 0xd83d, 0xde00]
        );
        // Case conversion also receives the complete supplementary character.
        assert_eq!(string_printx(&wide(">?"), &wide("𐐨")), wide("𐐀"));
        let mut context = context();
        assert_eq!(
            eval(&mut context, "util.printx('?', '😀')")
                .as_string()
                .unwrap()
                .to_vec(),
            wide("😀")
        );
    }
    #[test]
    fn printf_formats_safe_conversions_and_preserves_coercion_failures() {
        let mut context = context();
        assert_eq!(
            eval(
                &mut context,
                r#"util.printf('n=%+06d hex=%#x f=%.2f s=%5s',12,31,1.25,'ok')"#
            )
            .as_string()
            .unwrap(),
            js_string!("n=+00012 hex=0x1f f=1.25 s=   ok")
        );
        assert_eq!(
            eval(
                &mut context,
                r#"util.printf('%u %o %X %c %.3g %.2e',-1,9,31,65,1234,1.25)"#
            )
            .as_string()
            .unwrap(),
            js_string!("4294967295 11 1F A 1.23e+03 1.25e+00")
        );
        assert_eq!(
            eval(
                &mut context,
                r#"util.printf('Values = %0.100x .9999 %x',1,2)"#
            )
            .as_string()
            .unwrap(),
            js_string!("Values = %0.100x .9999 2")
        );
        assert_eq!(
            eval(&mut context, r#"util.printf('%d %s',7)"#)
                .as_string()
                .unwrap(),
            js_string!("7 %s")
        );
        assert_eq!(eval(&mut context, r#"let order=[]; util.printf('%d %s',{valueOf(){order.push(1);return 7}},{toString(){order.push(2);return 'x'}});order.join(',')"#).as_string().unwrap(), js_string!("1,2"));
        assert_eq!(
            eval(
                &mut context,
                r#"let sentinel={}; try{util.printf('%d',{valueOf(){throw sentinel}})}catch(e){e===sentinel}"#
            ),
            JsValue::from(true)
        );
        assert_eq!(
            eval(&mut context, r#"util.printf('%s','\ud800').charCodeAt(0)"#),
            JsValue::from(0xd800)
        );
    }
    #[test]
    fn pdf_dates_follow_boa_local_timezone_and_dst() {
        let mut context = context();
        super::super::cjs_publicmethods::register_public_methods(&mut context).unwrap();
        assert_eq!(
            eval(
                &mut context,
                r#"
            (() => {
                const format='yyyy-mm-dd HH:MM:ss';
                // Winter/summer, both DST transitions in Europe and America,
                // pre-epoch times and leap dates use the same path as JS Date.
                for (const parts of [
                    [2024,0,15,12,34,56], [2024,6,15,12,34,56],
                    [2024,2,31,2,30,0], [2024,9,27,2,30,0],
                    [2024,2,10,2,30,0], [2024,10,3,1,30,0],
                    [1969,11,31,23,59,59], [2024,1,29,0,0,0]
                ]) {
                    const [y,m,d,h,min,s]=parts;
                    const pad=n=>String(n).padStart(2,'0');
                    const text=`${y}-${pad(m+1)}-${pad(d)} ${pad(h)}:${pad(min)}:${pad(s)}`;
                    const expected=new Date(...parts);
                    const actual=util.scand(format,text);
                    if (+actual !== +expected) throw 'local Date construction: '+text;
                    const display=`${expected.getFullYear()}-${pad(expected.getMonth()+1)}-${pad(expected.getDate())} ${pad(expected.getHours())}:${pad(expected.getMinutes())}:${pad(expected.getSeconds())}`;
                    if (util.printd(format,actual) !== display) throw 'local Date formatting: '+text;
                    // AFParseDateEx returns the source helper's local wall time.
                    const wall=Date.UTC(y,m,d,h,min,s);
                    if (AFParseDateEx(text,format) !== wall) throw 'AF local date parse: '+text;
                }
                const before=Date.now(), empty=+util.scand(format,''), after=Date.now();
                if (empty < before || empty > after) throw 'current Date clock';
                return true;
            })()
        "#
            ),
            JsValue::from(true)
        );
        // The test runner selects these process timezones; never mutate TZ
        // from a library or concurrent Rust test.
        let expected = match std::env::var("TZ").as_deref() {
            Ok("Europe/Berlin") => Some("-60,-120"),
            Ok("America/New_York") => Some("300,240"),
            Ok("UTC") => Some("0,0"),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(eval(&mut context,
                "[new Date(2024,0,15).getTimezoneOffset(),new Date(2024,6,15).getTimezoneOffset()].join(',')")
                .as_string().unwrap(), JsString::from(expected));
        }
    }
    #[test]
    fn date_methods_and_byte_conversion_use_real_javascript_values() {
        let mut context = context();
        assert_eq!(
            eval(
                &mut context,
                r#"util.printd('mmmm d, yyyy dddd HH:MM:ss tt',new Date(2014,6,4,15,59,58))"#
            )
            .as_string()
            .unwrap(),
            js_string!("July 4, 2014 Friday 15:59:58 pm")
        );
        assert_eq!(
            eval(
                &mut context,
                r#"JSON.stringify([0,1,2].map(n=>util.printd(n,new Date(2014,6,4,15,59,58))))"#
            )
            .as_string()
            .unwrap(),
            js_string!(r#"["D:20140704155958","2014.07.04 15:59:58","2014/07/04 15:59:58"]"#)
        );
        assert_eq!(
            eval(
                &mut context,
                r#"util.printd(1,util.scand('yyyy/mm/dd HH:MM:ss','2016/12/20 11:22:03'))"#
            )
            .as_string()
            .unwrap(),
            js_string!("2016.12.20 11:22:03")
        );
        assert_eq!(
            eval(
                &mut context,
                r#"JSON.stringify([0,65,127,128,255].map(n=>util.byteToChar(n).charCodeAt(0)))"#
            )
            .as_string()
            .unwrap(),
            js_string!("[0,65,127,128,255]")
        );
        for script in [
            "util.byteToChar(-1)",
            "util.byteToChar(256)",
            "util.printd('mm',0)",
            "util.printd('mm',new Date(NaN))",
            "util.printd('mm',new Date(),true)",
            "util.printf()",
            "util.scand()",
            "util.printx('99')",
        ] {
            assert!(
                context.eval(Source::from_bytes(script)).is_err(),
                "{script}"
            );
        }
        assert_eq!(
            eval(
                &mut context,
                r#"try{util.printd('mm',new Date(NaN))}catch(e){typeof e+':'+e}"#
            )
            .as_string()
            .unwrap(),
            js_string!("string:util.printd: The second parameter is an invalid Date.")
        );
        assert_eq!(
            eval(
                &mut context,
                r#"try{util.byteToChar.call([],65)}catch(e){e instanceof TypeError}"#
            ),
            JsValue::from(true)
        );
        assert_eq!(
            eval(
                &mut context,
                r#"let d=Object.getOwnPropertyDescriptor(util,'printf');!d.writable&&d.enumerable&&d.configurable&&!('prototype' in util.printf)"#
            ),
            JsValue::from(true)
        );
    }
}
