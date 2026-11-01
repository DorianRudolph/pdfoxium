// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Glyph mapping from `core/fpdfapi/font/cpdf_truetypefont.{h,cpp}`.
//! The common font load path supplies face, descriptor, and ToUnicode state.

use super::cpdf_facebasedsimplefont::CpdfFaceBasedSimpleFont;
use super::cpdf_font::CpdfFontBase;
use super::cpdf_fontencoding::{
    char_code_from_unicode_for_encoding, char_name_from_predefined_char_set,
    unicode_from_apple_roman_char_code, unicodes_for_predefined_char_set, FontEncoding,
};
use super::cpdf_tounicodemap::CpdfToUnicodeMap;
use crate::fpdfapi::page::cpdf_docpagedata::CachedDocumentFace;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxge::cfx_face::{
    charmap_id_pair_to_font_encoding, CfxFace, CfxFaceError, CharMapIdPair,
};
use crate::fxge::fx_font::{
    font_style_is_non_symbolic, font_style_is_symbolic, unicode_from_adobe_name,
    MAC_ENCODING_ROMAN, PLATFORM_APPLE_UNICODE, PLATFORM_MAC, PLATFORM_WINDOWS,
    WINDOWS_ENCODING_SYMBOL, WINDOWS_ENCODING_UNICODE,
};
use crate::fxge::fx_fontencoding::FontEncoding as FxFontEncoding;

const WINDOWS_UNICODE: CharMapIdPair = CharMapIdPair {
    platform_id: PLATFORM_WINDOWS,
    encoding_id: WINDOWS_ENCODING_UNICODE,
};
const WINDOWS_SYMBOL: CharMapIdPair = CharMapIdPair {
    platform_id: PLATFORM_WINDOWS,
    encoding_id: WINDOWS_ENCODING_SYMBOL,
};
pub(super) const MAC_ROMAN: CharMapIdPair = CharMapIdPair {
    platform_id: PLATFORM_MAC,
    encoding_id: MAC_ENCODING_ROMAN,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CharmapType {
    MsUnicode,
    MsSymbol,
    MacRoman,
    Other,
}

pub struct CpdfTrueTypeFont {
    pub base: CpdfFaceBasedSimpleFont,
    pub font_base: CpdfFontBase,
    pub face: Option<std::rc::Rc<CachedDocumentFace>>,
}

impl Default for CpdfTrueTypeFont {
    fn default() -> Self {
        Self {
            base: CpdfFaceBasedSimpleFont::default(),
            font_base: CpdfFontBase::default(),
            face: None,
        }
    }
}

impl CpdfTrueTypeFont {
    /// Loads an embedded TrueType font through the document's retained stream cache.
    /// Returns false when no usable embedded font stream is present.
    pub fn load_embedded(
        &mut self,
        document: &CpdfDocument,
        font_dict: &CpdfDictionary,
        base_font_name: &[u8],
    ) -> Result<bool, CfxFaceError> {
        let descriptor = font_dict.get_dict_for(b"FontDescriptor");
        let font_stream = descriptor.as_ref().and_then(|object| {
            let borrowed = object.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            self.font_base.load_font_descriptor(dict)
        });
        let Some(face) = CpdfFontBase::load_embedded_face(document, font_stream.as_ref())? else {
            return Ok(false);
        };
        let flags = self.font_base.flags;
        if let Some(descriptor) = &descriptor {
            let borrowed = descriptor.borrow();
            self.base.load_char_widths(
                font_dict,
                borrowed.as_any().downcast_ref::<CpdfDictionary>(),
            );
        } else {
            self.base.load_char_widths(font_dict, None);
        }
        self.base
            .load_pdf_encoding(font_dict, base_font_name, flags, true, true);
        let to_unicode_stream = font_dict.get_stream_for(b"ToUnicode");
        let to_unicode = to_unicode_stream.as_ref().and_then(|object| {
            let borrowed = object.borrow();
            Some(CpdfToUnicodeMap::from_stream(
                borrowed.as_any().downcast_ref::<CpdfStream>()?,
            ))
        });
        let first_char = font_dict.get_integer_for(b"FirstChar", 0);
        let has_to_unicode = to_unicode_stream.is_some();
        self.load_glyph_map_with_details(
            face.face(),
            first_char,
            has_to_unicode,
            flags,
            true,
            to_unicode.as_ref(),
        );
        if crate::fxge::fx_font::font_style_is_all_caps(flags) {
            self.base.apply_all_caps(true);
        }
        self.face = Some(face);
        let face_ref = self.face.as_ref().unwrap().face();
        self.font_base.check_font_metrics(Some(face_ref), |code| {
            self.base.get_char_bbox(code, Some(face_ref), true)
        });
        Ok(true)
    }

    pub fn load_glyph_map(
        &mut self,
        face: &CfxFace<'_>,
        font_dict: &CpdfDictionary,
        flags: u32,
        embedded: bool,
        to_unicode: Option<&CpdfToUnicodeMap>,
    ) {
        self.load_glyph_map_with_details(
            face,
            font_dict.get_integer_for(b"FirstChar", 0),
            font_dict.key_exist(b"ToUnicode"),
            flags,
            embedded,
            to_unicode,
        );
    }

    fn load_glyph_map_with_details(
        &mut self,
        face: &CfxFace<'_>,
        first_char: i32,
        has_to_unicode: bool,
        flags: u32,
        embedded: bool,
        to_unicode: Option<&CpdfToUnicodeMap>,
    ) {
        let base_encoding =
            determine_encoding(face, self.base.simple.base_encoding(), flags, embedded);
        let names = self.base.simple.take_char_names();
        if (matches!(
            base_encoding,
            FontEncoding::WinAnsi | FontEncoding::MacRoman
        ) && names.is_empty())
            || font_style_is_non_symbolic(flags)
        {
            if face.has_glyph_names() && face.charmap_count() == 0 {
                self.base.set_glyph_indices_from_first_char(first_char);
                return;
            }
            let charmap_type = determine_charmap_type(face, flags);
            let mac_charmap = (0..face.charmap_count())
                .find(|&index| face.charmap_id_pair_by_index(index) == MAC_ROMAN);
            for code in 0u32..256 {
                let name = adobe_char_name(base_encoding, &names, code as u8);
                let Some(name) = name else {
                    self.base.set_glyph_index(
                        code as u8,
                        if embedded {
                            face.get_char_index(code) as u16
                        } else {
                            0xffff
                        },
                    );
                    continue;
                };
                let name_str = std::str::from_utf8(name).unwrap_or_default();
                let unicode = unicode_from_adobe_name(name_str);
                self.base
                    .simple
                    .encoding_mut()
                    .set_unicode(code as u8, unicode);
                let mut glyph = match charmap_type {
                    CharmapType::MsSymbol => glyph_index_for_ms_symbol(face, code),
                    CharmapType::MsUnicode if unicode != 0 => {
                        let glyph = face.get_char_index(unicode);
                        if glyph != 0 {
                            glyph as u16
                        } else {
                            // Hayro's per-character fallback: an unavailable
                            // Unicode mapping (e.g. unsupported cmap format 2)
                            // can still have a usable Macintosh Roman record.
                            // Look up that record without changing the selected
                            // map for the next character.
                            let mac = char_code_from_unicode_for_encoding(
                                FxFontEncoding::AppleRoman,
                                unicode,
                            );
                            mac_charmap
                                .filter(|_| {
                                    mac != 0
                                        && unicode_from_apple_roman_char_code(mac as u8) == unicode
                                })
                                .map_or(0, |index| face.cmap_char_to_gid(index, mac) as u16)
                        }
                    }
                    CharmapType::MacRoman if unicode != 0 => {
                        let mac = char_code_from_unicode_for_encoding(
                            FxFontEncoding::AppleRoman,
                            unicode,
                        );
                        if mac == 0 {
                            face.name_index(name_str) as u16
                        } else {
                            face.get_char_index(mac) as u16
                        }
                    }
                    _ => 0xffff,
                };
                if glyph != 0 && glyph != 0xffff {
                    self.base.set_glyph_index(code as u8, glyph);
                    continue;
                }
                if name == b".notdef" {
                    self.base
                        .set_glyph_index(code as u8, face.get_char_index(32) as u16);
                    continue;
                }
                glyph = face.name_index(name_str) as u16;
                if glyph == 0 && has_to_unicode {
                    let mapped = self.base.simple.unicode_from_char_code(code, to_unicode);
                    if let Some(&first) = mapped.first() {
                        glyph = face.get_char_index(first) as u16;
                        self.base
                            .simple
                            .encoding_mut()
                            .set_unicode(code as u8, first);
                    }
                }
                self.base.set_glyph_index(code as u8, glyph);
            }
            return;
        }

        if use_tt_charmap(face, WINDOWS_SYMBOL) {
            for code in 0u32..256 {
                self.base
                    .set_glyph_index(code as u8, glyph_index_for_ms_symbol(face, code));
            }
            if self.base.has_any_glyph_index() {
                if base_encoding != FontEncoding::Builtin {
                    for code in 0u32..256 {
                        if let Some(name) = adobe_char_name(base_encoding, &names, code as u8) {
                            let name = std::str::from_utf8(name).unwrap_or_default();
                            self.base
                                .simple
                                .encoding_mut()
                                .set_unicode(code as u8, unicode_from_adobe_name(name));
                        }
                    }
                } else if use_tt_charmap(face, MAC_ROMAN) {
                    for code in 0u32..256 {
                        self.base.simple.encoding_mut().set_unicode(
                            code as u8,
                            unicode_from_apple_roman_char_code(code as u8),
                        );
                    }
                }
                return;
            }
        }
        if use_tt_charmap(face, MAC_ROMAN) {
            for code in 0u32..256 {
                self.base
                    .set_glyph_index(code as u8, face.get_char_index(code) as u16);
                self.base
                    .simple
                    .encoding_mut()
                    .set_unicode(code as u8, unicode_from_apple_roman_char_code(code as u8));
            }
            if embedded || self.base.has_any_glyph_index() {
                return;
            }
        }
        if face.select_charmap(FxFontEncoding::Unicode) {
            let unicodes = unicodes_for_predefined_char_set(base_encoding);
            for code in 0u32..256 {
                if embedded {
                    self.base
                        .simple
                        .encoding_mut()
                        .set_unicode(code as u8, code);
                } else if let Some(name) =
                    adobe_char_name(FontEncoding::Builtin, &names, code as u8)
                {
                    let name = std::str::from_utf8(name).unwrap_or_default();
                    self.base
                        .simple
                        .encoding_mut()
                        .set_unicode(code as u8, unicode_from_adobe_name(name));
                } else if let Some(&unicode) = unicodes.get(code as usize) {
                    self.base
                        .simple
                        .encoding_mut()
                        .set_unicode(code as u8, u32::from(unicode));
                }
                let unicode = self
                    .base
                    .simple
                    .encoding()
                    .unicode_from_char_code(code as u8);
                self.base
                    .set_glyph_index(code as u8, face.get_char_index(unicode) as u16);
            }
            if self.base.has_any_glyph_index() {
                return;
            }
        }
        for code in 0u32..256 {
            self.base.set_glyph_index(code as u8, code as u16);
        }
    }
}

pub(crate) fn adobe_char_name<'a>(
    encoding: FontEncoding,
    names: &'a [Vec<u8>],
    code: u8,
) -> Option<&'a [u8]> {
    if let Some(name) = names.get(usize::from(code)).filter(|name| !name.is_empty()) {
        return Some(name);
    }
    char_name_from_predefined_char_set(encoding, code).map(str::as_bytes)
}

pub(super) fn glyph_index_for_ms_symbol(face: &CfxFace<'_>, code: u32) -> u16 {
    for prefix in [0u32, 0xf0, 0xf1, 0xf2] {
        let glyph = face.get_char_index(prefix * 256 + code) as u16;
        if glyph != 0 {
            return glyph;
        }
    }
    0
}

pub(super) fn use_tt_charmap(face: &CfxFace<'_>, target: CharMapIdPair) -> bool {
    for index in 0..face.charmap_count() {
        if face.charmap_id_pair_by_index(index) == target {
            return face.set_charmap(Some(index));
        }
    }
    false
}

pub(super) fn use_tt_charmap_unicode(face: &CfxFace<'_>) -> bool {
    let mut unicode_index = None;
    let mut has_symbol = false;
    for index in 0..face.charmap_count() {
        let pair = face.charmap_id_pair_by_index(index);
        if pair == WINDOWS_UNICODE {
            return face.set_charmap(Some(index));
        }
        if pair == WINDOWS_SYMBOL {
            has_symbol = true;
            continue;
        }
        if unicode_index.is_none()
            && charmap_id_pair_to_font_encoding(pair) == FxFontEncoding::Unicode
        {
            unicode_index = Some(index);
        }
    }
    if !has_symbol {
        if let Some(index) = unicode_index {
            return face.set_charmap(Some(index));
        }
    }
    false
}

fn determine_charmap_type(face: &CfxFace<'_>, flags: u32) -> CharmapType {
    if use_tt_charmap_unicode(face) {
        return CharmapType::MsUnicode;
    }
    let targets = if font_style_is_non_symbolic(flags) {
        [
            (MAC_ROMAN, CharmapType::MacRoman),
            (WINDOWS_SYMBOL, CharmapType::MsSymbol),
        ]
    } else {
        [
            (WINDOWS_SYMBOL, CharmapType::MsSymbol),
            (MAC_ROMAN, CharmapType::MacRoman),
        ]
    };
    for (target, kind) in targets {
        if use_tt_charmap(face, target) {
            return kind;
        }
    }
    CharmapType::Other
}

fn determine_encoding(
    face: &CfxFace<'_>,
    base: FontEncoding,
    flags: u32,
    embedded: bool,
) -> FontEncoding {
    if !embedded
        || !font_style_is_symbolic(flags)
        || !matches!(base, FontEncoding::WinAnsi | FontEncoding::MacRoman)
        || face.charmap_count() == 0
    {
        return base;
    }
    let mut win = false;
    let mut mac = false;
    for index in 0..face.charmap_count() {
        match face.charmap_id_pair_by_index(index).platform_id {
            PLATFORM_APPLE_UNICODE | PLATFORM_WINDOWS => win = true,
            PLATFORM_MAC => mac = true,
            _ => {}
        }
        if win && mac {
            break;
        }
    }
    if base == FontEncoding::WinAnsi && !win {
        return if mac {
            FontEncoding::MacRoman
        } else {
            FontEncoding::Builtin
        };
    }
    if base == FontEncoding::MacRoman && !mac {
        return if win {
            FontEncoding::WinAnsi
        } else {
            FontEncoding::Builtin
        };
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::make_reference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fxge::fx_font::{FONT_STYLE_NON_SYMBOLIC, FONT_STYLE_SYMBOLIC};

    #[test]
    fn embedded_post_names_without_cmap_do_not_use_sequential_glyph_ids() {
        use read_fonts::types::Tag;
        use skrifa::FontRef;

        let original = include_bytes!("../../../tests/fixtures/hint-optional.ttf");
        let source = FontRef::new(original).unwrap();
        let cmap = source
            .table_directory()
            .table_records()
            .iter()
            .position(|r| r.tag.get() == Tag::new(b"cmap"))
            .unwrap();
        let mut bytes = original.to_vec();
        bytes[12 + cmap * 16..16 + cmap * 16].copy_from_slice(b"CMAP");
        let face = CfxFace::new(&bytes, 0).unwrap();
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        let mut font = CpdfTrueTypeFont::default();
        font.base.simple.set_base_encoding(FontEncoding::WinAnsi);
        font.load_glyph_map(&face, &dict, FONT_STYLE_NON_SYMBOLIC, true, None);
        assert_eq!(font.base.glyph_from_char_code(65), 1); // not FirstChar + 3
        assert_eq!(font.base.glyph_from_char_code(66), 0);
        assert_eq!(face.current_charmap_encoding(), FxFontEncoding::Unicode);
        assert!(face.load_glyph_path(1, 0, false, None).is_some());
    }

    #[test]
    fn unicode_lookup_falls_back_per_character_to_mac_roman_without_changing_map() {
        use read_fonts::types::Tag;
        use skrifa::FontRef;

        let original = include_bytes!("../../../tests/fixtures/hint-optional.ttf");
        let source = FontRef::new(original).unwrap();
        let records = source.table_directory().table_records();
        let cmap_record = records
            .iter()
            .position(|r| r.tag.get() == Tag::new(b"cmap"))
            .unwrap();
        let post = records
            .iter()
            .find(|r| r.tag.get() == Tag::new(b"post"))
            .unwrap()
            .offset() as usize;
        for (format2, with_mac) in [(false, true), (true, true), (true, false)] {
            // Two usable outlines, no glyph names to rescue a failed lookup.
            let mut bytes = original.to_vec();
            bytes[post..post + 4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
            let mut unicode: Vec<u8> = if format2 {
                // Valid format 2: all one-byte codes use subheader zero.
                let mut table = vec![0; 526 + 512];
                table[..2].copy_from_slice(&2u16.to_be_bytes());
                table[2..4].copy_from_slice(&1038u16.to_be_bytes());
                table[520..522].copy_from_slice(&256u16.to_be_bytes());
                table[524..526].copy_from_slice(&2u16.to_be_bytes());
                table[526 + 2 * 65..528 + 2 * 65].copy_from_slice(&1u16.to_be_bytes());
                table
            } else {
                // Format 4 maps A to glyph 1; B and eacute are absent.
                [
                    4u16, 32, 0, 4, 4, 1, 0, 65, 65535, 0, 65, 65535, 65472, 1, 0, 0,
                ]
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect()
            };
            let unicode_offset = if with_mac { 20 } else { 12 };
            let mac_offset = unicode_offset + unicode.len();
            let mut cmap: Vec<u8> = [0u16, if with_mac { 2 } else { 1 }, 3, 1]
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect();
            cmap.extend_from_slice(&(unicode_offset as u32).to_be_bytes());
            if with_mac {
                cmap.extend_from_slice(&1u16.to_be_bytes());
                cmap.extend_from_slice(&0u16.to_be_bytes());
                cmap.extend_from_slice(&(mac_offset as u32).to_be_bytes());
            }
            cmap.append(&mut unicode);
            if with_mac {
                cmap.extend_from_slice(&[0, 0, 1, 6, 0, 0]); // format 0, length 262
                let mut mac = [0u8; 256];
                mac[65] = u8::from(format2);
                mac[66] = 1;
                mac[0x8e] = 1; // Mac Roman eacute; PDF WinAnsi uses 0xe9.
                cmap.extend_from_slice(&mac);
            }
            let record = 12 + cmap_record * 16;
            let offset = bytes.len() as u32;
            bytes[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
            bytes[record + 12..record + 16].copy_from_slice(&(cmap.len() as u32).to_be_bytes());
            bytes.extend_from_slice(&cmap);

            let face = CfxFace::new(&bytes, 0).unwrap();
            assert_eq!(face.name_index("B"), 0);
            let mut font = CpdfTrueTypeFont::default();
            font.base.simple.set_base_encoding(FontEncoding::WinAnsi);
            font.load_glyph_map(
                &face,
                &CpdfDictionary::new(),
                FONT_STYLE_NON_SYMBOLIC,
                true,
                None,
            );
            assert_eq!(face.current_charmap(), Some(0));
            for code in [65, 66, 0xe9] {
                assert_eq!(
                    font.base.glyph_from_char_code(code),
                    i32::from(code == 65 || with_mac),
                    "code={code}, format2={format2}, with_mac={with_mac}"
                );
            }
            assert_eq!(font.base.glyph_from_char_code(67), 0);
            assert_eq!(
                font.base.simple.encoding().unicode_from_char_code(0xe9),
                0xe9
            );
            assert!(face.load_glyph_path(1, 0, false, None).is_some());
        }
    }

    #[test]
    fn embedded_font_load_connects_descriptor_encoding_and_face() {
        let bytes =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let document = CpdfDocument::new();
        let holder = document.holder();
        let stream = holder.borrow_mut().new_indirect_stream_from_bytes(bytes);
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"TrueType");
        dict.set_new_name_for(b"BaseFont", b"Ahem");
        dict.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
        let descriptor = dict.set_new_dictionary_for(b"FontDescriptor");
        {
            let mut borrowed = descriptor.borrow_mut();
            let descriptor = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            descriptor.set_new_for(
                b"Flags",
                CpdfScalarObject::number_from_signed(FONT_STYLE_NON_SYMBOLIC as i32),
            );
            descriptor.set_for(b"FontFile2", Some(make_reference(&stream, &holder)));
        }
        let mut font = CpdfTrueTypeFont::default();
        assert!(font.load_embedded(&document, &dict, b"Ahem").unwrap());
        assert_eq!(font.base.simple.base_encoding(), FontEncoding::WinAnsi);
        assert!(font.base.glyph_from_char_code(b'A' as u32) > 0);
        assert!(font.font_base.ascent > 0);
        assert!(font.face.is_some());
    }

    #[test]
    fn ahem_truetype_maps_winansi_and_uses_unicode_cmap() {
        let data =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let mut face = CfxFace::new(data, 0).unwrap();
        let mut font = CpdfTrueTypeFont::default();
        font.base.simple.set_base_encoding(FontEncoding::WinAnsi);
        font.load_glyph_map(
            &mut face,
            &CpdfDictionary::new(),
            FONT_STYLE_NON_SYMBOLIC,
            false,
            None,
        );
        assert_eq!(
            font.base.glyph_from_char_code(b'A' as u32),
            face.get_char_index(b'A' as u32) as i32
        );
        assert_eq!(
            font.base.simple.encoding().unicode_from_char_code(b'A'),
            b'A' as u32
        );
    }

    #[test]
    fn first_char_fallback_starts_at_glyph_three() {
        let mut font = CpdfTrueTypeFont::default();
        font.base.set_glyph_indices_from_first_char(65);
        assert_eq!(font.base.glyph_from_char_code(64), 0);
        assert_eq!(font.base.glyph_from_char_code(65), 3);
        assert_eq!(font.base.glyph_from_char_code(66), 4);
    }

    #[test]
    fn embedded_symbolic_font_uses_unicode_charmap_when_needed() {
        let data =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let mut face = CfxFace::new(data, 0).unwrap();
        let mut font = CpdfTrueTypeFont::default();
        font.load_glyph_map(
            &mut face,
            &CpdfDictionary::new(),
            FONT_STYLE_SYMBOLIC,
            true,
            None,
        );
        assert_eq!(
            font.base.glyph_from_char_code(b'A' as u32),
            face.get_char_index(b'A' as u32) as i32
        );
        assert_eq!(
            font.base.simple.encoding().unicode_from_char_code(b'A'),
            b'A' as u32
        );
    }
}

#[cfg(test)]
#[path = "cpdf_truetypefont_unittest.rs"]
mod upstream_tests;
