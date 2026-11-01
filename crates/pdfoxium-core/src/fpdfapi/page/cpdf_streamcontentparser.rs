// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Page content operators from `core/fpdfapi/page/cpdf_streamcontentparser.{h,cpp}`.
//! Static page/form operators, operand slots, resource lookup, and image/text
//! emission follow the pinned source; decoder internals have separate adapters.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;

use super::cpdf_allstates::CpdfAllStates;
use super::cpdf_colorspace::{get_stock_cs_for_name, ColorSpaceRef};
use super::cpdf_contentmarks::CpdfContentMarks;
use super::cpdf_docpagedata::CpdfDocPageData;
use super::cpdf_form::CpdfForm;
use super::cpdf_formobject::CpdfFormObject;
use super::cpdf_image::CpdfImage;
use super::cpdf_imageobject::CpdfImageObject;
use super::cpdf_meshstream::get_shading_bbox;
use super::cpdf_pageobject::PageObject;
use super::cpdf_pageobjectholder::CpdfPageObjectHolder;
use super::cpdf_path::CpdfPath;
use super::cpdf_pathobject::CpdfPathObject;
use super::cpdf_shadingobject::CpdfShadingObject;
use super::cpdf_streamparser::{CpdfStreamParser, ElementType};
use super::cpdf_textobject::CpdfTextObject;
use super::cpdf_textstate::{
    set_text_rendering_mode_from_int, text_rendering_mode_is_clip_mode,
    text_rendering_mode_is_stroke_mode, TextRenderingMode,
};
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_name::CpdfName;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::fpdf_parser_utility::pdf_name_decode;
use crate::fpdfapi::render::cpdf_renderstatus::with_render_font_mapper;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use crate::fxcrt::fx_number::FxNumber;
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::fxge::cfx_graphstatedata::{LineCap, LineJoin};
use crate::fxge::cfx_path::{PathPoint, PointType};

enum ContentParam {
    Number(FxNumber),
    Name(Vec<u8>),
    Object(Option<ObjectRef>),
}

struct ParsedStreamEntry {
    parsed: Rc<RefCell<HashSet<usize>>>,
    identity: usize,
}
impl Drop for ParsedStreamEntry {
    fn drop(&mut self) {
        self.parsed.borrow_mut().remove(&self.identity);
    }
}

pub struct CpdfStreamContentParser {
    cur_states: CpdfAllStates,
    page_data: Rc<CpdfDocPageData>,
    state_stack: Vec<CpdfAllStates>,
    params: [ContentParam; 16],
    param_start_pos: usize,
    param_count: usize,
    content_marks_stack: Vec<CpdfContentMarks>,
    path_points: Vec<PathPoint>,
    path_start: CfxPointF,
    path_current: CfxPointF,
    path_clip_type: FillType,
    clip_text_list: Vec<Box<CpdfTextObject>>,
    content_to_user: CfxMatrix,
    all_ctms: BTreeMap<i32, CfxMatrix>,
    last_image: Option<(Vec<u8>, Rc<CpdfImage>)>,
    form_visited: Rc<RefCell<HashSet<usize>>>,
    form_depth: usize,
    type3_data: [f32; 6],
    colored: bool,
}

impl CpdfStreamContentParser {
    pub fn new(states: Option<CpdfAllStates>, content_to_user: CfxMatrix) -> Self {
        Self::with_page_data(states, content_to_user, Rc::new(CpdfDocPageData::new()))
    }
    pub fn with_page_data(
        states: Option<CpdfAllStates>,
        content_to_user: CfxMatrix,
        page_data: Rc<CpdfDocPageData>,
    ) -> Self {
        Self::with_page_data_and_forms(
            states,
            content_to_user,
            page_data,
            Rc::new(RefCell::new(HashSet::new())),
            0,
        )
    }
    pub(crate) fn with_page_data_and_forms(
        states: Option<CpdfAllStates>,
        content_to_user: CfxMatrix,
        page_data: Rc<CpdfDocPageData>,
        form_visited: Rc<RefCell<HashSet<usize>>>,
        form_depth: usize,
    ) -> Self {
        let cur_states = states.unwrap_or_else(|| {
            let mut states = CpdfAllStates::new();
            states.mutable_general_state().emplace();
            states.mutable_graph_state().emplace();
            states.mutable_text_state().emplace();
            states.mutable_color_state().emplace();
            states
        });
        let mut all_ctms = BTreeMap::new();
        all_ctms.insert(0, cur_states.current_transformation_matrix());
        Self {
            cur_states,
            page_data,
            state_stack: Vec::new(),
            params: std::array::from_fn(|_| ContentParam::Object(None)),
            param_start_pos: 0,
            param_count: 0,
            content_marks_stack: vec![CpdfContentMarks::new()],
            path_points: Vec::new(),
            path_start: CfxPointF::default(),
            path_current: CfxPointF::default(),
            path_clip_type: FillType::NoFill,
            clip_text_list: Vec::new(),
            content_to_user,
            all_ctms,
            last_image: None,
            form_visited,
            form_depth,
            type3_data: [0.0; 6],
            colored: false,
        }
    }
    pub fn cur_states(&self) -> &CpdfAllStates {
        &self.cur_states
    }
    pub(crate) fn cur_states_mut(&mut self) -> &mut CpdfAllStates {
        &mut self.cur_states
    }
    pub fn take_all_ctms(&mut self) -> BTreeMap<i32, CfxMatrix> {
        std::mem::take(&mut self.all_ctms)
    }

    pub fn type3_data(&self) -> [f32; 6] {
        self.type3_data
    }

    pub fn is_colored(&self) -> bool {
        self.colored
    }

    fn number(&self, index: usize) -> f32 {
        match self.param(index) {
            Some(ContentParam::Number(number)) => number.get_float(),
            Some(ContentParam::Object(Some(object))) => object.borrow().get_number(),
            _ => 0.0,
        }
    }
    fn integer(&self, index: usize) -> Option<i32> {
        let number = self.number(index);
        (f64::from(number) >= f64::from(i32::MIN) && f64::from(number) <= f64::from(i32::MAX))
            .then_some(number as i32)
    }
    fn numbers(&self, count: usize) -> Vec<f32> {
        (0..count).rev().map(|index| self.number(index)).collect()
    }
    fn param_pos(&self, index: usize) -> Option<usize> {
        (index < self.param_count)
            .then(|| (self.param_start_pos + self.param_count - index - 1) % 16)
    }
    fn param(&self, index: usize) -> Option<&ContentParam> {
        self.param_pos(index).map(|position| &self.params[position])
    }
    fn object(&mut self, index: usize) -> Option<ObjectRef> {
        let position = self.param_pos(index)?;
        let object = match &self.params[position] {
            ContentParam::Object(object) => return object.clone(),
            ContentParam::Name(name) => retain_object(CpdfName::new(None, name)),
            ContentParam::Number(number) => retain_object(if number.is_integer() {
                CpdfScalarObject::number_from_signed(number.get_signed())
            } else {
                CpdfScalarObject::number_from_float(number.get_float())
            }),
        };
        self.params[position] = ContentParam::Object(Some(object.clone()));
        Some(object)
    }
    fn string(&self, index: usize) -> Vec<u8> {
        match self.param(index) {
            Some(ContentParam::Name(name)) => name.clone(),
            Some(ContentParam::Object(Some(object))) => object.borrow().get_string(),
            _ => Vec::new(),
        }
    }
    fn find_resource(resources: Option<&ObjectRef>, kind: &[u8], name: &[u8]) -> Option<ObjectRef> {
        let resources = resources?;
        let borrowed = resources.borrow();
        let subdict = borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_dict_for(kind)?;
        let borrowed = subdict.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_direct_object_for(name)
    }
    fn find_resource_holder(resources: Option<&ObjectRef>, kind: &[u8]) -> Option<ObjectRef> {
        let resources = resources?;
        let borrowed = resources.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_dict_for(kind)
    }

    fn find_font(
        page_data: &Rc<CpdfDocPageData>,
        holder: &CpdfPageObjectHolder,
        name: &[u8],
    ) -> (Option<CpdfFont>, Option<ObjectRef>) {
        let dict = Self::find_resource(
            Self::resource_scope_for_kind(holder, b"Font"),
            b"Font",
            name,
        )
        .filter(|object| object.borrow().as_any().is::<CpdfDictionary>());
        let (font, dict) = if let Some(dict) = dict {
            let font = with_render_font_mapper(|mapper| page_data.get_font(&dict, mapper));
            if let Some(font) = &font {
                font.set_resource_name(name);
            }
            let dict = font.as_ref().map(|_| dict);
            (font, dict)
        } else {
            let font = with_render_font_mapper(|mapper| {
                page_data.get_stock_font(
                    crate::fxge::cfx_font::DEFAULT_ANSI_FONT_NAME.as_bytes(),
                    mapper,
                )
            });
            let dict = font.as_ref().and_then(|font| font.font_dict());
            (font.map(CpdfFont::Simple), dict)
        };
        if let Some(CpdfFont::Type3(type3)) = &font {
            type3.set_page_resources(holder.resources().cloned());
            type3.check_type3_font_metrics();
        }
        (font, dict)
    }
    fn resource_scope_for_kind<'a>(
        holder: &'a CpdfPageObjectHolder,
        kind: &[u8],
    ) -> Option<&'a ObjectRef> {
        let resources = holder.resources();
        if Self::find_resource_holder(resources, kind).is_some() {
            resources
        } else {
            holder.page_resources()
        }
    }
    fn find_color_space(
        &self,
        holder: &CpdfPageObjectHolder,
        name: &[u8],
    ) -> Option<ColorSpaceRef> {
        if crate::fxcrt::bytestring::eq_cstr(name, b"Pattern") {
            return get_stock_cs_for_name(name);
        }
        let resources = Self::resource_scope_for_kind(holder, b"ColorSpace");
        if matches!(
            crate::fxcrt::bytestring::c_str(name),
            b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK"
        ) {
            let mut default_name = b"Default".to_vec();
            default_name.extend_from_slice(&name[6..]);
            return match Self::find_resource(resources, b"ColorSpace", &default_name) {
                Some(object) => self.page_data.get_color_space(&object, None),
                None => get_stock_cs_for_name(name),
            };
        }
        let object = Self::find_resource(resources, b"ColorSpace", name)?;
        self.page_data.get_color_space(&object, None)
    }
    fn find_key_abbreviation(name: &[u8]) -> &'static [u8] {
        match name {
            b"BPC" => b"BitsPerComponent",
            b"CS" => b"ColorSpace",
            b"D" => b"Decode",
            b"DP" => b"DecodeParms",
            b"F" => b"Filter",
            b"H" => b"Height",
            b"IM" => b"ImageMask",
            b"I" => b"Interpolate",
            b"W" => b"Width",
            _ => b"",
        }
    }
    fn find_value_abbreviation(name: &[u8]) -> &'static [u8] {
        match name {
            b"G" => b"DeviceGray",
            b"RGB" => b"DeviceRGB",
            b"CMYK" => b"DeviceCMYK",
            b"I" => b"Indexed",
            b"AHx" => b"ASCIIHexDecode",
            b"A85" => b"ASCII85Decode",
            b"LZW" => b"LZWDecode",
            b"Fl" => b"FlateDecode",
            b"RL" => b"RunLengthDecode",
            b"CCF" => b"CCITTFaxDecode",
            b"DCT" => b"DCTDecode",
            _ => b"",
        }
    }
    fn expand_inline_abbreviations(object: &ObjectRef) {
        let mut borrowed = object.borrow_mut();
        if let Some(dict) = borrowed.as_any_mut().downcast_mut::<CpdfDictionary>() {
            // Defer replacements until every original entry has been visited,
            // as the native dictionary locker requires. Collision order matters.
            enum Replacement {
                Key(Rc<[u8]>, &'static [u8]),
                Value(Rc<[u8]>, &'static [u8], &'static [u8]),
            }
            let mut replacements = Vec::new();
            dict.with_locked_entries(|entries| {
                for (key, value) in entries {
                    let full_key = Self::find_key_abbreviation(key);
                    if !full_key.is_empty() {
                        replacements.push(Replacement::Key(Rc::clone(key), full_key));
                    }
                    let full_name = {
                        let value = value.borrow();
                        value
                            .as_any()
                            .downcast_ref::<CpdfName>()
                            .map(|name| Self::find_value_abbreviation(name.get_string()))
                    };
                    if let Some(full) = full_name {
                        if !full.is_empty() {
                            replacements.push(Replacement::Value(Rc::clone(key), full_key, full));
                        }
                    } else {
                        Self::expand_inline_abbreviations(value);
                    }
                }
            });
            for replacement in replacements {
                match replacement {
                    Replacement::Key(old, new) => dict.replace_key(&old, &new),
                    Replacement::Value(key, full_key, name) => {
                        let key = if full_key.is_empty() {
                            key.as_ref()
                        } else {
                            full_key
                        };
                        dict.set_new_name_for(key, name);
                    }
                }
            }
        } else if let Some(array) = borrowed.as_any_mut().downcast_mut::<CpdfArray>() {
            for index in 0..array.size() {
                let Some(value) = array.get_object_at(index) else {
                    continue;
                };
                let full_name = {
                    let value = value.borrow();
                    value
                        .as_any()
                        .downcast_ref::<CpdfName>()
                        .map(|name| Self::find_value_abbreviation(name.get_string()))
                };
                if let Some(full) = full_name {
                    if !full.is_empty() {
                        array.set_new_at(index, CpdfName::new(None, full));
                    }
                } else {
                    Self::expand_inline_abbreviations(&value);
                }
            }
        }
    }
    fn handle_begin_image(
        &mut self,
        syntax: &mut CpdfStreamParser<'_>,
        start_offset: usize,
        stream_offsets: &[u32],
        holder: &mut CpdfPageObjectHolder,
    ) {
        let saved_pos = syntax.get_pos();
        let mut dict = CpdfDictionary::with_pool(syntax.byte_string_pool());
        loop {
            let kind = syntax.parse_next_element();
            if kind == ElementType::Keyword && syntax.get_word() != b"ID" {
                syntax.set_pos(saved_pos);
                return;
            }
            if kind != ElementType::Name {
                break;
            }
            let key = syntax.get_word().get(1..).unwrap_or_default().to_vec();
            let value = syntax.read_next_object(false, false, 0);
            dict.set_for(&key, value);
        }
        let dict = retain_object(dict);
        Self::expand_inline_abbreviations(&dict);
        let mut color_space = {
            let borrowed = dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_direct_object_for(b"ColorSpace")
        };
        if let Some(object) = &color_space {
            let name = {
                let object = object.borrow();
                object
                    .as_any()
                    .is::<CpdfName>()
                    .then(|| object.get_string())
            };
            if let Some(name) = name {
                if !matches!(
                    crate::fxcrt::bytestring::c_str(&name),
                    b"DeviceRGB" | b"DeviceGray" | b"DeviceCMYK"
                ) {
                    color_space = Self::find_resource(
                        Self::resource_scope_for_kind(holder, b"ColorSpace"),
                        b"ColorSpace",
                        &name,
                    );
                    if let Some(object) = &color_space {
                        if object.borrow().obj_num() == 0 {
                            // This snapshot is explicitly required by native BI.
                            let cloned = object.borrow().clone_object();
                            dict.borrow_mut()
                                .as_any_mut()
                                .downcast_mut::<CpdfDictionary>()
                                .unwrap()
                                .set_for(b"ColorSpace", Some(cloned));
                        }
                    }
                }
            }
        }
        let color_components = color_space.and_then(|cs| {
            self.page_data
                .get_color_space(&cs, None)
                .map(|space| space.component_count())
        });
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_name_for(b"Subtype", b"Image");
        let stream = syntax.read_inline_stream(dict, color_components);
        loop {
            match syntax.parse_next_element() {
                ElementType::EndOfData => return,
                ElementType::Keyword if syntax.get_word() == b"EI" => break,
                _ => {}
            }
        }
        let stream_index =
            Self::stream_index(start_offset + syntax.get_pos() as usize, stream_offsets);
        if let Some(image) = stream
            .and_then(|stream| CpdfImage::from_stream_with_page_data(stream, true, &self.page_data))
            .map(Rc::new)
        {
            self.add_image_object(image, b"", stream_index, holder);
        }
    }
    fn push_param(&mut self, param: ContentParam) {
        let position = if self.param_count == 16 {
            // Preserve the pinned GetNextParamPos overwrite order, including
            // advancing the logical start before selecting the full-buffer slot.
            self.param_start_pos = (self.param_start_pos + 1) % 16;
            self.param_start_pos
        } else {
            let position = (self.param_start_pos + self.param_count) % 16;
            self.param_count += 1;
            position
        };
        self.params[position] = param;
    }
    fn clear_all_params(&mut self) {
        for offset in 0..self.param_count {
            let position = (self.param_start_pos + offset) % 16;
            if matches!(self.params[position], ContentParam::Object(_)) {
                self.params[position] = ContentParam::Object(None);
            }
        }
        self.param_start_pos = 0;
        self.param_count = 0;
    }
    fn stream_index(position: usize, offsets: &[u32]) -> i32 {
        offsets
            .partition_point(|&offset| offset as usize <= position)
            .saturating_sub(1) as i32
    }
    fn add_path_point(&mut self, point: CfxPointF, kind: PointType) {
        if kind == PointType::Move
            && self.path_points.last().is_some_and(|last| {
                !last.close_figure
                    && last.point_type == PointType::Move
                    && self.path_current == point
            })
        {
            return;
        }
        self.path_current = point;
        if kind == PointType::Move {
            self.path_start = point;
            if let Some(last) = self.path_points.last_mut() {
                if last.point_type == PointType::Move && !last.close_figure {
                    last.point = point;
                    return;
                }
            }
        } else if self.path_points.is_empty() {
            return;
        }
        self.path_points.push(PathPoint::new(point, kind, false));
    }
    fn add_path_point_and_close(&mut self, point: CfxPointF, kind: PointType) {
        self.path_current = point;
        if !self.path_points.is_empty() {
            self.path_points.push(PathPoint::new(point, kind, true));
        }
    }
    fn close_path(&mut self) {
        if self.path_points.is_empty() {
            return;
        }
        if self.path_start != self.path_current {
            self.add_path_point_and_close(self.path_start, PointType::Line);
        } else if let Some(last) = self.path_points.last_mut() {
            last.close_figure = true;
        }
    }
    fn add_path_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.add_path_point(CfxPointF::new(x, y), PointType::Move);
        self.add_path_point(CfxPointF::new(x + w, y), PointType::Line);
        self.add_path_point(CfxPointF::new(x + w, y + h), PointType::Line);
        self.add_path_point(CfxPointF::new(x, y + h), PointType::Line);
        self.add_path_point_and_close(CfxPointF::new(x, y), PointType::Line);
    }
    fn add_path_object(
        &mut self,
        fill: FillType,
        stroke: bool,
        stream_index: i32,
        holder: &mut CpdfPageObjectHolder,
    ) {
        let mut points = std::mem::take(&mut self.path_points);
        let clip = std::mem::replace(&mut self.path_clip_type, FillType::NoFill);
        if points.is_empty() {
            return;
        }
        if points.len() == 1 {
            if clip != FillType::NoFill {
                let mut path = CpdfPath::new();
                path.append_rect(0.0, 0.0, 0.0, 0.0);
                self.cur_states
                    .mutable_clip_path()
                    .append_path_with_auto_merge(path, FillType::Winding);
                return;
            }
            let point = points[0];
            if point.point_type != PointType::Move
                || !point.close_figure
                || self.cur_states.graph_state().get_line_cap() != LineCap::ROUND
            {
                return;
            }
            points[0].close_figure = false;
            points.push(PathPoint::new(point.point, PointType::Line, true));
        }
        if points
            .last()
            .is_some_and(|point| point.point_type == PointType::Move && !point.close_figure)
        {
            points.pop();
        }
        let mut path = CpdfPath::new();
        for point in points {
            if point.close_figure {
                path.append_point_and_close(point.point, point.point_type);
            } else {
                path.append_point(point.point, point.point_type);
            }
        }
        let matrix = self.cur_states.current_transformation_matrix() * self.content_to_user;
        if stroke || fill != FillType::NoFill {
            let mut object = CpdfPathObject::new(stream_index);
            object.set_stroke(stroke);
            object.set_filltype(fill);
            *object.mutable_path() = path.clone();
            *object.base_mut().mutable_general_state() = self.cur_states.general_state().clone();
            *object.base_mut().mutable_clip_path() = self.cur_states.clip_path().clone();
            *object.base_mut().mutable_color_state() = self.cur_states.color_state().clone();
            *object.base_mut().mutable_graph_state() = self.cur_states.graph_state().clone();
            object
                .base_mut()
                .set_content_marks(self.content_marks_stack.last().unwrap().clone());
            object.set_path_matrix(matrix);
            holder.append_page_object(Box::new(object));
        }
        if clip != FillType::NoFill {
            if !matrix.is_identity() {
                path.transform(matrix);
            }
            self.cur_states
                .mutable_clip_path()
                .append_path_with_auto_merge(path, clip);
        }
    }
    fn add_image_object(
        &self,
        image: Rc<CpdfImage>,
        name: &[u8],
        stream_index: i32,
        holder: &mut CpdfPageObjectHolder,
    ) {
        let mut object = CpdfImageObject::new(stream_index);
        object.set_page_data(Rc::downgrade(&self.page_data));
        object.base_mut().set_resource_name(name);
        *object.base_mut().mutable_general_state() = self.cur_states.general_state().clone();
        *object.base_mut().mutable_clip_path() = self.cur_states.clip_path().clone();
        if image.is_mask() {
            *object.base_mut().mutable_color_state() = self.cur_states.color_state().clone();
        }
        object
            .base_mut()
            .set_content_marks(self.content_marks_stack.last().unwrap().clone());
        object.set_image(Some(image.clone()));
        object.set_initial_image_matrix(
            self.cur_states.current_transformation_matrix() * self.content_to_user,
        );
        if image.is_mask() {
            holder.add_image_mask_bounding_box(object.base().rect());
        }
        holder.append_page_object(Box::new(object));
    }
    fn add_form_object(
        &self,
        stream: ObjectRef,
        name: &[u8],
        stream_index: i32,
        holder: &mut CpdfPageObjectHolder,
    ) {
        let Some(mut form) = CpdfForm::new(
            stream.clone(),
            holder.page_resources().cloned(),
            holder.resources().cloned(),
        ) else {
            return;
        };
        let mut states = CpdfAllStates::new();
        *states.mutable_general_state() = self.cur_states.general_state().clone();
        *states.mutable_graph_state() = self.cur_states.graph_state().clone();
        *states.mutable_color_state() = self.cur_states.color_state().clone();
        *states.mutable_text_state() = self.cur_states.text_state().clone();
        form.parse_content(
            Some(states),
            None,
            Rc::clone(&self.page_data),
            Rc::clone(&self.form_visited),
            self.form_depth + 1,
        );
        let matrix = self.cur_states.current_transformation_matrix() * self.content_to_user;
        let mut object = CpdfFormObject::new(stream_index, form, matrix);
        object.base_mut().set_resource_name(name);
        if !holder.background_alpha_needed() && object.form().holder().background_alpha_needed() {
            holder.set_background_alpha_needed(true);
        }
        object.calc_bounding_box();
        *object.base_mut().mutable_general_state() = self.cur_states.general_state().clone();
        *object.base_mut().mutable_clip_path() = self.cur_states.clip_path().clone();
        *object.base_mut().mutable_color_state() = self.cur_states.color_state().clone();
        *object.base_mut().mutable_text_state() = self.cur_states.text_state().clone();
        *object.base_mut().mutable_graph_state() = self.cur_states.graph_state().clone();
        object
            .base_mut()
            .set_content_marks(self.content_marks_stack.last().unwrap().clone());
        holder.append_page_object(Box::new(object));
    }
    fn on_change_text_matrix(&mut self) {
        let scale = self.cur_states.text_horz_scale();
        let matrix = CfxMatrix::new(scale, 0.0, 0.0, 1.0, 0.0, 0.0)
            * self.cur_states.text_matrix()
            * self.cur_states.current_transformation_matrix()
            * self.content_to_user;
        *self.cur_states.mutable_text_state().get_mutable_matrix() =
            [matrix.a, matrix.c, matrix.b, matrix.d];
    }
    fn add_text_object(
        &mut self,
        strings: &[Vec<u8>],
        kernings: &[f32],
        initial_kerning: f32,
        stream_index: i32,
        holder: &mut CpdfPageObjectHolder,
    ) {
        let Some(font) = self.cur_states.text_state().get_font() else {
            return;
        };
        let vertical = font.is_vertical();
        let text_mode = if matches!(font, CpdfFont::Type3(_)) {
            TextRenderingMode::Fill
        } else {
            self.cur_states.text_state().get_text_mode()
        };
        let scale = self.cur_states.text_horz_scale();
        if initial_kerning != 0.0 {
            self.shift_text_for_kerning(initial_kerning, vertical);
        }
        if strings.is_empty() {
            return;
        }
        let mut object = CpdfTextObject::new(stream_index);
        if let Some(font) = self.cur_states.text_state().get_font() {
            object.base_mut().set_resource_name(&font.resource_name());
        }
        *object.base_mut().mutable_general_state() = self.cur_states.general_state().clone();
        *object.base_mut().mutable_clip_path() = self.cur_states.clip_path().clone();
        *object.base_mut().mutable_color_state() = self.cur_states.color_state().clone();
        *object.base_mut().mutable_text_state() = self.cur_states.text_state().clone();
        *object.base_mut().mutable_graph_state() = self.cur_states.graph_state().clone();
        object
            .base_mut()
            .set_content_marks(self.content_marks_stack.last().unwrap().clone());
        if text_rendering_mode_is_stroke_mode(text_mode) {
            let ctm = self.cur_states.current_transformation_matrix();
            *object.base_mut().mutable_text_state().get_mutable_ctm() =
                [ctm.a, ctm.c, ctm.b, ctm.d];
        }
        object.set_segments(strings, kernings);
        object.set_position(
            self.content_to_user
                .transform(self.cur_states.get_transformed_text_position()),
        );
        let advance = object.calc_position_data(scale);
        self.cur_states.increment_text_position_x(advance.x);
        self.cur_states.increment_text_position_y(advance.y);
        if text_rendering_mode_is_clip_mode(text_mode) {
            self.clip_text_list.push(Box::new(object.clone()));
        }
        holder.append_page_object(Box::new(object));
        if let Some(&trailing) = kernings.last() {
            self.shift_text_for_kerning(trailing, vertical);
        }
    }
    fn shift_text_for_kerning(&mut self, kerning: f32, vertical: bool) {
        let size = kerning * self.cur_states.text_state().get_font_size() / 1000.0;
        if vertical {
            self.cur_states.increment_text_position_y(-size);
        } else {
            self.cur_states
                .increment_text_position_x(-size * self.cur_states.text_horz_scale());
        }
    }
    fn show_positioned_text(&mut self, stream_index: i32, holder: &mut CpdfPageObjectHolder) {
        let Some(object) = self.object(0) else { return };
        let borrowed = object.borrow();
        let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
            return;
        };
        let segments = (0..array.size())
            .filter(|&index| {
                array.get_direct_object_at(index).is_some_and(|object| {
                    object.borrow().object_type()
                        == crate::fpdfapi::parser::cpdf_scalar_object::ObjectType::String
                })
            })
            .count();
        if segments == 0 {
            for index in 0..array.size() {
                let kerning = array.get_float_at(index);
                if kerning != 0.0 {
                    // Native numeric-only TJ moves X even for vertical fonts.
                    self.shift_text_for_kerning(kerning, false);
                }
            }
            return;
        }
        let mut strings = Vec::with_capacity(segments);
        let mut kernings = vec![0.0; segments];
        let mut initial_kerning = 0.0;
        for index in 0..array.size() {
            let Some(element) = array.get_direct_object_at(index) else {
                continue;
            };
            let element = element.borrow();
            if element.object_type()
                == crate::fpdfapi::parser::cpdf_scalar_object::ObjectType::String
            {
                let bytes = element.get_string();
                if !bytes.is_empty() {
                    kernings[strings.len()] = 0.0;
                    strings.push(bytes);
                }
            } else if strings.is_empty() {
                initial_kerning += element.get_number();
            } else {
                kernings[strings.len() - 1] += element.get_number();
            }
        }
        drop(borrowed);
        // Keep the original-sized kerning array: empty PDF strings leave
        // trailing zero entries, which native AddTextObject observes.
        self.add_text_object(&strings, &kernings, initial_kerning, stream_index, holder);
    }
    fn set_pattern_color(&mut self, fill: bool, holder: &CpdfPageObjectHolder) {
        if self.param_count == 0 {
            return;
        }
        let Some(last) = self.object(0) else { return };
        let name = if last.borrow().object_type()
            == crate::fpdfapi::parser::cpdf_scalar_object::ObjectType::Name
        {
            self.string(0)
        } else {
            let values = self.numbers(self.param_count);
            if fill {
                self.cur_states
                    .mutable_color_state()
                    .set_fill_color(None, values);
            } else {
                self.cur_states
                    .mutable_color_state()
                    .set_stroke_color(None, values);
            }
            return;
        };
        let resources = Self::resource_scope_for_kind(holder, b"Pattern");
        let Some(object) = Self::find_resource(resources, b"Pattern", &name) else {
            return;
        };
        let Some(pattern) = self
            .page_data
            .get_pattern(&object, self.cur_states.parent_matrix())
        else {
            return;
        };
        let values = (1..self.param_count)
            .rev()
            .map(|index| self.number(index))
            .collect::<Vec<_>>();
        if fill {
            self.cur_states
                .mutable_color_state()
                .set_fill_pattern(pattern, &values);
        } else {
            self.cur_states
                .mutable_color_state()
                .set_stroke_pattern(pattern, &values);
        }
    }
    fn operator(&mut self, op: &[u8], stream_index: i32, holder: &mut CpdfPageObjectHolder) {
        let n = self.param_count;
        let last_number = self.number(0);
        let last_integer = self.integer(0).unwrap_or(0);
        let color_values = if matches!(op, b"g" | b"G") {
            self.numbers(1)
        } else if matches!(
            op,
            b"g" | b"G" | b"rg" | b"RG" | b"k" | b"K" | b"sc" | b"SC"
        ) {
            self.numbers(n.min(4))
        } else {
            Vec::new()
        };
        match op {
            b"d0" => {
                self.type3_data[0] = self.number(1);
                self.type3_data[1] = self.number(0);
                self.colored = true;
            }
            b"d1" => {
                for index in 0..6 {
                    self.type3_data[index] = self.number(5 - index);
                }
                self.colored = false;
            }
            b"BT" => {
                self.cur_states.set_text_matrix(CfxMatrix::default());
                self.on_change_text_matrix();
                self.cur_states.reset_text_position();
            }
            b"ET" => {
                if !self.clip_text_list.is_empty() {
                    if text_rendering_mode_is_clip_mode(
                        self.cur_states.text_state().get_text_mode(),
                    ) {
                        self.cur_states
                            .mutable_clip_path()
                            .append_texts(&mut self.clip_text_list);
                    }
                    self.clip_text_list.clear();
                }
            }
            b"Tm" => {
                self.cur_states.set_text_matrix(CfxMatrix::new(
                    self.number(5),
                    self.number(4),
                    self.number(3),
                    self.number(2),
                    self.number(1),
                    self.number(0),
                ));
                self.on_change_text_matrix();
                self.cur_states.reset_text_position();
            }
            b"Td" => self
                .cur_states
                .move_text_point(CfxPointF::new(self.number(1), self.number(0))),
            b"TD" => {
                self.cur_states
                    .move_text_point(CfxPointF::new(self.number(1), self.number(0)));
                self.cur_states.set_text_leading(-self.number(0));
            }
            b"T*" => self.cur_states.move_text_to_next_line(),
            b"'" => {
                self.cur_states.move_text_to_next_line();
                let text = self.string(0);
                if !text.is_empty() {
                    self.add_text_object(&[text], &[], 0.0, stream_index, holder);
                }
            }
            b"\"" => {
                let word_space = self.number(2);
                let char_space = self.number(1);
                self.cur_states
                    .mutable_text_state()
                    .set_word_space(word_space);
                self.cur_states
                    .mutable_text_state()
                    .set_char_space(char_space);
                self.cur_states.move_text_to_next_line();
                let text = self.string(0);
                if !text.is_empty() {
                    self.add_text_object(&[text], &[], 0.0, stream_index, holder);
                }
            }
            b"Tj" => {
                let text = self.string(0);
                if !text.is_empty() {
                    self.add_text_object(&[text], &[], 0.0, stream_index, holder);
                }
            }
            b"TJ" => self.show_positioned_text(stream_index, holder),
            b"Tc" => self
                .cur_states
                .mutable_text_state()
                .set_char_space(last_number),
            b"Tw" => self
                .cur_states
                .mutable_text_state()
                .set_word_space(last_number),
            b"TL" => self.cur_states.set_text_leading(last_number),
            b"Ts" => self.cur_states.set_text_rise(last_number),
            b"Tr" => {
                if let Some(mode) = set_text_rendering_mode_from_int(last_integer) {
                    self.cur_states.mutable_text_state().set_text_mode(mode);
                }
            }
            b"Tz" if n == 1 => {
                self.cur_states.set_text_horz_scale(last_number / 100.0);
                self.on_change_text_matrix();
            }
            b"Tf" => {
                self.cur_states
                    .mutable_text_state()
                    .set_font_size(last_number);
                let name = self.string(1);
                let (font, dict) = Self::find_font(&self.page_data, holder, &name);
                if font.is_some() {
                    self.cur_states.mutable_text_state().set_font(font, dict);
                }
            }
            b"BMC" => {
                let mut marks = self.content_marks_stack.last().unwrap().clone_marks();
                marks.add_mark(&self.string(0));
                self.content_marks_stack.push(marks);
            }
            b"BDC" => {
                let Some(property) = self.object(0) else {
                    return;
                };
                let tag = self.string(1);
                let mut marks = self.content_marks_stack.last().unwrap().clone_marks();
                let kind = property.borrow().object_type();
                if kind == crate::fpdfapi::parser::cpdf_scalar_object::ObjectType::Name {
                    let property_name = property.borrow().get_string();
                    let resources = Self::resource_scope_for_kind(holder, b"Properties");
                    let Some(properties) = Self::find_resource_holder(resources, b"Properties")
                    else {
                        return;
                    };
                    let valid = properties
                        .borrow()
                        .as_any()
                        .downcast_ref::<CpdfDictionary>()
                        .is_some_and(|dict| dict.get_dict_for(&property_name).is_some());
                    if !valid {
                        return;
                    }
                    marks.add_mark_with_properties_holder(&tag, properties, &property_name);
                } else if kind == crate::fpdfapi::parser::cpdf_scalar_object::ObjectType::Dictionary
                {
                    if marks.add_mark_with_direct_dict(&tag, &property).is_none() {
                        return;
                    }
                } else {
                    return;
                }
                self.content_marks_stack.push(marks);
            }
            b"EMC" => {
                if self.content_marks_stack.len() > 1 {
                    self.content_marks_stack.pop();
                }
            }
            b"Do" => {
                let name = self.string(0);
                if let Some((last_name, last_image)) = &self.last_image {
                    if *last_name == name && last_image.stream().borrow().obj_num() != 0 {
                        self.add_image_object(Rc::clone(last_image), &name, stream_index, holder);
                        return;
                    }
                }
                let resources = Self::resource_scope_for_kind(holder, b"XObject");
                let Some(stream) = Self::find_resource(resources, b"XObject", &name) else {
                    return;
                };
                let (obj_num, subtype) = {
                    let borrowed = stream.borrow();
                    let Some(stream) = borrowed.as_any().downcast_ref::<CpdfStream>() else {
                        return;
                    };
                    let dict = stream.get_dict();
                    let borrowed = dict.borrow();
                    let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
                        return;
                    };
                    (stream.obj_num(), dict.get_byte_string_for(b"Subtype", b""))
                };
                if crate::fxcrt::bytestring::eq_cstr(&subtype, b"Form") {
                    self.add_form_object(stream, &name, stream_index, holder);
                    return;
                }
                if !crate::fxcrt::bytestring::eq_cstr(&subtype, b"Image") {
                    return;
                }
                let image = if obj_num == 0 {
                    let clone = stream.borrow().clone_object();
                    CpdfImage::from_stream_with_page_data(clone, true, &self.page_data).map(Rc::new)
                } else {
                    self.page_data.get_image(&stream)
                };
                if let Some(image) = image {
                    self.add_image_object(Rc::clone(&image), &name, stream_index, holder);
                    self.last_image = Some((name, image));
                }
            }
            b"sh" => {
                let name = self.string(0);
                let resources = Self::resource_scope_for_kind(holder, b"Shading");
                let Some(shading) = Self::find_resource(resources, b"Shading", &name) else {
                    return;
                };
                let matrix = self.cur_states.current_transformation_matrix() * self.content_to_user;
                let Some(pattern) = self
                    .page_data
                    .get_shading(&shading, self.cur_states.parent_matrix())
                else {
                    return;
                };
                if !pattern.is_shading_object() || !pattern.load(&self.page_data) {
                    return;
                }
                let mut object = CpdfShadingObject::new(stream_index, pattern, matrix);
                *object.base_mut().mutable_general_state() =
                    self.cur_states.general_state().clone();
                *object.base_mut().mutable_clip_path() = self.cur_states.clip_path().clone();
                object
                    .base_mut()
                    .set_content_marks(self.content_marks_stack.last().unwrap().clone());
                let bbox = if object.base().clip_path().has_ref() {
                    object.base().clip_path().clip_box()
                } else {
                    holder.bbox()
                };
                let mut bbox = bbox;
                if object.pattern().is_mesh_shading() {
                    bbox.intersect(get_shading_bbox(object.pattern(), object.matrix()));
                }
                object.base_mut().set_rect(bbox);
                holder.append_page_object(Box::new(object));
            }
            b"q" => self.state_stack.push(self.cur_states.clone()),
            b"Q" => {
                if let Some(states) = self.state_stack.pop() {
                    self.cur_states = states;
                    self.all_ctms.insert(
                        stream_index,
                        self.cur_states.current_transformation_matrix(),
                    );
                }
            }
            b"cm" => {
                self.cur_states
                    .prepend_to_current_transformation_matrix(CfxMatrix::new(
                        self.number(5),
                        self.number(4),
                        self.number(3),
                        self.number(2),
                        self.number(1),
                        self.number(0),
                    ));
                self.all_ctms.insert(
                    stream_index,
                    self.cur_states.current_transformation_matrix(),
                );
                self.on_change_text_matrix();
            }
            b"w" => self
                .cur_states
                .mutable_graph_state()
                .set_line_width(last_number),
            b"J" => self
                .cur_states
                .mutable_graph_state()
                .set_line_cap(LineCap::from_pdf_integer(last_integer)),
            b"j" => self
                .cur_states
                .mutable_graph_state()
                .set_line_join(LineJoin::from_pdf_integer(last_integer)),
            b"M" => self
                .cur_states
                .mutable_graph_state()
                .set_miter_limit(last_number),
            b"i" => self
                .cur_states
                .mutable_general_state()
                .set_flatness(last_number),
            b"d" => {
                if let Some(array) = self.object(1) {
                    let borrowed = array.borrow();
                    if let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() {
                        self.cur_states.set_line_dash(array, last_number);
                    }
                }
            }
            b"gs" => {
                let name = self.string(0);
                let resources = Self::resource_scope_for_kind(holder, b"ExtGState").cloned();
                let gs = Self::find_resource(resources.as_ref(), b"ExtGState", &name);
                if let Some(gs) = gs {
                    let borrowed = gs.borrow();
                    if let Some(gs) = borrowed.as_any().downcast_ref::<CpdfDictionary>() {
                        assert!(!name.is_empty());
                        self.cur_states
                            .mutable_general_state()
                            .append_graphics_resource_name(name);
                        let page_data = Rc::clone(&self.page_data);
                        let needs_background = self.cur_states.process_ext_gs(gs, |font_name| {
                            Self::find_font(&page_data, holder, font_name)
                        });
                        if needs_background {
                            holder.set_background_alpha_needed(true);
                        }
                    }
                }
            }
            b"cs" => {
                if let Some(space) = self.find_color_space(holder, &self.string(0)) {
                    self.cur_states
                        .mutable_color_state()
                        .get_mutable_fill_color()
                        .set_color_space(space);
                }
            }
            b"CS" => {
                if let Some(space) = self.find_color_space(holder, &self.string(0)) {
                    self.cur_states
                        .mutable_color_state()
                        .get_mutable_stroke_color()
                        .set_color_space(space);
                }
            }
            b"sc" => self
                .cur_states
                .mutable_color_state()
                .set_fill_color(None, color_values),
            b"SC" => self
                .cur_states
                .mutable_color_state()
                .set_stroke_color(None, color_values),
            b"scn" => self.set_pattern_color(true, holder),
            b"SCN" => self.set_pattern_color(false, holder),
            b"g" => self
                .cur_states
                .mutable_color_state()
                .set_fill_color(get_stock_cs_for_name(b"DeviceGray"), color_values),
            b"G" => self
                .cur_states
                .mutable_color_state()
                .set_stroke_color(get_stock_cs_for_name(b"DeviceGray"), color_values),
            b"rg" if n == 3 => self
                .cur_states
                .mutable_color_state()
                .set_fill_color(get_stock_cs_for_name(b"DeviceRGB"), color_values),
            b"RG" if n == 3 => self
                .cur_states
                .mutable_color_state()
                .set_stroke_color(get_stock_cs_for_name(b"DeviceRGB"), color_values),
            b"k" if n == 4 => self
                .cur_states
                .mutable_color_state()
                .set_fill_color(get_stock_cs_for_name(b"DeviceCMYK"), color_values),
            b"K" if n == 4 => self
                .cur_states
                .mutable_color_state()
                .set_stroke_color(get_stock_cs_for_name(b"DeviceCMYK"), color_values),
            b"m" if n == 2 => self.add_path_point(
                CfxPointF::new(self.number(1), self.number(0)),
                PointType::Move,
            ),
            b"l" if n == 2 => self.add_path_point(
                CfxPointF::new(self.number(1), self.number(0)),
                PointType::Line,
            ),
            b"c" => {
                for index in [4, 2, 0] {
                    self.add_path_point(
                        CfxPointF::new(self.number(index + 1), self.number(index)),
                        PointType::Bezier,
                    );
                }
            }
            b"v" => {
                self.add_path_point(self.path_current, PointType::Bezier);
                for index in [2, 0] {
                    self.add_path_point(
                        CfxPointF::new(self.number(index + 1), self.number(index)),
                        PointType::Bezier,
                    );
                }
            }
            b"y" => {
                self.add_path_point(
                    CfxPointF::new(self.number(3), self.number(2)),
                    PointType::Bezier,
                );
                let point = CfxPointF::new(self.number(1), self.number(0));
                self.add_path_point(point, PointType::Bezier);
                self.add_path_point(point, PointType::Bezier);
            }
            b"re" => self.add_path_rect(
                self.number(3),
                self.number(2),
                self.number(1),
                self.number(0),
            ),
            b"h" => self.close_path(),
            b"W" => self.path_clip_type = FillType::Winding,
            b"W*" => self.path_clip_type = FillType::EvenOdd,
            b"S" => self.add_path_object(FillType::NoFill, true, stream_index, holder),
            b"s" => {
                self.close_path();
                self.add_path_object(FillType::NoFill, true, stream_index, holder);
            }
            b"f" | b"F" => self.add_path_object(FillType::Winding, false, stream_index, holder),
            b"f*" => self.add_path_object(FillType::EvenOdd, false, stream_index, holder),
            b"B" => self.add_path_object(FillType::Winding, true, stream_index, holder),
            b"B*" => self.add_path_object(FillType::EvenOdd, true, stream_index, holder),
            b"b" => {
                self.close_path();
                self.add_path_object(FillType::Winding, true, stream_index, holder);
            }
            b"b*" => {
                self.close_path();
                self.add_path_object(FillType::EvenOdd, true, stream_index, holder);
            }
            b"n" => self.add_path_object(FillType::NoFill, false, stream_index, holder),
            _ => {}
        }
    }
    fn parse_path_object(&mut self, syntax: &mut CpdfStreamParser<'_>) {
        #[cfg(feature = "render-profile")]
        let _profile = crate::render_profile::Scope::new("parse path operators");
        let mut params = [0.0; 6];
        let mut count = 0;
        let mut last_pos = syntax.get_pos();
        loop {
            let kind = syntax.parse_next_element();
            let mut processed = true;
            match kind {
                ElementType::EndOfData => return,
                ElementType::Number => {
                    if count < 6 {
                        params[count] = FxNumber::from_string(syntax.get_word()).get_float();
                        count += 1;
                    }
                }
                ElementType::Keyword => {
                    match syntax.get_word() {
                        b"m" => self
                            .add_path_point(CfxPointF::new(params[0], params[1]), PointType::Move),
                        b"l" => self
                            .add_path_point(CfxPointF::new(params[0], params[1]), PointType::Line),
                        b"c" => {
                            for index in [0, 2, 4] {
                                self.add_path_point(
                                    CfxPointF::new(params[index], params[index + 1]),
                                    PointType::Bezier,
                                );
                            }
                        }
                        b"v" => {
                            self.add_path_point(self.path_current, PointType::Bezier);
                            self.add_path_point(
                                CfxPointF::new(params[0], params[1]),
                                PointType::Bezier,
                            );
                            self.add_path_point(
                                CfxPointF::new(params[2], params[3]),
                                PointType::Bezier,
                            );
                        }
                        b"y" => {
                            self.add_path_point(
                                CfxPointF::new(params[0], params[1]),
                                PointType::Bezier,
                            );
                            for _ in 0..2 {
                                self.add_path_point(
                                    CfxPointF::new(params[2], params[3]),
                                    PointType::Bezier,
                                );
                            }
                        }
                        b"h" => self.close_path(),
                        b"re" => self.add_path_rect(params[0], params[1], params[2], params[3]),
                        _ => processed = false,
                    }
                    if processed {
                        count = 0;
                        last_pos = syntax.get_pos();
                    }
                }
                _ => processed = false,
            }
            if !processed {
                syntax.set_pos(last_pos);
                return;
            }
        }
    }
    pub fn parse(
        &mut self,
        data: &[u8],
        start_offset: usize,
        max_cost: usize,
        stream_offsets: &[u32],
        holder: &mut CpdfPageObjectHolder,
    ) -> usize {
        #[cfg(feature = "render-profile")]
        let _profile = crate::render_profile::Scope::new("parse content other");
        if start_offset >= data.len() {
            return 0;
        }
        let input = &data[start_offset..];
        let identity = input.as_ptr() as usize;
        {
            let mut parsed = self.form_visited.borrow_mut();
            if parsed.len() > 40 || parsed.contains(&identity) {
                return input.len();
            }
            parsed.insert(identity);
        }
        let _entry = ParsedStreamEntry {
            parsed: Rc::clone(&self.form_visited),
            identity,
        };
        let pool = self
            .page_data
            .document_objects()
            .map(|objects| objects.borrow().byte_string_pool())
            .unwrap_or_default();
        let mut syntax = CpdfStreamParser::with_pool(&data[start_offset..], pool);
        let initial_objects = holder.page_object_count();
        loop {
            if max_cost != 0 && holder.page_object_count() - initial_objects >= max_cost {
                break;
            }
            match syntax.parse_next_element() {
                ElementType::EndOfData => break,
                ElementType::Keyword => {
                    let op = syntax.get_word().to_vec();
                    let index = Self::stream_index(
                        start_offset + syntax.get_pos() as usize,
                        stream_offsets,
                    );
                    if op == b"BI" {
                        self.handle_begin_image(&mut syntax, start_offset, stream_offsets, holder);
                    } else {
                        self.operator(&op, index, holder);
                        if op == b"m" && self.param_count == 2 {
                            self.parse_path_object(&mut syntax);
                        }
                    }
                    self.clear_all_params();
                }
                ElementType::Number => self.push_param(ContentParam::Number(
                    FxNumber::from_string(syntax.get_word()),
                )),
                ElementType::Name => self.push_param(ContentParam::Name(pdf_name_decode(
                    syntax.get_word().get(1..).unwrap_or_default(),
                ))),
                ElementType::Other => self.push_param(ContentParam::Object(syntax.get_object())),
            }
        }
        syntax.get_pos() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_textstate::TextRenderingMode;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;

    fn float_array(values: &[f32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_float(value));
        }
        array
    }

    #[test]
    fn operand_conversion_and_full_parameter_buffer_follow_native_order() {
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        parser.push_param(ContentParam::Number(FxNumber::from_string(b"1.9")));
        assert_eq!(parser.integer(0), Some(1));
        let object = parser.object(0).unwrap();
        assert!(Rc::ptr_eq(&object, &parser.object(0).unwrap()));
        assert_eq!(parser.integer(0), Some(1));
        parser.clear_all_params();
        assert_eq!(parser.integer(0), Some(0));
        for value in 0..17 {
            parser.push_param(ContentParam::Number(FxNumber::from_string(
                value.to_string().as_bytes(),
            )));
        }
        assert_eq!(parser.param_count, 16);
        // GetNextParamPos advances the start before overwriting that slot.
        assert_eq!(parser.number(0), 0.0);
        assert_eq!(parser.number(15), 16.0);
        parser.clear_all_params();
        parser.push_param(ContentParam::Number(FxNumber::from_string(b"2147483648")));
        assert_eq!(parser.integer(0), None);
    }

    #[test]
    fn path_fast_loop_retains_unused_coordinate_slots_and_rewinds_at_paint() {
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        let input = b"1 2 m 3 4 5 l 9 l S 2 w";
        let consumed = parser.parse(input, 0, 1, &[0], &mut holder);
        assert_eq!(&input[consumed..], b" 2 w");
        let object = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfPathObject>()
            .unwrap();
        let points = object.path().points();
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].point, CfxPointF::new(1.0, 2.0));
        assert_eq!(points[1].point, CfxPointF::new(3.0, 4.0));
        assert_eq!(points[2].point, CfxPointF::new(9.0, 4.0));
        assert!(parser.form_visited.borrow().is_empty());
        parser
            .form_visited
            .borrow_mut()
            .insert(input.as_ptr() as usize);
        assert_eq!(parser.parse(input, 0, 1, &[0], &mut holder), input.len());
        assert_eq!(holder.page_object_count(), 1);
    }

    #[test]
    fn form_initial_state_excludes_the_parent_clip() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"Form");
        dict.set_new_for(b"BBox", float_array(&[0.0, 0.0, 10.0, 10.0]));
        let objects = CpdfIndirectObjectHolder::new();
        let stream = objects.borrow_mut().add_indirect_object(retain_object(
            CpdfStream::from_data_and_dict(b"0 0 1 1 re f".to_vec(), retain_object(dict)),
        ));
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Fm", CpdfReference::new(&objects, stream));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"XObject", xobjects);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        parser.parse(b"0 0 .5 .5 re W n /Fm Do", 0, 0, &[0], &mut holder);
        let form = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfFormObject>()
            .unwrap();
        assert!(form.base().clip_path().has_ref());
        assert!(!form
            .form()
            .holder()
            .page_object_by_index(0)
            .unwrap()
            .base()
            .clip_path()
            .has_ref());
    }

    #[test]
    fn parses_stroked_and_filled_paths_with_state_stack() {
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        let bytes = b"q 2 w 1 0 0 rg 1 2 3 4 re B Q 5 6 m 8 9 l S";
        let first = parser.parse(bytes, 0, 1, &[0], &mut holder);
        assert_eq!(first, 27);
        assert_eq!(holder.page_object_count(), 1);
        assert_eq!(
            holder
                .page_object_by_index(0)
                .unwrap()
                .base()
                .color_state()
                .get_fill_color_ref(),
            0x0000_00ff
        );
        assert_eq!(
            holder
                .page_object_by_index(0)
                .unwrap()
                .base()
                .graph_state()
                .get_line_width(),
            2.0
        );
        let consumed = parser.parse(bytes, first, 0, &[0], &mut holder);
        assert_eq!(consumed, bytes.len() - first);
        assert_eq!(holder.page_object_count(), 2);
        assert_eq!(
            holder
                .page_object_by_index(1)
                .unwrap()
                .base()
                .graph_state()
                .get_line_width(),
            1.0
        );
        assert_eq!(
            holder.page_object_by_index(0).unwrap().base().rect(),
            CfxFloatRect::new(-1.0, 0.0, 6.0, 8.0)
        );
    }

    #[test]
    fn named_ext_gs_updates_emitted_path_and_page_alpha_need() {
        let mut gs = CpdfDictionary::new();
        gs.set_new_for(b"LW", CpdfScalarObject::number_from_float(2.0));
        gs.set_new_for(b"ca", CpdfScalarObject::number_from_float(0.5));
        gs.set_new_name_for(b"BM", b"Screen");
        let mut named = CpdfDictionary::new();
        named.set_new_for(b"GS1", gs);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"ExtGState", named);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        let bytes = b"/GS1 gs 0 0 10 10 re B";
        assert_eq!(parser.parse(bytes, 0, 0, &[0], &mut holder), bytes.len());
        let object = holder.page_object_by_index(0).unwrap().base();
        assert_eq!(object.graph_state().get_line_width(), 2.0);
        assert_eq!(object.general_state().get_fill_alpha(), 0.5);
        assert!(holder.background_alpha_needed());
    }

    #[test]
    fn named_color_space_is_resolved_from_resources() {
        let mut spaces = CpdfDictionary::new();
        spaces.set_new_name_for(b"Alias", b"DeviceRGB");
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"ColorSpace", spaces);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        let bytes = b"/Alias cs 1 0 0 sc 0 0 2 2 re f";
        parser.parse(bytes, 0, 0, &[0], &mut holder);
        assert_eq!(
            holder
                .page_object_by_index(0)
                .unwrap()
                .base()
                .color_state()
                .get_fill_color_ref(),
            0x0000_00ff
        );
    }

    #[test]
    fn marked_content_is_copied_to_emitted_paths() {
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        let bytes = b"/Span << /MCID 3 >> BDC 0 0 2 2 re f EMC 3 0 2 2 re f";
        parser.parse(bytes, 0, 0, &[0], &mut holder);
        assert_eq!(holder.page_object_count(), 2);
        assert_eq!(
            holder
                .page_object_by_index(0)
                .unwrap()
                .base()
                .content_marks()
                .marked_content_id(),
            3
        );
        assert_eq!(
            holder
                .page_object_by_index(1)
                .unwrap()
                .base()
                .content_marks()
                .count_items(),
            0
        );
    }

    #[test]
    fn repeated_image_xobject_reuses_cached_image() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"Image");
        dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(3));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            vec![0; 18],
            retain_object(dict),
        ));
        let obj_num = objects.borrow_mut().add_indirect_object(stream);
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Im1", CpdfReference::new(&objects, obj_num));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"XObject", xobjects);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        parser.parse(b"/Im1 Do /Im1 Do", 0, 0, &[0], &mut holder);
        assert_eq!(holder.page_object_count(), 2);
        let first = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfImageObject>()
            .unwrap();
        let second = holder
            .page_object_by_index(1)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfImageObject>()
            .unwrap();
        assert!(Rc::ptr_eq(
            &first.image().unwrap(),
            &second.image().unwrap()
        ));
        assert_eq!(first.image().unwrap().pixel_height(), 3);
    }

    #[test]
    fn inline_image_abbreviations_emit_image_object() {
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        let bytes = b"BI /W 1 /H 1 /BPC 8 /CS /G ID \x7f EI";
        assert_eq!(parser.parse(bytes, 0, 0, &[0], &mut holder), bytes.len());
        let image = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfImageObject>()
            .unwrap()
            .image()
            .unwrap();
        assert_eq!((image.pixel_width(), image.pixel_height()), (1, 1));
        assert!(image.is_inline());
        assert!(!image.is_mask());
    }

    #[test]
    fn inline_images_share_document_key_and_name_storage() {
        use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
        let objects = CpdfIndirectObjectHolder::new();
        let pool = objects.borrow().byte_string_pool();
        let page_data = Rc::new(CpdfDocPageData::new());
        page_data.bind_document_resources(&objects, None);
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        let mut parser =
            CpdfStreamContentParser::with_page_data(None, CfxMatrix::default(), page_data);
        let content = b"BI /W 1 /H 1 /BPC 8 /CS /G /DP <</Columns 1>> ID \x7f EI \
                        BI /W 1 /H 1 /BPC 8 /CS /G /DP <</Columns 1>> ID \x40 EI";
        assert_eq!(
            parser.parse(content, 0, 0, &[0], &mut holder),
            content.len()
        );
        assert_eq!(holder.page_object_count(), 2);
        let mut previous_key = None;
        let expected_name = pool.upgrade().unwrap().borrow_mut().intern(b"DeviceGray");
        for index in 0..2 {
            let image = holder
                .page_object_by_index(index)
                .unwrap()
                .as_any()
                .downcast_ref::<CpdfImageObject>()
                .unwrap()
                .image()
                .unwrap();
            let dictionary = image.dict();
            let borrowed = dictionary.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            assert!(std::rc::Weak::ptr_eq(&dict.byte_string_pool(), &pool));
            let key = dict.with_locked_entries(|entries| {
                Rc::clone(entries.get_key_value(b"Width".as_slice()).unwrap().0)
            });
            if let Some(previous) = previous_key.replace(key.clone()) {
                assert!(Rc::ptr_eq(&previous, &key));
            }
            let name = dict.get_object_for(b"ColorSpace").unwrap();
            let name = name.borrow();
            let name = name.as_any().downcast_ref::<CpdfName>().unwrap();
            assert!(std::ptr::eq(name.get_string(), expected_name.as_ref()));
            let params = dict.get_dict_for(b"DecodeParms").unwrap();
            let params = params.borrow();
            let params = params.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            assert!(std::rc::Weak::ptr_eq(&params.byte_string_pool(), &pool));
        }
    }

    #[test]
    fn inline_resource_snapshot_and_abbreviation_collision_order() {
        let mut dict = CpdfDictionary::new();
        let gray = dict.set_new_name_for(b"CS", b"G");
        dict.set_new_name_for(b"ColorSpace", b"RGB");
        let dict = retain_object(dict);
        CpdfStreamContentParser::expand_inline_abbreviations(&dict);
        assert_eq!(gray.borrow().get_string(), b"G");
        assert_eq!(
            dict.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_byte_string_for(b"ColorSpace", b""),
            b"DeviceRGB"
        );

        let mut spaces = CpdfDictionary::new();
        let mut array = CpdfArray::new();
        array.append_new_name(b"DeviceRGB");
        let original = spaces.set_new_for(b"Named", array);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"ColorSpace", spaces);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        let bytes = b"BI /W 1 /H 1 /BPC 8 /CS /Named ID \x7f\x40\x20 EI";
        parser.parse(bytes, 0, 0, &[0, 10], &mut holder);
        let image = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfImageObject>()
            .unwrap();
        assert_eq!(image.base().content_stream(), 1);
        let image = image.image().unwrap();
        let image_stream = image.stream();
        let stream = image_stream.borrow();
        let dict = stream
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_dict();
        let snapshot = dict
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_array_for(b"ColorSpace")
            .unwrap();
        assert!(!Rc::ptr_eq(&original, &snapshot));
    }

    #[test]
    fn axial_shading_uses_holder_bbox() {
        let mut function = CpdfDictionary::new();
        function.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(2));
        function.set_new_for(b"Domain", float_array(&[0.0, 1.0]));
        function.set_new_for(b"C0", float_array(&[0.0]));
        function.set_new_for(b"C1", float_array(&[1.0]));
        function.set_new_for(b"N", CpdfScalarObject::number_from_signed(1));
        let mut shading = CpdfDictionary::new();
        shading.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(2));
        shading.set_new_name_for(b"ColorSpace", b"DeviceGray");
        shading.set_new_for(b"Function", function);
        let mut shadings = CpdfDictionary::new();
        shadings.set_new_for(b"S1", shading);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Shading", shadings);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        holder.set_bbox(CfxFloatRect::new(0.0, 0.0, 100.0, 80.0));
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        parser.parse(b"/S1 sh", 0, 0, &[0], &mut holder);
        assert_eq!(holder.page_object_count(), 1);
        let object = holder.page_object_by_index(0).unwrap();
        assert_eq!(
            object.object_type(),
            crate::fpdfapi::page::cpdf_pageobject::PageObjectType::Shading
        );
        assert_eq!(object.base().rect(), holder.bbox());
    }

    #[test]
    fn mesh_shading_intersects_decoded_bounds() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut mesh = CpdfDictionary::new();
        mesh.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(5));
        mesh.set_new_name_for(b"ColorSpace", b"DeviceGray");
        mesh.set_new_for(
            b"BitsPerCoordinate",
            CpdfScalarObject::number_from_signed(8),
        );
        mesh.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        mesh.set_new_for(b"Decode", float_array(&[0.0, 100.0, 0.0, 100.0, 0.0, 1.0]));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            vec![51, 102, 255, 204, 153, 0],
            retain_object(mesh),
        ));
        let obj_num = objects.borrow_mut().add_indirect_object(stream);
        let mut shadings = CpdfDictionary::new();
        shadings.set_new_for(b"S1", CpdfReference::new(&objects, obj_num));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Shading", shadings);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        holder.set_bbox(CfxFloatRect::new(30.0, 30.0, 100.0, 100.0));
        CpdfStreamContentParser::new(None, CfxMatrix::default()).parse(
            b"/S1 sh",
            0,
            0,
            &[0],
            &mut holder,
        );
        assert_eq!(holder.page_object_count(), 1);
        assert_eq!(
            holder.page_object_by_index(0).unwrap().base().rect(),
            CfxFloatRect::new(30.0, 40.0, 80.0, 60.0)
        );
    }

    #[test]
    fn form_xobject_parses_content_with_form_and_parent_matrices() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut form_dict = CpdfDictionary::new();
        form_dict.set_new_name_for(b"Subtype", b"Form");
        form_dict.set_new_for(b"BBox", float_array(&[0.0, 0.0, 10.0, 10.0]));
        form_dict.set_new_for(b"Matrix", float_array(&[2.0, 0.0, 0.0, 2.0, 3.0, 4.0]));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            b"0 0 2 2 re f".to_vec(),
            retain_object(form_dict),
        ));
        let number = objects.borrow_mut().add_indirect_object(stream);
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Fm1", CpdfReference::new(&objects, number));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"XObject", xobjects);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        CpdfStreamContentParser::new(None, CfxMatrix::default()).parse(
            b"1 0 0 1 5 6 cm /Fm1 Do",
            0,
            0,
            &[0],
            &mut holder,
        );
        let object = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfFormObject>()
            .unwrap();
        assert_eq!(object.form().holder().page_object_count(), 1);
        assert_eq!(
            object.form().calc_bounding_box(),
            CfxFloatRect::new(3.0, 4.0, 7.0, 8.0)
        );
        assert_eq!(
            object.base().rect(),
            CfxFloatRect::new(8.0, 10.0, 12.0, 14.0)
        );
    }

    #[test]
    fn self_referencing_form_stops_recursive_parse() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut form_dict = CpdfDictionary::new();
        form_dict.set_new_name_for(b"Subtype", b"Form");
        let stream = retain_object(CpdfStream::from_data_and_dict(
            b"/Fm1 Do".to_vec(),
            retain_object(form_dict),
        ));
        let number = objects.borrow_mut().add_indirect_object(stream.clone());
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Fm1", CpdfReference::new(&objects, number));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"XObject", xobjects);
        let resources = retain_object(resources);
        let dict = {
            let borrowed = stream.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfStream>()
                .unwrap()
                .get_dict()
        };
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"Resources", Some(resources.clone()));
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, Some(resources));
        CpdfStreamContentParser::new(None, CfxMatrix::default()).parse(
            b"/Fm1 Do",
            0,
            0,
            &[0],
            &mut holder,
        );
        let outer = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfFormObject>()
            .unwrap();
        assert_eq!(outer.form().holder().page_object_count(), 1);
        let inner = outer
            .form()
            .holder()
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfFormObject>()
            .unwrap();
        assert_eq!(inner.form().holder().page_object_count(), 0);
    }

    #[test]
    fn text_state_operators_compose_position_and_font_resource() {
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", CpdfDictionary::new());
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        parser.parse(
            b"BT 2 Tc 3 Tw 120 Tz 1 0 0 1 5 6 Tm 10 20 Td 0 -12 TD T* 5 Ts 2 Tr /F1 14 Tf",
            0,
            0,
            &[0],
            &mut holder,
        );
        let state = parser.cur_states();
        assert_eq!(
            state.get_transformed_text_position(),
            CfxPointF::new(15.0, 7.0)
        );
        assert_eq!(state.text_state().get_matrix(), &[1.2, 0.0, 0.0, 1.0]);
        assert_eq!(state.text_state().get_char_space(), 2.0);
        assert_eq!(state.text_state().get_word_space(), 3.0);
        assert_eq!(state.text_state().get_font_size(), 14.0);
        assert!(state.text_state().get_font_dict().is_some());
        assert_eq!(
            state.text_state().get_text_mode(),
            TextRenderingMode::FillStroke
        );
        parser.parse(b"2 0 0 2 0 0 cm", 0, 0, &[0], &mut holder);
        assert_eq!(
            parser.cur_states().text_state().get_matrix(),
            &[2.4, 0.0, 0.0, 2.0]
        );
    }

    #[test]
    fn text_advances_use_retained_embedded_and_scaled_type3_fonts() {
        let objects = CpdfIndirectObjectHolder::new();
        let data =
            include_bytes!("../../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let stream = objects.borrow_mut().new_indirect_stream_from_bytes(data);
        let mut descriptor = CpdfDictionary::new();
        descriptor.set_new_for(
            b"FontFile2",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        descriptor.set_new_for(b"Flags", CpdfScalarObject::number_from_signed(32));
        let mut embedded = CpdfDictionary::new();
        embedded.set_new_name_for(b"Subtype", b"TrueType");
        embedded.set_new_for(b"FontDescriptor", descriptor);
        let mut type3 = CpdfDictionary::new();
        type3.set_new_name_for(b"Subtype", b"Type3");
        type3.set_matrix_for(
            b"FontMatrix",
            CfxMatrix::new(0.002, 0.0, 0.0, 0.001, 0.0, 0.0),
        );
        type3.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        type3.set_new_for(b"Widths", float_array(&[500.0]));
        for font in [embedded, type3] {
            let mut fonts = CpdfDictionary::new();
            fonts.set_new_for(b"F1", font);
            let mut resources = CpdfDictionary::new();
            resources.set_new_for(b"Font", fonts);
            let mut holder = CpdfPageObjectHolder::new(
                retain_object(CpdfDictionary::new()),
                None,
                Some(retain_object(resources)),
            );
            let page_data = Rc::new(CpdfDocPageData::new());
            let mut parser = CpdfStreamContentParser::with_page_data(
                None,
                CfxMatrix::default(),
                Rc::clone(&page_data),
            );
            parser.parse(b"BT /F1 10 Tf (AA) Tj (A) Tj ET", 0, 0, &[0], &mut holder);
            let first = holder
                .page_object_by_index(0)
                .unwrap()
                .as_any()
                .downcast_ref::<CpdfTextObject>()
                .unwrap();
            let second = holder
                .page_object_by_index(1)
                .unwrap()
                .as_any()
                .downcast_ref::<CpdfTextObject>()
                .unwrap();
            assert_eq!(first.char_positions(), &[0.0, 10.0]);
            assert_eq!(second.position(), CfxPointF::new(20.0, 0.0));
            assert_eq!(
                first
                    .base()
                    .text_state()
                    .get_font()
                    .unwrap()
                    .get_char_width(65),
                1000
            );
            let dict = first.base().text_state().get_font_dict().unwrap();
            let cached =
                with_render_font_mapper(|mapper| page_data.get_font(&dict, mapper)).unwrap();
            let mut detached_state = first.base().text_state().clone();
            detached_state.set_font_size(12.0);
            assert_eq!(first.base().text_state().get_font_size(), 10.0);
            match (detached_state.get_font().unwrap(), cached) {
                (CpdfFont::Simple(first), CpdfFont::Simple(cached)) => {
                    assert!(Rc::ptr_eq(first, &cached))
                }
                (CpdfFont::Type3(first), CpdfFont::Type3(cached)) => {
                    assert!(Rc::ptr_eq(first, &cached))
                }
                _ => panic!("font factory changed font kind"),
            }
        }
    }

    #[test]
    fn text_bounds_use_face_metrics_kerning_and_stroke_ctm() {
        let objects = CpdfIndirectObjectHolder::new();
        let stream = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(include_bytes!("../../../tests/fixtures/triangle.pfa"));
        let mut descriptor = CpdfDictionary::new();
        descriptor.set_new_for(
            b"FontFile",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        descriptor.set_new_for(b"Flags", CpdfScalarObject::number_from_signed(32));
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"Type1");
        font.set_new_for(b"FontDescriptor", descriptor);
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        parser.parse(
            b"2 0 0 3 10 20 cm 2 w BT /F1 10 Tf 1 Tc 1 Tr [(A) 100 (A)] TJ ET",
            0,
            0,
            &[0],
            &mut holder,
        );
        let text = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        assert_eq!(text.char_positions(), &[0.0, 6.0]);
        assert_eq!(text.char_kernings(), &[100.0, 0.0]);
        assert_eq!(
            text.base().original_rect(),
            CfxFloatRect::new(0.0, 0.0, 11.0, 7.0)
        );
        assert_eq!(text.base().rect(), CfxFloatRect::new(9.0, 19.0, 33.0, 42.0));
        assert_eq!(text.base().text_state().get_ctm(), &[2.0, 0.0, 0.0, 3.0]);
        let mut snapshot = text.clone();
        assert_eq!(snapshot.base().content_stream(), -1);
        assert_eq!(snapshot.base().rect(), text.base().rect());
        assert_eq!(snapshot.base().original_rect(), CfxFloatRect::default());
        assert_eq!(
            snapshot.calc_position_data(0.8),
            CfxPointF::new(10.400001, 0.0)
        );
        snapshot.transform(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 5.0, -3.0));
        assert_eq!(
            snapshot.base().rect(),
            CfxFloatRect::new(14.0, 16.0, 38.0, 39.0)
        );
    }

    #[test]
    fn ext_gs_and_missing_font_selection_retain_loaded_fonts() {
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"Type1");
        font.set_new_name_for(b"BaseFont", b"Courier");
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut font_setting = CpdfArray::new();
        font_setting.append_new_name(b"F1");
        font_setting.append_new(CpdfScalarObject::number_from_signed(10));
        let mut gs = CpdfDictionary::new();
        gs.set_new_for(b"Font", font_setting);
        let mut graphics_states = CpdfDictionary::new();
        graphics_states.set_new_for(b"GS1", gs);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        resources.set_new_for(b"ExtGState", graphics_states);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        CpdfStreamContentParser::new(None, CfxMatrix::default()).parse(
            b"BT /GS1 gs (AA) Tj /Missing 10 Tf (AA) Tj ET",
            0,
            0,
            &[0],
            &mut holder,
        );
        assert_eq!(holder.page_object_count(), 2);
        let first = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        let second = holder
            .page_object_by_index(1)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        assert_eq!(first.char_positions(), &[0.0, 6.0]);
        let Some(CpdfFont::Simple(font)) = second.base().text_state().get_font() else {
            panic!("missing stock font");
        };
        assert_eq!(font.base_font_name(), b"Helvetica");
        assert!(second.char_positions()[1] > 0.0);
    }

    #[test]
    fn text_show_operators_emit_positioned_page_objects() {
        let mut widths = CpdfArray::new();
        widths.append_new(CpdfScalarObject::number_from_signed(500));
        widths.append_new(CpdfScalarObject::number_from_signed(500));
        let mut font = CpdfDictionary::new();
        font.set_new_for(b"FirstChar", CpdfScalarObject::number_from_signed(65));
        font.set_new_for(b"Widths", widths);
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        parser.parse(
            b"BT /F1 10 Tf 1 0 0 1 3 4 Tm (AB) Tj [(A) 120 (B)] TJ",
            0,
            0,
            &[0],
            &mut holder,
        );
        assert_eq!(holder.page_object_count(), 2);
        let first = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        assert_eq!(first.char_codes(), &[65, 66]);
        assert_eq!(first.char_positions(), &[0.0, 5.0]);
        assert_eq!(first.position(), CfxPointF::new(3.0, 4.0));
        let second = holder
            .page_object_by_index(1)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        assert_eq!(second.position(), CfxPointF::new(13.0, 4.0));
        assert!((second.char_positions()[1] - 3.8).abs() < 0.0001);
        assert!((parser.cur_states().get_transformed_text_position().x - 21.8).abs() < 0.0001);
        parser.parse(
            b"BT 10 TL /F1 10 Tf (A) ' 3 1 (B) \"",
            0,
            0,
            &[0],
            &mut holder,
        );
        assert_eq!(holder.page_object_count(), 4);
        let quoted = holder
            .page_object_by_index(2)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        assert_eq!(quoted.position(), CfxPointF::new(0.0, -10.0));
        let spaced = holder
            .page_object_by_index(3)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        assert_eq!(spaced.position(), CfxPointF::new(0.0, -20.0));
        assert_eq!(spaced.base().text_state().get_char_space(), 1.0);
        parser.parse(
            b"BT /F1 10 Tf 0 Tc [(A) 100 ()] TJ",
            0,
            0,
            &[0],
            &mut holder,
        );
        assert_eq!(parser.cur_states().get_transformed_text_position().x, 5.0);
        assert_eq!(holder.iter().last().unwrap().base().resource_name(), b"F1");

        let count = holder.page_object_count();
        parser.parse(
            b"BT /F1 10 Tf 12 TL 1 0 0 1 5 30 Tm () Tj () ' 3 1 () \" [40 () 60] TJ",
            0,
            0,
            &[0],
            &mut holder,
        );
        // Empty strings create no text objects, but line movement, spacing
        // changes and initial TJ kerning still apply, as in Handle_ShowText().
        assert_eq!(holder.page_object_count(), count);
        assert_eq!(
            parser.cur_states().get_transformed_text_position(),
            CfxPointF::new(4.0, 6.0)
        );
        assert_eq!(parser.cur_states().text_state().get_char_space(), 1.0);
        assert_eq!(parser.cur_states().text_state().get_word_space(), 3.0);
    }

    #[test]
    fn type0_text_uses_cmap_character_boundaries_and_cid_widths() {
        let mut descendant = CpdfDictionary::new();
        descendant.set_new_name_for(b"Subtype", b"CIDFontType2");
        let mut descendants = CpdfArray::new();
        descendants.append_new(descendant);
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Subtype", b"Type0");
        font.set_new_name_for(b"Encoding", b"Identity-H");
        font.set_new_for(b"DescendantFonts", descendants);
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(b"F1", font);
        let mut vertical_descendants = CpdfArray::new();
        let mut vertical_descendant = CpdfDictionary::new();
        vertical_descendant.set_new_name_for(b"Subtype", b"CIDFontType2");
        vertical_descendants.append_new(vertical_descendant);
        let mut vertical_font = CpdfDictionary::new();
        vertical_font.set_new_name_for(b"Subtype", b"Type0");
        vertical_font.set_new_name_for(b"Encoding", b"Identity-V");
        vertical_font.set_new_for(b"DescendantFonts", vertical_descendants);
        fonts.set_new_for(b"F2", vertical_font);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        parser.parse(
            b"BT /F1 10 Tf 1 0 0 1 2 3 Tm <00410042> Tj [<0041> 200 <0042>] TJ",
            0,
            0,
            &[0],
            &mut holder,
        );
        assert_eq!(holder.page_object_count(), 2);
        let first = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        assert_eq!(first.char_codes(), &[65, 66]);
        assert_eq!(first.char_positions(), &[0.0, 10.0]);
        let second = holder
            .page_object_by_index(1)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        assert_eq!(second.position(), CfxPointF::new(22.0, 3.0));
        assert_eq!(second.char_positions(), &[0.0, 8.0]);
        assert_eq!(
            parser.cur_states().get_transformed_text_position(),
            CfxPointF::new(40.0, 3.0)
        );
        parser.parse(b"BT /F2 10 Tf <0041> Tj", 0, 0, &[0], &mut holder);
        let vertical = holder
            .page_object_by_index(2)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfTextObject>()
            .unwrap();
        assert!(vertical.is_vertical());
        assert_eq!(vertical.char_codes(), &[65]);
        assert_eq!(
            parser.cur_states().get_transformed_text_position(),
            CfxPointF::new(0.0, -10.0)
        );
    }

    #[test]
    fn form_resource_category_falls_back_to_page_resources() {
        let objects = CpdfIndirectObjectHolder::new();
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_name_for(b"Subtype", b"Image");
        image_dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        let image = retain_object(CpdfStream::from_data_and_dict(
            vec![0],
            retain_object(image_dict),
        ));
        let image_num = objects.borrow_mut().add_indirect_object(image);
        let mut form_resources = CpdfDictionary::new();
        form_resources.set_new_for(b"ExtGState", CpdfDictionary::new());
        let mut form_dict = CpdfDictionary::new();
        form_dict.set_new_name_for(b"Subtype", b"Form");
        form_dict.set_new_for(b"Resources", form_resources);
        let form = retain_object(CpdfStream::from_data_and_dict(
            b"/Im1 Do".to_vec(),
            retain_object(form_dict),
        ));
        let form_num = objects.borrow_mut().add_indirect_object(form);
        let mut xobjects = CpdfDictionary::new();
        xobjects.set_new_for(b"Im1", CpdfReference::new(&objects, image_num));
        xobjects.set_new_for(b"Fm1", CpdfReference::new(&objects, form_num));
        let mut page_resources = CpdfDictionary::new();
        page_resources.set_new_for(b"XObject", xobjects);
        let page_resources = retain_object(page_resources);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            Some(page_resources.clone()),
            Some(page_resources),
        );
        CpdfStreamContentParser::new(None, CfxMatrix::default()).parse(
            b"/Fm1 Do",
            0,
            0,
            &[0],
            &mut holder,
        );
        let form = holder
            .page_object_by_index(0)
            .unwrap()
            .as_any()
            .downcast_ref::<CpdfFormObject>()
            .unwrap();
        assert_eq!(form.form().holder().page_object_count(), 1);
        assert_eq!(
            form.form()
                .holder()
                .page_object_by_index(0)
                .unwrap()
                .object_type(),
            crate::fpdfapi::page::cpdf_pageobject::PageObjectType::Image
        );
    }

    #[test]
    fn named_pattern_colors_reuse_document_pattern() {
        let mut tile = CpdfDictionary::new();
        tile.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(1));
        tile.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(1));
        let mut patterns = CpdfDictionary::new();
        patterns.set_new_for(b"P1", tile);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Pattern", patterns);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        CpdfStreamContentParser::new(None, CfxMatrix::default()).parse(
            b"/Pattern cs /P1 scn 0 0 2 2 re f /P1 scn 3 0 2 2 re f",
            0,
            0,
            &[0],
            &mut holder,
        );
        assert_eq!(holder.page_object_count(), 2);
        let first = holder
            .page_object_by_index(0)
            .unwrap()
            .base()
            .color_state()
            .get_fill_color()
            .unwrap()
            .get_pattern()
            .unwrap();
        let second = holder
            .page_object_by_index(1)
            .unwrap()
            .base()
            .color_state()
            .get_fill_color()
            .unwrap()
            .get_pattern()
            .unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert!(first.colored_tiling());
    }

    #[test]
    fn uncolored_pattern_uses_preceding_color_components() {
        let mut tile = CpdfDictionary::new();
        tile.set_new_for(b"PatternType", CpdfScalarObject::number_from_signed(1));
        tile.set_new_for(b"PaintType", CpdfScalarObject::number_from_signed(2));
        let mut patterns = CpdfDictionary::new();
        patterns.set_new_for(b"P1", tile);
        let mut pattern_cs = CpdfArray::new();
        pattern_cs.append_new_name(b"Pattern");
        pattern_cs.append_new_name(b"DeviceRGB");
        let mut color_spaces = CpdfDictionary::new();
        color_spaces.set_new_for(b"PatRGB", pattern_cs);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Pattern", patterns);
        resources.set_new_for(b"ColorSpace", color_spaces);
        let mut holder = CpdfPageObjectHolder::new(
            retain_object(CpdfDictionary::new()),
            None,
            Some(retain_object(resources)),
        );
        let mut parser = CpdfStreamContentParser::new(None, CfxMatrix::default());
        parser.parse(b"/PatRGB cs 0.25 0.5 0.75 /P1 scn", 0, 0, &[0], &mut holder);
        let rgb = parser
            .cur_states()
            .color_state()
            .get_fill_color()
            .unwrap()
            .get_rgb()
            .unwrap();
        assert_eq!((rgb.red, rgb.green, rgb.blue), (0.25, 0.5, 0.75));
    }
}

#[cfg(test)]
#[path = "cpdf_streamcontentparser_unittest.rs"]
mod upstream_tests;
