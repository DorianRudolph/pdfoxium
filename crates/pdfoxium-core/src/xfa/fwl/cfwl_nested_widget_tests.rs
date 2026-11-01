// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Real registered outer chains exercise the host-caret coordinate boundary.
use super::*;
use crate::xfa::fwl::{
    cfwl_listbox::CfwlListBox, cfwl_picturebox::CfwlPictureBox, cfwl_widget::WidgetAdapter,
    test_support::new_app,
};

struct Adapter {
    matrix: CfxMatrix,
    calls: RefCell<Vec<(bool, Option<CfxRectF>)>>,
}
impl Adapter {
    fn new(matrix: CfxMatrix) -> Rc<Self> {
        Rc::new(Self {
            matrix,
            calls: RefCell::new(Vec::new()),
        })
    }
}
impl WidgetAdapter for Adapter {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.matrix
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        self.calls.borrow_mut().push((visible, anchor));
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        (0xff000000, 1.)
    }
}
struct Nested {
    _app: Rc<CfwlApp>,
    root: Rc<CfwlPictureBox>,
    middle: Rc<CfwlPictureBox>,
    edit: Rc<CfwlEdit>,
    adapter: Rc<Adapter>,
}
fn nested(matrix: CfxMatrix) -> Nested {
    let (app, _host) = new_app();
    let root = CfwlPictureBox::new(&app);
    root.base()
        .set_widget_rect(CfxRectF::new(30., 40., 240., 160.));
    let middle = CfwlPictureBox::with_properties(&app, Properties::default(), Some(root.clone()));
    middle
        .base()
        .set_widget_rect(CfxRectF::new(7., 11., 150., 90.));
    let edit = CfwlEdit::new(&app, Properties::default(), Some(middle.clone()));
    edit.base().set_widget_rect(CfxRectF::new(3., 5., 80., 24.));
    let adapter = Adapter::new(matrix);
    root.base().set_adapter_iface(Some(adapter.clone()));
    // Deliberately do not Update(): an outer consumer supplies its caret
    // through WidgetAdapter when this component owns no separate caret widget.
    assert!(edit.caret().is_none());
    Nested {
        _app: app,
        root,
        middle,
        edit,
        adapter,
    }
}

#[test]
fn nested_host_caret_accumulates_each_outer_origin_once() {
    let f = nested(CfxMatrix::default());
    f.edit.show_caret(CfxRectF::new(1., 2., 1., 12.));
    assert_eq!(
        *f.adapter.calls.borrow(),
        vec![(true, Some(CfxRectF::new(41., 58., 1., 12.)))]
    );
}
#[test]
fn nested_host_caret_transforms_the_complete_rectangle_at_the_outer_boundary() {
    for (matrix, expected) in [
        (
            CfxMatrix::new(1., 0., 0., 1., 100., 200.),
            CfxRectF::new(141., 258., 1., 12.),
        ),
        (
            CfxMatrix::new(0., 1., -1., 0., 100., 0.),
            CfxRectF::new(30., 41., 12., 1.),
        ),
        (
            CfxMatrix::new(-1., 0., 0., 1., 100., 0.),
            CfxRectF::new(58., 58., 1., 12.),
        ),
        (
            CfxMatrix::new(2., 0., 0., 3., -10., 7.),
            CfxRectF::new(72., 181., 2., 36.),
        ),
    ] {
        let f = nested(matrix);
        f.edit.show_caret(CfxRectF::new(1., 2., 1., 12.));
        assert_eq!(*f.adapter.calls.borrow(), vec![(true, Some(expected))]);
    }
}
#[test]
fn only_the_outmost_adapter_receives_nested_show_and_hide_requests() {
    let f = nested(CfxMatrix::default());
    let inner = Adapter::new(CfxMatrix::new(1., 0., 0., 1., 900., 900.));
    f.middle.base().set_adapter_iface(Some(inner.clone()));
    f.edit.show_caret(CfxRectF::new(1., 2., 1., 12.));
    f.edit.hide_caret(None);
    assert_eq!(
        *f.adapter.calls.borrow(),
        vec![
            (true, Some(CfxRectF::new(41., 58., 1., 12.))),
            (false, None)
        ]
    );
    assert!(inner.calls.borrow().is_empty());
    f.root.base().set_adapter_iface(None);
    f.edit.show_caret(CfxRectF::new(1., 2., 1., 12.));
    assert_eq!(f.adapter.calls.borrow().len(), 2);
    assert!(inner.calls.borrow().is_empty());
}
#[test]
fn an_owned_caret_keeps_local_geometry_and_does_not_call_the_outer_adapter() {
    let f = nested(CfxMatrix::new(0., 1., -1., 0., 100., 0.));
    f.edit.init_caret();
    let local = CfxRectF::new(1., 2., 1., 12.);
    f.edit.show_caret(local);
    let caret = f.edit.caret().unwrap();
    assert_eq!(caret.base().get_widget_rect(), local);
    assert!(!caret.base().get_states().contains(WidgetState::INVISIBLE));
    f.edit.hide_caret(None);
    assert!(caret.base().get_states().contains(WidgetState::INVISIBLE));
    assert!(f.adapter.calls.borrow().is_empty());
}

#[test]
fn explicitly_initialized_horizontal_list_scroll_clamps_without_changing_selection() {
    for (code, start, supplied, expected, handled) in [
        (ScrollCode::Min, 20., 99., 0., true),
        (ScrollCode::Max, 20., -99., 40., true),
        (ScrollCode::StepBackward, 5., 5., 0., true),
        (ScrollCode::StepForward, 35., 35., 40., true),
        (ScrollCode::PageBackward, 30., 30., 10., true),
        (ScrollCode::PageForward, 30., 30., 40., true),
        (ScrollCode::TrackPos, 20., 90., 40., true),
        (ScrollCode::Pos, 20., -10., 0., true),
        (ScrollCode::EndScroll, 20., 90., 20., false),
        (ScrollCode::None, 20., 90., 40., true),
    ] {
        let (app, host) = new_app();
        let list = CfwlListBox::new(&app, Properties::default(), None);
        list.base().set_widget_rect(CfxRectF::new(0., 0., 80., 40.));
        let item = list.add_string(&[65, 66, 67]);
        list.update();
        list.set_sel_item(Some(&item), true);
        list.set_focus_item(Some(&item));
        list.init_horizontal_scroll_bar();
        let bar = list.get_horz_scroll_bar().unwrap();
        bar.set_range(0., 40.);
        bar.set_step_size(10.);
        bar.set_page_size(20.);
        bar.set_pos(start);
        bar.set_track_pos(start);
        host.repaints.borrow_mut().clear();
        assert_eq!(list.on_scroll(&bar, code, supplied), handled);
        assert_eq!(bar.get_pos(), expected);
        assert_eq!(bar.get_track_pos(), expected);
        assert_eq!(list.count_sel_items(), 1);
        assert!(Rc::ptr_eq(&list.get_sel_item(0).unwrap(), &item));
        assert!(Rc::ptr_eq(&list.get_focused_item().unwrap(), &item));
        assert_eq!(host.repaints.borrow().len(), usize::from(start != expected));
    }
}
#[test]
fn horizontal_offset_affects_item_hit_geometry_and_resize_clears_the_unused_bar() {
    let (app, _host) = new_app();
    let list = CfwlListBox::new(&app, Properties::default(), None);
    list.base().set_widget_rect(CfxRectF::new(5., 7., 80., 40.));
    let item = list.add_string(&[65, 66, 67]);
    list.update();
    list.set_sel_item(Some(&item), true);
    list.init_horizontal_scroll_bar();
    let bar = list.get_horz_scroll_bar().unwrap();
    bar.set_range(0., 40.);
    bar.set_step_size(10.);
    bar.set_page_size(20.);
    bar.set_pos(0.);
    bar.set_track_pos(0.);
    // A consumer can supply an indented item and an explicit horizontal bar.
    // The normal source CalcSize path itself never enables horizontal bars.
    item.set_rect(CfxRectF::new(30., 0., 60., 16.));
    let point = CfxPointF::new(list.content_rect().left + 10., list.content_rect().top + 8.);
    assert!(list.get_item_at_point(point).is_none());
    assert!(list.on_scroll(&bar, ScrollCode::TrackPos, 20.));
    assert!(Rc::ptr_eq(&list.get_item_at_point(point).unwrap(), &item));
    list.base()
        .set_widget_rect(CfxRectF::new(5., 7., 140., 60.));
    list.update();
    assert_eq!(bar.get_pos(), 0.);
    assert_eq!(bar.get_track_pos(), 0.);
    assert!(bar.base().get_states().contains(WidgetState::INVISIBLE));
    assert_eq!(list.count_sel_items(), 1);
    assert!(Rc::ptr_eq(&list.get_sel_item(0).unwrap(), &item));
}
