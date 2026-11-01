// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Face and charset operations from `core/fxge/cfx_font.{h,cpp}`.
//! Font manager substitution and AGG glyph bitmap caching.

use std::cell::OnceCell;
use std::rc::Rc;

use crate::fxcrt::fx_codepage::{FxCharset, FxCodePage};
use crate::fxcrt::fx_coordinates::{CfxMatrix, FxRect};
use crate::fxge::cfx_cttgsubtable::CfxCttgsubTable;
use crate::fxge::cfx_face::{CfxFace, CfxFaceError, CfxRetainedFace};
use crate::fxge::cfx_fontmapper::{CfxFontMapper, SubstFace};
use crate::fxge::cfx_fontmgr::{with_font_manager, SharedGlyphCache};
use crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap;
use crate::fxge::cfx_path::CfxPath;
use crate::fxge::cfx_substfont::CfxSubstFont;
use crate::fxge::fx_font::{normalize_font_metric, CharCodeAndIndex, FontAntiAliasingMode};

pub const UNTITLED_FONT_NAME: &str = "Untitled";
pub const DEFAULT_ANSI_FONT_NAME: &str = "Helvetica";
pub const UNIVERSAL_DEFAULT_FONT_NAME: &str = "Arial Unicode MS";

const DEFAULT_TTF_MAP: &[(FxCharset, &str)] = &[
    (FxCharset::Ansi, DEFAULT_ANSI_FONT_NAME),
    (FxCharset::ChineseSimplified, "SimSun"),
    (FxCharset::ChineseTraditional, "MingLiU"),
    (FxCharset::ShiftJis, "MS Gothic"),
    (FxCharset::Hangul, "Batang"),
    (FxCharset::MswinCyrillic, "Arial"),
    (FxCharset::MswinEasternEuropean, "Arial"),
    (FxCharset::MswinArabic, "Arial"),
];

pub fn default_ttf_map() -> &'static [(FxCharset, &'static str)] {
    DEFAULT_TTF_MAP
}

pub fn default_font_name_by_charset(charset: FxCharset) -> &'static str {
    DEFAULT_TTF_MAP
        .iter()
        .find(|entry| entry.0 == charset)
        .map_or(UNIVERSAL_DEFAULT_FONT_NAME, |entry| entry.1)
}

pub fn charset_from_unicode(word: u16) -> FxCharset {
    if word < 0x7f {
        return FxCharset::Ansi;
    }
    if (0x4e00..=0x9fa5).contains(&word)
        || (0xe7c7..=0xe7f3).contains(&word)
        || (0x3000..=0x303f).contains(&word)
        || (0x2000..=0x206f).contains(&word)
    {
        return FxCharset::ChineseSimplified;
    }
    if (0x3040..=0x309f).contains(&word)
        || (0x30a0..=0x30ff).contains(&word)
        || (0x31f0..=0x31ff).contains(&word)
        || (0xff00..=0xffef).contains(&word)
    {
        return FxCharset::ShiftJis;
    }
    if (0xac00..=0xd7af).contains(&word)
        || (0x1100..=0x11ff).contains(&word)
        || (0x3130..=0x318f).contains(&word)
    {
        return FxCharset::Hangul;
    }
    if (0x0e00..=0x0e7f).contains(&word) {
        return FxCharset::Thai;
    }
    if (0x0370..=0x03ff).contains(&word) || (0x1f00..=0x1fff).contains(&word) {
        return FxCharset::MswinGreek;
    }
    if (0x0600..=0x06ff).contains(&word) || (0xfb50..=0xfefc).contains(&word) {
        return FxCharset::MswinArabic;
    }
    if (0x0590..=0x05ff).contains(&word) {
        return FxCharset::MswinHebrew;
    }
    if (0x0400..=0x04ff).contains(&word) {
        return FxCharset::MswinCyrillic;
    }
    if (0x0100..=0x024f).contains(&word) {
        return FxCharset::MswinEasternEuropean;
    }
    if (0x1e00..=0x1eff).contains(&word) {
        return FxCharset::MswinVietnamese;
    }
    FxCharset::Ansi
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontType {
    Unknown,
    CidTrueType,
}

enum FontFace<'a> {
    Owned(CfxFace<'a>),
    Borrowed(&'a CfxFace<'a>),
    Shared(Rc<dyn CfxRetainedFace>),
}

/// Owns a parsed face or borrows a face retained by document page data.
pub struct CfxFont<'a> {
    face: Option<FontFace<'a>>,
    subst_selection: Option<SubstFace>,
    glyph_cache: OnceCell<SharedGlyphCache>,
    subst_font: Option<CfxSubstFont>,
    vertical: bool,
    font_type: FontType,
    object_tag: u64,
}

impl<'a> Default for CfxFont<'a> {
    fn default() -> Self {
        Self {
            face: None,
            subst_selection: None,
            glyph_cache: OnceCell::new(),
            subst_font: None,
            vertical: false,
            font_type: FontType::Unknown,
            object_tag: 0,
        }
    }
}

impl<'a> CfxFont<'a> {
    pub fn from_span(
        data: &'a [u8],
        vertical: bool,
        object_tag: u64,
    ) -> Result<Self, CfxFaceError> {
        Self::from_span_at_index(data, 0, vertical, object_tag)
    }

    pub fn from_span_at_index(
        data: &'a [u8],
        face_index: u32,
        vertical: bool,
        object_tag: u64,
    ) -> Result<Self, CfxFaceError> {
        Ok(Self {
            face: Some(FontFace::Owned(CfxFace::new(data, face_index)?)),
            vertical,
            object_tag,
            ..Self::default()
        })
    }

    pub fn from_face(face: &'a CfxFace<'a>, vertical: bool, object_tag: u64) -> Self {
        Self {
            face: Some(FontFace::Borrowed(face)),
            vertical,
            object_tag,
            ..Self::default()
        }
    }

    /// Source SetFaceFromFont/SetSubstFont view used by XFA text output. The
    /// original font retains the face; metrics/outlines are shared without
    /// cloning embedded bytes, while synthetic style belongs to this view.
    pub fn from_face_with_substitute(
        face: &'a CfxFace<'a>,
        subst_font: CfxSubstFont,
        vertical: bool,
        object_tag: u64,
    ) -> Self {
        Self {
            subst_font: Some(subst_font),
            ..Self::from_face(face, vertical, object_tag)
        }
    }

    pub fn from_retained_face(
        face: Rc<dyn CfxRetainedFace>,
        vertical: bool,
        object_tag: u64,
    ) -> Self {
        Self {
            face: Some(FontFace::Shared(face)),
            vertical,
            object_tag,
            ..Self::default()
        }
    }

    pub fn from_substitute(face: CfxFace<'a>, subst_font: CfxSubstFont, vertical: bool) -> Self {
        Self {
            face: Some(FontFace::Owned(face)),
            subst_selection: None,
            glyph_cache: OnceCell::new(),
            subst_font: Some(subst_font),
            vertical,
            ..Self::default()
        }
    }

    pub fn face(&self) -> Option<&CfxFace<'_>> {
        match self.subst_selection.as_ref() {
            Some(selection) => Some(selection.face()),
            None => self.face.as_ref().map(|face| match face {
                FontFace::Owned(face) => face,
                FontFace::Borrowed(face) => *face,
                FontFace::Shared(face) => face.face(),
            }),
        }
    }

    pub fn load_subst_face(
        &mut self,
        mapper: &mut CfxFontMapper,
        face_name: &[u8],
        is_truetype: bool,
        flags: u32,
        weight: i32,
        italic_angle: i32,
        codepage: FxCodePage,
        vertical: bool,
    ) {
        self.vertical = vertical;
        self.object_tag = 0;
        self.face = None;
        self.clear_glyph_cache();
        let mut subst_font = CfxSubstFont::new();
        self.subst_selection = mapper.find_subst_face(
            face_name,
            is_truetype,
            flags,
            weight,
            italic_angle,
            codepage,
            &mut subst_font,
        );
        self.subst_font = Some(subst_font);
    }

    pub fn subst_selection(&self) -> Option<&SubstFace> {
        self.subst_selection.as_ref()
    }

    pub fn face_mut(&mut self) -> Option<&mut CfxFace<'a>> {
        match self.face.as_mut()? {
            FontFace::Owned(face) => Some(face),
            FontFace::Borrowed(_) | FontFace::Shared(_) => None,
        }
    }

    pub fn subst_font(&self) -> Option<&CfxSubstFont> {
        self.subst_font.as_ref()
    }

    pub fn subst_font_italic_angle(&self) -> i32 {
        self.subst_font
            .as_ref()
            .map_or(0, CfxSubstFont::italic_angle)
    }

    pub fn is_vertical(&self) -> bool {
        self.vertical
    }

    pub fn font_type(&self) -> FontType {
        self.font_type
    }

    pub fn set_font_type(&mut self, font_type: FontType) {
        self.font_type = font_type;
    }

    pub fn object_tag(&self) -> u64 {
        self.object_tag
    }

    pub fn font_span(&self) -> &[u8] {
        self.face().map_or(&[], CfxFace::data)
    }

    pub fn has_any_glyphs(&self) -> bool {
        self.face().is_some_and(|face| face.glyph_count() > 0)
    }

    pub fn is_tt_font(&self) -> bool {
        self.face().is_some_and(CfxFace::is_tt_ot)
    }

    pub fn get_char_index(&self, code: u32) -> u32 {
        self.face().map_or(0, |face| face.get_char_index(code))
    }

    pub fn get_name_index(&self, name: &str) -> u32 {
        self.face().map_or(0, |face| face.name_index(name))
    }

    pub fn get_char_codes_and_indices(&self, max_char: u32) -> Vec<CharCodeAndIndex> {
        self.face()
            .map_or_else(Vec::new, |face| face.get_char_codes_and_indices(max_char))
    }

    pub fn get_glyph_width(&self, glyph_index: u32, dest_width: i32) -> i32 {
        self.get_glyph_width_with_weight(glyph_index, dest_width, 0)
    }

    pub fn get_glyph_width_with_weight(
        &self,
        glyph_index: u32,
        dest_width: i32,
        weight: i32,
    ) -> i32 {
        self.get_or_create_glyph_cache()
            .borrow_mut()
            .get_glyph_width(self, glyph_index, dest_width, weight)
    }

    pub fn get_glyph_width_impl(&self, glyph_index: u32, dest_width: i32, weight: i32) -> i32 {
        self.face().map_or(0, |face| {
            face.glyph_width_with_weight(glyph_index, dest_width, weight, self.subst_font.as_ref())
        })
    }

    pub fn get_ascent(&self) -> i32 {
        self.face().map_or(0, |face| {
            normalize_font_metric(i64::from(face.ascender()), face.units_per_em())
        })
    }

    pub fn get_descent(&self) -> i32 {
        self.face().map_or(0, |face| {
            normalize_font_metric(i64::from(face.descender()), face.units_per_em())
        })
    }

    pub fn get_glyph_bbox(&self, glyph_index: u32) -> Option<FxRect> {
        self.face()?.font_glyph_bbox(glyph_index)
    }

    pub fn get_char_bbox(&self, glyph_index: u32) -> Option<FxRect> {
        self.face().map(|face| face.char_bbox(glyph_index as i32))
    }

    pub fn is_italic(&self) -> bool {
        self.face().is_some_and(|face| {
            if face.is_italic() {
                return true;
            }
            let mut style = face.style_name();
            style.make_ascii_lowercase();
            style.contains("italic")
        })
    }

    pub fn is_bold(&self) -> bool {
        self.face().is_some_and(CfxFace::is_bold)
    }

    pub fn is_fixed_width(&self) -> bool {
        self.face().is_some_and(CfxFace::is_fixed_width)
    }

    pub fn ps_name(&self) -> String {
        self.face().map_or_else(String::new, |face| {
            let name = face.postscript_name();
            if name.is_empty() {
                UNTITLED_FONT_NAME.to_owned()
            } else {
                name
            }
        })
    }

    pub fn family_name(&self) -> String {
        if let Some(face) = self.face() {
            return face.family_name();
        }
        self.subst_font.as_ref().map_or_else(String::new, |subst| {
            String::from_utf8_lossy(subst.family()).into_owned()
        })
    }

    pub fn base_font_name(&self) -> String {
        let ps_name = self.ps_name();
        if !ps_name.is_empty() && ps_name != UNTITLED_FONT_NAME {
            return ps_name;
        }
        if let Some(face) = self.face() {
            let mut family = self.family_name();
            if family.is_empty() {
                family = UNTITLED_FONT_NAME.to_owned();
            }
            if self.is_tt_font() {
                family.retain(|ch| ch != ' ');
            }
            let style = face.style_name();
            if !style.is_empty() && style != "Regular" {
                family.push_str(if self.is_tt_font() { "," } else { " " });
                family.push_str(&style);
            }
            return family;
        }
        self.family_name()
    }

    pub fn raw_bbox(&self) -> Option<FxRect> {
        self.face().map(CfxFace::bbox)
    }

    pub fn bbox(&self) -> Option<FxRect> {
        let face = self.face()?;
        let mut bbox = face.bbox();
        let upem = face.units_per_em();
        if upem != 0 {
            let em = upem as f32;
            bbox.left = (bbox.left as f32 * 1000.0 / em) as i32;
            bbox.top = (bbox.top as f32 * 1000.0 / em) as i32;
            bbox.right = (bbox.right as f32 * 1000.0 / em) as i32;
            bbox.bottom = (bbox.bottom as f32 * 1000.0 / em) as i32;
        }
        Some(bbox)
    }

    pub fn parse_gsub_table(&self) -> Option<CfxCttgsubTable> {
        self.face()?.parse_gsub_table()
    }

    pub fn load_glyph_path(&self, glyph_index: u32, dest_width: i32) -> Option<Rc<CfxPath>> {
        self.get_or_create_glyph_cache()
            .borrow_mut()
            .load_glyph_path(self, glyph_index, dest_width)
    }

    pub fn load_glyph_bitmap(
        &self,
        glyph_index: u32,
        is_cid_font: bool,
        matrix: CfxMatrix,
        dest_width: i32,
        anti_alias: FontAntiAliasingMode,
    ) -> Option<Rc<CfxGlyphBitmap>> {
        self.get_or_create_glyph_cache()
            .borrow_mut()
            .load_glyph_bitmap(
                self,
                glyph_index,
                is_cid_font,
                matrix,
                dest_width,
                anti_alias,
            )
    }

    fn get_or_create_glyph_cache(&self) -> &SharedGlyphCache {
        self.glyph_cache
            .get_or_init(|| with_font_manager(|manager| manager.get_glyph_cache(self)))
    }

    pub fn clear_glyph_cache(&mut self) {
        self.glyph_cache.take();
    }

    pub fn render_glyph_impl(
        &self,
        glyph_index: u32,
        is_cid_font: bool,
        matrix: CfxMatrix,
        dest_width: i32,
        anti_alias: FontAntiAliasingMode,
    ) -> Option<CfxGlyphBitmap> {
        self.face()?.render_glyph(
            glyph_index,
            is_cid_font,
            self.vertical,
            matrix,
            dest_width,
            anti_alias,
            self.subst_font.as_ref(),
        )
    }

    pub fn load_glyph_path_impl(&self, glyph_index: u32, dest_width: i32) -> Option<CfxPath> {
        self.face()?.load_glyph_path(
            glyph_index,
            dest_width,
            self.vertical,
            self.subst_font.as_ref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::cfx_folderfontinfo::CfxFolderFontInfo;
    use crate::fxge::dib::fx_dib::FxdibFormat;
    use crate::fxge::fx_font::FONT_STYLE_SERIF;
    use std::path::PathBuf;

    #[test]
    fn default_fonts_and_unicode_charsets_match_linux_source() {
        assert_eq!(default_ttf_map().len(), 8);
        assert_eq!(
            default_font_name_by_charset(FxCharset::ChineseTraditional),
            "MingLiU"
        );
        assert_eq!(
            default_font_name_by_charset(FxCharset::Symbol),
            UNIVERSAL_DEFAULT_FONT_NAME
        );
        assert_eq!(charset_from_unicode(b'A' as u16), FxCharset::Ansi);
        assert_eq!(charset_from_unicode(0x4e00), FxCharset::ChineseSimplified);
        assert_eq!(charset_from_unicode(0x3042), FxCharset::ShiftJis);
        assert_eq!(charset_from_unicode(0xac00), FxCharset::Hangul);
        assert_eq!(charset_from_unicode(0x0e01), FxCharset::Thai);
        assert_eq!(charset_from_unicode(0x05d0), FxCharset::MswinHebrew);
        assert_eq!(charset_from_unicode(0x0410), FxCharset::MswinCyrillic);
        assert_eq!(charset_from_unicode(0x1e01), FxCharset::MswinVietnamese);
    }

    #[test]
    fn borrowed_sfnt_font_exposes_face_metrics_and_paths() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let font = CfxFont::from_span(data, false, 77).unwrap();
        assert!(font.is_tt_font());
        assert!(font.has_any_glyphs());
        assert_eq!(font.object_tag(), 77);
        assert_eq!(font.family_name(), "Ahem");
        assert!(!font.base_font_name().is_empty());
        let glyph = font.get_char_index('A' as u32);
        assert_ne!(glyph, 0);
        assert!(font.get_glyph_width(glyph, 0) > 0);
        assert!(font.get_glyph_bbox(glyph).is_some());
        assert!(font.load_glyph_path(glyph, 0).is_some());
        assert_eq!(font.get_char_codes_and_indices(0xffff).len(), 278);
        assert!(!font.font_span().is_empty());
        assert!(font.bbox().is_some());
        let empty = CfxFont::default();
        assert!(!empty.has_any_glyphs());
        assert_eq!(empty.get_char_index('A' as u32), 0);
    }

    #[test]
    fn mapped_external_face_remains_owned_after_mapper_is_dropped() {
        let mut provider = CfxFolderFontInfo::new();
        provider.add_path(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/ahem"
        )));
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(provider)));
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
        drop(mapper);
        assert!(matches!(
            font.subst_selection(),
            Some(SubstFace::External(_))
        ));
        assert!(font.has_any_glyphs());
        assert_eq!(font.family_name(), "Ahem");
        assert_eq!(
            font.font_span().len(),
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf")
                .len()
        );
        assert!(font.get_char_index(u32::from(b'A')) > 0);
    }

    #[test]
    fn builtin_substitute_faces_are_cached_and_retained_by_fonts() {
        let mut mapper = CfxFontMapper::new();
        let mut first = CfxFont::default();
        let mut second = CfxFont::default();
        for font in [&mut first, &mut second] {
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
        }
        assert!(std::ptr::eq(first.face().unwrap(), second.face().unwrap()));
        assert!(first
            .face()
            .unwrap()
            .select_charmap(crate::fxge::fx_fontencoding::FontEncoding::AdobeCustom));
        assert_eq!(
            second.face().unwrap().current_charmap_encoding(),
            crate::fxge::fx_fontencoding::FontEncoding::AdobeCustom
        );

        let mut serif = CfxFont::default();
        let mut sans = CfxFont::default();
        serif.load_subst_face(
            &mut mapper,
            b"UnknownSerif",
            true,
            FONT_STYLE_SERIF | crate::fxge::fx_font::FONT_USE_EXTERN_ATTR,
            400,
            0,
            FxCodePage::DefAnsi,
            false,
        );
        sans.load_subst_face(
            &mut mapper,
            b"UnknownSans",
            true,
            0,
            400,
            0,
            FxCodePage::DefAnsi,
            false,
        );
        drop(mapper);
        for font in [&first, &second, &serif, &sans] {
            assert!(font.has_any_glyphs());
            assert!(!font.font_span().is_empty());
            let glyph = font.get_char_index(u32::from(b'A'));
            assert!(glyph > 0);
            assert!(font.get_glyph_width(glyph, 0) > 0);
            let bitmap = font
                .load_glyph_bitmap(
                    glyph,
                    false,
                    CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0),
                    600,
                    FontAntiAliasingMode::Normal,
                )
                .unwrap();
            assert!(bitmap.bitmap().buffer().iter().any(|&value| value != 0));
        }
    }

    #[test]
    fn shared_glyph_cache_reuses_entries_and_expires_with_last_font() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let face = CfxFace::new(data, 0).unwrap();
        let first = CfxFont::from_face(&face, false, 0);
        let mut second = CfxFont::from_face(&face, false, 0);
        assert!(Rc::ptr_eq(
            first.get_or_create_glyph_cache(),
            second.get_or_create_glyph_cache()
        ));
        let glyph = first.get_char_index(u32::from(b'A'));
        let matrix = CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0);
        let first_bitmap = first
            .load_glyph_bitmap(glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        let second_bitmap = second
            .load_glyph_bitmap(glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        assert!(Rc::ptr_eq(&first_bitmap, &second_bitmap));
        // Upstream uses six key fields without a substitute and nine with one.
        second.subst_font = Some(CfxSubstFont::new());
        let substituted = second
            .load_glyph_bitmap(glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        assert!(!Rc::ptr_eq(&first_bitmap, &substituted));
        let weak_cache = Rc::downgrade(first.get_or_create_glyph_cache());
        drop(first);
        assert!(weak_cache.upgrade().is_some());
        drop(second);
        assert!(weak_cache.upgrade().is_none());
        assert!(first_bitmap
            .bitmap()
            .buffer()
            .iter()
            .any(|&value| value != 0));
    }

    #[test]
    fn glyph_path_cache_keeps_one_path_per_substitution_style() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let mut font =
            CfxFont::from_substitute(CfxFace::new(data, 0).unwrap(), CfxSubstFont::new(), false);
        let glyph = font.get_char_index(u32::from(b'A'));
        let first = font.load_glyph_path(glyph, 0).unwrap();
        let second = font.load_glyph_path(glyph, 0).unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        drop(first);
        drop(second);
        assert_eq!(font.get_or_create_glyph_cache().borrow().path_count(), 1);
        assert!(font.load_glyph_path(u32::MAX, 0).is_none());
        assert_eq!(font.get_or_create_glyph_cache().borrow().path_count(), 1);
        font.subst_font.as_mut().unwrap().set_weight(700);
        assert!(font.load_glyph_path(glyph, 0).is_some());
        assert_eq!(font.get_or_create_glyph_cache().borrow().path_count(), 2);
        assert!(font.get_glyph_width(glyph, 0) > 0);
        assert!(font.get_glyph_width(glyph, 0) > 0);
        assert_eq!(font.get_or_create_glyph_cache().borrow().width_count(), 1);
    }

    #[test]
    fn generic_width_queries_and_cached_outlines_keep_requested_weights() {
        use crate::fxge::cfx_standardfont::get_generic_serif_font_data;
        let mut subst = CfxSubstFont::new();
        subst.set_is_builtin_generic_font();
        subst.set_weight(300);
        let mut font = CfxFont::from_substitute(
            CfxFace::new(get_generic_serif_font_data(), 0).unwrap(),
            subst,
            false,
        );
        let glyph = font.face().unwrap().name_index("H");
        let normal = font.load_glyph_path(glyph, 700).unwrap();
        let regular_width = font.get_glyph_width_with_weight(glyph, 0, 300);
        let bold_width = font.get_glyph_width_with_weight(glyph, 0, 700);
        assert_eq!(regular_width, 819);
        assert_eq!(bold_width, 1049);
        font.subst_font.as_mut().unwrap().set_weight(700);
        let bold = font.load_glyph_path(glyph, 700).unwrap();
        assert_ne!(bold.points(), normal.points());
        assert_eq!(
            font.get_glyph_width_with_weight(glyph, 0, 300),
            regular_width
        );
        assert_eq!(font.get_glyph_width_with_weight(glyph, 0, 700), bold_width);
        font.subst_font.as_mut().unwrap().set_weight(300);
        assert!(Rc::ptr_eq(
            &normal,
            &font.load_glyph_path(glyph, 700).unwrap()
        ));
        let cache = font.get_or_create_glyph_cache().borrow();
        assert_eq!(cache.width_count(), 2);
        assert_eq!(cache.path_count(), 2);
    }

    #[test]
    fn glyph_bitmap_renders_all_mask_modes_and_reuses_cache_entry() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let font = CfxFont::from_span(data, false, 0).unwrap();
        let glyph = font.get_char_index(u32::from(b'A'));
        let matrix = CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0);
        let normal = font
            .load_glyph_bitmap(glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        let cached = font
            .load_glyph_bitmap(glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        assert!(Rc::ptr_eq(&normal, &cached));
        let bitmap = normal.bitmap();
        assert_eq!(bitmap.format(), FxdibFormat::EightBppMask);
        assert!(bitmap.buffer().iter().any(|&coverage| coverage != 0));
        drop(bitmap);

        let mono = font
            .load_glyph_bitmap(glyph, false, matrix, 0, FontAntiAliasingMode::Mono)
            .unwrap();
        let bitmap = mono.bitmap();
        assert_eq!(bitmap.format(), FxdibFormat::OneBppMask);
        assert!(bitmap.buffer().iter().any(|&coverage| coverage != 0));
        drop(bitmap);

        let lcd = font
            .load_glyph_bitmap(glyph, false, matrix, 0, FontAntiAliasingMode::Lcd)
            .unwrap();
        let bitmap = lcd.bitmap();
        assert_eq!(bitmap.format(), FxdibFormat::EightBppMask);
        assert_eq!(bitmap.width() % 3, 0);
        assert!(bitmap.buffer().iter().any(|&coverage| coverage != 0));
        assert_eq!(font.get_or_create_glyph_cache().borrow().bitmap_count(), 3);
    }

    #[test]
    fn substituted_bold_increases_glyph_coverage() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let mut font =
            CfxFont::from_substitute(CfxFace::new(data, 0).unwrap(), CfxSubstFont::new(), false);
        let glyph = font.get_char_index(u32::from(b'A'));
        let matrix = CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0);
        let regular = font
            .load_glyph_bitmap(glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        let regular_coverage: u32 = regular
            .bitmap()
            .buffer()
            .iter()
            .map(|&v| u32::from(v))
            .sum();
        font.subst_font.as_mut().unwrap().set_weight(900);
        let bold = font
            .load_glyph_bitmap(glyph, false, matrix, 0, FontAntiAliasingMode::Normal)
            .unwrap();
        let bold_coverage: u32 = bold.bitmap().buffer().iter().map(|&v| u32::from(v)).sum();
        assert!(bold_coverage > regular_coverage);
        assert_eq!(font.get_or_create_glyph_cache().borrow().bitmap_count(), 2);
    }
}

#[cfg(test)]
#[path = "cfx_font_unittest.rs"]
mod upstream_tests;
