// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Acrobat form helpers from `fxjs/cjs_publicmethods.{h,cpp}`. All AF globals
//! execute on Boa values; event strings preserve UTF-16 and host callbacks
//! release the active engine context before entering the form graph.
use super::cfxjs_engine::CfxJsEngine;
use super::cjs_color::{convert_array_to_pwl_color, convert_pwl_color_to_array};
use super::cjs_event_context::{wide_from_utf16, CjsEventContext};
use super::cjs_field::maybe_coerce_to_number;
use super::cjs_runtime::CjsRuntime;
use super::cjs_util::string_printx;
use super::fx_date_helpers as date;
use super::js_date::{current_date_time, date_parse, JsDateCharacters};
use super::js_resources::{js_format_error_string, js_get_string_from_id, JsMessage};
use crate::fpdfdoc::cpdf_formfield::FormFieldType;
use crate::fxcrt::fx_string::{string_to_double_wide, string_to_float_wide};
use crate::fxge::cfx_color::CfxColor;
use boa_engine::{
    object::{builtins::JsArray, FunctionObjectBuilder},
    property::Attribute,
    Context, JsError, JsResult, JsString, JsValue, NativeFunction,
};
use std::rc::Rc;

const DATE_FORMATS: [&str; 14] = [
    "m/d",
    "m/d/yy",
    "mm/dd/yy",
    "mm/yy",
    "d-mmm",
    "d-mmm-yy",
    "dd-mmm-yy",
    "yy-mm-dd",
    "mmm-yy",
    "mmmm-yy",
    "mmm d, yyyy",
    "mmmm d, yyyy",
    "m/d/yy h:MM tt",
    "m/d/yy HH:MM",
];
const TIME_FORMATS: [&str; 4] = ["HH:MM", "h:MM tt", "HH:MM:ss", "h:MM:ss tt"];
type Method = fn(&mut Context, &[JsValue], &str) -> JsResult<JsValue>;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn js_string(text: &[u16]) -> JsValue {
    JsString::from(text).into()
}
fn success() -> JsResult<JsValue> {
    Ok(JsValue::undefined())
}
fn failure_details(name: &str, details: &[u16]) -> JsError {
    let message = js_format_error_string(name, None, details);
    JsError::from_opaque(JsString::from(message.as_slice()).into())
}
fn failure(name: &str, message: JsMessage) -> JsError {
    failure_details(name, &js_get_string_from_id(message))
}
fn event(context: &Context, name: &str) -> JsResult<Rc<CjsEventContext>> {
    CjsRuntime::from_context(context)
        .and_then(|runtime| runtime.current_event_context())
        .filter(|event| event.is_valid())
        .ok_or_else(|| failure(name, JsMessage::BadObjectError))
}
fn same_event(context: &Context, expected: &Rc<CjsEventContext>, name: &str) -> JsResult<()> {
    if Rc::ptr_eq(&event(context, name)?, expected) {
        Ok(())
    } else {
        Err(failure(name, JsMessage::BadObjectError))
    }
}
fn value(event: &CjsEventContext, name: &str) -> JsResult<super::cjs_event_context::SharedString> {
    event
        .value()
        .ok_or_else(|| failure(name, JsMessage::BadObjectError))
}
fn trim_spaces(text: &[u16]) -> &[u16] {
    let start = text.iter().position(|&c| c != 32).unwrap_or(text.len());
    let end = text.iter().rposition(|&c| c != 32).map_or(start, |i| i + 1);
    &text[start..end]
}
fn normalize_decimal_mark(text: &mut [u16]) {
    for c in text {
        if *c == u16::from(b',') {
            *c = u16::from(b'.');
        }
    }
}
fn number(text: &[u16]) -> f64 {
    // These callers use C atof, unlike the calculation helpers' deliberately
    // more permissive StringToDouble. Skip C whitespace and only one sign.
    let start = text
        .iter()
        .position(|&c| !matches!(c, 9..=13 | 32))
        .unwrap_or(text.len());
    let mut text = &text[start..];
    let negative = text.first() == Some(&45);
    if matches!(text.first(), Some(43 | 45)) {
        text = &text[1..];
    }
    if !matches!(text.first(), Some(48..=57 | 46 | 73 | 105 | 78 | 110)) {
        return 0.0;
    }
    let value = string_to_double_wide(&wide_from_utf16(text));
    if negative {
        -value
    } else {
        value
    }
}
fn valid_style(style: i32) -> i32 {
    if (0..4).contains(&style) {
        style
    } else {
        0
    }
}
fn decimal_mark(style: i32) -> u16 {
    u16::from(if style == 2 || style == 3 { b',' } else { b'.' })
}
fn merged(event: &CjsEventContext, value: &[u16], change: &[u16]) -> Vec<u16> {
    // WideString::First tolerates an oversized (including negative) count.
    let start = usize::try_from(event.sel_start())
        .unwrap_or(usize::MAX)
        .min(value.len());
    let mut result = Vec::with_capacity(value.len().saturating_add(change.len()));
    result.extend_from_slice(&value[..start]);
    result.extend_from_slice(change);
    if let Ok(end) = usize::try_from(event.sel_end()) {
        if let Some(postfix) = value.get(end..) {
            result.extend_from_slice(postfix);
        }
    }
    result
}
fn alert_if_possible(
    context: &mut Context,
    event: &CjsEventContext,
    caller: &str,
    message: &[u16],
) {
    if let Some(environment) = event.environment() {
        let message = wide_from_utf16(message);
        let caller: Vec<u32> = caller.chars().map(u32::from).collect();
        CfxJsEngine::with_host_callback(context, || {
            environment.js_app_alert(&message, &caller, 0, 3)
        });
    }
}
fn format_message(message: JsMessage, arguments: &[&[u16]]) -> Vec<u16> {
    let template = js_get_string_from_id(message);
    let mut result = Vec::new();
    let mut offset = 0;
    let mut argument = 0;
    while offset < template.len() {
        if template.get(offset..offset + 3) == Some(&[37, 108, 115]) && argument < arguments.len() {
            result.extend_from_slice(arguments[argument]);
            argument += 1;
            offset += 3;
        } else {
            result.push(template[offset]);
            offset += 1;
        }
    }
    result
}

/// Source IsNumber permits a decimal mark and an explicitly signed exponent.
pub fn is_number(text: &[u16]) -> bool {
    let text = trim_spaces(text);
    let mut dot = false;
    let mut exponent = false;
    let mut index = 0;
    while index < text.len() && text[index] != 0 {
        match text[index] {
            46 | 44 => {
                if dot {
                    return false;
                }
                dot = true;
            }
            45 | 43 => {
                if index != 0 {
                    return false;
                }
            }
            101 | 69 => {
                if exponent {
                    return false;
                }
                index += 1;
                if !matches!(text.get(index), Some(43 | 45)) {
                    return false;
                }
                exponent = true;
            }
            48..=57 => {}
            _ => return false,
        }
        index += 1;
    }
    true
}
pub fn mask_satisfied(change: u16, mask: u16) -> bool {
    match mask {
        57 => (48..=57).contains(&change),
        65 => u8::try_from(change).is_ok_and(|c| c.is_ascii_alphabetic()),
        79 => u8::try_from(change).is_ok_and(|c| c.is_ascii_alphanumeric()),
        88 => true,
        _ => change == mask,
    }
}
pub fn is_reserved_mask_char(c: u16) -> bool {
    matches!(c, 57 | 65 | 79 | 88)
}

pub fn parse_date(context: &mut Context, text: &[u16], wrong: Option<&mut bool>) -> JsResult<f64> {
    let now = current_date_time(context);
    let (mut year, mut month, mut day) = (
        date::get_year_from_time(now),
        date::get_month_from_time(now) + 1,
        date::get_day_from_time(now),
    );
    let mut numbers = Vec::with_capacity(3);
    let source = wide_from_utf16(text);
    let mut index = 0;
    while index < source.len() && numbers.len() < 3 {
        if (48..=57).contains(&source[index]) {
            let mut skip = 0;
            numbers.push(date::parse_string_integer(&source, index, &mut skip, 4));
            index += skip;
        } else {
            index += 1;
        }
    }
    let bad = !matches!(numbers.len(), 2 | 3);
    if let Some(wrong) = wrong {
        *wrong = bad;
    }
    if bad {
        return Ok(now);
    }
    if numbers.len() == 2 {
        if date::is_valid_month(numbers[0]) && date::is_valid_day(numbers[1]) {
            month = numbers[0];
            day = numbers[1];
        } else if date::is_valid_day(numbers[0]) && date::is_valid_month(numbers[1]) {
            day = numbers[0];
            month = numbers[1];
        }
    } else if numbers[0] > 12 && date::is_valid_month(numbers[1]) && date::is_valid_day(numbers[2])
    {
        year = numbers[0];
        month = numbers[1];
        day = numbers[2];
    } else if date::is_valid_month(numbers[0]) && date::is_valid_day(numbers[1]) && numbers[2] > 31
    {
        month = numbers[0];
        day = numbers[1];
        year = numbers[2];
    } else if date::is_valid_day(numbers[0]) && date::is_valid_month(numbers[1]) && numbers[2] > 31
    {
        day = numbers[0];
        month = numbers[1];
        year = numbers[2];
    }
    if (0..100).contains(&year) {
        year += if year < 50 { 2000 } else { 1900 };
    }
    date_parse(
        context,
        &wide(&format!(
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
            date::get_hour_from_time(now),
            date::get_min_from_time(now),
            date::get_sec_from_time(now)
        )),
    )
}
pub fn parse_date_using_format(
    context: &mut Context,
    text: &[u16],
    format: &[u16],
    wrong: Option<&mut bool>,
) -> JsResult<f64> {
    let mut result = f64::NAN;
    let status = date::parse_date_using_format_at(
        &wide_from_utf16(text),
        &wide_from_utf16(format),
        &mut result,
        current_date_time(context),
        &JsDateCharacters,
    );
    if status == date::ConversionStatus::Success {
        return Ok(result);
    }
    if status == date::ConversionStatus::BadDate {
        result = date_parse(context, text)?;
        if !result.is_nan() {
            return Ok(result);
        }
    }
    parse_date(context, text, wrong)
}
pub fn parse_date_as_gmt(context: &mut Context, text: &[u16]) -> JsResult<f64> {
    let parts: Vec<&[u16]> = text.split(|&c| c == 32 || c == 58).collect();
    if parts.len() != 8 {
        return Ok(0.0);
    }
    let month = date::MONTHS
        .iter()
        .position(|name| parts[1] == wide(name))
        .map_or(1, |i| i as i32 + 1);
    // Source StringToFloat narrows these components before integer conversion.
    let part = |i: usize| string_to_float_wide(&wide_from_utf16(parts[i])) as i32;
    let year = part(7);
    if !(-271_820..=275_760).contains(&year) {
        return date_parse(context, text);
    }
    let result = date::make_date(
        date::make_day(year, month - 1, part(2)),
        date::make_time(part(3), part(4), part(5), 0),
    );
    if result.is_nan() {
        date_parse(context, text)
    } else {
        Ok(result)
    }
}
pub fn print_date_using_format(time: f64, format: &[u16]) -> Vec<u16> {
    // Avoid the native float-to-int undefined behavior for invalid Date values.
    if !time.is_finite() || time.abs() > 8.64e15 {
        return Vec::new();
    }
    let year = date::get_year_from_time(time);
    let month = date::get_month_from_time(time) + 1;
    let day = date::get_day_from_time(time);
    let hour = date::get_hour_from_time(time);
    let minute = date::get_min_from_time(time);
    let second = date::get_sec_from_time(time);
    let mut result = Vec::new();
    let mut index = 0;
    while index < format.len() {
        let c = format[index];
        if !matches!(c, 121 | 109 | 100 | 72 | 104 | 77 | 115 | 116) {
            result.push(c);
            index += 1;
            continue;
        }
        let run = format[index..]
            .iter()
            .take(5)
            .take_while(|&&v| v == c)
            .count();
        let count = if run > 4 { 1 } else { run };
        let part = match (c, count) {
            (121, 2) => wide(&format!("{:02}", year % 100)),
            (121, 4) => wide(&format!("{year:04}")),
            (109, 3) if date::is_valid_month(month) => wide(date::MONTHS[(month - 1) as usize]),
            (109, 4) if date::is_valid_month(month) => {
                wide(date::FULL_MONTHS[(month - 1) as usize])
            }
            (116, 1) => wide(if hour > 12 { "p" } else { "a" }),
            (116, 2) => wide(if hour > 12 { "pm" } else { "am" }),
            (109 | 100 | 72 | 104 | 77 | 115, 1 | 2) => {
                let value = match c {
                    109 => month,
                    100 => day,
                    72 => hour,
                    104 => {
                        if hour > 12 {
                            hour - 12
                        } else {
                            hour
                        }
                    }
                    77 => minute,
                    _ => second,
                };
                wide(&if count == 1 {
                    value.to_string()
                } else {
                    format!("{value:02}")
                })
            }
            _ => vec![c; count],
        };
        result.extend(part);
        index += count;
    }
    result
}

fn grouped(mut text: String, style: i32, apostrophe: bool, negative: bool) -> String {
    let point = text.find('.').unwrap_or(text.len());
    if decimal_mark(style) == 44 && point < text.len() {
        text.replace_range(point..point + 1, ",");
    }
    let separator = match style {
        0 => Some(','),
        2 => Some('.'),
        4.. if apostrophe => Some('\''),
        _ => None,
    };
    if let Some(separator) = separator {
        let stop = usize::from(negative);
        let mut position = point;
        while position > stop + 3 {
            position -= 3;
            text.insert(position, separator);
        }
    }
    text
}
fn af_number_format(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 6 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let event = event(context, name)?;
    let channel = event
        .value()
        .ok_or_else(|| failure_details(name, &wide("No event handler")))?;
    let mut text = trim_spaces(&channel.borrow()).to_vec();
    if text.is_empty() {
        return success();
    }
    let decimals = args[0].to_i32(context)?.unsigned_abs().min(15) as usize;
    let separator = valid_style(args[1].to_i32(context)?);
    let negative_style = valid_style(args[2].to_i32(context)?);
    // Source currStyle (argument 3) is intentionally unused and uncoerced.
    let currency = args[4].to_string(context)?.to_vec();
    let prepend = args[5].to_boolean();
    same_event(context, &event, name)?;
    normalize_decimal_mark(&mut text);
    let mut value = number(&text);
    if decimals > 0 {
        value += 1.0e-15;
    }
    let negative = value < 0.0;
    let formatted = grouped(
        format!("{:.*}", decimals, value.abs()),
        separator,
        false,
        false,
    );
    let mut result = Vec::with_capacity(formatted.len() + currency.len() + 2);
    if negative {
        if negative_style == 0 {
            result.push(45);
        } else if negative_style == 2 || negative_style == 3 {
            result.push(40);
        }
    }
    if prepend {
        result.extend_from_slice(&currency);
    }
    result.extend(formatted.encode_utf16());
    if !prepend {
        result.extend_from_slice(&currency);
    }
    if negative && matches!(negative_style, 2 | 3) {
        result.push(41);
    }
    *channel.borrow_mut() = result;
    if matches!(negative_style, 1 | 3) {
        if let Ok(reference) = event.target_field() {
            let target = CjsRuntime::field_object(context, reference)?;
            let color = CfxColor::make_rgb(if negative { 1.0 } else { 0.0 }, 0.0, 0.0);
            let array = convert_pwl_color_to_array(context, color);
            let needs_update = if negative {
                true
            } else {
                let old = target.get(JsString::from("textColor"), context)?;
                let old = old
                    .as_object()
                    .ok_or_else(|| failure(name, JsMessage::ObjectTypeError))?;
                convert_array_to_pwl_color(context, &old)? != color
            };
            same_event(context, &event, name)?;
            if needs_update {
                target.set(JsString::from("textColor"), array, true, context)?;
            }
        }
    }
    success()
}
fn af_number_keystroke(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() < 2 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let event = event(context, name)?;
    let channel = value(&event, name)?;
    let text = channel.borrow().clone();
    if event.will_commit() {
        let mut trimmed = trim_spaces(&text).to_vec();
        if trimmed.is_empty() {
            return success();
        }
        normalize_decimal_mark(&mut trimmed);
        if !is_number(&trimmed) {
            event.rc().set(false);
            let message = js_get_string_from_id(JsMessage::InvalidInputError);
            alert_if_possible(context, &event, "AFNumber_Keystroke", &message);
            return Err(failure_details(name, &message));
        }
        return success();
    }
    let selected = if let (Ok(start), Ok(end)) = (
        usize::try_from(event.sel_start()),
        usize::try_from(event.sel_end()),
    ) {
        text.get(start..end).unwrap_or(&[])
    } else {
        &[]
    };
    let mut sign = text.contains(&45) && !selected.contains(&45);
    if sign && !selected.is_empty() && event.sel_start() == 0 {
        event.rc().set(false);
        return success();
    }
    let separator = decimal_mark(valid_style(args[1].to_i32(context)?));
    same_event(context, &event, name)?;
    let change = event.change();
    let change = change.borrow();
    let mut has_separator = text.contains(&separator);
    for (index, &c) in change.iter().enumerate() {
        if c == separator {
            if has_separator {
                event.rc().set(false);
                return success();
            }
            has_separator = true;
        } else if c == 45 {
            if sign || index != 0 || event.sel_start() != 0 {
                event.rc().set(false);
                return success();
            }
            sign = true;
        } else if !(48..=57).contains(&c) {
            event.rc().set(false);
            return success();
        }
    }
    *channel.borrow_mut() = merged(&event, &text, &change);
    success()
}
fn af_percent_format(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() < 2 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let event = event(context, name)?;
    let channel = value(&event, name)?;
    let decimals = args[0].to_i32(context)?;
    let separator = args[1].to_i32(context)?;
    let prepend = args.get(2).is_some_and(JsValue::to_boolean);
    same_event(context, &event, name)?;
    if decimals < 0 || !(0..=49).contains(&separator) {
        return Err(failure(name, JsMessage::ValueError));
    }
    if decimals > 512 {
        *channel.borrow_mut() = wide("%");
        return success();
    }
    let number = number(trim_spaces(&channel.borrow())) * 100.0;
    // Rust formatting avoids the source sizing loop's nontermination on infinity.
    let formatted = format!("{:.*}", decimals as usize, number);
    let formatted = grouped(formatted, separator, true, number < 0.0);
    *channel.borrow_mut() = wide(&if prepend {
        format!("%{formatted}")
    } else {
        format!("{formatted}%")
    });
    success()
}
fn af_date_format_ex(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let event = event(context, name)?;
    let channel = value(&event, name)?;
    let text = channel.borrow().clone();
    if text.is_empty() {
        return success();
    }
    let format = args[0].to_string(context)?.to_vec();
    let time = if text.windows(3).any(|w| w == [71, 77, 84]) {
        parse_date_as_gmt(context, &text)?
    } else {
        parse_date_using_format(context, &text, &format, None)?
    };
    same_event(context, &event, name)?;
    if time.is_nan() {
        let message = format_message(JsMessage::ParseDateError, &[&format]);
        alert_if_possible(context, &event, "AFDate_FormatEx", &message);
        return Err(failure(name, JsMessage::ParseDateError));
    }
    *channel.borrow_mut() = print_date_using_format(time, &format);
    success()
}
fn af_date_keystroke_ex(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(failure_details(
            name,
            &wide("AFDate_KeystrokeEx's parameter size not correct"),
        ));
    }
    let event = event(context, name)?;
    if !event.will_commit() {
        return success();
    }
    let channel = value(&event, name)?;
    let text = channel.borrow().clone();
    if text.is_empty() {
        return success();
    }
    let format = args[0].to_string(context)?.to_vec();
    let mut wrong = false;
    let time = parse_date_using_format(context, &text, &format, Some(&mut wrong))?;
    same_event(context, &event, name)?;
    if wrong || time.is_nan() {
        let message = format_message(JsMessage::ParseDateError, &[&format]);
        alert_if_possible(context, &event, "AFDate_KeystrokeEx", &message);
        event.rc().set(false);
    }
    success()
}
fn indexed_date(
    context: &mut Context,
    args: &[JsValue],
    name: &str,
    formats: &[&str],
    method: Method,
) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let index = args[0].to_i32(context)?;
    let index = usize::try_from(index)
        .ok()
        .filter(|&i| i < formats.len())
        .unwrap_or(0);
    method(context, &[JsString::from(formats[index]).into()], name)
}
fn af_date_format(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    indexed_date(context, args, name, &DATE_FORMATS, af_date_format_ex)
}
fn af_date_keystroke(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    indexed_date(context, args, name, &DATE_FORMATS, af_date_keystroke_ex)
}
fn af_time_format(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    indexed_date(context, args, name, &TIME_FORMATS, af_date_format_ex)
}
fn af_time_keystroke(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    indexed_date(context, args, name, &TIME_FORMATS, af_date_keystroke_ex)
}

fn af_special_format(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let event = event(context, name)?;
    let channel = value(&event, name)?;
    let style = args[0].to_i32(context)?;
    same_event(context, &event, name)?;
    let source = channel.borrow();
    let format = match style {
        0 => "99999",
        1 => "99999-9999",
        2 => {
            if string_printx(&wide("9999999999"), &source).len() >= 10 {
                "(999) 999-9999"
            } else {
                "999-9999"
            }
        }
        3 => "999-99-9999",
        _ => "",
    };
    let result = string_printx(&wide(format), &source);
    drop(source);
    *channel.borrow_mut() = result;
    success()
}
fn af_special_keystroke_ex(
    context: &mut Context,
    args: &[JsValue],
    name: &str,
) -> JsResult<JsValue> {
    if args.is_empty() {
        return Err(failure(name, JsMessage::ParamError));
    }
    let event = event(context, name)?;
    let channel = value(&event, name)?;
    let mask = args[0].to_string(context)?.to_vec();
    same_event(context, &event, name)?;
    if mask.is_empty() {
        return success();
    }
    if event.will_commit() {
        let text = channel.borrow();
        if text.is_empty() {
            return success();
        }
        let error = if text.len() > mask.len() {
            Some(JsMessage::ParamTooLongError)
        } else if text.len() != mask.len()
            || !text.iter().zip(&mask).all(|(&c, &m)| mask_satisfied(c, m))
        {
            Some(JsMessage::InvalidInputError)
        } else {
            None
        };
        drop(text);
        if let Some(error) = error {
            alert_if_possible(
                context,
                &event,
                "AFSpecial_KeystrokeEx",
                &js_get_string_from_id(error),
            );
            event.rc().set(false);
        }
        return success();
    }
    let change_channel = event.change();
    let mut change = change_channel.borrow().clone();
    if change.is_empty() {
        return success();
    }
    // Checked signed lengths replace the native unsigned wraparound.
    let start = usize::try_from(event.sel_start()).ok();
    let end = usize::try_from(event.sel_end()).ok();
    let combined = start.zip(end).and_then(|(start, end)| {
        channel
            .borrow()
            .len()
            .checked_add(change.len())?
            .checked_add(start)?
            .checked_sub(end)
    });
    let mut index = start.unwrap_or(usize::MAX);
    if combined.is_none_or(|length| length > mask.len())
        || index >= mask.len()
        || change.len() > mask.len() - index
    {
        alert_if_possible(
            context,
            &event,
            "AFSpecial_KeystrokeEx",
            &js_get_string_from_id(JsMessage::ParamTooLongError),
        );
        event.rc().set(false);
        return success();
    }
    for c in &mut change {
        let mask = mask[index];
        if !is_reserved_mask_char(mask) {
            *c = mask;
        }
        if !mask_satisfied(*c, mask) {
            event.rc().set(false);
            return success();
        }
        index += 1;
    }
    *change_channel.borrow_mut() = change;
    success()
}
fn af_special_keystroke(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let event = event(context, name)?;
    let channel = value(&event, name)?;
    let style = args[0].to_i32(context)?;
    same_event(context, &event, name)?;
    let format = match style {
        0 => "99999",
        1 | 3 => "999999999",
        2 => {
            if channel
                .borrow()
                .len()
                .saturating_add(event.change().borrow().len())
                > 7
            {
                "9999999999"
            } else {
                "9999999"
            }
        }
        _ => "",
    };
    af_special_keystroke_ex(context, &[JsString::from(format).into()], name)
}
fn af_merge_change(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let event = event(context, name)?;
    let text = event
        .value()
        .map_or_else(Vec::new, |value| value.borrow().clone());
    if event.will_commit() {
        Ok(js_string(&text))
    } else {
        Ok(js_string(&merged(&event, &text, &event.change().borrow())))
    }
}
fn af_parse_date_ex(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 2 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let text = args[0].to_string(context)?.to_vec();
    let format = args[1].to_string(context)?.to_vec();
    let time = parse_date_using_format(context, &text, &format, None)?;
    if time.is_nan() {
        let event = event(context, name)?;
        alert_if_possible(
            context,
            &event,
            "AFParseDateEx",
            &format_message(JsMessage::ParseDateError, &[&format]),
        );
        return Err(failure(name, JsMessage::ParseDateError));
    }
    Ok(time.into())
}
fn equals_no_case(text: &[u16], ascii: &str) -> bool {
    text.len() == ascii.len()
        && text
            .iter()
            .zip(ascii.bytes())
            .all(|(&a, b)| u8::try_from(a).is_ok_and(|a| a.eq_ignore_ascii_case(&b)))
}
fn apply_operation(function: &[u16], left: f64, right: f64) -> Option<f64> {
    Some(
        if equals_no_case(function, "AVG") || equals_no_case(function, "SUM") {
            left + right
        } else if equals_no_case(function, "PRD") {
            left * right
        } else if equals_no_case(function, "MIN") {
            if right < left {
                right
            } else {
                left
            }
        } else if equals_no_case(function, "MAX") {
            if left < right {
                right
            } else {
                left
            }
        } else {
            return None;
        },
    )
}
fn af_simple(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 3 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let function = args[0].to_string(context)?.to_vec();
    let left = args[1].to_number(context)?;
    let right = args[2].to_number(context)?;
    if left.is_nan() || right.is_nan() {
        return Err(failure(name, JsMessage::ValueError));
    }
    let mut result = apply_operation(&function, left, right)
        .ok_or_else(|| failure(name, JsMessage::ValueError))?;
    if function == wide("AVG") {
        result /= 2.0;
    }
    Ok(result.into())
}
fn af_make_number(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let mut text = args[0].to_string(context)?.to_vec();
    normalize_decimal_mark(&mut text);
    let converted = maybe_coerce_to_number(js_string(&text), context);
    Ok(if converted.is_number() {
        converted
    } else {
        0.into()
    })
}
fn make_array_from_list(context: &mut Context, value: &JsValue) -> JsResult<JsArray> {
    if let Some(object) = value.as_object().filter(|object| object.is_array()) {
        return JsArray::from_object(object.clone());
    }
    let text = value.to_string(context)?.to_vec();
    let mut parts = Vec::new();
    let mut offset = 0;
    while offset < text.len() && text[offset] != 0 {
        let end = text[offset..]
            .iter()
            .position(|&c| c == 44 || c == 0)
            .map_or(text.len(), |i| i + offset);
        parts.push(js_string(trim_spaces(&text[offset..end])));
        if text.get(end) != Some(&44) {
            break;
        }
        offset = end + 1;
    }
    Ok(JsArray::from_iter(parts, context))
}
fn af_simple_calculate(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 2
        || !(args[1].is_string() || args[1].as_object().is_some_and(|o| o.is_array()))
    {
        return Err(failure(name, JsMessage::ParamError));
    }
    let function = args[0].to_string(context)?.to_vec();
    let fields = make_array_from_list(context, &args[1])?;
    let runtime = CjsRuntime::from_context(context)
        .ok_or_else(|| failure(name, JsMessage::BadObjectError))?;
    let environment = runtime
        .environment()
        .ok_or_else(|| failure(name, JsMessage::BadObjectError))?;
    let mut result = if function == wide("PRD") { 1.0 } else { 0.0 };
    let mut count = 0usize;
    let mut index = 0;
    while index < fields.length(context)? {
        let field_name = fields.get(index, context)?.to_string(context)?.to_vec();
        let field_name = wide_from_utf16(&field_name);
        let values = CfxJsEngine::with_host_callback(context, || {
            if !environment.is_alive() {
                return Err(failure(name, JsMessage::BadObjectError));
            }
            let form = environment.interactive_form().core_form();
            let mut values = Vec::new();
            for j in 0..form.count_fields(&field_name) {
                let Some(field) = form.get_field(j, &field_name) else {
                    continue;
                };
                let trim = |text: Vec<u32>| {
                    let start = text
                        .iter()
                        .position(|&c| char::from_u32(c).is_none_or(|c| !c.is_whitespace()))
                        .unwrap_or(text.len());
                    let end = text
                        .iter()
                        .rposition(|&c| char::from_u32(c).is_none_or(|c| !c.is_whitespace()))
                        .map_or(start, |i| i + 1);
                    string_to_double_wide(&text[start..end])
                };
                let temp = match field.field_type() {
                    FormFieldType::TextField | FormFieldType::ComboBox => trim(field.value()),
                    FormFieldType::CheckBox | FormFieldType::RadioButton => {
                        let mut value = 0.0;
                        for control in 0..field.count_controls() {
                            if let Some(control) =
                                field.get_control(control).filter(|c| c.is_checked())
                            {
                                value = f64::from(string_to_float_wide(&control.export_value()));
                                break;
                            }
                        }
                        value
                    }
                    FormFieldType::ListBox if field.count_selected_items() <= 1 => {
                        f64::from(trim(field.value()) as f32)
                    }
                    _ => 0.0,
                };
                values.push((j, temp));
            }
            Ok(values)
        })?;
        for (j, temp) in values {
            if index == 0 && j == 0 && (function == wide("MIN") || function == wide("MAX")) {
                result = temp;
            }
            result = apply_operation(&function, result, temp)
                .ok_or_else(|| failure(name, JsMessage::ValueError))?;
            count += 1;
        }
        index += 1;
    }
    if function == wide("AVG") && count > 0 {
        result /= count as f64;
    }
    result = (result * 1_000_000.0 + 0.49).floor() / 1_000_000.0;
    let result = JsValue::from(result).to_string(context)?.to_vec();
    let event = event(context, name)?;
    if let Some(channel) = event.value() {
        *channel.borrow_mut() = result;
    }
    success()
}
fn af_range_validate(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 4 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let event = event(context, name)?;
    let channel = value(&event, name)?;
    if channel.borrow().is_empty() {
        return success();
    }
    let value = number(&channel.borrow());
    let lower = args[0].to_boolean();
    let lower_value = args[1].to_number(context)?;
    let upper = args[2].to_boolean();
    let upper_value = args[3].to_number(context)?;
    let message = if lower && upper && (value < lower_value || value > upper_value) {
        let a = args[1].to_string(context)?.to_vec();
        let b = args[3].to_string(context)?.to_vec();
        format_message(JsMessage::RangeBetweenError, &[&a, &b])
    } else if lower && !upper && value < lower_value {
        let a = args[1].to_string(context)?.to_vec();
        format_message(JsMessage::RangeGreaterError, &[&a])
    } else if upper && !lower && value > upper_value {
        let b = args[3].to_string(context)?.to_vec();
        format_message(JsMessage::RangeLessError, &[&b])
    } else {
        Vec::new()
    };
    same_event(context, &event, name)?;
    if !message.is_empty() {
        alert_if_possible(context, &event, "AFRange_Validate", &message);
        event.rc().set(false);
    }
    success()
}
fn af_extract_nums(context: &mut Context, args: &[JsValue], name: &str) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(failure(name, JsMessage::ParamError));
    }
    let mut text = args[0].to_string(context)?.to_vec();
    if matches!(text.first(), Some(46 | 44)) {
        text.insert(0, 48);
    }
    let mut numbers = Vec::new();
    let mut part = Vec::new();
    for c in text {
        if (48..=57).contains(&c) {
            part.push(c);
        } else if !part.is_empty() {
            numbers.push(js_string(&part));
            part.clear();
        }
    }
    if !part.is_empty() {
        numbers.push(js_string(&part));
    }
    if numbers.is_empty() {
        success()
    } else {
        Ok(JsArray::from_iter(numbers, context).into())
    }
}

macro_rules! global_method {
    ($callback:ident, $method:ident, $name:literal) => {
        fn $callback(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            $method(context, args, $name)
        }
    };
}
global_method!(number_format_callback, af_number_format, "AFNumber_Format");
global_method!(
    number_keystroke_callback,
    af_number_keystroke,
    "AFNumber_Keystroke"
);
global_method!(
    percent_format_callback,
    af_percent_format,
    "AFPercent_Format"
);
global_method!(
    percent_keystroke_callback,
    af_number_keystroke,
    "AFPercent_Keystroke"
);
global_method!(date_format_callback, af_date_format, "AFDate_Format");
global_method!(
    date_format_ex_callback,
    af_date_format_ex,
    "AFDate_FormatEx"
);
global_method!(
    date_keystroke_callback,
    af_date_keystroke,
    "AFDate_Keystroke"
);
global_method!(
    date_keystroke_ex_callback,
    af_date_keystroke_ex,
    "AFDate_KeystrokeEx"
);
global_method!(time_format_callback, af_time_format, "AFTime_Format");
global_method!(
    time_format_ex_callback,
    af_date_format_ex,
    "AFTime_FormatEx"
);
global_method!(
    time_keystroke_callback,
    af_time_keystroke,
    "AFTime_Keystroke"
);
global_method!(
    time_keystroke_ex_callback,
    af_date_keystroke_ex,
    "AFTime_KeystrokeEx"
);
global_method!(
    special_format_callback,
    af_special_format,
    "AFSpecial_Format"
);
global_method!(
    special_keystroke_callback,
    af_special_keystroke,
    "AFSpecial_Keystroke"
);
global_method!(
    special_keystroke_ex_callback,
    af_special_keystroke_ex,
    "AFSpecial_KeystrokeEx"
);
global_method!(simple_callback, af_simple, "AFSimple");
global_method!(make_number_callback, af_make_number, "AFMakeNumber");
global_method!(
    simple_calculate_callback,
    af_simple_calculate,
    "AFSimple_Calculate"
);
global_method!(
    range_validate_callback,
    af_range_validate,
    "AFRange_Validate"
);
global_method!(merge_change_callback, af_merge_change, "AFMergeChange");
global_method!(parse_date_ex_callback, af_parse_date_ex, "AFParseDateEx");
global_method!(extract_nums_callback, af_extract_nums, "AFExtractNums");
pub fn register_public_methods(context: &mut Context) -> JsResult<()> {
    type Callback = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;
    const METHODS: &[(&str, Callback)] = &[
        ("AFDate_Format", date_format_callback),
        ("AFDate_FormatEx", date_format_ex_callback),
        ("AFDate_Keystroke", date_keystroke_callback),
        ("AFDate_KeystrokeEx", date_keystroke_ex_callback),
        ("AFExtractNums", extract_nums_callback),
        ("AFMakeNumber", make_number_callback),
        ("AFMergeChange", merge_change_callback),
        ("AFNumber_Format", number_format_callback),
        ("AFNumber_Keystroke", number_keystroke_callback),
        ("AFParseDateEx", parse_date_ex_callback),
        ("AFPercent_Format", percent_format_callback),
        ("AFPercent_Keystroke", percent_keystroke_callback),
        ("AFRange_Validate", range_validate_callback),
        ("AFSimple", simple_callback),
        ("AFSimple_Calculate", simple_calculate_callback),
        ("AFSpecial_Format", special_format_callback),
        ("AFSpecial_Keystroke", special_keystroke_callback),
        ("AFSpecial_KeystrokeEx", special_keystroke_ex_callback),
        ("AFTime_Format", time_format_callback),
        ("AFTime_FormatEx", time_format_ex_callback),
        ("AFTime_Keystroke", time_keystroke_callback),
        ("AFTime_KeystrokeEx", time_keystroke_ex_callback),
    ];
    for &(name, callback) in METHODS {
        let function =
            FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(callback))
                .name(JsString::from(name))
                .length(0)
                .constructor(false)
                .build();
        context.register_global_property(
            JsString::from(name),
            function,
            Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{cpdf_document::CpdfDocument, cpdf_parser::ParserError};
    use crate::fpdfdoc::cpdf_formfield::CpdfFormField;
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::FormFillHost;
    use crate::fpdfsdk::formfiller::cffl_fieldaction::CfflFieldAction;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
    use crate::fxjs::cjs_event_context::FieldActionChannels;
    use std::cell::RefCell;

    fn fixture() -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<CpdfSdkFormFillEnvironment>,
        Rc<CjsRuntime>,
        Rc<CpdfFormField>,
    ) {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        assert_eq!(
            document.borrow_mut().load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(
                    include_bytes!("../../../../upstream/pdfium/testing/resources/calculate.pdf")
                        .to_vec()
                )),
                b""
            ),
            ParserError::Success
        );
        let environment = CpdfSdkFormFillEnvironment::new(&document, None);
        let runtime = CjsRuntime::new(Rc::downgrade(&environment)).unwrap();
        let field = environment
            .interactive_form()
            .core_form()
            .get_field(0, &[])
            .unwrap();
        (document, environment, runtime, field)
    }
    fn run(
        runtime: &Rc<CjsRuntime>,
        field: &CpdfFormField,
        data: &mut CfflFieldAction,
        script: &str,
    ) {
        let channels = FieldActionChannels::from_action(data);
        let event = runtime.new_event_context();
        event.on_field_keystroke(field, &channels);
        event.run_script(&wide(script)).unwrap();
        channels.write_back(data);
    }

    // Complete `cjs_publicmethods_unittest.cpp` input/reference table.
    #[test]
    #[allow(non_snake_case)]
    fn IsNumber() {
        for (text, expected) in [
            ("", true),
            ("  ", true),
            ("xyz00", false),
            ("1%", false),
            ("0x234", false),
            ("+123", true),
            ("-98765", true),
            ("  345 ", true),
            ("-1e5", false),
            ("-2e", false),
            ("e-5", true),
            ("0.023", true),
            (".356089", true),
            ("1e-9", true),
            ("-1.23e+23", true),
            ("1,000,000", false),
            ("560,024", true),
            ("0", true),
            ("0123", true),
            ("9876123", true),
        ] {
            assert_eq!(is_number(&wide(text)), expected, "{text}");
        }
    }
    // Complete cases from `cjs_publicmethods_embeddertest.cpp`.
    #[test]
    #[allow(non_snake_case)]
    fn ParseDateUsingFormat() {
        let mut context = Context::default();
        for (text, format, expected) in [
            ("06/25/1968", "mm/dd/yyyy", -47_865_600_000.0),
            ("25061968", "ddmmyyyy", -47_865_600_000.0),
            ("19680625", "yyyymmdd", -47_865_600_000.0),
            ("31121985", "ddmmyyyy", 504_835_200_000.0),
            ("311285", "ddmmyy", 504_835_200_000.0),
            ("01021995", "ddmmyyyy", 791_596_800_000.0),
            ("010295", "ddmmyy", 791_596_800_000.0),
            ("01022005", "ddmmyyyy", 1_107_216_000_000.0),
            ("010205", "ddmmyy", 1_107_216_000_000.0),
            ("050201", "yymmdd", 1_107_216_000_000.0),
        ] {
            let mut wrong = false;
            let result =
                parse_date_using_format(&mut context, &wide(text), &wide(format), Some(&mut wrong))
                    .unwrap();
            assert_eq!(result - result % 86_400_000.0, expected, "{text}/{format}");
            assert!(!wrong);
        }
    }
    #[test]
    #[allow(non_snake_case)]
    fn PrintDateUsingFormat() {
        for (time, format, expected) in [
            (-47_952_000_000.0, "ddmmyy", "250668"),
            (-47_952_000_000.0, "yy/mm/dd", "68/06/25"),
            (-0.0001, "ddmmyy", "311269"),
            (-0.0001, "yy!mmdd", "69!1231"),
            (0.0, "ddmmyy", "010170"),
            (0.0, "mm-yyyy-dd", "01-1970-01"),
            (504_835_200_000.0, "ddmmyy", "311285"),
            (504_835_200_000.0, "yymmdd", "851231"),
            (791_596_800_000.0, "ddmmyy", "010295"),
            (791_596_800_000.0, "yyyymmdd", "19950201"),
            (1_107_216_000_000.0, "ddmmyy", "010205"),
            (1_107_216_000_000.0, "yyyyddmm", "20050102"),
            (3_660_595_200_000.0, "ddmmyy", "311285"),
            (3_660_595_200_000.0, "yyyydd", "208531"),
            (3_947_356_800_000.0, "ddmmyy", "010295"),
            (3_947_356_800_000.0, "mmddyyyy", "02012095"),
        ] {
            assert_eq!(print_date_using_format(time, &wide(format)), wide(expected));
        }
    }
    #[test]
    fn repeated_date_tokens_and_fallback_components_use_the_engine_date_parser() {
        let mut format = vec![u16::from(b'y'); 100_000];
        format.extend([0xd800, 0]);
        let result = print_date_using_format(0.0, &format);
        assert_eq!(result.len(), format.len());
        assert!(result[..99_996].iter().all(|&c| c == u16::from(b'y')));
        assert_eq!(&result[99_996..100_000], wide("1970"));
        assert_eq!(&result[100_000..], &[0xd800, 0]);

        let mut context = Context::default();
        let mut wrong = true;
        let result = parse_date_using_format(
            &mut context,
            &wide("2005|02|01"),
            &wide("broken-format"),
            Some(&mut wrong),
        )
        .unwrap();
        assert!(result.is_finite());
        assert_eq!(
            print_date_using_format(result, &wide("yyyy-mm-dd")),
            wide("2005-02-01")
        );
        assert!(!wrong);
        context.eval(boa_engine::Source::from_bytes(
            "Date.parse = function(text) { if(this !== globalThis || !text.startsWith('2016-01-02T')) throw 'bad reconstructed Date.parse'; return 1234; };"
        )).unwrap();
        assert_eq!(
            parse_date(&mut context, &wide("16|01|02"), None).unwrap(),
            super::super::js_date::local_time(&context, 1234.0)
        );
        context
            .eval(boa_engine::Source::from_bytes(
                "var marker={}; Date.parse=()=>{throw marker};",
            ))
            .unwrap();
        let error = parse_date(&mut context, &wide("2005|02|01"), None)
            .unwrap_err()
            .into_opaque(&mut context)
            .unwrap();
        assert_eq!(
            error,
            context
                .global_object()
                .get(JsString::from("marker"), &mut context)
                .unwrap()
        );
    }
    #[test]
    fn atof_prefix_adapter_keeps_c_whitespace_single_sign_and_special_values() {
        for (input, expected) in [
            ("\t20", 20.0),
            ("\r\n\u{000b}\u{000c} -12.5tail", -12.5),
            ("+2.5e+2stop", 250.0),
            ("--20", 0.0),
            ("-+20", 0.0),
            ("+ 20", 0.0),
            ("-\t20", 0.0),
            (".75", 0.75),
            ("1e+", 1.0),
            ("20\0more", 20.0),
            ("\u{a0}20", 0.0),
        ] {
            assert_eq!(number(&wide(input)), expected, "{input:?}");
        }
        assert_eq!(number(&wide("+infinity")), f64::INFINITY);
        assert_eq!(number(&wide("-INF")), f64::NEG_INFINITY);
        assert!(number(&wide(" NaN(payload)")).is_nan());
    }
    #[test]
    #[allow(non_snake_case)]
    fn AFSimpleCalculateSum() {
        let (_document, _environment, runtime, field) = fixture();
        let mut data = CfflFieldAction::default();
        run(
            &runtime,
            &field,
            &mut data,
            "AFSimple_Calculate('SUM', ['Calc1_A', 'Calc1_B']);",
        );
        assert_eq!(data.s_value, vec![55]);
    }
    #[test]
    #[allow(non_snake_case)]
    fn AFNumberKeystroke() {
        let (_document, _environment, runtime, field) = fixture();
        let mut data = CfflFieldAction {
            s_value: "-10".chars().map(u32::from).collect(),
            ..Default::default()
        };
        run(
            &runtime,
            &field,
            &mut data,
            "AFNumber_Keystroke('-10', '');",
        );
        assert!(data.b_rc);
        assert_eq!(data.s_value, vec![45, 49, 48]);
    }
    #[test]
    fn actual_global_arithmetic_arrays_coercions_and_opaque_errors() {
        let (_document, _environment, runtime, field) = fixture();
        let mut data = CfflFieldAction::default();
        run(
            &runtime,
            &field,
            &mut data,
            r#"
            for(const name of ['AFSimple', 'AFNumber_Format', 'AFDate_FormatEx']) {
                const descriptor = Object.getOwnPropertyDescriptor(globalThis, name);
                if(descriptor.writable || !descriptor.enumerable || !descriptor.configurable ||
                   descriptor.value.prototype !== undefined) throw 'wrong global method attributes';
            }
            if(AFSimple('SUM', 3, 4) !== 7 || AFSimple('AVG', 2, 6) !== 4 ||
               AFSimple('PRD', 3, 4) !== 12 || AFSimple('MIN', -2, 5) !== -2 ||
               AFSimple('MAX', -2, 5) !== 5) throw 'arithmetic operation';
            if(AFMakeNumber('12,5') !== 12.5 || AFMakeNumber('') !== 0 ||
               AFMakeNumber('bad') !== 0 || !Number.isNaN(AFMakeNumber('NaN')))
               throw 'numeric conversion';
            if(JSON.stringify(AFExtractNums('.12 then 345\ud80067')) !== '["0","12","345","67"]' ||
               AFExtractNums('no numbers') !== undefined) throw 'number extraction';
            try {AFSimple('unknown', 1, 2); throw 'missing error'} catch(error) {
                if(typeof error !== 'string' || error !== 'AFSimple: Incorrect parameter value.') throw error;
            }
            try {AFDate_FormatEx(); throw 'missing error'} catch(error) {
                if(error !== 'AFDate_FormatEx: Incorrect number of parameters passed to function.') throw error;
            }
            const marker = {};
            try {AFSimple({toString(){throw marker}}, 1, 2); throw 'missing coercion error'}
            catch(error) {if(error !== marker) throw 'opaque coercion error lost'}
            const originalParse = Date.parse;
            try {
                Date.parse = function(value) {
                    if(this !== globalThis) throw 'incorrect Date.parse receiver';
                    event.change = 'parse callback';
                    return 1234;
                };
                if(AFParseDateEx('99/99/9999', 'mm/dd/yyyy') !== 1234-new Date(1234).getTimezoneOffset()*60000 ||
                   event.change !== 'parse callback') throw 'Date.parse fallback';
                Date.parse = () => {throw marker};
                try {AFParseDateEx('99/99/9999', 'mm/dd/yyyy'); throw 'missing Date.parse error'}
                catch(error) {if(error !== marker) throw 'Date.parse opaque error lost'}
            } finally {Date.parse = originalParse}
            AFSimple_Calculate('SUM', ' Calc1_A, Calc1_B ');
            if(event.value !== '7') throw 'comma field list sum';
            AFSimple_Calculate('PRD', ['Calc1_A', 'Calc1_B']);
            if(event.value !== '10') throw 'field product';
            AFSimple_Calculate('AVG', ['Calc1_A', 'Calc1_B']);
            if(event.value !== '3.5') throw 'field average';
            const fields = ['Calc1_A'];
            Object.defineProperty(fields, 1, {get(){
                event.change='array callback';
                if(AFSimple('PRD', 2, 3) !== 6) throw 'nested arithmetic';
                return 'Calc1_B';
            }});
            AFSimple_Calculate('SUM',fields);
            if(event.value !== '7' || event.change !== 'array callback') throw 'array getter reentry';
        "#,
        );
    }
    #[test]
    fn actual_event_number_percent_mask_range_and_date_paths() {
        let (_document, _environment, runtime, field) = fixture();
        let mut data = CfflFieldAction {
            s_value: vec![49, 50, 51, 52, 46, 53],
            ..Default::default()
        };
        run(
            &runtime,
            &field,
            &mut data,
            r#"
            AFNumber_Format(2, 0, 0, {valueOf(){throw 'unused currStyle'}}, '$', true);
            if(event.value !== '$1,234.50') throw 'number formatting';
            event.value = '-1234.5'; AFNumber_Format(2, 2, 2, 0, '\ud800', false);
            if(event.value !== '(1.234,50\ud800)') throw 'negative/currency formatting';
            event.value = '.125'; AFPercent_Format(1, 0);
            if(event.value !== '12.5%') throw 'percent formatting';
            event.value = '-1234.125'; AFPercent_Format(2, 4, true);
            if(event.value !== "%-123'412.50") throw 'percent apostrophe grouping';
            AFPercent_Format(513, 0); if(event.value !== '%') throw 'precision limit';
            event.value = '1234567890'; AFSpecial_Format(2);
            if(event.value !== '(123) 456-7890') throw 'phone mask';
            event.value = '123456789'; AFSpecial_Format(3);
            if(event.value !== '123-45-6789') throw 'SSN mask';
            event.value = '12'; event.change = 'a'; event.selStart=event.selEnd=2;
            AFNumber_Keystroke(2,0); if(event.rc) throw 'invalid numeric change accepted';
            event.rc=true; event.change='3'; AFNumber_Keystroke(2,0);
            if(!event.rc || event.value !== '123') throw 'numeric merge';
            event.value='ab'; event.change='-9'; event.selStart=event.selEnd=2;
            AFSpecial_KeystrokeEx('AA-9'); if(event.change !== '-9' || !event.rc) throw 'mask change';
            event.value='abc'; event.change='\ud800'; event.selStart=1; event.selEnd=2;
            if(AFMergeChange('unused') !== 'a\ud800c') throw 'UTF16 merge';
            event.value='20'; AFRange_Validate(true,0,true,10);
            if(event.rc) throw 'range veto lost';
            event.rc=true; event.value='\t20'; AFRange_Validate(true,0,true,10);
            if(event.rc) throw 'atof whitespace range veto lost';
            event.value='--20'; AFNumber_Format(2,0,0,0,'',false);
            if(event.value !== '0.00') throw 'atof repeated signs accepted';
            event.value='+ 20'; AFPercent_Format(0,0);
            if(event.value !== '0%') throw 'atof spaced sign accepted';
            event.rc=true; event.value='06/25/1968'; AFDate_FormatEx('mm/dd/yyyy');
            if(event.value !== '06/25/1968') throw 'date formatting';
            event.value='23:45'; AFTime_Format(0);
            if(event.value !== '23:45') throw 'time formatting';
            if(!Number.isFinite(AFParseDateEx('050201','yymmdd'))) throw 'date parser';
        "#,
        );
        assert!(data.b_rc);
    }
    #[test]
    fn commit_validators_and_reentrant_coercion_release_event_borrows() {
        let (_document, _environment, runtime, field) = fixture();
        let mut data = CfflFieldAction {
            b_will_commit: true,
            s_value: vec![97],
            ..Default::default()
        };
        run(
            &runtime,
            &field,
            &mut data,
            r#"
            try {AFNumber_Keystroke(0,0); throw 'missing commit error'} catch(error) {
                if(error !== 'AFNumber_Keystroke: The input value is invalid.') throw error;
            }
            if(event.rc) throw 'numeric commit veto lost';
            event.rc=true; event.value='1234'; AFSpecial_KeystrokeEx('99999');
            if(event.rc) throw 'short committed mask accepted';
            event.rc=true; event.value='12345'; AFSpecial_KeystrokeEx('99999');
            if(!event.rc) throw 'valid committed mask rejected';
            event.value='1.5'; AFNumber_Format({valueOf(){event.change='coercion';return 2}},0,0,0,'',false);
            if(event.value !== '1.50' || event.change !== 'coercion') throw 'formatter coercion';
            event.value=''; AFDate_KeystrokeEx('mm/dd/yyyy');
            if(!event.rc) throw 'empty date rejected';
            event.value='not a date'; AFDate_KeystrokeEx('mm/dd/yyyy');
            if(event.rc) throw 'bad date commit accepted';
        "#,
        );
    }

    #[test]
    fn validator_alert_allows_synchronous_script_reentry_and_restores_event() {
        struct AlertHost {
            runtime: RefCell<std::rc::Weak<CjsRuntime>>,
            calls: RefCell<Vec<(Vec<u32>, Vec<u32>, i32, i32)>>,
        }
        impl FormFillHost for AlertHost {
            fn js_app_alert(&self, message: &[u32], title: &[u32], kind: i32, icon: i32) -> i32 {
                self.calls
                    .borrow_mut()
                    .push((message.to_vec(), title.to_vec(), kind, icon));
                let runtime = self.runtime.borrow().upgrade().unwrap();
                runtime.run_document_script(&[], &wide(
                    "if(event.type !== 'Doc' || AFSimple('SUM',1,2) !== 3) throw 'nested alert script';"
                )).unwrap();
                1
            }
        }
        let (document, _old_environment, _old_runtime, _old_field) = fixture();
        let host = Rc::new(AlertHost {
            runtime: RefCell::new(std::rc::Weak::new()),
            calls: RefCell::new(Vec::new()),
        });
        let interface: Rc<dyn FormFillHost> = host.clone();
        let environment = CpdfSdkFormFillEnvironment::new(&document, Some(&interface));
        let runtime = environment.get_or_create_runtime().unwrap();
        *host.runtime.borrow_mut() = Rc::downgrade(&runtime);
        let field = environment
            .interactive_form()
            .core_form()
            .get_field(0, &[])
            .unwrap();
        let mut data = CfflFieldAction {
            s_value: vec![50, 48],
            ..Default::default()
        };
        run(&runtime, &field, &mut data,
            "AFRange_Validate(true,0,true,10); if(event.type !== 'Field') throw 'outer event lost'; event.change='restored';");
        assert!(!data.b_rc);
        assert_eq!(
            data.s_change,
            "restored".chars().map(u32::from).collect::<Vec<_>>()
        );
        let calls = host.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].0,
            "The input value must be greater than or equal to 0 and less than or equal to 10."
                .chars()
                .map(u32::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            calls[0].1,
            "AFRange_Validate"
                .chars()
                .map(u32::from)
                .collect::<Vec<_>>()
        );
        assert_eq!((calls[0].2, calls[0].3), (0, 3));
    }
}
