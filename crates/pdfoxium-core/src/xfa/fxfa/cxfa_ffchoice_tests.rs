// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Actual FF document/view/FWL choice and date-edit integration.
use super::*;
use crate::{
    fxcrt::fx_coordinates::CfxPointF,
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::fxfa::{
        cxfa_ffcombobox::CxfaFfComboBox,
        cxfa_ffdatetimeedit::CxfaFfDateTimeEdit,
        cxfa_ffdoc::CxfaFfDoc,
        cxfa_ffpageview::CxfaFfPageView,
        cxfa_ffwidget::{FfDocViewHost, HighlightOption},
        fxfa::XfaWidgetStatus as S,
        fxfa_basic::{XfaAttribute as A, XfaElement as E, XfaPacketType, XfaUnit},
        parser::cxfa_measurement::CxfaMeasurement,
        test_support::{new_document, HostCall},
    },
};
fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}
fn attach<W: FfWidget + 'static>(
    doc: &Rc<CxfaFfDoc>,
    node: &CxfaNode,
    widget: &Rc<W>,
    rect: CfxRectF,
) {
    let view = doc.create_doc_view();
    let host: Rc<dyn FfDocViewHost> = view.clone();
    widget.widget().set_doc_view(Some(&host));
    let area = doc
        .model_document()
        .create_node(XfaPacketType::Form, E::PageArea)
        .unwrap();
    let medium = area.get_or_create_property(0, E::Medium).unwrap();
    medium.set_measure(A::Short, CxfaMeasurement::new(160., XfaUnit::Pt), false);
    medium.set_measure(A::Long, CxfaMeasurement::new(160., XfaUnit::Pt), false);
    let owner = CxfaFfPageView::new(Rc::downgrade(&view), &area);
    let processor = doc.layout_processor();
    let page = processor.store().create_view(&area, Some(owner));
    widget.widget().set_page_view(Some(page.clone()));
    let item = processor.store().create_content(node, Some(widget.clone()));
    page.append_last_child(&item);
    node.set_layout_item(Some(item.0.clone()));
    node.set_widget_ready();
    item.set_position(rect.top_left());
    item.set_size(rect.size());
    item.set_status_bits(S::kVisible);
}
#[test]
fn actual_choice_widgets_restore_commit_notify_and_dispatch_open_events() {
    let (doc,embedding)=new_document(br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <config xmlns="http://www.xfa.org/schema/xci/3.0/"><present><pdf><interactive>1</interactive></pdf></present></config>
      <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record">
        <field name="list"><bind match="none"/><ui><choiceList open="multiSelect"/></ui><items save="0"><text>Alpha</text><text>Beta</text><text>Gamma</text></items><items save="1"><text>v1</text><text>v2</text><text>v3</text></items></field>
        <field name="combo"><bind match="none"/><ui><choiceList textEntry="1" commitOn="select"/></ui><items save="0"><text>Alpha</text><text>Beta</text><text>Gamma</text></items><items save="1"><text>v1</text><text>v2</text><text>v3</text></items><value><text>v1</text></value>
          <event activity="preOpen"><script contentType="application/x-javascript">globalThis.trace.push('pre');</script></event>
          <event activity="postOpen"><script contentType="application/x-javascript">globalThis.trace.push('post');</script></event>
          <event activity="change"><script contentType="application/x-javascript">globalThis.trace.push('change');</script></event>
        </field>
      </subform></template></xdp:xdp>"#);
    let view = doc.create_doc_view();
    let runtime = doc.script_context();
    // Change marks are gated until native layout/ready processing finishes.
    assert!(view.start_layout().unwrap() >= 0);
    assert_eq!(view.do_layout().unwrap(), 100);
    view.stop_layout().unwrap();
    runtime.eval("globalThis.trace=[]").unwrap();
    let record = view.root_subform().unwrap();
    let list_node = record.get_first_child_by_name(&wide("list")).unwrap();
    let combo_node = record.get_first_child_by_name(&wide("combo")).unwrap();
    assert!(list_node.set_selected_items(&[0, 2], false, true));
    let list = CxfaFfListBox::new(&list_node);
    let combo = CxfaFfComboBox::new(&combo_node);
    attach(&doc, &list_node, &list, CfxRectF::new(8., 8., 100., 56.));
    attach(&doc, &combo_node, &combo, CfxRectF::new(8., 80., 100., 20.));
    assert!(list.load_widget().unwrap());
    assert!(combo.load_widget().unwrap());
    let control = list.control().unwrap();
    assert_eq!(control.count_sel_items(), 2);
    assert_eq!(control.get_sel_index(0), 0);
    assert_eq!(control.get_sel_index(1), 2);
    control.clear_selection();
    list.update_fwl_data();
    assert_eq!(control.get_sel_index(1), 2);
    // Script selection writes must promote notifications to the field, not
    // stop at its Value child. Exercise both multi-select reconstruction on
    // deselection and additive selection against an already loaded control.
    runtime
        .eval("xfa.form.record.list.setItemState(0, false)")
        .unwrap();
    assert_eq!(list_node.get_selected_items(), [2]);
    assert_eq!(control.count_sel_items(), 1);
    assert_eq!(control.get_sel_index(0), 2);
    runtime
        .eval("xfa.form.record.list.setItemState(1, true)")
        .unwrap();
    assert_eq!(list_node.get_selected_items(), [2, 1]);
    assert_eq!(control.count_sel_items(), 2);
    assert_eq!(control.get_sel_index(0), 1);
    assert_eq!(control.get_sel_index(1), 2);
    runtime.eval("xfa.form.record.list.setItemState(1, false);xfa.form.record.list.setItemState(0, true)").unwrap();
    assert_eq!(control.count_sel_items(), 2);
    assert_eq!(control.get_sel_index(0), 0);
    assert_eq!(control.get_sel_index(1), 2);
    assert!(view.set_focus(Some(list.clone())).unwrap());
    let item = control.get_item(1).unwrap();
    let point = item.get_rect().center() + list.ui_rect().top_left();
    assert!(list.on_l_button_down(0, point));
    assert!(list.on_l_button_up(0, point));
    assert!(view.set_focus(None).unwrap());
    assert_eq!(list_node.get_selected_items(), [1]);
    assert!(view.set_focus(Some(combo.clone())).unwrap());
    embedding.clear_calls();
    assert!(combo.open_drop_down_list());
    let picker = combo.control().unwrap();
    assert!(picker.is_drop_list_visible());
    assert!(runtime
        .eval("trace.slice(-2).join('|') === 'pre|post'")
        .unwrap()
        .as_boolean()
        .unwrap());
    picker.hide_drop_down_list();
    // ProcessSelChanged consumes the popup list selection. SetCurSel alone
    // only updates the combobox/edit state, as in the native implementation.
    picker.get_combo_list().change_selected(1);
    picker.process_sel_changed(true);
    assert_eq!(combo_node.get_raw_value(), wide("v2"));
    assert!(view.focus_widget().is_none());
    assert!(runtime
        .eval("trace.includes('change')")
        .unwrap()
        .as_boolean()
        .unwrap());
    assert!(embedding
        .calls
        .borrow()
        .iter()
        .any(|call| matches!(call, HostCall::Popup(..))));
    assert!(view.set_focus(Some(list.clone())).unwrap());
    assert!(list_node.insert_choice_list_item_with_notify(&wide("Delta"), &wide("v4"), true));
    assert_eq!(control.count_items(), 4);
    assert!(list_node.delete_item_with_notify(3, true));
    assert_eq!(control.count_items(), 3);
    assert!(combo_node.insert_choice_list_item_with_notify(&wide("Delta"), &wide("v4"), true));
    assert_eq!(picker.get_text_by_index(3), wide("Delta"));
    combo.select_all();
    assert_eq!(combo.copy(), Some(wide("Beta")));
    let mut bitmap = CfxDibitmap::create(160, 160, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        assert!(list
            .render_widget(
                &mut graphics,
                CfxMatrix::default(),
                HighlightOption::NoHighlight
            )
            .unwrap());
        assert!(combo
            .render_widget(
                &mut graphics,
                CfxMatrix::default(),
                HighlightOption::NoHighlight
            )
            .unwrap());
    }
    assert!(bitmap
        .buffer()
        .chunks_exact(4)
        .any(|pixel| pixel != [255, 255, 255, 255]));
}
#[test]
fn actual_date_picker_formats_commits_focus_loss_and_read_only_clipboard() {
    let (doc,embedding)=new_document(br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <config xmlns="http://www.xfa.org/schema/xci/3.0/"><present><pdf><interactive>1</interactive></pdf></present></config>
      <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record">
        <field name="date" locale="en_US"><bind match="none"/><ui><dateTimeEdit/><picture>date{YYYY-MM-DD}</picture></ui><value><date>2024-01-02</date></value>
          <event activity="change"><script contentType="application/x-javascript">globalThis.previousDate = xfa.event.prevText;</script></event>
        </field>
      </subform></template></xdp:xdp>"#);
    let view = doc.create_doc_view();
    let runtime = doc.script_context();
    // Change marks are gated until native layout/ready processing finishes.
    assert!(view.start_layout().unwrap() >= 0);
    assert_eq!(view.do_layout().unwrap(), 100);
    view.stop_layout().unwrap();
    assert!(doc.model_document().get_locale_mgr().is_some());
    let model = view
        .root_subform()
        .unwrap()
        .get_first_child_by_name(&wide("date"))
        .unwrap();
    let date = CxfaFfDateTimeEdit::new(&model);
    attach(&doc, &model, &date, CfxRectF::new(8., 8., 120., 24.));
    assert!(date.load_widget().unwrap());
    let control = date.control().unwrap();
    assert_eq!(control.get_cur_sel(), (2024, 1, 2));
    assert!(view.set_focus(Some(date.clone())).unwrap());
    control.process_sel_changed(2025, 3, 4);
    assert_eq!(model.get_raw_value(), wide("2025-03-04"));
    assert_eq!(control.get_cur_sel(), (2025, 3, 4));
    assert!(view.focus_widget().is_none());
    assert!(runtime
        .eval("previousDate === '2024-01-02'")
        .unwrap()
        .as_boolean()
        .unwrap());
    assert!(embedding.calls.borrow().contains(&HostCall::ChangeMark));
    model.set_enum(A::Access, V::ReadOnly, false);
    date.update_widget_property();
    date.select_all();
    assert!(date.can_copy());
    assert!(!date.can_cut());
    assert!(!date.can_paste());
    let before = date.get_text();
    // Native direct Paste is the edit primitive; the public widget handler
    // checks CanPaste before invoking it for read-only controls.
    view.get_widget_handler()
        .paste_text(date.as_ref(), &wide("2000-01-01"));
    assert_eq!(date.get_text(), before);
    assert!(date.copy().is_some());
}

#[test]
fn repopulated_combo_retains_selected_export_through_popup_and_real_focus_commit() {
    use crate::xfa::fwl::cfwl_message::{FwlMessage, KeyCommand, MouseCommand};
    use crate::xfa::fwl::cfwl_widget::FwlWidget;
    use crate::xfa::fxfa::cxfa_fftextedit::CxfaFfTextEdit;
    let (doc, _embedding) = new_document(br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <config xmlns="http://www.xfa.org/schema/xci/3.0/"><present><pdf><interactive>1</interactive></pdf></present></config>
      <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record">
        <field name="combo"><bind match="none"/><ui><choiceList textEntry="1"/></ui>
          <items save="0"><text>A</text><text>BB</text><text>CCC</text><text>DDDD</text><text>EEEEE</text></items>
          <items save="1"><text>a</text><text>b</text><text>c</text><text>d</text><text>e</text></items>
          <value><text>c</text></value></field>
        <field name="sink"><bind match="none"/><ui><textEdit/></ui><value><text>Sink</text></value></field>
      </subform></template></xdp:xdp>"#);
    let view = doc.create_doc_view();
    assert!(view.start_layout().unwrap() >= 0);
    assert_eq!(view.do_layout().unwrap(), 100);
    view.stop_layout().unwrap();
    let record = view.root_subform().unwrap();
    let node = record.get_first_child_by_name(&wide("combo")).unwrap();
    let sink_node = record.get_first_child_by_name(&wide("sink")).unwrap();
    let combo = CxfaFfComboBox::new(&node);
    let sink = CxfaFfTextEdit::new(&sink_node);
    attach(&doc, &node, &combo, CfxRectF::new(8., 8., 120., 24.));
    attach(&doc, &sink_node, &sink, CfxRectF::new(8., 60., 120., 24.));
    assert!(combo.load_widget().unwrap());
    assert!(sink.load_widget().unwrap());
    let picker = combo.control().unwrap();
    assert_eq!(picker.get_cur_sel(), 2);
    assert_eq!(
        picker.get_edit_text(),
        wide("CCC"),
        "newly loaded selected display"
    );
    assert_eq!(
        combo.get_current_text(),
        wide("c"),
        "newly loaded selected export"
    );
    let runtime = doc.script_context();
    runtime.eval("var f=xfa.form.record.combo;f.deleteItem(0);f.deleteItem(1);f.clearItems();f.addItem('XX','x');f.addItem('YYY','y');f.selectedIndex=1;").unwrap();
    // Public widget event handlers flush their UpdateScope after executing
    // script. Bare runtime evaluation needs the corresponding document-view
    // update before inspecting the live control.
    view.update_doc_view().unwrap();
    let registered = view.get_widget_for_node(&node).unwrap();
    assert!(
        std::ptr::eq(registered.as_ff_widget().unwrap().widget(), combo.widget(),),
        "attached widget remains the live notification target"
    );
    assert_eq!(node.get_raw_value(), wide("y"), "script field value");
    assert_eq!(
        picker.get_cur_sel(),
        1,
        "live repopulated control selection"
    );
    assert_eq!(
        picker.get_edit_text(),
        wide("YYY"),
        "live repopulated control display"
    );
    assert!(view.set_focus(Some(combo.clone())).unwrap());
    assert_eq!(combo.get_current_text(), wide("y"), "focused export");
    assert!(combo.open_drop_down_list());
    assert!(picker.is_drop_list_visible());
    let target = picker.clone();
    let mut escape = FwlMessage::key(Some(target), KeyCommand::KeyDown, 0, 0x1b);
    picker.on_process_message(&mut escape);
    assert!(!picker.is_drop_list_visible());
    assert_eq!(
        combo.get_current_text(),
        wide("y"),
        "dismissed popup export"
    );
    let edit = picker.get_combo_edit();
    let mut click = FwlMessage::mouse(
        Some(edit.clone()),
        MouseCommand::LeftButtonDown,
        0,
        CfxPointF::new(1., 4.),
    );
    edit.on_process_message(&mut click);
    let mut release = FwlMessage::mouse(
        Some(edit.clone()),
        MouseCommand::LeftButtonUp,
        0,
        CfxPointF::new(1., 4.),
    );
    edit.on_process_message(&mut release);
    assert_eq!(
        combo.get_current_text(),
        wide("y"),
        "collapsed caret export"
    );
    assert!(view.set_focus(Some(sink)).unwrap());
    assert_eq!(node.get_raw_value(), wide("y"), "real blur commit");
    assert_eq!(node.get_selected_items(), [1]);
}
