// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Folder font policy from `core/fxge/apple/capple_platform.cpp`.
//! Platform graphics and native glyph drawing remain excluded.

use std::path::PathBuf;

use super::cfx_folderfontinfo::{base14_subst_name, CfxFolderFontInfo};
use super::fx_font::{font_family_is_fixed_pitch, font_family_is_roman};
use super::systemfontinfo_iface::SystemFontInfo;
use crate::fxcrt::fx_codepage::FxCharset;

const DEFAULT_PATHS: [&str; 3] = ["~/Library/Fonts", "/Library/Fonts", "/System/Library/Fonts"];

pub struct CfxMacFontInfo {
    folder: CfxFolderFontInfo,
}

impl Default for CfxMacFontInfo {
    fn default() -> Self {
        // PDFium passes these strings directly to the POSIX folder scanner;
        // it does not perform shell expansion on the leading tilde.
        Self::from_paths(DEFAULT_PATHS.map(PathBuf::from))
    }
}

impl CfxMacFontInfo {
    pub fn from_paths(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut folder = CfxFolderFontInfo::new();
        for path in paths {
            folder.add_path(path);
        }
        Self { folder }
    }
}

// Keep the policy independent of font storage so source-derived cases can
// exercise every name/charset branch without requiring host-installed fonts.
fn map_font<T>(
    weight: i32,
    italic: bool,
    charset: FxCharset,
    pitch_family: u32,
    face: &[u8],
    mut get_font: impl FnMut(&[u8]) -> Option<T>,
) -> Option<T> {
    if let Some(substitute) = base14_subst_name(face) {
        return get_font(substitute);
    }
    if !face.windows(4).any(|part| part == b"Bold")
        && !face.windows(6).any(|part| part == b"Italic")
    {
        let mut styled = Vec::with_capacity(face.len() + 12);
        styled.extend_from_slice(face);
        if weight > 400 {
            styled.extend_from_slice(b" Bold");
        }
        if italic {
            styled.extend_from_slice(b" Italic");
        }
        if let Some(font) = get_font(&styled) {
            return Some(font);
        }
    }
    if let Some(font) = get_font(face) {
        return Some(font);
    }
    if charset == FxCharset::Ansi && font_family_is_fixed_pitch(pitch_family) {
        return get_font(b"Courier New");
    }
    if matches!(charset, FxCharset::Ansi | FxCharset::Symbol) {
        return None;
    }
    let other: &[u8] = match charset {
        FxCharset::ShiftJis => {
            if face.windows(6).any(|part| part == b"Gothic") {
                b"Hiragino Kaku Gothic Pro W6"
            } else if font_family_is_roman(pitch_family) || weight <= 400 {
                b"Hiragino Mincho Pro W6"
            } else {
                b"Hiragino Kaku Gothic Pro W6"
            }
        }
        FxCharset::ChineseSimplified => b"STSong",
        FxCharset::Hangul => b"AppleMyungjo",
        FxCharset::ChineseTraditional => b"LiSong Pro Light",
        _ => face,
    };
    get_font(other)
}

impl SystemFontInfo for CfxMacFontInfo {
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
        map_font(weight, italic, charset, pitch_family, face, |name| {
            self.folder.font_handle(name)
        })
    }

    fn get_font(&mut self, face: &[u8]) -> Option<u64> {
        self.folder.font_handle(face)
    }

    fn get_font_data(&mut self, font: u64, table: u32, buffer: &mut [u8]) -> usize {
        self.folder.get_font_data(font, table, buffer)
    }

    fn get_face_name(&mut self, font: u64) -> Option<Vec<u8>> {
        self.folder.get_face_name(font)
    }

    fn get_font_charset(&mut self, font: u64) -> Option<FxCharset> {
        self.folder.get_font_charset(font)
    }

    fn delete_font(&mut self, font: u64) {
        self.folder.delete_font(font);
    }
}

#[cfg(test)]
#[path = "cfx_macfontinfo_tests.rs"]
mod tests;
