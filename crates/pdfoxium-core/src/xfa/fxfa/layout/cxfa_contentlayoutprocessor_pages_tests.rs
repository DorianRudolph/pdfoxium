// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Native source-derived pagination scenarios. Container-only fixtures use
//! actual specified sizes; the split-hook fixture tests callback transport,
//! not fabricated font/line metrics. Actual text metrics are tested by FFView.
use super::*;
use crate::xfa::fxfa::{fxfa_basic::XfaPacketType as P, parser::cxfa_document::CxfaDocument};
use std::cell::RefCell;
fn node(doc: &Rc<CxfaDocument>, element: E) -> CxfaNode {
    doc.create_node(P::Form, element).unwrap()
}
fn child(doc: &Rc<CxfaDocument>, parent: &CxfaNode, element: E) -> CxfaNode {
    let child = node(doc, element);
    assert!(parent.insert_child_and_notify(-1, &child));
    child
}
fn measure(node: &CxfaNode, attribute: A, value: f32) {
    node.set_measure(attribute, CxfaMeasurement::new(value, XfaUnit::Pt), false);
}
fn container(doc: &Rc<CxfaDocument>, parent: &CxfaNode, width: f32, height: f32) -> CxfaNode {
    let node = child(doc, parent, E::Subform);
    measure(&node, A::W, width);
    measure(&node, A::H, height);
    node.set_enum(A::Layout, V::Position, false);
    node
}
fn root(doc: &Rc<CxfaDocument>) -> CxfaNode {
    let node = node(doc, E::Subform);
    node.set_enum(A::Layout, V::Tb, false);
    node
}
fn view(
    doc: &Rc<CxfaDocument>,
    store: Rc<CxfaLayoutStore>,
    heights: &[f32],
) -> CxfaViewLayoutProcessor {
    let view = CxfaViewLayoutProcessor::new(store);
    let set = node(doc, E::PageSet);
    assert!(view.initialize_page_set(&set));
    let page = child(doc, &set, E::PageArea);
    let mut first = None;
    for &height in heights {
        let area = child(doc, &page, E::ContentArea);
        measure(&area, A::H, height);
        first.get_or_insert(area);
    }
    let record = view.create_view_record(&page, false).unwrap();
    view.add_page_area_layout_item(record, &page).unwrap();
    view.add_content_area_layout_item(record, first.as_ref())
        .unwrap();
    view.reset_to_first_view_record();
    view
}
#[test]
fn completed_intact_flowed_children_keep_height_when_retried_on_the_next_page() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let root = root(&doc);
    let mut blocks = Vec::new();
    for _ in 0..3 {
        let block = container(&doc, &root, 80., 70.);
        block.set_enum(A::Layout, V::Tb, false);
        let keep = child(&doc, &block, E::Keep);
        keep.set_enum(A::Intact, V::ContentArea, false);
        container(&doc, &block, 80., 70.);
        blocks.push(block);
    }
    let view = view(&doc, store.clone(), &[100.]);
    let mut processor = CxfaContentLayoutProcessor::new(&root, store, None);
    for (index, expected) in blocks.iter().enumerate() {
        assert_eq!(
            processor.do_layout(&view, true, 100., 100.).unwrap(),
            if index == 2 {
                LayoutResult::Done
            } else {
                LayoutResult::PageFullBreak
            }
        );
        let fragment = processor.extract_layout_item().unwrap();
        assert_eq!(fragment.size(), CfxSizeF::new(80., 70.));
        assert_eq!(fragment.children().len(), 1);
        let retained = fragment
            .first_child()
            .unwrap()
            .as_content_layout_item()
            .unwrap();
        assert_eq!(retained.form_node(), *expected);
        assert_eq!(retained.size(), CfxSizeF::new(80., 70.));
        assert_eq!(
            retained
                .first_child()
                .unwrap()
                .as_content_layout_item()
                .unwrap()
                .size(),
            CfxSizeF::new(80., 70.)
        );
    }
    assert!(processor.extract_layout_item().is_none());
}

#[test]
fn page_full_suspends_child_and_resume_retains_fragment_identity() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let root = root(&doc);
    let first = container(&doc, &root, 20., 30.);
    let second = container(&doc, &root, 25., 30.);
    let view = view(&doc, store.clone(), &[50.]);
    let mut processor = CxfaContentLayoutProcessor::new(&root, store, None);
    assert_eq!(
        processor.do_layout(&view, true, 50., 50.).unwrap(),
        LayoutResult::PageFullBreak
    );
    let first_page = processor.extract_layout_item().unwrap();
    assert_eq!(first_page.size(), CfxSizeF::new(20., 30.));
    assert_eq!(
        first_page
            .children()
            .iter()
            .map(|item| item.form_node())
            .collect::<Vec<_>>(),
        [first]
    );
    assert_eq!(processor.pages.current_child, Some(second.clone()));
    assert_eq!(
        processor.do_layout(&view, true, 50., 50.).unwrap(),
        LayoutResult::Done
    );
    let second_page = processor.extract_layout_item().unwrap();
    assert_eq!(second_page.size(), CfxSizeF::new(25., 30.));
    assert_eq!(second_page.first_child().unwrap().form_node(), second);
    assert_eq!(first_page.next(), Some(second_page.clone()));
    assert_eq!(second_page.get_index(), 1);
    assert!(processor.extract_layout_item().is_none());
}
#[test]
fn keep_group_moves_to_taller_content_area_without_losing_prior_items() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let root = root(&doc);
    let prefix = container(&doc, &root, 10., 20.);
    let leader = container(&doc, &root, 20., 30.);
    let follower = container(&doc, &root, 30., 30.);
    let keep = child(&doc, &leader, E::Keep);
    keep.set_enum(A::Next, V::ContentArea, false);
    let view = view(&doc, store.clone(), &[50., 100.]);
    let mut processor = CxfaContentLayoutProcessor::new(&root, store, None);
    assert_eq!(
        processor.do_layout(&view, true, 50., 50.).unwrap(),
        LayoutResult::PageFullBreak
    );
    let first = processor.extract_layout_item().unwrap();
    assert_eq!(first.size().height, 20.);
    assert_eq!(
        first
            .children()
            .iter()
            .map(|item| item.form_node())
            .collect::<Vec<_>>(),
        [prefix]
    );
    assert_eq!(
        processor.do_layout(&view, true, 100., 100.).unwrap(),
        LayoutResult::Done
    );
    let next = processor.extract_layout_item().unwrap();
    let children = next.children();
    assert_eq!(
        children
            .iter()
            .map(|item| item.form_node())
            .collect::<Vec<_>>(),
        [leader, follower]
    );
    assert_eq!(
        children[0].as_content_layout_item().unwrap().position().y,
        0.
    );
    assert_eq!(
        children[1].as_content_layout_item().unwrap().position().y,
        30.
    );
    assert_eq!(next.size().height, 60.);
}
#[test]
fn detached_split_roots_are_produced_in_order_and_not_nested_in_first_page() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let root = root(&doc);
    let first = container(&doc, &root, 20., 40.);
    let second = container(&doc, &root, 20., 40.);
    let mut processor = CxfaContentLayoutProcessor::new(&root, store.clone(), None);
    let item = processor.create_content_layout_item(&root);
    item.set_size(CfxSizeF::new(20., 80.));
    let first_item = store.create_content(&first, None);
    first_item.set_size(CfxSizeF::new(20., 40.));
    let second_item = store.create_content(&second, None);
    second_item.set_size(CfxSizeF::new(20., 40.));
    second_item.set_position(CfxPointF::new(0., 40.));
    item.append_last_child(&first_item);
    item.append_last_child(&second_item);
    processor.layout_item = Some(item.clone());
    processor.stage = LayoutStage::Done;
    assert_eq!(processor.find_split_pos(35.).unwrap(), 0.);
    processor.split_layout_item(40.).unwrap();
    let first_page = processor.extract_layout_item().unwrap();
    let second_page = processor.extract_layout_item().unwrap();
    assert_eq!(first_page.size().height, 40.);
    assert_eq!(second_page.size().height, 40.);
    assert_eq!(first_page.children(), [(*first_item).clone()]);
    assert_eq!(second_page.children(), [(*second_item).clone()]);
    assert_eq!(second_item.position().y, 0.);
    assert_eq!(first_page.next(), Some(second_page.clone()));
    assert_eq!(second_page.get_index(), 1);
    assert!(processor.extract_layout_item().is_none());
}
#[test]
fn native_stage_order_preserves_all_breaks_bookends_and_generated_cleanup() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let root = root(&doc);
    let bookend = child(&doc, &root, E::Bookend);
    let generated = container(&doc, &root, 1., 1.);
    generated.set_flag(XfaNodeFlag::LayoutGeneratedNode);
    generated.set_flag(XfaNodeFlag::UnusedNode);
    let first = container(&doc, &root, 10., 10.);
    let before = child(&doc, &first, E::BreakBefore);
    let legacy = child(&doc, &first, E::Break);
    legacy.set_enum(A::Before, V::PageArea, false);
    let after = child(&doc, &first, E::BreakAfter);
    let second = container(&doc, &root, 20., 20.);
    let mut processor = CxfaContentLayoutProcessor::new(&root, store, None);
    let mut actions = Vec::new();
    loop {
        processor.goto_next_container_node_simple();
        let Some(node) = processor.pages.current_child.clone() else {
            break;
        };
        actions.push((processor.stage, node));
        assert!(actions.len() < 20);
    }
    assert_eq!(
        actions,
        [
            (LayoutStage::BookendLeader, bookend.clone()),
            (LayoutStage::BreakBefore, before),
            (LayoutStage::BreakBefore, legacy.clone()),
            (LayoutStage::Container, first),
            (LayoutStage::BreakAfter, after),
            (LayoutStage::Container, second),
            (LayoutStage::BookendTrailer, bookend)
        ]
    );
    assert!(generated.parent().is_none());
}
struct SplitHost {
    calls: RefCell<Vec<(usize, f32)>>,
    invalid: bool,
}
impl ContentLayoutHost for SplitHost {
    fn create_content_widget(&self, _: &CxfaNode) -> Option<Rc<dyn LayoutItemWidget>> {
        None
    }
    fn start_field_draw_layout(
        &self,
        node: &CxfaNode,
        _: CfxSizeF,
    ) -> Result<CfxSizeF, LayoutError> {
        Ok(CfxSizeF::new(
            node.get_measure_in_unit(A::W, XfaUnit::Pt),
            node.get_measure_in_unit(A::H, XfaUnit::Pt),
        ))
    }
    fn find_split_pos(
        &self,
        _: &CxfaNode,
        index: usize,
        proposed: f32,
    ) -> Result<Option<f32>, LayoutError> {
        self.calls.borrow_mut().push((index, proposed));
        if self.invalid {
            Ok(Some(proposed + 1.))
        } else {
            Ok((proposed > 24.).then_some(24.))
        }
    }
}
#[test]
fn split_hook_receives_real_fragment_index_and_invalid_nonprogress_is_rejected() {
    let doc = CxfaDocument::new();
    doc.recognize_xfa_version_number(
        &"http://www.xfa.org/schema/xfa-template/2.8/"
            .chars()
            .map(u32::from)
            .collect::<Vec<_>>(),
    );
    let store = CxfaLayoutStore::new();
    let root = root(&doc);
    let field = child(&doc, &root, E::Field);
    measure(&field, A::W, 20.);
    measure(&field, A::H, 60.);
    let keep = child(&doc, &field, E::Keep);
    keep.set_enum(A::Intact, V::None, false);
    let host = Rc::new(SplitHost {
        calls: RefCell::new(Vec::new()),
        invalid: false,
    });
    let mut processor = CxfaContentLayoutProcessor::new(&field, store.clone(), Some(host.clone()));
    processor.layout_unpaginated().unwrap();
    assert_eq!(processor.find_split_pos(35.).unwrap(), 24.);
    processor.split_layout_item(24.).unwrap();
    let first = processor.extract_layout_item().unwrap();
    assert_eq!(first.size().height, 24.);
    assert_eq!(processor.find_split_pos(20.).unwrap(), 20.);
    assert!(host.calls.borrow().contains(&(1, 20.)));
    let host = Rc::new(SplitHost {
        calls: RefCell::new(Vec::new()),
        invalid: true,
    });
    let mut invalid = CxfaContentLayoutProcessor::new(&field, store, Some(host));
    invalid.layout_unpaginated().unwrap();
    assert!(matches!(
        invalid.find_split_pos(10.),
        Err(LayoutError::InvalidSplitPosition)
    ));
}

use crate::xfa::fxfa::layout::cxfa_layoutitem::CxfaLayoutItem;
struct GeneratedRemovalNotify {
    root: CxfaContentLayoutItem,
    generated: CxfaNode,
    expected: Vec<CxfaLayoutItem>,
    seen: Rc<RefCell<Vec<CxfaLayoutItem>>>,
}
impl crate::xfa::fxfa::layout::cxfa_layoutitem::LayoutItemNotify for GeneratedRemovalNotify {
    fn on_layout_item_removing(&self, item: &CxfaLayoutItem) {
        // Keep the old tree usable for page lookup and widget invalidation.
        assert_eq!(self.root.children().len(), 3);
        assert!(self.generated.parent().is_some());
        assert!(self.generated.get_layout_item().is_some());
        let index = self.seen.borrow().len();
        assert_eq!(item, &self.expected[index]);
        self.seen.borrow_mut().push(item.clone());
    }
    fn on_page_view_removed(&self, _: &crate::xfa::fxfa::layout::cxfa_viewlayoutitem::CxfaViewLayoutItem) {
        panic!("Retiring generated body items must not remove the page view");
    }
    fn on_layout_item_added(&self, _: &CxfaContentLayoutItem, _: i32, _: crate::xfa::fxfa::fxfa::XfaWidgetStatus) {
        panic!("Cleanup must not create layout items");
    }
}
fn check_generated_fragment_retirement(with_notify: bool) {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let root = root(&doc);
    let generated = container(&doc, &root, 60., 8.);
    generated.set_flag(XfaNodeFlag::LayoutGeneratedNode);
    generated.set_flag(XfaNodeFlag::UnusedNode);
    let leaf = child(&doc, &generated, E::Draw);
    let live = container(&doc, &root, 60., 20.);
    let root_layout = store.create_content(&root, None);
    root.set_layout_item(Some((*root_layout).clone()));
    let first = store.create_content(&generated, None);
    let second = store.create_content(&generated, None);
    first.insert_after(&second);
    generated.set_layout_item(Some((*first).clone()));
    let first_leaf = store.create_content(&leaf, None);
    let second_leaf = store.create_content(&leaf, None);
    first_leaf.insert_after(&second_leaf);
    leaf.set_layout_item(Some((*first_leaf).clone()));
    first.append_last_child(&first_leaf);
    second.append_last_child(&second_leaf);
    let live_layout = store.create_content(&live, None);
    live.set_layout_item(Some((*live_layout).clone()));
    root_layout.append_last_child(&first);
    root_layout.append_last_child(&second);
    root_layout.append_last_child(&live_layout);
    let expected = vec![(*first).clone(), (*second).clone(), (*first_leaf).clone(), (*second_leaf).clone()];
    let seen = Rc::new(RefCell::new(Vec::new()));
    if with_notify {
        store.set_notify(Some(Rc::new(GeneratedRemovalNotify {
            root: root_layout.clone(),
            generated: generated.clone(),
            expected: expected.clone(),
            seen: seen.clone(),
        })));
    }
    let mut processor = CxfaContentLayoutProcessor::new(&root, store.clone(), None);
    processor.goto_next_container_node_simple();
    assert_eq!(processor.pages.current_child, Some(live.clone()));
    assert_eq!(processor.stage, LayoutStage::Container);
    assert!(generated.parent().is_none());
    assert_eq!(live.parent(), Some(root));
    assert_eq!(root_layout.children(), [(*live_layout).clone()]);
    assert!(generated.get_layout_item().is_none());
    assert!(leaf.get_layout_item().is_none());
    for item in [&first, &second, &first_leaf, &second_leaf] {
        // The arena retains allocations: retirement must sever paint edges.
        assert!(item.is_alive());
        assert!(item.parent().is_none());
        assert!(item.previous().is_none());
        assert!(item.next().is_none());
    }
    assert_eq!(*seen.borrow(), if with_notify { expected } else { Vec::new() });
}
#[test]
fn unused_generated_cleanup_retires_all_fragment_edges_without_notify() {
    check_generated_fragment_retirement(false);
}
#[test]
fn generated_cleanup_notifications_see_the_complete_old_tree_before_retirement() {
    check_generated_fragment_retirement(true);
}

#[test]
fn splitting_into_a_reused_fragment_does_not_replay_its_previous_children() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let root = root(&doc);
    let first_node = container(&doc, &root, 60., 40.);
    let second_node = container(&doc, &root, 60., 40.);
    let cached_node = container(&doc, &root, 60., 8.);
    let old_first = store.create_content(&root, None);
    let old_second = store.create_content(&root, None);
    old_first.insert_after(&old_second);
    root.set_layout_item(Some((*old_first).clone()));
    let cached = store.create_content(&cached_node, None);
    cached_node.set_layout_item(Some((*cached).clone()));
    old_second.append_last_child(&cached);
    let mut processor = CxfaContentLayoutProcessor::new(&root, store.clone(), None);
    let item = processor.create_content_layout_item(&root);
    assert_eq!(item, old_first);
    let first = store.create_content(&first_node, None);
    first.set_size(CfxSizeF::new(60., 40.));
    let second = store.create_content(&second_node, None);
    second.set_size(CfxSizeF::new(60., 40.));
    second.set_position(CfxPointF::new(0., 40.));
    item.append_last_child(&first);
    item.append_last_child(&second);
    item.set_size(CfxSizeF::new(60., 80.));
    processor.layout_item = Some(item);
    processor.stage = LayoutStage::Done;
    processor.split_layout_item(40.).unwrap();
    let first_page = processor.extract_layout_item().unwrap();
    let second_page = processor.extract_layout_item().unwrap();
    assert_eq!(first_page, old_first);
    assert_eq!(second_page, old_second);
    assert_eq!(first_page.children(), [(*first).clone()]);
    assert_eq!(second_page.children(), [(*second).clone()]);
    assert_eq!(second_page.size(), CfxSizeF::new(60., 40.));
    assert_eq!(second.position(), CfxPointF::default());
    assert!(processor.extract_layout_item().is_none());
    // Reuse is not permanent retirement: child processors still own the
    // node-to-layout association and can recycle the detached cached item.
    assert!(cached.parent().is_none());
    assert_eq!(cached_node.get_layout_item(), Some((*cached).clone()));
    let mut child = CxfaContentLayoutProcessor::new(&cached_node, store, None);
    assert_eq!(child.create_content_layout_item(&cached_node), cached);
}
