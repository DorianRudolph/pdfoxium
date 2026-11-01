// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2015 The PDFium Authors
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Portable/non-Windows `cfgas_fontmgr.{h,cpp}`. Enumeration uses the existing
//! Rust font provider; Fontations supplies SFNT metadata and glyph mappings.
use super::{
    cfgas_gefont::CfgasGeFont,
    fgas_fontutils::{get_unicode_bit_field, FgasFontUsb},
};
use crate::fxcrt::{bytestring::hash_code, fx_codepage::FxCodePage};
use crate::fxge::{
    cfx_face::{CfxFace, CfxOwnedFace},
    cfx_font::CfxFont,
    cfx_fontmgr::with_font_manager,
    fx_font::{
        font_style_is_fixed_pitch, font_style_is_force_bold, font_style_is_italic,
        font_style_is_serif, font_style_is_symbolic,
    },
    fx_fontencoding::FontEncoding,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, HashSet},
    rc::Rc,
};

type OwnedFace = CfxOwnedFace<Rc<Vec<u8>>>;
struct FontDescriptor {
    face_name: Vec<u32>,
    face_index: u32,
    font_styles: u32,
    usb: [u32; 4],
    csb: [u32; 2],
    family_names: Vec<Vec<u32>>,
    face: RefCell<Option<Rc<OwnedFace>>>,
}
#[derive(Default)]
pub struct CfgasFontMgr {
    fonts_enumerated: Cell<bool>,
    system_font_generation: Cell<u64>,
    installed_fonts: RefCell<Vec<Rc<FontDescriptor>>>,
    hash_to_candidates: RefCell<BTreeMap<u32, Rc<[usize]>>>,
    hash_to_fonts: RefCell<BTreeMap<u32, Vec<Rc<CfgasGeFont>>>>,
    failed_unicodes: RefCell<BTreeSet<u32>>,
}
fn bytes(name: &[u32]) -> Vec<u8> {
    name.iter()
        .filter_map(|&code| char::from_u32(code))
        .collect::<String>()
        .into_bytes()
}
fn wide(name: &[u8]) -> Vec<u32> {
    String::from_utf8_lossy(name)
        .chars()
        .map(u32::from)
        .collect()
}
fn short_hash(page: FxCodePage, styles: u32, family: &[u32]) -> u32 {
    let mut text = format!("{}, {}", page as u16, styles).into_bytes();
    text.extend(bytes(family));
    hash_code(&text)
}
fn long_hash(page: FxCodePage, bit: u16, styles: u32, family: &[u32]) -> u32 {
    let mut text = format!("{}, {}, {}", page as u16, bit, styles).into_bytes();
    text.extend(bytes(family));
    hash_code(&text)
}
fn font_bytes(name: &[u32]) -> Option<Vec<u8>> {
    let name = bytes(name);
    with_font_manager(|manager| {
        let mut mapper = manager.builtin_mapper_mut();
        mapper.load_installed_fonts();
        let index =
            (0..mapper.face_count()).find(|&i| mapper.face_name(i) == Some(name.as_slice()))?;
        let data = mapper.raw_bytes_for_index(index);
        (!data.is_empty()).then_some(data)
    })
}
fn table(face: &CfxFace<'_>, tag: [u8; 4]) -> Vec<u8> {
    let tag = u32::from_be_bytes(tag);
    let size = face.get_sfnt_table(tag, &mut []);
    let mut result = vec![0; size];
    if size > 0 && face.get_sfnt_table(tag, &mut result) != size {
        return Vec::new();
    }
    result
}
#[cfg(test)]
fn be16(data: &[u8], index: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(index..index.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn be32(data: &[u8], index: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(index..index.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn family_names(data: &[u8]) -> Vec<Vec<u32>> {
    crate::fxge::cfx_cttnametable::CfxCttNameTable::new(data)
        .family_names()
        .to_vec()
}

fn is_part_name(name: &[u32], part: &[u32]) -> bool {
    part.is_empty() || name.windows(part.len()).any(|chunk| chunk == part)
}
fn code_page_bit(page: FxCodePage) -> Option<usize> {
    CODE_PAGES.iter().position(|&candidate| candidate == page)
}
fn calc_penalty(
    font: &FontDescriptor,
    page: FxCodePage,
    styles: u32,
    name: &[u32],
    unicode: u32,
) -> i32 {
    let mut penalty = 30000;
    if !name.is_empty() {
        if name != font.face_name {
            if font.family_names.iter().any(|family| family == name) {
                penalty -= 28000;
            } else {
                penalty += 0xffff;
            }
        } else {
            penalty -= 30000;
        }
        if penalty == 30000 && !is_part_name(&font.face_name, name) {
            if font
                .family_names
                .iter()
                .any(|family| is_part_name(family, name))
            {
                penalty -= 26000;
            } else {
                penalty += 0xffff;
            }
        } else {
            penalty -= 27000;
        }
    }
    let changed = font.font_styles ^ styles;
    if font_style_is_force_bold(changed) {
        penalty += 4500;
    }
    if font_style_is_fixed_pitch(changed) {
        penalty += 10000;
    }
    if font_style_is_italic(changed) {
        penalty += 10000;
    }
    if font_style_is_serif(changed) {
        penalty += 500;
    }
    if font_style_is_symbolic(changed) {
        penalty += 0xffff;
    }
    if penalty >= 0xffff {
        return 0xffff;
    }
    if page != FxCodePage::DefAnsi && page != FxCodePage::Failure {
        if let Some(bit) = code_page_bit(page) {
            if font.csb[bit / 32] & (1u32 << (bit % 32)) == 0 {
                penalty += 0xffff;
            } else {
                penalty -= 60000;
            }
        }
    }
    if unicode != 0 && unicode != 0xfffe {
        if let Some(entry) = get_unicode_bit_field(unicode) {
            let bit = usize::from(entry.bit_field);
            if bit < 128 {
                if font.usb[bit / 32] & (1u32 << (bit % 32)) == 0 {
                    penalty += 0xffff;
                } else {
                    penalty -= 60000;
                }
            }
        }
    }
    penalty
}
impl FontDescriptor {
    fn verify_unicode(&self, unicode: u32) -> bool {
        if self.face.borrow().is_none() {
            let Some(data) = font_bytes(&self.face_name) else {
                return false;
            };
            let Ok(face) = OwnedFace::new(Rc::new(data), self.face_index) else {
                return false;
            };
            *self.face.borrow_mut() = Some(Rc::new(face));
        }
        let face = self.face.borrow();
        let face = face.as_ref().expect("loaded face").face();
        face.select_charmap(FontEncoding::Unicode) && face.get_char_index(unicode) != 0
    }
}
impl CfgasFontMgr {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn fonts_enumerated(&self) -> bool {
        self.fonts_enumerated.get()
    }
    pub fn ensure_fonts_enumerated(&self) {
        let generation = with_font_manager(|manager| manager.system_font_generation());
        if self.system_font_generation.replace(generation) != generation {
            self.fonts_enumerated.set(false);
            self.installed_fonts.borrow_mut().clear();
            self.hash_to_candidates.borrow_mut().clear();
            self.hash_to_fonts.borrow_mut().clear();
            self.failed_unicodes.borrow_mut().clear();
        }
        if !self.fonts_enumerated.replace(true) {
            self.enum_fonts_from_font_mapper();
        }
    }
    pub fn enum_fonts(&self) -> bool {
        self.enum_fonts_from_font_mapper()
    }
    fn enum_fonts_from_font_mapper(&self) -> bool {
        with_font_manager(|manager| {
            let mut mapper = manager.builtin_mapper_mut();
            mapper.load_installed_fonts();
            let mut seen = HashSet::new();
            for index in 0..mapper.face_count() {
                let face_name = mapper.face_name(index).unwrap_or_default();
                // The mapper enumerates a face once per supported charset.
                // RawBytesForIndex maps by name, so those entries yield the
                // same stream and descriptors. Keep their first occurrence.
                if !seen.insert(face_name.to_vec()) {
                    continue;
                }
                let name = wide(face_name);
                let data = mapper.raw_bytes_for_index(index);
                if !data.is_empty() {
                    self.register_faces(&data, &name);
                }
            }
        });
        !self.installed_fonts.borrow().is_empty()
    }
    fn register_faces(&self, data: &[u8], name: &[u32]) {
        let count = if data.starts_with(b"ttcf") {
            be32(data, 8).unwrap_or(0)
        } else {
            1
        };
        // Every face directory needs at least four bytes in the input. This
        // input-derived bound avoids malicious collection counts causing a loop.
        for index in 0..count.min((data.len() / 4) as u32) {
            let Ok(face) = CfxFace::new(data, index) else {
                continue;
            };
            let os2 = table(&face, *b"OS/2");
            let usb = std::array::from_fn(|i| be32(&os2, 42 + 4 * i).unwrap_or(0));
            let mut names = family_names(&table(&face, *b"name"));
            names.push(face.family_name().chars().map(u32::from).collect());
            self.installed_fonts
                .borrow_mut()
                .push(Rc::new(FontDescriptor {
                    face_name: name.to_vec(),
                    face_index: index,
                    font_styles: face.font_style(),
                    usb,
                    csb: face.os2_code_page_range().unwrap_or([0; 2]),
                    family_names: names,
                    face: RefCell::new(None),
                }));
        }
    }
    fn match_fonts(&self, page: FxCodePage, styles: u32, name: &[u32], unicode: u32) -> Vec<usize> {
        self.ensure_fonts_enumerated();
        let installed = self.installed_fonts.borrow();
        let mut matches = Vec::new();
        for (index, font) in installed.iter().enumerate() {
            let penalty = calc_penalty(font, page, styles, name, unicode);
            if penalty >= 0xffff {
                continue;
            }
            matches.push((index, penalty));
            if matches.len() == 0xffff {
                break;
            }
        }
        matches.sort_by_key(|&(_, penalty)| penalty);
        matches.into_iter().map(|(index, _)| index).collect()
    }
    fn candidates(
        &self,
        hash: u32,
        page: FxCodePage,
        styles: u32,
        name: &[u32],
        unicode: u32,
    ) -> Rc<[usize]> {
        if let Some(candidates) = self.hash_to_candidates.borrow().get(&hash) {
            return candidates.clone();
        }
        let candidates: Rc<[usize]> = self.match_fonts(page, styles, name, unicode).into();
        self.hash_to_candidates
            .borrow_mut()
            .insert(hash, candidates.clone());
        candidates
    }
    fn load_font_internal(self: &Rc<Self>, descriptor: &FontDescriptor) -> Option<Rc<CfgasGeFont>> {
        let data = Rc::new(font_bytes(&descriptor.face_name)?);
        let face = Rc::new(OwnedFace::new(data, descriptor.face_index).ok()?);
        let font = CfxFont::from_retained_face(face, false, 0);
        CfgasGeFont::from_font_with_manager(font, Rc::downgrade(self))
    }
    pub fn get_font_by_code_page(
        self: &Rc<Self>,
        page: FxCodePage,
        styles: u32,
        family: Option<&[u32]>,
    ) -> Option<Rc<CfgasGeFont>> {
        self.ensure_fonts_enumerated();
        let name = family.unwrap_or_default();
        let hash = short_hash(page, styles, name);
        if let Some(font) = self
            .hash_to_fonts
            .borrow()
            .get(&hash)
            .and_then(|fonts| fonts.first())
            .cloned()
        {
            return Some(font);
        }
        let candidates = self.candidates(hash, page, styles, name, 0);
        let index = *candidates.first()?;
        let descriptor = self.installed_fonts.borrow().get(index)?.clone();
        let font = self.load_font_internal(&descriptor)?;
        font.set_logical_font_style(styles);
        self.hash_to_fonts
            .borrow_mut()
            .entry(hash)
            .or_default()
            .push(font.clone());
        Some(font)
    }
    pub fn get_font_by_unicode(
        self: &Rc<Self>,
        unicode: u32,
        styles: u32,
        family: Option<&[u32]>,
    ) -> Option<Rc<CfgasGeFont>> {
        self.ensure_fonts_enumerated();
        if self.failed_unicodes.borrow().contains(&unicode) {
            return None;
        }
        let range = get_unicode_bit_field(unicode);
        let page = range.map_or(FxCodePage::Failure, |entry| entry.code_page);
        let bit = range.map_or(FgasFontUsb::NO_BIT_FIELD, |entry| entry.bit_field);
        let name = family.unwrap_or_default();
        let hash = if page == FxCodePage::Failure {
            long_hash(page, bit, styles, name)
        } else {
            short_hash(page, styles, name)
        };
        {
            // Verification reads only the face's charmap; it cannot reenter the
            // FGAS manager. Borrow this list instead of allocating a clone for
            // each new character. Drop it before loading/inserting a font.
            let cached = self.hash_to_fonts.borrow();
            if let Some(fonts) = cached.get(&hash) {
                for font in fonts {
                    if font.verify_unicode(unicode) {
                        return Some(font.clone());
                    }
                }
            }
        }
        for index in self
            .candidates(hash, page, styles, name, unicode)
            .iter()
            .copied()
        {
            let descriptor = self.installed_fonts.borrow().get(index)?.clone();
            if !descriptor.verify_unicode(unicode) {
                continue;
            }
            let Some(font) = self.load_font_internal(&descriptor) else {
                continue;
            };
            font.set_logical_font_style(styles);
            self.hash_to_fonts
                .borrow_mut()
                .entry(hash)
                .or_default()
                .push(font.clone());
            return Some(font);
        }
        if family.is_none() {
            self.failed_unicodes.borrow_mut().insert(unicode);
        }
        None
    }
    pub fn load_font(
        self: &Rc<Self>,
        family: Option<&[u32]>,
        styles: u32,
        page: FxCodePage,
    ) -> Option<Rc<CfgasGeFont>> {
        self.get_font_by_code_page(page, styles, family)
    }
}

const CODE_PAGES: &[FxCodePage] = &[
    FxCodePage::MswinWesternEuropean,
    FxCodePage::MswinEasternEuropean,
    FxCodePage::MswinCyrillic,
    FxCodePage::MswinGreek,
    FxCodePage::MswinTurkish,
    FxCodePage::MswinHebrew,
    FxCodePage::MswinArabic,
    FxCodePage::MswinBaltic,
    FxCodePage::MswinVietnamese,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::MsdosThai,
    FxCodePage::ShiftJis,
    FxCodePage::ChineseSimplified,
    FxCodePage::Hangul,
    FxCodePage::ChineseTraditional,
    FxCodePage::Johab,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::DefAnsi,
    FxCodePage::MsdosGreek2,
    FxCodePage::MsdosRussian,
    FxCodePage::MsdosNorwegian,
    FxCodePage::MsdosArabic,
    FxCodePage::MsdosFrenchCanadian,
    FxCodePage::MsdosHebrew,
    FxCodePage::MsdosIcelandic,
    FxCodePage::MsdosPortuguese,
    FxCodePage::MsdosTurkish,
    FxCodePage::MsdosCyrillic,
    FxCodePage::MsdosEasternEuropean,
    FxCodePage::MsdosBaltic,
    FxCodePage::MsdosGreek1,
    FxCodePage::ArabicAsmo708,
    FxCodePage::MsdosWesternEuropean,
    FxCodePage::MsdosUs,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::fx_codepage::FxCharset;
    use crate::fxge::systemfontinfo_iface::{SystemFontInfo, TABLE_NONE};
    const AHEM: &[u8] =
        include_bytes!("../../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
    fn name(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }
    #[test]
    fn lazy_enumeration() {
        let manager = CfgasFontMgr::new();
        assert!(!manager.fonts_enumerated());
        manager.ensure_fonts_enumerated();
        assert!(manager.fonts_enumerated());
        let count = manager.installed_fonts.borrow().len();
        manager.ensure_fonts_enumerated();
        assert!(manager.fonts_enumerated());
        assert_eq!(manager.installed_fonts.borrow().len(), count);
    }
    #[test]
    fn retained_font_manager_refreshes_after_graphics_module_replacement() {
        use crate::fxge::cfx_gemodule::{destroy_ge_module, initialize_ge_module};
        use std::path::PathBuf;
        destroy_ge_module();
        initialize_ge_module(Some(Vec::new()));
        let manager = Rc::new(CfgasFontMgr::new());
        assert!(manager
            .load_font(Some(&name("Ahem")), 0, FxCodePage::DefAnsi)
            .is_none());
        destroy_ge_module();
        initialize_ge_module(Some(vec![PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem"
        ))]));
        assert!(manager
            .load_font(Some(&name("Ahem")), 0, FxCodePage::DefAnsi)
            .is_some());
        destroy_ge_module();
    }
    #[test]
    fn family_table_offsets_bounds_and_ranking() {
        // A single Windows UTF-16 family entry, whose name ID is six bytes
        // into its record (the native helper accidentally reads the platform).
        let mut table = vec![0, 0, 0, 1, 0, 18, 0, 3, 0, 1, 4, 9, 0, 1, 0, 8, 0, 0];
        table.extend_from_slice(&[0, b'T', 0, b'e', 0, b's', 0, b't']);
        assert_eq!(family_names(&table), vec![name("Test")]);
        table.truncate(table.len() - 1);
        assert!(family_names(&table).is_empty());
        for length in 0..18 {
            assert!(family_names(&table[..length]).is_empty());
        }
        let font = FontDescriptor {
            face_name: name("Face"),
            face_index: 0,
            font_styles: 0,
            usb: [1, 0, 0, 0],
            csb: [1, 0],
            family_names: vec![name("Family")],
            face: RefCell::new(None),
        };
        assert!(
            calc_penalty(&font, FxCodePage::DefAnsi, 0, &name("Face"), 0)
                < calc_penalty(&font, FxCodePage::DefAnsi, 0, &name("Family"), 0)
        );
        assert_eq!(
            calc_penalty(&font, FxCodePage::DefAnsi, 0, &name("Missing"), 0),
            0xffff
        );
        assert!(calc_penalty(&font, FxCodePage::MswinWesternEuropean, 0, &[], 0) < 0);
        assert_eq!(code_page_bit(FxCodePage::MswinWesternEuropean), Some(0));
        assert_eq!(code_page_bit(FxCodePage::Utf8), None);
    }
    struct Fonts {
        greek: Vec<u8>,
        reads: Rc<Cell<usize>>,
    }
    impl SystemFontInfo for Fonts {
        fn enum_font_list(&mut self) -> Vec<(Vec<u8>, FxCharset)> {
            vec![
                (b"Ahem".to_vec(), FxCharset::Ansi),
                (b"Greek Ahem".to_vec(), FxCharset::MswinGreek),
                (b"Ahem".to_vec(), FxCharset::MswinEasternEuropean),
                (b"Greek Ahem".to_vec(), FxCharset::Ansi),
                (b"Ahem".to_vec(), FxCharset::Ansi),
            ]
        }
        fn map_font(&mut self, _: i32, _: bool, _: FxCharset, _: u32, face: &[u8]) -> Option<u64> {
            self.get_font(face)
        }
        fn get_font(&mut self, face: &[u8]) -> Option<u64> {
            match face {
                b"Ahem" => Some(1),
                b"Greek Ahem" => Some(2),
                _ => None,
            }
        }
        fn get_font_data(&mut self, font: u64, table: u32, buffer: &mut [u8]) -> usize {
            if table != TABLE_NONE {
                return 0;
            }
            let data = if font == 1 {
                AHEM
            } else {
                self.greek.as_slice()
            };
            if buffer.is_empty() {
                return data.len();
            }
            if buffer.len() < data.len() {
                return 0;
            }
            self.reads.set(self.reads.get() + 1);
            buffer[..data.len()].copy_from_slice(data);
            data.len()
        }
        fn get_face_name(&mut self, font: u64) -> Option<Vec<u8>> {
            Some(if font == 1 {
                b"Ahem".to_vec()
            } else {
                b"Greek Ahem".to_vec()
            })
        }
        fn get_font_charset(&mut self, font: u64) -> Option<FxCharset> {
            Some(if font == 1 {
                FxCharset::Ansi
            } else {
                FxCharset::MswinGreek
            })
        }
        fn delete_font(&mut self, _: u64) {}
    }
    #[test]
    fn duplicate_charsets_read_each_font_once_and_preserve_descriptor_order() {
        let reads = Rc::new(Cell::new(0));
        let previous =
            with_font_manager(|manager| manager.builtin_mapper_mut().take_system_font_info());
        with_font_manager(|manager| {
            manager
                .builtin_mapper_mut()
                .set_system_font_info(Some(Box::new(Fonts {
                    greek: greek_ahem(),
                    reads: reads.clone(),
                })))
        });
        let manager = CfgasFontMgr::new();
        manager.ensure_fonts_enumerated();
        assert_eq!(reads.get(), 2);
        let descriptors = manager.installed_fonts.borrow();
        assert_eq!(descriptors.len(), 2);
        assert_eq!(descriptors[0].face_name, name("Ahem"));
        assert_eq!(descriptors[1].face_name, name("Greek Ahem"));
        drop(descriptors);
        manager.ensure_fonts_enumerated();
        assert_eq!(reads.get(), 2);
        with_font_manager(|manager| {
            manager.builtin_mapper_mut().take_system_font_info();
            manager.builtin_mapper_mut().set_system_font_info(previous);
        });
    }
    fn greek_ahem() -> Vec<u8> {
        // Keep genuine Ahem outlines/metrics; add format-12 Unicode mappings
        // for alpha/beta and 256 CJK characters. No external fonts are needed.
        let face = CfxFace::new(AHEM, 0).unwrap();
        let glyph = face.unicode_char_index(u32::from('A'));
        assert_ne!(glyph, 0);
        let mut data = AHEM.to_vec();
        let count = usize::from(be16(&data, 4).unwrap());
        let record = (0..count)
            .map(|i| 12 + i * 16)
            .find(|&i| data.get(i..i + 4) == Some(b"cmap"))
            .unwrap();
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let start = data.len();
        let mut cmap = vec![0, 0, 0, 1, 0, 3, 0, 10, 0, 0, 0, 12, 0, 12, 0, 0];
        for value in [40u32, 0, 2, 0x3b1, 0x3b2, glyph, 0x4f00, 0x4fff, 1] {
            cmap.extend_from_slice(&value.to_be_bytes());
        }
        data[record + 8..record + 12].copy_from_slice(&(start as u32).to_be_bytes());
        data[record + 12..record + 16].copy_from_slice(&(cmap.len() as u32).to_be_bytes());
        data.extend(cmap);
        let os2 = (0..count)
            .map(|i| 12 + i * 16)
            .find(|&i| data.get(i..i + 4) == Some(b"OS/2"))
            .unwrap();
        let offset = be32(&data, os2 + 8).unwrap() as usize;
        let usb = be32(&data, offset + 42).unwrap() | (1 << 7);
        data[offset + 42..offset + 46].copy_from_slice(&usb.to_be_bytes());
        let usb = be32(&data, offset + 46).unwrap() | (1 << 27);
        data[offset + 46..offset + 50].copy_from_slice(&usb.to_be_bytes());
        let csb = be32(&data, offset + 78).unwrap() | (1 << 3) | (1 << 18);
        data[offset + 78..offset + 82].copy_from_slice(&csb.to_be_bytes());
        data
    }
    #[test]
    fn packed_substitution_metrics_and_weak_wrapper_lifetimes() {
        let previous =
            with_font_manager(|manager| manager.builtin_mapper_mut().take_system_font_info());
        with_font_manager(|manager| {
            manager
                .builtin_mapper_mut()
                .set_system_font_info(Some(Box::new(Fonts {
                    greek: greek_ahem(),
                    reads: Default::default(),
                })))
        });
        let manager = Rc::new(CfgasFontMgr::new());
        let base = manager
            .load_font(Some(&name("Ahem")), 0, FxCodePage::DefAnsi)
            .unwrap();
        assert!(!base.verify_unicode(0x3b1));
        let packed = base.get_glyph_index(0x3b1);
        assert_eq!((packed as u32) >> 24, 1);
        assert_eq!((base.get_glyph_index(0x3b2) as u32) >> 24, 1);
        // A single substitute keeps its ID for all 256 characters instead of
        // exhausting the packed font index. Different code-page selections
        // retain their distinct manager-owned font resources.
        for unicode in 0x4f00..=0x4fff {
            assert!(!base.verify_unicode(unicode), "base maps U+{unicode:04X}");
            assert_eq!(
                (base.get_glyph_index(unicode) as u32) >> 24,
                2,
                "U+{unicode:04X}"
            );
        }
        let fallback = base.get_subst_font(packed).unwrap();
        assert!(fallback.verify_unicode(0x3b1));
        let width = base.get_char_width(0x3b1).unwrap();
        assert_eq!(fallback.get_char_width(0x3b1), Some(width));
        let bbox = base.get_char_bbox(0x3b1).unwrap();
        assert_eq!(fallback.get_char_bbox(0x3b1), Some(bbox));
        let weak = Rc::downgrade(&fallback);
        drop(fallback);
        drop(manager);
        assert!(weak.upgrade().is_none());
        // The packed index still retains its resource after manager/wrapper
        // teardown, and recreating a small wrapper cannot create a cycle.
        let fallback = base.get_subst_font(packed).unwrap();
        assert_eq!(fallback.get_char_width(0x3b1), Some(width));
        assert_eq!(fallback.get_char_bbox(0x3b1), Some(bbox));
        let weak_base = Rc::downgrade(&base);
        drop(base);
        assert!(weak_base.upgrade().is_none());
        with_font_manager(|manager| {
            manager.builtin_mapper_mut().take_system_font_info();
            manager.builtin_mapper_mut().set_system_font_info(previous);
        });
    }
}
