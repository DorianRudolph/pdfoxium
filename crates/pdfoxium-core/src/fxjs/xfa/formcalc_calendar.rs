// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Date2Num/Num2Date and Local2IsoDate/IsoDate2Local from
//! `fxjs/xfa/cfxjse_formcalc_context.cpp`. The existing picture formatter owns
//! locale parsing; ordinal inversion reuses the shared calendar arithmetic.
use super::cfxjse_formcalc_context::locale_host;
use super::formcalc_iso::date_string_to_num;
use super::formcalc_locale::FormcalcLocaleHost;
use super::formcalc_patterns::{date_format_from_string, locale_from_string};
use super::formcalc_values::{
    get_simple_value, param_count, value_is_null, value_to_float, value_to_utf8_string,
};
use crate::fxcrt::fx_string::{utf16_encode, utf8_decode};
use crate::fxjs::fx_date_helpers as calendar;
use crate::xfa::fgas::crt::locale_mgr_iface::LocaleMgrIface;
use crate::xfa::fxfa::parser::cxfa_localevalue::{CxfaLocaleValue, ValueType, XfaValuePicture};
use boa_engine::{native_function::NativeFunctionPointer, Context, JsResult, JsString, JsValue};
use std::rc::Rc;

pub(super) const METHODS: &[(&str, NativeFunctionPointer)] =
    &[("Date2Num", date_to_num), ("Num2Date", num_to_date)];

fn string(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}

/// GetSimpleValue/ValueIsNull remain separate source operations: expanded
/// properties may execute getters, and their opaque exceptions must propagate.
fn optional_text(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<Option<Vec<u8>>> {
    if index >= args.len() {
        return Ok(Some(Vec::new()));
    }
    let value = get_simple_value(args, index, context)?;
    if value_is_null(&value, context)? {
        return Ok(None);
    }
    value_to_utf8_string(&value, context).map(Some)
}

fn local_to_iso_date(
    host: &FormcalcLocaleHost,
    value: &[u8],
    format: &[u8],
    locale_name: &[u8],
) -> Vec<u8> {
    let Some(locale) = locale_from_string(host, locale_name) else {
        return Vec::new();
    };
    let pattern = date_format_from_string(locale.as_ref(), format);
    let manager: Rc<dyn LocaleMgrIface> = host.manager.clone();
    let value = CxfaLocaleValue::with_pattern(
        ValueType::Date,
        &utf8_decode(value),
        &pattern,
        Some(locale),
        Some(manager),
    );
    if !value.is_valid() {
        return Vec::new();
    }
    let date = value.get_date();
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day).into_bytes()
}

fn iso_date_to_local(
    host: &FormcalcLocaleHost,
    value: &[u8],
    format: &[u8],
    locale_name: &[u8],
) -> Vec<u32> {
    let Some(locale) = locale_from_string(host, locale_name) else {
        return Vec::new();
    };
    let pattern = date_format_from_string(locale.as_ref(), format);
    let manager: Rc<dyn LocaleMgrIface> = host.manager.clone();
    let value = CxfaLocaleValue::with_value(ValueType::Date, &utf8_decode(value), Some(manager));
    let mut result = Vec::new();
    // Source returns the formatter's output even when it reports failure;
    // that output may contain the original canonical value as its fallback.
    value.format_patterns(
        &mut result,
        &pattern,
        Some(locale),
        XfaValuePicture::Display,
    );
    result
}

fn date_to_num(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if !(1..=3).contains(&args.len()) {
        return Err(param_count("Date2Num"));
    }
    let Some(value) = optional_text(args, 0, context)? else {
        return Ok(JsValue::null());
    };
    let Some(format) = optional_text(args, 1, context)? else {
        return Ok(JsValue::null());
    };
    let Some(locale) = optional_text(args, 2, context)? else {
        return Ok(JsValue::null());
    };
    // Clone host state only after all user-controlled extraction/coercion.
    let host = locale_host(this, context)?;
    let value = local_to_iso_date(&host, &value, &format, &locale);
    Ok(date_string_to_num(&value).into())
}

fn num_to_date(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if !(1..=3).contains(&args.len()) {
        return Err(param_count("Num2Date"));
    }
    let value = get_simple_value(args, 0, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    // Retain the source's f32 conversion and truncation. Rust's saturating cast
    // gives non-finite and out-of-range inputs defined behavior instead of UB.
    let ordinal = value_to_float(&value, context)? as i32;
    if ordinal < 1 {
        return Ok(JsValue::null());
    }
    let Some(format) = optional_text(args, 1, context)? else {
        return Ok(JsValue::null());
    };
    let Some(locale) = optional_text(args, 2, context)? else {
        return Ok(JsValue::null());
    };
    let host = locale_host(this, context)?;
    // The picture canonical grammar accepts four-digit years. Avoid the native
    // month-by-month loop for values outside that grammar, then use the same
    // shared calendar arithmetic that DateString2Num uses in the other direction.
    let last_ordinal = calendar::make_day(9999, 11, 31) - calendar::make_day(1900, 0, 1) + 1.;
    if f64::from(ordinal) > last_ordinal {
        return Ok(string(&[]));
    }
    let time = calendar::make_date(calendar::make_day(1900, 0, 1) + f64::from(ordinal) - 1., 0.);
    let iso = format!(
        "{:04}{:02}{:02}",
        calendar::get_year_from_time(time),
        calendar::get_month_from_time(time) + 1,
        calendar::get_day_from_time(time),
    );
    Ok(string(&iso_date_to_local(
        &host,
        iso.as_bytes(),
        &format,
        &locale,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxjs::xfa::cfxjse_formcalc_context::CfxJseFormcalcContext;
    use boa_engine::js_string;

    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn runtime() -> CfxJseFormcalcContext {
        CfxJseFormcalcContext::new_with_locale(&wide("en_US")).unwrap()
    }
    fn run(runtime: &CfxJseFormcalcContext, program: &str) -> JsValue {
        runtime
            .execute(&wide(program))
            .unwrap_or_else(|error| panic!("{program}: {error}"))
    }

    // cfxjse_formcalc_context_embeddertest.cpp: Date2Num, all active source
    // assertions plus the source's two #if 0 examples, through compiler + Boa.
    #[test]
    fn source_date_to_num_programs() {
        let runtime = runtime();
        for (program, expected) in [
            (r#"Date2Num("1/1/1900", "D/M/YYYY")"#, 1.),
            (r#"Date2Num("03/15/96", "MM/DD/YY")"#, 35138.),
            (r#"Date2Num("96-08-20", "YY-MM-DD", "fr_FR")"#, 35296.),
            (
                r#"Date2Num("1/3/00", "D/M/YY") - Date2Num("1/2/00", "D/M/YY")"#,
                29.,
            ),
            (r#"Date2Num("Mar 15, 1996")"#, 35138.),
            (r#"Date2Num("Aug 1, 1996", "MMM D, YYYY")"#, 35277.),
        ] {
            assert_eq!(
                run(&runtime, program).as_number(),
                Some(expected),
                "{program}"
            );
        }
    }

    // cfxjse_formcalc_context_embeddertest.cpp: Num2Date, all active source
    // assertions plus the disabled nested Date2Num example.
    #[test]
    fn source_num_to_date_programs() {
        let runtime = runtime();
        for (program, expected) in [
            (r#"Num2Date(1, "DD/MM/YYYY")"#, "01/01/1900"),
            (r#"Num2Date(35139, "DD-MMM-YYYY", "de_DE")"#, "16-Mrz-1996"),
            (
                r#"Num2Date(Date2Num("Mar 15, 2000") - Date2Num("98-03-15", "YY-MM-DD", "fr_CA"))"#,
                "Jan 1, 1902",
            ),
        ] {
            assert_eq!(
                run(&runtime, program).as_string().unwrap(),
                JsString::from(expected),
                "{program}"
            );
        }
    }

    #[test]
    fn calendar_conversion_boundaries_nulls_and_recovery() {
        let runtime = runtime();
        for (program, expected) in [
            (r#"Date2Num("bad", "YYYY-MM-DD")"#, 0.),
            (r#"Date2Num("1900-02-29", "YYYY-MM-DD")"#, 0.),
            (r#"Date2Num("2024-02-29", "YYYY-MM-DD")"#, 45350.),
            (r#"Date2Num("9999-12-31", "YYYY-MM-DD")"#, 2958464.),
            (r#"Date2Num("2024-02-29", "YYYY-MM-DD", "x")"#, 0.),
        ] {
            assert_eq!(
                run(&runtime, program).as_number(),
                Some(expected),
                "{program}"
            );
        }
        for (program, expected) in [
            (r#"Num2Date(60, "YYYY-MM-DD")"#, "1900-03-01"),
            (r#"Num2Date(36584, "YYYY-MM-DD")"#, "2000-02-29"),
            (r#"Num2Date(2958464, "YYYY-MM-DD")"#, "9999-12-31"),
            (r#"Num2Date(2958465, "YYYY-MM-DD")"#, ""),
            (r#"Num2Date(Infinity, "YYYY-MM-DD")"#, ""),
            (r#"Num2Date(2147483647, "YYYY-MM-DD")"#, ""),
            (r#"Num2Date(1, "YYYY-MM-DD", "x")"#, ""),
            (r#"Num2Date(1.9, "YYYY-MM-DD")"#, "1900-01-01"),
        ] {
            assert_eq!(
                run(&runtime, program).as_string().unwrap(),
                JsString::from(expected),
                "{program}"
            );
        }
        for program in [
            "Date2Num(null)",
            "Date2Num(1,null)",
            "Date2Num(1,1,null)",
            "Num2Date(null)",
            "Num2Date(0)",
            "Num2Date(-1)",
            "Num2Date(NaN)",
            "Num2Date(1,null)",
            "Num2Date(1,1,null)",
        ] {
            assert!(run(&runtime, program).is_null(), "{program}");
        }
        for program in [
            "Date2Num()",
            "Date2Num(1,2,3,4)",
            "Num2Date()",
            "Num2Date(1,2,3,4)",
        ] {
            assert!(runtime.execute(&wide(program)).is_err(), "{program}");
        }
        // Genuine array-property getters preserve thrown object identity and
        // leave no locale/host borrow active across extraction or recovery.
        runtime
            .engine()
            .eval(
                &"globalThis.marker={};globalThis.input=[0,'v',{get v(){throw marker}}]"
                    .encode_utf16()
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        for program in ["Date2Num(input)", "Num2Date(input)"] {
            let error = runtime.execute(&wide(program)).unwrap_err();
            assert!(runtime
                .engine()
                .with_context(|context| {
                    let opaque = error.into_opaque(context)?;
                    let marker = context.global_object().get(js_string!("marker"), context)?;
                    Ok(opaque == marker)
                })
                .unwrap());
        }
        assert_eq!(run(&runtime, "2+3"), 5.into());
    }
}
