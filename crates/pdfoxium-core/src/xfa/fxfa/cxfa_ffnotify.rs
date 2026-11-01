// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Actual model/layout/FF event notification routing from `cxfa_ffnotify` and
//! CXFA_Node::SendAttributeChangeMessage, with actual widget factories and
//! model/text/image measurement services for the layout processors.
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_ffdoc::{CxfaFfDoc, PageViewEvent},
    cxfa_ffdocview::{CxfaFfDocView, LayoutStatus, UpdateScope},
    cxfa_ffpageview::CxfaFfPageView,
    cxfa_ffwidget::{CxfaFfWidget, FfDocViewHost, FfResult, FfWidgetError},
    fxfa::{FormType, XfaEventError, XfaWidgetStatus as S},
    fxfa_basic::{XfaAttribute as A, XfaElement as E, XfaPacketType},
    layout::{
        cxfa_contentlayoutitem::CxfaContentLayoutItem,
        cxfa_contentlayoutprocessor::{ContentLayoutHost, CxfaContentLayoutProcessor, LayoutError},
        cxfa_layoutitem::{CxfaLayoutItem, LayoutItemNotify, LayoutItemWidget},
        cxfa_viewlayoutitem::CxfaViewLayoutItem,
        cxfa_viewlayoutprocessor::{ViewLayoutError, ViewLayoutHost, ViewLayoutResult},
    },
    parser::{
        cxfa_document::XfaDocumentNotify,
        cxfa_node::{CxfaNode, XfaFfWidgetType, XfaNodeFlag},
    },
};
use crate::{
    fxcrt::fx_coordinates::CfxSizeF,
    fxjs::xfa::{cfxjse_engine::CfxJseEngine, cjx_instancemanager::XfaInstanceManagerHost},
};
use boa_engine::{JsError, JsNativeError, JsResult};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
pub struct CxfaFfNotify {
    doc: Weak<CxfaFfDoc>,
    last_widget_error: RefCell<Option<FfWidgetError>>,
    last_factory_error: RefCell<Option<JsError>>,
}
impl CxfaFfNotify {
    pub fn new(doc: Weak<CxfaFfDoc>) -> Rc<Self> {
        Rc::new(Self {
            doc,
            last_widget_error: RefCell::new(None),
            last_factory_error: RefCell::new(None),
        })
    }
    pub fn doc(&self) -> Option<Rc<CxfaFfDoc>> {
        self.doc.upgrade()
    }
    fn view(&self) -> Option<Rc<CxfaFfDocView>> {
        self.doc()?.get_doc_view()
    }
    pub fn take_widget_error(&self) -> Option<FfWidgetError> {
        self.last_widget_error.take()
    }
    pub fn take_factory_error(&self) -> Option<JsError> {
        self.last_factory_error.take()
    }
    fn record(&self, result: FfResult<()>) {
        if let Err(error) = result {
            *self.last_widget_error.borrow_mut() = Some(error);
        }
    }
    pub fn on_page_view_event(&self, sender: Option<&CxfaViewLayoutItem>, event: PageViewEvent) {
        let Some(doc) = self.doc() else {
            return;
        };
        if sender.is_some_and(|sender| {
            sender
                .get_layout()
                .is_none_or(|layout| !Rc::ptr_eq(&layout, &doc.layout_processor()))
        }) {
            return;
        }
        if doc.get_doc_view().is_some() {
            doc.on_page_view_event(sender, event);
        }
    }
    pub fn on_create_view_layout_item(&self, node: &CxfaNode) -> Option<Rc<CxfaFfPageView>> {
        if node.get_element_type() != E::PageArea {
            return None;
        }
        Some(CxfaFfPageView::new(Rc::downgrade(&self.view()?), node))
    }
    pub fn on_create_content_layout_item(
        &self,
        node: &CxfaNode,
    ) -> JsResult<Option<Rc<dyn LayoutItemWidget>>> {
        if !node.has_created_ui_widget() {
            return Ok(None);
        }
        let child = || {
            node.get_ui_child_node()
                .ok_or_else(|| JsNativeError::error().with_message("XFA widget UI child is absent"))
        };
        let widget: Rc<dyn LayoutItemWidget> = match node.get_ff_widget_type() {
            XfaFfWidgetType::None => return Ok(None),
            XfaFfWidgetType::Subform => CxfaFfWidget::new(node),
            XfaFfWidgetType::Text => super::cxfa_fftext::CxfaFfText::new(node),
            XfaFfWidgetType::Arc => super::cxfa_ffarc::CxfaFfArc::new(node),
            XfaFfWidgetType::Line => super::cxfa_ffline::CxfaFfLine::new(node),
            XfaFfWidgetType::Rectangle => super::cxfa_ffrectangle::CxfaFfRectangle::new(node),
            XfaFfWidgetType::Button => {
                super::cxfa_ffpushbutton::CxfaFfPushButton::new(node, &child()?)
            }
            XfaFfWidgetType::CheckButton => {
                super::cxfa_ffcheckbutton::CxfaFfCheckButton::new(node, &child()?)
            }
            XfaFfWidgetType::ExclGroup => super::cxfa_ffexclgroup::CxfaFfExclGroup::new(node),
            XfaFfWidgetType::TextEdit => super::cxfa_fftextedit::CxfaFfTextEdit::new(node),
            XfaFfWidgetType::NumericEdit => super::cxfa_ffnumericedit::CxfaFfNumericEdit::new(node),
            XfaFfWidgetType::PasswordEdit => {
                super::cxfa_ffpasswordedit::CxfaFfPasswordEdit::new(node, &child()?)
            }
            XfaFfWidgetType::ChoiceList => {
                if node.is_list_box() {
                    super::cxfa_fflistbox::CxfaFfListBox::new(node)
                } else {
                    super::cxfa_ffcombobox::CxfaFfComboBox::new(node)
                }
            }
            XfaFfWidgetType::Signature => super::cxfa_ffsignature::CxfaFfSignature::new(node),
            XfaFfWidgetType::DateTimeEdit => {
                super::cxfa_ffdatetimeedit::CxfaFfDateTimeEdit::new(node)
            }
            XfaFfWidgetType::Barcode => {
                super::cxfa_ffbarcode_widget::CxfaFfBarcode::new(node, &child()?)
            }
            XfaFfWidgetType::Image => super::cxfa_ffimage_widget::CxfaFfImage::new(node),
            XfaFfWidgetType::ImageEdit => {
                super::cxfa_ffimageedit_widget::CxfaFfImageEdit::new(node)
            }
        };
        if let Some(view) = self.view() {
            let host: Rc<dyn FfDocViewHost> = view;
            widget
                .as_ff_widget()
                .unwrap()
                .widget()
                .set_doc_view(Some(&host));
        }
        Ok(Some(widget))
    }
    pub fn run_script(&self, script: &CxfaNode, item: &CxfaNode) -> bool {
        let Some(view) = self.view() else {
            return false;
        };
        let mut event = CxfaEventParam::new(XfaEventType::Unknown);
        event.targeted = false;
        item.execute_bool_script(&view, Some(script), &mut event) == (XfaEventError::kSuccess, true)
    }
    pub fn exec_event_by_deep_first(
        &self,
        node: Option<&CxfaNode>,
        kind: XfaEventType,
        ready: bool,
        recursive: bool,
    ) -> XfaEventError {
        self.view().map_or(XfaEventError::kNotExist, |view| {
            view.exec_event_activity_by_deep_first(node, kind, ready, recursive)
        })
    }
    pub fn add_calc_validate(&self, node: &CxfaNode) {
        if let Some(view) = self.view() {
            view.add_calculate_node(node);
            view.add_validate_node(node);
        }
    }
    pub fn handle_widget_event(&self, node: &CxfaNode, event: &mut CxfaEventParam) {
        if let Some(view) = self.view() {
            view.get_widget_handler()
                .process_event(Some(node), Some(event));
        }
    }
    pub fn reset_data(&self, node: Option<&CxfaNode>) {
        if let Some(view) = self.view() {
            view.reset_node(node);
        }
    }
    pub fn get_layout_status(&self) -> LayoutStatus {
        self.view()
            .map_or(LayoutStatus::None, |view| view.layout_status())
    }
    pub fn run_node_initialize(&self, node: &CxfaNode) {
        if let Some(view) = self.view() {
            view.add_new_form_node(node);
        }
    }
    pub fn run_subform_index_change(&self, node: &CxfaNode) {
        if let Some(view) = self.view() {
            view.add_index_changed_subform(node);
        }
    }
    pub fn get_focus_widget_node(&self) -> Option<CxfaNode> {
        self.view()?.focus_node()
    }
    pub fn set_focus_widget_node(&self, node: Option<&CxfaNode>) -> FfResult<()> {
        if let Some(view) = self.view() {
            view.set_focus_node(node)
        } else {
            Ok(())
        }
    }
    pub fn open_drop_down_list(&self, node: &CxfaNode) -> JsResult<()> {
        let Some(view) = self.view() else {
            return Ok(());
        };
        let Some(owner) = view.get_widget_for_node(node) else {
            return Ok(());
        };
        if let Some(doc) = self.doc() {
            doc.set_focus_widget(Some(owner.clone()));
        }
        let Some(widget) = owner.as_ff_widget() else {
            return Ok(());
        };
        if node.get_ff_widget_type() != XfaFfWidgetType::ChoiceList
            || node.is_list_box()
            || !widget.is_loaded()
        {
            return Ok(());
        }
        let _scope = UpdateScope::new(&view);
        if widget.open_drop_down_list() {
            Ok(())
        } else {
            Err(JsNativeError::error()
                .with_message("XFA dropdown dispatch is not yet connected")
                .into())
        }
    }
    pub fn on_container_changed(&self) {
        if let Some(doc) = self.doc() {
            doc.layout_processor().set_has_changed_container();
        }
    }
    pub fn on_value_changing(&self, sender: &CxfaNode, attribute: A) {
        if attribute != A::Presence
            || sender.get_packet_type() == XfaPacketType::Datasets
            || !sender.is_form_container()
        {
            return;
        }
        let Some(view) = self
            .view()
            .filter(|view| view.layout_status() == LayoutStatus::End)
        else {
            return;
        };
        let mut current = view.get_widget_for_node(sender);
        while let Some(owner) = current {
            let Some(widget) = owner.as_ff_widget() else {
                break;
            };
            current = widget.widget().next_ff_widget();
            if widget.is_loaded() {
                widget.widget().invalidate_rect();
            }
        }
    }
    pub fn on_value_changed(
        &self,
        sender: &CxfaNode,
        attribute: A,
        parent: &CxfaNode,
        widget_node: &CxfaNode,
    ) {
        let Some(view) = self.view() else {
            return;
        };
        if sender.get_packet_type() != XfaPacketType::Form {
            if attribute == A::Value {
                view.add_calculate_node_notify(sender);
            }
            return;
        }
        view.invalidate_widget_layout(widget_node);
        let kind = parent.get_element_type();
        let container = parent.is_container_node();
        let mut update_property = false;
        view.set_change_mark();
        match kind {
            E::Caption => {
                if view.caption_layout(widget_node).is_none() {
                    return;
                }
                view.invalidate_caption(widget_node);
            }
            E::Ui | E::Para => update_property = true,
            _ => {}
        }
        if container && attribute == A::Access {
            update_property = true;
        }
        if attribute == A::Value {
            view.add_calculate_node_notify(sender);
            // Boa records the logical field read; native TryContent can record
            // its underlying raw node. Notify both identities at this boundary.
            if sender != widget_node {
                view.add_calculate_node_notify(widget_node);
            }
            if container {
                view.update_ui_display(widget_node, None);
                view.add_calculate_node(widget_node);
                view.add_validate_node(widget_node);
                return;
            }
            if kind == E::Value {
                if widget_node
                    .parent()
                    .is_some_and(|parent| parent.get_element_type() == E::ExclGroup)
                {
                    view.update_ui_display(widget_node, None);
                }
                return;
            }
        }
        let mut current = view.get_widget_for_node(widget_node);
        while let Some(owner) = current {
            let Some(widget) = owner.as_ff_widget() else {
                break;
            };
            current = widget.widget().next_ff_widget();
            if !widget.is_loaded() {
                continue;
            }
            if update_property {
                widget.update_widget_property();
            }
            self.record(widget.perform_layout());
            widget.widget().invalidate_rect();
        }
    }
    fn send_attribute_change_message(&self, sender: &CxfaNode, attribute: A, script_modify: bool) {
        if sender.get_packet_type() != XfaPacketType::Form {
            self.on_value_changed(sender, attribute, sender, sender);
            return;
        }
        let mut need_container = false;
        let dispatch = |parent: Option<CxfaNode>, widget: Option<CxfaNode>| {
            if let (Some(parent), Some(widget)) = (parent, widget) {
                self.on_value_changed(sender, attribute, &parent, &widget);
            }
        };
        match sender.get_element_type() {
            E::Caption => {
                need_container = true;
                dispatch(Some(sender.clone()), sender.parent());
            }
            E::Font | E::Para => {
                need_container = true;
                if let Some(parent) = sender.parent() {
                    if parent.get_element_type() == E::Caption {
                        dispatch(Some(parent.clone()), parent.parent());
                    } else {
                        dispatch(Some(sender.clone()), Some(parent));
                    }
                }
            }
            E::Margin => {
                need_container = true;
                if let Some(parent) = sender.parent() {
                    if parent.is_container_node() {
                        dispatch(Some(sender.clone()), Some(parent));
                    } else if parent.get_element_type() == E::Caption {
                        dispatch(Some(parent.clone()), parent.parent());
                    } else if let Some(ui) = parent
                        .parent()
                        .filter(|node| node.get_element_type() == E::Ui)
                    {
                        dispatch(Some(ui.clone()), ui.parent());
                    }
                }
            }
            E::Comb => {
                if let Some(edit) = sender.parent().filter(|node| {
                    matches!(
                        node.get_element_type(),
                        E::DateTimeEdit | E::NumericEdit | E::TextEdit
                    )
                }) {
                    if let Some(ui) = edit.parent() {
                        dispatch(Some(ui.clone()), ui.parent());
                    }
                }
            }
            E::Button
            | E::Barcode
            | E::ChoiceList
            | E::DateTimeEdit
            | E::NumericEdit
            | E::PasswordEdit
            | E::TextEdit
            | E::CheckButton => {
                need_container = sender.get_element_type() == E::CheckButton;
                if let Some(ui) = sender.parent() {
                    dispatch(Some(ui.clone()), ui.parent());
                }
            }
            E::Keep | E::Bookend | E::Break | E::BreakAfter | E::BreakBefore | E::Overflow => {
                need_container = true
            }
            E::Area | E::Draw | E::ExclGroup | E::Field | E::Subform | E::SubformSet => {
                self.on_container_changed();
                dispatch(Some(sender.clone()), Some(sender.clone()));
            }
            E::Sharptext | E::Sharpxml | E::SharpxHTML => {
                let Some(text) = sender.parent() else {
                    return;
                };
                let Some(value) = text.parent() else {
                    return;
                };
                if value.get_element_type() == E::Value {
                    need_container = true;
                    if let Some(node) = value.parent() {
                        if node.is_container_node() {
                            // Native promotes script content changes to the
                            // container so already loaded controls refresh.
                            dispatch(
                                Some(if script_modify { node.clone() } else { value }),
                                Some(node),
                            );
                        } else {
                            dispatch(Some(node.clone()), node.parent());
                        }
                    }
                } else if value.get_element_type() == E::Items {
                    if let Some(node) = value.parent().filter(|node| node.is_container_node()) {
                        dispatch(Some(value), Some(node));
                    }
                }
            }
            _ => {}
        }
        if need_container {
            let mut parent = Some(sender.clone());
            while let Some(node) = parent {
                if node.is_container_node() {
                    self.on_container_changed();
                    break;
                }
                parent = node.parent();
            }
        }
    }
}
impl XfaDocumentNotify for CxfaFfNotify {
    fn on_node_ready(&self, node: &CxfaNode) {
        let Some(view) = self.view() else {
            return;
        };
        if node.has_created_ui_widget() {
            node.set_widget_ready();
            return;
        }
        match node.get_element_type() {
            E::BindItems => view.add_bind_item(node),
            E::Validate => node.set_flag(XfaNodeFlag::NeedsInitApp),
            _ => {}
        }
    }
    fn on_attribute_changing(&self, node: &CxfaNode, attribute: A) {
        self.on_value_changing(node, attribute);
    }
    fn on_attribute_changed(&self, node: &CxfaNode, attribute: A) {
        self.send_attribute_change_message(node, attribute, false);
    }
    fn on_attribute_changed_by_script(&self, node: &CxfaNode, attribute: A) {
        self.send_attribute_change_message(node, attribute, true);
    }
    fn on_child_added(&self, parent: &CxfaNode, _child: &CxfaNode) {
        if !parent.is_form_container() {
            return;
        }
        if let Some(view) = self
            .view()
            .filter(|view| !view.in_layout_status() && view.layout_status() == LayoutStatus::End)
        {
            if let Some(doc) = view.doc() {
                doc.set_change_mark();
            }
        }
    }
    fn on_child_removed(&self, _parent: &CxfaNode, _child: &CxfaNode) {
        if let Some(view) = self
            .view()
            .filter(|view| !view.in_layout_status() && view.layout_status() == LayoutStatus::End)
        {
            if let Some(doc) = view.doc() {
                doc.set_change_mark();
            }
        }
    }
    fn on_widget_list_item_added(&self, node: &CxfaNode, label: &[u32], index: i32) {
        if node.get_ff_widget_type() != XfaFfWidgetType::ChoiceList {
            return;
        }
        let Some(view) = self.view() else {
            return;
        };
        let mut current = view.get_widget_for_node(node);
        while let Some(owner) = current {
            let Some(widget) = owner.as_ff_widget() else {
                break;
            };
            current = widget.widget().next_ff_widget();
            if widget.is_loaded() {
                widget.insert_choice_item(label, index);
            }
        }
    }
    fn on_widget_list_item_removed(&self, node: &CxfaNode, index: i32) {
        if node.get_ff_widget_type() != XfaFfWidgetType::ChoiceList {
            return;
        }
        let Some(view) = self.view() else {
            return;
        };
        let mut current = view.get_widget_for_node(node);
        while let Some(owner) = current {
            let Some(widget) = owner.as_ff_widget() else {
                break;
            };
            current = widget.widget().next_ff_widget();
            if widget.is_loaded() {
                widget.delete_choice_item(index);
            }
        }
    }
}
impl LayoutItemNotify for CxfaFfNotify {
    fn on_page_view_removed(&self, page: &CxfaViewLayoutItem) {
        self.on_page_view_event(Some(page), PageViewEvent::PostRemoved);
    }
    fn on_layout_item_added(&self, item: &CxfaContentLayoutItem, page_index: i32, status: S) {
        let Some(view) = self.view() else {
            return;
        };
        let Some(owner) = CxfaFfWidget::from_layout_item(item) else {
            return;
        };
        let Some(widget) = owner.as_ff_widget() else {
            return;
        };
        let page = view.get_page_view(page_index);
        let previous = widget.widget().page_view();
        widget
            .widget()
            .modify_status(status, S::kVisible | S::kViewable | S::kPrintable);
        if previous != page || status.contains(S::kVisible | S::kViewable) {
            widget.widget().set_page_view(page);
            if let Some(doc) = self.doc() {
                doc.widget_post_add(widget);
            }
        }
        if view.layout_status() != LayoutStatus::End || !status.contains(S::kVisible) {
            return;
        }
        if widget.is_loaded() {
            if widget.widget().get_widget_rect() != widget.widget().recache_widget_rect() {
                self.record(widget.perform_layout());
            }
        } else {
            if let Err(error) = widget.load_widget() {
                *self.last_widget_error.borrow_mut() = Some(error);
            }
        }
        widget.widget().invalidate_rect();
    }
    fn on_layout_item_removing(&self, item: &CxfaLayoutItem) {
        let Some(view) = self.view() else {
            return;
        };
        let Some(owner) = CxfaFfWidget::from_layout_item(item) else {
            return;
        };
        let Some(widget) = owner.as_ff_widget() else {
            return;
        };
        view.invalidate_widget_layout(widget.widget().node());
        view.delete_layout_item(widget);
        if let Some(doc) = self.doc() {
            doc.widget_pre_remove(widget);
        }
        widget.widget().invalidate_rect();
    }
}
impl XfaInstanceManagerHost for CxfaFfNotify {
    fn is_full_xfa(&self) -> bool {
        self.doc()
            .is_some_and(|doc| doc.form_type() == FormType::kXFAFull)
    }
    fn run_node_initialize(&self, node: &CxfaNode) -> JsResult<()> {
        CxfaFfNotify::run_node_initialize(self, node);
        Ok(())
    }
    fn run_subform_index_change(&self, node: &CxfaNode) -> JsResult<()> {
        CxfaFfNotify::run_subform_index_change(self, node);
        Ok(())
    }
    fn mark_layout_changed(&self) -> JsResult<()> {
        self.on_container_changed();
        Ok(())
    }
}

impl ContentLayoutHost for CxfaFfNotify {
    fn create_content_widget(&self, node: &CxfaNode) -> Option<Rc<dyn LayoutItemWidget>> {
        match self.on_create_content_layout_item(node) {
            Ok(widget) => widget,
            Err(error) => {
                *self.last_factory_error.borrow_mut() = Some(error);
                None
            }
        }
    }
    fn start_field_draw_layout(
        &self,
        node: &CxfaNode,
        size: CfxSizeF,
    ) -> Result<CfxSizeF, LayoutError> {
        self.view()
            .ok_or(LayoutError::FieldMeasurementFailed)?
            .start_field_draw_layout(node, size)
            .map_err(|error| {
                *self.last_widget_error.borrow_mut() = Some(error);
                LayoutError::FieldMeasurementFailed
            })
    }
    fn find_split_pos(
        &self,
        node: &CxfaNode,
        index: usize,
        proposed: f32,
    ) -> Result<Option<f32>, LayoutError> {
        self.view()
            .ok_or(LayoutError::FieldMeasurementFailed)?
            .find_split_pos(node, index, proposed)
            .map_err(|error| {
                *self.last_widget_error.borrow_mut() = Some(error);
                LayoutError::FieldMeasurementFailed
            })
    }
}
impl ViewLayoutHost for CxfaFfNotify {
    fn create_page_view(
        &self,
        node: &CxfaNode,
    ) -> Option<Rc<dyn super::layout::cxfa_layoutitem::LayoutItemPageView>> {
        Some(self.on_create_view_layout_item(node)?)
    }
    fn runtime(&self) -> ViewLayoutResult<Rc<CfxJseEngine>> {
        self.doc()
            .and_then(|doc| doc.try_script_context())
            .ok_or(ViewLayoutError::ExpiredDocument)
    }
    fn run_break_script(&self, script: &CxfaNode, container: &CxfaNode) -> ViewLayoutResult<bool> {
        if self.view().is_none() {
            return Err(ViewLayoutError::ExpiredDocument);
        }
        Ok(self.run_script(script, container))
    }
    fn layout_page_area(&self, page: &CxfaViewLayoutItem) -> ViewLayoutResult<()> {
        let doc = self.doc().ok_or(ViewLayoutError::ExpiredDocument)?;
        let processor = CxfaContentLayoutProcessor::new(
            &page.form_node(),
            doc.layout_processor().store().clone(),
            Some(doc.notify()),
        );
        processor.do_layout_page_area(page)?;
        Ok(())
    }
}
impl crate::fxjs::xfa::cjx_layoutpseudomodel::XfaLayoutHost for CxfaFfNotify {
    fn layout_processor(
        &self,
    ) -> Option<Rc<super::layout::cxfa_layoutprocessor::CxfaLayoutProcessor>> {
        Some(self.doc()?.layout_processor())
    }
    fn layout_status(&self) -> LayoutStatus {
        self.get_layout_status()
    }
}
