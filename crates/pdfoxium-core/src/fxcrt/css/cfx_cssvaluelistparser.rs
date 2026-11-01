// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Borrowed token scanning from `core/fxcrt/css/cfx_cssvaluelistparser.{h,cpp}`.
use super::cfx_cssvalue::CfxCssPrimitiveType;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Result<'a> {
    pub value_type: CfxCssPrimitiveType,
    pub string_view: &'a [u32],
}
#[derive(Debug)]
pub struct CfxCssValueListParser<'a> {
    current: &'a [u32],
    separator: u32,
}
impl<'a> CfxCssValueListParser<'a> {
    pub fn new(list: &'a [u32], separator: u32) -> Self {
        Self {
            current: list,
            separator,
        }
    }
    pub fn use_comma_separator(&mut self) {
        self.separator = u32::from(b',');
    }
    fn chars_remain(&self) -> bool {
        !self.current.is_empty()
    }
    fn current_char(&self) -> u32 {
        self.current.first().copied().unwrap_or(0)
    }
    fn advance(&mut self) {
        self.current = self.current.get(1..).unwrap_or_default();
    }
    fn skip_to_char(&mut self, character: u32) -> usize {
        let count = self
            .current
            .iter()
            .position(|&value| value == character)
            .unwrap_or(self.current.len());
        self.current = &self.current[count..];
        count
    }
    fn skip_to_char_matching_parens(&mut self, character: u32) -> usize {
        let mut length = 0;
        let mut bracket_count = 0i64;
        while self.chars_remain() && self.current_char() != character {
            let current = self.current_char();
            if current <= u32::from(b' ') {
                break;
            }
            if current == u32::from(b'(') {
                bracket_count += 1;
            } else if current == u32::from(b')') {
                bracket_count -= 1;
            }
            length += 1;
            self.advance();
        }
        while bracket_count > 0 && self.chars_remain() {
            if self.current_char() == u32::from(b')') {
                bracket_count -= 1;
            }
            length += 1;
            self.advance();
        }
        length
    }
    pub fn next_value(&mut self) -> Option<Result<'a>> {
        while self.chars_remain()
            && (self.current_char() <= u32::from(b' ') || self.current_char() == self.separator)
        {
            self.advance();
        }
        if !self.chars_remain() {
            return None;
        }
        let mut value_type = CfxCssPrimitiveType::Unknown;
        let mut start = self.current;
        let character = self.current_char();
        let length;
        if character == u32::from(b'#') {
            length = self.skip_to_char(u32::from(b' '));
            if length == 4 || length == 7 {
                value_type = CfxCssPrimitiveType::RGB;
            }
        } else if matches!(character, 0x30..=0x39 | 0x2e | 0x2d | 0x2b) {
            let mut count = 0;
            while self.chars_remain()
                && self.current_char() > u32::from(b' ')
                && self.current_char() != self.separator
            {
                count += 1;
                self.advance();
            }
            length = count;
            value_type = CfxCssPrimitiveType::Number;
        } else if character == u32::from(b'"') || character == u32::from(b'\'') {
            start = &start[1..];
            self.advance();
            length = self.skip_to_char(character);
            self.advance();
            value_type = CfxCssPrimitiveType::String;
        } else if self.current.get(..4).is_some_and(|prefix| {
            prefix
                .iter()
                .zip(b"rgb(")
                .all(|(&a, &b)| a <= 127 && (a as u8).eq_ignore_ascii_case(&b))
        }) {
            length = self.skip_to_char(u32::from(b')')) + 1;
            self.advance();
            value_type = CfxCssPrimitiveType::RGB;
        } else {
            length = self.skip_to_char_matching_parens(self.separator);
            value_type = CfxCssPrimitiveType::String;
        }
        if length == 0 {
            None
        } else {
            // WideStringView::First() clips its requested size too; an
            // unterminated rgb(...) token therefore cannot overread input.
            Some(Result {
                value_type,
                string_view: &start[..length.min(start.len())],
            })
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
    fn next(parser: &mut CfxCssValueListParser<'_>, value_type: CfxCssPrimitiveType, text: &str) {
        let result = parser.next_value().expect("source expects a token");
        assert_eq!(result.value_type, value_type);
        assert_eq!(result.string_view, wide(text));
    }
    // CFXCSSValueListParserTest.RGBShort: all four source inputs/assertions.
    #[test]
    fn RGBShort() {
        for (input, value_type, check_end) in [
            ("#abc", CfxCssPrimitiveType::RGB, true),
            ("#abcdef", CfxCssPrimitiveType::RGB, false),
            ("rgb(1, 255, 4)", CfxCssPrimitiveType::RGB, false),
            ("#abcdefghij", CfxCssPrimitiveType::Unknown, true),
        ] {
            let input = wide(input);
            let mut parser = CfxCssValueListParser::new(&input, u32::from(b' '));
            let result = parser.next_value().expect("source expects a token");
            assert_eq!(result.value_type, value_type);
            assert_eq!(result.string_view, input);
            if check_end {
                assert!(parser.next_value().is_none());
            }
        }
    }
    // NumberParsing classifies a token; ParseCSSNumber validates its contents.
    #[test]
    fn NumberParsing() {
        for input in [
            "1234",
            "-1234",
            "+1234",
            ".1234",
            "4321.1234",
            "4321.12.34",
            "43a1.12.34",
        ] {
            let input = wide(input);
            let mut parser = CfxCssValueListParser::new(&input, u32::from(b' '));
            let result = parser.next_value().expect("source expects a token");
            assert_eq!(result.value_type, CfxCssPrimitiveType::Number);
            assert_eq!(result.string_view, input);
        }
    }
    #[test]
    fn StringParsing() {
        for (input, token) in [
            ("'string'", "string"),
            ("\"another string\"", "another string"),
            ("standalone", "standalone"),
        ] {
            let input = wide(input);
            let mut parser = CfxCssValueListParser::new(&input, u32::from(b' '));
            next(&mut parser, CfxCssPrimitiveType::String, token);
        }
    }
    #[test]
    fn MultiParsing() {
        let input = wide("1, 2, 3");
        let mut parser = CfxCssValueListParser::new(&input, u32::from(b','));
        for token in ["1", "2", "3"] {
            next(&mut parser, CfxCssPrimitiveType::Number, token);
        }
        assert!(parser.next_value().is_none());
        let input = wide("'str', rgb(1, 2, 3), 4");
        let mut parser = CfxCssValueListParser::new(&input, u32::from(b','));
        next(&mut parser, CfxCssPrimitiveType::String, "str");
        next(&mut parser, CfxCssPrimitiveType::RGB, "rgb(1, 2, 3)");
        next(&mut parser, CfxCssPrimitiveType::Number, "4");
    }
    #[test]
    fn clipped_unterminated_tokens_functions_separators_and_empty_input() {
        for (input, token, value_type) in [
            ("rgb(1", "rgb(1", CfxCssPrimitiveType::RGB),
            ("'unfinished", "unfinished", CfxCssPrimitiveType::String),
            ("f(a, b)", "f(a, b)", CfxCssPrimitiveType::String),
            ("RGB(1,2,3)", "RGB(1,2,3)", CfxCssPrimitiveType::RGB),
        ] {
            let input = wide(input);
            let mut parser = CfxCssValueListParser::new(&input, u32::from(b' '));
            next(&mut parser, value_type, token);
            assert!(parser.next_value().is_none());
            assert!(parser.next_value().is_none());
        }
        let input = wide("A,B");
        let mut parser = CfxCssValueListParser::new(&input, u32::from(b' '));
        parser.use_comma_separator();
        next(&mut parser, CfxCssPrimitiveType::String, "A");
        next(&mut parser, CfxCssPrimitiveType::String, "B");
        assert!(parser.next_value().is_none());
        for input in ["", " ", ",,,", "''"] {
            let input = wide(input);
            let mut parser = CfxCssValueListParser::new(&input, u32::from(b','));
            assert!(parser.next_value().is_none());
        }
    }
}
