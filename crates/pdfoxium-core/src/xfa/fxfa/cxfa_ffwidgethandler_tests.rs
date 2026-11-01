// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Actual Boa event dispatch plus recording virtuals for handler geometry,
//! capability checks and callback invalidation. No font measurements are faked.
use super::*;
use crate::{
    fxcrt::fx_coordinates::{CfxRectF, CfxSizeF},
    xfa::{
        fwl::cfwl_widget::WidgetAdapter,
        fxfa::{
            cxfa_ffwidget::{CxfaFfWidget, FfDocViewHost},
            fxfa_basic::{XfaAttribute as A, XfaPacketType},
            parser::cxfa_node::XfaNodeFlag,
            test_support::{new_document, HostCall},
        },
    },
};
use std::cell::{Cell, RefCell};
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
struct RecordingWidget {
    base: Rc<CxfaFfWidget>,
    points: RefCell<Vec<(u32, CfxPointF)>>,
    accepts: Cell<bool>,
    copy_allowed: Cell<bool>,
    text: RefCell<Vec<u32>>,
    focus_hook: RefCell<Option<Box<dyn Fn()>>>,
}
impl LayoutItemWidget for RecordingWidget {
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
    fn set_layout_item(
        &self,
        item: Option<super::super::layout::cxfa_contentlayoutitem::CxfaContentLayoutItem>,
    ) {
        self.base.set_layout_item(item);
    }
}
impl WidgetAdapter for RecordingWidget {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.base.get_rotate_matrix()
    }
    fn display_caret(&self, _: bool, _: Option<CfxRectF>) {}
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        (0, 0.)
    }
}
impl FfWidget for RecordingWidget {
    fn widget(&self) -> &CxfaFfWidget {
        &self.base
    }
    fn is_loaded(&self) -> bool {
        true
    }
    fn accepts_focus_on_button_down(&self, flags: u32, point: CfxPointF, _: MouseCommand) -> bool {
        self.points.borrow_mut().push((flags, point));
        self.accepts.get()
    }
    fn on_set_focus(&self, _: Option<&dyn FfWidget>) -> FfResult<bool> {
        let hook = self.focus_hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        Ok(true)
    }
    fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.points.borrow_mut().push((flags, point));
        true
    }
    fn on_mouse_move(&self, flags: u32, point: CfxPointF) -> bool {
        self.points.borrow_mut().push((flags, point));
        true
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        self.points.borrow_mut().push((0, point));
        FwlWidgetHit::Client
    }
    fn can_copy(&self) -> bool {
        self.copy_allowed.get()
    }
    fn copy(&self) -> Option<Vec<u32>> {
        Some(self.text.borrow().clone())
    }
    fn can_paste(&self) -> bool {
        self.copy_allowed.get()
    }
    fn paste(&self, text: &[u32]) -> bool {
        *self.text.borrow_mut() = text.to_vec();
        true
    }
    fn can_select_all(&self) -> bool {
        self.copy_allowed.get()
    }
}
fn fixture() -> (
    Rc<super::super::cxfa_ffdoc::CxfaFfDoc>,
    Rc<CxfaFfDocView>,
    Rc<RecordingWidget>,
    Rc<dyn LayoutItemWidget>,
) {
    let (doc, _) = new_document(
        r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"/></template></xdp:xdp>"#,
    );
    let view = doc.create_doc_view();
    assert_eq!(view.start_layout().unwrap(), 0);
    assert_eq!(view.do_layout().unwrap(), 100);
    let node = doc
        .model_document()
        .create_node(XfaPacketType::Form, XfaElement::Field)
        .unwrap();
    node.set_widget_ready();
    let base = CxfaFfWidget::new(&node);
    let host: Rc<dyn FfDocViewHost> = view.clone();
    base.set_doc_view(Some(&host));
    let widget = Rc::new(RecordingWidget {
        base,
        points: RefCell::new(Vec::new()),
        accepts: Cell::new(true),
        copy_allowed: Cell::new(false),
        text: RefCell::new(wide("selected")),
        focus_hook: RefCell::new(None),
    });
    let owner: Rc<dyn LayoutItemWidget> = widget.clone();
    let item = doc
        .layout_processor()
        .store()
        .create_content(&node, Some(owner.clone()));
    item.set_position(CfxPointF::new(10., 20.));
    item.set_size(CfxSizeF::new(40., 30.));
    item.set_status_bits(XfaWidgetStatus::kVisible);
    (doc, view, widget, owner)
}
#[test]
fn focus_acceptance_and_button_dispatch_rotate_independently_across_focus_callback() {
    let (_doc, view, widget, owner) = fixture();
    let handler = CxfaFfWidgetHandler::new(Rc::downgrade(&view));
    widget.widget().node().set_integer(A::Rotate, 90, false);
    let point = CfxPointF::new(15., 25.);
    let before = widget.widget().rotate_to_normal(point);
    let node = widget.widget().node().clone();
    *widget.focus_hook.borrow_mut() =
        Some(Box::new(move || node.set_integer(A::Rotate, 180, false)));
    assert!(handler.on_l_button_down(&owner, 17, point).unwrap());
    let after = widget.widget().rotate_to_normal(point);
    assert_ne!(before, after);
    assert_eq!(&*widget.points.borrow(), &[(17, before), (17, after)]);
    widget.accepts.set(false);
    assert!(!handler.on_l_button_down(&owner, 2, point).unwrap());
    assert_eq!(widget.points.borrow().len(), 3);
    assert!(handler.on_mouse_move(widget.as_ref(), 4, point));
    assert_eq!(widget.points.borrow().last(), Some(&(4, after)));
    assert_eq!(
        handler.hit_test(widget.as_ref(), point),
        FwlWidgetHit::Client
    );
    widget
        .widget()
        .layout_item()
        .unwrap()
        .clear_status_bits(XfaWidgetStatus::kVisible);
    assert_eq!(
        handler.hit_test(widget.as_ref(), point),
        FwlWidgetHit::Unknown
    );
}
#[test]
fn capability_gates_and_focus_callback_layout_removal_prevent_stale_dispatch() {
    let (_doc, view, widget, owner) = fixture();
    let handler = CxfaFfWidgetHandler::new(Rc::downgrade(&view));
    assert!(handler.get_selected_text(widget.as_ref()).is_empty());
    handler.paste_text(widget.as_ref(), &wide("blocked"));
    assert_eq!(&*widget.text.borrow(), &wide("selected"));
    assert!(!handler.select_all_text(widget.as_ref()));
    widget.copy_allowed.set(true);
    assert_eq!(handler.get_selected_text(widget.as_ref()), wide("selected"));
    handler.paste_text(widget.as_ref(), &wide("pasted"));
    assert_eq!(&*widget.text.borrow(), &wide("pasted"));
    assert!(handler.select_all_text(widget.as_ref()));
    let base = widget.base.clone();
    *widget.focus_hook.borrow_mut() = Some(Box::new(move || base.set_layout_item(None)));
    assert!(!handler
        .on_l_button_down(&owner, 0, CfxPointF::new(12., 22.))
        .unwrap());
    assert_eq!(widget.points.borrow().len(), 1);
}
#[test]
fn event_handlers_execute_real_calculation_and_preserve_validation_and_null_event_guards() {
    let (doc, host) = new_document(
        r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><field name="calc"><ui><textEdit/></ui><calculate><script contentType="application/x-javascript">'42'</script></calculate><event activity="click"><script contentType="application/x-javascript">globalThis.clicked=true</script></event></field><field name="required"><ui><textEdit/></ui><validate nullTest="error"><message><text name="nullTest">Required</text></message></validate></field><draw name="draw"/></subform></template></xdp:xdp>"#,
    );
    let view = doc.create_doc_view();
    doc.model_document()
        .do_data_merge_with_resolver(doc.script_context().resolver())
        .unwrap();
    let root = view.root_subform().unwrap();
    let calc = root.get_first_child_by_name(&wide("calc")).unwrap();
    let required = root.get_first_child_by_name(&wide("required")).unwrap();
    let draw = root.get_first_child_by_name(&wide("draw")).unwrap();
    let handler = CxfaFfWidgetHandler::new(Rc::downgrade(&view));
    assert!(handler.has_event(Some(&calc), XfaEventType::Calculate));
    assert!(handler.has_event(Some(&calc), XfaEventType::Click));
    assert!(!handler.has_event(Some(&required), XfaEventType::Validate));
    assert!(!handler.has_event(Some(&draw), XfaEventType::Click));
    assert!(!handler.has_event(None, XfaEventType::Click));
    assert_eq!(
        handler.process_event(Some(&calc), None),
        XfaEventError::kNotExist
    );
    let mut event = CxfaEventParam::new(XfaEventType::Unknown);
    assert_eq!(
        handler.process_event(Some(&calc), Some(&mut event)),
        XfaEventError::kNotExist
    );
    event.event_type = XfaEventType::Calculate;
    assert_eq!(
        handler.process_event(Some(&calc), Some(&mut event)),
        XfaEventError::kSuccess
    );
    assert_eq!(calc.get_raw_value(), wide("42"));
    event.event_type = XfaEventType::Click;
    assert_eq!(
        handler.process_event(Some(&calc), Some(&mut event)),
        XfaEventError::kSuccess
    );
    assert_eq!(
        doc.script_context().eval("clicked").unwrap().as_boolean(),
        Some(true)
    );
    event.event_type = XfaEventType::InitCalculate;
    calc.set_flag(XfaNodeFlag::UserInteractive);
    assert_eq!(
        handler.process_event(Some(&calc), Some(&mut event)),
        XfaEventError::kDisabled
    );
    event.event_type = XfaEventType::Validate;
    host.validations.set(false);
    assert_eq!(
        handler.process_event(Some(&required), Some(&mut event)),
        XfaEventError::kDisabled
    );
    host.validations.set(true);
    required.set_is_null(false);
    required
        .get_child(0, XfaElement::Validate, false)
        .unwrap()
        .clear_flag(XfaNodeFlag::NeedsInitApp);
    assert_eq!(
        handler.process_event(Some(&required), Some(&mut event)),
        XfaEventError::kError
    );
    host.calls.borrow_mut().clear();
    doc.notify().handle_widget_event(&required, &mut event);
    assert!(host
        .calls
        .borrow()
        .iter()
        .any(|call| matches!(call,HostCall::Message(text,..) if *text==wide("Required"))));
}

#[test]
fn removing_another_fragment_of_the_same_node_preserves_the_actual_focus_owner() {
    let (doc, view, widget, owner) = fixture();
    let other = Rc::new(RecordingWidget {
        base: CxfaFfWidget::new(widget.widget().node()),
        points: RefCell::new(Vec::new()),
        accepts: Cell::new(true),
        copy_allowed: Cell::new(false),
        text: RefCell::new(Vec::new()),
        focus_hook: RefCell::new(None),
    });
    let other_owner: Rc<dyn LayoutItemWidget> = other.clone();
    let other_item = doc
        .layout_processor()
        .store()
        .create_content(widget.widget().node(), Some(other_owner));
    other_item.set_status_bits(XfaWidgetStatus::kVisible);
    assert!(view.set_focus(Some(owner.clone())).unwrap());
    view.delete_layout_item(other.as_ref());
    assert!(Rc::ptr_eq(&view.focus_widget().unwrap(), &owner));
    assert_eq!(view.focus_node().as_ref(), Some(widget.widget().node()));
    view.delete_layout_item(widget.as_ref());
    assert!(view.focus_widget().is_none());
    assert!(view.focus_node().is_none());
}
