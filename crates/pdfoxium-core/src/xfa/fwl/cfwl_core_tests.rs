// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived core tree/geometry/lifecycle checks with an explicit host.
use super::*;
use crate::xfa::fwl::{
    cfwl_event::FwlEvent,
    cfwl_message::{FwlMessage, MouseCommand},
    cfwl_widget::{dispatch_message_to, CfwlWidget, FwlWidget, Properties, WidgetStyle},
    fwl_widgethit::FwlWidgetHit,
    ifwl_widgetdelegate::WidgetDelegate,
    test_support::new_app,
};
use std::cell::Cell;
struct TestWidget {
    base: CfwlWidget,
    kind: FwlType,
    draws: Cell<usize>,
    finalized: Cell<bool>,
}
impl TestWidget {
    fn new(
        app: &Rc<CfwlApp>,
        parent: Option<WidgetRef>,
        kind: FwlType,
        rect: CfxRectF,
    ) -> Rc<Self> {
        let widget = Rc::new(Self {
            base: CfwlWidget::new(app, Properties::default(), parent),
            kind,
            draws: Cell::new(0),
            finalized: Cell::new(false),
        });
        widget.base.set_widget_rect(rect);
        app.register_widget(widget.clone());
        widget
    }
}
impl FwlWidget for TestWidget {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        self.kind
    }
    fn update(&self) {}
    fn draw_widget(&self, _graphics: &mut CfgasGeGraphics<'_, '_, '_>, _matrix: CfxMatrix) {
        self.draws.set(self.draws.get() + 1);
    }
    fn pre_finalize(&self) {
        self.finalized.set(true);
        self.base.pre_finalize();
    }
}
#[test]
fn ordered_tree_reparenting_hit_testing_default_button_and_cycle_guard() {
    let (app, _) = new_app();
    let root = TestWidget::new(
        &app,
        None,
        FwlType::Form,
        CfxRectF::new(200., 300., 100., 100.),
    );
    let a = TestWidget::new(
        &app,
        Some(root.clone()),
        FwlType::PictureBox,
        CfxRectF::new(10., 10., 50., 50.),
    );
    let b = TestWidget::new(
        &app,
        Some(root.clone()),
        FwlType::PictureBox,
        CfxRectF::new(20., 20., 50., 50.),
    );
    let root_ref: WidgetRef = root.clone();
    assert_eq!(
        app.widget_mgr()
            .get_first_child_widget(Some(root.base.id()))
            .unwrap()
            .base()
            .id(),
        a.base.id()
    );
    assert_eq!(
        app.widget_mgr()
            .get_next_sibling_widget(a.base.id())
            .unwrap()
            .base()
            .id(),
        b.base.id()
    );
    assert_eq!(
        app.widget_mgr()
            .get_prior_sibling_widget(b.base.id())
            .unwrap()
            .base()
            .id(),
        a.base.id()
    );
    assert_eq!(
        app.widget_mgr()
            .get_widget_at_point(&root_ref, CfxPointF::new(25., 25.))
            .unwrap()
            .base()
            .id(),
        b.base.id()
    );
    b.base.set_states(WidgetState::INVISIBLE);
    assert_eq!(
        app.widget_mgr()
            .get_widget_at_point(&root_ref, CfxPointF::new(25., 25.))
            .unwrap()
            .base()
            .id(),
        a.base.id()
    );
    b.base.clear_states(WidgetState::INVISIBLE);
    let button = TestWidget::new(
        &app,
        Some(a.clone()),
        FwlType::PushButton,
        CfxRectF::new(1., 1., 10., 10.),
    );
    button.base.set_states(WidgetState::PUSHBUTTON_DEFAULT);
    assert_eq!(
        app.widget_mgr()
            .get_default_button(&root_ref)
            .unwrap()
            .base()
            .id(),
        button.base.id()
    );
    assert!(app
        .widget_mgr()
        .insert_widget(Some(b.base.id()), button.clone()));
    assert_eq!(
        app.widget_mgr()
            .get_parent_widget(button.base.id())
            .unwrap()
            .base()
            .id(),
        b.base.id()
    );
    assert!(app
        .widget_mgr()
        .get_first_child_widget(Some(a.base.id()))
        .is_none());
    assert!(!app
        .widget_mgr()
        .insert_widget(Some(button.base.id()), root.clone()));
    app.remove_widget(b.base.id());
    assert!(b.finalized.get() && button.finalized.get());
    assert!(b.base.is_destroyed() && button.base.is_destroyed());
    assert!(app.widget_mgr().get_default_button(&root_ref).is_none());
}
#[test]
fn nested_local_coordinates_and_common_base_geometry() {
    let (app, _) = new_app();
    let root = TestWidget::new(
        &app,
        None,
        FwlType::Form,
        CfxRectF::new(200., 300., 200., 200.),
    );
    let a = TestWidget::new(
        &app,
        Some(root.clone()),
        FwlType::PictureBox,
        CfxRectF::new(10., 20., 100., 100.),
    );
    let b = TestWidget::new(
        &app,
        Some(a.clone()),
        FwlType::PictureBox,
        CfxRectF::new(5., 6., 50., 50.),
    );
    let c = TestWidget::new(
        &app,
        Some(b.clone()),
        FwlType::PictureBox,
        CfxRectF::new(2., 3., 20., 20.),
    );
    let root_ref: WidgetRef = root.clone();
    let c_ref: WidgetRef = c.clone();
    assert_eq!(
        c.base.transform_to(&root_ref, CfxPointF::new(1., 1.)),
        CfxPointF::new(18., 30.)
    );
    assert_eq!(
        root.base.transform_to(&c_ref, CfxPointF::new(18., 30.)),
        CfxPointF::new(1., 1.)
    );
    assert_eq!(
        c.base.get_matrix().transform(CfxPointF::default()),
        CfxPointF::new(15., 26.)
    );
    assert_eq!(
        app.widget_mgr()
            .get_widget_at_point(&root_ref, CfxPointF::new(18., 30.))
            .unwrap()
            .base()
            .id(),
        c.base.id()
    );
    c.base.set_styles(WidgetStyle::CHILD | WidgetStyle::BORDER);
    assert_eq!(c.base.get_client_rect(), CfxRectF::new(1., 1., 18., 18.));
    assert_eq!(
        c.base.hit_test(CfxPointF::new(0., 0.)),
        FwlWidgetHit::Border
    );
    assert_eq!(
        c.base.hit_test(CfxPointF::new(2., 2.)),
        FwlWidgetHit::Client
    );
    assert_eq!(
        c.base.hit_test(CfxPointF::new(21., 21.)),
        FwlWidgetHit::Unknown
    );
    let mut rect = CfxRectF::new(1., 1., 10., 10.);
    c.base.inflate_widget_rect(&mut rect);
    assert_eq!(rect, CfxRectF::new(0., 0., 12., 12.));
    assert!(c.base.is_child() && !c.base.is_popup() && !c.base.is_over_lapper());
    {
        let _lock = c.base.scoped_update_lock();
        assert!(c.base.is_locked());
        {
            let _nested = c.base.scoped_update_lock();
            assert!(c.base.is_locked());
        }
        assert!(c.base.is_locked());
    }
    assert!(!c.base.is_locked());
}
#[test]
fn hiding_parent_clears_focus_capture_and_hover_of_descendants() {
    let (app, _) = new_app();
    let root = TestWidget::new(&app, None, FwlType::Form, CfxRectF::new(0., 0., 100., 100.));
    let child = TestWidget::new(
        &app,
        Some(root.clone()),
        FwlType::PictureBox,
        CfxRectF::new(10., 10., 30., 30.),
    );
    let widget: WidgetRef = child.clone();
    app.note_driver()
        .process_message(&mut FwlMessage::set_focus(Some(widget.clone())));
    child.base.set_grab(true);
    assert!(app.note_driver().focus().is_some() && app.note_driver().grab().is_some());
    root.base.set_states(WidgetState::INVISIBLE);
    assert!(
        app.note_driver().focus().is_none()
            && app.note_driver().hover().is_none()
            && app.note_driver().grab().is_none()
    );
    assert!(app.widget_mgr().get_widget(child.base.id()).is_some());
}
#[test]
fn delegated_input_and_repaint_can_remove_widgets_without_borrowing_the_tree() {
    struct Delegate {
        app: Weak<CfwlApp>,
        widget: WidgetId,
        calls: Cell<usize>,
    }
    impl WidgetDelegate for Delegate {
        fn on_process_message(&self, _message: &mut FwlMessage) {
            self.calls.set(self.calls.get() + 1);
            self.app.upgrade().unwrap().remove_widget(self.widget);
        }
        fn on_process_event(&self, _event: &mut FwlEvent) {}
        fn on_draw_widget(&self, _graphics: &mut CfgasGeGraphics<'_, '_, '_>, _matrix: CfxMatrix) {}
    }
    let (app, host) = new_app();
    let widget = TestWidget::new(
        &app,
        None,
        FwlType::PictureBox,
        CfxRectF::new(0., 0., 30., 30.),
    );
    let delegate = Rc::new(Delegate {
        app: Rc::downgrade(&app),
        widget: widget.base.id(),
        calls: Cell::new(0),
    });
    widget.base.set_delegate(Some(delegate.clone()));
    let widget_ref: WidgetRef = widget.clone();
    let mut message = FwlMessage::mouse(
        Some(widget_ref.clone()),
        MouseCommand::Move,
        0,
        CfxPointF::new(2., 2.),
    );
    dispatch_message_to(&widget_ref, &mut message);
    assert_eq!(delegate.calls.get(), 1);
    assert!(widget.finalized.get());
    let outer = TestWidget::new(
        &app,
        None,
        FwlType::PictureBox,
        CfxRectF::new(0., 0., 30., 30.),
    );
    let inner = TestWidget::new(
        &app,
        Some(outer.clone()),
        FwlType::PictureBox,
        CfxRectF::new(1., 1., 10., 10.),
    );
    let weak = Rc::downgrade(&app);
    *host.repaint_hook.borrow_mut() = Some(Rc::new(move |widget| {
        weak.upgrade().unwrap().remove_widget(widget.base().id());
    }));
    inner.base.repaint_rect(CfxRectF::new(0., 0., 5., 5.));
    assert_eq!(host.repaints.borrow().last(), Some(&outer.base.id()));
    assert!(outer.finalized.get() && inner.finalized.get());
}
#[test]
fn app_drop_finalizes_external_widget_owners_and_popup_uses_host() {
    let (app, _) = new_app();
    let widget = TestWidget::new(
        &app,
        None,
        FwlType::PictureBox,
        CfxRectF::new(3., 4., 30., 20.),
    );
    let reference: WidgetRef = widget.clone();
    let mut popup = CfxRectF::new(0., 0., 0., 100.);
    assert!(app.widget_mgr().get_adapter_popup_pos(
        &reference,
        10.,
        50.,
        widget.get_widget_rect(),
        &mut popup
    ));
    assert_eq!(popup, CfxRectF::new(3., 24., 30., 50.));
    drop(app);
    assert!(widget.finalized.get() && widget.base.is_destroyed());
    assert!(widget.base.app().is_none() && widget.base.self_ref().is_none());
}

#[test]
fn composite_child_is_painted_once_and_grandchildren_are_still_drawn() {
    use crate::fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    };
    use std::cell::RefCell;
    struct Composite {
        base: CfwlWidget,
        child: RefCell<Option<Rc<TestWidget>>>,
    }
    impl FwlWidget for Composite {
        fn base(&self) -> &CfwlWidget {
            &self.base
        }
        fn get_class_id(&self) -> FwlType {
            FwlType::Form
        }
        fn update(&self) {}
        fn draws_child(&self, id: WidgetId) -> bool {
            self.child
                .borrow()
                .as_ref()
                .is_some_and(|child| child.base.id() == id)
        }
        fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
            let child = self.child.borrow().clone();
            if let Some(child) = child {
                child.draw_widget(graphics, matrix);
            }
        }
    }
    let (app, _) = new_app();
    let root = Rc::new(Composite {
        base: CfwlWidget::new(&app, Properties::default(), None),
        child: RefCell::new(None),
    });
    root.base.set_widget_rect(CfxRectF::new(0., 0., 60., 60.));
    app.register_widget(root.clone());
    let child = TestWidget::new(
        &app,
        Some(root.clone()),
        FwlType::Edit,
        CfxRectF::new(0., 0., 40., 40.),
    );
    let grandchild = TestWidget::new(
        &app,
        Some(child.clone()),
        FwlType::Caret,
        CfxRectF::new(0., 0., 2., 12.),
    );
    *root.child.borrow_mut() = Some(child.clone());
    let mut bitmap = CfxDibitmap::create(60, 60, FxdibFormat::Bgra, 0).unwrap();
    let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
    let mut graphics = CfgasGeGraphics::new(&mut device);
    app.widget_mgr()
        .on_draw_widget(&(root as WidgetRef), &mut graphics, CfxMatrix::default());
    assert_eq!(child.draws.get(), 1);
    assert_eq!(grandchild.draws.get(), 1);
}
