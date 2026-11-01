// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Connected real FFDoc/Boa/model/layout contracts; no PDF embedder coverage
//! coverage is implied by these cases.
use super::*;
use crate::fxcrt::fx_coordinates::{CfxPointF, CfxSizeF, FxRect};
use crate::xfa::fxfa::{
    cxfa_ffpageview::CxfaFfPageView,
    cxfa_fftextedit::CxfaFfTextEdit,
    cxfa_ffwidget::FfWidgetIterator,
    fxfa::XfaWidgetStatus as Status,
    fxfa_basic::{XfaPacketType, XfaUnit},
    test_support::{new_document, HostCall},
};
fn fixture() -> (
    Rc<CxfaFfDoc>,
    Rc<crate::xfa::fxfa::test_support::RecordingHost>,
    Rc<CxfaFfDocView>,
    CxfaNode,
) {
    let (doc,host)=new_document(br##"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <template xmlns="http://www.xfa.org/schema/xfa-template/2.8/"><subform name="record">
        <event activity="initialize"><script contentType="application/x-javascript">globalThis.trace=(globalThis.trace||'')+'root;';</script></event>
        <field name="source"><ui><textEdit/></ui><value><text>3</text></value>
          <caption><value><text>Source</text></value></caption>
          <event activity="initialize"><script contentType="application/x-javascript">globalThis.trace=(globalThis.trace||'')+'source;';</script></event>
          <event activity="change"><script contentType="application/x-javascript">xfa.event.change='rewritten';</script></event>
        </field>
        <field name="calc"><ui><textEdit/></ui><value><text>0</text></value>
          <calculate><script contentType="application/x-javascript">Number(xfa.form.record.source.rawValue)*2;</script></calculate>
          <event activity="initialize"><script contentType="application/x-javascript">globalThis.trace=(globalThis.trace||'')+'calc;';</script></event>
        </field>
        <field name="required"><ui><textEdit/></ui><value><text/></value>
          <validate nullTest="error"><message><text name="nullTest">Required value</text></message></validate>
        </field>
      </subform></template></xdp:xdp>"##);
    let view = doc.create_doc_view();
    doc.model_document()
        .do_data_merge_with_resolver(doc.script_context().resolver())
        .unwrap();
    let root = view.root_subform().unwrap();
    (doc, host, view, root)
}
fn named(root: &CxfaNode, name: &str) -> CxfaNode {
    root.get_first_child_by_name(&wide(name)).unwrap()
}
#[test]
fn actual_view_events_calculation_dependencies_validation_callbacks_and_reset() {
    let (doc, host, view, root) = fixture();
    let source = named(&root, "source");
    let calculated = named(&root, "calc");
    let required = named(&root, "required");
    assert!(source.is_widget_ready() && calculated.is_widget_ready());
    view.init_layout(&root);
    assert_eq!(
        doc.script_context()
            .eval("trace")
            .unwrap()
            .as_string()
            .unwrap()
            .to_std_string_escaped(),
        "source;calc;root;"
    );
    assert_eq!(
        view.exec_event_activity_by_deep_first(
            Some(&root),
            XfaEventType::InitCalculate,
            false,
            true
        ),
        EventError::kSuccess
    );
    assert_eq!(calculated.get_raw_value(), wide("6"));
    let mut event = CxfaEventParam::new(XfaEventType::Change);
    event.change = wide("original");
    assert_eq!(
        source.process_event(&view, V::Change, &mut event),
        EventError::kSuccess
    );
    assert_eq!(event.change, wide("rewritten"));
    assert!(doc.script_context().current_event().is_none());
    source.sync_value(&wide("5"), true);
    view.run_calculate_widgets();
    assert_eq!(calculated.get_raw_value(), wide("10"));
    host.calculations.set(false);
    view.add_calculate_node(&calculated);
    assert_eq!(view.run_calculate_widgets(), EventError::kDisabled);
    host.calculations.set(true);
    assert_eq!(view.run_calculate_widgets(), EventError::kSuccess);
    doc.script_context().eval("xfa.form.record.required.rawValue='temporary'; xfa.form.record.required.rawValue=null;").unwrap();
    let weak_doc = Rc::downgrade(&doc);
    let source_copy = source.clone();
    *host.on_callback.borrow_mut() = Some(Rc::new(move |call| {
        if matches!(call, HostCall::Message(..)) {
            let doc = weak_doc.upgrade().unwrap();
            assert_eq!(
                doc.script_context()
                    .eval("xfa.form.record.source.rawValue")
                    .unwrap()
                    .as_string()
                    .unwrap()
                    .to_std_string_escaped(),
                "5"
            );
            source_copy.sync_value(&wide("7"), true);
        }
    }));
    required
        .get_child(0, E::Validate, false)
        .unwrap()
        .clear_flag(XfaNodeFlag::NeedsInitApp);
    assert_eq!(required.process_validate(&view, 0), EventError::kError);
    assert!(host.calls.borrow().iter().any(
        |call| matches!(call,HostCall::Message(message,..) if *message==wide("Required value"))
    ));
    host.on_callback.borrow_mut().take();
    view.run_calculate_widgets();
    assert_eq!(calculated.get_raw_value(), wide("14"));
    source.reset_data();
    assert_eq!(source.get_raw_value(), wide("3"));
    let caption = view.load_caption(&source).unwrap().unwrap();
    assert!(Rc::ptr_eq(
        &caption,
        &view.load_caption(&source).unwrap().unwrap()
    ));
    caption
        .with_layout(|layout| layout.layout(CfxSizeF::new(100.0, 20.0)))
        .unwrap();
    view.update_doc_view().unwrap();
    assert_eq!(view.lock.get(), 0);
    let retained_page = doc.layout_processor().get_page(0).unwrap();
    assert!(retained_page.get_layout().is_some());
    doc.pre_finalize();
    assert!(retained_page.get_layout().is_none());
    assert_eq!(retained_page.get_page_index(), -1);
    assert!(view.get_layout_processor().is_none());
    assert_eq!(view.do_layout(), Err(FfDocViewError::DetachedDocument));
    assert_eq!(view.run_layout(), Err(FfDocViewError::DetachedDocument));
    assert_eq!(calculated.process_calculate(&view), EventError::kNotExist);
    let weak = Rc::downgrade(&doc);
    drop(doc);
    assert!(weak.upgrade().is_none());
    assert!(view.doc().is_none());
}
#[test]
fn actual_page_geometry_spatial_and_explicit_widget_order_and_owner_lifetime() {
    let (doc, _host, view, root) = fixture();
    let model = doc.model_document();
    let store = doc.layout_processor().store().clone();
    let area = model.create_node(XfaPacketType::Form, E::PageArea).unwrap();
    let medium = model.create_node(XfaPacketType::Form, E::Medium).unwrap();
    medium.set_measure(
        A::Short,
        super::super::parser::cxfa_measurement::CxfaMeasurement::new(100.0, XfaUnit::Pt),
        false,
    );
    medium.set_measure(
        A::Long,
        super::super::parser::cxfa_measurement::CxfaMeasurement::new(200.0, XfaUnit::Pt),
        false,
    );
    area.insert_child_and_notify(-1, &medium);
    let page = CxfaFfPageView::new(Rc::downgrade(&view), &area);
    let page_item = store.create_view(&area, Some(page.clone()));
    let content_area = model
        .create_node(XfaPacketType::Form, E::ContentArea)
        .unwrap();
    let content = store.create_view(&content_area, None);
    page_item.append_last_child(&content);
    let host: Rc<dyn FfDocViewHost> = view.clone();
    let mut widgets = Vec::new();
    for (index, name) in ["source", "calc", "required"].into_iter().enumerate() {
        let node = named(&root, name);
        let widget = CxfaFfTextEdit::new(&node);
        widget.widget().set_doc_view(Some(&host));
        let item = store.create_content(&node, Some(widget.clone()));
        item.set_position(CfxPointF::new(10.0 + 20.0 * index as f32, 10.0));
        item.set_size(CfxSizeF::new(18.0, 15.0));
        item.set_status_bits(Status::kVisible | Status::kViewable);
        content.append_last_child(&item);
        node.set_layout_item(Some((*item).clone()));
        widget.widget().set_page_view(Some(page_item.clone()));
        widgets.push((node, widget, item));
    }
    let traversal = widgets[0]
        .0
        .get_or_create_property(0, E::Traversal)
        .unwrap();
    // Traverse is an ordinary child; Traversal's schema properties contain
    // only Extras, so GetOrCreateProperty must reject this element.
    let traverse = model.create_node(XfaPacketType::Form, E::Traverse).unwrap();
    traversal.insert_child_and_notify(-1, &traverse);
    traverse.set_cdata(A::Ref, &wide("required"));
    assert_eq!(
        page.get_page_view_rect(),
        CfxRectF::new(0.0, 0.0, 100.0, 200.0)
    );
    let display = FxRect::new(20, 30, 220, 430);
    let matrix = page.get_display_matrix_for_rect(display, 1).unwrap();
    for (point, expected) in [
        (CfxPointF::new(0.0, 0.0), CfxPointF::new(220.0, 30.0)),
        (CfxPointF::new(100.0, 200.0), CfxPointF::new(20.0, 430.0)),
    ] {
        let actual = matrix.transform(point);
        assert!((actual.x - expected.x).abs() < 0.001 && (actual.y - expected.y).abs() < 0.001);
    }
    assert!(page.get_display_matrix_for_rect(display, -1).is_none());
    let mut ordered = page.create_traverse_widget_iterator(Status::kVisible | Status::kFocused);
    let name = |owner: Rc<dyn LayoutItemWidget>| {
        owner
            .as_ff_widget()
            .unwrap()
            .widget()
            .node()
            .get_cdata(A::Name)
    };
    assert_eq!(name(ordered.move_to_first().unwrap()), wide("source"));
    assert_eq!(name(ordered.move_to_next().unwrap()), wide("required"));
    assert_eq!(name(ordered.move_to_next().unwrap()), wide("calc"));
    assert!(ordered.move_to_next().is_none());
    widgets[1].2.set_status_bits(Status::kDisabled);
    assert_eq!(name(ordered.move_to_last().unwrap()), wide("required"));
    let mut natural = page.create_widget_iterator(Status::kVisible | Status::kViewable);
    assert_eq!(name(natural.move_to_first().unwrap()), wide("source"));
    assert_eq!(name(natural.move_to_next().unwrap()), wide("calc"));
    assert!(natural.set_current_widget(widgets[2].1.as_ref()));
    assert_eq!(name(natural.current_widget().unwrap()), wide("required"));
    let weak = Rc::downgrade(&doc);
    doc.pre_finalize();
    drop(doc);
    assert!(weak.upgrade().is_none());
    assert_eq!(name(ordered.move_to_first().unwrap()), wide("source"));
}

#[test]
fn remerge_updates_queued_choice_bindings_before_view_callbacks_without_replaying_them() {
    let (doc, _host) = new_document(
        br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root">
        <field name="choice" w="100pt" h="24pt"><bind match="once"/>
          <ui><choiceList open="userControl"/></ui><value><text>g</text></value>
          <bindItems ref="xfa.datasets.data.root.options.row[*]" valueRef="code" labelRef="label"/>
        </field>
      </subform></template>
      <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><root><options>
        <row><code>r</code><label>Red</label></row><row><code>g</code><label>Green</label></row>
        <row><code>b</code><label>Blue</label></row>
      </options></root></xfa:data></xfa:datasets></xdp:xdp>"#,
    );
    let view = doc.create_doc_view();
    assert!(view.start_layout().unwrap() >= 0);
    assert_eq!(view.do_layout().unwrap(), 100);
    view.stop_layout().unwrap();
    let initial = named(&view.root_subform().unwrap(), "choice");
    assert_eq!(initial.count_choice_list_items(false), 3);
    assert_eq!(initial.get_choice_list_item(1, true), Some(wide("g")));
    doc.script_context().eval(
        "xfa.datasets.data.root.options.loadXML('<options><row><code>b</code><label>Blue</label></row><row><code>a</code><label>Amber</label></row></options>',true,true);xfa.form.remerge();"
    ).unwrap();
    let replacement = named(&view.root_subform().unwrap(), "choice");
    assert_ne!(initial, replacement);
    assert!(!view.bind_items.borrow().is_empty());
    // Reentrant updates retain the pending queue until the owner unlocks.
    view.lock_update();
    view.update_doc_view().unwrap();
    assert!(!view.bind_items.borrow().is_empty());
    view.unlock_update();
    view.update_doc_view().unwrap();
    assert!(view.bind_items.borrow().is_empty());
    assert_eq!(replacement.count_choice_list_items(false), 2);
    assert_eq!(
        replacement.get_choice_list_item(0, false),
        Some(wide("Blue"))
    );
    assert_eq!(
        replacement.get_choice_list_item(1, false),
        Some(wide("Amber"))
    );
    assert_eq!(replacement.get_choice_list_item(0, true), Some(wide("b")));
    assert_eq!(replacement.get_choice_list_item(1, true), Some(wide("a")));
    assert!(replacement.sync_value(&wide("a"), true));
    view.update_doc_view().unwrap();
    assert_eq!(replacement.get_raw_value(), wide("a"));
    assert_eq!(replacement.count_choice_list_items(false), 2);
    assert!(view.bind_items.borrow().is_empty());
}
