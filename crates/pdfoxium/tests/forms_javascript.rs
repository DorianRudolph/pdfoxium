// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#![cfg(feature = "js")]
#[path = "support/forms.rs"]
#[allow(dead_code)]
mod support;
use pdfoxium::*;
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};
fn options() -> FormOptions {
    FormOptions {
        javascript: true,
        javascript_loop_limit: Some(1000),
        ..Default::default()
    }
}
fn click(form: &FormSession, x: f32, y: f32) -> Vec<FormEvent> {
    let mut events = Vec::new();
    for input in [
        FormInput::MouseMove(Point { x, y }, Default::default()),
        FormInput::MouseDown(Point { x, y }, Default::default()),
        FormInput::MouseUp(Point { x, y }, Default::default()),
    ] {
        events.extend(form.input(0, input).unwrap().events);
    }
    events
}
fn value(form: &FormSession, name: &str) -> String {
    form.fields(0)
        .unwrap()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap()
        .value
}
#[test]
fn scripts_initialize_once_format_calculate_validate_and_recover_after_exception() {
    let bytes = support::scripted();
    if let Some(path) = std::env::var_os("PDFOXIUM_JS_FIXTURE") {
        std::fs::write(path, &bytes).unwrap();
    }
    let doc = Document::from_bytes(bytes).unwrap();
    let form = doc.forms(options()).unwrap();
    let revision = doc.revision();
    form.initialize().unwrap();
    assert!(doc.revision() > revision && doc.is_dirty());
    let revision = doc.revision();
    assert_eq!(value(&form, "startup"), "ready 42");
    form.page_action(0, true).unwrap();
    assert!(doc.revision() > revision);
    form.initialize().unwrap();
    assert_eq!(value(&form, "startup"), "ready 42 open");
    let before = form.render(0, &RenderConfig::scale(2.)).unwrap();
    click(&form, 40., 335.);
    form.input(0, FormInput::SelectAll).unwrap();
    form.input(0, FormInput::Text("25.5".into(), Default::default()))
        .unwrap();
    let rejected = form
        .input(0, FormInput::Text("x".into(), Default::default()))
        .unwrap();
    assert_eq!(rejected.state.text, "25.5");
    form.input(0, FormInput::Blur).unwrap();
    assert_eq!(value(&form, "amount"), "25.5");
    assert_eq!(value(&form, "total"), "51");
    let after = form.render(0, &RenderConfig::scale(2.)).unwrap();
    assert_ne!(before.pixels(), after.pixels());
    click(&form, 40., 95.);
    form.input(0, FormInput::SelectAll).unwrap();
    form.input(0, FormInput::Text("reject".into(), Default::default()))
        .unwrap();
    form.input(0, FormInput::Blur).unwrap();
    assert_eq!(value(&form, "validated"), "keep");
    let errors = click(&form, 250., 215.);
    assert!(errors
        .iter()
        .any(|e| matches!(e,FormEvent::ScriptError{message,..} if message.contains("boom"))));
    click(&form, 40., 275.);
    form.input(0, FormInput::SelectAll).unwrap();
    let input = form
        .input(0, FormInput::Text("3".into(), Default::default()))
        .unwrap();
    assert!(!input
        .events
        .iter()
        .any(|e| matches!(e, FormEvent::ScriptError { .. })));
    form.input(0, FormInput::Blur).unwrap();
    assert_eq!(value(&form, "total"), "76.5");
    form.page_action(0, false).unwrap();
    assert!(value(&form, "startup").ends_with(" close"));
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    let forms = reopened.forms(FormOptions::default()).unwrap();
    assert_eq!(value(&forms, "amount"), "25.5");
    assert_eq!(value(&forms, "total"), "76.5");
}
#[test]
fn dialog_answers_resume_scripts_and_unicode_prompts_use_utf16() {
    let doc = Document::from_bytes(support::scripted()).unwrap();
    let form = doc.forms(options()).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let output = seen.clone();
    form.set_dialog_handler(move |dialog| {
        output.borrow_mut().push(dialog.clone());
        match dialog {
            FormDialog::Alert { .. } => FormDialogResponse::Cancel,
            _ => FormDialogResponse::Text("七😀".into()),
        }
    });
    form.initialize().unwrap();
    click(&form, 250., 335.);
    assert_eq!(value(&form, "qty"), "2");
    click(&form, 250., 275.);
    assert_eq!(value(&form, "qty"), "七😀");
    assert!(
        matches!(&seen.borrow()[0],FormDialog::Alert{message,title,buttons:1,..} if message=="Hello" && title=="Question")
    );
    assert!(
        matches!(&seen.borrow()[1],FormDialog::Response{question,default,..} if question=="How many?"&&default=="2")
    );
}
#[test]
fn script_timers_loop_limits_and_disabled_sessions_are_explicit() {
    let doc = Document::from_bytes(support::scripted_with_scripts(
        "var boot=42; app.setTimeOut(\"this.getField('qty').value=7;\", 10);",
        "while(true){};",
    ))
    .unwrap();
    let form = doc.forms(options()).unwrap();
    form.initialize().unwrap();
    let due = form.next_timer_deadline().unwrap();
    form.run_due_timers(due + Duration::from_millis(1)).unwrap();
    assert_eq!(value(&form, "qty"), "7");
    let errors = click(&form, 250., 215.);
    assert!(errors
        .iter()
        .any(|e| matches!(e, FormEvent::ScriptError { .. })));
    click(&form, 40., 335.);
    assert_eq!(form.state().unwrap().text, "1234.5");
    form.input(0, FormInput::SelectAll).unwrap();
    form.input(0, FormInput::Text("12".into(), Default::default()))
        .unwrap();
    form.input(0, FormInput::Blur).unwrap();
    assert_eq!(value(&form, "total"), "84");
    let disabled = Document::from_bytes(support::scripted()).unwrap();
    let f = disabled.forms(FormOptions::default()).unwrap();
    f.initialize().unwrap();
    assert!(!f.javascript_enabled());
    assert_eq!(value(&f, "startup"), "");
    let clear = Document::from_bytes(support::scripted_with_scripts(
        "var boot=42;",
        "this.getField('qty').value=9;this.dirty=false;",
    ))
    .unwrap();
    let form = clear.forms(options()).unwrap();
    form.initialize().unwrap();
    assert!(clear.is_dirty());
    click(&form, 250., 215.);
    assert_eq!(value(&form, "qty"), "9");
    assert!(!clear.is_dirty());
    #[cfg(target_os = "macos")]
    {
        let doc =
            Document::from_bytes(support::scripted_with_scripts("var boot=app.platform;", ""))
                .unwrap();
        let form = doc.forms(options()).unwrap();
        form.initialize().unwrap();
        assert_eq!(value(&form, "startup"), "ready MAC");
    }
}
fn take<T>(mut ticket: pdfoxium::thread::Ticket<T>) -> Result<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(r) = ticket.try_take() {
            return r;
        }
        assert!(Instant::now() < deadline, "worker stalled");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn dialog(thread: &pdfoxium::thread::DocumentThread) -> (u64, FormDialog) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        for e in thread.take_events() {
            if let pdfoxium::thread::DocEvent::Dialog { id, dialog } = e {
                return (id, dialog);
            }
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn worker_dialog_replies_save_actions_and_shutdown_do_not_deadlock() {
    use pdfoxium::thread::*;
    let thread = DocumentThread::spawn(
        Source::Bytes(support::scripted().into()),
        Config::default(),
        DocOptions {
            forms: options(),
            ..Default::default()
        },
    )
    .unwrap();
    for input in [
        FormInput::MouseMove(Point { x: 250., y: 335. }, Default::default()),
        FormInput::MouseDown(Point { x: 250., y: 335. }, Default::default()),
    ] {
        take(thread.input(0, input)).unwrap();
    }
    let up = thread.input(
        0,
        FormInput::MouseUp(Point { x: 250., y: 335. }, Default::default()),
    );
    let (id, request) = dialog(&thread);
    assert!(matches!(request, FormDialog::Alert { .. }));
    assert!(matches!(
        thread.answer_dialog(id + 1, FormDialogResponse::Ok),
        Err(Error::Stale)
    ));
    assert!(matches!(
        thread.answer_dialog(id, FormDialogResponse::Text("wrong".into())),
        Err(Error::InvalidArgument(_))
    ));
    thread.answer_dialog(id, FormDialogResponse::Ok).unwrap();
    take(up).unwrap();
    assert_eq!(
        take(thread.call(|_, form| Ok(value(form, "qty")))).unwrap(),
        "1"
    );
    let path = std::env::temp_dir().join(format!("pdfoxium-js-save-{}.pdf", std::process::id()));
    let _ = std::fs::remove_file(&path);
    take(thread.save(path.clone())).unwrap();
    assert!(take(thread.call(|_, form| Ok(value(form, "startup"))))
        .unwrap()
        .ends_with(" will-save did-save"));
    let saved = Document::open(&path).unwrap();
    let f = saved.forms(FormOptions::default()).unwrap();
    assert!(value(&f, "startup").ends_with(" will-save"));
    std::fs::remove_file(path).unwrap();
    take(thread.input(
        0,
        FormInput::MouseDown(Point { x: 250., y: 335. }, Default::default()),
    ))
    .unwrap();
    let pending = thread.input(
        0,
        FormInput::MouseUp(Point { x: 250., y: 335. }, Default::default()),
    );
    let _ = dialog(&thread);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(thread);
        let _ = tx.send(());
    });
    rx.recv_timeout(Duration::from_secs(3))
        .expect("closing a pending script dialog must wake the owner");
    drop(pending);
}

#[test]
fn initialization_dialog_is_available_after_spawn_and_keeps_the_realm() {
    use pdfoxium::thread::*;
    let thread = DocumentThread::spawn(
        Source::Bytes(
            support::scripted_with_scripts("var boot=42;app.alert('opening');", "").into(),
        ),
        Config::default(),
        DocOptions {
            forms: options(),
            ..Default::default()
        },
    )
    .unwrap();
    let (id, request) = dialog(&thread);
    assert!(matches!(request,FormDialog::Alert{message,..} if message=="opening"));
    thread.answer_dialog(id, FormDialogResponse::Ok).unwrap();
    assert_eq!(
        take(thread.call(|_, form| Ok(value(form, "startup")))).unwrap(),
        "ready 42"
    );
}

#[test]
fn annotation_page_hooks_initialize_animate_and_stop_hidden_controllers() {
    let doc = Document::from_bytes(support::animation()).unwrap();
    let form = doc.forms(options()).unwrap();
    form.initialize().unwrap();
    form.page_action(0, true).unwrap();
    assert_eq!(
        value(&form, "log"),
        "page-open|screen-open|widget-open|screen-visible|widget-visible"
    );
    let config = RenderConfig::scale(2.).format(PixelFormat::Rgba8);
    let first = form.render(0, &config).unwrap();
    click(&form, 160., 215.);
    let before = doc.revision();
    let deadline = form.next_timer_deadline().unwrap();
    form.run_due_timers(deadline).unwrap();
    let fields = form.fields(0).unwrap();
    assert!(!fields.iter().find(|f| f.name == "f0").unwrap().visible);
    assert!(fields.iter().find(|f| f.name == "f1").unwrap().visible);
    assert!(doc.revision() > before);
    assert!(!doc.is_dirty());
    let second = form.render(0, &config).unwrap();
    // The frame itself, not the focused play button or log, changes red→blue.
    let pixel = |bitmap: &Bitmap| {
        let start = 300 * bitmap.stride() + 100 * 4;
        bitmap.pixels()[start..start + 3].to_vec()
    };
    assert_eq!(pixel(&first), [255, 0, 0]);
    assert_eq!(pixel(&second), [0, 0, 255]);
    form.page_action(0, false).unwrap();
    assert_eq!(value(&form,"log"),"page-open|screen-open|widget-open|screen-visible|widget-visible|screen-invisible|widget-invisible|screen-close|widget-close|page-close");
    let fields = form.fields(0).unwrap();
    // A focused control may still own a caret timer; the animation must stop.
    form.run_due_timers(Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert_eq!(form.fields(0).unwrap(), fields);
}

#[test]
fn annotation_edits_keep_javascript_globals_and_document_timers_alive() {
    let doc = Document::from_bytes(support::scripted_with_scripts(
        "var boot=42;app.setTimeOut(\"this.getField('qty').value=boot;\",100);",
        "",
    ))
    .unwrap();
    let form = doc.forms(options()).unwrap();
    form.initialize().unwrap();
    let due = form.next_timer_deadline().unwrap();
    form.fields(0).unwrap();
    let id = form
        .add_annotation(
            0,
            AnnotationSpec::text_note(
                Rect {
                    left: 20.,
                    bottom: 20.,
                    right: 42.,
                    top: 42.,
                },
                "A note",
            ),
        )
        .unwrap();
    form.set_annotation_contents(0, id, "Edited").unwrap();
    form.move_note(0, id, pdfoxium::Point { x: 30., y: 10. })
        .unwrap();
    let shape = form
        .add_annotation(
            0,
            AnnotationSpec::square(Rect {
                left: 70.,
                bottom: 20.,
                right: 100.,
                top: 50.,
            }),
        )
        .unwrap();
    form.resize_annotation(
        0,
        shape,
        Rect {
            left: 70.,
            bottom: 20.,
            right: 130.,
            top: 70.,
        },
    )
    .unwrap();
    assert_eq!(form.next_timer_deadline(), Some(due));
    form.run_due_timers(due + Duration::from_millis(1)).unwrap();
    assert_eq!(value(&form, "qty"), "42");
    form.remove_annotation(0, id).unwrap();
    assert!(doc.is_dirty());
}
