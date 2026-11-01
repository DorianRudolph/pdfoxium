// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Date, Time and GUIDString/Uuid from `cfxjse_formcalc_context.cpp`.
//! Boa supplies the clock and intrinsic Date fields. The explicit GMT results
//! of these two FormCalc methods do not change JavaScript's local timezone.
use super::formcalc_iso::date_string_to_num;
use super::formcalc_values::{get_simple_value, param_count, value_to_float};
use crate::fxcrt::{fx_extension::int_to_two_hex_chars, fx_random};
use boa_engine::{
    native_function::NativeFunctionPointer, object::builtins::JsDate, Context, JsResult, JsString,
    JsValue,
};

pub(super) const METHODS: &[(&str, NativeFunctionPointer)] =
    &[("Date", date), ("Time", time), ("Uuid", uuid)];

fn date(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if !args.is_empty() {
        return Err(param_count("Date"));
    }
    let now = JsDate::new(context);
    let year = now
        .get_utc_full_year(context)?
        .as_number()
        .unwrap_or(f64::NAN);
    let month = now.get_utc_month(context)?.as_number().unwrap_or(f64::NAN);
    let day = now.get_utc_date(context)?.as_number().unwrap_or(f64::NAN);
    if !year.is_finite() || !month.is_finite() || !day.is_finite() {
        return Ok(0.into());
    }
    let canonical = format!("{:04}{:02}{:02}", year as i32, month as i32 + 1, day as i32);
    Ok(date_string_to_num(canonical.as_bytes()).into())
}

fn time(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if !args.is_empty() {
        return Err(param_count("Time"));
    }
    let now = JsDate::new(context);
    let hour = now.get_utc_hours(context)?.as_number().unwrap_or(f64::NAN);
    let minute = now
        .get_utc_minutes(context)?
        .as_number()
        .unwrap_or(f64::NAN);
    let second = now
        .get_utc_seconds(context)?
        .as_number()
        .unwrap_or(f64::NAN);
    // The source returns whole seconds since GMT midnight, without the +1
    // encoding used by Time2Num. Retain that public distinction.
    Ok(((hour * 3600.0 + minute * 60.0 + second) * 1000.0).into())
}

fn guid_string(mut data: [u8; 16], separator: bool) -> JsValue {
    data[6] = (data[6] & 0x0f) | 0x40;
    // Complete the UUID v4 variant that the source GUID helper omits.
    data[8] = (data[8] & 0x3f) | 0x80;
    let mut output = Vec::with_capacity(if separator { 36 } else { 32 });
    for (index, byte) in data.into_iter().enumerate() {
        if separator && matches!(index, 4 | 6 | 8 | 10) {
            output.push(b'-');
        }
        let mut pair = [0; 2];
        int_to_two_hex_chars(byte, &mut pair);
        output.extend_from_slice(&pair);
    }
    JsString::from(std::str::from_utf8(&output).expect("GUID hex is ASCII")).into()
}

fn uuid(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() > 1 {
        return Err(param_count("Uuid"));
    }
    let separator = if args.is_empty() {
        false
    } else {
        let argument = get_simple_value(args, 0, context)?;
        value_to_float(&argument, context)? as i32 != 0
    };
    let mut words = [0u32; 4];
    fx_random::fill(&mut words);
    let mut data = [0; 16];
    for (word, bytes) in words.into_iter().zip(data.chunks_exact_mut(4)) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    Ok(guid_string(data, separator))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxjs::xfa::cfxjse_formcalc_context::{execute_in_context, register_formcalc};
    use boa_engine::{context::time::FixedClock, context::HostHooks, Source};
    use std::rc::Rc;

    #[derive(Debug)]
    struct Offset;
    impl HostHooks for Offset {
        fn local_timezone_offset_seconds(&self, _: i64) -> i32 {
            9 * 3600
        }
    }
    fn execute(context: &mut Context, script: &str) -> JsValue {
        execute_in_context(context, &script.chars().map(u32::from).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn date_and_time_use_boa_clock_and_source_gmt_contract() {
        // 2024-02-29 23:59:58.321 GMT, already March in the local zone.
        let clock = Rc::new(FixedClock::from_millis(1_709_251_198_321));
        let mut context = Context::builder()
            .clock(clock.clone())
            .host_hooks(Rc::new(Offset))
            .build()
            .unwrap();
        register_formcalc(&mut context).unwrap();
        assert_eq!(execute(&mut context, "Date()"), 45350.into());
        assert_eq!(execute(&mut context, "Time()"), 86_398_000.into());
        assert_eq!(
            context
                .eval(Source::from_bytes("new Date().getDate()"))
                .unwrap(),
            1.into()
        );
        clock.forward(2000);
        assert_eq!(execute(&mut context, "Date()"), 45351.into());
        assert_eq!(execute(&mut context, "Time()"), 0.into());
        for name in ["Date", "Time"] {
            assert!(execute_in_context(
                &mut context,
                &format!("{name}(1)")
                    .chars()
                    .map(u32::from)
                    .collect::<Vec<_>>()
            )
            .is_err());
        }
        // PDF scripts cannot replace the intrinsic calendar used by the host.
        context
            .eval(Source::from_bytes("Date=function(){throw 'replaced'}"))
            .unwrap();
        assert_eq!(execute(&mut context, "Date()"), 45351.into());
    }

    #[test]
    fn uuid_native_embedder_vectors_and_version_variant() {
        let mut context = Context::default();
        register_formcalc(&mut context).unwrap();
        let first = execute(&mut context, "Uuid()")
            .as_string()
            .unwrap()
            .to_std_string_escaped();
        assert_eq!(first.len(), 32);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(first.bytes().any(|byte| byte != b'0'));
        assert_eq!(first.as_bytes()[12], b'4');
        assert!(matches!(first.as_bytes()[16], b'8' | b'9' | b'A' | b'B'));
        let second = execute(&mut context, "Uuid(1)")
            .as_string()
            .unwrap()
            .to_std_string_escaped();
        assert_eq!(second.len(), 36);
        for (index, byte) in second.bytes().enumerate() {
            assert!(if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            });
        }
        assert_ne!(first, second.replace('-', ""));
        assert_eq!(
            guid_string([0; 16], true),
            JsString::from("00000000-0000-4000-8000-000000000000").into()
        );
        for program in ["Uuid(null)", "Uuid(0)", "Uuid(0.5)"] {
            assert_eq!(
                execute(&mut context, program).as_string().unwrap().len(),
                32
            );
        }
        assert!(execute_in_context(
            &mut context,
            &"Uuid(0,1)".chars().map(u32::from).collect::<Vec<_>>()
        )
        .is_err());
    }
}
