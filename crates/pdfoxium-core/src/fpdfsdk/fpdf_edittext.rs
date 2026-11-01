// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Font embedding and text editing from `fpdfsdk/fpdf_edittext.cpp`.
//! The C boundary supplies strings/slices and retains public font handles.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::fpdfapi::edit::cpdf_font_util::{create_widths_array, load_unicode};
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fpdfapi::page::cpdf_textobject::CpdfTextObject;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use crate::fxge::cfx_font::CfxFont;
use crate::fxge::cfx_gemodule::with_ge_module;
use crate::fxge::cfx_path::CfxPath;
use crate::fxge::fx_font::{
    FONT_STYLE_FIXED_PITCH, FONT_STYLE_FORCE_BOLD, FONT_STYLE_ITALIC, FONT_STYLE_NON_SYMBOLIC,
    FONT_STYLE_SERIF,
};

pub const FONT_TYPE1: i32 = 1;
pub const FONT_TRUETYPE: i32 = 2;

fn dictionary_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut borrowed = object.borrow_mut();
    f(borrowed
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("font dictionary"))
}
fn number(dictionary: &mut CpdfDictionary, key: &[u8], value: i32) {
    dictionary.set_new_for(key, CpdfScalarObject::number_from_signed(value));
}
fn reference(dictionary: &ObjectRef, key: &[u8], holder: &HolderRef, target: &ObjectRef) {
    let target_number = target.borrow().obj_num();
    dictionary_mut(dictionary, |dict| {
        dict.set_new_for(key, CpdfReference::new(holder, target_number));
    });
}
fn add_object(
    holder: &HolderRef,
    object: impl crate::fpdfapi::parser::cpdf_object::PdfObject + 'static,
) -> ObjectRef {
    let object = retain_object(object);
    holder.borrow_mut().add_indirect_object(Rc::clone(&object));
    object
}
fn font_name(font: &CfxFont<'_>, font_type: i32) -> Vec<u8> {
    let name = if font_type == FONT_TYPE1 {
        font.ps_name()
    } else {
        font.base_font_name()
    };
    if name.is_empty() {
        b"Untitled".to_vec()
    } else {
        name.into_bytes()
    }
}
fn composite_font_dictionary(holder: &HolderRef, font_type: i32, name: &[u8]) -> ObjectRef {
    let dict = holder.borrow_mut().new_indirect_dictionary();
    dictionary_mut(&dict, |d| {
        d.set_new_name_for(b"Type", b"Font");
        d.set_new_name_for(b"Subtype", b"Type0");
        d.set_new_name_for(b"Encoding", b"Identity-H");
        let base_name = if font_type == FONT_TYPE1 {
            [name, b"-Identity-H"].concat()
        } else {
            name.to_vec()
        };
        d.set_new_name_for(b"BaseFont", &base_name);
    });
    dict
}
fn cid_font_dictionary(holder: &HolderRef, font_type: i32, name: &[u8]) -> ObjectRef {
    let dict = holder.borrow_mut().new_indirect_dictionary();
    dictionary_mut(&dict, |d| {
        d.set_new_name_for(b"Type", b"Font");
        d.set_new_name_for(
            b"Subtype",
            if font_type == FONT_TYPE1 {
                b"CIDFontType0"
            } else {
                b"CIDFontType2"
            },
        );
        d.set_new_name_for(b"BaseFont", name);
    });
    let system = holder.borrow_mut().new_indirect_dictionary();
    dictionary_mut(&system, |d| {
        d.set_new_for(b"Registry", CpdfString::from_bytes(None, b"Adobe"));
        d.set_new_for(b"Ordering", CpdfString::from_bytes(None, b"Identity"));
        number(d, b"Supplement", 0);
    });
    reference(&dict, b"CIDSystemInfo", holder, &system);
    dict
}
fn font_descriptor(
    holder: &HolderRef,
    name: &[u8],
    font: &CfxFont<'_>,
    data: &[u8],
    font_type: i32,
) -> ObjectRef {
    let descriptor = holder.borrow_mut().new_indirect_dictionary();
    dictionary_mut(&descriptor, |d| {
        d.set_new_name_for(b"Type", b"FontDescriptor");
        d.set_new_name_for(b"FontName", name);
        let mut flags = FONT_STYLE_NON_SYMBOLIC;
        if font.is_fixed_width() {
            flags |= FONT_STYLE_FIXED_PITCH;
        }
        if name.windows(5).any(|p| p == b"Serif") {
            flags |= FONT_STYLE_SERIF;
        }
        if font.is_italic() {
            flags |= FONT_STYLE_ITALIC;
        }
        if font.is_bold() {
            flags |= FONT_STYLE_FORCE_BOLD;
        }
        number(d, b"Flags", flags as i32);
        let bbox = font.bbox().unwrap_or_default();
        d.set_rect_for(
            b"FontBBox",
            CfxFloatRect::new(
                bbox.left as f32,
                bbox.bottom as f32,
                bbox.right as f32,
                bbox.top as f32,
            ),
        );
        number(d, b"ItalicAngle", if font.is_italic() { -12 } else { 0 });
        number(d, b"Ascent", font.get_ascent());
        number(d, b"Descent", font.get_descent());
        number(d, b"CapHeight", font.get_ascent());
        number(d, b"StemV", if font.is_bold() { 120 } else { 70 });
    });
    let stream = holder.borrow_mut().new_indirect_stream_from_bytes(data);
    if font_type == FONT_TRUETYPE {
        let dict = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .expect("font stream")
            .get_dict();
        dictionary_mut(&dict, |d| number(d, b"Length1", data.len() as i32));
    }
    reference(
        &descriptor,
        if font_type == FONT_TYPE1 {
            b"FontFile"
        } else {
            b"FontFile2"
        },
        holder,
        &stream,
    );
    descriptor
}
fn descendant_fonts(holder: &HolderRef, font: &ObjectRef, descendant: &ObjectRef) {
    let mut array = CpdfArray::new();
    array.append_new(CpdfReference::new(holder, descendant.borrow().obj_num()));
    dictionary_mut(font, |d| {
        d.set_new_for(b"DescendantFonts", array);
    });
}
fn get_font(document: &CpdfDocument, dictionary: &ObjectRef) -> Option<CpdfFont> {
    with_ge_module(|module| {
        document
            .page_data_ref()
            .get_font(dictionary, &mut module.font_mgr().builtin_mapper_mut())
    })
}

/// Load a simple or Identity-H composite font using its original embedded bytes.
/// Raw Type1 and CFF faces use the same enumerated charmap path as SFNT faces.
pub fn load_font(
    document: &CpdfDocument,
    data: &[u8],
    font_type: i32,
    cid: bool,
) -> Option<CpdfFont> {
    if data.is_empty()
        || data.len() > i32::MAX as usize
        || !matches!(font_type, FONT_TYPE1 | FONT_TRUETYPE)
    {
        return None;
    }
    let font = CfxFont::from_span(data, false, 0).ok()?;
    if !font.has_any_glyphs() {
        return None;
    }
    let codes = font.get_char_codes_and_indices(if cid { 0x10ffff } else { 0xff });
    if codes.is_empty() {
        return None;
    }
    let holder = document.holder();
    let name = font_name(&font, font_type);
    let font_dict;
    if cid {
        font_dict = composite_font_dictionary(&holder, font_type, &name);
        let cid_dict = cid_font_dictionary(&holder, font_type, &name);
        let descriptor = font_descriptor(&holder, &name, &font, data, font_type);
        reference(&cid_dict, b"FontDescriptor", &holder, &descriptor);
        let mut to_unicode = BTreeMap::<u32, Vec<u32>>::new();
        let mut widths = BTreeMap::new();
        for code in codes {
            widths
                .entry(code.glyph_index)
                .or_insert_with(|| font.get_glyph_width(code.glyph_index, 0) as u32);
            to_unicode
                .entry(code.glyph_index)
                .or_default()
                .push(code.char_code);
        }
        let widths = add_object(&holder, create_widths_array(&widths));
        reference(&cid_dict, b"W", &holder, &widths);
        descendant_fonts(&holder, &font_dict, &cid_dict);
        let unicode = add_object(&holder, load_unicode(&to_unicode));
        reference(&font_dict, b"ToUnicode", &holder, &unicode);
    } else {
        font_dict = holder.borrow_mut().new_indirect_dictionary();
        let first = codes.first()?.char_code;
        let last = codes.last()?.char_code;
        dictionary_mut(&font_dict, |d| {
            d.set_new_name_for(b"Type", b"Font");
            d.set_new_name_for(
                b"Subtype",
                if font_type == FONT_TYPE1 {
                    b"Type1"
                } else {
                    b"TrueType"
                },
            );
            d.set_new_name_for(b"BaseFont", &name);
            number(d, b"FirstChar", first as i32);
            number(d, b"LastChar", last as i32);
        });
        let mut widths = CpdfArray::new();
        let mut next_code = first;
        for code in codes {
            // The native loop misplaced gap entries. PDF widths are indexed
            // by charcode, so insert holes before the following width.
            while next_code < code.char_code {
                widths.append_new(CpdfScalarObject::number_from_signed(0));
                next_code += 1;
            }
            widths.append_new(CpdfScalarObject::number_from_signed(
                font.get_glyph_width(code.glyph_index, 0),
            ));
            next_code += 1;
        }
        let widths = add_object(&holder, widths);
        reference(&font_dict, b"Widths", &holder, &widths);
        let descriptor = font_descriptor(&holder, &name, &font, data, font_type);
        reference(&font_dict, b"FontDescriptor", &holder, &descriptor);
    }
    get_font(document, &font_dict)
}

/// Embed a caller-provided CID-to-GID map and ToUnicode CMap.
pub fn load_cid_type2_font(
    document: &CpdfDocument,
    data: &[u8],
    unicode_cmap: &[u8],
    cid_to_gid: &[u8],
) -> Option<CpdfFont> {
    if data.is_empty()
        || data.len() > i32::MAX as usize
        || unicode_cmap.is_empty()
        || cid_to_gid.is_empty()
        || cid_to_gid.len() % 2 != 0
        || cid_to_gid.len() > u32::MAX as usize
    {
        return None;
    }
    let font = CfxFont::from_span(data, false, 0).ok()?;
    if !font.has_any_glyphs() {
        return None;
    }
    let holder = document.holder();
    let name = font_name(&font, FONT_TRUETYPE);
    let font_dict = composite_font_dictionary(&holder, FONT_TRUETYPE, &name);
    let cid_dict = cid_font_dictionary(&holder, FONT_TRUETYPE, &name);
    let descriptor = font_descriptor(&holder, &name, &font, data, FONT_TRUETYPE);
    reference(&cid_dict, b"FontDescriptor", &holder, &descriptor);
    let widths: BTreeMap<u32, u32> = cid_to_gid
        .chunks_exact(2)
        .enumerate()
        .map(|(cid, pair)| {
            (
                cid as u32,
                font.get_glyph_width(u32::from(u16::from_be_bytes([pair[0], pair[1]])), 0) as u32,
            )
        })
        .collect();
    let widths = add_object(&holder, create_widths_array(&widths));
    reference(&cid_dict, b"W", &holder, &widths);
    let map = holder
        .borrow_mut()
        .new_indirect_stream_from_bytes(cid_to_gid);
    reference(&cid_dict, b"CIDToGIDMap", &holder, &map);
    descendant_fonts(&holder, &font_dict, &cid_dict);
    let unicode = holder
        .borrow_mut()
        .new_indirect_stream_from_bytes(unicode_cmap);
    reference(&font_dict, b"ToUnicode", &holder, &unicode);
    get_font(document, &font_dict)
}

pub fn create_text_object(
    document: &CpdfDocument,
    font: &CpdfFont,
    size: f32,
) -> Option<CpdfTextObject> {
    let font = get_font(document, &font.font_dict()?)?;
    Some(new_text_object(font, size))
}
pub fn new_text_object(font: CpdfFont, size: f32) -> CpdfTextObject {
    let mut object = CpdfTextObject::new(-1);
    let dictionary = font.font_dict();
    object
        .base_mut()
        .mutable_text_state()
        .set_font(Some(font), dictionary);
    object.base_mut().mutable_text_state().set_font_size(size);
    object.base_mut().set_default_states();
    object
}
pub fn set_text(object: &mut CpdfTextObject, text: &[u16]) -> bool {
    let Some(font) = object.font() else {
        return false;
    };
    let mut bytes = Vec::new();
    // Public FPDF_WIDESTRING is UTF-16. Combine surrogate pairs so Unicode
    // supplementary mappings survive the Rust host boundary.
    for unicode in char::decode_utf16(text.iter().copied()).map(|c| c.map_or(0xfffd, u32::from)) {
        font.append_char(&mut bytes, font.char_code_from_unicode(unicode));
    }
    if bytes.is_empty() {
        return false;
    }
    object.set_text(&bytes);
    true
}
pub fn set_charcodes(object: &mut CpdfTextObject, codes: &[u32]) -> bool {
    let Some(font) = object.font() else {
        return false;
    };
    let mut bytes = Vec::new();
    for &code in codes {
        font.append_char(&mut bytes, code);
    }
    if bytes.is_empty() {
        return false;
    }
    object.set_text(&bytes);
    true
}
pub fn glyph_width(font: &CpdfFont, unicode: u32, size: f32) -> f32 {
    let code = font.char_code_from_unicode(unicode);
    let width = match font {
        CpdfFont::Cid(cid) if cid.metrics.borrow().is_vertical() => {
            let metrics = cid.metrics.borrow();
            i32::from(metrics.get_vert_width(metrics.cid_from_char_code(code)))
        }
        _ => font.get_char_width(code),
    };
    width as f32 * size / 1000.0
}
pub fn glyph_path(font: &CpdfFont, unicode: u32, size: f32) -> Option<Rc<CfxPath>> {
    if unicode > i32::MAX as u32 {
        return None;
    }
    let code = font.char_code_from_unicode(unicode);
    let positions = with_ge_module(|module| {
        font.get_char_pos_list(
            &[code],
            &[0.0],
            size,
            &mut module.font_mgr().builtin_mapper_mut(),
        )
    });
    let position = positions.first()?;
    font.font_at(position.fallback_font_position)?
        .load_glyph_path(position.glyph_index, position.font_char_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AHEM: &[u8] =
        include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
    fn document() -> CpdfDocument {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        document
    }

    #[test]
    fn simple_and_composite_embedding_keep_original_font_and_pdf_width_mapping() {
        let document = document();
        for composite in [false, true] {
            let font = load_font(&document, AHEM, FONT_TRUETYPE, composite).unwrap();
            assert!(font.is_embedded());
            assert_eq!(font.font().unwrap().font_span(), AHEM);
            let code = font.char_code_from_unicode(u32::from(b'A'));
            assert_eq!(font.get_char_width(code), 1000);
            assert_eq!(glyph_width(&font, u32::from(b'A'), 12.0), 12.0);
            let mut object = create_text_object(&document, &font, 12.0).unwrap();
            assert!(set_text(&mut object, &[u16::from(b'A'), u16::from(b'B')]));
            assert_eq!(object.char_positions(), &[0.0, 12.0]);
            if !composite {
                let dict = font.font_dict().unwrap();
                let borrowed = dict.borrow();
                let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
                let first = dict.get_integer_for(b"FirstChar", 0);
                let last = dict.get_integer_for(b"LastChar", 0);
                let widths = dict.get_array_for(b"Widths").unwrap();
                let widths = widths.borrow();
                let widths = widths.as_any().downcast_ref::<CpdfArray>().unwrap();
                assert_eq!(widths.size(), (last - first + 1) as usize);
                assert_eq!(
                    widths.get_integer_at((i32::from(b'A') - first) as usize),
                    1000
                );
            }
        }
    }

    #[test]
    fn custom_cid_font_embeds_requested_mapping_and_rejects_odd_map() {
        let document = document();
        let face = CfxFont::from_span(AHEM, false, 0).unwrap();
        let gid = face.get_char_index(u32::from(b'A')) as u16;
        let mapping = [0, 0, gid.to_be_bytes()[0], gid.to_be_bytes()[1]];
        let unicode = load_unicode(&BTreeMap::from([(1, vec![u32::from(b'A')])]));
        let cmap = unicode.get_in_memory_raw_data();
        let font = load_cid_type2_font(&document, AHEM, cmap, &mapping).unwrap();
        assert_eq!(font.char_code_from_unicode(u32::from(b'A')), 1);
        assert_eq!(font.glyph_from_char_code(1), i32::from(gid));
        assert_eq!(font.get_char_width(1), 1000);
        assert!(load_cid_type2_font(&document, AHEM, cmap, &[0]).is_none());
        assert!(load_font(&document, b"broken font", FONT_TRUETYPE, false).is_none());
        assert!(load_font(&document, AHEM, 99, false).is_none());
    }

    #[test]
    fn absolute_positions_include_spacing_and_replace_previous_kerning() {
        // Source: CPDF_TextObject::SetAbsolutePositions and FPDFEditEmbedderTest
        // SetPositions/SetPositionsZeroFontSize/SetPositionsBadParams.
        let document = document();
        let font = CpdfFont::get_stock_font(&document, b"Helvetica").unwrap();
        let mut object = new_text_object(font, 12.0);
        object.base_mut().mutable_text_state().set_char_space(1.5);
        object.base_mut().mutable_text_state().set_word_space(3.0);
        assert!(set_charcodes(&mut object, &[65, 32, 66]));
        assert!(object.set_absolute_positions(&[20.0, 42.0]));
        for (&actual, expected) in object.char_positions().iter().zip([0.0, 20.0, 42.0]) {
            assert!((actual - expected).abs() < 0.0001);
        }
        assert!(object.set_absolute_positions(&[9.0, 18.0]));
        for (&actual, expected) in object.char_positions().iter().zip([0.0, 9.0, 18.0]) {
            assert!((actual - expected).abs() < 0.0001);
        }
        assert!(!object.set_absolute_positions(&[9.0]));
        object.base_mut().mutable_text_state().set_font_size(0.0);
        assert!(!object.set_absolute_positions(&[9.0, 18.0]));
        assert!(!set_charcodes(&mut object, &[]));
        assert!(!set_text(&mut object, &[]));
    }

    #[test]
    fn embedded_text_object_bitmap_preserves_transparency_and_scale() {
        let document = document();
        let font = load_font(&document, AHEM, FONT_TRUETYPE, true).unwrap();
        let mut text = new_text_object(font, 12.0);
        assert!(set_text(&mut text, &[u16::from(b'A'), u16::from(b'B')]));
        let bitmap = rendered_bitmap(&document, None, &text, 1.0).unwrap();
        assert!(bitmap.width() > 0 && bitmap.height() > 0);
        assert!(bitmap.buffer().chunks_exact(4).any(|pixel| pixel[3] != 0));
        let larger = rendered_bitmap(&document, None, &text, 2.0).unwrap();
        assert!(larger.width() >= bitmap.width() * 2 - 1);
        assert!(larger.height() >= bitmap.height() * 2 - 1);
        assert!(rendered_bitmap(&document, None, &text, 0.0).is_none());
    }

    #[test]
    fn glyph_path_uses_cached_font_outline_without_scaling_font_units() {
        let document = document();
        let font = load_font(&document, AHEM, FONT_TRUETYPE, true).unwrap();
        let path = glyph_path(&font, u32::from(b'A'), 12.0).unwrap();
        assert!(!path.points().is_empty());
        assert!(Rc::ptr_eq(
            &path,
            &glyph_path(&font, u32::from(b'A'), 24.0).unwrap()
        ));
        assert!(glyph_path(&font, u32::MAX, 12.0).is_none());
    }
}

/// Render one text object into a transparent BGRA bitmap, translating its
/// existing bounds to the top-left and flipping the PDF coordinate system.
pub fn rendered_bitmap(
    document: &CpdfDocument,
    page: Option<&crate::fpdfapi::page::cpdf_page::CpdfPage>,
    object: &CpdfTextObject,
    scale: f32,
) -> Option<crate::fxge::dib::cfx_dibitmap::CfxDibitmap> {
    use crate::fpdfapi::page::cpdf_page::CpdfPage;
    use crate::fxcrt::fx_coordinates::CfxMatrix;
    use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
    use crate::fxge::dib::fx_dib::FxdibFormat;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let text_rect = object.base().rect();
    let scale_matrix = CfxMatrix::new(scale, 0., 0., scale, 0., 0.);
    let rect = scale_matrix
        .transform_float_rect(text_rect)
        .get_outer_rect();
    if rect.is_empty() {
        return None;
    }
    let mut bitmap = CfxDibitmap::create(rect.width(), rect.height(), FxdibFormat::Bgra, 0)?;
    let fallback_page;
    let page = if let Some(page) = page {
        page
    } else {
        let mut dictionary = CpdfDictionary::new();
        dictionary.set_new_name_for(b"Type", b"Page");
        fallback_page =
            CpdfPage::new_with_page_data(retain_object(dictionary), document.page_data_ref())?;
        &fallback_page
    };
    let matrix = CfxMatrix::new(1., 0., 0., -1., -text_rect.left, text_rect.top) * scale_matrix;
    let device_matrix = CfxMatrix::new(rect.width() as f32, 0., 0., rect.height() as f32, 0., 0.);
    crate::fpdfapi::render::cpdf_renderstatus::render_single_page_object_for_sdk(
        page,
        object,
        &mut bitmap,
        matrix,
        device_matrix,
    )
    .then_some(bitmap)
}
