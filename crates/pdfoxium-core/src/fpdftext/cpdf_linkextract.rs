// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `core/fpdftext/cpdf_linkextract.{h,cpp}`. Link candidates retain original
//! character ranges; the extractor owns its immutable text-page snapshot.
use super::cpdf_textpage::{is_normal, CharType, CpdfTextPage};
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    pub start: usize,
    pub count: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Link {
    range: Range,
    url: Vec<u32>,
}
pub struct CpdfLinkExtract {
    text_page: Rc<CpdfTextPage>,
    links: Vec<Link>,
}

fn find(text: &[u32], needle: &[u32]) -> Option<usize> {
    if needle.is_empty() {
        Some(0)
    } else {
        text.windows(needle.len()).position(|s| s == needle)
    }
}
fn alnum(ch: u32) -> bool {
    char::from_u32(ch).is_some_and(char::is_alphanumeric)
}
fn decimal(ch: u32) -> bool {
    (48..=57).contains(&ch)
}

fn find_web_link_ending(text: &[u32], start: usize, mut end: usize) -> Option<usize> {
    if start > end || end >= text.len() {
        return None;
    }
    if text[start..].contains(&47) {
        return Some(end);
    }
    if text[start] == 91 {
        if let Some(close) = text[start + 1..]
            .iter()
            .position(|&c| c == 93)
            .map(|p| p + start + 1)
        {
            end = close;
            if close > start + 1 && text.get(close + 1) == Some(&58) {
                let mut off = close + 2;
                while text.get(off).is_some_and(|&c| decimal(c)) {
                    off += 1;
                }
                if off > close + 2 {
                    end = off - 1;
                }
            }
        }
        return Some(end);
    }
    while end > start && text[end] < 128 {
        let ch = text[end];
        if decimal(ch) || (97..=122).contains(&ch) || ch == 46 {
            break;
        }
        end -= 1;
    }
    Some(end)
}
fn trim_external_brackets(text: &[u32], start: usize, mut end: usize) -> Option<usize> {
    for &ch in &text[..start] {
        let close = match ch {
            40 => 41,
            91 => 93,
            123 => 125,
            60 => 62,
            34 => 34,
            39 => 39,
            _ => continue,
        };
        if end < start {
            return None;
        }
        if let Some(pos) = text[start..=end].iter().rposition(|&c| c == close) {
            // Native unsigned subtraction underflows when a closing delimiter
            // is the first URL character. An empty candidate is simply invalid.
            end = (start + pos).checked_sub(1)?;
        }
    }
    (end >= start).then_some(end)
}
fn check_web_link(original: &[u32]) -> Option<Link> {
    let text: Vec<u32> = original
        .iter()
        .map(|&c| {
            char::from_u32(c)
                .and_then(|c| c.to_lowercase().next())
                .map_or(c, |c| c as u32)
        })
        .collect();
    if let Some(start) = find(&text, &[104, 116, 116, 112]) {
        let mut off = start + 4;
        if text.len() > off + 4 {
            if text[off] == 115 {
                off += 1;
            }
            if text.get(off..off + 3) == Some(&[58, 47, 47]) {
                off += 3;
                if let Some(end) = trim_external_brackets(&text, start, text.len() - 1)
                    .and_then(|end| find_web_link_ending(&text, off, end))
                {
                    if end > off {
                        return Some(Link {
                            range: Range {
                                start,
                                count: end - start + 1,
                            },
                            url: original[start..=end].to_vec(),
                        });
                    }
                }
            }
        }
    }
    if let Some(start) = find(&text, &[119, 119, 119, 46]) {
        let off = start + 4;
        if text.len() > off {
            if let Some(end) = trim_external_brackets(&text, start, text.len() - 1)
                .and_then(|end| find_web_link_ending(&text, start, end))
            {
                if end > off {
                    let mut url = vec![104, 116, 116, 112, 58, 47, 47];
                    url.extend_from_slice(&original[start..=end]);
                    return Some(Link {
                        range: Range {
                            start,
                            count: end - start + 1,
                        },
                        url,
                    });
                }
            }
        }
    }
    None
}

/// Return the selected span as well as rewriting the mailto URL. The span is
/// needed to keep the public range on the actual address after native trimming.
fn check_mail_link(text: &mut Vec<u32>) -> Option<Range> {
    let mut at = text.iter().position(|&c| c == 64)?;
    if at == 0 || at == text.len() - 1 {
        return None;
    }
    let mut start = 0;
    let mut previous = at;
    for i in (1..=at).rev() {
        let ch = text[i - 1];
        if ch == 95 || ch == 45 || alnum(ch) {
            continue;
        }
        if ch != 46 || i == previous || i == 1 {
            if i == at {
                return None;
            }
            start = if i == previous { i + 1 } else { i };
            text.drain(..start);
            break;
        }
        previous = i - 1;
    }
    at = text.iter().position(|&c| c == 64)?;
    if at == 0 {
        return None;
    }
    while text.last() == Some(&46) {
        text.pop();
    }
    let first_dot = text[at + 1..]
        .iter()
        .position(|&c| c == 46)
        .map(|i| i + at + 1)?;
    if first_dot == at + 1 {
        return None;
    }
    previous = 0;
    for i in at + 1..text.len() {
        let ch = text[i];
        if ch == 45 || alnum(ch) {
            continue;
        }
        if ch != 46 || i == previous + 1 {
            let end = if i == previous + 1 {
                i.checked_sub(2)?
            } else {
                i - 1
            };
            if previous > 0 && end.saturating_sub(at) >= 3 {
                text.truncate(end + 1);
                break;
            }
            return None;
        }
        previous = i;
    }
    let count = text.len();
    if find(text, &[109, 97, 105, 108, 116, 111, 58]).is_none() {
        text.splice(..0, [109, 97, 105, 108, 116, 111, 58]);
    }
    Some(Range { start, count })
}

impl CpdfLinkExtract {
    pub fn new(text_page: Rc<CpdfTextPage>) -> Self {
        Self {
            text_page,
            links: Vec::new(),
        }
    }
    pub fn count_links(&self) -> usize {
        self.links.len()
    }
    pub fn get_url(&self, index: usize) -> Vec<u32> {
        self.links
            .get(index)
            .map_or_else(Vec::new, |l| l.url.clone())
    }
    pub fn get_rects(&self, index: usize) -> Vec<CfxFloatRect> {
        let Some(link) = self.links.get(index) else {
            return Vec::new();
        };
        let (Ok(start), Ok(count)) = (
            i32::try_from(link.range.start),
            i32::try_from(link.range.count),
        ) else {
            return Vec::new();
        };
        self.text_page.get_rect_array(start, count)
    }
    pub fn get_text_range(&self, index: usize) -> Option<Range> {
        self.links.get(index).map(|l| l.range)
    }
    pub fn extract_links(&mut self) {
        self.links.clear();
        let mut after_hyphen = false;
        let mut line_break = false;
        let total = self.text_page.count_chars() as usize;
        let page_text = self.text_page.get_all_page_text();
        let mut text_pos = 0;
        let mut text = Vec::new();
        let mut offsets = Vec::new();
        for pos in 0..total {
            let info = self.text_page.get_char_info(pos);
            let kind = info.char_type();
            let unicode = info.unicode();
            let candidate_continues =
                kind != CharType::Generated && unicode != 32 && pos != total - 1;
            let hyphen_break = pos != total - 1 && after_hyphen && matches!(unicode, 10 | 13);
            // Walk the same printable map as TextPage::Init, in lockstep with
            // the compact text buffer. Native slicing by CharInfo indices
            // shifts links after suppressed controls. Retained offsets also
            // keep ranges spanning removed CR/LF characters correct, in O(n).
            if kind == CharType::Generated || is_normal(info) {
                if candidate_continues || hyphen_break || pos == total - 1 {
                    if let Some(&ch) = page_text.get(text_pos) {
                        text.push(ch);
                        offsets.push(pos);
                    }
                }
                text_pos += 1;
            }
            if candidate_continues {
                after_hyphen =
                    kind == CharType::Hyphen || (kind == CharType::Normal && unicode == 45);
                continue;
            }
            if hyphen_break {
                line_break = true;
                continue;
            }
            self.extract_candidate(&mut text, &mut offsets, line_break);
            text.clear();
            offsets.clear();
            line_break = false;
        }
    }
    fn extract_candidate(
        &mut self,
        text: &mut Vec<u32>,
        offsets: &mut Vec<usize>,
        remove_breaks: bool,
    ) {
        let mut output = 0;
        for i in 0..text.len() {
            if remove_breaks && matches!(text[i], 10 | 13) {
                continue;
            }
            offsets[output] = offsets[i];
            text[output] = if text[i] == 0xfffe { 45 } else { text[i] };
            output += 1;
        }
        text.truncate(output);
        offsets.truncate(output);
        if text.len() <= 5 {
            return;
        }
        while text.last().is_some_and(|&c| matches!(c, 41 | 44 | 62 | 46)) {
            text.pop();
            offsets.pop();
        }
        if text.len() <= 5 {
            return;
        }
        let selected = if let Some(link) = check_web_link(&text) {
            Some(link)
        } else {
            check_mail_link(text).map(|range| Link {
                range,
                url: std::mem::take(text),
            })
        };
        let Some(mut link) = selected else {
            return;
        };
        let Some((&first, &last)) = offsets.get(link.range.start).zip(
            link.range
                .count
                .checked_sub(1)
                .and_then(|last| link.range.start.checked_add(last))
                .and_then(|i| offsets.get(i)),
        ) else {
            return;
        };
        link.range = Range {
            start: first,
            count: last - first + 1,
        };
        self.links.push(link);
    }
}

#[cfg(test)]
#[path = "cpdf_linkextract_tests.rs"]
mod tests;
