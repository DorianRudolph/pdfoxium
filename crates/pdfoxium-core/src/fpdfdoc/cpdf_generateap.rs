// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Annotation and widget appearance generation from `core/fpdfdoc/cpdf_generateap.cpp`.
//! Generators receive the document's borrowed object table, root and font data.

use super::cpdf_annot::{
    bounding_rect_from_quad_points, quad_point_count, rect_from_quad_points, Subtype,
};
use super::cpdf_color_utils::cfx_color_from_array;
use super::cpdf_defaultappearance::{CpdfDefaultAppearance, FontNameAndSize};
use super::cpdf_formfield::{dict_mut, get_field_attr_for_dict};
use super::cpdf_interactiveform::init_acro_form_dict;
use super::cpvt_fontmap::CpvtFontMap;
use super::cpvt_variabletext::{
    Alignment, CpvtVariableText, FontMapProvider, Iterator as VtIterator,
};
use super::cpvt_wordplace::CpvtWordPlace;
use super::ipvt_fontmap::IpvtFontMap;
use crate::fpdfapi::font::cpdf_font::{CpdfFont, INVALID_CHAR_CODE};
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfapi::parser::fpdf_parser_decode::pdf_encode_string;
use crate::fpdfapi::parser::fpdf_parser_utility::{pdf_name_encode, validate_font_resource_dict};
use crate::fxcrt::bytestring::{eq_cstr, format_character};
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxcrt::fx_string_wrappers::write_default_float;
use crate::fxcrt::stream_write_utils::{write_float, write_point, write_rect};
use crate::fxge::cfx_color::{CfxColor, ColorType};
use crate::fxge::cfx_fontmapper::CfxFontMapper;
use crate::fxge::cfx_fontmgr::with_font_manager;
use std::fmt::{self, Write};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub enum PaintOperation {
    Stroke,
    Fill,
}

pub fn generate_annot_ap(
    objects: &HolderRef,
    root: Option<&ObjectRef>,
    data: &Rc<CpdfDocPageData>,
    mapper: &mut CfxFontMapper,
    annot: &mut CpdfDictionary,
    subtype: Subtype,
) -> bool {
    match subtype {
        Subtype::Circle => generate_circle_ap(objects, annot),
        Subtype::FreeText => generate_free_text_ap(objects, root, data, mapper, annot),
        Subtype::Highlight => generate_highlight_ap(objects, annot),
        Subtype::Ink => generate_ink_ap(objects, annot),
        Subtype::Popup => generate_popup_ap(objects, data, mapper, annot),
        Subtype::Square => generate_square_ap(objects, annot),
        Subtype::Squiggly => generate_squiggly_ap(objects, annot),
        Subtype::StrikeOut => generate_strikeout_ap(objects, annot),
        Subtype::Text => generate_text_ap(objects, annot),
        Subtype::Underline => generate_underline_ap(objects, annot),
        _ => false,
    }
}

// Font loading can invoke user callbacks. Keep annotation/resource borrows out
// of that operation and use the configured document font provider, as rendering does.
fn configured_font(data: &Rc<CpdfDocPageData>, font: &ObjectRef) -> Option<CpdfFont> {
    with_font_manager(|manager| data.get_font(font, &mut manager.builtin_mapper_mut()))
}

/// Free-text editing can opt into multiline, wrapping, top-aligned layout.
/// Native-compatible generation keeps the pinned single-line defaults.
pub fn generate_free_text_ap_for_object(
    objects: &HolderRef,
    root: Option<&ObjectRef>,
    data: &Rc<CpdfDocPageData>,
    annot: &ObjectRef,
    multiline: bool,
) -> bool {
    let Some(root) = root else {
        return false;
    };
    let form = dict_entry(root, b"AcroForm").unwrap_or_else(|| {
        with_font_manager(|manager| {
            init_acro_form_dict(objects, root, data, &mut manager.builtin_mapper_mut())
        })
    });
    let selection = {
        let borrowed = annot.borrow();
        select_free_text_font(
            objects,
            &form,
            borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap(),
        )
    };
    let Some((info, color, font_dict)) = selection else {
        return false;
    };
    let Some(font) = configured_font(data, &font_dict) else {
        return false;
    };
    dict_mut(annot, |annot| {
        write_free_text_ap(objects, annot, info, color, font_dict, font, multiline)
    })
}

pub fn generate_annot_ap_for_object(
    objects: &HolderRef,
    root: Option<&ObjectRef>,
    data: &Rc<CpdfDocPageData>,
    annot: &ObjectRef,
    subtype: Subtype,
) -> bool {
    if subtype == Subtype::FreeText {
        return generate_free_text_ap_for_object(objects, root, data, annot, false);
    }
    if subtype == Subtype::Popup {
        let font_dict = fallback_font_dict(objects);
        let Some(font) = configured_font(data, &font_dict) else {
            return false;
        };
        return dict_mut(annot, |annot| {
            write_popup_ap(objects, annot, font_dict, font)
        });
    }
    // Remaining annotation kinds do not resolve fonts or call the provider.
    dict_mut(annot, |annot| {
        generate_annot_ap(
            objects,
            root,
            data,
            &mut CfxFontMapper::new(),
            annot,
            subtype,
        )
    })
}

pub fn generate_form_ap_for_object(
    objects: &HolderRef,
    root: Option<&ObjectRef>,
    data: &Rc<CpdfDocPageData>,
    annot: &ObjectRef,
    kind: FormType,
) -> bool {
    let selection = {
        let borrowed = annot.borrow();
        select_form_font(
            objects,
            root,
            borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap(),
        )
    };
    let Some((info, color, dr, font_dict)) = selection else {
        return false;
    };
    let Some(font) = configured_font(data, &font_dict) else {
        return false;
    };
    dict_mut(annot, |annot| {
        write_form_ap(objects, annot, kind, info, color, dr, font_dict, font)
    })
}

pub fn generate_default_appearance_with_color_for_object(
    objects: &HolderRef,
    root: Option<&ObjectRef>,
    data: &Rc<CpdfDocPageData>,
    annot: &ObjectRef,
    color: CfxColor,
) -> bool {
    let Some(root) = root else {
        return false;
    };
    let form = dict_entry(root, b"AcroForm").unwrap_or_else(|| {
        with_font_manager(|manager| {
            init_acro_form_dict(objects, root, data, &mut manager.builtin_mapper_mut())
        })
    });
    dict_mut(annot, |annot| {
        write_default_appearance_with_color(&form, annot, color)
    })
}

// Appearance strings can contain arbitrary PDF font names and encoded bytes.
// Only numeric formatting goes through fmt::Write; raw bytes stay unmodified.
struct ByteStream<'a>(&'a mut Vec<u8>);
impl Write for ByteStream<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0.extend_from_slice(value.as_bytes());
        Ok(())
    }
}

fn pdf_word_string(map: &dyn IpvtFontMap, index: i32, word: u16, sub_word: u16) -> Vec<u8> {
    if sub_word > 0 {
        return format_character(sub_word);
    }
    let Some(font) = map.get_pdf_font(index) else {
        return Vec::new();
    };
    if eq_cstr(font.base_font_name(), b"Symbol") || eq_cstr(font.base_font_name(), b"ZapfDingbats")
    {
        return format_character(word);
    }
    let mut result = Vec::new();
    let code = font.char_code_from_unicode(u32::from(word));
    if code != INVALID_CHAR_CODE {
        font.append_char(&mut result, code);
    }
    result
}

fn append_word_render_string(output: &mut Vec<u8>, words: &[u8]) {
    if words.is_empty() {
        return;
    }
    output.extend_from_slice(&pdf_encode_string(words));
    output.extend_from_slice(b" Tj\n");
}

fn append_font_set_string(output: &mut Vec<u8>, map: &dyn IpvtFontMap, index: i32, size: f32) {
    let name = map.get_pdf_font_alias(index);
    if !name.is_empty() && size > 0.0 {
        output.push(b'/');
        output.extend_from_slice(&pdf_name_encode(&name));
        output.push(b' ');
        write_float(&mut ByteStream(output), size).expect("byte write");
        output.extend_from_slice(b" Tf\n");
    }
}

fn append_text_position(output: &mut Vec<u8>, point: CfxPointF) {
    write_point(&mut ByteStream(output), point).expect("byte write");
    output.extend_from_slice(b" Td\n");
}

pub fn generate_edit_ap(
    map: &dyn IpvtFontMap,
    iterator: &mut VtIterator<'_, '_>,
    offset: CfxPointF,
    continuous: bool,
    sub_word: u16,
) -> Vec<u8> {
    let mut edit_stream = Vec::new();
    let mut line_stream = Vec::new();
    let mut old_point = CfxPointF::default();
    let mut new_point = CfxPointF::default();
    let mut current_font_index = -1;
    let mut old_place = CpvtWordPlace::default();
    let mut words = Vec::new();
    iterator.set_at_index(0);
    while iterator.next_word() {
        let place = iterator.word_place();
        let word = iterator.word();
        let is_rtl = word.is_some_and(|word| word.is_rtl);
        if continuous && !is_rtl {
            if place.line_cmp(old_place) != 0 {
                if !words.is_empty() {
                    append_word_render_string(&mut line_stream, &words);
                    edit_stream.extend_from_slice(&line_stream);
                    line_stream.clear();
                    words.clear();
                }
                if let Some(word) = word {
                    new_point = word.location + offset;
                } else if let Some(line) = iterator.line() {
                    new_point = line.point + offset;
                }
                if new_point != old_point {
                    append_text_position(&mut line_stream, new_point - old_point);
                    old_point = new_point;
                }
            } else if words.is_empty() && word.is_some() {
                new_point = word.unwrap().location + offset;
                if new_point != old_point {
                    append_text_position(&mut line_stream, new_point - old_point);
                    old_point = new_point;
                }
            }
            if let Some(word) = word {
                if word.font_index != current_font_index {
                    if !words.is_empty() {
                        append_word_render_string(&mut line_stream, &words);
                        words.clear();
                    }
                    append_font_set_string(&mut line_stream, map, word.font_index, word.font_size);
                    current_font_index = word.font_index;
                }
                words.extend_from_slice(&pdf_word_string(
                    map,
                    current_font_index,
                    word.word,
                    sub_word,
                ));
            }
        } else {
            if !words.is_empty() {
                append_word_render_string(&mut line_stream, &words);
                edit_stream.extend_from_slice(&line_stream);
                line_stream.clear();
                words.clear();
            }
            if let Some(word) = word {
                new_point = word.location + offset;
                if new_point != old_point {
                    append_text_position(&mut edit_stream, new_point - old_point);
                    old_point = new_point;
                }
                if word.font_index != current_font_index {
                    append_font_set_string(&mut edit_stream, map, word.font_index, word.font_size);
                    current_font_index = word.font_index;
                }
                append_word_render_string(
                    &mut edit_stream,
                    &pdf_word_string(map, current_font_index, word.word, sub_word),
                );
            }
        }
        old_place = place;
    }
    if !words.is_empty() {
        append_word_render_string(&mut line_stream, &words);
        edit_stream.extend_from_slice(&line_stream);
    }
    edit_stream
}

#[derive(Clone, Copy, PartialEq)]
enum BorderStyle {
    Solid,
    Dash,
    Beveled,
    Inset,
    Underline,
}
struct BorderStyleInfo {
    width: f32,
    style: BorderStyle,
    dash: [i32; 3],
}
impl Default for BorderStyleInfo {
    fn default() -> Self {
        Self {
            width: 1.0,
            style: BorderStyle::Solid,
            dash: [3, 0, 0],
        }
    }
}
fn border_style_info(dict: Option<&CpdfDictionary>) -> BorderStyleInfo {
    let mut info = BorderStyleInfo::default();
    let Some(dict) = dict else {
        return info;
    };
    if dict.key_exist(b"W") {
        info.width = dict.get_float_for(b"W");
    }
    match dict.get_byte_string_for(b"S", b"").first() {
        Some(b'S') => info.style = BorderStyle::Solid,
        Some(b'D') => info.style = BorderStyle::Dash,
        Some(b'B') => {
            info.style = BorderStyle::Beveled;
            info.width *= 2.0;
        }
        Some(b'I') => {
            info.style = BorderStyle::Inset;
            info.width *= 2.0;
        }
        Some(b'U') => info.style = BorderStyle::Underline,
        _ => {}
    }
    if let Some(dash) = dict.get_array_for(b"D") {
        let borrowed = dash.borrow();
        let array = borrowed
            .as_any()
            .downcast_ref::<CpdfArray>()
            .expect("dash array");
        info.dash = [
            array.get_integer_at(0),
            array.get_integer_at(1),
            array.get_integer_at(2),
        ];
    }
    info
}

fn generate_border_ap(rect: CfxFloatRect, info: &BorderStyleInfo, color: CfxColor) -> String {
    let width = info.width;
    if width <= 0.0 {
        return String::new();
    }
    let (left, bottom, right, top) = (rect.left, rect.bottom, rect.right, rect.top);
    let half = width / 2.0;
    let mut output = String::new();
    let point = |output: &mut String, x, y, op| {
        write_point(output, CfxPointF::new(x, y)).expect("String write");
        output.push_str(op);
    };
    match info.style {
        BorderStyle::Solid => {
            let fill = generate_color_ap(color, PaintOperation::Fill);
            if !fill.is_empty() {
                output.push_str(&fill);
                write_rect(&mut output, rect).expect("String write");
                output.push_str(" re\n");
                let mut inner = rect;
                inner.deflate_xy(width, width);
                write_rect(&mut output, inner).expect("String write");
                output.push_str(" re f*\n");
            }
        }
        BorderStyle::Dash => {
            let stroke = generate_color_ap(color, PaintOperation::Stroke);
            if !stroke.is_empty() {
                output.push_str(&stroke);
                write_float(&mut output, width).expect("String write");
                writeln!(
                    output,
                    " w [{} {}] {} d",
                    info.dash[0], info.dash[1], info.dash[2]
                )
                .expect("String write");
                point(&mut output, left + half, bottom + half, " m\n");
                point(&mut output, left + half, top - half, " l\n");
                point(&mut output, right - half, top - half, " l\n");
                point(&mut output, right - half, bottom + half, " l\n");
                point(&mut output, left + half, bottom + half, " l S\n");
            }
        }
        BorderStyle::Beveled | BorderStyle::Inset => {
            let gray = if info.style == BorderStyle::Beveled {
                1.0
            } else {
                0.5
            };
            output.push_str(&generate_color_ap(
                CfxColor::make_gray(gray),
                PaintOperation::Fill,
            ));
            point(&mut output, left + half, bottom + half, " m\n");
            point(&mut output, left + half, top - half, " l\n");
            point(&mut output, right - half, top - half, " l\n");
            point(&mut output, right - width, top - width, " l\n");
            point(&mut output, left + width, top - width, " l\n");
            point(&mut output, left + width, bottom + width, " l f\n");
            let gray = if info.style == BorderStyle::Beveled {
                0.5
            } else {
                0.75
            };
            output.push_str(&generate_color_ap(
                CfxColor::make_gray(gray),
                PaintOperation::Fill,
            ));
            point(&mut output, right - half, top - half, " m\n");
            point(&mut output, right - half, bottom + half, " l\n");
            point(&mut output, left + half, bottom + half, " l\n");
            point(&mut output, left + width, bottom + width, " l\n");
            point(&mut output, right - width, bottom + width, " l\n");
            point(&mut output, right - width, top - width, " l f\n");
            let fill = generate_color_ap(color, PaintOperation::Fill);
            if !fill.is_empty() {
                output.push_str(&fill);
                write_rect(&mut output, rect).expect("String write");
                output.push_str(" re\n");
                let mut inner = rect;
                inner.deflate_xy(half, half);
                write_rect(&mut output, inner).expect("String write");
                output.push_str(" re f*\n");
            }
        }
        BorderStyle::Underline => {
            let stroke = generate_color_ap(color, PaintOperation::Stroke);
            if !stroke.is_empty() {
                output.push_str(&stroke);
                write_float(&mut output, width).expect("String write");
                output.push_str(" w\n");
                point(&mut output, left, bottom + half, " m\n");
                point(&mut output, right, bottom + half, " l S\n");
            }
        }
    }
    output
}

pub fn generate_free_text_ap(
    objects: &HolderRef,
    root: Option<&ObjectRef>,
    data: &Rc<CpdfDocPageData>,
    mapper: &mut CfxFontMapper,
    annot: &mut CpdfDictionary,
) -> bool {
    let Some(root) = root else {
        return false;
    };
    let form = root
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .expect("root dictionary")
        .get_dict_for(b"AcroForm");
    let form = form.unwrap_or_else(|| init_acro_form_dict(objects, root, data, mapper));
    let Some((font_info, color, font_dict)) = select_free_text_font(objects, &form, annot) else {
        return false;
    };
    let Some(default_font) = data.get_font(&font_dict, mapper) else {
        return false;
    };
    write_free_text_ap(
        objects,
        annot,
        font_info,
        color,
        font_dict,
        default_font,
        false,
    )
}

fn select_free_text_font(
    objects: &HolderRef,
    form: &ObjectRef,
    annot: &CpdfDictionary,
) -> Option<(FontNameAndSize, CfxColor, ObjectRef)> {
    let (font_info, color, fonts) = {
        let borrowed = form.borrow();
        let form = borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("AcroForm dictionary");
        let appearance = CpdfDefaultAppearance::from_dicts(Some(annot), Some(form));
        let Some(font_info) = appearance.font() else {
            return None;
        };
        let color = appearance
            .color()
            .unwrap_or_else(CfxColor::make_transparent);
        let Some(dr) = form.get_dict_for(b"DR") else {
            return None;
        };
        let fonts = dr
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("DR dictionary")
            .get_dict_for(b"Font");
        (font_info, color, fonts)
    };
    let Some(fonts) = fonts else {
        return None;
    };
    let font_dict = {
        let borrowed = fonts.borrow();
        let fonts = borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("font dictionary");
        if !validate_font_resource_dict(Some(fonts)) {
            return None;
        }
        fonts.get_dict_for(&font_info.name)
    };
    let font_dict = font_dict.unwrap_or_else(|| {
        let mut dict = CpdfDictionary::with_pool(objects.borrow().byte_string_pool());
        dict.set_new_name_for(b"Type", b"Font");
        dict.set_new_name_for(b"Subtype", b"Type1");
        dict.set_new_name_for(b"BaseFont", b"Helvetica");
        dict.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
        let dict = retain_object(dict);
        let number = objects.borrow_mut().add_indirect_object(Rc::clone(&dict));
        fonts
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .expect("font dictionary")
            .set_new_for(&font_info.name, CpdfReference::new(objects, number));
        dict
    });
    Some((font_info, color, font_dict))
}

fn write_free_text_ap(
    objects: &HolderRef,
    annot: &mut CpdfDictionary,
    font_info: FontNameAndSize,
    color: CfxColor,
    font_dict: ObjectRef,
    default_font: CpdfFont,
    multiline: bool,
) -> bool {
    let mut output = Vec::from(b"/GS gs ");
    let style = annot.get_dict_for(b"BS");
    let info = if let Some(style) = style {
        let borrowed = style.borrow();
        border_style_info(borrowed.as_any().downcast_ref::<CpdfDictionary>())
    } else {
        BorderStyleInfo::default()
    };
    let rect = annot.get_rect_for(b"Rect");
    let half_width = info.width / 2.0;
    let mut background = rect;
    background.deflate_xy(half_width, half_width);
    let mut body_rect = background;
    body_rect.deflate_xy(half_width, half_width);
    if let Some(array) = annot.get_array_for(b"C") {
        let borrowed = array.borrow();
        let color = cfx_color_from_array(
            borrowed
                .as_any()
                .downcast_ref::<CpdfArray>()
                .expect("color array"),
        );
        output.extend_from_slice(b"q\n");
        output.extend_from_slice(generate_color_ap(color, PaintOperation::Fill).as_bytes());
        write_rect(&mut ByteStream(&mut output), background).expect("byte write");
        output.extend_from_slice(b" re f\nQ\n");
    }
    let border = generate_border_ap(rect, &info, color);
    if !border.is_empty() {
        output.extend_from_slice(b"q\n");
        output.extend_from_slice(border.as_bytes());
        output.extend_from_slice(b"Q\n");
    }
    let map = CpvtFontMap::new(Some(default_font), font_info.name);
    let provider = FontMapProvider { font_map: &map };
    let mut vt = CpvtVariableText::new(Some(&provider));
    vt.set_plate_rect(body_rect);
    vt.set_alignment(Alignment::from_pdf(annot.get_integer_for(b"Q", 0)));
    vt.set_multi_line(multiline);
    vt.set_auto_return(multiline);
    if font_info.size < 0.0001 && font_info.size > -0.0001 {
        vt.set_auto_font_size(true);
    } else {
        vt.set_font_size(font_info.size);
    }
    vt.initialize();
    vt.set_text(&annot.get_unicode_text_for(b"Contents"));
    vt.rearrange_all();
    let content_rect = vt.content_rect();
    let offset = CfxPointF::new(
        0.0,
        if multiline {
            0.0
        } else {
            (content_rect.height() - body_rect.height()) / 2.0
        },
    );
    let body = generate_edit_ap(&map, &mut vt.iterator(), offset, true, 0);
    if !body.is_empty() {
        output.extend_from_slice(b"/Tx BMC\nq\n");
        if content_rect.width() > body_rect.width() || content_rect.height() > body_rect.height() {
            write_rect(&mut ByteStream(&mut output), body_rect).expect("byte write");
            output.extend_from_slice(b" re\nW\nn\n");
        }
        output.extend_from_slice(b"BT\n");
        output.extend_from_slice(generate_color_ap(color, PaintOperation::Fill).as_bytes());
        output.extend_from_slice(&body);
        output.extend_from_slice(b"ET\nQ\nEMC\n");
    }
    let resources = resources(objects, ext_gstate(annot, b"Normal"));
    let mut font_resources = CpdfDictionary::with_pool(objects.borrow().byte_string_pool());
    font_resources.set_new_for(
        &map.get_pdf_font_alias(0),
        CpdfReference::new(objects, font_dict.borrow().obj_num()),
    );
    resources
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("resources dictionary")
        .set_new_for(b"Font", font_resources);
    set_ap(objects, annot, output, resources, false);
    true
}

pub fn generate_color_ap(color: CfxColor, operation: PaintOperation) -> String {
    let (values, stroke, fill): (&[f32], &str, &str) = match color.color_type {
        ColorType::Transparent => return String::new(),
        ColorType::Gray => (&[color.color1], "G", "g"),
        ColorType::Rgb => (&[color.color1, color.color2, color.color3], "RG", "rg"),
        ColorType::Cmyk => (
            &[color.color1, color.color2, color.color3, color.color4],
            "K",
            "k",
        ),
    };
    let mut result = String::new();
    for &value in values {
        write_float(&mut result, value).expect("String write");
        result.push(' ');
    }
    result.push_str(match operation {
        PaintOperation::Stroke => stroke,
        PaintOperation::Fill => fill,
    });
    result.push('\n');
    result
}

fn color_with_default(
    array: Option<&ObjectRef>,
    default: CfxColor,
    operation: PaintOperation,
) -> String {
    if let Some(array) = array {
        let borrowed = array.borrow();
        if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
            return generate_color_ap(cfx_color_from_array(array), operation);
        }
    }
    generate_color_ap(default, operation)
}

fn border_width(annot: &CpdfDictionary) -> f32 {
    if let Some(style) = annot.get_dict_for(b"BS") {
        let borrowed = style.borrow();
        if let Some(style) = borrowed.as_any().downcast_ref::<CpdfDictionary>() {
            if style.key_exist(b"W") {
                return style.get_float_for(b"W");
            }
        }
    }
    if let Some(border) = annot.get_array_for(b"Border") {
        if let Some(border) = border.borrow().as_any().downcast_ref::<CpdfArray>() {
            if border.size() > 2 {
                return border.get_float_at(2);
            }
        }
    }
    1.0
}

fn dash_array(annot: &CpdfDictionary) -> Option<ObjectRef> {
    if let Some(style) = annot.get_dict_for(b"BS") {
        let borrowed = style.borrow();
        if let Some(style) = borrowed.as_any().downcast_ref::<CpdfDictionary>() {
            if eq_cstr(&style.get_byte_string_for(b"S", b""), b"D") {
                return style.get_array_for(b"D");
            }
        }
    }
    let border = annot.get_array_for(b"Border")?;
    let borrowed = border.borrow();
    let border = borrowed.as_any().downcast_ref::<CpdfArray>()?;
    if border.size() == 4 {
        border.get_array_at(3)
    } else {
        None
    }
}

fn dash_pattern(annot: &CpdfDictionary) -> String {
    let Some(array) = dash_array(annot) else {
        return String::new();
    };
    let borrowed = array.borrow();
    let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
        return String::new();
    };
    if array.is_empty() {
        return String::new();
    }
    let mut result = String::from("[");
    for index in 0..array.size().min(10) {
        write_float(&mut result, array.get_float_at(index)).expect("String write");
        result.push(' ');
    }
    result.push_str("] 0 d\n");
    result
}

fn paint_operator(stroke: bool, fill: bool) -> &'static str {
    match (stroke, fill) {
        (true, true) => "b",
        (true, false) => "s",
        (false, true) => "f",
        (false, false) => "n",
    }
}

fn default_numbers(output: &mut String, values: &[f32]) {
    for (index, &value) in values.iter().enumerate() {
        if index != 0 {
            output.push(' ');
        }
        write_default_float(output, value).expect("String write");
    }
}

fn ext_gstate(annot: &CpdfDictionary, blend_mode: &[u8]) -> ObjectRef {
    let mut gs = CpdfDictionary::with_pool(annot.byte_string_pool());
    gs.set_new_name_for(b"Type", b"ExtGState");
    let opacity = if annot.key_exist(b"CA") {
        annot.get_float_for(b"CA")
    } else {
        1.0
    };
    gs.set_new_for(b"CA", CpdfScalarObject::number_from_float(opacity));
    gs.set_new_for(b"ca", CpdfScalarObject::number_from_float(opacity));
    gs.set_new_for(b"AIS", CpdfScalarObject::boolean(false));
    gs.set_new_name_for(b"BM", blend_mode);
    let mut resources = CpdfDictionary::with_pool(annot.byte_string_pool());
    resources.set_new_for(b"GS", gs);
    retain_object(resources)
}

fn resources(objects: &HolderRef, gs: ObjectRef) -> ObjectRef {
    let mut resources = CpdfDictionary::with_pool(objects.borrow().byte_string_pool());
    resources.set_for(b"ExtGState", Some(gs));
    retain_object(resources)
}

fn set_ap(
    objects: &HolderRef,
    annot: &mut CpdfDictionary,
    content: impl Into<Vec<u8>>,
    resources: ObjectRef,
    text_markup: bool,
) {
    let mut dict = CpdfDictionary::with_pool(objects.borrow().byte_string_pool());
    dict.set_new_for(b"FormType", CpdfScalarObject::number_from_signed(1));
    dict.set_new_name_for(b"Type", b"XObject");
    dict.set_new_name_for(b"Subtype", b"Form");
    dict.set_matrix_for(b"Matrix", CfxMatrix::default());
    let rect = if text_markup {
        bounding_rect_from_quad_points(annot)
    } else {
        annot.get_rect_for(b"Rect")
    };
    dict.set_rect_for(b"BBox", rect);
    dict.set_for(b"Resources", Some(resources));
    let stream = retain_object(CpdfStream::from_data_and_dict(
        content.into(),
        retain_object(dict),
    ));
    let number = objects.borrow_mut().add_indirect_object(stream);
    let ap = annot.get_or_create_dict_for(b"AP");
    ap.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("AP dictionary")
        .set_new_for(b"N", CpdfReference::new(objects, number));
}

fn shape_prefix(annot: &CpdfDictionary) -> (String, bool) {
    let interior = annot.get_array_for(b"IC");
    let mut output = String::from("/GS gs ");
    output.push_str(&color_with_default(
        interior.as_ref(),
        CfxColor::make_transparent(),
        PaintOperation::Fill,
    ));
    output.push_str(&color_with_default(
        annot.get_array_for(b"C").as_ref(),
        CfxColor::make_rgb(0.0, 0.0, 0.0),
        PaintOperation::Stroke,
    ));
    let fill = interior.is_some_and(|array| {
        array
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .is_some_and(|array| !array.is_empty())
    });
    (output, fill)
}

pub fn generate_square_ap(objects: &HolderRef, annot: &mut CpdfDictionary) -> bool {
    let (mut output, fill) = shape_prefix(annot);
    let width = border_width(annot);
    let stroke = width > 0.0;
    if stroke {
        default_numbers(&mut output, &[width]);
        output.push_str(" w ");
        output.push_str(&dash_pattern(annot));
    }
    let mut rect = annot.get_rect_for(b"Rect");
    rect.normalize();
    if stroke {
        rect.deflate_xy(width / 2.0, width / 2.0);
    }
    default_numbers(
        &mut output,
        &[rect.left, rect.bottom, rect.width(), rect.height()],
    );
    output.push_str(" re ");
    output.push_str(paint_operator(stroke, fill));
    output.push('\n');
    let resources = resources(objects, ext_gstate(annot, b"Normal"));
    set_ap(objects, annot, output, resources, false);
    true
}

pub fn generate_circle_ap(objects: &HolderRef, annot: &mut CpdfDictionary) -> bool {
    let (mut output, fill) = shape_prefix(annot);
    let width = border_width(annot);
    let stroke = width > 0.0;
    if stroke {
        default_numbers(&mut output, &[width]);
        output.push_str(" w ");
        output.push_str(&dash_pattern(annot));
    }
    let mut rect = annot.get_rect_for(b"Rect");
    rect.normalize();
    if stroke {
        rect.deflate_xy(width / 2.0, width / 2.0);
    }
    let x = (rect.left + rect.right) / 2.0;
    let y = (rect.top + rect.bottom) / 2.0;
    // Four cubic Beziers use the pinned approximate quarter-circle factor.
    let dx = 0.5523 * rect.width() / 2.0;
    let dy = 0.5523 * rect.height() / 2.0;
    default_numbers(&mut output, &[x, rect.top]);
    output.push_str(" m\n");
    for points in [
        [x + dx, rect.top, rect.right, y + dy, rect.right, y],
        [rect.right, y - dy, x + dx, rect.bottom, x, rect.bottom],
        [x - dx, rect.bottom, rect.left, y - dy, rect.left, y],
        [rect.left, y + dy, x - dx, rect.top, x, rect.top],
    ] {
        default_numbers(&mut output, &points);
        output.push_str(" c\n");
    }
    output.push_str(paint_operator(stroke, fill));
    output.push('\n');
    let resources = resources(objects, ext_gstate(annot, b"Normal"));
    set_ap(objects, annot, output, resources, false);
    true
}

pub fn generate_highlight_ap(objects: &HolderRef, annot: &mut CpdfDictionary) -> bool {
    let mut output = String::from("/GS gs ");
    output.push_str(&color_with_default(
        annot.get_array_for(b"C").as_ref(),
        CfxColor::make_rgb(1.0, 1.0, 0.0),
        PaintOperation::Fill,
    ));
    if let Some(quads) = annot.get_array_for(b"QuadPoints") {
        let count = quad_point_count(
            quads
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .expect("quad array"),
        );
        for index in 0..count {
            let mut rect = rect_from_quad_points(annot, index);
            rect.normalize();
            default_numbers(&mut output, &[rect.left, rect.top]);
            output.push_str(" m ");
            default_numbers(&mut output, &[rect.right, rect.top]);
            output.push_str(" l ");
            default_numbers(&mut output, &[rect.right, rect.bottom]);
            output.push_str(" l ");
            default_numbers(&mut output, &[rect.left, rect.bottom]);
            output.push_str(" l h f\n");
        }
    }
    let resources = resources(objects, ext_gstate(annot, b"Multiply"));
    set_ap(objects, annot, output, resources, true);
    true
}

pub fn generate_ink_ap(objects: &HolderRef, annot: &mut CpdfDictionary) -> bool {
    let Some(ink) = annot.get_array_for(b"InkList") else {
        return false;
    };
    let borrowed = ink.borrow();
    let ink = borrowed
        .as_any()
        .downcast_ref::<CpdfArray>()
        .expect("ink array");
    if ink.is_empty() {
        return false;
    }
    let width = border_width(annot);
    if width <= 0.0 || width.is_nan() {
        return false;
    }
    let mut output = String::from("/GS gs ");
    output.push_str(&color_with_default(
        annot.get_array_for(b"C").as_ref(),
        CfxColor::make_rgb(0.0, 0.0, 0.0),
        PaintOperation::Stroke,
    ));
    default_numbers(&mut output, &[width]);
    output.push_str(" w ");
    output.push_str(&dash_pattern(annot));
    // Paths at the edge with wide strokes need the inflated annotation rect.
    let mut rect = annot.get_rect_for(b"Rect");
    rect.inflate_xy(width / 2.0, width / 2.0);
    annot.set_rect_for(b"Rect", rect);
    for index in 0..ink.size() {
        let Some(coordinates) = ink.get_array_at(index) else {
            continue;
        };
        let borrowed = coordinates.borrow();
        let coordinates = borrowed
            .as_any()
            .downcast_ref::<CpdfArray>()
            .expect("coordinates array");
        if coordinates.size() < 2 {
            continue;
        }
        default_numbers(
            &mut output,
            &[coordinates.get_float_at(0), coordinates.get_float_at(1)],
        );
        output.push_str(" m ");
        for index in (0..coordinates.size() - 1).step_by(2) {
            // Native deliberately repeats the initial point as a line vertex.
            default_numbers(
                &mut output,
                &[
                    coordinates.get_float_at(index),
                    coordinates.get_float_at(index + 1),
                ],
            );
            output.push_str(" l ");
        }
        output.push_str("S\n");
    }
    let resources = resources(objects, ext_gstate(annot, b"Normal"));
    set_ap(objects, annot, output, resources, false);
    true
}

pub fn generate_underline_ap(objects: &HolderRef, annot: &mut CpdfDictionary) -> bool {
    let mut output = String::from("/GS gs ");
    output.push_str(&color_with_default(
        annot.get_array_for(b"C").as_ref(),
        CfxColor::make_rgb(0.0, 0.0, 0.0),
        PaintOperation::Stroke,
    ));
    if let Some(quads) = annot.get_array_for(b"QuadPoints") {
        output.push_str("1 w ");
        let count = quad_point_count(
            quads
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .expect("quad array"),
        );
        for index in 0..count {
            let mut rect = rect_from_quad_points(annot, index);
            rect.normalize();
            default_numbers(&mut output, &[rect.left, rect.bottom + 1.0]);
            output.push_str(" m ");
            default_numbers(&mut output, &[rect.right, rect.bottom + 1.0]);
            output.push_str(" l S\n");
        }
    }
    let resources = resources(objects, ext_gstate(annot, b"Normal"));
    set_ap(objects, annot, output, resources, true);
    true
}

pub fn generate_strikeout_ap(objects: &HolderRef, annot: &mut CpdfDictionary) -> bool {
    let mut output = String::from("/GS gs ");
    output.push_str(&color_with_default(
        annot.get_array_for(b"C").as_ref(),
        CfxColor::make_rgb(0.0, 0.0, 0.0),
        PaintOperation::Stroke,
    ));
    if let Some(quads) = annot.get_array_for(b"QuadPoints") {
        let count = quad_point_count(
            quads
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .expect("quad array"),
        );
        for index in 0..count {
            let mut rect = rect_from_quad_points(annot, index);
            rect.normalize();
            let y = (rect.top + rect.bottom) / 2.0;
            output.push_str("1 w ");
            default_numbers(&mut output, &[rect.left, y]);
            output.push_str(" m ");
            default_numbers(&mut output, &[rect.right, y]);
            output.push_str(" l S\n");
        }
    }
    let resources = resources(objects, ext_gstate(annot, b"Normal"));
    set_ap(objects, annot, output, resources, true);
    true
}

pub fn generate_squiggly_ap(objects: &HolderRef, annot: &mut CpdfDictionary) -> bool {
    let mut output = String::from("/GS gs ");
    output.push_str(&color_with_default(
        annot.get_array_for(b"C").as_ref(),
        CfxColor::make_rgb(0.0, 0.0, 0.0),
        PaintOperation::Stroke,
    ));
    if let Some(quads) = annot.get_array_for(b"QuadPoints") {
        output.push_str("1 w ");
        let count = quad_point_count(
            quads
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .expect("quad array"),
        );
        for index in 0..count {
            let mut rect = rect_from_quad_points(annot, index);
            rect.normalize();
            let top = rect.bottom + 2.0;
            let bottom = rect.bottom;
            default_numbers(&mut output, &[rect.left, top]);
            output.push_str(" m ");
            let mut x = rect.left + 2.0;
            let mut upwards = false;
            while x < rect.right {
                default_numbers(&mut output, &[x, if upwards { top } else { bottom }]);
                output.push_str(" l ");
                let next = x + 2.0;
                if next == x {
                    return false;
                } // Native would loop forever at this precision.
                x = next;
                upwards = !upwards;
            }
            let remainder = rect.right - (x - 2.0);
            default_numbers(
                &mut output,
                &[
                    rect.right,
                    if upwards {
                        bottom + remainder
                    } else {
                        top - remainder
                    },
                ],
            );
            output.push_str(" l S\n");
        }
    }
    let resources = resources(objects, ext_gstate(annot, b"Normal"));
    set_ap(objects, annot, output, resources, true);
    true
}

fn generate_text_symbol_ap(rect: CfxFloatRect) -> String {
    let mut output = generate_color_ap(CfxColor::make_rgb(1.0, 1.0, 0.0), PaintOperation::Fill);
    output.push_str(&generate_color_ap(
        CfxColor::make_rgb(0.0, 0.0, 0.0),
        PaintOperation::Stroke,
    ));
    output.push_str("1 w\n");
    let mut outer = rect;
    outer.deflate_xy(0.5, 0.5);
    outer.bottom += 4.0;
    let mut tip = outer;
    tip.left += 4.0;
    tip.right = tip.left + 4.0;
    tip.top = tip.bottom - 4.0;
    let middle = (tip.left + tip.right) / 2.0;
    for (index, point) in [
        CfxPointF::new(outer.left, outer.bottom),
        CfxPointF::new(outer.left, outer.top),
        CfxPointF::new(outer.right, outer.top),
        CfxPointF::new(outer.right, outer.bottom),
        CfxPointF::new(tip.right, tip.bottom),
        CfxPointF::new(middle, tip.top),
        CfxPointF::new(tip.left, tip.bottom),
        CfxPointF::new(outer.left, outer.bottom),
    ]
    .into_iter()
    .enumerate()
    {
        write_point(&mut output, point).expect("String write");
        output.push_str(if index == 0 { " m\n" } else { " l\n" });
    }
    let mut line = outer;
    let dy = (line.top - line.bottom) / 4.0;
    line.left += 2.0;
    line.right -= 2.0;
    for _ in 0..3 {
        line.top -= dy;
        write_point(&mut output, CfxPointF::new(line.left, line.top)).expect("String write");
        output.push_str(" m\n");
        write_point(&mut output, CfxPointF::new(line.right, line.top)).expect("String write");
        output.push_str(" l\n");
    }
    output.push_str("B*\n");
    output
}

pub fn generate_text_ap(objects: &HolderRef, annot: &mut CpdfDictionary) -> bool {
    let rect = annot.get_rect_for(b"Rect");
    let note_rect = CfxFloatRect::new(rect.left, rect.bottom, rect.left + 20.0, rect.bottom + 20.0);
    annot.set_rect_for(b"Rect", note_rect);
    let mut output = String::from("/GS gs ");
    output.push_str(&generate_text_symbol_ap(note_rect));
    let resources = resources(objects, ext_gstate(annot, b"Normal"));
    set_ap(objects, annot, output, resources, false);
    true
}

/// The three ordinary widget appearances generated by CPDF_GenerateAP.
/// Button appearances belong to the SDK's AppStream implementation.
#[derive(Clone, Copy, Debug)]
pub enum FormType {
    TextField,
    ComboBox,
    ListBox,
}

fn field_integer(annot: &CpdfDictionary, name: &[u8]) -> i32 {
    get_field_attr_for_dict(Some(annot), name).map_or(0, |v| v.borrow().get_integer())
}
fn field_text(annot: &CpdfDictionary, name: &[u8]) -> Vec<u32> {
    get_field_attr_for_dict(Some(annot), name)
        .map_or_else(Vec::new, |v| v.borrow().get_unicode_text())
}
fn set_vt_font_size(vt: &mut CpvtVariableText<'_>, size: f32) {
    if size > -0.0001 && size < 0.0001 {
        vt.set_auto_font_size(true);
    } else {
        vt.set_font_size(size);
    }
}

fn annotation_dimensions_and_color(
    annot: &CpdfDictionary,
) -> (CfxFloatRect, CfxMatrix, CfxColor, CfxColor) {
    let (rotation, border, background) = annot.get_dict_for(b"MK").map_or(
        (
            0,
            CfxColor::make_transparent(),
            CfxColor::make_transparent(),
        ),
        |mk| {
            let b = mk.borrow();
            let mk = b.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            let color = |name| {
                mk.get_array_for(name)
                    .map_or_else(CfxColor::make_transparent, |array| {
                        cfx_color_from_array(
                            array.borrow().as_any().downcast_ref::<CpdfArray>().unwrap(),
                        )
                    })
            };
            (mk.get_integer_for(b"R", 0), color(b"BC"), color(b"BG"))
        },
    );
    let rect = annot.get_rect_for(b"Rect");
    let (w, h) = (rect.right - rect.left, rect.top - rect.bottom);
    let (bbox, matrix) = match rotation.rem_euclid(360) {
        0 => (CfxFloatRect::new(0., 0., w, h), CfxMatrix::default()),
        90 => (
            CfxFloatRect::new(0., 0., h, w),
            CfxMatrix::new(0., 1., -1., 0., w, 0.),
        ),
        180 => (
            CfxFloatRect::new(0., 0., w, h),
            CfxMatrix::new(-1., 0., 0., -1., w, h),
        ),
        270 => (
            CfxFloatRect::new(0., 0., h, w),
            CfxMatrix::new(0., -1., 1., 0., 0., h),
        ),
        _ => (CfxFloatRect::default(), CfxMatrix::default()),
    };
    (bbox, matrix, border, background)
}

fn fallback_font_dict(objects: &HolderRef) -> ObjectRef {
    let mut dict = CpdfDictionary::with_pool(objects.borrow().byte_string_pool());
    dict.set_new_name_for(b"Type", b"Font");
    dict.set_new_name_for(b"Subtype", b"Type1");
    dict.set_new_name_for(b"BaseFont", b"Helvetica");
    dict.set_new_name_for(b"Encoding", b"WinAnsiEncoding");
    let dict = retain_object(dict);
    objects.borrow_mut().add_indirect_object(dict.clone());
    dict
}
fn get_font_or_fallback(objects: &HolderRef, fonts: &ObjectRef, name: &[u8]) -> ObjectRef {
    let existing = fonts
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .get_dict_for(name);
    existing.unwrap_or_else(|| {
        let font = fallback_font_dict(objects);
        let number = font.borrow().obj_num();
        fonts
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(name, CpdfReference::new(objects, number));
        font
    })
}
fn stream_dictionary(stream: &ObjectRef) -> ObjectRef {
    stream
        .borrow()
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .get_dict()
}
fn dict_entry(dict: &ObjectRef, name: &[u8]) -> Option<ObjectRef> {
    dict.borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_dict_for(name)
}
fn clone_resources_if_missing(stream_dict: &ObjectRef, dr: &ObjectRef) -> bool {
    if dict_entry(stream_dict, b"Resources").is_some() {
        return false;
    }
    // This is the source's explicit resource-dictionary clone, not a borrow workaround.
    let resources = dr.borrow().clone_object();
    stream_dict
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(b"Resources", Some(resources));
    true
}
fn validate_or_create_font_resources(
    objects: &HolderRef,
    stream_dict: &ObjectRef,
    font: &ObjectRef,
    name: &[u8],
) -> bool {
    let Some(resources) = dict_entry(stream_dict, b"Resources") else {
        return false;
    };
    let fonts = resources
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .get_or_create_dict_for(b"Font");
    let valid = {
        let borrowed = fonts.borrow();
        validate_font_resource_dict(borrowed.as_any().downcast_ref::<CpdfDictionary>())
    };
    if !valid {
        return false;
    }
    let number = font.borrow().obj_num();
    let mut borrowed = fonts.borrow_mut();
    let dict = borrowed
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap();
    if !dict.key_exist(name) {
        dict.set_new_for(name, CpdfReference::new(objects, number));
    }
    true
}

fn generate_text_field_ap(
    annot: &CpdfDictionary,
    body: CfxFloatRect,
    size: f32,
    map: &CpvtFontMap,
    vt: &mut CpvtVariableText<'_>,
) -> Vec<u8> {
    let flags = field_integer(annot, b"Ff") as u32;
    let max_length = field_integer(annot, b"MaxLen");
    vt.set_plate_rect(body);
    vt.set_alignment(Alignment::from_pdf(field_integer(annot, b"Q")));
    set_vt_font_size(vt, size);
    let multiline = flags & (1 << 12) != 0;
    if multiline {
        vt.set_multi_line(true);
        vt.set_auto_return(true);
    }
    let substitute = if flags & (1 << 13) != 0 {
        b'*' as u16
    } else {
        0
    };
    if substitute != 0 {
        vt.set_password_char(substitute);
    }
    let comb = flags & (1 << 24) != 0;
    if comb {
        vt.set_char_array(max_length);
    } else {
        vt.set_limit_char(max_length);
    }
    vt.initialize();
    vt.set_text(&field_text(annot, b"V"));
    vt.rearrange_all();
    let offset = if multiline {
        CfxPointF::default()
    } else {
        CfxPointF::new(0., (vt.content_rect().height() - body.height()) / 2.)
    };
    generate_edit_ap(map, &mut vt.iterator(), offset, !comb, substitute)
}

fn generate_combo_box_ap(
    annot: &CpdfDictionary,
    body: CfxFloatRect,
    color: CfxColor,
    size: f32,
    map: &CpvtFontMap,
    provider: &FontMapProvider<'_>,
) -> Vec<u8> {
    let mut output = Vec::new();
    let mut button_rect = body;
    button_rect.left = button_rect.right - 13.;
    button_rect.normalize();
    let mut edit_rect = body;
    edit_rect.right = button_rect.left;
    edit_rect.normalize();
    let mut vt = CpvtVariableText::new(Some(provider));
    vt.set_plate_rect(edit_rect);
    set_vt_font_size(&mut vt, size);
    vt.initialize();
    vt.set_text(&field_text(annot, b"V"));
    vt.rearrange_all();
    let offset = CfxPointF::new(0., (vt.content_rect().height() - edit_rect.height()) / 2.);
    let edit = generate_edit_ap(map, &mut vt.iterator(), offset, true, 0);
    if !edit.is_empty() {
        output.extend_from_slice(b"/Tx BMC\nq\n");
        write_rect(&mut ByteStream(&mut output), edit_rect).unwrap();
        output.extend_from_slice(b" re\nW\nn\nBT\n");
        output.extend_from_slice(generate_color_ap(color, PaintOperation::Fill).as_bytes());
        output.extend_from_slice(&edit);
        output.extend_from_slice(b"ET\nQ\nEMC\n");
    }
    let button = generate_color_ap(
        CfxColor::make_rgb_bytes(220, 220, 220),
        PaintOperation::Fill,
    );
    if !button.is_empty() && !button_rect.is_empty() {
        output.extend_from_slice(b"q\n");
        output.extend_from_slice(button.as_bytes());
        write_rect(&mut ByteStream(&mut output), button_rect).unwrap();
        output.extend_from_slice(b" re f\nQ\n");
        let border = generate_border_ap(
            button_rect,
            &BorderStyleInfo {
                width: 2.,
                style: BorderStyle::Beveled,
                dash: [3, 0, 0],
            },
            CfxColor::make_gray(0.),
        );
        if !border.is_empty() {
            output.extend_from_slice(b"q\n");
            output.extend_from_slice(border.as_bytes());
            output.extend_from_slice(b"Q\n");
        }
        let center = CfxPointF::new(
            (button_rect.left + button_rect.right) / 2.,
            (button_rect.top + button_rect.bottom) / 2.,
        );
        if button_rect.width() - 6. >= 0.0001 && button_rect.height() - 6. >= 0.0001 {
            output.extend_from_slice(b"q\n0 g\n");
            for (point, op) in [
                (
                    CfxPointF::new(center.x - 3., center.y + 1.5),
                    b" m\n".as_slice(),
                ),
                (CfxPointF::new(center.x + 3., center.y + 1.5), b" l\n"),
                (CfxPointF::new(center.x, center.y - 1.5), b" l\n"),
                (CfxPointF::new(center.x - 3., center.y + 1.5), b" l f\n"),
            ] {
                write_point(&mut ByteStream(&mut output), point).unwrap();
                output.extend_from_slice(op);
            }
            output.extend_from_slice(button.as_bytes());
            output.extend_from_slice(b"Q\n");
        }
    }
    output
}

fn generate_list_box_ap(
    annot: &CpdfDictionary,
    body: CfxFloatRect,
    color: CfxColor,
    size: f32,
    map: &CpvtFontMap,
    provider: &FontMapProvider<'_>,
) -> Vec<u8> {
    let Some(opts) = get_field_attr_for_dict(Some(annot), b"Opt") else {
        return Vec::new();
    };
    let selections = get_field_attr_for_dict(Some(annot), b"I");
    let len = match opts.borrow().as_any().downcast_ref::<CpdfArray>() {
        Some(a) => a.size(),
        None => return Vec::new(),
    };
    let mut output = Vec::new();
    let mut y = body.top;
    for index in field_integer(annot, b"TI") as usize..len {
        if body.bottom - y >= 0.0001 {
            break;
        }
        let opt = opts
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .get_direct_object_at(index);
        let Some(opt) = opt else {
            continue;
        };
        let item = {
            let borrowed = opt.borrow();
            if borrowed.object_type() == ObjectType::String {
                borrowed.get_unicode_text()
            } else if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                array.get_unicode_text_at(1)
            } else {
                Vec::new()
            }
        };
        let selected = selections.as_ref().is_some_and(|selections| {
            let borrowed = selections.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfArray>()
                .is_some_and(|array| {
                    (0..array.size()).any(|i| {
                        let selected = array.get_integer_at(i);
                        selected >= 0 && index == selected as usize
                    })
                })
        });
        let mut vt = CpvtVariableText::new(Some(provider));
        vt.set_plate_rect(CfxFloatRect::new(body.left, 0., body.right, 0.));
        vt.set_font_size(if size > -0.0001 && size < 0.0001 {
            12.
        } else {
            size
        });
        vt.initialize();
        vt.set_text(&item);
        vt.rearrange_all();
        let height = vt.content_rect().height();
        if selected {
            output.extend_from_slice(b"q\n");
            output.extend_from_slice(
                generate_color_ap(CfxColor::make_rgb_bytes(0, 51, 113), PaintOperation::Fill)
                    .as_bytes(),
            );
            write_rect(
                &mut ByteStream(&mut output),
                CfxFloatRect::new(body.left, y - height, body.right, y),
            )
            .unwrap();
            output.extend_from_slice(b" re f\nQ\n");
        }
        output.extend_from_slice(b"BT\n");
        output.extend_from_slice(
            generate_color_ap(
                if selected {
                    CfxColor::make_gray(1.)
                } else {
                    color
                },
                PaintOperation::Fill,
            )
            .as_bytes(),
        );
        output.extend_from_slice(&generate_edit_ap(
            map,
            &mut vt.iterator(),
            CfxPointF::new(0., y),
            true,
            0,
        ));
        output.extend_from_slice(b"ET\n");
        y -= height;
    }
    output
}

/// `CPDF_GenerateAP::GenerateFormAP()` with borrowed document resources.
/// Returns false if required AcroForm/default-font resources cannot be used.
pub fn generate_form_ap(
    objects: &HolderRef,
    root: Option<&ObjectRef>,
    data: &Rc<CpdfDocPageData>,
    mapper: &mut CfxFontMapper,
    annot: &mut CpdfDictionary,
    kind: FormType,
) -> bool {
    let Some((font_info, color, dr, font_dict)) = select_form_font(objects, root, annot) else {
        return false;
    };
    let Some(default_font) = data.get_font(&font_dict, mapper) else {
        return false;
    };
    write_form_ap(
        objects,
        annot,
        kind,
        font_info,
        color,
        dr,
        font_dict,
        default_font,
    )
}

fn select_form_font(
    objects: &HolderRef,
    root: Option<&ObjectRef>,
    annot: &CpdfDictionary,
) -> Option<(FontNameAndSize, CfxColor, ObjectRef, ObjectRef)> {
    let Some(form) = root.and_then(|root| dict_entry(root, b"AcroForm")) else {
        return None;
    };
    let (font_info, color, dr, fonts) = {
        let borrowed = form.borrow();
        let form = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        let appearance = CpdfDefaultAppearance::from_dicts(Some(annot), Some(form));
        let Some(font) = appearance.font() else {
            return None;
        };
        let color = appearance.color().unwrap_or_default();
        let Some(dr) = form.get_dict_for(b"DR") else {
            return None;
        };
        let fonts = dict_entry(&dr, b"Font");
        (font, color, dr, fonts)
    };
    let Some(fonts) = fonts else {
        return None;
    };
    {
        let borrowed = fonts.borrow();
        if !validate_font_resource_dict(borrowed.as_any().downcast_ref::<CpdfDictionary>()) {
            return None;
        }
    }
    let font_dict = get_font_or_fallback(objects, &fonts, &font_info.name);
    Some((font_info, color, dr, font_dict))
}

fn write_form_ap(
    objects: &HolderRef,
    annot: &mut CpdfDictionary,
    kind: FormType,
    font_info: FontNameAndSize,
    color: CfxColor,
    dr: ObjectRef,
    font_dict: ObjectRef,
    default_font: CpdfFont,
) -> bool {
    let (bbox, matrix, border_color, background_color) = annotation_dimensions_and_color(annot);
    let mut output = Vec::new();
    let background = generate_color_ap(background_color, PaintOperation::Fill);
    if !background.is_empty() {
        output.extend_from_slice(b"q\n");
        output.extend_from_slice(background.as_bytes());
        write_rect(&mut ByteStream(&mut output), bbox).unwrap();
        output.extend_from_slice(b" re f\nQ\n");
    }
    let style = annot.get_dict_for(b"BS");
    let style = style
        .as_ref()
        .map_or_else(BorderStyleInfo::default, |style| {
            border_style_info(style.borrow().as_any().downcast_ref::<CpdfDictionary>())
        });
    let border = generate_border_ap(bbox, &style, border_color);
    if !border.is_empty() {
        output.extend_from_slice(b"q\n");
        output.extend_from_slice(border.as_bytes());
        output.extend_from_slice(b"Q\n");
    }
    let mut body_rect = bbox;
    body_rect.deflate_xy(style.width, style.width);
    let ap = annot.get_or_create_dict_for(b"AP");
    let existing = ap
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .get_stream_for(b"N");
    let (normal, resources) = if let Some(normal) = existing {
        let dict = stream_dictionary(&normal);
        if !clone_resources_if_missing(&dict, &dr)
            && !validate_or_create_font_resources(objects, &dict, &font_dict, &font_info.name)
        {
            return false;
        }
        let resources = dict_entry(&dict, b"Resources");
        (normal, resources)
    } else {
        let normal = objects.borrow_mut().new_indirect_stream_from_bytes(&[]);
        let number = normal.borrow().obj_num();
        ap.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"N", CpdfReference::new(objects, number));
        (normal, None)
    };
    // The selected portable FontMap has no native Windows font to add. Its
    // resource/default-font contract otherwise matches the source adapter.
    let _resources = resources;
    let map = CpvtFontMap::new(Some(default_font), font_info.name.clone());
    let provider = FontMapProvider { font_map: &map };
    match kind {
        FormType::TextField => {
            let mut vt = CpvtVariableText::new(Some(&provider));
            let body = generate_text_field_ap(annot, body_rect, font_info.size, &map, &mut vt);
            if !body.is_empty() {
                output.extend_from_slice(b"/Tx BMC\nq\n");
                let content = vt.content_rect();
                if content.width() > body_rect.width() || content.height() > body_rect.height() {
                    write_rect(&mut ByteStream(&mut output), body_rect).unwrap();
                    output.extend_from_slice(b" re\nW\nn\n");
                }
                output.extend_from_slice(b"BT\n");
                output.extend_from_slice(generate_color_ap(color, PaintOperation::Fill).as_bytes());
                output.extend_from_slice(&body);
                output.extend_from_slice(b"ET\nQ\nEMC\n");
            }
        }
        FormType::ComboBox => output.extend_from_slice(&generate_combo_box_ap(
            annot,
            body_rect,
            color,
            font_info.size,
            &map,
            &provider,
        )),
        FormType::ListBox => {
            let body =
                generate_list_box_ap(annot, body_rect, color, font_info.size, &map, &provider);
            if !body.is_empty() {
                output.extend_from_slice(b"/Tx BMC\nq\n");
                write_rect(&mut ByteStream(&mut output), body_rect).unwrap();
                output.extend_from_slice(b" re\nW\nn\n");
                output.extend_from_slice(&body);
                output.extend_from_slice(b"Q\nEMC\n");
            }
        }
    }
    normal
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfStream>()
        .unwrap()
        .set_data_and_remove_filter(&output);
    let dict = stream_dictionary(&normal);
    {
        let mut borrowed = dict.borrow_mut();
        let dict = borrowed
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap();
        dict.set_matrix_for(b"Matrix", matrix);
        dict.set_rect_for(b"BBox", bbox);
    }
    clone_resources_if_missing(&dict, &dr)
        || validate_or_create_font_resources(objects, &dict, &font_dict, &font_info.name)
}

pub fn generate_empty_ap(objects: &HolderRef, annot: &mut CpdfDictionary) -> bool {
    let resource = resources(objects, ext_gstate(annot, b"Normal"));
    set_ap(objects, annot, Vec::new(), resource, false);
    true
}

pub fn generate_default_appearance_with_color(
    objects: &HolderRef,
    root: Option<&ObjectRef>,
    data: &Rc<CpdfDocPageData>,
    mapper: &mut CfxFontMapper,
    annot: &mut CpdfDictionary,
    color: CfxColor,
) -> bool {
    let Some(root) = root else {
        return false;
    };
    let form = dict_entry(root, b"AcroForm")
        .unwrap_or_else(|| init_acro_form_dict(objects, root, data, mapper));
    write_default_appearance_with_color(&form, annot, color)
}

fn write_default_appearance_with_color(
    form: &ObjectRef,
    annot: &mut CpdfDictionary,
    color: CfxColor,
) -> bool {
    let font = {
        let borrowed = form.borrow();
        CpdfDefaultAppearance::from_dicts(
            Some(annot),
            borrowed.as_any().downcast_ref::<CpdfDictionary>(),
        )
        .font()
    };
    let Some(font) = font else {
        return false;
    };
    if font.name.is_empty() || font.size <= 0. {
        return false;
    }
    let color = generate_color_ap(color, PaintOperation::Fill);
    if color.is_empty() {
        return false;
    }
    let mut output = vec![b'/'];
    output.extend_from_slice(&pdf_name_encode(&font.name));
    output.push(b' ');
    write_float(&mut ByteStream(&mut output), font.size).unwrap();
    output.extend_from_slice(b" Tf\n");
    output.extend_from_slice(color.as_bytes());
    annot.set_new_for(b"DA", CpdfString::from_bytes(None, &output));
    true
}

pub fn generate_popup_ap(
    objects: &HolderRef,
    data: &Rc<CpdfDocPageData>,
    mapper: &mut CfxFontMapper,
    annot: &mut CpdfDictionary,
) -> bool {
    let font_dict = fallback_font_dict(objects);
    let Some(font) = data.get_font(&font_dict, mapper) else {
        return false;
    };
    write_popup_ap(objects, annot, font_dict, font)
}

fn write_popup_ap(
    objects: &HolderRef,
    annot: &mut CpdfDictionary,
    font_dict: ObjectRef,
    font: CpdfFont,
) -> bool {
    let mut output = Vec::from(b"/GS gs\n".as_slice());
    output.extend_from_slice(
        generate_color_ap(CfxColor::make_rgb(1., 1., 0.), PaintOperation::Fill).as_bytes(),
    );
    output.extend_from_slice(
        generate_color_ap(CfxColor::make_rgb(0., 0., 0.), PaintOperation::Stroke).as_bytes(),
    );
    output.extend_from_slice(b"1 w\n");
    let mut rect = annot.get_rect_for(b"Rect");
    rect.normalize();
    rect.deflate_xy(0.5, 0.5);
    write_rect(&mut ByteStream(&mut output), rect).unwrap();
    output.extend_from_slice(b" re b\n");
    let map = CpvtFontMap::new(Some(font), b"FONT".to_vec());
    let provider = FontMapProvider { font_map: &map };
    let mut vt = CpvtVariableText::new(Some(&provider));
    vt.set_plate_rect(annot.get_rect_for(b"Rect"));
    vt.set_font_size(12.);
    vt.set_auto_return(true);
    vt.set_multi_line(true);
    vt.initialize();
    let mut text = annot.get_unicode_text_for(b"T");
    text.push(u32::from(b'\n'));
    text.extend(annot.get_unicode_text_for(b"Contents"));
    vt.set_text(&text);
    vt.rearrange_all();
    let body = generate_edit_ap(&map, &mut vt.iterator(), CfxPointF::new(3., -3.), false, 0);
    if !body.is_empty() {
        // The source's popup text ended with Q without a matching q. Balance
        // the text graphics state so generated PDFs remain well formed.
        output.extend_from_slice(b"q\nBT\n0 0 0 rg\n");
        output.extend_from_slice(&body);
        output.extend_from_slice(b"ET\nQ\n");
    }
    let resource = resources(objects, ext_gstate(annot, b"Normal"));
    let mut fonts = CpdfDictionary::new();
    fonts.set_new_for(
        b"FONT",
        CpdfReference::new(objects, font_dict.borrow().obj_num()),
    );
    resource
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"Font", fonts);
    set_ap(objects, annot, output, resource, false);
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn appearance_font_aliases_are_serialized_as_pdf_names() {
        use super::*;
        for (alias, expected) in [
            (b"Left Font".as_slice(), b"/Left#20Font 18 Tf\n".as_slice()),
            (b"Font#Two", b"/Font#23Two 18 Tf\n"),
            (b"A/B(C)", b"/A#2FB#28C#29 18 Tf\n"),
            (b"Literal#20", b"/Literal#2320 18 Tf\n"),
            (b"Plain", b"/Plain 18 Tf\n"),
        ] {
            let map = CpvtFontMap::new(None, alias.to_vec());
            let mut output = Vec::new();
            append_font_set_string(&mut output, &map, 0, 18.0);
            assert_eq!(output, expected);
        }
    }

    #[test]
    fn recoloring_default_appearance_roundtrips_escaped_font_identity() {
        use super::*;
        for (encoded, decoded) in [
            (b"Left#20Font".as_slice(), b"Left Font".as_slice()),
            (b"Font#23Two", b"Font#Two"),
            (b"Literal#2320", b"Literal#20"),
        ] {
            let mut original_da = vec![b'/'];
            original_da.extend_from_slice(encoded);
            original_da.extend_from_slice(b" 18 Tf 0 g");
            let mut form = CpdfDictionary::new();
            form.set_new_for(b"DA", CpdfString::from_bytes(None, &original_da));
            let form = retain_object(form);
            let mut annot = CpdfDictionary::new();
            annot.set_new_for(b"Contents", CpdfString::from_bytes(None, b"A \xe9"));
            for gray in [0.25, 0.5] {
                assert!(write_default_appearance_with_color(
                    &form,
                    &mut annot,
                    CfxColor::make_gray(gray),
                ));
                let da = annot.get_byte_string_for(b"DA", b"");
                let mut expected_prefix = vec![b'/'];
                expected_prefix.extend_from_slice(encoded);
                expected_prefix.extend_from_slice(b" 18 Tf\n");
                assert!(da.starts_with(&expected_prefix));
                let appearance = CpdfDefaultAppearance::new(da);
                let font = appearance.font().unwrap();
                assert_eq!(font.name, decoded);
                assert_eq!(font.size, 18.0);
                let color = appearance.color().unwrap();
                assert_eq!(color.color_type, ColorType::Gray);
                assert_eq!(color.color1, gray);
                assert_eq!(annot.get_byte_string_for(b"Contents", b""), b"A \xe9");
            }
        }
    }

    use super::*;
    use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
    use crate::fpdfapi::page::cpdf_form::CpdfForm;
    use crate::fpdfapi::page::cpdf_pageobject::PageObject;
    use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfdoc::cpdf_annot::{get_annot_ap, AppearanceMode};
    use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF};
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;

    #[test]
    fn free_text_editor_layout_wraps_and_keeps_native_single_line_default() {
        use crate::fpdfapi::page::cpdf_pageobject::PageObjectType;
        use crate::fpdfapi::parser::cpdf_string::CpdfString;
        let rows = |multiline, text: &[u8], width| {
            let objects = CpdfIndirectObjectHolder::new();
            let root = retain_object(CpdfDictionary::new());
            let data = Rc::new(CpdfDocPageData::new());
            let mut annot = CpdfDictionary::new();
            annot.set_rect_for(b"Rect", CfxFloatRect::new(0., 0., width, 100.));
            annot.set_new_for(b"DA", CpdfString::from_bytes(None, b"/Helv 12 Tf 0 g"));
            annot.set_new_for(b"Contents", CpdfString::from_bytes(None, text));
            let annot = retain_object(annot);
            assert!(generate_free_text_ap_for_object(
                &objects,
                Some(&root),
                &data,
                &annot,
                multiline
            ));
            let stream = {
                let a = annot.borrow();
                get_annot_ap(
                    a.as_any().downcast_ref::<CpdfDictionary>().unwrap(),
                    AppearanceMode::Normal,
                    true,
                )
                .unwrap()
            };
            let mut form = CpdfForm::new(stream, None, None).unwrap();
            form.parse_content(None, None, data, Rc::new(RefCell::new(HashSet::new())), 0);
            (0..form.holder().page_object_count())
                .filter_map(|i| {
                    let o = form.holder().page_object_by_index(i)?;
                    (o.object_type() == PageObjectType::Text).then(|| o.base().rect())
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(false, b"First\nSecond", 100.).len(), 1);
        let explicit = rows(true, b"First\r\nSecond", 100.);
        assert_eq!(explicit.len(), 2);
        assert!(explicit[0].top > 80. && explicit[0].bottom > explicit[1].top);
        assert!(rows(true, b"alpha beta gamma delta epsilon", 55.).len() >= 3);
    }

    #[test]
    fn free_text_keeps_pdf_font_name_and_encoded_character_bytes() {
        use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
        use crate::fpdfapi::parser::cpdf_string::CpdfString;
        let objects = CpdfIndirectObjectHolder::new();
        let root = retain_object(CpdfDictionary::new());
        let data = Rc::new(CpdfDocPageData::new());
        let mut mapper = CfxFontMapper::new();
        let mut annot = CpdfDictionary::new();
        annot.set_rect_for(b"Rect", CfxFloatRect::new(0.0, 0.0, 100.0, 20.0));
        annot.set_new_for(b"DA", CpdfString::from_bytes(None, b"/F#FF 12 Tf 0 g"));
        annot.set_new_for(b"Contents", CpdfString::from_bytes(None, b"A \xe9"));
        assert!(generate_free_text_ap(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut annot
        ));
        let stream = get_annot_ap(&annot, AppearanceMode::Normal, true).unwrap();
        {
            let borrowed = stream.borrow();
            let mut acc =
                CpdfStreamAcc::new(borrowed.as_any().downcast_ref::<CpdfStream>().unwrap());
            acc.load_all_data_filtered();
            let bytes = acc.get_span();
            assert!(bytes
                .windows(b"/F#FF 12 Tf\n".len())
                .any(|part| part == b"/F#FF 12 Tf\n"));
            assert!(bytes
                .windows(b"(A \xe9) Tj\n".len())
                .any(|part| part == b"(A \xe9) Tj\n"));
        }
        let mut form = CpdfForm::new(stream, None, None).unwrap();
        form.parse_content(None, None, data, Rc::new(RefCell::new(HashSet::new())), 0);
        assert_eq!(form.holder().page_object_count(), 2);
        // An invalid resource is rejected before a fallback font or AP is made.
        let form = root
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_dict_for(b"AcroForm")
            .unwrap();
        let dr = form
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_dict_for(b"DR")
            .unwrap();
        let fonts = dr
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_dict_for(b"Font")
            .unwrap();
        fonts
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Invalid", CpdfScalarObject::number_from_signed(1));
        let mut invalid = CpdfDictionary::new();
        assert!(!generate_free_text_ap(
            &objects,
            Some(&root),
            &Rc::new(CpdfDocPageData::new()),
            &mut mapper,
            &mut invalid
        ));
        assert!(!invalid.key_exist(b"AP"));
    }

    #[test]
    fn generated_shape_streams_parse_with_native_geometry_and_resources() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut annot = CpdfDictionary::new();
        annot.set_rect_for(b"Rect", CfxFloatRect::new(0.0, 0.0, 10.0, 20.0));
        annot.set_new_for(b"CA", CpdfScalarObject::number_from_float(0.5));
        for (generate, points) in [
            (
                generate_square_ap as fn(&HolderRef, &mut CpdfDictionary) -> bool,
                5,
            ),
            (generate_circle_ap, 13),
        ] {
            assert!(generate(&objects, &mut annot));
            let stream = get_annot_ap(&annot, AppearanceMode::Normal, true).unwrap();
            assert_ne!(stream.borrow().obj_num(), 0);
            let mut form = CpdfForm::new(stream, None, None).unwrap();
            form.parse_content(
                None,
                None,
                Rc::new(CpdfDocPageData::new()),
                Rc::new(RefCell::new(HashSet::new())),
                0,
            );
            let path = form
                .holder()
                .page_object_by_index(0)
                .unwrap()
                .as_any()
                .downcast_ref::<CpdfPathObject>()
                .unwrap();
            assert_eq!(path.path().points().len(), points);
            assert!(path.stroke());
            assert_eq!(path.base().general_state().get_stroke_alpha(), 0.5);
            if points == 5 {
                assert_eq!(path.path().get_point(0), CfxPointF::new(0.5, 0.5));
            }
        }
    }

    #[test]
    fn highlight_uses_quad_bbox_and_ink_keeps_initial_segment_and_inflated_rect() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut annot = CpdfDictionary::new();
        annot.set_rect_for(b"Rect", CfxFloatRect::new(100.0, 100.0, 200.0, 200.0));
        let mut quads = CpdfArray::new();
        for value in [0.0, 10.0, 20.0, 10.0, 0.0, 0.0, 20.0, 0.0] {
            quads.append_new(CpdfScalarObject::number_from_float(value));
        }
        annot.set_new_for(b"QuadPoints", quads);
        assert!(generate_highlight_ap(&objects, &mut annot));
        let stream = get_annot_ap(&annot, AppearanceMode::Normal, true).unwrap();
        let dict = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_dict();
        assert_eq!(
            dict.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_rect_for(b"BBox"),
            CfxFloatRect::new(0.0, 0.0, 20.0, 10.0)
        );
        let mut form = CpdfForm::new(stream, None, None).unwrap();
        form.parse_content(
            None,
            None,
            Rc::new(CpdfDocPageData::new()),
            Rc::new(RefCell::new(HashSet::new())),
            0,
        );
        assert_eq!(
            form.holder()
                .page_object_by_index(0)
                .unwrap()
                .base()
                .general_state()
                .get_blend_type(),
            crate::fxge::dib::fx_dib::BlendMode::Multiply
        );

        annot.set_rect_for(b"Rect", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
        let mut border = CpdfDictionary::new();
        border.set_new_for(b"W", CpdfScalarObject::number_from_signed(2));
        annot.set_new_for(b"BS", border);
        let mut ink = CpdfArray::new();
        let mut coordinates = CpdfArray::new();
        for value in [1, 2, 3, 4] {
            coordinates.append_new(CpdfScalarObject::number_from_signed(value));
        }
        ink.append_new(coordinates);
        annot.set_new_for(b"InkList", ink);
        assert!(generate_ink_ap(&objects, &mut annot));
        assert_eq!(
            annot.get_rect_for(b"Rect"),
            CfxFloatRect::new(-1.0, -1.0, 11.0, 11.0)
        );
        let stream = get_annot_ap(&annot, AppearanceMode::Normal, true).unwrap();
        let mut form = CpdfForm::new(stream, None, None).unwrap();
        form.parse_content(
            None,
            None,
            Rc::new(CpdfDocPageData::new()),
            Rc::new(RefCell::new(HashSet::new())),
            0,
        );
        let path = form
            .holder()
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfPathObject>()
            .unwrap();
        assert_eq!(path.path().get_point(0), path.path().get_point(1));
        assert_eq!(path.path().points().len(), 3);
    }
}

#[cfg(test)]
mod widget_tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;

    fn setup() -> (HolderRef, ObjectRef, Rc<CpdfDocPageData>, CfxFontMapper) {
        let objects = CpdfIndirectObjectHolder::new();
        let root = retain_object(CpdfDictionary::new());
        let data = Rc::new(CpdfDocPageData::new());
        data.bind_document_resources(&objects, Some(&root));
        let mut mapper = CfxFontMapper::new();
        init_acro_form_dict(&objects, &root, &data, &mut mapper);
        (objects, root, data, mapper)
    }
    fn widget(value: &[u8]) -> CpdfDictionary {
        let mut annot = CpdfDictionary::new();
        annot.set_new_name_for(b"Subtype", b"Widget");
        annot.set_rect_for(b"Rect", CfxFloatRect::new(10., 20., 110., 60.));
        annot.set_new_for(b"V", CpdfString::from_bytes(None, value));
        annot.set_new_for(b"DA", CpdfString::from_bytes(None, b"/TestFont 12 Tf 0 g"));
        annot
    }
    fn appearance(annot: &CpdfDictionary) -> ObjectRef {
        annot
            .get_dict_for(b"AP")
            .unwrap()
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_stream_for(b"N")
            .unwrap()
    }
    fn bytes(stream: &ObjectRef) -> Vec<u8> {
        stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_in_memory_raw_data()
            .to_vec()
    }
    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|part| part == needle)
    }
    fn number(annot: &mut CpdfDictionary, name: &[u8], value: i32) {
        annot.set_new_for(name, CpdfScalarObject::number_from_signed(value));
    }

    #[test]
    fn widget_rotations_resources_and_regeneration_retain_indirect_appearance() {
        let (objects, root, data, mut mapper) = setup();
        for (rotation, bbox, matrix) in [
            (
                0,
                CfxFloatRect::new(0., 0., 100., 40.),
                CfxMatrix::default(),
            ),
            (
                90,
                CfxFloatRect::new(0., 0., 40., 100.),
                CfxMatrix::new(0., 1., -1., 0., 100., 0.),
            ),
            (
                180,
                CfxFloatRect::new(0., 0., 100., 40.),
                CfxMatrix::new(-1., 0., 0., -1., 100., 40.),
            ),
            (
                270,
                CfxFloatRect::new(0., 0., 40., 100.),
                CfxMatrix::new(0., -1., 1., 0., 0., 40.),
            ),
            (
                -90,
                CfxFloatRect::new(0., 0., 40., 100.),
                CfxMatrix::new(0., -1., 1., 0., 0., 40.),
            ),
        ] {
            let mut annot = widget(b"Before");
            let mut mk = CpdfDictionary::new();
            number(&mut mk, b"R", rotation);
            annot.set_new_for(b"MK", mk);
            assert!(generate_form_ap(
                &objects,
                Some(&root),
                &data,
                &mut mapper,
                &mut annot,
                FormType::TextField
            ));
            let stream = appearance(&annot);
            assert!(stream.borrow().obj_num() != 0);
            let dict = stream_dictionary(&stream);
            {
                let borrowed = dict.borrow();
                let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
                assert_eq!(dict.get_rect_for(b"BBox"), bbox);
                assert_eq!(dict.get_matrix_for(b"Matrix"), matrix);
            }
            let resources = dict_entry(&dict, b"Resources").unwrap();
            let form = dict_entry(&root, b"AcroForm").unwrap();
            let dr = dict_entry(&form, b"DR").unwrap();
            assert!(!Rc::ptr_eq(&resources, &dr));
            let fonts = dict_entry(&resources, b"Font").unwrap();
            assert!(dict_entry(&fonts, b"TestFont").is_some());
            annot.set_new_for(b"V", CpdfString::from_bytes(None, b"After"));
            assert!(generate_form_ap(
                &objects,
                Some(&root),
                &data,
                &mut mapper,
                &mut annot,
                FormType::TextField
            ));
            assert!(Rc::ptr_eq(&stream, &appearance(&annot)));
            assert!(contains(&bytes(&stream), b"(After) Tj"));
            assert!(!contains(&bytes(&stream), b"Before"));
        }
    }

    #[test]
    fn password_comb_and_multiline_appearances_preserve_field_flags_and_limits() {
        let (objects, root, data, mut mapper) = setup();
        let mut password = widget(b"Secret");
        number(&mut password, b"Ff", 1 << 13);
        number(&mut password, b"MaxLen", 3);
        assert!(generate_form_ap(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut password,
            FormType::TextField
        ));
        let encoded = bytes(&appearance(&password));
        assert!(contains(&encoded, b"(***) Tj"));
        assert!(!contains(&encoded, b"Secret"));
        let mut comb = widget(b"ABCD");
        number(&mut comb, b"Ff", 1 << 24);
        number(&mut comb, b"MaxLen", 3);
        assert!(generate_form_ap(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut comb,
            FormType::TextField
        ));
        let encoded = bytes(&appearance(&comb));
        for letter in [b"(A) Tj", b"(B) Tj", b"(C) Tj"] {
            assert!(contains(&encoded, letter));
        }
        assert!(!contains(&encoded, b"(D) Tj"));
        let mut multiline = widget(b"first\nsecond");
        number(&mut multiline, b"Ff", 1 << 12);
        assert!(generate_form_ap(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut multiline,
            FormType::TextField
        ));
        let encoded = bytes(&appearance(&multiline));
        assert!(contains(&encoded, b"(first) Tj"));
        assert!(contains(&encoded, b"(second) Tj"));
    }

    #[test]
    fn combo_arrow_and_scrolled_selected_list_use_option_labels_and_white_text() {
        let (objects, root, data, mut mapper) = setup();
        let mut combo = widget(b"Choice");
        assert!(generate_form_ap(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut combo,
            FormType::ComboBox
        ));
        let encoded = bytes(&appearance(&combo));
        assert!(contains(&encoded, b"(Choice) Tj"));
        assert!(contains(&encoded, b"q\n0 g\n"));
        assert!(contains(&encoded, b" l f\n"));
        let mut list = widget(b"");
        let mut options = CpdfArray::new();
        options.append_new(CpdfString::from_bytes(None, b"Hidden"));
        let mut second = CpdfArray::new();
        second.append_new(CpdfString::from_bytes(None, b"export-value"));
        second.append_new(CpdfString::from_bytes(None, b"Visible"));
        options.append_new(second);
        list.set_new_for(b"Opt", options);
        let mut selected = CpdfArray::new();
        selected.append_new(CpdfScalarObject::number_from_signed(1));
        list.set_new_for(b"I", selected);
        number(&mut list, b"TI", 1);
        assert!(generate_form_ap(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut list,
            FormType::ListBox
        ));
        let encoded = bytes(&appearance(&list));
        assert!(contains(&encoded, b"(Visible) Tj"));
        assert!(!contains(&encoded, b"Hidden"));
        assert!(!contains(&encoded, b"export-value"));
        assert!(contains(&encoded, b"1 g\n"));
        assert!(contains(
            &encoded,
            generate_color_ap(CfxColor::make_rgb_bytes(0, 51, 113), PaintOperation::Fill)
                .as_bytes()
        ));
    }

    #[test]
    fn invalid_existing_font_resources_preserve_old_stream_and_empty_ap_is_explicit() {
        let (objects, root, data, mut mapper) = setup();
        let mut annot = widget(b"Before");
        assert!(generate_form_ap(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut annot,
            FormType::TextField
        ));
        let stream = appearance(&annot);
        let previous = bytes(&stream);
        let resources = dict_entry(&stream_dictionary(&stream), b"Resources").unwrap();
        let fonts = dict_entry(&resources, b"Font").unwrap();
        fonts
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Invalid", CpdfScalarObject::number_from_signed(17));
        assert!(!generate_form_ap(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut annot,
            FormType::TextField
        ));
        assert_eq!(bytes(&stream), previous);
        assert!(generate_empty_ap(&objects, &mut annot));
        assert!(bytes(&appearance(&annot)).is_empty());
        assert!(!Rc::ptr_eq(&appearance(&annot), &stream));
        assert!(generate_default_appearance_with_color(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut annot,
            CfxColor::make_rgb(1., 0., 0.)
        ));
        let da = CpdfDefaultAppearance::from_dicts(Some(&annot), None);
        assert_eq!(da.font().unwrap().name, b"TestFont");
        assert_eq!(da.font().unwrap().size, 12.);
        assert_eq!(da.color().unwrap().color1, 1.);
        assert!(!generate_default_appearance_with_color(
            &objects,
            Some(&root),
            &data,
            &mut mapper,
            &mut annot,
            CfxColor::make_transparent()
        ));
    }

    #[test]
    fn popup_contents_generate_multiline_text_and_balanced_graphics_state() {
        let (objects, _, data, mut mapper) = setup();
        let mut annot = CpdfDictionary::new();
        annot.set_rect_for(b"Rect", CfxFloatRect::new(0., 0., 200., 100.));
        annot.set_new_for(b"T", CpdfString::from_bytes(None, b"Title"));
        annot.set_new_for(b"Contents", CpdfString::from_bytes(None, b"Contents"));
        assert!(generate_popup_ap(&objects, &data, &mut mapper, &mut annot));
        let stream = appearance(&annot);
        let encoded = bytes(&stream);
        assert!(contains(&encoded, b"q\nBT\n"));
        assert!(encoded.ends_with(b"ET\nQ\n"));
        // Popup contents use non-continuous formatting: each glyph can have
        // its own Tj operator. Check parsed text and line placement rather
        // than requiring an incidental grouping of literal strings.
        let mut form = crate::fpdfapi::page::cpdf_form::CpdfForm::new(stream, None, None).unwrap();
        form.parse_content(
            None,
            None,
            data,
            Rc::new(std::cell::RefCell::new(std::collections::HashSet::new())),
            0,
        );
        let text: Vec<_> = form
            .holder()
            .iter()
            .filter_map(|object| {
                object
                    .as_any()
                    .downcast_ref::<crate::fpdfapi::page::cpdf_textobject::CpdfTextObject>()
            })
            .collect();
        let glyphs: Vec<_> = text
            .iter()
            .flat_map(|text| text.char_codes().iter().copied())
            .collect();
        assert_eq!(
            glyphs,
            b"TitleContents"
                .iter()
                .map(|&c| u32::from(c))
                .collect::<Vec<_>>()
        );
        assert!(text.last().unwrap().position().y < text[0].position().y);
    }
}
