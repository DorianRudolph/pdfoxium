// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Complete content layout, pagination and geometry from
//! `cxfa_contentlayoutprocessor.{h,cpp}`. View pagination is a scoped service;
//! recursive processors own their suspended child state and produced fragments.
//! Field sizing uses the supplied actual measurement host; it never guesses
//! widget sizes from model attributes or installs an FF success fallback.
use super::{
    cxfa_contentlayoutitem::CxfaContentLayoutItem,
    cxfa_layoutitem::{CxfaLayoutStore, LayoutItemWidget},
    cxfa_viewlayoutitem::CxfaViewLayoutItem,
};
use crate::{
    fxcrt::fx_coordinates::{CfxPointF, CfxSizeF},
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute as A, XfaAttributeValue as V, XfaElement as E, XfaUnit},
        parser::{
            cxfa_document::CxfaNodeStore, cxfa_measurement::CxfaMeasurement, cxfa_node::CxfaNode,
            xfa_utils::map_rotation,
        },
    },
};
use std::{collections::HashMap, rc::Rc};

pub const XFA_LAYOUT_PRECISION: f32 = 0.0005;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutResult {
    Done,
    PageFullBreak,
    RowFullBreak,
    ManualBreak,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutStage {
    None,
    BookendLeader,
    BreakBefore,
    Keep,
    Container,
    BreakAfter,
    BookendTrailer,
    Done,
}
#[derive(Debug)]
pub enum LayoutError {
    MissingFieldMeasurementHost,
    FieldMeasurementFailed,
    ExpiredNode,
    InvalidLayoutTree,
    InvalidSplitPosition,
    View(Box<super::cxfa_viewlayoutprocessor::ViewLayoutError>),
}
impl PartialEq for LayoutError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::View(left), Self::View(right)) => std::ptr::eq(&**left, &**right),
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}
impl Eq for LayoutError {}
/// The FF sizing/creation seam used by the native processor. Implementations
/// may execute real field/text layout and reenter the model. No layout borrow
/// is held while a host method is called.
pub trait ContentLayoutHost {
    fn find_split_pos(
        &self,
        node: &CxfaNode,
        fragment_index: usize,
        proposed: f32,
    ) -> Result<Option<f32>, LayoutError>;
    fn create_content_widget(&self, node: &CxfaNode) -> Option<Rc<dyn LayoutItemWidget>>;
    fn start_field_draw_layout(
        &self,
        node: &CxfaNode,
        size: CfxSizeF,
    ) -> Result<CfxSizeF, LayoutError>;
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MarginInset {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}
pub fn get_margin_inset(node: Option<&CxfaNode>) -> MarginInset {
    node.map_or(MarginInset::default(), |node| MarginInset {
        left: node.get_measure_in_unit(A::LeftInset, XfaUnit::Pt),
        top: node.get_measure_in_unit(A::TopInset, XfaUnit::Pt),
        right: node.get_measure_in_unit(A::RightInset, XfaUnit::Pt),
        bottom: node.get_measure_in_unit(A::BottomInset, XfaUnit::Pt),
    })
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpecifiedSize {
    pub size: CfxSizeF,
    pub width_auto: bool,
    pub height_auto: bool,
}
pub fn calculate_container_specified_size(node: &CxfaNode) -> SpecifiedSize {
    let mut result = SpecifiedSize {
        size: CfxSizeF::default(),
        width_auto: true,
        height_auto: true,
    };
    if matches!(node.get_element_type(), E::Subform | E::ExclGroup) {
        if let Some(value) = node
            .try_measure(A::W, false)
            .filter(|value| value.get_value() > XFA_LAYOUT_PRECISION)
        {
            result.size.width = value.to_unit(XfaUnit::Pt);
            result.width_auto = false;
        }
        if let Some(value) = node
            .try_measure(A::H, false)
            .filter(|value| value.get_value() > XFA_LAYOUT_PRECISION)
        {
            result.size.height = value.to_unit(XfaUnit::Pt);
            result.height_auto = false;
        }
    }
    if result.width_auto && node.get_element_type() == E::Subform {
        if let Some(value) = node
            .try_measure(A::MaxW, false)
            .filter(|value| value.get_value() > XFA_LAYOUT_PRECISION)
        {
            result.size.width = value.to_unit(XfaUnit::Pt);
            result.width_auto = false;
        }
        if let Some(value) = node
            .try_measure(A::MaxH, false)
            .filter(|value| value.get_value() > XFA_LAYOUT_PRECISION)
        {
            result.size.height = value.to_unit(XfaUnit::Pt);
            result.height_auto = false;
        }
    }
    result
}
pub fn calculate_container_component_size_from_content_size(
    node: &CxfaNode,
    specified: SpecifiedSize,
    content: CfxSizeF,
) -> CfxSizeF {
    let mut result = specified.size;
    let margin = node.get_first_child_by_class(E::Margin);
    if specified.width_auto {
        result.width = content.width;
        if let Some(margin) = &margin {
            for attr in [A::LeftInset, A::RightInset] {
                if let Some(value) = margin.try_measure(attr, false) {
                    result.width += value.to_unit(XfaUnit::Pt);
                }
            }
        }
    }
    if specified.height_auto {
        result.height = content.height;
        if let Some(margin) = &margin {
            for attr in [A::TopInset, A::BottomInset] {
                if let Some(value) = margin.try_measure(attr, false) {
                    result.height += value.to_unit(XfaUnit::Pt);
                }
            }
        }
    }
    result
}
pub fn get_layout(node: &CxfaNode) -> (V, bool) {
    if let Some(layout) = node.try_enum(A::Layout, false) {
        return (layout, false);
    }
    if node
        .parent()
        .is_some_and(|parent| parent.get_element_type() == E::Form)
    {
        (V::Tb, true)
    } else {
        (V::Position, false)
    }
}
pub fn get_subform_set_parent(node: &CxfaNode) -> CxfaNode {
    let mut current = node.clone();
    while current.get_element_type() == E::SubformSet {
        let Some(parent) = current.parent() else {
            break;
        };
        current = parent;
    }
    current
}
pub fn exist_container_keep(node: Option<&CxfaNode>, previous: bool) -> bool {
    let Some(node) = node.filter(|node| node.presence_requires_space()) else {
        return false;
    };
    let sibling = if previous {
        node.get_prev_container_sibling()
    } else {
        node.get_next_container_sibling()
    };
    let Some(sibling) = sibling else {
        return false;
    };
    let attr = if previous { A::Previous } else { A::Next };
    let opposite = if previous { A::Next } else { A::Previous };
    let applies = |node: &CxfaNode, attribute| {
        node.get_first_child_by_class(E::Keep)
            .and_then(|keep| keep.try_enum(attribute, false))
            .is_some_and(|value| matches!(value, V::ContentArea | V::PageArea))
    };
    applies(node, attr) || applies(&sibling, opposite)
}
pub fn calculate_positioned_container_pos(node: &CxfaNode, size: CfxSizeF) -> CfxPointF {
    const ROTATIONS: [[usize; 9]; 4] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8],
        [6, 3, 0, 7, 4, 1, 8, 5, 2],
        [8, 7, 6, 5, 4, 3, 2, 1, 0],
        [2, 5, 8, 1, 4, 7, 0, 3, 6],
    ];
    let anchor = match node.get_enum(A::AnchorType) {
        V::TopCenter => 1,
        V::TopRight => 2,
        V::MiddleLeft => 3,
        V::MiddleCenter => 4,
        V::MiddleRight => 5,
        V::BottomLeft => 6,
        V::BottomCenter => 7,
        V::BottomRight => 8,
        _ => 0,
    };
    let rotated = ROTATIONS[(map_rotation(node.get_integer(A::Rotate)) / 90) as usize][anchor];
    CfxPointF::new(
        node.get_measure_in_unit(A::X, XfaUnit::Pt) - (rotated % 3) as f32 * size.width / 2.,
        node.get_measure_in_unit(A::Y, XfaUnit::Pt) - (rotated / 3) as f32 * size.height / 2.,
    )
}
fn h_align(value: V) -> usize {
    match value {
        V::Center => 1,
        V::Right => 2,
        _ => 0,
    }
}
fn content_children(item: &CxfaContentLayoutItem) -> impl Iterator<Item = CxfaContentLayoutItem> {
    item.children()
        .into_iter()
        .filter_map(|item| item.as_content_layout_item())
}
fn update_widget_size(
    item: &CxfaContentLayoutItem,
    mut size: CfxSizeF,
    host: Option<&dyn ContentLayoutHost>,
) -> Result<CfxSizeF, LayoutError> {
    match item.form_node().get_element_type() {
        E::Subform | E::Area | E::ExclGroup | E::SubformSet => {
            if size.width < -XFA_LAYOUT_PRECISION {
                size.width = item.size().width;
            }
            if size.height < -XFA_LAYOUT_PRECISION {
                size.height = item.size().height;
            }
            Ok(size)
        }
        E::Draw | E::Field => host
            .ok_or(LayoutError::MissingFieldMeasurementHost)?
            .start_field_draw_layout(&item.form_node(), size),
        _ => Err(LayoutError::InvalidLayoutTree),
    }
}
pub fn relocate_table_row_cells(
    row: &CxfaContentLayoutItem,
    columns: &[f32],
    host: Option<&dyn ContentLayoutHost>,
) -> Result<(), LayoutError> {
    let node = row.form_node();
    let specified = calculate_container_specified_size(&node);
    let inset = get_margin_inset(node.get_first_child_by_class(E::Margin).as_ref());
    let limit = if specified.width_auto {
        f32::MAX
    } else {
        specified.size.width - inset.left - inset.right
    };
    let content_height = row.size().height - inset.top - inset.bottom;
    let (mut x, mut column, mut whole, mut height) = (0f32, 0usize, false, 0f32);
    for child in content_children(row) {
        let original = child.form_node().get_integer(A::ColSpan);
        let span = match original {
            1.. => original as usize,
            -1 => columns.len(),
            _ => continue,
        }
        .min(columns.len() - column);
        let mut width = columns[column..column + span].iter().sum::<f32>();
        if original == -1 || span != original as usize {
            width = if whole {
                0.
            } else {
                width.max(child.size().height)
            };
        }
        if original == -1 {
            whole = true;
        }
        child.set_position(CfxPointF::new(x, 0.));
        child.set_size(CfxSizeF::new(width, child.size().height));
        if !child.form_node().presence_requires_space() {
            continue;
        }
        x += width;
        column += span;
        let updated = update_widget_size(
            &child,
            CfxSizeF::new(
                width,
                if specified.height_auto {
                    -1.
                } else {
                    content_height
                },
            ),
            host,
        )?;
        child.set_size(CfxSizeF::new(child.size().width, updated.height));
        if specified.height_auto {
            height = height.max(updated.height);
        }
    }
    if specified.height_auto {
        for child in content_children(row) {
            let updated =
                update_widget_size(&child, CfxSizeF::new(child.size().width, height), host)?;
            height = updated.height;
            let old_height = child.size().height;
            child.set_size(CfxSizeF::new(updated.width, height));
            let Some(para) = child.form_node().get_first_child_by_class(E::Para) else {
                continue;
            };
            let offset = match para.get_enum(A::VAlign) {
                V::Middle => (height - old_height) / 2.,
                V::Bottom => height - old_height,
                _ => 0.,
            };
            if offset > 0. {
                for inner in content_children(&child) {
                    let position = inner.position();
                    inner.set_position(CfxPointF::new(position.x, position.y + offset));
                }
            }
        }
    }
    let width = if specified.width_auto {
        if limit < f32::MAX {
            x.max(limit)
        } else {
            x
        }
    } else {
        specified.size.width - inset.left - inset.right
    };
    if node.get_enum(A::Layout) == V::Rl_row {
        for child in content_children(row) {
            let position = child.position();
            child.set_position(CfxPointF::new(
                width - position.x - child.size().width,
                position.y,
            ));
        }
    }
    row.set_size(calculate_container_component_size_from_content_size(
        &node,
        specified,
        CfxSizeF::new(width, height),
    ));
    Ok(())
}

pub struct CxfaContentLayoutProcessor {
    form_node: CxfaNode,
    // Retain the form node before first allocation, without retaining its doc.
    _node_store: Rc<CxfaNodeStore>,
    store: Rc<CxfaLayoutStore>,
    host: Option<Rc<dyn ContentLayoutHost>>,
    layout_item: Option<CxfaContentLayoutItem>,
    old_layout_item: Option<CxfaContentLayoutItem>,
    pub stage: LayoutStage,
    pub specified_column_widths: Vec<f32>,
    pub last_row_width: f32,
    pub last_row_y: f32,
    pending_nodes_count: HashMap<CxfaNode, i32>,
    pages: PaginationState,
}
impl CxfaContentLayoutProcessor {
    pub fn new(
        form_node: &CxfaNode,
        store: Rc<CxfaLayoutStore>,
        host: Option<Rc<dyn ContentLayoutHost>>,
    ) -> Self {
        Self {
            form_node: form_node.clone(),
            _node_store: form_node
                .retain_store()
                .expect("layout form node has expired"),
            store,
            host,
            layout_item: None,
            old_layout_item: form_node
                .get_layout_item()
                .and_then(|item| item.as_content_layout_item()),
            stage: LayoutStage::None,
            specified_column_widths: Vec::new(),
            last_row_width: 0.,
            last_row_y: 0.,
            pending_nodes_count: HashMap::new(),
            pages: PaginationState::default(),
        }
    }
    pub fn form_node(&self) -> CxfaNode {
        self.form_node.clone()
    }
    pub fn layout_item(&self) -> Option<CxfaContentLayoutItem> {
        self.layout_item.clone()
    }
    pub fn create_content_layout_item(&mut self, node: &CxfaNode) -> CxfaContentLayoutItem {
        if let Some(old) = self.old_layout_item.take() {
            self.old_layout_item = old.next();
            // Rebuild this fragment's children for the current pass. The
            // arena and each child's node association keep its allocation
            // available for the child processor's own reuse; tree edges from
            // the previous pass must not enter a newly split continuation.
            while let Some(child) = old.first_child() {
                child.remove_self_if_parented();
            }
            return old;
        }
        let widget = self
            .host
            .as_ref()
            .and_then(|host| host.create_content_widget(node));
        let item = self.store.create_content(node, widget);
        if let Some(previous) = node
            .get_layout_item()
            .and_then(|item| item.as_content_layout_item())
        {
            previous.get_last().insert_after(&item);
        } else {
            node.set_layout_item(Some((*item).clone()));
        }
        item
    }
    pub fn extract_layout_item(&mut self) -> Option<CxfaContentLayoutItem> {
        let item = self.layout_item.take();
        if let Some(item) = &item {
            self.layout_item = item
                .next_sibling()
                .and_then(|next| next.as_content_layout_item())
                .or_else(|| self.pages.detached_fragments.pop_front());
            item.remove_self_if_parented();
        }
        if self.stage == LayoutStage::Done {
            while let Some(old) = self.old_layout_item.take() {
                self.old_layout_item = old.next();
                if Some(&old) == item.as_ref() {
                    break;
                }
                if let Some(notify) = self.store.notify() {
                    notify.on_layout_item_removing(&old);
                }
                old.retire_from_layout();
            }
        }
        item
    }
    pub fn judge_leader_or_trailer_for_occur(&mut self, node: Option<&CxfaNode>) -> bool {
        let Some(node) = node else {
            return false;
        };
        let template = node
            .get_template_node_if_exists()
            .unwrap_or_else(|| node.clone());
        let Some(occur) = template.get_first_child_by_class(E::Occur) else {
            return false;
        };
        let maximum = occur.occur_get_max();
        if maximum < 0 {
            return true;
        }
        let count = self.pending_nodes_count.entry(template).or_default();
        if *count >= maximum {
            return false;
        }
        *count += 1;
        true
    }
    /// Native `DoLayout(false, FLT_MAX, FLT_MAX)` path, with no view processor.
    /// This is also the actual path used to size positioned master-page content.
    /// Paginated callers must use the future connected keep/break state machine.
    pub fn layout_unpaginated(&mut self) -> Result<LayoutResult, LayoutError> {
        self.layout_unpaginated_internal(None, None)?;
        Ok(LayoutResult::Done)
    }
    fn layout_unpaginated_internal(
        &mut self,
        columns: Option<&[f32]>,
        column_width: Option<f32>,
    ) -> Result<(), LayoutError> {
        let mut context = LayoutContext {
            columns,
            column_width,
            ..Default::default()
        };
        self.do_layout_internal(None, false, f32::MAX, f32::MAX, &mut context)
            .map(|_| ())
    }

    fn child_processor(&self, node: &CxfaNode) -> Self {
        Self::new(node, self.store.clone(), self.host.clone())
    }
    pub fn do_layout_positioned_container(
        &mut self,
        columns: Option<&[f32]>,
    ) -> Result<(), LayoutError> {
        self.do_layout_positioned_internal(
            None,
            &mut LayoutContext {
                columns,
                ..Default::default()
            },
        )
    }
    pub fn do_layout_page_area(&self, page: &CxfaViewLayoutItem) -> Result<(), LayoutError> {
        self.do_layout_page_area_internal(page)
    }
    pub fn do_layout_table_container(&mut self, layout_node: &CxfaNode) -> Result<(), LayoutError> {
        self.do_layout_table_internal(None, layout_node)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn calculate_row_child_position(
        &mut self,
        groups: &[Vec<CxfaContentLayoutItem>; 3],
        flow: V,
        specified: SpecifiedSize,
        calculated: &mut CfxSizeF,
        y: &mut f32,
        height: f32,
        limit: f32,
        force_tb: bool,
    ) -> bool {
        if groups.iter().all(Vec::is_empty) {
            if specified.height_auto {
                calculated.height = calculated.height.min(*y);
            }
            return false;
        }
        if self.layout_item.is_none() {
            let node = self.form_node.clone();
            self.layout_item = Some(self.create_content_layout_item(&node));
        }
        let parent = self.layout_item.clone().unwrap();
        let widths: [f32; 3] = std::array::from_fn(|group| {
            groups[group]
                .iter()
                .filter(|item| item.form_node().presence_requires_space())
                .map(|item| item.size().width)
                .sum()
        });
        let starts = if flow == V::Rl_tb {
            [
                widths[0],
                (limit + widths[0] + widths[1] - widths[2]) / 2.,
                limit,
            ]
        } else {
            [
                0.,
                (limit + widths[0] - widths[1] - widths[2]) / 2.,
                limit - widths[2],
            ]
        };
        for group in 0..3 {
            let mut x = starts[group];
            for item in &groups[group] {
                let takes_space = item.form_node().presence_requires_space();
                if flow == V::Rl_tb {
                    if takes_space {
                        x -= item.size().width;
                    }
                    item.set_position(CfxPointF::new(x, *y));
                } else if force_tb {
                    item.set_position(calculate_positioned_container_pos(
                        &item.form_node(),
                        item.size(),
                    ));
                } else {
                    item.set_position(CfxPointF::new(x, *y));
                    if takes_space {
                        x += item.size().width;
                    }
                }
                parent.append_last_child(item);
                self.last_row_width = x;
            }
        }
        self.last_row_y = *y;
        *y += height;
        if specified.width_auto {
            let supplied = if limit < f32::MAX {
                widths[0].max(limit)
            } else {
                widths[0]
            };
            calculated.width = calculated.width.max(supplied);
        }
        if specified.height_auto {
            calculated.height = calculated.height.max(*y);
        }
        true
    }
}

include!("cxfa_contentlayoutprocessor_pages.rs");
