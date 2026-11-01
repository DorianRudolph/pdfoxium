// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Shared folder font scanning and matching from
//! `core/fxge/cfx_folderfontinfo.{h,cpp}`.
//! The shared provider returns no result from `MapFont`; platform subclasses
//! supply that policy. Direct installed face lookup is available here.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::fxcrt::fx_codepage::{charset_flag, FxCharset};
use crate::fxge::fx_font::{
    find_font_table_location, font_family_is_fixed_pitch, font_family_is_roman,
    font_family_is_script, font_style_is_fixed_pitch, font_style_is_force_bold,
    font_style_is_italic, font_style_is_script, font_style_is_serif, get_code_page_range_from_os2,
    get_glyph_count_from_maxp, get_name_from_tt, FONT_STYLE_FORCE_BOLD, FONT_STYLE_ITALIC,
    FONT_STYLE_SERIF,
};
use crate::fxge::systemfontinfo_iface::{SystemFontInfo, TABLE_NAME, TABLE_NONE, TABLE_TTCF};

const TABLE_OS2: u32 = u32::from_be_bytes(*b"OS/2");
const TABLE_MAXP: u32 = u32::from_be_bytes(*b"maxp");
const CODEPAGE_CHARSETS: &[(u32, FxCharset)] = &[
    (1, FxCharset::MswinEasternEuropean),
    (2, FxCharset::MswinCyrillic),
    (3, FxCharset::MswinGreek),
    (4, FxCharset::MswinTurkish),
    (5, FxCharset::MswinHebrew),
    (6, FxCharset::MswinArabic),
    (7, FxCharset::MswinBaltic),
    (8, FxCharset::MswinVietnamese),
    (16, FxCharset::Thai),
    (17, FxCharset::ShiftJis),
    (18, FxCharset::ChineseSimplified),
    (19, FxCharset::Hangul),
    (20, FxCharset::ChineseTraditional),
    (21, FxCharset::Johab),
    (30, FxCharset::Oem),
    (31, FxCharset::Symbol),
];
const CHARSET_PRIORITY: &[FxCharset] = &[
    FxCharset::Symbol,
    FxCharset::ShiftJis,
    FxCharset::ChineseSimplified,
    FxCharset::ChineseTraditional,
    FxCharset::Hangul,
    FxCharset::Johab,
    FxCharset::MswinCyrillic,
    FxCharset::MswinGreek,
    FxCharset::MswinTurkish,
    FxCharset::MswinHebrew,
    FxCharset::MswinArabic,
    FxCharset::MswinBaltic,
    FxCharset::MswinVietnamese,
    FxCharset::Thai,
    FxCharset::MswinEasternEuropean,
    FxCharset::Oem,
    FxCharset::Ansi,
];

const BASE14_SUBSTS: &[(&[u8], &[u8])] = &[
    (b"Courier", b"Courier New"),
    (b"Courier-Bold", b"Courier New Bold"),
    (b"Courier-BoldOblique", b"Courier New Bold Italic"),
    (b"Courier-Oblique", b"Courier New Italic"),
    (b"Helvetica", b"Arial"),
    (b"Helvetica-Bold", b"Arial Bold"),
    (b"Helvetica-BoldOblique", b"Arial Bold Italic"),
    (b"Helvetica-Oblique", b"Arial Italic"),
    (b"Times-Roman", b"Times New Roman"),
    (b"Times-Bold", b"Times New Roman Bold"),
    (b"Times-BoldItalic", b"Times New Roman Bold Italic"),
    (b"Times-Italic", b"Times New Roman Italic"),
];

pub(super) fn base14_subst_name(name: &[u8]) -> Option<&'static [u8]> {
    BASE14_SUBSTS
        .iter()
        .find(|(base, _)| *base == name)
        .map(|(_, subst)| *subst)
}

pub fn find_family_name_match(family: &[u8], installed_name: &[u8]) -> bool {
    if family.is_empty() {
        return false;
    }
    let Some(position) = installed_name
        .windows(family.len())
        .position(|part| part == family)
    else {
        return false;
    };
    let next = position + family.len();
    !installed_name.get(next).is_some_and(u8::is_ascii_lowercase)
}

pub struct FontFaceInfo {
    pub file_path: PathBuf,
    pub face_name: Vec<u8>,
    pub font_tables: Vec<u8>,
    pub font_offset: u32,
    pub file_size: u32,
    pub styles: u32,
    pub charsets: u32,
    pub glyph_count: u32,
}

impl FontFaceInfo {
    pub const SIMILARITY_SCORE_MAX: i32 = 68;

    pub fn new(
        file_path: PathBuf,
        face_name: Vec<u8>,
        font_tables: Vec<u8>,
        font_offset: u32,
        file_size: u32,
    ) -> Self {
        Self {
            file_path,
            face_name,
            font_tables,
            font_offset,
            file_size,
            styles: 0,
            charsets: 0,
            glyph_count: 0,
        }
    }

    pub fn is_eligible_for_find_font(&self, charset: FxCharset) -> bool {
        self.charsets & charset_flag(charset) != 0 || charset == FxCharset::Default
    }

    pub fn similarity_score(
        &self,
        weight: i32,
        italic: bool,
        pitch_family: u32,
        exact_match_bonus: bool,
    ) -> i32 {
        let mut score = 0;
        if font_style_is_force_bold(self.styles) == (weight > 400) {
            score += 16;
        }
        if font_style_is_italic(self.styles) == italic {
            score += 16;
        }
        if font_style_is_serif(self.styles) == font_family_is_roman(pitch_family) {
            score += 16;
        }
        if font_style_is_script(self.styles) == font_family_is_script(pitch_family as i32) {
            score += 8;
        }
        if font_style_is_fixed_pitch(self.styles) == font_family_is_fixed_pitch(pitch_family) {
            score += 8;
        }
        if exact_match_bonus {
            score += 4;
        }
        score
    }
}

#[derive(Default)]
pub struct CfxFolderFontInfo {
    font_list: BTreeMap<Vec<u8>, FontFaceInfo>,
    font_names: Vec<Vec<u8>>,
    handle_by_name: BTreeMap<Vec<u8>, u64>,
    path_list: Vec<PathBuf>,
}

impl CfxFolderFontInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_path(&mut self, path: PathBuf) {
        self.path_list.push(path);
    }

    pub fn enum_font_list(&mut self) -> Vec<(Vec<u8>, FxCharset)> {
        let mut installed = Vec::new();
        for path in self.path_list.clone() {
            self.scan_path(&path, &mut installed);
        }
        installed
    }

    fn scan_path(&mut self, path: &PathBuf, installed: &mut Vec<(Vec<u8>, FxCharset)>) {
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let full_path = entry.path();
            if full_path.is_dir() {
                self.scan_path(&full_path, installed);
                continue;
            }
            let is_font = full_path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "ttc" | "otf")
                });
            if is_font {
                self.scan_file(full_path, installed);
            }
        }
    }

    fn scan_file(&mut self, path: PathBuf, installed: &mut Vec<(Vec<u8>, FxCharset)>) {
        let Ok(mut file) = File::open(&path) else {
            return;
        };
        let Ok(file_size) = file.metadata().map(|meta| meta.len()) else {
            return;
        };
        let Ok(file_size) = u32::try_from(file_size) else {
            return;
        };
        let mut header = [0u8; 12];
        if file.read_exact(&mut header).is_err() {
            return;
        }
        if u32::from_be_bytes(header[..4].try_into().unwrap()) != TABLE_TTCF {
            self.report_face(path, &mut file, file_size, 0, installed);
            return;
        }
        let face_count = u32::from_be_bytes(header[8..12].try_into().unwrap());
        let Some(offsets_size) = face_count.checked_mul(4) else {
            return;
        };
        if u64::from(offsets_size) > u64::from(file_size).saturating_sub(12) {
            return;
        }
        let mut offsets = vec![0; offsets_size as usize];
        if file.read_exact(&mut offsets).is_err() {
            return;
        }
        for chunk in offsets.chunks_exact(4) {
            let offset = u32::from_be_bytes(chunk.try_into().unwrap());
            self.report_face(path.clone(), &mut file, file_size, offset, installed);
        }
    }

    fn report_face(
        &mut self,
        path: PathBuf,
        file: &mut File,
        file_size: u32,
        offset: u32,
        installed: &mut Vec<(Vec<u8>, FxCharset)>,
    ) {
        if file.seek(SeekFrom::Start(u64::from(offset))).is_err() {
            return;
        }
        let mut header = [0u8; 12];
        if file.read_exact(&mut header).is_err() {
            return;
        }
        let tables_count = usize::from(u16::from_be_bytes([header[4], header[5]]));
        let mut tables = vec![0; tables_count * 16];
        if file.read_exact(&mut tables).is_err() {
            return;
        }
        if tables.is_empty() {
            return;
        }
        let Some(name_loc) = find_font_table_location(&tables, TABLE_NAME) else {
            return;
        };
        let Some(name_data) = read_table_at(file, file_size, name_loc.offset, name_loc.size) else {
            return;
        };
        let mut face_name = get_name_from_tt(&name_data, 1);
        if face_name.is_empty() {
            return;
        }
        let style = get_name_from_tt(&name_data, 2);
        if style != b"Regular" {
            face_name.push(b' ');
            face_name.extend_from_slice(&style);
        }
        if self.font_list.contains_key(&face_name) {
            return;
        }
        let mut info =
            FontFaceInfo::new(path, face_name.clone(), tables.clone(), offset, file_size);
        let codepages = find_font_table_location(&tables, TABLE_OS2)
            .and_then(|loc| read_table_at(file, file_size, loc.offset, loc.size))
            .map_or(0, |data| get_code_page_range_from_os2(&data));
        for &(bit, charset) in CODEPAGE_CHARSETS {
            if codepages & (1 << bit) != 0 {
                installed.push((face_name.clone(), charset));
                info.charsets |= charset_flag(charset);
            }
        }
        info.glyph_count = find_font_table_location(&tables, TABLE_MAXP)
            .and_then(|loc| read_table_at(file, file_size, loc.offset, loc.size))
            .map_or(0, |data| u32::from(get_glyph_count_from_maxp(&data)));
        installed.push((face_name.clone(), FxCharset::Ansi));
        info.charsets |= charset_flag(FxCharset::Ansi);
        if style.windows(4).any(|part| part == b"Bold") {
            info.styles |= FONT_STYLE_FORCE_BOLD;
        }
        if style.windows(6).any(|part| part == b"Italic")
            || style.windows(7).any(|part| part == b"Oblique")
        {
            info.styles |= FONT_STYLE_ITALIC;
        }
        if face_name.windows(5).any(|part| part == b"Serif") {
            info.styles |= FONT_STYLE_SERIF;
        }
        let handle = self.font_names.len() as u64 + 1;
        self.font_names.push(face_name.clone());
        self.handle_by_name.insert(face_name.clone(), handle);
        self.font_list.insert(face_name, info);
    }

    pub fn get_font(&self, name: &[u8]) -> Option<&FontFaceInfo> {
        self.font_list.get(name)
    }

    fn font_for_handle(&self, handle: u64) -> Option<&FontFaceInfo> {
        let index = usize::try_from(handle.checked_sub(1)?).ok()?;
        self.font_names
            .get(index)
            .and_then(|name| self.font_list.get(name))
    }

    pub fn get_subst_font(&self, name: &[u8]) -> Option<&FontFaceInfo> {
        self.get_font(base14_subst_name(name)?)
    }

    pub(super) fn font_handle(&self, name: &[u8]) -> Option<u64> {
        self.handle_by_name.get(name).copied()
    }

    pub fn find_font(
        &self,
        weight: i32,
        italic: bool,
        charset: FxCharset,
        pitch_family: u32,
        family: &[u8],
        must_match_name: bool,
    ) -> Option<&FontFaceInfo> {
        let mut best = None;
        let mut best_score = 0;
        if must_match_name {
            if let Some(font) = self.font_list.get(family) {
                if font.is_eligible_for_find_font(charset) {
                    best_score = font.similarity_score(weight, italic, pitch_family, true);
                    best = Some(font);
                    if best_score == FontFaceInfo::SIMILARITY_SCORE_MAX {
                        return best;
                    }
                }
            }
        }
        for font in self.font_list.values() {
            if !font.is_eligible_for_find_font(charset) {
                continue;
            }
            let score = font.similarity_score(
                weight,
                italic,
                pitch_family,
                must_match_name && family.len() == font.face_name.len(),
            );
            if score > best_score
                && (!must_match_name || find_family_name_match(family, &font.face_name))
            {
                best_score = score;
                best = Some(font);
            }
        }
        if best.is_some() {
            return best;
        }
        if charset == FxCharset::Ansi && font_family_is_fixed_pitch(pitch_family) {
            return self.get_font(b"Courier New");
        }
        None
    }
}

impl SystemFontInfo for CfxFolderFontInfo {
    fn enum_font_list(&mut self) -> Vec<(Vec<u8>, FxCharset)> {
        CfxFolderFontInfo::enum_font_list(self)
    }

    fn map_font(&mut self, _: i32, _: bool, _: FxCharset, _: u32, _: &[u8]) -> Option<u64> {
        // PDFium's shared CFX_FolderFontInfo::MapFont() returns nullptr.
        None
    }

    fn get_font(&mut self, face: &[u8]) -> Option<u64> {
        self.font_handle(face)
    }

    fn get_font_data(&mut self, handle: u64, table: u32, buffer: &mut [u8]) -> usize {
        let Some(font) = self.font_for_handle(handle) else {
            return 0;
        };
        let (size, offset) = if table == TABLE_NONE {
            (
                if font.font_offset == 0 {
                    font.file_size
                } else {
                    0
                },
                0,
            )
        } else if table == TABLE_TTCF {
            (
                if font.font_offset != 0 {
                    font.file_size
                } else {
                    0
                },
                0,
            )
        } else {
            let Some(loc) = find_font_table_location(&font.font_tables, table) else {
                return 0;
            };
            (loc.size, loc.offset)
        };
        let size = size as usize;
        if size == 0 || buffer.len() < size {
            return size;
        }
        let Ok(mut file) = File::open(&font.file_path) else {
            return 0;
        };
        if file.seek(SeekFrom::Start(u64::from(offset))).is_err() {
            return 0;
        }
        if file.read_exact(&mut buffer[..size]).is_err() {
            return 0;
        }
        size
    }

    fn get_face_name(&mut self, handle: u64) -> Option<Vec<u8>> {
        Some(self.font_for_handle(handle)?.face_name.clone())
    }

    fn get_font_charset(&mut self, handle: u64) -> Option<FxCharset> {
        let font = self.font_for_handle(handle)?;
        CHARSET_PRIORITY
            .iter()
            .copied()
            .find(|&charset| font.charsets & charset_flag(charset) != 0)
    }

    fn delete_font(&mut self, _: u64) {
        // Handles identify entries owned by the provider, as in PDFium.
    }
}

fn read_table_at(file: &mut File, file_size: u32, offset: u32, size: u32) -> Option<Vec<u8>> {
    if offset.checked_add(size)? > file_size {
        return None;
    }
    file.seek(SeekFrom::Start(u64::from(offset))).ok()?;
    let mut data = vec![0; size as usize];
    file.read_exact(&mut data).ok()?;
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::fx_codepage::FxCodePage;
    use crate::fxge::cfx_fontmapper::{CfxFontMapper, SubstFace};
    use crate::fxge::cfx_substfont::CfxSubstFont;
    use crate::fxge::fx_font::{FONT_PITCH_FAMILY_ROMAN, FONT_STYLE_FORCE_BOLD};

    #[test]
    fn family_boundaries_and_similarity_match_pdfium() {
        assert!(find_family_name_match(
            b"Oxygen-Sans",
            b"Oxygen-Sans Sans-Bold"
        ));
        assert!(!find_family_name_match(b"Univers", b"Universal"));
        assert!(find_family_name_match(b"Tofu", b"Tofu, Bold Italic"));
        assert!(!find_family_name_match(b"", b"Tofu"));
        assert!(!find_family_name_match(b"", b""));
        let mut info = FontFaceInfo::new(PathBuf::new(), b"Test".to_vec(), Vec::new(), 0, 0);
        info.styles = FONT_STYLE_FORCE_BOLD;
        info.charsets = charset_flag(FxCharset::Ansi);
        assert_eq!(info.similarity_score(700, false, 0, true), 68);
        assert!(!info.is_eligible_for_find_font(FxCharset::Symbol));
        assert!(info.is_eligible_for_find_font(FxCharset::Default));
    }

    #[test]
    fn find_font_uses_name_boundary_and_lexicographic_score_order() {
        let mut folder = CfxFolderFontInfo::new();
        for (name, charset) in [
            (b"Universal".as_slice(), FxCharset::Ansi),
            (b"Univers Bold".as_slice(), FxCharset::Ansi),
            (b"Bookshelf Symbol 7".as_slice(), FxCharset::Symbol),
        ] {
            let mut info = FontFaceInfo::new(PathBuf::new(), name.to_vec(), Vec::new(), 0, 0);
            info.charsets = charset_flag(charset);
            folder.font_list.insert(name.to_vec(), info);
        }
        assert_eq!(
            folder
                .find_font(400, false, FxCharset::Ansi, 0, b"Univers", true)
                .unwrap()
                .face_name,
            b"Univers Bold"
        );
        assert!(folder
            .find_font(
                400,
                false,
                FxCharset::Symbol,
                FONT_PITCH_FAMILY_ROMAN,
                b"Book",
                true
            )
            .is_none());
    }

    #[test]
    fn scans_configured_folder_and_reads_real_sfnt_metadata() {
        let mut folder = CfxFolderFontInfo::new();
        folder.add_path(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem"
        )));
        let installed = folder.enum_font_list();
        assert!(installed
            .iter()
            .any(|(name, charset)| name == b"Ahem" && *charset == FxCharset::Ansi));
        let font = folder.get_font(b"Ahem").unwrap();
        assert_eq!(font.font_offset, 0);
        assert!(font.file_size > 0);
        assert!(font.glyph_count > 0);
        assert!(!font.font_tables.is_empty());
        assert!(folder.get_subst_font(b"Helvetica").is_none());
    }

    #[test]
    fn folder_provider_supplies_font_tables_to_mapper_direct_match() {
        let mut folder = CfxFolderFontInfo::new();
        folder.add_path(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem"
        )));
        let entries = SystemFontInfo::enum_font_list(&mut folder);
        assert!(entries.iter().any(|(name, _)| name == b"Ahem"));
        let handle = SystemFontInfo::get_font(&mut folder, b"Ahem").unwrap();
        let size = SystemFontInfo::get_font_data(&mut folder, handle, TABLE_NAME, &mut []);
        let mut name_table = vec![0; size];
        assert_eq!(
            SystemFontInfo::get_font_data(&mut folder, handle, TABLE_NAME, &mut name_table),
            size
        );
        assert_eq!(get_name_from_tt(&name_table, 1), b"Ahem");
        assert_eq!(
            SystemFontInfo::get_font_data(&mut folder, 999, TABLE_NAME, &mut []),
            0
        );

        let mut mapper_folder = CfxFolderFontInfo::new();
        mapper_folder.add_path(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem"
        )));
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(mapper_folder)));
        let mut subst = CfxSubstFont::new();
        let face =
            mapper.find_subst_face(b"Ahem", true, 0, 400, 0, FxCodePage::DefAnsi, &mut subst);
        assert!(matches!(face, Some(SubstFace::External(_))));
        assert_eq!(subst.family(), b"Ahem");
    }
}

#[cfg(test)]
#[path = "cfx_folderfontinfo_unittest.rs"]
mod upstream_tests;
