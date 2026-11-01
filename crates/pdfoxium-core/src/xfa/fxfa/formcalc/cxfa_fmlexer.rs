// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! FormCalc lexing from `xfa/fxfa/formcalc/cxfa_fmlexer.{h,cpp}`.
//! Tokens retain shared source spans rather than copying every suffix. Stored
//! AST text is copied by its owner, while token clones share the input.
use std::ops::Range;
use std::rc::Rc;
use unicode_general_category::{get_general_category, GeneralCategory};

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TokenType {
    TOKand,
    TOKlparen,
    TOKrparen,
    TOKmul,
    TOKplus,
    TOKcomma,
    TOKminus,
    TOKdot,
    TOKdiv,
    TOKlt,
    TOKassign,
    TOKgt,
    TOKlbracket,
    TOKrbracket,
    TOKor,
    TOKdotscream,
    TOKdotstar,
    TOKdotdot,
    TOKle,
    TOKne,
    TOKeq,
    TOKge,
    TOKdo,
    TOKkseq,
    TOKksge,
    TOKksgt,
    TOKif,
    TOKin,
    TOKksle,
    TOKkslt,
    TOKksne,
    TOKksor,
    TOKnull,
    TOKbreak,
    TOKksand,
    TOKend,
    TOKeof,
    TOKfor,
    TOKnan,
    TOKksnot,
    TOKvar,
    TOKthen,
    TOKelse,
    TOKexit,
    TOKdownto,
    TOKreturn,
    TOKinfinity,
    TOKendwhile,
    TOKforeach,
    TOKendfunc,
    TOKelseif,
    TOKwhile,
    TOKendfor,
    TOKthrow,
    TOKstep,
    TOKupto,
    TOKcontinue,
    TOKfunc,
    TOKendif,
    TOKstar,
    TOKidentifier,
    TOKunderscore,
    TOKdollar,
    TOKexclamation,
    TOKcall,
    TOKstring,
    TOKnumber,
    #[default]
    TOKreserver,
}
pub type XfaFmToken = TokenType;

#[derive(Clone, Debug, Default)]
pub struct Token {
    pub kind: TokenType,
    input: Option<Rc<[u32]>>,
    range: Range<usize>,
}
impl Token {
    pub fn new(kind: TokenType) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }
    fn source(kind: TokenType, input: &Rc<[u32]>, range: Range<usize>) -> Self {
        Self {
            kind,
            input: Some(input.clone()),
            range,
        }
    }
    pub fn get_type(&self) -> TokenType {
        self.kind
    }
    pub fn string(&self) -> &[u32] {
        self.input
            .as_ref()
            .map_or(&[], |input| &input[self.range.clone()])
    }
    pub fn get_string(&self) -> &[u32] {
        self.string()
    }
}

pub struct CxfaFmLexer {
    input: Rc<[u32]>,
    cursor: usize,
    lexer_error: bool,
}
impl CxfaFmLexer {
    pub fn new(input: impl Into<Rc<[u32]>>) -> Self {
        Self {
            input: input.into(),
            cursor: 0,
            lexer_error: false,
        }
    }
    pub fn is_complete(&self) -> bool {
        self.cursor >= self.input.len()
    }
    pub fn has_error(&self) -> bool {
        self.lexer_error
    }
    pub fn next_token(&mut self) -> Token {
        use TokenType::*;
        if self.lexer_error {
            return Token::default();
        }
        while !self.is_complete() && self.input[self.cursor] != 0 {
            let ch = self.input[self.cursor];
            if !is_formcalc_character(ch) {
                return self.raise_error();
            }
            match ch {
                0x0a | 0x0d => self.cursor += 1,
                0x3b => self.advance_for_comment(),
                0x22 => return self.advance_for_string(),
                0x30..=0x39 => return self.advance_for_number(),
                0x3d | 0x3c | 0x3e => {
                    self.cursor += 1;
                    let plain = match ch {
                        0x3d => TOKassign,
                        0x3c => TOKlt,
                        _ => TOKgt,
                    };
                    if self.is_complete() {
                        return Token::new(plain);
                    }
                    let next = self.input[self.cursor];
                    if !is_formcalc_character(next) {
                        return self.raise_error();
                    }
                    let compound = match (ch, next) {
                        (0x3d, 0x3d) => Some(TOKeq),
                        (0x3c, 0x3d) => Some(TOKle),
                        (0x3c, 0x3e) => Some(TOKne),
                        (0x3e, 0x3d) => Some(TOKge),
                        _ => None,
                    };
                    if let Some(kind) = compound {
                        self.cursor += 1;
                        return Token::new(kind);
                    }
                    return Token::new(plain);
                }
                0x2c | 0x28 | 0x29 | 0x5b | 0x5d | 0x26 | 0x7c | 0x2b | 0x2d | 0x2a => {
                    self.cursor += 1;
                    return Token::new(match ch {
                        0x2c => TOKcomma,
                        0x28 => TOKlparen,
                        0x29 => TOKrparen,
                        0x5b => TOKlbracket,
                        0x5d => TOKrbracket,
                        0x26 => TOKand,
                        0x7c => TOKor,
                        0x2b => TOKplus,
                        0x2d => TOKminus,
                        _ => TOKmul,
                    });
                }
                0x2f => {
                    self.cursor += 1;
                    if self.is_complete() {
                        return Token::new(TOKdiv);
                    }
                    let next = self.input[self.cursor];
                    if !is_formcalc_character(next) {
                        return self.raise_error();
                    }
                    if next != 0x2f {
                        return Token::new(TOKdiv);
                    }
                    self.advance_for_comment();
                }
                0x2e => {
                    self.cursor += 1;
                    if self.is_complete() {
                        return Token::new(TOKdot);
                    }
                    let next = self.input[self.cursor];
                    if !is_formcalc_character(next) {
                        return self.raise_error();
                    }
                    let compound = match next {
                        0x2e => Some(TOKdotdot),
                        0x2a => Some(TOKdotstar),
                        0x23 => Some(TOKdotscream),
                        _ => None,
                    };
                    if let Some(kind) = compound {
                        self.cursor += 1;
                        return Token::new(kind);
                    }
                    if is_decimal_digit(next) {
                        self.cursor -= 1;
                        return self.advance_for_number();
                    }
                    return Token::new(TOKdot);
                }
                _ if matches!(ch, 0x09 | 0x0b | 0x0c | 0x20) => self.cursor += 1,
                _ => {
                    if !is_initial_identifier_character(ch) {
                        return self.raise_error();
                    }
                    return self.advance_for_identifier();
                }
            }
        }
        Token::new(TOKeof)
    }
    fn raise_error(&mut self) -> Token {
        self.lexer_error = true;
        Token::default()
    }
    fn advance_for_number(&mut self) -> Token {
        // FXSYS_wcstof uses fast_float's decimal/general grammar. This caller
        // needs only its consumed span, not the rounded numeric result.
        let start = self.cursor;
        let mut end = start;
        while self.input.get(end).copied().is_some_and(is_decimal_digit) {
            end += 1;
        }
        let mut has_digit = end != start;
        if self.input.get(end) == Some(&0x2e) {
            end += 1;
            let fraction = end;
            while self.input.get(end).copied().is_some_and(is_decimal_digit) {
                end += 1;
            }
            has_digit |= end != fraction;
        }
        if !has_digit {
            return self.raise_error();
        }
        if matches!(self.input.get(end), Some(0x45 | 0x65)) {
            let mut exponent = end + 1;
            if matches!(self.input.get(exponent), Some(0x2b | 0x2d)) {
                exponent += 1;
            }
            let digits = exponent;
            while self
                .input
                .get(exponent)
                .copied()
                .is_some_and(is_decimal_digit)
            {
                exponent += 1;
            }
            if exponent != digits {
                end = exponent;
            }
        }
        if self.input.get(end).copied().is_some_and(is_alpha) {
            return self.raise_error();
        }
        self.cursor = end;
        Token::source(TokenType::TOKnumber, &self.input, start..end)
    }
    fn advance_for_string(&mut self) -> Token {
        let start = self.cursor;
        self.cursor += 1;
        while !self.is_complete() && self.input[self.cursor] != 0 {
            if !is_formcalc_character(self.input[self.cursor]) {
                break;
            }
            if self.input[self.cursor] == 0x22 {
                self.cursor += 1;
                if self.is_complete() {
                    return Token::source(TokenType::TOKstring, &self.input, start..self.cursor);
                }
                if self.input[self.cursor] != 0x22 {
                    if !is_formcalc_character(self.input[self.cursor]) {
                        break;
                    }
                    return Token::source(TokenType::TOKstring, &self.input, start..self.cursor);
                }
            }
            self.cursor += 1;
        }
        self.raise_error()
    }
    fn advance_for_identifier(&mut self) -> Token {
        let start = self.cursor;
        self.cursor += 1;
        while !self.is_complete() && self.input[self.cursor] != 0 {
            let ch = self.input[self.cursor];
            if !is_formcalc_character(ch) {
                return self.raise_error();
            }
            if !is_identifier_character(ch) {
                break;
            }
            self.cursor += 1;
        }
        let kind = tokenize_identifier(&self.input[start..self.cursor]);
        Token::source(kind, &self.input, start..self.cursor)
    }
    fn advance_for_comment(&mut self) {
        self.cursor += 1;
        while !self.is_complete() && self.input[self.cursor] != 0 {
            let ch = self.input[self.cursor];
            if !is_formcalc_character(ch) {
                self.lexer_error = true;
                return;
            }
            self.cursor += 1;
            if matches!(ch, 0x0d | 0x0a) {
                return;
            }
        }
    }
}
fn is_formcalc_character(ch: u32) -> bool {
    matches!(ch, 0x09..=0x0d | 0x20..=0xd7ff | 0xe000..=0xfffd)
}
fn is_decimal_digit(ch: u32) -> bool {
    (0x30..=0x39).contains(&ch)
}
fn is_alpha(ch: u32) -> bool {
    char::from_u32(ch).is_some_and(|ch| {
        matches!(
            get_general_category(ch),
            GeneralCategory::UppercaseLetter
                | GeneralCategory::LowercaseLetter
                | GeneralCategory::TitlecaseLetter
                | GeneralCategory::ModifierLetter
                | GeneralCategory::OtherLetter
        )
    })
}
fn is_identifier_character(ch: u32) -> bool {
    is_alpha(ch)
        || matches!(ch, 0x5f | 0x24)
        || char::from_u32(ch)
            .is_some_and(|ch| get_general_category(ch) == GeneralCategory::DecimalNumber)
}
fn is_initial_identifier_character(ch: u32) -> bool {
    is_alpha(ch) || matches!(ch, 0x5f | 0x24 | 0x21)
}
fn tokenize_identifier(text: &[u32]) -> TokenType {
    use TokenType::*;
    const KEYWORDS: &[(TokenType, &[u8])] = &[
        (TOKdo, b"do"),
        (TOKkseq, b"eq"),
        (TOKksge, b"ge"),
        (TOKksgt, b"gt"),
        (TOKif, b"if"),
        (TOKin, b"in"),
        (TOKksle, b"le"),
        (TOKkslt, b"lt"),
        (TOKksne, b"ne"),
        (TOKksor, b"or"),
        (TOKnull, b"null"),
        (TOKbreak, b"break"),
        (TOKksand, b"and"),
        (TOKend, b"end"),
        (TOKeof, b"eof"),
        (TOKfor, b"for"),
        (TOKnan, b"nan"),
        (TOKksnot, b"not"),
        (TOKvar, b"var"),
        (TOKthen, b"then"),
        (TOKelse, b"else"),
        (TOKexit, b"exit"),
        (TOKdownto, b"downto"),
        (TOKreturn, b"return"),
        (TOKinfinity, b"infinity"),
        (TOKendwhile, b"endwhile"),
        (TOKforeach, b"foreach"),
        (TOKendfunc, b"endfunc"),
        (TOKelseif, b"elseif"),
        (TOKwhile, b"while"),
        (TOKendfor, b"endfor"),
        (TOKthrow, b"throw"),
        (TOKstep, b"step"),
        (TOKupto, b"upto"),
        (TOKcontinue, b"continue"),
        (TOKfunc, b"func"),
        (TOKendif, b"endif"),
    ];
    KEYWORDS
        .iter()
        .find(|(_, word)| {
            text.len() == word.len() && text.iter().copied().eq(word.iter().copied().map(u32::from))
        })
        .map_or(TOKidentifier, |(kind, _)| *kind)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use TokenType::*;
    fn lexer(text: &str) -> CxfaFmLexer {
        CxfaFmLexer::new(text.chars().map(u32::from).collect::<Vec<_>>())
    }
    fn token_text(token: &Token) -> String {
        token
            .string()
            .iter()
            .map(|&c| char::from_u32(c).unwrap())
            .collect()
    }
    #[test]
    fn NullString() {
        let mut lexer = CxfaFmLexer::new(Vec::new());
        assert_eq!(lexer.next_token().kind, TOKeof);
        assert!(lexer.is_complete());
    }
    #[test]
    fn EmptyString() {
        let mut lexer = lexer("");
        assert_eq!(lexer.next_token().kind, TOKeof);
        assert!(lexer.is_complete());
    }
    #[test]
    fn Numbers() {
        let mut negative = lexer("-12");
        assert_eq!(negative.next_token().kind, TOKminus);
        assert_eq!(token_text(&negative.next_token()), "12");
        assert_eq!(negative.next_token().kind, TOKeof);
        for text in [
            "1.5362",
            "0.875",
            "5.56e-2",
            "1.234E10",
            "123456789.012345678",
            "99999999999999999",
        ] {
            let mut lexer = lexer(text);
            let token = lexer.next_token();
            assert_eq!(token.kind, TOKnumber);
            assert_eq!(token_text(&token), text);
            if text == "99999999999999999" {
                assert!(lexer.is_complete());
            }
        }
    }
    #[test]
    fn Strings() {
        let text = "\"The cat jumped over the fence.\"";
        let mut first = lexer(text);
        let token = first.next_token();
        assert_eq!(token.kind, TOKstring);
        assert_eq!(token_text(&token), text);
        assert_eq!(first.next_token().kind, TOKeof);
        for text in [
            "\"\"",
            "\"The message reads: \"\"Warning: Insufficient Memory\"\"\"",
            "\"\\u0047\\u006f\\u0066\\u0069\\u0073\\u0068\\u0021\\u000d\\u000a\"",
        ] {
            let mut lexer = lexer(text);
            let token = lexer.next_token();
            assert_eq!(token.kind, TOKstring);
            assert_eq!(token_text(&token), text);
            if text.contains("\\u0047") {
                assert!(lexer.is_complete());
            }
        }
    }
    #[test]
    fn OperatorsAndKeywords() {
        let cases = [
            ("+", TOKplus),
            ("/", TOKdiv),
            ("-", TOKminus),
            ("&", TOKand),
            ("|", TOKor),
            ("*", TOKmul),
            ("<", TOKlt),
            (">", TOKgt),
            ("==", TOKeq),
            ("<>", TOKne),
            ("<=", TOKle),
            (">=", TOKge),
            ("and", TOKksand),
            ("break", TOKbreak),
            ("continue", TOKcontinue),
            ("do", TOKdo),
            ("downto", TOKdownto),
            ("else", TOKelse),
            ("elseif", TOKelseif),
            ("end", TOKend),
            ("endfor", TOKendfor),
            ("endfunc", TOKendfunc),
            ("endif", TOKendif),
            ("endwhile", TOKendwhile),
            ("eq", TOKkseq),
            ("exit", TOKexit),
            ("for", TOKfor),
            ("foreach", TOKforeach),
            ("func", TOKfunc),
            ("ge", TOKksge),
            ("gt", TOKksgt),
            ("if", TOKif),
            ("in", TOKin),
            ("infinity", TOKinfinity),
            ("le", TOKksle),
            ("lt", TOKkslt),
            ("nan", TOKnan),
            ("ne", TOKksne),
            ("not", TOKksnot),
            ("null", TOKnull),
            ("or", TOKksor),
            ("return", TOKreturn),
            ("step", TOKstep),
            ("then", TOKthen),
            ("throw", TOKthrow),
            ("upto", TOKupto),
            ("var", TOKvar),
            ("while", TOKwhile),
            ("(", TOKlparen),
            (")", TOKrparen),
            (",", TOKcomma),
            (".", TOKdot),
            ("[", TOKlbracket),
            ("]", TOKrbracket),
            ("..", TOKdotdot),
            (".#", TOKdotscream),
            (".*", TOKdotstar),
        ];
        for (text, expected) in cases {
            let mut lexer = lexer(text);
            assert_eq!(lexer.next_token().kind, expected, "{text}");
            assert!(lexer.is_complete(), "{text}");
        }
    }
    #[test]
    fn Comments() {
        for text in ["// Empty.", "//", ";", "; Empty."] {
            assert_eq!(lexer(text).next_token().kind, TOKeof);
        }
        for text in ["123 // Empty.\n\"str\"", "123 ;Empty.\n\"str\""] {
            let mut lexer = lexer(text);
            let number = lexer.next_token();
            assert_eq!(number.kind, TOKnumber);
            assert_eq!(token_text(&number), "123");
            let string = lexer.next_token();
            assert_eq!(string.kind, TOKstring);
            assert_eq!(token_text(&string), "\"str\"");
            assert_eq!(lexer.next_token().kind, TOKeof);
            if text.contains(';') {
                assert!(lexer.is_complete());
            }
        }
    }
    #[test]
    fn ValidIdentifiers() {
        for text in [
            "a",
            "an_identifier",
            "_ident",
            "$ident",
            "!ident",
            "GetAddr",
        ] {
            let mut lexer = lexer(text);
            let token = lexer.next_token();
            assert_eq!(token.kind, TOKidentifier);
            assert_eq!(token_text(&token), text);
            assert!(lexer.is_complete());
        }
    }
    #[test]
    fn InvalidIdentifiers() {
        for text in ["#a", "1a"] {
            assert_eq!(lexer(text).next_token().kind, TOKreserver);
        }
        let mut middle = lexer("an@identifier");
        assert_ne!(middle.next_token().kind, TOKreserver);
        assert_eq!(middle.next_token().kind, TOKreserver);
        assert_eq!(middle.next_token().kind, TOKreserver);
        let mut trailing = lexer("_ident@");
        assert_ne!(trailing.next_token().kind, TOKreserver);
        assert_eq!(trailing.next_token().kind, TOKreserver);
        assert!(!trailing.is_complete());
    }
    #[test]
    fn Whitespace() {
        assert_eq!(lexer(" \t\x0c\x09\x0b").next_token().kind, TOKeof);
        let mut lexer = lexer("123 \t\x0c\x09\x0b 456");
        let first = lexer.next_token();
        assert_eq!(first.kind, TOKnumber);
        assert_eq!(token_text(&first), "123");
        let second = lexer.next_token();
        assert_eq!(second.kind, TOKnumber);
        assert_eq!(token_text(&second), "456");
        assert_eq!(lexer.next_token().kind, TOKeof);
        assert!(lexer.is_complete());
    }
    #[test]
    fn NullData() {
        let mut lexer = CxfaFmLexer::new(vec![0x2d, 0x32, 0, 0x2d, 0x32]);
        assert_eq!(lexer.next_token().kind, TOKminus);
        let number = lexer.next_token();
        assert_eq!(number.kind, TOKnumber);
        assert_eq!(token_text(&number), "2");
        assert_eq!(lexer.next_token().kind, TOKeof);
        assert!(!lexer.is_complete());
    }
    #[test]
    fn source_spans_survive_lexer_drop_and_preserve_unicode_identifier_categories() {
        let token = {
            let mut lexer = lexer("漢字١");
            lexer.next_token()
        };
        assert_eq!(token.kind, TOKidentifier);
        assert_eq!(token_text(&token), "漢字١");
        // ICU u_isalpha is General_Category L*, not Unicode Alphabetic: a
        // combining Hebrew point is not an initial identifier character.
        assert_eq!(lexer("\u{05b0}").next_token().kind, TOKreserver);
        // Nondecimal numeric characters do not belong to u_isalnum either.
        let mut numeric = lexer("a²");
        assert_eq!(token_text(&numeric.next_token()), "a");
        assert_eq!(numeric.next_token().kind, TOKreserver);
    }
    #[test]
    fn malformed_numbers_strings_controls_and_case_sensitive_keywords() {
        for text in [
            "1e",
            "1e+",
            "0x10",
            "1α",
            "\"unterminated",
            "\"a\"\u{1}",
            "//bad\u{1}",
            ";bad\u{1}",
        ] {
            let mut lexer = lexer(text);
            assert_eq!(lexer.next_token().kind, TOKreserver, "{text:?}");
            assert_eq!(lexer.next_token().kind, TOKreserver);
            assert!(lexer.has_error());
        }
        for text in [".5", "1.", "1.e2", "0.1E+3"] {
            let token = lexer(text).next_token();
            assert_eq!(token.kind, TOKnumber);
            assert_eq!(token_text(&token), text);
        }
        assert_eq!(lexer("While").next_token().kind, TOKidentifier);
        assert_eq!(lexer("this").next_token().kind, TOKidentifier);
    }
}
