// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_psengine_unittest.cpp`.
#![allow(non_snake_case)]

use super::{add_operator, CpdfPsEngine, PsOperator, PsToken};
use crate::test_support::assert_float_eq;

fn DoOperator0(engine: &mut CpdfPsEngine, op: PsOperator) -> f32 {
    assert_eq!(engine.stack_size(), 0);
    engine.do_operator(op);
    let ret = engine.pop();
    assert_eq!(engine.stack_size(), 0);
    ret
}

fn DoOperator1(engine: &mut CpdfPsEngine, v1: f32, op: PsOperator) -> f32 {
    assert_eq!(engine.stack_size(), 0);
    engine.push(v1);
    engine.do_operator(op);
    let ret = engine.pop();
    assert_eq!(engine.stack_size(), 0);
    ret
}

fn DoOperator2(engine: &mut CpdfPsEngine, v1: f32, v2: f32, op: PsOperator) -> f32 {
    assert_eq!(engine.stack_size(), 0);
    engine.push(v1);
    engine.push(v2);
    engine.do_operator(op);
    let ret = engine.pop();
    assert_eq!(engine.stack_size(), 0);
    ret
}

#[test]
fn AddOperator() {
    let kTestData: &[(&[u8], Option<PsOperator>)] = &[
        (b"add", Some(PsOperator::Add)),
        (b"sub", Some(PsOperator::Sub)),
        (b"mul", Some(PsOperator::Mul)),
        (b"div", Some(PsOperator::Div)),
        (b"idiv", Some(PsOperator::IDiv)),
        (b"mod", Some(PsOperator::Mod)),
        (b"neg", Some(PsOperator::Neg)),
        (b"abs", Some(PsOperator::Abs)),
        (b"ceiling", Some(PsOperator::Ceiling)),
        (b"floor", Some(PsOperator::Floor)),
        (b"round", Some(PsOperator::Round)),
        (b"truncate", Some(PsOperator::Truncate)),
        (b"sqrt", Some(PsOperator::Sqrt)),
        (b"sin", Some(PsOperator::Sin)),
        (b"cos", Some(PsOperator::Cos)),
        (b"atan", Some(PsOperator::Atan)),
        (b"exp", Some(PsOperator::Exp)),
        (b"ln", Some(PsOperator::Ln)),
        (b"log", Some(PsOperator::Log)),
        (b"cvi", Some(PsOperator::Cvi)),
        (b"cvr", Some(PsOperator::Cvr)),
        (b"eq", Some(PsOperator::Eq)),
        (b"ne", Some(PsOperator::Ne)),
        (b"gt", Some(PsOperator::Gt)),
        (b"ge", Some(PsOperator::Ge)),
        (b"lt", Some(PsOperator::Lt)),
        (b"le", Some(PsOperator::Le)),
        (b"and", Some(PsOperator::And)),
        (b"or", Some(PsOperator::Or)),
        (b"xor", Some(PsOperator::Xor)),
        (b"not", Some(PsOperator::Not)),
        (b"bitshift", Some(PsOperator::Bitshift)),
        (b"true", Some(PsOperator::True)),
        (b"false", Some(PsOperator::False)),
        (b"if", Some(PsOperator::If)),
        (b"ifelse", Some(PsOperator::IfElse)),
        (b"pop", Some(PsOperator::Pop)),
        (b"exch", Some(PsOperator::Exch)),
        (b"dup", Some(PsOperator::Dup)),
        (b"copy", Some(PsOperator::Copy)),
        (b"index", Some(PsOperator::Index)),
        (b"roll", Some(PsOperator::Roll)),
        (b"55", None),
        (b"123.4", None),
        (b"-5", None),
        (b"invalid", None),
    ];
    let mut proc = Vec::new();
    for &(word, expected_op) in kTestData {
        add_operator(&mut proc, word);
        let new_psop = proc.last();
        assert!(new_psop.is_some());
        let new_psop = new_psop.unwrap();
        let new_op = match new_psop {
            PsToken::Operator(op) => Some(*op),
            PsToken::Constant(_) => None,
            PsToken::Procedure(_) => panic!("unexpected procedure"),
        };
        assert_eq!(new_op, expected_op);
        if new_op.is_none() {
            let PsToken::Constant(fv) = new_psop else {
                unreachable!()
            };
            match word {
                b"55" => assert_float_eq(55.0, *fv),
                b"123.4" => assert_float_eq(123.4, *fv),
                b"-5" => assert_float_eq(-5.0, *fv),
                b"invalid" => assert_float_eq(0.0, *fv),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn Basic() {
    let mut engine = CpdfPsEngine::new();

    assert_float_eq(
        300.0,
        DoOperator2(&mut engine, 100.0, 200.0, PsOperator::Add),
    );
    assert_float_eq(
        -50.0,
        DoOperator2(&mut engine, 100.0, 150.0, PsOperator::Sub),
    );
    assert_float_eq(600.0, DoOperator2(&mut engine, 5.0, 120.0, PsOperator::Mul));
    assert_float_eq(1.5, DoOperator2(&mut engine, 15.0, 10.0, PsOperator::Div));
    assert_float_eq(1.0, DoOperator2(&mut engine, 15.0, 10.0, PsOperator::IDiv));
    assert_float_eq(5.0, DoOperator2(&mut engine, 15.0, 10.0, PsOperator::Mod));
    assert_float_eq(5.0, DoOperator1(&mut engine, -5.0, PsOperator::Neg));
    assert_float_eq(5.0, DoOperator1(&mut engine, -5.0, PsOperator::Abs));
}

#[test]
fn DivByZero() {
    let mut engine = CpdfPsEngine::new();

    // Integer divide by zero is defined as resulting in 0.
    assert_float_eq(0.0, DoOperator2(&mut engine, 100.0, 0.0, PsOperator::IDiv));
    assert_float_eq(0.0, DoOperator2(&mut engine, 100.0, 0.0, PsOperator::Mod));

    // floating divide by zero is defined as resulting in 0.
    assert_float_eq(0.0, DoOperator2(&mut engine, 100.0, 0.0, PsOperator::Div));
}

#[test]
fn Ceiling() {
    let mut engine = CpdfPsEngine::new();

    // Smallest positive float value.
    let min_float = f32::MIN_POSITIVE;
    // Largest positive float value.
    let max_float = f32::MAX;
    assert_float_eq(
        1.0,
        DoOperator1(&mut engine, min_float, PsOperator::Ceiling),
    );
    assert_float_eq(
        max_float,
        DoOperator1(&mut engine, max_float, PsOperator::Ceiling),
    );
    assert_float_eq(
        0.0,
        DoOperator1(&mut engine, -min_float, PsOperator::Ceiling),
    );
    assert_float_eq(
        -max_float,
        DoOperator1(&mut engine, -max_float, PsOperator::Ceiling),
    );
    assert_float_eq(0.0, DoOperator1(&mut engine, -0.9, PsOperator::Ceiling));
    assert_float_eq(
        1.0,
        DoOperator1(&mut engine, 0.0000000001, PsOperator::Ceiling),
    );
    assert_float_eq(0.0, DoOperator1(&mut engine, 0.0, PsOperator::Ceiling));
    assert_float_eq(3.0, DoOperator1(&mut engine, 2.3, PsOperator::Ceiling));
    assert_float_eq(4.0, DoOperator1(&mut engine, 3.8, PsOperator::Ceiling));
    assert_float_eq(6.0, DoOperator1(&mut engine, 5.5, PsOperator::Ceiling));
    assert_float_eq(-2.0, DoOperator1(&mut engine, -2.3, PsOperator::Ceiling));
    assert_float_eq(-3.0, DoOperator1(&mut engine, -3.8, PsOperator::Ceiling));
    assert_float_eq(-5.0, DoOperator1(&mut engine, -5.5, PsOperator::Ceiling));
}

#[test]
fn Floor() {
    let mut engine = CpdfPsEngine::new();

    // Smallest positive float value.
    let min_float = f32::MIN_POSITIVE;
    // Largest positive float value.
    let max_float = f32::MAX;
    assert_float_eq(0.0, DoOperator1(&mut engine, min_float, PsOperator::Floor));
    assert_float_eq(
        max_float,
        DoOperator1(&mut engine, max_float, PsOperator::Floor),
    );
    assert_float_eq(
        -1.0,
        DoOperator1(&mut engine, -min_float, PsOperator::Floor),
    );
    assert_float_eq(
        -max_float,
        DoOperator1(&mut engine, -max_float, PsOperator::Floor),
    );
    assert_float_eq(5.0, DoOperator1(&mut engine, 5.9, PsOperator::Floor));
    assert_float_eq(
        -4.0,
        DoOperator1(&mut engine, -4.0000000001, PsOperator::Floor),
    );
    assert_float_eq(0.0, DoOperator1(&mut engine, 0.0, PsOperator::Floor));
    assert_float_eq(-1.0, DoOperator1(&mut engine, -0.9, PsOperator::Floor));
    assert_float_eq(
        0.0,
        DoOperator1(&mut engine, 0.0000000001, PsOperator::Floor),
    );
    assert_float_eq(0.0, DoOperator1(&mut engine, 0.0, PsOperator::Floor));
    assert_float_eq(2.0, DoOperator1(&mut engine, 2.3, PsOperator::Floor));
    assert_float_eq(3.0, DoOperator1(&mut engine, 3.8, PsOperator::Floor));
    assert_float_eq(5.0, DoOperator1(&mut engine, 5.5, PsOperator::Floor));
    assert_float_eq(-3.0, DoOperator1(&mut engine, -2.3, PsOperator::Floor));
    assert_float_eq(-4.0, DoOperator1(&mut engine, -3.8, PsOperator::Floor));
    assert_float_eq(-6.0, DoOperator1(&mut engine, -5.5, PsOperator::Floor));
}

#[test]
fn Round() {
    let mut engine = CpdfPsEngine::new();

    assert_float_eq(6.0, DoOperator1(&mut engine, 5.9, PsOperator::Round));
    assert_float_eq(
        -4.0,
        DoOperator1(&mut engine, -4.0000000001, PsOperator::Round),
    );
    assert_float_eq(0.0, DoOperator1(&mut engine, 0.0, PsOperator::Round));
    assert_float_eq(-1.0, DoOperator1(&mut engine, -0.9, PsOperator::Round));
    assert_float_eq(
        0.0,
        DoOperator1(&mut engine, 0.0000000001, PsOperator::Round),
    );
    assert_float_eq(0.0, DoOperator1(&mut engine, 0.0, PsOperator::Round));
    // Smallest positive float value.
    let min_float = f32::MIN_POSITIVE;
    // Largest positive float value.
    let max_float = f32::MAX;
    assert_float_eq(0.0, DoOperator1(&mut engine, min_float, PsOperator::Round));
    assert_float_eq(0.0, DoOperator1(&mut engine, -min_float, PsOperator::Round));
    assert_float_eq(
        max_float,
        DoOperator1(&mut engine, max_float, PsOperator::Round),
    );
    assert_float_eq(
        -max_float,
        DoOperator1(&mut engine, -max_float, PsOperator::Round),
    );
    assert_float_eq(2.0, DoOperator1(&mut engine, 2.3, PsOperator::Round));
    assert_float_eq(4.0, DoOperator1(&mut engine, 3.8, PsOperator::Round));
    assert_float_eq(6.0, DoOperator1(&mut engine, 5.5, PsOperator::Round));
    assert_float_eq(-2.0, DoOperator1(&mut engine, -2.3, PsOperator::Round));
    assert_float_eq(-4.0, DoOperator1(&mut engine, -3.8, PsOperator::Round));
    assert_float_eq(-5.0, DoOperator1(&mut engine, -5.5, PsOperator::Round));
}

#[test]
fn Truncate() {
    let mut engine = CpdfPsEngine::new();

    assert_float_eq(0.0, DoOperator1(&mut engine, -0.9, PsOperator::Truncate));
    assert_float_eq(
        0.0,
        DoOperator1(&mut engine, 0.0000000001, PsOperator::Truncate),
    );
    assert_float_eq(0.0, DoOperator1(&mut engine, 0.0, PsOperator::Truncate));
    // Smallest positive float value.
    let min_float = f32::MIN_POSITIVE;
    assert_float_eq(
        0.0,
        DoOperator1(&mut engine, min_float, PsOperator::Truncate),
    );
    assert_float_eq(
        0.0,
        DoOperator1(&mut engine, -min_float, PsOperator::Truncate),
    );
    assert_float_eq(2.0, DoOperator1(&mut engine, 2.3, PsOperator::Truncate));
    assert_float_eq(3.0, DoOperator1(&mut engine, 3.8, PsOperator::Truncate));
    assert_float_eq(5.0, DoOperator1(&mut engine, 5.5, PsOperator::Truncate));
    assert_float_eq(-2.0, DoOperator1(&mut engine, -2.3, PsOperator::Truncate));
    assert_float_eq(-3.0, DoOperator1(&mut engine, -3.8, PsOperator::Truncate));
    assert_float_eq(-5.0, DoOperator1(&mut engine, -5.5, PsOperator::Truncate));

    // Truncate does not behave according to the PostScript Language Reference for
    // values beyond the range of integers. This seems to match Acrobat's
    // behavior. See https://crbug.com/42270316.
    let max_int = i32::MAX as f32;
    assert_float_eq(
        -max_int,
        DoOperator1(&mut engine, max_int * -1.5, PsOperator::Truncate),
    );
}

#[test]
fn Comparisons() {
    let mut engine = CpdfPsEngine::new();

    assert_float_eq(1.0, DoOperator2(&mut engine, 0.0, 0.0, PsOperator::Eq));
    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 1.0, PsOperator::Eq));
    assert_float_eq(0.0, DoOperator2(&mut engine, 255.0, 1.0, PsOperator::Eq));
    assert_float_eq(0.0, DoOperator2(&mut engine, -1.0, 0.0, PsOperator::Eq));

    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 0.0, PsOperator::Ne));
    assert_float_eq(1.0, DoOperator2(&mut engine, 0.0, 1.0, PsOperator::Ne));
    assert_float_eq(1.0, DoOperator2(&mut engine, 255.0, 1.0, PsOperator::Ne));
    assert_float_eq(1.0, DoOperator2(&mut engine, -1.0, 0.0, PsOperator::Ne));

    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 0.0, PsOperator::Gt));
    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 1.0, PsOperator::Gt));
    assert_float_eq(1.0, DoOperator2(&mut engine, 255.0, 1.0, PsOperator::Gt));
    assert_float_eq(0.0, DoOperator2(&mut engine, -1.0, 0.0, PsOperator::Gt));

    assert_float_eq(1.0, DoOperator2(&mut engine, 0.0, 0.0, PsOperator::Ge));
    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 1.0, PsOperator::Ge));
    assert_float_eq(1.0, DoOperator2(&mut engine, 255.0, 1.0, PsOperator::Ge));
    assert_float_eq(0.0, DoOperator2(&mut engine, -1.0, 0.0, PsOperator::Ge));

    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 0.0, PsOperator::Lt));
    assert_float_eq(1.0, DoOperator2(&mut engine, 0.0, 1.0, PsOperator::Lt));
    assert_float_eq(0.0, DoOperator2(&mut engine, 255.0, 1.0, PsOperator::Lt));
    assert_float_eq(1.0, DoOperator2(&mut engine, -1.0, 0.0, PsOperator::Lt));

    assert_float_eq(1.0, DoOperator2(&mut engine, 0.0, 0.0, PsOperator::Le));
    assert_float_eq(1.0, DoOperator2(&mut engine, 0.0, 1.0, PsOperator::Le));
    assert_float_eq(0.0, DoOperator2(&mut engine, 255.0, 1.0, PsOperator::Le));
    assert_float_eq(1.0, DoOperator2(&mut engine, -1.0, 0.0, PsOperator::Le));
}

#[test]
fn Logic() {
    let mut engine = CpdfPsEngine::new();

    assert_float_eq(1.0, DoOperator0(&mut engine, PsOperator::True));
    assert_float_eq(0.0, DoOperator0(&mut engine, PsOperator::False));

    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 0.0, PsOperator::And));
    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 1.0, PsOperator::And));
    assert_float_eq(0.0, DoOperator2(&mut engine, 1.0, 0.0, PsOperator::And));
    assert_float_eq(1.0, DoOperator2(&mut engine, 1.0, 1.0, PsOperator::And));

    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 0.0, PsOperator::Or));
    assert_float_eq(1.0, DoOperator2(&mut engine, 0.0, 1.0, PsOperator::Or));
    assert_float_eq(1.0, DoOperator2(&mut engine, 1.0, 0.0, PsOperator::Or));
    assert_float_eq(1.0, DoOperator2(&mut engine, 1.0, 1.0, PsOperator::Or));

    assert_float_eq(0.0, DoOperator2(&mut engine, 0.0, 0.0, PsOperator::Xor));
    assert_float_eq(1.0, DoOperator2(&mut engine, 0.0, 1.0, PsOperator::Xor));
    assert_float_eq(1.0, DoOperator2(&mut engine, 1.0, 0.0, PsOperator::Xor));
    assert_float_eq(0.0, DoOperator2(&mut engine, 1.0, 1.0, PsOperator::Xor));

    // Numeric operands use integer bitwise not; Boolean literals are tested
    // separately. Pinned native incorrectly applies logical not to both.
    assert_float_eq(-1.0, DoOperator1(&mut engine, 0.0, PsOperator::Not));
    assert_float_eq(-2.0, DoOperator1(&mut engine, 1.0, PsOperator::Not));
}

#[test]
fn MathFunctions() {
    let mut engine = CpdfPsEngine::new();

    assert_float_eq(1.4142135, DoOperator1(&mut engine, 2.0, PsOperator::Sqrt));
    assert_float_eq(0.8660254, DoOperator1(&mut engine, 60.0, PsOperator::Sin));
    assert_float_eq(0.5, DoOperator1(&mut engine, 60.0, PsOperator::Cos));
    assert_float_eq(45.0, DoOperator2(&mut engine, 1.0, 1.0, PsOperator::Atan));
    assert_float_eq(1000.0, DoOperator2(&mut engine, 10.0, 3.0, PsOperator::Exp));
    assert_float_eq(3.0, DoOperator1(&mut engine, 1000.0, PsOperator::Log));
    assert_float_eq(2.302585, DoOperator1(&mut engine, 10.0, PsOperator::Ln));
}

// Roll consumes its n/j operands before validating n. Valid payloads therefore
// hold at most 98 entries in the 100-entry stack; include that full-capacity case.
fn tagged_roll_input(count: usize) -> Vec<(u32, bool)> {
    (0..count)
        .map(|index| match index % 6 {
            0 => (0x7fc0_0001 + index as u32, false), // Distinct quiet NaN payloads.
            1 => ((-0.0f32).to_bits(), false),
            2 => (1.0f32.to_bits(), true),
            3 => (1.0f32.to_bits(), false),
            4 => (0.0f32.to_bits(), true),
            _ => ((index as f32 + 0.25).to_bits(), false),
        })
        .collect()
}

fn assert_tagged_roll(input: &[(u32, bool)], count: i32, shift: i32) {
    let mut engine = CpdfPsEngine::new();
    for &(bits, boolean) in input {
        engine.push_tagged(f32::from_bits(bits), boolean);
    }
    engine.push(count as f32);
    engine.push(shift as f32);
    assert_eq!(engine.stack_size() as usize, input.len() + 2);
    assert!(engine.do_operator(PsOperator::Roll));
    assert_eq!(engine.stack_size() as usize, input.len());

    let mut expected = input.to_vec();
    // Interpret the actual numeric operands placed on the float stack, then
    // map each original sample directly to its cyclic destination. This is
    // independent of either the specialized movement or slice::rotate_right.
    let count = (count as f32) as i32;
    let shift = (shift as f32) as i32;
    if count > 0 && count as usize <= input.len() {
        let start = input.len() - count as usize;
        for source in 0..count as usize {
            let destination = (source as i64 + i64::from(shift)).rem_euclid(i64::from(count));
            expected[start + destination as usize] = input[start + source];
        }
    }
    let actual: Vec<_> = (0..input.len())
        .map(|index| (engine.stack[index].to_bits(), engine.boolean[index]))
        .collect();
    assert_eq!(
        actual,
        expected,
        "payload={}, n={count}, j={shift}",
        input.len()
    );
}

#[test]
fn RollPreservesFloatBitsTagsPrefixesAndArgumentPopping() {
    // Include both directions of the short-stack path, its 16/17 boundary,
    // larger general rotations, full turns, and signed shifts far beyond n.
    let shifts = [
        i32::MIN,
        -1_000_003,
        -201,
        -101,
        -17,
        -2,
        -1,
        0,
        1,
        2,
        17,
        101,
        201,
        1_000_003,
        i32::MAX,
    ];
    for payload in 0..=98 {
        let input = tagged_roll_input(payload);
        for count in 0..=payload + 1 {
            for shift in shifts {
                assert_tagged_roll(&input, count as i32, shift);
            }
        }
    }
}

#[test]
fn RollInvalidLengthsAndUnderflowStillConsumeOperands() {
    for payload in 0..=98 {
        let input = tagged_roll_input(payload);
        for count in [i32::MIN, -100, -1, 99, 100, i32::MAX] {
            for shift in [i32::MIN, -1, 0, 1, i32::MAX] {
                assert_tagged_roll(&input, count, shift);
            }
        }
    }
    for operands in [vec![], vec![1.0], vec![-1.0], vec![f32::NAN]] {
        let mut engine = CpdfPsEngine::new();
        for value in operands {
            engine.push(value);
        }
        assert!(engine.do_operator(PsOperator::Roll));
        assert_eq!(engine.stack_size(), 0);
    }
}
