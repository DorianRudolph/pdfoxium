// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/font/cpdf_fontencoding.{h,cpp}`.

use std::cell::RefCell;
use std::rc::Weak;

use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_name::CpdfName;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::fpdf_parser_decode::PDF_DOC_ENCODING;
use crate::fxcrt::bytestring_pool::ByteStringPool;
use crate::fxge::fx_font::adobe_name_from_unicode;
use crate::fxge::fx_fontencoding::FontEncoding as FxFontEncoding;

use super::cpdf_fontencoding_tables as tables;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FontEncoding {
    Builtin = 0,
    WinAnsi = 1,
    MacRoman = 2,
    MacExpert = 3,
    Standard = 4,
    AdobeSymbol = 5,
    ZapfDingbats = 6,
    PdfDoc = 7,
    MsSymbol = 8,
}

pub fn unicodes_for_predefined_char_set(encoding: FontEncoding) -> &'static [u16] {
    match encoding {
        FontEncoding::Builtin => &[],
        FontEncoding::WinAnsi => &tables::ADOBE_WIN_ANSI,
        FontEncoding::MacRoman => &tables::MAC_ROMAN,
        FontEncoding::MacExpert => &tables::MAC_EXPERT,
        FontEncoding::Standard => &tables::STANDARD,
        FontEncoding::AdobeSymbol => &tables::ADOBE_SYMBOL,
        FontEncoding::ZapfDingbats => &tables::ZAPF,
        FontEncoding::PdfDoc => &PDF_DOC_ENCODING,
        FontEncoding::MsSymbol => &tables::MS_SYMBOL,
    }
}

pub fn char_name_from_predefined_char_set(
    encoding: FontEncoding,
    charcode: u8,
) -> Option<&'static str> {
    let first = if encoding == FontEncoding::PdfDoc {
        24
    } else {
        32
    };
    let index = usize::from(charcode.checked_sub(first)?);
    match encoding {
        FontEncoding::WinAnsi => tables::ADOBE_WIN_ANSI_NAMES[index],
        FontEncoding::MacRoman => tables::MAC_ROMAN_NAMES[index],
        FontEncoding::MacExpert => tables::MAC_EXPERT_NAMES[index],
        FontEncoding::Standard => tables::STANDARD_NAMES[index],
        FontEncoding::AdobeSymbol => tables::ADOBE_SYMBOL_NAMES[index],
        FontEncoding::ZapfDingbats => tables::ZAPF_NAMES[index],
        FontEncoding::PdfDoc => tables::PDF_DOC_NAMES[index],
        FontEncoding::Builtin | FontEncoding::MsSymbol => None,
    }
}

fn find_code(codes: &[u16], unicode: u16) -> u32 {
    codes
        .iter()
        .position(|&code| code == unicode)
        .map_or(0, |index| index as u32)
}

pub fn char_code_from_unicode_for_encoding(encoding: FxFontEncoding, unicode: u32) -> u32 {
    let unicode16 = unicode as u16;
    match encoding {
        FxFontEncoding::Unicode => unicode,
        FxFontEncoding::AdobeStandard => find_code(&tables::STANDARD, unicode16),
        FxFontEncoding::AdobeExpert => find_code(&tables::MAC_EXPERT, unicode16),
        FxFontEncoding::Latin1 => find_code(&tables::ADOBE_WIN_ANSI, unicode16),
        FxFontEncoding::AppleRoman => find_code(&tables::MAC_ROMAN, unicode16),
        FxFontEncoding::AdobeCustom => find_code(&PDF_DOC_ENCODING, unicode16),
        FxFontEncoding::Symbol => find_code(&tables::MS_SYMBOL, unicode16),
        _ => 0,
    }
}

pub fn unicode_from_apple_roman_char_code(charcode: u8) -> u32 {
    u32::from(tables::MAC_ROMAN[usize::from(charcode)])
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CpdfFontEncoding {
    unicodes: [u32; 256],
}

impl CpdfFontEncoding {
    pub fn new(predefined_encoding: FontEncoding) -> Self {
        let mut unicodes = [0; 256];
        let src = unicodes_for_predefined_char_set(predefined_encoding);
        if !src.is_empty() {
            for (dst, &unicode) in unicodes.iter_mut().zip(src) {
                *dst = u32::from(unicode);
            }
        }
        Self { unicodes }
    }

    pub fn is_identical(&self, other: &Self) -> bool {
        self.unicodes == other.unicodes
    }

    pub fn unicode_from_char_code(&self, charcode: u8) -> u32 {
        self.unicodes[usize::from(charcode)]
    }

    pub fn char_code_from_unicode(&self, unicode: u32) -> i32 {
        self.unicodes
            .iter()
            .position(|&value| value == unicode)
            .map_or(-1, |index| index as i32)
    }

    pub fn set_unicode(&mut self, charcode: u8, unicode: u32) {
        self.unicodes[usize::from(charcode)] = unicode;
    }

    pub fn realize(&self, pool: Weak<RefCell<ByteStringPool>>) -> Option<ObjectRef> {
        const MATCH_ENCODINGS: [FontEncoding; 5] = [
            FontEncoding::WinAnsi,
            FontEncoding::MacRoman,
            FontEncoding::MacExpert,
            FontEncoding::Standard,
            FontEncoding::AdobeSymbol,
        ];
        for encoding in MATCH_ENCODINGS {
            if self
                .unicodes
                .iter()
                .zip(unicodes_for_predefined_char_set(encoding))
                .all(|(&unicode, &predefined)| unicode == u32::from(predefined))
            {
                let name: &[u8] = match encoding {
                    FontEncoding::WinAnsi => b"WinAnsiEncoding",
                    FontEncoding::MacRoman => b"MacRomanEncoding",
                    FontEncoding::MacExpert => b"MacExpertEncoding",
                    _ => return None,
                };
                let name = if let Some(pool) = pool.upgrade() {
                    CpdfName::new(Some(&mut pool.borrow_mut()), name)
                } else {
                    CpdfName::new(None, name)
                };
                return Some(retain_object(name));
            }
        }
        let mut differences = CpdfArray::new();
        for (index, &unicode) in self.unicodes.iter().enumerate() {
            if unicode == u32::from(tables::ADOBE_WIN_ANSI[index]) {
                continue;
            }
            differences.append_new(CpdfScalarObject::number_from_signed(index as i32));
            differences.append_new(CpdfName::new(None, &adobe_name_from_unicode(unicode)));
        }
        let mut dict = CpdfDictionary::with_pool(pool);
        dict.set_new_name_for(b"BaseEncoding", b"WinAnsiEncoding");
        dict.set_for(b"Differences", Some(retain_object(differences)));
        Some(retain_object(dict))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;

    #[test]
    fn predefined_encoding_lookup_and_realization() {
        assert_eq!(unicode_from_apple_roman_char_code(0x80), 0x00c4);
        assert_eq!(
            char_code_from_unicode_for_encoding(FxFontEncoding::AdobeStandard, 0x41),
            65
        );
        assert_eq!(
            char_name_from_predefined_char_set(FontEncoding::Standard, 65),
            Some("A")
        );
        assert_eq!(
            char_name_from_predefined_char_set(FontEncoding::Standard, 0),
            None
        );
        let mut encoding = CpdfFontEncoding::new(FontEncoding::WinAnsi);
        assert_eq!(encoding.unicode_from_char_code(65), 65);
        assert_eq!(encoding.char_code_from_unicode(65), 65);
        assert_eq!(
            encoding
                .realize(Weak::new())
                .unwrap()
                .borrow()
                .object_type(),
            ObjectType::Name
        );
        encoding.set_unicode(65, 0x0391);
        assert_eq!(encoding.unicode_from_char_code(65), 0x0391);
        let realized = encoding.realize(Weak::new()).unwrap();
        let borrowed = realized.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        assert_eq!(dict.get_name_for(b"BaseEncoding"), b"WinAnsiEncoding");
        let differences = dict.get_array_for(b"Differences").unwrap();
        let borrowed_differences = differences.borrow();
        let array = borrowed_differences
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap();
        assert_eq!(array.size(), 2);
        assert_eq!(array.get_integer_at(0), 65);
        assert_eq!(
            array.get_object_at(1).unwrap().borrow().get_string(),
            b"Alpha"
        );
    }
}
