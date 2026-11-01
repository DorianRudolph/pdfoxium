// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived layout graph and unpaginated layout tests. These are not
//! substitutions for the four PDF/FF pagination embedder regressions.
use super::{
    cxfa_contentlayoutprocessor::*, cxfa_layoutitem::*, cxfa_layoutprocessor::CxfaLayoutProcessor,
    cxfa_traversestrategy_layoutitem::*,
};
use crate::{
    fxcrt::fx_coordinates::{CfxPointF, CfxRectF, CfxSizeF},
    xfa::fxfa::{
        fxfa::XfaWidgetStatus as S,
        fxfa_basic::{
            XfaAttribute as A, XfaAttributeValue as V, XfaElement as E, XfaPacketType as P, XfaUnit,
        },
        parser::{
            cxfa_document::CxfaDocument, cxfa_measurement::CxfaMeasurement, cxfa_node::CxfaNode,
        },
    },
};
use std::{cell::RefCell, rc::Rc};
fn node(doc: &Rc<CxfaDocument>, element: E) -> CxfaNode {
    doc.create_node(P::Form, element).unwrap()
}
fn child(doc: &Rc<CxfaDocument>, parent: &CxfaNode, element: E) -> CxfaNode {
    let node = node(doc, element);
    assert!(parent.insert_child_and_notify(-1, &node));
    node
}
fn measure(node: &CxfaNode, attribute: A, value: f32) {
    node.set_measure(attribute, CxfaMeasurement::new(value, XfaUnit::Pt), false);
}
fn set_enum(node: &CxfaNode, attribute: A, value: V) {
    node.set_enum(attribute, value, false);
}

#[test]
fn fragment_identity_relink_status_and_arena_teardown() {
    let doc = CxfaDocument::new();
    let form = node(&doc, E::Subform);
    let store = CxfaLayoutStore::new();
    let first = store.create_content(&form, None);
    let second = store.create_content(&form, None);
    let third = store.create_content(&form, None);
    form.set_layout_item(Some((*first).clone()));
    first.insert_after(&second);
    second.insert_after(&third);
    assert_eq!(third.get_first(), first);
    assert_eq!(first.get_last(), third);
    assert_eq!(third.get_index(), 2);
    first.insert_after(&third);
    assert_eq!(third.next(), Some(second.clone()));
    assert_eq!(second.get_index(), 2);
    first.append_last_child(&second);
    assert_eq!(second.parent(), Some((*first).clone()));
    third.insert_after(&second);
    assert_eq!(second.parent(), Some((*first).clone()));
    second.set_status_bits(S::kVisible | S::kViewable);
    assert!(second.test_status_bits(S::kVisible | S::kViewable));
    second.clear_status_bits(S::kVisible);
    assert_eq!(second.status(), S::kViewable);
    second.pre_finalize();
    assert_eq!(form.get_layout_item(), Some((*first).clone()));
    drop(store);
    assert!(!first.is_alive());
    assert!(form.get_layout_item().is_none());
}
#[test]
fn repeated_fragment_relayout_retires_unused_tail_without_accumulating_scroll() {
    let doc = CxfaDocument::new();
    let field = node(&doc, E::Field);
    let store = CxfaLayoutStore::new();
    // Alternate long and short layouts while retaining the arena and the old
    // handles, just as loaded widgets survive document relayout.
    let mut retired = Vec::new();
    for count in [5, 2, 5, 1, 5] {
        let mut layout = CxfaContentLayoutProcessor::new(&field, store.clone(), None);
        let fragments: Vec<_> = (0..count)
            .map(|_| {
                let item = layout.create_content_layout_item(&field);
                item.set_size(CfxSizeF::new(120., 180.));
                item
            })
            .collect();
        let mut unused = fragments.last().unwrap().next();
        while let Some(item) = unused {
            unused = item.next();
            retired.push(item);
        }
        layout.stage = LayoutStage::Done;
        assert!(layout.extract_layout_item().is_none());
        assert_eq!(fragments[0].get_last(), fragments[count - 1]);
        for (index, fragment) in fragments.iter().enumerate() {
            assert_eq!(fragment.get_index(), index);
            let mut previous = fragment.previous();
            let mut scroll = 0.;
            while let Some(item) = previous {
                scroll += item.size().height;
                previous = item.previous();
            }
            assert_eq!(scroll, index as f32 * 180.);
        }
        for item in &retired {
            assert!(item.previous().is_none());
            assert!(item.next().is_none());
        }
    }
    // Releasing the head must leave the remaining live fragments reachable;
    // releasing the rest must clear the node back-reference altogether.
    let first = field.get_layout_item().unwrap();
    let next = first.as_content_layout_item().unwrap().next().unwrap();
    release_layout_item(&first);
    assert_eq!(field.get_layout_item(), Some((*next).clone()));
    assert!(next.previous().is_none());
    while let Some(item) = field.get_layout_item() {
        release_layout_item(&item);
    }
    assert!(field.get_layout_item().is_none());
}
#[test]
fn page_geometry_margin_offsets_and_filtered_traversal() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let page_node = node(&doc, E::PageArea);
    let medium = child(&doc, &page_node, E::Medium);
    measure(&medium, A::Short, 200.);
    measure(&medium, A::Long, 300.);
    set_enum(&medium, A::Orientation, V::Landscape);
    let page = store.create_view(&page_node, None);
    assert_eq!(page.get_page_size(), CfxSizeF::new(300., 200.));
    let area_node = child(&doc, &page_node, E::ContentArea);
    measure(&area_node, A::X, 10.);
    measure(&area_node, A::Y, 20.);
    let area = store.create_view(&area_node, None);
    page.append_last_child(&area);
    let parent_node = node(&doc, E::Subform);
    let margin = child(&doc, &parent_node, E::Margin);
    measure(&margin, A::LeftInset, 3.);
    measure(&margin, A::TopInset, 4.);
    let parent = store.create_content(&parent_node, None);
    parent.set_position(CfxPointF::new(5., 6.));
    area.append_last_child(&parent);
    let field = store.create_content(&node(&doc, E::Field), None);
    field.set_position(CfxPointF::new(7., 8.));
    field.set_size(CfxSizeF::new(30., 40.));
    parent.append_last_child(&field);
    assert_eq!(field.get_relative_rect(), CfxRectF::new(7., 8., 30., 40.));
    assert_eq!(field.get_absolute_rect(), CfxRectF::new(25., 38., 30., 40.));
    assert_eq!(field.get_page(), Some(page.clone()));
    let mut all = CxfaLayoutItemIterator::new(Some((*page).clone()));
    assert_eq!(all.move_to_next(), Some((*area).clone()));
    assert_eq!(all.move_to_next(), Some((*parent).clone()));
    assert_eq!(all.move_to_next(), Some((*field).clone()));
    assert!(all.move_to_next().is_none());
    let mut views = ViewLayoutItemIterator::new(Some(page));
    assert_eq!(views.move_to_next(), Some(area));
    assert!(views.move_to_next().is_none());
}
struct MeasurementHost;
impl ContentLayoutHost for MeasurementHost {
    fn find_split_pos(&self, _: &CxfaNode, _: usize, _: f32) -> Result<Option<f32>, LayoutError> {
        Ok(None)
    }
    fn create_content_widget(&self, _: &CxfaNode) -> Option<Rc<dyn LayoutItemWidget>> {
        None
    }
    fn start_field_draw_layout(
        &self,
        node: &CxfaNode,
        size: CfxSizeF,
    ) -> Result<CfxSizeF, LayoutError> {
        Ok(CfxSizeF::new(
            if size.width < 0. {
                node.get_measure_in_unit(A::W, XfaUnit::Pt)
            } else {
                size.width
            },
            if size.height < 0. {
                node.get_measure_in_unit(A::H, XfaUnit::Pt)
            } else {
                size.height
            },
        ))
    }
}
#[test]
fn positioned_rotation_flow_rows_and_field_host_boundary() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let container = node(&doc, E::Subform);
    set_enum(&container, A::Layout, V::Position);
    let field = child(&doc, &container, E::Field);
    measure(&field, A::W, 30.);
    measure(&field, A::H, 10.);
    measure(&field, A::X, 50.);
    measure(&field, A::Y, 40.);
    set_enum(&field, A::AnchorType, V::BottomRight);
    field.set_integer(A::Rotate, 90, false);
    let mut layout =
        CxfaContentLayoutProcessor::new(&container, store.clone(), Some(Rc::new(MeasurementHost)));
    assert_eq!(layout.layout_unpaginated(), Ok(LayoutResult::Done));
    let item = layout.extract_layout_item().unwrap();
    let field_item = item
        .first_child()
        .unwrap()
        .as_content_layout_item()
        .unwrap();
    assert_eq!(field_item.size(), CfxSizeF::new(10., 30.));
    assert_eq!(field_item.position(), CfxPointF::new(40., 40.));
    assert_eq!(item.size(), CfxSizeF::new(50., 70.));
    let flow = node(&doc, E::Subform);
    set_enum(&flow, A::Layout, V::Lr_tb);
    measure(&flow, A::W, 50.);
    for (width, height) in [(30., 10.), (30., 20.), (20., 5.)] {
        let field = child(&doc, &flow, E::Field);
        measure(&field, A::W, width);
        measure(&field, A::H, height);
    }
    let mut layout =
        CxfaContentLayoutProcessor::new(&flow, store.clone(), Some(Rc::new(MeasurementHost)));
    layout.layout_unpaginated().unwrap();
    let item = layout.extract_layout_item().unwrap();
    let items = item.children();
    assert_eq!(
        items[0].as_content_layout_item().unwrap().position(),
        CfxPointF::new(0., 0.)
    );
    assert_eq!(
        items[1].as_content_layout_item().unwrap().position(),
        CfxPointF::new(0., 10.)
    );
    assert_eq!(
        items[2].as_content_layout_item().unwrap().position(),
        CfxPointF::new(30., 10.)
    );
    assert_eq!(item.size(), CfxSizeF::new(50., 30.));
    let mut no_host = CxfaContentLayoutProcessor::new(&node(&doc, E::Field), store, None);
    assert_eq!(
        no_host.layout_unpaginated(),
        Err(LayoutError::MissingFieldMeasurementHost)
    );
    assert!(no_host.layout_item().is_none());
}
#[test]
fn table_columns_row_spans_and_right_to_left_cells() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let table = node(&doc, E::Subform);
    set_enum(&table, A::Layout, V::Table);
    table.set_cdata_and_notify(
        A::ColumnWidths,
        &"20pt 30pt".chars().map(u32::from).collect::<Vec<_>>(),
        false,
    );
    let row = child(&doc, &table, E::Subform);
    set_enum(&row, A::Layout, V::Rl_row);
    for height in [10., 20.] {
        let cell = child(&doc, &row, E::Field);
        measure(&cell, A::W, 5.);
        measure(&cell, A::H, height);
    }
    let mut layout = CxfaContentLayoutProcessor::new(&table, store, Some(Rc::new(MeasurementHost)));
    layout.layout_unpaginated().unwrap();
    assert_eq!(layout.specified_column_widths, [20., 30.]);
    let table = layout.extract_layout_item().unwrap();
    assert_eq!(table.size(), CfxSizeF::new(50., 20.));
    let row = table
        .first_child()
        .unwrap()
        .as_content_layout_item()
        .unwrap();
    let cells = row.children();
    assert_eq!(
        cells[0]
            .as_content_layout_item()
            .unwrap()
            .get_relative_rect(),
        CfxRectF::new(30., 0., 20., 20.)
    );
    assert_eq!(
        cells[1]
            .as_content_layout_item()
            .unwrap()
            .get_relative_rect(),
        CfxRectF::new(0., 0., 30., 20.)
    );
}
struct Notify {
    processor: std::rc::Weak<CxfaLayoutProcessor>,
    events: RefCell<Vec<(E, i32, S)>>,
    removed: RefCell<Vec<E>>,
}
impl LayoutItemNotify for Notify {
    fn on_layout_item_removing(&self, item: &CxfaLayoutItem) {
        self.removed
            .borrow_mut()
            .push(item.form_node().get_element_type());
        if let Some(processor) = self.processor.upgrade() {
            assert_eq!(processor.count_pages(), 1);
        }
    }
    fn on_page_view_removed(&self, _: &super::cxfa_viewlayoutitem::CxfaViewLayoutItem) {}
    fn on_layout_item_added(
        &self,
        item: &super::cxfa_contentlayoutitem::CxfaContentLayoutItem,
        page: i32,
        status: S,
    ) {
        self.events
            .borrow_mut()
            .push((item.form_node().get_element_type(), page, status));
        assert_eq!(self.processor.upgrade().unwrap().count_pages(), 1);
    }
}
#[test]
fn view_records_status_callbacks_and_postorder_removal_can_reenter() {
    let doc = CxfaDocument::new();
    let processor = CxfaLayoutProcessor::new(&doc);
    let view = processor.get_layout_page_mgr();
    let set = node(&doc, E::PageSet);
    assert!(view.initialize_page_set(&set));
    let page_node = child(&doc, &set, E::PageArea);
    let first = child(&doc, &page_node, E::ContentArea);
    let second = child(&doc, &page_node, E::ContentArea);
    measure(&first, A::H, 100.);
    measure(&second, A::H, 200.);
    let record = view.create_view_record(&page_node, false).unwrap();
    let page = view.add_page_area_layout_item(record, &page_node).unwrap();
    view.add_content_area_layout_item(record, Some(&first))
        .unwrap();
    view.reset_to_first_view_record();
    assert_eq!(view.get_avail_height(), 100.);
    assert_eq!(page.get_page_index(), 0);
    assert!(view.get_next_content_area(None));
    assert!(view.set_current_view_record(1));
    assert_eq!(view.get_avail_height(), 200.);
    assert!(!view.get_next_content_area(Some(&second)));
    let parent = processor
        .store()
        .create_content(&node(&doc, E::Subform), None);
    let field_node = node(&doc, E::Field);
    field_node.set_cdata_and_notify(
        A::Relevant,
        &"+print".chars().map(u32::from).collect::<Vec<_>>(),
        false,
    );
    let field = processor.store().create_content(&field_node, None);
    parent.append_last_child(&field);
    assert!(view.submit_content_to_current(Some(&parent)));
    let notify = Rc::new(Notify {
        processor: Rc::downgrade(&processor),
        events: RefCell::new(Vec::new()),
        removed: RefCell::new(Vec::new()),
    });
    processor.store().set_notify(Some(notify.clone()));
    view.sync_layout_item_status();
    assert_eq!(
        notify.events.borrow().as_slice(),
        [
            (E::Subform, 0, S::kVisible | S::kViewable | S::kPrintable),
            (E::Field, 0, S::kVisible | S::kPrintable)
        ]
    );
    release_layout_item(&page);
    assert_eq!(
        notify.removed.borrow().as_slice(),
        [
            E::ContentArea,
            E::Field,
            E::Subform,
            E::ContentArea,
            E::PageArea
        ]
    );
    assert!(field.parent().is_none());
    assert!(parent.parent().is_none());
}
#[test]
fn occurrence_limits_and_inherited_keep_neighbors() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let parent = node(&doc, E::Subform);
    let first = child(&doc, &parent, E::Subform);
    let second = child(&doc, &parent, E::Subform);
    let keep = child(&doc, &first, E::Keep);
    set_enum(&keep, A::Next, V::PageArea);
    assert!(exist_container_keep(Some(&first), false));
    assert!(exist_container_keep(Some(&second), true));
    let occur = child(&doc, &first, E::Occur);
    occur.set_integer(A::Max, 2, false);
    let mut layout = CxfaContentLayoutProcessor::new(&parent, store, None);
    assert!(layout.judge_leader_or_trailer_for_occur(Some(&first)));
    assert!(layout.judge_leader_or_trailer_for_occur(Some(&first)));
    assert!(!layout.judge_leader_or_trailer_for_occur(Some(&first)));
    assert!(!layout.judge_leader_or_trailer_for_occur(None));
}
