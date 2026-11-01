// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Appearance generation, caching, popup state and geometry from
//! `core/fpdfdoc/cpdf_annot.{h,cpp}`.

use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::page::cpdf_form::CpdfForm;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::render::cpdf_rendercontext::CpdfRenderContext;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Subtype {
    Unknown = 0,
    Text,
    Link,
    FreeText,
    Line,
    Square,
    Circle,
    Polygon,
    PolyLine,
    Highlight,
    Underline,
    Squiggly,
    StrikeOut,
    Stamp,
    Caret,
    Ink,
    Popup,
    FileAttachment,
    Sound,
    Movie,
    Widget,
    Screen,
    PrinterMark,
    TrapNet,
    Watermark,
    ThreeD,
    RichMedia,
    XfaWidget,
    Redact,
}
impl Subtype {
    /// Numeric public ABI values. Invalid integers remain outside the Rust enum.
    pub fn from_i32(value: i32) -> Option<Self> {
        const TYPES: [Subtype; 29] = [
            Subtype::Unknown,
            Subtype::Text,
            Subtype::Link,
            Subtype::FreeText,
            Subtype::Line,
            Subtype::Square,
            Subtype::Circle,
            Subtype::Polygon,
            Subtype::PolyLine,
            Subtype::Highlight,
            Subtype::Underline,
            Subtype::Squiggly,
            Subtype::StrikeOut,
            Subtype::Stamp,
            Subtype::Caret,
            Subtype::Ink,
            Subtype::Popup,
            Subtype::FileAttachment,
            Subtype::Sound,
            Subtype::Movie,
            Subtype::Widget,
            Subtype::Screen,
            Subtype::PrinterMark,
            Subtype::TrapNet,
            Subtype::Watermark,
            Subtype::ThreeD,
            Subtype::RichMedia,
            Subtype::XfaWidget,
            Subtype::Redact,
        ];
        usize::try_from(value)
            .ok()
            .and_then(|index| TYPES.get(index))
            .copied()
    }
    pub fn from_pdf(name: &[u8]) -> Self {
        match crate::fxcrt::bytestring::c_str(name) {
            b"Text" => Self::Text,
            b"Link" => Self::Link,
            b"FreeText" => Self::FreeText,
            b"Line" => Self::Line,
            b"Square" => Self::Square,
            b"Circle" => Self::Circle,
            b"Polygon" => Self::Polygon,
            b"PolyLine" => Self::PolyLine,
            b"Highlight" => Self::Highlight,
            b"Underline" => Self::Underline,
            b"Squiggly" => Self::Squiggly,
            b"StrikeOut" => Self::StrikeOut,
            b"Stamp" => Self::Stamp,
            b"Caret" => Self::Caret,
            b"Ink" => Self::Ink,
            b"Popup" => Self::Popup,
            b"FileAttachment" => Self::FileAttachment,
            b"Sound" => Self::Sound,
            b"Movie" => Self::Movie,
            b"Widget" => Self::Widget,
            b"Screen" => Self::Screen,
            b"PrinterMark" => Self::PrinterMark,
            b"TrapNet" => Self::TrapNet,
            b"Watermark" => Self::Watermark,
            b"3D" => Self::ThreeD,
            b"RichMedia" => Self::RichMedia,
            b"XFAWidget" => Self::XfaWidget,
            b"Redact" => Self::Redact,
            _ => Self::Unknown,
        }
    }
    pub fn as_pdf(self) -> &'static [u8] {
        match self {
            Self::Unknown => b"",
            Self::Text => b"Text",
            Self::Link => b"Link",
            Self::FreeText => b"FreeText",
            Self::Line => b"Line",
            Self::Square => b"Square",
            Self::Circle => b"Circle",
            Self::Polygon => b"Polygon",
            Self::PolyLine => b"PolyLine",
            Self::Highlight => b"Highlight",
            Self::Underline => b"Underline",
            Self::Squiggly => b"Squiggly",
            Self::StrikeOut => b"StrikeOut",
            Self::Stamp => b"Stamp",
            Self::Caret => b"Caret",
            Self::Ink => b"Ink",
            Self::Popup => b"Popup",
            Self::FileAttachment => b"FileAttachment",
            Self::Sound => b"Sound",
            Self::Movie => b"Movie",
            Self::Widget => b"Widget",
            Self::Screen => b"Screen",
            Self::PrinterMark => b"PrinterMark",
            Self::TrapNet => b"TrapNet",
            Self::Watermark => b"Watermark",
            Self::ThreeD => b"3D",
            Self::RichMedia => b"RichMedia",
            Self::XfaWidget => b"XFAWidget",
            Self::Redact => b"Redact",
        }
    }
    fn is_text_markup(self) -> bool {
        matches!(
            self,
            Self::Highlight | Self::Squiggly | Self::StrikeOut | Self::Underline
        )
    }
}

pub struct CpdfAnnot {
    annot_dict: ObjectRef,
    document_data: Weak<CpdfDocPageData>,
    ap_map: HashMap<usize, (ObjectRef, CpdfForm)>,
    subtype: Subtype,
    is_text_markup: bool,
    has_generated_ap: bool,
    open_state: Rc<Cell<bool>>,
    popup: Option<(ObjectRef, Rc<Cell<bool>>)>,
}
impl Drop for CpdfAnnot {
    fn drop(&mut self) {
        self.ap_map.clear();
    }
}
impl CpdfAnnot {
    pub fn dict(&self) -> &ObjectRef {
        &self.annot_dict
    }
    pub fn new(dict: ObjectRef, data: &Rc<CpdfDocPageData>) -> Self {
        let (subtype, has_generated_ap) = {
            let borrowed = dict.borrow();
            let dict = borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .expect("annotation dictionary");
            (
                Subtype::from_pdf(&dict.get_byte_string_for(b"Subtype", b"")),
                dict.get_boolean_for(b"PDFIUM_HasGeneratedAP", false),
            )
        };
        let mut annot = Self {
            annot_dict: dict,
            document_data: Rc::downgrade(data),
            ap_map: HashMap::new(),
            subtype,
            is_text_markup: subtype.is_text_markup(),
            has_generated_ap,
            open_state: Rc::new(Cell::new(false)),
            popup: None,
        };
        annot.generate_ap_if_needed();
        annot
    }
    pub fn subtype(&self) -> Subtype {
        self.subtype
    }
    pub fn set_popup_annot(&mut self, popup: &Self) {
        self.popup = Some((Rc::clone(&popup.annot_dict), Rc::clone(&popup.open_state)));
    }
    pub fn set_popup_annot_open_state(&self, open: bool) {
        if let Some((_, state)) = &self.popup {
            state.set(open);
        }
    }
    pub fn popup_annot_rect(&self) -> Option<CfxFloatRect> {
        let (dict, _) = self.popup.as_ref()?;
        let borrowed = dict.borrow();
        Some(annot_rect(
            borrowed.as_any().downcast_ref::<CpdfDictionary>()?,
        ))
    }
    pub fn is_open(&self) -> bool {
        self.open_state.get()
    }
    pub fn set_open_state(&self, open: bool) {
        self.open_state.set(open);
    }
    pub fn clear_cached_ap(&mut self) {
        self.ap_map.clear();
    }
    pub fn flags(&self) -> u32 {
        self.annot_dict
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("annotation dictionary")
            .get_integer_for(b"F", 0) as u32
    }
    pub fn rect(&self) -> CfxFloatRect {
        let borrowed = self.annot_dict.borrow();
        let dict = borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("annotation dictionary");
        let mut rect = if self.is_text_markup && self.has_generated_ap {
            bounding_rect_from_quad_points(dict)
        } else {
            dict.get_rect_for(b"Rect")
        };
        rect.normalize();
        rect
    }
    fn should_draw_annotation(&self) -> bool {
        self.flags() & 2 == 0 && (self.subtype != Subtype::Popup || self.open_state.get())
    }
    fn should_generate_ap(&self) -> bool {
        let has_normal = {
            let borrowed = self.annot_dict.borrow();
            let dict = borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .expect("annotation dictionary");
            dict.get_dict_for(b"AP").is_some_and(|ap| {
                ap.borrow()
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .expect("AP dictionary")
                    .get_dict_for(b"N")
                    .is_some()
            })
        };
        !has_normal && self.should_draw_annotation()
    }
    pub(crate) fn generate_ap_if_needed(&mut self) {
        if !self.should_generate_ap() {
            return;
        }
        let Some(data) = self.document_data.upgrade() else {
            return;
        };
        let Some(objects) = data.document_objects() else {
            return;
        };
        let root = data.document_root();
        if !super::cpdf_generateap::generate_annot_ap_for_object(
            &objects,
            root.as_ref(),
            &data,
            &self.annot_dict,
            self.subtype,
        ) {
            return;
        }
        let mut borrowed = self.annot_dict.borrow_mut();
        let dict = borrowed
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .expect("annotation dictionary");
        dict.set_new_for(b"PDFIUM_HasGeneratedAP", CpdfScalarObject::boolean(true));
        self.has_generated_ap = true;
    }
    fn get_ap_form(&mut self, page: &CpdfPage, mode: AppearanceMode) -> Option<&CpdfForm> {
        let stream = {
            let borrowed = self.annot_dict.borrow();
            get_annot_ap(
                borrowed.as_any().downcast_ref::<CpdfDictionary>()?,
                mode,
                true,
            )?
        };
        let identity = Rc::as_ptr(&stream) as *const () as usize;
        if !self.ap_map.contains_key(&identity) {
            let mut form = CpdfForm::new(Rc::clone(&stream), page.resources(), None)?;
            form.parse_content(
                None,
                None,
                page.page_data_ref(),
                Rc::new(RefCell::new(HashSet::new())),
                0,
            );
            self.ap_map.insert(identity, (stream, form));
        }
        Some(&self.ap_map.get(&identity)?.1)
    }
    pub(super) fn draw_in_context<'a>(
        &'a mut self,
        page: &CpdfPage,
        context: &mut CpdfRenderContext<'a>,
        page_to_device: CfxMatrix,
        mode: AppearanceMode,
    ) -> bool {
        if !self.should_draw_annotation() {
            return false;
        }
        self.generate_ap_if_needed();
        let rect = self.rect();
        let flags = self.flags();
        let Some(form) = self.get_ap_form(page, mode) else {
            return false;
        };
        let dict = form.holder().dict();
        let borrowed = dict.borrow();
        let dict = borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("form dictionary");
        let matrix = matrix_for_rect(page, rect, flags, dict, page_to_device);
        context.append_layer(form.holder(), matrix);
        true
    }
    pub fn draw_appearance(
        &mut self,
        page: &CpdfPage,
        driver: &mut crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        mode: AppearanceMode,
    ) -> bool {
        self.draw_appearance_with_options(page, driver, matrix, mode, None)
    }
    pub fn draw_appearance_with_options(
        &mut self,
        page: &CpdfPage,
        driver: &mut crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        mode: AppearanceMode,
        options: Option<&crate::fpdfapi::render::cpdf_renderoptions::CpdfRenderOptions>,
    ) -> bool {
        let oc = self
            .dict()
            .borrow()
            .as_dictionary()
            .and_then(|d| d.get_dict_for(b"OC"));
        if options.is_some_and(|o| !o.check_ocg_dict_visible(oc.as_ref())) {
            return false;
        }
        let mut context = CpdfRenderContext::new(page);
        if !self.draw_in_context(page, &mut context, matrix, mode) {
            return false;
        }
        context.render(driver, None, options, None)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppearanceMode {
    Normal,
    Rollover,
    Down,
}

pub fn get_annot_ap(
    annot: &CpdfDictionary,
    mode: AppearanceMode,
    fallback_to_normal: bool,
) -> Option<ObjectRef> {
    let ap = annot.get_dict_for(b"AP")?;
    let borrowed = ap.borrow();
    let ap = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
    let mut entry: &[u8] = match mode {
        AppearanceMode::Normal => b"N",
        AppearanceMode::Rollover => b"R",
        AppearanceMode::Down => b"D",
    };
    if fallback_to_normal && !ap.key_exist(entry) {
        entry = b"N";
    }
    let sub = ap.get_direct_object_for(entry)?;
    let borrowed = sub.borrow();
    if borrowed.as_any().is::<CpdfStream>() {
        drop(borrowed);
        return Some(sub);
    }
    let states = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
    let mut state = annot.get_byte_string_for(b"AS", b"");
    if state.is_empty() {
        let mut value = annot.get_byte_string_for(b"V", b"");
        if value.is_empty() {
            if let Some(parent) = annot.get_dict_for(b"Parent") {
                if let Some(parent) = parent.borrow().as_any().downcast_ref::<CpdfDictionary>() {
                    value = parent.get_byte_string_for(b"V", b"");
                }
            }
        }
        state = if !value.is_empty() && states.key_exist(&value) {
            value
        } else {
            b"Off".to_vec()
        };
    }
    states.get_stream_for(&state)
}

pub fn quad_point_count(array: &CpdfArray) -> usize {
    array.size() / 8
}

pub fn rect_from_quad_points_array(array: &CpdfArray, index: usize) -> CfxFloatRect {
    debug_assert!(index < quad_point_count(array));
    // Native uses bottom-left and top-right, not all four corners' extrema.
    CfxFloatRect::new(
        array.get_float_at(4 + index * 8),
        array.get_float_at(5 + index * 8),
        array.get_float_at(2 + index * 8),
        array.get_float_at(3 + index * 8),
    )
}

pub fn bounding_rect_from_quad_points(annot: &CpdfDictionary) -> CfxFloatRect {
    let Some(array) = annot.get_array_for(b"QuadPoints") else {
        return CfxFloatRect::default();
    };
    let borrowed = array.borrow();
    let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
        return CfxFloatRect::default();
    };
    let count = quad_point_count(array);
    if count == 0 {
        return CfxFloatRect::default();
    }
    let mut rect = rect_from_quad_points_array(array, 0);
    for index in 1..count {
        rect.union(rect_from_quad_points_array(array, index));
    }
    rect
}

pub fn rect_from_quad_points(annot: &CpdfDictionary, index: usize) -> CfxFloatRect {
    let Some(array) = annot.get_array_for(b"QuadPoints") else {
        return CfxFloatRect::default();
    };
    let borrowed = array.borrow();
    let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
        return CfxFloatRect::default();
    };
    if index >= quad_point_count(array) {
        return CfxFloatRect::default();
    }
    rect_from_quad_points_array(array, index)
}

pub fn annot_rect(annot: &CpdfDictionary) -> CfxFloatRect {
    let text_markup = matches!(
        annot.get_byte_string_for(b"Subtype", b"").as_slice(),
        b"Highlight" | b"Squiggly" | b"StrikeOut" | b"Underline"
    );
    let mut rect = if text_markup && annot.get_boolean_for(b"PDFIUM_HasGeneratedAP", false) {
        bounding_rect_from_quad_points(annot)
    } else {
        annot.get_rect_for(b"Rect")
    };
    rect.normalize();
    rect
}

pub fn annot_matrix(
    page: &CpdfPage,
    annot: &CpdfDictionary,
    form_dict: &CpdfDictionary,
    page_to_device: CfxMatrix,
) -> CfxMatrix {
    matrix_for_rect(
        page,
        annot_rect(annot),
        annot.get_integer_for(b"F", 0) as u32,
        form_dict,
        page_to_device,
    )
}

fn matrix_for_rect(
    page: &CpdfPage,
    rect: CfxFloatRect,
    flags: u32,
    form_dict: &CpdfDictionary,
    page_to_device: CfxMatrix,
) -> CfxMatrix {
    let form_bbox = form_dict
        .get_matrix_for(b"Matrix")
        .transform_float_rect(form_dict.get_rect_for(b"BBox"));
    let mut matrix = CfxMatrix::default();
    matrix.match_rect(rect, form_bbox);
    if flags & 16 != 0 && page.rotation() != 0 {
        matrix.concat(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, -rect.left, -rect.top));
        matrix.rotate(std::f32::consts::PI / 2.0 * page.rotation() as f32);
        matrix.concat(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, rect.left, rect.top));
    }
    matrix.concat(page_to_device);
    matrix
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn appearance_state_uses_parent_value_and_only_falls_back_for_missing_mode() {
        let objects = CpdfIndirectObjectHolder::new();
        let stream = objects.borrow_mut().new_indirect_stream_from_bytes(b"q Q");
        let mut states = CpdfDictionary::new();
        states.set_new_for(
            b"On",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        let mut ap = CpdfDictionary::new();
        ap.set_new_for(b"N", states);
        ap.set_new_for(b"R", CpdfScalarObject::number_from_signed(0));
        let mut parent = CpdfDictionary::new();
        parent.set_new_name_for(b"V", b"On");
        let mut annot = CpdfDictionary::new();
        annot.set_new_for(b"Parent", parent);
        annot.set_new_for(b"AP", ap);
        assert!(std::rc::Rc::ptr_eq(
            &get_annot_ap(&annot, AppearanceMode::Normal, true).unwrap(),
            &stream
        ));
        assert!(get_annot_ap(&annot, AppearanceMode::Rollover, true).is_none());
        assert!(get_annot_ap(&annot, AppearanceMode::Down, true).is_some());
        assert!(get_annot_ap(&annot, AppearanceMode::Down, false).is_none());
    }

    #[test]
    fn fit_keeps_the_form_matrix_and_compensates_no_rotate_at_top_left() {
        use crate::fxcrt::fx_coordinates::CfxPointF;
        let mut page = CpdfDictionary::new();
        page.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 100.0, 100.0));
        page.set_new_for(b"Rotate", CpdfScalarObject::number_from_signed(90));
        let page = CpdfPage::new(retain_object(page)).unwrap();
        let mut annot = CpdfDictionary::new();
        annot.set_rect_for(b"Rect", CfxFloatRect::new(10.0, 20.0, 30.0, 40.0));
        let mut form = CpdfDictionary::new();
        form.set_rect_for(b"BBox", CfxFloatRect::new(0.0, 0.0, 10.0, 10.0));
        let rotation = CfxMatrix::new(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
        form.set_matrix_for(b"Matrix", rotation);
        let matrix = annot_matrix(&page, &annot, &form, CfxMatrix::default());
        assert_eq!(
            (rotation * matrix).transform(CfxPointF::new(2.0, 8.0)),
            CfxPointF::new(14.0, 24.0)
        );
        annot.set_new_for(b"F", CpdfScalarObject::number_from_signed(16));
        let matrix = rotation * annot_matrix(&page, &annot, &form, CfxMatrix::default());
        let point = matrix.transform(CfxPointF::new(2.0, 8.0));
        assert!((point.x - 26.0).abs() < 0.0001);
        assert!((point.y - 44.0).abs() < 0.0001);
    }
}

#[cfg(test)]
#[path = "cpdf_annot_unittest.rs"]
mod upstream_tests;
