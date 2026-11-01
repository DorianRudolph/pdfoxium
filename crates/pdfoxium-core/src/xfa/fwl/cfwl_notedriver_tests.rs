// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, CfxVector},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_event::{EventData, EventType, ScrollCode},
            cfwl_widget::{CfwlWidget, FwlWidget, Properties, WidgetState},
            test_support::new_app,
        },
    },
};

type EventHook = Rc<dyn Fn(&mut FwlEvent)>;
type MessageHook = Rc<dyn Fn(&mut FwlMessage)>;
struct Spy {
    base: CfwlWidget,
    kind: FwlType,
    messages: RefCell<Vec<MessageData>>,
    events: RefCell<Vec<EventType>>,
    event_hook: RefCell<Option<EventHook>>,
    message_hook: RefCell<Option<MessageHook>>,
}
impl Spy {
    fn new(
        app: &Rc<CfwlApp>,
        kind: FwlType,
        parent: Option<WidgetRef>,
        rect: CfxRectF,
    ) -> Rc<Self> {
        let widget = Rc::new(Self {
            base: CfwlWidget::new(app, Properties::default(), parent),
            kind,
            messages: RefCell::new(Vec::new()),
            events: RefCell::new(Vec::new()),
            event_hook: RefCell::new(None),
            message_hook: RefCell::new(None),
        });
        widget.base.set_widget_rect(rect);
        app.register_widget(widget.clone());
        widget
    }
    fn widget(&self) -> WidgetRef {
        self.base.self_ref().unwrap()
    }
}
impl FwlWidget for Spy {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        self.kind
    }
    fn is_form(&self) -> bool {
        self.kind == FwlType::Form
    }
    fn update(&self) {}
    fn draw_widget(&self, _: &mut CfgasGeGraphics<'_, '_, '_>, _: CfxMatrix) {}
    fn on_process_message(&self, message: &mut FwlMessage) {
        self.messages.borrow_mut().push(message.data.clone());
        let hook = self.message_hook.borrow().clone();
        if let Some(hook) = hook {
            hook(message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        self.events.borrow_mut().push(event.get_type());
        let hook = self.event_hook.borrow().clone();
        if let Some(hook) = hook {
            hook(event);
        }
    }
}
fn widget(app: &Rc<CfwlApp>) -> Rc<Spy> {
    Spy::new(app, FwlType::Unknown, None, CfxRectF::default())
}
fn click(source: &WidgetRef) -> FwlEvent {
    FwlEvent::new(Some(source.clone()), EventData::Click)
}

#[test]
fn source_filters_invalidation_reregistration_and_new_targets_during_dispatch() {
    let (app, _) = new_app();
    let source = widget(&app).widget();
    let other = widget(&app).widget();
    let first = widget(&app);
    let second = widget(&app);
    let third = widget(&app);
    let weak_app = Rc::downgrade(&app);
    let second_ref = second.widget();
    let third_ref = third.widget();
    let source_ref = source.clone();
    let once = Rc::new(Cell::new(true));
    *first.event_hook.borrow_mut() = Some(Rc::new(move |_| {
        if once.replace(false) {
            let app = weak_app.upgrade().unwrap();
            app.note_driver().unregister_event_target(&second_ref);
            app.note_driver()
                .register_event_target(&third_ref, Some(&source_ref));
        }
    }));
    app.note_driver()
        .register_event_target(&first.widget(), Some(&source));
    app.note_driver()
        .register_event_target(&second.widget(), Some(&source));
    app.note_driver().send_event(&mut click(&source));
    assert_eq!(first.events.borrow().len(), 1);
    assert!(second.events.borrow().is_empty());
    assert!(third.events.borrow().is_empty());
    app.note_driver().send_event(&mut click(&source));
    assert_eq!(third.events.borrow().len(), 1);
    app.note_driver()
        .register_event_target(&second.widget(), Some(&source));
    app.note_driver().send_event(&mut click(&source));
    assert_eq!(second.events.borrow().len(), 1);
    app.note_driver().send_event(&mut click(&other));
    assert_eq!(first.events.borrow().len(), 3);
    assert_eq!(second.events.borrow().len(), 1);
    assert_eq!(third.events.borrow().len(), 2);
    app.remove_widget(source.base().id());
    app.note_driver().send_event(&mut click(&other));
    assert_eq!(first.events.borrow().len(), 3);
}

#[test]
fn focus_key_default_return_and_null_routing() {
    let (app, _) = new_app();
    let form = Spy::new(&app, FwlType::Form, None, CfxRectF::new(0., 0., 200., 100.));
    let input = Spy::new(
        &app,
        FwlType::Unknown,
        Some(form.widget()),
        CfxRectF::new(10., 20., 40., 30.),
    );
    let button = Spy::new(
        &app,
        FwlType::PushButton,
        Some(form.widget()),
        CfxRectF::new(80., 20., 40., 30.),
    );
    button.base.set_states(WidgetState::PUSHBUTTON_DEFAULT);
    app.note_driver()
        .process_message(&mut FwlMessage::set_focus(Some(input.widget())));
    assert!(Rc::ptr_eq(
        &app.note_driver().focus().unwrap(),
        &input.widget()
    ));
    let mut key = FwlMessage::key(Some(form.widget()), KeyCommand::Char, 4, u32::from('a'));
    app.note_driver().process_message(&mut key);
    assert!(Rc::ptr_eq(
        key.destination.as_ref().unwrap(),
        &input.widget()
    ));
    assert_eq!(input.messages.borrow().len(), 2);
    app.note_driver()
        .process_message(&mut FwlMessage::kill_focus(Some(button.widget())));
    assert!(app.note_driver().focus().is_some());
    app.note_driver()
        .process_message(&mut FwlMessage::kill_focus(Some(input.widget())));
    assert!(app.note_driver().focus().is_none());
    let mut enter = FwlMessage::key(Some(form.widget()), KeyCommand::KeyDown, 0, 0x0d);
    app.note_driver().process_message(&mut enter);
    assert!(Rc::ptr_eq(
        enter.destination.as_ref().unwrap(),
        &button.widget()
    ));
    let before = form.messages.borrow().len();
    app.note_driver().process_message(&mut FwlMessage::key(
        Some(form.widget()),
        KeyCommand::Char,
        0,
        u32::from('x'),
    ));
    assert_eq!(form.messages.borrow().len(), before);
    app.note_driver()
        .process_message(&mut FwlMessage::set_focus(None));
    app.remove_widget(input.base.id());
    app.note_driver()
        .process_message(&mut FwlMessage::set_focus(Some(input.clone())));
    assert!(app.note_driver().focus().is_none());
}

#[test]
fn mouse_hover_capture_wheel_transforms_and_hidden_state() {
    let (app, _) = new_app();
    let form = Spy::new(&app, FwlType::Form, None, CfxRectF::new(0., 0., 200., 100.));
    let first = Spy::new(
        &app,
        FwlType::Unknown,
        Some(form.widget()),
        CfxRectF::new(10., 20., 40., 30.),
    );
    let second = Spy::new(
        &app,
        FwlType::Unknown,
        Some(form.widget()),
        CfxRectF::new(80., 20., 40., 30.),
    );
    let mut move_first = FwlMessage::mouse(
        Some(form.widget()),
        MouseCommand::Move,
        0,
        CfxPointF::new(15., 25.),
    );
    app.note_driver().process_message(&mut move_first);
    assert!(matches!(move_first.data, MessageData::Mouse {pos,..} if pos==CfxPointF::new(5.,5.)));
    assert!(matches!(
        first.messages.borrow().last(),
        Some(MessageData::Mouse {
            command: MouseCommand::Hover,
            ..
        })
    ));
    let mut move_second = FwlMessage::mouse(
        Some(form.widget()),
        MouseCommand::Move,
        0,
        CfxPointF::new(85., 25.),
    );
    app.note_driver().process_message(&mut move_second);
    assert!(
        matches!(first.messages.borrow().last(), Some(MessageData::Mouse {command:MouseCommand::Leave,pos,..}) if *pos==CfxPointF::new(75.,5.))
    );
    assert!(Rc::ptr_eq(
        &app.note_driver().hover().unwrap(),
        &second.widget()
    ));
    app.note_driver().set_grab(Some(first.widget()));
    let mut captured = FwlMessage::mouse(
        Some(form.widget()),
        MouseCommand::Move,
        0,
        CfxPointF::new(180., 90.),
    );
    app.note_driver().process_message(&mut captured);
    assert!(Rc::ptr_eq(
        captured.destination.as_ref().unwrap(),
        &first.widget()
    ));
    assert!(matches!(captured.data, MessageData::Mouse {pos,..} if pos==CfxPointF::new(170.,70.)));
    let mut wheel = FwlMessage::mouse_wheel(
        Some(form.widget()),
        CfxPointF::new(85., 25.),
        CfxVector::new(0, -120),
    );
    app.note_driver().process_message(&mut wheel);
    assert!(Rc::ptr_eq(
        wheel.destination.as_ref().unwrap(),
        &second.widget()
    ));
    assert!(
        matches!(wheel.data, MessageData::MouseWheel {pos,delta} if pos==CfxPointF::new(5.,5.) && delta.y==-120)
    );
    app.note_driver()
        .process_message(&mut FwlMessage::set_focus(Some(first.widget())));
    first.base.set_states(WidgetState::INVISIBLE);
    assert!(app.note_driver().grab().is_none());
    assert!(app.note_driver().focus().is_none());
    assert!(app.note_driver().hover().is_none());
}

#[test]
fn nested_dispatch_and_mutable_text_validation_payloads() {
    let (app, _) = new_app();
    let source = widget(&app).widget();
    let first = widget(&app);
    let second = widget(&app);
    let entered = Rc::new(Cell::new(false));
    let weak_app = Rc::downgrade(&app);
    let source_ref = source.clone();
    *first.event_hook.borrow_mut() = Some(Rc::new(move |event| {
        if let EventData::TextWillChange {
            change_text,
            selection_start,
            selection_end,
            cancelled,
            ..
        } = &mut event.data
        {
            *change_text = vec![u32::from('X')];
            *selection_start = 2;
            *selection_end = 4;
            *cancelled = true;
        }
        if !entered.replace(true) {
            weak_app
                .upgrade()
                .unwrap()
                .note_driver()
                .send_event(&mut click(&source_ref));
        }
    }));
    app.note_driver()
        .register_event_target(&first.widget(), None);
    app.note_driver()
        .register_event_target(&second.widget(), None);
    let mut text =
        FwlEvent::text_will_change(Some(source.clone()), vec![1, 2], vec![3, 4, 5], 1, 1);
    app.note_driver().send_event(&mut text);
    assert_eq!(first.events.borrow().len(), 2);
    assert_eq!(second.events.borrow().len(), 2);
    assert!(
        matches!(text.data, EventData::TextWillChange {ref change_text,ref previous_text,selection_start:2,selection_end:4,cancelled:true} if change_text==&[u32::from('X')] && previous_text==&[3,4,5])
    );
    *first.event_hook.borrow_mut() = Some(Rc::new(|event| {
        if let EventData::Validate { validate, .. } = &mut event.data {
            *validate = false;
        }
    }));
    let mut validate = FwlEvent::validate(Some(source.clone()), vec![u32::from('9')]);
    app.note_driver().send_event(&mut validate);
    assert!(matches!(
        validate.data,
        EventData::Validate {
            validate: false,
            ..
        }
    ));
    let selected = FwlEvent::select_changed(Some(source.clone()), true);
    assert!(matches!(
        selected.data,
        EventData::SelectChanged {
            left_button_up: true,
            year: -1,
            month: -1,
            day: -1
        }
    ));
    let date = FwlEvent::date_selected(Some(source.clone()), 2026, 10, 23);
    assert!(matches!(
        date.data,
        EventData::SelectChanged {
            left_button_up: false,
            year: 2026,
            month: 10,
            day: 23
        }
    ));
    let scroll = FwlEvent::scroll(Some(source), ScrollCode::TrackPos, 42.5);
    assert!(matches!(
        scroll.data,
        EventData::Scroll {
            code: ScrollCode::TrackPos,
            pos: 42.5
        }
    ));
}

#[test]
fn target_destroyed_in_mouse_callback_does_not_receive_secondary_hover() {
    let (app, _) = new_app();
    let form = Spy::new(&app, FwlType::Form, None, CfxRectF::new(0., 0., 100., 100.));
    let target = Spy::new(
        &app,
        FwlType::Unknown,
        Some(form.widget()),
        CfxRectF::new(10., 10., 30., 30.),
    );
    let weak_app = Rc::downgrade(&app);
    let id = target.base.id();
    *target.message_hook.borrow_mut() = Some(Rc::new(move |message| {
        if matches!(
            message.data,
            MessageData::Mouse {
                command: MouseCommand::Move,
                ..
            }
        ) {
            weak_app.upgrade().unwrap().remove_widget(id);
        }
    }));
    app.note_driver().process_message(&mut FwlMessage::mouse(
        Some(form.widget()),
        MouseCommand::Move,
        0,
        CfxPointF::new(15., 15.),
    ));
    assert_eq!(target.messages.borrow().len(), 1);
    assert!(app.note_driver().hover().is_none());
    assert!(app.widget_mgr().get_widget(id).is_none());
}
