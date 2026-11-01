// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_ffwidget.{h,cpp}`. FF virtual dispatch, actual layout/FWL adapters
//! and explicit document-view services replace native GC/casts. Native base
//! input/edit no-ops remain defaults; concrete field implementations override
//! them. Image decoding/drawing delegates to the shared image port.
pub use super::cxfa_ffimage::load_image_from_buffer;
pub use super::cxfa_imagerenderer::draw_image;
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_fffield::CxfaFfField,
    cxfa_textlayout::{CxfaTextLayout, TextLayoutError},
    fxfa::{AlertButton, AlertIcon, AlertReturn, XfaEventError, XfaWidgetStatus as S},
    fxfa_basic::XfaAttributeValue as V,
    layout::{
        cxfa_contentlayoutitem::CxfaContentLayoutItem,
        cxfa_layoutitem::{CxfaLayoutItem, LayoutItemWidget},
        cxfa_viewlayoutitem::CxfaViewLayoutItem,
    },
    parser::{cxfa_document::CxfaNodeStore, cxfa_node::CxfaNode},
};
use crate::{
    fpdfdoc::cpdf_formfield::FormFieldType,
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, CfxVector},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_app::CfwlApp,
            cfwl_event::FwlEvent,
            cfwl_message::{FwlMessage, MouseCommand},
            cfwl_widget::WidgetAdapter,
            fwl_widgethit::FwlWidgetHit,
        },
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
pub const XFA_WIDGET_PRECISION: f32 = 0.001;
pub fn unit_px_to_pt(pixels: f32, dpi: f32) -> f32 {
    pixels * 72. / dpi
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusOption {
    DoNotDrawFocus,
    DrawFocus,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighlightOption {
    NoHighlight,
    Highlight,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FfWidgetError {
    DetachedDocumentView,
    ReentrantTextLayout,
    TextLayout(TextLayoutError),
}
impl From<TextLayoutError> for FfWidgetError {
    fn from(value: TextLayoutError) -> Self {
        Self::TextLayout(value)
    }
}
pub type FfResult<T> = Result<T, FfWidgetError>;
/// A real text layout is parked outside its slot during font callbacks.
/// Recursive access to the same layout is an explicit service error, while
/// other layouts and model operations can safely reenter the document view.
pub struct FfTextLayout {
    layout: RefCell<Option<CxfaTextLayout>>,
}
pub type CaptionLayout = Rc<FfTextLayout>;
impl FfTextLayout {
    pub fn new(layout: CxfaTextLayout) -> Rc<Self> {
        Rc::new(Self {
            layout: RefCell::new(Some(layout)),
        })
    }
    pub fn with_layout<R>(
        &self,
        operation: impl FnOnce(&mut CxfaTextLayout) -> Result<R, TextLayoutError>,
    ) -> FfResult<R> {
        let layout = self
            .layout
            .borrow_mut()
            .take()
            .ok_or(FfWidgetError::ReentrantTextLayout)?;
        struct Restore<'a> {
            slot: &'a RefCell<Option<CxfaTextLayout>>,
            layout: Option<CxfaTextLayout>,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.slot.replace(self.layout.take());
            }
        }
        let mut restore = Restore {
            slot: &self.layout,
            layout: Some(layout),
        };
        operation(restore.layout.as_mut().unwrap()).map_err(FfWidgetError::from)
    }
}
/// Actual embedding/document-view services. The widget stores a weak edge;
/// model observers are not used as a substitute for these FF services.
pub trait FfDocViewHost {
    fn fwl_app(&self) -> Rc<CfwlApp>;
    fn is_interactive(&self) -> bool;
    fn page_rect(&self, page: &CxfaViewLayoutItem) -> CfxRectF;
    fn invalidate_rect(&self, page: Option<&CxfaViewLayoutItem>, rect: CfxRectF);
    fn display_caret(&self, widget: &dyn FfWidget, visible: bool, anchor: Option<CfxRectF>);
    fn process_event(
        &self,
        node: &CxfaNode,
        activity: V,
        event: &mut CxfaEventParam,
    ) -> XfaEventError;
    fn set_change_mark(&self);
    fn add_validate_node(&self, node: &CxfaNode);
    fn highlight_color(&self) -> u32;
    fn message_box(
        &self,
        message: &[u32],
        title: &[u32],
        icon: AlertIcon,
        buttons: AlertButton,
    ) -> Option<AlertReturn>;
    fn load_caption(&self, node: &CxfaNode) -> FfResult<Option<CaptionLayout>>;
    fn caption_layout(&self, node: &CxfaNode) -> Option<CaptionLayout>;
    fn text_layout(&self, node: &CxfaNode) -> Option<CaptionLayout>;
    fn load_text_layout(
        &self,
        node: &CxfaNode,
        kind: super::cxfa_textprovider::TextProviderType,
    ) -> FfResult<Option<CaptionLayout>>;
    fn get_font(
        &self,
        node: &CxfaNode,
    ) -> Option<Rc<crate::xfa::fgas::font::cfgas_gefont::CfgasGeFont>>;
    fn get_popup_pos(
        &self,
        widget: &dyn FfWidget,
        minimum: f32,
        maximum: f32,
        anchor: CfxRectF,
        popup: &mut CfxRectF,
    ) -> bool;
    fn goto_url(&self, url: &[u32]);
    fn add_calculate_node(&self, node: &CxfaNode);
    fn popup_menu(&self, widget: &dyn FfWidget, point: CfxPointF);
    fn update_ui_display(&self, node: &CxfaNode, except: Option<&dyn FfWidget>);
    fn app_title(&self) -> Vec<u32>;
    fn set_focus_node(&self, node: Option<&CxfaNode>) -> FfResult<()>;
    fn image_owner(&self) -> Option<Rc<dyn super::cxfa_ffimage::XfaImageOwner>>;
}
pub trait FfWidgetHandler {
    fn has_event(&self, node: &CxfaNode, event_type: XfaEventType) -> bool;
    fn process_event(&self, node: &CxfaNode, event: &mut CxfaEventParam) -> XfaEventError;
}
pub trait FfWidgetIterator {
    fn move_to_first(&mut self) -> Option<Rc<dyn LayoutItemWidget>>;
    fn move_to_last(&mut self) -> Option<Rc<dyn LayoutItemWidget>>;
    fn move_to_next(&mut self) -> Option<Rc<dyn LayoutItemWidget>>;
    fn move_to_previous(&mut self) -> Option<Rc<dyn LayoutItemWidget>>;
    fn current_widget(&self) -> Option<Rc<dyn LayoutItemWidget>>;
    fn set_current_widget(&mut self, widget: &dyn FfWidget) -> bool;
}
/// Native CXFA_FFWidget virtuals. Defaults are its actual base behavior, with
/// field-specific overrides routed through the composed actual field base.
pub trait FfWidget: WidgetAdapter + LayoutItemWidget {
    fn widget(&self) -> &CxfaFfWidget;
    fn as_field(&self) -> Option<&CxfaFfField> {
        None
    }
    fn as_text_edit(&self) -> Option<&super::cxfa_fftextedit::CxfaFfTextEdit> {
        None
    }
    fn pre_finalize(&self) {
        if let Some(edit) = self.as_text_edit() {
            edit.pre_finalize();
        }
    }
    fn set_fwl_rect(&self) {
        if let Some(field) = self.as_field() {
            field.set_fwl_rect();
        }
    }
    fn insert_choice_item(&self, _label: &[u32], _index: i32) -> bool {
        false
    }
    fn delete_choice_item(&self, _index: i32) -> bool {
        false
    }
    fn open_drop_down_list(&self) -> bool {
        false
    }
    fn get_bbox(&self, focus: FocusOption) -> CfxRectF {
        if let Some(field) = self.as_field() {
            field.get_bbox(focus)
        } else {
            self.widget().get_bbox(focus)
        }
    }
    fn render_widget(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        highlight: HighlightOption,
    ) -> FfResult<bool> {
        if let Some(field) = self.as_field() {
            field.render_widget(graphics, matrix, highlight)
        } else {
            self.widget().render_widget(graphics, matrix, highlight)
        }
    }
    fn is_loaded(&self) -> bool {
        if let Some(field) = self.as_field() {
            field.is_loaded()
        } else {
            self.widget().is_loaded()
        }
    }
    fn load_widget(&self) -> FfResult<bool> {
        if let Some(edit) = self.as_text_edit() {
            return edit.load_widget();
        }

        if let Some(field) = self.as_field() {
            field.load_widget_with_layout(|| self.perform_layout())
        } else {
            // Native LoadWidget dispatches the virtual PerformLayout. Text
            // draws use that override to build every paginated text block.
            self.perform_layout()?;
            Ok(true)
        }
    }
    fn perform_layout(&self) -> FfResult<()> {
        if let Some(field) = self.as_field() {
            field.perform_layout_with_rect(|| self.set_fwl_rect())
        } else {
            self.widget().perform_layout();
            Ok(())
        }
    }
    fn update_fwl_data(&self) {
        if let Some(edit) = self.as_text_edit() {
            return edit.update_fwl_data();
        }
    }
    fn update_widget_property(&self) {
        if let Some(edit) = self.as_text_edit() {
            return edit.update_widget_property();
        }
    }
    fn accepts_focus_on_button_down(
        &self,
        flags: u32,
        point: CfxPointF,
        command: MouseCommand,
    ) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.accepts_focus_on_button_down(flags, point, command);
        }

        self.as_field()
            .is_some_and(|field| field.accepts_focus_on_button_down(flags, point, command))
    }
    fn on_mouse_enter(&self) -> bool {
        self.as_field().is_some_and(CxfaFfField::on_mouse_enter)
    }
    fn on_mouse_exit(&self) -> bool {
        self.as_field().is_some_and(CxfaFfField::on_mouse_exit)
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.on_l_button_down(flags, point);
        }

        self.as_field()
            .is_some_and(|field| field.on_l_button_down(flags, point))
    }
    fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        self.as_field()
            .is_some_and(|field| field.on_l_button_up(flags, point))
    }
    fn on_l_button_dbl_clk(&self, flags: u32, point: CfxPointF) -> bool {
        self.as_field()
            .is_some_and(|field| field.on_l_button_dbl_clk(flags, point))
    }
    fn on_mouse_move(&self, flags: u32, point: CfxPointF) -> bool {
        self.as_field()
            .is_some_and(|field| field.on_mouse_move(flags, point))
    }
    fn on_mouse_wheel(&self, flags: u32, point: CfxPointF, delta: CfxVector) -> bool {
        self.as_field()
            .is_some_and(|field| field.on_mouse_wheel(flags, point, delta))
    }
    fn on_r_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.on_r_button_down(flags, point);
        }

        self.as_field()
            .is_some_and(|field| field.on_r_button_down(flags, point))
    }
    fn on_r_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.on_r_button_up(flags, point);
        }

        self.as_field()
            .is_some_and(|field| field.on_r_button_up(flags, point))
    }
    fn on_r_button_dbl_clk(&self, flags: u32, point: CfxPointF) -> bool {
        self.as_field()
            .is_some_and(|field| field.on_r_button_dbl_clk(flags, point))
    }
    fn on_set_focus(&self, old: Option<&dyn FfWidget>) -> FfResult<bool> {
        if let Some(edit) = self.as_text_edit() {
            return edit.on_set_focus(old);
        }

        if let Some(field) = self.as_field() {
            field.on_set_focus(old)
        } else {
            self.widget().on_set_focus(old)
        }
    }
    fn on_kill_focus(&self, new: Option<&dyn FfWidget>) -> FfResult<bool> {
        if let Some(edit) = self.as_text_edit() {
            return edit.on_kill_focus(new);
        }

        if let Some(field) = self.as_field() {
            field.on_kill_focus(new)
        } else {
            self.widget().on_kill_focus(new)
        }
    }
    fn on_key_down(&self, key: u32, flags: u32) -> bool {
        self.as_field()
            .is_some_and(|field| field.on_key_down(key, flags))
    }
    fn on_char(&self, ch: u32, flags: u32) -> bool {
        self.as_field()
            .is_some_and(|field| field.on_char(ch, flags))
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        self.as_field()
            .map_or(FwlWidgetHit::Unknown, |field| field.hit_test(point))
    }
    fn can_undo(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.can_undo();
        }

        false
    }
    fn can_redo(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.can_redo();
        }

        false
    }
    fn can_copy(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.can_copy();
        }

        false
    }
    fn can_cut(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.can_cut();
        }

        false
    }
    fn can_paste(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.can_paste();
        }

        false
    }
    fn can_select_all(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.can_select_all();
        }

        false
    }
    fn can_delete(&self) -> bool {
        self.can_cut()
    }
    fn can_de_select(&self) -> bool {
        self.can_copy()
    }
    fn undo(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.undo();
        }

        false
    }
    fn redo(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.redo();
        }

        false
    }
    fn copy(&self) -> Option<Vec<u32>> {
        if let Some(edit) = self.as_text_edit() {
            return edit.copy();
        }

        None
    }
    fn cut(&self) -> Option<Vec<u32>> {
        if let Some(edit) = self.as_text_edit() {
            return edit.cut();
        }

        None
    }
    fn paste(&self, _text: &[u32]) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.paste(_text);
        }

        false
    }
    fn select_all(&self) {
        if let Some(edit) = self.as_text_edit() {
            return edit.select_all();
        }
    }
    fn delete(&self) {
        if let Some(edit) = self.as_text_edit() {
            return edit.delete();
        }
    }
    fn de_select(&self) {
        if let Some(edit) = self.as_text_edit() {
            return edit.de_select();
        }
    }
    fn get_text(&self) -> Vec<u32> {
        if let Some(edit) = self.as_text_edit() {
            return edit.get_text();
        }

        Vec::new()
    }
    fn get_form_field_type(&self) -> FormFieldType {
        if self.as_text_edit().is_some() {
            return FormFieldType::XfaTextField;
        }

        FormFieldType::Xfa
    }
    fn commit_data(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.commit_data();
        }

        false
    }
    fn is_data_changed(&self) -> bool {
        if let Some(edit) = self.as_text_edit() {
            return edit.is_data_changed();
        }

        false
    }
    fn on_process_message(&self, _message: &mut FwlMessage) {
        if let Some(edit) = self.as_text_edit() {
            return edit.on_process_message(_message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        if let Some(edit) = self.as_text_edit() {
            return edit.on_process_event(event);
        }

        if let Some(field) = self.as_field() {
            field.on_process_event(event);
        }
    }
    fn on_draw_widget(&self, _graphics: &mut CfgasGeGraphics<'_, '_, '_>, _matrix: CfxMatrix) {
        if let Some(edit) = self.as_text_edit() {
            return edit.on_draw_widget(_graphics, _matrix);
        }
    }
}
pub struct CxfaFfWidget {
    node: CxfaNode,
    _node_store: Rc<CxfaNodeStore>,
    layout_item: RefCell<Option<CxfaContentLayoutItem>>,
    doc_view: RefCell<Option<Weak<dyn FfDocViewHost>>>,
    page_view: RefCell<Option<CxfaViewLayoutItem>>,
    widget_rect: Cell<CfxRectF>,
}
impl CxfaFfWidget {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new(Self::new_base(node))
    }
    pub(crate) fn new_base(node: &CxfaNode) -> Self {
        Self {
            node: node.clone(),
            _node_store: node.retain_store().expect("FF widget node has expired"),
            layout_item: RefCell::new(None),
            doc_view: RefCell::new(None),
            page_view: RefCell::new(None),
            widget_rect: Cell::new(CfxRectF::default()),
        }
    }
    pub fn node(&self) -> &CxfaNode {
        &self.node
    }
    pub fn layout_item(&self) -> Option<CxfaContentLayoutItem> {
        self.layout_item
            .borrow()
            .clone()
            .filter(|item| item.is_alive())
    }
    pub fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        *self.layout_item.borrow_mut() = item;
    }
    pub fn set_doc_view(&self, host: Option<&Rc<dyn FfDocViewHost>>) {
        *self.doc_view.borrow_mut() = host.map(Rc::downgrade);
    }
    pub fn doc_view(&self) -> Option<Rc<dyn FfDocViewHost>> {
        self.doc_view.borrow().as_ref()?.upgrade()
    }
    pub fn require_doc_view(&self) -> FfResult<Rc<dyn FfDocViewHost>> {
        self.doc_view().ok_or(FfWidgetError::DetachedDocumentView)
    }
    pub fn page_view(&self) -> Option<CxfaViewLayoutItem> {
        self.page_view
            .borrow()
            .clone()
            .filter(|item| item.is_alive())
    }
    pub fn set_page_view(&self, page: Option<CxfaViewLayoutItem>) {
        *self.page_view.borrow_mut() = page;
    }
    pub fn fwl_app(&self) -> Option<Rc<CfwlApp>> {
        Some(self.doc_view()?.fwl_app())
    }
    pub fn from_layout_item(item: &CxfaLayoutItem) -> Option<Rc<dyn LayoutItemWidget>> {
        if !item.form_node().has_created_ui_widget() {
            return None;
        }
        let widget = item.as_content_layout_item()?.widget()?;
        widget.as_ff_widget()?;
        Some(widget)
    }
    pub fn next_ff_widget(&self) -> Option<Rc<dyn LayoutItemWidget>> {
        let next = self.layout_item()?.next()?;
        Self::from_layout_item(&next)
    }
    pub fn get_widget_rect(&self) -> CfxRectF {
        if self
            .layout_item()
            .is_some_and(|item| !item.test_status_bits(S::kRectCached))
        {
            self.recache_widget_rect();
        }
        self.widget_rect.get()
    }
    pub fn recache_widget_rect(&self) -> CfxRectF {
        if let Some(item) = self.layout_item() {
            item.set_status_bits(S::kRectCached);
            self.widget_rect.set(item.get_absolute_rect());
        }
        self.widget_rect.get()
    }
    pub fn get_rect_without_rotate(&self) -> CfxRectF {
        let mut rect = self.get_widget_rect();
        match self.node.get_rotate() {
            90 => {
                rect.top = rect.bottom();
                std::mem::swap(&mut rect.width, &mut rect.height);
            }
            180 => {
                rect.left = rect.right();
                rect.top = rect.bottom();
            }
            270 => {
                rect.left = rect.right();
                std::mem::swap(&mut rect.width, &mut rect.height);
            }
            _ => {}
        }
        rect
    }
    pub fn modify_status(&self, added: S, removed: S) {
        if let Some(item) = self.layout_item() {
            item.clear_status_bits(removed);
            item.set_status_bits(added);
        }
    }
    pub fn get_bbox(&self, focus: FocusOption) -> CfxRectF {
        if focus == FocusOption::DrawFocus {
            return CfxRectF::default();
        }
        self.page_view()
            .and_then(|page| self.doc_view().map(|host| host.page_rect(&page)))
            .unwrap_or_default()
    }
    pub fn render_widget(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        _highlight: HighlightOption,
    ) -> FfResult<bool> {
        if !self.has_visible_status() {
            return Ok(true);
        }
        let Some(border) = self.node.get_border_if_exists() else {
            return Ok(true);
        };
        let mut rect = self.get_rect_without_rotate();
        rect_without_margin(&mut rect, border.get_margin_if_exists().as_ref());
        rect.normalize();
        Ok(border.box_draw(graphics, rect, matrix, false))
    }
    pub fn is_loaded(&self) -> bool {
        self.page_view().is_some()
    }
    pub fn perform_layout(&self) {
        self.recache_widget_rect();
    }
    pub fn has_event_under_handler(
        &self,
        event_type: XfaEventType,
        handler: &dyn FfWidgetHandler,
    ) -> bool {
        self.node.is_widget_ready() && handler.has_event(&self.node, event_type)
    }
    pub fn process_event_under_handler(
        &self,
        event: &mut CxfaEventParam,
        handler: &dyn FfWidgetHandler,
    ) -> bool {
        self.node.is_widget_ready()
            && handler.process_event(&self.node, event) == XfaEventError::kSuccess
    }
    pub fn draw_border(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        border: Option<&CxfaNode>,
        rect: CfxRectF,
        matrix: CfxMatrix,
        round: bool,
    ) -> bool {
        border.is_none_or(|border| border.box_draw(graphics, rect, matrix, round))
    }
    pub fn invalidate_rect(&self) {
        if let Some(host) = self.doc_view() {
            let mut rect = self.get_bbox(FocusOption::DoNotDrawFocus);
            rect.inflate(2., 2., 2., 2.);
            host.invalidate_rect(self.page_view().as_ref(), rect);
        }
    }
    pub fn on_set_focus(&self, old: Option<&dyn FfWidget>) -> FfResult<bool> {
        let host = self.require_doc_view()?;
        if let Some(parent) = self.parent().as_ref().and_then(Self::from_layout_item) {
            if let Some(parent) = parent.as_ff_widget() {
                if !parent.widget().is_ancestor_of(old) && !parent.on_set_focus(old)? {
                    return Ok(false);
                }
            }
        }
        self.modify_status(S::kFocused, S::kNone);
        let mut event = CxfaEventParam::new(XfaEventType::Enter);
        host.process_event(&self.node, V::Enter, &mut event);
        Ok(true)
    }
    pub fn on_kill_focus(&self, new: Option<&dyn FfWidget>) -> FfResult<bool> {
        self.modify_status(S::kNone, S::kFocused);
        self.event_kill_focus()?;
        let Some(new) = new else {
            return Ok(true);
        };
        if let Some(parent) = self.parent().as_ref().and_then(Self::from_layout_item) {
            if let Some(parent) = parent.as_ff_widget() {
                if !parent.widget().is_ancestor_of(Some(new)) && !parent.on_kill_focus(Some(new))? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    pub fn event_kill_focus(&self) -> FfResult<()> {
        if self
            .layout_item()
            .is_some_and(|item| item.test_status_bits(S::kAccess))
        {
            self.modify_status(S::kNone, S::kAccess);
            return Ok(());
        }
        let host = self.require_doc_view()?;
        let mut event = CxfaEventParam::new(XfaEventType::Exit);
        host.process_event(&self.node, V::Exit, &mut event);
        Ok(())
    }
    pub fn rotate_to_normal(&self, point: CfxPointF) -> CfxPointF {
        let matrix = self.get_rotate_matrix();
        if matrix.is_identity() {
            point
        } else {
            matrix.get_inverse().transform(point)
        }
    }
    pub fn is_layout_rect_empty(&self) -> bool {
        let rect = self.get_rect_without_rotate();
        rect.width < 0.1 && rect.height < 0.1
    }
    pub fn parent(&self) -> Option<CxfaLayoutItem> {
        self.node.parent()?.get_layout_item()
    }
    pub fn is_ancestor_of(&self, widget: Option<&dyn FfWidget>) -> bool {
        let Some(widget) = widget else {
            return false;
        };
        let mut node = Some(widget.widget().node.clone());
        while let Some(current) = node {
            if current == self.node {
                return true;
            }
            node = current.parent();
        }
        false
    }
    pub fn pt_in_active_rect(&self, point: CfxPointF) -> bool {
        self.get_widget_rect().contains_point(point)
    }
    pub fn has_visible_status(&self) -> bool {
        self.layout_item()
            .is_some_and(|item| item.test_status_bits(S::kVisible))
    }
    pub fn is_focused(&self) -> bool {
        self.layout_item()
            .is_some_and(|item| item.test_status_bits(S::kFocused))
    }
    pub fn is_button_down(&self) -> bool {
        self.layout_item()
            .is_some_and(|item| item.test_status_bits(S::kButtonDown))
    }
    pub fn set_button_down(&self, down: bool) {
        if down {
            self.modify_status(S::kButtonDown, S::kNone);
        } else {
            self.modify_status(S::kNone, S::kButtonDown);
        }
    }
}
impl LayoutItemWidget for CxfaFfWidget {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl FfWidget for CxfaFfWidget {
    fn widget(&self) -> &CxfaFfWidget {
        self
    }
}
impl WidgetAdapter for CxfaFfWidget {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        let rotation = self.node.get_rotate();
        if rotation == 0 {
            return CfxMatrix::default();
        }
        let rect = self.get_rect_without_rotate();
        match rotation {
            90 => CfxMatrix::new(0., -1., 1., 0., rect.left - rect.top, rect.left + rect.top),
            180 => CfxMatrix::new(-1., 0., 0., -1., rect.left * 2., rect.top * 2.),
            270 => CfxMatrix::new(0., 1., -1., 0., rect.left + rect.top, rect.top - rect.left),
            _ => CfxMatrix::default(),
        }
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(host) = self.doc_view() {
            host.display_caret(self, visible, anchor);
        }
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.node
            .get_ui_border()
            .and_then(|border| border.box_get_edge(0))
            // The only consuming theme part is CombTextLine. Its native
            // caller initializes black/one-point defaults before asking the
            // widget to override them; an absent border leaves those intact.
            .map_or((0xff000000, 1.), |edge| {
                (edge.stroke_get_color(), edge.stroke_get_thickness())
            })
    }
}
pub fn rect_without_margin(rect: &mut CfxRectF, margin: Option<&CxfaNode>) {
    if let Some(margin) = margin {
        rect.deflate(
            margin.margin_get_left_inset(),
            margin.margin_get_top_inset(),
            margin.margin_get_right_inset(),
            margin.margin_get_bottom_inset(),
        );
    }
}
/// A FWL-owned adapter never owns its FF widget (the FF widget owns the FWL
/// control). Upgrading a small owner before callback preserves safe reentry.
pub(crate) struct FfWidgetAdapter(pub Weak<dyn FfWidget>);
impl WidgetAdapter for FfWidgetAdapter {
    fn xfa_widget(&self) -> Option<Rc<dyn FfWidget>> {
        self.0.upgrade()
    }
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.0
            .upgrade()
            .map_or(CfxMatrix::default(), |widget| widget.get_rotate_matrix())
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(widget) = self.0.upgrade() {
            widget.display_caret(visible, anchor);
        }
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.0
            .upgrade()
            .map_or((0, 0.), |widget| widget.get_border_color_and_thickness())
    }
}
#[cfg(test)]
#[path = "cxfa_ffwidget_tests.rs"]
mod tests;
