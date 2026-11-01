// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Live PWL replacement after ordinary JavaScript changes widget age counters.
use super::*;
use crate::fpdfapi::{
    page::cpdf_page::CpdfPage,
    parser::{
        cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument, cpdf_parser::ParserError,
    },
};
use crate::fpdfsdk::{
    cpdfsdk_formfillenvironment::FormFillHost,
    cpdfsdk_pageview::SdkPage,
    pwl::{cpwl_edit::CpwlEdit, cpwl_wnd::Window},
};
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
struct Host;
impl FormFillHost for Host {}
struct Fixture {
    env: Rc<CpdfSdkFormFillEnvironment>,
    view: Rc<CpdfSdkPageView>,
    widget: Rc<CpdfSdkWidget>,
    _page: Rc<SdkPage>,
    _host: Rc<Host>,
    _document: Rc<RefCell<CpdfDocument>>,
}
impl Fixture {
    fn new() -> Self {
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        assert_eq!(
            document.borrow_mut().load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(
                    include_bytes!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../upstream/pdfium/testing/resources/text_form.pdf"
                    ))
                    .to_vec()
                )),
                b""
            ),
            ParserError::Success
        );
        let host = Rc::new(Host);
        let interface: Rc<dyn FormFillHost> = host.clone();
        let env = CpdfSdkFormFillEnvironment::new(&document, Some(&interface));
        let page = CpdfPage::from_document(&mut document.borrow_mut(), 0).unwrap();
        let page = SdkPage::new(&document, &page);
        let view = env.get_or_create_page_view(&page).unwrap();
        view.set_valid(true);
        let annotation = view.annot_list()[0].clone();
        let widget = annotation.widget().unwrap();
        assert!(widget.set_value(&wide("Seed")));
        assert!(env.set_focus_annot(&annotation));
        Self {
            env,
            view,
            widget,
            _page: page,
            _host: host,
            _document: document,
        }
    }
    fn field(&self) -> Rc<dyn FormField> {
        self.env
            .interactive_form_filler()
            .get_form_field(&self.widget)
            .unwrap()
    }
    fn window(&self) -> Rc<dyn Window> {
        self.field().base().pwl_window(&self.view).unwrap()
    }
    fn selection(&self) -> (i32, i32) {
        let window = self.window();
        window
            .as_any()
            .downcast_ref::<CpwlEdit>()
            .unwrap()
            .selection()
    }
    fn set_selection(&self, start: i32, end: i32) {
        let window = self.window();
        window
            .as_any()
            .downcast_ref::<CpwlEdit>()
            .unwrap()
            .set_selection(start, end);
    }
    fn script(&self, script: &[u8]) {
        let mut action = CpdfDictionary::new();
        action.set_new_name_for(b"S", b"JavaScript");
        action.set_new_string_for(b"JS", script);
        let mut actions = CpdfDictionary::new();
        actions.set_new_for(b"K", action);
        let dict = self.widget.dict();
        let mut borrow = dict.borrow_mut();
        borrow
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"AA", actions);
    }
    fn before(&self) -> BeforeKeystrokeResult {
        let field = self.field();
        let data = field.base().get_per_pwl_window_data(&self.view).unwrap();
        let mut change = wide("X");
        self.env.run_input_event(|| {
            self.env.interactive_form_filler().on_before_key_stroke(
                Some(data.as_ref()),
                &mut change,
                &[],
                1,
                3,
                true,
                0,
            )
        })
    }
    fn text(&self, expected: &str) {
        assert_eq!(self.widget.get_text(), wide(expected));
    }
}

#[test]
fn appearance_only_keystroke_recreates_the_window_and_applies_the_pending_change_once() {
    let f = Fixture::new();
    f.set_selection(1, 3);
    let old = f.window();
    let age = f.widget.appearance_age();
    let value_age = f.widget.value_age();
    f.script(b"event.target.lineWidth=3;");
    let result = f.before();
    assert!(!result.rc && result.exit);
    assert!(f.widget.appearance_age() > age);
    assert_eq!(f.widget.value_age(), value_age);
    let fresh = f.window();
    assert!(!Rc::ptr_eq(&old, &fresh));
    assert!(!old.wnd().is_alive());
    assert!(fresh.wnd().is_alive());
    f.text("SXd");
    assert_eq!(f.widget.value(), wide("Seed"));
    assert_eq!(f.selection(), (2, 2));
    let data = f.field().base().get_per_pwl_window_data(&f.view).unwrap();
    assert!(data.appearance_age_equals(f.widget.appearance_age()));
}

#[test]
fn rejected_keystroke_after_appearance_change_restores_saved_text_and_selection() {
    let f = Fixture::new();
    f.set_selection(1, 3);
    let old = f.window();
    let value_age = f.widget.value_age();
    f.script(b"event.target.lineWidth=3;event.rc=false;");
    let result = f.before();
    assert!(!result.rc && result.exit);
    assert_eq!(f.widget.value_age(), value_age);
    assert!(!old.wnd().is_alive());
    f.text("Seed");
    assert_eq!(f.widget.get_selected_text(), wide("ee"));
    assert_eq!(f.selection(), (1, 3));
    assert!(f.window().wnd().is_alive());
    assert!(f.env.get_focus_annot().is_some());
}

#[test]
fn value_age_change_reloads_the_new_document_value_before_applying_the_pending_range() {
    let f = Fixture::new();
    f.widget.select_all_text();
    f.widget.replace_selection(&wide("Draft"));
    f.set_selection(1, 3);
    let old = f.window();
    let value_age = f.widget.value_age();
    f.script(b"event.target.value='Reset';");
    let result = f.before();
    assert!(!result.rc && result.exit);
    assert!(f.widget.value_age() > value_age);
    assert!(!old.wnd().is_alive());
    assert_eq!(f.widget.value(), wide("Reset"));
    f.text("RXet");
    assert_eq!(f.selection(), (2, 2));
}

#[test]
fn cancelling_after_a_script_value_change_preserves_the_side_effect_and_saved_edit_state() {
    let f = Fixture::new();
    f.widget.select_all_text();
    f.widget.replace_selection(&wide("Draft"));
    f.set_selection(1, 3);
    let old = f.window();
    f.script(b"event.target.value='Reset';event.rc=false;");
    let result = f.before();
    assert!(!result.rc && result.exit);
    assert!(!old.wnd().is_alive());
    assert_eq!(f.widget.value(), wide("Reset"));
    f.text("Draft");
    assert_eq!(f.widget.get_selected_text(), wide("ra"));
    assert_eq!(f.selection(), (1, 3));
}

#[test]
fn assigning_the_current_appearance_property_keeps_the_existing_window() {
    let f = Fixture::new();
    f.set_selection(1, 3);
    let old = f.window();
    let age = f.widget.appearance_age();
    f.script(b"event.target.lineWidth=event.target.lineWidth;");
    let result = f.before();
    assert!(!result.rc && !result.exit);
    assert_eq!(f.widget.appearance_age(), age);
    assert!(Rc::ptr_eq(&old, &f.window()));
    f.text("SXd");
    assert_eq!(f.selection(), (2, 2));
}

#[test]
fn recreated_window_uses_rewritten_change_and_selection_instead_of_the_old_request() {
    let f = Fixture::new();
    f.set_selection(1, 3);
    let old = f.window();
    f.script(b"event.target.lineWidth=3;event.selStart=0;event.selEnd=event.value.length;event.change='Done';");
    let result = f.before();
    assert!(!result.rc && result.exit);
    assert!(!old.wnd().is_alive());
    f.text("Done");
    assert_eq!(f.selection(), (4, 4));
    f.script(b"");
    assert!(f.view.on_char(u32::from(b'Y'), 0));
    f.text("DoneY");
}

#[test]
fn dispatch_through_the_retired_window_neither_replays_the_key_nor_runs_the_script_twice() {
    let f = Fixture::new();
    f.set_selection(1, 3);
    let old = f.window();
    f.script(
        b"globalThis.recreationKeys=(globalThis.recreationKeys||0)+1;event.target.lineWidth=3;",
    );
    // The input did apply, but its retired dispatch window reports false.
    assert!(!f.view.on_char(u32::from(b'X'), 0));
    f.text("SXd");
    assert!(!old.wnd().is_alive());
    let mut check = CpdfDictionary::new();
    check.set_new_name_for(b"S", b"JavaScript");
    check.set_new_string_for(
        b"JS",
        b"if(globalThis.recreationKeys!==1)throw 'duplicate key';",
    );
    let action = crate::fpdfdoc::cpdf_action::CpdfAction::new(Some(
        crate::fpdfapi::parser::cpdf_object::retain_object(check),
    ));
    assert_eq!(f.env.do_action_doc_open(&action), Ok(true));
    f.script(b"");
    assert!(f.view.on_char(u32::from(b'Y'), 0));
    f.text("SXYd");
    assert!(f.window().wnd().is_alive());
}
