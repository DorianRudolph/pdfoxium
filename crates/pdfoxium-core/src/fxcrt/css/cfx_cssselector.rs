// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! The source CSS selector subset: ASCII element names, wildcard and descendant
//! chains. Matching remains the style selector's responsibility.
use super::cfx_cssdata::hash_code_lowered_wide;
use std::rc::Rc;

#[derive(Debug)]
pub struct CfxCssSelector {
    is_descendant: bool,
    name_hash: u32,
    next: Option<Rc<Self>>,
}
impl CfxCssSelector {
    pub fn new(name: &[u32], next: Option<Rc<Self>>) -> Self {
        Self {
            is_descendant: false,
            name_hash: hash_code_lowered_wide(name),
            next,
        }
    }
    pub fn from_string(text: &[u32]) -> Option<Rc<Self>> {
        if text
            .iter()
            .any(|&character| matches!(character, 62 | 91 | 43))
        {
            return None;
        }
        let mut head: Option<Rc<Self>> = None;
        let mut index = 0;
        while index < text.len() {
            let character = text[index];
            if character == u32::from(b' ') {
                index += 1;
                continue;
            }
            let is_star = character == u32::from(b'*');
            if !is_star && !(character <= 127 && (character as u8).is_ascii_alphabetic()) {
                return None;
            }
            if let Some(previous) = &mut head {
                // Only this constructor owns the unpublished head.
                Rc::get_mut(previous)
                    .expect("private selector chain")
                    .is_descendant = true;
            }
            let length = if is_star {
                1
            } else {
                text[index..]
                    .iter()
                    .position(|&character| {
                        character > 127
                            || !((character as u8).is_ascii_alphanumeric()
                                || matches!(character, 95 | 45))
                    })
                    .unwrap_or(text.len() - index)
            };
            head = Some(Rc::new(Self::new(&text[index..index + length], head)));
            index += length;
        }
        head
    }
    pub fn is_descendant(&self) -> bool {
        self.is_descendant
    }
    pub fn name_hash(&self) -> u32 {
        self.name_hash
    }
    pub fn next_selector(&self) -> Option<&Self> {
        self.next.as_deref()
    }
}
impl Drop for CfxCssSelector {
    fn drop(&mut self) {
        // Source chains may be arbitrarily long. Retaining a shared suffix
        // still works; uniquely owned suffixes are dismantled iteratively.
        let mut next = self.next.take();
        while let Some(node) = next {
            match Rc::try_unwrap(node) {
                Ok(mut selector) => next = selector.next.take(),
                Err(_) => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    #[test]
    fn selector_subset_and_chain_flags_follow_source() {
        for text in [
            "", " ", "#id", ".class", "2tag", "a > b", "a[first]", "a+b", "a ^ b", "a\tb", "é",
        ] {
            assert!(CfxCssSelector::from_string(&wide(text)).is_none(), "{text}");
        }
        let selector = CfxCssSelector::from_string(&wide(" A_b-2 * C ")).unwrap();
        assert_eq!(selector.name_hash(), hash_code_lowered_wide(&wide("c")));
        assert!(!selector.is_descendant());
        let middle = selector.next_selector().unwrap();
        assert_eq!(middle.name_hash(), hash_code_lowered_wide(&wide("*")));
        assert!(middle.is_descendant());
        let last = middle.next_selector().unwrap();
        assert_eq!(last.name_hash(), hash_code_lowered_wide(&wide("a_b-2")));
        assert!(last.is_descendant());
        assert!(last.next_selector().is_none());
    }
    #[test]
    fn long_selector_chains_drop_without_host_stack_recursion() {
        let text = wide(&"a ".repeat(50_000));
        let selector = CfxCssSelector::from_string(&text).unwrap();
        let retained = selector.next.as_ref().unwrap().clone();
        drop(selector);
        assert_eq!(retained.name_hash(), hash_code_lowered_wide(&wide("a")));
        drop(retained);
    }
}
