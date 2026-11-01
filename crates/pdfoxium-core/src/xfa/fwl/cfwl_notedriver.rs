// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cfwl_notedriver.{h,cpp}`: synchronous event subscriptions and input routing.
//! Listener/filter storage is borrowed only to select a small owned handle;
//! every widget/delegate callback executes after that borrow has been released.
use super::{
    cfwl_app::CfwlApp,
    cfwl_event::FwlEvent,
    cfwl_message::{FwlMessage, KeyCommand, MessageData, MouseCommand},
    cfwl_widget::{dispatch_event_to, dispatch_message_to, FwlType, WidgetId, WidgetRef},
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::{Rc, Weak},
};

type WidgetWeak = Weak<dyn super::cfwl_widget::FwlWidget>;
struct Target {
    listener: WidgetWeak,
    valid: Cell<bool>,
    sources: RefCell<BTreeSet<WidgetId>>,
}
impl Target {
    fn new(listener: &WidgetRef) -> Self {
        Self {
            listener: Rc::downgrade(listener),
            valid: Cell::new(true),
            sources: RefCell::new(BTreeSet::new()),
        }
    }
    fn accepts(&self, event: &FwlEvent) -> bool {
        let sources = self.sources.borrow();
        sources.is_empty()
            || event
                .source
                .as_ref()
                .is_some_and(|widget| sources.contains(&widget.base().id()))
    }
}

pub struct CfwlNoteDriver {
    app: Weak<CfwlApp>,
    hover: RefCell<Option<WidgetWeak>>,
    focus: RefCell<Option<WidgetWeak>>,
    grab: RefCell<Option<WidgetWeak>>,
    hover_generation: Cell<u64>,
    next_listener_key: Cell<u64>,
    event_targets: RefCell<BTreeMap<u64, Rc<Target>>>,
}
impl CfwlNoteDriver {
    pub fn new(app: Weak<CfwlApp>) -> Self {
        Self {
            app,
            hover: RefCell::new(None),
            focus: RefCell::new(None),
            grab: RefCell::new(None),
            hover_generation: Cell::new(0),
            next_listener_key: Cell::new(1),
            event_targets: RefCell::new(BTreeMap::new()),
        }
    }
    fn is_live(&self, widget: &WidgetRef) -> bool {
        self.app.upgrade().is_some_and(|app| {
            app.widget_mgr()
                .get_widget(widget.base().id())
                .is_some_and(|registered| Rc::ptr_eq(&registered, widget))
        })
    }
    fn live_ref(&self, slot: &RefCell<Option<WidgetWeak>>) -> Option<WidgetRef> {
        let widget = slot.borrow().as_ref().and_then(Weak::upgrade)?;
        self.is_live(&widget).then_some(widget)
    }
    pub fn focus(&self) -> Option<WidgetRef> {
        self.live_ref(&self.focus)
    }
    pub fn hover(&self) -> Option<WidgetRef> {
        self.live_ref(&self.hover)
    }
    pub fn grab(&self) -> Option<WidgetRef> {
        self.live_ref(&self.grab)
    }
    pub fn set_grab(&self, widget: Option<WidgetRef>) {
        *self.grab.borrow_mut() = widget
            .as_ref()
            .filter(|widget| self.is_live(widget))
            .map(Rc::downgrade);
    }
    pub fn send_event(&self, event: &mut FwlEvent) {
        // New subscriptions begin with the next event. Already selected targets
        // still consult their current validity/filter, so callbacks can remove
        // themselves or a later target, and nested dispatch is safe.
        let targets: Vec<_> = self.event_targets.borrow().values().cloned().collect();
        for target in targets {
            if !target.valid.get() || !target.accepts(event) {
                continue;
            }
            let Some(listener) = target.listener.upgrade() else {
                continue;
            };
            if !self.is_live(&listener) {
                continue;
            }
            dispatch_event_to(&listener, event);
        }
    }
    pub fn register_event_target(&self, listener: &WidgetRef, source: Option<&WidgetRef>) {
        let mut key = listener.base().get_event_key();
        if key == 0 {
            key = self.next_listener_key.get();
            // Keys are local to a driver. Reuse is prevented for its lifetime.
            let Some(next) = key.checked_add(1) else {
                return;
            };
            self.next_listener_key.set(next);
            listener.base().set_event_key(key);
        }
        let target = self
            .event_targets
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| Rc::new(Target::new(listener)))
            .clone();
        // A registration reactivates a listener previously unregistered; the
        // native target remained invalid forever after the first removal.
        target.valid.set(true);
        if let Some(source) = source {
            target.sources.borrow_mut().insert(source.base().id());
        }
    }
    pub fn unregister_event_target(&self, listener: &WidgetRef) {
        let key = listener.base().get_event_key();
        if key == 0 {
            return;
        }
        if let Some(target) = self.event_targets.borrow().get(&key) {
            target.valid.set(false);
        }
    }
    pub fn notify_target_hide(&self, widget: &WidgetRef) {
        for slot in [&self.focus, &self.hover, &self.grab] {
            let matches = slot
                .borrow()
                .as_ref()
                .and_then(Weak::upgrade)
                .is_some_and(|value| Rc::ptr_eq(&value, widget));
            if matches {
                *slot.borrow_mut() = None;
                if std::ptr::eq(slot, &self.hover) {
                    self.hover_generation
                        .set(self.hover_generation.get().wrapping_add(1));
                }
            }
        }
    }
    pub fn notify_target_destroy(&self, widget: &WidgetRef) {
        self.notify_target_hide(widget);
        self.unregister_event_target(widget);
        // Filters store stable IDs, not a retaining pointer to a dead source.
        // Keep a removed source's ID: removing the last filter would wrongly
        // turn the listener into an unfiltered wildcard subscription.
        let key = widget.base().get_event_key();
        if let Some(target) = self.event_targets.borrow_mut().remove(&key) {
            target.valid.set(false);
        }
    }
    pub fn process_message(&self, message: &mut FwlMessage) {
        let Some(form) = message.destination.clone() else {
            return;
        };
        if !self.is_live(&form) || !self.dispatch_message(message, Some(&form)) {
            return;
        }
        if matches!(message.data, MessageData::Mouse { .. }) {
            self.mouse_secondary(message);
        }
    }
    fn dispatch_message(&self, message: &mut FwlMessage, form: Option<&WidgetRef>) -> bool {
        let Some(destination) = message.destination.clone() else {
            return false;
        };
        if !self.is_live(&destination) {
            return false;
        }
        match &mut message.data {
            MessageData::SetFocus => {
                *self.focus.borrow_mut() = Some(Rc::downgrade(&destination));
            }
            MessageData::KillFocus { .. } => {
                if self
                    .focus()
                    .as_ref()
                    .is_some_and(|focus| Rc::ptr_eq(focus, &destination))
                {
                    *self.focus.borrow_mut() = None;
                }
            }
            MessageData::Key {
                command,
                key_code_or_char,
                ..
            } => {
                if let Some(focus) = self.focus() {
                    message.destination = Some(focus);
                } else if *command == KeyCommand::KeyDown && *key_code_or_char == 0x0d {
                    let Some(form) = form else {
                        return false;
                    };
                    let Some(app) = self.app.upgrade() else {
                        return false;
                    };
                    let Some(button) = app.widget_mgr().get_default_button(form) else {
                        return false;
                    };
                    message.destination = Some(button);
                } else {
                    return false;
                }
            }
            MessageData::Mouse { command, pos, .. } => {
                if !matches!(
                    command,
                    MouseCommand::Leave | MouseCommand::Hover | MouseCommand::Enter
                ) {
                    let Some(form) = form else {
                        return false;
                    };
                    if !Rc::ptr_eq(&destination, form) {
                        *pos = destination.base().transform_to(form, *pos);
                    }
                    let target = self.grab().or_else(|| {
                        self.app
                            .upgrade()
                            .and_then(|app| app.widget_mgr().get_widget_at_point(form, *pos))
                    });
                    if let Some(target) = target {
                        if !Rc::ptr_eq(&target, form) {
                            *pos = form.base().transform_to(&target, *pos);
                        }
                        message.destination = Some(target);
                    } else {
                        message.destination = Some(form.clone());
                    }
                }
            }
            MessageData::MouseWheel { pos, .. } => {
                let Some(form) = form else {
                    return false;
                };
                let Some(app) = self.app.upgrade() else {
                    return false;
                };
                let Some(target) = app.widget_mgr().get_widget_at_point(form, *pos) else {
                    return false;
                };
                *pos = form.base().transform_to(&target, *pos);
                message.destination = Some(target);
            }
        }
        let Some(target) = message.destination.clone() else {
            return false;
        };
        if !self.is_live(&target) {
            return false;
        }
        dispatch_message_to(&target, message);
        true
    }
    fn mouse_secondary(&self, message: &FwlMessage) {
        let Some(target) = message
            .destination
            .as_ref()
            .filter(|target| self.is_live(target))
        else {
            return;
        };
        let MessageData::Mouse { pos, .. } = message.data else {
            return;
        };
        let hover = self.hover();
        if hover
            .as_ref()
            .is_some_and(|hover| Rc::ptr_eq(hover, target))
        {
            return;
        }
        let generation = self.hover_generation.get();
        if let Some(hover) = hover {
            let mut leave = FwlMessage::mouse(
                Some(hover.clone()),
                MouseCommand::Leave,
                0,
                target.base().transform_to(&hover, pos),
            );
            self.dispatch_message(&mut leave, None);
        }
        // A leave handler may hide/destroy the new target or perform its own
        // nested mouse routing. Respect that update instead of reviving it.
        if self.hover_generation.get() != generation || !self.is_live(target) {
            return;
        }
        self.hover_generation.set(generation.wrapping_add(1));
        if target.get_class_id() == FwlType::Form {
            *self.hover.borrow_mut() = None;
            return;
        }
        *self.hover.borrow_mut() = Some(Rc::downgrade(target));
        let mut hover = FwlMessage::mouse(Some(target.clone()), MouseCommand::Hover, 0, pos);
        self.dispatch_message(&mut hover, None);
    }
}

#[cfg(test)]
#[path = "cfwl_notedriver_tests.rs"]
mod tests;
