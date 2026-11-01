// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Explicit test host: source theme drawing, embedded font and captured host
//! callbacks. Production hosts must provide their own properties and scheduler.
use super::{
    cfwl_app::{AppAdapter, CfwlApp},
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::CfwlThemePart,
    cfwl_themetext::CfwlThemeText,
    cfwl_widget::{WidgetId, WidgetRef},
    cfwl_widgetmgr::WidgetMgrAdapter,
    ifwl_themeprovider::{ThemeProvider, ThemeRenderer},
};
use crate::{
    fpdfapi::parser::cpdf_document::CpdfDocument,
    fxcrt::{
        cfx_timer::TimerHandler,
        fx_coordinates::{CfxRectF, CfxSizeF},
    },
    xfa::fgas::{font::cfgas_gefont::CfgasGeFont, graphics::cfgas_gegraphics::CfgasGeGraphics},
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};

pub struct TestHost {
    myself: Weak<Self>,
    renderer: ThemeRenderer,
    font: Rc<CfgasGeFont>,
    next_timer: Cell<i32>,
    timers: RefCell<BTreeMap<i32, (i32, Rc<dyn Fn(i32)>)>>,
    pub killed: RefCell<Vec<i32>>,
    pub repaints: RefCell<Vec<WidgetId>>,
    pub repaint_hook: RefCell<Option<Rc<dyn Fn(WidgetRef)>>>,
}
impl TestHost {
    pub fn new() -> Rc<Self> {
        let font = CfgasGeFont::load_stock_font(&CpdfDocument::new(), b"Helvetica-Bold").unwrap();
        Rc::new_cyclic(|myself| Self {
            myself: myself.clone(),
            renderer: ThemeRenderer::new(),
            font,
            next_timer: Cell::new(100),
            timers: RefCell::new(BTreeMap::new()),
            killed: RefCell::new(Vec::new()),
            repaints: RefCell::new(Vec::new()),
            repaint_hook: RefCell::new(None),
        })
    }
    pub fn scheduled(&self) -> Vec<(i32, i32)> {
        self.timers
            .borrow()
            .iter()
            .map(|(&id, &(interval, _))| (id, interval))
            .collect()
    }
    pub fn timer_count(&self) -> usize {
        self.timers.borrow().len()
    }
    pub fn timer_callback(&self, id: i32) -> Rc<dyn Fn(i32)> {
        self.timers.borrow().get(&id).unwrap().1.clone()
    }
    pub fn fire(&self, id: i32) {
        let callback = self
            .timers
            .borrow()
            .get(&id)
            .map(|(_, callback)| callback.clone());
        if let Some(callback) = callback {
            callback(id);
        }
    }
}
pub fn new_app() -> (Rc<CfwlApp>, Rc<TestHost>) {
    let host = TestHost::new();
    (CfwlApp::new(host.clone()), host)
}
impl AppAdapter for TestHost {
    fn widget_mgr_adapter(&self) -> Rc<dyn WidgetMgrAdapter> {
        self.myself.upgrade().unwrap()
    }
    fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        Some(self.myself.upgrade().unwrap())
    }
    fn theme_provider(&self) -> Rc<dyn ThemeProvider> {
        self.myself.upgrade().unwrap()
    }
}
impl WidgetMgrAdapter for TestHost {
    fn repaint_widget(&self, widget: &WidgetRef) {
        self.repaints.borrow_mut().push(widget.base().id());
        let hook = self.repaint_hook.borrow().clone();
        if let Some(hook) = hook {
            hook(widget.clone());
        }
    }
    fn get_popup_pos(
        &self,
        _widget: &WidgetRef,
        min_height: f32,
        max_height: f32,
        anchor: CfxRectF,
        popup_rect: &mut CfxRectF,
    ) -> bool {
        let preferred_height = if popup_rect.height > 0. {
            popup_rect.height
        } else {
            max_height
        };
        *popup_rect = CfxRectF::new(
            anchor.left,
            anchor.bottom(),
            anchor.width,
            preferred_height.max(min_height).min(max_height),
        );
        true
    }
}
impl TimerHandler for TestHost {
    fn set_timer(&self, interval: i32, callback: Rc<dyn Fn(i32)>) -> i32 {
        let id = self.next_timer.get();
        self.next_timer.set(id + 1);
        self.timers.borrow_mut().insert(id, (interval, callback));
        id
    }
    fn kill_timer(&self, id: i32) {
        self.timers.borrow_mut().remove(&id);
        self.killed.borrow_mut().push(id);
    }
}
impl ThemeProvider for TestHost {
    fn draw_background(
        &self,
        params: &CfwlThemeBackground<'_>,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
    ) {
        self.renderer.draw_background(params, graphics);
    }
    fn draw_text(&self, params: &CfwlThemeText<'_>, graphics: &mut CfgasGeGraphics<'_, '_, '_>) {
        self.renderer.draw_text(self, params, graphics);
    }
    fn calc_text_rect(&self, params: &CfwlThemeText<'_>, rect: &mut CfxRectF) {
        self.renderer.calc_text_rect(self, params, rect);
    }
    fn get_cx_border_size(&self) -> f32 {
        1.
    }
    fn get_cy_border_size(&self) -> f32 {
        1.
    }
    fn get_ui_margin(&self, _part: &CfwlThemePart<'_>) -> CfxRectF {
        CfxRectF::default()
    }
    fn get_font_size(&self, _part: &CfwlThemePart<'_>) -> f32 {
        12.
    }
    fn get_font(&self, _part: &CfwlThemePart<'_>) -> Rc<CfgasGeFont> {
        self.font.clone()
    }
    fn get_fwl_font(&self) -> Rc<CfgasGeFont> {
        self.font.clone()
    }
    fn get_line_height(&self, _part: &CfwlThemePart<'_>) -> f32 {
        14.
    }
    fn get_scroll_bar_width(&self) -> f32 {
        16.
    }
    fn get_text_color(&self, _part: &CfwlThemePart<'_>) -> u32 {
        0xff000000
    }
    fn get_space_above_below(&self, _part: &CfwlThemePart<'_>) -> CfxSizeF {
        CfxSizeF::default()
    }
}
