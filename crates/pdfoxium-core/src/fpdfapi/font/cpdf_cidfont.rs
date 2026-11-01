// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! CID width and vertical metrics from `core/fpdfapi/font/cpdf_cidfont.{h,cpp}`.
//! CID loading, glyph selection, substitution, and metrics.

use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

use super::cpdf_cid2unicodemap::CpdfCid2UnicodeMap;
use super::cpdf_cidfont_japan1::JAPAN1_VERTICAL_CIDS;
use super::cpdf_cmap::CidCoding;
use super::cpdf_cmap::CpdfCmap;
use super::cpdf_cmapparser::charset_from_ordering;
use super::cpdf_cmapparser::CidSet;
use super::cpdf_font::CpdfFontBase;
use super::cpdf_fontencoding::{char_code_from_unicode_for_encoding, FontEncoding};
use super::cpdf_fontglobals::CpdfFontGlobals;
use super::cpdf_tounicodemap::CpdfToUnicodeMap;
use super::cpdf_truetypefont::{
    adobe_char_name, use_tt_charmap, use_tt_charmap_unicode, MAC_ROMAN,
};
use crate::fpdfapi::cmaps::unicode_map_for_charset;
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{key_for_cache, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
use crate::fxcrt::fx_codepage::FxCodePage;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPoint16, FxRect};
use crate::fxge::cfx_cttgsubtable::CfxCttgsubTable;
use crate::fxge::cfx_face::{charmap_id_pair_to_font_encoding, CfxFace};
use crate::fxge::cfx_font::{CfxFont, FontType};
use crate::fxge::cfx_fontmapper::CfxFontMapper;
use crate::fxge::fx_font::unicode_from_adobe_name;
use crate::fxge::fx_fontencoding::FontEncoding as FxFontEncoding;

#[derive(Clone, Copy)]
struct Metric {
    low: i32,
    high: i32,
    value: i32,
}

#[derive(Clone, Copy)]
struct VerticalMetric {
    range: Metric,
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CidFontType {
    Type1,
    TrueType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CidTransform {
    pub cid: u16,
    pub a: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub f: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CidFontLoadError {
    InvalidDescendantFonts,
    MissingEncoding,
    InvalidEncoding,
}

pub struct CpdfCidFontLoad {
    pub base_font_name: Vec<u8>,
    pub descriptor: CpdfFontBase,
    pub font_stream: Option<ObjectRef>,
}

/// The retained face and mutable glyph/metric state of a loaded CID font.
pub struct CpdfLoadedCidFont {
    font_dict: ObjectRef,
    pub base_font_name: Vec<u8>,
    pub font_base: CpdfFontBase,
    font: CfxFont<'static>,
    pub metrics: RefCell<CpdfCidFontMetrics>,
    pub to_unicode: Option<CpdfToUnicodeMap>,
    embedded_charmap: Option<usize>,
    embedded: bool,
    pub(crate) fallback_font: OnceCell<CfxFont<'static>>,
}

impl CpdfLoadedCidFont {
    pub fn load(
        font_dict: ObjectRef,
        page_data: &CpdfDocPageData,
        mapper: &mut CfxFontMapper,
        globals: &mut CpdfFontGlobals,
    ) -> Option<Self> {
        let borrowed = font_dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        let mut metrics = CpdfCidFontMetrics::default();
        let load = metrics.load_from_pdf(dict, globals).ok()?;
        let tag = load.font_stream.as_ref().map_or(0, key_for_cache);
        let retained_face = load
            .font_stream
            .and_then(|stream| page_data.get_font_file_face(&stream).ok().flatten());
        let embedded = retained_face.is_some();
        metrics.set_embedded(embedded);
        // Identity is only accepted after successfully loading an embedded face.
        if !embedded {
            metrics.cid_is_gid = false;
        }
        let mut font = if let Some(face) = retained_face {
            let font = CfxFont::from_retained_face(face, metrics.is_vertical(), tag);
            if !crate::fxcrt::bytestring::eq_cstr(
                &dict.get_byte_string_for(b"Subtype", b""),
                b"TrueType",
            ) {
                let face = font.face().expect("retained face");
                if metrics.font_type == CidFontType::Type1 {
                    face.select_charmap(FxFontEncoding::Unicode);
                } else {
                    use_cid_charmap(
                        face,
                        metrics
                            .cmap
                            .as_ref()
                            .map_or(CidCoding::Unknown, |cmap| cmap.coding()),
                    );
                }
            }
            font
        } else {
            let codepage = match metrics.charset() {
                CidSet::GB1 => FxCodePage::ChineseSimplified,
                CidSet::CNS1 => FxCodePage::ChineseTraditional,
                CidSet::Japan1 => FxCodePage::ShiftJis,
                CidSet::Korea1 => FxCodePage::Hangul,
                CidSet::Unicode => FxCodePage::Utf16Le,
                CidSet::Unknown => FxCodePage::DefAnsi,
            };
            let mut font = CfxFont::default();
            font.load_subst_face(
                mapper,
                &load.base_font_name,
                metrics.font_type == CidFontType::TrueType,
                load.descriptor.flags,
                load.descriptor.stem_v.checked_mul(5).unwrap_or(400),
                load.descriptor.italic_angle,
                codepage,
                metrics.is_vertical(),
            );
            font
        };
        let to_unicode = dict.get_stream_for(b"ToUnicode").and_then(|object| {
            let borrowed = object.borrow();
            Some(CpdfToUnicodeMap::from_stream(
                borrowed.as_any().downcast_ref::<CpdfStream>()?,
            ))
        });
        let mut font_base = load.descriptor;
        font_base.check_font_metrics(font.face(), |code| {
            metrics.get_char_bbox(code, font.face(), to_unicode.as_ref())
        });
        if metrics.font_type == CidFontType::TrueType
            && embedded
            && font
                .face()
                .is_some_and(|face| face.font_format() == "TrueType")
        {
            font.set_font_type(FontType::CidTrueType);
        }
        let embedded_charmap = font.face().and_then(CfxFace::current_charmap);
        drop(borrowed);
        Some(Self {
            font_dict,
            base_font_name: load.base_font_name,
            font_base,
            font,
            metrics: RefCell::new(metrics),
            to_unicode,
            embedded_charmap,
            embedded,
            fallback_font: OnceCell::new(),
        })
    }

    pub fn font(&self) -> &CfxFont<'static> {
        // Several PDF dictionaries can share a decoded face. Restore this
        // CID font's selected charmap before its dynamic glyph mapping.
        if self.embedded {
            if let Some(face) = self.font.face() {
                face.set_charmap(self.embedded_charmap);
            }
        }
        &self.font
    }
    pub fn font_dict(&self) -> &ObjectRef {
        &self.font_dict
    }
    pub fn is_embedded(&self) -> bool {
        self.embedded
    }
}

fn use_cid_charmap(face: &CfxFace<'_>, coding: CidCoding) {
    let encoding = match coding {
        CidCoding::GB => FxFontEncoding::GB2312,
        CidCoding::Big5 => FxFontEncoding::Big5,
        CidCoding::Jis => FxFontEncoding::Sjis,
        CidCoding::Korea => FxFontEncoding::Johab,
        _ => FxFontEncoding::Unicode,
    };
    if !face.select_charmap(encoding)
        && !face.select_charmap(FxFontEncoding::Unicode)
        && face.charmap_count() != 0
    {
        face.set_charmap(Some(0));
    }
}

pub struct CpdfCidFontMetrics {
    cmap: Option<Rc<CpdfCmap>>,
    charset: CidSet,
    cid2unicode_map: Option<CpdfCid2UnicodeMap>,
    default_width: i16,
    default_vy: i16,
    default_w1: i16,
    ansi_widths_fixed: bool,
    widths: Vec<Metric>,
    vertical_metrics: Vec<VerticalMetric>,
    embedded: bool,
    font_type: CidFontType,
    cid_is_gid: bool,
    adobe_courier_std: bool,
    cid_to_gid_map: Option<Vec<u8>>,
    gsub: Option<CfxCttgsubTable>,
    char_bbox: [FxRect; 256],
}

impl Default for CpdfCidFontMetrics {
    fn default() -> Self {
        Self {
            cmap: None,
            charset: CidSet::Unknown,
            cid2unicode_map: None,
            default_width: 1000,
            default_vy: 880,
            default_w1: -1000,
            ansi_widths_fixed: false,
            widths: Vec::new(),
            vertical_metrics: Vec::new(),
            embedded: false,
            font_type: CidFontType::TrueType,
            cid_is_gid: false,
            adobe_courier_std: false,
            cid_to_gid_map: None,
            gsub: None,
            char_bbox: [FxRect::new(-1, -1, -1, -1); 256],
        }
    }
}

impl CpdfCidFontMetrics {
    /// Parses the Type0/Chinese TrueType dictionary. The document font cache
    /// loads and retains `font_stream`, then passes the face to glyph methods.
    pub fn load_from_pdf(
        &mut self,
        font_dict: &CpdfDictionary,
        globals: &mut CpdfFontGlobals,
    ) -> Result<CpdfCidFontLoad, CidFontLoadError> {
        if crate::fxcrt::bytestring::eq_cstr(
            &font_dict.get_byte_string_for(b"Subtype", b""),
            b"TrueType",
        ) {
            self.set_charset(CidSet::GB1);
            self.set_cmap(globals.get_predefined_cmap(b"GBK-EUC-H"));
            let mut descriptor = CpdfFontBase::default();
            let font_stream = font_dict
                .get_dict_for(b"FontDescriptor")
                .and_then(|object| {
                    let borrowed = object.borrow();
                    let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
                    descriptor.load_font_descriptor(dict)
                });
            self.ansi_widths_fixed = true;
            return Ok(CpdfCidFontLoad {
                base_font_name: font_dict.get_byte_string_for(b"BaseFont", b""),
                descriptor,
                font_stream,
            });
        }

        let descendant = font_dict
            .get_array_for(b"DescendantFonts")
            .ok_or(CidFontLoadError::InvalidDescendantFonts)?;
        let descendant = {
            let borrowed = descendant.borrow();
            let array = borrowed
                .as_any()
                .downcast_ref::<CpdfArray>()
                .ok_or(CidFontLoadError::InvalidDescendantFonts)?;
            if array.size() != 1 {
                return Err(CidFontLoadError::InvalidDescendantFonts);
            }
            array
                .get_dict_at(0)
                .ok_or(CidFontLoadError::InvalidDescendantFonts)?
        };
        let encoding = font_dict
            .get_direct_object_for(b"Encoding")
            .ok_or(CidFontLoadError::MissingEncoding)?;
        let cmap = {
            let borrowed = encoding.borrow();
            match borrowed.object_type() {
                ObjectType::Stream => {
                    let stream = borrowed
                        .as_any()
                        .downcast_ref::<CpdfStream>()
                        .ok_or(CidFontLoadError::InvalidEncoding)?;
                    let mut acc = CpdfStreamAcc::new(stream);
                    acc.load_all_data_filtered();
                    Rc::new(CpdfCmap::from_embedded(acc.get_span()))
                }
                ObjectType::Name => globals.get_predefined_cmap(&borrowed.get_string()),
                _ => return Err(CidFontLoadError::InvalidEncoding),
            }
        };
        self.set_cmap(cmap);

        let borrowed = descendant.borrow();
        let dict = borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .ok_or(CidFontLoadError::InvalidDescendantFonts)?;
        let base_font_name = dict.get_byte_string_for(b"BaseFont", b"");
        self.adobe_courier_std = matches!(
            crate::fxcrt::bytestring::c_str(&base_font_name),
            b"CourierStd" | b"CourierStd-Bold" | b"CourierStd-BoldOblique" | b"CourierStd-Oblique"
        );
        self.font_type = if crate::fxcrt::bytestring::eq_cstr(
            &dict.get_byte_string_for(b"Subtype", b""),
            b"CIDFontType0",
        ) {
            CidFontType::Type1
        } else {
            CidFontType::TrueType
        };
        let mut descriptor = CpdfFontBase::default();
        let font_stream = dict.get_dict_for(b"FontDescriptor").and_then(|object| {
            let borrowed = object.borrow();
            let descriptor_dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            descriptor.load_font_descriptor(descriptor_dict)
        });
        if self.charset == CidSet::Unknown {
            if let Some(info) = dict.get_dict_for(b"CIDSystemInfo") {
                let borrowed = info.borrow();
                if let Some(info) = borrowed.as_any().downcast_ref::<CpdfDictionary>() {
                    self.set_charset(charset_from_ordering(
                        &info.get_byte_string_for(b"Ordering", b""),
                    ));
                }
            }
        }
        self.load_widths(dict);
        if let Some(mapping) = dict.get_direct_object_for(b"CIDToGIDMap") {
            let borrowed = mapping.borrow();
            if let Some(stream) = borrowed.as_any().downcast_ref::<CpdfStream>() {
                let mut acc = CpdfStreamAcc::new(stream);
                acc.load_all_data_filtered();
                self.set_cid_to_gid_map(acc.detach_data());
            } else if font_stream.is_some()
                && borrowed.object_type() == ObjectType::Name
                && crate::fxcrt::bytestring::eq_cstr(&borrowed.get_string(), b"Identity")
            {
                self.cid_is_gid = true;
            }
        }
        Ok(CpdfCidFontLoad {
            base_font_name,
            descriptor,
            font_stream,
        })
    }

    pub fn set_cmap(&mut self, cmap: Rc<CpdfCmap>) {
        self.set_charset(cmap.charset());
        self.cmap = Some(cmap);
    }

    pub fn set_charset(&mut self, charset: CidSet) {
        self.charset = charset;
        self.cid2unicode_map =
            (charset != CidSet::Unknown).then(|| CpdfCid2UnicodeMap::new(charset));
    }

    pub fn charset(&self) -> CidSet {
        self.charset
    }

    pub fn is_unicode_compatible(&self) -> bool {
        let Some(cmap) = &self.cmap else { return false };
        if self
            .cid2unicode_map
            .as_ref()
            .is_some_and(CpdfCid2UnicodeMap::is_loaded)
            && cmap.is_loaded()
        {
            return true;
        }
        cmap.coding() != CidCoding::Unknown
    }

    pub fn unicode_from_char_code(
        &self,
        charcode: u32,
        to_unicode: Option<&CpdfToUnicodeMap>,
    ) -> Vec<u32> {
        if let Some(map) = to_unicode {
            let mapped = map.lookup(charcode);
            if !mapped.is_empty() {
                return mapped;
            }
        }
        let unicode = self.get_unicode_from_char_code(charcode);
        if unicode == 0 {
            Vec::new()
        } else {
            vec![unicode]
        }
    }

    pub fn get_unicode_from_char_code(&self, charcode: u32) -> u32 {
        let Some(cmap) = &self.cmap else { return 0 };
        match cmap.coding() {
            CidCoding::Ucs2 | CidCoding::Utf16 => return charcode,
            CidCoding::Cid => {
                return self
                    .cid2unicode_map
                    .as_ref()
                    .filter(|map| map.is_loaded())
                    .map_or(0, |map| map.unicode_from_cid(charcode as u16));
            }
            _ => {}
        }
        if let Some(map) = self
            .cid2unicode_map
            .as_ref()
            .filter(|map| map.is_loaded() && cmap.is_loaded())
        {
            return map.unicode_from_cid(self.cid_from_char_code(charcode));
        }
        if cmap.embed_map().is_none()
            || !matches!(
                self.charset,
                CidSet::GB1 | CidSet::CNS1 | CidSet::Japan1 | CidSet::Korea1
            )
        {
            return 0;
        }
        let cid = self.cid_from_char_code(charcode);
        if cid == 0 {
            return 0;
        }
        unicode_map_for_charset(self.charset)
            .get(usize::from(cid))
            .copied()
            .map(u32::from)
            .unwrap_or(0)
    }

    pub fn char_code_from_unicode(
        &self,
        unicode: u32,
        to_unicode: Option<&CpdfToUnicodeMap>,
    ) -> u32 {
        if let Some(map) = to_unicode {
            let charcode = map.reverse_lookup(unicode);
            if charcode != 0 {
                return charcode;
            }
        }
        let Some(cmap) = &self.cmap else { return 0 };
        match cmap.coding() {
            CidCoding::Unknown => return 0,
            CidCoding::Ucs2 | CidCoding::Utf16 => return unicode,
            CidCoding::Cid => {
                let Some(map) = self.cid2unicode_map.as_ref().filter(|map| map.is_loaded()) else {
                    return 0;
                };
                for cid in 0u32..65536 {
                    if map.unicode_from_cid(cid as u16) == unicode {
                        return cid;
                    }
                }
            }
            _ => {}
        }
        if unicode < 0x80 {
            return unicode;
        }
        if cmap.coding() == CidCoding::Cid {
            return 0;
        }
        if cmap.embed_map().is_none()
            || !matches!(
                self.charset,
                CidSet::GB1 | CidSet::CNS1 | CidSet::Japan1 | CidSet::Korea1
            )
        {
            return 0;
        }
        for (cid, &value) in unicode_map_for_charset(self.charset).iter().enumerate() {
            if u32::from(value) == unicode {
                let code = cmap.embedded_char_code_from_cid(cid as u16);
                if code != 0 {
                    return code;
                }
            }
        }
        0
    }

    pub fn set_ansi_widths_fixed(&mut self, value: bool) {
        self.ansi_widths_fixed = value;
    }

    pub fn set_embedded(&mut self, embedded: bool) {
        self.embedded = embedded;
    }

    pub fn set_font_type(&mut self, font_type: CidFontType) {
        self.font_type = font_type;
    }

    pub fn set_cid_is_gid(&mut self, cid_is_gid: bool) {
        self.cid_is_gid = cid_is_gid;
    }

    pub fn set_adobe_courier_std(&mut self, adobe_courier_std: bool) {
        self.adobe_courier_std = adobe_courier_std;
    }

    pub fn set_cid_to_gid_map(&mut self, map: Vec<u8>) {
        self.cid_to_gid_map = Some(map);
    }

    pub fn cid_transform_to_float(value: u8) -> f32 {
        (if value < 128 {
            i32::from(value)
        } else {
            i32::from(value) - 255
        }) as f32
            * (1.0 / 127.0)
    }

    pub fn get_cid_transform(&self, cid: u16) -> Option<&'static CidTransform> {
        if self.charset != CidSet::Japan1 || self.embedded {
            return None;
        }
        let index = JAPAN1_VERTICAL_CIDS.partition_point(|entry| entry.cid < cid);
        JAPAN1_VERTICAL_CIDS
            .get(index)
            .filter(|entry| entry.cid == cid)
    }

    pub fn get_char_bbox(
        &mut self,
        charcode: u32,
        face: Option<&CfxFace<'_>>,
        to_unicode: Option<&CpdfToUnicodeMap>,
    ) -> FxRect {
        let cache_index = usize::try_from(charcode).ok().filter(|&code| code < 256);
        if let Some(index) = cache_index {
            if self.char_bbox[index].right != -1 {
                return self.char_bbox[index];
            }
        }
        let (glyph, vertical) = self.glyph_from_char_code(charcode, face, to_unicode);
        let rect = face
            .as_ref()
            .map_or_else(FxRect::default, |face| face.char_bbox(glyph));
        let rect =
            if let Some(transform) = self.get_cid_transform(self.cid_from_char_code(charcode)) {
                if vertical {
                    rect
                } else {
                    let convert = Self::cid_transform_to_float;
                    let matrix = CfxMatrix::new(
                        convert(transform.a),
                        convert(transform.b),
                        convert(transform.c),
                        convert(transform.d),
                        convert(transform.e) * 1000.0,
                        convert(transform.f) * 1000.0,
                    );
                    matrix
                        .transform_float_rect(CfxFloatRect::from_fx_rect(rect))
                        .get_outer_rect()
                }
            } else {
                rect
            };
        if let Some(index) = cache_index {
            self.char_bbox[index] = rect;
        }
        rect
    }

    pub fn glyph_from_char_code(
        &mut self,
        charcode: u32,
        face: Option<&CfxFace<'_>>,
        to_unicode: Option<&CpdfToUnicodeMap>,
    ) -> (i32, bool) {
        let cid = self.cid_from_char_code(charcode);
        if !self.embedded && (self.cid_to_gid_map.is_none() || self.cid2unicode_map.is_some()) {
            if self.cid_is_gid {
                return (i32::from(cid), false);
            }
            let mut unicode = 0;
            if cid != 0 {
                if let Some(map) = self.cid2unicode_map.as_ref().filter(|map| map.is_loaded()) {
                    unicode = map.unicode_from_cid(cid);
                }
            }
            if unicode == 0 {
                unicode = self.get_unicode_from_char_code(charcode);
            }
            if unicode == 0 {
                unicode = self
                    .unicode_from_char_code(charcode, to_unicode)
                    .first()
                    .copied()
                    .unwrap_or(0);
            }
            if unicode == 0 {
                if !self.adobe_courier_std {
                    return (if charcode == 0 { -1 } else { charcode as i32 }, false);
                }
                return (self.courier_std_glyph(charcode, face), false);
            }
            if self.charset == CidSet::Japan1 {
                if unicode == b'\\' as u32 {
                    unicode = b'/' as u32;
                } else if unicode == 0xa5 {
                    unicode = b'\\' as u32;
                }
            }
            let Some(face) = face else {
                return (unicode as i32, false);
            };
            let count = face.charmap_count();
            if !face.select_charmap(FxFontEncoding::Unicode) {
                let mut found = false;
                for index in 0..count {
                    let encoding =
                        charmap_id_pair_to_font_encoding(face.charmap_id_pair_by_index(index));
                    let mapped = char_code_from_unicode_for_encoding(encoding, charcode);
                    if mapped != 0 {
                        face.set_charmap(Some(index));
                        unicode = mapped;
                        found = true;
                        break;
                    }
                }
                if !found && count != 0 {
                    face.set_charmap(Some(0));
                    unicode = charcode;
                }
            }
            if count != 0 {
                let (glyph, vertical) = self.glyph_index(face, unicode);
                return (if glyph == 0 { -1 } else { glyph }, vertical);
            }
            return (unicode as i32, false);
        }

        let Some(face) = face else {
            return (-1, false);
        };
        if let Some(map) = &self.cid_to_gid_map {
            let position = usize::from(cid) * 2;
            let Some(bytes) = map.get(position..position + 2) else {
                return (-1, false);
            };
            return (i32::from(bytes[0]) * 256 + i32::from(bytes[1]), false);
        }
        if self.font_type == CidFontType::Type1
            || (self.embedded
                && self
                    .cmap
                    .as_ref()
                    .is_some_and(|cmap| cmap.is_direct_charcode_to_cid_table_empty()))
            || self
                .cmap
                .as_ref()
                .is_some_and(|cmap| cmap.coding() == CidCoding::Unknown)
            || face.current_charmap().is_none()
        {
            return (i32::from(cid), false);
        }
        let mut lookup_code = charcode;
        if face.current_charmap_encoding() == FxFontEncoding::Unicode {
            let Some(&unicode) = self.unicode_from_char_code(charcode, to_unicode).first() else {
                return (-1, false);
            };
            lookup_code = unicode;
        }
        self.glyph_index(face, lookup_code)
    }

    fn glyph_index(&mut self, face: &CfxFace<'_>, unicode: u32) -> (i32, bool) {
        let glyph = face.get_char_index(unicode) as i32;
        if unicode == 0x2502 || glyph == 0 || !self.is_vertical() {
            return (glyph, false);
        }
        if self.gsub.is_none() {
            self.gsub = face.parse_gsub_table();
        }
        let vertical = self
            .gsub
            .as_ref()
            .map_or(0, |gsub| gsub.get_vertical_glyph(glyph as u32));
        if vertical == 0 {
            (glyph, false)
        } else {
            (vertical as i32, true)
        }
    }

    fn courier_std_glyph(&self, charcode: u32, face: Option<&CfxFace<'_>>) -> i32 {
        let code = charcode.wrapping_add(31);
        let Some(face) = face else {
            return if code == 0 { -1 } else { code as i32 };
        };
        let ms_unicode = use_tt_charmap_unicode(face);
        let mac_roman = !ms_unicode && use_tt_charmap(face, MAC_ROMAN);
        let encoding = if ms_unicode {
            FontEncoding::WinAnsi
        } else if mac_roman {
            FontEncoding::MacRoman
        } else {
            FontEncoding::Standard
        };
        let Some(name) = u8::try_from(code)
            .ok()
            .and_then(|code| adobe_char_name(encoding, &[], code))
        else {
            return if code == 0 { -1 } else { code as i32 };
        };
        let name = std::str::from_utf8(name).unwrap_or_default();
        let unicode = unicode_from_adobe_name(name);
        if unicode == 0 {
            return if code == 0 { -1 } else { code as i32 };
        }
        if encoding == FontEncoding::Standard {
            return face.get_char_index(unicode) as i32;
        }
        let glyph = if encoding == FontEncoding::WinAnsi {
            face.get_char_index(unicode)
        } else {
            let mac = char_code_from_unicode_for_encoding(FxFontEncoding::AppleRoman, unicode);
            if mac != 0 {
                face.get_char_index(mac)
            } else {
                face.name_index(name)
            }
        };
        if glyph == 0 || glyph == 0xffff {
            if code == 0 {
                -1
            } else {
                code as i32
            }
        } else {
            glyph as i32
        }
    }

    pub fn cid_from_char_code(&self, charcode: u32) -> u16 {
        self.cmap
            .as_ref()
            .map_or(charcode as u16, |cmap| cmap.cid_from_char_code(charcode))
    }

    pub fn is_vertical(&self) -> bool {
        self.cmap.as_ref().is_some_and(|cmap| cmap.is_vertical())
    }

    pub fn get_next_char(&self, bytes: &[u8], offset: &mut usize) -> u32 {
        match &self.cmap {
            Some(cmap) => cmap.get_next_char(bytes, offset),
            None => super::cpdf_font::CpdfFontBase::get_next_char(bytes, offset),
        }
    }

    pub fn count_char(&self, bytes: &[u8]) -> usize {
        self.cmap
            .as_ref()
            .map_or(bytes.len(), |cmap| cmap.count_char(bytes))
    }

    pub fn append_char(&self, bytes: &mut Vec<u8>, charcode: u32) {
        if let Some(cmap) = &self.cmap {
            cmap.append_char(bytes, charcode);
        } else {
            super::cpdf_font::CpdfFontBase::append_char(bytes, charcode);
        }
    }

    pub fn get_char_size(&self, charcode: u32) -> usize {
        self.cmap
            .as_ref()
            .map_or(1, |cmap| cmap.get_char_size(charcode))
    }

    pub fn load_widths(&mut self, descendant: &CpdfDictionary) {
        self.default_width = descendant.get_integer_for(b"DW", 1000) as i16;
        if let Some(widths) = descendant.get_array_for(b"W") {
            let borrowed = widths.borrow();
            if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                self.widths
                    .extend(
                        load_metrics_array(array, 1)
                            .chunks_exact(3)
                            .map(|values| Metric {
                                low: values[0],
                                high: values[1],
                                value: values[2],
                            }),
                    );
            }
        }
        if !self.is_vertical() {
            return;
        }
        if let Some(metrics) = descendant.get_array_for(b"W2") {
            let borrowed = metrics.borrow();
            if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                self.vertical_metrics
                    .extend(load_metrics_array(array, 3).chunks_exact(5).map(|values| {
                        VerticalMetric {
                            range: Metric {
                                low: values[0],
                                high: values[1],
                                value: values[2],
                            },
                            x: values[3],
                            y: values[4],
                        }
                    }));
            }
        }
        if let Some(defaults) = descendant.get_array_for(b"DW2") {
            let borrowed = defaults.borrow();
            if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                self.default_vy = array.get_integer_at(0) as i16;
                self.default_w1 = array.get_integer_at(1) as i16;
            }
        }
    }

    pub fn get_char_width(&self, charcode: u32) -> i32 {
        if charcode < 0x80 && self.ansi_widths_fixed {
            return if (32..127).contains(&charcode) {
                500
            } else {
                0
            };
        }
        self.width_for_cid(self.cid_from_char_code(charcode))
    }

    fn width_for_cid(&self, cid: u16) -> i32 {
        self.widths
            .iter()
            .find(|metric| metric.low <= i32::from(cid) && i32::from(cid) <= metric.high)
            .map_or(i32::from(self.default_width), |metric| metric.value)
    }

    pub fn get_vert_width(&self, cid: u16) -> i16 {
        self.vertical_metrics
            .iter()
            .find(|metric| {
                metric.range.low <= i32::from(cid) && i32::from(cid) <= metric.range.high
            })
            .map_or(self.default_w1, |metric| metric.range.value as i16)
    }

    pub fn get_vert_origin(&self, cid: u16) -> CfxPoint16 {
        if let Some(metric) = self.vertical_metrics.iter().find(|metric| {
            metric.range.low <= i32::from(cid) && i32::from(cid) <= metric.range.high
        }) {
            return CfxPoint16::new(metric.x as i16, metric.y as i16);
        }
        CfxPoint16::new((self.width_for_cid(cid) / 2) as i16, self.default_vy)
    }
}

fn load_metrics_array(array: &CpdfArray, elements: usize) -> Vec<i32> {
    let mut result = Vec::new();
    let mut status = 0;
    let mut first = 0;
    let mut last = 0;
    let mut current = 0;
    for index in 0..array.size() {
        let Some(object) = array.get_direct_object_at(index) else {
            continue;
        };
        let borrowed = object.borrow();
        if let Some(values) = borrowed.as_any().downcast_ref::<CpdfArray>() {
            if status != 1 {
                return result;
            }
            if first > i32::MAX - i32::try_from(values.size()).unwrap_or(i32::MAX) {
                status = 0;
                continue;
            }
            for item in (0..values.size()).step_by(elements) {
                result.extend([first, first]);
                for part in 0..elements {
                    result.push(values.get_integer_at(item + part));
                }
                first += 1;
            }
            status = 0;
        } else if status == 0 {
            first = borrowed.get_integer();
            status = 1;
        } else if status == 1 {
            last = borrowed.get_integer();
            status = 2;
            current = 0;
        } else {
            if current == 0 {
                result.extend([first, last]);
            }
            result.push(borrowed.get_integer());
            current += 1;
            if current == elements {
                status = 0;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn number(value: i32) -> CpdfScalarObject {
        CpdfScalarObject::number_from_signed(value)
    }

    #[test]
    fn loaded_embedded_cid_font_selects_charmap_and_retains_state() {
        use crate::fpdfapi::parser::cpdf_object::{make_reference, retain_object};
        let document = crate::fpdfapi::parser::cpdf_document::CpdfDocument::new();
        let holder = document.holder();
        let bytes =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let stream = holder.borrow_mut().new_indirect_stream_from_bytes(bytes);
        let mut descriptor = CpdfDictionary::new();
        descriptor.set_for(b"FontFile2", Some(make_reference(&stream, &holder)));
        let mut descendant = CpdfDictionary::new();
        descendant.set_new_name_for(b"Subtype", b"CIDFontType2");
        descendant.set_new_for(b"FontDescriptor", descriptor);
        let mut descendants = CpdfArray::new();
        descendants.append_new(descendant);
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"Type0");
        dict.set_new_name_for(b"Encoding", b"UniJIS-UCS2-H");
        dict.set_new_for(b"DescendantFonts", descendants);
        let dict = retain_object(dict);
        let page_data = CpdfDocPageData::new();
        let mut mapper = CfxFontMapper::new();
        let font = page_data.get_cid_font(&dict, &mut mapper).unwrap();
        assert!(Rc::ptr_eq(
            &font,
            &page_data.get_cid_font(&dict, &mut mapper).unwrap()
        ));
        assert_eq!(font.font().font_type(), FontType::CidTrueType);
        let face = font.font().face().unwrap();
        assert_eq!(face.current_charmap_encoding(), FxFontEncoding::Unicode);
        // Predefined CMaps have no direct table, so embedded fonts use the CID.
        let expected = i32::from(font.metrics.borrow().cid_from_char_code(65));
        assert!(expected > 0);
        assert_eq!(
            font.metrics
                .borrow_mut()
                .glyph_from_char_code(65, Some(face), None),
            (expected, false)
        );
        // A different PDF font sharing the stream can change the shared face.
        face.set_charmap(None);
        assert_eq!(
            font.font().face().unwrap().current_charmap_encoding(),
            FxFontEncoding::Unicode
        );
        drop(page_data);
        assert_eq!(font.font().font_span(), bytes);
    }

    #[test]
    fn invalid_embedded_cid_font_uses_stem_weight_and_rejects_identity_gid() {
        use crate::fpdfapi::parser::cpdf_object::{make_reference, retain_object};
        let document = crate::fpdfapi::parser::cpdf_document::CpdfDocument::new();
        let holder = document.holder();
        let stream = holder
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"invalid font");
        let mut descriptor = CpdfDictionary::new();
        descriptor.set_for(b"FontFile2", Some(make_reference(&stream, &holder)));
        for (key, value) in [
            (b"StemV".as_slice(), 100),
            (b"FontWeight".as_slice(), 800),
            (b"ItalicAngle".as_slice(), -12),
            (b"Ascent".as_slice(), 800),
            (b"Descent".as_slice(), -200),
            (b"CapHeight".as_slice(), 700),
        ] {
            descriptor.set_new_for(key, number(value));
        }
        let mut descendant = CpdfDictionary::new();
        descendant.set_new_name_for(b"Subtype", b"CIDFontType2");
        descendant.set_new_name_for(b"BaseFont", b"MissingFont");
        descendant.set_new_name_for(b"CIDToGIDMap", b"Identity");
        descendant.set_new_for(b"FontDescriptor", descriptor);
        let mut descendants = CpdfArray::new();
        descendants.append_new(descendant);
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"Type0");
        dict.set_new_name_for(b"Encoding", b"Identity-H");
        dict.set_new_for(b"DescendantFonts", descendants);
        let page_data = CpdfDocPageData::new();
        let font = page_data
            .get_cid_font(&retain_object(dict), &mut CfxFontMapper::new())
            .unwrap();
        let metrics = font.metrics.borrow();
        assert!(!metrics.embedded);
        assert!(!metrics.cid_is_gid);
        assert_eq!(font.font().subst_font().unwrap().weight(), 500);
        assert_eq!(font.font().subst_font().unwrap().italic_angle(), -12);
    }

    #[test]
    fn cid_horizontal_and_vertical_widths_follow_range_priority() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"DW", number(900));
        let widths = dict.set_new_array_for(b"W");
        let mut borrowed = widths.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        array.append_new(number(1));
        let listed = array.append_new(CpdfArray::new());
        {
            let mut inner_borrowed = listed.borrow_mut();
            let inner = inner_borrowed
                .as_any_mut()
                .downcast_mut::<CpdfArray>()
                .unwrap();
            inner.append_new(number(500));
            inner.append_new(number(600));
        }
        array.append_new(number(10));
        array.append_new(number(12));
        array.append_new(number(700));
        drop(borrowed);

        let vertical = dict.set_new_array_for(b"W2");
        let mut borrowed = vertical.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        array.append_new(number(10));
        array.append_new(number(11));
        for value in [-1200, 300, 880] {
            array.append_new(number(value));
        }
        drop(borrowed);
        let defaults = dict.set_new_array_for(b"DW2");
        let mut borrowed = defaults.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        array.append_new(number(850));
        array.append_new(number(-950));
        drop(borrowed);

        let mut font = CpdfCidFontMetrics::default();
        font.set_cmap(Rc::new(CpdfCmap::from_predefined(b"Identity-V")));
        font.load_widths(&dict);
        assert!(font.is_vertical());
        assert_eq!(font.get_char_width(1), 500);
        assert_eq!(font.get_char_width(2), 600);
        assert_eq!(font.get_char_width(11), 700);
        assert_eq!(font.get_char_width(20), 900);
        assert_eq!(font.get_vert_width(10), -1200);
        assert_eq!(font.get_vert_origin(11), CfxPoint16::new(300, 880));
        assert_eq!(font.get_vert_width(20), -950);
        assert_eq!(font.get_vert_origin(20), CfxPoint16::new(450, 850));
        let mut bytes = Vec::new();
        font.append_char(&mut bytes, 0x1234);
        assert_eq!(font.count_char(&bytes), 1);
        assert_eq!(font.get_next_char(&bytes, &mut 0), 0x1234);
    }

    #[test]
    fn cid_metric_parser_stops_on_bad_nested_array_position() {
        let mut array = CpdfArray::new();
        array.append_new(number(5));
        array.append_new(number(6));
        array.append_new(number(700));
        array.append_new(CpdfArray::new());
        assert_eq!(load_metrics_array(&array, 1), vec![5, 6, 700]);

        let mut font = CpdfCidFontMetrics::default();
        font.set_ansi_widths_fixed(true);
        assert_eq!(font.get_char_width(31), 0);
        assert_eq!(font.get_char_width(65), 500);
        assert_eq!(font.get_char_width(127), 0);
        assert_eq!(font.get_char_width(128), 1000);
    }

    #[test]
    fn cid_unicode_uses_tounicode_then_charset_and_embedded_map() {
        let mut font = CpdfCidFontMetrics::default();
        font.set_cmap(Rc::new(CpdfCmap::from_predefined(b"GB-EUC-H")));
        assert!(font.is_unicode_compatible());
        let cid = font.cid_from_char_code(0x20);
        assert_ne!(cid, 0);
        let expected = CpdfCid2UnicodeMap::new(CidSet::GB1).unicode_from_cid(cid);
        assert_eq!(font.get_unicode_from_char_code(0x20), expected);
        assert_eq!(font.unicode_from_char_code(0x20, None), vec![expected]);
        let reverse = font.char_code_from_unicode(expected, None);
        assert_ne!(reverse, 0);
        assert_eq!(font.get_unicode_from_char_code(reverse), expected);

        let override_map = CpdfToUnicodeMap::from_data(b"1 beginbfchar <20><0041> endbfchar");
        assert_eq!(
            font.unicode_from_char_code(0x20, Some(&override_map)),
            vec![65]
        );
        assert_eq!(font.char_code_from_unicode(65, Some(&override_map)), 0x20);

        let mut direct = CpdfCidFontMetrics::default();
        direct.set_cmap(Rc::new(CpdfCmap::from_predefined(b"Identity-H")));
        assert_eq!(direct.get_unicode_from_char_code(0x41), 0);
        assert!(direct.is_unicode_compatible());
    }

    #[test]
    fn cid_glyph_selection_handles_identity_and_cid_to_gid_streams() {
        let mut font = CpdfCidFontMetrics::default();
        font.set_cmap(Rc::new(CpdfCmap::from_predefined(b"Identity-H")));
        font.set_charset(CidSet::GB1);
        assert_eq!(font.glyph_from_char_code(1, None, None), (32, false));
        let data =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let mut face = CfxFace::new(data, 0).unwrap();
        assert_eq!(
            font.glyph_from_char_code(1, Some(&mut face), None),
            (face.get_char_index(32) as i32, false)
        );
        font.set_cid_is_gid(true);
        assert_eq!(font.glyph_from_char_code(1, None, None), (1, false));

        font.set_embedded(true);
        font.set_cid_is_gid(false);
        font.set_cid_to_gid_map(vec![0, 0, 0, 7]);
        assert_eq!(
            font.glyph_from_char_code(1, Some(&mut face), None),
            (7, false)
        );
        assert_eq!(
            font.glyph_from_char_code(2, Some(&mut face), None),
            (-1, false)
        );
    }

    #[test]
    fn japan1_vertical_transform_table_and_bbox_cache_match_source() {
        assert!(JAPAN1_VERTICAL_CIDS.len() > 100);
        assert_eq!(CpdfCidFontMetrics::cid_transform_to_float(127), 1.0);
        assert_eq!(CpdfCidFontMetrics::cid_transform_to_float(255), 0.0);
        assert_eq!(
            CpdfCidFontMetrics::cid_transform_to_float(129),
            -126.0 / 127.0
        );
        let mut font = CpdfCidFontMetrics::default();
        font.set_cmap(Rc::new(CpdfCmap::from_predefined(b"Identity-V")));
        font.set_charset(CidSet::Japan1);
        let transform = font.get_cid_transform(97).unwrap();
        assert_eq!(transform.a, 129);
        assert_eq!(transform.e, 55);
        assert!(font.get_cid_transform(96).is_none());

        let box_without_face = font.get_char_bbox(97, None, None);
        assert_eq!(font.get_char_bbox(97, None, None), box_without_face);
        assert_ne!(box_without_face, FxRect::default());
        font.set_embedded(true);
        assert!(font.get_cid_transform(97).is_none());
    }

    #[test]
    fn cid_dictionary_load_validates_descendant_and_cmap() {
        let mut globals = CpdfFontGlobals::default();
        let mut font = CpdfCidFontMetrics::default();
        assert!(matches!(
            font.load_from_pdf(&CpdfDictionary::new(), &mut globals),
            Err(CidFontLoadError::InvalidDescendantFonts)
        ));

        let mut root = CpdfDictionary::new();
        root.set_new_name_for(b"Subtype", b"Type0");
        root.set_new_name_for(b"Encoding", b"Identity-V");
        let descendants = root.set_new_array_for(b"DescendantFonts");
        let child = descendants
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append_new(CpdfDictionary::new());
        {
            let mut borrowed = child.borrow_mut();
            let descendant = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            descendant.set_new_name_for(b"Subtype", b"CIDFontType0");
            descendant.set_new_name_for(b"BaseFont", b"CourierStd");
            descendant.set_new_for(b"DW", number(725));
        }
        let loaded = font.load_from_pdf(&root, &mut globals).unwrap();
        assert_eq!(loaded.base_font_name, b"CourierStd");
        assert!(loaded.font_stream.is_none());
        assert_eq!(font.font_type, CidFontType::Type1);
        assert!(font.adobe_courier_std);
        assert!(font.is_vertical());
        assert_eq!(font.get_char_width(65), 725);

        let mut gb = CpdfDictionary::new();
        gb.set_new_name_for(b"Subtype", b"TrueType");
        gb.set_new_name_for(b"BaseFont", b"SimSun");
        let mut font = CpdfCidFontMetrics::default();
        let loaded = font.load_from_pdf(&gb, &mut globals).unwrap();
        assert_eq!(loaded.base_font_name, b"SimSun");
        assert_eq!(font.charset, CidSet::GB1);
        assert_eq!(font.get_char_width(65), 500);
    }
}

#[cfg(test)]
#[path = "cpdf_cidfont_unittest.rs"]
mod upstream_tests;
