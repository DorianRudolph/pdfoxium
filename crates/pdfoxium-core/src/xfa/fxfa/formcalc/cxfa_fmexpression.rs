// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Immutable FormCalc AST and JavaScript generation from
//! `xfa/fxfa/formcalc/cxfa_fmexpression.{h,cpp}`. Rc edges replace cppgc's
//! immutable tree edges. An explicit work stack retains source traversal and
//! output order without overflowing the host stack on a deep expression tree.
use super::cxfa_fmlexer::TokenType;
use super::cxfa_fmtojavascriptdepth::CxfaFmToJavaScriptDepth;
use std::rc::Rc;

pub type Expr = Rc<CxfaFmExpression>;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReturnType {
    Implied,
    Inferred,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessorIndex {
    NoIndex,
    NoRelativeIndex,
    PositiveIndex,
    NegativeIndex,
}
#[derive(Debug)]
pub enum CxfaFmExpression {
    Null,
    Number(Vec<u32>),
    String(Vec<u32>),
    Identifier(Vec<u32>),
    Assign {
        op: TokenType,
        left: Expr,
        right: Expr,
    },
    Binary {
        op: TokenType,
        left: Expr,
        right: Expr,
    },
    Unary {
        op: TokenType,
        expression: Expr,
    },
    Call {
        expression: Expr,
        arguments: Vec<Expr>,
        is_som_method: bool,
    },
    DotAccessor {
        accessor: Option<Expr>,
        op: TokenType,
        identifier: Vec<u32>,
        index: Expr,
    },
    Index {
        accessor_index: AccessorIndex,
        expression: Option<Expr>,
        is_star_index: bool,
    },
    DotDotAccessor {
        accessor: Expr,
        op: TokenType,
        identifier: Vec<u32>,
        index: Expr,
    },
    MethodCall {
        accessor: Expr,
        call: Expr,
    },
    FunctionDefinition {
        name: Vec<u32>,
        arguments: Vec<Vec<u32>>,
        expressions: Vec<Expr>,
    },
    Var {
        name: Vec<u32>,
        init: Option<Expr>,
    },
    Exp(Expr),
    Block(Vec<Expr>),
    Do(Expr),
    If {
        condition: Expr,
        if_expression: Option<Expr>,
        else_if_expressions: Vec<Expr>,
        else_expression: Option<Expr>,
    },
    While {
        condition: Expr,
        expression: Expr,
    },
    Break,
    Continue,
    For {
        variant: Vec<u32>,
        assignment: Expr,
        accessor: Expr,
        direction: bool,
        step: Option<Expr>,
        list: Expr,
    },
    Foreach {
        identifier: Vec<u32>,
        accessors: Vec<Expr>,
        list: Expr,
    },
}
impl CxfaFmExpression {
    pub fn rc(self) -> Expr {
        Rc::new(self)
    }
    pub fn operator_token(&self) -> TokenType {
        use TokenType::*;
        match self {
            Self::Null => TOKnull,
            Self::Number(_) => TOKnumber,
            Self::String(_) => TOKstring,
            Self::Identifier(_) => TOKidentifier,
            Self::Assign { op, .. }
            | Self::Binary { op, .. }
            | Self::Unary { op, .. }
            | Self::DotAccessor { op, .. }
            | Self::DotDotAccessor { op, .. } => *op,
            Self::Call { .. } => TOKcall,
            Self::Index { .. } => TOKlbracket,
            Self::MethodCall { .. } => TOKdot,
            _ => TOKreserver,
        }
    }
    pub fn to_javascript(&self, js: &mut Vec<u32>, return_type: ReturnType) -> bool {
        let original = std::mem::take(js);
        let mut emitter = Emitter {
            buffers: vec![original],
            tasks: Vec::new(),
            free_buffers: Vec::new(),
        };
        emitter.tasks.push(Task::Emit(
            self,
            return_type,
            0,
            CxfaFmToJavaScriptDepth::root().descend(),
            false,
        ));
        let result = emitter.run();
        *js = std::mem::take(&mut emitter.buffers[0]);
        result
    }
    pub fn is_built_in_func(name: &mut Vec<u32>) -> bool {
        if name.len() > 12 {
            return false;
        }
        let found =
            BUILT_INS.binary_search_by(|candidate| compare_case_insensitive(candidate, name));
        if let Ok(index) = found {
            name.clear();
            push(name, BUILT_INS[index]);
            true
        } else {
            false
        }
    }
    pub fn is_method_with_obj_param(name: &[u32]) -> u32 {
        SOM_METHODS
            .binary_search_by(|(candidate, _)| compare_ascii(candidate, name))
            .ok()
            .map_or(0, |index| SOM_METHODS[index].1)
    }
}
#[derive(Debug)]
pub struct CxfaFmAst {
    expressions: Vec<Expr>,
}
impl CxfaFmAst {
    pub fn new(expressions: Vec<Expr>) -> Self {
        Self { expressions }
    }
    pub fn expressions(&self) -> &[Expr] {
        &self.expressions
    }
    pub fn to_javascript(&self) -> Option<Vec<u32>> {
        if self.expressions.is_empty() {
            return Some(wide("// comments only"));
        }
        let mut emitter = Emitter {
            buffers: vec![wide(AST_PREFIX)],
            tasks: vec![Task::Text(
                "return pfm_rt.get_val(pfm_ret);\n}).call(this);",
                0,
            )],
            free_buffers: Vec::new(),
        };
        emitter.expression_list(
            &self.expressions,
            ReturnType::Implied,
            0,
            CxfaFmToJavaScriptDepth::root(),
        );
        if !emitter.run() || cxfa_is_too_big(&emitter.buffers[0]) {
            return None;
        }
        Some(std::mem::take(&mut emitter.buffers[0]))
    }
}
// The native wide buffer reports bytes. The selected Linux/portable source
// wchar representation is 32 bit, as is this engine's Vec<u32> text.
pub fn cxfa_is_too_big(js: &[u32]) -> bool {
    js.len() >= (256 * 1024 * 1024) / std::mem::size_of::<u32>()
}
fn push(output: &mut Vec<u32>, text: &str) {
    output.extend(text.chars().map(u32::from));
}
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn eq_ascii(input: &[u32], text: &str) -> bool {
    input.iter().copied().eq(text.bytes().map(u32::from))
}
fn compare_ascii(text: &str, input: &[u32]) -> std::cmp::Ordering {
    text.bytes().map(u32::from).cmp(input.iter().copied())
}
fn compare_case_insensitive(text: &str, input: &[u32]) -> std::cmp::Ordering {
    text.bytes().map(|b| u32::from(b.to_ascii_lowercase())).cmp(
        input
            .iter()
            .map(|&c| char::from_u32(c).map_or(c, |v| v.to_lowercase().next().unwrap() as u32)),
    )
}
fn identifier_to_name(input: &[u32]) -> Vec<u32> {
    if input.first() == Some(&u32::from(b'!')) {
        let mut name = wide("pfm__excl__");
        name.extend_from_slice(&input[1..]);
        name
    } else {
        input.to_vec()
    }
}
const BUILT_INS: &[&str] = &[
    "Abs",
    "Apr",
    "At",
    "Avg",
    "Ceil",
    "Choose",
    "Concat",
    "Count",
    "Cterm",
    "Date",
    "Date2Num",
    "DateFmt",
    "Decode",
    "Encode",
    "Eval",
    "Exists",
    "Floor",
    "Format",
    "FV",
    "Get",
    "HasValue",
    "If",
    "Ipmt",
    "IsoDate2Num",
    "IsoTime2Num",
    "Left",
    "Len",
    "LocalDateFmt",
    "LocalTimeFmt",
    "Lower",
    "Ltrim",
    "Max",
    "Min",
    "Mod",
    "NPV",
    "Num2Date",
    "Num2GMTime",
    "Num2Time",
    "Oneof",
    "Parse",
    "Pmt",
    "Post",
    "PPmt",
    "Put",
    "PV",
    "Rate",
    "Ref",
    "Replace",
    "Right",
    "Round",
    "Rtrim",
    "Space",
    "Str",
    "Stuff",
    "Substr",
    "Sum",
    "Term",
    "Time",
    "Time2Num",
    "TimeFmt",
    "UnitType",
    "UnitValue",
    "Upper",
    "Uuid",
    "Within",
    "WordNum",
];
const SOM_METHODS: &[(&str, u32)] = &[
    ("absPage", 0x01),
    ("absPageInBatch", 0x01),
    ("absPageSpan", 0x01),
    ("append", 0x01),
    ("clear", 0x01),
    ("formNodes", 0x01),
    ("h", 0x01),
    ("insert", 0x03),
    ("isRecordGroup", 0x01),
    ("page", 0x01),
    ("pageSpan", 0x01),
    ("remove", 0x01),
    ("saveFilteredXML", 0x01),
    ("setElement", 0x01),
    ("sheet", 0x01),
    ("sheetInBatch", 0x01),
    ("sign", 0x61),
    ("verify", 0x0d),
    ("w", 0x01),
    ("x", 0x01),
    ("y", 0x01),
];
const AST_PREFIX:&str="(function() {\nlet pfm_method_runner = function(obj, cb) {\n  if (pfm_rt.is_ary(obj)) {\n    let pfm_method_return = null;\n    for (var idx = obj.length -1; idx > 1; idx--) {\n      pfm_method_return = cb(obj[idx]);\n    }\n    return pfm_method_return;\n  }\n  return cb(obj);\n};\nvar pfm_ret = null;\n";

// Finish tasks consume generated temporary buffers directly. No complete
// expression or output graph is cloned merely to satisfy Rust borrows.
enum Task<'a> {
    Emit(
        &'a CxfaFmExpression,
        ReturnType,
        usize,
        CxfaFmToJavaScriptDepth,
        bool, // An else-if shares its outer if's implied-result initializer.
    ),
    Text(&'static str, usize),
    Wide(&'a [u32], usize),
    Owned(Vec<u32>, usize),
    Check(usize),
    AssignStart(
        &'a CxfaFmExpression,
        ReturnType,
        usize,
        usize,
        usize,
        CxfaFmToJavaScriptDepth,
    ),
    AssignFinish(&'a CxfaFmExpression, ReturnType, usize, usize, usize),
    CallFinish(&'a CxfaFmExpression, usize, usize, CxfaFmToJavaScriptDepth),
}
struct Emitter<'a> {
    buffers: Vec<Vec<u32>>,
    tasks: Vec<Task<'a>>,
    free_buffers: Vec<usize>,
}
impl<'a> Emitter<'a> {
    fn text(&mut self, text: &str, output: usize) {
        push(&mut self.buffers[output], text);
    }
    fn append_buffer(&mut self, src: usize, dst: usize) {
        if src < dst {
            let (a, b) = self.buffers.split_at_mut(dst);
            b[0].extend_from_slice(&a[src]);
        } else if src > dst {
            let (a, b) = self.buffers.split_at_mut(src);
            a[dst].extend_from_slice(&b[0]);
        }
    }
    fn temp(&mut self) -> usize {
        if let Some(index) = self.free_buffers.pop() {
            return index;
        }
        let index = self.buffers.len();
        self.buffers.push(Vec::new());
        index
    }
    fn release_temp(&mut self, index: usize) {
        self.buffers[index].clear();
        self.free_buffers.push(index);
    }
    fn child(
        &mut self,
        expr: &'a CxfaFmExpression,
        ret: ReturnType,
        out: usize,
        depth: CxfaFmToJavaScriptDepth,
    ) {
        self.tasks
            .push(Task::Emit(expr, ret, out, depth.descend(), false));
    }
    fn expression_list(
        &mut self,
        expressions: &'a [Expr],
        ret: ReturnType,
        out: usize,
        depth: CxfaFmToJavaScriptDepth,
    ) {
        for (index, expr) in expressions.iter().enumerate().rev() {
            self.child(
                expr,
                if ret == ReturnType::Implied && index + 1 == expressions.len() {
                    ReturnType::Implied
                } else {
                    ReturnType::Inferred
                },
                out,
                depth,
            );
        }
    }
    fn run(&mut self) -> bool {
        while let Some(task) = self.tasks.pop() {
            match task {
                Task::Text(text, out) => self.text(text, out),
                Task::Wide(text, out) => self.buffers[out].extend_from_slice(text),
                Task::Owned(text, out) => self.buffers[out].extend_from_slice(&text),
                Task::Check(out) => {
                    if cxfa_is_too_big(&self.buffers[out]) {
                        return false;
                    }
                }
                Task::Emit(expr, ret, out, depth, is_else_if) => {
                    if cxfa_is_too_big(&self.buffers[out]) || !depth.is_within_max_depth() {
                        return false;
                    }
                    self.tasks.push(Task::Check(out));
                    if !self.emit(expr, ret, out, depth, is_else_if) {
                        return false;
                    }
                }
                Task::AssignStart(expr, ret, out, left_buf, right_buf, depth) => {
                    let CxfaFmExpression::Assign { right, .. } = expr else {
                        return false;
                    };
                    self.text("if (pfm_rt.is_obj(", out);
                    self.append_buffer(left_buf, out);
                    self.text("))\n{\n", out);
                    if ret == ReturnType::Implied {
                        self.text("pfm_ret = ", out);
                    }
                    self.tasks
                        .push(Task::AssignFinish(expr, ret, out, left_buf, right_buf));
                    self.child(right, ReturnType::Inferred, right_buf, depth);
                }
                Task::AssignFinish(expr, ret, out, left_buf, right_buf) => {
                    self.assign_finish(expr, ret, out, left_buf, right_buf)
                }
                Task::CallFinish(expr, out, name_buf, depth) => {
                    if !self.call_finish(expr, out, name_buf, depth) {
                        return false;
                    }
                }
            }
        }
        true
    }
    fn emit(
        &mut self,
        expr: &'a CxfaFmExpression,
        ret: ReturnType,
        out: usize,
        depth: CxfaFmToJavaScriptDepth,
        is_else_if: bool,
    ) -> bool {
        use CxfaFmExpression::*;
        use ReturnType::{Implied, Inferred};
        match expr {
            Null => self.text("null", out),
            Number(number) => self.buffers[out].extend_from_slice(number),
            String(string) => {
                if string.len() <= 2 {
                    self.buffers[out].extend_from_slice(string);
                    return true;
                }
                self.text("\"", out);
                let mut index = 1;
                while index + 1 < string.len() {
                    match string[index] {
                        34 => {
                            index += 1;
                            self.text("\\\"", out);
                        }
                        13 => {}
                        10 => self.text("\\n", out),
                        ch => self.buffers[out].push(ch),
                    }
                    index += 1;
                }
                self.text("\"", out);
            }
            Identifier(identifier) => {
                let replacement = if eq_ascii(identifier, "$") {
                    Some("this")
                } else if eq_ascii(identifier, "!") {
                    Some("xfa.datasets")
                } else if eq_ascii(identifier, "$data") {
                    Some("xfa.datasets.data")
                } else if eq_ascii(identifier, "$event") {
                    Some("xfa.event")
                } else if eq_ascii(identifier, "$form") {
                    Some("xfa.form")
                } else if eq_ascii(identifier, "$host") {
                    Some("xfa.host")
                } else if eq_ascii(identifier, "$layout") {
                    Some("xfa.layout")
                } else if eq_ascii(identifier, "$template") {
                    Some("xfa.template")
                } else {
                    None
                };
                if let Some(replacement) = replacement {
                    self.text(replacement, out);
                } else if identifier.first() == Some(&33) {
                    self.text("pfm__excl__", out);
                    self.buffers[out].extend_from_slice(&identifier[1..]);
                } else {
                    self.buffers[out].extend_from_slice(identifier);
                }
            }
            Assign { left, .. } => {
                let left_buf = self.temp();
                let right_buf = self.temp();
                self.tasks.push(Task::AssignStart(
                    expr, ret, out, left_buf, right_buf, depth,
                ));
                self.child(left, Inferred, left_buf, depth);
            }
            Binary { op, left, right } => {
                let Some(name) = binary_name(*op) else {
                    return false;
                };
                self.text("pfm_rt.", out);
                self.text(name, out);
                self.text("(", out);
                self.tasks.push(Task::Text(")", out));
                self.child(right, Inferred, out, depth);
                self.tasks.push(Task::Text(", ", out));
                self.child(left, Inferred, out, depth);
            }
            Unary { op, expression } => {
                let name = match op {
                    TokenType::TOKplus => "pos_op",
                    TokenType::TOKminus => "neg_op",
                    TokenType::TOKksnot => "log_not_op",
                    _ => return false,
                };
                self.text("pfm_rt.", out);
                self.text(name, out);
                self.text("(", out);
                self.tasks.push(Task::Text(")", out));
                self.child(expression, Inferred, out, depth);
            }
            Call { expression, .. } => {
                let name_buf = self.temp();
                self.tasks
                    .push(Task::CallFinish(expr, out, name_buf, depth));
                self.child(expression, Inferred, name_buf, depth);
            }
            DotAccessor {
                accessor,
                op,
                identifier,
                index,
            } => {
                self.text("pfm_rt.dot_acc(", out);
                self.tasks.push(Task::Text(")", out));
                self.child(index, Inferred, out, depth);
                self.tasks.push(Task::Text("\", ", out));
                if *op == TokenType::TOKdotscream {
                    self.tasks.push(Task::Wide(identifier, out));
                    self.tasks.push(Task::Text("\"#", out));
                } else if *op == TokenType::TOKdotstar {
                    self.tasks.push(Task::Text("\"*", out));
                } else if *op == TokenType::TOKcall {
                    self.tasks.push(Task::Text("\"", out));
                } else {
                    self.tasks.push(Task::Wide(identifier, out));
                    self.tasks.push(Task::Text("\"", out));
                }
                self.tasks.push(Task::Text("\", ", out));
                if let Some(accessor) = accessor {
                    if accessor.operator_token() == TokenType::TOKidentifier {
                        self.child(accessor, Inferred, out, depth);
                    }
                    self.tasks.push(Task::Text(", \"", out));
                    self.child(accessor, Inferred, out, depth);
                } else {
                    self.tasks.push(Task::Text("null, \"", out));
                }
            }
            Index {
                accessor_index,
                expression,
                is_star_index,
            } => {
                self.text(
                    match accessor_index {
                        AccessorIndex::NoIndex => "0",
                        AccessorIndex::NoRelativeIndex => "1",
                        AccessorIndex::PositiveIndex => "2",
                        AccessorIndex::NegativeIndex => "3",
                    },
                    out,
                );
                if !is_star_index {
                    self.text(", ", out);
                    if let Some(expression) = expression {
                        self.child(expression, Inferred, out, depth);
                    } else {
                        self.text("0", out);
                    }
                }
            }
            DotDotAccessor {
                accessor,
                identifier,
                index,
                ..
            } => {
                self.text("pfm_rt.dotdot_acc(", out);
                self.tasks.push(Task::Text(")", out));
                self.child(index, Inferred, out, depth);
                self.tasks.push(Task::Text("\", ", out));
                self.tasks.push(Task::Wide(identifier, out));
                self.tasks.push(Task::Text("\", \"", out));
                if accessor.operator_token() == TokenType::TOKidentifier {
                    self.child(accessor, Inferred, out, depth);
                }
                self.tasks.push(Task::Text(", \"", out));
                self.child(accessor, Inferred, out, depth);
            }
            MethodCall { accessor, call } => {
                self.text("(function() {\n  return pfm_method_runner(", out);
                self.tasks.push(Task::Text(";\n  });\n}).call(this)", out));
                self.child(call, Inferred, out, depth);
                self.tasks
                    .push(Task::Text(", function(obj) {\n    return obj.", out));
                self.child(accessor, Inferred, out, depth);
            }
            FunctionDefinition {
                name,
                arguments,
                expressions,
            } => {
                if name.is_empty() {
                    return false;
                }
                self.text("function ", out);
                self.buffers[out].extend(identifier_to_name(name));
                self.text("(", out);
                for (index, arg) in arguments.iter().enumerate() {
                    if index > 0 {
                        self.text(", ", out);
                    }
                    self.buffers[out].extend(identifier_to_name(arg));
                }
                self.text(") {\nvar pfm_ret = null;\n", out);
                self.tasks.push(Task::Text("return pfm_ret;\n}\n", out));
                self.expression_list(expressions, Implied, out, depth);
            }
            Var { name, init } => {
                let name = identifier_to_name(name);
                self.text("var ", out);
                self.buffers[out].extend_from_slice(&name);
                self.text(" = ", out);
                if ret == Implied {
                    self.tasks.push(Task::Text(";\n", out));
                    self.tasks.push(Task::Owned(name.clone(), out));
                    self.tasks.push(Task::Text("pfm_ret = ", out));
                }
                if let Some(init) = init {
                    self.tasks.push(Task::Text(");\n", out));
                    self.tasks.push(Task::Owned(name.clone(), out));
                    self.tasks.push(Task::Text(" = pfm_rt.var_filter(", out));
                    self.tasks.push(Task::Owned(name, out));
                    self.tasks.push(Task::Text(";\n", out));
                    self.child(init, Inferred, out, depth);
                } else {
                    self.text("\"\";\n", out);
                }
            }
            Exp(expression) => {
                let op = expression.operator_token();
                if ret == Inferred {
                    if op != TokenType::TOKassign {
                        self.tasks.push(Task::Text(";\n", out));
                    }
                    self.child(expression, Inferred, out, depth);
                } else if op == TokenType::TOKassign {
                    self.child(expression, Implied, out, depth);
                } else if matches!(
                    op,
                    TokenType::TOKstar
                        | TokenType::TOKdotstar
                        | TokenType::TOKdotscream
                        | TokenType::TOKdotdot
                        | TokenType::TOKdot
                ) {
                    self.text("pfm_ret = pfm_rt.get_val(", out);
                    self.tasks.push(Task::Text(");\n", out));
                    self.child(expression, Inferred, out, depth);
                } else {
                    self.text("pfm_ret = ", out);
                    self.tasks.push(Task::Text(";\n", out));
                    self.child(expression, Inferred, out, depth);
                }
            }
            Block(expressions) => {
                self.text("{\n", out);
                self.tasks.push(Task::Text("}\n", out));
                self.expression_list(expressions, ret, out, depth);
            }
            Do(list) => self.child(list, ret, out, depth),
            If {
                condition,
                if_expression,
                else_if_expressions,
                else_expression,
            } => {
                if ret == Implied && !is_else_if {
                    self.text("pfm_ret = 0;\n", out);
                }
                self.text("if (pfm_rt.get_val(", out);
                if let Some(else_expression) = else_expression {
                    self.child(else_expression, ret, out, depth);
                    self.tasks.push(Task::Text("else ", out));
                }
                for branch in else_if_expressions.iter().rev() {
                    // The source emits else-if bodies as Inferred, dropping
                    // their final value. Propagate the actual return mode,
                    // sharing the outer initializer to keep `else if` valid.
                    self.tasks
                        .push(Task::Emit(branch, ret, out, depth.descend(), true));
                    self.tasks.push(Task::Text("else ", out));
                }
                if let Some(if_expression) = if_expression {
                    self.child(if_expression, ret, out, depth);
                }
                self.tasks.push(Task::Check(out));
                self.tasks.push(Task::Text("))\n", out));
                self.child(condition, Inferred, out, depth);
            }
            While {
                condition,
                expression,
            } => {
                if ret == Implied {
                    self.text("pfm_ret = 0;\n", out);
                }
                self.text("while (", out);
                self.child(expression, ret, out, depth);
                self.tasks.push(Task::Check(out));
                self.tasks.push(Task::Text(")\n", out));
                self.child(condition, Inferred, out, depth);
            }
            Break => self.text("pfm_ret = 0;\nbreak;\n", out),
            Continue => self.text("pfm_ret = 0;\ncontinue;\n", out),
            For {
                variant,
                assignment,
                accessor,
                direction,
                step,
                list,
            } => {
                if ret == Implied {
                    self.text("pfm_ret = 0;\n", out);
                }
                self.text("{\n", out);
                let name = identifier_to_name(variant);
                self.text("var ", out);
                self.buffers[out].extend_from_slice(&name);
                self.text(" = null;\nfor (", out);
                self.buffers[out].extend_from_slice(&name);
                self.text(" = pfm_rt.get_val(", out);
                self.tasks.push(Task::Text("}\n", out));
                self.child(list, ret, out, depth);
                self.tasks.push(Task::Check(out));
                self.tasks.push(Task::Text(")\n", out));
                if let Some(step) = step {
                    self.tasks.push(Task::Text(")", out));
                    self.child(step, Inferred, out, depth);
                    self.tasks.push(Task::Text("pfm_rt.get_val(", out));
                } else {
                    self.tasks.push(Task::Text("1", out));
                }
                self.tasks
                    .push(Task::Text(if *direction { " += " } else { " -= " }, out));
                self.tasks.push(Task::Owned(name.clone(), out));
                self.tasks.push(Task::Text("); ", out));
                self.child(accessor, Inferred, out, depth);
                self.tasks.push(Task::Text("pfm_rt.get_val(", out));
                self.tasks
                    .push(Task::Text(if *direction { " <= " } else { " >= " }, out));
                self.tasks.push(Task::Owned(name, out));
                self.tasks.push(Task::Text("); ", out));
                self.child(assignment, Inferred, out, depth);
            }
            Foreach {
                identifier,
                accessors,
                list,
            } => {
                if ret == Implied {
                    self.text("pfm_ret = 0;\n", out);
                }
                self.text("{\n", out);
                let name = identifier_to_name(identifier);
                self.text("var ", out);
                self.buffers[out].extend_from_slice(&name);
                self.text(" = null;\nvar pfm_ary = pfm_rt.concat_obj(", out);
                self.tasks.push(Task::Text("}\n}\n", out));
                self.child(list, ret, out, depth);
                self.tasks
                    .push(Task::Text(" = pfm_ary[pfm_ary_idx++];\n", out));
                self.tasks.push(Task::Owned(name, out));
                self.tasks.push(Task::Text(
                    ");\nvar pfm_ary_idx = 0;\nwhile(pfm_ary_idx < pfm_ary.length)\n{\n",
                    out,
                ));
                for (index, accessor) in accessors.iter().enumerate().rev() {
                    if index + 1 < accessors.len() {
                        self.tasks.push(Task::Text(", ", out));
                    }
                    self.child(accessor, Inferred, out, depth);
                }
            }
        }
        true
    }
    fn assign_finish(
        &mut self,
        expr: &'a CxfaFmExpression,
        ret: ReturnType,
        out: usize,
        left_buf: usize,
        right_buf: usize,
    ) {
        let CxfaFmExpression::Assign { left, .. } = expr else {
            return;
        };
        self.text("pfm_rt.asgn_val_op(", out);
        self.append_buffer(left_buf, out);
        self.text(", ", out);
        self.append_buffer(right_buf, out);
        self.text(");\n}\n", out);
        if left.operator_token() == TokenType::TOKidentifier
            && !eq_ascii(&self.buffers[left_buf], "this")
        {
            self.text("else\n{\n", out);
            if ret == ReturnType::Implied {
                self.text("pfm_ret = ", out);
            }
            self.append_buffer(left_buf, out);
            self.text(" = pfm_rt.asgn_val_op(", out);
            self.append_buffer(left_buf, out);
            self.text(", ", out);
            self.append_buffer(right_buf, out);
            self.text(");\n}\n", out);
        }
        self.release_temp(left_buf);
        self.release_temp(right_buf);
    }
    fn call_finish(
        &mut self,
        expr: &'a CxfaFmExpression,
        out: usize,
        name_buf: usize,
        depth: CxfaFmToJavaScriptDepth,
    ) -> bool {
        let CxfaFmExpression::Call {
            arguments,
            is_som_method,
            ..
        } = expr
        else {
            return false;
        };
        use ReturnType::Inferred;
        if *is_som_method {
            self.append_buffer(name_buf, out);
            self.text("(", out);
            self.tasks.push(Task::Text(")", out));
            let mask = CxfaFmExpression::is_method_with_obj_param(&self.buffers[name_buf]);
            for (index, arg) in arguments.iter().enumerate().rev() {
                if index + 1 < arguments.len() {
                    self.tasks.push(Task::Text(", ", out));
                }
                self.tasks.push(Task::Text(")", out));
                self.child(arg, Inferred, out, depth);
                self.tasks.push(Task::Text(
                    if index < 32 && mask & (1u32 << index) != 0 {
                        "pfm_rt.get_jsobj("
                    } else {
                        "pfm_rt.get_val("
                    },
                    out,
                ));
            }
            self.release_temp(name_buf);
            return true;
        }
        if !CxfaFmExpression::is_built_in_func(&mut self.buffers[name_buf]) {
            return false;
        }
        let is_eval = eq_ascii(&self.buffers[name_buf], "Eval");
        let is_exists = eq_ascii(&self.buffers[name_buf], "Exists");
        if is_eval {
            self.text("eval.call(this, pfm_rt.Translate", out);
        } else {
            self.text("pfm_rt.", out);
            self.append_buffer(name_buf, out);
        }
        self.text("(", out);
        if is_eval {
            self.tasks.push(Task::Text(")", out));
        }
        self.tasks.push(Task::Text(")", out));
        if is_exists {
            self.text("\n(\nfunction ()\n{\ntry\n{\n", out);
            self.tasks.push(Task::Text(
                ";\n}\ncatch(accessExceptions)\n{\nreturn 0;\n}\n}\n).call(this)\n",
                out,
            ));
            self.text("return ", out);
            if let Some(arg) = arguments.first() {
                self.child(arg, Inferred, out, depth);
            } else {
                self.text("0", out);
            }
        } else {
            for (index, arg) in arguments.iter().enumerate().rev() {
                if index + 1 < arguments.len() {
                    self.tasks.push(Task::Text(", ", out));
                }
                self.child(arg, Inferred, out, depth);
            }
        }
        self.release_temp(name_buf);
        true
    }
}
fn binary_name(op: TokenType) -> Option<&'static str> {
    use TokenType::*;
    Some(match op {
        TOKor | TOKksor => "log_or_op",
        TOKand | TOKksand => "log_and_op",
        TOKeq | TOKkseq => "eq_op",
        TOKne | TOKksne => "neq_op",
        TOKgt | TOKksgt => "gt_op",
        TOKge | TOKksge => "ge_op",
        TOKlt | TOKkslt => "lt_op",
        TOKle | TOKksle => "le_op",
        TOKplus => "plus_op",
        TOKminus => "minus_op",
        TOKmul => "mul_op",
        TOKdiv => "div_op",
        _ => return None,
    })
}

impl CxfaFmExpression {
    fn has_non_null_children(&self) -> bool {
        let nonnull = |node: &Expr| !matches!(node.as_ref(), Self::Null);
        let optional = |node: &Option<Expr>| node.as_ref().is_some_and(nonnull);
        match self {
            Self::Assign { left, right, .. }
            | Self::Binary { left, right, .. }
            | Self::MethodCall {
                accessor: left,
                call: right,
            } => nonnull(left) || nonnull(right),
            Self::Unary { expression, .. } | Self::Exp(expression) | Self::Do(expression) => {
                nonnull(expression)
            }
            Self::Call {
                expression,
                arguments,
                ..
            } => nonnull(expression) || arguments.iter().any(nonnull),
            Self::DotAccessor {
                accessor, index, ..
            } => optional(accessor) || nonnull(index),
            Self::Index { expression, .. }
            | Self::Var {
                init: expression, ..
            } => optional(expression),
            Self::DotDotAccessor {
                accessor, index, ..
            } => nonnull(accessor) || nonnull(index),
            Self::FunctionDefinition { expressions, .. } | Self::Block(expressions) => {
                expressions.iter().any(nonnull)
            }
            Self::If {
                condition,
                if_expression,
                else_if_expressions,
                else_expression,
            } => {
                nonnull(condition)
                    || optional(if_expression)
                    || else_if_expressions.iter().any(nonnull)
                    || optional(else_expression)
            }
            Self::While {
                condition,
                expression,
            } => nonnull(condition) || nonnull(expression),
            Self::For {
                assignment,
                accessor,
                step,
                list,
                ..
            } => nonnull(assignment) || nonnull(accessor) || optional(step) || nonnull(list),
            Self::Foreach {
                accessors, list, ..
            } => accessors.iter().any(nonnull) || nonnull(list),
            _ => false,
        }
    }
    fn take_children(&mut self, pending: &mut Vec<Expr>, null: &Expr) {
        let take = |node: &mut Expr| std::mem::replace(node, Rc::clone(null));
        match self {
            Self::Assign { left, right, .. }
            | Self::Binary { left, right, .. }
            | Self::MethodCall {
                accessor: left,
                call: right,
            } => {
                pending.push(take(left));
                pending.push(take(right));
            }
            Self::Unary { expression, .. } | Self::Exp(expression) | Self::Do(expression) => {
                pending.push(take(expression))
            }
            Self::Call {
                expression,
                arguments,
                ..
            } => {
                pending.push(take(expression));
                pending.append(arguments);
            }
            Self::DotAccessor {
                accessor, index, ..
            } => {
                pending.extend(accessor.take());
                pending.push(take(index));
            }
            Self::Index { expression, .. }
            | Self::Var {
                init: expression, ..
            } => pending.extend(expression.take()),
            Self::DotDotAccessor {
                accessor, index, ..
            } => {
                pending.push(take(accessor));
                pending.push(take(index));
            }
            Self::FunctionDefinition { expressions, .. } | Self::Block(expressions) => {
                pending.append(expressions)
            }
            Self::If {
                condition,
                if_expression,
                else_if_expressions,
                else_expression,
            } => {
                pending.push(take(condition));
                pending.extend(if_expression.take());
                pending.append(else_if_expressions);
                pending.extend(else_expression.take());
            }
            Self::While {
                condition,
                expression,
            } => {
                pending.push(take(condition));
                pending.push(take(expression));
            }
            Self::For {
                assignment,
                accessor,
                step,
                list,
                ..
            } => {
                pending.push(take(assignment));
                pending.push(take(accessor));
                pending.extend(step.take());
                pending.push(take(list));
            }
            Self::Foreach {
                accessors, list, ..
            } => {
                pending.append(accessors);
                pending.push(take(list));
            }
            _ => {}
        }
    }
}
impl Drop for CxfaFmExpression {
    fn drop(&mut self) {
        if !self.has_non_null_children() {
            return;
        }
        // cppgc destroys nodes individually. Drain unique Rc descendants on an
        // explicit stack as well; shared subtrees retain their other owners.
        let null = Rc::new(Self::Null);
        let mut pending = Vec::new();
        self.take_children(&mut pending, &null);
        while let Some(node) = pending.pop() {
            if let Ok(mut node) = Rc::try_unwrap(node) {
                node.take_children(&mut pending, &null);
            }
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    fn emit(expr: CxfaFmExpression, ret: ReturnType) -> Vec<u32> {
        let mut output = Vec::new();
        assert!(expr.to_javascript(&mut output, ret));
        output
    }
    fn identifier(name: &str) -> Expr {
        CxfaFmExpression::Identifier(wide(name)).rc()
    }
    fn number(value: &str) -> Expr {
        CxfaFmExpression::Number(wide(value)).rc()
    }
    #[test]
    fn MoreThan32Arguments() {
        let expr = CxfaFmExpression::Call {
            expression: identifier("sign"),
            arguments: (0..50).map(|_| CxfaFmExpression::Null.rc()).collect(),
            is_som_method: true,
        };
        let actual = emit(expr, ReturnType::Inferred);
        let mut expected = String::from("sign(");
        for index in 0..50 {
            if index > 0 {
                expected.push_str(", ");
            }
            expected.push_str(if matches!(index, 0 | 5 | 6) {
                "pfm_rt.get_jsobj(null)"
            } else {
                "pfm_rt.get_val(null)"
            });
        }
        expected.push(')');
        assert_eq!(actual, wide(&expected));
    }
    #[test]
    fn Empty() {
        assert_eq!(
            emit(CxfaFmExpression::String(Vec::new()), ReturnType::Inferred),
            Vec::<u32>::new()
        );
    }
    #[test]
    fn Short() {
        assert_eq!(
            emit(CxfaFmExpression::String(wide("a")), ReturnType::Inferred),
            wide("a")
        );
    }
    #[test]
    fn Medium() {
        assert_eq!(
            emit(
                CxfaFmExpression::String(wide(".abcd.")),
                ReturnType::Inferred
            ),
            wide("\"abcd\"")
        );
    }
    #[test]
    fn Long() {
        assert_eq!(
            emit(
                CxfaFmExpression::String(vec![u32::from(b'A'); 140000]),
                ReturnType::Inferred
            )
            .len(),
            140000
        );
    }
    #[test]
    fn Quoted() {
        assert_eq!(
            emit(
                CxfaFmExpression::String(wide(r#".Simon says ""run""."#)),
                ReturnType::Inferred
            ),
            wide("\"Simon says \\\"run\\\"\"")
        );
    }
    #[test]
    fn VarExpressionInitNull() {
        assert_eq!(
            emit(
                CxfaFmExpression::Var {
                    name: wide("s"),
                    init: None
                },
                ReturnType::Inferred
            ),
            wide("var s = \"\";\n")
        );
    }
    #[test]
    fn VarExpressionInitBlank() {
        assert_eq!(
            emit(
                CxfaFmExpression::Var {
                    name: wide("s"),
                    init: Some(CxfaFmExpression::String(wide("\"\"")).rc())
                },
                ReturnType::Inferred
            ),
            wide("var s = \"\";\ns = pfm_rt.var_filter(s);\n")
        );
    }
    #[test]
    fn VarExpressionInitString() {
        assert_eq!(
            emit(
                CxfaFmExpression::Var {
                    name: wide("s"),
                    init: Some(CxfaFmExpression::String(wide("\"foo\"")).rc())
                },
                ReturnType::Inferred
            ),
            wide("var s = \"foo\";\ns = pfm_rt.var_filter(s);\n")
        );
    }
    #[test]
    fn VarExpressionInitNumeric() {
        assert_eq!(
            emit(
                CxfaFmExpression::Var {
                    name: wide("s"),
                    init: Some(number("112"))
                },
                ReturnType::Inferred
            ),
            wide("var s = 112;\ns = pfm_rt.var_filter(s);\n")
        );
    }
    #[test]
    fn builtins_preserve_canonical_names_and_unknown_functions_fail() {
        for builtin in BUILT_INS {
            let mut name = wide(&builtin.to_ascii_lowercase());
            assert!(CxfaFmExpression::is_built_in_func(&mut name));
            assert_eq!(name, wide(builtin));
        }
        let mut unknown = wide("UnknownFunction");
        assert!(!CxfaFmExpression::is_built_in_func(&mut unknown));
        let call = CxfaFmExpression::Call {
            expression: identifier("UnknownFunction"),
            arguments: Vec::new(),
            is_som_method: false,
        };
        assert!(!call.to_javascript(&mut Vec::new(), ReturnType::Inferred));
        assert_eq!(
            emit(
                CxfaFmExpression::Call {
                    expression: identifier("eval"),
                    arguments: vec![number("1")],
                    is_som_method: false
                },
                ReturnType::Inferred
            ),
            wide("eval.call(this, pfm_rt.Translate(1))")
        );
        assert_eq!(emit(CxfaFmExpression::Call{expression:identifier("Exists"),arguments:Vec::new(),is_som_method:false},ReturnType::Inferred),wide("pfm_rt.Exists(\n(\nfunction ()\n{\ntry\n{\nreturn 0;\n}\ncatch(accessExceptions)\n{\nreturn 0;\n}\n}\n).call(this)\n)"));
    }
    #[test]
    fn accessors_preserve_source_quoting_indices_and_special_names() {
        let index = CxfaFmExpression::Index {
            accessor_index: AccessorIndex::NoIndex,
            expression: None,
            is_star_index: false,
        }
        .rc();
        assert_eq!(
            emit(
                CxfaFmExpression::DotAccessor {
                    accessor: Some(identifier("$form")),
                    op: TokenType::TOKdot,
                    identifier: wide("field"),
                    index: index.clone()
                },
                ReturnType::Inferred
            ),
            wide("pfm_rt.dot_acc(xfa.form, \"xfa.form\", \"field\", 0, 0)")
        );
        assert_eq!(
            emit(
                CxfaFmExpression::DotDotAccessor {
                    accessor: identifier("$"),
                    op: TokenType::TOKdotdot,
                    identifier: wide("field"),
                    index
                },
                ReturnType::Inferred
            ),
            wide("pfm_rt.dotdot_acc(this, \"this\", \"field\", 0, 0)")
        );
        assert_eq!(
            emit(
                CxfaFmExpression::Identifier(wide("!name")),
                ReturnType::Inferred
            ),
            wide("pfm__excl__name")
        );
    }
    #[test]
    fn repeated_argument_nodes_and_names_use_positional_delimiters() {
        // Source pointer/value comparisons can omit commas for repeated nodes
        // or names. Emission follows argument positions instead.
        let null = CxfaFmExpression::Null.rc();
        assert_eq!(
            emit(
                CxfaFmExpression::Call {
                    expression: identifier("Abs"),
                    arguments: vec![null.clone(), null],
                    is_som_method: false
                },
                ReturnType::Inferred
            ),
            wide("pfm_rt.Abs(null, null)")
        );
        assert_eq!(
            emit(
                CxfaFmExpression::FunctionDefinition {
                    name: wide("f"),
                    arguments: vec![wide("a"), wide("a")],
                    expressions: Vec::new()
                },
                ReturnType::Inferred
            ),
            wide("function f(a, a) {\nvar pfm_ret = null;\nreturn pfm_ret;\n}\n")
        );
    }
    #[test]
    fn translation_depth_and_drop_do_not_use_the_host_call_stack() {
        let mut expr = CxfaFmExpression::Null.rc();
        for _ in 0..6000 {
            expr = CxfaFmExpression::Unary {
                op: TokenType::TOKplus,
                expression: expr,
            }
            .rc();
        }
        assert!(!expr.to_javascript(&mut Vec::new(), ReturnType::Inferred));
        drop(expr);
        assert_eq!(
            emit(CxfaFmExpression::Number(wide("7")), ReturnType::Inferred),
            wide("7")
        );
    }
}
