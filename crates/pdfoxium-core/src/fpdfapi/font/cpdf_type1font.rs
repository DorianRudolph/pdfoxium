// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Base-14 setup and glyph mapping from `core/fpdfapi/font/cpdf_type1font.{h,cpp}`.
//! The shared font load path supplies descriptor, substitution, and face state.

use super::cpdf_facebasedsimplefont::CpdfFaceBasedSimpleFont;
use super::cpdf_fontencoding::FontEncoding;
use super::cpdf_simplefont::{NOT_DEF, SPACE};
use super::cpdf_truetypefont::{adobe_char_name, glyph_index_for_ms_symbol, use_tt_charmap};
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fxge::cfx_face::{CfxFace, CharMapIdPair};
use crate::fxge::cfx_standardfont::{
    get_canonical_font_name, get_standard_font_index, is_fixed_font, is_symbolic_font,
    StandardFontIndex,
};
use crate::fxge::fx_font::{
    font_style_is_non_symbolic, font_style_is_symbolic, unicode_from_adobe_name,
    FONT_STYLE_NON_SYMBOLIC, FONT_STYLE_SYMBOLIC, PLATFORM_WINDOWS, WINDOWS_ENCODING_SYMBOL,
};
use crate::fxge::fx_fontencoding::FontEncoding as FxFontEncoding;

const WINDOWS_SYMBOL: CharMapIdPair = CharMapIdPair {
    platform_id: PLATFORM_WINDOWS,
    encoding_id: WINDOWS_ENCODING_SYMBOL,
};

#[derive(Default)]
pub struct CpdfType1Font {
    pub base: CpdfFaceBasedSimpleFont,
    base14_font: Option<StandardFontIndex>,
}

impl CpdfType1Font {
    /// Applies the part of `Load()` that precedes `LoadCommon()`.
    /// Returns the canonical base name and flags passed to the common path.
    pub fn prepare_base14(
        &mut self,
        font_dict: &CpdfDictionary,
        base_font_name: &[u8],
        flags: u32,
    ) -> (Vec<u8>, u32) {
        self.base14_font = get_standard_font_index(base_font_name);
        let Some(index) = self.base14_font else {
            return (base_font_name.to_vec(), flags);
        };
        let descriptor_flags = font_dict.get_dict_for(b"FontDescriptor").and_then(|desc| {
            let borrowed = desc.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            dict.key_exist(b"Flags")
                .then(|| dict.get_integer_for(b"Flags", 0) as u32)
        });
        let flags = descriptor_flags.unwrap_or_else(|| {
            if is_symbolic_font(index) {
                FONT_STYLE_SYMBOLIC
            } else {
                FONT_STYLE_NON_SYMBOLIC
            }
        });
        if is_fixed_font(index) {
            self.base.fill_char_widths(600);
        }
        if index == StandardFontIndex::Symbol {
            self.base
                .simple
                .set_base_encoding(FontEncoding::AdobeSymbol);
        } else if index == StandardFontIndex::Dingbats {
            self.base
                .simple
                .set_base_encoding(FontEncoding::ZapfDingbats);
        } else if font_style_is_non_symbolic(flags) {
            self.base.simple.set_base_encoding(FontEncoding::Standard);
        }
        (get_canonical_font_name(index).as_bytes().to_vec(), flags)
    }

    pub fn is_base14_font(&self) -> bool {
        self.base14_font.is_some()
    }

    pub fn load_glyph_map(&mut self, face: &CfxFace<'_>, flags: u32, embedded: bool) {
        let mut base_encoding = self.base.simple.base_encoding();
        let names = self.base.simple.take_char_names();
        if !embedded && !font_style_is_symbolic(flags) && face.is_tt_ot() {
            if use_tt_charmap(face, WINDOWS_SYMBOL) {
                for code in 0u32..256 {
                    self.base
                        .set_glyph_index(code as u8, glyph_index_for_ms_symbol(face, code));
                }
                if self.base.has_any_glyph_index() {
                    return;
                }
            }
            face.select_charmap(FxFontEncoding::Unicode);
            if base_encoding == FontEncoding::Builtin {
                base_encoding = FontEncoding::Standard;
                self.base.simple.set_base_encoding(base_encoding);
            }
            for code in 0u32..256 {
                let Some(name) = adobe_char_name(base_encoding, &names, code as u8) else {
                    continue;
                };
                let unicode =
                    unicode_from_adobe_name(std::str::from_utf8(name).unwrap_or_default());
                self.base
                    .simple
                    .encoding_mut()
                    .set_unicode(code as u8, unicode);
                let mut glyph = face.get_char_index(unicode) as u16;
                if glyph == 0 && name == NOT_DEF {
                    self.base.simple.encoding_mut().set_unicode(code as u8, 32);
                    glyph = face.get_char_index(32) as u16;
                }
                self.base.set_glyph_index(code as u8, glyph);
            }
            return;
        }

        // Fontations currently selects the Adobe custom charmap for Type1.
        face.select_charmap(FxFontEncoding::AdobeCustom);
        if font_style_is_symbolic(flags) {
            for code in 0u32..256 {
                let glyph = if let Some(name) = adobe_char_name(base_encoding, &names, code as u8) {
                    let name = std::str::from_utf8(name).unwrap_or_default();
                    self.base
                        .simple
                        .encoding_mut()
                        .set_unicode(code as u8, unicode_from_adobe_name(name));
                    face.name_index(name) as u16
                } else {
                    let glyph = face.get_char_index(code) as u16;
                    if glyph != 0 {
                        let name = face.glyph_name(u32::from(glyph));
                        self.base
                            .simple
                            .encoding_mut()
                            .set_unicode(code as u8, unicode_from_adobe_name(&name));
                    }
                    glyph
                };
                self.base.set_glyph_index(code as u8, glyph);
            }
            return;
        }

        let unicode_charmap = face.select_charmap(FxFontEncoding::Unicode);
        for code in 0u32..256 {
            let Some(name) = adobe_char_name(base_encoding, &names, code as u8) else {
                continue;
            };
            let name = std::str::from_utf8(name).unwrap_or_default();
            let unicode = unicode_from_adobe_name(name);
            self.base
                .simple
                .encoding_mut()
                .set_unicode(code as u8, unicode);
            let mut glyph = face.name_index(name) as u16;
            if glyph == 0 {
                if name.as_bytes() != NOT_DEF && name.as_bytes() != SPACE {
                    glyph =
                        face.get_char_index(if unicode_charmap { unicode } else { code }) as u16;
                } else {
                    self.base.simple.encoding_mut().set_unicode(code as u8, 32);
                    glyph = 0xffff;
                }
            }
            self.base.set_glyph_index(code as u8, glyph);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn base14_setup_applies_descriptor_and_fixed_width() {
        let mut font = CpdfType1Font::default();
        let dict = CpdfDictionary::new();
        let (name, flags) = font.prepare_base14(&dict, b"CourierNew", 0);
        assert_eq!(name, b"Courier");
        assert_eq!(flags, FONT_STYLE_NON_SYMBOLIC);
        assert!(font.is_base14_font());
        assert_eq!(font.base.get_char_width(65, None, false), 600);
        assert_eq!(font.base.simple.base_encoding(), FontEncoding::Standard);

        let mut font = CpdfType1Font::default();
        let (name, flags) = font.prepare_base14(&dict, b"Symbol", 0);
        assert_eq!(name, b"Symbol");
        assert_eq!(flags, FONT_STYLE_SYMBOLIC);
        assert_eq!(font.base.simple.base_encoding(), FontEncoding::AdobeSymbol);

        let mut dict = CpdfDictionary::new();
        let mut descriptor = CpdfDictionary::new();
        descriptor.set_new_for(b"Flags", CpdfScalarObject::number_from_signed(0));
        dict.set_new_for(b"FontDescriptor", descriptor);
        let (_, flags) = font.prepare_base14(&dict, b"Symbol", 1);
        assert_eq!(flags, 0);
    }

    #[test]
    fn substituted_truetype_uses_adobe_names_and_unicode_cmap() {
        let data =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let mut face = CfxFace::new(data, 0).unwrap();
        let mut font = CpdfType1Font::default();
        font.base.simple.set_base_encoding(FontEncoding::Standard);
        font.load_glyph_map(&mut face, FONT_STYLE_NON_SYMBOLIC, false);
        assert_eq!(font.base.simple.encoding().unicode_from_char_code(b'A'), 65);
        assert_eq!(
            font.base.glyph_from_char_code(65),
            face.get_char_index(65) as i32
        );
        assert!(font.base.glyph_from_char_code(65) > 0);
    }
}
