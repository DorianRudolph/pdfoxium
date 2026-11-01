// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `core/fpdftext/cpdf_textpage.{h,cpp}`. Construction borrows the parsed page;
//! results retain only font handles, object identities and extraction metadata.
//! No page/text object clone or self-referential ownership is required.
use super::unicodenormalizationdata::get_unicode_normalization;
use crate::{
    fpdfapi::{
        font::cpdf_font::{CpdfFont, INVALID_CHAR_CODE},
        page::{
            cpdf_contentmarkitem::CpdfContentMarkItem,
            cpdf_formobject::CpdfFormObject,
            cpdf_page::CpdfPage,
            cpdf_pageobject::{PageObject, PageObjectType},
            cpdf_pageobjectholder::{CpdfPageObjectHolder, PageObjectId},
            cpdf_textobject::CpdfTextObject,
        },
        parser::cpdf_object::ObjectRef,
    },
    fxcrt::{
        fx_bidi::{CfxBidiString, Direction},
        fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF, CfxSizeF},
        fx_unicode::get_mirror_char,
    },
};
use std::{collections::HashMap, rc::Rc};
const SIZE_EPSILON: f32 = 0.01;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CharType {
    Normal,
    Generated,
    NotUnicode,
    Hyphen,
    Piece,
    ActualText,
}
#[derive(Clone, Debug)]
pub struct CharInfo {
    char_type: CharType,
    char_code: u32,
    unicode: u32,
    origin: CfxPointF,
    char_box: CfxFloatRect,
    loose_char_box: CfxFloatRect,
    matrix: CfxMatrix,
    text_object: Option<PageObjectId>,
}
impl CharInfo {
    pub fn char_type(&self) -> CharType {
        self.char_type
    }
    pub fn char_code(&self) -> u32 {
        self.char_code
    }
    pub fn unicode(&self) -> u32 {
        self.unicode
    }
    pub fn origin(&self) -> CfxPointF {
        self.origin
    }
    pub fn char_box(&self) -> CfxFloatRect {
        self.char_box
    }
    pub fn loose_char_box(&self) -> CfxFloatRect {
        self.loose_char_box
    }
    pub fn matrix(&self) -> CfxMatrix {
        self.matrix
    }
    pub fn text_object(&self) -> Option<PageObjectId> {
        self.text_object
    }
    pub fn set_unicode(&mut self, unicode: u32) {
        self.unicode = unicode;
    }
    pub fn set_char_type(&mut self, kind: CharType) {
        self.char_type = kind;
    }
}
/// Retained metadata for the native text-object identity. Only one font owner is
/// retained per object, independent of its number of extracted characters.
#[derive(Clone)]
pub struct TextObjectInfo {
    pub font: CpdfFont,
    pub font_size: f32,
    pub text_matrix: CfxMatrix,
    pub position: CfxPointF,
    pub fill_color: [u32; 4],
    pub stroke_color: [u32; 4],
}
#[derive(Clone, Copy)]
struct CharSegment {
    index: i32,
    count: i32,
}
#[derive(Default)]
pub struct CpdfTextPage {
    char_indices: Vec<CharSegment>,
    char_list: Vec<CharInfo>,
    text_buf: Vec<u32>,
    sel_rects: Vec<CfxFloatRect>,
    objects: HashMap<PageObjectId, TextObjectInfo>,
}
impl CpdfTextPage {
    pub fn new(page: &CpdfPage, rtl: bool) -> Self {
        Builder::new(page, rtl).build()
    }
    pub fn size(&self) -> usize {
        self.char_list.len()
    }
    pub fn count_chars(&self) -> i32 {
        self.char_list.len().min(i32::MAX as usize) as i32
    }
    pub fn get_char_info(&self, index: usize) -> &CharInfo {
        &self.char_list[index]
    }
    pub fn get_char_info_mut(&mut self, index: usize) -> &mut CharInfo {
        &mut self.char_list[index]
    }
    pub fn text_object_info(&self, id: PageObjectId) -> Option<&TextObjectInfo> {
        self.objects.get(&id)
    }
    pub fn get_char_font(&self, index: usize) -> Option<&CpdfFont> {
        self.char_list[index]
            .text_object
            .and_then(|id| self.objects.get(&id))
            .map(|info| &info.font)
    }
    pub fn get_char_font_size(&self, index: usize) -> f32 {
        self.char_list[index]
            .text_object
            .and_then(|id| self.objects.get(&id))
            .map_or(1.0, |info| info.font_size)
    }
    pub fn get_char_loose_bounds(&self, index: usize) -> CfxFloatRect {
        self.char_list[index].loose_char_box
    }
    pub fn char_index_from_text_index(&self, text_index: i32) -> i32 {
        if text_index < 0 {
            return -1;
        }
        let mut count = 0;
        for info in &self.char_indices {
            count += info.count;
            if count > text_index {
                return text_index - count + info.count + info.index;
            }
        }
        -1
    }
    pub fn text_index_from_char_index(&self, char_index: i32) -> i32 {
        if char_index < 0 {
            return -1;
        }
        let mut count = 0;
        for info in &self.char_indices {
            let text_index = char_index - info.index;
            if text_index < info.count {
                return if text_index >= 0 {
                    text_index + count
                } else {
                    -1
                };
            }
            count += info.count;
        }
        -1
    }
    pub fn get_rect_array(&self, start: i32, count: i32) -> Vec<CfxFloatRect> {
        let mut result = Vec::new();
        if start < 0 || count == 0 || start >= self.count_chars() {
            return result;
        }
        let end = if count < 0 {
            self.char_list.len()
        } else {
            (start as usize)
                .saturating_add(count as usize)
                .min(self.char_list.len())
        };
        let mut text_object = None;
        let mut rect = CfxFloatRect::default();
        let mut new_rect = true;
        for info in &self.char_list[start as usize..end] {
            if info.char_type == CharType::Generated
                || info.char_box.width() < SIZE_EPSILON
                || info.char_box.height() < SIZE_EPSILON
            {
                continue;
            }
            if text_object.is_none() {
                text_object = info.text_object;
            }
            if text_object != info.text_object {
                result.push(rect);
                text_object = info.text_object;
                new_rect = true;
            }
            if new_rect {
                new_rect = false;
                rect = info.char_box;
                rect.normalize();
            } else {
                rect.union(info.char_box);
            }
        }
        // Match the source's final empty rectangle when a nonempty range has
        // no characters with meaningful bounds.
        result.push(rect);
        result
    }
    pub fn get_index_at_pos(&self, point: CfxPointF, tolerance: CfxSizeF) -> i32 {
        let mut near = -1;
        let mut distance = 10000.0_f64;
        for (index, info) in self.char_list.iter().enumerate() {
            let mut rect = info.char_box;
            if rect.contains_point(point) {
                return index as i32;
            }
            if tolerance.width <= 0.0 && tolerance.height <= 0.0 {
                continue;
            }
            rect.normalize();
            let extended = CfxFloatRect::new(
                rect.left - tolerance.width / 2.0,
                rect.bottom - tolerance.height / 2.0,
                rect.right + tolerance.width / 2.0,
                rect.top + tolerance.height / 2.0,
            );
            if !extended.contains_point(point) {
                continue;
            }
            let dx = f64::from(
                (point.x - rect.left)
                    .abs()
                    .min((point.x - rect.right).abs()),
            );
            let dy = f64::from(
                (point.y - rect.bottom)
                    .abs()
                    .min((point.y - rect.top).abs()),
            );
            if dx + dy < distance {
                distance = dx + dy;
                near = index as i32;
            }
        }
        near
    }
    fn get_text_by_predicate(&self, predicate: impl Fn(&CharInfo) -> bool) -> Vec<u32> {
        let mut pos_y = 0.0;
        let mut contained_previous = false;
        let mut line_feed = false;
        let mut text = Vec::new();
        for info in &self.char_list {
            if predicate(info) {
                if (pos_y - info.origin.y).abs() > 0.0 && !contained_previous && line_feed {
                    pos_y = info.origin.y;
                    if !text.is_empty() {
                        text.extend([13, 10]);
                    }
                }
                contained_previous = true;
                line_feed = false;
                if info.unicode != 0 {
                    text.push(info.unicode);
                }
            } else if info.unicode == 32 {
                if contained_previous {
                    text.push(32);
                    contained_previous = false;
                    line_feed = false;
                }
            } else {
                contained_previous = false;
                line_feed = true;
            }
        }
        text
    }
    pub fn get_text_by_rect(&self, rect: CfxFloatRect) -> Vec<u32> {
        self.get_text_by_predicate(|info| {
            let mut intersection = rect;
            intersection.intersect(info.char_box);
            !intersection.is_empty()
        })
    }
    pub fn get_text_by_object_id(&self, id: PageObjectId) -> Vec<u32> {
        self.get_text_by_predicate(|info| info.text_object == Some(id))
    }
    pub fn get_page_text(&self, start: i32, count: i32) -> Vec<u32> {
        self.page_text_slice(start, count).to_vec()
    }
    fn page_text_slice(&self, mut start: i32, mut count: i32) -> &[u32] {
        if start < 0 || start >= self.count_chars() || count <= 0 || self.text_buf.is_empty() {
            return &[];
        }
        let mut text_start = self.text_index_from_char_index(start);
        while text_start < 0 {
            if start >= self.count_chars() {
                return &[];
            }
            start += 1;
            text_start = self.text_index_from_char_index(start);
        }
        count = count.min(self.count_chars() - start);
        let mut last = start + count - 1;
        let mut text_last = self.text_index_from_char_index(last);
        while text_last < 0 {
            if last < start {
                return &[];
            }
            last -= 1;
            text_last = self.text_index_from_char_index(last);
        }
        if text_last < text_start {
            return &[];
        }
        &self.text_buf[text_start as usize..=text_last as usize]
    }
    pub fn get_all_page_text(&self) -> Vec<u32> {
        self.all_page_text().to_vec()
    }
    pub(super) fn all_page_text(&self) -> &[u32] {
        self.page_text_slice(0, self.count_chars())
    }
    pub fn count_rects(&mut self, start: i32, count: i32) -> i32 {
        if start < 0 {
            return -1;
        }
        self.sel_rects = self.get_rect_array(start, count);
        self.sel_rects.len() as i32
    }
    pub fn get_rect(&self, index: i32) -> Option<CfxFloatRect> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.sel_rects.get(index).copied())
    }
}
fn is_control(info: &CharInfo) -> bool {
    matches!(
        info.unicode,
        2 | 3 | 0x93 | 0x94 | 0x96 | 0x97 | 0x98 | 0xfffe
    ) && info.char_type != CharType::Hyphen
}
pub(super) fn is_normal(info: &CharInfo) -> bool {
    if info.unicode != 0 {
        !is_control(info)
    } else {
        info.char_code != 0
    }
}
fn is_hyphen_code(code: u32) -> bool {
    code == 45 || code == 0xad
}
fn normalize_threshold(value: f32, a: i32, b: i32, c: i32) -> f32 {
    value
        / if value < (a as f32) {
            2.0
        } else if value < (b as f32) {
            4.0
        } else if value < (c as f32) {
            5.0
        } else {
            6.0
        }
}
fn font_same(first: &CpdfFont, second: &CpdfFont) -> bool {
    match (first, second) {
        (CpdfFont::Simple(a), CpdfFont::Simple(b)) => Rc::ptr_eq(a, b),
        (CpdfFont::Cid(a), CpdfFont::Cid(b)) => Rc::ptr_eq(a, b),
        (CpdfFont::Type3(a), CpdfFont::Type3(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn font_box(font: &CpdfFont) -> crate::fxcrt::fx_coordinates::FxRect {
    match font {
        CpdfFont::Simple(font) => font.font_base.font_bbox,
        CpdfFont::Cid(font) => font.font_base.font_bbox,
        CpdfFont::Type3(font) => font.font_base.borrow().font_bbox,
    }
}
fn char_width(code: u32, font: &CpdfFont) -> i32 {
    if code == INVALID_CHAR_CODE {
        return 0;
    }
    let width = font.get_char_width(code);
    if width > 0 {
        return width;
    }
    let mut bytes = Vec::new();
    font.append_char(&mut bytes, code);
    let mut offset = 0;
    let mut width = 0_i64;
    while offset < bytes.len() {
        let previous = offset;
        let code = font.get_next_char(&bytes, &mut offset);
        width += i64::from(font.get_char_width(code));
        if offset <= previous {
            break;
        }
    }
    if width > 0 {
        return width.min(i64::from(i32::MAX)) as i32;
    }
    let rect = font.get_char_bbox(code);
    if !rect.valid() {
        return 0;
    }
    rect.width().max(0)
}
fn object_char_width(text: &CpdfTextObject, code: u32) -> f32 {
    let Some(font) = text.font() else {
        return 0.0;
    };
    let width = match &font {
        CpdfFont::Cid(cid) if font.is_vertical() => {
            let metrics = cid.metrics.borrow();
            i32::from(metrics.get_vert_width(metrics.cid_from_char_code(code)))
        }
        _ => font.get_char_width(code),
    };
    width as f32 * text.font_size() / 1000.0
}
#[derive(Clone, Copy, Default)]
struct Item {
    code: u32,
    origin: CfxPointF,
}
fn item(text: &CpdfTextObject, index: usize) -> Item {
    let Some(&code) = text.char_codes().get(index) else {
        return Item::default();
    };
    let position = text.char_positions().get(index).copied().unwrap_or(0.0);
    let mut origin = CfxPointF::new(position, 0.0);
    if let Some(CpdfFont::Cid(cid)) = text.font() {
        let metrics = cid.metrics.borrow();
        if metrics.is_vertical() {
            let v = metrics.get_vert_origin(metrics.cid_from_char_code(code));
            origin = CfxPointF::new(
                -text.font_size() * f32::from(v.x) / 1000.0,
                position - text.font_size() * f32::from(v.y) / 1000.0,
            );
        }
    }
    Item { code, origin }
}
fn is_rtl(text: &CpdfTextObject) -> bool {
    let Some(font) = text.font() else {
        return false;
    };
    let mut value = Vec::new();
    for &code in text.char_codes() {
        let unicode = font
            .unicode_from_char_code(code)
            .first()
            .copied()
            .unwrap_or(0);
        let value_code = if unicode == 0 { code } else { unicode };
        if value_code != 0 {
            value.push(value_code);
        }
    }
    CfxBidiString::new(&value, true).overall_direction() == Direction::Right
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Orientation {
    Unknown,
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Generate {
    None,
    Space,
    LineBreak,
    Hyphen,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkedContent {
    Pass,
    Done,
    Delay,
}
#[derive(Clone, Copy)]
struct Source<'a> {
    text: &'a CpdfTextObject,
    id: PageObjectId,
    form_matrix: CfxMatrix,
}
struct Builder<'a> {
    page: &'a CpdfPage,
    result: CpdfTextPage,
    rtl: bool,
    display: CfxMatrix,
    temp_chars: Vec<CharInfo>,
    temp_text: Vec<u32>,
    previous: Option<Source<'a>>,
    text_objects: Vec<Source<'a>>,
    sources: HashMap<PageObjectId, Source<'a>>,
    textline: Orientation,
    current_line: CfxFloatRect,
}
impl<'a> Builder<'a> {
    fn new(page: &'a CpdfPage, rtl: bool) -> Self {
        Self {
            page,
            result: CpdfTextPage::default(),
            rtl,
            display: page.display_matrix(),
            temp_chars: Vec::new(),
            temp_text: Vec::new(),
            previous: None,
            text_objects: Vec::new(),
            sources: HashMap::new(),
            textline: Orientation::Unknown,
            current_line: CfxFloatRect::default(),
        }
    }
    fn build(mut self) -> CpdfTextPage {
        if self.page.holder().active_page_object_count() > 0 {
            self.textline = self.find_orientation();
            self.process_holder(self.page.holder(), CfxMatrix::default());
            self.process_transformed();
            self.text_objects.clear();
            self.close_line();
        }
        let count = self.result.count_chars();
        if count > 0 {
            self.result
                .char_indices
                .push(CharSegment { index: 0, count: 0 });
        }
        let mut skipped = false;
        for (i, info) in self.result.char_list.iter().enumerate() {
            let segment = self.result.char_indices.last_mut().unwrap();
            if info.char_type == CharType::Generated || is_normal(info) {
                segment.count += 1;
                skipped = true;
            } else if skipped {
                self.result.char_indices.push(CharSegment {
                    index: i as i32 + 1,
                    count: 0,
                });
                skipped = false;
            } else {
                segment.index = i as i32 + 1;
            }
        }
        self.result
    }
    fn previous_info(&self) -> Option<&CharInfo> {
        self.temp_chars
            .last()
            .or_else(|| self.result.char_list.last())
    }
    fn char_info(
        &self,
        kind: CharType,
        code: u32,
        unicode: u32,
        origin: CfxPointF,
        rect: CfxFloatRect,
        matrix: CfxMatrix,
        source: Option<Source<'a>>,
    ) -> CharInfo {
        let mut info = CharInfo {
            char_type: kind,
            char_code: code,
            unicode,
            origin,
            char_box: rect,
            loose_char_box: rect,
            matrix,
            text_object: source.map(|s| s.id),
        };
        if let Some(source) = source {
            info.loose_char_box = loose_bounds(&info, source.text);
        }
        info
    }
}
fn loose_bounds(info: &CharInfo, text: &CpdfTextObject) -> CfxFloatRect {
    let rect = info.char_box;
    let size = text.font_size();
    if rect.is_empty() || size.abs() < 0.0001 || info.char_code == INVALID_CHAR_CODE {
        return rect;
    }
    let Some(font) = text.font() else {
        return rect;
    };
    if let CpdfFont::Cid(cid) = &font {
        if font.is_vertical() {
            let metrics = cid.metrics.borrow();
            let code = metrics.cid_from_char_code(info.char_code);
            let origin = metrics.get_vert_origin(code);
            let dx = f64::from(i32::from(origin.x) - 500) * f64::from(size) / 1000.0;
            let dy = f64::from(origin.y) * f64::from(size) / 1000.0;
            let height = f64::from(metrics.get_vert_width(code)) * f64::from(size) / 1000.0;
            let left = (f64::from(info.origin.x) + dx) as f32;
            let top = (f64::from(info.origin.y) + dy) as f32;
            let mut loose =
                CfxFloatRect::new(left, (f64::from(top) + height) as f32, left + size, top);
            loose.union(rect);
            return loose;
        }
    }
    let mut ascent = font.type_ascent();
    let mut descent = font.type_descent();
    let bbox = font_box(&font);
    if bbox.top > bbox.bottom {
        ascent = ascent.min(bbox.top);
        descent = descent.max(bbox.bottom);
    }
    if ascent != descent {
        let width = object_char_width(text, info.char_code);
        let origin = info.matrix.get_inverse().transform(info.origin);
        let right = origin.x + if font.is_vertical() { -width } else { width };
        let mut loose = info.matrix.transform_float_rect(CfxFloatRect::new(
            origin.x,
            origin.y + descent as f32 * size / 1000.0,
            right,
            origin.y + ascent as f32 * size / 1000.0,
        ));
        loose.union(rect);
        return loose;
    }
    rect
}
#[path = "cpdf_textpage_process.rs"]
mod process;
#[cfg(test)]
#[path = "cpdf_textpage_tests.rs"]
mod tests;
