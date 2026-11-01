// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fxge::{
    agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
    dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
};
use crate::xfa::fwl::{cfwl_event::EventData, test_support::new_app};

fn near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.0001,
        "{actual} != {expected}"
    );
}
fn bar(vertical: bool) -> (Rc<CfwlApp>, Rc<CfwlScrollBar>) {
    let (app, _) = new_app();
    let scrollbar = CfwlScrollBar::new(
        &app,
        Properties {
            style_exts: if vertical { FWL_STYLEEXT_SCB_VERT } else { 0 },
            ..Default::default()
        },
        None,
    );
    scrollbar.base.set_widget_rect(if vertical {
        CfxRectF::new(0., 0., 20., 200.)
    } else {
        CfxRectF::new(0., 0., 200., 20.)
    });
    scrollbar.set_range(0., 100.);
    scrollbar.set_track_pos(50.);
    scrollbar.update();
    (app, scrollbar)
}
#[test]
fn horizontal_and_vertical_geometry_preserve_source_numeric_steps() {
    for vertical in [false, true] {
        let (_app, bar) = bar(vertical);
        assert_eq!(bar.get_range(), (0., 100.));
        let min = bar.min_btn_rect.get();
        let max = bar.max_btn_rect.get();
        let thumb = bar.thumb_rect.get();
        if vertical {
            assert_eq!(min, CfxRectF::new(0., 0., 20., 20.));
            assert_eq!(max, CfxRectF::new(0., 180., 20., 20.));
            near(thumb.top, 50.76923);
            near(thumb.height, 98.46154);
            near(bar.min_track_rect.get().height, 100.);
            near(bar.max_track_rect.get().top, 100.);
            near(bar.max_track_rect.get().height, 100.);
        } else {
            assert_eq!(min, CfxRectF::new(0., 0., 20., 20.));
            assert_eq!(max, CfxRectF::new(180., 0., 20., 20.));
            near(thumb.left, 50.76923);
            near(thumb.width, 98.46154);
            near(bar.min_track_rect.get().width, 100.);
            near(bar.max_track_rect.get().left, 100.);
            near(bar.max_track_rect.get().width, 100.);
        }
        bar.set_track_pos(-100.);
        let thumb = bar.thumb_rect.get();
        near(if vertical { thumb.top } else { thumb.left }, 20.);
        bar.set_track_pos(1000.);
        let thumb = bar.thumb_rect.get();
        near(
            if vertical {
                thumb.bottom()
            } else {
                thumb.right()
            },
            180.,
        );
        bar.set_page_size(20.);
        bar.set_step_size(2.);
        bar.set_pos(30.);
        assert_eq!(
            (bar.get_page_size(), bar.get_step_size(), bar.get_pos()),
            (20., 2., 30.)
        );
    }
}
#[test]
fn horizontal_track_extent_uses_width_and_border_origin() {
    let (app, _) = new_app();
    let bar = CfwlScrollBar::new(
        &app,
        Properties {
            styles: super::super::cfwl_widget::WidgetStyle::CHILD
                | super::super::cfwl_widget::WidgetStyle::BORDER,
            ..Default::default()
        },
        None,
    );
    bar.base.set_widget_rect(CfxRectF::new(0., 0., 200., 20.));
    bar.set_range(0., 100.);
    bar.set_track_pos(50.);
    bar.update();
    let c = bar.client_rect.get();
    let low = bar.min_track_rect.get();
    let high = bar.max_track_rect.get();
    near(low.left, c.left);
    near(low.right(), high.left);
    near(high.right(), c.right());
    assert!(high.width > 50.);
}
#[test]
fn invalid_zero_and_minimum_ranges_are_bounded() {
    let (_app, bar) = bar(true);
    bar.set_range(1., 1.);
    bar.update();
    assert_eq!(bar.thumb_rect.get(), CfxRectF::default());
    bar.set_range(2., 1.);
    bar.update();
    assert_eq!(bar.thumb_rect.get().height, 0.);
    bar.set_range(f32::NAN, 100.);
    assert_eq!(bar.get_range(), (2., 1.));
    bar.set_range(0., 100.);
    bar.base.set_widget_rect(CfxRectF::new(0., 0., 20., 10.));
    bar.update();
    assert_eq!(bar.button_len.get(), 5.);
    assert!(bar.min_size.get());
    assert_eq!(bar.thumb_rect.get().height, 0.);
    bar.base.set_states(WidgetState::DISABLED);
    bar.update();
    assert_eq!(bar.thumb_rect.get(), CfxRectF::default());
    assert!(!bar.do_scroll(ScrollCode::None, 0.));
    assert!(!bar.do_scroll(ScrollCode::Pos, f32::NAN));
}

struct Listener {
    base: CfwlWidget,
    events: RefCell<Vec<(ScrollCode, f32)>>,
    callback: RefCell<Option<Rc<dyn Fn(&FwlEvent)>>>,
}
impl Listener {
    fn new(app: &Rc<CfwlApp>) -> Rc<Self> {
        let listener = Rc::new(Self {
            base: CfwlWidget::new(app, Properties::default(), None),
            events: RefCell::new(Vec::new()),
            callback: RefCell::new(None),
        });
        app.register_widget(listener.clone() as WidgetRef);
        listener
    }
}
impl FwlWidget for Listener {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::Form
    }
    fn update(&self) {}
    fn draw_widget(&self, _graphics: &mut CfgasGeGraphics<'_, '_, '_>, _matrix: CfxMatrix) {}
    fn on_process_event(&self, event: &mut FwlEvent) {
        if let EventData::Scroll { code, pos } = event.data {
            self.events.borrow_mut().push((code, pos));
        }
        let callback = self.callback.borrow().clone();
        if let Some(callback) = callback {
            callback(event);
        }
    }
}
fn mouse(bar: &Rc<CfwlScrollBar>, command: MouseCommand, pos: CfxPointF) {
    let mut message = FwlMessage::mouse(Some(bar.clone() as WidgetRef), command, 0, pos);
    bar.on_process_message(&mut message);
}
#[test]
fn wheel_and_drag_events_allow_reentrant_track_updates() {
    let (app, _) = new_app();
    let listener = Listener::new(&app);
    let bar = CfwlScrollBar::new(
        &app,
        Properties::default(),
        Some(listener.clone() as WidgetRef),
    );
    bar.base.set_widget_rect(CfxRectF::new(0., 0., 200., 20.));
    bar.set_range(0., 100.);
    bar.set_track_pos(50.);
    bar.update();
    let weak = Rc::downgrade(&bar);
    *listener.callback.borrow_mut() = Some(Rc::new(move |event| {
        if let EventData::Scroll {
            code: ScrollCode::TrackPos,
            pos,
        } = event.data
        {
            weak.upgrade().unwrap().set_track_pos(pos);
        }
    }));
    let mut wheel = FwlMessage::mouse_wheel(
        Some(bar.clone() as WidgetRef),
        CfxPointF::new(100., 10.),
        CfxVector::new(0, -120),
    );
    bar.on_process_message(&mut wheel);
    assert_eq!(listener.events.borrow()[0], (ScrollCode::StepForward, 50.));
    mouse(
        &bar,
        MouseCommand::LeftButtonDown,
        CfxPointF::new(100., 10.),
    );
    mouse(&bar, MouseCommand::Move, CfxPointF::new(110., 10.));
    let events = listener.events.borrow();
    let (code, pos) = events.last().unwrap();
    assert_eq!(*code, ScrollCode::TrackPos);
    assert!(*pos > 50.);
    drop(events);
    near(bar.get_track_pos(), 66.25);
    near(bar.thumb_rect.get().left, 60.76923);
    mouse(&bar, MouseCommand::LeftButtonUp, CfxPointF::new(110., 10.));
    assert!(listener
        .events
        .borrow()
        .iter()
        .any(|(code, _)| *code == ScrollCode::EndScroll));
    assert!(!bar.mouse_down.get());
}
#[test]
fn real_scrollbar_theme_draws_both_orientations() {
    for vertical in [false, true] {
        let (_app, bar) = bar(vertical);
        let mut bitmap = CfxDibitmap::create(220, 220, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0xffffffff);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut graphics = CfgasGeGraphics::new(&mut device);
            bar.draw_widget(&mut graphics, CfxMatrix::default());
        }
        assert!(
            bitmap
                .buffer()
                .chunks_exact(4)
                .filter(|pixel| *pixel != [255, 255, 255, 255])
                .count()
                > 500
        );
        assert!(bitmap
            .scanline(210)
            .unwrap()
            .iter()
            .all(|&byte| byte == 255));
    }
}

#[test]
fn repeat_timer_stops_on_release_destroy_and_reentrant_release() {
    let (app, host) = new_app();
    let listener = Listener::new(&app);
    let bar = CfwlScrollBar::new(
        &app,
        Properties::default(),
        Some(listener.clone() as WidgetRef),
    );
    bar.base.set_widget_rect(CfxRectF::new(0., 0., 200., 20.));
    bar.set_range(0., 100.);
    bar.set_track_pos(50.);
    bar.update();
    mouse(&bar, MouseCommand::LeftButtonDown, CfxPointF::new(5., 10.));
    assert_eq!(listener.events.borrow()[0], (ScrollCode::StepBackward, 50.));
    let scheduled = host.scheduled();
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0].1, 500);
    let first = scheduled[0].0;
    let late = host.timer_callback(first);
    host.fire(first);
    assert_eq!(listener.events.borrow().len(), 2);
    assert!(host.killed.borrow().contains(&first));
    let repeat = host.scheduled();
    assert_eq!(repeat.len(), 1);
    assert_eq!(repeat[0].1, 0);
    mouse(&bar, MouseCommand::LeftButtonUp, CfxPointF::new(5., 10.));
    assert_eq!(host.timer_count(), 0);
    let count = listener.events.borrow().len();
    late(first);
    assert_eq!(listener.events.borrow().len(), count);
    assert_eq!(host.timer_count(), 0);
    let weak = Rc::downgrade(&bar);
    *listener.callback.borrow_mut() = Some(Rc::new(move |event| {
        if matches!(
            event.data,
            EventData::Scroll {
                code: ScrollCode::StepBackward,
                ..
            }
        ) {
            mouse(
                &weak.upgrade().unwrap(),
                MouseCommand::LeftButtonUp,
                CfxPointF::new(5., 10.),
            );
        }
    }));
    mouse(&bar, MouseCommand::LeftButtonDown, CfxPointF::new(5., 10.));
    assert_eq!(host.timer_count(), 0);
    assert!(!bar.mouse_down.get());
    listener.callback.borrow_mut().take();
    mouse(&bar, MouseCommand::LeftButtonDown, CfxPointF::new(5., 10.));
    assert_eq!(host.timer_count(), 1);
    bar.pre_finalize();
    assert!(bar.base.is_destroyed());
    assert_eq!(host.timer_count(), 0);
    let after = listener.events.borrow().len();
    bar.on_timer_fired();
    assert_eq!(listener.events.borrow().len(), after);
}
