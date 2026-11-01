// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Locale picture selection and symbol translation from
//! `fxjs/xfa/cfxjse_formcalc_context.cpp`. Explicit locale hosts stand in for
//! document/this-node locale selection until the XFA node binding is connected.
use super::cfxjse_formcalc_context::locale_host;
use super::formcalc_locale::FormcalcLocaleHost;
use super::formcalc_values::{get_simple_value, param_count, value_to_float, value_to_utf8_string};
use crate::fxcrt::fx_string::{utf16_encode, utf8_decode};
use crate::xfa::fgas::crt::locale_iface::{DateTimeSubcategory, LocaleIface};
use boa_engine::{native_function::NativeFunctionPointer, Context, JsResult, JsString, JsValue};
use std::rc::Rc;

pub(super) const METHODS: &[(&str, NativeFunctionPointer)] = &[
    ("DateFmt", date_fmt),
    ("TimeFmt", time_fmt),
    ("LocalDateFmt", local_date_fmt),
    ("LocalTimeFmt", local_time_fmt),
];

const ALT_TABLE_DATE: [u8; 33] = [
    255, 255, 255, 3, 9, 255, 255, 255, 255, 255, 255, 255, 2, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 1, 255, 255, 255, 255, 255, 255, 255, 255,
];
const ALT_TABLE_TIME: [u8; 33] = [
    14, 255, 255, 3, 9, 255, 255, 15, 255, 255, 255, 255, 6, 255, 255, 255, 255, 255, 7, 255, 255,
    255, 255, 255, 1, 17, 255, 255, 255, 255, 255, 255, 255,
];

pub(super) fn locale_from_string(
    host: &FormcalcLocaleHost,
    name: &[u8],
) -> Option<Rc<dyn LocaleIface>> {
    if name.is_empty() {
        host.manager.get_def_locale()
    } else {
        host.manager.get_locale_by_name(&utf8_decode(name))
    }
}

/// The source's FormatFromString helper is specifically date-oriented.
pub(super) fn date_format_from_string(locale: &dyn LocaleIface, format: &[u8]) -> Vec<u32> {
    if format.is_empty() {
        locale.get_date_pattern(DateTimeSubcategory::Default)
    } else {
        utf8_decode(format)
    }
}

fn subcategory_from_int(style: i32) -> DateTimeSubcategory {
    match style {
        1 => DateTimeSubcategory::Short,
        3 => DateTimeSubcategory::Long,
        4 => DateTimeSubcategory::Full,
        _ => DateTimeSubcategory::Medium,
    }
}

/// Source AlternateDateTimeSymbols. A linear output buffer avoids repeated
/// in-place deletions of quote characters. Consecutive quotes retain one literal
/// quote, and quoted text is never translated. Malformed short symbol tables
/// retain the corresponding original token instead of indexing out of bounds.
fn alternate_date_time_symbols(pattern: &[u32], symbols: &[u32], is_date: bool) -> Vec<u32> {
    let table = if is_date {
        &ALT_TABLE_DATE
    } else {
        &ALT_TABLE_TIME
    };
    let mut output = Vec::with_capacity(pattern.len());
    let mut in_literal = false;
    let mut escape = false;
    for &ch in pattern {
        if ch == u32::from(b'\'') {
            in_literal = !in_literal;
            if escape {
                output.push(ch);
            }
            escape = !escape;
            continue;
        }
        let replacement = if !in_literal && (u32::from(b'A')..=u32::from(b'a')).contains(&ch) {
            let alternate = table[(ch - u32::from(b'A')) as usize];
            if alternate == 255 {
                None
            } else {
                symbols.get(usize::from(alternate)).copied()
            }
        } else {
            None
        };
        output.push(replacement.unwrap_or(ch));
        escape = false;
    }
    output
}

fn get_local_date_time_format(
    host: &FormcalcLocaleHost,
    style: i32,
    name: &[u8],
    standard: bool,
    is_date: bool,
) -> Vec<u32> {
    let Some(locale) = locale_from_string(host, name) else {
        return Vec::new();
    };
    let category = subcategory_from_int(style);
    let pattern = if is_date {
        locale.get_date_pattern(category)
    } else {
        locale.get_time_pattern(category)
    };
    if standard {
        pattern
    } else {
        alternate_date_time_symbols(&pattern, &locale.get_date_time_symbols(), is_date)
    }
}

fn picture_format(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    method: &str,
    standard: bool,
    is_date: bool,
) -> JsResult<JsValue> {
    if args.len() > 2 {
        // DateFmt's source diagnostic mistakenly names Date2Num.
        return Err(param_count(method));
    }
    let mut style = 0;
    if !args.is_empty() {
        let value = get_simple_value(args, 0, context)?;
        if value.is_null() {
            return Ok(JsValue::null());
        }
        style = value_to_float(&value, context)? as i32;
        if !(0..=4).contains(&style) {
            style = 0;
        }
    }
    let name = if args.len() > 1 {
        let value = get_simple_value(args, 1, context)?;
        if value.is_null() {
            return Ok(JsValue::null());
        }
        value_to_utf8_string(&value, context)?
    } else {
        Vec::new()
    };
    // Getters/coercions finish before borrowing any host state; errors and
    // nested script execution retain their original Boa values.
    let host = locale_host(this, context)?;
    let pattern = get_local_date_time_format(&host, style, &name, standard, is_date);
    Ok(JsString::from(utf16_encode(&pattern).as_slice()).into())
}
fn date_fmt(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    picture_format(this, args, context, "DateFmt", true, true)
}
fn time_fmt(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    picture_format(this, args, context, "TimeFmt", true, false)
}
fn local_date_fmt(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    picture_format(this, args, context, "LocalDateFmt", false, true)
}
fn local_time_fmt(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    picture_format(this, args, context, "LocalTimeFmt", false, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxjs::xfa::cfxjse_formcalc_context::CfxJseFormcalcContext;
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn run(runtime: &CfxJseFormcalcContext, program: &str) -> JsValue {
        runtime
            .execute(&wide(program))
            .unwrap_or_else(|error| panic!("{program}: {error}"))
    }
    fn expect_text(runtime: &CfxJseFormcalcContext, program: &str, expected: &str) {
        assert_eq!(
            run(runtime, program).as_string().unwrap(),
            JsString::from(expected),
            "{program}"
        );
    }

    #[test]
    fn source_date_fmt_local_date_fmt_and_time_fmt_programs() {
        let runtime = CfxJseFormcalcContext::new_with_locale(&wide("en_US")).unwrap();
        // All enabled DateFmt, LocalDateFmt and TimeFmt embedder assertions.
        for (program, expected) in [
            (r#"DateFmt(3, "de_DE")"#, "D. MMMM YYYY"),
            (r#"LocalDateFmt(3, "de_CH")"#, "t. MMMM jjjj"),
            (r#"LocalDateFmt(4, "fr_FR")"#, "EEEE j MMMM aaaa"),
            (r#"TimeFmt(2, "fr_CA")"#, "HH:MM:SS"),
            (r#"TimeFmt(3, "fr_FR")"#, "HH:MM:SS Z"),
        ] {
            expect_text(&runtime, program, expected);
        }
    }

    #[test]
    fn picture_styles_local_symbols_and_locale_selection() {
        let runtime = CfxJseFormcalcContext::new_with_locale(&wide("en_US")).unwrap();
        for (program, expected) in [
            ("DateFmt()", "MMM D, YYYY"),
            ("DateFmt(0)", "MMM D, YYYY"),
            ("DateFmt(1)", "M/D/YY"),
            ("DateFmt(2)", "MMM D, YYYY"),
            ("DateFmt(3)", "MMMM D, YYYY"),
            ("DateFmt(4)", "EEEE, MMMM D, YYYY"),
            ("DateFmt(-1)", "MMM D, YYYY"),
            ("DateFmt(5)", "MMM D, YYYY"),
            ("DateFmt(NaN)", "MMM D, YYYY"),
            ("TimeFmt()", "h:MM:SS A"),
            ("TimeFmt(1)", "h:MM A"),
            ("TimeFmt(4)", "h:MM:SS A Z"),
            (r#"LocalDateFmt(1, "de_DE")"#, "tt.MM.jj"),
            (r#"LocalDateFmt(2, "fr_CA")"#, "j MMM aa"),
            // These exercise all four disabled LocalTimeFmt source examples.
            // Its stale expectations retain uppercase hour tokens, double ':',
            // or retain quotes that AlternateDateTimeSymbols strips.
            (r#"LocalTimeFmt(1, "de_DE")"#, "hh:mm"),
            (r#"LocalTimeFmt(2, "fr_CA")"#, "hh:mm:ss"),
            (r#"LocalTimeFmt(3, "de_CH")"#, "hh:mm:ss z"),
            (r#"LocalTimeFmt(4, "fr_FR")"#, "hh h mm z"),
            (r#"TimeFmt(4, "de_DE")"#, "H:MM' Uhr 'Z"),
            (r#"DateFmt(2, "x")"#, ""),
            (r#"DateFmt(2, "")"#, "MMM D, YYYY"),
        ] {
            expect_text(&runtime, program, expected);
        }
        let german = CfxJseFormcalcContext::new_with_locale(&wide("de_DE")).unwrap();
        expect_text(&german, "DateFmt()", "DD.MM.YYYY");
        expect_text(&german, r#"DateFmt(1, "en_US")"#, "M/D/YY");
        expect_text(&german, "DateFmt()", "DD.MM.YYYY");
        for method in ["DateFmt", "TimeFmt", "LocalDateFmt", "LocalTimeFmt"] {
            for suffix in ["(null)", "(1,null)"] {
                assert!(run(&runtime, &format!("{method}{suffix}")).is_null());
            }
            assert!(runtime.execute(&wide(&format!("{method}(1,2,3)"))).is_err());
        }
    }

    #[test]
    fn alternate_symbol_tables_quotes_and_damaged_symbol_sets() {
        let symbols = wide("0123456789abcdefgh");
        let pattern = wide("ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`a");
        assert_eq!(
            alternate_date_time_symbols(&pattern, &symbols, true),
            wide("ABC39FGHIJKL2NOPQRSTUVWX1Z[\\]^_`a")
        );
        assert_eq!(
            alternate_date_time_symbols(&pattern, &symbols, false),
            wide("eBC39FGfIJKL6NOPQR7TUVWX1h[\\]^_`a")
        );
        for (pattern, expected) in [
            ("'D M Y' D M Y", "D M Y 3 2 1"),
            ("''D''", "'3'"),
            ("'''D'''", "'D'"),
            ("'unterminated D", "unterminated D"),
            ("D' D 'D", "3 D 3"),
        ] {
            assert_eq!(
                alternate_date_time_symbols(&wide(pattern), &symbols, true),
                wide(expected)
            );
        }
        assert_eq!(
            alternate_date_time_symbols(&wide("DMY 'D'"), &[], true),
            wide("DMY D")
        );
    }

    #[test]
    fn argument_getters_preserve_errors_and_reentry_without_leaking_state() {
        let runtime = CfxJseFormcalcContext::new_with_locale(&wide("en_US")).unwrap();
        runtime
            .engine()
            .eval(
                &"globalThis.marker={}; globalThis.input=[0,'value',{get value(){throw marker}}]"
                    .encode_utf16()
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let error = runtime.execute(&wide("LocalDateFmt(1,input)")).unwrap_err();
        assert!(runtime
            .engine()
            .with_context(|context| {
                let marker = context
                    .global_object()
                    .get(JsString::from("marker"), context)?;
                Ok(error.into_opaque(context)? == marker)
            })
            .unwrap());
        runtime.engine().eval(&"globalThis.input=[0,'value',{get value(){return pfm_rt.LocalDateFmt(3,'de_DE')==='t. MMMM jjjj'?'fr_FR':'de_DE'}}]".encode_utf16().collect::<Vec<_>>()).unwrap();
        expect_text(&runtime, "LocalDateFmt(4,input)", "EEEE j MMMM aaaa");
        expect_text(&runtime, "DateFmt()", "MMM D, YYYY");
    }
}
