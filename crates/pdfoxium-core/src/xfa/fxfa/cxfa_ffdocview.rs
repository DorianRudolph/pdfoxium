// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! FF document events, update/calculation/validation queues and focus from
//! `cxfa_ffdocview.{h,cpp}`, connected to the native paginated coordinator.
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_ffdoc::{CxfaFfDoc, PageViewEvent},
    cxfa_ffwidget::{
        CaptionLayout, FfDocViewHost, FfResult, FfTextLayout, FfWidget, FfWidgetError,
    },
    cxfa_ffwidgethandler::CxfaFfWidgetHandler,
    cxfa_readynodeiterator::CxfaReadyNodeIterator,
    cxfa_textlayout::CxfaTextLayout,
    cxfa_textparser::XfaTextFontHost,
    cxfa_textprovider::{CxfaTextProvider, TextProviderType},
    fxfa::{AlertButton, AlertIcon, AlertReturn, XfaEventError as EventError},
    fxfa_basic::{XfaAttribute as A, XfaAttributeValue as V, XfaElement as E, XfaHashCode},
    layout::{
        cxfa_layoutitem::LayoutItemWidget,
        cxfa_layoutprocessor::{CxfaLayoutProcessor, LayoutProcessorError},
        cxfa_viewlayoutitem::CxfaViewLayoutItem,
    },
    parser::{
        cxfa_node::{CxfaNode, XfaFfWidgetType, XfaNodeFlag},
        xfa_utils::event_error_accumulate,
    },
};
use crate::{
    fxcrt::{fx_coordinates::CfxRectF, xml::cfx_xmlparser::CfxXmlParser},
    fxjs::xfa::{
        cfxjse_resolveprocessor::{ResolveResultType, XfaResolveFlags},
        cjx_eventpseudomodel::XfaEventHost,
        cjx_subform::XfaWidgetScriptHost,
    },
    xfa::{fgas::font::cfgas_gefont::CfgasGeFont, fwl::cfwl_app::CfwlApp},
};
use boa_engine::JsResult;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    rc::{Rc, Weak},
};
#[path = "cxfa_ffdocview_events.rs"]
mod events;
#[path = "cxfa_ffdocview_layout.rs"]
mod layout;

pub const EVENT_ACTIVITY: [V; 27] = [
    V::Click,
    V::Change,
    V::DocClose,
    V::DocReady,
    V::Enter,
    V::Exit,
    V::Full,
    V::IndexChange,
    V::Initialize,
    V::MouseDown,
    V::MouseEnter,
    V::MouseExit,
    V::MouseUp,
    V::PostExecute,
    V::PostOpen,
    V::PostPrint,
    V::PostSave,
    V::PostSign,
    V::PostSubmit,
    V::PreExecute,
    V::PreOpen,
    V::PrePrint,
    V::PreSave,
    V::PreSign,
    V::PreSubmit,
    V::Ready,
    V::Unknown,
];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutStatus {
    None,
    Start,
    Doing,
    End,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FfDocViewError {
    DetachedDocument,
    Layout(String),
    LayoutFailed(i32),
    ModelMerge(String),
}
impl From<LayoutProcessorError> for FfDocViewError {
    fn from(value: LayoutProcessorError) -> Self {
        Self::Layout(format!("{value:?}"))
    }
}
impl std::fmt::Display for FfDocViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for FfDocViewError {}
pub struct CxfaFfDocView {
    myself: Weak<Self>,
    doc: Weak<CxfaFfDoc>,
    fwl_app: Rc<CfwlApp>,
    status: Cell<LayoutStatus>,
    layout_event: Cell<bool>,
    in_layout_status: Cell<bool>,
    lock: Cell<usize>,
    focus_node: RefCell<Option<CxfaNode>>,
    focus_widget: RefCell<Option<Rc<dyn LayoutItemWidget>>>,
    validate_nodes: RefCell<VecDeque<CxfaNode>>,
    calculate_nodes: RefCell<Vec<CxfaNode>>,
    bind_items: RefCell<VecDeque<CxfaNode>>,
    new_added_nodes: RefCell<VecDeque<CxfaNode>>,
    index_changed_subforms: RefCell<VecDeque<CxfaNode>>,
    null_test_messages: RefCell<Vec<Vec<u32>>>,
    captions: RefCell<HashMap<CxfaNode, CaptionLayout>>,
    text_layouts: RefCell<HashMap<(CxfaNode, u8), CaptionLayout>>,
    widget_layouts: RefCell<HashMap<CxfaNode, Rc<layout::WidgetLayoutSlot>>>,
    execute_recursions: RefCell<HashMap<CxfaNode, usize>>,
    last_update_error: RefCell<Option<FfDocViewError>>,
}
impl CxfaFfDocView {
    pub fn new(doc: Weak<CxfaFfDoc>) -> Rc<Self> {
        let fwl_app = doc
            .upgrade()
            .expect("document view requires a live document")
            .app()
            .fwl_app();
        Rc::new_cyclic(|myself| Self {
            myself: myself.clone(),
            doc,
            fwl_app,
            status: Cell::new(LayoutStatus::None),
            layout_event: Cell::new(false),
            in_layout_status: Cell::new(false),
            lock: Cell::new(0),
            focus_node: RefCell::new(None),
            focus_widget: RefCell::new(None),
            validate_nodes: RefCell::new(VecDeque::new()),
            calculate_nodes: RefCell::new(Vec::new()),
            bind_items: RefCell::new(VecDeque::new()),
            new_added_nodes: RefCell::new(VecDeque::new()),
            index_changed_subforms: RefCell::new(VecDeque::new()),
            null_test_messages: RefCell::new(Vec::new()),
            captions: RefCell::new(HashMap::new()),
            text_layouts: RefCell::new(HashMap::new()),
            widget_layouts: RefCell::new(HashMap::new()),
            execute_recursions: RefCell::new(HashMap::new()),
            last_update_error: RefCell::new(None),
        })
    }
    pub fn weak_self(&self) -> Weak<Self> {
        self.myself.clone()
    }
    pub fn doc(&self) -> Option<Rc<CxfaFfDoc>> {
        self.doc.upgrade()
    }
    /// The handler has no identity-bearing state; a weak stack facade avoids
    /// a second ownership edge from the view back to itself.
    pub fn get_widget_handler(&self) -> CxfaFfWidgetHandler {
        CxfaFfWidgetHandler::new(self.weak_self())
    }
    pub fn get_layout_processor(&self) -> Option<Rc<CxfaLayoutProcessor>> {
        CxfaLayoutProcessor::from_document(&self.doc()?.model_document())
    }
    pub fn layout_status(&self) -> LayoutStatus {
        self.status.get()
    }
    pub fn in_layout_status(&self) -> bool {
        self.in_layout_status.get()
    }
    pub fn set_layout_event(&self) {
        self.layout_event.set(true);
    }
    pub fn lock_update(&self) {
        self.lock.set(self.lock.get().saturating_add(1));
    }
    pub fn unlock_update(&self) {
        self.lock.set(self.lock.get().saturating_sub(1));
    }
    pub fn take_update_error(&self) -> Option<FfDocViewError> {
        self.last_update_error.take()
    }
    pub fn start_layout(&self) -> Result<i32, FfDocViewError> {
        self.status.set(LayoutStatus::Start);
        let doc = self.doc().ok_or(FfDocViewError::DetachedDocument)?;
        let model = doc.model_document();
        let engine = doc
            .try_script_context()
            .ok_or(FfDocViewError::DetachedDocument)?;
        model
            .do_proto_merge_with_resolver(engine.resolver())
            .map_err(|error| FfDocViewError::ModelMerge(error.to_string()))?;
        model
            .do_data_merge_with_resolver(engine.resolver())
            .map_err(|error| FfDocViewError::ModelMerge(error.to_string()))?;
        let status = doc.layout_processor().start_layout()?;
        if status < 0 {
            return Ok(status);
        }
        let Some(root) = self.form_root() else {
            return Ok(status);
        };
        self.init_layout(&root);
        self.init_calculate(&root);
        self.init_validate(&root);
        self.exec_event_activity_by_deep_first(Some(&root), XfaEventType::Ready, true, true);
        self.status.set(LayoutStatus::Start);
        Ok(status)
    }
    pub fn do_layout(&self) -> Result<i32, FfDocViewError> {
        let layout = self
            .get_layout_processor()
            .ok_or(FfDocViewError::DetachedDocument)?;
        let status = layout.do_layout()?;
        if status == 100 {
            self.status.set(LayoutStatus::Doing);
        }
        Ok(status)
    }
    pub fn run_layout(&self) -> Result<bool, FfDocViewError> {
        let doc = self.doc().ok_or(FfDocViewError::DetachedDocument)?;
        let layout = self
            .get_layout_processor()
            .ok_or(FfDocViewError::DetachedDocument)?;
        self.lock_update();
        let unlock = Unlock(self);
        let previous = self.in_layout_status.replace(true);
        let in_layout = RestoreLayoutStatus {
            slot: &self.in_layout_status,
            previous,
        };
        let performed = if !layout.increment_layout()? {
            let status = layout.start_layout()?;
            if status < 0 {
                return Err(FfDocViewError::LayoutFailed(status));
            }
            if status < 100 {
                let status = layout.do_layout()?;
                if status < 0 {
                    return Err(FfDocViewError::LayoutFailed(status));
                }
                true
            } else {
                false
            }
        } else {
            false
        };
        if performed {
            // Native layout-ready callbacks observe the unlocked completed pass.
            drop(unlock);
            drop(in_layout);
            doc.on_page_view_event(None, PageViewEvent::StopLayout);
        } else {
            drop(in_layout);
            doc.on_page_view_event(None, PageViewEvent::StopLayout);
            drop(unlock);
        }
        Ok(performed)
    }
    pub fn init_layout(&self, node: &CxfaNode) {
        self.run_bind_items();
        self.exec_event_activity_by_deep_first(Some(node), XfaEventType::Initialize, false, true);
        self.exec_event_activity_by_deep_first(Some(node), XfaEventType::IndexChange, false, true);
    }
    pub fn stop_layout(&self) -> Result<(), FfDocViewError> {
        let Some(root) = self.form_root() else {
            return Ok(());
        };
        let Some(subform) = root.get_child(0, E::Subform, false) else {
            return Ok(());
        };
        let Some(page_set) = subform.get_first_child_by_class(E::PageSet) else {
            return Ok(());
        };
        self.run_calculate_widgets();
        self.run_validate();
        self.init_layout(&page_set);
        self.init_calculate(&page_set);
        self.init_validate(&page_set);
        self.exec_event_activity_by_deep_first(Some(&page_set), XfaEventType::Ready, true, true);
        self.exec_event_activity_by_deep_first(Some(&root), XfaEventType::Ready, false, true);
        self.exec_event_activity_by_deep_first(Some(&root), XfaEventType::DocReady, false, true);
        self.run_calculate_widgets();
        self.run_validate();
        if self.run_layout()? {
            self.exec_event_activity_by_deep_first(Some(&root), XfaEventType::Ready, false, true);
        }
        self.calculate_nodes.borrow_mut().clear();
        if self.focus_node().is_some() && self.focus_widget().is_none() {
            let node = self.focus_node();
            let _ = self.set_focus_node(node.as_ref());
        }
        self.status.set(LayoutStatus::End);
        Ok(())
    }
    fn form_root(&self) -> Option<CxfaNode> {
        self.doc()?
            .model_document()
            .get_xfa_node(XfaHashCode::Form as u32)
    }
    pub fn root_subform(&self) -> Option<CxfaNode> {
        self.form_root()?.get_first_child_by_class(E::Subform)
    }
    pub fn count_page_views(&self) -> i32 {
        self.get_layout_processor()
            .map_or(0, |layout| layout.count_pages())
    }
    pub fn get_page_view(&self, index: i32) -> Option<CxfaViewLayoutItem> {
        let page = self.get_layout_processor()?.get_page(index)?;
        page.page_view()?;
        Some(page)
    }
    pub fn get_widget_for_node(&self, node: &CxfaNode) -> Option<Rc<dyn LayoutItemWidget>> {
        super::cxfa_ffwidget::CxfaFfWidget::from_layout_item(
            &self.get_layout_processor()?.get_layout_item(node)?,
        )
    }
    pub fn get_widget_by_name(
        &self,
        name: &[u32],
        reference: Option<&dyn FfWidget>,
    ) -> Option<Rc<dyn LayoutItemWidget>> {
        if !name
            .iter()
            .enumerate()
            .all(|(index, &ch)| CfxXmlParser::is_xml_name_char(ch, index == 0))
        {
            return None;
        }
        let doc = self.doc()?;
        let engine = doc.try_script_context()?;
        let reference = reference
            .map(|widget| widget.widget().node())
            .filter(|node| node.is_widget_ready());
        let mut expression = if reference.is_none() {
            wide("$form.")
        } else {
            Vec::new()
        };
        expression.extend_from_slice(name);
        let result = engine
            .resolve_objects(
                reference,
                &expression,
                XfaResolveFlags::CHILDREN
                    | XfaResolveFlags::PROPERTIES
                    | XfaResolveFlags::SIBLINGS
                    | XfaResolveFlags::PARENT,
            )
            .ok()??;
        if result.result_type != ResolveResultType::Nodes {
            return None;
        }
        let node = result.objects.first()?;
        node.is_widget_ready()
            .then(|| self.get_widget_for_node(node))
            .flatten()
    }
    pub fn update_ui_display(&self, node: &CxfaNode, except: Option<&dyn FfWidget>) {
        let mut current = self.get_widget_for_node(node);
        while let Some(owner) = current {
            let Some(widget) = owner.as_ff_widget() else {
                break;
            };
            current = widget.widget().next_ff_widget();
            if except.is_some_and(|except| {
                except.widget().layout_item() == widget.widget().layout_item()
            }) || !widget.is_loaded()
                || (node.get_ff_widget_type() != XfaFfWidgetType::CheckButton
                    && widget.widget().is_focused())
            {
                continue;
            }
            widget.update_fwl_data();
            widget.widget().invalidate_rect();
        }
    }
    pub fn focus_node(&self) -> Option<CxfaNode> {
        self.focus_node.borrow().clone()
    }
    pub fn focus_widget(&self) -> Option<Rc<dyn LayoutItemWidget>> {
        self.focus_widget.borrow().clone()
    }
    pub fn set_focus(&self, mut new: Option<Rc<dyn LayoutItemWidget>>) -> FfResult<bool> {
        let mut old = self.focus_widget();
        if same_widget(old.as_ref(), new.as_ref()) {
            return Ok(false);
        }
        if let Some(owner) = old.as_ref() {
            if let Some(widget) = owner.as_ff_widget() {
                if widget.widget().has_visible_status() && !widget.widget().is_focused() {
                    if !widget.is_loaded() {
                        widget.load_widget()?;
                    }
                    if !widget.on_set_focus(Some(widget))? {
                        old = None;
                        self.focus_widget.borrow_mut().take();
                    }
                }
            }
        }
        if let Some(widget) = old.as_ref().and_then(|owner| owner.as_ff_widget()) {
            if !widget.on_kill_focus(new.as_ref().and_then(|owner| owner.as_ff_widget()))? {
                return Ok(false);
            }
        }
        if let Some(widget) = new.as_ref().and_then(|owner| owner.as_ff_widget()) {
            if widget.widget().has_visible_status() {
                if !widget.is_loaded() {
                    widget.load_widget()?;
                }
                if !widget.on_set_focus(old.as_ref().and_then(|owner| owner.as_ff_widget()))? {
                    new = None;
                }
            }
        }
        let node = new
            .as_ref()
            .and_then(|owner| owner.as_ff_widget())
            .map(|widget| widget.widget().node())
            .filter(|node| node.is_widget_ready())
            .cloned();
        *self.focus_node.borrow_mut() = node;
        *self.focus_widget.borrow_mut() = new;
        Ok(true)
    }
    pub fn set_focus_node(&self, node: Option<&CxfaNode>) -> FfResult<()> {
        if !self.set_focus(node.and_then(|node| self.get_widget_for_node(node)))? {
            return Ok(());
        }
        *self.focus_node.borrow_mut() = node.cloned();
        if self.status.get() == LayoutStatus::End {
            if let Some(doc) = self.doc() {
                doc.set_focus_widget(self.focus_widget());
            }
        }
        Ok(())
    }
    pub fn delete_layout_item(&self, widget: &dyn FfWidget) {
        if self
            .focus_widget()
            .as_ref()
            .and_then(|owner| owner.as_ff_widget())
            .is_some_and(|focused| std::ptr::eq(focused.widget(), widget.widget()))
        {
            self.focus_node.borrow_mut().take();
            self.focus_widget.borrow_mut().take();
        }
    }
    pub fn add_validate_node(&self, node: &CxfaNode) {
        let mut nodes = self.validate_nodes.borrow_mut();
        if !nodes.contains(node) {
            nodes.push_back(node.clone());
        }
    }
    pub fn add_calculate_node(&self, node: &CxfaNode) {
        let mut nodes = self.calculate_nodes.borrow_mut();
        if nodes.last() != Some(node) {
            nodes.push(node.clone());
        }
    }
    pub fn add_calculate_node_notify(&self, node: &CxfaNode) {
        let dependents = node.calculation_dependents().unwrap_or_default();
        for dependent in dependents {
            if dependent.is_alive()
                && !dependent.has_removed_children()
                && dependent.is_widget_ready()
            {
                self.add_calculate_node(&dependent);
            }
        }
    }
    pub fn run_calculate_widgets(&self) -> EventError {
        let Some(doc) = self.doc() else {
            return EventError::kNotExist;
        };
        if !doc.is_calculations_enabled() {
            return EventError::kDisabled;
        }
        let mut index = 0;
        loop {
            let Some(node) = self.calculate_nodes.borrow().get(index).cloned() else {
                break;
            };
            self.add_calculate_node_notify(&node);
            let count = node.get_calc_recursion_count().saturating_add(1);
            node.set_calc_recursion_count(count);
            if count > 11 {
                break;
            }
            if node.process_calculate(self) == EventError::kSuccess && node.is_widget_ready() {
                self.add_validate_node(&node);
            }
            index += 1;
        }
        let calculated = self.calculate_nodes.take();
        for node in calculated {
            if node.is_alive() {
                node.set_calc_recursion_count(0);
            }
        }
        EventError::kSuccess
    }
    pub fn run_validate(&self) {
        if self.doc().is_none_or(|doc| !doc.is_validations_enabled()) {
            return;
        }
        loop {
            let node = self.validate_nodes.borrow_mut().pop_front();
            let Some(node) = node else {
                break;
            };
            if node.is_alive() && !node.has_removed_children() {
                node.process_validate(self, 0);
            }
        }
    }
    fn init_calculate(&self, node: &CxfaNode) {
        self.exec_event_activity_by_deep_first(
            Some(node),
            XfaEventType::InitCalculate,
            false,
            true,
        );
    }
    fn init_validate(&self, node: &CxfaNode) {
        if self.doc().is_none_or(|doc| !doc.is_validations_enabled()) {
            return;
        }
        self.exec_event_activity_by_deep_first(Some(node), XfaEventType::Validate, false, true);
        self.validate_nodes.borrow_mut().clear();
    }
    pub fn process_value_changed(&self, node: &CxfaNode) {
        self.add_validate_node(node);
        self.add_calculate_node(node);
        self.run_calculate_widgets();
        self.run_validate();
    }
    pub fn add_bind_item(&self, node: &CxfaNode) {
        self.bind_items.borrow_mut().push_back(node.clone());
    }
    pub fn add_new_form_node(&self, node: &CxfaNode) {
        self.new_added_nodes.borrow_mut().push_back(node.clone());
        self.init_layout(node);
    }
    pub fn add_index_changed_subform(&self, node: &CxfaNode) {
        let mut nodes = self.index_changed_subforms.borrow_mut();
        if !nodes.contains(node) {
            nodes.push_back(node.clone());
        }
    }
    fn run_subform_index_change(&self) {
        let mut seen = HashSet::new();
        loop {
            let node = self.index_changed_subforms.borrow_mut().pop_front();
            let Some(node) = node else {
                break;
            };
            if !seen.insert(node.clone()) || !node.is_widget_ready() {
                continue;
            }
            let mut event = CxfaEventParam::new(XfaEventType::IndexChange);
            node.process_event(self, V::IndexChange, &mut event);
        }
    }
    pub fn add_null_test_msg(&self, message: &[u32]) {
        self.null_test_messages.borrow_mut().push(message.to_vec());
    }
    fn show_null_test_messages(&self) {
        let messages = self.null_test_messages.take();
        if messages.is_empty() {
            return;
        }
        let mut message = Vec::new();
        for value in messages.iter().take(7) {
            message.extend_from_slice(value);
            message.push(10);
        }
        if messages.len() > 7 {
            message.extend(wide(&format!(
                "\nMessage limit exceeded. Remaining {} validation errors not reported.",
                messages.len() - 7
            )));
        }
        if let Some(doc) = self.doc() {
            let _ = app_message(
                &doc.app(),
                &message,
                &doc.app().provider().get_app_title(),
                AlertIcon::kStatus,
                AlertButton::kOK,
            );
        }
    }
    pub fn update_doc_view(&self) -> Result<(), FfDocViewError> {
        if self.lock.get() > 0 {
            return Ok(());
        }
        self.lock_update();
        let _unlock = Unlock(self);
        // Initial layout drains this queue before initialization/calculation.
        // A data remerge also creates BindItems nodes, but regular RunLayout
        // does not call InitLayout. Populate those new choices before update
        // callbacks can inspect them, using the same queue and update lock.
        self.run_bind_items();
        loop {
            let node = self.new_added_nodes.borrow_mut().pop_front();
            let Some(node) = node else {
                break;
            };
            if !node.is_alive() {
                continue;
            }
            self.init_calculate(&node);
            self.init_validate(&node);
            self.exec_event_activity_by_deep_first(Some(&node), XfaEventType::Ready, true, true);
        }
        self.run_subform_index_change();
        self.run_calculate_widgets();
        self.run_validate();
        self.show_null_test_messages();
        if self.run_layout()? && self.layout_event.get() {
            if let Some(root) = self.form_root() {
                self.exec_event_activity_by_deep_first(
                    Some(&root),
                    XfaEventType::Ready,
                    false,
                    true,
                );
                self.run_layout()?;
            }
        }
        self.layout_event.set(false);
        self.calculate_nodes.borrow_mut().clear();
        Ok(())
    }
    pub fn run_doc_close(&self) {
        if let Some(root) = self.form_root() {
            self.exec_event_activity_by_deep_first(
                Some(&root),
                XfaEventType::DocClose,
                false,
                true,
            );
        }
    }
    pub fn set_change_mark(&self) {
        if self.status.get() == LayoutStatus::End {
            if let Some(doc) = self.doc() {
                doc.set_change_mark();
            }
        }
    }
    pub fn invalidate_caption(&self, node: &CxfaNode) {
        let layout = self.captions.borrow().get(node).cloned();
        if let Some(layout) = layout {
            let _ = layout.with_layout(|layout| {
                layout.unload();
                Ok(())
            });
        }
    }
}
struct RestoreLayoutStatus<'a> {
    slot: &'a Cell<bool>,
    previous: bool,
}
impl Drop for RestoreLayoutStatus<'_> {
    fn drop(&mut self) {
        self.slot.set(self.previous);
    }
}
struct Unlock<'a>(&'a CxfaFfDocView);
impl Drop for Unlock<'_> {
    fn drop(&mut self) {
        self.0.unlock_update();
    }
}
pub struct UpdateScope<'a>(&'a CxfaFfDocView);
impl<'a> UpdateScope<'a> {
    pub fn new(view: &'a CxfaFfDocView) -> Self {
        view.lock_update();
        Self(view)
    }
}
impl Drop for UpdateScope<'_> {
    fn drop(&mut self) {
        self.0.unlock_update();
        if let Err(error) = self.0.update_doc_view() {
            *self.0.last_update_error.borrow_mut() = Some(error);
        }
    }
}
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn same_widget(
    left: Option<&Rc<dyn LayoutItemWidget>>,
    right: Option<&Rc<dyn LayoutItemWidget>>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}

impl CxfaFfDocView {
    pub fn process_event(&self, node: &CxfaNode, event: &mut CxfaEventParam) -> EventError {
        if event.event_type == XfaEventType::Unknown || node.get_element_type() == E::Draw {
            return EventError::kNotExist;
        }
        match event.event_type {
            XfaEventType::Calculate => node.process_calculate(self),
            XfaEventType::Validate => {
                if self.doc().is_some_and(|doc| doc.is_validations_enabled()) {
                    node.process_validate(self, 1)
                } else {
                    EventError::kDisabled
                }
            }
            XfaEventType::InitCalculate => {
                if node.is_user_interactive() {
                    return EventError::kDisabled;
                }
                let script = node
                    .get_child(0, E::Calculate, false)
                    .and_then(|calc| calc.get_child(0, E::Script, false));
                node.execute_bool_script(self, script.as_ref(), event).0
            }
            kind => node.process_event(
                self,
                EVENT_ACTIVITY
                    .get(kind as usize)
                    .copied()
                    .unwrap_or(V::Unknown),
                event,
            ),
        }
    }
    pub fn exec_event_activity_by_deep_first(
        &self,
        node: Option<&CxfaNode>,
        kind: XfaEventType,
        form_ready: bool,
        recursive: bool,
    ) -> EventError {
        let Some(node) = node.filter(|node| node.is_alive()) else {
            return EventError::kNotExist;
        };
        if node.get_element_type() == E::Field {
            if kind == XfaEventType::IndexChange || !node.is_widget_ready() {
                return EventError::kNotExist;
            }
            let mut event = CxfaEventParam::new(kind);
            event.is_form_ready = form_ready;
            return self.process_event(node, &mut event);
        }
        let mut result = EventError::kNotExist;
        // Snapshot each next edge before callbacks can mutate the model.
        if recursive {
            let mut child = node.get_first_container_child();
            while let Some(current) = child {
                child = current.get_next_container_sibling();
                if !matches!(current.get_element_type(), E::Variables | E::Draw) {
                    event_error_accumulate(
                        &mut result,
                        self.exec_event_activity_by_deep_first(
                            Some(&current),
                            kind,
                            form_ready,
                            true,
                        ),
                    );
                }
            }
        }
        if node.is_widget_ready() {
            let mut event = CxfaEventParam::new(kind);
            event.is_form_ready = form_ready;
            event_error_accumulate(&mut result, self.process_event(node, &mut event));
        }
        result
    }
    fn run_bind_items(&self) {
        let Some(doc) = self.doc() else {
            return;
        };
        let Some(engine) = doc.try_script_context() else {
            return;
        };
        loop {
            let item = self.bind_items.borrow_mut().pop_front();
            let Some(item) = item else {
                break;
            };
            if !item.is_alive() || item.has_removed_children() {
                continue;
            }
            let Some(widget) = item.parent().filter(|node| node.is_widget_ready()) else {
                continue;
            };
            let expression = item.get_cdata(A::Ref);
            let result = engine
                .resolve_objects(
                    Some(&widget),
                    &expression,
                    XfaResolveFlags::CHILDREN
                        | XfaResolveFlags::PROPERTIES
                        | XfaResolveFlags::SIBLINGS
                        | XfaResolveFlags::PARENT
                        | XfaResolveFlags::ALL,
                )
                .ok()
                .flatten();
            widget.delete_item(-1);
            let Some(result) =
                result.filter(|result| result.result_type == ResolveResultType::Nodes)
            else {
                continue;
            };
            let value_ref = item.get_cdata(A::ValueRef);
            let label_ref = item.get_cdata(A::LabelRef);
            let use_value = label_ref.is_empty() || label_ref == value_ref;
            let label_content = label_ref.is_empty() || label_ref == wide("$");
            let value_content = value_ref.is_empty() || value_ref == wide("$");
            let mut label = Vec::new();
            for node in result.objects {
                let value = if value_content {
                    node.get_content()
                } else {
                    node.get_first_child_by_name(&value_ref)
                        .unwrap_or_else(|| node.clone())
                        .get_content()
                };
                if use_value {
                    label = value.clone();
                } else if label_content {
                    label = node.get_content();
                } else if let Some(node) = node.get_first_child_by_name(&label_ref) {
                    label = node.get_content();
                }
                widget.insert_choice_list_item(&label, &value);
            }
        }
    }
    fn reset_single_node_data(&self, node: &CxfaNode) -> bool {
        if !matches!(node.get_element_type(), E::Field | E::ExclGroup) {
            return false;
        }
        node.reset_data();
        self.invalidate_widget_layout(node);
        self.update_ui_display(node, None);
        if let Some(validate) = node.get_child(0, E::Validate, false) {
            self.add_validate_node(node);
            validate.set_flag(XfaNodeFlag::NeedsInitApp);
        }
        true
    }
    pub fn reset_node(&self, node: Option<&CxfaNode>) {
        self.layout_event.set(true);
        let mut changed = node.is_some_and(|node| self.reset_single_node_data(node));
        let Some(root) = node.cloned().or_else(|| self.root_subform()) else {
            return;
        };
        if !matches!(root.get_element_type(), E::Field | E::ExclGroup) {
            let mut iterator = CxfaReadyNodeIterator::new(Some(root));
            while let Some(node) = iterator.move_to_next() {
                changed |= self.reset_single_node_data(&node);
                if node.get_element_type() == E::ExclGroup {
                    iterator.skip_tree();
                }
            }
        }
        if changed {
            if let Some(doc) = self.doc() {
                doc.set_change_mark();
            }
        }
    }
}
struct DocFontHost(Weak<CxfaFfDoc>);
impl XfaTextFontHost for DocFontHost {
    fn get_font(&self, family: &[u32], styles: u32) -> Option<Rc<CfgasGeFont>> {
        self.0.upgrade()?.get_font(family, styles)
    }
}
impl FfDocViewHost for CxfaFfDocView {
    fn fwl_app(&self) -> Rc<CfwlApp> {
        self.fwl_app.clone()
    }
    fn is_interactive(&self) -> bool {
        self.doc()
            .is_some_and(|doc| doc.model_document().is_interactive())
    }
    fn page_rect(&self, page: &CxfaViewLayoutItem) -> CfxRectF {
        CfxRectF::from_point_size(Default::default(), page.get_page_size())
    }
    fn invalidate_rect(&self, page: Option<&CxfaViewLayoutItem>, rect: CfxRectF) {
        if let Some(doc) = self.doc() {
            doc.invalidate_rect(page, rect);
        }
    }
    fn display_caret(&self, widget: &dyn FfWidget, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(doc) = self.doc() {
            doc.display_caret(widget, visible, anchor);
        }
    }
    fn process_event(
        &self,
        node: &CxfaNode,
        activity: V,
        event: &mut CxfaEventParam,
    ) -> EventError {
        node.process_event(self, activity, event)
    }
    fn set_change_mark(&self) {
        CxfaFfDocView::set_change_mark(self);
    }
    fn add_validate_node(&self, node: &CxfaNode) {
        CxfaFfDocView::add_validate_node(self, node);
    }
    fn highlight_color(&self) -> u32 {
        self.doc().map_or(0, |doc| doc.get_highlight_color())
    }
    fn message_box(
        &self,
        message: &[u32],
        title: &[u32],
        icon: AlertIcon,
        buttons: AlertButton,
    ) -> Option<AlertReturn> {
        app_message(&self.doc()?.app(), message, title, icon, buttons)
    }
    fn get_popup_pos(
        &self,
        widget: &dyn FfWidget,
        min: f32,
        max: f32,
        anchor: CfxRectF,
        popup: &mut CfxRectF,
    ) -> bool {
        self.doc()
            .is_some_and(|doc| doc.get_popup_pos(widget, min, max, anchor, popup))
    }
    fn get_font(&self, node: &CxfaNode) -> Option<Rc<CfgasGeFont>> {
        let provider = CxfaTextProvider::new(node.clone(), TextProviderType::Text);
        super::cxfa_textparser::CxfaTextParser::new().get_font(
            &DocFontHost(self.doc.clone()),
            &provider,
            None,
        )
    }
    fn load_caption(&self, node: &CxfaNode) -> FfResult<Option<CaptionLayout>> {
        if let Some(layout) = self.caption_layout(node) {
            return Ok(Some(layout));
        }
        if !node.is_widget_ready() {
            return Ok(None);
        }
        let Some(caption) = node.get_child(0, E::Caption, false) else {
            return Ok(None);
        };
        if !caption.caption_is_visible() {
            return Ok(None);
        }
        if self.doc().is_none() {
            return Err(FfWidgetError::DetachedDocumentView);
        }
        let provider = Rc::new(CxfaTextProvider::new(
            node.clone(),
            TextProviderType::Caption,
        ));
        let layout = FfTextLayout::new(CxfaTextLayout::new(
            provider,
            Rc::new(DocFontHost(self.doc.clone())),
        )?);
        self.captions
            .borrow_mut()
            .insert(node.clone(), layout.clone());
        Ok(Some(layout))
    }
    fn caption_layout(&self, node: &CxfaNode) -> Option<CaptionLayout> {
        self.captions.borrow().get(node).cloned()
    }
    fn popup_menu(&self, widget: &dyn FfWidget, point: crate::fxcrt::fx_coordinates::CfxPointF) {
        if let Some(doc) = self.doc() {
            doc.popup_menu(widget, point);
        }
    }
    fn update_ui_display(&self, node: &CxfaNode, except: Option<&dyn FfWidget>) {
        CxfaFfDocView::update_ui_display(self, node, except);
    }
    fn app_title(&self) -> Vec<u32> {
        self.doc()
            .map_or_else(Vec::new, |doc| doc.app().provider().get_app_title())
    }
    fn set_focus_node(&self, node: Option<&CxfaNode>) -> FfResult<()> {
        CxfaFfDocView::set_focus_node(self, node)
    }
    fn image_owner(&self) -> Option<Rc<dyn super::cxfa_ffimage::XfaImageOwner>> {
        self.doc()
            .map(|doc| doc as Rc<dyn super::cxfa_ffimage::XfaImageOwner>)
    }
    fn text_layout(&self, node: &CxfaNode) -> Option<CaptionLayout> {
        self.text_layouts
            .borrow()
            .get(&(node.clone(), TextProviderType::Text as u8))
            .cloned()
    }
    fn load_text_layout(
        &self,
        node: &CxfaNode,
        kind: TextProviderType,
    ) -> FfResult<Option<CaptionLayout>> {
        if kind == TextProviderType::Caption {
            return self.load_caption(node);
        }
        if let Some(layout) = self
            .text_layouts
            .borrow()
            .get(&(node.clone(), kind as u8))
            .cloned()
        {
            return Ok(Some(layout));
        }
        if self.doc().is_none() {
            return Err(FfWidgetError::DetachedDocumentView);
        }
        let provider = Rc::new(CxfaTextProvider::new(node.clone(), kind));
        let layout = FfTextLayout::new(CxfaTextLayout::new(
            provider,
            Rc::new(DocFontHost(self.doc.clone())),
        )?);
        self.text_layouts
            .borrow_mut()
            .insert((node.clone(), kind as u8), layout.clone());
        Ok(Some(layout))
    }
    fn goto_url(&self, url: &[u32]) {
        if let Some(doc) = self.doc() {
            doc.goto_url(url);
        }
    }
    fn add_calculate_node(&self, node: &CxfaNode) {
        CxfaFfDocView::add_calculate_node(self, node);
    }
}
impl XfaWidgetScriptHost for CxfaFfDocView {
    fn exec_deep_first(
        &self,
        node: &CxfaNode,
        event: XfaEventType,
        ready: bool,
        recursive: bool,
    ) -> JsResult<EventError> {
        Ok(self.exec_event_activity_by_deep_first(Some(node), event, ready, recursive))
    }
    fn add_calc_validate(&self, node: &CxfaNode) -> JsResult<()> {
        self.add_calculate_node(node);
        self.add_validate_node(node);
        Ok(())
    }
}
impl XfaEventHost for CxfaFfDocView {
    fn handle_widget_event(
        &self,
        target: Option<&CxfaNode>,
        event: Rc<RefCell<CxfaEventParam>>,
    ) -> JsResult<()> {
        if let Some(node) = target {
            let mut value = event.borrow().clone();
            self.process_event(node, &mut value);
            *event.borrow_mut() = value;
        }
        Ok(())
    }
}

fn app_message(
    app: &super::cxfa_ffapp::CxfaFfApp,
    message: &[u32],
    title: &[u32],
    icon: AlertIcon,
    buttons: AlertButton,
) -> Option<AlertReturn> {
    match app
        .provider()
        .message_box(message, title, icon as u32, buttons as u32)
    {
        1 => Some(AlertReturn::kOK),
        2 => Some(AlertReturn::kCancel),
        3 => Some(AlertReturn::kNo),
        4 => Some(AlertReturn::kYes),
        _ => None,
    }
}

#[cfg(test)]
#[path = "cxfa_ffdocview_tests.rs"]
mod tests;
