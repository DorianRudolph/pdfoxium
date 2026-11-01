// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Retained fonts, descriptor state, fallback, and character positioning from
//! `core/fpdfapi/font/cpdf_font.{h,cpp}`.

use super::cpdf_fontencoding::CpdfFontEncoding;
use super::cpdf_tounicodemap::CpdfToUnicodeMap;
use crate::fpdfapi::page::cpdf_docpagedata::CachedDocumentFace;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_codepage::FxCodePage;
use crate::fxcrt::fx_coordinates::CfxPointF;
use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxge::cfx_face::{CfxFace, CfxFaceError};
use crate::fxge::cfx_font::CfxFont;
use crate::fxge::cfx_fontmapper::CfxFontMapper;
use crate::fxge::cfx_standardfont::get_standard_font_index;
use crate::fxge::fx_font::{
    normalize_font_metric, FONT_STYLE_ITALIC, FONT_STYLE_NON_SYMBOLIC, FONT_USE_EXTERN_ATTR,
};
use crate::fxge::text_char_pos::TextCharPos;
use std::cell::{OnceCell, Ref, RefCell};
use std::rc::Rc;

pub const INVALID_CHAR_CODE: u32 = u32::MAX;

/// Retained runtime font handle, replacing CPDF_Font's subclass pointer.
#[derive(Clone)]
pub enum CpdfFont {
    Simple(Rc<super::cpdf_facebasedsimplefont::CpdfLoadedSimpleFont>),
    Cid(Rc<super::cpdf_cidfont::CpdfLoadedCidFont>),
    Type3(Rc<super::cpdf_type3font::CpdfType3Font>),
}

impl CpdfFont {
    /// `CPDF_Font::GetStockFont()`: the document's page data owns its stock
    /// cache in place of the native global table keyed by document address.
    pub fn get_stock_font(document: &CpdfDocument, name: &[u8]) -> Option<Self> {
        crate::fxge::cfx_gemodule::with_ge_module(|module| {
            document
                .page_data()
                .get_stock_font(name, &mut module.font_mgr().builtin_mapper_mut())
                .map(Self::Simple)
        })
    }

    /// Retain the actual font dictionary; never reconstruct it from metrics.
    pub fn font_dict(&self) -> Option<ObjectRef> {
        match self {
            Self::Simple(font) => font.font_dict(),
            Self::Cid(font) => Some(Rc::clone(font.font_dict())),
            Self::Type3(font) => font.font_dict(),
        }
    }

    pub fn is_type1(&self) -> bool {
        matches!(self, Self::Simple(font) if font.is_type1())
    }

    pub fn is_cid_font(&self) -> bool {
        matches!(self, Self::Cid(_))
    }

    /// Type 1 and TrueType share `CPDF_SimpleFont::GetEncoding()`. The mapped
    /// borrow retains its real mutable state rather than copying its table.
    pub fn encoding(&self) -> Option<Ref<'_, CpdfFontEncoding>> {
        match self {
            Self::Simple(font) => Some(Ref::map(font.base(), |base| base.simple.encoding())),
            _ => None,
        }
    }

    pub fn char_code_from_unicode(&self, unicode: u32) -> u32 {
        match self {
            Self::Simple(font) => font.char_code_from_unicode(unicode),
            Self::Cid(font) => font
                .metrics
                .borrow()
                .char_code_from_unicode(unicode, font.to_unicode.as_ref()),
            Self::Type3(font) => font.char_code_from_unicode(unicode),
        }
    }

    pub fn append_char(&self, bytes: &mut Vec<u8>, charcode: u32) {
        match self {
            Self::Cid(font) => font.metrics.borrow().append_char(bytes, charcode),
            _ => CpdfFontBase::append_char(bytes, charcode),
        }
    }

    pub fn type_ascent(&self) -> i32 {
        match self {
            Self::Simple(font) => font.font_base.ascent,
            Self::Cid(font) => font.font_base.ascent,
            Self::Type3(font) => font.font_base.borrow().ascent,
        }
    }

    pub fn type_descent(&self) -> i32 {
        match self {
            Self::Simple(font) => font.font_base.descent,
            Self::Cid(font) => font.font_base.descent,
            Self::Type3(font) => font.font_base.borrow().descent,
        }
    }

    pub fn base_font_name(&self) -> &[u8] {
        match self {
            Self::Simple(font) => font.base_font_name(),
            Self::Cid(font) => &font.base_font_name,
            Self::Type3(font) => &font.base_font_name,
        }
    }

    pub fn set_resource_name(&self, name: &[u8]) {
        match self {
            Self::Simple(font) => *font.font_base.resource_name.borrow_mut() = name.to_vec(),
            Self::Cid(font) => *font.font_base.resource_name.borrow_mut() = name.to_vec(),
            Self::Type3(font) => {
                *font.font_base.borrow().resource_name.borrow_mut() = name.to_vec()
            }
        }
    }
    pub fn resource_name(&self) -> Vec<u8> {
        match self {
            Self::Simple(font) => font.font_base.resource_name.borrow().clone(),
            Self::Cid(font) => font.font_base.resource_name.borrow().clone(),
            Self::Type3(font) => font.font_base.borrow().resource_name.borrow().clone(),
        }
    }
    pub fn count_char(&self, bytes: &[u8]) -> usize {
        match self {
            Self::Cid(font) => font.metrics.borrow().count_char(bytes),
            _ => bytes.len(),
        }
    }
    pub fn get_next_char(&self, bytes: &[u8], offset: &mut usize) -> u32 {
        match self {
            Self::Cid(font) => font.metrics.borrow().get_next_char(bytes, offset),
            _ => CpdfFontBase::get_next_char(bytes, offset),
        }
    }
    pub fn is_vertical(&self) -> bool {
        match self {
            Self::Cid(font) => font.metrics.borrow().is_vertical(),
            Self::Simple(font) => font.font().is_vertical(),
            Self::Type3(_) => false,
        }
    }

    pub fn get_char_width(&self, code: u32) -> i32 {
        match self {
            Self::Simple(font) => font.get_char_width(code),
            Self::Cid(font) => font.metrics.borrow().get_char_width(code),
            Self::Type3(font) => font.get_char_width(code),
        }
    }

    pub fn get_char_bbox(&self, code: u32) -> FxRect {
        match self {
            Self::Simple(font) => font.get_char_bbox(code),
            Self::Cid(font) => font.metrics.borrow_mut().get_char_bbox(
                code,
                font.font().face(),
                font.to_unicode.as_ref(),
            ),
            Self::Type3(font) => font.get_char_bbox(code),
        }
    }

    pub fn font(&self) -> Option<&CfxFont<'static>> {
        match self {
            Self::Simple(font) => Some(font.font()),
            Self::Cid(font) => Some(font.font()),
            Self::Type3(_) => None,
        }
    }

    pub fn font_flags(&self) -> u32 {
        match self {
            Self::Type3(font) => font.font_base.borrow().flags,
            _ => self.descriptor().map_or(0, |base| base.flags),
        }
    }

    pub fn font_weight(&self) -> Option<i32> {
        match self {
            Self::Type3(font) => font.font_base.borrow().get_font_weight(),
            _ => self.descriptor()?.get_font_weight(),
        }
    }

    pub fn italic_angle(&self) -> i32 {
        match self {
            Self::Type3(font) => font.font_base.borrow().italic_angle,
            _ => self.descriptor().map_or(0, |base| base.italic_angle),
        }
    }

    fn descriptor(&self) -> Option<&CpdfFontBase> {
        match self {
            Self::Simple(font) => Some(&font.font_base),
            Self::Cid(font) => Some(&font.font_base),
            Self::Type3(_) => None,
        }
    }

    fn fallback_cell(&self) -> Option<&OnceCell<CfxFont<'static>>> {
        match self {
            Self::Simple(font) => Some(&font.fallback_font),
            Self::Cid(font) => Some(&font.fallback_font),
            Self::Type3(_) => None,
        }
    }

    pub fn font_at(&self, position: i32) -> Option<&CfxFont<'static>> {
        if position == -1 {
            return self.font();
        }
        if position != 0 {
            return None;
        }
        self.fallback_cell()?.get()
    }

    pub fn unicode_from_char_code(&self, code: u32) -> Vec<u32> {
        match self {
            Self::Simple(font) => font.unicode_from_char_code(code),
            Self::Cid(font) => font
                .metrics
                .borrow()
                .unicode_from_char_code(code, font.to_unicode.as_ref()),
            Self::Type3(font) => font.unicode_from_char_code(code),
        }
    }

    /// `GlyphFromCharCode(charcode, nullptr)`, without rendering fallback.
    /// Font subsetting needs the original embedded face's glyph identities.
    pub fn glyph_from_char_code(&self, code: u32) -> i32 {
        match self {
            Self::Simple(font) => font.base().glyph_from_char_code(code),
            Self::Cid(font) => {
                font.metrics
                    .borrow_mut()
                    .glyph_from_char_code(code, font.font().face(), font.to_unicode.as_ref())
                    .0
            }
            // Type 3 inherits CPDF_SimpleFont's -1 default; its glyphs are forms.
            Self::Type3(_) => -1,
        }
    }

    pub fn is_true_type(&self) -> bool {
        matches!(self, Self::Simple(font) if font.is_true_type())
    }

    pub fn is_embedded(&self) -> bool {
        match self {
            Self::Simple(font) => font.is_embedded(),
            Self::Cid(font) => font.is_embedded(),
            Self::Type3(_) => true,
        }
    }

    fn has_to_unicode(&self) -> bool {
        match self {
            Self::Simple(font) => font.has_to_unicode(),
            Self::Cid(font) => font.to_unicode.is_some(),
            Self::Type3(_) => false,
        }
    }

    fn should_apply_glyph_spacing_heuristic(
        &self,
        current_font: &CfxFont<'_>,
        vertical: bool,
    ) -> bool {
        let (base_name, has_widths) = match self {
            Self::Simple(font) => (font.base_font_name(), font.base().has_font_widths()),
            Self::Cid(font) => (font.base_font_name.as_slice(), true),
            Self::Type3(_) => return false,
        };
        if vertical || self.is_embedded() || !has_widths {
            return false;
        }
        let mut base_name = base_name.to_vec();
        crate::fxcrt::fx_system::strlwr(&mut base_name);
        if get_standard_font_index(&base_name).is_some() {
            return false;
        }
        let Some(subst) = current_font.subst_font() else {
            return false;
        };
        !subst.is_builtin_generic_font() && !subst.is_actual_font_loaded(&base_name)
    }

    fn get_or_load_fallback(&self, mapper: &mut CfxFontMapper) -> Option<&CfxFont<'static>> {
        let descriptor = self.descriptor()?;
        Some(self.fallback_cell()?.get_or_init(|| {
            let mut font = CfxFont::default();
            font.load_subst_face(
                mapper,
                b"Arial",
                self.is_true_type(),
                descriptor.flags,
                descriptor.stem_v.checked_mul(5).unwrap_or(400),
                descriptor.italic_angle,
                FxCodePage::DefAnsi,
                self.is_vertical(),
            );
            font
        }))
    }

    /// Selected AGG portion of `GetCharPosList()`. Type 3 text uses its form renderer.
    pub fn get_char_pos_list(
        &self,
        codes: &[u32],
        positions: &[f32],
        font_size: f32,
        mapper: &mut CfxFontMapper,
    ) -> Vec<TextCharPos> {
        let Some(primary) = self.font() else {
            return Vec::new();
        };
        let cid_font = match self {
            Self::Cid(font) => Some(font),
            _ => None,
        };
        let vertical = self.is_vertical();
        let has_to_unicode = self.has_to_unicode();
        let mut results = Vec::with_capacity(codes.len());
        for (&code, &position) in codes.iter().zip(positions) {
            if code == INVALID_CHAR_CODE {
                continue;
            }
            let unicode = self
                .unicode_from_char_code(code)
                .first()
                .copied()
                .unwrap_or(code);
            let (glyph, vertical_glyph) = match self {
                Self::Simple(font) => (font.base().glyph_from_char_code(code), false),
                Self::Cid(font) => font.metrics.borrow_mut().glyph_from_char_code(
                    code,
                    primary.face(),
                    font.to_unicode.as_ref(),
                ),
                Self::Type3(_) => unreachable!("Type 3 text uses forms"),
            };
            let mut char_pos = TextCharPos {
                unicode,
                glyph_index: glyph as u32,
                is_cid_font: cid_font.is_some(),
                origin: CfxPointF::new(position, 0.0),
                ..TextCharPos::default()
            };
            let current_font = if CpdfFontBase::should_use_font(
                glyph as u32,
                self.is_embedded(),
                self.is_true_type(),
                has_to_unicode,
            ) {
                char_pos.fallback_font_position = -1;
                primary
            } else {
                let Some(fallback) = self.get_or_load_fallback(mapper) else {
                    continue;
                };
                char_pos.fallback_font_position = 0;
                let mapped = self
                    .unicode_from_char_code(code)
                    .first()
                    .copied()
                    .unwrap_or(code);
                let glyph = fallback.get_char_index(mapped);
                char_pos.glyph_index = if glyph == 0 { u32::MAX } else { glyph };
                fallback
            };
            if !self.is_embedded() && cid_font.is_none() {
                char_pos.font_char_width = self.get_char_width(code);
            }
            let mut scaling_factor = 1.0;
            if self.should_apply_glyph_spacing_heuristic(current_font, vertical) {
                let pdf_width = self.get_char_width(code);
                let font_width = current_font.get_glyph_width(char_pos.glyph_index, 0);
                if font_width != 0 && i64::from(pdf_width) > i64::from(font_width) + 1 {
                    char_pos.origin.x +=
                        (i64::from(pdf_width) - i64::from(font_width)) as f32 * font_size / 2000.0;
                } else if pdf_width != 0 && font_width != 0 && pdf_width < font_width {
                    scaling_factor = pdf_width as f32 / font_width as f32;
                    char_pos.adjust_matrix = [scaling_factor, 0.0, 0.0, 1.0];
                    char_pos.glyph_adjust = true;
                }
            }
            if let Some(font) = cid_font {
                let metrics = font.metrics.borrow();
                let cid = metrics.cid_from_char_code(code);
                if vertical {
                    char_pos.origin = CfxPointF::new(0.0, char_pos.origin.x);
                    let origin = metrics.get_vert_origin(cid);
                    char_pos.origin.x -= font_size * f32::from(origin.x) / 1000.0;
                    char_pos.origin.y -= font_size * f32::from(origin.y) / 1000.0;
                }
                if let Some(transform) = metrics.get_cid_transform(cid).filter(|_| !vertical_glyph)
                {
                    let convert = super::cpdf_cidfont::CpdfCidFontMetrics::cid_transform_to_float;
                    char_pos.adjust_matrix = [
                        convert(transform.a) * scaling_factor,
                        convert(transform.b) * scaling_factor,
                        convert(transform.c),
                        convert(transform.d),
                    ];
                    char_pos.origin.x += convert(transform.e) * font_size;
                    char_pos.origin.y += convert(transform.f) * font_size;
                    char_pos.glyph_adjust = true;
                }
            }
            results.push(char_pos);
        }
        results
    }
}

pub(super) fn load_to_unicode_map(dict: &CpdfDictionary) -> Option<CpdfToUnicodeMap> {
    let object = dict.get_stream_for(b"ToUnicode")?;
    let borrowed = object.borrow();
    Some(CpdfToUnicodeMap::from_stream(
        borrowed.as_any().downcast_ref::<CpdfStream>()?,
    ))
}

#[derive(Default)]
pub struct CpdfFontBase {
    resource_name: RefCell<Vec<u8>>,
    pub flags: u32,
    pub stem_v: i32,
    pub font_weight: Option<i32>,
    pub ascent: i32,
    pub descent: i32,
    pub italic_angle: i32,
    pub font_bbox: FxRect,
}

impl CpdfFontBase {
    /// CID branches of `CPDF_Font::Create()`, including legacy Chinese
    /// TrueType dictionaries without an embedded FontFile2 entry.
    pub fn uses_cid_font(dict: &CpdfDictionary) -> bool {
        let subtype = dict.get_byte_string_for(b"Subtype", b"");
        if crate::fxcrt::bytestring::eq_cstr(&subtype, b"Type0") {
            return true;
        }
        if !crate::fxcrt::bytestring::eq_cstr(&subtype, b"TrueType") {
            return false;
        }
        let name = dict.get_byte_string_for(b"BaseFont", b"");
        let chinese = [
            b"\xcb\xce\xcc\xe5",
            b"\xbf\xac\xcc\xe5",
            b"\xba\xda\xcc\xe5",
            b"\xb7\xc2\xcb\xce",
            b"\xd0\xc2\xcb\xce",
        ];
        if !chinese.iter().any(|prefix| name.starts_with(*prefix)) {
            return false;
        }
        !dict.get_dict_for(b"FontDescriptor").is_some_and(|object| {
            let borrowed = object.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .is_some_and(|dict| dict.key_exist(b"FontFile2"))
        })
    }

    /// Equivalent to the embedded face portion of `LoadFontDescriptor()`.
    /// The returned face retains the document's decoded font data cache entry.
    pub fn load_embedded_face(
        document: &CpdfDocument,
        font_stream: Option<&ObjectRef>,
    ) -> Result<Option<Rc<CachedDocumentFace>>, CfxFaceError> {
        let Some(stream) = font_stream else {
            return Ok(None);
        };
        document.page_data_ref().get_font_file_face(stream)
    }

    /// Reads descriptor attributes and returns the first embedded font stream.
    /// The document cache owns decoding and face creation after this step.
    pub fn load_font_descriptor(&mut self, descriptor: &CpdfDictionary) -> Option<ObjectRef> {
        self.flags = descriptor.get_integer_for(b"Flags", FONT_STYLE_NON_SYMBOLIC as i32) as u32;
        let has_italic_angle = descriptor.key_exist(b"ItalicAngle");
        let italic_angle = descriptor.get_integer_for(b"ItalicAngle", 0);
        self.italic_angle = italic_angle;
        if italic_angle != 0 {
            self.flags |= FONT_STYLE_ITALIC;
        }
        let has_stem_v = descriptor.key_exist(b"StemV");
        if has_stem_v {
            self.stem_v = descriptor.get_integer_for(b"StemV", 0);
        }
        let mut has_valid_weight = false;
        if descriptor.key_exist(b"FontWeight") {
            let weight = descriptor.get_integer_for(b"FontWeight", 0);
            has_valid_weight = weight > 0;
            if has_valid_weight {
                self.font_weight = Some(weight);
            }
        }
        let has_ascent = descriptor.key_exist(b"Ascent");
        if has_ascent {
            self.ascent = descriptor.get_integer_for(b"Ascent", 0);
        }
        let has_descent = descriptor.key_exist(b"Descent");
        if has_descent {
            self.descent = descriptor.get_integer_for(b"Descent", 0);
        }
        if has_italic_angle
            && has_ascent
            && descriptor.key_exist(b"CapHeight")
            && has_descent
            && (has_stem_v || has_valid_weight)
        {
            self.flags |= FONT_USE_EXTERN_ATTR;
        }
        if self.descent > 10 {
            self.descent = -self.descent;
        }
        if let Some(bbox) = descriptor.get_array_for(b"FontBBox") {
            let borrowed = bbox.borrow();
            if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                self.font_bbox = FxRect::new(
                    array.get_integer_at(0),
                    array.get_integer_at(3),
                    array.get_integer_at(2),
                    array.get_integer_at(1),
                );
            }
        }
        descriptor
            .get_stream_for(b"FontFile")
            .or_else(|| descriptor.get_stream_for(b"FontFile2"))
            .or_else(|| descriptor.get_stream_for(b"FontFile3"))
    }

    pub fn check_font_metrics(
        &mut self,
        face: Option<&CfxFace<'_>>,
        mut char_bbox: impl FnMut(u32) -> FxRect,
    ) {
        if self.font_bbox == FxRect::default() {
            if let Some(face) = face {
                let raw = face.bbox();
                let upem = face.units_per_em();
                // PDFium deliberately flips the face bounding box vertically.
                self.font_bbox = FxRect::new(
                    normalize_font_metric(i64::from(raw.left), upem),
                    normalize_font_metric(i64::from(raw.bottom), upem),
                    normalize_font_metric(i64::from(raw.right), upem),
                    normalize_font_metric(i64::from(raw.top), upem),
                );
                self.ascent = normalize_font_metric(i64::from(face.ascender()), upem);
                self.descent = normalize_font_metric(i64::from(face.descender()), upem);
            } else {
                let mut first = true;
                for code in 0..256 {
                    let rect = char_bbox(code);
                    if rect.left == rect.right {
                        continue;
                    }
                    if first {
                        self.font_bbox = rect;
                        first = false;
                    } else {
                        self.font_bbox.left = self.font_bbox.left.min(rect.left);
                        self.font_bbox.top = self.font_bbox.top.max(rect.top);
                        self.font_bbox.right = self.font_bbox.right.max(rect.right);
                        self.font_bbox.bottom = self.font_bbox.bottom.min(rect.bottom);
                    }
                }
            }
        }
        if self.ascent == 0 && self.descent == 0 {
            let upper = char_bbox(u32::from(b'A'));
            self.ascent = if upper.bottom == upper.top {
                self.font_bbox.top
            } else {
                upper.top
            };
            let lower = char_bbox(u32::from(b'g'));
            self.descent = if lower.bottom == lower.top {
                self.font_bbox.bottom
            } else {
                lower.bottom
            };
        }
    }

    pub fn get_font_weight(&self) -> Option<i32> {
        if let Some(weight) = self.font_weight {
            return Some(weight);
        }
        if self.stem_v < 140 {
            self.stem_v.checked_mul(5)
        } else {
            self.stem_v.checked_mul(4)?.checked_add(140)
        }
    }

    pub fn should_use_font(
        glyph_id: u32,
        embedded: bool,
        true_type: bool,
        has_to_unicode: bool,
    ) -> bool {
        glyph_id != u32::MAX && (embedded || !true_type || glyph_id != 0 || has_to_unicode)
    }

    pub fn get_next_char(bytes: &[u8], offset: &mut usize) -> u32 {
        if bytes.is_empty() {
            return 0;
        }
        if let Some(&value) = bytes.get(*offset) {
            *offset += 1;
            u32::from(value)
        } else {
            u32::from(*bytes.last().unwrap_or(&0))
        }
    }

    pub fn append_char(bytes: &mut Vec<u8>, charcode: u32) {
        bytes.push(charcode as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    #[test]
    fn japanese_cid_positions_apply_vertical_origin_and_transform() {
        let mut descendant = CpdfDictionary::new();
        descendant.set_new_name_for(b"Subtype", b"CIDFontType0");
        descendant.set_new_name_for(b"BaseFont", b"MissingFont");
        let mut info = CpdfDictionary::new();
        info.set_new_name_for(b"Ordering", b"Japan1");
        descendant.set_new_for(b"CIDSystemInfo", info);
        let mut descendants = CpdfArray::new();
        descendants.append_new(descendant);
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"Type0");
        dict.set_new_name_for(b"Encoding", b"Identity-V");
        dict.set_new_for(b"DescendantFonts", descendants);
        let page_data = Rc::new(crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData::new());
        let mut mapper = CfxFontMapper::new();
        let font = page_data
            .get_font(&retain_object(dict), &mut mapper)
            .unwrap();
        let chars = font.get_char_pos_list(&[97], &[12.0], 10.0, &mut mapper);
        assert_eq!(chars.len(), 1);
        let ch = &chars[0];
        assert!(ch.is_cid_font && ch.glyph_adjust);
        assert_eq!(ch.font_char_width, 0);
        assert_eq!(ch.adjust_matrix, [-126.0 / 127.0, 0.0, 0.0, 1.0]);
        assert!((ch.origin.x - (-5.0 + 550.0 / 127.0)).abs() < 0.00001);
        assert!((ch.origin.y - 3.2).abs() < 0.00001);
    }

    #[test]
    fn chinese_truetype_factory_dispatch_checks_fontfile2_key() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"TrueType");
        dict.set_new_name_for(b"BaseFont", b"\xcb\xce\xcc\xe5,Bold");
        assert!(CpdfFontBase::uses_cid_font(&dict));
        let descriptor = dict.set_new_dictionary_for(b"FontDescriptor");
        descriptor
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"FontFile2", CpdfScalarObject::null());
        assert!(!CpdfFontBase::uses_cid_font(&dict));
        dict.set_new_name_for(b"Subtype", b"Type0");
        assert!(CpdfFontBase::uses_cid_font(&dict));
    }

    #[test]
    fn embedded_face_load_keeps_cached_sfnt_and_raw_cff_bytes() {
        let document = CpdfDocument::new();
        let bytes =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let stream = retain_object(CpdfStream::from_bytes(bytes));
        let face = CpdfFontBase::load_embedded_face(&document, Some(&stream))
            .unwrap()
            .unwrap();
        assert!(face.face().get_char_index(b'A' as u32) > 0);
        let cff =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/minion.cff");
        let stream = retain_object(CpdfStream::from_bytes(cff));
        let face = CpdfFontBase::load_embedded_face(&document, Some(&stream))
            .unwrap()
            .unwrap();
        assert_eq!(face.face().font_format(), "CFF");
        assert!(!face.face().is_tt_ot());
        assert_eq!(face.face().data(), cff);
    }

    #[test]
    fn descriptor_fields_and_weight_follow_pdfium_rules() {
        let mut descriptor = CpdfDictionary::new();
        for (key, value) in [
            (b"Flags".as_slice(), 0),
            (b"ItalicAngle".as_slice(), -12),
            (b"StemV".as_slice(), 180),
            (b"Ascent".as_slice(), 700),
            (b"Descent".as_slice(), 200),
            (b"CapHeight".as_slice(), 680),
        ] {
            descriptor.set_new_for(key, CpdfScalarObject::number_from_signed(value));
        }
        let bbox = descriptor.set_new_array_for(b"FontBBox");
        let mut borrowed = bbox.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        for value in [-100, -200, 900, 800] {
            array.append_new(CpdfScalarObject::number_from_signed(value));
        }
        drop(borrowed);
        let mut font = CpdfFontBase::default();
        assert!(font.load_font_descriptor(&descriptor).is_none());
        assert_eq!(font.flags, FONT_STYLE_ITALIC | FONT_USE_EXTERN_ATTR);
        assert_eq!(font.italic_angle, -12);
        assert_eq!(font.descent, -200);
        assert_eq!(font.font_bbox, FxRect::new(-100, 800, 900, -200));
        assert_eq!(font.get_font_weight(), Some(860));
        descriptor.set_new_for(b"FontWeight", CpdfScalarObject::number_from_signed(500));
        font.load_font_descriptor(&descriptor);
        assert_eq!(font.get_font_weight(), Some(500));
        descriptor.set_new_for(b"ItalicAngle", CpdfScalarObject::number_from_signed(15));
        font.load_font_descriptor(&descriptor);
        assert_eq!(font.italic_angle, 15);
        assert_ne!(font.flags & FONT_STYLE_ITALIC, 0);
    }

    #[test]
    fn metrics_and_glyph_selection_include_fallback_rules() {
        let data =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let face = CfxFace::new(data, 0).unwrap();
        let mut font = CpdfFontBase::default();
        font.check_font_metrics(Some(&face), |_| FxRect::default());
        assert_eq!(
            font.ascent,
            normalize_font_metric(i64::from(face.ascender()), face.units_per_em())
        );
        assert_ne!(font.font_bbox, FxRect::default());
        assert!(!CpdfFontBase::should_use_font(u32::MAX, true, false, false));
        assert!(!CpdfFontBase::should_use_font(0, false, true, false));
        assert!(CpdfFontBase::should_use_font(0, false, true, true));
        assert!(CpdfFontBase::should_use_font(0, false, false, false));

        let mut offset = 2;
        assert_eq!(
            CpdfFontBase::get_next_char(b"abc", &mut offset),
            b'c' as u32
        );
        assert_eq!(
            CpdfFontBase::get_next_char(b"abc", &mut offset),
            b'c' as u32
        );
    }
}
