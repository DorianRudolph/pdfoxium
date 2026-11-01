// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Ordinary widget data, appearances, input dispatch and editing from
//! `fpdfsdk/cpdfsdk_widget.{h,cpp}`. Page-view and formfiller owners use weak
//! observation. Ordinary field actions execute through the environment's
//! JavaScript runtime; foreground XFA synchronization uses the real FF graph.

use super::cpdfsdk_annot::CpdfSdkAnnot;
use super::cpdfsdk_appstream::CpdfSdkAppStream;
use super::cpdfsdk_baannot::{BorderStyle, CpdfSdkBaAnnot};
use super::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
use super::cpdfsdk_pageview::CpdfSdkPageView;
use super::formfiller::cffl_fieldaction::CfflFieldAction;
use super::formfiller::cffl_interactiveformfiller::CfflInteractiveFormFiller;
use super::pwl::pwl_draw::PwlDraw;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfdoc::cpdf_aaction::AActionType;
use crate::fpdfdoc::cpdf_action::CpdfAction;
use crate::fpdfdoc::cpdf_annot::AppearanceMode;
use crate::fpdfdoc::cpdf_formcontrol::{CpdfFormControl, HighlightingMode};
use crate::fpdfdoc::cpdf_formfield::{
    dict_get, dict_has, dict_mut, typed, CpdfFormField, FormFieldType, NotificationOption,
};
use crate::fxcrt::cfx_bidi_resolver::ParagraphDirection;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxge::cfx_color::{CfxColor, ColorType, TypeAndArgb};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

pub struct CpdfSdkWidget {
    document: Weak<RefCell<CpdfDocument>>,
    control: Rc<CpdfFormControl>,
    annot: CpdfSdkBaAnnot,
    appearance_modified: Cell<bool>,
    appearance_age: Cell<u32>,
    value_age: Cell<u32>,
    text_direction: Cell<ParagraphDirection>,
    annotation: RefCell<Option<Weak<CpdfSdkAnnot>>>,
}

impl CpdfSdkWidget {
    pub fn new(document: &Rc<RefCell<CpdfDocument>>, control: Rc<CpdfFormControl>) -> Self {
        Self {
            document: Rc::downgrade(document),
            annot: CpdfSdkBaAnnot::new(control.widget_dict()),
            control,
            appearance_modified: Cell::new(false),
            appearance_age: Cell::new(0),
            value_age: Cell::new(0),
            text_direction: Cell::new(ParagraphDirection::Auto),
            annotation: RefCell::new(None),
        }
    }
    pub fn document(&self) -> Option<Rc<RefCell<CpdfDocument>>> {
        self.document.upgrade()
    }
    pub fn bind_annotation(&self, annotation: &Rc<CpdfSdkAnnot>) {
        *self.annotation.borrow_mut() = Some(Rc::downgrade(annotation));
    }
    pub fn annotation(&self) -> Option<Rc<CpdfSdkAnnot>> {
        self.annotation.borrow().as_ref().and_then(Weak::upgrade)
    }
    pub fn page_view(&self) -> Option<Rc<CpdfSdkPageView>> {
        self.annotation().and_then(|a| a.page_view())
    }
    pub fn env(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
        self.page_view().and_then(|v| v.environment())
    }
    pub fn is_alive(&self) -> bool {
        if self.document.upgrade().is_none() {
            return false;
        }
        self.annotation
            .borrow()
            .as_ref()
            .is_none_or(|a| a.upgrade().is_some_and(|a| a.is_alive()))
    }
    fn ui(&self) -> Option<(Rc<CfflInteractiveFormFiller>, Rc<Self>, Rc<CpdfSdkPageView>)> {
        if self.is_signature_widget() || !self.is_alive() {
            return None;
        }
        let annot = self.annotation()?;
        let view = annot.page_view()?;
        let env = view.environment()?;
        Some((env.get_interactive_form_filler(), annot.widget()?, view))
    }
    pub fn get_text(&self) -> Vec<u32> {
        self.ui().map_or_else(Vec::new, |(f, w, _)| f.get_text(&w))
    }
    pub fn get_selected_text(&self) -> Vec<u32> {
        self.ui()
            .map_or_else(Vec::new, |(f, w, _)| f.get_selected_text(&w))
    }
    pub fn replace_selection(&self, text: &[u32]) {
        if let Some((f, w, _)) = self.ui() {
            f.replace_selection(&w, text);
        }
    }
    pub fn replace_and_keep_selection(&self, text: &[u32]) {
        if let Some((f, w, _)) = self.ui() {
            f.replace_and_keep_selection(&w, text);
        }
    }
    pub fn select_all_text(&self) -> bool {
        self.ui().is_some_and(|(f, w, _)| f.select_all_text(&w))
    }
    pub fn can_undo(&self) -> bool {
        self.ui().is_some_and(|(f, w, _)| f.can_undo(&w))
    }
    pub fn can_redo(&self) -> bool {
        self.ui().is_some_and(|(f, w, _)| f.can_redo(&w))
    }
    pub fn undo(&self) -> bool {
        self.ui().is_some_and(|(f, w, _)| f.undo(&w))
    }
    pub fn redo(&self) -> bool {
        self.ui().is_some_and(|(f, w, _)| f.redo(&w))
    }
    pub fn set_index_selected(&self, index: i32, selected: bool) -> bool {
        self.ui()
            .is_some_and(|(f, w, _)| f.set_index_selected(&w, index, selected))
    }
    pub fn is_index_selected(&self, index: i32) -> bool {
        self.ui()
            .is_some_and(|(f, w, _)| f.is_index_selected(&w, index))
    }
    pub fn reset_field_appearance(&self) -> bool {
        let (Some(env), Some(field)) = (self.env(), self.field()) else {
            return false;
        };
        env.interactive_form().reset_field_appearance(&field, None);
        self.is_alive()
    }
    pub fn update_field(&self) -> bool {
        let (Some(env), Some(field)) = (self.env(), self.field()) else {
            return false;
        };
        env.interactive_form().update_field(&field);
        self.is_alive()
    }
    pub fn set_top_visible_index(&self, _index: i32) {} // Source CPDFSDK_Widget method is empty.
    pub fn on_a_action(
        &self,
        kind: AActionType,
        data: &mut CfflFieldAction,
        _view: &Rc<CpdfSdkPageView>,
    ) -> bool {
        let (Some(env), Some(field)) = (self.env(), self.field()) else {
            data.b_rc = false;
            return false;
        };
        #[cfg(feature = "xfa")]
        if self.handle_xfa_action(kind, data) {
            return true;
        }
        if !self.is_alive() || !env.is_alive() {
            data.b_rc = false;
            return false;
        }
        let action = self.additional_action(kind);
        if !action.has_dict() {
            return false;
        }
        let javascript_enabled = env.is_javascript_enabled();
        // Unlike the source's XFA-only status, ordinary script actions must
        // report handling so OnBeforeKeyStroke applies change/selection/rc.
        match env.do_action_field(&action, kind, &field, data) {
            Ok(handled) => javascript_enabled && handled,
            Err(error) => {
                data.b_rc = false;
                env.record_action_error(error);
                javascript_enabled
            }
        }
    }
    pub fn on_format(&self) -> Option<Vec<u32>> {
        let env = self.env()?;
        let field = self.field()?;
        match env.interactive_form().on_format(&field) {
            Ok(value) => value,
            Err(error) => {
                env.record_action_error(error);
                None
            }
        }
    }
    pub fn on_load(&self) {
        if self.is_signature_widget() {
            return;
        }
        if !self.is_appearance_valid() {
            self.reset_appearance(None, false);
        }
        if matches!(
            self.field_type(),
            FormFieldType::TextField | FormFieldType::ComboBox
        ) {
            let formatted = self.on_format();
            if self.is_alive() && self.field_type() == FormFieldType::ComboBox {
                if let Some(value) = formatted {
                    self.reset_appearance(Some(&value), false);
                }
            }
        }
        #[cfg(feature = "xfa")]
        if self.is_alive()
            && self.foreground_xfa_view().is_some()
            && !self.is_appearance_valid()
            && !self.value().is_empty()
        {
            self.reset_xfa_appearance(false);
        }
    }
    pub fn draw_appearance(
        &self,
        draw: &mut dyn PwlDraw,
        matrix: CfxMatrix,
        mode: AppearanceMode,
    ) -> bool {
        if matches!(
            self.field_type(),
            FormFieldType::CheckBox | FormFieldType::RadioButton
        ) && mode == AppearanceMode::Normal
            && !self.is_widget_appearance_valid(mode)
        {
            draw.stroke_rect(matrix, self.rect(), 0xffaaaaaa, 0.0);
            return true;
        }
        let Some(page) = self.page_view().and_then(|v| v.page()) else {
            return false;
        };
        draw.draw_annotation_ap(&page, &self.dict(), mode, matrix)
    }
    pub fn on_draw(&self, draw: &mut dyn PwlDraw, matrix: CfxMatrix, _draw_annots: bool) {
        if self.is_signature_widget() {
            self.draw_appearance(draw, matrix, AppearanceMode::Normal);
            return;
        }
        if let Some((f, w, v)) = self.ui() {
            f.on_draw(&v, &w, draw, matrix);
        }
    }
    pub fn view_bbox(&self) -> CfxFloatRect {
        self.ui().map_or_else(CfxFloatRect::default, |(f, w, v)| {
            let r = f.get_view_bbox(&v, &w);
            CfxFloatRect::new(r.left as f32, r.top as f32, r.right as f32, r.bottom as f32)
        })
    }
    pub fn do_hit_test(&self, point: crate::fxcrt::fx_coordinates::CfxPointF) -> bool {
        if self.is_signature_widget() || !self.annot.is_visible() || self.field_flags() & 1 != 0 {
            return false;
        }
        let allowed = self.field_type() == FormFieldType::PushButton
            || self.env().is_some_and(|e| e.has_permissions(0x100 | 0x20));
        allowed && self.view_bbox().contains_point(point)
    }
    pub fn draw_shadow(&self, draw: &mut dyn PwlDraw, view: &CpdfSdkPageView) {
        let Some(env) = self.env() else {
            return;
        };
        let form = env.interactive_form();
        let kind = self.field_type();
        if !form.is_need_highlight(kind) {
            return;
        }
        let rect = view.current_matrix().transform_float_rect(self.rect());
        let color = crate::fxge::dib::fx_dib::alpha_and_color_ref_to_argb(
            i32::from(form.highlight_alpha()),
            form.highlight_color(kind),
        );
        draw.fill_rect(CfxMatrix::default(), rect, color);
    }
    pub fn control(&self) -> Rc<CpdfFormControl> {
        Rc::clone(&self.control)
    }
    pub fn field(&self) -> Option<Rc<CpdfFormField>> {
        self.control.field()
    }
    pub fn annot(&self) -> &CpdfSdkBaAnnot {
        &self.annot
    }
    pub fn dict(&self) -> ObjectRef {
        self.annot.dict()
    }
    pub fn rect(&self) -> CfxFloatRect {
        self.annot.rect()
    }
    pub fn set_rect(&self, rect: CfxFloatRect) {
        dict_mut(&self.dict(), |dict| dict.set_rect_for(b"Rect", rect));
    }
    pub fn field_type(&self) -> FormFieldType {
        self.field()
            .map_or(FormFieldType::Unknown, |f| f.field_type())
    }
    pub fn field_flags(&self) -> u32 {
        self.field().map_or(0, |f| f.field_flags())
    }
    pub fn is_signature_widget(&self) -> bool {
        self.field_type() == FormFieldType::Signature
    }
    pub fn layout_order(&self) -> i32 {
        2
    }
    pub fn rotate(&self) -> i32 {
        self.control.rotation() % 360
    }
    pub fn fill_color(&self) -> Option<u32> {
        color_ref(self.control.color_argb(b"BG"))
    }
    pub fn border_color(&self) -> Option<u32> {
        color_ref(self.control.color_argb(b"BC"))
    }
    pub fn text_color(&self) -> Option<u32> {
        color_ref(self.control.default_appearance().color_argb()?)
    }
    pub fn font_size(&self) -> f32 {
        self.control.default_appearance().font_size_or_zero()
    }
    pub fn selected_index(&self, index: i32) -> i32 {
        #[cfg(feature = "xfa")]
        if let Some(index) = self.xfa_selected_index(index) {
            return index;
        }
        self.field().map_or(-1, |f| f.selected_index(index))
    }
    pub fn value(&self) -> Vec<u32> {
        #[cfg(feature = "xfa")]
        if let Some(value) = self.xfa_value() {
            return value;
        }
        self.field().map_or_else(Vec::new, |f| f.value())
    }
    pub fn export_value(&self) -> Vec<u32> {
        self.control.export_value()
    }
    pub fn option_label(&self, index: i32) -> Vec<u32> {
        self.field()
            .map_or_else(Vec::new, |f| f.option_label(index))
    }
    pub fn select_export_text(&self, index: i32) -> Vec<u32> {
        if index < 0 {
            return Vec::new();
        }
        let Some(field) = self.field() else {
            return Vec::new();
        };
        let value = field.option_value(index);
        if value.is_empty() {
            field.option_label(index)
        } else {
            value
        }
    }
    pub fn count_options(&self) -> usize {
        self.field().map_or(0, |f| f.count_options())
    }
    pub fn is_option_selected(&self, index: i32) -> bool {
        #[cfg(feature = "xfa")]
        if let Some(selected) = self.xfa_option_selected(index) {
            return selected;
        }
        self.field().is_some_and(|f| f.is_item_selected(index))
    }
    pub fn top_visible_index(&self) -> i32 {
        self.field().map_or(0, |f| f.top_visible_index())
    }
    pub fn is_checked(&self) -> bool {
        #[cfg(feature = "xfa")]
        if let Some(checked) = self.xfa_checked() {
            return checked;
        }
        self.control.is_checked()
    }
    pub fn alignment(&self) -> i32 {
        self.control.control_alignment()
    }
    pub fn max_len(&self) -> i32 {
        self.field().map_or(0, |f| f.max_len())
    }
    pub fn set_check(&self, checked: bool) -> bool {
        let Some(field) = self.field() else {
            return false;
        };
        let Ok(index) = usize::try_from(field.control_index(&self.control)) else {
            return false;
        };
        field.check_control(index, checked, NotificationOption::DoNotNotify);
        #[cfg(feature = "xfa")]
        self.finish_xfa_check();
        true
    }
    pub fn set_value(&self, value: &[u32]) -> bool {
        let result = self
            .field()
            .is_some_and(|f| f.set_value(value, NotificationOption::DoNotNotify));
        #[cfg(feature = "xfa")]
        if result && self.is_alive() {
            self.synchronize(true);
        }
        result
    }
    pub fn set_option_selection(&self, index: i32) -> bool {
        let Some(field) = self.field() else {
            return false;
        };
        if index < 0 || index as usize >= field.count_options() {
            return false;
        }
        field.set_item_selection(index, NotificationOption::DoNotNotify);
        #[cfg(feature = "xfa")]
        self.synchronize(true);
        true
    }
    pub fn clear_selection(&self) -> bool {
        let result = self
            .field()
            .is_some_and(|f| f.clear_selection(NotificationOption::DoNotNotify));
        #[cfg(feature = "xfa")]
        if result && self.is_alive() {
            self.synchronize(true);
        }
        result
    }
    pub fn border_width(&self) -> i32 {
        self.annot.border_width()
    }
    pub fn border_style(&self) -> BorderStyle {
        self.annot.border_style()
    }
    pub fn app_state(&self) -> Vec<u8> {
        self.annot.app_state()
    }
    pub fn set_app_state_off(&self) {
        self.annot.set_app_state_off();
    }
    pub fn is_appearance_valid(&self) -> bool {
        #[cfg(feature = "xfa")]
        if self
            .env()
            .and_then(|env| env.xfa_context())
            .is_some_and(|ctx| ctx.contains_extension_full_form())
        {
            return true;
        }
        self.annot.is_appearance_valid()
    }
    pub fn is_widget_appearance_valid(&self, mode: AppearanceMode) -> bool {
        let Some(ap) = typed(dict_get(&self.dict(), b"AP"), ObjectType::Dictionary) else {
            return false;
        };
        let requested: &[u8] = match mode {
            AppearanceMode::Normal => b"N",
            AppearanceMode::Down => b"D",
            AppearanceMode::Rollover => b"R",
        };
        let key = if dict_has(&ap, requested) {
            requested
        } else {
            b"N"
        };
        let Some(sub) = dict_get(&ap, key) else {
            return false;
        };
        match self.field_type() {
            FormFieldType::PushButton
            | FormFieldType::ComboBox
            | FormFieldType::ListBox
            | FormFieldType::TextField
            | FormFieldType::Signature => sub.borrow().object_type() == ObjectType::Stream,
            FormFieldType::CheckBox | FormFieldType::RadioButton => {
                let state = self.app_state();
                let borrowed = sub.borrow();
                borrowed
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .is_some_and(|d| d.get_stream_for(&state).is_some())
            }
            _ => true,
        }
    }
    pub fn is_push_highlighted(&self) -> bool {
        self.control.highlighting_mode() == HighlightingMode::Push
    }
    pub fn appearance_modified(&self) -> bool {
        self.appearance_modified.get()
    }
    pub fn set_appearance_modified(&self, modified: bool) {
        self.appearance_modified.set(modified);
    }
    pub fn appearance_age(&self) -> u32 {
        self.appearance_age.get()
    }
    pub fn value_age(&self) -> u32 {
        self.value_age.get()
    }
    pub fn reset_appearance(&self, value: Option<&[u32]>, value_changed: bool) -> bool {
        self.appearance_modified.set(true);
        self.appearance_age
            .set(self.appearance_age.get().wrapping_add(1));
        if value_changed {
            self.value_age.set(self.value_age.get().wrapping_add(1));
        }
        let app = CpdfSdkAppStream::new(self);
        match self.field_type() {
            FormFieldType::PushButton => app.set_as_push_button(),
            FormFieldType::CheckBox => app.set_as_check_box(),
            FormFieldType::RadioButton => app.set_as_radio_button(),
            FormFieldType::ComboBox => app.set_as_combo_box(value),
            FormFieldType::ListBox => app.set_as_list_box(),
            FormFieldType::TextField => app.set_as_text_field(value),
            _ => false,
        }
    }
    pub fn text_direction(&self) -> ParagraphDirection {
        self.text_direction.get()
    }
    pub fn set_text_direction(&self, direction: ParagraphDirection) {
        self.text_direction.set(direction);
    }
    pub fn client_rect(&self) -> CfxFloatRect {
        let mut width = self.border_width() as f32;
        if matches!(
            self.border_style(),
            BorderStyle::Beveled | BorderStyle::Inset
        ) {
            width *= 2.0;
        }
        self.rotated_rect().get_deflated(width, width)
    }
    pub fn rotated_rect(&self) -> CfxFloatRect {
        let rect = self.rect();
        let (width, height) = (rect.width(), rect.height());
        match self.rotate().abs() {
            90 | 270 => CfxFloatRect::new(0.0, 0.0, height, width),
            _ => CfxFloatRect::new(0.0, 0.0, width, height),
        }
    }
    pub fn matrix(&self) -> CfxMatrix {
        let rect = self.rect();
        let (width, height) = (rect.width(), rect.height());
        match self.rotate().abs() {
            90 => CfxMatrix::new(0.0, 1.0, -1.0, 0.0, width, 0.0),
            180 => CfxMatrix::new(-1.0, 0.0, 0.0, -1.0, width, height),
            270 => CfxMatrix::new(0.0, -1.0, 1.0, 0.0, 0.0, height),
            _ => CfxMatrix::default(),
        }
    }
    pub fn text_pwl_color(&self) -> CfxColor {
        self.control
            .default_appearance()
            .color()
            .unwrap_or(CfxColor::make_gray(0.0))
    }
    pub fn border_pwl_color(&self) -> CfxColor {
        self.control.original_border_color()
    }
    pub fn fill_pwl_color(&self) -> CfxColor {
        self.control.original_background_color()
    }
    pub fn additional_action(&self, kind: AActionType) -> CpdfAction {
        match kind {
            AActionType::CursorEnter
            | AActionType::CursorExit
            | AActionType::ButtonDown
            | AActionType::ButtonUp
            | AActionType::GetFocus
            | AActionType::LoseFocus
            | AActionType::PageOpen
            | AActionType::PageClose
            | AActionType::PageVisible
            | AActionType::PageInvisible => self.annot.additional_action(kind),
            AActionType::KeyStroke
            | AActionType::Format
            | AActionType::Validate
            | AActionType::Calculate => {
                if let Some(field) = self.field() {
                    let actions = field.additional_action();
                    if actions.has_dict() {
                        return actions.action(kind);
                    }
                }
                self.annot.additional_action(kind)
            }
            _ => CpdfAction::new(None),
        }
    }
}

fn color_ref(color: TypeAndArgb) -> Option<u32> {
    if color.color_type == ColorType::Transparent {
        return None;
    }
    let argb = color.argb;
    Some(((argb >> 16) & 0xff) | (argb & 0x0000ff00) | ((argb & 0xff) << 16))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "js")]
    use crate::fpdfapi::page::cpdf_page::CpdfPage;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    #[cfg(feature = "js")]
    use crate::fpdfapi::parser::cpdf_parser::ParserError;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fpdfdoc::cpdf_action::ActionType;
    use crate::fpdfdoc::cpdf_interactiveform::CpdfInteractiveForm;
    #[cfg(feature = "js")]
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::{ActionError, FormFillHost};
    #[cfg(feature = "js")]
    use crate::fpdfsdk::cpdfsdk_pageview::SdkPage;
    #[cfg(feature = "js")]
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn widget_dict(kind: &[u8]) -> CpdfDictionary {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"Widget");
        dict.set_new_name_for(b"FT", kind);
        dict.set_new_string_for(b"T", b"field");
        dict.set_rect_for(b"Rect", CfxFloatRect::new(10.0, 20.0, 110.0, 60.0));
        dict
    }
    fn load(
        dict: CpdfDictionary,
    ) -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<CpdfInteractiveForm>,
        CpdfSdkWidget,
    ) {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let root = document.borrow().root().unwrap();
        let mut fields = CpdfArray::new();
        fields.append_new(dict);
        dict_mut(&root, |root| {
            let acro = root.set_new_dictionary_for(b"AcroForm");
            dict_mut(&acro, |acro| {
                acro.set_new_for(b"Fields", fields);
            });
        });
        let form = CpdfInteractiveForm::new(&document);
        let field = form.get_field(0, &wide("field")).unwrap();
        let control = field.get_control(0).unwrap();
        let widget = CpdfSdkWidget::new(&document, control);
        (document, form, widget)
    }
    fn stream() -> CpdfStream {
        CpdfStream::from_data_and_dict(Vec::new(), retain_object(CpdfDictionary::new()))
    }

    #[test]
    #[cfg(feature = "js")]
    fn ordinary_javascript_keystroke_applies_changes_and_veto_to_live_edit() {
        struct Host(Cell<bool>);
        impl FormFillHost for Host {
            fn javascript_enabled(&self) -> bool {
                self.0.get()
            }
        }
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        assert_eq!(
            document.borrow_mut().load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(
                    include_bytes!("../../../../upstream/pdfium/testing/resources/text_form.pdf")
                        .to_vec()
                )),
                b""
            ),
            ParserError::Success
        );
        let host = Rc::new(Host(Cell::new(true)));
        let interface: Rc<dyn FormFillHost> = host.clone();
        let environment = CpdfSdkFormFillEnvironment::new(&document, Some(&interface));
        let page = CpdfPage::from_document(&mut document.borrow_mut(), 0).unwrap();
        let page = SdkPage::new(&document, &page);
        let view = environment.get_or_create_page_view(&page).unwrap();
        view.set_valid(true);
        let annotation = view.annot_list()[0].clone();
        let widget = annotation.widget().unwrap();
        assert!(environment.set_focus_annot(&annotation));

        let mut action_data = CfflFieldAction::default();
        assert!(!widget.on_a_action(AActionType::KeyStroke, &mut action_data, &view));
        assert!(action_data.b_rc);
        assert!(view.on_char(u32::from(b'a'), 0));
        assert_eq!(widget.get_text(), wide("a"));

        let set_script = |script: &[u8]| {
            let mut action = CpdfDictionary::new();
            action.set_new_name_for(b"S", b"JavaScript");
            action.set_new_string_for(b"JS", script);
            let mut actions = CpdfDictionary::new();
            actions.set_new_for(b"K", action);
            dict_mut(&widget.dict(), |dict| {
                dict.set_new_for(b"AA", actions);
            });
        };
        set_script(b"event.change = event.change.toUpperCase();");
        assert!(view.on_char(u32::from(b'b'), 0));
        assert_eq!(widget.get_text(), wide("aB"));

        set_script(b"event.rc = false;");
        assert!(view.on_char(u32::from(b'x'), 0));
        assert_eq!(widget.get_text(), wide("aB"));

        set_script(b"event.selStart = 0; event.selEnd = event.value.length; event.change = 'Z';");
        assert!(view.on_char(u32::from(b'd'), 0));
        assert_eq!(widget.get_text(), wide("Z"));

        set_script(b"throw new Error('keystroke failure');");
        assert!(!view.on_char(u32::from(b'e'), 0));
        assert_eq!(widget.get_text(), wide("Z"));
        assert_eq!(
            environment.action_error(),
            Some(ActionError::JavaScriptException)
        );
        assert!(
            String::from_utf16_lossy(&environment.last_js_error().unwrap().message)
                .contains("keystroke failure")
        );

        host.0.set(false);
        assert!(!environment.run_input_event(|| {
            widget.on_a_action(AActionType::KeyStroke, &mut action_data, &view)
        }));
        assert!(view.on_char(u32::from(b'c'), 0));
        assert_eq!(widget.get_text(), wide("Zc"));
        assert!(environment.action_error().is_none());
        assert!(environment.last_js_error().is_none());
    }

    #[test]
    fn rotation_geometry_border_and_original_colors() {
        let mut dict = widget_dict(b"Tx");
        let mut mk = CpdfDictionary::new();
        mk.set_new_for(b"R", CpdfScalarObject::number_from_signed(-90));
        let mut color = CpdfArray::new();
        for component in [1, 0, 0] {
            color.append_new(CpdfScalarObject::number_from_signed(component));
        }
        mk.set_new_for(b"BG", color);
        dict.set_new_for(b"MK", mk);
        dict.set_new_string_for(b"DA", b"/Helv 12 Tf 0 0 1 rg");
        let (_document, _form, widget) = load(dict);
        widget.annot().set_border_width(2);
        widget.annot().set_border_style(BorderStyle::Beveled);
        assert_eq!(widget.rotate(), -90);
        assert_eq!(
            widget.rotated_rect(),
            CfxFloatRect::new(0.0, 0.0, 40.0, 100.0)
        );
        assert_eq!(
            widget.client_rect(),
            CfxFloatRect::new(4.0, 4.0, 36.0, 96.0)
        );
        assert_eq!(
            widget.matrix(),
            CfxMatrix::new(0.0, 1.0, -1.0, 0.0, 100.0, 0.0)
        );
        assert_eq!(widget.fill_pwl_color(), CfxColor::make_rgb(1.0, 0.0, 0.0));
        assert_eq!(widget.text_pwl_color(), CfxColor::make_rgb(0.0, 0.0, 1.0));
        assert_eq!(widget.fill_color(), Some(0x000000ff));
        assert_eq!(widget.text_color(), Some(0x00ff0000));
        assert_eq!(widget.font_size(), 12.0);
    }

    #[test]
    fn checkbox_state_and_appearance_mode_fallback() {
        let mut dict = widget_dict(b"Btn");
        let mut ap = CpdfDictionary::new();
        ap.set_new_for(b"N", CpdfDictionary::new());
        dict.set_new_for(b"AP", ap);
        dict.set_new_name_for(b"AS", b"Off");
        let (document, form, widget) = load(dict);
        let objects = document.borrow().holder();
        let normal = dict_get(&widget.annot().ap_dict(), b"N").unwrap();
        for state in [b"Off".as_slice(), b"Yes"] {
            let number = objects
                .borrow_mut()
                .add_indirect_object(retain_object(stream()));
            dict_mut(&normal, |normal| {
                normal.set_new_for(state, CpdfReference::new(&objects, number));
            });
        }
        assert!(widget.is_widget_appearance_valid(AppearanceMode::Rollover));
        assert!(!widget.is_checked());
        assert!(widget.set_check(true));
        assert!(widget.is_checked());
        assert_eq!(widget.value(), wide("Yes"));
        assert_eq!(form.count_fields(&[]), 1);
        assert!(widget.is_widget_appearance_valid(AppearanceMode::Normal));
        dict_mut(&widget.annot().ap_dict(), |ap| {
            ap.set_new_for(b"D", CpdfDictionary::new());
        });
        assert!(!widget.is_widget_appearance_valid(AppearanceMode::Down));
        widget.set_check(false);
        assert_eq!(widget.app_state(), b"Off");
        assert_eq!(widget.value(), wide("Off"));
        // A widget does not keep the form/field graph alive after its owner dies.
        drop(form);
        assert_eq!(widget.field_type(), FormFieldType::Unknown);
        assert!(!widget.set_check(true));
    }

    #[test]
    fn action_selection_uses_field_actions_for_editing_events() {
        let mut parent = widget_dict(b"Tx");
        let mut actions = CpdfDictionary::new();
        let mut format = CpdfDictionary::new();
        format.set_new_name_for(b"S", b"JavaScript");
        actions.set_new_for(b"F", format);
        parent.set_new_for(b"AA", actions);
        let mut child = CpdfDictionary::new();
        child.set_new_name_for(b"Subtype", b"Widget");
        let mut actions = CpdfDictionary::new();
        let mut format = CpdfDictionary::new();
        format.set_new_name_for(b"S", b"Named");
        actions.set_new_for(b"F", format);
        let mut up = CpdfDictionary::new();
        up.set_new_name_for(b"S", b"URI");
        actions.set_new_for(b"U", up);
        child.set_new_for(b"AA", actions);
        let mut children = CpdfArray::new();
        children.append_new(child);
        parent.set_new_for(b"Kids", children);
        let (_document, _form, widget) = load(parent);
        assert_eq!(
            widget.additional_action(AActionType::Format).action_type(),
            ActionType::JavaScript
        );
        assert_eq!(
            widget
                .additional_action(AActionType::ButtonUp)
                .action_type(),
            ActionType::Uri
        );
        assert!(!widget.additional_action(AActionType::Validate).has_dict());
        assert!(!widget
            .additional_action(AActionType::DocumentOpen)
            .has_dict());
    }
}
