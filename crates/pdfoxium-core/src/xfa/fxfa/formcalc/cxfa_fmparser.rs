// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Recursive-descent FormCalc grammar from
//! `xfa/fxfa/formcalc/cxfa_fmparser.{h,cpp}`. Immutable owned AST nodes replace
//! cppgc allocations; parse-depth restoration follows the source AutoRestorer.
use super::cxfa_fmexpression::{AccessorIndex, CxfaFmAst, CxfaFmExpression as E, Expr};
use super::cxfa_fmlexer::{CxfaFmLexer, Token, TokenType};
use std::rc::Rc;
use TokenType::*;

const MAX_PARSE_DEPTH: usize = 1250;
const MAX_POST_EXPRESSIONS: usize = 256;
const MAX_EXPRESSION_LIST_SIZE: usize = 10_000;

pub struct CxfaFmParser {
    lexer: CxfaFmLexer,
    token: Token,
    error: bool,
    parse_depth: usize,
    max_parse_depth: usize,
}
impl CxfaFmParser {
    pub fn new(lexer: CxfaFmLexer) -> Self {
        Self {
            lexer,
            token: Token::new(TOKreserver),
            error: false,
            parse_depth: 0,
            max_parse_depth: MAX_PARSE_DEPTH,
        }
    }
    pub fn parse(&mut self) -> Option<CxfaFmAst> {
        self.token = self.lexer.next_token();
        if self.has_error() {
            return None;
        }
        let expressions = self.parse_expression_list();
        if self.has_error() || !self.lexer.is_complete() {
            return None;
        }
        Some(CxfaFmAst::new(expressions))
    }
    pub fn has_error(&self) -> bool {
        self.error || self.token.kind == TOKreserver
    }
    pub fn set_max_parse_depth_for_test(&mut self, max_depth: usize) {
        self.max_parse_depth = max_depth;
    }
    fn next_token(&mut self) -> bool {
        if self.has_error() {
            return false;
        }
        self.token = self.lexer.next_token();
        while !self.has_error() && self.token.kind == TOKreserver {
            self.token = self.lexer.next_token();
        }
        !self.has_error()
    }
    fn check_then_next(&mut self, op: TokenType) -> bool {
        if self.has_error() {
            return false;
        }
        if self.token.kind != op {
            self.error = true;
            return false;
        }
        self.next_token()
    }
    fn increment_parse_depth_and_check(&mut self) -> bool {
        self.parse_depth += 1;
        self.parse_depth < self.max_parse_depth
    }
    /// The closure lets every early return restore the source recursion count.
    fn with_depth<T>(&mut self, body: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let previous = self.parse_depth;
        let result = if self.has_error() || !self.increment_parse_depth_and_check() {
            None
        } else {
            body(self)
        };
        self.parse_depth = previous;
        result
    }
    fn parse_expression_list(&mut self) -> Vec<Expr> {
        self.with_depth(|s| {
            let mut expressions = Vec::new();
            while !s.has_error() {
                if matches!(
                    s.token.kind,
                    TOKeof
                        | TOKendfunc
                        | TOKendif
                        | TOKelseif
                        | TOKelse
                        | TOKendwhile
                        | TOKendfor
                        | TOKend
                        | TOKreserver
                ) {
                    break;
                }
                let expr = if s.token.kind == TOKfunc {
                    s.parse_function()
                } else {
                    s.parse_expression()
                };
                let Some(expr) = expr else {
                    s.error = true;
                    return None;
                };
                if expressions.len() >= MAX_EXPRESSION_LIST_SIZE {
                    s.error = true;
                    return None;
                }
                expressions.push(expr);
            }
            Some(expressions)
        })
        .unwrap_or_default()
    }
    // Func := 'func' Identifier '(' ParameterList ')' do ExpressionList 'endfunc'.
    fn parse_function(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            if !s.check_then_next(TOKfunc) || s.token.kind != TOKidentifier {
                return None;
            }
            let name = s.token.string().to_vec();
            if !s.next_token() || !s.check_then_next(TOKlparen) {
                return None;
            }
            let mut arguments = Vec::new();
            let mut last_was_comma = false;
            loop {
                if s.token.kind == TOKrparen {
                    break;
                }
                if s.token.kind != TOKidentifier {
                    return None;
                }
                last_was_comma = false;
                arguments.push(s.token.string().to_vec());
                if !s.next_token() {
                    return None;
                }
                if s.token.kind != TOKcomma {
                    continue;
                }
                last_was_comma = true;
                if !s.next_token() {
                    return None;
                }
            }
            if last_was_comma || !s.check_then_next(TOKrparen) || !s.check_then_next(TOKdo) {
                return None;
            }
            let expressions = if s.token.kind == TOKendfunc {
                Vec::new()
            } else {
                s.parse_expression_list()
            };
            if !s.check_then_next(TOKendfunc) {
                return None;
            }
            Some(Rc::new(E::FunctionDefinition {
                name,
                arguments,
                expressions,
            }))
        })
    }
    fn parse_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| match s.token.kind {
            TOKvar => s.parse_declaration_expression(),
            TOKnull | TOKnumber | TOKstring | TOKplus | TOKminus | TOKksnot | TOKidentifier
            | TOKlparen => s.parse_exp_expression(),
            TOKif => s.parse_if_expression(),
            TOKwhile => s.parse_while_expression(),
            TOKfor => s.parse_for_expression(),
            TOKforeach => s.parse_foreach_expression(),
            TOKdo => s.parse_do_expression(),
            TOKbreak => s.next_token().then(|| Rc::new(E::Break)),
            TOKcontinue => s.next_token().then(|| Rc::new(E::Continue)),
            _ => None,
        })
    }
    // Declaration := 'var' Variable ('=' SimpleExpression)?.
    fn parse_declaration_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            if !s.next_token() || s.token.kind != TOKidentifier {
                return None;
            }
            let name = s.token.string().to_vec();
            if !s.next_token() {
                return None;
            }
            let init = if s.token.kind == TOKassign {
                if !s.next_token() {
                    return None;
                }
                Some(s.parse_simple_expression()?)
            } else {
                None
            };
            Some(Rc::new(E::Var { name, init }))
        })
    }
    fn parse_simple_expression(&mut self) -> Option<Expr> {
        if self.has_error() {
            None
        } else {
            self.parse_logical_or_expression()
        }
    }
    // Exp := SimpleExpression ('=' SimpleExpression)?.
    fn parse_exp_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            let mut expression = s.parse_simple_expression()?;
            if s.token.kind == TOKassign {
                if !s.next_token() {
                    return None;
                }
                let right = s.parse_simple_expression()?;
                expression = Rc::new(E::Assign {
                    op: TOKassign,
                    left: expression,
                    right,
                });
            }
            Some(Rc::new(E::Exp(expression)))
        })
    }
    fn parse_logical_or_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            let mut left = s.parse_logical_and_expression()?;
            loop {
                if !s.increment_parse_depth_and_check() {
                    return None;
                }
                if !matches!(s.token.kind, TOKor | TOKksor) {
                    return Some(left);
                }
                if !s.next_token() {
                    return None;
                }
                let right = s.parse_logical_and_expression()?;
                left = Rc::new(E::Binary {
                    op: TOKor,
                    left,
                    right,
                });
            }
        })
    }
    fn parse_logical_and_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            let mut left = s.parse_equality_expression()?;
            loop {
                if !s.increment_parse_depth_and_check() {
                    return None;
                }
                if !matches!(s.token.kind, TOKand | TOKksand) {
                    return Some(left);
                }
                if !s.next_token() {
                    return None;
                }
                let right = s.parse_equality_expression()?;
                left = Rc::new(E::Binary {
                    op: TOKand,
                    left,
                    right,
                });
            }
        })
    }
    fn parse_equality_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            let mut left = s.parse_relational_expression()?;
            loop {
                if !s.increment_parse_depth_and_check() {
                    return None;
                }
                let op = match s.token.kind {
                    TOKeq | TOKkseq => TOKeq,
                    TOKne | TOKksne => TOKne,
                    _ => return Some(left),
                };
                if !s.next_token() {
                    return None;
                }
                let right = s.parse_relational_expression()?;
                left = Rc::new(E::Binary { op, left, right });
            }
        })
    }
    fn parse_relational_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            let mut left = s.parse_additive_expression()?;
            loop {
                if !s.increment_parse_depth_and_check() {
                    return None;
                }
                let op = match s.token.kind {
                    TOKlt | TOKkslt => TOKlt,
                    TOKgt | TOKksgt => TOKgt,
                    TOKle | TOKksle => TOKle,
                    TOKge | TOKksge => TOKge,
                    _ => return Some(left),
                };
                if !s.next_token() {
                    return None;
                }
                let right = s.parse_additive_expression()?;
                left = Rc::new(E::Binary { op, left, right });
            }
        })
    }
    fn parse_additive_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            let mut left = s.parse_multiplicative_expression()?;
            loop {
                if !s.increment_parse_depth_and_check() {
                    return None;
                }
                let op = match s.token.kind {
                    TOKplus => TOKplus,
                    TOKminus => TOKminus,
                    _ => return Some(left),
                };
                if !s.next_token() {
                    return None;
                }
                let right = s.parse_multiplicative_expression()?;
                left = Rc::new(E::Binary { op, left, right });
            }
        })
    }
    fn parse_multiplicative_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            let mut left = s.parse_unary_expression()?;
            loop {
                if !s.increment_parse_depth_and_check() {
                    return None;
                }
                let op = match s.token.kind {
                    TOKmul => TOKmul,
                    TOKdiv => TOKdiv,
                    _ => return Some(left),
                };
                if !s.next_token() {
                    return None;
                }
                let right = s.parse_unary_expression()?;
                left = Rc::new(E::Binary { op, left, right });
            }
        })
    }
    fn parse_unary_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            let op = match s.token.kind {
                TOKplus => TOKplus,
                TOKminus => TOKminus,
                TOKksnot => TOKksnot,
                _ => return s.parse_primary_expression(),
            };
            if !s.next_token() {
                return None;
            }
            Some(Rc::new(E::Unary {
                op,
                expression: s.parse_unary_expression()?,
            }))
        })
    }
    fn parse_primary_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            if let Some(expr) = s.parse_literal() {
                return s.next_token().then_some(expr);
            }
            let expr = match s.token.kind {
                TOKidentifier => {
                    let identifier = s.token.string().to_vec();
                    if !s.next_token() {
                        return None;
                    }
                    if s.token.kind == TOKlbracket {
                        let index = s.parse_index_expression()?;
                        if !s.next_token() {
                            return None;
                        }
                        Rc::new(E::DotAccessor {
                            accessor: None,
                            op: TOKdot,
                            identifier,
                            index,
                        })
                    } else {
                        Rc::new(E::Identifier(identifier))
                    }
                }
                TOKlparen => s.parse_paren_expression()?,
                _ => return None,
            };
            s.parse_post_expression(expr)
        })
    }
    fn parse_literal(&self) -> Option<Expr> {
        match self.token.kind {
            TOKnumber => Some(Rc::new(E::Number(self.token.string().to_vec()))),
            TOKstring => Some(Rc::new(E::String(self.token.string().to_vec()))),
            TOKnull => Some(Rc::new(E::Null)),
            _ => None,
        }
    }
    // Accessors: calls, '.', '..', '.#', '.*', with optional index suffixes.
    fn parse_post_expression(&mut self, mut expr: Expr) -> Option<Expr> {
        self.with_depth(|s| {
            let mut count = 0;
            loop {
                count += 1;
                if count > MAX_POST_EXPRESSIONS {
                    return None;
                }
                match s.token.kind {
                    TOKlparen => {
                        let arguments = s.parse_argument_list()?;
                        expr = Rc::new(E::Call {
                            expression: expr,
                            arguments,
                            is_som_method: false,
                        });
                        if !s.next_token() {
                            return None;
                        }
                        if s.token.kind != TOKlbracket {
                            continue;
                        }
                        let index = s.parse_index_expression()?;
                        expr = Rc::new(E::DotAccessor {
                            accessor: Some(expr),
                            op: TOKcall,
                            identifier: Vec::new(),
                            index,
                        });
                    }
                    TOKdot => {
                        if !s.next_token() || s.token.kind != TOKidentifier {
                            return None;
                        }
                        let identifier = s.token.string().to_vec();
                        if !s.next_token() {
                            return None;
                        }
                        if s.token.kind == TOKlparen {
                            let arguments = s.parse_argument_list()?;
                            let call = Rc::new(E::Call {
                                expression: Rc::new(E::Identifier(identifier)),
                                arguments,
                                is_som_method: true,
                            });
                            expr = Rc::new(E::MethodCall {
                                accessor: expr,
                                call,
                            });
                            if !s.next_token() {
                                return None;
                            }
                            if s.token.kind != TOKlbracket {
                                continue;
                            }
                            let index = s.parse_index_expression()?;
                            expr = Rc::new(E::DotAccessor {
                                accessor: Some(expr),
                                op: TOKcall,
                                identifier: Vec::new(),
                                index,
                            });
                        } else if s.token.kind == TOKlbracket {
                            let index = s.parse_index_expression()?;
                            expr = Rc::new(E::DotAccessor {
                                accessor: Some(expr),
                                op: TOKdot,
                                identifier,
                                index,
                            });
                        } else {
                            expr = Rc::new(E::DotAccessor {
                                accessor: Some(expr),
                                op: TOKdot,
                                identifier,
                                index: no_index(),
                            });
                            continue;
                        }
                    }
                    TOKdotdot => {
                        if !s.next_token() || s.token.kind != TOKidentifier {
                            return None;
                        }
                        let identifier = s.token.string().to_vec();
                        if !s.next_token() {
                            return None;
                        }
                        if s.token.kind == TOKlbracket {
                            let index = s.parse_index_expression()?;
                            expr = Rc::new(E::DotDotAccessor {
                                accessor: expr,
                                op: TOKdotdot,
                                identifier,
                                index,
                            });
                        } else {
                            expr = Rc::new(E::DotDotAccessor {
                                accessor: expr,
                                op: TOKdotdot,
                                identifier,
                                index: no_index(),
                            });
                            continue;
                        }
                    }
                    TOKdotscream => {
                        if !s.next_token() || s.token.kind != TOKidentifier {
                            return None;
                        }
                        let identifier = s.token.string().to_vec();
                        if !s.next_token() {
                            return None;
                        }
                        if s.token.kind != TOKlbracket {
                            expr = Rc::new(E::DotAccessor {
                                accessor: Some(expr),
                                op: TOKdotscream,
                                identifier,
                                index: no_index(),
                            });
                            continue;
                        }
                        let index = s.parse_index_expression()?;
                        expr = Rc::new(E::DotAccessor {
                            accessor: Some(expr),
                            op: TOKdotscream,
                            identifier,
                            index,
                        });
                    }
                    TOKdotstar => {
                        expr = Rc::new(E::DotAccessor {
                            accessor: Some(expr),
                            op: TOKdotstar,
                            identifier: vec![u32::from(b'*')],
                            index: no_index(),
                        });
                    }
                    _ => return Some(expr),
                }
                if !s.next_token() {
                    return None;
                }
            }
        })
    }
    // '(' (SimpleExpression (',' SimpleExpression)*)? ')'.
    fn parse_argument_list(&mut self) -> Option<Vec<Expr>> {
        if self.token.kind != TOKlparen || !self.next_token() {
            return None;
        }
        let mut expressions = Vec::new();
        let mut first = true;
        while self.token.kind != TOKrparen {
            if first {
                first = false;
            } else if self.token.kind != TOKcomma || !self.next_token() {
                return None;
            }
            expressions.push(self.parse_simple_expression()?);
            if expressions.len() > MAX_POST_EXPRESSIONS {
                return None;
            }
        }
        Some(expressions)
    }
    // '[' ('*' | '+' SimpleExpression | '-' SimpleExpression) ']'.
    fn parse_index_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            if !s.check_then_next(TOKlbracket) {
                return None;
            }
            if s.token.kind == TOKmul {
                if !s.next_token() || s.token.kind != TOKrbracket {
                    return None;
                }
                return Some(Rc::new(E::Index {
                    accessor_index: AccessorIndex::NoRelativeIndex,
                    expression: None,
                    is_star_index: true,
                }));
            }
            let accessor_index = if s.token.kind == TOKplus {
                if !s.next_token() {
                    return None;
                }
                AccessorIndex::PositiveIndex
            } else if s.token.kind == TOKminus {
                if !s.next_token() {
                    return None;
                }
                AccessorIndex::NegativeIndex
            } else {
                AccessorIndex::NoRelativeIndex
            };
            let expression = s.parse_simple_expression()?;
            if s.token.kind != TOKrbracket {
                return None;
            }
            Some(Rc::new(E::Index {
                accessor_index,
                expression: Some(expression),
                is_star_index: false,
            }))
        })
    }
    fn parse_paren_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            if !s.check_then_next(TOKlparen) || s.token.kind == TOKrparen {
                return None;
            }
            let expression = s.parse_simple_expression()?;
            if !s.check_then_next(TOKrparen) {
                return None;
            }
            Some(expression)
        })
    }
    // 'if' '(' SimpleExpression ')' 'then' ExpressionList ... 'endif'.
    fn parse_if_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            if !s.check_then_next(TOKif) {
                return None;
            }
            let condition = s.parse_paren_expression()?;
            if !s.check_then_next(TOKthen) {
                return None;
            }
            let if_expression = Some(Rc::new(E::Block(s.parse_expression_list())));
            let mut else_if_expressions = Vec::new();
            while s.token.kind == TOKelseif {
                if !s.next_token() {
                    return None;
                }
                let condition = s.parse_paren_expression()?;
                if !s.check_then_next(TOKthen) {
                    return None;
                }
                let if_expression = Some(Rc::new(E::Block(s.parse_expression_list())));
                else_if_expressions.push(Rc::new(E::If {
                    condition,
                    if_expression,
                    else_if_expressions: Vec::new(),
                    else_expression: None,
                }));
            }
            let else_expression = if s.token.kind == TOKelse {
                if !s.next_token() {
                    return None;
                }
                Some(Rc::new(E::Block(s.parse_expression_list())))
            } else {
                None
            };
            if !s.check_then_next(TOKendif) {
                return None;
            }
            Some(Rc::new(E::If {
                condition,
                if_expression,
                else_if_expressions,
                else_expression,
            }))
        })
    }
    fn parse_while_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            if !s.check_then_next(TOKwhile) {
                return None;
            }
            let condition = s.parse_paren_expression()?;
            if !s.check_then_next(TOKdo) {
                return None;
            }
            let expressions = s.parse_expression_list();
            if !s.check_then_next(TOKendwhile) {
                return None;
            }
            Some(Rc::new(E::While {
                condition,
                expression: Rc::new(E::Block(expressions)),
            }))
        })
    }
    fn parse_for_expression(&mut self) -> Option<Expr> {
        self.with_depth(|s| {
            if !s.check_then_next(TOKfor) || s.token.kind != TOKidentifier {
                return None;
            }
            let variant = s.token.string().to_vec();
            if !s.next_token() || !s.check_then_next(TOKassign) {
                return None;
            }
            let assignment = s.parse_simple_expression()?;
            let direction = match s.token.kind {
                TOKupto => true,
                TOKdownto => false,
                _ => return None,
            };
            if !s.next_token() {
                return None;
            }
            let accessor = s.parse_simple_expression()?;
            let step = if s.token.kind == TOKstep {
                if !s.next_token() {
                    return None;
                }
                Some(s.parse_simple_expression()?)
            } else {
                None
            };
            if !s.check_then_next(TOKdo) {
                return None;
            }
            let expressions = s.parse_expression_list();
            if !s.check_then_next(TOKendfor) {
                return None;
            }
            Some(Rc::new(E::For {
                variant,
                assignment,
                accessor,
                direction,
                step,
                list: Rc::new(E::Block(expressions)),
            }))
        })
    }
    fn parse_foreach_expression(&mut self) -> Option<Expr> {
        if self.token.kind != TOKforeach {
            return None;
        }
        self.with_depth(|s| {
            if !s.check_then_next(TOKforeach) || s.token.kind != TOKidentifier {
                return None;
            }
            let identifier = s.token.string().to_vec();
            if !s.next_token() || !s.check_then_next(TOKin) || !s.check_then_next(TOKlparen) {
                return None;
            }
            let mut accessors = Vec::new();
            while s.token.kind != TOKrparen {
                accessors.push(s.parse_simple_expression()?);
                if s.token.kind != TOKcomma {
                    break;
                }
                if !s.next_token() {
                    return None;
                }
            }
            if accessors.is_empty() || !s.check_then_next(TOKrparen) {
                return None;
            }
            // Consume the grammar's loop delimiter, as for/while do. The
            // source omits this and tries to parse `do` as a nested block
            // requiring `end`, rejecting otherwise valid foreach programs.
            if !s.check_then_next(TOKdo) {
                return None;
            }
            let expressions = s.parse_expression_list();
            if !s.check_then_next(TOKendfor) {
                return None;
            }
            Some(Rc::new(E::Foreach {
                identifier,
                accessors,
                list: Rc::new(E::Block(expressions)),
            }))
        })
    }
    fn parse_do_expression(&mut self) -> Option<Expr> {
        if self.token.kind != TOKdo {
            return None;
        }
        self.with_depth(|s| {
            if !s.check_then_next(TOKdo) {
                return None;
            }
            let expressions = s.parse_expression_list();
            if !s.check_then_next(TOKend) {
                return None;
            }
            Some(Rc::new(E::Do(Rc::new(E::Block(expressions)))))
        })
    }
}
fn no_index() -> Expr {
    Rc::new(E::Index {
        accessor_index: AccessorIndex::NoIndex,
        expression: None,
        is_star_index: false,
    })
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    fn parser(input: &str) -> CxfaFmParser {
        CxfaFmParser::new(CxfaFmLexer::new(
            input.chars().map(u32::from).collect::<Vec<_>>(),
        ))
    }
    fn wide(input: &str) -> Vec<u32> {
        input.chars().map(u32::from).collect()
    }
    #[test]
    fn Empty() {
        let input = r#""#;
        let expected = r#"// comments only"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn CommentOnlyIsError() {
        let input = r#"; Just comment"#;
        let expected = r#"// comments only"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn CommentThenValue() {
        let input = r#"; Just comment
12"#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
pfm_ret = 12;
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn Parse() {
        let input = r#"$ = Avg (-3, 5, -6, 12, -13);
$ = Avg (Table2..Row[*].Cell1);
if ($ ne -1)then
  border.fill.color.value = "255,64,64";
elseif ($ ne -2) then
  border.fill.color.value = "128,128,128";
else
  border.fill.color.value = "20,170,13";
endif
$"#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
if (pfm_rt.is_obj(this))
{
pfm_rt.asgn_val_op(this, pfm_rt.Avg(pfm_rt.neg_op(3), 5, pfm_rt.neg_op(6), 12, pfm_rt.neg_op(13)));
}
if (pfm_rt.is_obj(this))
{
pfm_rt.asgn_val_op(this, pfm_rt.Avg(pfm_rt.dot_acc(pfm_rt.dotdot_acc(Table2, "Table2", "Row", 1), "", "Cell1", 0, 0)));
}
if (pfm_rt.get_val(pfm_rt.neq_op(this, pfm_rt.neg_op(1))))
{
if (pfm_rt.is_obj(pfm_rt.dot_acc(pfm_rt.dot_acc(pfm_rt.dot_acc(border, "border", "fill", 0, 0), "", "color", 0, 0), "", "value", 0, 0)))
{
pfm_rt.asgn_val_op(pfm_rt.dot_acc(pfm_rt.dot_acc(pfm_rt.dot_acc(border, "border", "fill", 0, 0), "", "color", 0, 0), "", "value", 0, 0), "255,64,64");
}
}
else if (pfm_rt.get_val(pfm_rt.neq_op(this, pfm_rt.neg_op(2))))
{
if (pfm_rt.is_obj(pfm_rt.dot_acc(pfm_rt.dot_acc(pfm_rt.dot_acc(border, "border", "fill", 0, 0), "", "color", 0, 0), "", "value", 0, 0)))
{
pfm_rt.asgn_val_op(pfm_rt.dot_acc(pfm_rt.dot_acc(pfm_rt.dot_acc(border, "border", "fill", 0, 0), "", "color", 0, 0), "", "value", 0, 0), "128,128,128");
}
}
else {
if (pfm_rt.is_obj(pfm_rt.dot_acc(pfm_rt.dot_acc(pfm_rt.dot_acc(border, "border", "fill", 0, 0), "", "color", 0, 0), "", "value", 0, 0)))
{
pfm_rt.asgn_val_op(pfm_rt.dot_acc(pfm_rt.dot_acc(pfm_rt.dot_acc(border, "border", "fill", 0, 0), "", "color", 0, 0), "", "value", 0, 0), "20,170,13");
}
}
pfm_ret = this;
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn MaxParseDepth() {
        let input = r#"foo(bar[baz(fizz[0])])"#;
        let mut parser = parser(input);
        parser.set_max_parse_depth_for_test(5);
        assert!(parser.parse().is_none());
        assert!(parser.has_error());
    }
    #[test]
    fn chromium752201() {
        let input = r#"fTep a
.#
fo@ =[=l"#;
        let mut parser = parser(input);
        assert!(parser.parse().is_none());
        assert!(parser.has_error());
    }
    #[test]
    fn MultipleAssignmentIsNotAllowed() {
        let input = r#"(a=(b=t))=u"#;
        let mut parser = parser(input);
        assert!(parser.parse().is_none());
        assert!(parser.has_error());
    }
    #[test]
    fn ParseFuncWithParams() {
        let input = r#"func MyFunction(param1, param2) do
  param1 * param2
endfunc"#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
function MyFunction(param1, param2) {
var pfm_ret = null;
pfm_ret = pfm_rt.mul_op(param1, param2);
return pfm_ret;
}
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn ParseFuncWithoutParams() {
        let input = r#"func MyFunction() do
  42
endfunc"#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
function MyFunction() {
var pfm_ret = null;
pfm_ret = 42;
return pfm_ret;
}
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn ParseFuncWithBadParamsList() {
        let input = r#"func MyFunction(param1,) do
  param1 * param2
endfunc"#;
        let mut parser = parser(input);
        assert!(parser.parse().is_none());
        assert!(parser.has_error());
    }
    #[test]
    fn ParseBadIfExpression() {
        let input = r#"if ( then"#;
        let mut parser = parser(input);
        assert!(parser.parse().is_none());
        assert!(parser.has_error());
    }
    #[test]
    fn ParseBadElseIfExpression() {
        let input = r#"if ($ ne -1) then"
elseif( then"#;
        let mut parser = parser(input);
        assert!(parser.parse().is_none());
        assert!(parser.has_error());
    }
    #[test]
    fn ParseDepthWithWideTree() {
        let input = r#"a <> b <> c <> d <> e <> f <> g <> h <> i <> j"#;
        let mut ordinary = parser(input);
        assert!(ordinary.parse().is_some());
        assert!(!ordinary.has_error());
        let mut limited = parser(input);
        limited.set_max_parse_depth_for_test(5);
        assert!(limited.parse().is_none());
        assert!(limited.has_error());
    }
    #[test]
    fn ParseCallSmall() {
        let input = r#"i.f(O)"#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
pfm_ret = pfm_rt.get_val((function() {
  return pfm_method_runner(i, function(obj) {
    return obj.f(pfm_rt.get_val(O));
  });
}).call(this));
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn ParseCallBig() {
        let input = r#"i.f(O.e(O.e(O)))"#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
pfm_ret = pfm_rt.get_val((function() {
  return pfm_method_runner(i, function(obj) {
    return obj.f(pfm_rt.get_val((function() {
  return pfm_method_runner(O, function(obj) {
    return obj.e(pfm_rt.get_val((function() {
  return pfm_method_runner(O, function(obj) {
    return obj.e(pfm_rt.get_val(O));
  });
}).call(this)));
  });
}).call(this)));
  });
}).call(this));
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn ParseVar() {
        let input = r#"var s = """#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
var s = "";
s = pfm_rt.var_filter(s);
pfm_ret = s;
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn ParseFunctionCallNoArguments() {
        let input = r#"P.x()"#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
pfm_ret = pfm_rt.get_val((function() {
  return pfm_method_runner(P, function(obj) {
    return obj.x();
  });
}).call(this));
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn ParseFunctionCallSingleArgument() {
        let input = r#"P.x(foo)"#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
pfm_ret = pfm_rt.get_val((function() {
  return pfm_method_runner(P, function(obj) {
    return obj.x(pfm_rt.get_jsobj(foo));
  });
}).call(this));
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn ParseFunctionCallMultipleArguments() {
        let input = r#"P.x(foo, bar, baz)"#;
        let expected = r#"(function() {
let pfm_method_runner = function(obj, cb) {
  if (pfm_rt.is_ary(obj)) {
    let pfm_method_return = null;
    for (var idx = obj.length -1; idx > 1; idx--) {
      pfm_method_return = cb(obj[idx]);
    }
    return pfm_method_return;
  }
  return cb(obj);
};
var pfm_ret = null;
pfm_ret = pfm_rt.get_val((function() {
  return pfm_method_runner(P, function(obj) {
    return obj.x(pfm_rt.get_jsobj(foo), pfm_rt.get_val(bar), pfm_rt.get_val(baz));
  });
}).call(this));
return pfm_rt.get_val(pfm_ret);
}).call(this);"#;
        let mut parser = parser(input);
        let ast = parser.parse().expect("source accepts this program");
        assert!(!parser.has_error());
        assert_eq!(
            ast.to_javascript().expect("source JavaScript output"),
            wide(expected)
        );
    }
    #[test]
    fn ParseFunctionCallMissingCommas() {
        let input = r#"P.x(!foo!bar!baz)"#;
        let mut parser = parser(input);
        assert!(parser.parse().is_none());
        assert!(parser.has_error());
    }
    #[test]
    fn ParseFunctionCallTrailingComma() {
        let input = r#"P.x(foo,bar,baz,)"#;
        let mut parser = parser(input);
        assert!(parser.parse().is_none());
        assert!(parser.has_error());
    }
    #[test]
    fn ParseFunctionCallExtraComma() {
        let input = r#"P.x(foo,bar,,baz)"#;
        let mut parser = parser(input);
        assert!(parser.parse().is_none());
        assert!(parser.has_error());
    }
}
