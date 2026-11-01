// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct translation of `core/fxcrt/xml/cfx_xmlparser.{h,cpp}` for wchar32.
//! The fixed decoding plane, permissive EOF behavior and unsigned entity
//! arithmetic follow the source. Delimiter lookahead refills across planes so
//! valid comments and CDATA do not depend on their offset in the input.

use std::rc::Rc;

use super::cfx_xmldocument::CfxXmlDocument;
use super::cfx_xmlnode::{XmlNodeRef, XmlNodeType};
use crate::fxcrt::cfx_seekablestreamproxy::CfxSeekableStreamProxy;
use crate::fxcrt::fx_codepage::FxCodePage;
use crate::fxcrt::fx_extension::is_wide_space;
use crate::fxcrt::fx_stream::SeekableReadStream;

const CURRENT_TEXT_RESERVE: usize = 128;
const MAX_CHAR_RANGE: u32 = 0x10ffff;

fn is_xml_white_space(ch: u32) -> bool {
    matches!(ch, 0x20 | 0x0a | 0x0d | 0x09)
}

// The source table describes uint16_t ranges and whether the range may begin
// an XML name. In particular ':' is accepted only after the first character.
const XML_NAME_CHARS: [(u32, u32, bool); 20] = [
    (0x2d, 0x2e, false),
    (0x30, 0x39, false),
    (0x3a, 0x3a, false),
    (0x41, 0x5a, true),
    (0x5f, 0x5f, true),
    (0x61, 0x7a, true),
    (0xb7, 0xb7, false),
    (0xc0, 0xd6, true),
    (0xd8, 0xf6, true),
    (0xf8, 0x02ff, true),
    (0x0300, 0x036f, false),
    (0x0370, 0x037d, true),
    (0x037f, 0x1fff, true),
    (0x200c, 0x200d, true),
    (0x203f, 0x2040, false),
    (0x2070, 0x218f, true),
    (0x2c00, 0x2fef, true),
    (0x3001, 0xd7ff, true),
    (0xf900, 0xfdcf, true),
    (0xfdf0, 0xfffd, true),
];

fn equals_ascii(value: &[u32], ascii: &[u8]) -> bool {
    value.len() == ascii.len() && value.iter().zip(ascii).all(|(&a, &b)| a == u32::from(b))
}

fn starts_ascii(value: &[u32], ascii: &[u8]) -> bool {
    value.len() >= ascii.len() && equals_ascii(&value[..ascii.len()], ascii)
}

fn starts_ascii_no_case(value: &[u32], ascii: &[u8]) -> bool {
    value.len() >= ascii.len()
        && value
            .iter()
            .zip(ascii)
            .all(|(&a, &b)| a <= 127 && (a as u8).to_ascii_lowercase() == b.to_ascii_lowercase())
}

#[derive(Clone, Copy)]
enum XmlSyntaxState {
    Text,
    Node,
    Target,
    Tag,
    AttriName,
    AttriEqualSign,
    AttriQuotation,
    AttriValue,
    CloseInstruction,
    BreakElement,
    CloseElement,
    SkipDeclNode,
    SkipComment,
    SkipCommentOrDecl,
    SkipCData,
    TargetData,
}

pub struct CfxXmlParser<'a> {
    current_node: Option<XmlNodeRef>,
    stream: CfxSeekableStreamProxy<'a>,
    current_text: Vec<u32>,
    xml_plane_size: usize,
    entity_start: Option<usize>,
}

impl<'a> CfxXmlParser<'a> {
    pub fn is_xml_name_char(ch: u32, first_char: bool) -> bool {
        let index = XML_NAME_CHARS.partition_point(|&(_, end, _)| end < ch);
        XML_NAME_CHARS
            .get(index)
            .is_some_and(|&(start, _, may_start)| ch >= start && (!first_char || may_start))
    }

    pub fn new(stream: Rc<dyn SeekableReadStream + 'a>) -> Self {
        let mut proxy = CfxSeekableStreamProxy::new(stream);
        if !matches!(
            proxy.get_code_page(),
            FxCodePage::Utf16Le | FxCodePage::Utf16Be | FxCodePage::Utf8
        ) {
            proxy.set_code_page(FxCodePage::Utf8);
        }
        let xml_plane_size = 1024.min(usize::try_from(proxy.get_size()).expect("XML stream size"));
        Self {
            current_node: None,
            stream: proxy,
            current_text: Vec::with_capacity(CURRENT_TEXT_RESERVE),
            xml_plane_size,
            entity_start: None,
        }
    }

    pub fn parse(&mut self) -> Option<CfxXmlDocument> {
        let mut document = CfxXmlDocument::new();
        let previous_node = self.current_node.replace(document.root());
        let success = self.do_syntax_parse(&mut document);
        self.current_node = previous_node;
        success.then_some(document)
    }

    fn node(&self) -> &XmlNodeRef {
        self.current_node.as_ref().expect("XML parser current node")
    }

    fn do_syntax_parse(&mut self, document: &mut CfxXmlDocument) -> bool {
        use XmlSyntaxState::*;
        if self.xml_plane_size == 0 {
            return false;
        }
        let Some(alloc_size) = self.xml_plane_size.checked_add(1) else {
            return false;
        };
        let mut current_buffer_idx = 0usize;
        let mut buffer_size = 0usize;
        let mut buffer = vec![0u32; alloc_size];
        let mut character_to_skip_to_stack = Vec::new();
        let mut node_type_stack = Vec::new();
        let mut current_attribute_name = Vec::new();
        let mut state = Text;
        let mut current_quote_character = 0;
        let mut current_character_to_skip_to = 0;

        loop {
            if current_buffer_idx >= buffer_size {
                if self.stream.is_eof() {
                    return true;
                }
                let buffer_chars = self.stream.read_block(&mut buffer[..self.xml_plane_size]);
                if buffer_chars == 0 {
                    return true;
                }
                current_buffer_idx = 0;
                buffer_size = buffer_chars;
            }

            while current_buffer_idx < buffer_size {
                let lookahead = match state {
                    SkipCommentOrDecl => 7,
                    SkipCData | SkipComment => 3,
                    _ => 1,
                };
                let remaining = buffer_size - current_buffer_idx;
                if remaining < lookahead && !self.stream.is_eof() {
                    // Native looks into the stale allocated tail and misses
                    // delimiters split over its decoding plane. Keep the same
                    // state machine, retaining the pending prefix while filling
                    // its lookahead with actual decoded characters instead.
                    buffer.copy_within(current_buffer_idx..buffer_size, 0);
                    let read = self
                        .stream
                        .read_block(&mut buffer[remaining..self.xml_plane_size]);
                    current_buffer_idx = 0;
                    buffer_size = remaining + read;
                }
                let ch = buffer[current_buffer_idx];
                match state {
                    Text => {
                        if ch == u32::from(b'<') {
                            if !self.current_text.is_empty() {
                                let data = self.get_text_data();
                                self.node()
                                    .append_last_child(&document.create_text_owned(data));
                            } else {
                                current_buffer_idx += 1;
                                state = Node;
                            }
                        } else {
                            // Fail on text outside the root element; ignore
                            // native ICU whitespace/null for this check.
                            if node_type_stack.is_empty() && ch != 0 && !is_wide_space(ch) {
                                return false;
                            }
                            self.process_text_char(ch);
                            current_buffer_idx += 1;
                        }
                    }
                    Node => match ch {
                        0x21 => {
                            current_buffer_idx += 1;
                            state = SkipCommentOrDecl;
                        }
                        0x2f => {
                            current_buffer_idx += 1;
                            state = CloseElement;
                        }
                        0x3f => {
                            node_type_stack.push(XmlNodeType::Instruction);
                            current_buffer_idx += 1;
                            state = Target;
                        }
                        _ => {
                            node_type_stack.push(XmlNodeType::Element);
                            state = Tag;
                        }
                    },
                    Target => {
                        if !Self::is_xml_name_char(ch, self.current_text.is_empty()) {
                            if self.current_text.is_empty() {
                                return false;
                            }
                            state = TargetData;
                            let target_name = self.get_text_data();
                            if equals_ascii(&target_name, b"originalXFAVersion")
                                || equals_ascii(&target_name, b"acrobat")
                            {
                                let node = document.create_instruction_owned(target_name);
                                self.node().append_last_child(&node);
                                self.current_node = Some(node);
                            }
                        } else {
                            self.current_text.push(ch);
                            current_buffer_idx += 1;
                        }
                    }
                    Tag => {
                        if !Self::is_xml_name_char(ch, self.current_text.is_empty()) {
                            if self.current_text.is_empty() {
                                return false;
                            }
                            state = AttriName;
                            let data = self.get_text_data();
                            let child = document.create_element_owned(data);
                            self.node().append_last_child(&child);
                            self.current_node = Some(child);
                        } else {
                            self.current_text.push(ch);
                            current_buffer_idx += 1;
                        }
                    }
                    AttriName => {
                        if self.current_text.is_empty() && is_xml_white_space(ch) {
                            current_buffer_idx += 1;
                            continue;
                        }
                        if !Self::is_xml_name_char(ch, self.current_text.is_empty()) {
                            if self.current_text.is_empty() {
                                match node_type_stack.last().expect("XML node type") {
                                    XmlNodeType::Element if ch == 0x3e || ch == 0x2f => {
                                        state = BreakElement;
                                        continue;
                                    }
                                    XmlNodeType::Instruction => {
                                        if ch == 0x3f {
                                            state = CloseInstruction;
                                            current_buffer_idx += 1;
                                        } else {
                                            state = TargetData;
                                        }
                                        continue;
                                    }
                                    _ => return false,
                                }
                            } else {
                                if node_type_stack.last() == Some(&XmlNodeType::Instruction)
                                    && ch != 0x3d
                                    && !is_xml_white_space(ch)
                                {
                                    state = TargetData;
                                    continue;
                                }
                                state = AttriEqualSign;
                                current_attribute_name = self.get_text_data();
                            }
                        } else {
                            self.current_text.push(ch);
                            current_buffer_idx += 1;
                        }
                    }
                    AttriEqualSign => {
                        if is_xml_white_space(ch) {
                            current_buffer_idx += 1;
                            continue;
                        }
                        if ch != 0x3d {
                            if node_type_stack.last() == Some(&XmlNodeType::Instruction) {
                                state = TargetData;
                                continue;
                            }
                            return false;
                        }
                        state = AttriQuotation;
                        current_buffer_idx += 1;
                    }
                    AttriQuotation => {
                        if is_xml_white_space(ch) {
                            current_buffer_idx += 1;
                            continue;
                        }
                        if ch != 0x22 && ch != 0x27 {
                            return false;
                        }
                        current_quote_character = ch;
                        state = AttriValue;
                        current_buffer_idx += 1;
                    }
                    AttriValue => {
                        if ch == current_quote_character {
                            if self.entity_start.is_some() {
                                return false;
                            }
                            current_quote_character = 0;
                            current_buffer_idx += 1;
                            state = AttriName;
                            if self.node().node_type() == XmlNodeType::Element {
                                let value = self.get_text_data();
                                self.node().set_attribute_owned(
                                    std::mem::take(&mut current_attribute_name),
                                    value,
                                );
                            }
                            current_attribute_name.clear();
                        } else {
                            self.process_text_char(ch);
                            current_buffer_idx += 1;
                        }
                    }
                    CloseInstruction => {
                        if ch != 0x3e {
                            self.current_text.push(ch);
                            state = TargetData;
                        } else if !self.current_text.is_empty() {
                            self.process_target_data();
                        } else {
                            current_buffer_idx += 1;
                            if node_type_stack.pop().is_none() {
                                return false;
                            }
                            state = Text;
                            if self
                                .current_node
                                .as_ref()
                                .is_some_and(|node| node.node_type() == XmlNodeType::Instruction)
                            {
                                self.current_node = self.node().parent();
                            }
                        }
                    }
                    BreakElement => {
                        if ch == 0x3e {
                            state = Text;
                        } else if ch == 0x2f {
                            state = CloseElement;
                        } else {
                            return false;
                        }
                        current_buffer_idx += 1;
                    }
                    CloseElement => {
                        if !Self::is_xml_name_char(ch, self.current_text.is_empty()) {
                            if ch == 0x3e {
                                if node_type_stack.pop().is_none() {
                                    return false;
                                }
                                state = Text;
                                if self.node().node_type() != XmlNodeType::Element {
                                    return false;
                                }
                                let name = self.get_text_data();
                                if !name.is_empty()
                                    && name.as_slice() != self.node().name().as_ref()
                                {
                                    return false;
                                }
                                self.current_node = self.node().parent();
                            } else if !is_xml_white_space(ch) {
                                return false;
                            }
                        } else {
                            self.current_text.push(ch);
                        }
                        current_buffer_idx += 1;
                    }
                    SkipCommentOrDecl => {
                        let current_view = &buffer[current_buffer_idx..buffer_size];
                        if starts_ascii(current_view, b"--") {
                            current_buffer_idx += 2;
                            state = SkipComment;
                        } else if starts_ascii_no_case(current_view, b"[CDATA[") {
                            current_buffer_idx += 7;
                            state = SkipCData;
                        } else {
                            state = SkipDeclNode;
                            current_character_to_skip_to = u32::from(b'>');
                            character_to_skip_to_stack.push(u32::from(b'>'));
                        }
                    }
                    SkipCData => {
                        if starts_ascii(&buffer[current_buffer_idx..buffer_size], b"]]>") {
                            current_buffer_idx += 3;
                            state = Text;
                            let data = self.get_text_data();
                            self.node()
                                .append_last_child(&document.create_chardata_owned(data));
                        } else {
                            self.current_text.push(ch);
                            current_buffer_idx += 1;
                        }
                    }
                    SkipDeclNode => {
                        if current_character_to_skip_to == 0x27
                            || current_character_to_skip_to == 0x22
                        {
                            current_buffer_idx += 1;
                            if ch != current_character_to_skip_to {
                                continue;
                            }
                            character_to_skip_to_stack.pop();
                            if let Some(&next) = character_to_skip_to_stack.last() {
                                current_character_to_skip_to = next;
                            } else {
                                state = Text;
                            }
                        } else {
                            match ch {
                                0x3c | 0x5b | 0x28 | 0x27 | 0x22 => {
                                    current_character_to_skip_to = match ch {
                                        0x3c => 0x3e,
                                        0x5b => 0x5d,
                                        0x28 => 0x29,
                                        _ => ch,
                                    };
                                    character_to_skip_to_stack.push(current_character_to_skip_to);
                                }
                                _ if ch == current_character_to_skip_to => {
                                    character_to_skip_to_stack.pop();
                                    if let Some(&next) = character_to_skip_to_stack.last() {
                                        current_character_to_skip_to = next;
                                    } else {
                                        state = Text;
                                    }
                                }
                                _ => {}
                            }
                            current_buffer_idx += 1;
                        }
                    }
                    SkipComment => {
                        if starts_ascii(&buffer[current_buffer_idx..buffer_size], b"-->") {
                            current_buffer_idx += 2;
                            state = Text;
                        }
                        current_buffer_idx += 1;
                    }
                    TargetData => {
                        if is_xml_white_space(ch) {
                            if self.current_text.is_empty() {
                                current_buffer_idx += 1;
                                continue;
                            }
                            if current_quote_character == 0 {
                                current_buffer_idx += 1;
                                self.process_target_data();
                                continue;
                            }
                        }
                        if ch == 0x3f {
                            state = CloseInstruction;
                            current_buffer_idx += 1;
                        } else if ch == 0x22 {
                            if current_quote_character == 0 {
                                current_quote_character = ch;
                                current_buffer_idx += 1;
                            } else if ch == current_quote_character {
                                current_quote_character = 0;
                                current_buffer_idx += 1;
                                self.process_target_data();
                            } else {
                                return false;
                            }
                        } else {
                            self.current_text.push(ch);
                            current_buffer_idx += 1;
                        }
                    }
                }
            }
        }
    }

    fn process_text_char(&mut self, mut character: u32) {
        self.current_text.push(character);
        if let Some(start) = self.entity_start.filter(|_| character == u32::from(b';')) {
            // Exclude both '&' and ';' from the copied entity, then remove the
            // entire entity from the current text as in WideString::Delete.
            let mut entity = self.current_text.split_off(start + 1);
            entity.pop();
            self.current_text.truncate(start);
            if !entity.is_empty() {
                if entity[0] == u32::from(b'#') {
                    let mut ch = 0u32;
                    if entity.len() > 1 && entity[1] == u32::from(b'x') {
                        for &wide in &entity[2..] {
                            // The source calls the narrow-char hex helpers,
                            // narrowing wchar_t to char before classification.
                            let value = match wide as u8 {
                                b'0'..=b'9' => u32::from(wide as u8 - b'0'),
                                b'a'..=b'f' => u32::from(wide as u8 - b'a' + 10),
                                b'A'..=b'F' => u32::from(wide as u8 - b'A' + 10),
                                _ => break,
                            };
                            ch = (ch << 4).wrapping_add(value);
                        }
                    } else {
                        for &wide in &entity[1..] {
                            if !(u32::from(b'0')..=u32::from(b'9')).contains(&wide) {
                                break;
                            }
                            ch = ch.wrapping_mul(10).wrapping_add(wide - u32::from(b'0'));
                        }
                    }
                    if ch > MAX_CHAR_RANGE {
                        ch = u32::from(b' ');
                    }
                    character = ch;
                    if character != 0 {
                        self.current_text.push(character);
                    }
                } else {
                    let decoded = if equals_ascii(&entity, b"amp") {
                        Some(b'&')
                    } else if equals_ascii(&entity, b"lt") {
                        Some(b'<')
                    } else if equals_ascii(&entity, b"gt") {
                        Some(b'>')
                    } else if equals_ascii(&entity, b"apos") {
                        Some(b'\'')
                    } else if equals_ascii(&entity, b"quot") {
                        Some(b'"')
                    } else {
                        None
                    };
                    if let Some(decoded) = decoded {
                        self.current_text.push(u32::from(decoded));
                    }
                }
            }
            self.entity_start = None;
        } else if self.entity_start.is_none() && character == u32::from(b'&') {
            self.entity_start = Some(self.current_text.len() - 1);
        }
    }

    fn process_target_data(&mut self) {
        let target_data = self.get_text_data();
        if !target_data.is_empty() && self.node().node_type() == XmlNodeType::Instruction {
            self.node().append_data_owned(target_data);
        }
    }

    fn get_text_data(&mut self) -> Vec<u32> {
        let text = std::mem::replace(
            &mut self.current_text,
            Vec::with_capacity(CURRENT_TEXT_RESERVE),
        );
        self.entity_start = None;
        text
    }
}

#[cfg(test)]
#[path = "cfx_xmlparser_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "cfx_xmlparser_regression_tests.rs"]
mod regression_tests;
