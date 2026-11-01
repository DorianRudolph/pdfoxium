// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/font/cfgas_gefont.{h,cpp}` and the XFA alternate charmap resolver.
//! Font owners are shared without cloning font data. Substitution edges retain
//! the font resource and weakly reference its wrapper, avoiding A<->B cycles.
use super::{cfgas_fontmgr::CfgasFontMgr, cfgas_gemodule, fgas_fontutils::get_unicode_bit_field};
use crate::fpdfapi::{font::cpdf_font::CpdfFont, parser::cpdf_document::CpdfDocument};
use crate::fxcrt::{fx_codepage::FxCodePage, fx_coordinates::FxRect};
use crate::fxge::{
    cfx_face::charmap_id_pair_to_font_encoding,
    cfx_font::CfxFont,
    fx_font::{FONT_STYLE_FORCE_BOLD, FONT_STYLE_ITALIC},
    fx_fontencoding::FontEncoding,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};

enum FontOwner {
    Owned(Rc<CfxFont<'static>>),
    Pdf(CpdfFont),
}
impl FontOwner {
    fn font(&self) -> &CfxFont<'static> {
        match self {
            Self::Owned(font) => font,
            Self::Pdf(font) => font.font().expect("validated PDF font owner"),
        }
    }
}
struct SubstituteFont {
    owner: Rc<FontOwner>,
    font: Weak<CfgasGeFont>,
    style: Option<u32>,
    manager: Weak<CfgasFontMgr>,
}
pub struct CfgasGeFont {
    owner: Rc<FontOwner>,
    encoding: Cell<FontEncoding>,
    log_font_style: Cell<Option<u32>>,
    widths: RefCell<BTreeMap<u32, Option<u16>>>,
    boxes: RefCell<BTreeMap<u32, FxRect>>,
    substitutes: RefCell<Vec<SubstituteFont>>,
    font_mapper: RefCell<BTreeMap<u32, usize>>,
    manager: Weak<CfgasFontMgr>,
}
impl std::fmt::Debug for CfgasGeFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CfgasGeFont")
            .field("family", &self.get_dev_font().family_name())
            .field("style", &self.get_font_styles())
            .finish()
    }
}
const ENCODINGS: [FontEncoding; 13] = [
    FontEncoding::Symbol,
    FontEncoding::Unicode,
    FontEncoding::Sjis,
    FontEncoding::GB2312,
    FontEncoding::Big5,
    FontEncoding::Wansung,
    FontEncoding::Johab,
    FontEncoding::AdobeStandard,
    FontEncoding::AdobeExpert,
    FontEncoding::AdobeCustom,
    FontEncoding::Latin1,
    FontEncoding::OldLatin2,
    FontEncoding::AppleRoman,
];
impl CfgasGeFont {
    fn from_owner(
        owner: Rc<FontOwner>,
        manager: Weak<CfgasFontMgr>,
        style: Option<u32>,
    ) -> Option<Rc<Self>> {
        let face = owner.font().face()?;
        let encoding = ENCODINGS.into_iter().find(|&id| face.select_charmap(id))?;
        Some(Rc::new(Self {
            owner,
            encoding: Cell::new(encoding),
            log_font_style: Cell::new(style),
            widths: RefCell::new(BTreeMap::new()),
            boxes: RefCell::new(BTreeMap::new()),
            substitutes: RefCell::new(Vec::new()),
            font_mapper: RefCell::new(BTreeMap::new()),
            manager,
        }))
    }
    pub fn from_font(font: CfxFont<'static>) -> Option<Rc<Self>> {
        Self::from_owner(Rc::new(FontOwner::Owned(Rc::new(font))), Weak::new(), None)
    }
    pub(super) fn from_font_with_manager(
        font: CfxFont<'static>,
        manager: Weak<CfgasFontMgr>,
    ) -> Option<Rc<Self>> {
        Self::from_owner(Rc::new(FontOwner::Owned(Rc::new(font))), manager, None)
    }
    pub fn from_pdf_font(font: CpdfFont) -> Option<Rc<Self>> {
        font.font()?;
        Self::from_owner(Rc::new(FontOwner::Pdf(font)), Weak::new(), None)
    }
    pub fn load_stock_font(document: &CpdfDocument, family: &[u8]) -> Option<Rc<Self>> {
        Self::from_pdf_font(CpdfFont::get_stock_font(document, family)?)
    }
    pub fn load_font(
        family: Option<&[u32]>,
        styles: u32,
        code_page: FxCodePage,
    ) -> Option<Rc<Self>> {
        cfgas_gemodule::font_manager().get_font_by_code_page(code_page, styles, family)
    }
    pub fn get_dev_font(&self) -> &CfxFont<'static> {
        self.owner.font()
    }
    pub fn set_logical_font_style(&self, style: u32) {
        self.log_font_style.set(Some(style));
    }
    pub fn get_font_styles(&self) -> u32 {
        if let Some(style) = self.log_font_style.get() {
            return style;
        }
        let font = self.get_dev_font();
        let mut style = 0;
        if let Some(subst) = font.subst_font() {
            if subst.is_bold() {
                style |= FONT_STYLE_FORCE_BOLD;
            }
        } else {
            if font.face().is_some_and(|face| face.is_bold()) {
                style |= FONT_STYLE_FORCE_BOLD;
            }
            if font.face().is_some_and(|face| face.is_italic()) {
                style |= FONT_STYLE_ITALIC;
            }
        }
        style
    }
    fn family_name(&self) -> Vec<u32> {
        let font = self.get_dev_font();
        let family = font
            .subst_font()
            .filter(|subst| !subst.family().is_empty())
            .map(|subst| String::from_utf8_lossy(subst.family()).into_owned())
            .unwrap_or_else(|| font.family_name());
        family.chars().map(u32::from).collect()
    }
    fn own_glyph_index(&self, code: u32) -> u32 {
        let Some(face) = self.get_dev_font().face() else {
            return 0;
        };
        let glyph = face.get_char_index(code);
        if glyph > 0 {
            return glyph;
        }
        for index in 0..face.charmap_count() {
            let encoding = charmap_id_pair_to_font_encoding(face.charmap_id_pair_by_index(index));
            if encoding == self.encoding.get() || !face.select_charmap(encoding) {
                continue;
            }
            let glyph = face.get_char_index(code);
            if glyph > 0 {
                self.encoding.set(encoding);
                return glyph;
            }
        }
        face.select_charmap(self.encoding.get());
        0
    }
    fn substitute(&self, index: usize) -> Option<Rc<Self>> {
        let (owner, style, manager) = {
            let entries = self.substitutes.borrow();
            let entry = entries.get(index)?;
            if let Some(font) = entry.font.upgrade() {
                return Some(font);
            }
            (entry.owner.clone(), entry.style, entry.manager.clone())
        };
        let font = Self::from_owner(owner, manager, style)?;
        self.substitutes.borrow_mut().get_mut(index)?.font = Rc::downgrade(&font);
        Some(font)
    }
    fn glyph_index_and_font(
        self: &Rc<Self>,
        code: u32,
        recursive: bool,
    ) -> Option<(u32, Rc<Self>)> {
        let glyph = self.own_glyph_index(code);
        if glyph > 0 {
            return Some((glyph, self.clone()));
        }
        if get_unicode_bit_field(code).is_none_or(|entry| entry.bit_field >= 128) {
            return None;
        }
        let index = self.font_mapper.borrow().get(&code).copied();
        if let Some(index) = index {
            if let Some(font) = self.substitute(index) {
                let glyph = font.own_glyph_index(code);
                if glyph != 0 {
                    return Some((glyph | (((index + 1) as u32) << 24), font));
                }
            }
        }
        if !recursive {
            return None;
        }
        let manager = self
            .manager
            .upgrade()
            .unwrap_or_else(cfgas_gemodule::font_manager);
        let family = self.family_name();
        let styles = self.get_font_styles();
        let font = manager
            .get_font_by_unicode(code, styles, Some(&family))
            .or_else(|| manager.get_font_by_unicode(code, styles, None))?;
        if Rc::ptr_eq(&font, self) || Rc::ptr_eq(&font.owner, &self.owner) {
            return None;
        }
        let glyph = font.own_glyph_index(code);
        if glyph == 0 {
            return None;
        }
        let mut substitutes = self.substitutes.borrow_mut();
        // Native code appends the same font once per new character and then
        // finds its first entry on subsequent lookups. Use one stable ID per
        // font resource, avoiding both changing IDs and eight-bit exhaustion
        // for ordinary text containing more than 255 fallback characters.
        if let Some(index) = substitutes
            .iter()
            .position(|entry| Rc::ptr_eq(&entry.owner, &font.owner))
        {
            drop(substitutes);
            self.font_mapper.borrow_mut().insert(code, index);
            return Some((glyph | (((index + 1) as u32) << 24), font));
        }
        // The packed source format has only eight bits for the substitute ID.
        // Avoid wrapping an invalid index back onto the original font.
        if substitutes.len() >= 255 {
            return None;
        }
        let index = substitutes.len();
        substitutes.push(SubstituteFont {
            owner: font.owner.clone(),
            font: Rc::downgrade(&font),
            style: font.log_font_style.get(),
            manager: font.manager.clone(),
        });
        drop(substitutes);
        self.font_mapper.borrow_mut().insert(code, index);
        Some((glyph | (((index + 1) as u32) << 24), font))
    }
    pub fn get_glyph_index(self: &Rc<Self>, code: u32) -> i32 {
        self.glyph_index_and_font(code, true)
            .map_or(0xffff, |(glyph, _)| glyph as i32)
    }
    pub fn get_char_width(self: &Rc<Self>, code: u32) -> Option<u16> {
        if let Some(width) = self.widths.borrow().get(&code) {
            return *width;
        }
        let result = self
            .glyph_index_and_font(code, true)
            .and_then(|(glyph, font)| {
                let width = font.get_dev_font().get_glyph_width(glyph & 0x00ff_ffff, 0);
                u16::try_from(width).ok()
            });
        self.widths.borrow_mut().insert(code, result);
        result
    }
    pub fn get_char_bbox(self: &Rc<Self>, code: u32) -> Option<FxRect> {
        if let Some(bbox) = self.boxes.borrow().get(&code) {
            return Some(*bbox);
        }
        let (glyph, font) = self.glyph_index_and_font(code, true)?;
        let bbox = font.get_dev_font().get_glyph_bbox(glyph & 0x00ff_ffff)?;
        self.boxes.borrow_mut().insert(code, bbox);
        Some(bbox)
    }
    pub fn get_ascent(&self) -> i32 {
        self.get_dev_font().get_ascent()
    }
    pub fn get_descent(&self) -> i32 {
        self.get_dev_font().get_descent()
    }
    pub fn get_subst_font(self: &Rc<Self>, glyph: i32) -> Option<Rc<Self>> {
        let index = (glyph as u32) >> 24;
        if index == 0 {
            Some(self.clone())
        } else {
            self.substitute(index as usize - 1)
        }
    }
    pub fn verify_unicode(&self, code: u32) -> bool {
        let Some(face) = self.get_dev_font().face() else {
            return false;
        };
        let previous = face.current_charmap();
        if !face.select_charmap(FontEncoding::Unicode) {
            return false;
        }
        if face.get_char_index(code) == 0 {
            face.set_charmap(previous);
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn font() -> Rc<CfgasGeFont> {
        CfgasGeFont::from_font(
            CfxFont::from_span(
                include_bytes!(
                    "../../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
                ),
                false,
                0,
            )
            .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn own_metrics_packed_ids_and_pdf_font_lifetimes() {
        let font = font();
        assert!(font.get_glyph_index(u32::from('A')) > 0);
        assert!(font.get_char_width(u32::from('A')).is_some());
        assert!(font.get_char_bbox(u32::from('A')).is_some());
        assert!(font.get_ascent() > 0);
        assert!(font.get_descent() <= 0);
        assert!(Rc::ptr_eq(&font, &font.get_subst_font(1).unwrap()));
        assert!(font.get_subst_font(0x7f00_0001).is_none());
        font.set_logical_font_style(FONT_STYLE_FORCE_BOLD | FONT_STYLE_ITALIC);
        assert_eq!(
            font.get_font_styles(),
            FONT_STYLE_FORCE_BOLD | FONT_STYLE_ITALIC
        );
        let document = CpdfDocument::new();
        let stock = CfgasGeFont::load_stock_font(&document, b"Helvetica").unwrap();
        drop(document);
        assert!(stock.get_char_width(u32::from('A')).is_some());
    }
}
