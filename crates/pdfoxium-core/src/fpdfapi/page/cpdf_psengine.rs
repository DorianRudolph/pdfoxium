// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/page/cpdf_psengine.{h,cpp}`.

use crate::fpdfapi::parser::cpdf_simple_parser::CpdfSimpleParser;
use crate::fxcrt::fx_string::string_to_float;

const STACK_SIZE: usize = 100;
const MAX_DEPTH: i32 = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PsOperator {
    Add,
    Sub,
    Mul,
    Div,
    IDiv,
    Mod,
    Neg,
    Abs,
    Ceiling,
    Floor,
    Round,
    Truncate,
    Sqrt,
    Sin,
    Cos,
    Atan,
    Exp,
    Ln,
    Log,
    Cvi,
    Cvr,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    And,
    Or,
    Xor,
    Not,
    Bitshift,
    True,
    False,
    If,
    IfElse,
    Pop,
    Exch,
    Dup,
    Copy,
    Index,
    Roll,
}

fn op_from_name(word: &[u8]) -> Option<PsOperator> {
    Some(match word {
        b"abs" => PsOperator::Abs,
        b"add" => PsOperator::Add,
        b"and" => PsOperator::And,
        b"atan" => PsOperator::Atan,
        b"bitshift" => PsOperator::Bitshift,
        b"ceiling" => PsOperator::Ceiling,
        b"copy" => PsOperator::Copy,
        b"cos" => PsOperator::Cos,
        b"cvi" => PsOperator::Cvi,
        b"cvr" => PsOperator::Cvr,
        b"div" => PsOperator::Div,
        b"dup" => PsOperator::Dup,
        b"eq" => PsOperator::Eq,
        b"exch" => PsOperator::Exch,
        b"exp" => PsOperator::Exp,
        b"false" => PsOperator::False,
        b"floor" => PsOperator::Floor,
        b"ge" => PsOperator::Ge,
        b"gt" => PsOperator::Gt,
        b"idiv" => PsOperator::IDiv,
        b"if" => PsOperator::If,
        b"ifelse" => PsOperator::IfElse,
        b"index" => PsOperator::Index,
        b"le" => PsOperator::Le,
        b"ln" => PsOperator::Ln,
        b"log" => PsOperator::Log,
        b"lt" => PsOperator::Lt,
        b"mod" => PsOperator::Mod,
        b"mul" => PsOperator::Mul,
        b"ne" => PsOperator::Ne,
        b"neg" => PsOperator::Neg,
        b"not" => PsOperator::Not,
        b"or" => PsOperator::Or,
        b"pop" => PsOperator::Pop,
        b"roll" => PsOperator::Roll,
        b"round" => PsOperator::Round,
        b"sin" => PsOperator::Sin,
        b"sqrt" => PsOperator::Sqrt,
        b"sub" => PsOperator::Sub,
        b"true" => PsOperator::True,
        b"truncate" => PsOperator::Truncate,
        b"xor" => PsOperator::Xor,
        _ => return None,
    })
}

enum PsToken {
    Operator(PsOperator),
    Constant(f32),
    Procedure(Vec<PsToken>),
}

// CPDF_PSProc::AddOperator(). Procedure nesting remains in parse_proc().
fn add_operator(operators: &mut Vec<PsToken>, word: &[u8]) {
    if let Some(op) = op_from_name(word) {
        operators.push(PsToken::Operator(op));
    } else {
        operators.push(PsToken::Constant(string_to_float(word)));
    }
}

fn parse_proc(parser: &mut CpdfSimpleParser<'_>, depth: i32, operators: &mut Vec<PsToken>) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    loop {
        let word = parser.get_word();
        if word.is_empty() {
            return false;
        }
        if word == b"}" {
            return true;
        }
        if word == b"{" {
            operators.push(PsToken::Procedure(Vec::new()));
            let Some(PsToken::Procedure(nested)) = operators.last_mut() else {
                unreachable!()
            };
            if !parse_proc(parser, depth + 1, nested) {
                return false;
            }
        } else {
            add_operator(operators, word);
        }
    }
}

fn execute_proc(operators: &[PsToken], engine: &mut CpdfPsEngine) -> bool {
    for (index, token) in operators.iter().enumerate() {
        match token {
            PsToken::Procedure(_) => continue,
            PsToken::Constant(value) => engine.push(*value),
            PsToken::Operator(PsOperator::If) => {
                let Some(PsToken::Procedure(proc)) =
                    index.checked_sub(1).and_then(|i| operators.get(i))
                else {
                    return false;
                };
                if engine.pop_int() != 0 {
                    execute_proc(proc, engine);
                }
            }
            PsToken::Operator(PsOperator::IfElse) => {
                if index < 2 {
                    return false;
                }
                let (PsToken::Procedure(false_proc), PsToken::Procedure(true_proc)) =
                    (&operators[index - 1], &operators[index - 2])
                else {
                    return false;
                };
                if engine.pop_int() != 0 {
                    execute_proc(true_proc, engine);
                } else {
                    execute_proc(false_proc, engine);
                }
            }
            PsToken::Operator(op) => {
                engine.do_operator(*op);
            }
        }
    }
    true
}

pub struct CpdfPsEngine {
    stack_count: usize,
    main_proc: Vec<PsToken>,
    stack: [f32; STACK_SIZE],
    // Preserve Boolean provenance for overloaded logical/integer operators.
    boolean: [bool; STACK_SIZE],
}

impl Default for CpdfPsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl CpdfPsEngine {
    pub fn new() -> Self {
        Self {
            stack_count: 0,
            main_proc: Vec::new(),
            stack: [0.0; STACK_SIZE],
            boolean: [false; STACK_SIZE],
        }
    }
    pub fn parse(&mut self, input: &[u8]) -> bool {
        let mut parser = CpdfSimpleParser::new(input);
        if parser.get_word() != b"{" {
            return false;
        }
        parse_proc(&mut parser, 0, &mut self.main_proc)
    }
    pub fn execute(&mut self) -> bool {
        // The procedure is read only; moving it out avoids a mutable alias
        // between stack updates and the parsed operator vector.
        let proc = std::mem::take(&mut self.main_proc);
        let result = execute_proc(&proc, self);
        self.main_proc = proc;
        result
    }
    pub fn reset(&mut self) {
        self.stack_count = 0;
    }
    pub fn push(&mut self, value: f32) {
        self.push_tagged(value, false);
    }
    fn push_bool(&mut self, value: bool) {
        self.push_tagged(f32::from(value), true);
    }
    fn push_tagged(&mut self, value: f32, boolean: bool) {
        if self.stack_count < STACK_SIZE {
            self.stack[self.stack_count] = value;
            self.boolean[self.stack_count] = boolean;
            self.stack_count += 1;
        }
    }
    fn pop_tagged(&mut self) -> (f32, bool) {
        if self.stack_count == 0 {
            return (0.0, false);
        }
        let boolean = self.boolean[self.stack_count - 1];
        (self.pop(), boolean)
    }
    pub fn pop(&mut self) -> f32 {
        if self.stack_count == 0 {
            return 0.0;
        }
        self.stack_count -= 1;
        self.stack[self.stack_count]
    }
    pub fn pop_int(&mut self) -> i32 {
        self.pop() as i32
    }
    pub fn stack_size(&self) -> u32 {
        self.stack_count as u32
    }

    pub fn do_operator(&mut self, op: PsOperator) -> bool {
        use PsOperator::*;
        match op {
            Add => {
                let a = self.pop();
                let b = self.pop();
                self.push(a + b);
            }
            Sub => {
                let b = self.pop();
                let a = self.pop();
                self.push(a - b);
            }
            Mul => {
                let a = self.pop();
                let b = self.pop();
                self.push(a * b);
            }
            Div => {
                let b = self.pop();
                let a = self.pop();
                self.push(if b != 0.0 { a / b } else { 0.0 });
            }
            IDiv => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(a.checked_div(b).unwrap_or(0) as f32);
            }
            Mod => {
                let b = self.pop_int();
                let a = self.pop_int();
                self.push(a.checked_rem(b).unwrap_or(0) as f32);
            }
            Neg => {
                let a = self.pop();
                self.push(-a);
            }
            Abs => {
                let a = self.pop();
                self.push(a.abs());
            }
            Ceiling => {
                let a = self.pop();
                self.push(a.ceil());
            }
            Floor => {
                let a = self.pop();
                self.push(a.floor());
            }
            Round => {
                let a = self.pop();
                self.push(if a.is_nan() {
                    0.0
                } else if a > f32::MAX - 0.5 {
                    f32::MAX
                } else {
                    (a + 0.5).floor()
                });
            }
            Truncate | Cvi => {
                let a = self.pop_int();
                self.push(a as f32);
            }
            Sqrt => {
                let a = self.pop();
                self.push(a.sqrt());
            }
            Sin => {
                let a = self.pop();
                self.push((a * std::f32::consts::PI / 180.0).sin());
            }
            Cos => {
                let a = self.pop();
                self.push((a * std::f32::consts::PI / 180.0).cos());
            }
            Atan => {
                let b = self.pop();
                let a = self.pop();
                let mut value = a.atan2(b) * 180.0 / std::f32::consts::PI;
                if value < 0.0 {
                    value += 360.0;
                }
                self.push(value);
            }
            Exp => {
                let b = self.pop();
                let a = self.pop();
                self.push(a.powf(b));
            }
            Ln => {
                let a = self.pop();
                self.push(a.ln());
            }
            Log => {
                let a = self.pop();
                self.push(a.log10());
            }
            Cvr => {}
            Eq => {
                let b = self.pop_tagged();
                let a = self.pop_tagged();
                self.push_bool(a == b);
            }
            Ne => {
                let b = self.pop_tagged();
                let a = self.pop_tagged();
                self.push_bool(a != b);
            }
            Gt => {
                let b = self.pop();
                let a = self.pop();
                self.push_bool(a > b);
            }
            Ge => {
                let b = self.pop();
                let a = self.pop();
                self.push_bool(a >= b);
            }
            Lt => {
                let b = self.pop();
                let a = self.pop();
                self.push_bool(a < b);
            }
            Le => {
                let b = self.pop();
                let a = self.pop();
                self.push_bool(a <= b);
            }
            And => {
                let (a, a_boolean) = self.pop_tagged();
                let (b, b_boolean) = self.pop_tagged();
                if a_boolean && b_boolean {
                    self.push_bool((a != 0.0) & (b != 0.0));
                } else {
                    self.push(((a as i32) & (b as i32)) as f32);
                }
            }
            Or => {
                let (a, a_boolean) = self.pop_tagged();
                let (b, b_boolean) = self.pop_tagged();
                if a_boolean && b_boolean {
                    self.push_bool((a != 0.0) | (b != 0.0));
                } else {
                    self.push(((a as i32) | (b as i32)) as f32);
                }
            }
            Xor => {
                let (a, a_boolean) = self.pop_tagged();
                let (b, b_boolean) = self.pop_tagged();
                if a_boolean && b_boolean {
                    self.push_bool((a != 0.0) ^ (b != 0.0));
                } else {
                    self.push(((a as i32) ^ (b as i32)) as f32);
                }
            }
            Not => {
                let (a, boolean) = self.pop_tagged();
                if boolean {
                    self.push_bool(a == 0.0);
                } else {
                    self.push((!(a as i32)) as f32);
                }
            }
            Bitshift => {
                let shift = self.pop_int();
                let value = self.pop_int();
                let result = if shift > 0 {
                    value
                        .checked_shl(shift as u32)
                        .filter(|_| i32::try_from(i64::from(value) << shift).is_ok())
                        .unwrap_or(0)
                } else {
                    value
                        .checked_shr(shift.checked_neg().unwrap_or(0) as u32)
                        .unwrap_or(0)
                };
                self.push(result as f32);
            }
            True => self.push_bool(true),
            False => self.push_bool(false),
            Pop => {
                self.pop();
            }
            Exch => {
                let b = self.pop_tagged();
                let a = self.pop_tagged();
                self.push_tagged(b.0, b.1);
                self.push_tagged(a.0, a.1);
            }
            Dup => {
                let a = self.pop_tagged();
                self.push_tagged(a.0, a.1);
                self.push_tagged(a.0, a.1);
            }
            Copy => {
                let n = self.pop_int();
                if let Ok(n) = usize::try_from(n) {
                    if n <= self.stack_count && n <= STACK_SIZE - self.stack_count {
                        let start = self.stack_count - n;
                        self.stack
                            .copy_within(start..self.stack_count, self.stack_count);
                        self.boolean
                            .copy_within(start..self.stack_count, self.stack_count);
                        self.stack_count += n;
                    }
                }
            }
            Index => {
                let n = self.pop_int();
                if let Ok(n) = usize::try_from(n) {
                    if n < self.stack_count {
                        let index = self.stack_count - n - 1;
                        self.push_tagged(self.stack[index], self.boolean[index]);
                    }
                }
            }
            Roll => {
                let j = self.pop_int();
                let n = self.pop_int();
                if j != 0 && n != 0 && self.stack_count != 0 {
                    if let Ok(n) = usize::try_from(n) {
                        if n <= self.stack_count {
                            let values = &mut self.stack[self.stack_count - n..self.stack_count];
                            let tags = &mut self.boolean[self.stack_count - n..self.stack_count];
                            // Calculator tint functions repeatedly roll a few
                            // operands by one slot. Move values and provenance
                            // together, avoiding two general rotations and their
                            // tiny temporary-buffer copies. Keep the general
                            // signed-roll path for other lengths and shifts.
                            if n <= 16 && j == 1 {
                                let last = (values[n - 1], tags[n - 1]);
                                for i in (1..n).rev() {
                                    values[i] = values[i - 1];
                                    tags[i] = tags[i - 1];
                                }
                                values[0] = last.0;
                                tags[0] = last.1;
                            } else if n <= 16 && j == -1 {
                                let first = (values[0], tags[0]);
                                for i in 0..n - 1 {
                                    values[i] = values[i + 1];
                                    tags[i] = tags[i + 1];
                                }
                                values[n - 1] = first.0;
                                tags[n - 1] = first.1;
                            } else {
                                let shift = j.rem_euclid(n as i32) as usize;
                                values.rotate_right(shift);
                                tags.rotate_right(shift);
                            }
                        }
                    }
                }
            }
            If | IfElse => {}
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{CpdfPsEngine, PsOperator};

    #[test]
    fn procedures_and_stack_rotation_follow_pdfium() {
        let mut engine = CpdfPsEngine::new();
        assert!(engine.parse(b"{ 1 2 3 3 1 roll 1 { 7 } { 8 } ifelse }"));
        assert!(engine.execute());
        assert_eq!(engine.stack_size(), 4);
        assert_eq!(engine.pop(), 7.0);
        assert_eq!(engine.pop(), 2.0);
        assert_eq!(engine.pop(), 1.0);
        assert_eq!(engine.pop(), 3.0);
        assert!(!engine.parse(b"{ { 2 }"));
    }

    #[test]
    fn boolean_tags_survive_stack_operators_and_integer_not_is_bitwise() {
        for program in [
            "{ true not }",
            "{ true dup pop not }",
            "{ 7 true exch pop not }",
            "{ true 1 copy pop not }",
            "{ true 0 index pop not }",
            "{ 7 true 2 1 roll pop not }",
            "{ 7 true 2 -1 roll pop not }",
            "{ 2 3 lt not }",
            "{ true false or not }",
            "{ true true and not }",
            "{ true false xor not }",
            "{ true 1 eq }",
        ] {
            let mut engine = CpdfPsEngine::new();
            assert!(engine.parse(program.as_bytes()));
            assert!(engine.execute());
            assert_eq!(engine.stack_size(), 1, "{program}");
            assert_eq!(engine.pop(), 0.0, "{program}");
        }
        for (operand, expected) in [(0.0, -1.0), (1.0, -2.0), (5.0, -6.0), (-6.0, 5.0)] {
            let mut engine = CpdfPsEngine::new();
            engine.push(operand);
            engine.do_operator(PsOperator::Not);
            assert_eq!(engine.pop(), expected);
        }
        let mut engine = CpdfPsEngine::new();
        assert!(engine.parse(b"{ true 0 add not }"));
        assert!(engine.execute());
        assert_eq!(engine.pop(), -2.0); // Arithmetic produces a numeric value.
    }

    #[test]
    fn arithmetic_handles_division_and_rounding_edges() {
        let mut engine = CpdfPsEngine::new();
        engine.push(100.0);
        engine.push(0.0);
        engine.do_operator(PsOperator::Div);
        assert_eq!(engine.pop(), 0.0);
        engine.push(-5.5);
        engine.do_operator(PsOperator::Round);
        assert_eq!(engine.pop(), -5.0);
        engine.push(i32::MIN as f32);
        engine.push(-1.0);
        engine.do_operator(PsOperator::IDiv);
        assert_eq!(engine.pop(), 0.0);
    }
}

#[cfg(test)]
#[path = "cpdf_psengine_unittest.rs"]
mod upstream_tests;
