// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Edit scrollbar events are internal FWL messages, not exported C API codes.
//! Test the retained scroll delta, selection/history and repaint contract.
use super::*;
use crate::xfa::fwl::test_support::{new_app, TestHost};

struct Fixture {
    _app: Rc<CfwlApp>,
    host: Rc<TestHost>,
    edit: Rc<CfwlEdit>,
    bar: Rc<CfwlScrollBar>,
}
impl Fixture {
    fn new(position: f32) -> Self {
        let (app, host) = new_app();
        let edit = CfwlEdit::new(&app, Properties::default(), None);
        edit.base()
            .set_widget_rect(CfxRectF::new(0., 0., 120., 50.));
        edit.set_text_skip_notify(&"ABCDEFGH".chars().map(u32::from).collect::<Vec<_>>());
        edit.update();
        edit.set_selection(2, 3);
        edit.cursor.set(5);
        edit.caret_rect.set(CfxRectF::new(25., 20., 1., 12.));
        edit.scroll_x.set(7.);
        edit.scroll_y.set(position);
        let bar = CfwlScrollBar::new(
            &app,
            Properties {
                style_exts: FWL_STYLEEXT_SCB_VERT,
                ..Default::default()
            },
            Some(edit.clone()),
        );
        bar.set_range(10., 90.);
        bar.set_step_size(8.);
        bar.set_page_size(25.);
        bar.set_pos(position);
        bar.set_track_pos(position);
        *edit.vert_scroll_bar.borrow_mut() = Some(bar.clone());
        host.repaints.borrow_mut().clear();
        Self {
            _app: app,
            host,
            edit,
            bar,
        }
    }
    fn check(&self, code: ScrollCode, supplied: f32, expected: f32, handled: bool) {
        let before = self.bar.get_pos();
        let caret = self.edit.caret_rect.get();
        let text = self.edit.get_text();
        let cursor = self.edit.cursor_position();
        let selection = self.edit.get_selection();
        let undo = self.edit.can_undo();
        let redo = self.edit.can_redo();
        let timers = self.host.timer_count();
        self.host.repaints.borrow_mut().clear();
        let observations = Rc::new(RefCell::new(Vec::new()));
        let observed = observations.clone();
        let edit = Rc::downgrade(&self.edit);
        let bar = Rc::downgrade(&self.bar);
        *self.host.repaint_hook.borrow_mut() = Some(Rc::new(move |widget| {
            let edit = edit.upgrade().unwrap();
            let bar = bar.upgrade().unwrap();
            observed.borrow_mut().push((
                widget.base().id(),
                edit.caret()
                    .unwrap()
                    .base()
                    .get_states()
                    .contains(WidgetState::INVISIBLE),
                edit.scroll_y.get(),
                bar.get_pos(),
                bar.get_track_pos(),
            ));
        }));
        assert_eq!(
            self.edit.on_scroll(&self.bar, code, supplied),
            handled,
            "{code:?}"
        );
        assert_eq!(self.bar.get_pos(), expected);
        assert_eq!(self.bar.get_track_pos(), expected);
        assert_eq!(self.edit.scroll_y.get(), expected);
        assert_eq!(self.edit.scroll_x.get(), 7.);
        assert_eq!(
            self.edit.caret_rect.get(),
            caret,
            "logical caret must not move with the viewport"
        );
        assert_eq!(self.edit.get_text(), text);
        assert_eq!(self.edit.cursor_position(), cursor);
        assert_eq!(self.edit.get_selection(), selection);
        assert_eq!(self.edit.can_undo(), undo);
        assert_eq!(self.edit.can_redo(), redo);
        assert_eq!(self.host.timer_count(), timers);
        self.host.repaint_hook.borrow_mut().take();
        // Update() installs a real owned caret. A changed scroll first hides
        // that unfocused caret and repaints the engine area, then OnScroll
        // repaints the complete widget. Both callbacks must observe the new
        // position and the already-hidden caret, never stale intermediate state.
        // WidgetMgrAdapter exposes the root widget, not the requested rect.
        let expected_calls = if expected == before {
            Vec::new()
        } else {
            vec![self.edit.base().id(), self.edit.base().id()]
        };
        assert_eq!(*self.host.repaints.borrow(), expected_calls);
        for &(widget, hidden, offset, position, track) in observations.borrow().iter() {
            assert_eq!(widget, self.edit.base().id());
            assert!(hidden);
            assert_eq!((offset, position, track), (expected, expected, expected));
        }
        assert_eq!(observations.borrow().len(), expected_calls.len());
    }
}

#[test]
fn scroll_commands_apply_independent_limits_steps_and_pages() {
    for (code, start, supplied, expected) in [
        (ScrollCode::Min, 50., 777., 10.),
        (ScrollCode::Max, 50., -777., 90.),
        (ScrollCode::StepBackward, 13., 13., 10.),
        (ScrollCode::StepBackward, 24., 24., 16.),
        (ScrollCode::StepForward, 85., 85., 90.),
        (ScrollCode::StepForward, 76., 76., 84.),
        (ScrollCode::PageBackward, 30., 30., 10.),
        (ScrollCode::PageBackward, 60., 60., 35.),
        (ScrollCode::PageForward, 80., 80., 90.),
        (ScrollCode::PageForward, 30., 30., 55.),
        (ScrollCode::Pos, 30., 68., 68.),
        (ScrollCode::TrackPos, 30., 72., 72.),
    ] {
        Fixture::new(start).check(code, supplied, expected, true);
    }
}

#[test]
fn boundary_and_identical_position_events_do_not_repaint_or_restart_caret() {
    for (code, position) in [
        (ScrollCode::Min, 10.),
        (ScrollCode::Max, 90.),
        (ScrollCode::StepBackward, 10.),
        (ScrollCode::StepForward, 90.),
        (ScrollCode::PageBackward, 10.),
        (ScrollCode::PageForward, 90.),
        (ScrollCode::Pos, 50.),
        (ScrollCode::TrackPos, 50.),
        (ScrollCode::None, 50.),
    ] {
        Fixture::new(position).check(code, position, position, true);
    }
}

#[test]
fn end_scroll_ignores_a_pending_position_without_any_side_effects() {
    for position in [10., 50., 90.] {
        Fixture::new(position).check(ScrollCode::EndScroll, 13., position, false);
    }
}

#[test]
fn half_step_snapping_preserves_the_source_strict_boundary() {
    for (code, start, expected) in [
        (ScrollCode::StepBackward, 21.5, 10.), // 13.5 is within half a step of10.
        (ScrollCode::StepBackward, 22., 14.),
        (ScrollCode::StepForward, 78., 86.),
        (ScrollCode::StepForward, 78.5, 90.),
    ] {
        Fixture::new(start).check(code, start, expected, true);
    }
}

#[test]
fn successive_scroll_events_use_the_current_position_and_preserve_edit_state() {
    let f = Fixture::new(50.);
    for (code, supplied, expected, handled) in [
        (ScrollCode::Min, 50., 10., true),
        (ScrollCode::StepForward, 10., 18., true),
        (ScrollCode::PageForward, 18., 43., true),
        (ScrollCode::Max, 43., 90., true),
        (ScrollCode::StepBackward, 90., 82., true),
        (ScrollCode::PageBackward, 82., 57., true),
        (ScrollCode::TrackPos, 40., 40., true),
        (ScrollCode::EndScroll, 80., 40., false),
    ] {
        f.check(code, supplied, expected, handled);
    }
}
