// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Connected tree routing -> control state/event -> theme -> AGG bitmap checks.
use super::{
    cfwl_app::CfwlApp,
    cfwl_checkbox::CfwlCheckBox,
    cfwl_event::{EventType, FwlEvent},
    cfwl_message::{FwlMessage, KeyCommand, MouseCommand},
    cfwl_picturebox::CfwlPictureBox,
    cfwl_widget::{FwlWidget, Properties, WidgetId, WidgetRef, WidgetState, WidgetStyle},
    ifwl_widgetdelegate::WidgetDelegate,
    test_support::new_app,
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

struct Observer {
    app: Weak<CfwlApp>,
    parent: Weak<dyn FwlWidget>,
    events: RefCell<Vec<(EventType, Option<WidgetId>)>>,
    remove_on_change: Cell<bool>,
}
impl WidgetDelegate for Observer {
    fn on_process_message(&self, message: &mut FwlMessage) {
        if let Some(parent) = self.parent.upgrade() {
            parent.on_process_message(message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        self.events.borrow_mut().push((
            event.get_type(),
            event.source.as_ref().map(|w| w.base().id()),
        ));
        if event.get_type() == EventType::CheckStateChanged && self.remove_on_change.get() {
            if let (Some(app), Some(source)) = (self.app.upgrade(), event.source.as_ref()) {
                app.remove_widget(source.base().id());
            }
        }
    }
    fn on_draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if let Some(parent) = self.parent.upgrade() {
            parent.draw_widget(graphics, matrix);
        }
    }
}
fn draw(app: &CfwlApp, root: &WidgetRef) -> Vec<u8> {
    let mut bitmap = CfxDibitmap::create(160, 70, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        app.widget_mgr()
            .on_draw_widget(root, &mut graphics, CfxMatrix::default());
    }
    bitmap.buffer().to_vec()
}
#[test]
fn child_input_changes_rendering_and_callback_removal_clears_routing() {
    let (app, host) = new_app();
    let parent = CfwlPictureBox::with_properties(
        &app,
        Properties {
            styles: WidgetStyle::OVER_LAPPER | WidgetStyle::BORDER,
            ..Default::default()
        },
        None,
    );
    parent
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 150., 60.));
    parent.update();
    let parent_ref: WidgetRef = parent.clone();
    let observer = Rc::new(Observer {
        app: Rc::downgrade(&app),
        parent: Rc::downgrade(&parent_ref),
        events: RefCell::new(Vec::new()),
        remove_on_change: Cell::new(false),
    });
    parent.base().set_delegate(Some(observer.clone()));
    let check =
        CfwlCheckBox::with_properties(&app, Properties::default(), Some(parent_ref.clone()));
    check
        .base()
        .set_widget_rect(CfxRectF::new(10., 8., 125., 32.));
    check.update();
    let check_id = check.base().id();
    let unchecked = draw(&app, &parent_ref);
    for command in [MouseCommand::LeftButtonDown, MouseCommand::LeftButtonUp] {
        let mut message = FwlMessage::mouse(
            Some(parent_ref.clone()),
            command,
            0,
            CfxPointF::new(15., 13.),
        );
        app.widget_mgr().on_process_message_to_form(&mut message);
        assert_eq!(message.destination.as_ref().unwrap().base().id(), check_id);
    }
    assert_eq!(check.get_check_state(), 1);
    assert!(observer
        .events
        .borrow()
        .contains(&(EventType::CheckStateChanged, Some(check_id))));
    assert!(host.repaints.borrow().contains(&parent.base().id()));
    assert_ne!(draw(&app, &parent_ref), unchecked);
    let mut focus = FwlMessage::set_focus(Some(check.clone()));
    app.note_driver().process_message(&mut focus);
    assert!(check.base().get_states().contains(WidgetState::FOCUSED));
    observer.remove_on_change.set(true);
    let mut key = FwlMessage::key(Some(parent_ref.clone()), KeyCommand::KeyDown, 0, 0x20);
    app.note_driver().process_message(&mut key);
    assert!(app.widget_mgr().get_widget(check_id).is_none());
    assert!(app.note_driver().focus().is_none());
    assert!(app.note_driver().hover().is_none());
    assert!(check.base().is_destroyed());
    assert!(app
        .widget_mgr()
        .get_first_child_widget(Some(parent.base().id()))
        .is_none());
    assert_ne!(draw(&app, &parent_ref), unchecked);
}
