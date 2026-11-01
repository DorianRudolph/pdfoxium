// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source page-selection and actual-runtime break/merge contracts.
use super::*;
use crate::xfa::fxfa::{cxfa_ffdoc::CxfaFfDoc, test_support::new_document};
use std::rc::Weak;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn node(document: &Rc<CxfaDocument>, element: E) -> CxfaNode {
    document
        .create_node(XfaPacketType::Template, element)
        .unwrap()
}
fn attach(parent: &CxfaNode, child: &CxfaNode) {
    assert!(parent.insert_child_and_notify(-1, child));
}
fn setup(mode: V) -> (Rc<CxfaDocument>, CxfaNode, CxfaViewLayoutProcessor) {
    let document = CxfaDocument::new();
    let pages = node(&document, E::PageSet);
    pages.set_enum(A::Relation, mode, false);
    let processor = CxfaViewLayoutProcessor::new(CxfaLayoutStore::new());
    assert!(processor.initialize_page_set(&pages));
    (document, pages, processor)
}
fn page(document: &Rc<CxfaDocument>, parent: &CxfaNode, name: &str, height: f32) -> CxfaNode {
    let page = node(document, E::PageArea);
    page.set_cdata(A::Name, &wide(name));
    attach(parent, &page);
    let content = node(document, E::ContentArea);
    content.set_measure(A::H, CxfaMeasurement::new(height, XfaUnit::Pt), false);
    attach(&page, &content);
    page
}
fn occur(document: &Rc<CxfaDocument>, parent: &CxfaNode, min: i32, max: i32) {
    let occur = node(document, E::Occur);
    occur.set_integer(A::Min, min, false);
    occur.set_integer(A::Max, max, false);
    attach(parent, &occur);
}
fn current_tail(processor: &CxfaViewLayoutProcessor) {
    assert!(processor.set_current_view_record(processor.record_count() - 1));
}
#[test]
fn init_creates_native_default_letter_master_with_quarter_inch_content_insets() {
    let document = CxfaDocument::new();
    let template = node(&document, E::Subform);
    let form = template.clone_template_to_form(false).unwrap();
    let processor = CxfaViewLayoutProcessor::new(CxfaLayoutStore::new());
    assert!(processor.init_layout_page(&form));
    assert!(processor.prepare_first_page(&form).unwrap());
    let record = processor.current_view_record().unwrap();
    let page = record.page_area.unwrap().form_node();
    let content = record.content_area.unwrap().form_node();
    let medium = page.get_first_child_by_class(E::Medium).unwrap();
    assert_eq!(content.get_measure_in_unit(A::X, XfaUnit::Pt), 18.0);
    assert_eq!(content.get_measure_in_unit(A::Y, XfaUnit::Pt), 18.0);
    assert_eq!(content.get_measure_in_unit(A::W, XfaUnit::Pt), 576.0);
    assert_eq!(processor.get_avail_height(), 756.0);
    assert_eq!(medium.get_measure_in_unit(A::Short, XfaUnit::Pt), 612.0);
    assert_eq!(medium.get_measure_in_unit(A::Long, XfaUnit::Pt), 792.0);
}
#[test]
fn ordered_maximums_repeat_root_page_sets_without_losing_orphan_roots() {
    let (document, pages, processor) = setup(V::OrderedOccurrence);
    occur(&document, &pages, 1, 2);
    let first = page(&document, &pages, "first", 10.0);
    occur(&document, &first, 1, 1);
    let rest = page(&document, &pages, "rest", 20.0);
    occur(&document, &rest, 1, 2);
    processor.init_page_set_map();
    let mut expected = vec![
        first.clone(),
        rest.clone(),
        rest.clone(),
        first.clone(),
        rest.clone(),
        rest.clone(),
    ];
    for expected in expected.drain(..) {
        let actual = processor
            .get_next_avail_page_area(None, None, true, false)
            .unwrap();
        assert_eq!(actual, expected);
        current_tail(&processor);
    }
    assert!(processor
        .get_next_avail_page_area(None, None, true, false)
        .is_none());
    assert_eq!(processor.get_page_count(), 6);
    assert_eq!(processor.root_layout_items().len(), 2);
    assert_eq!(
        processor
            .root_layout_items()
            .iter()
            .map(|root| root.children().len())
            .sum::<usize>(),
        6
    );
}
#[test]
fn simplex_final_only_and_last_pages_and_duplex_parity_follow_source_choices() {
    for count in [1, 2] {
        let (document, pages, processor) = setup(V::SimplexPaginated);
        let first = page(&document, &pages, "first", 100.0);
        first.set_enum(A::PagePosition, V::First, false);
        let rest = page(&document, &pages, "rest", 100.0);
        rest.set_enum(A::PagePosition, V::Rest, false);
        let only = page(&document, &pages, "only", 100.0);
        only.set_enum(A::PagePosition, V::Only, false);
        let last = page(&document, &pages, "last", 100.0);
        last.set_enum(A::PagePosition, V::Last, false);
        for _ in 0..count {
            assert!(processor
                .get_next_avail_page_area(None, None, true, false)
                .is_some());
            current_tail(&processor);
        }
        processor.finish_paginated_page_sets();
        let selected = processor.get_page(count - 1).unwrap().form_node();
        assert_eq!(selected, if count == 1 { only } else { last });
        assert_eq!(processor.get_page_count(), count);
    }
    let (document, pages, processor) = setup(V::DuplexPaginated);
    let odd_first = page(&document, &pages, "oddFirst", 100.0);
    odd_first.set_enum(A::PagePosition, V::First, false);
    odd_first.set_enum(A::OddOrEven, V::Odd, false);
    let even = page(&document, &pages, "evenRest", 100.0);
    even.set_enum(A::PagePosition, V::Rest, false);
    even.set_enum(A::OddOrEven, V::Even, false);
    let odd = page(&document, &pages, "oddRest", 100.0);
    odd.set_enum(A::PagePosition, V::Rest, false);
    odd.set_enum(A::OddOrEven, V::Odd, false);
    for expected in [odd_first, even, odd] {
        assert_eq!(
            processor.get_next_avail_page_area(None, None, true, false),
            Some(expected)
        );
        current_tail(&processor);
    }
}
#[test]
fn content_height_query_switches_areas_and_preserves_record_state() {
    let (document, pages, processor) = setup(V::OrderedOccurrence);
    let first = page(&document, &pages, "first", 10.0);
    occur(&document, &first, 1, 1);
    let second_content = node(&document, E::ContentArea);
    second_content.set_measure(A::H, CxfaMeasurement::new(30.0, XfaUnit::Pt), false);
    attach(&first, &second_content);
    let second = page(&document, &pages, "second", 50.0);
    occur(&document, &second, 1, 1);
    processor.init_page_set_map();
    assert_eq!(
        processor.get_next_avail_page_area(None, None, false, false),
        Some(first.clone())
    );
    current_tail(&processor);
    assert!(processor.get_next_avail_content_height(20.0));
    assert_eq!(processor.record_count(), 1);
    assert_eq!(
        processor.get_next_avail_page_area(None, None, false, false),
        Some(first)
    );
    current_tail(&processor);
    assert_eq!(processor.get_avail_height(), 30.0);
    let record = processor.current_view_record();
    assert!(processor.get_next_avail_content_height(40.0));
    assert!(!processor.get_next_avail_content_height(50.0));
    assert_eq!(processor.record_count(), 2);
    assert_eq!(processor.get_page_count(), 1);
    assert_eq!(
        processor.current_view_record().unwrap().content_area,
        record.unwrap().content_area
    );
    assert_eq!(
        processor.get_next_avail_page_area(None, None, true, false),
        Some(second)
    );
}
struct ActualRuntime(Weak<CxfaFfDoc>);
impl ViewLayoutHost for ActualRuntime {
    fn create_page_view(&self, node: &CxfaNode) -> Option<Rc<dyn LayoutItemPageView>> {
        Some(
            self.0
                .upgrade()?
                .notify()
                .on_create_view_layout_item(node)?,
        )
    }
    fn runtime(&self) -> ViewLayoutResult<Rc<CfxJseEngine>> {
        self.0
            .upgrade()
            .and_then(|doc| doc.try_script_context())
            .ok_or(ViewLayoutError::ExpiredDocument)
    }
    fn run_break_script(&self, script: &CxfaNode, container: &CxfaNode) -> ViewLayoutResult<bool> {
        Ok(self
            .0
            .upgrade()
            .ok_or(ViewLayoutError::ExpiredDocument)?
            .notify()
            .run_script(script, container))
    }
    fn layout_page_area(&self, _: &CxfaViewLayoutItem) -> ViewLayoutResult<()> {
        Err(ViewLayoutError::MissingHost)
    }
}
#[test]
fn break_target_alternatives_make_progress_and_real_boa_scripts_gate_generated_bindings() {
    let input = r##"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><pageSet><pageArea name="one" id="one"><contentArea name="area" h="100pt"/></pageArea><pageArea name="two" id="two"><contentArea name="area" h="200pt"/></pageArea></pageSet><proto><subform name="leader" id="lead"><field name="amount"><ui><textEdit/></ui><bind match="dataRef" ref="$record.amount"/></field></subform><subform name="trailer" id="trail"/></proto><subform name="body"><breakBefore targetType="pageArea" target="#two" leader="#lead" trailer="#trail"><script contentType="application/x-javascript">globalThis.breakCalls=(globalThis.breakCalls||0)+1;false</script></breakBefore></subform></subform></template><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><record><amount>bound</amount></record></xfa:data></xfa:datasets></xdp:xdp>"##;
    let (doc, _host) = new_document(input);
    doc.create_doc_view();
    let document = doc.model_document();
    document
        .do_data_merge_with_resolver(doc.script_context().resolver())
        .unwrap();
    let template = document
        .get_xfa_node(XfaHashCode::Template as u32)
        .unwrap()
        .get_first_child_by_class(E::Subform)
        .unwrap();
    let page_set = template.get_first_child_by_class(E::PageSet).unwrap();
    let processor = CxfaViewLayoutProcessor::new(doc.layout_processor().store().clone());
    processor.set_host(Some(Rc::new(ActualRuntime(Rc::downgrade(&doc)))));
    assert!(processor.initialize_page_set(&page_set));
    let second = page_set.get_first_child_by_name(&wide("two")).unwrap();
    assert_eq!(
        processor
            .resolve_break_target(Some(&page_set), true, &wide(" #missing   #two "))
            .unwrap(),
        Some(second.clone())
    );
    assert_eq!(
        processor
            .resolve_break_target(Some(&page_set), true, &wide("som(two)"))
            .unwrap(),
        Some(second.clone())
    );
    assert!(processor
        .resolve_break_target(Some(&page_set), false, &wide("#missing #alsoMissing"))
        .unwrap()
        .is_none());
    let form = document
        .get_xfa_node(XfaHashCode::Form as u32)
        .unwrap()
        .get_first_child_by_class(E::Subform)
        .unwrap();
    let body = form.get_first_child_by_name(&wide("body")).unwrap();
    let break_node = body.get_first_child_by_class(E::BreakBefore).unwrap();
    let result = processor
        .process_break_before(&break_node)
        .unwrap()
        .unwrap();
    assert!(!result.create_page);
    assert!(result.leader.is_none() && result.trailer.is_none());
    assert_eq!(
        doc.script_context().eval("breakCalls").unwrap().as_number(),
        Some(1.0)
    );
    let script = break_node.get_first_child_by_class(E::Script).unwrap();
    script.set_content(
        &wide("globalThis.breakCalls++;true"),
        &wide("globalThis.breakCalls++;true"),
        false,
    );
    let result = processor
        .process_break_before(&break_node)
        .unwrap()
        .unwrap();
    assert!(result.create_page);
    let leader = result.leader.unwrap();
    let trailer = result.trailer.unwrap();
    assert!(leader.is_layout_generated_node() && trailer.is_layout_generated_node());
    assert!(!leader.is_unused_node());
    let amount = leader.get_first_child_by_class(E::Field).unwrap();
    assert_eq!(amount.get_raw_value(), wide("bound"));
    assert!(amount.get_bind_data().is_some());
    assert_eq!(
        doc.script_context().eval("breakCalls").unwrap().as_number(),
        Some(2.0)
    );
}

#[test]
fn duplex_relayout_uses_current_ordinal_when_reusing_odd_and_even_page_caches() {
    for count in [1, 2, 3, 4] {
        let (document, pages, processor) = setup(V::DuplexPaginated);
        let first = page(&document, &pages, "first", 100.);
        first.set_enum(A::PagePosition, V::First, false);
        first.set_enum(A::OddOrEven, V::Odd, false);
        let even = page(&document, &pages, "even", 100.);
        even.set_enum(A::PagePosition, V::Rest, false);
        even.set_enum(A::OddOrEven, V::Even, false);
        let odd = page(&document, &pages, "odd", 100.);
        odd.set_enum(A::PagePosition, V::Rest, false);
        odd.set_enum(A::OddOrEven, V::Odd, false);
        for pass in 0..3 {
            if pass > 0 {
                assert!(processor.initialize_page_set(&pages));
                assert_eq!(processor.get_page_count(), count);
            }
            for index in 0..count {
                let expected = if index == 0 {
                    &first
                } else if index % 2 == 1 {
                    &even
                } else {
                    &odd
                };
                assert_eq!(
                    processor
                        .get_next_avail_page_area(None, None, true, false)
                        .as_ref(),
                    Some(expected),
                    "count={count} pass={pass} page={index}"
                );
                current_tail(&processor);
                assert_eq!(processor.get_page(index).unwrap().form_node(), *expected);
            }
            assert_eq!(processor.get_page_count(), count);
        }
    }
}
