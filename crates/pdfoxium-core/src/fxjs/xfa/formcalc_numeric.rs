// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Numeric and financial methods from `fxjs/xfa/cfxjse_formcalc_context.cpp`.
//! FormCalc coercions and expansion are shared with the operator bindings;
//! decimal rounding uses the direct CFGAS_Decimal port.
use super::formcalc_values::{
    apply_to_expansion, argument_mismatch, exception, extract_double, get_simple_value,
    param_count, value_is_null, value_to_double, value_to_float,
};
use crate::xfa::fgas::crt::cfgas_decimal::CfgasDecimal;
use boa_engine::{native_function::NativeFunctionPointer, Context, JsResult, JsValue};

pub const METHODS: &[(&str, NativeFunctionPointer)] = &[
    ("Abs", abs),
    ("Avg", avg),
    ("Ceil", ceil),
    ("Count", count),
    ("Floor", floor),
    ("Max", max),
    ("Min", min),
    ("Mod", modulus),
    ("Round", round),
    ("Sum", sum),
    ("Apr", apr),
    ("Cterm", c_term),
    ("FV", fv),
    ("Ipmt", i_pmt),
    ("NPV", npv),
    ("Pmt", pmt),
    ("PPmt", p_pmt),
    ("PV", pv),
    ("Rate", rate),
    ("Term", term),
];

fn exact_count(args: &[JsValue], expected: usize, method: &str) -> JsResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(param_count(method))
    }
}
fn simple_values<const N: usize>(
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<[JsValue; N]> {
    let mut values = std::array::from_fn(|_| JsValue::undefined());
    for (index, value) in values.iter_mut().enumerate() {
        *value = get_simple_value(args, index, context)?;
    }
    Ok(values)
}
fn any_null(values: &[JsValue], context: &mut Context) -> JsResult<bool> {
    for value in values {
        if value_is_null(value, context)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn abs(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 1, "Abs")?;
    if value_is_null(&args[0], context)? {
        return Ok(JsValue::null());
    }
    let value = value_to_double(&args[0], context)?;
    // Preserve negative zero: the source tests value < 0 rather than fabs.
    Ok(if value < 0.0 { -value } else { value }.into())
}
fn avg(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (mut count, mut sum) = (0u32, 0.0);
    apply_to_expansion(args, false, context, |value, context| {
        sum += value_to_double(&value, context)?;
        count = count.wrapping_add(1);
        Ok(())
    })?;
    Ok(if count == 0 {
        JsValue::null()
    } else {
        (sum / f64::from(count)).into()
    })
}
fn ceil(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 1, "Ceil")?;
    let value = get_simple_value(args, 0, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    Ok(f64::from(value_to_float(&value, context)?.ceil()).into())
}
fn count(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let mut count = 0u32;
    apply_to_expansion(args, true, context, |_, _| {
        count = count.wrapping_add(1);
        Ok(())
    })?;
    Ok(count.into())
}
fn floor(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 1, "Floor")?;
    let value = get_simple_value(args, 0, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    Ok(f64::from(value_to_float(&value, context)?.floor()).into())
}
fn max(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (mut count, mut result) = (0u32, 0.0);
    apply_to_expansion(args, true, context, |value, context| {
        count = count.wrapping_add(1);
        let value = value_to_double(&value, context)?;
        // std::max keeps its first operand for unordered comparisons.
        if count == 1 || result < value {
            result = value;
        }
        Ok(())
    })?;
    Ok(if count == 0 {
        JsValue::null()
    } else {
        result.into()
    })
}
fn min(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (mut count, mut result) = (0u32, 0.0);
    apply_to_expansion(args, true, context, |value, context| {
        count = count.wrapping_add(1);
        let value = value_to_double(&value, context)?;
        if count == 1 || value < result {
            result = value;
        }
        Ok(())
    })?;
    Ok(if count == 0 {
        JsValue::null()
    } else {
        result.into()
    })
}
fn modulus(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 2, "Mod")?;
    if args[0].is_null() || args[1].is_null() {
        return Ok(JsValue::null());
    }
    let dividend = extract_double(&args[0], context)?.ok_or_else(argument_mismatch)?;
    let divisor = extract_double(&args[1], context)?.ok_or_else(argument_mismatch)?;
    if divisor == 0.0 {
        return Err(exception("Divide by zero."));
    }
    // Truncation gives the source result without its undefined out-of-range
    // double-to-int32 conversion for large quotients or NaN.
    Ok((dividend - divisor * (dividend / divisor).trunc()).into())
}
fn round(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if !(1..=2).contains(&args.len()) {
        return Err(param_count("Round"));
    }
    if args[0].is_null() {
        return Ok(JsValue::null());
    }
    let value = extract_double(&args[0], context)?.ok_or_else(argument_mismatch)?;
    let precision = if args.len() == 2 {
        if args[1].is_null() {
            return Ok(JsValue::null());
        }
        let precision = extract_double(&args[1], context)?.ok_or_else(argument_mismatch)?;
        if precision.is_nan() {
            return Err(argument_mismatch());
        }
        precision.clamp(0.0, 12.0) as u8
    } else {
        0
    };
    if !value.is_finite() {
        return Ok(value.into());
    }
    let decimal =
        CfgasDecimal::try_from_f32(value as f32, precision).ok_or_else(argument_mismatch)?;
    Ok(decimal.to_double().into())
}
fn sum(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let (mut count, mut sum) = (0u32, 0.0);
    apply_to_expansion(args, true, context, |value, context| {
        count = count.wrapping_add(1);
        sum += value_to_double(&value, context)?;
        Ok(())
    })?;
    Ok(if count == 0 {
        JsValue::null()
    } else {
        sum.into()
    })
}

/// Source GetValidatedPaymentPeriods: truncate a positive, representable count.
/// Nonfinite input is rejected before narrowing instead of invoking C++ UB.
fn payment_periods(value: &JsValue, context: &mut Context) -> JsResult<i32> {
    let periods = value_to_double(value, context)?;
    Ok(
        if periods.is_finite() && (1.0..=f64::from(i32::MAX)).contains(&periods) {
            periods as i32
        } else {
            0
        },
    )
}
fn repeated_power(base: f64, periods: i32) -> f64 {
    let mut value = 1.0;
    for index in 0..periods {
        let previous = value;
        value *= base;
        // Repeated multiplication has reached a terminal value. Avoid billions
        // of redundant operations for a valid but very large period count.
        if value.is_infinite() {
            return if base < 0.0 && (periods - index - 1) % 2 != 0 {
                -value
            } else {
                value
            };
        }
        if value.is_nan() || (base >= 0.0 && value == previous) || (value == 0.0 && base >= 0.0) {
            break;
        }
    }
    value
}
fn apr(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 3, "Apr")?;
    let values = simple_values::<3>(args, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let principal = value_to_double(&values[0], context)?;
    let payment = value_to_double(&values[1], context)?;
    let periods = payment_periods(&values[2], context)?;
    if principal <= 0.0 || payment <= 0.0 || periods == 0 {
        return Err(argument_mismatch());
    }
    let n = f64::from(periods);
    let mut rate = 2.0 * (n * payment - principal) / (n * principal);
    let mut power = repeated_power(1.0 + rate, periods);
    let mut result = rate * power / (power - 1.0) - payment / principal;
    let mut iterations = 0;
    while result.abs() > 0.00000001 {
        let derivative = ((power + rate * n * (power / (1.0 + rate))) * (power - 1.0)
            - rate * power * n * (power / (1.0 + rate)))
            / ((power - 1.0) * (power - 1.0));
        if derivative == 0.0 {
            return Ok(JsValue::null());
        }
        rate -= result / derivative;
        power = repeated_power(1.0 + rate, periods);
        result = rate * power / (power - 1.0) - payment / principal;
        iterations += 1;
        // The source has no termination bound for a nonconvergent Newton
        // iteration. Fail explicitly rather than hanging the script host.
        if iterations == 1024 {
            return Err(argument_mismatch());
        }
    }
    Ok((rate * 12.0).into())
}
fn c_term(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 3, "CTerm")?;
    let values = simple_values::<3>(args, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let rate = value_to_float(&values[0], context)?;
    let future = value_to_float(&values[1], context)?;
    let initial = value_to_float(&values[2], context)?;
    if rate <= 0.0 || future <= 0.0 || initial <= 0.0 {
        return Err(argument_mismatch());
    }
    Ok((f64::from(future / initial).ln() / f64::from(1.0 + rate).ln()).into())
}
fn fv(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 3, "FV")?;
    let values = simple_values::<3>(args, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let amount = value_to_double(&values[0], context)?;
    let rate = value_to_double(&values[1], context)?;
    let periods = payment_periods(&values[2], context)?;
    if amount <= 0.0 || rate < 0.0 || periods == 0 {
        return Err(argument_mismatch());
    }
    Ok(if rate != 0.0 {
        amount * (repeated_power(1.0 + rate, periods) - 1.0) / rate
    } else {
        amount * f64::from(periods)
    }
    .into())
}
fn i_pmt(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    amortized_payment(args, false, context)
}
fn p_pmt(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    amortized_payment(args, true, context)
}
fn amortized_payment(
    args: &[JsValue],
    principal_payment: bool,
    context: &mut Context,
) -> JsResult<JsValue> {
    exact_count(args, 5, if principal_payment { "PPmt" } else { "IPmt" })?;
    let values = simple_values::<5>(args, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let mut principal = value_to_float(&values[0], context)?;
    let rate = value_to_float(&values[1], context)?;
    let payment = value_to_float(&values[2], context)?;
    let first_month = value_to_float(&values[3], context)?;
    let number_of_months = value_to_float(&values[4], context)?;
    if principal <= 0.0
        || rate <= 0.0
        || payment <= 0.0
        || first_month < 0.0
        || number_of_months < 0.0
    {
        return Err(argument_mismatch());
    }
    let monthly_rate = rate / 12.0;
    // Source computes logarithms before this check, leading to an undefined
    // NaN-to-int conversion for a payment below the monthly interest.
    if payment < principal * monthly_rate {
        return if principal_payment {
            Err(argument_mismatch())
        } else {
            Ok(0.into())
        };
    }
    let periods = ((f64::from(payment / principal).log10()
        - f64::from(payment / principal - monthly_rate).log10())
        / f64::from(1.0 + monthly_rate).log10()) as i32;
    let end = ((first_month + number_of_months - 1.0) as i32).min(periods);
    // A range entirely after repayment has no contributions; the source still
    // simulates all preceding months even though its sum remains zero.
    if first_month - 1.0 >= end as f32 {
        return Ok(0.into());
    }
    let mut index = 0i32;
    while (index as f32) < first_month - 1.0 {
        principal -= payment - principal * monthly_rate;
        index = index.checked_add(1).ok_or_else(argument_mismatch)?;
    }
    let mut sum = 0.0f32;
    while index < end {
        if principal_payment {
            let contribution = payment - principal * monthly_rate;
            sum += contribution;
            principal -= contribution;
        } else {
            sum += principal * monthly_rate;
            principal -= payment - principal * monthly_rate;
        }
        index += 1;
    }
    Ok(f64::from(sum).into())
}
fn npv(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() < 2 {
        return Err(param_count("NPV"));
    }
    let value = get_simple_value(args, 0, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    let rate = value_to_double(&value, context)?;
    if rate <= 0.0 {
        return Err(argument_mismatch());
    }
    let mut data = Vec::with_capacity(args.len() - 1);
    for index in 1..args.len() {
        let value = get_simple_value(args, index, context)?;
        if value_is_null(&value, context)? {
            return Ok(JsValue::null());
        }
        data.push(value_to_double(&value, context)?);
    }
    let mut sum = 0.0;
    while let Some(value) = data.pop() {
        sum = (sum + value) / (1.0 + rate);
    }
    Ok(sum.into())
}
fn pmt(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 3, "Pmt")?;
    let values = simple_values::<3>(args, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let principal = value_to_double(&values[0], context)?;
    let rate = value_to_double(&values[1], context)?;
    let periods = payment_periods(&values[2], context)?;
    if principal <= 0.0 || rate <= 0.0 || periods == 0 {
        return Err(argument_mismatch());
    }
    let power = (1.0 + rate).powf(f64::from(periods));
    Ok((principal * rate * power / (power - 1.0)).into())
}
fn pv(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 3, "PV")?;
    let values = simple_values::<3>(args, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let amount = value_to_double(&values[0], context)?;
    let rate = value_to_double(&values[1], context)?;
    let periods = payment_periods(&values[2], context)?;
    if amount <= 0.0 || rate < 0.0 || periods == 0 {
        return Err(argument_mismatch());
    }
    // Zero is accepted by the source's validation. Use the continuous limit
    // instead of its subsequent 0/0.
    if rate == 0.0 {
        return Ok((amount * f64::from(periods)).into());
    }
    let power = 1.0 / (1.0 + rate).powf(f64::from(periods));
    Ok((amount * ((1.0 - power) / rate)).into())
}
fn rate(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 3, "Rate")?;
    let values = simple_values::<3>(args, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let future = value_to_float(&values[0], context)?;
    let present = value_to_float(&values[1], context)?;
    let periods = payment_periods(&values[2], context)?;
    if future <= 0.0 || present < 0.0 || periods == 0 {
        return Err(argument_mismatch());
    }
    Ok(f64::from((future / present).powf(1.0 / periods as f32) - 1.0).into())
}
fn term(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exact_count(args, 3, "Term")?;
    let values = simple_values::<3>(args, context)?;
    if any_null(&values, context)? {
        return Ok(JsValue::null());
    }
    let amount = value_to_float(&values[0], context)?;
    let rate = value_to_float(&values[1], context)?;
    let future = value_to_float(&values[2], context)?;
    if amount <= 0.0 || rate <= 0.0 || future <= 0.0 {
        return Err(argument_mismatch());
    }
    Ok((f64::from(future / amount * rate + 1.0).ln() / f64::from(1.0 + rate).ln()).into())
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::super::cfxjse_formcalc_context::CfxJseFormcalcContext;
    use boa_engine::JsValue;

    fn execute(runtime: &CfxJseFormcalcContext, source: &str) -> JsValue {
        runtime
            .execute(&source.chars().map(u32::from).collect::<Vec<_>>())
            .unwrap_or_else(|error| panic!("{source}: {error:?}"))
    }
    fn expect_int(runtime: &CfxJseFormcalcContext, source: &str, expected: i32) {
        assert_eq!(
            execute(runtime, source).as_number(),
            Some(f64::from(expected)),
            "{source}"
        );
    }
    fn expect_float(runtime: &CfxJseFormcalcContext, source: &str, expected: f32) {
        let actual = execute(runtime, source)
            .as_number()
            .expect("numeric result") as f32;
        // EXPECT_FLOAT_EQ permits four float ULPs. These source vectors are
        // finite and positive, so the bit representation is monotonic.
        assert!(
            actual == expected || actual.to_bits().abs_diff(expected.to_bits()) <= 4,
            "{source}: actual {actual}, expected {expected}"
        );
    }
    fn expect_near(runtime: &CfxJseFormcalcContext, source: &str, expected: f32) {
        let actual = execute(runtime, source)
            .as_number()
            .expect("numeric result") as f32;
        assert!(
            (actual - expected).abs() <= 0.000001,
            "{source}: actual {actual}, expected {expected}"
        );
    }
    fn expect_error(runtime: &CfxJseFormcalcContext, source: &str) {
        assert!(
            runtime
                .execute(&source.chars().map(u32::from).collect::<Vec<_>>())
                .is_err(),
            "{source}"
        );
    }
    fn expect_nan(runtime: &CfxJseFormcalcContext, source: &str) {
        assert!(
            execute(runtime, source)
                .as_number()
                .expect("numeric result")
                .is_nan(),
            "{source}"
        );
    }

    // CFXJSEFormCalcContextEmbedderTest.Abs, all enabled inputs/assertions.
    #[test]
    fn Abs() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(&runtime, "Abs(1.03)", 1.03_f32);
        expect_float(&runtime, "Abs(-1.03)", 1.03_f32);
        expect_float(&runtime, "Abs(0)", 0.0_f32);
    }

    // CFXJSEFormCalcContextEmbedderTest.Avg, all enabled inputs/assertions.
    #[test]
    fn Avg() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(&runtime, "Avg(0, 32, 16)", 16.0_f32);
        expect_float(&runtime, "Avg(2.5, 17, null)", 9.75_f32);
    }

    // CFXJSEFormCalcContextEmbedderTest.Ceil, all enabled inputs/assertions.
    #[test]
    fn Ceil() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_int(&runtime, "Ceil(2.5875)", 3);
        expect_int(&runtime, "Ceil(-5.9)", -5);
        expect_int(&runtime, "Ceil(\"abc\")", 0);
    }

    // CFXJSEFormCalcContextEmbedderTest.Count, all enabled inputs/assertions.
    #[test]
    fn Count() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_int(&runtime, "Count(\"Tony\", \"Blue\", 41)", 3);
    }

    // CFXJSEFormCalcContextEmbedderTest.Floor, all enabled inputs/assertions.
    #[test]
    fn Floor() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_int(&runtime, "Floor(21.3409873)", 21);
        expect_int(&runtime, "Floor(5.999965342)", 5);
        expect_int(&runtime, "Floor(3.2 * 15)", 48);
    }

    // CFXJSEFormCalcContextEmbedderTest.Max, all enabled inputs/assertions.
    #[test]
    fn Max() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_int(&runtime, "Max(234, 15, 107)", 234);
        expect_int(&runtime, "Max(\"abc\", 15, \"Tony Blue\")", 15);
        expect_int(&runtime, "Max(\"abc\")", 0);
    }

    // CFXJSEFormCalcContextEmbedderTest.Min, all enabled inputs/assertions.
    #[test]
    fn Min() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_int(&runtime, "Min(234, 15, 107)", 15);
        expect_int(&runtime, "Min(\"abc\")", 0);
    }

    // CFXJSEFormCalcContextEmbedderTest.Mod, all enabled inputs/assertions.
    #[test]
    fn Mod() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_int(&runtime, "Mod(64, -3)", 1);
        expect_int(&runtime, "Mod(-13, 3)", -1);
        expect_int(&runtime, "Mod(\"abc\", 2)", 0);
        expect_nan(&runtime, "Mod(10, NaN)");
        expect_nan(&runtime, "Mod(10, Infinity)");
    }

    // CFXJSEFormCalcContextEmbedderTest.Round, all enabled inputs/assertions.
    #[test]
    fn Round() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(&runtime, "Round(12.389764537, 4)", 12.3898_f32);
        expect_float(&runtime, "Round(20/3, 2)", 6.67_f32);
        expect_float(&runtime, "Round(8.9897, \"abc\")", 9.0_f32);
        expect_float(&runtime, "Round(FV(400, 0.10/12, 30*12), 2)", 904195.17_f32);
    }

    // CFXJSEFormCalcContextEmbedderTest.Sum, all enabled inputs/assertions.
    #[test]
    fn Sum() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_int(&runtime, "Sum(2, 4, 6, 8)", 20);
        expect_int(&runtime, "Sum(-2, 4, -6, 8)", 4);
        expect_int(&runtime, "Sum(4, 16, \"abc\", 19)", 39);
    }

    // CFXJSEFormCalcContextEmbedderTest.Apr, all enabled inputs/assertions.
    #[test]
    fn Apr() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_near(&runtime, "Apr(35000, 269.50, 360)", 0.08515404566_f32);
        expect_near(
            &runtime,
            "Apr(210000 * 0.75, 850 + 110, 25 * 26)",
            0.07161332404_f32,
        );
        expect_error(&runtime, "Apr(2, 2, 2147483648)");
    }

    // CFXJSEFormCalcContextEmbedderTest.CTerm, all enabled inputs/assertions.
    #[test]
    fn CTerm() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(&runtime, "CTerm(0.10, 500000, 12000)", 39.13224648502_f32);
    }

    // CFXJSEFormCalcContextEmbedderTest.FV, all enabled inputs/assertions.
    #[test]
    fn FV() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(
            &runtime,
            "FV(400, 0.10 / 12, 30 * 12)",
            904195.16991842445_f32,
        );
        expect_float(
            &runtime,
            "FV(1000, 0.075 / 4, 10 * 4)",
            58791.96145535981_f32,
        );
        expect_error(&runtime, "FV(2, 2, 2147483648)");
    }

    // CFXJSEFormCalcContextEmbedderTest.IPmt, all enabled inputs/assertions.
    #[test]
    fn IPmt() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(
            &runtime,
            "IPmt(30000, 0.085, 295.50, 7, 3)",
            624.8839283142_f32,
        );
        expect_float(
            &runtime,
            "IPmt(160000, 0.0475, 980, 24, 12)",
            7103.80833569485_f32,
        );
        expect_float(&runtime, "IPmt(15000, 0.065, 65.50, 15, 1)", 0.0_f32);
    }

    // CFXJSEFormCalcContextEmbedderTest.NPV, all enabled inputs/assertions.
    #[test]
    fn NPV() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(&runtime, "NPV(0.065, 5000)", 4694.83568075117_f32);
        expect_float(
            &runtime,
            "NPV(0.10, 500, 1500, 4000, 10000)",
            11529.60863329007_f32,
        );
        expect_float(
            &runtime,
            "NPV(0.0275 / 12, 50, 60, 40, 100, 25)",
            273.14193838457_f32,
        );
    }

    // CFXJSEFormCalcContextEmbedderTest.Pmt, all enabled inputs/assertions.
    #[test]
    fn Pmt() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(&runtime, "Pmt(25000, 0.085, 12)", 3403.82145169876_f32);
        expect_float(&runtime, "Pmt(5000, 0.01, 1)", 5050_f32);
        expect_float(&runtime, "Pmt(5000, 0.01, 1.5)", 5050_f32);
        expect_float(
            &runtime,
            "Pmt(30000.00, .085 / 12, 12 * 12)",
            333.01666929435_f32,
        );
        expect_float(&runtime, "Pmt(10000, .08 / 12, 10)", 1037.03208935916_f32);
        expect_float(
            &runtime,
            "Pmt(150000, 0.0475 / 12, 25 * 12)",
            855.17604207164_f32,
        );
        expect_error(&runtime, "Pmt(2, 2, 99999997952)");
    }

    // CFXJSEFormCalcContextEmbedderTest.PPmt, all enabled inputs/assertions.
    #[test]
    fn PPmt() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(
            &runtime,
            "PPmt(30000, 0.085, 295.50, 7, 3)",
            261.6160716858_f32,
        );
        expect_float(
            &runtime,
            "PPmt(160000, 0.0475, 980, 24, 12)",
            4656.19166430515_f32,
        );
    }

    // CFXJSEFormCalcContextEmbedderTest.PV, all enabled inputs/assertions.
    #[test]
    fn PV() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(
            &runtime,
            "PV(400, 0.10 / 12, 30 * 12)",
            45580.32799074439_f32,
        );
        expect_float(
            &runtime,
            "PV(1000, 0.075 / 4, 10 * 4)",
            27964.88770467326_f32,
        );
        expect_error(&runtime, "PV(2, 2, 2147483648)");
    }

    // CFXJSEFormCalcContextEmbedderTest.Rate, all enabled inputs/assertions.
    #[test]
    fn Rate() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_near(&runtime, "Rate(12000, 8000, 5)", 0.0844717712_f32);
        expect_near(
            &runtime,
            "Rate(10000, 0.25 * 5000, 4 * 12)",
            0.04427378243_f32,
        );
        expect_error(&runtime, "Rate(2, 2, 2147483648)");
    }

    // CFXJSEFormCalcContextEmbedderTest.Term, all enabled inputs/assertions.
    #[test]
    fn Term() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        expect_float(
            &runtime,
            "Term(2500, 0.0275 + 0.0025, 5000)",
            1.97128786369_f32,
        );
    }

    #[test]
    fn nulls_argument_counts_and_financial_validation() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        for source in [
            "Abs(null)",
            "Avg(null)",
            "Ceil(null)",
            "Floor(null)",
            "Max(null)",
            "Min(null)",
            "Mod(null, 0)",
            "Round(null)",
            "Round(5, null)",
            "Sum(null, null)",
            "Apr(null, 2, 3)",
            "CTerm(1, null, 3)",
            "FV(1, 1, null)",
            "IPmt(1, 1, 1, 1, null)",
            "NPV(1, 1, null)",
            "Pmt(null, 1, 1)",
            "PPmt(1, 1, null, 1, 1)",
            "PV(1, null, 1)",
            "Rate(1, 1, null)",
            "Term(null, 1, 1)",
            "Avg()",
            "Max()",
            "Min()",
            "Sum()",
        ] {
            assert!(execute(&runtime, source).is_null(), "{source}");
        }
        expect_int(&runtime, "Count()", 0);
        expect_int(&runtime, "Count(null)", 0);
        for source in [
            "Abs()",
            "Abs(1, 2)",
            "Ceil()",
            "Floor()",
            "Mod(1)",
            "Round()",
            "Round(1, 2, 3)",
            "Apr(1, 2)",
            "CTerm()",
            "FV(1)",
            "IPmt(1, 2, 3)",
            "NPV(1)",
            "Pmt(1, 2)",
            "PPmt(1)",
            "PV()",
            "Rate(1)",
            "Term(1, 2)",
            "Mod(1, 0)",
            "Apr(0, 1, 1)",
            "CTerm(0, 1, 1)",
            "FV(1, -1, 1)",
            "IPmt(1, 0, 1, 1, 1)",
            "NPV(0, 1)",
            "Pmt(1, 0, 1)",
            "PPmt(15000, 0.065, 65.50, 15, 1)",
            "PV(1, -1, 1)",
            "Rate(0, 1, 1)",
            "Term(1, 0, 1)",
        ] {
            expect_error(&runtime, source);
        }
        for method in ["Apr", "FV", "Pmt", "PV", "Rate"] {
            for periods in ["0", "0.5", "-1", "2147483648", "NaN", "Infinity"] {
                expect_error(&runtime, &format!("{method}(1, 1, {periods})"));
            }
        }
        expect_int(&runtime, "FV(25, 0, 12)", 300);
        expect_int(&runtime, "PV(25, 0, 12)", 300);
        expect_int(&runtime, "Round(2.5, -5)", 3);
        expect_int(&runtime, "Mod(4294967296, 2)", 0);
    }

    #[test]
    fn native_expansion_and_decimal_error_paths() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        // Named-property expansions are ordinary JS objects here; XFA node
        // default-property resolution remains the shared adapter's open edge.
        let script = r#"
            function check(value) { if (!value) throw Error('numeric assertion'); }
            let calls = 0;
            let value = { get amount() { calls++; return 4; } };
            let expansion = [0, 'amount', value, { amount: null }, { amount: '6' }];
            check(pfm_rt.Sum(expansion, 2) === 12);
            check(pfm_rt.Avg(expansion) === 5);
            check(pfm_rt.Max(expansion) === 6 && pfm_rt.Min(expansion) === 4);
            check(pfm_rt.Count(expansion) === 2 && calls === 5);
            check(pfm_rt.Count(undefined, null, {}, [0, 'amount', { amount: undefined }]) === 2);
            check(pfm_rt.Avg([]) === null);
            for (let method of ['Sum', 'Min', 'Max', 'Count']) {
                let caught = false;
                try { pfm_rt[method]([]); } catch (e) { caught = e.message === 'Argument mismatch in property or function argument.'; }
                check(caught);
            }
            for (let operation of [() => pfm_rt.Mod([], 2), () => pfm_rt.Round([]), () => pfm_rt.Round(1, []), () => pfm_rt.Round(1, NaN), () => pfm_rt.Round(1e100)]) {
                let caught = false;
                try { operation(); } catch (e) { caught = e.message === 'Argument mismatch in property or function argument.'; }
                check(caught);
            }
            check(Object.is(pfm_rt.Abs(-0), -0));
            check(Number.isNaN(pfm_rt.Round(NaN)) && pfm_rt.Round(Infinity) === Infinity);
            check(pfm_rt.FV(1, 2, 2147483647) === Infinity);
            check(pfm_rt.Apr(2147483647, 1, 2147483647) === 0);
        "#;
        runtime
            .engine()
            .eval(&script.encode_utf16().collect::<Vec<_>>())
            .unwrap();
    }
}
