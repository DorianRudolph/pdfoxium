// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Eight-bit encoding state from `core/fpdfapi/font/cpdf_simplefont.{h,cpp}`.
//! Shared font loading connects its derived glyph maps later.

use super::cpdf_fontencoding::{CpdfFontEncoding, FontEncoding};
use super::cpdf_tounicodemap::CpdfToUnicodeMap;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_name::CpdfName;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fxge::fx_font::font_style_is_symbolic;

pub const NOT_DEF: &[u8] = b".notdef";
pub const SPACE: &[u8] = b"space";

pub struct CpdfSimpleFont {
    encoding: CpdfFontEncoding,
    base_encoding: FontEncoding,
    char_names: Vec<Vec<u8>>,
}

impl Default for CpdfSimpleFont {
    fn default() -> Self {
        Self {
            encoding: CpdfFontEncoding::new(FontEncoding::Builtin),
            base_encoding: FontEncoding::Builtin,
            char_names: Vec::new(),
        }
    }
}

impl CpdfSimpleFont {
    pub fn encoding(&self) -> &CpdfFontEncoding {
        &self.encoding
    }

    pub fn encoding_mut(&mut self) -> &mut CpdfFontEncoding {
        &mut self.encoding
    }

    pub fn base_encoding(&self) -> FontEncoding {
        self.base_encoding
    }

    pub fn set_base_encoding(&mut self, encoding: FontEncoding) {
        self.base_encoding = encoding;
    }

    pub fn char_names(&self) -> &[Vec<u8>] {
        &self.char_names
    }

    pub fn take_char_names(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.char_names)
    }

    pub fn load_differences(&mut self, encoding: &CpdfDictionary) {
        let Some(array) = encoding.get_array_for(b"Differences") else {
            return;
        };
        let borrowed = array.borrow();
        let Some(diffs) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
            return;
        };
        self.char_names.resize(256, Vec::new());
        let mut code = 0u32;
        for index in 0..diffs.size() {
            let Some(element) = diffs.get_direct_object_at(index) else {
                continue;
            };
            let object = element.borrow();
            if let Some(name) = object.as_any().downcast_ref::<CpdfName>() {
                if let Some(slot) = self.char_names.get_mut(code as usize) {
                    *slot = name.get_string().to_vec();
                }
                code = code.wrapping_add(1);
            } else {
                code = object.get_integer() as u32;
            }
        }
    }

    pub fn load_pdf_encoding(
        &mut self,
        font_dict: &CpdfDictionary,
        base_font_name: &[u8],
        flags: u32,
        embedded: bool,
        true_type: bool,
    ) {
        let Some(encoding) = font_dict.get_direct_object_for(b"Encoding") else {
            if crate::fxcrt::bytestring::eq_cstr(base_font_name, b"Symbol") {
                self.base_encoding = if true_type {
                    FontEncoding::MsSymbol
                } else {
                    FontEncoding::AdobeSymbol
                };
            } else if !embedded && self.base_encoding == FontEncoding::Builtin {
                self.base_encoding = FontEncoding::WinAnsi;
            }
            return;
        };
        let object = encoding.borrow();
        if object.object_type() == ObjectType::Name {
            if matches!(
                self.base_encoding,
                FontEncoding::AdobeSymbol | FontEncoding::ZapfDingbats
            ) {
                return;
            }
            if font_style_is_symbolic(flags)
                && crate::fxcrt::bytestring::eq_cstr(base_font_name, b"Symbol")
            {
                if !true_type {
                    self.base_encoding = FontEncoding::AdobeSymbol;
                }
                return;
            }
            let mut name = object.get_string();
            if crate::fxcrt::bytestring::eq_cstr(&name, b"MacExpertEncoding") {
                name = b"WinAnsiEncoding".to_vec();
            }
            set_predefined_encoding(&name, &mut self.base_encoding);
            return;
        }
        let Some(dict) = object.as_any().downcast_ref::<CpdfDictionary>() else {
            return;
        };
        if !matches!(
            self.base_encoding,
            FontEncoding::AdobeSymbol | FontEncoding::ZapfDingbats
        ) {
            let mut name = dict.get_byte_string_for(b"BaseEncoding", b"");
            if true_type && crate::fxcrt::bytestring::eq_cstr(&name, b"MacExpertEncoding") {
                name = b"WinAnsiEncoding".to_vec();
            }
            set_predefined_encoding(&name, &mut self.base_encoding);
        }
        if (!embedded || true_type) && self.base_encoding == FontEncoding::Builtin {
            self.base_encoding = FontEncoding::Standard;
        }
        self.load_differences(dict);
    }

    pub fn is_unicode_compatible(&self) -> bool {
        !matches!(
            self.base_encoding,
            FontEncoding::Builtin | FontEncoding::AdobeSymbol | FontEncoding::ZapfDingbats
        )
    }

    pub fn unicode_from_char_code(
        &self,
        charcode: u32,
        to_unicode: Option<&CpdfToUnicodeMap>,
    ) -> Vec<u32> {
        if let Some(map) = to_unicode {
            let unicode = map.lookup(charcode);
            if !unicode.is_empty() {
                return unicode;
            }
        }
        let unicode = self.encoding.unicode_from_char_code(charcode as u8);
        if unicode == 0 {
            Vec::new()
        } else {
            vec![unicode]
        }
    }

    pub fn char_code_from_unicode(
        &self,
        unicode: u32,
        to_unicode: Option<&CpdfToUnicodeMap>,
    ) -> u32 {
        if let Some(map) = to_unicode {
            let code = map.reverse_lookup(unicode);
            if code != 0 {
                return code;
            }
        }
        self.encoding.char_code_from_unicode(unicode) as u32
    }

    pub fn glyph_from_char_code(&self, _charcode: u32) -> i32 {
        -1
    }
}

fn set_predefined_encoding(name: &[u8], base: &mut FontEncoding) {
    *base = match crate::fxcrt::bytestring::c_str(name) {
        b"WinAnsiEncoding" => FontEncoding::WinAnsi,
        b"MacRomanEncoding" => FontEncoding::MacRoman,
        b"MacExpertEncoding" => FontEncoding::MacExpert,
        b"PDFDocEncoding" => FontEncoding::PdfDoc,
        _ => return,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn simple_font_encoding_rules_and_differences() {
        let mut font = CpdfSimpleFont::default();
        let mut dict = CpdfDictionary::new();
        font.load_pdf_encoding(&dict, b"Symbol", 0, false, false);
        assert_eq!(font.base_encoding(), FontEncoding::AdobeSymbol);
        assert!(!font.is_unicode_compatible());

        let mut font = CpdfSimpleFont::default();
        dict.set_new_name_for(b"Encoding", b"MacExpertEncoding");
        font.load_pdf_encoding(&dict, b"Helvetica", 0, false, false);
        assert_eq!(font.base_encoding(), FontEncoding::WinAnsi);

        let mut encoding = CpdfDictionary::new();
        let array = encoding.set_new_array_for(b"Differences");
        let mut borrowed = array.borrow_mut();
        let diffs = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        diffs.append_new(CpdfScalarObject::number_from_signed(65));
        diffs.append_new_name(b"A.alt");
        diffs.append_new_name(b"B.alt");
        drop(borrowed);
        dict.set_new_for(b"Encoding", encoding);
        font.load_pdf_encoding(&dict, b"Helvetica", 0, false, false);
        assert_eq!(font.char_names()[65], b"A.alt");
        assert_eq!(font.char_names()[66], b"B.alt");
        assert_eq!(font.base_encoding(), FontEncoding::WinAnsi);

        let map = CpdfToUnicodeMap::from_data(b"1 beginbfchar <41><20ac> endbfchar");
        font.encoding_mut().set_unicode(65, 0x41);
        assert_eq!(font.unicode_from_char_code(65, Some(&map)), vec![0x20ac]);
        assert_eq!(font.unicode_from_char_code(65, None), vec![0x41]);
        assert_eq!(font.char_code_from_unicode(0x20ac, Some(&map)), 65);
    }
}

#[cfg(test)]
#[path = "cpdf_simplefont_unittest.rs"]
mod upstream_tests;
