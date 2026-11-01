// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Metrics and glyph storage from `core/fpdfapi/font/cpdf_facebasedsimplefont.{h,cpp}`.
//! Shared loading, substitution, metrics, and glyph storage.

use super::cpdf_font::CpdfFontBase;
use super::cpdf_fontencoding::FontEncoding;
use super::cpdf_simplefont::CpdfSimpleFont;
use super::cpdf_tounicodemap::CpdfToUnicodeMap;
use super::cpdf_truetypefont::CpdfTrueTypeFont;
use super::cpdf_type1font::CpdfType1Font;
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{key_for_cache, ObjectRef};
use crate::fxcrt::fx_codepage::FxCodePage;
use crate::fxcrt::fx_coordinates::FxRect;
use crate::fxge::cfx_face::CfxFace;
use crate::fxge::cfx_font::CfxFont;
use crate::fxge::cfx_fontmapper::CfxFontMapper;
use crate::fxge::fx_font::{
    font_style_is_all_caps, font_style_is_symbolic, maybe_remove_subsetted_font_prefix,
    FONT_STYLE_FIXED_PITCH,
};
use std::cell::{OnceCell, Ref, RefCell};

const INVALID_GLYPH: u16 = 0xffff;
const UNSET_BBOX: FxRect = FxRect::new(-1, -1, -1, -1);

enum SimpleFontKind {
    TrueType(CpdfTrueTypeFont),
    Type1(CpdfType1Font),
}

/// The common face-based PDF font object. Page data retains it by dictionary
/// identity, just as `CPDF_DocPageData::GetFont()` retains the C++ subclasses.
pub struct CpdfLoadedSimpleFont {
    font_dict: RefCell<Option<ObjectRef>>,
    base_font_name: Vec<u8>,
    pub font_base: CpdfFontBase,
    font: CfxFont<'static>,
    kind: RefCell<SimpleFontKind>,
    embedded: bool,
    pub(crate) fallback_font: OnceCell<CfxFont<'static>>,
    to_unicode: OnceCell<Option<CpdfToUnicodeMap>>,
}

impl CpdfLoadedSimpleFont {
    pub fn load(
        font_dict: ObjectRef,
        page_data: &CpdfDocPageData,
        mapper: &mut CfxFontMapper,
    ) -> Option<Self> {
        let borrowed = font_dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        let true_type = crate::fxcrt::bytestring::eq_cstr(
            &dict.get_byte_string_for(b"Subtype", b""),
            b"TrueType",
        );
        let mut base_font_name = dict.get_byte_string_for(b"BaseFont", b"");
        let mut font_base = CpdfFontBase::default();
        let mut kind = if true_type {
            SimpleFontKind::TrueType(CpdfTrueTypeFont::default())
        } else {
            let mut type1 = CpdfType1Font::default();
            (base_font_name, font_base.flags) =
                type1.prepare_base14(dict, &base_font_name, font_base.flags);
            SimpleFontKind::Type1(type1)
        };
        let base = match &mut kind {
            SimpleFontKind::TrueType(font) => &mut font.base,
            SimpleFontKind::Type1(font) => &mut font.base,
        };
        let descriptor = dict.get_dict_for(b"FontDescriptor");
        let descriptor_borrow = descriptor.as_ref().map(|object| object.borrow());
        let descriptor = descriptor_borrow
            .as_ref()
            .and_then(|object| object.as_any().downcast_ref::<CpdfDictionary>());
        let stream = descriptor.and_then(|dict| font_base.load_font_descriptor(dict));
        base.load_char_widths(dict, descriptor);
        let object_tag = stream.as_ref().map_or(0, key_for_cache);
        let retained_face =
            stream.and_then(|stream| page_data.get_font_file_face(&stream).ok().flatten());
        let embedded = retained_face.is_some();
        let font = if let Some(face) = retained_face {
            maybe_remove_subsetted_font_prefix(&mut base_font_name);
            CfxFont::from_retained_face(face, false, object_tag)
        } else {
            if base.widths_imply_fixed_pitch() {
                font_base.flags |= FONT_STYLE_FIXED_PITCH;
            }
            let weight = font_base
                .get_font_weight()
                .filter(|weight| (100..=900).contains(weight))
                .unwrap_or(400);
            let mut font = CfxFont::default();
            font.load_subst_face(
                mapper,
                &base_font_name,
                true_type,
                font_base.flags,
                weight,
                font_base.italic_angle,
                FxCodePage::DefAnsi,
                false,
            );
            font
        };
        base.load_pdf_encoding(
            dict,
            &base_font_name,
            font_base.flags,
            embedded,
            font.is_tt_font(),
        );
        let to_unicode = OnceCell::new();
        if let Some(face) = font.face() {
            match &mut kind {
                SimpleFontKind::TrueType(pdf_font) => {
                    let to_unicode =
                        to_unicode.get_or_init(|| super::cpdf_font::load_to_unicode_map(dict));
                    pdf_font.load_glyph_map(
                        face,
                        dict,
                        font_base.flags,
                        embedded,
                        to_unicode.as_ref(),
                    );
                }
                SimpleFontKind::Type1(pdf_font) => {
                    pdf_font.load_glyph_map(face, font_base.flags, embedded)
                }
            }
        }
        let base = match &mut kind {
            SimpleFontKind::TrueType(font) => &mut font.base,
            SimpleFontKind::Type1(font) => &mut font.base,
        };
        base.simple.take_char_names();
        if let Some(face) = font.face() {
            if font_style_is_all_caps(font_base.flags) {
                base.apply_all_caps(embedded);
            }
            font_base.check_font_metrics(Some(face), |code| {
                base.get_char_bbox(code, Some(face), embedded)
            });
        }
        drop(descriptor_borrow);
        drop(borrowed);
        Some(Self {
            font_dict: RefCell::new(Some(font_dict)),
            base_font_name,
            font_base,
            font,
            kind: RefCell::new(kind),
            embedded,
            fallback_font: OnceCell::new(),
            to_unicode,
        })
    }

    pub fn font(&self) -> &CfxFont<'static> {
        &self.font
    }
    pub fn font_dict(&self) -> Option<ObjectRef> {
        self.font_dict.borrow().clone()
    }
    pub fn base_font_name(&self) -> &[u8] {
        &self.base_font_name
    }
    pub fn is_embedded(&self) -> bool {
        self.embedded
    }
    pub fn is_type1(&self) -> bool {
        matches!(&*self.kind.borrow(), SimpleFontKind::Type1(_))
    }
    pub fn is_true_type(&self) -> bool {
        matches!(*self.kind.borrow(), SimpleFontKind::TrueType(_))
    }
    pub fn has_to_unicode(&self) -> bool {
        self.font_dict().is_some_and(|object| {
            let borrowed = object.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .is_some_and(|dict| dict.get_stream_for(b"ToUnicode").is_some())
        })
    }
    pub fn unicode_from_char_code(&self, code: u32) -> Vec<u32> {
        let map = self.to_unicode.get_or_init(|| {
            let object = self.font_dict()?;
            let borrowed = object.borrow();
            super::cpdf_font::load_to_unicode_map(
                borrowed.as_any().downcast_ref::<CpdfDictionary>()?,
            )
        });
        self.base()
            .simple
            .unicode_from_char_code(code, map.as_ref())
    }
    pub fn char_code_from_unicode(&self, unicode: u32) -> u32 {
        let map = self.to_unicode.get_or_init(|| {
            let object = self.font_dict()?;
            let borrowed = object.borrow();
            super::cpdf_font::load_to_unicode_map(
                borrowed.as_any().downcast_ref::<CpdfDictionary>()?,
            )
        });
        self.base()
            .simple
            .char_code_from_unicode(unicode, map.as_ref())
    }
    pub fn base(&self) -> Ref<'_, CpdfFaceBasedSimpleFont> {
        Ref::map(self.kind.borrow(), |kind| match kind {
            SimpleFontKind::TrueType(font) => &font.base,
            SimpleFontKind::Type1(font) => &font.base,
        })
    }

    pub fn get_char_width(&self, code: u32) -> i32 {
        let mut kind = self.kind.borrow_mut();
        let base = match &mut *kind {
            SimpleFontKind::TrueType(font) => &mut font.base,
            SimpleFontKind::Type1(font) => &mut font.base,
        };
        base.get_char_width(code, self.font.face(), self.embedded)
    }

    pub fn get_char_bbox(&self, code: u32) -> FxRect {
        let mut kind = self.kind.borrow_mut();
        let base = match &mut *kind {
            SimpleFontKind::TrueType(font) => &mut font.base,
            SimpleFontKind::Type1(font) => &mut font.base,
        };
        base.get_char_bbox(code, self.font.face(), self.embedded)
    }
}

impl super::cpdf_stockfontarray::StockFont for CpdfLoadedSimpleFont {
    fn clear_font_dict(&self) {
        self.font_dict.borrow_mut().take();
    }
}

pub struct CpdfFaceBasedSimpleFont {
    pub simple: CpdfSimpleFont,
    use_font_width: bool,
    glyph_index: [u16; 256],
    char_width: [u16; 256],
    char_bbox: [FxRect; 256],
}

impl Default for CpdfFaceBasedSimpleFont {
    fn default() -> Self {
        Self {
            simple: CpdfSimpleFont::default(),
            use_font_width: false,
            glyph_index: [INVALID_GLYPH; 256],
            char_width: [INVALID_GLYPH; 256],
            char_bbox: [UNSET_BBOX; 256],
        }
    }
}

impl CpdfFaceBasedSimpleFont {
    /// Encoding portion of `LoadCommon()`, before the derived glyph map.
    pub fn load_pdf_encoding(
        &mut self,
        font_dict: &CpdfDictionary,
        base_font_name: &[u8],
        flags: u32,
        embedded: bool,
        true_type: bool,
    ) {
        if !font_style_is_symbolic(flags) {
            self.simple.set_base_encoding(FontEncoding::Standard);
        }
        self.simple
            .load_pdf_encoding(font_dict, base_font_name, flags, embedded, true_type);
    }

    pub fn fill_char_widths(&mut self, width: u16) {
        self.char_width.fill(width);
    }

    pub fn set_glyph_index(&mut self, charcode: u8, glyph_index: u16) {
        self.glyph_index[usize::from(charcode)] = glyph_index;
    }

    pub fn glyph_from_char_code(&self, charcode: u32) -> i32 {
        let Some(&glyph) = self.glyph_index.get(charcode as usize) else {
            return -1;
        };
        if glyph == INVALID_GLYPH {
            -1
        } else {
            i32::from(glyph)
        }
    }

    pub fn has_any_glyph_index(&self) -> bool {
        self.glyph_index.iter().any(|&index| index != 0)
    }

    pub fn set_glyph_indices_from_first_char(&mut self, start_char: i32) {
        let Ok(start) = usize::try_from(start_char) else {
            return;
        };
        if start > 255 {
            return;
        }
        self.glyph_index[..start].fill(0);
        for (offset, slot) in self.glyph_index[start..].iter_mut().enumerate() {
            *slot = (offset + 3) as u16;
        }
    }

    pub fn has_font_widths(&self) -> bool {
        !self.use_font_width
    }

    pub fn load_char_widths(
        &mut self,
        font_dict: &CpdfDictionary,
        descriptor: Option<&CpdfDictionary>,
    ) {
        let Some(widths) = font_dict.get_array_for(b"Widths") else {
            self.use_font_width = true;
            return;
        };
        self.use_font_width = false;
        if let Some(missing) = descriptor.filter(|desc| desc.key_exist(b"MissingWidth")) {
            self.char_width
                .fill(missing.get_integer_for(b"MissingWidth", 0) as u16);
        }
        let borrowed = widths.borrow();
        let Some(width_array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
            return;
        };
        let start = font_dict.get_integer_for(b"FirstChar", 0) as usize;
        let mut end = font_dict.get_integer_for(b"LastChar", 0) as usize;
        if start > 255 {
            return;
        }
        if end == 0 || end >= start + width_array.size() {
            end = (start + width_array.size()).wrapping_sub(1);
        }
        end = end.min(255);
        for code in start..=end {
            self.char_width[code] = width_array.get_integer_at(code - start) as u16;
        }
    }

    pub fn get_char_width(
        &mut self,
        charcode: u32,
        face: Option<&CfxFace<'_>>,
        embedded: bool,
    ) -> i32 {
        let index = if charcode > 255 { 0 } else { charcode as usize };
        if self.char_width[index] == INVALID_GLYPH {
            self.load_char_metrics(index, face, embedded);
            if self.char_width[index] == INVALID_GLYPH {
                self.char_width[index] = 0;
            }
        }
        i32::from(self.char_width[index])
    }

    pub fn get_char_bbox(
        &mut self,
        charcode: u32,
        face: Option<&CfxFace<'_>>,
        embedded: bool,
    ) -> FxRect {
        let index = if charcode > 255 { 0 } else { charcode as usize };
        if self.char_bbox[index].left == -1 {
            self.load_char_metrics(index, face, embedded);
        }
        self.char_bbox[index]
    }

    fn load_char_metrics(&mut self, code: usize, face: Option<&CfxFace<'_>>, embedded: bool) {
        let Some(face) = face else {
            return;
        };
        let glyph = self.glyph_index[code];
        if glyph == INVALID_GLYPH {
            if !embedded && code != 32 {
                self.load_char_metrics(32, Some(face), embedded);
                self.char_bbox[code] = self.char_bbox[32];
                if self.use_font_width {
                    self.char_width[code] = self.char_width[32];
                }
            }
            return;
        }
        if face.font_glyph_bbox(u32::from(glyph)).is_none() {
            return;
        }
        self.char_bbox[code] = face.glyph_bbox(u32::from(glyph));
        if self.use_font_width {
            let tt_width = face.glyph_tt_width(u32::from(glyph));
            if self.char_width[code] == INVALID_GLYPH {
                self.char_width[code] = tt_width as u16;
            } else if tt_width != 0 && !embedded {
                let pdf_width = i32::from(self.char_width[code]);
                self.char_bbox[code].right =
                    scale_bbox(self.char_bbox[code].right, pdf_width, tt_width);
                self.char_bbox[code].left =
                    scale_bbox(self.char_bbox[code].left, pdf_width, tt_width);
            }
        }
    }

    pub fn apply_all_caps(&mut self, embedded: bool) {
        for (first, last) in [(b'a', b'z'), (0xe0, 0xf6), (0xf8, 0xfd)] {
            for code in first..=last {
                let index = usize::from(code);
                if self.glyph_index[index] != INVALID_GLYPH && embedded {
                    continue;
                }
                let upper = index - 32;
                self.glyph_index[index] = self.glyph_index[upper];
                if self.char_width[upper] != 0 {
                    self.char_width[index] = self.char_width[upper];
                    self.char_bbox[index] = self.char_bbox[upper];
                }
            }
        }
    }

    pub fn widths_imply_fixed_pitch(&self) -> bool {
        if self.use_font_width {
            return false;
        }
        let mut width = 0;
        for &current in &self.char_width {
            if current == 0 || current == INVALID_GLYPH {
                continue;
            }
            if width == 0 {
                width = current;
            } else if width != current {
                return false;
            }
        }
        width != 0
    }
}

fn scale_bbox(value: i32, width: i32, tt_width: i32) -> i32 {
    (i64::from(value) * i64::from(width) / i64::from(tt_width))
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn substitute_spacing_uses_pdf_width_for_compression_and_centering() {
        use crate::fpdfapi::font::cpdf_font::CpdfFont;
        use crate::fpdfapi::parser::cpdf_object::retain_object;
        use crate::fxge::cfx_substfont::CfxSubstFont;
        use std::rc::Rc;
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"TrueType");
        dict.set_new_name_for(b"BaseFont", b"RequestedFont");
        dict.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        let mut widths = CpdfArray::new();
        for value in [500, 1400] {
            widths.append_new(CpdfScalarObject::number_from_signed(value));
        }
        dict.set_new_for(b"Widths", widths);
        let mut mapper = CfxFontMapper::new();
        let mut font =
            CpdfLoadedSimpleFont::load(retain_object(dict), &CpdfDocPageData::new(), &mut mapper)
                .unwrap();
        let bytes =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let face = CfxFace::new(bytes, 0).unwrap();
        let mut subst = CfxSubstFont::new();
        subst.configure_external_subst(b"Ahem", 0, 400, false, 0, false, false);
        match &mut *font.kind.borrow_mut() {
            SimpleFontKind::TrueType(font) => {
                for code in [65, 66] {
                    font.base
                        .set_glyph_index(code, face.get_char_index(u32::from(code)) as u16);
                }
            }
            _ => unreachable!(),
        }
        font.font = CfxFont::from_substitute(face, subst, false);
        let font = CpdfFont::Simple(Rc::new(font));
        let chars = font.get_char_pos_list(&[65, 66], &[0.0, 10.0], 10.0, &mut mapper);
        assert!(chars[0].glyph_adjust);
        assert_eq!(chars[0].adjust_matrix, [0.5, 0.0, 0.0, 1.0]);
        assert_eq!(chars[0].origin.x, 0.0);
        assert!(!chars[1].glyph_adjust);
        assert_eq!(chars[1].origin.x, 12.0);
    }

    #[test]
    fn missing_truetype_glyph_uses_fallback_and_mixed_font_clip_runs() {
        use crate::fpdfapi::font::cpdf_font::CpdfFont;
        use crate::fpdfapi::parser::cpdf_object::retain_object;
        use crate::fpdfapi::render::cpdf_textrenderer::draw_pdf_text_path_agg;
        use crate::fxcrt::fx_coordinates::CfxMatrix;
        use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
        use crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions;
        use crate::fxge::cfx_path::CfxPath;
        use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
        use crate::fxge::dib::fx_dib::FxdibFormat;
        use std::rc::Rc;
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"TrueType");
        dict.set_new_name_for(b"BaseFont", b"Helvetica");
        let mut mapper = CfxFontMapper::new();
        let font =
            CpdfLoadedSimpleFont::load(retain_object(dict), &CpdfDocPageData::new(), &mut mapper)
                .unwrap();
        // A missing glyph in a non-embedded TrueType map needs font fallback.
        match &mut *font.kind.borrow_mut() {
            SimpleFontKind::TrueType(font) => font.base.set_glyph_index(65, 0),
            _ => unreachable!(),
        }
        let font = CpdfFont::Simple(Rc::new(font));
        let codes = [65, 66, u32::MAX, 65];
        let positions = [0.0, 10.0, 100.0, 20.0];
        let chars = font.get_char_pos_list(&codes, &positions, 10.0, &mut mapper);
        assert_eq!(
            chars
                .iter()
                .map(|ch| ch.fallback_font_position)
                .collect::<Vec<_>>(),
            [0, -1, 0]
        );
        assert_eq!(chars[0].unicode, 65);
        assert_ne!(chars[0].glyph_index, u32::MAX);
        assert!(font.font_at(0).is_some());
        assert!(font.font_at(1).is_none());
        let mut bitmap = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
        let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut path = CfxPath::new();
        assert!(draw_pdf_text_path_agg(
            &mut driver,
            &codes,
            &positions,
            &font,
            10.0,
            CfxMatrix::default(),
            None,
            None,
            0,
            0,
            Some(&mut path),
            CfxFillRenderOptions::default()
        ));
        let bbox = path.bounding_box();
        assert!(bbox.right > 20.0 && bbox.right < 100.0);
    }

    #[test]
    fn pdf_width_array_and_all_caps_follow_source_order() {
        let mut font = CpdfFaceBasedSimpleFont::default();
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        dict.set_new_for(b"LastChar", CpdfScalarObject::number_from_signed(66));
        let widths = dict.set_new_array_for(b"Widths");
        let mut borrowed = widths.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        array.append_new(CpdfScalarObject::number_from_signed(600));
        array.append_new(CpdfScalarObject::number_from_signed(600));
        drop(borrowed);
        font.load_char_widths(&dict, None);
        assert!(font.has_font_widths());
        assert_eq!(font.get_char_width(65, None, false), 600);
        assert!(font.widths_imply_fixed_pitch());
        font.set_glyph_index(b'A', 12);
        font.apply_all_caps(false);
        assert_eq!(font.glyph_from_char_code(b'a' as u32), 12);
        assert_eq!(font.get_char_width(b'a' as u32, None, false), 600);
        assert_eq!(font.glyph_from_char_code(256), -1);
    }

    #[test]
    fn missing_nonembedded_glyph_reuses_space_metrics() {
        let data =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let face = CfxFace::new(data, 0).unwrap();
        let mut font = CpdfFaceBasedSimpleFont::default();
        font.load_char_widths(&CpdfDictionary::new(), None);
        let space = face.get_char_index(32) as u16;
        font.set_glyph_index(32, space);
        let width = font.get_char_width(32, Some(&face), false);
        assert!(width > 0);
        assert_eq!(font.get_char_width(65, Some(&face), false), width);
        assert_eq!(
            font.get_char_bbox(65, Some(&face), false),
            font.get_char_bbox(32, Some(&face), false)
        );
    }
}
