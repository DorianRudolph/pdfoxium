// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Partial Linux folder font policy from `core/fxge/linux/fx_linux_impl.cpp`.
//! The CJK preference tables remain to be translated.

use std::path::PathBuf;

use super::cfx_folderfontinfo::CfxFolderFontInfo;
use super::systemfontinfo_iface::SystemFontInfo;
use crate::fxcrt::fx_codepage::{charset_is_cjk, FxCharset};

pub struct CfxLinuxFontInfo {
    folder: CfxFolderFontInfo,
}

impl Default for CfxLinuxFontInfo {
    fn default() -> Self {
        #[allow(unused_mut)] // Extended on Windows.
        let mut paths: Vec<_> = [
            "/usr/share/fonts",
            "/usr/share/X11/fonts/Type1",
            "/usr/share/X11/fonts/TTF",
            "/usr/local/share/fonts",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        #[cfg(target_os = "windows")]
        paths.push(PathBuf::from(r"C:\Windows\Fonts"));
        Self::from_paths(paths)
    }
}

impl CfxLinuxFontInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_paths(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut folder = CfxFolderFontInfo::new();
        for path in paths {
            folder.add_path(path);
        }
        Self { folder }
    }

    pub fn add_path(&mut self, path: PathBuf) {
        self.folder.add_path(path);
    }
}

impl SystemFontInfo for CfxLinuxFontInfo {
    fn enum_font_list(&mut self) -> Vec<(Vec<u8>, FxCharset)> {
        self.folder.enum_font_list()
    }

    fn map_font(
        &mut self,
        weight: i32,
        italic: bool,
        charset: FxCharset,
        pitch_family: u32,
        face: &[u8],
    ) -> Option<u64> {
        let name = self
            .folder
            .get_subst_font(face)
            .or_else(|| {
                self.folder.find_font(
                    weight,
                    italic,
                    charset,
                    pitch_family,
                    face,
                    !charset_is_cjk(charset),
                )
            })?
            .face_name
            .clone();
        SystemFontInfo::get_font(&mut self.folder, &name)
    }

    fn get_font(&mut self, face: &[u8]) -> Option<u64> {
        SystemFontInfo::get_font(&mut self.folder, face)
    }

    fn get_font_data(&mut self, font: u64, table: u32, buffer: &mut [u8]) -> usize {
        SystemFontInfo::get_font_data(&mut self.folder, font, table, buffer)
    }

    fn get_face_name(&mut self, font: u64) -> Option<Vec<u8>> {
        SystemFontInfo::get_face_name(&mut self.folder, font)
    }

    fn get_font_charset(&mut self, font: u64) -> Option<FxCharset> {
        SystemFontInfo::get_font_charset(&mut self.folder, font)
    }

    fn delete_font(&mut self, font: u64) {
        SystemFontInfo::delete_font(&mut self.folder, font);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::fx_codepage::FxCodePage;
    use crate::fxge::cfx_font::CfxFont;
    use crate::fxge::cfx_fontmapper::CfxFontMapper;
    use crate::fxge::cfx_fontmapper::SubstFace;
    use crate::fxge::cfx_standardfont::StandardFontIndex;

    #[test]
    fn missing_base14_font_uses_bundled_face_instead_of_unrelated_system_font() {
        let info = CfxLinuxFontInfo::from_paths([PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/harfbuzz"
        ))]);
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(info)));
        let mut font = CfxFont::default();
        font.load_subst_face(
            &mut mapper,
            b"Helvetica",
            false,
            0,
            400,
            0,
            FxCodePage::DefAnsi,
            false,
        );
        assert!(matches!(
            font.subst_selection(),
            Some(SubstFace::Standard(StandardFontIndex::Helvetica, _))
        ));
        assert!(font.has_any_glyphs());
    }

    #[test]
    fn maps_installed_font_through_linux_policy() {
        let mut info = CfxLinuxFontInfo {
            folder: CfxFolderFontInfo::new(),
        };
        info.add_path(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem"
        )));
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(info)));
        let mut font = CfxFont::default();
        font.load_subst_face(
            &mut mapper,
            b"Ahem",
            true,
            0,
            400,
            0,
            FxCodePage::DefAnsi,
            false,
        );
        assert!(font.has_any_glyphs());
    }
}
