// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived concrete FF geometry/layout/input regression fixtures.
use super::*;
use crate::{
    fxcrt::fx_coordinates::{CfxPointF, CfxSizeF},
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::fxfa::{
        cxfa_ffexclgroup::CxfaFfExclGroup,
        cxfa_ffline::{rect_from_hand, CxfaFfLine},
        cxfa_ffrectangle::CxfaFfRectangle,
        fxfa::XfaWidgetStatus as S,
        fxfa_basic::{
            XfaAttribute as A, XfaAttributeValue as V, XfaElement as E, XfaPacketType, XfaUnit,
        },
        layout::cxfa_layoutitem::CxfaLayoutStore,
        parser::{cxfa_document::CxfaDocument, cxfa_measurement::CxfaMeasurement},
    },
};
fn node(doc: &Rc<CxfaDocument>, element: E) -> CxfaNode {
    doc.create_node(XfaPacketType::Form, element).unwrap()
}
fn attach<W: FfWidget + 'static>(
    store: &Rc<CxfaLayoutStore>,
    node: &CxfaNode,
    widget: &Rc<W>,
    rect: CfxRectF,
) {
    let item = store.create_content(node, Some(widget.clone()));
    item.set_position(rect.top_left());
    item.set_size(rect.size());
    item.set_status_bits(S::kVisible);
}
fn bitmap() -> CfxDibitmap {
    let mut b = CfxDibitmap::create(64, 64, FxdibFormat::Bgra, 0).unwrap();
    b.clear(0xffffffff);
    b
}
fn pixel(b: &CfxDibitmap, x: usize, y: usize) -> u32 {
    u32::from_le_bytes(
        b.scanline(y as i32).unwrap()[x * 4..x * 4 + 4]
            .try_into()
            .unwrap(),
    )
}
#[test]
fn rectangle_and_arc_widgets_apply_model_margin_and_layout_visibility() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    for arc in [false, true] {
        let model = node(&doc, E::Draw);
        let value = model.get_or_create_property(0, E::Value).unwrap();
        let shape = value
            .get_or_create_property(0, if arc { E::Arc } else { E::Rectangle })
            .unwrap();
        shape
            .box_get_or_create_fill()
            .unwrap()
            .fill_set_color(0xffff0000);
        shape
            .box_get_edge(0)
            .unwrap()
            .set_enum(A::Presence, V::Hidden, false);
        let margin = model.get_or_create_property(0, E::Margin).unwrap();
        for attribute in [A::LeftInset, A::TopInset, A::RightInset, A::BottomInset] {
            margin.set_measure(attribute, CxfaMeasurement::new(4., XfaUnit::Pt), false);
        }
        let widget: Rc<dyn FfWidget> = if arc {
            CxfaFfArc::new(&model)
        } else {
            CxfaFfRectangle::new(&model)
        };
        let item = store.create_content(&model, None);
        item.set_position(CfxPointF::new(8., 8.));
        item.set_size(CfxSizeF::new(40., 32.));
        widget.set_layout_item(Some(item.clone()));
        item.set_status_bits(S::kVisible);
        widget.perform_layout().unwrap();
        let mut b = bitmap();
        {
            let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
            let mut g = CfgasGeGraphics::new(&mut device);
            assert!(widget
                .render_widget(&mut g, CfxMatrix::default(), HighlightOption::NoHighlight)
                .unwrap());
        }
        assert_eq!(pixel(&b, 28, 24), 0xffff0000);
        assert_eq!(pixel(&b, 9, 20), 0xffffffff);
        if arc {
            assert_eq!(pixel(&b, 12, 12), 0xffffffff);
        } else {
            assert_eq!(pixel(&b, 13, 13), 0xffff0000);
        }
        item.clear_status_bits(S::kVisible);
        let mut b = bitmap();
        {
            let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
            let mut g = CfgasGeGraphics::new(&mut device);
            assert!(widget
                .render_widget(&mut g, CfxMatrix::default(), HighlightOption::NoHighlight)
                .unwrap());
        }
        assert_eq!(pixel(&b, 28, 24), 0xffffffff);
    }
}
#[test]
fn line_widget_uses_slope_edge_visibility_and_hand_offsets() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let model = node(&doc, E::Draw);
    let value = model.get_or_create_property(0, E::Value).unwrap();
    let line = value.get_or_create_property(0, E::Line).unwrap();
    line.set_enum(A::Slope, V::Slash, false);
    let edge = line.get_or_create_property(0, E::Edge).unwrap();
    edge.stroke_set_color(0xff0000ff);
    edge.stroke_set_ms_thickness(CxfaMeasurement::new(2., XfaUnit::Pt));
    let widget = CxfaFfLine::new(&model);
    attach(&store, &model, &widget, CfxRectF::new(8., 8., 32., 32.));
    widget.perform_layout().unwrap();
    let mut b = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
        let mut g = CfgasGeGraphics::new(&mut device);
        widget
            .render_widget(&mut g, CfxMatrix::default(), HighlightOption::NoHighlight)
            .unwrap();
    }
    // The diagonal crosses this pixel partly; AGG blends the blue stroke
    // against white at its antialiased edge.
    let stroke = pixel(&b, 24, 24);
    assert_eq!(stroke & 0xff0000ff, 0xff0000ff);
    assert_eq!((stroke >> 16) & 255, (stroke >> 8) & 255);
    assert!((stroke >> 16) & 255 < 128);
    assert_eq!(pixel(&b, 12, 12), 0xffffffff);
    edge.set_enum(A::Presence, V::Hidden, false);
    let mut b = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
        let mut g = CfgasGeGraphics::new(&mut device);
        widget
            .render_widget(&mut g, CfxMatrix::default(), HighlightOption::NoHighlight)
            .unwrap();
    }
    assert_eq!(pixel(&b, 24, 24), 0xffffffff);
    for (hand, expected) in [(V::Left, 12.), (V::Right, 8.), (V::Even, 10.)] {
        let mut vertical = CfxRectF::new(10., 20., 0., 30.);
        rect_from_hand(&mut vertical, hand, 4.);
        assert_eq!(vertical.left, expected);
    }
    let mut diagonal = CfxRectF::new(10., 20., 30., 40.);
    rect_from_hand(&mut diagonal, V::Left, 4.);
    assert_eq!(diagonal, CfxRectF::new(8., 18., 34., 44.));
}
#[test]
fn exclusion_group_draws_its_actual_border_model() {
    let doc = CxfaDocument::new();
    let store = CxfaLayoutStore::new();
    let model = node(&doc, E::ExclGroup);
    let border = model.get_or_create_property(0, E::Border).unwrap();
    border
        .box_get_or_create_fill()
        .unwrap()
        .fill_set_color(0xff00ff00);
    let widget = CxfaFfExclGroup::new(&model);
    attach(&store, &model, &widget, CfxRectF::new(8., 8., 40., 32.));
    widget.perform_layout().unwrap();
    let mut b = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
        let mut g = CfgasGeGraphics::new(&mut device);
        widget
            .render_widget(&mut g, CfxMatrix::default(), HighlightOption::NoHighlight)
            .unwrap();
    }
    assert_eq!(pixel(&b, 28, 24), 0xff00ff00);
    assert_eq!(pixel(&b, 4, 4), 0xffffffff);
}

use crate::xfa::fwl::{
    cfwl_message::MouseCommand,
    cfwl_widget::{FwlWidget, WidgetState},
    fwl_widgethit::FwlWidgetHit,
};
use crate::xfa::fxfa::{
    cxfa_ffcheckbutton::CxfaFfCheckButton,
    cxfa_ffpushbutton::CxfaFfPushButton,
    cxfa_fftext::CxfaFfText,
    cxfa_ffwidget::FfDocViewHost,
    cxfa_textprovider::TextProviderType,
    parser::cxfa_node_check::XfaCheckState,
    test_support::{new_document, HostCall},
};
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn attach_live<W: FfWidget + 'static>(
    doc: &crate::xfa::fxfa::cxfa_ffdoc::CxfaFfDoc,
    node: &CxfaNode,
    widget: &Rc<W>,
    rect: CfxRectF,
) {
    let processor = doc.layout_processor();
    let item = processor.store().create_content(node, Some(widget.clone()));
    node.set_layout_item(Some(item.0.clone()));
    node.set_widget_ready();
    item.set_position(rect.top_left());
    item.set_size(rect.size());
    item.set_status_bits(S::kVisible);
}
#[test]
fn actual_doc_view_push_and_check_input_commit_values_and_boa_events() {
    let (doc,embedding)=new_document(br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <config xmlns="http://www.xfa.org/schema/xci/3.0/"><present><pdf><interactive>1</interactive></pdf></present></config>
      <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record">
        <field name="push"><ui><button highlight="inverted"/></ui><font typeface="Helvetica" size="12pt"/>
          <caption><value><text>Push</text></value></caption><items><text name="rollover">Hover</text><text name="down">Down</text></items>
          <event activity="click"><script contentType="application/x-javascript">globalThis.trace.push('push');</script></event>
        </field>
        <field name="check"><bind match="none"/><ui><checkButton/></ui><value><text>off</text></value><items><text>yes</text><text>off</text><text>neutral</text></items>
          <event activity="change"><script contentType="application/x-javascript">globalThis.trace.push('change:'+xfa.event.prevText+':'+this.rawValue);</script></event>
          <event activity="click"><script contentType="application/x-javascript">globalThis.trace.push('click:'+this.rawValue);</script></event>
        </field>
      </subform></template></xdp:xdp>"#);
    let view = doc.create_doc_view();
    let runtime = doc.script_context();
    // User commits publish a change mark after the native ready-layout phase.
    // Complete that real lifecycle before using fixed geometry for input.
    assert!(view.start_layout().unwrap() >= 0);
    assert_eq!(view.do_layout().unwrap(), 100);
    view.stop_layout().unwrap();
    runtime.eval("globalThis.trace=[]").unwrap();
    let record = view.root_subform().unwrap();
    let push_node = record.get_first_child_by_name(&wide("push")).unwrap();
    let check_node = record.get_first_child_by_name(&wide("check")).unwrap();
    let push = CxfaFfPushButton::new(&push_node, &push_node.get_ui_child_node().unwrap());
    let check = CxfaFfCheckButton::new(&check_node, &check_node.get_ui_child_node().unwrap());
    let host: Rc<dyn FfDocViewHost> = view;
    push.widget().set_doc_view(Some(&host));
    check.widget().set_doc_view(Some(&host));
    attach_live(&doc, &push_node, &push, CfxRectF::new(8., 8., 40., 20.));
    attach_live(&doc, &check_node, &check, CfxRectF::new(8., 40., 40., 16.));
    assert!(push.load_widget().unwrap());
    assert!(check.load_widget().unwrap());
    embedding.clear_calls();
    assert!(push.on_l_button_down(0, CfxPointF::new(20., 16.)));
    assert!(push.on_l_button_up(0, CfxPointF::new(20., 16.)));
    let rect = check.ui_rect();
    let point = CfxPointF::new(rect.left + 5., rect.top + 5.);
    assert!(check.on_l_button_down(0, point));
    assert!(check.on_l_button_up(0, point));
    assert_eq!(check_node.get_raw_value(), wide("yes"));
    assert_eq!(check_node.get_check_state(), XfaCheckState::On);
    assert!(runtime
        .eval("trace.join('|') === 'push|change:off:yes|click:yes'")
        .unwrap()
        .as_boolean()
        .unwrap());
    assert!(embedding.calls.borrow().contains(&HostCall::ChangeMark));
    check.set_fwl_check_state(XfaCheckState::Neutral);
    check.set_fwl_check_state(XfaCheckState::Off);
    assert_eq!(
        (check.control().unwrap().base().get_states() & WidgetState::CHECKBOX_CHECK_MASK).0,
        0
    );
    let control = push.control().unwrap();
    control
        .base()
        .set_states(WidgetState::PRESSED | WidgetState::HOVERED);
    let mut b = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
        let mut g = CfgasGeGraphics::new(&mut device);
        assert!(push
            .render_widget(&mut g, CfxMatrix::default(), HighlightOption::NoHighlight)
            .unwrap());
        assert!(check
            .render_widget(&mut g, CfxMatrix::default(), HighlightOption::NoHighlight)
            .unwrap());
    }
    assert_ne!(pixel(&b, 28, 18), 0xffffffff);
}
#[test]
fn paginated_rich_text_loads_all_blocks_and_renders_the_second_fragment() {
    let mut paragraphs = String::new();
    for index in 10..30 {
        paragraphs.push_str(&format!(
            "<p style=\"margin:0pt;line-height:16pt\"><span style=\"color:red;font-weight:bold\">MMMM{index}</span></p>"
        ));
    }
    let xml = format!(
        "<xdp:xdp xmlns:xdp=\"http://ns.adobe.com/xdp/\">\
         <template xmlns=\"http://www.xfa.org/schema/xfa-template/3.3/\">\
         <subform name=\"record\" layout=\"tb\">\
         <pageSet><pageArea name=\"page\"><occur min=\"1\" max=\"-1\"/>\
         <medium short=\"240pt\" long=\"240pt\"/><contentArea w=\"180pt\" h=\"180pt\"/>\
         </pageArea></pageSet><draw name=\"story\" w=\"180pt\"><keep intact=\"none\"/>\
         <font typeface=\"Helvetica\" size=\"14pt\"/><value><exData contentType=\"text/html\">\
         <body xmlns=\"http://www.w3.org/1999/xhtml\">{paragraphs}</body>\
         </exData></value></draw></subform></template></xdp:xdp>"
    );
    let (doc, _embedding) = new_document(xml);
    let view = doc.create_doc_view();
    assert!(view.start_layout().unwrap() >= 0);
    assert_eq!(view.do_layout().unwrap(), 100);
    view.stop_layout().unwrap();
    assert_eq!(view.count_page_views(), 2);
    let story = view
        .root_subform()
        .unwrap()
        .get_first_child_by_name(&wide("story"))
        .unwrap();
    let first = story
        .get_layout_item()
        .unwrap()
        .as_content_layout_item()
        .unwrap()
        .get_first();
    let second = first.next().unwrap();
    assert!(second.next().is_none());
    assert!(first.size().height > 0. && second.size().height > 0.);
    let host: Rc<dyn FfDocViewHost> = view;
    let layout = host.text_layout(&story).unwrap();
    for fragment in [&first, &second] {
        let owner = fragment.widget().unwrap();
        let widget = owner.as_ff_widget().unwrap();
        assert!(widget.load_widget().unwrap());
        assert!(!layout.with_layout(|layout| Ok(layout.has_block())).unwrap());
        let mut bitmap = CfxDibitmap::create(240, 240, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0xffffffff);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut graphics = CfgasGeGraphics::new(&mut device);
            assert!(widget
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
            .any(|pixel| pixel[..3] != [255, 255, 255]));
    }
    let actual = layout
        .with_layout(|layout| {
            Ok(layout
                .piece_lines()
                .iter()
                .flat_map(|line| &line.text_pieces)
                .flat_map(|piece| &piece.piece.text)
                .copied()
                .filter(|&ch| ch != 10 && ch != 13)
                .collect::<Vec<_>>())
        })
        .unwrap();
    let expected = (10..30)
        .map(|index| format!("MMMM{index}"))
        .collect::<String>();
    assert_eq!(actual, wide(&expected));
}

#[test]
fn actual_rich_text_widget_renders_and_activates_its_link_through_document_view() {
    let (doc,embedding)=new_document(br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record">
        <draw name="link"><font typeface="Helvetica" size="12pt"/><value><exData contentType="text/html"><body xmlns="http://www.w3.org/1999/xhtml"><p><a href="https://example.org">Open link</a></p></body></exData></value></draw>
      </subform></template></xdp:xdp>"#);
    let view = doc.create_doc_view();
    let runtime = doc.script_context();
    doc.model_document()
        .do_data_merge_with_resolver(runtime.resolver())
        .unwrap();
    let model = view
        .root_subform()
        .unwrap()
        .get_first_child_by_name(&wide("link"))
        .unwrap();
    let widget = CxfaFfText::new(&model);
    let host: Rc<dyn FfDocViewHost> = view;
    widget.set_doc_view(Some(&host));
    attach_live(&doc, &model, &widget, CfxRectF::new(8., 8., 48., 40.));
    let layout = host
        .load_text_layout(&model, TextProviderType::Text)
        .unwrap()
        .unwrap();
    layout
        .with_layout(|layout| layout.layout(CfxSizeF::new(48., 40.)))
        .unwrap();
    widget.perform_layout().unwrap();
    let local = layout
        .with_layout(|layout| {
            Ok(layout
                .piece_lines()
                .iter()
                .flat_map(|line| &line.text_pieces)
                .find(|p| p.link_data.is_some())
                .unwrap()
                .piece
                .rect
                .center())
        })
        .unwrap();
    let point = local + CfxPointF::new(8., 8.);
    assert!(widget.is_loaded());
    assert_eq!(widget.hit_test(point), FwlWidgetHit::HyperLink);
    assert!(widget.accepts_focus_on_button_down(0, point, MouseCommand::LeftButtonDown));
    assert!(widget.on_l_button_down(0, point));
    assert!(widget.on_l_button_up(0, point));
    assert!(embedding
        .calls
        .borrow()
        .contains(&HostCall::Goto(wide("https://example.org"))));
    let mut b = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut b, false, None);
        let mut g = CfgasGeGraphics::new(&mut device);
        assert!(widget
            .render_widget(&mut g, CfxMatrix::default(), HighlightOption::NoHighlight)
            .unwrap());
    }
    assert!(b
        .buffer()
        .chunks_exact(4)
        .any(|p| p != [255, 255, 255, 255]));
    // The real shared layout owner is unavailable only during its own active
    // callback, with state restored for the next draw rather than a RefCell panic.
    let other = layout.clone();
    layout
        .with_layout(|_| {
            assert!(matches!(
                other.with_layout(|_| Ok(())),
                Err(crate::xfa::fxfa::cxfa_ffwidget::FfWidgetError::ReentrantTextLayout)
            ));
            Ok(())
        })
        .unwrap();
    assert!(layout.with_layout(|layout| Ok(layout.is_loaded())).unwrap());
}

#[test]
fn script_default_values_refresh_loaded_bound_text_and_multiselect_controls() {
    use crate::xfa::fxfa::{cxfa_fflistbox::CxfaFfListBox, cxfa_fftextedit::CxfaFfTextEdit};
    let (doc, _embedding) = new_document(br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record"><pageSet><pageArea name="page"><medium short="240pt" long="240pt"/><contentArea w="200pt" h="200pt"/></pageArea></pageSet>
        <field name="text" w="100pt" h="20pt"><bind match="once"/><ui><textEdit/></ui><font typeface="Helvetica" size="12pt"/><value><text>before</text></value></field>
        <field name="choices" w="100pt" h="60pt"><bind match="once"/><ui><choiceList open="multiSelect"/></ui><font typeface="Helvetica" size="12pt"/>
          <items><text>A</text><text>B</text><text>C</text></items><value><exData contentType="text/xml">A</exData></value>
        </field>
      </subform></template>
      <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><record><text>before</text><choices><value>A</value></choices></record></xfa:data></xfa:datasets>
      </xdp:xdp>"#);
    let view = doc.create_doc_view();
    assert!(view.start_layout().unwrap() >= 0);
    assert_eq!(view.do_layout().unwrap(), 100);
    view.stop_layout().unwrap();
    let root = view.root_subform().unwrap();
    let text_node = root.get_first_child_by_name(&wide("text")).unwrap();
    let list_node = root.get_first_child_by_name(&wide("choices")).unwrap();
    let text = CxfaFfTextEdit::new(&text_node);
    let list = CxfaFfListBox::new(&list_node);
    let page = view.get_page_view(0).expect("real laid-out page view");
    let host: Rc<dyn FfDocViewHost> = view;
    text.widget().set_doc_view(Some(&host));
    list.widget().set_doc_view(Some(&host));
    attach_live(&doc, &text_node, &text, CfxRectF::new(0., 0., 100., 20.));
    attach_live(&doc, &list_node, &list, CfxRectF::new(0., 20., 100., 60.));
    // Loaded in PDFium means associated with a page, not merely owning an
    // FWL control. Preserve the real lifecycle for our replacement controls.
    for (model, widget) in [(&text_node, text.widget()), (&list_node, list.widget())] {
        widget.set_layout_item(model.get_layout_item().unwrap().as_content_layout_item());
        widget.set_page_view(Some(page.clone()));
    }
    assert!(text.load_widget().unwrap());
    assert!(list.load_widget().unwrap());
    assert!(text.is_loaded());
    assert!(list.is_loaded());
    assert_eq!(text.get_text(), wide("before"));
    assert_eq!(list.control().unwrap().count_sel_items(), 1);
    let runtime = doc.script_context();
    runtime
        .eval("xfa.form.record.text.rawValue='after';xfa.form.record.choices.rawValue='B\\nC';")
        .unwrap();
    assert_eq!(text_node.get_raw_value(), wide("after"));
    assert_eq!(text.get_text(), wide("after"));
    assert_eq!(
        text_node.get_bind_data().unwrap().get_content(),
        wide("after")
    );
    let control = list.control().unwrap();
    assert_eq!(control.count_sel_items(), 2);
    assert!(!control.get_item(0).unwrap().is_selected());
    assert!(control.get_item(1).unwrap().is_selected());
    assert!(control.get_item(2).unwrap().is_selected());
    // Repeating the same script write does not create a second notification
    // or reset control state: the existing stored-value equality guard wins.
    runtime
        .eval("xfa.form.record.text.rawValue='after';xfa.form.record.choices.rawValue='B\\nC';")
        .unwrap();
    assert_eq!(text.get_text(), wide("after"));
    assert_eq!(control.count_sel_items(), 2);
}
