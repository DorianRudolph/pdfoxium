// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/parser/cpdf_simple_parser.h` and `.cpp`.

use super::fpdf_parser_utility::{
    pdf_char_is_delimiter, pdf_char_is_line_ending, pdf_char_is_whitespace,
};

pub struct CpdfSimpleParser<'a> {
    data: &'a [u8],
    cur_position: u32,
}

impl<'a> CpdfSimpleParser<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            data: input,
            cur_position: 0,
        }
    }

    pub fn get_word(&mut self) -> &'a [u8] {
        let Some(start_char) = self.skip_spaces_and_comments() else {
            return &[];
        };

        assert!(self.cur_position > 0);
        let start_position = self.cur_position - 1;
        assert!((start_position as usize) < self.data.len());

        if !pdf_char_is_delimiter(start_char) {
            return self.handle_non_delimiter();
        }

        match start_char {
            b'/' => self.handle_name(),
            b'<' => self.handle_begin_angle_bracket(),
            b'>' => self.handle_end_angle_bracket(),
            b'(' => self.handle_parentheses(),
            _ => self.data_to_current_position(start_position),
        }
    }

    pub fn set_current_position(&mut self, position: u32) {
        self.cur_position = position;
    }

    pub fn current_position(&self) -> u32 {
        self.cur_position
    }

    fn data_to_current_position(&self, start_position: u32) -> &'a [u8] {
        &self.data[start_position as usize..self.cur_position as usize]
    }

    fn skip_spaces_and_comments(&mut self) -> Option<u8> {
        loop {
            if self.cur_position as usize >= self.data.len() {
                return None;
            }

            let mut cur_char = self.data[self.cur_position as usize];
            self.cur_position = self.cur_position.wrapping_add(1);
            while pdf_char_is_whitespace(cur_char) {
                if self.cur_position as usize >= self.data.len() {
                    return None;
                }
                cur_char = self.data[self.cur_position as usize];
                self.cur_position = self.cur_position.wrapping_add(1);
            }

            if cur_char != b'%' {
                return Some(cur_char);
            }

            loop {
                if self.cur_position as usize >= self.data.len() {
                    return None;
                }
                cur_char = self.data[self.cur_position as usize];
                self.cur_position = self.cur_position.wrapping_add(1);
                if pdf_char_is_line_ending(cur_char) {
                    break;
                }
            }
        }
    }

    fn handle_name(&mut self) -> &'a [u8] {
        let start_position = self.cur_position - 1;
        while (self.cur_position as usize) < self.data.len() {
            let cur_char = self.data[self.cur_position as usize];
            if pdf_char_is_whitespace(cur_char) || pdf_char_is_delimiter(cur_char) {
                return self.data_to_current_position(start_position);
            }
            self.cur_position = self.cur_position.wrapping_add(1);
        }
        &[]
    }

    fn handle_begin_angle_bracket(&mut self) -> &'a [u8] {
        let start_position = self.cur_position - 1;
        if self.cur_position as usize >= self.data.len() {
            return self.data_to_current_position(start_position);
        }

        let mut cur_char = self.data[self.cur_position as usize];
        self.cur_position = self.cur_position.wrapping_add(1);
        if cur_char == b'<' {
            return self.data_to_current_position(start_position);
        }

        while (self.cur_position as usize) < self.data.len() && cur_char != b'>' {
            cur_char = self.data[self.cur_position as usize];
            self.cur_position = self.cur_position.wrapping_add(1);
        }
        self.data_to_current_position(start_position)
    }

    fn handle_end_angle_bracket(&mut self) -> &'a [u8] {
        let start_position = self.cur_position - 1;
        if (self.cur_position as usize) < self.data.len()
            && self.data[self.cur_position as usize] == b'>'
        {
            self.cur_position = self.cur_position.wrapping_add(1);
        }
        self.data_to_current_position(start_position)
    }

    fn handle_parentheses(&mut self) -> &'a [u8] {
        let start_position = self.cur_position - 1;
        let mut level = 1;
        while (self.cur_position as usize) < self.data.len() && level > 0 {
            let cur_char = self.data[self.cur_position as usize];
            self.cur_position = self.cur_position.wrapping_add(1);
            if cur_char == b'(' {
                level += 1;
            } else if cur_char == b')' {
                level -= 1;
            }
        }
        self.data_to_current_position(start_position)
    }

    fn handle_non_delimiter(&mut self) -> &'a [u8] {
        let start_position = self.cur_position - 1;
        while (self.cur_position as usize) < self.data.len() {
            let cur_char = self.data[self.cur_position as usize];
            if pdf_char_is_delimiter(cur_char) || pdf_char_is_whitespace(cur_char) {
                break;
            }
            self.cur_position = self.cur_position.wrapping_add(1);
        }
        self.data_to_current_position(start_position)
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfSimpleParser;

    #[test]
    fn token_boundaries_include_pdfium_edge_cases() {
        for (input, expected) in [
            (&b"/"[..], &b""[..]),
            (b"/99", b""),
            (b"/99}", b"/99"),
            (b"(It is a (long) day)!", b"(It is a (long) day)"),
            (b"<4545a<ed>ertt", b"<4545a<ed>"),
            (b"<< /abc>>", b"<<"),
            (b"%comment\r\nname", b"name"),
        ] {
            let mut parser = CpdfSimpleParser::new(input);
            assert_eq!(parser.get_word(), expected, "{input:?}");
        }
    }

    #[test]
    fn position_and_comment_eof_match_pdfium() {
        let mut parser = CpdfSimpleParser::new(b"1 % unfinished");
        assert_eq!(parser.get_word(), b"1");
        assert_eq!(parser.current_position(), 1);
        assert_eq!(parser.get_word(), b"");
        assert_eq!(parser.current_position(), 14);
        parser.set_current_position(0);
        assert_eq!(parser.get_word(), b"1");
    }
}

#[cfg(test)]
#[path = "cpdf_simple_parser_unittest.rs"]
mod upstream_tests;
