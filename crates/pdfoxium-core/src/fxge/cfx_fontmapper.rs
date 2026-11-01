// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Name, style, and installed font rules from `core/fxge/cfx_fontmapper.{h,cpp}`.
//! Bundled faces are retained by the mapper; external face caches remain weak.

use super::cfx_face::CfxFace;
use super::cfx_face::CfxOwnedFace;
use super::cfx_standardfont::{
    get_canonical_font_name, get_font_data, get_generic_sans_font_data,
    get_generic_serif_font_data, get_standard_font_index, StandardFontIndex, NUM_STANDARD_FONTS,
};
use super::cfx_substfont::CfxSubstFont;
use super::fx_font::{
    font_style_is_fixed_pitch, font_style_is_force_bold, font_style_is_italic,
    font_style_is_script, font_style_is_serif, font_style_is_symbolic, FONT_PITCH_FAMILY_FIXED,
    FONT_PITCH_FAMILY_ROMAN, FONT_PITCH_FAMILY_SCRIPT, FONT_STYLE_FORCE_BOLD, FONT_STYLE_ITALIC,
    FONT_STYLE_SYMBOLIC, FONT_USE_EXTERN_ATTR, FONT_WEIGHT_BOLD, FONT_WEIGHT_EXTRA_BOLD,
    FONT_WEIGHT_NORMAL,
};
use crate::fxcrt::fx_codepage::{charset_from_codepage, charset_is_cjk, FxCharset, FxCodePage};
use crate::fxge::fx_font::{get_name_from_tt, get_ttc_index};
use crate::fxge::systemfontinfo_iface::{SystemFontInfo, TABLE_NAME, TABLE_NONE, TABLE_TTCF};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

pub type CachedFace = CfxOwnedFace<Rc<Vec<u8>>>;

pub enum SubstFace {
    External(Rc<CachedFace>),
    Standard(StandardFontIndex, Rc<CfxFace<'static>>),
    GenericSerif(Rc<CfxFace<'static>>),
    GenericSans(Rc<CfxFace<'static>>),
}

impl SubstFace {
    pub fn face(&self) -> &CfxFace<'_> {
        match self {
            Self::External(face) => face.face(),
            Self::Standard(_, face) | Self::GenericSerif(face) | Self::GenericSans(face) => face,
        }
    }
}

pub struct CfxFontMapper {
    list_loaded: bool,
    skip_font_enumeration: bool,
    last_family: Vec<u8>,
    face_array: Vec<(Vec<u8>, FxCharset)>,
    font_info: Option<Box<dyn SystemFontInfo>>,
    installed_ttfonts: Vec<Vec<u8>>,
    localized_ttfonts: Vec<(Vec<u8>, Vec<u8>)>,
    standard_faces: [Option<Rc<CfxFace<'static>>>; NUM_STANDARD_FONTS],
    generic_sans_face: Option<Rc<CfxFace<'static>>>,
    generic_serif_face: Option<Rc<CfxFace<'static>>>,
    face_map: HashMap<(Vec<u8>, i32, bool), Weak<CachedFace>>,
    ttc_face_map: HashMap<(usize, u32), Vec<Weak<CachedFace>>>,
}

impl Default for CfxFontMapper {
    fn default() -> Self {
        Self {
            list_loaded: false,
            skip_font_enumeration: false,
            last_family: Vec::new(),
            face_array: Vec::new(),
            font_info: None,
            installed_ttfonts: Vec::new(),
            localized_ttfonts: Vec::new(),
            standard_faces: std::array::from_fn(|_| None),
            generic_sans_face: None,
            generic_serif_face: None,
            face_map: HashMap::new(),
            ttc_face_map: HashMap::new(),
        }
    }
}

impl CfxFontMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_system_font_info(&mut self, font_info: Option<Box<dyn SystemFontInfo>>) {
        if let Some(font_info) = font_info {
            self.list_loaded = false;
            self.font_info = Some(font_info);
        }
    }

    pub fn take_system_font_info(&mut self) -> Option<Box<dyn SystemFontInfo>> {
        self.font_info.take()
    }

    pub fn has_system_font_info(&self) -> bool {
        self.font_info.is_some()
    }

    pub fn set_skip_font_enumeration(&mut self, skip: bool) {
        self.skip_font_enumeration = skip;
    }

    pub fn add_installed_font(&mut self, name: &[u8], charset: FxCharset) {
        let Some(font_info) = self.font_info.as_mut() else {
            return;
        };
        self.face_array.push((name.to_vec(), charset));
        if name == self.last_family {
            return;
        }

        if name.iter().any(|&byte| byte > 0x80) {
            let handle = font_info
                .get_font(name)
                .or_else(|| font_info.map_font(0, false, FxCharset::Default, 0, name));
            let Some(handle) = handle else {
                return;
            };
            let ps_name = get_ps_name_from_tt(font_info.as_mut(), handle);
            font_info.delete_font(handle);
            if !ps_name.is_empty() {
                self.localized_ttfonts.push((ps_name, name.to_vec()));
            }
        }
        self.installed_ttfonts.push(name.to_vec());
        self.last_family = name.to_vec();
    }

    pub fn load_installed_fonts(&mut self) {
        if self.list_loaded {
            return;
        }
        let Some(font_info) = self.font_info.as_mut() else {
            return;
        };
        if !self.skip_font_enumeration {
            let fonts = font_info.enum_font_list();
            for (name, charset) in fonts {
                self.add_installed_font(&name, charset);
            }
        }
        self.list_loaded = true;
    }

    pub fn match_installed_fonts(&mut self, norm_name: &[u8]) -> Vec<u8> {
        self.load_installed_fonts();
        for font in self.installed_ttfonts.iter().rev() {
            if tt_normalize_name(font) == norm_name {
                return font.clone();
            }
        }
        for (ps_name, localized_name) in self.localized_ttfonts.iter().rev() {
            if tt_normalize_name(ps_name) == norm_name {
                return localized_name.clone();
            }
        }
        Vec::new()
    }

    pub fn face_count(&self) -> usize {
        self.face_array.len()
    }

    pub fn face_name(&self, index: usize) -> Option<&[u8]> {
        self.face_array.get(index).map(|(name, _)| name.as_slice())
    }

    /// XFA's RawBytesForIndex: obtain the provider's complete font stream for
    /// an enumerated face. The provider handle is released on every exit.
    pub fn raw_bytes_for_index(&mut self, index: usize) -> Vec<u8> {
        let Some((name, _)) = self.face_array.get(index) else {
            return Vec::new();
        };
        let Some(info) = self.font_info.as_mut() else {
            return Vec::new();
        };
        let Some(handle) = info.map_font(0, false, FxCharset::Default, 0, name) else {
            return Vec::new();
        };
        let scoped = ScopedProviderFont {
            info: info.as_mut(),
            handle,
        };
        let size = scoped.info.get_font_data(handle, TABLE_NONE, &mut []);
        read_font_data(scoped.info, handle, TABLE_NONE, size).unwrap_or_default()
    }

    pub fn first_face_for_charset(&self, charset: FxCharset) -> Option<&[u8]> {
        self.face_array
            .iter()
            .find(|(_, candidate)| *candidate == charset)
            .map(|(name, _)| name.as_slice())
    }

    pub fn has_installed_font(&self, name: &[u8]) -> bool {
        self.installed_ttfonts.iter().any(|font| font == name)
    }

    pub fn has_localized_font(&self, name: &[u8]) -> bool {
        self.localized_ttfonts
            .iter()
            .any(|(ps_name, _)| ps_name == name)
    }

    pub fn get_cached_face(
        &mut self,
        font_handle: u64,
        face_name: &[u8],
        weight: i32,
        italic: bool,
        data_size: usize,
    ) -> Option<Rc<CachedFace>> {
        let key = (face_name.to_vec(), weight, italic);
        if let Some(face) = self.face_map.get(&key).and_then(Weak::upgrade) {
            return Some(face);
        }
        let font_info = self.font_info.as_mut()?;
        let data = read_font_data(font_info.as_mut(), font_handle, TABLE_NONE, data_size)?;
        let face = Rc::new(CfxOwnedFace::new(Rc::new(data), 0).ok()?);
        self.face_map.insert(key, Rc::downgrade(&face));
        Some(face)
    }

    pub fn get_cached_ttc_face(
        &mut self,
        font_handle: u64,
        ttc_size: usize,
        data_size: usize,
    ) -> Option<Rc<CachedFace>> {
        let font_info = self.font_info.as_mut()?;
        let data = read_font_data(font_info.as_mut(), font_handle, TABLE_TTCF, ttc_size)?;
        let checksum = checksum_from_tt(&data);
        let offset = ttc_size.checked_sub(data_size)?;
        let index = usize::try_from(get_ttc_index(&data, offset)).ok()?;
        if index >= 16 {
            return None;
        }
        let entries = self.ttc_face_map.entry((ttc_size, checksum)).or_default();
        if let Some(face) = entries.get(index).and_then(Weak::upgrade) {
            return Some(face);
        }
        let owner = entries
            .iter()
            .find_map(Weak::upgrade)
            .map_or_else(|| Rc::new(data), |face| face.data_owner().clone());
        let face = Rc::new(CfxOwnedFace::new(owner, index as u32).ok()?);
        if entries.len() <= index {
            entries.resize_with(index + 1, Weak::new);
        }
        entries[index] = Rc::downgrade(&face);
        Some(face)
    }

    /// Consumes a provider handle, including on failed table reads or parsing.
    pub fn use_external_subst(
        &mut self,
        font_handle: u64,
        mut face_name: Vec<u8>,
        weight: i32,
        is_italic: bool,
        italic_angle: i32,
        mut charset: FxCharset,
        subst_font: &mut CfxSubstFont,
    ) -> Option<Rc<CachedFace>> {
        let result = (|| {
            let font_info = self.font_info.as_mut()?;
            let provider_face_name = font_info.get_face_name(font_handle);
            if let Some(name) = &provider_face_name {
                face_name = name.clone();
            }
            if charset == FxCharset::Default {
                if let Some(provider_charset) = font_info.get_font_charset(font_handle) {
                    charset = provider_charset;
                }
            }
            let ttc_size = font_info.get_font_data(font_handle, TABLE_TTCF, &mut []);
            let font_size = font_info.get_font_data(font_handle, TABLE_NONE, &mut []);
            if ttc_size == 0 && font_size == 0 {
                return None;
            }
            let face = if ttc_size != 0 {
                self.get_cached_ttc_face(font_handle, ttc_size, font_size)?
            } else {
                self.get_cached_face(font_handle, &face_name, weight, is_italic, font_size)?
            };
            if provider_face_name.is_none() {
                let parsed_name = font_name_from_face(face.face());
                if !parsed_name.is_empty() {
                    face_name = parsed_name;
                }
            }
            subst_font.configure_external_subst(
                &face_name,
                charset as u8,
                weight,
                is_italic,
                italic_angle,
                face.face().is_bold(),
                face.face().is_italic(),
            );
            Some(face)
        })();
        self.font_info.as_mut()?.delete_font(font_handle);
        result
    }

    pub fn find_subst_face(
        &mut self,
        name: &[u8],
        is_truetype: bool,
        flags: u32,
        mut weight: i32,
        mut italic_angle: i32,
        codepage: FxCodePage,
        subst_font: &mut CfxSubstFont,
    ) -> Option<SubstFace> {
        if weight == 0 {
            weight = i32::from(FONT_WEIGHT_NORMAL);
        }
        if flags & FONT_USE_EXTERN_ATTR == 0 {
            weight = i32::from(FONT_WEIGHT_NORMAL);
            italic_angle = 0;
        }
        let subst_name = get_subst_name(name, is_truetype);
        if subst_name == b"Symbol" && !is_truetype {
            subst_font.set_family(b"Chrome Symbol");
            subst_font.set_charset(FxCharset::Symbol as u8);
            return self.use_internal_subst(
                Some(StandardFontIndex::Symbol),
                weight,
                italic_angle,
                0,
                subst_font,
            );
        }
        if subst_name == b"ZapfDingbats" {
            subst_font.set_family(b"Chrome Dingbats");
            subst_font.set_charset(FxCharset::Symbol as u8);
            return self.use_internal_subst(
                Some(StandardFontIndex::Dingbats),
                weight,
                italic_angle,
                0,
                subst_font,
            );
        }

        let (mut family, style, has_comma) =
            if let Some(pos) = subst_name.iter().position(|&c| c == b',') {
                (
                    subst_name[..pos].to_vec(),
                    subst_name[pos + 1..].to_vec(),
                    true,
                )
            } else {
                (subst_name.clone(), Vec::new(), false)
            };
        let std_font = get_standard_font_index(&family);
        if has_comma {
            if let Some(index) = std_font {
                family = get_canonical_font_name(index).as_bytes().to_vec();
            }
        }
        let (mut n_style, mut pitch_family, mut base_font, has_hyphen) = if let Some(index) =
            std_font.filter(|index| (*index as u8) < StandardFontIndex::Symbol as u8)
        {
            (
                style_from_base_font(index),
                pitch_family_from_base_font(index),
                Some(index),
                false,
            )
        } else {
            let mut has_hyphen = false;
            let mut n_style = 0;
            if !has_comma {
                if let Some(pos) = family.iter().rposition(|&c| c == b'-') {
                    family.truncate(pos);
                    has_hyphen = true;
                }
            }
            if !has_hyphen {
                if let Some((len, style_bits)) = style_type(&family, true) {
                    family.truncate(family.len() - len);
                    n_style |= style_bits;
                }
            }
            (n_style, pitch_family_from_flags(flags), None, has_hyphen)
        };
        let style = if has_hyphen && !has_comma {
            subst_name[family.len() + 1..].to_vec()
        } else {
            style
        };
        let old_weight = weight;
        if font_style_is_force_bold(n_style) {
            weight = i32::from(FONT_WEIGHT_BOLD);
        }
        let mut is_style_available = false;
        if parse_styles(&style, &mut is_style_available, &mut weight, &mut n_style) {
            family = subst_name.clone();
            base_font = None;
        }
        if self.font_info.is_none() {
            return self.use_internal_subst(
                base_font,
                old_weight,
                italic_angle,
                pitch_family,
                subst_font,
            );
        }

        let charset = get_charset(codepage, base_font, flags);
        let is_cjk = charset_is_cjk(charset);
        let mut is_italic = font_style_is_italic(n_style);
        if let Some(alias) = get_font_family(n_style, &family) {
            family = alias.to_vec();
        }
        let mut matched = self.match_installed_fonts(&tt_normalize_name(&family));
        if matched.is_empty()
            && family != subst_name
            && !has_comma
            && (!has_hyphen || !is_style_available)
        {
            matched = self.match_installed_fonts(&tt_normalize_name(&subst_name));
        }
        if matched.is_empty() && base_font.is_none() {
            if !is_cjk {
                if !check_support_third_party_font(&family, &mut pitch_family) {
                    is_italic = italic_angle != 0;
                    if !self.skip_font_enumeration {
                        weight = old_weight;
                    }
                }
                if is_narrow_font_name(&subst_name) {
                    family = NARROW_FAMILY.to_vec();
                }
            } else {
                subst_font.set_subst_cjk(true);
                if n_style != 0 {
                    subst_font.set_weight_cjk(weight);
                }
                if font_style_is_italic(n_style) {
                    subst_font.set_italic_cjk(true);
                }
            }
            if font_style_is_italic(flags) {
                is_italic = true;
            }
        } else {
            italic_angle = 0;
            if n_style == 0 {
                weight = i32::from(FONT_WEIGHT_NORMAL);
            }
            if !matched.is_empty() {
                family = matched.clone();
            }
            if let Some(index) = base_font {
                let index = adjust_base_font_for_style(index, n_style);
                base_font = Some(index);
                family = get_canonical_font_name(index).as_bytes().to_vec();
            }
        }
        let font_handle =
            self.font_info
                .as_mut()?
                .map_font(weight, is_italic, charset, pitch_family, &family);
        if let Some(font_handle) = font_handle {
            return self
                .use_external_subst(
                    font_handle,
                    subst_name,
                    weight,
                    is_italic,
                    italic_angle,
                    charset,
                    subst_font,
                )
                .map(SubstFace::External);
        }
        if is_cjk {
            is_italic = italic_angle != 0;
            weight = old_weight;
        }
        if !matched.is_empty() {
            let font_handle = self.font_info.as_mut()?.get_font(&matched);
            return match font_handle {
                Some(font_handle) => self
                    .use_external_subst(
                        font_handle,
                        subst_name,
                        weight,
                        is_italic,
                        italic_angle,
                        charset,
                        subst_font,
                    )
                    .map(SubstFace::External),
                None => self.use_internal_subst(
                    base_font,
                    old_weight,
                    italic_angle,
                    pitch_family,
                    subst_font,
                ),
            };
        }
        if charset == FxCharset::Symbol {
            if subst_name == b"Symbol" {
                subst_font.set_family(b"Chrome Symbol");
                subst_font.set_charset(FxCharset::Symbol as u8);
                return self.use_internal_subst(
                    Some(StandardFontIndex::Symbol),
                    old_weight,
                    italic_angle,
                    pitch_family,
                    subst_font,
                );
            }
            return self.find_subst_face(
                &family,
                is_truetype,
                flags & !FONT_STYLE_SYMBOLIC,
                weight,
                italic_angle,
                FxCodePage::DefAnsi,
                subst_font,
            );
        }
        if charset == FxCharset::Ansi {
            return self.use_internal_subst(
                base_font,
                old_weight,
                italic_angle,
                pitch_family,
                subst_font,
            );
        }
        let Some(fallback) = self.first_face_for_charset(charset).map(<[u8]>::to_vec) else {
            return self.use_internal_subst(
                base_font,
                old_weight,
                italic_angle,
                pitch_family,
                subst_font,
            );
        };
        let font_handle = self.font_info.as_mut()?.get_font(&fallback)?;
        self.use_external_subst(
            font_handle,
            subst_name,
            weight,
            is_italic,
            italic_angle,
            charset,
            subst_font,
        )
        .map(SubstFace::External)
    }

    fn use_internal_subst(
        &mut self,
        base_font: Option<StandardFontIndex>,
        weight: i32,
        italic_angle: i32,
        pitch_family: u32,
        subst_font: &mut CfxSubstFont,
    ) -> Option<SubstFace> {
        if let Some(base_font) = base_font {
            let face = &mut self.standard_faces[base_font as usize];
            if face.is_none() {
                *face = CfxFace::new(get_font_data(base_font), 0).ok().map(Rc::new);
            }
            return face
                .as_ref()
                .map(|face| SubstFace::Standard(base_font, Rc::clone(face)));
        }
        subst_font.set_is_builtin_generic_font();
        subst_font.set_italic_angle(italic_angle);
        if weight != 0 {
            subst_font.set_weight(weight);
        }
        if pitch_family & FONT_PITCH_FAMILY_ROMAN != 0 {
            subst_font.use_chrome_serif();
            if self.generic_serif_face.is_none() {
                self.generic_serif_face = CfxFace::new(get_generic_serif_font_data(), 0)
                    .ok()
                    .map(Rc::new);
            }
            self.generic_serif_face
                .as_ref()
                .map(|face| SubstFace::GenericSerif(Rc::clone(face)))
        } else {
            subst_font.set_family(b"Chrome Sans");
            if self.generic_sans_face.is_none() {
                self.generic_sans_face = CfxFace::new(get_generic_sans_font_data(), 0)
                    .ok()
                    .map(Rc::new);
            }
            self.generic_sans_face
                .as_ref()
                .map(|face| SubstFace::GenericSans(Rc::clone(face)))
        }
    }
}

struct ScopedProviderFont<'a> {
    info: &'a mut dyn SystemFontInfo,
    handle: u64,
}
impl Drop for ScopedProviderFont<'_> {
    fn drop(&mut self) {
        self.info.delete_font(self.handle);
    }
}

fn read_font_data(
    font_info: &mut dyn SystemFontInfo,
    handle: u64,
    table: u32,
    size: usize,
) -> Option<Vec<u8>> {
    if size == 0 {
        return None;
    }
    let mut data = Vec::new();
    data.try_reserve_exact(size).ok()?;
    data.resize(size, 0);
    (font_info.get_font_data(handle, table, &mut data) == size).then_some(data)
}

fn get_ps_name_from_tt(font_info: &mut dyn SystemFontInfo, handle: u64) -> Vec<u8> {
    let size = font_info.get_font_data(handle, TABLE_NAME, &mut []);
    if size == 0 {
        return Vec::new();
    }
    let mut buffer = Vec::new();
    if buffer.try_reserve_exact(size).is_err() {
        return Vec::new();
    }
    buffer.resize(size, 0);
    if font_info.get_font_data(handle, TABLE_NAME, &mut buffer) != size {
        return Vec::new();
    }
    get_name_from_tt(&buffer, 6)
}

// CFX_FontMapper's kNarrowFamily is chosen before consulting the external
// provider. ChromeOS uses the Linux target; other desktop targets use Arial.
#[cfg(target_os = "linux")]
pub const NARROW_FAMILY: &[u8] = b"LiberationSansNarrow";
#[cfg(target_os = "android")]
pub const NARROW_FAMILY: &[u8] = b"RobotoCondensed";
#[cfg(not(any(target_os = "linux", target_os = "android")))]
pub const NARROW_FAMILY: &[u8] = b"ArialNarrow";

const ALT_FONT_FAMILIES: &[(&[u8], &[u8])] = &[
    (b"AGaramondPro", b"Adobe Garamond Pro"),
    (b"BankGothicBT-Medium", b"BankGothic Md BT"),
    (b"ForteMT", b"Forte"),
];

pub fn tt_normalize_name(name: &[u8]) -> Vec<u8> {
    let mut normalized: Vec<u8> = name
        .iter()
        .copied()
        .filter(|&ch| !matches!(ch, b' ' | b'-' | b','))
        .collect();
    if let Some(position) = normalized.iter().position(|&ch| ch == b'+') {
        if position != 0 {
            normalized.truncate(position);
        }
    }
    crate::fxcrt::fx_system::strlwr(&mut normalized);
    normalized
}

pub fn get_font_family(style: u32, font_name: &[u8]) -> Option<&'static [u8]> {
    if contains(font_name, b"Script") {
        if font_style_is_force_bold(style) {
            return Some(b"ScriptMTBold");
        }
        for (part, alternate) in [
            (b"Palace".as_slice(), b"PalaceScriptMT".as_slice()),
            (b"French".as_slice(), b"FrenchScriptMT".as_slice()),
            (b"FreeStyle".as_slice(), b"FreeStyleScript".as_slice()),
        ] {
            if contains(font_name, part) {
                return Some(alternate);
            }
        }
        return None;
    }
    ALT_FONT_FAMILIES
        .iter()
        .find(|(part, _)| contains(font_name, part))
        .map(|(_, name)| *name)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|part| part == needle)
}

fn style_type(font_name: &[u8], reverse: bool) -> Option<(usize, u32)> {
    const STYLES: &[(&[u8], u32)] = &[
        (b"Regular", 0),
        (b"Reg", 0),
        (b"BoldItalic", FONT_STYLE_FORCE_BOLD | FONT_STYLE_ITALIC),
        (b"Italic", FONT_STYLE_ITALIC),
        (b"Bold", FONT_STYLE_FORCE_BOLD),
    ];
    STYLES
        .iter()
        .find(|(name, _)| {
            if reverse {
                font_name.ends_with(name)
            } else {
                font_name.starts_with(name)
            }
        })
        .map(|(name, style)| (name.len(), *style))
}

/// Returns PDFium's stop signal. Style, weight, and availability are updated
/// even when a later comma-separated component is not recognized.
pub fn parse_styles(
    style_string: &[u8],
    available: &mut bool,
    weight: &mut i32,
    style: &mut u32,
) -> bool {
    if style_string.is_empty() {
        return false;
    }
    let mut position = 0;
    let mut first_item = true;
    while position < style_string.len() {
        let end = style_string[position..]
            .iter()
            .position(|&ch| ch == b',')
            .map_or(style_string.len(), |offset| position + offset);
        let part = &style_string[position..end];
        let parsed = style_type(part, false).map(|(_, style)| style);
        if (position != 0 && !*available) || (position == 0 && parsed.is_none()) {
            return true;
        }
        let parsed_style = parsed.unwrap_or(0);
        if parsed.is_some() {
            *available = true
        }
        if font_style_is_force_bold(parsed_style) {
            if font_style_is_force_bold(*style) {
                *weight = i32::from(FONT_WEIGHT_EXTRA_BOLD);
            } else {
                *weight = i32::from(FONT_WEIGHT_BOLD);
                *style |= FONT_STYLE_FORCE_BOLD;
            }
            first_item = false;
        }
        if font_style_is_italic(parsed_style) && font_style_is_force_bold(parsed_style) {
            *style |= FONT_STYLE_ITALIC;
        } else if font_style_is_italic(parsed_style) {
            if !first_item {
                return true;
            }
            *style |= FONT_STYLE_ITALIC;
            break;
        }
        position = end + 1;
    }
    false
}

pub fn check_support_third_party_font(name: &[u8], pitch_family: &mut u32) -> bool {
    if name != b"MyriadPro" {
        return false;
    }
    *pitch_family &= !FONT_PITCH_FAMILY_ROMAN;
    true
}

pub fn is_stylable_base_font(font: StandardFontIndex) -> bool {
    matches!(
        font,
        StandardFontIndex::Courier | StandardFontIndex::Helvetica | StandardFontIndex::Times
    )
}

pub fn style_from_base_font(font: StandardFontIndex) -> u32 {
    if font as u8 >= StandardFontIndex::Symbol as u8 {
        return 0;
    }
    let position = font as u8 % 4;
    let mut style = 0;
    if position == 1 || position == 2 {
        style |= FONT_STYLE_FORCE_BOLD
    }
    if position / 2 != 0 {
        style |= FONT_STYLE_ITALIC
    }
    style
}

pub fn pitch_family_from_base_font(font: StandardFontIndex) -> u32 {
    if (font as u8) < 4 {
        FONT_PITCH_FAMILY_FIXED
    } else if (font as u8) >= 8 {
        FONT_PITCH_FAMILY_ROMAN
    } else {
        0
    }
}

pub fn pitch_family_from_flags(flags: u32) -> u32 {
    let mut family = 0;
    if font_style_is_serif(flags) {
        family |= FONT_PITCH_FAMILY_ROMAN
    }
    if font_style_is_script(flags) {
        family |= FONT_PITCH_FAMILY_SCRIPT
    }
    if font_style_is_fixed_pitch(flags) {
        family |= FONT_PITCH_FAMILY_FIXED
    }
    family
}

pub fn adjust_base_font_for_style(font: StandardFontIndex, style: u32) -> StandardFontIndex {
    if style == 0 || !is_stylable_base_font(font) {
        return font;
    }
    let offset = if font_style_is_force_bold(style) && font_style_is_italic(style) {
        2
    } else if font_style_is_force_bold(style) {
        1
    } else if font_style_is_italic(style) {
        3
    } else {
        0
    };
    const FONTS: [StandardFontIndex; 14] = [
        StandardFontIndex::Courier,
        StandardFontIndex::CourierBold,
        StandardFontIndex::CourierBoldOblique,
        StandardFontIndex::CourierOblique,
        StandardFontIndex::Helvetica,
        StandardFontIndex::HelveticaBold,
        StandardFontIndex::HelveticaBoldOblique,
        StandardFontIndex::HelveticaOblique,
        StandardFontIndex::Times,
        StandardFontIndex::TimesBold,
        StandardFontIndex::TimesBoldOblique,
        StandardFontIndex::TimesOblique,
        StandardFontIndex::Symbol,
        StandardFontIndex::Dingbats,
    ];
    FONTS[font as usize + offset]
}

pub fn get_charset(
    codepage: FxCodePage,
    base_font: Option<StandardFontIndex>,
    flags: u32,
) -> FxCharset {
    if codepage != FxCodePage::DefAnsi {
        return charset_from_codepage(codepage);
    }
    if font_style_is_symbolic(flags) && base_font.is_none() {
        FxCharset::Symbol
    } else {
        FxCharset::Ansi
    }
}

pub fn get_subst_name(name: &[u8], is_truetype: bool) -> Vec<u8> {
    let mut name: Vec<u8> = if is_truetype && name.first() == Some(&b'@') {
        name[1..].to_vec()
    } else {
        name.iter().copied().filter(|&ch| ch != b' ').collect()
    };
    if name.len() > 6 && name[6] == b'+' && name[..6].iter().all(u8::is_ascii_uppercase) {
        name.drain(..7);
    }
    if let Some(index) = get_standard_font_index(&name) {
        name = get_canonical_font_name(index).as_bytes().to_vec();
    }
    name
}

pub fn font_name_from_face(face: &CfxFace<'_>) -> Vec<u8> {
    let family = face.family_name();
    if family.is_empty() {
        return Vec::new();
    }
    let style = face.style_name();
    if style.is_empty() || style == "Regular" {
        return family.into_bytes();
    }
    format!("{family} {style}").into_bytes()
}

pub fn is_narrow_font_name(name: &[u8]) -> bool {
    [b"Narrow".as_slice(), b"Condensed".as_slice()]
        .iter()
        .any(|part| {
            name.windows(part.len())
                .position(|window| window == *part)
                .is_some_and(|pos| pos != 0)
        })
}

pub fn checksum_from_tt(data: &[u8]) -> u32 {
    data[..data.len().min(1024)]
        .chunks_exact(4)
        .fold(0u32, |sum, word| {
            sum.wrapping_add(u32::from_be_bytes([word[0], word[1], word[2], word[3]]))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    struct TestFontInfo {
        enum_count: Rc<Cell<usize>>,
        delete_count: Rc<Cell<usize>>,
        font_data_reads: Rc<Cell<usize>>,
        map_requests: Rc<RefCell<Vec<(i32, bool, FxCharset, u32, Vec<u8>)>>>,
        map_available: bool,
        fonts: Vec<(Vec<u8>, FxCharset)>,
        name_table: Vec<u8>,
        font_data: Vec<u8>,
        short_reads: bool,
    }

    impl SystemFontInfo for TestFontInfo {
        fn enum_font_list(&mut self) -> Vec<(Vec<u8>, FxCharset)> {
            self.enum_count.set(self.enum_count.get() + 1);
            self.fonts.clone()
        }

        fn map_font(
            &mut self,
            weight: i32,
            italic: bool,
            charset: FxCharset,
            pitch_family: u32,
            face: &[u8],
        ) -> Option<u64> {
            self.map_requests.borrow_mut().push((
                weight,
                italic,
                charset,
                pitch_family,
                face.to_vec(),
            ));
            self.map_available.then_some(1)
        }

        fn get_font(&mut self, _: &[u8]) -> Option<u64> {
            Some(1)
        }

        fn get_font_data(&mut self, handle: u64, table: u32, buffer: &mut [u8]) -> usize {
            if handle != 1 {
                return 0;
            }
            let data = match table {
                TABLE_NAME => &self.name_table,
                TABLE_NONE => &self.font_data,
                TABLE_TTCF if self.font_data.starts_with(b"ttcf") => &self.font_data,
                _ => return 0,
            };
            if buffer.is_empty() {
                return data.len();
            }
            self.font_data_reads.set(self.font_data_reads.get() + 1);
            if buffer.len() != data.len() {
                return 0;
            }
            if self.short_reads {
                let count = data.len().saturating_sub(1);
                buffer[..count].copy_from_slice(&data[..count]);
                return count;
            }
            buffer.copy_from_slice(data);
            data.len()
        }

        fn get_face_name(&mut self, _: u64) -> Option<Vec<u8>> {
            None
        }

        fn get_font_charset(&mut self, _: u64) -> Option<FxCharset> {
            None
        }

        fn delete_font(&mut self, _: u64) {
            self.delete_count.set(self.delete_count.get() + 1);
        }
    }

    fn test_info(
        fonts: Vec<(Vec<u8>, FxCharset)>,
    ) -> (TestFontInfo, Rc<Cell<usize>>, Rc<Cell<usize>>) {
        let enum_count = Rc::new(Cell::new(0));
        let delete_count = Rc::new(Cell::new(0));
        // One Macintosh Roman PostScript name record, name ID 6.
        let mut name_table = vec![0, 0, 0, 1, 0, 18];
        name_table.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 6, 0, 7, 0, 0]);
        name_table.extend_from_slice(b"PS Name");
        (
            TestFontInfo {
                enum_count: enum_count.clone(),
                delete_count: delete_count.clone(),
                font_data_reads: Rc::new(Cell::new(0)),
                map_requests: Rc::new(RefCell::new(Vec::new())),
                map_available: true,
                fonts,
                name_table,
                font_data: Vec::new(),
                short_reads: false,
            },
            enum_count,
            delete_count,
        )
    }

    #[test]
    fn xfa_raw_font_streams_validate_reads_and_release_provider_handles() {
        assert!(CfxFontMapper::new().raw_bytes_for_index(0).is_empty());
        for (data, short, available, expected_deletes) in [
            (b"font bytes".as_slice(), false, true, 1),
            (b"ttcf complete collection".as_slice(), false, true, 1),
            (b"".as_slice(), false, true, 1),
            (b"short read".as_slice(), true, true, 1),
            (b"unavailable".as_slice(), false, false, 0),
        ] {
            let (mut info, _, deletes) = test_info(vec![(b"Installed".to_vec(), FxCharset::Ansi)]);
            info.font_data = data.to_vec();
            info.short_reads = short;
            info.map_available = available;
            let requests = info.map_requests.clone();
            let mut mapper = CfxFontMapper::new();
            mapper.set_system_font_info(Some(Box::new(info)));
            mapper.load_installed_fonts();
            let result = mapper.raw_bytes_for_index(0);
            assert_eq!(result, if short || !available { &[][..] } else { data });
            assert_eq!(deletes.get(), expected_deletes);
            assert_eq!(
                requests.borrow().as_slice(),
                &[(0, false, FxCharset::Default, 0, b"Installed".to_vec())]
            );
            assert!(mapper.raw_bytes_for_index(1).is_empty());
            assert_eq!(deletes.get(), expected_deletes);
        }
    }

    #[test]
    fn substitution_selects_base_font_or_generic_family_without_provider() {
        let mut mapper = CfxFontMapper::new();
        let mut subst = CfxSubstFont::new();
        assert!(matches!(
            mapper.find_subst_face(
                b"Arial-Bold",
                true,
                0,
                0,
                0,
                FxCodePage::DefAnsi,
                &mut subst,
            ),
            Some(SubstFace::Standard(StandardFontIndex::HelveticaBold, _))
        ));
        assert!(matches!(
            mapper.find_subst_face(
                b"UnknownSerif",
                true,
                crate::fxge::fx_font::FONT_STYLE_SERIF | FONT_USE_EXTERN_ATTR,
                700,
                -12,
                FxCodePage::DefAnsi,
                &mut subst,
            ),
            Some(SubstFace::GenericSerif(_))
        ));
        assert_eq!(subst.family(), b"Chrome Serif");
        assert_eq!(subst.weight(), 560);
        assert_eq!(subst.italic_angle(), -12);
        assert!(matches!(
            mapper.find_subst_face(
                b"ZapfDingbats",
                false,
                0,
                0,
                0,
                FxCodePage::DefAnsi,
                &mut CfxSubstFont::new(),
            ),
            Some(SubstFace::Standard(StandardFontIndex::Dingbats, _))
        ));
    }

    #[test]
    fn missing_installed_charset_uses_internal_substitute() {
        let (mut info, _, _) = test_info(Vec::new());
        info.map_available = false;
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(info)));
        let face = mapper.find_subst_face(
            b"UnknownCjk",
            true,
            0,
            400,
            0,
            FxCodePage::ShiftJis,
            &mut CfxSubstFont::new(),
        );
        assert!(matches!(face, Some(SubstFace::GenericSans(_))));
        assert!(face.unwrap().face().glyph_count() > 0);
    }

    #[test]
    fn substitution_maps_matched_name_then_uses_charset_fallback() {
        let (mut info, _, _) = test_info(vec![(b"Ahem".to_vec(), FxCharset::ShiftJis)]);
        info.font_data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf")
                .to_vec();
        info.map_available = false;
        let requests = info.map_requests.clone();
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(info)));
        let mut subst = CfxSubstFont::new();
        assert!(matches!(
            mapper.find_subst_face(b"Ahem", true, 0, 400, 0, FxCodePage::DefAnsi, &mut subst,),
            Some(SubstFace::External(_))
        ));
        assert_eq!(requests.borrow()[0].4, b"Ahem");
        assert_eq!(subst.family(), b"Ahem");

        let mut subst = CfxSubstFont::new();
        assert!(matches!(
            mapper.find_subst_face(
                b"MissingCID",
                true,
                0,
                400,
                0,
                FxCodePage::ShiftJis,
                &mut subst,
            ),
            Some(SubstFace::External(_))
        ));
        assert_eq!(requests.borrow()[1].2, FxCharset::ShiftJis);
        assert_eq!(requests.borrow()[1].4, b"MissingCID");
    }

    #[test]
    fn narrow_substitution_requests_the_platform_family_only_for_unmatched_faces() {
        // Source-derived provider contract from FindSubstFace/kNarrowFamily.
        // An installed match retains its name rather than using this fallback.
        let narrow_family: &[u8] = if cfg!(target_os = "linux") {
            b"LiberationSansNarrow"
        } else if cfg!(target_os = "android") {
            b"RobotoCondensed"
        } else {
            b"ArialNarrow"
        };
        for name in [
            b"RecoveryNarrow".as_slice(),
            b"RecoveryCondensed".as_slice(),
        ] {
            for installed in [false, true] {
                let fonts = if installed {
                    vec![(name.to_vec(), FxCharset::Ansi)]
                } else {
                    Vec::new()
                };
                let (mut info, enumerations, deletions) = test_info(fonts);
                info.font_data = include_bytes!(
                    "../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
                )
                .to_vec();
                let requests = info.map_requests.clone();
                let mut mapper = CfxFontMapper::new();
                mapper.set_system_font_info(Some(Box::new(info)));
                let face = mapper.find_subst_face(
                    name,
                    true,
                    0,
                    400,
                    0,
                    FxCodePage::DefAnsi,
                    &mut CfxSubstFont::new(),
                );
                assert!(matches!(face, Some(SubstFace::External(_))));
                assert_eq!(enumerations.get(), 1);
                assert_eq!(deletions.get(), 1);
                assert_eq!(
                    requests.borrow().as_slice(),
                    &[(
                        400,
                        false,
                        FxCharset::Ansi,
                        0,
                        if installed { name } else { narrow_family }.to_vec()
                    )]
                );
            }
        }
    }

    #[test]
    fn normal_face_cache_uses_weak_entries_and_keeps_font_bytes_alive() {
        let (mut info, _, _) = test_info(Vec::new());
        let font_bytes =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        info.font_data = font_bytes.to_vec();
        let reads = info.font_data_reads.clone();
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(info)));
        let first = mapper
            .get_cached_face(1, b"Ahem", 400, false, font_bytes.len())
            .unwrap();
        let second = mapper
            .get_cached_face(1, b"Ahem", 400, false, font_bytes.len())
            .unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert_eq!(reads.get(), 1);
        assert_eq!(first.face().family_name(), "Ahem");
        drop(first);
        drop(second);
        assert!(mapper
            .get_cached_face(1, b"Ahem", 400, false, font_bytes.len())
            .is_some());
        assert_eq!(reads.get(), 2);
        assert!(mapper
            .get_cached_face(1, b"Ahem", 400, false, font_bytes.len() + 1)
            .is_none());
    }

    #[test]
    fn ttc_face_cache_selects_collection_offset_and_reuses_live_face() {
        let (mut info, _, _) = test_info(Vec::new());
        let mut sfnt =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf")
                .to_vec();
        let table_count = usize::from(u16::from_be_bytes([sfnt[4], sfnt[5]]));
        for table in 0..table_count {
            let offset = 12 + table * 16 + 8;
            let original = u32::from_be_bytes(sfnt[offset..offset + 4].try_into().unwrap());
            sfnt[offset..offset + 4].copy_from_slice(&(original + 16).to_be_bytes());
        }
        let sfnt_size = sfnt.len();
        let mut ttc = b"ttcf\0\x01\0\0\0\0\0\x01\0\0\0\x10".to_vec();
        ttc.extend_from_slice(&sfnt);
        let ttc_size = ttc.len();
        info.font_data = ttc;
        let reads = info.font_data_reads.clone();
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(info)));
        let face = mapper.get_cached_ttc_face(1, ttc_size, sfnt_size).unwrap();
        assert_eq!(face.face().family_name(), "Ahem");
        let reused = mapper.get_cached_ttc_face(1, ttc_size, sfnt_size).unwrap();
        assert!(Rc::ptr_eq(&face, &reused));
        assert_eq!(reads.get(), 2);
        assert!(mapper
            .get_cached_ttc_face(1, sfnt_size - 1, sfnt_size)
            .is_none());
    }

    #[test]
    fn external_subst_reads_face_and_releases_handle_on_success_or_failure() {
        let (mut info, _, delete_count) = test_info(Vec::new());
        info.font_data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf")
                .to_vec();
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(info)));
        let mut subst = CfxSubstFont::new();
        let face = mapper
            .use_external_subst(
                1,
                b"Fallback".to_vec(),
                700,
                true,
                -12,
                FxCharset::Default,
                &mut subst,
            )
            .unwrap();
        assert_eq!(subst.family(), b"Ahem");
        assert_eq!(subst.charset(), FxCharset::Default as u8);
        assert_eq!(subst.weight(), 700);
        assert_eq!(subst.italic_angle(), -12);
        assert_eq!(face.face().family_name(), "Ahem");
        assert_eq!(delete_count.get(), 1);

        let mut failed_subst = CfxSubstFont::new();
        assert!(mapper
            .use_external_subst(
                2,
                b"Invalid".to_vec(),
                400,
                false,
                0,
                FxCharset::Ansi,
                &mut failed_subst,
            )
            .is_none());
        assert_eq!(delete_count.get(), 2);
    }

    #[test]
    fn installed_fonts_match_last_normalized_name_and_localized_ps_name() {
        let (info, enum_count, delete_count) = test_info(vec![
            (b"Foo Bar".to_vec(), FxCharset::Ansi),
            (b"Foo-Bar".to_vec(), FxCharset::Ansi),
            (vec![0x81, b'X'], FxCharset::ShiftJis),
        ]);
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(info)));
        assert_eq!(mapper.match_installed_fonts(b"foobar"), b"Foo-Bar");
        assert_eq!(mapper.match_installed_fonts(b"psname"), [0x81, b'X']);
        assert_eq!(enum_count.get(), 1);
        assert_eq!(delete_count.get(), 1);
        assert_eq!(mapper.face_count(), 3);
        assert_eq!(
            mapper.first_face_for_charset(FxCharset::ShiftJis),
            Some([0x81, b'X'].as_slice())
        );
        assert!(mapper.has_localized_font(b"PS Name"));
        assert!(mapper.has_installed_font(&[0x81, b'X']));
    }

    #[test]
    fn enumeration_can_be_skipped_but_directly_added_fonts_remain_available() {
        let (info, enum_count, _) = test_info(vec![(b"Skipped".to_vec(), FxCharset::Ansi)]);
        let mut mapper = CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(info)));
        mapper.set_skip_font_enumeration(true);
        mapper.add_installed_font(b"Direct", FxCharset::Ansi);
        mapper.add_installed_font(b"Direct", FxCharset::Ansi);
        assert_eq!(mapper.match_installed_fonts(b"direct"), b"Direct");
        assert_eq!(enum_count.get(), 0);
        assert_eq!(mapper.face_count(), 2);
        assert_eq!(mapper.face_name(1), Some(b"Direct".as_slice()));
        assert_eq!(mapper.match_installed_fonts(b"skipped"), b"");
    }

    #[test]
    fn mapper_names_and_base_font_style_follow_pdfium_order() {
        assert_eq!(tt_normalize_name(b"Times-New,Roman+Bold"), b"timesnewroman");
        assert_eq!(tt_normalize_name(b"+Leading"), b"+leading");
        assert_eq!(tt_normalize_name(b"ABC\0DEF"), b"abc\0DEF");
        assert_eq!(
            get_subst_name(b"ABCDEF+Arial,Bold", false),
            b"Helvetica-Bold"
        );
        assert_eq!(get_subst_name(b"@Arial Narrow", true), b"Arial Narrow");
        assert_eq!(get_subst_name(b"Arial Narrow", false), b"ArialNarrow");
        assert_eq!(
            get_font_family(FONT_STYLE_FORCE_BOLD, b"PalaceScript"),
            Some(b"ScriptMTBold".as_slice())
        );
        assert_eq!(
            get_font_family(0, b"PalaceScript"),
            Some(b"PalaceScriptMT".as_slice())
        );
        assert!(is_narrow_font_name(b"ArialNarrow"));
        assert!(!is_narrow_font_name(b"Narrow"));
        assert_eq!(
            style_from_base_font(StandardFontIndex::CourierBoldOblique),
            FONT_STYLE_FORCE_BOLD | FONT_STYLE_ITALIC
        );
        assert_eq!(
            adjust_base_font_for_style(StandardFontIndex::Times, FONT_STYLE_ITALIC),
            StandardFontIndex::TimesOblique
        );
        assert_eq!(
            pitch_family_from_base_font(StandardFontIndex::Courier),
            FONT_PITCH_FAMILY_FIXED
        );
        assert_eq!(
            get_charset(
                FxCodePage::DefAnsi,
                None,
                crate::fxge::fx_font::FONT_STYLE_SYMBOLIC
            ),
            FxCharset::Symbol
        );
        assert_eq!(checksum_from_tt(&[0, 0, 0, 1, 0, 0, 0, 2, 255]), 3);
    }

    #[test]
    fn comma_separated_style_parsing_keeps_double_bold_rule() {
        let mut available = false;
        let mut weight = 400;
        let mut style = 0;
        assert!(!parse_styles(
            b"Bold,BoldItalic",
            &mut available,
            &mut weight,
            &mut style
        ));
        assert!(available);
        assert_eq!(weight, i32::from(FONT_WEIGHT_EXTRA_BOLD));
        assert_eq!(style, FONT_STYLE_FORCE_BOLD | FONT_STYLE_ITALIC);

        let mut available = false;
        let mut weight = 400;
        let mut style = 0;
        assert!(parse_styles(
            b"Unknown,Bold",
            &mut available,
            &mut weight,
            &mut style
        ));
        assert!(!available);
        assert_eq!(style, 0);
        assert!(!parse_styles(
            b"Italic",
            &mut available,
            &mut weight,
            &mut style
        ));
        assert_eq!(style, FONT_STYLE_ITALIC);
    }
}

#[cfg(test)]
#[path = "cfx_fontmapper_unittest.rs"]
mod upstream_tests;
