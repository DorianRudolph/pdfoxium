// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Actual page geometry, filtered layout traversal and spatial/explicit tab
//! order from `cxfa_ffpageview.{h,cpp}`.
use super::{
    cxfa_ffdocview::CxfaFfDocView,
    cxfa_ffwidget::{CxfaFfWidget, FfWidget, FfWidgetError, FfWidgetIterator},
    fxfa::XfaWidgetStatus as S,
    fxfa_basic::{XfaAttribute as A, XfaElement as E},
    layout::{
        cxfa_contentlayoutitem::CxfaContentLayoutItem,
        cxfa_layoutitem::{CxfaLayoutItem, CxfaLayoutStore, LayoutItemPageView, LayoutItemWidget},
        cxfa_traversestrategy_layoutitem::CxfaLayoutItemIterator,
        cxfa_viewlayoutitem::CxfaViewLayoutItem,
    },
    parser::{
        cxfa_document::CxfaNodeStore,
        cxfa_node::{CxfaNode, XfaFfWidgetType},
    },
};
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxRectF, FxRect};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub struct CxfaFfPageView {
    page_area: CxfaNode,
    _node_store: Rc<CxfaNodeStore>,
    doc_view: Weak<CxfaFfDocView>,
    layout_item: RefCell<Option<CxfaViewLayoutItem>>,
}
impl CxfaFfPageView {
    pub fn new(view: Weak<CxfaFfDocView>, page_area: &CxfaNode) -> Rc<Self> {
        Rc::new(Self {
            page_area: page_area.clone(),
            _node_store: page_area.retain_store().expect("page area owner expired"),
            doc_view: view,
            layout_item: RefCell::new(None),
        })
    }
    pub fn page_area(&self) -> &CxfaNode {
        &self.page_area
    }
    pub fn doc_view(&self) -> Option<Rc<CxfaFfDocView>> {
        self.doc_view.upgrade()
    }
    pub fn layout_item(&self) -> Option<CxfaViewLayoutItem> {
        self.layout_item
            .borrow()
            .clone()
            .filter(|item| item.is_alive())
    }
    pub fn get_page_view_rect(&self) -> CfxRectF {
        self.layout_item().map_or_else(CfxRectF::default, |item| {
            CfxRectF::from_point_size(Default::default(), item.get_page_size())
        })
    }
    pub fn get_display_matrix_for_rect(&self, display: FxRect, rotation: i32) -> Option<CfxMatrix> {
        if !(0..=3).contains(&rotation) {
            return None;
        }
        let item = self.layout_item()?;
        let size = item.get_page_size();
        if size.width <= 0.0
            || size.height <= 0.0
            || !size.width.is_finite()
            || !size.height.is_finite()
        {
            return None;
        }
        let mut matrix = CfxMatrix::default();
        if rotation == 0 || rotation == 2 {
            matrix.a *= display.width() as f32 / size.width;
            matrix.d *= display.height() as f32 / size.height;
        } else {
            matrix.a *= display.height() as f32 / size.width;
            matrix.d *= display.width() as f32 / size.height;
        }
        matrix.rotate(rotation as f32 * 1.57079632675);
        (matrix.e, matrix.f) = match rotation {
            0 => (display.left as f32, display.top as f32),
            1 => (display.right as f32, display.top as f32),
            2 => (display.right as f32, display.bottom as f32),
            _ => (display.left as f32, display.bottom as f32),
        };
        Some(matrix)
    }
    fn ignore_relevant(&self) -> bool {
        self.doc_view()
            .and_then(|view| view.doc())
            .is_some_and(|doc| doc.model_document().get_cur_version_mode() < 205)
    }
    pub fn create_traverse_widget_iterator(&self, filter: S) -> CxfaFfTabOrderPageWidgetIterator {
        CxfaFfTabOrderPageWidgetIterator::new(self, filter)
    }
    pub fn create_widget_iterator(&self, filter: S) -> CxfaFfPageWidgetIterator {
        CxfaFfPageWidgetIterator::new(self, filter)
    }
}
impl LayoutItemPageView for CxfaFfPageView {
    fn as_ff_page_view(&self) -> Option<&CxfaFfPageView> {
        Some(self)
    }
    fn set_layout_item(&self, item: Option<CxfaViewLayoutItem>) {
        *self.layout_item.borrow_mut() = item;
    }
}
fn page_widget_filter(
    widget: &dyn FfWidget,
    filter: S,
    traversal: bool,
    ignore_relevant: bool,
) -> bool {
    if filter.contains(S::kFocused) && widget.widget().node().get_element_type() != E::Field {
        return false;
    }
    let Some(item) = widget.widget().layout_item() else {
        return false;
    };
    if traversal && item.test_status_bits(S::kDisabled) {
        return false;
    }
    if ignore_relevant {
        return item.test_status_bits(S::kVisible);
    }
    item.test_status_bits(filter & (S::kVisible | S::kViewable | S::kPrintable))
}
fn load_visible_widget(owner: &Rc<dyn LayoutItemWidget>) -> Result<bool, FfWidgetError> {
    if let Some(widget) = owner.as_ff_widget() {
        if !widget.is_loaded() && widget.widget().has_visible_status() {
            return widget.load_widget();
        }
    }
    Ok(true)
}
pub struct CxfaFfPageWidgetIterator {
    iterator: CxfaLayoutItemIterator,
    _store: Option<Rc<CxfaLayoutStore>>,
    filter: S,
    ignore_relevant: bool,
    last_error: Option<FfWidgetError>,
}
impl CxfaFfPageWidgetIterator {
    pub fn new(page: &CxfaFfPageView, filter: S) -> Self {
        let item = page.layout_item();
        Self {
            iterator: CxfaLayoutItemIterator::new(item.as_ref().map(|item| item.0.clone())),
            _store: item.and_then(|item| item.store()),
            filter,
            ignore_relevant: page.ignore_relevant(),
            last_error: None,
        }
    }
    pub fn take_error(&mut self) -> Option<FfWidgetError> {
        self.last_error.take()
    }
    fn candidate(&mut self, item: &CxfaLayoutItem) -> Option<Rc<dyn LayoutItemWidget>> {
        let owner = CxfaFfWidget::from_layout_item(item)?;
        if !page_widget_filter(
            owner.as_ff_widget()?,
            self.filter,
            false,
            self.ignore_relevant,
        ) {
            return None;
        }
        match load_visible_widget(&owner) {
            Ok(true) => Some(owner),
            Ok(false) => None,
            Err(error) => {
                self.last_error = Some(error);
                None
            }
        }
    }
}
impl FfWidgetIterator for CxfaFfPageWidgetIterator {
    fn move_to_first(&mut self) -> Option<Rc<dyn LayoutItemWidget>> {
        self.iterator.reset();
        let mut item = self.iterator.get_current();
        while let Some(current) = item {
            if let Some(widget) = self.candidate(&current) {
                return Some(widget);
            }
            item = self.iterator.move_to_next();
        }
        None
    }
    fn move_to_last(&mut self) -> Option<Rc<dyn LayoutItemWidget>> {
        self.iterator.set_current(None);
        self.move_to_previous()
    }
    fn move_to_next(&mut self) -> Option<Rc<dyn LayoutItemWidget>> {
        let mut item = self.iterator.move_to_next();
        while let Some(current) = item {
            if let Some(widget) = self.candidate(&current) {
                return Some(widget);
            }
            item = self.iterator.move_to_next();
        }
        None
    }
    fn move_to_previous(&mut self) -> Option<Rc<dyn LayoutItemWidget>> {
        let mut item = self.iterator.move_to_prev();
        while let Some(current) = item {
            if let Some(widget) = self.candidate(&current) {
                return Some(widget);
            }
            item = self.iterator.move_to_prev();
        }
        None
    }
    fn current_widget(&self) -> Option<Rc<dyn LayoutItemWidget>> {
        CxfaFfWidget::from_layout_item(&self.iterator.get_current()?)
    }
    fn set_current_widget(&mut self, widget: &dyn FfWidget) -> bool {
        widget
            .widget()
            .layout_item()
            .is_some_and(|item| self.iterator.set_current(Some(item.0)))
    }
}
struct TabParam {
    item: CxfaContentLayoutItem,
    children: Vec<CxfaContentLayoutItem>,
}
fn is_layout_element(element: E) -> bool {
    matches!(
        element,
        E::Area | E::Subform | E::ExclGroup | E::SubformSet | E::PageArea | E::Form
    )
}
fn order_container(
    iterator: &mut CxfaLayoutItemIterator,
    parent: Option<&CxfaLayoutItem>,
    out: &mut Vec<CxfaContentLayoutItem>,
    current_item: &mut bool,
    content_area: &mut bool,
    master: bool,
    depth: usize,
) {
    if depth >= 512 {
        return;
    }
    let mut params: Vec<TabParam> = Vec::new();
    let mut search = iterator.move_to_next();
    while let Some(item) = search {
        let Some(content) = item.as_content_layout_item() else {
            *content_area = true;
            search = iterator.move_to_next();
            continue;
        };
        if master && *content_area {
            break;
        }
        if master || *content_area {
            let Some(widget) = CxfaFfWidget::from_layout_item(&item) else {
                search = iterator.move_to_next();
                continue;
            };
            let _ = load_visible_widget(&widget);
            if parent.is_some_and(|parent| item.parent().as_ref() != Some(parent)) {
                *current_item = true;
                break;
            }
            let mut param = TabParam {
                item: content,
                children: Vec::new(),
            };
            if is_layout_element(item.form_node().get_element_type()) {
                order_container(
                    iterator,
                    Some(&item),
                    &mut param.children,
                    current_item,
                    content_area,
                    master,
                    depth + 1,
                );
            }
            params.push(param);
        }
        if *current_item {
            search = iterator.get_current();
            *current_item = false;
        } else {
            search = iterator.move_to_next();
        }
    }
    // Use a consistent ordering: native's asymmetric y comparison violated
    // strict weak ordering and could feed undefined behavior to std::sort.
    params.sort_by(|left, right| {
        let left = left
            .item
            .widget()
            .and_then(|owner| {
                owner
                    .as_ff_widget()
                    .map(|widget| widget.widget().get_widget_rect())
            })
            .unwrap_or_default();
        let right = right
            .item
            .widget()
            .and_then(|owner| {
                owner
                    .as_ff_widget()
                    .map(|widget| widget.widget().get_widget_rect())
            })
            .unwrap_or_default();
        left.top
            .total_cmp(&right.top)
            .then_with(|| left.left.total_cmp(&right.left))
    });
    for param in params {
        out.push(param.item);
        out.extend(param.children);
    }
}
pub struct CxfaFfTabOrderPageWidgetIterator {
    items: Vec<CxfaContentLayoutItem>,
    _store: Option<Rc<CxfaLayoutStore>>,
    filter: S,
    ignore_relevant: bool,
    current: Option<usize>,
}
impl CxfaFfTabOrderPageWidgetIterator {
    pub fn new(page: &CxfaFfPageView, filter: S) -> Self {
        let root = page.layout_item();
        let store = root.as_ref().and_then(|item| item.store());
        let mut iterator = CxfaLayoutItemIterator::new(root.as_ref().map(|item| item.0.clone()));
        let mut items = Vec::new();
        let (mut current, mut area) = (false, false);
        order_container(
            &mut iterator,
            None,
            &mut items,
            &mut current,
            &mut area,
            false,
            0,
        );
        iterator.reset();
        current = false;
        area = false;
        order_container(
            &mut iterator,
            None,
            &mut items,
            &mut current,
            &mut area,
            true,
            0,
        );
        let mut ordered = Vec::with_capacity(items.len());
        if let Some(first) = items.first() {
            let mut item = first.clone();
            while ordered.len() < items.len() {
                if !ordered.contains(&item) {
                    ordered.push(item.clone());
                    if item.form_node().get_ff_widget_type() == XfaFfWidgetType::ExclGroup {
                        let start = items
                            .iter()
                            .position(|candidate| candidate == &item)
                            .map_or(0, |index| index + 1);
                        for offset in 0..items.len() {
                            let candidate = &items[(start + offset) % items.len()];
                            if candidate.form_node().parent().as_ref() != Some(&item.form_node()) {
                                break;
                            }
                            if !ordered.contains(candidate) {
                                ordered.push(candidate.clone());
                            }
                        }
                    }
                    let target = item
                        .form_node()
                        .get_child(0, E::Traversal, false)
                        .and_then(|traversal| traversal.get_child(0, E::Traverse, false))
                        .and_then(|traverse| traverse.try_attribute(A::Ref, true));
                    if let Some(reference) = target {
                        if let (Some(view), Some(owner)) = (page.doc_view(), item.widget()) {
                            if let Some(next) = view
                                .get_widget_by_name(&reference, owner.as_ff_widget())
                                .and_then(|owner| {
                                    owner
                                        .as_ff_widget()
                                        .and_then(|widget| widget.widget().layout_item())
                                })
                            {
                                if items.contains(&next) {
                                    item = next;
                                    continue;
                                }
                            }
                        }
                    }
                }
                let next = items
                    .iter()
                    .position(|candidate| candidate == &item)
                    .map_or(0, |index| index + 1)
                    % items.len();
                item = items[next].clone();
            }
        }
        Self {
            items: ordered,
            _store: store,
            filter,
            ignore_relevant: page.ignore_relevant(),
            current: None,
        }
    }
    fn at(&self, index: usize) -> Option<Rc<dyn LayoutItemWidget>> {
        let owner = self.items.get(index)?.widget()?;
        page_widget_filter(
            owner.as_ff_widget()?,
            self.filter,
            true,
            self.ignore_relevant,
        )
        .then_some(owner)
    }
}
impl FfWidgetIterator for CxfaFfTabOrderPageWidgetIterator {
    fn move_to_first(&mut self) -> Option<Rc<dyn LayoutItemWidget>> {
        for index in 0..self.items.len() {
            if let Some(widget) = self.at(index) {
                self.current = Some(index);
                return Some(widget);
            }
        }
        None
    }
    fn move_to_last(&mut self) -> Option<Rc<dyn LayoutItemWidget>> {
        for index in (0..self.items.len()).rev() {
            if let Some(widget) = self.at(index) {
                self.current = Some(index);
                return Some(widget);
            }
        }
        None
    }
    fn move_to_next(&mut self) -> Option<Rc<dyn LayoutItemWidget>> {
        for index in self.current.map_or(0, |index| index + 1)..self.items.len() {
            if let Some(widget) = self.at(index) {
                self.current = Some(index);
                return Some(widget);
            }
        }
        self.current = None;
        None
    }
    fn move_to_previous(&mut self) -> Option<Rc<dyn LayoutItemWidget>> {
        for index in (0..self.current.unwrap_or(0)).rev() {
            if let Some(widget) = self.at(index) {
                self.current = Some(index);
                return Some(widget);
            }
        }
        self.current = None;
        None
    }
    fn current_widget(&self) -> Option<Rc<dyn LayoutItemWidget>> {
        self.items.get(self.current?)?.widget()
    }
    fn set_current_widget(&mut self, widget: &dyn FfWidget) -> bool {
        let Some(item) = widget.widget().layout_item() else {
            return false;
        };
        let Some(index) = self.items.iter().position(|candidate| candidate == &item) else {
            return false;
        };
        self.current = Some(index);
        true
    }
}
