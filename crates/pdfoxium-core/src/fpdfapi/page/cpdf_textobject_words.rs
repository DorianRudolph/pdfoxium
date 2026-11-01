// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `CPDF_TextObject::CountWords/GetWordString/IsLatinWord` from
//! `core/fpdfapi/page/cpdf_textobject.cpp`, used by the document JS API.
use super::cpdf_textobject::CpdfTextObject;
use crate::fxcrt::widetext_buffer::WideTextBuffer;
fn first_unicode_scalar(mapping: &[u32]) -> u32 {
    use crate::fxcrt::utf16::{is_high_surrogate, is_low_surrogate, SurrogatePair};
    match mapping {
        [high, low, ..] if is_high_surrogate(*high) && is_low_surrogate(*low) => {
            SurrogatePair::new(*high as u16, *low as u16).to_code_point()
        }
        [first, ..] => *first,
        [] => 0,
    }
}
fn is_latin_word(unicode: u32) -> bool {
    unicode != 0x20 && unicode <= 0x28ff
}
impl CpdfTextObject {
    pub fn count_words(&self) -> usize {
        let Some(font) = self.font() else {
            return 0;
        };
        let mut in_latin_word = false;
        let mut count = 0;
        for &code in self.char_codes() {
            // Retain the first mapped scalar, as the native algorithm does,
            // without truncating supplementary Unicode values to uint16_t.
            let unicode = first_unicode_scalar(&font.unicode_from_char_code(code));
            let latin = is_latin_word(unicode);
            if latin && in_latin_word {
                continue;
            }
            in_latin_word = latin;
            if unicode != 0x20 {
                count += 1;
            }
        }
        count
    }
    pub fn get_word_string(&self, index: i32) -> Vec<u32> {
        if index < 0 {
            return Vec::new();
        }
        let Some(font) = self.font() else {
            return Vec::new();
        };
        let mut result = WideTextBuffer::new();
        let mut count = 0i64;
        let mut in_latin_word = false;
        for &code in self.char_codes() {
            let unicode = first_unicode_scalar(&font.unicode_from_char_code(code));
            let latin = is_latin_word(unicode);
            if !latin || !in_latin_word {
                in_latin_word = latin;
                if unicode != 0x20 {
                    count += 1;
                }
            }
            if count - 1 == i64::from(index) {
                result.append_char(unicode);
            }
        }
        result.into_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::font::cpdf_font::CpdfFont;
    use crate::fpdfapi::page::{cpdf_docpagedata::CpdfDocPageData, cpdf_pageobject::PageObject};
    use crate::fxge::cfx_fontmapper::CfxFontMapper;
    fn text(value: &[u8]) -> CpdfTextObject {
        let font = CpdfFont::Simple(
            CpdfDocPageData::new()
                .get_stock_font(b"Helvetica", &mut CfxFontMapper::new())
                .unwrap(),
        );
        let mut object = CpdfTextObject::new(0);
        let dict = font.font_dict();
        object
            .base_mut()
            .mutable_text_state()
            .set_font(Some(font), dict);
        object.base_mut().mutable_text_state().set_font_size(12.0);
        object.set_text(value);
        object
    }
    #[test]
    fn native_latin_groups_preserve_spaces_punctuation_and_word_indices() {
        let object = text(b"  Hello, world!  nextword ");
        assert_eq!(object.count_words(), 3);
        assert_eq!(
            object.get_word_string(0),
            "Hello, ".chars().map(u32::from).collect::<Vec<_>>()
        );
        assert_eq!(
            object.get_word_string(1),
            "world!  ".chars().map(u32::from).collect::<Vec<_>>()
        );
        assert_eq!(
            object.get_word_string(2),
            "nextword ".chars().map(u32::from).collect::<Vec<_>>()
        );
        assert!(object.get_word_string(-1).is_empty());
        assert!(object.get_word_string(3).is_empty());
        assert_eq!(text(b"   ").count_words(), 0);
        let mut empty = CpdfTextObject::new(0);
        empty.base_mut().mutable_text_state().emplace();
        assert_eq!(empty.count_words(), 0);
        assert_eq!(first_unicode_scalar(&[0xd83d, 0xde00]), 0x1f600);
        assert_eq!(first_unicode_scalar(&[0x1f600]), 0x1f600);
        assert_eq!(first_unicode_scalar(&[0x41, 0x42]), 0x41);
        assert!(is_latin_word(0x28ff));
        assert!(!is_latin_word(0x2900));
        assert!(!is_latin_word(0x20));
        assert!(!is_latin_word(0x4e00));
        assert!(!is_latin_word(0x1f600));
    }
}
