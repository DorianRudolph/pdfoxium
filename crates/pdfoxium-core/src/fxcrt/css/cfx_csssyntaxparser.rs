// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Rule/declaration scanner from `core/fxcrt/css/cfx_csssyntaxparser.{h,cpp}`.
use super::cfx_cssinputtextbuf::CfxCssInputTextBuf;
use super::cfx_cssoutputtextbuf::CfxCssOutputTextBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Error,
    EOS,
    None,
    StyleRule,
    Selector,
    DeclOpen,
    DeclClose,
    PropertyName,
    PropertyValue,
}
pub type CfxCssSyntaxStatus = Status;
#[derive(Clone, Copy, Debug)]
enum Mode {
    RuleSet,
    Comment,
    Selector,
    PropertyName,
    PropertyValue,
}
#[derive(Debug)]
pub struct CfxCssSyntaxParser<'a> {
    has_error: bool,
    mode: Mode,
    output: CfxCssOutputTextBuf,
    input: CfxCssInputTextBuf<'a>,
    mode_stack: Vec<Mode>,
}
impl<'a> CfxCssSyntaxParser<'a> {
    pub fn new(input: &'a [u32]) -> Self {
        Self {
            has_error: false,
            mode: Mode::RuleSet,
            output: CfxCssOutputTextBuf::new(),
            input: CfxCssInputTextBuf::new(input),
            mode_stack: Vec::new(),
        }
    }
    pub fn set_parse_only_declarations(&mut self) {
        self.mode = Mode::PropertyName;
    }
    pub fn get_current_string(&self) -> &[u32] {
        self.output.get_trailing_blank_trimmed_string()
    }
    fn save_mode(&mut self, mode: Mode) {
        self.mode_stack.push(mode);
    }
    fn restore_mode(&mut self) -> bool {
        if let Some(mode) = self.mode_stack.pop() {
            self.mode = mode;
            true
        } else {
            self.has_error = true;
            false
        }
    }
    fn begin_comment(&mut self, mode: Mode) -> bool {
        if self.input.get_char() == u32::from(b'/') && self.input.get_next_char() == u32::from(b'*')
        {
            self.save_mode(mode);
            self.mode = Mode::Comment;
            true
        } else {
            false
        }
    }
    pub fn do_syntax_parse(&mut self) -> Status {
        self.output.clear();
        if self.has_error {
            return Status::Error;
        }
        while !self.input.is_eof() {
            let character = self.input.get_char();
            match self.mode {
                Mode::RuleSet => {
                    if character == u32::from(b'}') {
                        self.has_error = true;
                        return Status::Error;
                    }
                    if self.begin_comment(Mode::RuleSet) {
                        continue;
                    }
                    if character <= u32::from(b' ') {
                        self.input.move_next();
                    } else if matches!(character, 0x2e | 0x23 | 0x2a | 0x41..=0x5a | 0x61..=0x7a) {
                        self.mode = Mode::Selector;
                        return Status::StyleRule;
                    } else {
                        self.has_error = true;
                        return Status::Error;
                    }
                }
                Mode::Selector => {
                    if character == u32::from(b',') {
                        self.input.move_next();
                        if !self.output.is_empty() {
                            return Status::Selector;
                        }
                    } else if character == u32::from(b'{') {
                        if !self.output.is_empty() {
                            return Status::Selector;
                        }
                        self.input.move_next();
                        self.save_mode(Mode::RuleSet);
                        self.mode = Mode::PropertyName;
                        return Status::DeclOpen;
                    } else if self.begin_comment(Mode::Selector) {
                        if !self.output.is_empty() {
                            return Status::Selector;
                        }
                    } else {
                        self.output.append_char_if_not_leading_blank(character);
                        self.input.move_next();
                    }
                }
                Mode::PropertyName => {
                    if character == u32::from(b':') {
                        self.input.move_next();
                        self.mode = Mode::PropertyValue;
                        return Status::PropertyName;
                    }
                    if character == u32::from(b'}') {
                        self.input.move_next();
                        return if self.restore_mode() {
                            Status::DeclClose
                        } else {
                            Status::Error
                        };
                    }
                    if self.begin_comment(Mode::PropertyName) {
                        if !self.output.is_empty() {
                            return Status::PropertyName;
                        }
                    } else {
                        self.output.append_char_if_not_leading_blank(character);
                        self.input.move_next();
                    }
                }
                Mode::PropertyValue => {
                    if character == u32::from(b';') || character == u32::from(b'}') {
                        if character == u32::from(b';') {
                            self.input.move_next();
                        }
                        self.mode = Mode::PropertyName;
                        return Status::PropertyValue;
                    }
                    if self.begin_comment(Mode::PropertyValue) {
                        if !self.output.is_empty() {
                            return Status::PropertyValue;
                        }
                    } else {
                        self.output.append_char_if_not_leading_blank(character);
                        self.input.move_next();
                    }
                }
                Mode::Comment => {
                    if character == u32::from(b'*') && self.input.get_next_char() == u32::from(b'/')
                    {
                        if !self.restore_mode() {
                            return Status::Error;
                        }
                        self.input.move_next();
                    }
                    self.input.move_next();
                }
            }
        }
        if matches!(self.mode, Mode::PropertyValue) && !self.output.is_empty() {
            Status::PropertyValue
        } else {
            Status::EOS
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn inline(input: &str, expected: &[(Status, Option<&str>)]) {
        let input = wide(input);
        let mut parser = CfxCssSyntaxParser::new(&input);
        parser.set_parse_only_declarations();
        for &(status, text) in expected {
            assert_eq!(parser.do_syntax_parse(), status);
            if let Some(text) = text {
                assert_eq!(parser.get_current_string(), wide(text));
            }
        }
    }
    // CSSSyntaxParserTest.ParseEmpty: all source status/string assertions.
    #[test]
    fn ParseEmpty() {
        inline("", &[(Status::EOS, None), (Status::EOS, None)]);
    }
    // CSSSyntaxParserTest.ParseBlank: all source status/string assertions.
    #[test]
    fn ParseBlank() {
        inline("  \n\r\t", &[(Status::EOS, None), (Status::EOS, None)]);
    }
    // CSSSyntaxParserTest.ParseMissingColon: all source status/string assertions.
    #[test]
    fn ParseMissingColon() {
        inline("foo ", &[(Status::EOS, None), (Status::EOS, None)]);
    }
    // CSSSyntaxParserTest.ParseMissingValue: all source status/string assertions.
    #[test]
    fn ParseMissingValue() {
        inline(
            "foo: ",
            &[(Status::PropertyName, Some("foo")), (Status::EOS, None)],
        );
    }
    // CSSSyntaxParserTest.ParseSingleProp1: all source status/string assertions.
    #[test]
    fn ParseSingleProp1() {
        inline(
            "foo:bar",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar")),
                (Status::EOS, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseSingleProp2: all source status/string assertions.
    #[test]
    fn ParseSingleProp2() {
        inline(
            "foo:bar;",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar")),
                (Status::EOS, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseMissingColonMultiple: all source status/string assertions.
    #[test]
    fn ParseMissingColonMultiple() {
        inline(
            "foo:bar; baz",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar")),
                (Status::EOS, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseMissingPropertyMultiple: all source status/string assertions.
    #[test]
    fn ParseMissingPropertyMultiple() {
        inline(
            "foo:bar; baz: ",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar")),
                (Status::PropertyName, Some("baz")),
                (Status::EOS, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseMultipleProp1: all source status/string assertions.
    #[test]
    fn ParseMultipleProp1() {
        inline(
            "foo : bar; baz : bam",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar")),
                (Status::PropertyName, Some("baz")),
                (Status::PropertyValue, Some("bam")),
                (Status::EOS, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseMultipleProp2: all source status/string assertions.
    #[test]
    fn ParseMultipleProp2() {
        inline(
            "foo:bar;baz:bam;",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar")),
                (Status::PropertyName, Some("baz")),
                (Status::PropertyValue, Some("bam")),
                (Status::EOS, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseOpenBrace1: all source status/string assertions.
    #[test]
    fn ParseOpenBrace1() {
        inline(
            "{a:3}",
            &[
                (Status::PropertyName, Some("{a")),
                (Status::PropertyValue, Some("3")),
                (Status::Error, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseOpenBrace2: all source status/string assertions.
    #[test]
    fn ParseOpenBrace2() {
        inline(
            "foo {a:3}",
            &[
                (Status::PropertyName, Some("foo {a")),
                (Status::PropertyValue, Some("3")),
                (Status::Error, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseOpenBrace3: all source status/string assertions.
    #[test]
    fn ParseOpenBrace3() {
        inline(
            "foo: bar {a:3}",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar {a:3")),
                (Status::Error, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseOpenBrace4: all source status/string assertions.
    #[test]
    fn ParseOpenBrace4() {
        inline(
            "foo: bar; {a:3}",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar")),
                (Status::PropertyName, Some("{a")),
                (Status::PropertyValue, Some("3")),
                (Status::Error, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseCloseBrace1: all source status/string assertions.
    #[test]
    fn ParseCloseBrace1() {
        inline("} foo:bar", &[(Status::Error, None)]);
    }
    // CSSSyntaxParserTest.ParseCloseBrace2: all source status/string assertions.
    #[test]
    fn ParseCloseBrace2() {
        inline("foo}:bar", &[(Status::Error, None)]);
    }
    // CSSSyntaxParserTest.ParseCloseBrace3: all source status/string assertions.
    #[test]
    fn ParseCloseBrace3() {
        inline(
            "foo:bar}",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar")),
                (Status::Error, None),
            ],
        );
    }
    // CSSSyntaxParserTest.ParseCloseBrace4: all source status/string assertions.
    #[test]
    fn ParseCloseBrace4() {
        inline(
            "foo:bar;}",
            &[
                (Status::PropertyName, Some("foo")),
                (Status::PropertyValue, Some("bar")),
                (Status::Error, None),
            ],
        );
    }
    #[test]
    fn full_rule_comments_tokens_and_sticky_errors() {
        let input = wide("/* lead */ p, b { color: red; margin: 1pt } /* tail */");
        let mut parser = CfxCssSyntaxParser::new(&input);
        for (status, text) in [
            (Status::StyleRule, ""),
            (Status::Selector, "p"),
            (Status::Selector, "b"),
            (Status::DeclOpen, ""),
            (Status::PropertyName, "color"),
            (Status::PropertyValue, "red"),
            (Status::PropertyName, "margin"),
            (Status::PropertyValue, "1pt"),
            (Status::DeclClose, ""),
            (Status::EOS, ""),
            (Status::EOS, ""),
        ] {
            assert_eq!(parser.do_syntax_parse(), status);
            assert_eq!(parser.get_current_string(), wide(text));
        }
        for input in ["}", "@font-face{}", "1invalid{}"] {
            let input = wide(input);
            let mut parser = CfxCssSyntaxParser::new(&input);
            assert_eq!(parser.do_syntax_parse(), Status::Error);
            assert_eq!(parser.do_syntax_parse(), Status::Error);
        }
        let input = wide("/* unfinished");
        let mut parser = CfxCssSyntaxParser::new(&input);
        assert_eq!(parser.do_syntax_parse(), Status::EOS);
    }
    #[test]
    fn token_buffers_trim_only_source_blanks_and_handle_eof() {
        let input = [1, 0, 0x10000];
        let mut buffer = CfxCssInputTextBuf::new(&input);
        assert!(!buffer.is_eof());
        assert_eq!(buffer.get_char(), 1);
        assert_eq!(buffer.get_next_char(), 0);
        buffer.move_next();
        assert_eq!(buffer.get_char(), 0);
        assert_eq!(buffer.get_next_char(), 0x10000);
        buffer.move_next();
        assert_eq!(buffer.get_char(), 0x10000);
        assert_eq!(buffer.get_next_char(), 0);
        buffer.move_next();
        buffer.move_next();
        assert!(buffer.is_eof());
        assert_eq!(buffer.get_char(), 0);
        let mut output = CfxCssOutputTextBuf::new();
        for ch in wide(" \t a \r\n") {
            output.append_char_if_not_leading_blank(ch);
        }
        assert_eq!(output.get_trailing_blank_trimmed_string(), wide("a"));
        output.clear();
        assert!(output.is_empty());
        assert!(output.get_trailing_blank_trimmed_string().is_empty());
    }
}
